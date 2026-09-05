use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use chrono::Utc;
use pastor_core::{
    backend::PackageBackend,
    error::PastorError,
    package::{
        Package, PackageCategory, PackageIcon, PackageId, PackageSource, PackageState,
        PackageUpdate, ParchRepoType,
    },
    snapshot::{SnapshotBackend, SnapshotInfo},
    transaction::{TransactionEvent, TransactionStep},
};
use tokio::sync::mpsc::Sender;

pub struct MockSnapshotBackend {
    snapshots: Mutex<Vec<SnapshotInfo>>,
}

impl Default for MockSnapshotBackend {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            snapshots: Mutex::new(vec![
                SnapshotInfo {
                    number: 1,
                    date: now - chrono::Duration::days(3),
                    description: "Initial system installation".to_string(),
                    pre_number: None,
                    cleanup: "number".to_string(),
                },
                SnapshotInfo {
                    number: 2,
                    date: now - chrono::Duration::days(1),
                    description: "Pastor: Pre-update system upgrade".to_string(),
                    pre_number: None,
                    cleanup: "number".to_string(),
                },
                SnapshotInfo {
                    number: 3,
                    date: now - chrono::Duration::days(1),
                    description: "Pastor: Post-update system upgrade".to_string(),
                    pre_number: Some(2),
                    cleanup: "number".to_string(),
                },
            ]),
        }
    }
}

#[async_trait]
impl SnapshotBackend for MockSnapshotBackend {
    async fn create_pre_snapshot(&self, description: &str) -> Result<u64, PastorError> {
        let mut snaps = self.snapshots.lock().unwrap();
        let next_num = snaps.last().map(|s| s.number + 1).unwrap_or(1);
        snaps.push(SnapshotInfo {
            number: next_num,
            date: Utc::now(),
            description: format!("Pastor: Pre - {description}"),
            pre_number: None,
            cleanup: "number".to_string(),
        });
        Ok(next_num)
    }

    async fn create_post_snapshot(&self, pre_number: u64, description: &str) -> Result<u64, PastorError> {
        let mut snaps = self.snapshots.lock().unwrap();
        let next_num = snaps.last().map(|s| s.number + 1).unwrap_or(1);
        snaps.push(SnapshotInfo {
            number: next_num,
            date: Utc::now(),
            description: format!("Pastor: Post - {description}"),
            pre_number: Some(pre_number),
            cleanup: "number".to_string(),
        });
        Ok(next_num)
    }

    async fn list_snapshots(&self) -> Result<Vec<SnapshotInfo>, PastorError> {
        let snaps = self.snapshots.lock().unwrap().clone();
        Ok(snaps)
    }

    async fn delete_snapshot(&self, number: u64) -> Result<(), PastorError> {
        let mut snaps = self.snapshots.lock().unwrap();
        snaps.retain(|s| s.number != number);
        Ok(())
    }
}

pub struct MockPackageBackend {
    name: &'static str,
    packages: Arc<Mutex<HashMap<PackageId, Package>>>,
    snapshots: Arc<MockSnapshotBackend>,
}

impl MockPackageBackend {
    pub fn new() -> Self {
        let mut map = HashMap::new();
        let mock_pkgs = get_seed_packages();
        for pkg in mock_pkgs {
            map.insert(pkg.id.clone(), pkg);
        }

        Self {
            name: "mock-backend",
            packages: Arc::new(Mutex::new(map)),
            snapshots: Arc::new(MockSnapshotBackend::default()),
        }
    }

    pub fn snapshot_backend(&self) -> Arc<MockSnapshotBackend> {
        self.snapshots.clone()
    }
}

impl Default for MockPackageBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PackageBackend for MockPackageBackend {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>, PastorError> {
        tokio::time::sleep(Duration::from_millis(40)).await;
        let q = query.trim().to_lowercase();
        let pkgs = self.packages.lock().unwrap();

        if q.is_empty() {
            return Ok(pkgs.values().cloned().collect());
        }

        let results: Vec<Package> = pkgs
            .values()
            .filter(|p| {
                p.name.to_lowercase().contains(&q)
                    || p.display_title().to_lowercase().contains(&q)
                    || p.summary.to_lowercase().contains(&q)
                    || p.description.as_deref().unwrap_or("").to_lowercase().contains(&q)
            })
            .cloned()
            .collect();

        Ok(results)
    }

    async fn get_package(&self, id: &PackageId) -> Result<Option<Package>, PastorError> {
        let pkgs = self.packages.lock().unwrap();
        Ok(pkgs.get(id).cloned())
    }

    async fn installed(&self) -> Result<Vec<Package>, PastorError> {
        let pkgs = self.packages.lock().unwrap();
        let installed: Vec<Package> = pkgs
            .values()
            .filter(|p| p.is_installed())
            .cloned()
            .collect();
        Ok(installed)
    }

    async fn updates(&self) -> Result<Vec<PackageUpdate>, PastorError> {
        let pkgs = self.packages.lock().unwrap();
        let mut updates = Vec::new();
        for p in pkgs.values() {
            if p.state == PackageState::UpdateAvailable {
                updates.push(PackageUpdate {
                    id: p.id.clone(),
                    current_version: p.installed_version.clone().unwrap_or_default(),
                    new_version: p.version.clone(),
                    download_size: p.size_download,
                    changelog: Some(format!("Upgraded to {} with stability improvements.", p.version)),
                });
            }
        }
        Ok(updates)
    }

    async fn install(
        &self,
        id: &PackageId,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let pkg_opt = {
            let pkgs = self.packages.lock().unwrap();
            pkgs.get(id).cloned()
        };

        let pkg = pkg_opt.ok_or_else(|| PastorError::PackageNotFound(id.name.clone()))?;

        // 1. Snapshotting
        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::CreatingSnapshot(format!("Pre-install {}", pkg.name)),
                progress_fraction: 0.1,
                log_message: format!("-> Snapper: Creating pre-transaction snapshot for {}...", pkg.name),
            })
            .await;
        let snap_id = self.snapshots.create_pre_snapshot(&pkg.name).await?;
        tokio::time::sleep(Duration::from_millis(150)).await;

        // 2. Syncing & Dep Check
        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::SyncingDatabase,
                progress_fraction: 0.25,
                log_message: "-> libalpm: Syncing repository database [world]...".to_string(),
            })
            .await;
        tokio::time::sleep(Duration::from_millis(120)).await;

        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::CheckingDependencies,
                progress_fraction: 0.35,
                log_message: "-> Checking dependency satisfaction...".to_string(),
            })
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 3. Downloading
        let total_bytes = pkg.size_download.unwrap_or(25_000_000);
        let steps = 4;
        for i in 1..=steps {
            let current = (total_bytes / steps as u64) * i as u64;
            let frac = 0.35 + (0.35 * (i as f32 / steps as f32));
            let _ = progress_tx
                .send(TransactionEvent {
                    step: TransactionStep::Downloading {
                        current_bytes: current,
                        total_bytes,
                        speed_bps: 8_500_000,
                    },
                    progress_fraction: frac,
                    log_message: format!("-> Downloading {} ({}/{} bytes)", pkg.name, current, total_bytes),
                })
                .await;
            tokio::time::sleep(Duration::from_millis(120)).await;
        }

        // 4. Checking integrity & conflicts
        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::CheckingIntegrity,
                progress_fraction: 0.75,
                log_message: "-> Verifying package signature and sha256 checksums...".to_string(),
            })
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 5. Applying
        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::ApplyingChanges {
                    package: pkg.name.clone(),
                    index: 1,
                    total: 1,
                },
                progress_fraction: 0.85,
                log_message: format!("-> Extracting and committing files for {}...", pkg.name),
            })
            .await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        // 6. Post hooks & Snapper finalize
        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::RunningHooks("Updating desktop database and icon caches".into()),
                progress_fraction: 0.92,
                log_message: "-> Running post-transaction triggers...".to_string(),
            })
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::FinalizingSnapshot,
                progress_fraction: 0.98,
                log_message: format!("-> Snapper: Creating post-transaction snapshot paired with #{snap_id}"),
            })
            .await;
        let _ = self.snapshots.create_post_snapshot(snap_id, &pkg.name).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Update package state
        {
            let mut pkgs = self.packages.lock().unwrap();
            if let Some(target) = pkgs.get_mut(id) {
                target.state = PackageState::Installed;
                target.installed_version = Some(target.version.clone());
            }
        }

        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::Completed,
                progress_fraction: 1.0,
                log_message: format!("-> Finished installing {}!", pkg.name),
            })
            .await;

        Ok(())
    }

    async fn remove(
        &self,
        id: &PackageId,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let pkg_opt = {
            let pkgs = self.packages.lock().unwrap();
            pkgs.get(id).cloned()
        };

        let pkg = pkg_opt.ok_or_else(|| PastorError::PackageNotFound(id.name.clone()))?;

        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::CreatingSnapshot(format!("Pre-remove {}", pkg.name)),
                progress_fraction: 0.2,
                log_message: format!("-> Snapper: Creating pre-removal snapshot for {}...", pkg.name),
            })
            .await;
        let snap_id = self.snapshots.create_pre_snapshot(&format!("Remove {}", pkg.name)).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::ApplyingChanges {
                    package: pkg.name.clone(),
                    index: 1,
                    total: 1,
                },
                progress_fraction: 0.6,
                log_message: format!("-> Removing files for {}...", pkg.name),
            })
            .await;
        tokio::time::sleep(Duration::from_millis(150)).await;

        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::FinalizingSnapshot,
                progress_fraction: 0.9,
                log_message: format!("-> Snapper: Post-removal snapshot paired with #{snap_id}"),
            })
            .await;
        let _ = self.snapshots.create_post_snapshot(snap_id, &format!("Remove {}", pkg.name)).await?;

        // Update state
        {
            let mut pkgs = self.packages.lock().unwrap();
            if let Some(target) = pkgs.get_mut(id) {
                target.state = PackageState::NotInstalled;
                target.installed_version = None;
            }
        }

        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::Completed,
                progress_fraction: 1.0,
                log_message: format!("-> Successfully removed {}!", pkg.name),
            })
            .await;

        Ok(())
    }

    async fn downgrade_versions(&self, _id: &PackageId) -> Result<Vec<String>, PastorError> {
        Ok(vec![
            "1.34.0-1".to_string(),
            "1.33.2-1".to_string(),
            "1.32.1-2".to_string(),
            "1.30.0-1".to_string(),
        ])
    }

    async fn downgrade(
        &self,
        id: &PackageId,
        target_version: &str,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let pkg_opt = {
            let pkgs = self.packages.lock().unwrap();
            pkgs.get(id).cloned()
        };

        let pkg = pkg_opt.ok_or_else(|| PastorError::PackageNotFound(id.name.clone()))?;

        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::CreatingSnapshot(format!("Pre-downgrade {} to {}", pkg.name, target_version)),
                progress_fraction: 0.2,
                log_message: format!("-> Snapper: Creating snapshot before downgrade to {target_version}..."),
            })
            .await;
        let snap_id = self.snapshots.create_pre_snapshot(&format!("Downgrade {}", pkg.name)).await?;
        tokio::time::sleep(Duration::from_millis(150)).await;

        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::Downloading {
                    current_bytes: 15_000_000,
                    total_bytes: 15_000_000,
                    speed_bps: 12_000_000,
                },
                progress_fraction: 0.5,
                log_message: format!("-> Fetching historical package from Parch Linux Archive for {target_version}..."),
            })
            .await;
        tokio::time::sleep(Duration::from_millis(150)).await;

        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::ApplyingChanges {
                    package: pkg.name.clone(),
                    index: 1,
                    total: 1,
                },
                progress_fraction: 0.8,
                log_message: format!("-> Reverting {} to {target_version}...", pkg.name),
            })
            .await;
        tokio::time::sleep(Duration::from_millis(150)).await;

        let _ = self.snapshots.create_post_snapshot(snap_id, &format!("Downgrade {}", pkg.name)).await?;

        {
            let mut pkgs = self.packages.lock().unwrap();
            if let Some(target) = pkgs.get_mut(id) {
                target.installed_version = Some(target_version.to_string());
                target.state = PackageState::UpdateAvailable;
            }
        }

        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::Completed,
                progress_fraction: 1.0,
                log_message: format!("-> Downgraded {} to {target_version} successfully!", pkg.name),
            })
            .await;

        Ok(())
    }
}

fn get_seed_packages() -> Vec<Package> {
    vec![
        // Parch [world] stable packages
        Package {
            id: PackageId::new("parch-welcome", PackageSource::Parch(ParchRepoType::World)),
            name: "parch-welcome".to_string(),
            display_name: Some("Parch Welcome".to_string()),
            version: "2.4.0-1".to_string(),
            installed_version: Some("2.4.0-1".to_string()),
            summary: "Welcome and initial configuration wizard for ParchLinux".to_string(),
            description: Some("Parch Welcome provides an easy, intuitive onboarding experience for new ParchLinux installations. Select desktop components, configure mirrors, enable Flatpak and Snapper snapshots with a few clicks.".to_string()),
            icon: Some(PackageIcon::Themed("preferences-system".to_string())),
            screenshots: vec![],
            homepage: Some("https://parchlinux.com".to_string()),
            license: Some("GPL-3.0-or-later".to_string()),
            maintainer: Some("ParchLinux Core Team".to_string()),
            categories: vec![PackageCategory::ParchPicks, PackageCategory::Featured, PackageCategory::System, PackageCategory::Utilities],
            size_installed: Some(3_200_000),
            size_download: Some(950_000),
            changelog: None,
            dependencies: vec!["gtk4".into(), "libadwaita".into(), "python".into()],
            state: PackageState::Installed,
        },
        Package {
            id: PackageId::new("parch-software-center", PackageSource::Parch(ParchRepoType::World)),
            name: "parch-software-center".to_string(),
            display_name: Some("Parch Software Center (Pastor)".to_string()),
            version: "0.9.0-1".to_string(),
            installed_version: Some("0.8.4-2".to_string()),
            summary: "Native graphical package manager and app store for ParchLinux".to_string(),
            description: Some("Pastor is the native frontend for ParchLinux package management, integrating ALPM [world] and [void] repositories, AUR, Flatpak, Snapper safety snapshots, and bootc immutable updates.".to_string()),
            icon: Some(PackageIcon::Themed("system-software-install".to_string())),
            screenshots: vec![],
            homepage: Some("https://parchlinux.com".to_string()),
            license: Some("GPL-3.0-or-later".to_string()),
            maintainer: Some("ParchLinux Team".to_string()),
            categories: vec![PackageCategory::ParchPicks, PackageCategory::Featured, PackageCategory::System],
            size_installed: Some(18_500_000),
            size_download: Some(5_200_000),
            changelog: None,
            dependencies: vec!["libalpm".into(), "libflatpak".into(), "gtk4".into(), "libadwaita".into()],
            state: PackageState::UpdateAvailable,
        },
        Package {
            id: PackageId::new("parch-tweaks", PackageSource::Parch(ParchRepoType::World)),
            name: "parch-tweaks".to_string(),
            display_name: Some("Parch Tweaks".to_string()),
            version: "1.2.0-1".to_string(),
            installed_version: None,
            summary: "Fine-tune system settings, themes, and extensions for ParchLinux".to_string(),
            description: Some("Customize themes, fonts, startup services, gaming tweaks, and performance profiles effortlessly.".to_string()),
            icon: Some(PackageIcon::Themed("preferences-system".to_string())),
            screenshots: vec![],
            homepage: Some("https://parchlinux.com".to_string()),
            license: Some("GPL-3.0-or-later".to_string()),
            maintainer: Some("ParchLinux Team".to_string()),
            categories: vec![PackageCategory::ParchPicks, PackageCategory::System, PackageCategory::Utilities],
            size_installed: Some(5_800_000),
            size_download: Some(1_400_000),
            changelog: None,
            dependencies: vec!["python".into(), "gtk4".into()],
            state: PackageState::NotInstalled,
        },

        // Parch [void] unstable / bleeding-edge
        Package {
            id: PackageId::new("linux-parch-void", PackageSource::Parch(ParchRepoType::Void)),
            name: "linux-parch-void".to_string(),
            display_name: Some("Linux Kernel (Parch Bleeding Void)".to_string()),
            version: "6.14.rc2-1".to_string(),
            installed_version: None,
            summary: "Experimental bleeding-edge custom Linux kernel with latest patches".to_string(),
            description: Some("The [void] branch custom kernel optimized for high-throughput I/O, Btrfs enhancements, low latency, and experimental driver backports.".to_string()),
            icon: Some(PackageIcon::Themed("system-software-update".to_string())),
            screenshots: vec![],
            homepage: Some("https://parchlinux.com".to_string()),
            license: Some("GPL-2.0-only".to_string()),
            maintainer: Some("Parch Void Kernel Maintainers".to_string()),
            categories: vec![PackageCategory::System],
            size_installed: Some(140_000_000),
            size_download: Some(42_000_000),
            changelog: None,
            dependencies: vec!["kmod".into(), "initramfs".into()],
            state: PackageState::NotInstalled,
        },
        Package {
            id: PackageId::new("hyprland-void-git", PackageSource::Parch(ParchRepoType::Void)),
            name: "hyprland-void-git".to_string(),
            display_name: Some("Hyprland Compositor (Void Git)".to_string()),
            version: "0.45.0.r12-1".to_string(),
            installed_version: None,
            summary: "Dynamic tiling Wayland compositor that doesn't sacrifice on looks (bleeding-edge)".to_string(),
            description: Some("Compiled directly from master branch with Parch theme integrations and custom window animations.".to_string()),
            icon: Some(PackageIcon::Themed("preferences-desktop-display".to_string())),
            screenshots: vec![],
            homepage: Some("https://hyprland.org".to_string()),
            license: Some("BSD-3-Clause".to_string()),
            maintainer: Some("Parch Void Maintainers".to_string()),
            categories: vec![PackageCategory::System, PackageCategory::Utilities],
            size_installed: Some(28_000_000),
            size_download: Some(8_500_000),
            changelog: None,
            dependencies: vec!["wayland".into(), "libdrm".into(), "aquamarine".into()],
            state: PackageState::NotInstalled,
        },

        // Arch upstream packages
        Package {
            id: PackageId::new("firefox", PackageSource::Arch("extra".to_string())),
            name: "firefox".to_string(),
            display_name: Some("Firefox Browser".to_string()),
            version: "135.0-1".to_string(),
            installed_version: Some("135.0-1".to_string()),
            summary: "Fast, Private & Safe Web Browser".to_string(),
            description: Some("Firefox is the independent, people-first browser backed by Mozilla. Enjoy blazing fast browsing with strict tracking protection, containers, and rich web extensions.".to_string()),
            icon: Some(PackageIcon::Themed("firefox".to_string())),
            screenshots: vec![],
            homepage: Some("https://www.mozilla.org/firefox/".to_string()),
            license: Some("MPL-2.0".to_string()),
            maintainer: Some("Arch Linux Package Maintainers".to_string()),
            categories: vec![PackageCategory::Internet, PackageCategory::Featured],
            size_installed: Some(240_000_000),
            size_download: Some(68_000_000),
            changelog: None,
            dependencies: vec!["gtk3".into(), "nss".into(), "alsa-lib".into()],
            state: PackageState::Installed,
        },
        Package {
            id: PackageId::new("neovim", PackageSource::Arch("extra".to_string())),
            name: "neovim".to_string(),
            display_name: Some("Neovim Editor".to_string()),
            version: "0.10.4-1".to_string(),
            installed_version: Some("0.10.3-1".to_string()),
            summary: "Vim-fork focused on extensibility and usability".to_string(),
            description: Some("Neovim is a hyperextensible Vim-based text editor built for high-performance development, supporting Lua plugins, treesitter, and native LSP.".to_string()),
            icon: Some(PackageIcon::Themed("accessories-text-editor".to_string())),
            screenshots: vec![],
            homepage: Some("https://neovim.io/".to_string()),
            license: Some("Apache-2.0".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Development, PackageCategory::Featured],
            size_installed: Some(32_000_000),
            size_download: Some(8_200_000),
            changelog: None,
            dependencies: vec!["lua51".into(), "luajit".into(), "libvterm".into()],
            state: PackageState::UpdateAvailable,
        },
        Package {
            id: PackageId::new("vlc", PackageSource::Arch("extra".to_string())),
            name: "vlc".to_string(),
            display_name: Some("VLC Media Player".to_string()),
            version: "3.0.21-3".to_string(),
            installed_version: None,
            summary: "Multi-platform MPEG, VCD/DVD, and DivX player".to_string(),
            description: Some("VLC is a free and open-source cross-platform multimedia player and framework that plays most multimedia files, as well as DVDs, Audio CDs, VCDs, and various streaming protocols.".to_string()),
            icon: Some(PackageIcon::Themed("vlc".to_string())),
            screenshots: vec![],
            homepage: Some("https://www.videolan.org/vlc/".to_string()),
            license: Some("GPL-2.0-or-later".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Multimedia],
            size_installed: Some(85_000_000),
            size_download: Some(19_000_000),
            changelog: None,
            dependencies: vec!["ffmpeg".into(), "qt5-base".into()],
            state: PackageState::NotInstalled,
        },
        Package {
            id: PackageId::new("gimp", PackageSource::Arch("extra".to_string())),
            name: "gimp".to_string(),
            display_name: Some("GNU Image Manipulation Program".to_string()),
            version: "3.0.0-1".to_string(),
            installed_version: None,
            summary: "Create and edit images, graphics, and photos".to_string(),
            description: Some("GIMP is an advanced image manipulation tool suitable for tasks like photo retouching, image composition, and image construction.".to_string()),
            icon: Some(PackageIcon::Themed("gimp".to_string())),
            screenshots: vec![],
            homepage: Some("https://www.gimp.org/".to_string()),
            license: Some("GPL-3.0-or-later".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Graphics],
            size_installed: Some(120_000_000),
            size_download: Some(34_000_000),
            changelog: None,
            dependencies: vec!["gtk3".into(), "gegl".into(), "babl".into()],
            state: PackageState::NotInstalled,
        },


        // AUR packages
        Package {
            id: PackageId::new("spotify", PackageSource::Aur),
            name: "spotify".to_string(),
            display_name: Some("Spotify Music".to_string()),
            version: "1.2.53-1".to_string(),
            installed_version: None,
            summary: "Proprietary music streaming service client".to_string(),
            description: Some("Spotify gives you instant access to millions of songs - from old favorites to the latest hits. Just hit play to stream anything you like.".to_string()),
            icon: Some(PackageIcon::Themed("spotify-client".to_string())),
            screenshots: vec![],
            homepage: Some("https://www.spotify.com".to_string()),
            license: Some("Proprietary".to_string()),
            maintainer: Some("AUR Community".to_string()),
            categories: vec![PackageCategory::Multimedia, PackageCategory::Featured],
            size_installed: Some(180_000_000),
            size_download: Some(55_000_000),
            changelog: None,
            dependencies: vec!["alsa-lib".into(), "nss".into()],
            state: PackageState::NotInstalled,
        },
        Package {
            id: PackageId::new("visual-studio-code-bin", PackageSource::Aur),
            name: "visual-studio-code-bin".to_string(),
            display_name: Some("Visual Studio Code".to_string()),
            version: "1.97.2-1".to_string(),
            installed_version: None,
            summary: "Code editing. Redefined. Official binary release".to_string(),
            description: Some("Visual Studio Code is a lightweight but powerful source code editor which runs on your desktop and comes with built-in support for JavaScript, TypeScript and Node.js.".to_string()),
            icon: Some(PackageIcon::Themed("com.visualstudio.code".to_string())),
            screenshots: vec![],
            homepage: Some("https://code.visualstudio.com/".to_string()),
            license: Some("Proprietary".to_string()),
            maintainer: Some("AUR Community".to_string()),
            categories: vec![PackageCategory::Development, PackageCategory::Featured],
            size_installed: Some(320_000_000),
            size_download: Some(98_000_000),
            changelog: None,
            dependencies: vec!["gtk3".into(), "libsecret".into()],
            state: PackageState::NotInstalled,
        },

        // bootc OS Image Package
        Package {
            id: PackageId::new("parchlinux-immutable-system", PackageSource::Bootc),
            name: "parchlinux-immutable-system".to_string(),
            display_name: Some("ParchLinux Immutable OS Image".to_string()),
            version: "2026.09.01".to_string(),
            installed_version: Some("2026.08.15".to_string()),
            summary: "Transactional OCI container-based base OS deployment via bootc".to_string(),
            description: Some("The core immutable operating system of ParchLinux. Managed transactionally through bootc with atomic staging, verifiable digests, and instant reboot rollback.".to_string()),
            icon: Some(PackageIcon::Themed("computer".to_string())),
            screenshots: vec![],
            homepage: Some("https://parchlinux.com/immutable".to_string()),
            license: Some("GPL-3.0-or-later".to_string()),
            maintainer: Some("ParchLinux Immutable SIG".to_string()),
            categories: vec![PackageCategory::System, PackageCategory::Featured],
            size_installed: Some(3_200_000_000),
            size_download: Some(850_000_000),
            changelog: None,
            dependencies: vec!["bootc".into(), "systemd".into(), "snapper".into()],
            state: PackageState::UpdateAvailable,
        },

        // Waydroid Android apps
        Package {
            id: PackageId::new("org.fdroid.fdroid", PackageSource::Waydroid),
            name: "org.fdroid.fdroid".to_string(),
            display_name: Some("F-Droid (Android)".to_string()),
            version: "1.21.0".to_string(),
            installed_version: Some("1.21.0".to_string()),
            summary: "Free and Open Source Android Application Repository".to_string(),
            description: Some("F-Droid is an installable catalogue of FOSS (Free and Open Source Software) applications for the Android platform running inside Waydroid.".to_string()),
            icon: Some(PackageIcon::Themed("phone".to_string())),
            screenshots: vec![],
            homepage: Some("https://f-droid.org".to_string()),
            license: Some("GPL-3.0-or-later".to_string()),
            maintainer: Some("F-Droid Authors".to_string()),
            categories: vec![PackageCategory::Android, PackageCategory::Utilities],
            size_installed: Some(35_000_000),
            size_download: Some(12_000_000),
            changelog: None,
            dependencies: vec!["waydroid".into()],
            state: PackageState::Installed,
        },
        Package {
            id: PackageId::new("org.schabi.newpipe", PackageSource::Waydroid),
            name: "org.schabi.newpipe".to_string(),
            display_name: Some("NewPipe (Android)".to_string()),
            version: "0.27.2".to_string(),
            installed_version: None,
            summary: "Lightweight, privacy-focused streaming frontend for Android".to_string(),
            description: Some("NewPipe has been created with the purpose of getting the original YouTube streaming experience on your device without annoying ads and questionable permissions.".to_string()),
            icon: Some(PackageIcon::Themed("multimedia-player".to_string())),
            screenshots: vec![],
            homepage: Some("https://newpipe.net".to_string()),
            license: Some("GPL-3.0-or-later".to_string()),
            maintainer: Some("NewPipe Team".to_string()),
            categories: vec![PackageCategory::Android, PackageCategory::Multimedia],
            size_installed: Some(42_000_000),
            size_download: Some(15_000_000),
            changelog: None,
            dependencies: vec!["waydroid".into()],
            state: PackageState::NotInstalled,
        },
        Package {
            id: PackageId::new("com.aurora.store", PackageSource::Waydroid),
            name: "com.aurora.store".to_string(),
            display_name: Some("Aurora Store (Android)".to_string()),
            version: "4.5.1".to_string(),
            installed_version: None,
            summary: "Open source alternative Google Play frontend for Waydroid".to_string(),
            description: Some("Aurora Store is an unofficial, FOSS client to Google's Play Store with an elegant design. Download apps, update apps, search for apps, and get details about app tracker and permissions.".to_string()),
            icon: Some(PackageIcon::Themed("system-software-install".to_string())),
            screenshots: vec![],
            homepage: Some("https://auroraoss.com".to_string()),
            license: Some("GPL-3.0-or-later".to_string()),
            maintainer: Some("AuroraOSS Team".to_string()),
            categories: vec![PackageCategory::Android, PackageCategory::Utilities],
            size_installed: Some(28_000_000),
            size_download: Some(9_000_000),
            changelog: None,
            dependencies: vec!["waydroid".into()],
            state: PackageState::NotInstalled,
        },
        Package {
            id: PackageId::new("app.organicmaps", PackageSource::Waydroid),
            name: "app.organicmaps".to_string(),
            display_name: Some("Organic Maps (Android)".to_string()),
            version: "2026.02.10".to_string(),
            installed_version: None,
            summary: "Fast detailed offline maps for travelers, cyclists, and hikers".to_string(),
            description: Some("Organic Maps is a free Android & Linux offline map app based on crowd-sourced OpenStreetMap data. No tracking, no ads, completely privacy-focused.".to_string()),
            icon: Some(PackageIcon::Themed("phone".to_string())),
            screenshots: vec![],
            homepage: Some("https://organicmaps.app".to_string()),
            license: Some("Apache-2.0".to_string()),
            maintainer: Some("Organic Maps Authors".to_string()),
            categories: vec![PackageCategory::Android, PackageCategory::Utilities],
            size_installed: Some(95_000_000),
            size_download: Some(45_000_000),
            changelog: None,
            dependencies: vec!["waydroid".into()],
            state: PackageState::NotInstalled,
        },

        // Development Category
        Package {
            id: PackageId::new("rust", PackageSource::Arch("extra".to_string())),
            name: "rust".to_string(),
            display_name: Some("Rust Toolchain".to_string()),
            version: "1.95.0-1".to_string(),
            installed_version: Some("1.95.0-1".to_string()),
            summary: "Empowering everyone to build reliable and efficient software".to_string(),
            description: Some("Rust is a systems programming language that runs blazingly fast, prevents segfaults, and guarantees thread safety.".to_string()),
            icon: Some(PackageIcon::Themed("applications-development".to_string())),
            screenshots: vec![],
            homepage: Some("https://www.rust-lang.org/".to_string()),
            license: Some("Apache-2.0 OR MIT".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Development],
            size_installed: Some(310_000_000),
            size_download: Some(78_000_000),
            changelog: None,
            dependencies: vec!["llvm-libs".into(), "gcc".into()],
            state: PackageState::Installed,
        },
        Package {
            id: PackageId::new("docker", PackageSource::Arch("extra".to_string())),
            name: "docker".to_string(),
            display_name: Some("Docker Engine".to_string()),
            version: "28.0.1-1".to_string(),
            installed_version: None,
            summary: "Pack, ship and run any application as a lightweight container".to_string(),
            description: Some("Docker provides an open platform for developing, shipping, and running applications in isolated containers.".to_string()),
            icon: Some(PackageIcon::Themed("applications-development".to_string())),
            screenshots: vec![],
            homepage: Some("https://www.docker.com/".to_string()),
            license: Some("Apache-2.0".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Development, PackageCategory::System],
            size_installed: Some(92_000_000),
            size_download: Some(24_000_000),
            changelog: None,
            dependencies: vec!["containerd".into(), "runc".into()],
            state: PackageState::NotInstalled,
        },

        // Games Category
        Package {
            id: PackageId::new("lutris", PackageSource::Arch("extra".to_string())),
            name: "lutris".to_string(),
            display_name: Some("Lutris Gaming Platform".to_string()),
            version: "0.5.18-1".to_string(),
            installed_version: None,
            summary: "Open source gaming platform for GNU/Linux".to_string(),
            description: Some("Lutris helps you install and manage your games across GOG, Steam, Epic Games Store, emulators, and Wine bottles from a single unified library.".to_string()),
            icon: Some(PackageIcon::Themed("applications-games".to_string())),
            screenshots: vec![],
            homepage: Some("https://lutris.net/".to_string()),
            license: Some("GPL-3.0-or-later".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Games],
            size_installed: Some(22_000_000),
            size_download: Some(4_500_000),
            changelog: None,
            dependencies: vec!["python".into(), "wine".into()],
            state: PackageState::NotInstalled,
        },
        Package {
            id: PackageId::new("prismlauncher", PackageSource::Arch("extra".to_string())),
            name: "prismlauncher".to_string(),
            display_name: Some("Prism Launcher".to_string()),
            version: "9.2-1".to_string(),
            installed_version: None,
            summary: "A custom launcher for Minecraft that allows you to easily manage multiple installations".to_string(),
            description: Some("Prism Launcher is an open source Minecraft launcher with the ability to manage multiple instances, modpacks, CurseForge, and Modrinth directly.".to_string()),
            icon: Some(PackageIcon::Themed("applications-games".to_string())),
            screenshots: vec![],
            homepage: Some("https://prismlauncher.org/".to_string()),
            license: Some("GPL-3.0-only".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Games],
            size_installed: Some(34_000_000),
            size_download: Some(9_200_000),
            changelog: None,
            dependencies: vec!["qt6-base".into()],
            state: PackageState::NotInstalled,
        },
        Package {
            id: PackageId::new("retroarch", PackageSource::Arch("extra".to_string())),
            name: "retroarch".to_string(),
            display_name: Some("RetroArch Multi-Emulator".to_string()),
            version: "1.20.0-1".to_string(),
            installed_version: None,
            summary: "Reference frontend for the libretro API".to_string(),
            description: Some("RetroArch enables you to run classic games on a wide range of computers and consoles through its slick graphical interface.".to_string()),
            icon: Some(PackageIcon::Themed("applications-games".to_string())),
            screenshots: vec![],
            homepage: Some("https://www.retroarch.com/".to_string()),
            license: Some("GPL-3.0-only".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Games],
            size_installed: Some(45_000_000),
            size_download: Some(16_000_000),
            changelog: None,
            dependencies: vec!["ffmpeg".into(), "libdrm".into()],
            state: PackageState::NotInstalled,
        },

        // Graphics Category
        Package {
            id: PackageId::new("blender", PackageSource::Arch("extra".to_string())),
            name: "blender".to_string(),
            display_name: Some("Blender 3D Suite".to_string()),
            version: "4.3.2-1".to_string(),
            installed_version: None,
            summary: "Very fast and versatile 3D modeller, renderer and animator".to_string(),
            description: Some("Blender is the free and open source 3D creation suite supporting the entirety of the 3D pipeline—modeling, rigging, animation, simulation, rendering, and motion tracking.".to_string()),
            icon: Some(PackageIcon::Themed("applications-graphics".to_string())),
            screenshots: vec![],
            homepage: Some("https://www.blender.org/".to_string()),
            license: Some("GPL-3.0-or-later".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Graphics, PackageCategory::Featured],
            size_installed: Some(450_000_000),
            size_download: Some(110_000_000),
            changelog: None,
            dependencies: vec!["python".into(), "openexr".into(), "libgl".into()],
            state: PackageState::NotInstalled,
        },
        Package {
            id: PackageId::new("inkscape", PackageSource::Arch("extra".to_string())),
            name: "inkscape".to_string(),
            display_name: Some("Inkscape Vector Editor".to_string()),
            version: "1.4.1-1".to_string(),
            installed_version: None,
            summary: "Professional vector graphics editor for SVG".to_string(),
            description: Some("Inkscape is a professional quality vector graphics software which runs on Linux, macOS and Windows desktop computers.".to_string()),
            icon: Some(PackageIcon::Themed("applications-graphics".to_string())),
            screenshots: vec![],
            homepage: Some("https://inkscape.org/".to_string()),
            license: Some("GPL-2.0-or-later".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Graphics],
            size_installed: Some(180_000_000),
            size_download: Some(40_000_000),
            changelog: None,
            dependencies: vec!["gtkmm3".into(), "poppler".into()],
            state: PackageState::NotInstalled,
        },
        Package {
            id: PackageId::new("krita", PackageSource::Arch("extra".to_string())),
            name: "krita".to_string(),
            display_name: Some("Krita Digital Painting".to_string()),
            version: "5.2.6-1".to_string(),
            installed_version: None,
            summary: "Free and open source digital painting studio".to_string(),
            description: Some("Krita is a professional free and open source painting program designed for concept artists, illustrators, matte and texture artists, and the VFX industry.".to_string()),
            icon: Some(PackageIcon::Themed("applications-graphics".to_string())),
            screenshots: vec![],
            homepage: Some("https://krita.org/".to_string()),
            license: Some("GPL-3.0-or-later".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Graphics],
            size_installed: Some(210_000_000),
            size_download: Some(58_000_000),
            changelog: None,
            dependencies: vec!["qt5-base".into(), "kio".into()],
            state: PackageState::NotInstalled,
        },

        // Internet Category
        Package {
            id: PackageId::new("telegram-desktop", PackageSource::Arch("extra".to_string())),
            name: "telegram-desktop".to_string(),
            display_name: Some("Telegram Desktop".to_string()),
            version: "5.10.3-1".to_string(),
            installed_version: Some("5.10.3-1".to_string()),
            summary: "Official Telegram desktop messaging client".to_string(),
            description: Some("Fast and secure desktop messaging app, perfectly synced with your mobile phone.".to_string()),
            icon: Some(PackageIcon::Themed("applications-internet".to_string())),
            screenshots: vec![],
            homepage: Some("https://desktop.telegram.org/".to_string()),
            license: Some("GPL-3.0-only".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Internet],
            size_installed: Some(95_000_000),
            size_download: Some(36_000_000),
            changelog: None,
            dependencies: vec!["qt6-base".into(), "ffmpeg".into()],
            state: PackageState::Installed,
        },
        Package {
            id: PackageId::new("thunderbird", PackageSource::Arch("extra".to_string())),
            name: "thunderbird".to_string(),
            display_name: Some("Thunderbird Mail".to_string()),
            version: "128.7.0-1".to_string(),
            installed_version: None,
            summary: "Standalone mail, news and calendar client from Mozilla".to_string(),
            description: Some("Thunderbird makes email better for you, bringing together speed, privacy and the latest technologies in a free email client.".to_string()),
            icon: Some(PackageIcon::Themed("applications-internet".to_string())),
            screenshots: vec![],
            homepage: Some("https://www.thunderbird.net/".to_string()),
            license: Some("MPL-2.0".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Internet, PackageCategory::Office],
            size_installed: Some(190_000_000),
            size_download: Some(56_000_000),
            changelog: None,
            dependencies: vec!["gtk3".into(), "nss".into()],
            state: PackageState::NotInstalled,
        },

        // Multimedia Category
        Package {
            id: PackageId::new("mpv", PackageSource::Arch("extra".to_string())),
            name: "mpv".to_string(),
            display_name: Some("mpv Media Player".to_string()),
            version: "0.39.0-1".to_string(),
            installed_version: Some("0.39.0-1".to_string()),
            summary: "Command line video player based on MPlayer and mplayer2".to_string(),
            description: Some("mpv is a free, open source, and cross-platform media player with GPU video decoding, high quality on-screen display, and Lua scriptability.".to_string()),
            icon: Some(PackageIcon::Themed("applications-multimedia".to_string())),
            screenshots: vec![],
            homepage: Some("https://mpv.io/".to_string()),
            license: Some("GPL-2.0-or-later".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Multimedia],
            size_installed: Some(12_000_000),
            size_download: Some(3_800_000),
            changelog: None,
            dependencies: vec!["ffmpeg".into(), "libplacebo".into()],
            state: PackageState::Installed,
        },
        Package {
            id: PackageId::new("obs-studio", PackageSource::Arch("extra".to_string())),
            name: "obs-studio".to_string(),
            display_name: Some("OBS Studio".to_string()),
            version: "31.0.1-1".to_string(),
            installed_version: None,
            summary: "Free, open source software for live streaming and recording".to_string(),
            description: Some("OBS Studio is equipped with a powerful API, enabling plugins and scripts to provide further customization and functionality specific to your needs.".to_string()),
            icon: Some(PackageIcon::Themed("applications-multimedia".to_string())),
            screenshots: vec![],
            homepage: Some("https://obsproject.com/".to_string()),
            license: Some("GPL-2.0-or-later".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Multimedia],
            size_installed: Some(85_000_000),
            size_download: Some(21_000_000),
            changelog: None,
            dependencies: vec!["qt6-base".into(), "ffmpeg".into(), "pipewire".into()],
            state: PackageState::NotInstalled,
        },

        // Office Category
        Package {
            id: PackageId::new("libreoffice-fresh", PackageSource::Arch("extra".to_string())),
            name: "libreoffice-fresh".to_string(),
            display_name: Some("LibreOffice Office Suite".to_string()),
            version: "25.2.0-1".to_string(),
            installed_version: None,
            summary: "Comprehensive, professional-quality open-source productivity suite".to_string(),
            description: Some("LibreOffice is a powerful and free office suite, used by millions of people around the world. Includes Writer, Calc, Impress, Draw, and Base.".to_string()),
            icon: Some(PackageIcon::Themed("applications-office".to_string())),
            screenshots: vec![],
            homepage: Some("https://www.libreoffice.org/".to_string()),
            license: Some("MPL-2.0".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::Office],
            size_installed: Some(540_000_000),
            size_download: Some(150_000_000),
            changelog: None,
            dependencies: vec!["java-runtime".into(), "libxslt".into()],
            state: PackageState::NotInstalled,
        },

        // System Category
        Package {
            id: PackageId::new("btop", PackageSource::Arch("extra".to_string())),
            name: "btop".to_string(),
            display_name: Some("btop System Monitor".to_string()),
            version: "1.4.0-1".to_string(),
            installed_version: Some("1.4.0-1".to_string()),
            summary: "Resource monitor that shows usage and stats for processor, memory, disks, network and processes".to_string(),
            description: Some("Fast, beautiful C++ resource monitor with customizable themes, game controller support, and comprehensive hardware monitoring graphs.".to_string()),
            icon: Some(PackageIcon::Themed("applications-system".to_string())),
            screenshots: vec![],
            homepage: Some("https://github.com/aristocratos/btop".to_string()),
            license: Some("Apache-2.0".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::System, PackageCategory::Utilities],
            size_installed: Some(3_800_000),
            size_download: Some(1_100_000),
            changelog: None,
            dependencies: vec!["gcc-libs".into()],
            state: PackageState::Installed,
        },
        Package {
            id: PackageId::new("fastfetch", PackageSource::Arch("extra".to_string())),
            name: "fastfetch".to_string(),
            display_name: Some("Fastfetch".to_string()),
            version: "2.38.0-1".to_string(),
            installed_version: Some("2.38.0-1".to_string()),
            summary: "Extremely fast system information tool written in C".to_string(),
            description: Some("Fastfetch is a neofetch-like tool for fetching system information and displaying it prettily. Highly performant and feature-rich.".to_string()),
            icon: Some(PackageIcon::Themed("applications-system".to_string())),
            screenshots: vec![],
            homepage: Some("https://github.com/fastfetch-cli/fastfetch".to_string()),
            license: Some("MIT".to_string()),
            maintainer: Some("Arch Linux Maintainers".to_string()),
            categories: vec![PackageCategory::System, PackageCategory::Utilities],
            size_installed: Some(2_400_000),
            size_download: Some(650_000),
            changelog: None,
            dependencies: vec!["glibc".into()],
            state: PackageState::Installed,
        },
        Package {
            id: PackageId::new("snapper-gui", PackageSource::Aur),
            name: "snapper-gui".to_string(),
            display_name: Some("Snapper GUI".to_string()),
            version: "0.8.2-1".to_string(),
            installed_version: None,
            summary: "Graphical user interface for Btrfs Snapper snapshot tool".to_string(),
            description: Some("Inspect Btrfs snapshots, diff file system changes, and perform rollbacks visually with Snapper.".to_string()),
            icon: Some(PackageIcon::Themed("camera-photo".to_string())),
            screenshots: vec![],
            homepage: Some("https://github.com/ricardomv/snapper-gui".to_string()),
            license: Some("GPL-2.0-only".to_string()),
            maintainer: Some("AUR Maintainers".to_string()),
            categories: vec![PackageCategory::System, PackageCategory::Utilities],
            size_installed: Some(6_200_000),
            size_download: Some(1_800_000),
            changelog: None,
            dependencies: vec!["snapper".into(), "python".into()],
            state: PackageState::NotInstalled,
        },
    ]
}


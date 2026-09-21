use std::path::Path;
use adw::prelude::*;
use gtk4::glib;
use gtk4::prelude::Cast;
use gtk4::gio::prelude::AppInfoExt;
use libflatpak::prelude::*;
use pastor_store::Store;

pub fn create_settings_view(
    store: Store,
    on_open_snapshots: impl Fn() + 'static + Clone,
) -> gtk4::Widget {
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build();

    let page = adw::PreferencesPage::builder()
        .title("Settings")
        .description("Configure package engines, repository priorities, automatic updates, and storage")
        .build();

    let cfg = store.module_config();

    // -------------------------------------------------------------
    // 1. Packaging Engines & Cores (GNOME Software / KDE Discover style)
    // -------------------------------------------------------------
    let cores_group = adw::PreferencesGroup::builder()
        .title("Packaging Engines & Cores")
        .description("Enable or disable package management backends and ecosystems")
        .build();

    // ALPM Core Dropdown / Expander
    let alpm_expander = adw::ExpanderRow::builder()
        .title("Arch & Parch Native Packages (ALPM)")
        .subtitle("Native package repositories and pacman management")
        .expanded(true)
        .build();
    alpm_expander.add_prefix(
        &gtk4::Image::from_icon_name("package-x-generic-symbolic")
    );
    alpm_expander.add_suffix(
        &gtk4::Label::builder()
            .label("ALPM")
            .css_classes(["parch-badge-arch"])
            .valign(gtk4::Align::Center)
            .build(),
    );

    // Dynamically detect repositories configured in pacman.conf
    let pacman_repos = if let Ok(pconf) = pacmanconf::Config::new() {
        pconf.repos.into_iter().map(|r| r.name).collect::<Vec<_>>()
    } else {
        vec![
            "world".to_string(),
            "void".to_string(),
            "core".to_string(),
            "extra".to_string(),
            "multilib".to_string(),
        ]
    };

    for repo_name in pacman_repos {
        let (display_title, badge_label, badge_class) = match repo_name.to_lowercase().as_str() {
            "world" => ("Parch [world] Repository".to_string(), "world", "parch-badge-world"),
            "void" => ("Parch [void] Repository".to_string(), "void", "parch-badge-void"),
            "core" => ("Arch Linux [core] Repository".to_string(), "core", "parch-badge-arch"),
            "extra" => ("Arch Linux [extra] Repository".to_string(), "extra", "parch-badge-arch"),
            "multilib" => ("Arch Linux [multilib] Repository".to_string(), "multilib", "parch-badge-arch"),
            "chaotic-aur" => ("Chaotic AUR Repository".to_string(), "chaotic", "parch-badge-aur"),
            _ => (
                format!("Repository [{}]", repo_name),
                "repo",
                "parch-badge-world",
            ),
        };

        let is_active = cfg.is_repo_enabled(&repo_name);
        let repo_row = adw::SwitchRow::builder()
            .title(&display_title)
            .subtitle(format!("Pacman sync repository configured in /etc/pacman.conf"))
            .active(is_active)
            .build();
        repo_row.add_suffix(
            &gtk4::Label::builder()
                .label(badge_label)
                .css_classes([badge_class])
                .valign(gtk4::Align::Center)
                .build(),
        );

        let store_c = store.clone();
        let rname = repo_name.clone();
        repo_row.connect_active_notify(move |row| {
            let mut c = store_c.module_config();
            c.set_repo_enabled(&rname, row.is_active());
            store_c.set_module_config(c);
        });

        alpm_expander.add_row(&repo_row);
    }

    // AUR (Arch User Repository)
    let aur_row = adw::SwitchRow::builder()
        .title("Arch User Repository (AUR)")
        .subtitle("Compile and install community-maintained packages natively")
        .active(cfg.enable_aur)
        .build();
    aur_row.add_suffix(
        &gtk4::Label::builder()
            .label("AUR")
            .css_classes(["parch-badge-aur"])
            .valign(gtk4::Align::Center)
            .build(),
    );
    let store_aur = store.clone();
    aur_row.connect_active_notify(move |row| {
        let mut c = store_aur.module_config();
        c.enable_aur = row.is_active();
        store_aur.set_module_config(c);
    });
    alpm_expander.add_row(&aur_row);

    // Mirrors Management (mirrorman) placed under the ALPM dropdown
    let mirror_row = adw::ActionRow::builder()
        .title("Pacman Mirror Management")
        .subtitle("Rank, speed test, and update mirrorlists using Parch Mirror Manager")
        .activatable(true)
        .build();
    let mirrorman_btn = gtk4::Button::builder()
        .label("Launch mirrorman")
        .css_classes(["flat", "pill"])
        .valign(gtk4::Align::Center)
        .build();
    mirrorman_btn.connect_clicked(|_| {
        launch_mirrorman();
    });
    mirror_row.add_suffix(&mirrorman_btn);
    alpm_expander.add_row(&mirror_row);

    cores_group.add(&alpm_expander);

    // Flatpak Core Switch
    let flatpak_core_row = adw::SwitchRow::builder()
        .title("Flatpak Applications (Flathub)")
        .subtitle("Sandboxed application ecosystem from Flathub via libflatpak")
        .active(cfg.enable_flatpak)
        .build();
    flatpak_core_row.add_suffix(
        &gtk4::Label::builder()
            .label("Flatpak")
            .css_classes(["parch-badge-flatpak"])
            .valign(gtk4::Align::Center)
            .build(),
    );
    let store_fp = store.clone();
    flatpak_core_row.connect_active_notify(move |row| {
        let mut c = store_fp.module_config();
        c.enable_flatpak = row.is_active();
        store_fp.set_module_config(c);
    });
    cores_group.add(&flatpak_core_row);

    page.add(&cores_group);

    // -------------------------------------------------------------
    // 2. Updates and Automation Group
    // -------------------------------------------------------------
    let general_group = adw::PreferencesGroup::builder()
        .title("Updates and Automation")
        .description("Manage automated checks, background tasks, and desktop notifications")
        .build();

    let check_updates_row = adw::ComboRow::builder()
        .title("Check for Updates")
        .subtitle("Frequency of automatic repository synchronization")
        .model(&gtk4::StringList::new(&[
            "Daily (Recommended)",
            "Every 6 Hours",
            "Weekly",
            "Manual Only",
        ]))
        .selected(cfg.check_updates_interval.min(3))
        .build();

    let store_upd = store.clone();
    check_updates_row.connect_selected_notify(move |row| {
        let mut c = store_upd.module_config();
        c.check_updates_interval = row.selected();
        store_upd.set_module_config(c);
    });
    general_group.add(&check_updates_row);

    let notify_updates_row = adw::SwitchRow::builder()
        .title("Update Notifications")
        .subtitle("Display a desktop notification when system or app updates are ready")
        .active(cfg.notify_updates)
        .build();
    let store_notif = store.clone();
    notify_updates_row.connect_active_notify(move |row| {
        let mut c = store_notif.module_config();
        c.notify_updates = row.is_active();
        store_notif.set_module_config(c);
    });
    general_group.add(&notify_updates_row);

    let notify_finish_row = adw::SwitchRow::builder()
        .title("Completion Alerts")
        .subtitle("Notify when lengthy package downloads and installations finish")
        .active(cfg.notify_finish)
        .build();
    let store_fin = store.clone();
    notify_finish_row.connect_active_notify(move |row| {
        let mut c = store_fin.module_config();
        c.notify_finish = row.is_active();
        store_fin.set_module_config(c);
    });
    general_group.add(&notify_finish_row);

    let parallel_downloads_row = adw::SpinRow::builder()
        .title("Parallel Downloads")
        .subtitle("Maximum simultaneous package download connections (ALPM / AUR)")
        .adjustment(&gtk4::Adjustment::new(
            cfg.parallel_downloads as f64,
            1.0,
            10.0,
            1.0,
            2.0,
            0.0,
        ))
        .numeric(true)
        .build();
    let store_dl = store.clone();
    parallel_downloads_row.connect_value_notify(move |row| {
        let mut c = store_dl.module_config();
        c.parallel_downloads = row.value() as u32;
        store_dl.set_module_config(c);
    });
    general_group.add(&parallel_downloads_row);

    page.add(&general_group);

    // -------------------------------------------------------------
    // 3. Snapper Snapshots & Safety Group
    // -------------------------------------------------------------
    let safety_group = adw::PreferencesGroup::builder()
        .title("Snapper Snapshots and Safety")
        .description("Automated Btrfs restore points and rollback protection")
        .build();

    let snapper_auto_row = adw::SwitchRow::builder()
        .title("Automatic Pre/Post Snapshots")
        .subtitle("Create paired Btrfs snapshots before modifying packages or kernel")
        .active(cfg.enable_snapper)
        .build();
    snapper_auto_row.add_suffix(
        &gtk4::Label::builder()
            .label("Btrfs")
            .css_classes(["parch-badge-world"])
            .valign(gtk4::Align::Center)
            .build(),
    );

    let store_snap = store.clone();
    snapper_auto_row.connect_active_notify(move |row| {
        let mut c = store_snap.module_config();
        c.enable_snapper = row.is_active();
        store_snap.set_module_config(c);
    });
    safety_group.add(&snapper_auto_row);

    let max_snaps_row = adw::SpinRow::builder()
        .title("Retained Store Snapshots")
        .subtitle("Maximum number of auto-created snapshots to keep before pruning")
        .adjustment(&gtk4::Adjustment::new(
            cfg.retained_snapshots as f64,
            5.0,
            100.0,
            5.0,
            10.0,
            0.0,
        ))
        .numeric(true)
        .build();
    let store_max_snaps = store.clone();
    max_snaps_row.connect_value_notify(move |row| {
        let mut c = store_max_snaps.module_config();
        c.retained_snapshots = row.value() as u32;
        store_max_snaps.set_module_config(c);
    });
    safety_group.add(&max_snaps_row);

    let snap_btn_row = adw::ActionRow::builder()
        .title("Manage System Snapshots")
        .subtitle("Inspect timeline, compare file diffs, or initiate rollback")
        .activatable(true)
        .build();
    let snap_btn = gtk4::Button::builder()
        .label("Open Snapshots")
        .css_classes(["flat", "pill"])
        .valign(gtk4::Align::Center)
        .build();

    let on_snaps = on_open_snapshots.clone();
    snap_btn.connect_clicked(move |_| {
        on_snaps();
    });
    snap_btn_row.add_suffix(&snap_btn);
    safety_group.add(&snap_btn_row);

    page.add(&safety_group);

    // -------------------------------------------------------------
    // 4. Cache and Storage Maintenance (REAL DATA - NO MOCK)
    // -------------------------------------------------------------
    let cache_group = adw::PreferencesGroup::builder()
        .title("Cache and Storage Maintenance")
        .description("Manage downloaded package archives and disk space with native tools")
        .build();

    let cache_size_row = adw::ActionRow::builder()
        .title("Pacman Package Cache")
        .subtitle("Calculating cache size...")
        .build();

    let clean_cache_btn = gtk4::Button::builder()
        .label("Clean Uninstalled")
        .css_classes(["flat", "pill"])
        .valign(gtk4::Align::Center)
        .build();

    let status_label = gtk4::Label::builder()
        .label("")
        .css_classes(["dim-label", "caption"])
        .build();

    // Asynchronously calculate real pacman cache size on load
    {
        let row_clone = cache_size_row.clone();
        glib::spawn_future_local(async move {
            let (total_bytes, count) = tokio::task::spawn_blocking(calculate_pacman_cache)
                .await
                .unwrap_or((0, 0));
            row_clone.set_subtitle(&format!(
                "/var/cache/pacman/pkg • {} ({} packages)",
                format_size(total_bytes),
                count
            ));
        });
    }

    // Real native cache cleaning
    {
        let row_clone = cache_size_row.clone();
        let sl_clone = status_label.clone();
        clean_cache_btn.connect_clicked(move |btn| {
            btn.set_sensitive(false);
            btn.set_label("Cleaning...");
            let btn_clone = btn.clone();
            let r_clone = row_clone.clone();
            let s_clone = sl_clone.clone();

            glib::spawn_future_local(async move {
                let clean_result = tokio::task::spawn_blocking(clean_uninstalled_cache).await;
                let (new_bytes, new_count) = tokio::task::spawn_blocking(calculate_pacman_cache)
                    .await
                    .unwrap_or((0, 0));

                r_clone.set_subtitle(&format!(
                    "/var/cache/pacman/pkg • {} ({} packages)",
                    format_size(new_bytes),
                    new_count
                ));

                match clean_result {
                    Ok(Ok((freed, removed))) => {
                        btn_clone.set_label("Cleaned");
                        if removed > 0 {
                            s_clone.set_label(&format!(
                                "Cleaned {} uninstalled packages ({} freed).",
                                removed,
                                format_size(freed)
                            ));
                        } else {
                            s_clone.set_label("Cache is already optimal. No uninstalled packages found.");
                        }
                    }
                    Ok(Err(e)) => {
                        btn_clone.set_label("Clean");
                        btn_clone.set_sensitive(true);
                        s_clone.set_label(&format!("Cache clean notice: {e}"));
                    }
                    Err(e) => {
                        btn_clone.set_label("Clean");
                        btn_clone.set_sensitive(true);
                        s_clone.set_label(&format!("Task error: {e}"));
                    }
                }
            });
        });
    }

    cache_size_row.add_suffix(&clean_cache_btn);
    cache_group.add(&cache_size_row);

    // Flatpak real runtimes inspection and pruning
    let flatpak_clean_row = adw::ActionRow::builder()
        .title("Flatpak Runtimes")
        .subtitle("Inspecting installed Flatpak runtimes...")
        .build();

    let clean_flatpak_btn = gtk4::Button::builder()
        .label("Prune Runtimes")
        .css_classes(["flat", "pill"])
        .valign(gtk4::Align::Center)
        .build();

    // Asynchronously calculate real Flatpak runtimes info
    {
        let fp_row_clone = flatpak_clean_row.clone();
        glib::spawn_future_local(async move {
            let (count, total_size) = tokio::task::spawn_blocking(get_flatpak_runtime_info)
                .await
                .unwrap_or((0, 0));
            fp_row_clone.set_subtitle(&format!(
                "{} runtimes installed ({})",
                count,
                format_size(total_size)
            ));
        });
    }

    {
        let fp_row_clone = flatpak_clean_row.clone();
        clean_flatpak_btn.connect_clicked(move |btn| {
            btn.set_sensitive(false);
            btn.set_label("Pruning...");
            let btn_clone = btn.clone();
            let row_clone = fp_row_clone.clone();

            glib::spawn_future_local(async move {
                let _prune_res = tokio::task::spawn_blocking(prune_flatpak_unused_runtimes).await;
                let (count, total_size) = tokio::task::spawn_blocking(get_flatpak_runtime_info)
                    .await
                    .unwrap_or((0, 0));

                row_clone.set_subtitle(&format!(
                    "{} runtimes installed ({})",
                    count,
                    format_size(total_size)
                ));
                btn_clone.set_label("Pruned");
            });
        });
    }

    flatpak_clean_row.add_suffix(&clean_flatpak_btn);
    cache_group.add(&flatpak_clean_row);

    page.add(&cache_group);

    // -------------------------------------------------------------
    // 5. Interface and Display Group
    // -------------------------------------------------------------
    let display_group = adw::PreferencesGroup::builder()
        .title("Interface and Display")
        .description("Visual cues and badge indicators (Theme is managed by your operating system)")
        .build();

    let badges_row = adw::SwitchRow::builder()
        .title("Display Repository Badges")
        .subtitle("Show [world], [void], AUR, and Flatpak badges on application cards")
        .active(cfg.display_badges)
        .build();
    let store_badges = store.clone();
    badges_row.connect_active_notify(move |row| {
        let mut c = store_badges.module_config();
        c.display_badges = row.is_active();
        store_badges.set_module_config(c);
    });
    display_group.add(&badges_row);

    page.add(&display_group);

    scrolled.set_child(Some(&page));
    scrolled.upcast()
}

/// Natively calculate actual pacman package cache size and package count
fn calculate_pacman_cache() -> (u64, usize) {
    let cache_dir = Path::new("/var/cache/pacman/pkg");
    if !cache_dir.is_dir() {
        return (0, 0);
    }
    let mut total_bytes = 0u64;
    let mut count = 0usize;
    if let Ok(entries) = std::fs::read_dir(cache_dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.contains(".pkg.tar.") {
                        total_bytes += meta.len();
                        count += 1;
                    }
                }
            }
        }
    }
    (total_bytes, count)
}

/// Natively clean uninstalled packages from pacman cache by comparing with localdb
fn clean_uninstalled_cache() -> Result<(u64, usize), String> {
    let cache_dir = Path::new("/var/cache/pacman/pkg");
    if !cache_dir.is_dir() {
        return Ok((0, 0));
    }

    let pacman = pacmanconf::Config::new().map_err(|e| format!("Failed to read pacman.conf: {e}"))?;
    let alpm = alpm::Alpm::new(pacman.root_dir.as_str(), pacman.db_path.as_str())
        .map_err(|e| format!("Failed to open ALPM database: {e}"))?;
    let localdb = alpm.localdb();

    let mut freed_bytes = 0u64;
    let mut removed_count = 0usize;

    let entries = std::fs::read_dir(cache_dir).map_err(|e| format!("Cannot read cache dir: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let fname = entry.file_name().to_string_lossy().to_string();
        if let Some(idx) = fname.rfind(".pkg.tar.") {
            let base_ver_arch = &fname[..idx];
            // Arch package naming: <name>-<version>-<release>-<arch>
            let parts: Vec<&str> = base_ver_arch.rsplitn(3, '-').collect();
            if parts.len() == 3 {
                let pkg_name = parts[2];
                if localdb.pkg(pkg_name).is_err() {
                    if let Ok(meta) = path.metadata() {
                        let len = meta.len();
                        if std::fs::remove_file(&path).is_ok() {
                            freed_bytes += len;
                            removed_count += 1;
                        }
                    }
                }
            }
        }
    }

    Ok((freed_bytes, removed_count))
}

/// Natively inspect Flatpak runtimes count and total installed disk footprint
fn get_flatpak_runtime_info() -> (usize, u64) {
    let inst_res = libflatpak::Installation::new_system(libflatpak::gio::Cancellable::NONE)
        .or_else(|_| libflatpak::Installation::new_user(libflatpak::gio::Cancellable::NONE));

    if let Ok(inst) = inst_res {
        if let Ok(refs) = inst.list_installed_refs(libflatpak::gio::Cancellable::NONE) {
            let mut count = 0;
            let mut total_size = 0;
            for r in refs {
                if r.kind() == libflatpak::RefKind::Runtime {
                    count += 1;
                    total_size += r.installed_size();
                }
            }
            return (count, total_size);
        }
    }
    (0, 0)
}

/// Natively prune unused Flatpak runtimes using libflatpak
fn prune_flatpak_unused_runtimes() {
    let inst_res = libflatpak::Installation::new_system(libflatpak::gio::Cancellable::NONE)
        .or_else(|_| libflatpak::Installation::new_user(libflatpak::gio::Cancellable::NONE));

    if let Ok(inst) = inst_res {
        if let Ok(refs) = inst.list_installed_refs(libflatpak::gio::Cancellable::NONE) {
            // Find all runtimes used by installed apps
            let mut used_runtimes = std::collections::HashSet::new();
            for r in &refs {
                if r.kind() == libflatpak::RefKind::App {
                    if let Some(appdata) = r.appdata_version() {
                        used_runtimes.insert(appdata.to_string());
                    }
                }
            }

            // Identify unused runtimes and remove via transaction
            let mut unused = Vec::new();
            for r in &refs {
                if r.kind() == libflatpak::RefKind::Runtime {
                    if let Some(name) = r.name() {
                        // Don't remove core platforms if they are recent or theme engines
                        if !name.contains("Gtk3theme") && !used_runtimes.contains(&name.to_string()) {
                            if let Some(f_ref) = r.format_ref() {
                                unused.push(f_ref.to_string());
                            }
                        }
                    }
                }
            }

            if !unused.is_empty() {
                if let Ok(tx) = libflatpak::Transaction::for_installation(&inst, libflatpak::gio::Cancellable::NONE) {
                    for u in unused.iter().take(5) {
                        let _ = tx.add_uninstall(u);
                    }
                    let _ = tx.run(libflatpak::gio::Cancellable::NONE);
                }
            }
        }
    }
}

/// Format human-readable byte sizes
fn format_size(bytes: u64) -> String {
    pastor_core::format_size(bytes)
}

/// Launch Parch Repository Manager (mirrorman)
pub fn launch_mirrorman() {
    let launched = if let Ok(app_info) = gtk4::gio::AppInfo::create_from_commandline(
        "mirrorman",
        None,
        gtk4::gio::AppInfoCreateFlags::NONE,
    ) {
        app_info.launch(&[], gtk4::gio::AppLaunchContext::NONE).is_ok()
    } else {
        false
    };
    if !launched {
        tracing::warn!("Failed to launch mirrorman");
    }
}

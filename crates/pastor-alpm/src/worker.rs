use std::io::Write;
use alpm::{AnyDownloadEvent, AnyEvent, AnyQuestion, DownloadEvent, DownloadResult, LogLevel, Progress, TransFlag};
use alpm_utils::DbListExt;
use pastor_core::{TransactionEvent, TransactionStep};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerEvent {
    pub step: String,
    pub progress_fraction: f32,
    pub log_message: String,
}

impl WorkerEvent {
    pub fn emit(&self) {
        if let Ok(json) = serde_json::to_string(self) {
            let mut stdout = std::io::stdout();
            let _ = writeln!(stdout, "{}", json);
            let _ = stdout.flush();
        }
    }

    pub fn to_transaction_event(&self) -> TransactionEvent {
        let step = match self.step.as_str() {
            "SyncingDatabase" => TransactionStep::SyncingDatabase,
            "CheckingDependencies" => TransactionStep::CheckingDependencies,
            "Completed" => TransactionStep::Completed,
            "Cancelled" => TransactionStep::Cancelled,
            "Downloading" => TransactionStep::Downloading {
                current_bytes: 0,
                total_bytes: 0,
                speed_bps: 0,
            },
            _ => TransactionStep::ApplyingChanges {
                package: self.log_message.clone(),
                index: 1,
                total: 1,
            },
        };
        TransactionEvent {
            step,
            progress_fraction: self.progress_fraction,
            log_message: self.log_message.clone(),
        }
    }
}

fn create_alpm() -> Result<alpm::Alpm, Box<dyn std::error::Error>> {
    let pacman = pacmanconf::Config::new()?;
    let mut handle = alpm_utils::alpm_with_conf(&pacman)?;
    for pkg in &pacman.ignore_pkg {
        let _ = handle.add_ignorepkg(pkg.as_str());
    }
    for group in &pacman.ignore_group {
        let _ = handle.add_ignoregroup(group.as_str());
    }

    // The pacmanconf crate only knows the /etc/pacman.d/hooks default; real
    // pacman also ALWAYS scans the system hook directory. Without it, libalpm
    // runs transactions with no file-triggered hooks at all, so hooks like
    // update-desktop-database / update-mime-database never fire and in-app
    // self-updates leave stale MIME caches (breaking xdg-open pastor:// links).
    const SYSTEM_HOOK_DIR: &str = "/usr/share/libalpm/hooks";
    if !handle.hookdirs().iter().any(|d| d.trim_end_matches('/') == SYSTEM_HOOK_DIR) {
        handle.add_hookdir(SYSTEM_HOOK_DIR)?;
    }

    Ok(handle)
}

#[cfg(test)]
mod tests {
    #[test]
    fn system_hook_dir_is_configured() {
        let alpm = super::create_alpm().expect("worker alpm handle should initialize");
        let dirs: Vec<String> = alpm.hookdirs().iter().map(|d| d.to_string()).collect();
        let norm = |d: &String| d.trim_end_matches('/').to_string();
        assert!(
            dirs.iter().any(|d| norm(d) == "/usr/share/libalpm/hooks"),
            "system hook dir must be registered so alpm file-triggered hooks run; got {dirs:?}"
        );
        assert!(
            dirs.iter().any(|d| norm(d) == "/etc/pacman.d/hooks"),
            "pacman.d hooks dir must remain registered; got {dirs:?}"
        );
    }
}

fn setup_callbacks(alpm: &mut alpm::Alpm) {
    alpm.set_log_cb((), |_level: LogLevel, msg: &str, _| {
        tracing::debug!("alpm: {}", msg);
    });

    alpm.set_dl_cb((), |filename: &str, event: AnyDownloadEvent, _| {
        match event.event() {
            DownloadEvent::Init(_) => {
                WorkerEvent {
                    step: "Downloading".to_string(),
                    progress_fraction: 0.1,
                    log_message: format!("Downloading {}...", filename),
                }
                .emit();
            }
            DownloadEvent::Progress(p) => {
                let frac = (p.downloaded as f32 / p.total.max(1) as f32).min(0.99);
                WorkerEvent {
                    step: "Downloading".to_string(),
                    progress_fraction: frac,
                    log_message: format!(
                        "Downloading {}: {} / {}",
                        filename,
                        pastor_core::format_size(p.downloaded as u64),
                        pastor_core::format_size(p.total as u64)
                    ),
                }
                .emit();
            }
            DownloadEvent::Completed(c) => {
                if c.result == DownloadResult::Success {
                    WorkerEvent {
                        step: "Downloading".to_string(),
                        progress_fraction: 1.0,
                        log_message: format!("Downloaded {}", filename),
                    }
                    .emit();
                }
            }
            _ => {}
        }
    });

    alpm.set_progress_cb((), |prog: Progress, pkgname: &str, percent: i32, howmany: usize, current: usize, _| {
        let frac = (percent as f32 / 100.0).min(0.99);
        let step_name = match prog {
            Progress::AddStart | Progress::UpgradeStart => "Installing",
            Progress::DowngradeStart => "Downgrading",
            Progress::ReinstallStart => "Reinstalling",
            Progress::RemoveStart => "Removing",
            Progress::ConflictsStart => "Checking conflicts",
            Progress::DiskspaceStart => "Checking disk space",
            Progress::IntegrityStart => "Checking integrity",
            Progress::LoadStart => "Loading packages",
            Progress::KeyringStart => "Checking keyring",
        };
        WorkerEvent {
            step: "ApplyingChanges".to_string(),
            progress_fraction: frac,
            log_message: format!("{}: {} ({}%, {}/{})", step_name, pkgname, percent, current, howmany),
        }
        .emit();
    });

    alpm.set_event_cb((), |_event: AnyEvent, _| {});

    alpm.set_question_cb((), |question: AnyQuestion, _| {
        match question.question() {
            alpm::Question::SelectProvider(mut q) => {
                q.set_index(0);
            }
            alpm::Question::InstallIgnorepkg(mut q) => {
                q.set_install(true);
            }
            alpm::Question::Replace(q) => {
                q.set_replace(true);
            }
            alpm::Question::Conflict(mut q) => {
                q.set_remove(true);
            }
            alpm::Question::RemovePkgs(mut q) => {
                q.set_skip(false);
            }
            _ => {}
        }
    });
}

fn find_sync_pkg<'a>(alpm: &'a alpm::Alpm, target: &str) -> Result<&'a alpm::Package, String> {
    if let Some((repo, name)) = target.split_once('/') {
        for db in alpm.syncdbs() {
            if db.name() == repo {
                if let Ok(pkg) = db.pkg(name) {
                    return Ok(pkg);
                }
            }
        }
    }
    alpm.syncdbs()
        .pkg(target)
        .map_err(|e| format!("Package '{target}' not found in sync databases: {e}"))
}

pub fn run_worker(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.is_empty() {
        return Err("No worker action specified".into());
    }

    let action = args[0].as_str();
    let mut alpm = create_alpm()?;
    setup_callbacks(&mut alpm);

    match action {
        "refresh" => {
            WorkerEvent {
                step: "SyncingDatabase".to_string(),
                progress_fraction: 0.1,
                log_message: "Synchronizing remote package databases...".to_string(),
            }
            .emit();

            alpm.syncdbs_mut().update(false)?;

            WorkerEvent {
                step: "Completed".to_string(),
                progress_fraction: 1.0,
                log_message: "Remote databases successfully updated".to_string(),
            }
            .emit();
        }
        "install" => {
            let targets = &args[1..];
            if targets.is_empty() {
                return Err("No package targets provided for installation".into());
            }

            WorkerEvent {
                step: "CheckingDependencies".to_string(),
                progress_fraction: 0.1,
                log_message: format!("Initializing native ALPM transaction for {} package(s)...", targets.len()),
            }
            .emit();

            alpm.trans_init(TransFlag::NONE)?;

            let run_res: Result<(), String> = (|| {
                for t in targets {
                    let pkg = find_sync_pkg(&alpm, t)?;
                    alpm.trans_add_pkg(pkg).map_err(|e| e.to_string())?;
                }

                alpm.trans_prepare().map_err(|e| e.to_string())?;
                alpm.trans_commit().map_err(|e| e.to_string())?;
                Ok(())
            })();

            let _ = alpm.trans_release();
            if let Err(e) = run_res {
                return Err(e.into());
            }

            WorkerEvent {
                step: "Completed".to_string(),
                progress_fraction: 1.0,
                log_message: format!("Successfully installed {} package(s)", targets.len()),
            }
            .emit();
        }
        "upgrade" => {
            WorkerEvent {
                step: "CheckingDependencies".to_string(),
                progress_fraction: 0.1,
                log_message: "Preparing native system upgrade transaction...".to_string(),
            }
            .emit();

            alpm.trans_init(TransFlag::NONE)?;

            let run_res: Result<(), String> = (|| {
                alpm.sync_sysupgrade(false).map_err(|e| e.to_string())?;
                if alpm.trans_add().is_empty() {
                    return Ok(());
                }
                alpm.trans_prepare().map_err(|e| e.to_string())?;
                alpm.trans_commit().map_err(|e| e.to_string())?;
                Ok(())
            })();

            let _ = alpm.trans_release();
            if let Err(e) = run_res {
                return Err(e.into());
            }

            WorkerEvent {
                step: "Completed".to_string(),
                progress_fraction: 1.0,
                log_message: "System upgrade successfully completed".to_string(),
            }
            .emit();
        }
        "remove" => {
            let targets = &args[1..];
            if targets.is_empty() {
                return Err("No package targets provided for removal".into());
            }

            WorkerEvent {
                step: "ApplyingChanges".to_string(),
                progress_fraction: 0.1,
                log_message: format!("Preparing removal for {} package(s)...", targets.len()),
            }
            .emit();

            alpm.trans_init(TransFlag::NONE)?;

            let run_res: Result<(), String> = (|| {
                for t in targets {
                    let pkg = alpm.localdb().pkg(t.as_str()).map_err(|e| e.to_string())?;
                    alpm.trans_remove_pkg(pkg).map_err(|e| e.to_string())?;
                }

                alpm.trans_prepare().map_err(|e| e.to_string())?;
                alpm.trans_commit().map_err(|e| e.to_string())?;
                Ok(())
            })();

            let _ = alpm.trans_release();
            if let Err(e) = run_res {
                return Err(e.into());
            }

            WorkerEvent {
                step: "Completed".to_string(),
                progress_fraction: 1.0,
                log_message: format!("Successfully removed {} package(s)", targets.len()),
            }
            .emit();
        }
        "install-file" | "downgrade" => {
            let targets = &args[1..];
            if targets.is_empty() {
                return Err("No package target file provided".into());
            }
            let pkg_path = &targets[0];
            let is_install = action == "install-file";
            let op_desc = if is_install { "Installing package" } else { "Preparing downgrade" };
            WorkerEvent {
                step: "ApplyingChanges".to_string(),
                progress_fraction: 0.1,
                log_message: format!("{} from {}...", op_desc, pkg_path),
            }
            .emit();

            alpm.trans_init(TransFlag::NONE)?;

            let run_res: Result<(), String> = (|| {
                let path_bytes = pkg_path.as_bytes();
                let loaded_pkg = alpm.pkg_load(path_bytes, false, alpm::SigLevel::NONE).map_err(|e| e.to_string())?;
                alpm.trans_add_pkg(loaded_pkg).map_err(|e| e.to_string())?;
                alpm.trans_prepare().map_err(|e| e.to_string())?;
                alpm.trans_commit().map_err(|e| e.to_string())?;
                Ok(())
            })();

            let _ = alpm.trans_release();
            if let Err(e) = run_res {
                return Err(e.into());
            }

            WorkerEvent {
                step: "Completed".to_string(),
                progress_fraction: 1.0,
                log_message: if is_install {
                    "Package successfully installed".to_string()
                } else {
                    "Downgrade successfully completed".to_string()
                },
            }
            .emit();
        }
        other => {
            return Err(format!("Unknown worker action: {other}").into());
        }
    }

    Ok(())
}

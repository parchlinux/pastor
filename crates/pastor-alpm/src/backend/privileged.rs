use alpm::TransFlag;
use alpm_utils::DbListExt;
use pastor_core::{
    error::PastorError,
    transaction::{TransactionEvent, TransactionStep},
};
use tokio::sync::mpsc::Sender;

pub fn create_alpm(pacman: &pacmanconf::Config) -> Result<alpm::Alpm, PastorError> {
    let mut handle = alpm::Alpm::new(
        pacman.root_dir.as_str(),
        pacman.db_path.as_str(),
    )
    .map_err(|e| PastorError::BackendError {
        backend: "alpm".into(),
        message: format!("Failed to create ALPM handle: {e}"),
    })?;

    alpm_utils::configure_alpm(&mut handle, pacman).map_err(|e| PastorError::BackendError {
        backend: "alpm".into(),
        message: format!("Failed to configure ALPM from pacman.conf: {e}"),
    })?;

    for pkg in &pacman.ignore_pkg {
        let _ = handle.add_ignorepkg(pkg.as_str());
    }
    for group in &pacman.ignore_group {
        let _ = handle.add_ignoregroup(group.as_str());
    }

    Ok(handle)
}

pub fn detect_dependencies_internal(
    pacman: &pacmanconf::Config,
    targets: &[&str],
) -> Result<Vec<String>, PastorError> {
    let mut alpm = create_alpm(pacman)?;
    alpm.trans_init(TransFlag::NO_LOCK).map_err(|e| PastorError::TransactionFailed(format!(
        "Failed to initialize dependency resolution: {e}"
    )))?;

    let run_res: Result<Vec<String>, String> = (|| {
        for &t in targets {
            let pkg = if let Some((repo, name)) = t.split_once('/') {
                let mut found = None;
                for db in alpm.syncdbs() {
                    if db.name() == repo {
                        if let Ok(p) = db.pkg(name) {
                            found = Some(p);
                            break;
                        }
                    }
                }
                found.ok_or_else(|| format!("Package '{t}' not found in repo '{repo}'"))?
            } else {
                alpm.syncdbs().pkg(t).map_err(|e| e.to_string())?
            };
            alpm.trans_add_pkg(pkg).map_err(|e| e.to_string())?;
        }

        alpm.trans_prepare().map_err(|e| e.to_string())?;

        let mut deps = Vec::new();
        for p in alpm.trans_add() {
            let is_target = targets.iter().any(|&t| {
                let t_name = t.split_once('/').map(|(_, n)| n).unwrap_or(t);
                t_name == p.name()
            });
            if !is_target {
                deps.push(format!("{}-{}", p.name(), p.version()));
            }
        }
        Ok(deps)
    })();

    let _ = alpm.trans_release();
    run_res.map_err(|e| PastorError::TransactionFailed(format!("Dependency resolution failed: {e}")))
}

pub fn run_privileged(
    action: &str,
    extra_args: &[&str],
    progress_tx: Sender<TransactionEvent>,
) -> Result<(), PastorError> {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }

    let current_exe = std::env::current_exe().map_err(|e| PastorError::BackendError {
        backend: "alpm".into(),
        message: format!("Could not get current executable path: {e}"),
    })?;

    let is_root = unsafe { geteuid() == 0 };
    if !is_root {
        // pkexec will execute the binary as root. Refuse-to-warn when running
        // from a user-writable path (dev builds) — root must never execute a
        // binary the user could have replaced between checks.
        if let Ok(exe) = std::fs::canonicalize(&current_exe) {
            let path_str = exe.to_string_lossy();
            if path_str.starts_with("/home/")
                || path_str.starts_with("/tmp/")
                || path_str.contains("/target/")
            {
                tracing::warn!(
                    "Running privileged worker from user-writable path ({path_str}); \
                     production installs must live under /usr/bin"
                );
            }
        }
    }
    let mut cmd = if is_root {
        let mut c = std::process::Command::new(&current_exe);
        c.arg("--alpm-worker");
        c.arg(action);
        for a in extra_args {
            c.arg(a);
        }
        c
    } else {
        let mut c = std::process::Command::new("pkexec");
        c.arg(&current_exe);
        c.arg("--alpm-worker");
        c.arg(action);
        for a in extra_args {
            c.arg(a);
        }
        c
    };

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| PastorError::BackendError {
        backend: "alpm".into(),
        message: format!("Failed to start privileged worker: {e}"),
    })?;

    if let Some(stdout) = child.stdout.take() {
        let reader = std::io::BufReader::new(stdout);
        use std::io::BufRead;

        let throttle_window = std::time::Duration::from_millis(125);
        let mut last_forward = std::time::Instant::now()
            .checked_sub(throttle_window)
            .unwrap_or_else(std::time::Instant::now);

        for line in reader.lines().flatten() {
            if let Ok(worker_evt) = serde_json::from_str::<crate::worker::WorkerEvent>(&line) {
                let evt = worker_evt.to_transaction_event();
                if matches!(
                    evt.step,
                    TransactionStep::Downloading { .. }
                        | TransactionStep::ApplyingChanges { .. }
                ) {
                    let now = std::time::Instant::now();
                    if now.duration_since(last_forward) < throttle_window {
                        continue;
                    }
                    last_forward = now;
                }
                let _ = progress_tx.blocking_send(evt);
            }
        }
    }

    let status = child.wait().map_err(|e| PastorError::BackendError {
        backend: "alpm".into(),
        message: format!("Failed to wait for privileged worker: {e}"),
    })?;

    if !status.success() {
        let mut err_msg = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            use std::io::Read;
            let _ = stderr.read_to_string(&mut err_msg);
        }
        if err_msg.trim().is_empty() {
            err_msg = format!("Worker failed with exit code: {:?}", status.code());
        }
        return Err(PastorError::TransactionFailed(err_msg.trim().to_string()));
    }

    Ok(())
}

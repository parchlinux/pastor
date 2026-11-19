use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PacnewKind {
    Pacnew,
    Pacsave,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PacnewFile {
    pub original_path: PathBuf,
    pub candidate_path: PathBuf,
    pub kind: PacnewKind,
    pub original_exists: bool,
    pub candidate_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseLockInfo {
    pub lock_path: PathBuf,
    pub exists: bool,
    pub active_pid: Option<u32>,
    pub process_name: Option<String>,
    pub is_stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CacheHealthInfo {
    pub pacman_cache_dir: PathBuf,
    pub total_packages_cached: usize,
    pub total_cache_bytes: u64,
    pub reclaimable_packages: usize,
    pub reclaimable_bytes: u64,
    pub flatpak_unused_count: usize,
    pub flatpak_unused_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrphanPackage {
    pub name: String,
    pub version: String,
    pub size_bytes: u64,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DoctorReport {
    pub pacnew_files: Vec<PacnewFile>,
    pub database_lock: Option<DatabaseLockInfo>,
    pub cache_health: CacheHealthInfo,
    pub orphan_packages: Vec<OrphanPackage>,
}

impl DoctorReport {
    pub fn is_healthy(&self) -> bool {
        self.pacnew_files.is_empty()
            && self.database_lock.as_ref().map_or(true, |l| !l.exists || !l.is_stale)
            && self.orphan_packages.is_empty()
    }

    pub fn total_issues_count(&self) -> usize {
        let mut count = self.pacnew_files.len() + self.orphan_packages.len();
        if let Some(l) = &self.database_lock {
            if l.exists && l.is_stale {
                count += 1;
            }
        }
        count
    }
}

/// Recursively scans a root directory (defaulting to /etc) for .pacnew and .pacsave files.
pub fn scan_pacnew_files(search_root: impl AsRef<Path>) -> Vec<PacnewFile> {
    let mut results = Vec::new();
    let root = search_root.as_ref();
    if !root.exists() {
        return results;
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_symlink() {
                continue;
            }

            if path.is_dir() {
                stack.push(path);
                continue;
            }

            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };

            let kind = if file_name.ends_with(".pacnew") {
                Some((PacnewKind::Pacnew, ".pacnew"))
            } else if file_name.ends_with(".pacsave") {
                Some((PacnewKind::Pacsave, ".pacsave"))
            } else {
                None
            };

            if let Some((kind, ext)) = kind {
                let orig_name = &file_name[..file_name.len() - ext.len()];
                let original_path = dir.join(orig_name);
                let original_exists = original_path.exists();
                let candidate_size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);

                results.push(PacnewFile {
                    original_path,
                    candidate_path: path,
                    kind,
                    original_exists,
                    candidate_size_bytes,
                });
            }
        }
    }

    results.sort_by(|a, b| a.candidate_path.cmp(&b.candidate_path));
    results
}

/// Inspects whether the pacman database lock file exists and checks if its holder PID is still running.
pub fn check_database_lock(lock_path: impl AsRef<Path>) -> DatabaseLockInfo {
    let path = lock_path.as_ref().to_path_buf();
    if !path.exists() {
        return DatabaseLockInfo {
            lock_path: path,
            exists: false,
            active_pid: None,
            process_name: None,
            is_stale: false,
        };
    }

    // Check if any active process holds this file or matches pacman
    let (active_pid, process_name) = find_lock_holder_pid();
    let is_stale = active_pid.is_none();

    DatabaseLockInfo {
        lock_path: path,
        exists: true,
        active_pid,
        process_name,
        is_stale,
    }
}

/// Inspects /proc for running pacman, pastor, or paru processes.
fn find_lock_holder_pid() -> (Option<u32>, Option<String>) {
    let proc_dir = Path::new("/proc");
    let entries = match std::fs::read_dir(proc_dir) {
        Ok(e) => e,
        Err(_) => return (None, None),
    };

    let target_names = ["pacman", "paru", "yay", "pastor"];

    for entry in entries.flatten() {
        let name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue,
        };

        let pid: u32 = match name.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let comm_path = entry.path().join("comm");
        if let Ok(comm) = std::fs::read_to_string(comm_path) {
            let comm = comm.trim();
            for target in target_names {
                if comm.eq_ignore_ascii_case(target) {
                    return (Some(pid), Some(comm.to_string()));
                }
            }
        }
    }

    (None, None)
}

/// Scans the pacman cache directory and calculates total cached vs reclaimable packages.
/// Retains latest `keep_versions` (default 2) for downgrade safety, marking the rest as reclaimable.
pub fn scan_pacman_cache(cache_dir: impl AsRef<Path>, keep_versions: usize) -> CacheHealthInfo {
    let dir = cache_dir.as_ref();
    let mut info = CacheHealthInfo {
        pacman_cache_dir: dir.to_path_buf(),
        ..Default::default()
    };

    if !dir.exists() {
        return info;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return info,
    };

    // Group files by package base name: package-name -> Vec<(version, size, path)>
    let mut packages: HashMap<String, Vec<(String, u64, PathBuf)>> = HashMap::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        if !file_name.ends_with(".pkg.tar.zst") && !file_name.ends_with(".pkg.tar.xz") {
            continue;
        }

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let size = meta.len();
        info.total_cache_bytes += size;
        info.total_packages_cached += 1;

        if let Some((pkg_name, pkg_version)) = parse_pacman_pkg_filename(file_name) {
            packages
                .entry(pkg_name)
                .or_default()
                .push((pkg_version, size, path));
        }
    }

    for versions in packages.values_mut() {
        if versions.len() > keep_versions {
            // Sort ascending by modification time or version
            versions.sort_by(|a, b| {
                let m_a = a.2.metadata().and_then(|m| m.modified()).ok();
                let m_b = b.2.metadata().and_then(|m| m.modified()).ok();
                m_a.cmp(&m_b)
            });

            let to_reclaim = versions.len() - keep_versions;
            for (ver, size, _) in versions.iter().take(to_reclaim) {
                let _ = ver;
                info.reclaimable_packages += 1;
                info.reclaimable_bytes += size;
            }
        }
    }

    info
}

/// Helper to parse standard Arch package file names like `firefox-130.0-1-x86_64.pkg.tar.zst`.
pub fn parse_pacman_pkg_filename(filename: &str) -> Option<(String, String)> {
    let stripped = filename
        .strip_suffix(".pkg.tar.zst")
        .or_else(|| filename.strip_suffix(".pkg.tar.xz"))
        .or_else(|| filename.strip_suffix(".pkg.tar.gz"))?;

    // Format: name-version-pkgrel-arch
    let parts: Vec<&str> = stripped.rsplitn(4, '-').collect();
    if parts.len() < 4 {
        return None;
    }

    let arch = parts[0];
    let pkgrel = parts[1];
    let version = parts[2];
    let name = parts[3];

    let full_version = format!("{version}-{pkgrel}-{arch}");
    Some((name.to_string(), full_version))
}

/// Generates a unified diff between the original file and candidate file using diff -u.
pub fn generate_diff(original: impl AsRef<Path>, candidate: impl AsRef<Path>) -> String {
    let orig = original.as_ref();
    let cand = candidate.as_ref();

    let output = std::process::Command::new("diff")
        .args(["-u", "-N"])
        .arg(orig)
        .arg(cand)
        .output();

    match output {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout).to_string();
            if s.trim().is_empty() {
                "(No differences found between current configuration and candidate file)".to_string()
            } else {
                s
            }
        }
        Err(e) => format!("Failed to execute diff: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn make_test_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pastor_test_{label}_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_parse_pacman_pkg_filename() {
        let (name, ver) = parse_pacman_pkg_filename("linux-6.10.10.arch1-1-x86_64.pkg.tar.zst").unwrap();
        assert_eq!(name, "linux");
        assert_eq!(ver, "6.10.10.arch1-1-x86_64");

        let (name2, ver2) = parse_pacman_pkg_filename("glibc-2.40-1-x86_64.pkg.tar.xz").unwrap();
        assert_eq!(name2, "glibc");
        assert_eq!(ver2, "2.40-1-x86_64");
    }

    #[test]
    fn test_scan_pacnew_files() {
        let temp = make_test_dir("doctor_pacnew");
        let etc = temp.join("etc");
        std::fs::create_dir_all(&etc).unwrap();

        // Create original file and .pacnew candidate
        File::create(etc.join("pacman.conf")).unwrap();
        File::create(etc.join("pacman.conf.pacnew")).unwrap();

        // Create orphan .pacsave
        File::create(etc.join("nginx.conf.pacsave")).unwrap();

        let results = scan_pacnew_files(&etc);
        assert_eq!(results.len(), 2);

        let pacnew = results.iter().find(|r| r.kind == PacnewKind::Pacnew).unwrap();
        assert_eq!(pacnew.original_path, etc.join("pacman.conf"));
        assert!(pacnew.original_exists);

        let pacsave = results.iter().find(|r| r.kind == PacnewKind::Pacsave).unwrap();
        assert_eq!(pacsave.original_path, etc.join("nginx.conf"));
        assert!(!pacsave.original_exists);

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn test_check_database_lock_nonexistent() {
        let temp = make_test_dir("doctor_lock_none");
        let lock_path = temp.join("db.lck");
        let info = check_database_lock(&lock_path);
        assert!(!info.exists);
        assert!(!info.is_stale);
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn test_check_database_lock_stale() {
        let temp = make_test_dir("doctor_lock_stale");
        let lock_path = temp.join("db.lck");
        File::create(&lock_path).unwrap();

        let info = check_database_lock(&lock_path);
        assert!(info.exists);
        if info.active_pid.is_none() {
            assert!(info.is_stale);
        }
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn test_scan_pacman_cache() {
        let temp = make_test_dir("doctor_cache");
        let cache = temp.join("cache");
        std::fs::create_dir_all(&cache).unwrap();

        // Create 3 versions of pkgA
        File::create(cache.join("pkgA-1.0-1-x86_64.pkg.tar.zst")).unwrap();
        File::create(cache.join("pkgA-1.1-1-x86_64.pkg.tar.zst")).unwrap();
        File::create(cache.join("pkgA-1.2-1-x86_64.pkg.tar.zst")).unwrap();

        // Create 1 version of pkgB
        File::create(cache.join("pkgB-2.0-1-x86_64.pkg.tar.zst")).unwrap();

        let info = scan_pacman_cache(&cache, 2);
        assert_eq!(info.total_packages_cached, 4);
        assert_eq!(info.reclaimable_packages, 1);
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn test_generate_diff() {
        use std::io::Write;
        let temp = make_test_dir("doctor_diff");
        let a = temp.join("config.conf");
        let b = temp.join("config.conf.pacnew");

        let mut fa = File::create(&a).unwrap();
        writeln!(fa, "old line 1\nkeep line").unwrap();

        let mut fb = File::create(&b).unwrap();
        writeln!(fb, "new line 1\nkeep line\nadded line").unwrap();

        let diff = generate_diff(&a, &b);
        assert!(diff.contains("-old line 1"));
        assert!(diff.contains("+new line 1"));
        assert!(diff.contains("+added line"));

        let _ = std::fs::remove_dir_all(temp);
    }
}

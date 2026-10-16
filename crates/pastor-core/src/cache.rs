use std::path::{Path, PathBuf};

/// Get the base cache directory for Pastor (`$XDG_CACHE_HOME/pastor` or `~/.cache/pastor`).
pub fn pastor_cache_dir() -> PathBuf {
    let cache_home = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            PathBuf::from(home).join(".cache")
        });
    cache_home.join("pastor")
}

/// Automatically detect and remove obsolete, legacy, or corrupted cache files in the given directory.
/// Keeps current v3 catalogs and removes older format binaries (v1, v2, unversioned) that waste disk space or cause conflicts.
pub fn cleanup_stale_caches_in(cache_dir: &Path) {
    if !cache_dir.is_dir() {
        return;
    }

    let Ok(entries) = std::fs::read_dir(cache_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let fname = entry.file_name().to_string_lossy().to_string();

        if path.is_file() {
            // 1. Obsolete ALPM catalogs: anything starting with "alpm_catalog" and ending with ".bin"
            // that is NOT the active "alpm_catalog_v3.bin"
            if fname.starts_with("alpm_catalog") && fname.ends_with(".bin") && fname != "alpm_catalog_v3.bin" {
                tracing::info!("Removing obsolete ALPM catalog cache: {:?}", path);
                let _ = std::fs::remove_file(&path);
                continue;
            }

            // 2. Obsolete Flatpak catalogs: older versions (flatpak_*.bin not starting with flatpak_v3_)
            if fname.starts_with("flatpak") && fname.ends_with(".bin") && !fname.starts_with("flatpak_v3_") {
                tracing::info!("Removing obsolete Flatpak catalog cache: {:?}", path);
                let _ = std::fs::remove_file(&path);
                continue;
            }

            // 3. Truncated or empty 0-byte cache files that were interrupted during generation
            if fname.ends_with(".bin") {
                if let Ok(meta) = path.metadata() {
                    if meta.len() == 0 {
                        tracing::info!("Removing empty/truncated 0-byte cache file: {:?}", path);
                        let _ = std::fs::remove_file(&path);
                        continue;
                    }
                }
            }
        }
    }

    // Clean up temporary files in screenshots or icons subdirectories if present
    for subdir_name in &["screenshots", "icons"] {
        let sub = cache_dir.join(subdir_name);
        if sub.is_dir() {
            if let Ok(sub_entries) = std::fs::read_dir(&sub) {
                for sub_entry in sub_entries.flatten() {
                    let spath = sub_entry.path();
                    let sfname = sub_entry.file_name().to_string_lossy().to_string();
                    if sfname.ends_with(".tmp") {
                        let _ = std::fs::remove_file(&spath);
                    } else if let Ok(meta) = spath.metadata() {
                        if meta.is_file() && meta.len() == 0 {
                            let _ = std::fs::remove_file(&spath);
                        }
                    }
                }
            }
        }
    }
}

/// Automatically detect and remove obsolete, legacy, or corrupted cache files in the default Pastor cache directory.
pub fn cleanup_stale_caches() {
    cleanup_stale_caches_in(&pastor_cache_dir());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cleanup_stale_caches() {
        let temp_dir = std::env::temp_dir().join(format!("pastor_test_cache_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let v2_alpm = temp_dir.join("alpm_catalog_v2.bin");
        let v3_alpm = temp_dir.join("alpm_catalog_v3.bin");
        let old_flatpak = temp_dir.join("flatpak_flathub.bin");
        let v3_flatpak = temp_dir.join("flatpak_v3_flathub.bin");
        let zero_byte = temp_dir.join("corrupted.bin");

        let _ = std::fs::write(&v2_alpm, b"dummy v2 data");
        let _ = std::fs::write(&v3_alpm, b"dummy v3 data");
        let _ = std::fs::write(&old_flatpak, b"dummy old flatpak data");
        let _ = std::fs::write(&v3_flatpak, b"dummy v3 flatpak data");
        let _ = std::fs::write(&zero_byte, b"");

        cleanup_stale_caches_in(&temp_dir);

        assert!(!v2_alpm.exists(), "alpm_catalog_v2.bin should be deleted");
        assert!(v3_alpm.exists(), "alpm_catalog_v3.bin should be preserved");
        assert!(!old_flatpak.exists(), "flatpak_flathub.bin should be deleted");
        assert!(v3_flatpak.exists(), "flatpak_v3_flathub.bin should be preserved");
        assert!(!zero_byte.exists(), "0-byte corrupted.bin should be deleted");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

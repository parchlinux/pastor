use std::collections::HashSet;

use alpm::TransFlag;
use alpm_utils::DbListExt;
use pastor_core::{
    error::PastorError,
    package::{Package, PackageCategory, PackageId, PackageUpdate},
};

use super::{
    builder::{build_package, determine_source},
    privileged::create_alpm,
};
use crate::appstream::AlpmCatalog;

pub fn search(
    pacman: &pacmanconf::Config,
    catalog: &AlpmCatalog,
    query: &str,
) -> Result<Vec<Package>, PastorError> {
    let q = query.trim().to_lowercase();
    let alpm = create_alpm(pacman)?;
    let localdb = alpm.localdb();

    let mut matched_pkgs: Vec<(i32, Package)> = Vec::new();
    let mut seen_names = HashSet::new();

    let tokens: Vec<&str> = q.split_whitespace().collect();

    if tokens.is_empty() {
        // Return popular desktop applications when query is empty
        for (pkgname, meta) in &catalog.by_pkgname {
            if let Some(pkg) = alpm.syncdbs().pkg(pkgname.as_str()).ok().or_else(|| localdb.pkg(pkgname.as_str()).ok()) {
                if seen_names.insert(pkg.name().to_string()) {
                    let score = if !meta.screenshots.is_empty() { 100 } else { 50 };
                    let inst = localdb.pkg(pkg.name()).ok();
                    matched_pkgs.push((score, build_package(&alpm, &pkg, inst, catalog)));
                }
            }
        }
        matched_pkgs.sort_by(|a, b| b.0.cmp(&a.0));
        return Ok(matched_pkgs.into_iter().map(|(_, p)| p).collect());
    }

    let q_norm: String = q.chars().filter(|c| c.is_alphanumeric()).collect();

    let score_and_build = |pkg: &alpm::Package| -> (i32, Package) {
        let name = pkg.name();
        let name_lower = name.to_lowercase();
        let meta = catalog.get(name);

        let title_lower = meta.as_ref().map(|m| m.name.to_lowercase()).unwrap_or_default();
        let id_lower = meta.as_ref().map(|m| m.id.to_lowercase()).unwrap_or_default();
        let summary_lower = meta.as_ref().map(|m| m.summary.to_lowercase()).unwrap_or_default();
        let desc_lower = pkg.desc().unwrap_or_default().to_lowercase();
        let keywords: Vec<String> = meta.as_ref().map(|m| m.keywords.iter().map(|k| k.to_lowercase()).collect()).unwrap_or_default();

        let mut score = 0;

        if name_lower == q {
            score += 2500;
        } else if title_lower == q {
            score += 2200;
        } else if name_lower.starts_with(&q) {
            score += 1200;
        } else if title_lower.starts_with(&q) {
            score += 1100;
        } else if name_lower.contains(&q) {
            score += 800;
        } else if title_lower.contains(&q) {
            score += 750;
        }

        if !q_norm.is_empty() {
            let id_norm: String = id_lower.chars().filter(|c| c.is_alphanumeric()).collect();
            let name_norm: String = name_lower.chars().filter(|c| c.is_alphanumeric()).collect();
            if id_norm.contains(&q_norm) {
                score += 950;
            }
            if name_norm.contains(&q_norm) {
                score += 900;
            }
        }

        let mut matched_token_count = 0;
        for t in &tokens {
            let mut token_matched = false;
            if name_lower.contains(t) {
                score += 200;
                token_matched = true;
            }
            if title_lower.contains(t) {
                score += 220;
                token_matched = true;
            }
            if id_lower.contains(t) {
                score += 180;
                token_matched = true;
            }
            if summary_lower.contains(t) {
                score += 150;
                token_matched = true;
            }
            if keywords.iter().any(|k| k.contains(t)) {
                score += 160;
                token_matched = true;
            }
            if desc_lower.contains(t) {
                score += 100;
                token_matched = true;
            }
            if token_matched {
                matched_token_count += 1;
            }
        }

        if !tokens.is_empty() && matched_token_count == tokens.len() {
            score += 800;
        } else if matched_token_count > 0 {
            score += (matched_token_count as i32) * 150;
        }

        if meta.is_some() {
            score += 250;
            if !meta.as_ref().unwrap().screenshots.is_empty() {
                score += 50;
            }
        }

        let is_installed = localdb.pkg(name).is_ok();
        if is_installed {
            score += 50;
        }

        let installed_pkg = localdb.pkg(name).ok();
        let p = build_package(&alpm, pkg, installed_pkg, catalog);
        (score, p)
    };

    for db in alpm.syncdbs() {
        if let Ok(pkgs) = db.search(tokens.iter()) {
            for pkg in pkgs {
                if seen_names.insert(pkg.name().to_string()) {
                    matched_pkgs.push(score_and_build(&pkg));
                }
            }
        }
    }

    if tokens.len() > 1 {
        for db in alpm.syncdbs() {
            if let Ok(pkgs) = db.search([tokens[0]].iter()) {
                for pkg in pkgs {
                    if matched_pkgs.len() >= 200 {
                        break;
                    }
                    if seen_names.insert(pkg.name().to_string()) {
                        matched_pkgs.push(score_and_build(&pkg));
                    }
                }
            }
        }
    }

    for (pkgname, meta) in &catalog.by_pkgname {
        let name_match = pkgname.contains(&q) || meta.name.to_lowercase().contains(&q);
        let id_match = meta.id.to_lowercase().contains(&q);
        let kw_match = meta.keywords.iter().any(|k| k.to_lowercase().contains(&q));

        if name_match || id_match || kw_match {
            if !seen_names.contains(pkgname.as_str()) {
                if let Ok(pkg) = alpm.syncdbs().pkg(pkgname.as_str()) {
                    seen_names.insert(pkg.name().to_string());
                    matched_pkgs.push(score_and_build(&pkg));
                } else if let Ok(pkg) = localdb.pkg(pkgname.as_str()) {
                    seen_names.insert(pkg.name().to_string());
                    matched_pkgs.push(score_and_build(&pkg));
                }
            }
        }
    }

    matched_pkgs.retain(|(s, _)| *s > 0);
    matched_pkgs.sort_by(|a, b| b.0.cmp(&a.0));
    matched_pkgs.truncate(100);

    Ok(matched_pkgs.into_iter().map(|(_, p)| p).collect())
}

pub fn get_by_category(
    pacman: &pacmanconf::Config,
    catalog: &AlpmCatalog,
    category: PackageCategory,
) -> Result<Vec<Package>, PastorError> {
    let alpm = create_alpm(pacman)?;
    let localdb = alpm.localdb();

    let mut packages = Vec::new();
    let mut seen = HashSet::new();

    match category {
        PackageCategory::Featured => {
            let picks = [
                "firefox", "vlc", "gimp", "inkscape", "blender", "libreoffice-fresh",
                "obs-studio", "kdenlive", "audacity", "code", "steam", "telegram-desktop",
                "discord", "mpv", "parch-pacman", "fastfetch", "btop",
            ];
            for name in picks {
                if let Some(pkg) = alpm.syncdbs().pkg(name).ok().or_else(|| localdb.pkg(name).ok()) {
                    if seen.insert(pkg.name().to_string()) {
                        let inst = localdb.pkg(pkg.name()).ok();
                        packages.push(build_package(&alpm, &pkg, inst, catalog));
                    }
                }
            }
        }
        PackageCategory::System => {
            for (pkgname, meta) in &catalog.by_pkgname {
                if meta.categories.contains(&PackageCategory::System) {
                    if let Some(pkg) = alpm.syncdbs().pkg(pkgname.as_str()).ok().or_else(|| localdb.pkg(pkgname.as_str()).ok()) {
                        if seen.insert(pkg.name().to_string()) {
                            let inst = localdb.pkg(pkg.name()).ok();
                            packages.push(build_package(&alpm, &pkg, inst, catalog));
                        }
                    }
                }
            }
            let picks = [
                "parch-pacman", "mirrorman", "parch-alacritty", "paccache",
                "fastfetch", "btop", "htop", "fish", "zsh", "micro",
                "snapper", "btrfs-progs", "gparted",
            ];
            for name in picks {
                if let Some(pkg) = alpm.syncdbs().pkg(name).ok().or_else(|| localdb.pkg(name).ok()) {
                    if seen.insert(pkg.name().to_string()) {
                        let inst = localdb.pkg(pkg.name()).ok();
                        packages.push(build_package(&alpm, &pkg, inst, catalog));
                    }
                }
            }
        }
        _ => {
            for (pkgname, meta) in &catalog.by_pkgname {
                if meta.categories.contains(&category) {
                    if let Some(pkg) = alpm.syncdbs().pkg(pkgname.as_str()).ok().or_else(|| localdb.pkg(pkgname.as_str()).ok()) {
                        if seen.insert(pkg.name().to_string()) {
                            let inst = localdb.pkg(pkg.name()).ok();
                            packages.push(build_package(&alpm, &pkg, inst, catalog));
                        }
                    }
                }
            }
        }
    }

    packages.sort_by(|a, b| a.display_title().to_lowercase().cmp(&b.display_title().to_lowercase()));
    Ok(packages)
}

pub fn get_package(
    pacman: &pacmanconf::Config,
    catalog: &AlpmCatalog,
    name: &str,
) -> Result<Option<Package>, PastorError> {
    let alpm = create_alpm(pacman)?;
    let localdb = alpm.localdb();

    let pkg_opt = alpm.syncdbs().pkg(name).ok().or_else(|| localdb.pkg(name).ok());
    if let Some(pkg) = pkg_opt {
        let inst = localdb.pkg(name).ok();
        Ok(Some(build_package(&alpm, &pkg, inst, catalog)))
    } else {
        Ok(None)
    }
}

pub fn installed(
    pacman: &pacmanconf::Config,
    catalog: &AlpmCatalog,
) -> Result<Vec<Package>, PastorError> {
    let alpm = create_alpm(pacman)?;
    let localdb = alpm.localdb();

    let mut packages = Vec::new();
    for pkg in localdb.pkgs() {
        packages.push(build_package(&alpm, &pkg, Some(&pkg), catalog));
    }

    packages.sort_by(|a, b| a.display_title().to_lowercase().cmp(&b.display_title().to_lowercase()));
    Ok(packages)
}

pub fn updates(pacman: &pacmanconf::Config) -> Result<Vec<PackageUpdate>, PastorError> {
    let mut alpm = create_alpm(pacman)?;

    let flags = TransFlag::NO_LOCK;
    alpm.trans_init(flags).map_err(|e| PastorError::BackendError {
        backend: "alpm".into(),
        message: format!("Failed to initialize upgrade transaction: {e}"),
    })?;

    let mut updates = Vec::new();

    if alpm.sync_sysupgrade(false).is_ok() {
        {
            let localdb = alpm.localdb();
            for pkg in alpm.trans_add() {
                if let Ok(local_pkg) = localdb.pkg(pkg.name()) {
                    let source = determine_source(&alpm, &pkg);
                    let dl_size = if pkg.download_size() > 0 {
                        Some(pkg.download_size() as u64)
                    } else {
                        None
                    };

                    updates.push(PackageUpdate {
                        id: PackageId::new(pkg.name(), source),
                        current_version: local_pkg.version().to_string(),
                        new_version: pkg.version().to_string(),
                        download_size: dl_size,
                        changelog: None,
                    });
                }
            }
        }
        let _ = alpm.trans_release();
    } else {
        let _ = alpm.trans_release();
        let localdb = alpm.localdb();
        let ignores = &pacman.ignore_pkg;
        for local_pkg in localdb.pkgs() {
            if ignores.iter().any(|ig| ig == local_pkg.name()) {
                continue;
            }
            if let Ok(remote_pkg) = alpm.syncdbs().pkg(local_pkg.name()) {
                if alpm::vercmp(remote_pkg.version().as_str(), local_pkg.version().as_str()).is_gt() {
                    let source = determine_source(&alpm, &remote_pkg);
                    let dl_size = if remote_pkg.download_size() > 0 {
                        Some(remote_pkg.download_size() as u64)
                    } else {
                        None
                    };
                    updates.push(PackageUpdate {
                        id: PackageId::new(remote_pkg.name(), source),
                        current_version: local_pkg.version().to_string(),
                        new_version: remote_pkg.version().to_string(),
                        download_size: dl_size,
                        changelog: None,
                    });
                }
            }
        }
    }

    Ok(updates)
}

pub fn get_installed_pkg_version(pacman: &pacmanconf::Config, name: &str) -> Option<String> {
    if let Ok(alpm) = create_alpm(pacman) {
        alpm.localdb().pkg(name).ok().map(|p| p.version().to_string())
    } else {
        None
    }
}

pub fn get_foreign_packages(pacman: &pacmanconf::Config) -> Vec<(String, String)> {
    if let Ok(alpm) = create_alpm(pacman) {
        let syncdbs = alpm.syncdbs();
        alpm.localdb()
            .pkgs()
            .into_iter()
            .filter(|pkg| syncdbs.pkg(pkg.name()).is_err())
            .map(|pkg| (pkg.name().to_string(), pkg.version().to_string()))
            .collect()
    } else {
        vec![]
    }
}

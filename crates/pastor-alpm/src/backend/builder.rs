use std::path::PathBuf;

use pastor_core::package::{
    Package, PackageCategory, PackageIcon, PackageId, PackageSource, PackageState, ParchRepoType,
};

use crate::appstream::AlpmCatalog;

pub fn determine_source(alpm: &alpm::Alpm, pkg: &alpm::Package) -> PackageSource {
    let db_name = pkg.db().map(|d| d.name()).unwrap_or("local");
    if db_name == "local" {
        // Find which sync repo this package belongs to
        for sync_db in alpm.syncdbs() {
            if sync_db.pkg(pkg.name()).is_ok() {
                let sname = sync_db.name();
                if sname == "world" {
                    return PackageSource::Parch(ParchRepoType::World);
                } else if sname == "void" {
                    return PackageSource::Parch(ParchRepoType::Void);
                } else {
                    return PackageSource::Arch(sname.to_string());
                }
            }
        }
        return PackageSource::Arch("local".to_string());
    }

    if db_name == "world" {
        PackageSource::Parch(ParchRepoType::World)
    } else if db_name == "void" {
        PackageSource::Parch(ParchRepoType::Void)
    } else {
        PackageSource::Arch(db_name.to_string())
    }
}

pub fn infer_theme_icon(name: &str, groups: &[&str]) -> String {
    for g in groups {
        match *g {
            "base-devel" => return "applications-development".to_string(),
            "multimedia" => return "applications-multimedia".to_string(),
            "games" => return "applications-games".to_string(),
            _ => {}
        }
    }
    if name.contains("terminal") || name.contains("term") || name.contains("alacritty") || name.contains("kitty") {
        "utilities-terminal".to_string()
    } else if name.contains("editor") || name.contains("code") {
        "accessories-text-editor".to_string()
    } else if name.contains("browser") || name.contains("firefox") || name.contains("chromium") {
        "internet-web-browser".to_string()
    } else {
        "package-x-generic".to_string()
    }
}

pub fn infer_categories(groups: &[&str]) -> Vec<PackageCategory> {
    let mut cats = Vec::new();
    for g in groups {
        match *g {
            "base-devel" | "development" => cats.push(PackageCategory::Development),
            "games" => cats.push(PackageCategory::Games),
            "graphics" => cats.push(PackageCategory::Graphics),
            "multimedia" => cats.push(PackageCategory::Multimedia),
            "office" => cats.push(PackageCategory::Office),
            "system" | "base" => cats.push(PackageCategory::System),
            _ => {}
        }
    }
    if cats.is_empty() {
        cats.push(PackageCategory::Utilities);
    }
    cats
}

pub fn get_cache_dir(pacman_config: &pacmanconf::Config) -> PathBuf {
    let candidate = pacman_config.cache_dir.first().map(PathBuf::from);
    candidate
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("/var/cache/pacman/pkg"))
}

pub fn build_package(
    alpm: &alpm::Alpm,
    pkg: &alpm::Package,
    installed_pkg: Option<&alpm::Package>,
    catalog: &AlpmCatalog,
) -> Package {
    let name = pkg.name().to_string();
    let source = determine_source(alpm, pkg);
    let app_meta = catalog.get(&name);

    let groups_vec: Vec<&str> = pkg.groups().into_iter().collect();
    let (display_name, summary, description, icon, categories, screenshots) = if let Some(meta) = app_meta {
        let desc = meta.description.clone().unwrap_or_else(|| pkg.desc().unwrap_or_default().to_string());
        let sum = if !meta.summary.is_empty() {
            meta.summary.clone()
        } else {
            pkg.desc().unwrap_or_default().to_string()
        };
        (
            Some(meta.name.clone()),
            sum,
            Some(desc),
            meta.icon.clone(),
            meta.categories.clone(),
            meta.screenshots.clone(),
        )
    } else {
        let icon = PackageIcon::Themed(infer_theme_icon(&name, &groups_vec));
        let categories = infer_categories(&groups_vec);
        (
            None,
            pkg.desc().unwrap_or_default().to_string(),
            None,
            Some(icon),
            categories,
            vec![],
        )
    };

    let installed_version = installed_pkg.map(|p| p.version().to_string());
    let state = if let Some(ref inst_ver) = installed_version {
        if inst_ver != pkg.version().as_str() {
            PackageState::UpdateAvailable
        } else {
            PackageState::Installed
        }
    } else {
        PackageState::NotInstalled
    };

    let size_installed = if pkg.isize() > 0 {
        Some(pkg.isize() as u64)
    } else {
        None
    };

    let size_download = if pkg.download_size() > 0 {
        Some(pkg.download_size() as u64)
    } else {
        None
    };

    let dependencies: Vec<String> = pkg
        .depends()
        .iter()
        .map(|d| d.name().to_string())
        .collect();

    let license = pkg.licenses().first().map(|l| l.to_string());
    let homepage = pkg.url().map(|u| u.to_string());
    let maintainer = pkg.packager().map(|p| p.to_string());

    Package {
        id: PackageId::new(&name, source),
        name,
        display_name,
        version: pkg.version().to_string(),
        installed_version,
        summary,
        description,
        icon,
        screenshots,
        homepage,
        license,
        maintainer,
        categories,
        size_installed,
        size_download,
        dependencies,
        changelog: None,
        state,
    }
}

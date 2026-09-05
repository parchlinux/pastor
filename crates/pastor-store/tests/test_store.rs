use std::sync::Arc;

use pastor_mock::MockPackageBackend;
use pastor_store::Store;

#[tokio::test]
async fn test_store_search_and_priority() {
    let mock = Arc::new(MockPackageBackend::new());
    let snap = mock.snapshot_backend();
    let store = Store::with_backends(vec![mock], snap);

    let all_pkgs = store.search("").await.unwrap();
    assert!(!all_pkgs.is_empty());

    // Parch [world] packages should be ranked highest
    let first = &all_pkgs[0];
    assert!(first.id.source.is_parch_world() || first.id.source.is_parch());

    // Check updates query
    let updates = store.get_updates().await.unwrap();
    assert!(!updates.is_empty());
    assert!(updates.iter().any(|u| u.id.name == "parch-software-center"));

    // Check installed query
    let installed = store.get_installed().await.unwrap();
    assert!(!installed.is_empty());
    assert!(installed.iter().any(|p| p.name == "parch-welcome"));
}

#[tokio::test]
async fn test_gedit_alternatives() {
    let store = Store::new();
    let alts = store.get_alternatives("org.gnome.gedit.desktop").await.unwrap();
    assert_eq!(alts.len(), 1);
    assert_eq!(alts[0].name, "org.gnome.gedit");
}

#[tokio::test]
async fn test_active_transactions_tracking() {
    let mock = Arc::new(MockPackageBackend::new());
    let snap = mock.snapshot_backend();
    let store = Store::with_backends(vec![mock], snap);

    let all_pkgs = store.search("parch-welcome").await.unwrap();
    let pkg = all_pkgs.first().expect("parch-welcome found");

    let pkg_id = pkg.id.clone();
    let mut rx = store.install(pkg_id.clone());

    // Receive first event to ensure install has started
    let evt = rx.recv().await.expect("received event");
    assert!(evt.progress_fraction >= 0.0);

    // Verify store tracking
    assert!(store.is_installing(&pkg_id));
    let active = store.get_active_transactions();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].package_id, pkg_id);

    // Cancel installation
    let cancel_res = store.cancel_installation(&pkg_id).await;
    assert!(cancel_res.is_ok());
}

#[tokio::test]
async fn test_package_deduplication() {
    use pastor_core::{Package, PackageId, PackageSource, ParchRepoType, PackageState};

    let alpm_gedit = Package {
        id: PackageId {
            name: "gedit".to_string(),
            source: PackageSource::Parch(ParchRepoType::World),
        },
        name: "gedit".to_string(),
        display_name: Some("GNOME Text Editor".to_string()),
        version: "46.1-1".to_string(),
        installed_version: None,
        summary: "GNOME Text Editor".to_string(),
        description: Some("Official text editor for GNOME".to_string()),
        icon: None,
        screenshots: vec![],
        homepage: None,
        license: Some("GPL-2.0-or-later".to_string()),
        maintainer: Some("GNOME".to_string()),
        categories: vec![],
        size_installed: Some(1024 * 1024),
        size_download: Some(512 * 1024),
        dependencies: vec![],
        changelog: None,
        state: PackageState::NotInstalled,
    };

    let flatpak_gedit = Package {
        id: PackageId {
            name: "org.gnome.gedit".to_string(),
            source: PackageSource::Flatpak { remote: "flathub".to_string() },
        },
        name: "org.gnome.gedit".to_string(),
        display_name: Some("GNOME Text Editor".to_string()),
        version: "46.1".to_string(),
        installed_version: None,
        summary: "GNOME Text Editor".to_string(),
        description: Some("Official text editor for GNOME".to_string()),
        icon: None,
        screenshots: vec![],
        homepage: None,
        license: Some("GPL-2.0-or-later".to_string()),
        maintainer: Some("GNOME".to_string()),
        categories: vec![],
        size_installed: Some(2048 * 1024),
        size_download: Some(1024 * 1024),
        dependencies: vec![],
        changelog: None,
        state: PackageState::NotInstalled,
    };

    let pkgs = vec![alpm_gedit.clone(), flatpak_gedit.clone()];
    let deduped = Store::deduplicate_packages(pkgs);
    assert_eq!(deduped.len(), 1);
    assert_eq!(deduped[0].id, alpm_gedit.id);
}



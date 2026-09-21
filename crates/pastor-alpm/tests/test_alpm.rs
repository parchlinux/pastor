use pastor_core::PackageBackend;
use pastor_alpm::{AlpmBackend, AlpmCatalog};

#[tokio::test]
async fn test_alpm_catalog_metadata() {
    let catalog = AlpmCatalog::load_system();
    assert!(!catalog.by_pkgname.is_empty(), "Catalog should have packages");

    // Test Amberol
    let amberol = catalog.get("amberol").expect("Amberol should be found in catalog");
    assert_eq!(amberol.name, "Amberol");
    assert!(amberol.icon.is_some(), "Amberol should have an icon");
    assert!(!amberol.screenshots.is_empty(), "Amberol should have screenshots");

    // Test Dialect
    let dialect = catalog.get("dialect").expect("Dialect should be found in catalog");
    assert_eq!(dialect.name, "Dialect");
    assert!(dialect.icon.is_some(), "Dialect should have an icon");
    assert!(!dialect.screenshots.is_empty(), "Dialect should have screenshots");

    // Test Steam
    let steam = catalog.get("steam");
    println!("Steam in catalog: {:?}", steam.as_ref().map(|s| (&s.name, &s.icon, &s.screenshots)));
}

#[tokio::test]
async fn test_alpm_backend_operations() {
    let backend = AlpmBackend::new().expect("AlpmBackend should initialize");
    backend.wait_catalog_ready().await;

    // Spotlight must be Flatpak only
    let picks = backend.curated_picks().await.expect("curated_picks should succeed");
    assert!(picks.is_empty(), "ALPM backend should not provide curated picks (spotlight is Flatpak only)");

    // Updates should succeed and return pending updates
    let updates = backend.updates().await.expect("updates should succeed");
    println!("Found {} ALPM updates", updates.len());
    for u in updates.iter().take(5) {
        println!("  Update: {} ({} -> {})", u.id.name, u.current_version, u.new_version);
    }

    // Search should return enriched packages
    let results = backend.search("firefox").await.expect("search firefox should succeed");
    assert!(!results.is_empty(), "Search for firefox should return results");
    let ff = results.iter().find(|p| p.name == "firefox").expect("firefox package should exist");
    if ff.display_name.is_some() {
        assert_eq!(ff.display_title(), "Firefox");
        assert!(ff.icon.is_some(), "firefox should have an icon");
    } else {
        assert_eq!(ff.display_title(), "firefox");
    }

    // Search for "disk utility" multi-term query
    let disk_results = backend.search("disk utility").await.expect("search disk utility should succeed");
    assert!(!disk_results.is_empty(), "Search for 'disk utility' should return results");
    let names: Vec<&str> = disk_results.iter().map(|p| p.name.as_str()).collect();
    println!("'disk utility' search results: {:?}", names);
    assert!(names.contains(&"gnome-disk-utility"), "results should include gnome-disk-utility");
    assert_eq!(disk_results[0].name, "gnome-disk-utility", "gnome-disk-utility should be top ranked for 'disk utility'");
}

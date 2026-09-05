use pastor_core::PackageBackend;
use pastor_flatpak::FlatpakBackend;

#[tokio::test]
async fn test_flatpak_search_firefox() {
    if let Ok(backend) = FlatpakBackend::new(false) {
        let results = backend.search("firefox").await.unwrap();
        println!("Found {} results for 'firefox'", results.len());
        for p in &results {
            println!("- {} ({})", p.name, p.display_title());
        }
        assert!(results.iter().any(|p| p.name == "org.mozilla.firefox"));

        let internet = backend.get_by_category(pastor_core::PackageCategory::Internet).await.unwrap();
        println!("Internet category count: {}", internet.len());
        assert!(internet.len() > 10);
        assert!(internet.iter().any(|p| p.name == "org.mozilla.firefox"));

        let picks = backend.curated_picks().await.unwrap();
        println!("Curated picks count: {}", picks.len());
        assert!(!picks.is_empty());
        for p in &picks {
            println!("  - Pick: {} ({})", p.name, p.display_title());
        }

        let featured = backend.get_by_category(pastor_core::PackageCategory::Featured).await.unwrap();
        println!("Featured apps count: {}", featured.len());
        assert!(!featured.is_empty());
        for p in &featured {
            println!("  - Featured: {} ({})", p.name, p.display_title());
        }

        let parch_picks = backend.get_by_category(pastor_core::PackageCategory::ParchPicks).await.unwrap();
        println!("Parch picks count: {}", parch_picks.len());
        assert!(!parch_picks.is_empty());
        for p in &parch_picks {
            println!("  - ParchPick: {} ({})", p.name, p.display_title());
        }

        let gedit_results = backend.search("gedit").await.unwrap();
        println!("Gedit search count: {}", gedit_results.len());
        for p in &gedit_results {
            println!("  - Gedit: id={:?}, name={}, display_title={}, version={}", p.id, p.name, p.display_title(), p.version);
        }
    }
}

use pastor_core::{
    PackageCategory, PackageId, PackageSource, ParchRepoType,
};

#[test]
fn test_package_id_and_sources() {
    let world_id = PackageId::new("parch-welcome", PackageSource::Parch(ParchRepoType::World));
    assert!(world_id.source.is_parch());
    assert!(world_id.source.is_parch_world());
    assert!(!world_id.source.is_parch_void());
    assert_eq!(world_id.source.display_badge(), "Parch [world]");

    let void_id = PackageId::new("linux-parch-void", PackageSource::Parch(ParchRepoType::Void));
    assert!(void_id.source.is_parch_void());
    assert_eq!(void_id.source.display_badge(), "Parch [void]");

    let aur_id = PackageId::new("spotify", PackageSource::Aur);
    assert_eq!(aur_id.source.display_badge(), "AUR");

    let flatpak_id = PackageId::new("com.discordapp.Discord", PackageSource::Flatpak { remote: "flathub".into() });
    assert_eq!(flatpak_id.source.display_badge(), "Flatpak");
}

#[test]
fn test_categories() {
    let cats = PackageCategory::all();
    assert!(cats.contains(&PackageCategory::Development));
    assert!(cats.contains(&PackageCategory::Android));
    assert_eq!(PackageCategory::Android.title(), "Android Apps (Waydroid)");
}

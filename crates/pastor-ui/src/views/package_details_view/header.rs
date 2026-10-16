use adw::prelude::*;
use pastor_core::{Package, PackageCategory, PackageIcon, PackageSource, ParchRepoType};
use pastor_store::Store;

pub fn create_hero_box(
    pkg: &Package,
    store: &Store,
    on_select_package: impl Fn(Package) + 'static + Clone,
) -> (gtk4::Box, gtk4::Box) {
    let hero_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(24)
        .margin_top(8)
        .margin_bottom(6)
        .build();

    // App Icon
    let icon_container = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .valign(gtk4::Align::Center)
        .css_classes(["app-icon-hero"])
        .build();

    let icon = match &pkg.icon {
        Some(PackageIcon::LocalPath(path)) => {
            let img = gtk4::Image::from_file(path);
            img.set_pixel_size(crate::icons::ICON_SIZE_XL);
            img
        }
        Some(PackageIcon::Themed(name)) => gtk4::Image::builder()
            .icon_name(name)
            .pixel_size(crate::icons::ICON_SIZE_XL)
            .build(),
        _ => gtk4::Image::builder()
            .icon_name("application-x-executable")
            .pixel_size(crate::icons::ICON_SIZE_XL)
            .build(),
    };
    icon_container.append(&icon);
    hero_box.append(&icon_container);

    // Metadata: Title, Publisher, Badges
    let meta_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .hexpand(true)
        .valign(gtk4::Align::Center)
        .build();

    let title_label = gtk4::Label::builder()
        .label(pkg.display_title())
        .css_classes(["title-1"])
        .halign(gtk4::Align::Start)
        .build();
    meta_box.append(&title_label);

    let dev_name = pkg.maintainer.as_deref().unwrap_or("ParchLinux Contributors");
    let lic_str = pkg.license.as_deref().unwrap_or("Open Source");
    let dev_display = if let Some((name, _)) = dev_name.split_once('<') {
        name.trim()
    } else {
        dev_name.trim()
    };
    let meta_subtitle = format!("{} • v{} • {}", dev_display, pkg.version, lic_str);

    let developer_label = gtk4::Label::builder()
        .label(&meta_subtitle)
        .css_classes(["caption", "dim-label"])
        .halign(gtk4::Align::Start)
        .build();
    meta_box.append(&developer_label);

    // Badges Row
    let badges_row = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(8)
        .margin_top(6)
        .build();

    let (repo_badge_text, repo_badge_class) = match &pkg.id.source {
        PackageSource::Parch(ParchRepoType::World) => ("Parch [world]", "parch-badge-world"),
        PackageSource::Parch(ParchRepoType::Void) => ("Parch [void]", "parch-badge-void"),
        PackageSource::Arch(_) => ("Arch Repo", "parch-badge-arch"),
        PackageSource::Aur => ("AUR", "parch-badge-aur"),
        PackageSource::Flatpak { .. } => ("Flatpak", "parch-badge-flatpak"),
        PackageSource::Bootc => ("bootc OS", "parch-badge-bootc"),
        PackageSource::Waydroid => ("Waydroid", "parch-badge-waydroid"),
    };

    let repo_badge = gtk4::Label::builder()
        .label(repo_badge_text)
        .css_classes([repo_badge_class])
        .valign(gtk4::Align::Center)
        .build();
    badges_row.append(&repo_badge);

    let (safety_text, safety_class) = match &pkg.id.source {
        PackageSource::Flatpak { .. } => ("Sandboxed", "metadata-badge-accent"),
        PackageSource::Bootc => ("Immutable Root", "metadata-badge-accent"),
        PackageSource::Aur => ("Verified PKGBUILD", "metadata-badge-accent"),
        _ => ("Snapper Protected", "metadata-badge"),
    };
    let safety_badge = gtk4::Label::builder()
        .label(safety_text)
        .css_classes([safety_class])
        .valign(gtk4::Align::Center)
        .build();
    badges_row.append(&safety_badge);

    if let Some(first_cat) = pkg.categories.first() {
        let cat_badge = gtk4::Label::builder()
            .label(first_cat.title())
            .css_classes(["metadata-badge"])
            .valign(gtk4::Align::Center)
            .build();
        badges_row.append(&cat_badge);
    }

    meta_box.append(&badges_row);

    // Multi-source switcher
    let source_switcher_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(8)
        .margin_top(4)
        .visible(false)
        .build();

    let source_lbl = gtk4::Label::builder()
        .label("Source:")
        .css_classes(["caption", "dim-label"])
        .valign(gtk4::Align::Center)
        .build();
    source_switcher_box.append(&source_lbl);

    let source_dropdown = gtk4::DropDown::builder()
        .valign(gtk4::Align::Center)
        .build();
    source_switcher_box.append(&source_dropdown);
    meta_box.append(&source_switcher_box);

    let store_alt = store.clone();
    let current_pkg_clone = pkg.clone();
    let switcher_box_clone = source_switcher_box.clone();
    let dropdown_clone = source_dropdown.clone();
    let on_sel_alt = on_select_package.clone();

    gtk4::glib::spawn_future_local(async move {
        if let Ok(mut alts) = store_alt.get_alternatives(&current_pkg_clone.name).await {
            if !alts.iter().any(|p| p.id == current_pkg_clone.id) {
                alts.insert(0, current_pkg_clone.clone());
            }

            let mut titles: Vec<String> = Vec::new();
            let mut unique_alts: Vec<Package> = Vec::new();
            let mut seen_sources = std::collections::HashSet::new();
            let mut selected_idx = 0;

            for alt_p in alts {
                let source_desc = match &alt_p.id.source {
                    PackageSource::Parch(ParchRepoType::World) => "Parch [world]".to_string(),
                    PackageSource::Parch(ParchRepoType::Void) => "Parch [void]".to_string(),
                    PackageSource::Arch(r) => format!("Arch ({r})"),
                    PackageSource::Aur => "AUR (Community)".to_string(),
                    PackageSource::Flatpak { remote } => format!("Flatpak ({remote})"),
                    PackageSource::Bootc => "bootc (Base OS)".to_string(),
                    PackageSource::Waydroid => "Waydroid (Android)".to_string(),
                };

                let source_key = match &alt_p.id.source {
                    PackageSource::Flatpak { .. } => "flatpak".to_string(),
                    PackageSource::Arch(_) => "arch".to_string(),
                    PackageSource::Parch(_) => "parch".to_string(),
                    PackageSource::Aur => "aur".to_string(),
                    PackageSource::Bootc => "bootc".to_string(),
                    PackageSource::Waydroid => "waydroid".to_string(),
                };

                if seen_sources.insert(source_key) {
                    let label = format!("{} - v{}", source_desc, alt_p.version);
                    if alt_p.id == current_pkg_clone.id {
                        selected_idx = unique_alts.len();
                    }
                    titles.push(label);
                    unique_alts.push(alt_p);
                }
            }

            if unique_alts.len() > 1 {
                let str_slice: Vec<&str> = titles.iter().map(|s| s.as_str()).collect();
                let str_list = gtk4::StringList::new(&str_slice);
                dropdown_clone.set_model(Some(&str_list));
                dropdown_clone.set_selected(selected_idx as u32);
                switcher_box_clone.set_visible(true);

                let alts_rc = std::rc::Rc::new(unique_alts);
                let on_sel = on_sel_alt.clone();
                let current_id = current_pkg_clone.id.clone();
                dropdown_clone.connect_selected_notify(move |dd| {
                    let sel = dd.selected() as usize;
                    if let Some(target_pkg) = alts_rc.get(sel) {
                        if target_pkg.id != current_id {
                            on_sel(target_pkg.clone());
                        }
                    }
                });
            } else {
                switcher_box_clone.set_visible(false);
            }
        }
    });

    hero_box.append(&meta_box);

    let actions_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(10)
        .valign(gtk4::Align::Center)
        .halign(gtk4::Align::End)
        .build();

    (hero_box, actions_box)
}

pub fn create_preview_canvas(pkg: &Package, _slide_index: usize) -> gtk4::Widget {
    let card = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Center)
        .margin_top(32)
        .margin_bottom(32)
        .margin_start(32)
        .margin_end(32)
        .build();

    let icon_img = match &pkg.icon {
        Some(PackageIcon::LocalPath(path)) => {
            let img = gtk4::Image::from_file(path);
            img.set_pixel_size(crate::icons::ICON_SIZE_LG);
            img
        }
        Some(PackageIcon::Themed(name)) => gtk4::Image::builder()
            .icon_name(name)
            .pixel_size(crate::icons::ICON_SIZE_LG)
            .build(),
        _ => gtk4::Image::builder()
            .icon_name("application-x-executable")
            .pixel_size(crate::icons::ICON_SIZE_LG)
            .build(),
    };
    icon_img.add_css_class("app-icon-hero");
    card.append(&icon_img);

    let title_label = gtk4::Label::builder()
        .label(pkg.display_title())
        .css_classes(["title-2"])
        .halign(gtk4::Align::Center)
        .build();
    card.append(&title_label);

    let ver_label = gtk4::Label::builder()
        .label(format!("Version {}", pkg.version))
        .css_classes(["caption", "dim-label"])
        .halign(gtk4::Align::Center)
        .build();
    card.append(&ver_label);

    let cat = pkg.categories.first().copied().unwrap_or(PackageCategory::Utilities);
    let cat_label = gtk4::Label::builder()
        .label(cat.title())
        .css_classes(["metadata-badge"])
        .halign(gtk4::Align::Center)
        .margin_top(4)
        .build();
    card.append(&cat_label);

    card.upcast()
}

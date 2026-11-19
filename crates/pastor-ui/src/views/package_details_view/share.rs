use adw::prelude::*;
use gtk4::glib;
use pastor_core::{Package, PackageSource};
use pastor_store::Store;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareOption {
    pub title: String,
    pub description: String,
    pub url: String,
    pub icon_name: &'static str,
}

pub fn build_share_options(pkg: &Package, flatpak_id: Option<&str>) -> Vec<ShareOption> {
    let mut options = Vec::new();

    // 1. Parch Apps Web link (apps.parchlinux.com)
    options.push(ShareOption {
        title: pastor_core::tr("Parch Apps Store").to_string(),
        description: pastor_core::tr("Official web link (apps.parchlinux.com)").to_string(),
        url: format!("https://apps.parchlinux.com/pkg/{}", pkg.name),
        icon_name: "emblem-favorite-symbolic",
    });

    // 2. Flathub option
    let resolved_flatpak = match &pkg.id.source {
        PackageSource::Flatpak { .. } => Some(pkg.id.name.as_str()),
        _ => flatpak_id,
    };

    if let Some(f_id) = resolved_flatpak {
        options.push(ShareOption {
            title: pastor_core::tr("Flathub (Flatpak)").to_string(),
            description: pastor_core::tr("Cross-distro link for all Linux systems").to_string(),
            url: format!("https://flathub.org/apps/{f_id}"),
            icon_name: "package-x-generic-symbolic",
        });
    } else if pkg.name.contains('.') {
        options.push(ShareOption {
            title: pastor_core::tr("Flathub (Flatpak)").to_string(),
            description: pastor_core::tr("Cross-distro link for all Linux systems").to_string(),
            url: format!("https://flathub.org/apps/{}", pkg.name),
            icon_name: "package-x-generic-symbolic",
        });
    }

    // 3. Official Website
    if let Some(ref hp) = pkg.homepage {
        let trimmed = hp.trim();
        if !trimmed.is_empty() {
            options.push(ShareOption {
                title: pastor_core::tr("Official Website").to_string(),
                description: pastor_core::tr("Project homepage").to_string(),
                url: trimmed.to_string(),
                icon_name: "web-browser-symbolic",
            });
        }
    }

    // 4. Repository details (Arch, AUR, Parch)
    match &pkg.id.source {
        PackageSource::Arch(repo) => {
            options.push(ShareOption {
                title: format!("Arch Linux [{repo}]"),
                description: pastor_core::tr("Official Arch repository package").to_string(),
                url: format!("https://archlinux.org/packages/{}/x86_64/{}/", repo, pkg.name),
                icon_name: "applications-system-symbolic",
            });
        }
        PackageSource::Aur => {
            options.push(ShareOption {
                title: pastor_core::tr("Arch User Repository (AUR)").to_string(),
                description: pastor_core::tr("AUR package details and PKGBUILD").to_string(),
                url: format!("https://aur.archlinux.org/packages/{}", pkg.name),
                icon_name: "applications-system-symbolic",
            });
        }
        PackageSource::Parch(_) => {
            options.push(ShareOption {
                title: pastor_core::tr("Parch Linux Package").to_string(),
                description: pastor_core::tr("ParchLinux repository package").to_string(),
                url: format!("https://archlinux.org/packages/?q={}", pkg.name),
                icon_name: "applications-system-symbolic",
            });
        }
        _ => {}
    }

    // 5. AppStream Deep Link (opens in Pastor, GNOME Software, Discover, etc.)
    let deep_id = resolved_flatpak.unwrap_or(&pkg.id.name);
    options.push(ShareOption {
        title: pastor_core::tr("AppStream Link").to_string(),
        description: pastor_core::tr("Direct link to open in software centers").to_string(),
        url: format!("appstream://{deep_id}"),
        icon_name: "emblem-shared-symbolic",
    });

    options
}

fn populate_share_list(list_box: &gtk4::ListBox, options: &[ShareOption]) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    for opt in options {
        let row = adw::ActionRow::builder()
            .title(&opt.title)
            .subtitle(&opt.url)
            .subtitle_lines(1)
            .activatable(true)
            .build();

        let icon = gtk4::Image::builder()
            .icon_name(opt.icon_name)
            .pixel_size(20)
            .valign(gtk4::Align::Center)
            .build();
        row.add_prefix(&icon);

        // Copy button
        let copy_btn = gtk4::Button::builder()
            .icon_name("edit-copy-symbolic")
            .tooltip_text(pastor_core::tr("Copy link to clipboard"))
            .css_classes(["flat", "circular"])
            .valign(gtk4::Align::Center)
            .build();

        let url_c = opt.url.clone();
        let copy_btn_c = copy_btn.clone();
        copy_btn.connect_clicked(move |_| {
            copy_btn_c.clipboard().set_text(&url_c);
            copy_btn_c.set_icon_name("emblem-ok-symbolic");
            copy_btn_c.set_tooltip_text(Some(pastor_core::tr("Copied!")));
            let b = copy_btn_c.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(2000), move || {
                b.set_icon_name("edit-copy-symbolic");
                b.set_tooltip_text(Some(pastor_core::tr("Copy link to clipboard")));
            });
        });
        row.add_suffix(&copy_btn);

        // Open in browser button if http/https
        if opt.url.starts_with("http://") || opt.url.starts_with("https://") {
            let open_btn = gtk4::LinkButton::builder()
                .uri(&opt.url)
                .icon_name("external-link-symbolic")
                .tooltip_text(pastor_core::tr("Open in browser"))
                .css_classes(["flat", "circular"])
                .valign(gtk4::Align::Center)
                .build();
            row.add_suffix(&open_btn);
        }

        let url_row = opt.url.clone();
        let copy_btn_row = copy_btn.clone();
        row.connect_activated(move |_| {
            copy_btn_row.clipboard().set_text(&url_row);
            copy_btn_row.set_icon_name("emblem-ok-symbolic");
            copy_btn_row.set_tooltip_text(Some(pastor_core::tr("Copied!")));
            let b = copy_btn_row.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(2000), move || {
                b.set_icon_name("edit-copy-symbolic");
                b.set_tooltip_text(Some(pastor_core::tr("Copy link to clipboard")));
            });
        });

        list_box.append(&row);
    }
}

pub fn show_share_dialog(parent: &impl IsA<gtk4::Widget>, pkg: &Package, store: &Store) {
    let dialog = adw::Dialog::builder()
        .title(pastor_core::tr("Share Application"))
        .presentation_mode(adw::DialogPresentationMode::BottomSheet)
        .content_width(340)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header_bar = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header_bar);

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .propagate_natural_height(true)
        .build();

    let clamp = adw::Clamp::builder()
        .maximum_size(360)
        .tightening_threshold(320)
        .margin_top(8)
        .margin_bottom(18)
        .margin_start(8)
        .margin_end(8)
        .build();

    let content_vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .build();

    let app_header = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(4)
        .margin_bottom(6)
        .halign(gtk4::Align::Center)
        .build();

    let title_lbl = gtk4::Label::builder()
        .label(&format!("Share {}", pkg.display_title()))
        .css_classes(["title-3"])
        .halign(gtk4::Align::Center)
        .build();
    app_header.append(&title_lbl);

    let subtitle_lbl = gtk4::Label::builder()
        .label(pastor_core::tr("Choose a link below to copy or open in browser"))
        .css_classes(["caption", "dim-label"])
        .halign(gtk4::Align::Center)
        .build();
    app_header.append(&subtitle_lbl);

    content_vbox.append(&app_header);

    let list_box = gtk4::ListBox::builder()
        .css_classes(["boxed-list"])
        .selection_mode(gtk4::SelectionMode::None)
        .build();
    content_vbox.append(&list_box);

    let initial_options = build_share_options(pkg, None);
    populate_share_list(&list_box, &initial_options);

    let store_c = store.clone();
    let pkg_name = pkg.name.clone();
    let pkg_clone = pkg.clone();
    let list_box_c = list_box.clone();

    glib::spawn_future_local(async move {
        if let Ok(alts) = store_c.get_alternatives(&pkg_name).await {
            let flatpak_alt = alts.into_iter().find_map(|alt| match alt.id.source {
                PackageSource::Flatpak { .. } => Some(alt.id.name),
                _ => None,
            });
            if flatpak_alt.is_some() {
                let updated_options = build_share_options(&pkg_clone, flatpak_alt.as_deref());
                populate_share_list(&list_box_c, &updated_options);
            }
        }
    });

    clamp.set_child(Some(&content_vbox));
    scrolled.set_child(Some(&clamp));
    toolbar_view.set_content(Some(&scrolled));
    dialog.set_child(Some(&toolbar_view));

    dialog.present(Some(parent));
}

pub fn create_share_button(pkg: &Package, store: &Store) -> gtk4::Button {
    let btn = gtk4::Button::builder()
        .icon_name("emblem-shared-symbolic")
        .tooltip_text(pastor_core::tr("Share application"))
        .css_classes(["flat", "circular"])
        .valign(gtk4::Align::Center)
        .build();
    btn.update_property(&[gtk4::accessible::Property::Label(pastor_core::tr("Share application"))]);

    let pkg_c = pkg.clone();
    let store_c = store.clone();
    let btn_c = btn.clone();
    btn.connect_clicked(move |_| {
        show_share_dialog(&btn_c, &pkg_c, &store_c);
    });

    btn
}

#[cfg(test)]
mod tests {
    use super::*;
    use pastor_core::{PackageCategory, PackageId, PackageState};

    fn make_test_package(name: &str, source: PackageSource, homepage: Option<&str>) -> Package {
        Package {
            id: PackageId::new(name, source),
            name: name.to_string(),
            display_name: Some("Test App".to_string()),
            version: "1.0.0".to_string(),
            installed_version: None,
            summary: "A test application".to_string(),
            description: None,
            icon: None,
            screenshots: vec![],
            homepage: homepage.map(|s| s.to_string()),
            license: Some("GPL-3.0-only".to_string()),
            maintainer: None,
            categories: vec![PackageCategory::Utilities],
            size_installed: None,
            size_download: None,
            dependencies: vec![],
            changelog: None,
            state: PackageState::NotInstalled,
        }
    }

    #[test]
    fn test_flatpak_share_links() {
        let pkg = make_test_package(
            "org.mozilla.firefox",
            PackageSource::Flatpak {
                remote: "flathub".to_string(),
            },
            Some("https://www.mozilla.org/firefox"),
        );
        let opts = build_share_options(&pkg, None);

        assert!(opts.iter().any(|o| o.url == "https://apps.parchlinux.com/pkg/org.mozilla.firefox"));
        assert!(opts.iter().any(|o| o.url == "https://flathub.org/apps/org.mozilla.firefox"));
        assert!(opts.iter().any(|o| o.url == "https://www.mozilla.org/firefox"));
        assert!(opts.iter().any(|o| o.url == "appstream://org.mozilla.firefox"));
    }

    #[test]
    fn test_arch_share_links_with_flatpak_alt() {
        let pkg = make_test_package(
            "firefox",
            PackageSource::Arch("extra".to_string()),
            Some("https://www.mozilla.org"),
        );
        let opts = build_share_options(&pkg, Some("org.mozilla.firefox"));

        assert!(opts.iter().any(|o| o.url == "https://apps.parchlinux.com/pkg/firefox"));
        assert!(opts.iter().any(|o| o.url == "https://flathub.org/apps/org.mozilla.firefox"));
        assert!(opts.iter().any(|o| o.url == "https://archlinux.org/packages/extra/x86_64/firefox/"));
        assert!(opts.iter().any(|o| o.url == "appstream://org.mozilla.firefox"));
    }

    #[test]
    fn test_aur_share_links() {
        let pkg = make_test_package(
            "visual-studio-code-bin",
            PackageSource::Aur,
            Some("https://code.visualstudio.com"),
        );
        let opts = build_share_options(&pkg, None);

        assert!(opts.iter().any(|o| o.url == "https://aur.archlinux.org/packages/visual-studio-code-bin"));
        assert!(opts.iter().any(|o| o.url == "https://code.visualstudio.com"));
        assert!(opts.iter().any(|o| o.url == "appstream://visual-studio-code-bin"));
    }
}

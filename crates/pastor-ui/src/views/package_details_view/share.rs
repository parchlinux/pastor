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

    // 1. Flathub option
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

    // 2. Official Website
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

    // 3. Repository details (Arch, AUR, Parch)
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

    // 4. AppStream Deep Link (opens in Pastor, GNOME Software, Discover, etc.)
    let deep_id = resolved_flatpak.unwrap_or(&pkg.id.name);
    options.push(ShareOption {
        title: pastor_core::tr("AppStream Link").to_string(),
        description: pastor_core::tr("Direct link to open in software centers").to_string(),
        url: format!("appstream://{deep_id}"),
        icon_name: "emblem-shared-symbolic",
    });

    options
}

fn render_option_rows(
    options_box: &gtk4::Box,
    options: &[ShareOption],
    toast_lbl: &gtk4::Label,
) {
    while let Some(child) = options_box.first_child() {
        options_box.remove(&child);
    }

    for opt in options {
        let row_card = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(10)
            .css_classes(["card"])
            .margin_top(2)
            .margin_bottom(2)
            .build();

        let row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(10)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(10)
            .margin_end(10)
            .hexpand(true)
            .valign(gtk4::Align::Center)
            .build();

        let icon = gtk4::Image::builder()
            .icon_name(opt.icon_name)
            .pixel_size(22)
            .valign(gtk4::Align::Center)
            .build();
        row.append(&icon);

        let text_vbox = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(2)
            .hexpand(true)
            .valign(gtk4::Align::Center)
            .build();

        let title_lbl = gtk4::Label::builder()
            .label(&opt.title)
            .css_classes(["heading"])
            .halign(gtk4::Align::Start)
            .build();
        text_vbox.append(&title_lbl);

        let url_lbl = gtk4::Label::builder()
            .label(&opt.url)
            .css_classes(["caption", "dim-label"])
            .halign(gtk4::Align::Start)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .max_width_chars(28)
            .build();
        text_vbox.append(&url_lbl);

        row.append(&text_vbox);

        // Copy button
        let copy_btn = gtk4::Button::builder()
            .icon_name("edit-copy-symbolic")
            .tooltip_text("Copy link to clipboard")
            .css_classes(["flat", "circular"])
            .valign(gtk4::Align::Center)
            .build();

        let url_to_copy = opt.url.clone();
        let toast_target = toast_lbl.clone();
        copy_btn.connect_clicked(move |btn| {
            btn.clipboard().set_text(&url_to_copy);
            btn.set_icon_name("emblem-ok-symbolic");
            toast_target.set_visible(true);

            let btn_reset = btn.clone();
            let toast_reset = toast_target.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(2000), move || {
                btn_reset.set_icon_name("edit-copy-symbolic");
                toast_reset.set_visible(false);
            });
        });
        row.append(&copy_btn);

        // Open in browser button if http/https
        if opt.url.starts_with("http://") || opt.url.starts_with("https://") {
            let open_btn = gtk4::LinkButton::builder()
                .uri(&opt.url)
                .icon_name("external-link-symbolic")
                .tooltip_text("Open in browser")
                .css_classes(["flat", "circular"])
                .valign(gtk4::Align::Center)
                .build();
            row.append(&open_btn);
        }

        row_card.append(&row);
        options_box.append(&row_card);
    }
}

pub fn create_share_button(pkg: &Package, store: &Store) -> gtk4::MenuButton {
    let btn = gtk4::MenuButton::builder()
        .icon_name("emblem-shared-symbolic")
        .tooltip_text("Share application")
        .css_classes(["flat", "circular"])
        .build();

    let popover = gtk4::Popover::builder()
        .position(gtk4::PositionType::Bottom)
        .autohide(true)
        .build();

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(10)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .width_request(360)
        .build();

    // Title and app name
    let header_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(2)
        .build();

    let title_lbl = gtk4::Label::builder()
        .label(&format!("Share {}", pkg.display_title()))
        .css_classes(["title-4"])
        .halign(gtk4::Align::Start)
        .build();
    header_box.append(&title_lbl);

    let subtitle_lbl = gtk4::Label::builder()
        .label("Copy or open links to share with others")
        .css_classes(["caption", "dim-label"])
        .halign(gtk4::Align::Start)
        .build();
    header_box.append(&subtitle_lbl);

    content_box.append(&header_box);

    let options_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .build();
    content_box.append(&options_box);

    let toast_lbl = gtk4::Label::builder()
        .label("✓ Link copied to clipboard")
        .css_classes(["caption", "success"])
        .halign(gtk4::Align::Center)
        .visible(false)
        .build();
    content_box.append(&toast_lbl);

    // Initial render with current package information
    let initial_options = build_share_options(pkg, None);
    render_option_rows(&options_box, &initial_options, &toast_lbl);

    // Asynchronously resolve alternatives to find Flatpak ID if available
    let store_c = store.clone();
    let pkg_name = pkg.name.clone();
    let pkg_clone = pkg.clone();
    let options_box_c = options_box.clone();
    let toast_lbl_c = toast_lbl.clone();

    glib::spawn_future_local(async move {
        if let Ok(alts) = store_c.get_alternatives(&pkg_name).await {
            let flatpak_alt = alts.into_iter().find_map(|alt| match alt.id.source {
                PackageSource::Flatpak { .. } => Some(alt.id.name),
                _ => None,
            });
            if flatpak_alt.is_some() {
                let updated_options = build_share_options(&pkg_clone, flatpak_alt.as_deref());
                render_option_rows(&options_box_c, &updated_options, &toast_lbl_c);
            }
        }
    });

    popover.set_child(Some(&content_box));
    btn.set_popover(Some(&popover));

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

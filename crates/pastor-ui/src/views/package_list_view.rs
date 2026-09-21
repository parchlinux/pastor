use adw::prelude::*;
use gtk4::glib;
use pastor_core::{Package, PackageSource};
use pastor_store::Store;

use super::package_row::create_package_row;

pub fn create_package_list_view(
    title: &str,
    description: &str,
    packages: Vec<Package>,
    store: Store,
    on_select: impl Fn(Package) + 'static + Clone,
    on_transaction_start: impl Fn(tokio::sync::mpsc::Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
) -> gtk4::Widget {
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build();

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(16)
        .margin_top(16)
        .margin_bottom(24)
        .margin_start(16)
        .margin_end(16)
        .build();

    if packages.is_empty() {
        let status_page = adw::StatusPage::builder()
            .icon_name("system-search-symbolic")
            .title("No Packages Found")
            .description("No software matches your current query or category filter.")
            .build();
        content_box.append(&status_page);
    } else {
        let esc_title = glib::markup_escape_text(title);
        let esc_desc = glib::markup_escape_text(description);
        let group = adw::PreferencesGroup::builder()
            .title(esc_title.as_str())
            .description(esc_desc.as_str())
            .build();

        // -------------------------------------------------------------
        // Source Filter Bar (All, Official Repos, AUR, Flatpak)
        // -------------------------------------------------------------
        let all_count = packages.len();
        let repo_count = packages
            .iter()
            .filter(|p| matches!(p.id.source, PackageSource::Parch(_) | PackageSource::Arch(_)))
            .count();
        let aur_count = packages
            .iter()
            .filter(|p| matches!(p.id.source, PackageSource::Aur))
            .count();
        let flatpak_count = packages
            .iter()
            .filter(|p| matches!(p.id.source, PackageSource::Flatpak { .. }))
            .count();

        let filter_bar = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(10)
            .margin_bottom(8)
            .build();

        let filter_lbl = gtk4::Label::builder()
            .label("Filter source:")
            .css_classes(["caption", "dim-label"])
            .valign(gtk4::Align::Center)
            .build();
        filter_bar.append(&filter_lbl);

        let linked_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .css_classes(["linked"])
            .build();

        let btn_all = gtk4::ToggleButton::builder()
            .label(&format!("All ({all_count})"))
            .active(true)
            .build();

        let btn_repos = gtk4::ToggleButton::builder()
            .label(&format!("Official ({repo_count})"))
            .group(&btn_all)
            .build();

        let btn_aur = gtk4::ToggleButton::builder()
            .label(&format!("AUR ({aur_count})"))
            .group(&btn_all)
            .build();

        let btn_flatpak = gtk4::ToggleButton::builder()
            .label(&format!("Flatpak ({flatpak_count})"))
            .group(&btn_all)
            .build();

        linked_box.append(&btn_all);
        linked_box.append(&btn_repos);
        linked_box.append(&btn_aur);
        linked_box.append(&btn_flatpak);
        filter_bar.append(&linked_box);
        content_box.append(&filter_bar);

        let list_box = gtk4::ListBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();

        let mut row_sources: Vec<(gtk4::Widget, PackageSource)> = Vec::new();
        for pkg in &packages {
            let row = create_package_row(
                pkg,
                &store,
                on_select.clone(),
                on_transaction_start.clone(),
            );
            row_sources.push((row.clone().into(), pkg.id.source.clone()));
            list_box.append(&row);
        }

        group.add(&list_box);
        content_box.append(&group);

        let empty_filter_msg = adw::StatusPage::builder()
            .icon_name("system-search-symbolic")
            .title("No Packages in Filter")
            .description("No software packages in the selected repository category match this query.")
            .visible(false)
            .build();
        content_box.append(&empty_filter_msg);

        let update_filter = {
            let row_sources = row_sources.clone();
            let empty_msg = empty_filter_msg.clone();
            let list_box = list_box.clone();
            move |mode: &str| {
                let mut visible_count = 0;
                for (row, source) in &row_sources {
                    let is_visible = match mode {
                        "all" => true,
                        "repos" => matches!(source, PackageSource::Parch(_) | PackageSource::Arch(_)),
                        "aur" => matches!(source, PackageSource::Aur),
                        "flatpak" => matches!(source, PackageSource::Flatpak { .. }),
                        _ => true,
                    };
                    row.set_visible(is_visible);
                    if is_visible {
                        visible_count += 1;
                    }
                }
                list_box.set_visible(visible_count > 0);
                empty_msg.set_visible(visible_count == 0);
            }
        };

        let uf = update_filter.clone();
        btn_all.connect_toggled(move |btn| {
            if btn.is_active() {
                uf("all");
            }
        });

        let uf = update_filter.clone();
        btn_repos.connect_toggled(move |btn| {
            if btn.is_active() {
                uf("repos");
            }
        });

        let uf = update_filter.clone();
        btn_aur.connect_toggled(move |btn| {
            if btn.is_active() {
                uf("aur");
            }
        });

        let uf = update_filter.clone();
        btn_flatpak.connect_toggled(move |btn| {
            if btn.is_active() {
                uf("flatpak");
            }
        });
    }

    scrolled.set_child(Some(&content_box));
    scrolled.upcast()
}

pub fn create_loading_view(title: &str, description: &str) -> gtk4::Widget {
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build();

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(16)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(16)
        .margin_end(16)
        .build();

    let esc_title = glib::markup_escape_text(title);
    let esc_desc = glib::markup_escape_text(description);
    let group = adw::PreferencesGroup::builder()
        .title(esc_title.as_str())
        .description(esc_desc.as_str())
        .build();

    let spinner_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_top(48)
        .margin_bottom(48)
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Center)
        .build();

    let spinner = gtk4::Spinner::builder()
        .spinning(true)
        .width_request(40)
        .height_request(40)
        .build();

    let loading_lbl = gtk4::Label::builder()
        .label("Searching repositories, AUR, and Flathub catalog…")
        .css_classes(["dim-label", "caption"])
        .build();

    spinner_box.append(&spinner);
    spinner_box.append(&loading_lbl);
    group.add(&spinner_box);
    content_box.append(&group);

    scrolled.set_child(Some(&content_box));
    scrolled.upcast()
}

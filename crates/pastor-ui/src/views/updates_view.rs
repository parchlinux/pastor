use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::glib;
use pastor_core::{Package, PackageIcon, PackageState};
use pastor_store::Store;

use super::package_row::create_package_row;

pub fn create_updates_view(
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

    let dynamic_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(16)
        .build();

    content_box.append(&dynamic_box);
    scrolled.set_child(Some(&content_box));

    let load_ref: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));

    let load_updates = {
        let store = store.clone();
        let dynamic_box = dynamic_box.clone();
        let on_select = on_select.clone();
        let on_transaction_start = on_transaction_start.clone();
        let load_ref_clone = load_ref.clone();

        Rc::new(move || {
            // Clear current view
            while let Some(child) = dynamic_box.first_child() {
                dynamic_box.remove(&child);
            }

            // Spinner while scanning
            let loading_box = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Vertical)
                .spacing(14)
                .margin_top(64)
                .margin_bottom(64)
                .halign(gtk4::Align::Center)
                .valign(gtk4::Align::Center)
                .build();

            let spinner = gtk4::Spinner::builder()
                .spinning(true)
                .width_request(36)
                .height_request(36)
                .build();
            loading_box.append(&spinner);

            let lbl = gtk4::Label::builder()
                .label("Checking for application and system updates…")
                .css_classes(["dim-label"])
                .build();
            loading_box.append(&lbl);

            dynamic_box.append(&loading_box);

            let store_c = store.clone();
            let dyn_box_c = dynamic_box.clone();
            let on_sel_c = on_select.clone();
            let on_tx_c = on_transaction_start.clone();
            let load_ref_inner = load_ref_clone.clone();

            glib::spawn_future_local(async move {
                let updates = store_c.get_updates().await.unwrap_or_default();

                // Clear loading spinner
                while let Some(child) = dyn_box_c.first_child() {
                    dyn_box_c.remove(&child);
                }

                if updates.is_empty() {
                    let status_page = adw::StatusPage::builder()
                        .icon_name("emblem-ok-symbolic")
                        .title("Software Is Up to Date")
                        .description("All installed applications and Flatpaks are on their latest versions.")
                        .build();

                    let check_again_btn = gtk4::Button::builder()
                        .label("Check for Updates")
                        .css_classes(["pill", "suggested-action"])
                        .halign(gtk4::Align::Center)
                        .margin_top(16)
                        .build();

                    let retrigger = load_ref_inner.clone();
                    check_again_btn.connect_clicked(move |_| {
                        if let Some(f) = retrigger.borrow().as_ref() {
                            f();
                        }
                    });

                    status_page.set_child(Some(&check_again_btn));
                    dyn_box_c.append(&status_page);
                } else {
                    let mut pkgs = Vec::new();
                    for up in &updates {
                        let pkg = if let Ok(Some(mut p)) = store_c.get_package(&up.id).await {
                            p.state = PackageState::UpdateAvailable;
                            p.installed_version = Some(up.current_version.clone());
                            p.version = up.new_version.clone();
                            p.size_download = up.download_size;
                            if p.changelog.is_none() {
                                p.changelog = up.changelog.clone();
                            }
                            p
                        } else {
                            Package {
                                id: up.id.clone(),
                                name: up.id.name.clone(),
                                display_name: Some(up.id.name.clone()),
                                version: up.new_version.clone(),
                                installed_version: Some(up.current_version.clone()),
                                summary: format!("Update: {} → {}", up.current_version, up.new_version),
                                description: up.changelog.clone(),
                                icon: Some(PackageIcon::Themed("package-x-generic".to_string())),
                                screenshots: vec![],
                                homepage: None,
                                license: None,
                                maintainer: None,
                                categories: vec![],
                                size_installed: None,
                                size_download: up.download_size,
                                dependencies: vec![],
                                changelog: up.changelog.clone(),
                                state: PackageState::UpdateAvailable,
                            }
                        };
                        pkgs.push(pkg);
                    }

                    // Top Summary Card
                    let summary_card = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Horizontal)
                        .spacing(16)
                        .css_classes(["card"])
                        .margin_bottom(4)
                        .build();

                    let icon = gtk4::Image::builder()
                        .icon_name(crate::icons::update_icon())
                        .pixel_size(44)
                        .margin_start(16)
                        .margin_top(14)
                        .margin_bottom(14)
                        .build();
                    summary_card.append(&icon);

                    let text_box = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Vertical)
                        .spacing(4)
                        .hexpand(true)
                        .valign(gtk4::Align::Center)
                        .build();

                    let count = pkgs.len();
                    let heading_text = if count == 1 {
                        "1 Update Available".to_string()
                    } else {
                        format!("{count} Updates Available")
                    };

                    let title_lbl = gtk4::Label::builder()
                        .label(&heading_text)
                        .css_classes(["heading"])
                        .halign(gtk4::Align::Start)
                        .build();
                    text_box.append(&title_lbl);

                    let sub_lbl = gtk4::Label::builder()
                        .label("Updates are installed safely. Snapper snapshots are created prior to applying.")
                        .css_classes(["dim-label"])
                        .halign(gtk4::Align::Start)
                        .build();
                    text_box.append(&sub_lbl);
                    summary_card.append(&text_box);

                    let update_all_btn = gtk4::Button::builder()
                        .label("Update All")
                        .css_classes(["suggested-action", "pill"])
                        .valign(gtk4::Align::Center)
                        .margin_end(16)
                        .build();

                    let store_for_all = store_c.clone();
                    let on_tx_for_all = on_tx_c.clone();
                    let pkgs_for_all = pkgs.clone();
                    update_all_btn.connect_clicked(move |_| {
                        for p in &pkgs_for_all {
                            let rx = store_for_all.install(p.id.clone());
                            on_tx_for_all(rx);
                        }
                    });

                    summary_card.append(&update_all_btn);
                    dyn_box_c.append(&summary_card);

                    // Package Updates Group
                    let pkg_group = adw::PreferencesGroup::builder()
                        .title("Application Updates")
                        .description("Software packages with newer versions available")
                        .build();

                    let list_box = gtk4::ListBox::builder()
                        .selection_mode(gtk4::SelectionMode::None)
                        .css_classes(["boxed-list"])
                        .build();

                    for p in &pkgs {
                        let row = create_package_row(
                            p,
                            &store_c,
                            on_sel_c.clone(),
                            on_tx_c.clone(),
                        );
                        list_box.append(&row);
                    }

                    pkg_group.add(&list_box);
                    dyn_box_c.append(&pkg_group);
                }
            });
        })
    };

    *load_ref.borrow_mut() = Some(load_updates.clone());
    load_updates();

    scrolled.upcast()
}

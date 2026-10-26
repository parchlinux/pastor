use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::glib;
use pastor_core::{Package, PackageIcon, PackageSource, PackageState};
use pastor_store::Store;

use super::package_row::create_package_row;
use crate::models::TxMonitor;

/// Which top-level source group an update belongs to, in display order.
fn update_group(source: &PackageSource) -> &'static str {
    let grp = match source {
        PackageSource::Parch(_) | PackageSource::Arch(_) => "System Packages (Native)",
        PackageSource::Flatpak { .. } => "Flatpak Applications",
        PackageSource::Aur => "AUR Community Packages",
        PackageSource::Bootc => "System Image",
        PackageSource::Waydroid => "Android Apps",
    };
    pastor_core::tr_static(grp)
}

pub struct UpdatesViewHandle {
    pub widget: gtk4::Widget,
    pub refresh: Rc<dyn Fn()>,
}

pub fn create_updates_view(
    store: Store,
    monitor: TxMonitor,
    on_select: impl Fn(Package) + 'static + Clone,
    on_transaction_start: impl Fn(tokio::sync::mpsc::Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
) -> UpdatesViewHandle {
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .hexpand(true)
        .build();

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(16)
        .margin_top(16)
        .margin_bottom(24)
        .margin_start(16)
        .margin_end(16)
        .vexpand(true)
        .hexpand(true)
        .build();

    let load_ref: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));

    let error_banner = adw::Banner::builder()
        .title(pastor_core::tr("Could not check for updates. Check your connection and try again."))
        .button_label(pastor_core::tr("Retry"))
        .revealed(false)
        .build();
    {
        let retrig = load_ref.clone();
        error_banner.connect_button_clicked(move |_| {
            if let Some(f) = retrig.borrow().as_ref() {
                f();
            }
        });
    }

    let dynamic_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(16)
        .vexpand(true)
        .hexpand(true)
        .build();

    content_box.append(&error_banner);
    content_box.append(&dynamic_box);
    scrolled.set_child(Some(&content_box));

    let load_updates = {
        let store = store.clone();
        let dynamic_box = dynamic_box.clone();
        let error_banner = error_banner.clone();
        let on_select = on_select.clone();
        let on_transaction_start = on_transaction_start.clone();
        let load_ref_clone = load_ref.clone();

        Rc::new(move || {
            // Clear current view
            while let Some(child) = dynamic_box.first_child() {
                dynamic_box.remove(&child);
            }
            error_banner.set_revealed(false);

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
                .label(pastor_core::tr("Checking for application and system updates…"))
                .css_classes(["dim-label"])
                .build();
            loading_box.append(&lbl);

            dynamic_box.append(&loading_box);

            let store_c = store.clone();
            let dyn_box_c = dynamic_box.clone();
            let error_banner_c = error_banner.clone();
            let on_sel_c = on_select.clone();
            let on_tx_c = on_transaction_start.clone();
            let monitor_c = monitor.clone();
            let load_ref_inner = load_ref_clone.clone();

            glib::spawn_future_local(async move {
                match store_c.get_updates().await {
                    Err(_) => {
                        while let Some(child) = dyn_box_c.first_child() {
                            dyn_box_c.remove(&child);
                        }
                        let status_page = adw::StatusPage::builder()
                            .icon_name("network-error-symbolic")
                            .title(pastor_core::tr("Error checking for updates"))
                            .description(pastor_core::tr("Could not check for updates. Check your connection and try again."))
                            .build();
                        dyn_box_c.append(&status_page);
                        error_banner_c.set_revealed(true);
                    }
                    Ok(updates) => {
                        error_banner_c.set_revealed(false);
                        render_updates(
                            store_c,
                            monitor_c,
                            on_sel_c,
                            on_tx_c,
                            dyn_box_c,
                            load_ref_inner,
                            updates,
                        );
                    }
                }
            });
        })
    };

    *load_ref.borrow_mut() = Some(load_updates.clone());
    load_updates();

    UpdatesViewHandle {
        widget: scrolled.upcast(),
        refresh: load_updates,
    }
}

#[allow(clippy::too_many_arguments)]
fn render_updates(
    store: Store,
    monitor: TxMonitor,
    on_select: impl Fn(Package) + 'static + Clone,
    on_transaction_start: impl Fn(tokio::sync::mpsc::Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
    dyn_box: gtk4::Box,
    load_ref: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
    updates: Vec<pastor_core::PackageUpdate>,
) {
    let store_c = store.clone();
    let on_sel_c = on_select.clone();
    let on_tx_c = on_transaction_start.clone();
    let load_ref_inner = load_ref.clone();

    glib::spawn_future_local(async move {
        if updates.is_empty() {
            let status_page = adw::StatusPage::builder()
                .icon_name("emblem-ok-symbolic")
                .title(pastor_core::tr("All packages are up to date"))
                .description(pastor_core::tr("Your system software is current"))
                .vexpand(true)
                .hexpand(true)
                .build();

            let actions_box = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Horizontal)
                .spacing(12)
                .halign(gtk4::Align::Center)
                .margin_top(16)
                .build();

            let check_again_btn = gtk4::Button::builder()
                .label(pastor_core::tr("Check for Updates"))
                .css_classes(["pill"])
                .build();

            let retrigger = load_ref_inner.clone();
            check_again_btn.connect_clicked(move |_| {
                if let Some(f) = retrigger.borrow().as_ref() {
                    f();
                }
            });

            let refresh_db_btn = gtk4::Button::builder()
                .label(pastor_core::tr("Refresh Databases"))
                .css_classes(["pill", "suggested-action"])
                .build();

            let store_refresh = store_c.clone();
            let on_tx_refresh = on_tx_c.clone();
            let retrigger_refresh = load_ref_inner.clone();
            refresh_db_btn.connect_clicked(move |_| {
                let mut rx = store_refresh.refresh_databases();
                let (tx_forward, rx_forward) = tokio::sync::mpsc::channel(100);
                on_tx_refresh(rx_forward);
                let retrig = retrigger_refresh.clone();
                glib::spawn_future_local(async move {
                    while let Some(evt) = rx.recv().await {
                        let is_done = matches!(evt.step, pastor_core::TransactionStep::Completed);
                        let _ = tx_forward.send(evt).await;
                        if is_done {
                            if let Some(f) = retrig.borrow().as_ref() {
                                f();
                            }
                            break;
                        }
                    }
                });
            });

            actions_box.append(&check_again_btn);
            actions_box.append(&refresh_db_btn);
            status_page.set_child(Some(&actions_box));

            while let Some(child) = dyn_box.first_child() {
                dyn_box.remove(&child);
            }
            dyn_box.append(&status_page);
            return;
        }

        let fetch_futures = updates.into_iter().map(|up| {
            let s = store_c.clone();
            async move {
                if let Ok(Some(mut p)) = s.get_package(&up.id).await {
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
                }
            }
        });

        let pkgs = futures::future::join_all(fetch_futures).await;

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
        let heading_text = if pastor_core::is_persian() {
            if count == 1 {
                "۱ به‌روزرسانی موجود است".to_string()
            } else {
                format!("{count} به‌روزرسانی موجود است")
            }
        } else if count == 1 {
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
            .label(pastor_core::tr("Updates are installed safely. Snapper snapshots are created prior to applying."))
            .css_classes(["dim-label"])
            .halign(gtk4::Align::Start)
            .build();
        text_box.append(&sub_lbl);
        summary_card.append(&text_box);

        let card_actions = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .valign(gtk4::Align::Center)
            .margin_end(16)
            .build();

        let refresh_tip = pastor_core::tr("Refresh Databases");
        let refresh_btn = gtk4::Button::builder()
            .icon_name("view-refresh-symbolic")
            .tooltip_text(refresh_tip)
            .css_classes(["flat", "circular"])
            .build();
        refresh_btn.update_property(&[gtk4::accessible::Property::Label(refresh_tip)]);

        let store_refresh2 = store_c.clone();
        let on_tx_refresh2 = on_tx_c.clone();
        let retrigger_refresh2 = load_ref_inner.clone();
        refresh_btn.connect_clicked(move |_| {
            let mut rx = store_refresh2.refresh_databases();
            let (tx_forward, rx_forward) = tokio::sync::mpsc::channel(100);
            on_tx_refresh2(rx_forward);
            let retrig = retrigger_refresh2.clone();
            glib::spawn_future_local(async move {
                // A transaction can emit many intermediate Completed events (e.g.
                // batched update_all). Only end when the stream closes, then
                // refresh the list if a terminal step was seen.
                let mut saw_terminal = false;
                while let Some(evt) = rx.recv().await {
                    if matches!(
                        evt.step,
                        pastor_core::TransactionStep::Completed
                            | pastor_core::TransactionStep::Cancelled
                            | pastor_core::TransactionStep::Failed(_)
                    ) {
                        saw_terminal = true;
                    }
                    let _ = tx_forward.send(evt).await;
                }
                if saw_terminal {
                    if let Some(f) = retrig.borrow().as_ref() {
                        f();
                    }
                }
            });
        });

        let total_download_size: u64 = pkgs.iter().filter_map(|p| p.size_download).sum();
        let update_all_label = if pastor_core::is_persian() {
            if total_download_size > 0 {
                format!("به‌روزرسانی همه ({})", pastor_core::format_size(total_download_size))
            } else {
                "به‌روزرسانی همه".to_string()
            }
        } else if total_download_size > 0 {
            format!("Update All ({})", pastor_core::format_size(total_download_size))
        } else {
            "Update All".to_string()
        };

        let update_all_btn = gtk4::Button::builder()
            .label(&update_all_label)
            .css_classes(["suggested-action", "pill"])
            .build();

        let store_for_all = store_c.clone();
        let on_tx_for_all = on_tx_c.clone();
        let retrigger_for_all = load_ref_inner.clone();
        update_all_btn.connect_clicked(move |_| {
            let mut rx = store_for_all.update_all();
            let (tx_forward, rx_forward) = tokio::sync::mpsc::channel(100);
            on_tx_for_all(rx_forward);
            let retrig = retrigger_for_all.clone();
            glib::spawn_future_local(async move {
                // Batched update_all emits intermediate Completed events per
                // package; only end when the stream closes.
                let mut saw_terminal = false;
                while let Some(evt) = rx.recv().await {
                    if matches!(
                        evt.step,
                        pastor_core::TransactionStep::Completed
                            | pastor_core::TransactionStep::Cancelled
                            | pastor_core::TransactionStep::Failed(_)
                    ) {
                        saw_terminal = true;
                    }
                    let _ = tx_forward.send(evt).await;
                }
                if saw_terminal {
                    if let Some(f) = retrig.borrow().as_ref() {
                        f();
                    }
                }
            });
        });

        card_actions.append(&refresh_btn);
        card_actions.append(&update_all_btn);
        summary_card.append(&card_actions);
        while let Some(child) = dyn_box.first_child() {
            dyn_box.remove(&child);
        }
        dyn_box.append(&summary_card);

        // Package Updates Grouped by Source
        let mut groups: Vec<(&'static str, Vec<&Package>)> = Vec::new();
        for p in &pkgs {
            let group = update_group(&p.id.source);
            match groups.iter_mut().find(|(g, _)| *g == group) {
                Some((_, list)) => list.push(p),
                None => groups.push((group, vec![p])),
            }
        }

        for (group_title, group_pkgs) in &groups {
            let group_size: u64 = group_pkgs.iter().filter_map(|p| p.size_download).sum();
            let group_desc = if group_size > 0 {
                format!(
                    "{} update{} · {} to download",
                    group_pkgs.len(),
                    if group_pkgs.len() == 1 { "" } else { "s" },
                    pastor_core::format_size(group_size)
                )
            } else {
                format!(
                    "{} update{}",
                    group_pkgs.len(),
                    if group_pkgs.len() == 1 { "" } else { "s" }
                )
            };

            let pkg_group = adw::PreferencesGroup::builder()
                .title(*group_title)
                .description(&group_desc)
                .build();

            let list_box = gtk4::ListBox::builder()
                .selection_mode(gtk4::SelectionMode::None)
                .css_classes(["boxed-list"])
                .build();

            for p in group_pkgs.iter() {
                let row = create_package_row(p, &store_c, &monitor, on_sel_c.clone(), on_tx_c.clone());
                list_box.append(&row);
            }

            pkg_group.add(&list_box);
            dyn_box.append(&pkg_group);
        }
    });
}

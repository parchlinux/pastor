use adw::prelude::*;
use gtk4::pango;
use pastor_core::{Package, PackageIcon, PackageState};
use pastor_store::{ActiveTransactionEvent, Store};

pub fn create_package_row(
    pkg: &Package,
    store: &Store,
    on_select: impl Fn(Package) + 'static + Clone,
    on_transaction_start: impl Fn(tokio::sync::mpsc::Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::builder()
        .activatable(true)
        .build();

    let main_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(12)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(14)
        .margin_end(14)
        .build();

    // Clickable container covering the icon, title, summary, and badges
    let clickable_area = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(14)
        .hexpand(true)
        .valign(gtk4::Align::Center)
        .build();

    if let Some(_display) = gtk4::gdk::Display::default() {
        if let Some(cursor) = gtk4::gdk::Cursor::from_name("pointer", None) {
            clickable_area.set_cursor(Some(&cursor));
            row.set_cursor(Some(&cursor));
        }
    }

    // Icon
    let icon = match &pkg.icon {
        Some(PackageIcon::LocalPath(path)) => {
            let img = gtk4::Image::from_file(path);
            img.set_pixel_size(44);
            img.set_valign(gtk4::Align::Center);
            img
        }
        Some(PackageIcon::Themed(name)) => gtk4::Image::builder()
            .icon_name(name)
            .pixel_size(44)
            .valign(gtk4::Align::Center)
            .build(),
        _ => gtk4::Image::builder()
            .icon_name("application-x-executable")
            .pixel_size(44)
            .valign(gtk4::Align::Center)
            .build(),
    };
    clickable_area.append(&icon);

    // Text box (Title, summary, version)
    let text_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(4)
        .hexpand(true)
        .valign(gtk4::Align::Center)
        .build();

    let title_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(8)
        .build();

    let title_label = gtk4::Label::builder()
        .label(pkg.display_title())
        .css_classes(["heading"])
        .halign(gtk4::Align::Start)
        .build();
    title_box.append(&title_label);

    if pkg.state == PackageState::UpdateAvailable {
        let update_badge = gtk4::Label::builder()
            .label("Update Available")
            .css_classes(["parch-badge-update"])
            .valign(gtk4::Align::Center)
            .build();
        title_box.append(&update_badge);
    }

    let version_label = gtk4::Label::builder()
        .label(&pkg.version)
        .css_classes(["dim-label"])
        .valign(gtk4::Align::Center)
        .build();
    title_box.append(&version_label);

    text_box.append(&title_box);

    let summary_label = gtk4::Label::builder()
        .label(&pkg.summary)
        .css_classes(["dim-label"])
        .halign(gtk4::Align::Start)
        .ellipsize(pango::EllipsizeMode::End)
        .build();
    text_box.append(&summary_label);

    clickable_area.append(&text_box);

    // Attach GestureClick so clicking on icon, title, or summary opens details immediately
    let click_gesture = gtk4::GestureClick::new();
    let on_sel_gesture = on_select.clone();
    let pkg_gesture = pkg.clone();
    click_gesture.connect_released(move |_, _, _, _| {
        on_sel_gesture(pkg_gesture.clone());
    });
    clickable_area.add_controller(click_gesture);

    main_box.append(&clickable_area);

    // Action button (Install / Installed / Update) with circular progress support
    let action_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(6)
        .valign(gtk4::Align::Center)
        .build();

    let action_btn = gtk4::Button::builder()
        .valign(gtk4::Align::Center)
        .build();

    let quick_uninstall_btn = gtk4::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("Quick uninstall software")
        .css_classes(["destructive-action", "flat", "circular"])
        .valign(gtk4::Align::Center)
        .visible(pkg.state == PackageState::UpdateAvailable)
        .build();

    match pkg.state {
        PackageState::NotInstalled => {
            action_btn.set_label("Install");
            action_btn.add_css_class("suggested-action");
            action_btn.add_css_class("pill");
        }
        PackageState::Installed => {
            action_btn.set_label("Uninstall");
            action_btn.set_icon_name("user-trash-symbolic");
            action_btn.set_tooltip_text(Some("Quick uninstall software"));
            action_btn.add_css_class("destructive-action");
            action_btn.add_css_class("pill");
            action_btn.set_sensitive(true);
        }
        PackageState::UpdateAvailable => {
            let icon_name = crate::icons::update_icon();
            action_btn.set_label("Update");
            action_btn.set_icon_name(&icon_name);
            action_btn.add_css_class("suggested-action");
            action_btn.add_css_class("pill");
        }
        PackageState::Processing => {
            action_btn.set_label("Working...");
            action_btn.set_sensitive(false);
        }
    }

    let progress_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(6)
        .css_classes(["transaction-pill"])
        .valign(gtk4::Align::Center)
        .visible(false)
        .build();

    let circle = super::circular_progress::CircularProgress::new(20);
    let pct_label = gtk4::Label::builder()
        .label("0%")
        .css_classes(["caption"])
        .valign(gtk4::Align::Center)
        .build();

    let cancel_btn = gtk4::Button::builder()
        .icon_name("process-stop-symbolic")
        .tooltip_text("Cancel installation")
        .css_classes(["flat", "circular"])
        .build();

    let store_c = store.clone();
    let pkg_c = pkg.id.clone();
    cancel_btn.connect_clicked(move |_| {
        let s = store_c.clone();
        let p = pkg_c.clone();
        gtk4::glib::spawn_future_local(async move {
            let _ = s.cancel_installation(&p).await;
        });
    });

    progress_box.append(circle.widget());
    progress_box.append(&pct_label);
    progress_box.append(&cancel_btn);

    // Check if this package is already being installed to restore persistent state
    if let Some(active_info) = store.get_active_transaction(&pkg.id) {
        action_btn.set_visible(false);
        quick_uninstall_btn.set_visible(false);
        progress_box.set_visible(true);
        circle.set_fraction(active_info.progress_fraction as f64);
        let pct_num = (active_info.progress_fraction * 100.0) as u32;
        pct_label.set_text(&format!("{}%", pct_num));
    }

    // Subscribe to global store transaction events
    let mut tx_sub = store.subscribe_transactions();
    let circle_sub = circle.clone();
    let pct_sub = pct_label.clone();
    let pbox_sub = progress_box.clone();
    let abtn_sub = action_btn.clone();
    let quninst_sub = quick_uninstall_btn.clone();
    let target_pkg_id = pkg.id.clone();
    let target_clean_name = target_pkg_id.name.strip_suffix(".desktop").unwrap_or(&target_pkg_id.name).to_string();
    let store_sub_c = store.clone();
    let orig_is_update = pkg.state == PackageState::UpdateAvailable;

    gtk4::glib::spawn_future_local(async move {
        while let Ok(event) = tx_sub.recv().await {
            match event {
                ActiveTransactionEvent::Started(info) | ActiveTransactionEvent::Progress(info) => {
                    let info_clean = info.package_id.name.strip_suffix(".desktop").unwrap_or(&info.package_id.name);
                    if info.package_id == target_pkg_id || info_clean == target_clean_name {
                        abtn_sub.set_visible(false);
                        quninst_sub.set_visible(false);
                        pbox_sub.set_visible(true);
                        circle_sub.set_fraction(info.progress_fraction as f64);
                        let pct_num = (info.progress_fraction * 100.0) as u32;
                        pct_sub.set_text(&format!("{}%", pct_num));
                    }
                }
                ActiveTransactionEvent::Completed(id) => {
                    let id_clean = id.name.strip_suffix(".desktop").unwrap_or(&id.name);
                    if id == target_pkg_id || id_clean == target_clean_name {
                        circle_sub.set_fraction(1.0);
                        pct_sub.set_text("100%");
                        let pbox = pbox_sub.clone();
                        let abtn = abtn_sub.clone();
                        let quninst = quninst_sub.clone();
                        let s_chk = store_sub_c.clone();
                        let pid = target_pkg_id.clone();
                        gtk4::glib::spawn_future_local(async move {
                            let updated_pkg = s_chk.get_package(&pid).await.ok().flatten();
                            let is_installed = updated_pkg.map(|p| p.is_installed()).unwrap_or(false);
                            gtk4::glib::timeout_add_local_once(
                                std::time::Duration::from_millis(400),
                                move || {
                                    pbox.set_visible(false);
                                    quninst.set_visible(false);
                                    if is_installed {
                                        abtn.set_label("Uninstall");
                                        abtn.set_icon_name("user-trash-symbolic");
                                        abtn.set_tooltip_text(Some("Quick uninstall software"));
                                        abtn.remove_css_class("suggested-action");
                                        abtn.add_css_class("destructive-action");
                                        abtn.add_css_class("pill");
                                        abtn.set_sensitive(true);
                                        abtn.set_visible(true);
                                    } else {
                                        abtn.set_label("Install");
                                        abtn.set_icon_name("folder-download-symbolic");
                                        abtn.set_tooltip_text(None);
                                        abtn.remove_css_class("destructive-action");
                                        abtn.add_css_class("suggested-action");
                                        abtn.add_css_class("pill");
                                        abtn.set_sensitive(true);
                                        abtn.set_visible(true);
                                    }
                                },
                            );
                        });
                    }
                }
                ActiveTransactionEvent::Failed(id, _) | ActiveTransactionEvent::Cancelled(id) => {
                    let id_clean = id.name.strip_suffix(".desktop").unwrap_or(&id.name);
                    if id == target_pkg_id || id_clean == target_clean_name {
                        circle_sub.set_fraction(0.0);
                        pbox_sub.set_visible(false);
                        abtn_sub.set_visible(true);
                        if orig_is_update {
                            quninst_sub.set_visible(true);
                        }
                    }
                }
            }
        }
    });

    let pkg_for_btn = pkg.clone();
    let store_for_btn = store.clone();
    let on_tx_start = on_transaction_start.clone();
    let action_btn_clone = action_btn.clone();
    let quick_uninst_on_act = quick_uninstall_btn.clone();
    let progress_box_clone = progress_box.clone();

    action_btn.connect_clicked(move |_| {
        match pkg_for_btn.state {
            PackageState::NotInstalled | PackageState::UpdateAvailable => {
                action_btn_clone.set_visible(false);
                quick_uninst_on_act.set_visible(false);
                progress_box_clone.set_visible(true);
                let rx = store_for_btn.install(pkg_for_btn.id.clone());
                on_tx_start(rx);
            }
            PackageState::Installed => {
                action_btn_clone.set_visible(false);
                quick_uninst_on_act.set_visible(false);
                progress_box_clone.set_visible(true);
                let rx = store_for_btn.remove(pkg_for_btn.id.clone());
                on_tx_start(rx);
            }
            PackageState::Processing => {}
        }
    });

    let store_for_uninst = store.clone();
    let pkg_for_uninst = pkg.id.clone();
    let on_tx_uninst = on_transaction_start.clone();
    let action_btn_uninst = action_btn.clone();
    let quick_uninst_clk = quick_uninstall_btn.clone();
    let progress_box_uninst = progress_box.clone();

    quick_uninstall_btn.connect_clicked(move |_| {
        action_btn_uninst.set_visible(false);
        quick_uninst_clk.set_visible(false);
        progress_box_uninst.set_visible(true);
        let rx = store_for_uninst.remove(pkg_for_uninst.clone());
        on_tx_uninst(rx);
    });

    action_box.append(&action_btn);
    action_box.append(&quick_uninstall_btn);
    action_box.append(&progress_box);
    main_box.append(&action_box);

    // Chevron Details Button
    let details_btn = gtk4::Button::builder()
        .icon_name("go-next-symbolic")
        .css_classes(["flat", "circular"])
        .tooltip_text("Open application details page")
        .valign(gtk4::Align::Center)
        .build();

    let on_sel_btn = on_select.clone();
    let pkg_btn = pkg.clone();
    details_btn.connect_clicked(move |_| {
        on_sel_btn(pkg_btn.clone());
    });
    main_box.append(&details_btn);

    row.set_child(Some(&main_box));

    // Also support keyboard activation (Enter / Space)
    let pkg_clone = pkg.clone();
    let on_sel_row = on_select.clone();
    row.connect_activate(move |_| {
        on_sel_row(pkg_clone.clone());
    });

    row
}

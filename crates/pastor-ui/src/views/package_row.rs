use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::glib;
use gtk4::pango;
use pastor_core::{Package, PackageIcon, PackageState};
use pastor_store::Store;
use tokio::sync::mpsc::Receiver;

use crate::models::{TxMonitor, TxObject, TxPhase};

/// Event-key fragment match: the monitor key is `source:name`; a row matches when
/// the full key equals its package key, or the trailing name matches the cleaned
/// package name (covers `.desktop` suffix and cross-source id variants).
fn key_matches(evt_key: &str, pkg_key: &str, clean_name: &str) -> bool {
    if evt_key == pkg_key {
        return true;
    }
    let evt_name = evt_key.rsplit(':').next().unwrap_or(evt_key);
    evt_name.eq_ignore_ascii_case(clean_name)
}

pub fn create_pkg_icon(pkg: &Package, size: i32) -> gtk4::Image {
    match &pkg.icon {
        Some(PackageIcon::LocalPath(path)) => {
            let img = gtk4::Image::from_file(path);
            img.set_pixel_size(size);
            img.set_valign(gtk4::Align::Center);
            img
        }
        Some(PackageIcon::Themed(name)) => gtk4::Image::builder()
            .icon_name(name)
            .pixel_size(size)
            .valign(gtk4::Align::Center)
            .build(),
        _ => gtk4::Image::builder()
            .icon_name("application-x-executable")
            .pixel_size(size)
            .valign(gtk4::Align::Center)
            .build(),
    }
}

fn source_badge(source: &pastor_core::PackageSource) -> (&'static str, &'static str) {
    match source {
        pastor_core::PackageSource::Parch(pastor_core::ParchRepoType::World) => ("Parch", "parch-badge-world"),
        pastor_core::PackageSource::Parch(pastor_core::ParchRepoType::Void) => ("Void", "parch-badge-void"),
        pastor_core::PackageSource::Arch(_) => ("Arch", "parch-badge-arch"),
        pastor_core::PackageSource::Aur => ("AUR", "parch-badge-aur"),
        pastor_core::PackageSource::Flatpak { .. } => ("Flatpak", "parch-badge-flatpak"),
        pastor_core::PackageSource::Bootc => ("bootc", "parch-badge-bootc"),
        pastor_core::PackageSource::Waydroid => ("Waydroid", "parch-badge-waydroid"),
    }
}

fn version_string(pkg: &Package) -> String {
    if pkg.state == PackageState::UpdateAvailable {
        if let Some(ref cur) = pkg.installed_version {
            if let Some(dl_sz) = pkg.size_download {
                return format!("{} → {} · {}", cur, pkg.version, pastor_core::format_size(dl_sz));
            }
            return format!("{} → {}", cur, pkg.version);
        }
    }
    format!("v{}", pkg.version)
}

fn apply_action_state(abtn: &gtk4::Button, qu: &gtk4::Button, state: PackageState) {
    abtn.remove_css_class("suggested-action");
    abtn.remove_css_class("destructive-action");
    abtn.remove_css_class("pill");
    match state {
        PackageState::Installed | PackageState::UpdateAvailable => {
            abtn.set_label("Uninstall");
            abtn.set_icon_name("user-trash-symbolic");
            abtn.set_tooltip_text(Some("Quick uninstall software"));
            abtn.add_css_class("destructive-action");
            abtn.add_css_class("pill");
            abtn.set_sensitive(true);
            qu.set_visible(state == PackageState::UpdateAvailable);
        }
        _ => {
            abtn.set_label("Install");
            abtn.set_icon_name("folder-download-symbolic");
            abtn.set_tooltip_text(None);
            abtn.add_css_class("suggested-action");
            abtn.add_css_class("pill");
            abtn.set_sensitive(true);
            qu.set_visible(false);
        }
    }
}

/// Self-contained package row body, reusable in both `gtk4::ListBox` rows and
/// `gtk4::ListView` factories. All transaction updates arrive via the shared
/// `TxMonitor` (single broadcast subscriber, lag-safe). `bind` connects the
/// monitor signals, `unbind` drops them — call `unbind` on widget destruction
/// or list-item recycling so neither side outlives the other.
pub struct PackageRowWidgets {
    root: gtk4::Box,
    action_btn: gtk4::Button,
    quick_uninstall_btn: gtk4::Button,
    cancel_box: gtk4::Box,
    /// Spinner + percentage label shown INSIDE the action button while a
    /// transaction is running (install/remove/update all show live %).
    btn_status: gtk4::Box,
    btn_pct: gtk4::Label,

    pkg: Package,
    store: Store,
    monitor: TxMonitor,
    activate: Rc<dyn Fn()>,

    monitor_handlers: Rc<RefCell<Vec<glib::SignalHandlerId>>>,
    bound_tx: Rc<RefCell<Option<(TxObject, glib::SignalHandlerId)>>>,
}

impl PackageRowWidgets {
    pub fn build(
        pkg: &Package,
        store: &Store,
        monitor: &TxMonitor,
        on_select: impl Fn(Package) + 'static + Clone,
        on_transaction_start: impl Fn(Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
    ) -> Self {
        let main_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(12)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(14)
            .margin_end(14)
            .build();

        let clickable_area = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(14)
            .hexpand(true)
            .valign(gtk4::Align::Center)
            .build();

        if let Some(cursor) = gtk4::gdk::Cursor::from_name("pointer", None) {
            clickable_area.set_cursor(Some(&cursor));
        }

        let icon = create_pkg_icon(pkg, crate::icons::ICON_SIZE_MD);
        clickable_area.append(&icon);

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

        let (badge_text, badge_class) = source_badge(&pkg.id.source);
        let source_badge_lbl = gtk4::Label::builder()
            .label(badge_text)
            .css_classes([badge_class])
            .valign(gtk4::Align::Center)
            .build();
        title_box.append(&source_badge_lbl);

        if pkg.state == PackageState::UpdateAvailable {
            let update_badge = gtk4::Label::builder()
                .label("Update Available")
                .css_classes(["parch-badge-update"])
                .valign(gtk4::Align::Center)
                .build();
            title_box.append(&update_badge);
        }

        let version_label = gtk4::Label::builder()
            .label(&version_string(pkg))
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

        {
            let click_gesture = gtk4::GestureClick::new();
            let on_sel = on_select.clone();
            let pkg_c = pkg.clone();
            click_gesture.connect_released(move |_, _, _, _| {
                on_sel(pkg_c.clone());
            });
            clickable_area.add_controller(click_gesture);
        }

        main_box.append(&clickable_area);

        // ----- Action area -----
        let action_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(6)
            .valign(gtk4::Align::Center)
            .build();

        let action_btn = gtk4::Button::builder().valign(gtk4::Align::Center).build();

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
            }
            PackageState::UpdateAvailable => {
                action_btn.set_label("Update");
                action_btn.set_icon_name(&crate::icons::update_icon());
                action_btn.add_css_class("suggested-action");
                action_btn.add_css_class("pill");
            }
            PackageState::Processing => {
                action_btn.set_label("Working...");
                action_btn.set_sensitive(false);
            }
        }

        let cancel_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(6)
            .valign(gtk4::Align::Center)
            .visible(false)
            .build();

        let cancel_btn = gtk4::Button::builder()
            .icon_name("process-stop-symbolic")
            .tooltip_text("Cancel installation")
            .css_classes(["flat", "circular"])
            .build();
        cancel_btn.update_property(&[gtk4::accessible::Property::Label("Cancel installation")]);

        {
            let store_c = store.clone();
            let pkg_id = pkg.id.clone();
            cancel_btn.connect_clicked(move |_| {
                let s = store_c.clone();
                let p = pkg_id.clone();
                glib::spawn_future_local(async move {
                    let _ = s.cancel_installation(&p).await;
                });
            });
        }

        cancel_box.append(&cancel_btn);

        let btn_status = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .halign(gtk4::Align::Center)
            .build();
        let spinner = gtk4::Spinner::builder()
            .spinning(true)
            .width_request(14)
            .height_request(14)
            .build();
        let btn_pct = gtk4::Label::builder().label("0%").build();
        btn_status.append(&spinner);
        btn_status.append(&btn_pct);

        // ----- Button actions -----
        {
            let pkg_c = pkg.clone();
            let store_c = store.clone();
            let on_tx = on_transaction_start.clone();
            let btn_c = action_btn.clone();
            let qu_c = quick_uninstall_btn.clone();
            let pbox_c = cancel_box.clone();
            let btn_status_c = btn_status.clone();

            action_btn.connect_clicked(move |_| {
                match pkg_c.state {
                    PackageState::NotInstalled | PackageState::UpdateAvailable => {
                        if matches!(pkg_c.id.source, pastor_core::PackageSource::Aur) {
                            let s_aur = store_c.clone();
                            let p_aur = pkg_c.clone();
                            let tx_start = on_tx.clone();
                            let btn = btn_c.clone();
                            let qu = qu_c.clone();
                            let pbox = pbox_c.clone();
                            let btn_status = btn_status_c.clone();
                            btn.set_sensitive(false);

                            glib::spawn_future_local(async move {
                                match s_aur.prepare_aur_scan(&p_aur.name).await {
                                    Ok((_commit, trust, scan_report, diff, missing_deps)) => {
                                        btn.set_sensitive(true);
                                        let p_name_for_cb = p_aur.name.clone();
                                        let dialog_parent = btn.clone();
                                        crate::dialogs::show_aur_review_dialog(
                                            &dialog_parent,
                                            &p_aur.name,
                                            &p_aur.version,
                                            &trust,
                                            &scan_report,
                                            &diff,
                                            &missing_deps,
                                            move |use_sandbox| {
                                                s_aur.set_aur_sandbox_enabled(&p_name_for_cb, use_sandbox);
                                                set_installing_look(&btn, &btn_status, &qu, &pbox);
                                                let rx = s_aur.install(p_aur.id.clone());
                                                tx_start(rx);
                                            },
                                        );
                                    }
                                    Err(e) => {
                                        btn.set_sensitive(true);
                                        tracing::error!("Failed to prepare AUR review: {e}");
                                    }
                                }
                            });
                        } else {
                            set_installing_look(&btn_c, &btn_status_c, &qu_c, &pbox_c);
                            let rx = store_c.install(pkg_c.id.clone());
                            on_tx(rx);
                        }
                    }
                    PackageState::Installed => {
                        set_installing_look(&btn_c, &btn_status_c, &qu_c, &pbox_c);
                        let rx = store_c.remove(pkg_c.id.clone());
                        on_tx(rx);
                    }
                    PackageState::Processing => {}
                }
            });
        }

        {
            let store_u = store.clone();
            let pkg_id_u = pkg.id.clone();
            let on_tx_u = on_transaction_start.clone();
            let btn_u = action_btn.clone();
            let qu_u = quick_uninstall_btn.clone();
            let pbox_u = cancel_box.clone();
            let btn_status_u = btn_status.clone();

            quick_uninstall_btn.connect_clicked(move |_| {
                set_installing_look(&btn_u, &btn_status_u, &qu_u, &pbox_u);
                let rx = store_u.remove(pkg_id_u.clone());
                on_tx_u(rx);
            });
        }

        action_box.append(&action_btn);
        action_box.append(&quick_uninstall_btn);
        action_box.append(&cancel_box);
        main_box.append(&action_box);

        // Chevron Details Button
        let details_btn = gtk4::Button::builder()
            .icon_name("go-next-symbolic")
            .css_classes(["flat", "circular"])
            .tooltip_text("Open application details page")
            .valign(gtk4::Align::Center)
            .build();
        details_btn.update_property(&[gtk4::accessible::Property::Label("Open application details page")]);
        {
            let on_sel = on_select.clone();
            let pkg_c = pkg.clone();
            details_btn.connect_clicked(move |_| {
                on_sel(pkg_c.clone());
            });
        }
        main_box.append(&details_btn);

        let activate = {
            let on_sel = on_select.clone();
            let pkg_c = pkg.clone();
            move || on_sel(pkg_c.clone())
        };

        Self {
            root: main_box,
            action_btn,
            quick_uninstall_btn,
            cancel_box,
            btn_status,
            btn_pct,
            pkg: pkg.clone(),
            store: store.clone(),
            monitor: monitor.clone(),
            activate: Rc::new(activate),
            monitor_handlers: Rc::new(RefCell::new(Vec::new())),
            bound_tx: Rc::new(RefCell::new(None)),
        }
    }

    pub fn root(&self) -> &gtk4::Box {
        &self.root
    }

    pub fn activate(&self) {
        (self.activate)();
    }

    fn attach_tx(&self, tx: &TxObject) {
        self.detach_tx();
        let pct = self.btn_pct.clone();
        let h = tx.connect_notify_local(Some("fraction"), move |tx, _| {
            let frac = tx.property::<f32>("fraction");
            pct.set_text(&format!("{}%", (frac * 100.0) as u32));
        });
        *self.bound_tx.borrow_mut() = Some((tx.clone(), h));
        let frac = tx.property::<f32>("fraction");
        self.btn_pct.set_text(&format!("{}%", (frac * 100.0) as u32));
    }

    fn detach_tx(&self) {
        if let Some((tx, h)) = self.bound_tx.borrow_mut().take() {
            tx.disconnect(h);
        }
    }

    /// Connect to the shared TxMonitor. Call once per row lifetime (ListBox path);
    /// call in factory `bind` and pair with `unbind` for ListView recycling.
    pub fn bind(&self) {
        let pkg_key = self.pkg.id.event_key();
        let clean_name = self.pkg.id.clean_name();

        // Restore live transaction state when (re)binding a recycled row.
        if let Some(tx) = self.monitor.tx_for_package(&pkg_key, &clean_name) {
            if tx.current_phase() == TxPhase::Active {
                set_installing_look(&self.action_btn, &self.btn_status, &self.quick_uninstall_btn, &self.cancel_box);
                self.attach_tx(&tx);
            }
        }

        let started_h = self.monitor.connect_started({
            let monitor = self.monitor.clone();
            let bound_tx = self.bound_tx.clone();
            let pkg_key = pkg_key.clone();
            let clean = clean_name.clone();
            let pbox = self.cancel_box.clone();
            let abtn = self.action_btn.clone();
            let qu = self.quick_uninstall_btn.clone();
            let btn_status = self.btn_status.clone();
            let btn_pct = self.btn_pct.clone();
            move |evt_key| {
                if !key_matches(evt_key, &pkg_key, &clean) {
                    return;
                }
                if let Some(tx) = monitor.tx_for_key(evt_key) {
                    set_installing_look(&abtn, &btn_status, &qu, &pbox);
                    // Fresh notify binding for this transaction, replacing any
                    // previous one recorded in the shared slot.
                    let pct_in = btn_pct.clone();
                    let h = tx.connect_notify_local(Some("fraction"), move |tx, _| {
                        let frac = tx.property::<f32>("fraction");
                        pct_in.set_text(&format!("{}%", (frac * 100.0) as u32));
                    });
                    let old = bound_tx.borrow_mut().replace((tx.clone(), h));
                    if let Some((old_tx, old_h)) = old {
                        old_tx.disconnect(old_h);
                    }
                    let frac = tx.property::<f32>("fraction");
                    btn_pct.set_text(&format!("{}%", (frac * 100.0) as u32));
                }
            }
        });

        let terminal_h = self.monitor.connect_terminal({
            let store = self.store.clone();
            let pkg_id = self.pkg.id.clone();
            let clean = clean_name.clone();
            let bound_tx = self.bound_tx.clone();
            let pbox = self.cancel_box.clone();
            let abtn = self.action_btn.clone();
            let btn_status = self.btn_status.clone();
            let btn_pct = self.btn_pct.clone();
            let qu = self.quick_uninstall_btn.clone();
            move |evt_key, phase| {
                if !key_matches(evt_key, &pkg_id.event_key(), &clean) {
                    return;
                }
                // Detach the notify binding; the tx object may be reused later.
                if let Some((tx, h)) = bound_tx.borrow_mut().take() {
                    tx.disconnect(h);
                }
                btn_pct.set_text("100%");
                if phase != TxPhase::Active {
                    let s = store.clone();
                    let pid = pkg_id.clone();
                    let pbox = pbox.clone();
                    let abtn = abtn.clone();
                    let btn_status = btn_status.clone();
                    let qu = qu.clone();
                    glib::spawn_future_local(async move {
                        let updated_pkg = s.get_package(&pid).await.ok().flatten();
                        glib::timeout_add_local_once(
                            std::time::Duration::from_millis(400),
                            move || {
                                pbox.set_visible(false);
                                abtn.set_child(Option::<&gtk4::Widget>::None);
                                abtn.set_sensitive(true);
                                if let Some(new_pkg) = updated_pkg {
                                    apply_action_state(&abtn, &qu, new_pkg.state);
                                } else {
                                    qu.set_visible(false);
                                }
                            },
                        );
                    });
                }
            }
        });

        self.monitor_handlers.borrow_mut().push(started_h);
        self.monitor_handlers.borrow_mut().push(terminal_h);
    }

    pub fn unbind(&self) {
        for h in self.monitor_handlers.borrow_mut().drain(..) {
            self.monitor.disconnect(h);
        }
        self.detach_tx();
    }
}

/// Swap the action button into "in-progress" mode: spinner + percentage inside
/// the button (kept visible and pill-shaped), cancel button next to it.
fn set_installing_look(
    abtn: &gtk4::Button,
    btn_status: &gtk4::Box,
    qu: &gtk4::Button,
    cancel_box: &gtk4::Box,
) {
    abtn.set_sensitive(false);
    abtn.set_icon_name("");
    abtn.set_tooltip_text(None);
    abtn.remove_css_class("destructive-action");
    abtn.add_css_class("suggested-action");
    abtn.add_css_class("pill");
    abtn.set_child(Some(btn_status));
    qu.set_visible(false);
    cancel_box.set_visible(true);
}

/// ListBox-row wrapper used by explore/updates views (rows live and die with the
/// view; the handler connections are dropped when the row is destroyed).
pub fn create_package_row(
    pkg: &Package,
    store: &Store,
    monitor: &TxMonitor,
    on_select: impl Fn(Package) + 'static + Clone,
    on_transaction_start: impl Fn(Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::builder()
        .activatable(true)
        .build();

    let widgets = Rc::new(PackageRowWidgets::build(pkg, store, monitor, on_select, on_transaction_start));
    widgets.bind();

    row.set_child(Some(widgets.root()));

    {
        let widgets = widgets.clone();
        row.connect_activate(move |_| widgets.activate());
    }
    {
        let widgets = widgets.clone();
        row.connect_destroy(move |_| widgets.unbind());
    }

    row
}

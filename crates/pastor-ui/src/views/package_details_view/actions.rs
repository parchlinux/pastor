use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk4::glib;
use pastor_core::{Package, PackageSource, PackageState};
use pastor_store::Store;

use crate::models::{TxMonitor, TxPhase};
use crate::views::CircularProgress;

pub fn setup_actions(
    actions_box: &gtk4::Box,
    pkg: &Package,
    store: &Store,
    monitor: &TxMonitor,
    on_downgrade: impl Fn(String) + 'static + Clone,
    on_transaction_start: impl Fn(tokio::sync::mpsc::Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
) {
    let pkg_id = pkg.id.clone();
    let store_clone = store.clone();
    let on_tx_start = on_transaction_start.clone();
    let state = pkg.state;

    let render_actions_holder: Rc<RefCell<Option<Rc<dyn Fn(PackageState)>>>> =
        Rc::new(RefCell::new(None));

    // Live progress widgets (circle + label) and the current verb, populated by
    // render_actions and updated from the shared TxMonitor.
    let progress_ref: Rc<RefCell<Option<(CircularProgress, gtk4::Label)>>> =
        Rc::new(RefCell::new(None));
    let verb_ref: Rc<RefCell<String>> = Rc::new(RefCell::new("Installing… ".to_string()));

    let render_actions = {
        let target_box = actions_box.clone();
        let pkg = pkg.clone();
        let pkg_id = pkg_id.clone();
        let store = store_clone.clone();
        let on_tx = on_tx_start.clone();
        let on_downgrade = on_downgrade.clone();
        let progress_ref = progress_ref.clone();
        let verb_ref = verb_ref.clone();

        Rc::new(move |current_state: PackageState| {
            while let Some(child) = target_box.first_child() {
                target_box.remove(&child);
            }

            match current_state {
                PackageState::Installed => {
                    let is_runtime = pkg_id.name.starts_with("runtime/");
                    if !is_runtime {
                        let content = adw::ButtonContent::builder()
                            .label(pastor_core::tr("Open"))
                            .icon_name("media-playback-start-symbolic")
                            .build();
                        let launch_btn = gtk4::Button::builder()
                            .child(&content)
                            .css_classes(["suggested-action", "pill", "details-primary-action-btn"])
                            .build();
                        let app_name = pkg.display_title().to_string();
                        let pkg_for_run = pkg.clone();
                        launch_btn.connect_clicked(move |_| {
                            tracing::info!("Launching application: {}", app_name);
                            if let Err(e) = launch_package(&pkg_for_run) {
                                tracing::warn!("Failed to launch {}: {}", app_name, e);
                            }
                        });
                        target_box.append(&launch_btn);
                    }

                    // Progress widget for uninstallation
                    let progress_box = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Horizontal)
                        .spacing(10)
                        .css_classes(["transaction-pill"])
                        .valign(gtk4::Align::Center)
                        .visible(false)
                        .build();
                    let circle = CircularProgress::new(26);
                    let pct_label = gtk4::Label::builder()
                        .label(&format!("{} 0%", pastor_core::tr("Uninstalling…")))
                        .css_classes(["caption", "heading"])
                        .valign(gtk4::Align::Center)
                        .build();
                    let cancel_btn = gtk4::Button::builder()
                        .icon_name("process-stop-symbolic")
                        .tooltip_text(pastor_core::tr("Cancel uninstallation"))
                        .css_classes(["flat", "circular"])
                        .build();
                    let store_c = store.clone();
                    let pkg_c = pkg_id.clone();
                    cancel_btn.connect_clicked(move |_| {
                        let s = store_c.clone();
                        let p = pkg_c.clone();
                        glib::spawn_future_local(async move {
                            let _ = s.cancel_installation(&p).await;
                        });
                    });
                    progress_box.append(circle.widget());
                    progress_box.append(&pct_label);
                    progress_box.append(&cancel_btn);
                    *progress_ref.borrow_mut() = Some((circle.clone(), pct_label.clone()));
                    *verb_ref.borrow_mut() = format!("{} ", pastor_core::tr("Uninstalling…"));

                    let uninstall_btn = gtk4::Button::builder()
                        .icon_name("user-trash-symbolic")
                        .css_classes(["destructive-action", "flat", "circular"])
                        .tooltip_text(pastor_core::tr("Uninstall"))
                        .build();
                    let store_u = store.clone();
                    let pkg_u = pkg_id.clone();
                    let tx_u = on_tx.clone();
                    let pbox_u = progress_box.clone();
                    let ubtn_u = uninstall_btn.clone();
                    uninstall_btn.connect_clicked(move |_| {
                        ubtn_u.set_visible(false);
                        pbox_u.set_visible(true);
                        let rx = store_u.remove(pkg_u.clone());
                        tx_u(rx);
                    });
                    target_box.append(&uninstall_btn);
                    target_box.append(&progress_box);

                    let share_btn = super::share::create_share_button(&pkg, &store);
                    target_box.append(&share_btn);

                    if !is_runtime {
                        target_box.append(&create_more_menu_btn(&pkg, on_downgrade.clone()));
                    }
                }
                PackageState::NotInstalled | PackageState::UpdateAvailable => {
                    let is_update = matches!(current_state, PackageState::UpdateAvailable);
                    let btn_label = if is_update { pastor_core::tr("Update") } else { pastor_core::tr("Install") };
                    let btn_icon = if is_update {
                        crate::icons::update_icon()
                    } else {
                        "folder-download-symbolic".to_string()
                    };

                    let content = adw::ButtonContent::builder()
                        .label(btn_label)
                        .icon_name(&btn_icon)
                        .build();

                    let install_btn = gtk4::Button::builder()
                        .child(&content)
                        .css_classes(["suggested-action", "pill", "details-primary-action-btn"])
                        .build();

                    let progress_box = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Horizontal)
                        .spacing(10)
                        .css_classes(["transaction-pill"])
                        .valign(gtk4::Align::Center)
                        .visible(false)
                        .build();

                    let circle = CircularProgress::new(26);
                    let initial_verb = if is_update { pastor_core::tr("Updating…") } else { pastor_core::tr("Installing…") };
                    let pct_label = gtk4::Label::builder()
                        .label(&format!("{initial_verb} 0%"))
                        .css_classes(["caption", "heading"])
                        .valign(gtk4::Align::Center)
                        .build();

                    let cancel_btn = gtk4::Button::builder()
                        .icon_name("process-stop-symbolic")
                        .tooltip_text(pastor_core::tr("Cancel installation"))
                        .css_classes(["flat", "circular"])
                        .build();
                    let store_c = store.clone();
                    let pkg_c = pkg_id.clone();
                    cancel_btn.connect_clicked(move |_| {
                        let s = store_c.clone();
                        let p = pkg_c.clone();
                        glib::spawn_future_local(async move {
                            let _ = s.cancel_installation(&p).await;
                        });
                    });

                    progress_box.append(circle.widget());
                    progress_box.append(&pct_label);
                    progress_box.append(&cancel_btn);
                    *progress_ref.borrow_mut() = Some((circle.clone(), pct_label.clone()));
                    *verb_ref.borrow_mut() = format!("{initial_verb} ");

                    let store_i = store.clone();
                    let pkg_i = pkg_id.clone();
                    let tx_i = on_tx.clone();
                    let install_btn_clone = install_btn.clone();
                    let progress_box_clone = progress_box.clone();

                    let pkg_ver = pkg.version.clone();
                    install_btn.connect_clicked(move |_| {
                        if matches!(pkg_i.source, PackageSource::Aur) {
                            let s_aur = store_i.clone();
                            let p_id = pkg_i.clone();
                            let p_ver = pkg_ver.clone();
                            let tx_cb = tx_i.clone();
                            let btn_c = install_btn_clone.clone();
                            let pbox_c = progress_box_clone.clone();
                            btn_c.set_sensitive(false);
                            let btn_widget = btn_c.clone();

                            glib::spawn_future_local(async move {
                                match s_aur.prepare_aur_scan(&p_id.name).await {
                                    Ok((_commit, trust, scan_report, diff, missing_deps)) => {
                                        btn_c.set_sensitive(true);
                                        let p_id_install = p_id.clone();
                                        crate::dialogs::show_aur_review_dialog(
                                            &btn_widget,
                                            &p_id.name,
                                            &p_ver,
                                            &trust,
                                            &scan_report,
                                            &diff,
                                            &missing_deps,
                                            move |use_sandbox| {
                                                s_aur.set_aur_sandbox_enabled(&p_id_install.name, use_sandbox);
                                                btn_c.set_visible(false);
                                                pbox_c.set_visible(true);
                                                let rx_to_bar = s_aur.install(p_id_install);
                                                tx_cb(rx_to_bar);
                                            },
                                        );
                                    }
                                    Err(e) => {
                                        btn_c.set_sensitive(true);
                                        tracing::error!("Failed to prepare AUR review: {e}");
                                    }
                                }
                            });
                        } else {
                            install_btn_clone.set_visible(false);
                            progress_box_clone.set_visible(true);
                            let rx_to_bar = store_i.install(pkg_i.clone());
                            tx_i(rx_to_bar);
                        }
                    });

                    target_box.append(&install_btn);
                    target_box.append(&progress_box);

                    if is_update {
                        let uninstall_btn = gtk4::Button::builder()
                            .icon_name("user-trash-symbolic")
                            .css_classes(["destructive-action", "flat", "circular"])
                            .tooltip_text(pastor_core::tr("Uninstall"))
                            .build();
                        let store_u = store.clone();
                        let pkg_u = pkg_id.clone();
                        let tx_u = on_tx.clone();
                        let u_btn_c = uninstall_btn.clone();
                        let pbox_c = progress_box.clone();
                        uninstall_btn.connect_clicked(move |_| {
                            u_btn_c.set_visible(false);
                            pbox_c.set_visible(true);
                            let rx = store_u.remove(pkg_u.clone());
                            tx_u(rx);
                        });
                        target_box.append(&uninstall_btn);
                    }

                    let share_btn = super::share::create_share_button(&pkg, &store);
                    target_box.append(&share_btn);

                    let is_runtime = pkg.id.name.starts_with("runtime/");
                    if !is_runtime {
                        target_box.append(&create_more_menu_btn(&pkg, on_downgrade.clone()));
                    }
                }
                PackageState::Processing => {
                    let proc_btn = gtk4::Button::builder()
                        .label(pastor_core::tr("Processing…"))
                        .css_classes(["pill"])
                        .sensitive(false)
                        .build();
                    target_box.append(&proc_btn);

                    let share_btn = super::share::create_share_button(&pkg, &store);
                    target_box.append(&share_btn);

                    let is_runtime = pkg.id.name.starts_with("runtime/");
                    if !is_runtime {
                        target_box.append(&create_more_menu_btn(&pkg, on_downgrade.clone()));
                    }
                }
            }

            if let Some(_active_info) = store.get_active_transaction(&pkg_id) {
                for child in target_box.observe_children().into_iter().flatten() {
                    if let Ok(w) = child.downcast::<gtk4::Widget>() {
                        if w.has_css_class("transaction-pill") {
                            w.set_visible(true);
                        } else {
                            w.set_visible(false);
                        }
                    }
                }
            }
        })
    };

    *render_actions_holder.borrow_mut() = Some(render_actions.clone());
    render_actions(state);

    // Transaction subscription for this package details view, via the shared
    // lag-safe TxMonitor (single broadcast subscriber for the whole app).
    {
        let pkg_key = pkg_id.event_key();
        let target_clean_name = pkg_id.clean_name();
        let s_chk = store_clone.clone();
        let holder_c = render_actions_holder.clone();
        let monitor_c = monitor.clone();

        let refresh_state: Rc<dyn Fn()> = {
            let s_chk = s_chk.clone();
            let pid = pkg_id.clone();
            let holder = holder_c.clone();
            Rc::new(move || {
                let s = s_chk.clone();
                let pid = pid.clone();
                let holder = holder.clone();
                glib::spawn_future_local(async move {
                    let updated_pkg = s.get_package(&pid).await.ok().flatten();
                    let new_state = updated_pkg
                        .map(|p| p.state)
                        .unwrap_or(PackageState::NotInstalled);
                    if let Some(renderer) = holder.borrow().as_ref() {
                        renderer(new_state);
                    }
                });
            })
        };

        let progress_ref_c = progress_ref.clone();
        let verb_ref_c = verb_ref.clone();
        let key_for_progress = pkg_key.clone();
        let clean_for_progress = target_clean_name.clone();
        let monitor_for_progress = monitor_c.clone();
        let update_progress = move |evt_key: &str| {
            let matches = evt_key == key_for_progress.as_str()
                || evt_key
                    .rsplit(':')
                    .next()
                    .map(|n| n.eq_ignore_ascii_case(&clean_for_progress))
                    .unwrap_or(false);
            if !matches {
                return;
            }
            if let Some(tx) = monitor_for_progress.tx_for_key(evt_key) {
                let frac = tx.property::<f32>("fraction");
                if let Some((circle, label)) = progress_ref_c.borrow().as_ref() {
                    circle.set_fraction(frac as f64);
                    let verb = verb_ref_c.borrow().clone();
                    label.set_text(&format!("{}{}%", verb, (frac * 100.0) as u32));
                }
            }
        };
        monitor_c.connect_progress(update_progress.clone());
        monitor_c.connect_started(update_progress.clone());

        let delayed_refresh = refresh_state.clone();
        let terminal_h = monitor_c.connect_terminal(move |evt_key, phase| {
            let matches = evt_key == pkg_key.as_str()
                || evt_key
                    .rsplit(':')
                    .next()
                    .map(|n| n.eq_ignore_ascii_case(&target_clean_name))
                    .unwrap_or(false);
            if !matches || phase == TxPhase::Active {
                return;
            }
            if phase == TxPhase::Success {
                // Small delay so the store has settled before re-querying.
                let refresh = delayed_refresh.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
                    refresh();
                });
            } else {
                (refresh_state)();
            }
        });
        let th_cell = std::cell::Cell::new(Some(terminal_h));
        actions_box.connect_destroy(move |_| {
            if let Some(h) = th_cell.take() {
                monitor_c.disconnect(h);
            }
        });
    }
}

fn create_more_menu_btn(
    pkg: &Package,
    on_downgrade: impl Fn(String) + 'static + Clone,
) -> gtk4::MenuButton {
    let more_menu_btn = gtk4::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .css_classes(["flat", "circular"])
        .tooltip_text(pastor_core::tr("More Options"))
        .build();

    let popover = gtk4::Popover::new();
    let menu_vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(4)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();

    let downgrade_item = gtk4::Button::builder()
        .label(pastor_core::tr("Downgrade Version…"))
        .icon_name("edit-undo-symbolic")
        .css_classes(["flat"])
        .halign(gtk4::Align::Fill)
        .build();
    let on_dw = on_downgrade.clone();
    let name_clone = pkg.name.clone();
    let pop_clone = popover.clone();
    downgrade_item.connect_clicked(move |_| {
        pop_clone.popdown();
        on_dw(name_clone.clone());
    });
    menu_vbox.append(&downgrade_item);

    if let Some(ref url) = pkg.homepage {
        let web_item = gtk4::LinkButton::builder()
            .uri(url)
            .label(pastor_core::tr("Project Website"))
            .css_classes(["flat"])
            .halign(gtk4::Align::Fill)
            .build();
        menu_vbox.append(&web_item);
    }

    popover.set_child(Some(&menu_vbox));
    more_menu_btn.set_popover(Some(&popover));
    more_menu_btn
}

/// Resolves the desktop `AppInfo` for a package by reverse-DNS ID,
/// themed icon name, executable binary, ID suffix, or commandline fallback.
pub fn resolve_app_info(pkg: &Package) -> Option<gtk4::gio::AppInfo> {
    use gtk4::gio::prelude::*;

    let pkg_name = &pkg.id.name;
    let pkg_clean = pkg_name
        .trim_end_matches("-bin")
        .trim_end_matches("-git")
        .to_lowercase();

    // Direct desktop ID candidates
    let mut candidates = vec![
        format!("{pkg_name}.desktop"),
        format!("{pkg_clean}.desktop"),
    ];

    if let Some(ref icon) = pkg.icon {
        if let pastor_core::PackageIcon::Themed(name) = icon {
            candidates.push(format!("{name}.desktop"));
            candidates.push(name.clone());
        }
    }

    let all_apps = gtk4::gio::AppInfo::all();

    // 1. Match candidates against registered app IDs
    for c in &candidates {
        for app in &all_apps {
            if let Some(id) = app.id() {
                if id.eq_ignore_ascii_case(c) {
                    return Some(app.clone());
                }
            }
        }
    }

    // 2. Match by executable filename
    for app in &all_apps {
        let exe = app.executable();
        let exe_str = exe.to_string_lossy();
        let exe_base = exe.file_name().and_then(|n| n.to_str()).unwrap_or(&exe_str);
        if exe_base.eq_ignore_ascii_case(pkg_name) || exe_base.eq_ignore_ascii_case(&pkg_clean) {
            return Some(app.clone());
        }
    }

    // 3. Match by App ID suffix (e.g. org.kde.akregator.desktop matching akregator)
    let dot_suffix = format!(".{pkg_clean}.desktop");
    let exact_suffix = format!("{pkg_clean}.desktop");
    for app in &all_apps {
        if let Some(id) = app.id() {
            let id_lower = id.to_lowercase();
            if id_lower == exact_suffix || id_lower.ends_with(&dot_suffix) {
                return Some(app.clone());
            }
        }
    }

    // 4. Match by display name
    let display_title = pkg.display_title().to_lowercase();
    for app in &all_apps {
        let app_name = app.name().to_lowercase();
        if app_name == display_title || app_name == pkg_clean {
            return Some(app.clone());
        }
    }

    // 5. Commandline fallback if binary exists in PATH
    if let Ok(app) = gtk4::gio::AppInfo::create_from_commandline(
        pkg_name,
        None,
        gtk4::gio::AppInfoCreateFlags::NONE,
    ) {
        return Some(app);
    }

    if pkg_clean != *pkg_name {
        if let Ok(app) = gtk4::gio::AppInfo::create_from_commandline(
            &pkg_clean,
            None,
            gtk4::gio::AppInfoCreateFlags::NONE,
        ) {
            return Some(app);
        }
    }

    None
}

/// Robust desktop application launcher.
/// Resolves reverse-DNS desktop IDs (e.g. `org.kde.akregator.desktop`),
/// themed icons, binary names, and fallback commandlines via GIO.
pub fn launch_package(pkg: &Package) -> Result<(), String> {
    use gtk4::gio::prelude::*;

    if let PackageSource::Flatpak { .. } = &pkg.id.source {
        match std::process::Command::new("flatpak")
            .args(["run", &pkg.id.name])
            .spawn()
        {
            Ok(_) => return Ok(()),
            Err(e) => return Err(format!("Failed to launch flatpak {}: {}", pkg.id.name, e)),
        }
    }

    if let Some(app) = resolve_app_info(pkg) {
        if app.launch(&[], gtk4::gio::AppLaunchContext::NONE).is_ok() {
            return Ok(());
        }
    }

    let pkg_name = &pkg.id.name;
    Err(format!("Could not find desktop application or binary for {pkg_name}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pastor_core::{PackageCategory, PackageId, PackageSource, PackageState};

    #[test]
    fn test_resolve_app_info_for_system_app() {
        let pkg = Package {
            id: PackageId::new("akregator", PackageSource::Arch("extra".to_string())),
            name: "akregator".to_string(),
            display_name: Some("Akregator".to_string()),
            version: "24.08.0".to_string(),
            installed_version: Some("24.08.0".to_string()),
            summary: "News Feed Reader".to_string(),
            description: None,
            icon: Some(pastor_core::PackageIcon::Themed("org.kde.akregator".to_string())),
            screenshots: Vec::new(),
            homepage: None,
            license: None,
            maintainer: None,
            categories: vec![PackageCategory::Internet],
            size_installed: None,
            size_download: None,
            dependencies: Vec::new(),
            changelog: None,
            state: PackageState::Installed,
        };

        let resolved = resolve_app_info(&pkg);
        if let Some(app) = resolved {
            use gtk4::gio::prelude::AppInfoExt;
            assert!(
                app.id().map(|id| id.contains("akregator")).unwrap_or(false)
                    || app.executable().to_string_lossy().contains("akregator")
            );
        }
    }
}

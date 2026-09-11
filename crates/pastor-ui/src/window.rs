use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::{gio, glib, pango};
use pastor_core::{Package, PackageCategory};
use pastor_store::{ActiveTransactionEvent, Store};

use crate::{
    dialogs::show_about_dialog,
    views::{
        create_downgrade_view, create_explore_view,
        create_loading_view, create_package_details_view, create_package_list_view,
        create_settings_view, create_snapshots_view,
        create_updates_view, CircularProgress, TransactionBar,
    },
};

#[derive(Clone)]
pub struct MainWindow {
    window: adw::ApplicationWindow,
    show_updates_fn: Rc<dyn Fn()>,
}

impl MainWindow {
    pub fn new(app: &adw::Application, store: Store) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Parch Store")
            .default_width(1080)
            .default_height(720)
            .build();

        // Minimize / hide to tray on close request rather than terminating
        window.connect_close_request(|w| {
            w.set_visible(false);
            glib::Propagation::Stop
        });

        let split_view = adw::OverlaySplitView::builder()
            .min_sidebar_width(240.0)
            .max_sidebar_width(320.0)
            .sidebar_width_fraction(0.24)
            .enable_hide_gesture(true)
            .enable_show_gesture(true)
            .show_sidebar(true)
            .build();

        // -------------------------
        // SIDEBAR (Focused Navigation)
        // -------------------------
        let sidebar_toolbar = adw::ToolbarView::new();
        let sidebar_header = adw::HeaderBar::builder()
            .show_end_title_buttons(false)
            .show_start_title_buttons(false)
            .build();
        let sidebar_title = adw::WindowTitle::builder()
            .title("Parch Store")
            .build();
        sidebar_header.set_title_widget(Some(&sidebar_title));

        let sidebar_collapse_btn = gtk4::Button::builder()
            .icon_name(crate::icons::sidebar_toggle_icon())
            .css_classes(["flat"])
            .tooltip_text("Collapse Sidebar (F9)")
            .valign(gtk4::Align::Center)
            .build();
        sidebar_header.pack_end(&sidebar_collapse_btn);

        sidebar_toolbar.add_top_bar(&sidebar_header);

        let sidebar_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(12)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();

        // Search Entry
        let search_entry = gtk4::SearchEntry::builder()
            .placeholder_text("Search packages, apps, AUR… (Ctrl+K)")
            .build();
        sidebar_box.append(&search_entry);

        // Discover Group
        let nav_group = adw::PreferencesGroup::builder()
            .title("Discover")
            .build();
        let nav_list = gtk4::ListBox::builder()
            .selection_mode(gtk4::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();

        let explore_row = create_nav_row("Explore", &crate::icons::explore_icon());
        let installed_row = create_nav_row("Installed", &crate::icons::installed_icon());
        let updates_row = create_nav_row("Updates", &crate::icons::update_icon());

        nav_list.append(&explore_row);
        nav_list.append(&installed_row);
        nav_list.append(&updates_row);
        nav_group.add(&nav_list);
        sidebar_box.append(&nav_group);

        // Categories Group
        let cat_group = adw::PreferencesGroup::builder()
            .title("Categories")
            .build();
        let cat_list = gtk4::ListBox::builder()
            .selection_mode(gtk4::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();

        let active_categories: Vec<PackageCategory> = PackageCategory::all()
            .iter()
            .copied()
            .filter(|c| *c != PackageCategory::Android && *c != PackageCategory::Featured && *c != PackageCategory::ParchPicks)
            .collect();

        for cat in &active_categories {
            let row = create_nav_row(cat.title(), &crate::icons::category_icon(*cat));
            cat_list.append(&row);
        }
        cat_group.add(&cat_list);
        sidebar_box.append(&cat_group);

        let sidebar_scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .child(&sidebar_box)
            .build();

        sidebar_toolbar.set_content(Some(&sidebar_scrolled));
        split_view.set_sidebar(Some(&sidebar_toolbar));

        // -------------------------
        // CONTENT AREA
        // -------------------------
        let content_toolbar = adw::ToolbarView::new();
        let content_header = adw::HeaderBar::builder()
            .show_start_title_buttons(false)
            .build();

        // Sidebar Reveal Button on content header (visible when sidebar is collapsed)
        let reveal_sidebar_btn = gtk4::Button::builder()
            .icon_name(crate::icons::sidebar_toggle_icon())
            .css_classes(["flat"])
            .tooltip_text("Open Sidebar (F9)")
            .visible(false)
            .valign(gtk4::Align::Center)
            .build();
        content_header.pack_start(&reveal_sidebar_btn);

        // Standard GNOME HeaderBar Back Button
        let header_back_btn = gtk4::Button::builder()
            .icon_name("go-previous-symbolic")
            .css_classes(["flat"])
            .tooltip_text("Back")
            .visible(false)
            .build();
        content_header.pack_start(&header_back_btn);

        // Dynamic AdwWindowTitle for modern GNOME header
        let window_title = adw::WindowTitle::builder()
            .title("Explore")
            .subtitle("")
            .build();
        content_header.set_title_widget(Some(&window_title));

        // Connect Sidebar Buttons
        {
            let sv = split_view.clone();
            sidebar_collapse_btn.connect_clicked(move |_| {
                sv.set_show_sidebar(false);
            });
        }
        {
            let sv = split_view.clone();
            reveal_sidebar_btn.connect_clicked(move |_| {
                sv.set_show_sidebar(true);
            });
        }

        let sync_sidebar_buttons = {
            let sv = split_view.clone();
            let rev = reveal_sidebar_btn.clone();
            let col = sidebar_collapse_btn.clone();
            Rc::new(move || {
                let open = sv.shows_sidebar() && !sv.is_collapsed();
                rev.set_visible(!open);
                col.set_visible(open);
            })
        };

        {
            let ssb = sync_sidebar_buttons.clone();
            split_view.connect_show_sidebar_notify(move |_| {
                ssb();
            });
        }
        {
            let ssb = sync_sidebar_buttons.clone();
            split_view.connect_collapsed_notify(move |_| {
                ssb();
            });
        }

        // Global Keyboard Shortcuts (Ctrl+K for search, F9 / Ctrl+B for sidebar toggle)
        let key_controller = gtk4::EventControllerKey::new();
        let search_entry_focus = search_entry.clone();
        let sv_key = split_view.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, state| {
            if state.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
                && (keyval == gtk4::gdk::Key::k || keyval == gtk4::gdk::Key::K)
            {
                if !sv_key.shows_sidebar() {
                    sv_key.set_show_sidebar(true);
                }
                search_entry_focus.grab_focus();
                gtk4::glib::Propagation::Stop
            } else if keyval == gtk4::gdk::Key::F9
                || (state.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
                    && (keyval == gtk4::gdk::Key::b || keyval == gtk4::gdk::Key::B))
            {
                sv_key.set_show_sidebar(!sv_key.shows_sidebar());
                gtk4::glib::Propagation::Stop
            } else {
                gtk4::glib::Propagation::Proceed
            }
        });
        window.add_controller(key_controller);

        // Header Operations Button (Global Progress ring + Operations Popover)
        let header_ops_btn = gtk4::Button::builder()
            .tooltip_text("Active Operations")
            .css_classes(["flat", "circular"])
            .visible(false)
            .valign(gtk4::Align::Center)
            .build();

        let header_ops_circle = CircularProgress::new(22);
        header_ops_btn.set_child(Some(header_ops_circle.widget()));

        let header_ops_popover = gtk4::Popover::builder()
            .autohide(true)
            .build();
        header_ops_popover.set_parent(&header_ops_btn);

        {
            let pop = header_ops_popover.clone();
            header_ops_btn.connect_clicked(move |_| {
                if pop.is_visible() {
                    pop.popdown();
                } else {
                    pop.popup();
                }
            });
        }

        let pop_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(8)
            .margin_top(10)
            .margin_bottom(10)
            .margin_start(10)
            .margin_end(10)
            .width_request(340)
            .build();

        let pop_header = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .margin_bottom(4)
            .build();

        let pop_titles = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(2)
            .hexpand(true)
            .build();

        let pop_title_lbl = gtk4::Label::builder()
            .label("Active Operations")
            .css_classes(["heading"])
            .halign(gtk4::Align::Start)
            .build();
        pop_titles.append(&pop_title_lbl);

        let pop_sub_lbl = gtk4::Label::builder()
            .label("0 operations active")
            .css_classes(["caption", "dim-label"])
            .halign(gtk4::Align::Start)
            .build();
        pop_titles.append(&pop_sub_lbl);
        pop_header.append(&pop_titles);

        let pop_cancel_all_btn = gtk4::Button::builder()
            .icon_name("process-stop-symbolic")
            .tooltip_text("Cancel all operations")
            .css_classes(["flat", "circular"])
            .valign(gtk4::Align::Center)
            .build();
        pop_header.append(&pop_cancel_all_btn);
        pop_box.append(&pop_header);

        let pop_sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        pop_box.append(&pop_sep);

        let pop_list = gtk4::ListBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();

        let pop_scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .max_content_height(260)
            .propagate_natural_height(true)
            .child(&pop_list)
            .build();
        pop_box.append(&pop_scrolled);

        let pop_bottom_sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        pop_box.append(&pop_bottom_sep);

        let pop_console_btn = gtk4::Button::builder()
            .icon_name("utilities-terminal-symbolic")
            .label("Native Transaction Console")
            .css_classes(["flat"])
            .halign(gtk4::Align::Center)
            .build();
        pop_box.append(&pop_console_btn);

        header_ops_popover.set_child(Some(&pop_box));
        content_header.pack_end(&header_ops_btn);

        // Hamburger Menu in Header (System Tools & Preferences)
        let tools_menu_btn = gtk4::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Menu and Preferences")
            .build();

        let menu_model = gio::Menu::new();
        menu_model.append(Some("Settings"), Some("app.settings"));
        menu_model.append(Some("Snapper Snapshots"), Some("app.snapshots"));
        menu_model.append(Some("Downgrade Tool"), Some("app.downgrade"));
        menu_model.append(Some("Parch Mirror Manager"), Some("app.mirrors"));
        menu_model.append(Some("About Parch Store"), Some("app.about"));
        tools_menu_btn.set_menu_model(Some(&menu_model));
        content_header.pack_end(&tools_menu_btn);

        content_toolbar.add_top_bar(&content_header);

        let content_container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .build();

        let content_stack = gtk4::Stack::builder()
            .transition_type(gtk4::StackTransitionType::Crossfade)
            .vexpand(true)
            .build();

        let transaction_bar = TransactionBar::new();

        content_container.append(&content_stack);
        content_container.append(transaction_bar.widget());
        content_toolbar.set_content(Some(&content_container));

        split_view.set_content(Some(&content_toolbar));
        window.set_content(Some(&split_view));

        // Shared Navigation Controller State
        let store_rc = store.clone();
        let content_stack_rc = content_stack.clone();
        let tx_bar_rc = transaction_bar.clone();

        let current_page_name = Rc::new(RefCell::new("explore".to_string()));
        let nav_history: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

        let active_section_title = Rc::new(RefCell::new("Explore".to_string()));
        let active_section_sub = Rc::new(RefCell::new("".to_string()));

        let sync_title = {
            let sv = split_view.clone();
            let wt = window_title.clone();
            let cur_t = active_section_title.clone();
            let cur_s = active_section_sub.clone();
            Rc::new(move || {
                let sidebar_visible = sv.shows_sidebar() && !sv.is_collapsed();
                let title = cur_t.borrow().clone();
                let sub = cur_s.borrow().clone();
                if sidebar_visible {
                    wt.set_title(&title);
                    wt.set_subtitle(&sub);
                } else {
                    wt.set_title("Parch Store");
                    if sub.is_empty() {
                        wt.set_subtitle(&title);
                    } else {
                        wt.set_subtitle(&format!("{} — {}", title, sub));
                    }
                }
            })
        };

        {
            let sync = sync_title.clone();
            split_view.connect_show_sidebar_notify(move |_| {
                sync();
            });
        }
        {
            let sync = sync_title.clone();
            split_view.connect_collapsed_notify(move |_| {
                sync();
            });
        }

        let reload_inst_holder: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let reload_inst_holder_back = reload_inst_holder.clone();

        let go_back = {
            let history_rc = nav_history.clone();
            let content_stack_b = content_stack.clone();
            let page_ref = current_page_name.clone();
            let back_btn_clone = header_back_btn.clone();
            let cur_t = active_section_title.clone();
            let cur_s = active_section_sub.clone();
            let sync_fn = sync_title.clone();
            let r_holder = reload_inst_holder_back.clone();
            Rc::new(move || {
                let (prev_opt, is_empty) = {
                    let mut hist = history_rc.borrow_mut();
                    let prev = hist.pop();
                    let empty = hist.is_empty();
                    (prev, empty)
                };
                if let Some(prev) = prev_opt {
                    content_stack_b.set_visible_child_name(&prev);
                    *page_ref.borrow_mut() = prev.clone();
                    if is_empty {
                        back_btn_clone.set_visible(false);
                    }
                    if prev == "installed" {
                        if let Some(ref r) = *r_holder.borrow() {
                            r();
                        }
                    }
                    let (t, s) = match prev.as_str() {
                        "explore" => ("Explore", ""),
                        "installed" => ("Installed Software", ""),
                        "updates" => ("Updates", ""),
                        "immutable" => ("Immutable Base OS", ""),
                        "snapshots" => ("Btrfs Snapshots", ""),
                        "downgrade" => ("Downgrade Tool", ""),
                        "settings" => ("Settings", ""),
                        _ => (prev.as_str(), ""),
                    };
                    *cur_t.borrow_mut() = t.to_string();
                    *cur_s.borrow_mut() = s.to_string();
                    sync_fn();
                }
            })
        };

        // Wire HeaderBar Back Button
        {
            let go_back_btn = go_back.clone();
            header_back_btn.connect_clicked(move |_| {
                go_back_btn();
            });
        }

        // Helper to switch page views
        let navigate_to_view = {
            let content_stack = content_stack_rc.clone();
            let page_ref = current_page_name.clone();
            let history_rc = nav_history.clone();
            let back_btn = header_back_btn.clone();
            let cur_t = active_section_title.clone();
            let cur_s = active_section_sub.clone();
            let sync_fn = sync_title.clone();
            move |name: &str, widget: gtk4::Widget| {
                let cur = page_ref.borrow().clone();
                if cur != name {
                    if name == "details" || name == "snapshots" || name == "downgrade" || name == "settings" {
                        history_rc.borrow_mut().push(cur);
                        back_btn.set_visible(true);
                    } else if name == "explore" || name == "installed" || name == "updates" || name == "immutable" {
                        history_rc.borrow_mut().clear();
                        back_btn.set_visible(false);
                    }
                }
                let (t, s) = match name {
                    "explore" => ("Explore", ""),
                    "installed" => ("Installed Software", ""),
                    "updates" => ("Updates", ""),
                    "immutable" => ("Immutable Base OS", ""),
                    "snapshots" => ("Btrfs Snapshots", ""),
                    "downgrade" => ("Downgrade Tool", ""),
                    "settings" => ("Settings", ""),
                    _ => (name, ""),
                };
                *cur_t.borrow_mut() = t.to_string();
                *cur_s.borrow_mut() = s.to_string();
                sync_fn();

                if let Some(existing) = content_stack.child_by_name(name) {
                    content_stack.remove(&existing);
                }
                content_stack.add_named(&widget, Some(name));
                content_stack.set_visible_child_name(name);
                *page_ref.borrow_mut() = name.to_string();
            }
        };

        let on_tx_start = {
            let tx_bar = tx_bar_rc.clone();

            move |mut rx: tokio::sync::mpsc::Receiver<pastor_core::TransactionEvent>| {
                let (tx_to_bar, rx_to_bar) = tokio::sync::mpsc::channel(100);
                tx_bar.monitor_transaction(rx_to_bar);

                glib::spawn_future_local(async move {
                    while let Some(evt) = rx.recv().await {
                        let _ = tx_to_bar.send(evt).await;
                    }
                });
            }
        };

        // Open details view callback
        let on_select_pkg_holder: Rc<RefCell<Option<Rc<dyn Fn(Package)>>>> = Rc::new(RefCell::new(None));

        let on_select_pkg = {
            let nav = navigate_to_view.clone();
            let store = store_rc.clone();
            let on_tx = on_tx_start.clone();
            let go_back_pkg = go_back.clone();
            let cur_t = active_section_title.clone();
            let cur_s = active_section_sub.clone();
            let sync_fn = sync_title.clone();
            let holder_for_closure = on_select_pkg_holder.clone();

            move |pkg: Package| {
                *cur_t.borrow_mut() = pkg.display_title().to_string();
                *cur_s.borrow_mut() = format!("v{}", pkg.version);
                sync_fn();

                let nav_dw = nav.clone();
                let store_dw = store.clone();
                let tx_dw = on_tx.clone();
                let go_back_c = go_back_pkg.clone();
                let on_downgrade = move |pkg_name: String| {
                    let go_back_d = go_back_c.clone();
                    let down_view = create_downgrade_view(
                        store_dw.clone(),
                        Some(pkg_name),
                        move || {
                            go_back_d();
                        },
                        tx_dw.clone(),
                    );
                    nav_dw("downgrade", down_view);
                };

                let holder_c = holder_for_closure.clone();
                let on_sel_recurse = move |target_p: Package| {
                    if let Some(cb) = holder_c.borrow().as_ref() {
                        cb(target_p);
                    }
                };

                let details_view = create_package_details_view(
                    pkg,
                    store.clone(),
                    on_sel_recurse,
                    on_downgrade,
                    on_tx.clone(),
                );
                nav("details", details_view);
            }
        };

        *on_select_pkg_holder.borrow_mut() = Some(Rc::new(on_select_pkg.clone()));

        // Connect Transaction Bar Cancel Button
        {
            let store_cancel = store_rc.clone();
            transaction_bar.connect_cancel(move || {
                let active = store_cancel.get_active_transactions();
                let sc = store_cancel.clone();
                glib::spawn_future_local(async move {
                    for tx in active {
                        let _ = sc.cancel_installation(&tx.package_id).await;
                    }
                });
            });
        }

        // Connect Header Popover Cancel All Button
        {
            let store_cancel = store_rc.clone();
            pop_cancel_all_btn.connect_clicked(move |_| {
                let active = store_cancel.get_active_transactions();
                let sc = store_cancel.clone();
                glib::spawn_future_local(async move {
                    for tx in active {
                        let _ = sc.cancel_installation(&tx.package_id).await;
                    }
                });
            });
        }

        // Connect Header Popover Console Toggle Button
        {
            let pop_c = header_ops_popover.clone();
            let tx_b = transaction_bar.clone();
            pop_console_btn.connect_clicked(move |_| {
                pop_c.popdown();
                tx_b.toggle_console();
            });
        }

        // Reactive stream from store for header operations popover and global progress ring
        {
            let store_sub = store_rc.clone();
            let ops_btn = header_ops_btn.clone();
            let ops_circ = header_ops_circle.clone();
            let pop_lbl = pop_sub_lbl.clone();
            let pop_lst = pop_list.clone();
            let popover_c = header_ops_popover.clone();
            let on_sel_pkg_c = on_select_pkg.clone();

            let update_ops_ui = {
                let store = store_sub.clone();
                let btn = ops_btn.clone();
                let circ = ops_circ.clone();
                let lbl = pop_lbl.clone();
                let lst = pop_lst.clone();
                let pop = popover_c.clone();
                let on_sel = on_sel_pkg_c.clone();

                Rc::new(move || {
                    let active = store.get_active_transactions();
                    if active.is_empty() {
                        btn.set_visible(false);
                        pop.popdown();
                        while let Some(child) = lst.first_child() {
                            lst.remove(&child);
                        }
                    } else {
                        btn.set_visible(true);
                        let total_frac: f32 = active.iter().map(|a| a.progress_fraction).sum();
                        let avg_frac = (total_frac / active.len() as f32) as f64;
                        circ.set_fraction(avg_frac);
                        let pct = (avg_frac * 100.0).round() as u32;
                        lbl.set_text(&format!("{} operations active ({}%)", active.len(), pct));

                        // Clear and rebuild rows
                        while let Some(child) = lst.first_child() {
                            lst.remove(&child);
                        }

                        for item in &active {
                            let row = gtk4::ListBoxRow::builder()
                                .activatable(true)
                                .build();

                            let hbox = gtk4::Box::builder()
                                .orientation(gtk4::Orientation::Horizontal)
                                .spacing(10)
                                .margin_top(8)
                                .margin_bottom(8)
                                .margin_start(10)
                                .margin_end(10)
                                .build();

                            let icon = gtk4::Image::builder()
                                .icon_name(crate::icons::installed_icon())
                                .pixel_size(24)
                                .build();
                            hbox.append(&icon);

                            let vbox = gtk4::Box::builder()
                                .orientation(gtk4::Orientation::Vertical)
                                .spacing(2)
                                .hexpand(true)
                                .build();

                            let name_lbl = gtk4::Label::builder()
                                .label(&item.package_name)
                                .halign(gtk4::Align::Start)
                                .ellipsize(pango::EllipsizeMode::End)
                                .css_classes(["body", "bold"])
                                .build();
                            vbox.append(&name_lbl);

                            let sub_text = if !item.log_message.trim().is_empty() {
                                format!("{}% • {}", (item.progress_fraction * 100.0) as u32, item.log_message.trim())
                            } else {
                                format!("{}%", (item.progress_fraction * 100.0) as u32)
                            };
                            let sub_lbl = gtk4::Label::builder()
                                .label(&sub_text)
                                .halign(gtk4::Align::Start)
                                .ellipsize(pango::EllipsizeMode::End)
                                .css_classes(["caption", "dim-label"])
                                .build();
                            vbox.append(&sub_lbl);
                            hbox.append(&vbox);

                            let mini_circle = CircularProgress::new(18);
                            mini_circle.set_fraction(item.progress_fraction as f64);
                            hbox.append(mini_circle.widget());

                            let item_cancel_btn = gtk4::Button::builder()
                                .icon_name("process-stop-symbolic")
                                .tooltip_text("Cancel installation")
                                .css_classes(["flat", "circular"])
                                .valign(gtk4::Align::Center)
                                .build();

                            {
                                let s = store.clone();
                                let pid = item.package_id.clone();
                                item_cancel_btn.connect_clicked(move |_| {
                                    let s_c = s.clone();
                                    let pid_c = pid.clone();
                                    glib::spawn_future_local(async move {
                                        let _ = s_c.cancel_installation(&pid_c).await;
                                    });
                                });
                            }
                            hbox.append(&item_cancel_btn);

                            row.set_child(Some(&hbox));

                            // Clicking row content navigates to details and closes popover
                            {
                                let gesture = gtk4::GestureClick::new();
                                let on_sel_inner = on_sel.clone();
                                let s_inner = store.clone();
                                let pid_inner = item.package_id.clone();
                                let pop_close = pop.clone();
                                gesture.connect_released(move |_, _, _, _| {
                                    pop_close.popdown();
                                    let on_sel_c = on_sel_inner.clone();
                                    let s_c = s_inner.clone();
                                    let p_c = pid_inner.clone();
                                    glib::spawn_future_local(async move {
                                        if let Ok(Some(pkg)) = s_c.get_package(&p_c).await {
                                            on_sel_c(pkg);
                                        }
                                    });
                                });
                                vbox.add_controller(gesture);
                            }

                            lst.append(&row);
                        }
                    }
                })
            };

            // Initial sync
            update_ops_ui();

            // Navigation list selection (Explore, Installed, Updates, Immutable OS)
            let nav = navigate_to_view.clone();
            let store = store_rc.clone();
            let on_sel = on_select_pkg.clone();
            let on_tx = on_tx_start.clone();
            let split_for_nav = split_view.clone();

            let reload_installed = {
                let nav_i = nav.clone();
                let store_i = store.clone();
                let on_sel_i = on_sel.clone();
                let on_tx_i = on_tx.clone();
                Rc::new(move || {
                    let n = nav_i.clone();
                    let s = store_i.clone();
                    let sel = on_sel_i.clone();
                    let tx = on_tx_i.clone();
                    glib::spawn_future_local(async move {
                        if let Ok(mut pkgs) = s.get_installed().await {
                            // Include any actively installing packages in the list as live rows
                            let active = s.get_active_transactions();
                            for act in active {
                                if !pkgs.iter().any(|p| p.id == act.package_id) {
                                    if let Ok(Some(pkg)) = s.get_package(&act.package_id).await {
                                        pkgs.insert(0, pkg);
                                    }
                                }
                            }

                            let list_view = create_package_list_view(
                                "Installed Software",
                                "Applications and system packages currently installed on your ParchLinux system",
                                pkgs,
                                s,
                                sel,
                                tx,
                            );
                            n("installed", list_view);
                        }
                    });
                })
            };

            *reload_inst_holder.borrow_mut() = Some(reload_installed.clone());

            // Reactive stream from store
            let mut sub = store_sub.subscribe_transactions();
            let update_ops = update_ops_ui.clone();
            let reload_on_tx = reload_installed.clone();
            let page_chk = current_page_name.clone();
            glib::spawn_future_local(async move {
                loop {
                    match sub.recv().await {
                        Ok(evt) => {
                            update_ops();
                            if let ActiveTransactionEvent::Completed(_) = evt {
                                if *page_chk.borrow() == "installed" {
                                    reload_on_tx();
                                }
                            }
                        }
                        // A lagged receiver just means we missed events; keep listening
                        // instead of exiting the loop forever.
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });

            let on_cat = {
                let nav_c = nav.clone();
                let store_c = store.clone();
                let on_sel_c = on_sel.clone();
                let on_tx_c = on_tx.clone();
                let cur_t = active_section_title.clone();
                let cur_s = active_section_sub.clone();
                let sync_fn = sync_title.clone();
                move |cat: PackageCategory| {
                    let nav_inner = nav_c.clone();
                    let store_inner = store_c.clone();
                    let on_sel_inner = on_sel_c.clone();
                    let on_tx_inner = on_tx_c.clone();
                    *cur_t.borrow_mut() = cat.title().to_string();
                    *cur_s.borrow_mut() = "Category".to_string();
                    sync_fn();
                    glib::spawn_future_local(async move {
                        if let Ok(pkgs) = store_inner.get_by_category(cat).await {
                            let list_view = create_package_list_view(
                                cat.title(),
                                "Browse applications in this category",
                                pkgs,
                                store_inner,
                                on_sel_inner,
                                on_tx_inner,
                            );
                            nav_inner("category", list_view);
                        }
                    });
                }
            };

            let on_cat_clone = on_cat.clone();
            let explore_view = create_explore_view(
                store.clone(),
                on_sel.clone(),
                on_cat,
                on_tx.clone(),
            );
            nav("explore", explore_view);

            let reload_inst_for_sidebar = reload_installed.clone();
            nav_list.connect_row_activated(move |_, row| {
                if split_for_nav.is_collapsed() {
                    split_for_nav.set_show_sidebar(false);
                }
                let idx = row.index();
                match idx {
                    0 => {
                        // Explore
                        let explore = create_explore_view(
                            store.clone(),
                            on_sel.clone(),
                            on_cat_clone.clone(),
                            on_tx.clone(),
                        );
                        nav("explore", explore);
                    }
                    1 => {
                        // Installed
                        let nav_i = nav.clone();
                        let r_inst = reload_inst_for_sidebar.clone();
                        nav_i(
                            "installed",
                            create_loading_view(
                                "Installed Software",
                                "Scanning installed packages and Flatpaks on your system…",
                            ),
                        );
                        r_inst();
                    }
                    2 => {
                        // Updates
                        let updates_view = create_updates_view(
                            store.clone(),
                            on_sel.clone(),
                            on_tx.clone(),
                        );
                        nav("updates", updates_view);
                    }
                    _ => {}
                }
            });
        }

        let show_updates_fn: Rc<dyn Fn()> = {
            let nav = navigate_to_view.clone();
            let store = store_rc.clone();
            let on_sel = on_select_pkg.clone();
            let on_tx = on_tx_start.clone();
            let nav_list = nav_list.clone();
            Rc::new(move || {
                if let Some(row) = nav_list.row_at_index(2) {
                    nav_list.select_row(Some(&row));
                }
                let updates_view = create_updates_view(
                    store.clone(),
                    on_sel.clone(),
                    on_tx.clone(),
                );
                nav("updates", updates_view);
            })
        };

        // Categories list selection
        {
            let nav = navigate_to_view.clone();
            let store = store_rc.clone();
            let on_sel = on_select_pkg.clone();
            let on_tx = on_tx_start.clone();
            let split_for_cat = split_view.clone();
            let wt_cat = window_title.clone();
            let active_cats_click = active_categories.clone();

            cat_list.connect_row_activated(move |_, row| {
                if split_for_cat.is_collapsed() {
                    split_for_cat.set_show_sidebar(false);
                }
                let idx = row.index() as usize;
                if let Some(cat) = active_cats_click.get(idx) {
                    let cat = *cat;
                    wt_cat.set_subtitle(cat.title());
                    let nav_c = nav.clone();
                    let store_c = store.clone();
                    let on_sel_c = on_sel.clone();
                    let on_tx_c = on_tx.clone();

                    nav_c(
                        "category",
                        create_loading_view(
                            cat.title(),
                            &format!("Fetching all {} applications…", cat.title()),
                        ),
                    );

                    glib::spawn_future_local(async move {
                        if let Ok(pkgs) = store_c.get_by_category(cat).await {
                            let list_view = create_package_list_view(
                                cat.title(),
                                "Browse applications in this category",
                                pkgs,
                                store_c,
                                on_sel_c,
                                on_tx_c,
                            );
                            nav_c("category", list_view);
                        }
                    });
                }
            });
        }

        // Search Entry query changed
        {
            let nav = navigate_to_view.clone();
            let store = store_rc.clone();
            let on_sel = on_select_pkg.clone();
            let on_tx = on_tx_start.clone();
            let cur_t = active_section_title.clone();
            let cur_s = active_section_sub.clone();
            let sync_fn = sync_title.clone();

            search_entry.connect_search_changed(move |entry| {
                let query = entry.text().to_string();
                if query.trim().is_empty() {
                    return;
                }
                *cur_t.borrow_mut() = format!("Search “{}”", query);
                *cur_s.borrow_mut() = "".to_string();
                sync_fn();

                let nav_s = nav.clone();
                let store_s = store.clone();
                let on_sel_s = on_sel.clone();
                let on_tx_s = on_tx.clone();
                let q_title = format!("Search results for \"{}\"", query);

                nav_s(
                    "search",
                    create_loading_view(
                        &q_title,
                        "Searching repositories and Flathub catalog…",
                    ),
                );

                glib::spawn_future_local(async move {
                    if let Ok(results) = store_s.search(&query).await {
                        let list_view = create_package_list_view(
                            &q_title,
                            "Filtered packages matching your search query across all active ecosystems",
                            results,
                            store_s,
                            on_sel_s,
                            on_tx_s,
                        );
                        nav_s("search", list_view);
                    }
                });
            });
        }

        // Application Actions & Page Routing
        {
            // Toggle Sidebar Action (F9)
            let action_toggle_sidebar = gio::SimpleAction::new("toggle-sidebar", None);
            {
                let sv = split_view.clone();
                action_toggle_sidebar.connect_activate(move |_, _| {
                    let new_state = !sv.shows_sidebar();
                    sv.set_show_sidebar(new_state);
                });
            }
            app.add_action(&action_toggle_sidebar);
            app.set_accels_for_action("app.toggle-sidebar", &["F9"]);

            // Settings Page Action
            let nav_settings = navigate_to_view.clone();
            let s_settings = store_rc.clone();
            let nav_snaps_from_set = navigate_to_view.clone();
            let go_back_for_settings = go_back.clone();
            let action_settings = gio::SimpleAction::new("settings", None);
            action_settings.connect_activate(move |_, _| {
                let s_backend = s_settings.snapshot_backend();
                let nav_snaps_cb = nav_snaps_from_set.clone();
                let go_back_s = go_back_for_settings.clone();
                let open_snaps = move || {
                    let go_back_c = go_back_s.clone();
                    let snaps_view = create_snapshots_view(s_backend.clone(), move || {
                        go_back_c();
                    });
                    nav_snaps_cb("snapshots", snaps_view);
                };

                let set_view = create_settings_view(s_settings.clone(), open_snaps);
                nav_settings("settings", set_view);
            });
            app.add_action(&action_settings);
            app.set_accels_for_action("app.settings", &["<Control>comma"]);

            // Snapper Snapshots Page Action
            let nav_snapshots = navigate_to_view.clone();
            let s_snapshots = store_rc.clone();
            let go_back_for_snaps = go_back.clone();
            let action_snapshots = gio::SimpleAction::new("snapshots", None);
            action_snapshots.connect_activate(move |_, _| {
                let go_back_c = go_back_for_snaps.clone();
                let snaps_view = create_snapshots_view(
                    s_snapshots.snapshot_backend(),
                    move || {
                        go_back_c();
                    },
                );
                nav_snapshots("snapshots", snaps_view);
            });
            app.add_action(&action_snapshots);

            // Downgrade Tool Page Action
            let nav_downgrade = navigate_to_view.clone();
            let s_downgrade = store_rc.clone();
            let tx_down = on_tx_start.clone();
            let go_back_for_down = go_back.clone();
            let action_downgrade = gio::SimpleAction::new("downgrade", None);
            action_downgrade.connect_activate(move |_, _| {
                let go_back_c = go_back_for_down.clone();
                let down_view = create_downgrade_view(
                    s_downgrade.clone(),
                    None,
                    move || {
                        go_back_c();
                    },
                    tx_down.clone(),
                );
                nav_downgrade("downgrade", down_view);
            });
            app.add_action(&action_downgrade);

            // About Dialog Action
            let w_about = window.clone();
            let action_about = gio::SimpleAction::new("about", None);
            action_about.connect_activate(move |_, _| {
                show_about_dialog(&w_about);
            });
            app.add_action(&action_about);

            // Mirror Manager Action (mirrorman)
            let action_mirrors = gio::SimpleAction::new("mirrors", None);
            action_mirrors.connect_activate(|_, _| {
                crate::views::settings_view::launch_mirrorman();
            });
            app.add_action(&action_mirrors);
        }

        Self {
            window,
            show_updates_fn,
        }
    }

    pub fn present(&self) {
        self.window.present();
    }

    pub fn show_updates(&self) {
        self.window.present();
        (self.show_updates_fn)();
    }
}

fn create_nav_row(title: &str, icon_name: &str) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::builder()
        .activatable(true)
        .build();

    let hbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(12)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(10)
        .margin_end(10)
        .build();

    let icon = gtk4::Image::builder()
        .icon_name(icon_name)
        .pixel_size(18)
        .build();
    hbox.append(&icon);

    let label = gtk4::Label::builder()
        .label(title)
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .build();
    hbox.append(&label);

    row.set_child(Some(&hbox));
    row
}

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::{gio, glib};
use pastor_core::{Package, PackageCategory};
use pastor_store::{ActiveTransactionEvent, Store};

use crate::{
    dialogs::show_shortcuts_dialog,
    views::{
        create_explore_view, create_loading_view, create_package_details_view,
        create_package_list_view, create_updates_view, TransactionBar,
    },
};

pub mod actions;
pub mod operations_popover;
pub mod search;
pub mod sidebar;

use actions::setup_app_actions;
use operations_popover::create_operations_popover;
use search::setup_search_controller;
use sidebar::build_sidebar;

#[derive(Clone)]
pub struct MainWindow {
    window: adw::ApplicationWindow,
    show_updates_fn: Rc<dyn Fn()>,
    show_explore_fn: Rc<dyn Fn()>,
    show_installed_fn: Rc<dyn Fn()>,
    show_settings_fn: Rc<dyn Fn()>,
    show_snapshots_fn: Rc<dyn Fn()>,
    show_downgrade_fn: Rc<dyn Fn()>,
    show_search_fn: Rc<dyn Fn(String)>,
    open_package_fn: Rc<dyn Fn(String)>,
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
            .min_sidebar_width(200.0)
            .max_sidebar_width(320.0)
            .sidebar_width_fraction(0.25)
            .enable_hide_gesture(true)
            .enable_show_gesture(true)
            .show_sidebar(true)
            .build();

        // -------------------------
        // SIDEBAR
        // -------------------------
        let sidebar = build_sidebar();
        split_view.set_sidebar(Some(&sidebar.toolbar));

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
            sidebar.collapse_btn.connect_clicked(move |_| {
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
            let col = sidebar.collapse_btn.clone();
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
                        wt.set_subtitle(&format!("{} - {}", title, sub));
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
                } else if page_ref.borrow().as_str() == "search" {
                    content_stack_b.set_visible_child_name("explore");
                    *page_ref.borrow_mut() = "explore".to_string();
                    back_btn_clone.set_visible(false);
                    *cur_t.borrow_mut() = "Explore".to_string();
                    *cur_s.borrow_mut() = "".to_string();
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

        // Global Keyboard Shortcuts
        let key_controller = gtk4::EventControllerKey::new();
        let search_entry_focus = sidebar.search_entry.clone();
        let sv_key = split_view.clone();
        let go_back_key = go_back.clone();
        let back_btn_key = header_back_btn.clone();
        let win_key = window.clone();

        key_controller.connect_key_pressed(move |_, keyval, _, state| {
            let ctrl = state.contains(gtk4::gdk::ModifierType::CONTROL_MASK);
            let alt = state.contains(gtk4::gdk::ModifierType::ALT_MASK);

            if ctrl
                && (keyval == gtk4::gdk::Key::f
                    || keyval == gtk4::gdk::Key::F
                    || keyval == gtk4::gdk::Key::k
                    || keyval == gtk4::gdk::Key::K)
            {
                if !sv_key.shows_sidebar() {
                    sv_key.set_show_sidebar(true);
                }
                search_entry_focus.grab_focus();
                search_entry_focus.select_region(0, -1);
                gtk4::glib::Propagation::Stop
            } else if keyval == gtk4::gdk::Key::F9
                || (ctrl && (keyval == gtk4::gdk::Key::b || keyval == gtk4::gdk::Key::B))
            {
                sv_key.set_show_sidebar(!sv_key.shows_sidebar());
                gtk4::glib::Propagation::Stop
            } else if (alt && keyval == gtk4::gdk::Key::Left)
                || (alt && keyval == gtk4::gdk::Key::BackSpace)
            {
                if back_btn_key.is_visible() {
                    go_back_key();
                    gtk4::glib::Propagation::Stop
                } else {
                    gtk4::glib::Propagation::Proceed
                }
            } else if (ctrl
                && (keyval == gtk4::gdk::Key::question
                    || keyval == gtk4::gdk::Key::slash))
                || (ctrl && keyval == gtk4::gdk::Key::F1)
            {
                show_shortcuts_dialog(&win_key);
                gtk4::glib::Propagation::Stop
            } else if ctrl
                && (keyval == gtk4::gdk::Key::q
                    || keyval == gtk4::gdk::Key::Q
                    || keyval == gtk4::gdk::Key::w
                    || keyval == gtk4::gdk::Key::W)
            {
                win_key.close();
                gtk4::glib::Propagation::Stop
            } else {
                gtk4::glib::Propagation::Proceed
            }
        });
        window.add_controller(key_controller);

        // Helper to switch page views
        let navigate_to_view = {
            let content_stack = content_stack_rc.clone();
            let page_ref = current_page_name.clone();
            let history_rc = nav_history.clone();
            let back_btn = header_back_btn.clone();
            let cur_t = active_section_title.clone();
            let cur_s = active_section_sub.clone();
            let sync_fn = sync_title.clone();
            Rc::new(move |name: &str, widget: gtk4::Widget| {
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
            })
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
                    let down_view = crate::views::create_downgrade_view(
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

        // Header Operations Button + Popover
        let (header_ops_btn, update_ops_ui) = create_operations_popover(
            &store_rc,
            &transaction_bar,
            on_select_pkg.clone(),
        );
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
        menu_model.append(Some("Keyboard Shortcuts"), Some("app.shortcuts"));
        menu_model.append(Some("About Parch Store"), Some("app.about"));
        tools_menu_btn.set_menu_model(Some(&menu_model));
        content_header.pack_end(&tools_menu_btn);

        content_toolbar.add_top_bar(&content_header);

        let on_category_select = {
            let nav_inner_c = navigate_to_view.clone();
            let store_inner_c = store_rc.clone();
            let on_sel_inner_c = on_select_pkg.clone();
            let on_tx_inner_c = on_tx_start.clone();
            let cur_t = active_section_title.clone();
            let cur_s = active_section_sub.clone();
            let sync_fn = sync_title.clone();
            move |cat: PackageCategory| {
                let nav_inner = nav_inner_c.clone();
                let store_inner = store_inner_c.clone();
                let on_sel_inner = on_sel_inner_c.clone();
                let on_tx_inner = on_tx_inner_c.clone();
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

        // Initial sync of ops UI
        update_ops_ui();

        // Setup App Actions
        setup_app_actions(
            app,
            &window,
            &split_view,
            &store_rc,
            navigate_to_view.clone(),
            go_back.clone(),
            on_tx_start.clone(),
        );

        let reload_installed = {
            let nav_i = navigate_to_view.clone();
            let store_i = store_rc.clone();
            let sel = on_select_pkg.clone();
            let tx = on_tx_start.clone();
            Rc::new(move || {
                let n = nav_i.clone();
                let s = store_i.clone();
                let sel_c = sel.clone();
                let tx_c = tx.clone();
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
                            sel_c,
                            tx_c,
                        );
                        n("installed", list_view);
                    }
                });
            })
        };

        *reload_inst_holder.borrow_mut() = Some(reload_installed.clone());

        // Reactive stream from store
        {
            let mut sub = store_rc.subscribe_transactions();
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
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        }

        // Initial explore view
        {
            let on_cat = on_category_select.clone();
            let explore_view = create_explore_view(
                store_rc.clone(),
                on_select_pkg.clone(),
                on_cat,
                on_tx_start.clone(),
            );
            navigate_to_view("explore", explore_view);
        }

        // Connect Sidebar Nav List
        {
            let nav = navigate_to_view.clone();
            let store = store_rc.clone();
            let on_sel = on_select_pkg.clone();
            let on_tx = on_tx_start.clone();
            let on_cat = on_category_select.clone();
            let split_for_nav = split_view.clone();
            let reload_inst_for_sidebar = reload_installed.clone();

            sidebar.nav_list.connect_row_activated(move |_, row| {
                if split_for_nav.is_collapsed() {
                    split_for_nav.set_show_sidebar(false);
                }
                let idx = row.index();
                match idx {
                    0 => {
                        let explore = create_explore_view(
                            store.clone(),
                            on_sel.clone(),
                            on_cat.clone(),
                            on_tx.clone(),
                        );
                        nav("explore", explore);
                    }
                    1 => {
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

        let show_explore_fn: Rc<dyn Fn()> = {
            let nav = navigate_to_view.clone();
            let store = store_rc.clone();
            let on_sel = on_select_pkg.clone();
            let on_cat_c = on_category_select.clone();
            let on_tx = on_tx_start.clone();
            let nav_list = sidebar.nav_list.clone();
            Rc::new(move || {
                if let Some(row) = nav_list.row_at_index(0) {
                    nav_list.select_row(Some(&row));
                }
                let explore = create_explore_view(
                    store.clone(),
                    on_sel.clone(),
                    on_cat_c.clone(),
                    on_tx.clone(),
                );
                nav("explore", explore);
            })
        };

        let show_installed_fn: Rc<dyn Fn()> = {
            let nav = navigate_to_view.clone();
            let r_holder = reload_inst_holder.clone();
            let nav_list = sidebar.nav_list.clone();
            Rc::new(move || {
                if let Some(row) = nav_list.row_at_index(1) {
                    nav_list.select_row(Some(&row));
                }
                nav(
                    "installed",
                    create_loading_view(
                        "Installed Software",
                        "Scanning installed packages and Flatpaks on your system…",
                    ),
                );
                if let Some(r_inst) = r_holder.borrow().as_ref() {
                    r_inst();
                }
            })
        };

        let show_updates_fn: Rc<dyn Fn()> = {
            let nav = navigate_to_view.clone();
            let store = store_rc.clone();
            let on_sel = on_select_pkg.clone();
            let on_tx = on_tx_start.clone();
            let nav_list = sidebar.nav_list.clone();
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

        let app_handle = app.clone();
        let show_settings_fn: Rc<dyn Fn()> = {
            let a = app_handle.clone();
            Rc::new(move || {
                a.activate_action("settings", None);
            })
        };

        let show_snapshots_fn: Rc<dyn Fn()> = {
            let a = app_handle.clone();
            Rc::new(move || {
                a.activate_action("snapshots", None);
            })
        };

        let show_downgrade_fn: Rc<dyn Fn()> = {
            let a = app_handle.clone();
            Rc::new(move || {
                a.activate_action("downgrade", None);
            })
        };

        // Setup Search and Package navigation
        let search_controller = setup_search_controller(
            &sidebar.search_entry,
            &store_rc,
            navigate_to_view.clone(),
            on_select_pkg.clone(),
            on_tx_start.clone(),
            go_back.clone(),
            current_page_name.clone(),
            sync_title.clone(),
            active_section_title.clone(),
            active_section_sub.clone(),
        );

        // Categories list selection
        {
            let nav = navigate_to_view.clone();
            let store = store_rc.clone();
            let on_sel = on_select_pkg.clone();
            let on_tx = on_tx_start.clone();
            let split_for_cat = split_view.clone();
            let wt_cat = window_title.clone();
            let active_cats_click = sidebar.active_categories.clone();
            let page_ref_for_cat = current_page_name.clone();

            sidebar.cat_list.connect_row_activated(move |_, row| {
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

                    let page_ref_cat = page_ref_for_cat.clone();
                    glib::spawn_future_local(async move {
                        if let Ok(pkgs) = store_c.get_by_category(cat).await {
                            if page_ref_cat.borrow().as_str() != "category" {
                                return;
                            }
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

        Self {
            window,
            show_updates_fn,
            show_explore_fn,
            show_installed_fn,
            show_settings_fn,
            show_snapshots_fn,
            show_downgrade_fn,
            show_search_fn: search_controller.show_search_fn,
            open_package_fn: search_controller.open_package_fn,
        }
    }

    pub fn present(&self) {
        self.window.present();
    }

    pub fn show_explore(&self) {
        self.window.present();
        (self.show_explore_fn)();
    }

    pub fn show_installed(&self) {
        self.window.present();
        (self.show_installed_fn)();
    }

    pub fn show_updates(&self) {
        self.window.present();
        (self.show_updates_fn)();
    }

    pub fn show_settings(&self) {
        self.window.present();
        (self.show_settings_fn)();
    }

    pub fn show_snapshots(&self) {
        self.window.present();
        (self.show_snapshots_fn)();
    }

    pub fn show_downgrade(&self) {
        self.window.present();
        (self.show_downgrade_fn)();
    }

    pub fn show_search(&self, query: &str) {
        self.window.present();
        (self.show_search_fn)(query.to_string());
    }

    pub fn open_package(&self, pkg_name: &str) {
        self.window.present();
        (self.open_package_fn)(pkg_name.to_string());
    }

    pub fn handle_uri(&self, uri: &str) {
        self.window.present();
        match crate::uri_handler::parse_uri(uri) {
            Some(crate::uri_handler::PastorRoute::Install(pkg)) => {
                self.open_package(&pkg);
            }
            Some(crate::uri_handler::PastorRoute::Details(pkg)) => {
                self.open_package(&pkg);
            }
            Some(crate::uri_handler::PastorRoute::Search(query)) => {
                self.show_search(&query);
            }
            Some(crate::uri_handler::PastorRoute::Updates) => {
                self.show_updates();
            }
            Some(crate::uri_handler::PastorRoute::Installed) => {
                self.show_installed();
            }
            Some(crate::uri_handler::PastorRoute::Explore) => {
                self.show_explore();
            }
            Some(crate::uri_handler::PastorRoute::Settings) => {
                self.show_settings();
            }
            Some(crate::uri_handler::PastorRoute::Snapshots) => {
                self.show_snapshots();
            }
            Some(crate::uri_handler::PastorRoute::Downgrade) => {
                self.show_downgrade();
            }
            None => {
                tracing::warn!("Received unsupported or invalid URI: {}", uri);
            }
        }
    }
}

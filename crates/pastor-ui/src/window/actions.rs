use std::rc::Rc;

use adw::prelude::*;
use gtk4::gio;
use pastor_store::Store;

use crate::{
    dialogs::{show_about_dialog, show_shortcuts_dialog},
    views::{create_downgrade_view, create_snapshots_view},
};

pub fn setup_app_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    split_view: &adw::OverlaySplitView,
    store: &Store,
    navigate_to_view: Rc<dyn Fn(&str, gtk4::Widget)>,
    go_back: Rc<dyn Fn()>,
    on_tx_start: impl Fn(tokio::sync::mpsc::Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
) {
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

    // Settings Page Action (SET-001: AdwPreferencesWindow dialog)
    let win_for_settings = window.clone();
    let s_settings = store.clone();
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

        crate::views::show_settings_window(
            Some(win_for_settings.upcast_ref::<gtk4::Window>()),
            s_settings.clone(),
            open_snaps,
        );
    });
    app.add_action(&action_settings);
    app.set_accels_for_action("app.settings", &["<Control>comma"]);

    // Snapper Snapshots Page Action
    let nav_snapshots = navigate_to_view.clone();
    let s_snapshots = store.clone();
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
    let s_downgrade = store.clone();
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

    // Shortcuts Dialog Action
    let w_shortcuts = window.clone();
    let action_shortcuts = gio::SimpleAction::new("shortcuts", None);
    action_shortcuts.connect_activate(move |_, _| {
        show_shortcuts_dialog(&w_shortcuts);
    });
    app.add_action(&action_shortcuts);
    app.set_accels_for_action("app.shortcuts", &["<Primary>question", "<Primary>slash", "<Primary>F1"]);

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

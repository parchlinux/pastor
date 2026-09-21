use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk4::{gio, glib};
use pastor_store::Store;
use tokio::sync::Mutex;

use crate::{
    indicator::{spawn_indicator, AppCommand, IndicatorState},
    notifications::{send_updates_notification, withdraw_updates_notification},
    styles::APP_CSS,
    update_checker::{spawn_update_checker, UpdateCheckerOptions},
    window::MainWindow,
};

pub struct Application {
    app: adw::Application,
}

impl Application {
    pub fn new() -> Self {
        let app = adw::Application::builder()
            .application_id("com.parchlinux.pastor")
            .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE | gio::ApplicationFlags::HANDLES_OPEN)
            .build();

        let store = Store::new();
        let main_window: Rc<RefCell<Option<MainWindow>>> = Rc::new(RefCell::new(None));
        let indicator_state = IndicatorState::default();

        // Channel for commands from tray, background workers, and notifications
        let (cmd_sender, mut cmd_receiver) = tokio::sync::mpsc::unbounded_channel::<AppCommand>();

        // Channel for manually triggering update checks
        let (manual_check_tx, manual_check_rx) = tokio::sync::mpsc::channel::<()>(4);

        let get_or_create_window = {
            let main_window = main_window.clone();
            let store = store.clone();
            move |app: &adw::Application| -> MainWindow {
                let mut win_ref = main_window.borrow_mut();
                if let Some(w) = win_ref.as_ref() {
                    w.clone()
                } else {
                    let w = MainWindow::new(app, store.clone());
                    *win_ref = Some(w.clone());
                    w
                }
            }
        };

        // Activate signal handling (e.g. desktop launch with no args)
        {
            let get_window = get_or_create_window.clone();
            app.connect_activate(move |app| {
                let w = get_window(app);
                w.present();
            });
        }

        // Open signal handling (e.g. URI handling via D-Bus / GFile)
        {
            let get_window = get_or_create_window.clone();
            app.connect_open(move |app, files, _hint| {
                let w = get_window(app);
                for file in files {
                    let uri = file.uri().to_string();
                    w.handle_uri(&uri);
                }
            });
        }

        // Attach command receiver on the GTK main loop
        {
            let app_weak = app.downgrade();
            let get_window = get_or_create_window.clone();
            let manual_tx = manual_check_tx.clone();

            glib::MainContext::default().spawn_local(async move {
                while let Some(cmd) = cmd_receiver.recv().await {
                    let Some(app) = app_weak.upgrade() else {
                        break;
                    };

                    match cmd {
                        AppCommand::PresentWindow => {
                            let w = get_window(&app);
                            w.present();
                        }
                        AppCommand::OpenUpdatesView => {
                            let w = get_window(&app);
                            w.show_updates();
                        }
                        AppCommand::TriggerUpdateCheck => {
                            let _ = manual_tx.try_send(());
                        }
                        AppCommand::NotifyUpdates(count) => {
                            send_updates_notification(&app, count);
                        }
                        AppCommand::WithdrawNotification => {
                            withdraw_updates_notification(&app);
                        }
                        AppCommand::Quit => {
                            app.quit();
                            std::process::exit(0);
                        }
                    }
                }
            });
        }

        // Register GAction: app.open-updates
        {
            let app_weak = app.downgrade();
            let get_window = get_or_create_window.clone();
            let action = gio::SimpleAction::new("open-updates", None);
            action.connect_activate(move |_, _| {
                if let Some(app) = app_weak.upgrade() {
                    let w = get_window(&app);
                    w.show_updates();
                }
            });
            app.add_action(&action);
        }

        // Register GAction: app.check-updates
        {
            let manual_tx = manual_check_tx.clone();
            let action = gio::SimpleAction::new("check-updates", None);
            action.connect_activate(move |_, _| {
                let _ = manual_tx.try_send(());
            });
            app.add_action(&action);
        }

        // Register GAction: app.quit
        {
            let app_weak = app.downgrade();
            let action = gio::SimpleAction::new("quit", None);
            action.connect_activate(move |_, _| {
                if let Some(app) = app_weak.upgrade() {
                    app.quit();
                }
                std::process::exit(0);
            });
            app.add_action(&action);
        }

        // Startup: Setup styles, background tray, and background update checker
        {
            let store = store.clone();
            let indicator_state = indicator_state.clone();
            let cmd_sender = cmd_sender.clone();
            let manual_rx_arc = Arc::new(Mutex::new(Some(manual_check_rx)));

            app.connect_startup(move |_| {
                if let Some(settings) = gtk4::Settings::default() {
                    settings.set_gtk_application_prefer_dark_theme(false);
                }
                adw::init().expect("Failed to initialize Libadwaita");
                adw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);

                // Load custom CSS
                let provider = gtk4::CssProvider::new();
                provider.load_from_data(APP_CSS);
                if let Some(display) = gtk4::gdk::Display::default() {
                    gtk4::style_context_add_provider_for_display(
                        &display,
                        &provider,
                        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
                    );
                }

                // Spawn StatusNotifierItem tray and background update checker on Tokio runtime
                let store = store.clone();
                let state = indicator_state.clone();
                let sender = cmd_sender.clone();
                let manual_rx_arc = manual_rx_arc.clone();

                tokio::spawn(async move {
                    let rx_opt = {
                        let mut guard = manual_rx_arc.lock().await;
                        guard.take()
                    };

                    if let Some(rx) = rx_opt {
                        match spawn_indicator(state.clone(), sender.clone()).await {
                            Ok(tray_handle) => {
                                tracing::info!("StatusNotifierItem tray initialized successfully");
                                spawn_update_checker(
                                    store,
                                    state,
                                    Some(tray_handle),
                                    sender,
                                    rx,
                                    UpdateCheckerOptions::default(),
                                );
                            }
                            Err(e) => {
                                tracing::warn!("Tray initialization: {e}; running background update checker without SNI tray");
                                spawn_update_checker(
                                    store,
                                    state,
                                    None,
                                    sender,
                                    rx,
                                    UpdateCheckerOptions::default(),
                                );
                            }
                        }
                    }
                });
            });
        }

        // Command line handling: supports --hidden / --indicator, URIs, and subcommands
        {
            let get_window = get_or_create_window;
            let manual_tx = manual_check_tx;

            app.connect_command_line(move |app, cmdline| {
                let args: Vec<String> = cmdline
                    .arguments()
                    .into_iter()
                    .map(|a| a.to_string_lossy().to_string())
                    .collect();

                if args.iter().any(|a| a == "--version" || a == "-v") {
                    println!("pastor {}", env!("CARGO_PKG_VERSION"));
                    return 0.into();
                }

                if args.iter().any(|a| a == "--help" || a == "-h") {
                    println!("Parch Store - Application & Package Manager");
                    println!("\nUsage: pastor [OPTIONS] [URI | SUBCOMMAND]\n");
                    println!("Options:");
                    println!("  --hidden, --indicator  Start minimized to system tray");
                    println!("  --updates              Open directly to Updates view");
                    println!("  --check-updates        Trigger an immediate background update check");
                    println!("  -v, --version          Print version information");
                    println!("  -h, --help             Print this help message\n");
                    println!("Supported URIs & Subcommands:");
                    println!("  pastor://install/<pkg> Open package details and offer installation");
                    println!("  pastor://details/<pkg> View package information");
                    println!("  pastor://search/<term> Search repositories, AUR, and Flatpaks");
                    println!("  pastor://updates       Open updates page");
                    println!("  appstream://<pkg>      Open package via AppStream ID");
                    println!("  install <pkg>          Open package details view");
                    println!("  search <term>          Search packages");
                    return 0.into();
                }

                let is_hidden = args.iter().any(|a| a == "--hidden" || a == "--indicator");
                let is_check = args.iter().any(|a| a == "--check-updates");
                let is_updates = args.iter().any(|a| a == "--updates");

                if is_check {
                    let _ = manual_tx.try_send(());
                }

                // Check for URI or positional arguments
                let uri_arg = args.iter().skip(1).find(|a| {
                    a.starts_with("pastor:") || a.starts_with("appstream:")
                });

                if let Some(uri) = uri_arg {
                    let w = get_window(app);
                    w.handle_uri(uri);
                } else if args.len() >= 3 && args[1] == "install" {
                    let w = get_window(app);
                    w.open_package(&args[2]);
                } else if args.len() >= 3 && (args[1] == "search" || args[1] == "--search") {
                    let w = get_window(app);
                    w.show_search(&args[2]);
                } else if args.len() >= 3 && (args[1] == "show" || args[1] == "details") {
                    let w = get_window(app);
                    w.open_package(&args[2]);
                } else if is_updates {
                    let w = get_window(app);
                    w.show_updates();
                } else if !is_hidden {
                    let w = get_window(app);
                    w.present();
                }

                0.into()
            });
        }

        Self { app }
    }

    pub fn run(&self) -> glib::ExitCode {
        self.app.run()
    }
}

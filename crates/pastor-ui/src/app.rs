use adw::prelude::*;
use pastor_store::Store;

use crate::{styles::APP_CSS, window::MainWindow};

pub struct Application {
    app: adw::Application,
}

impl Application {
    pub fn new() -> Self {
        let app = adw::Application::builder()
            .application_id("org.parchlinux.Pastor")
            .build();

        app.connect_startup(|_| {
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
        });

        app.connect_activate(|app| {
            let store = Store::new();
            let window = MainWindow::new(app, store);
            window.present();
        });

        Self { app }
    }

    pub fn run(&self) -> glib::ExitCode {
        self.app.run()
    }
}

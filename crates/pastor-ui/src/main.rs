mod app;
mod dialogs;
mod icons;
mod indicator;
mod notifications;
mod styles;
mod update_checker;
mod views;
mod window;

use app::Application;
use gtk4::glib;

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt::init();

    // Initialize multi-threaded Tokio runtime so async tasks, network timers, and sleeps
    // running in glib futures have an active reactor context.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to initialize Tokio runtime");
    let _guard = rt.enter();

    let app = Application::new();
    app.run()
}

mod app;
mod dialogs;
mod icons;
mod indicator;
mod notifications;
mod styles;
mod update_checker;
pub mod uri_handler;
mod views;
mod window;

use app::Application;
use gtk4::glib;

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "--alpm-worker" {
        if let Err(e) = pastor_alpm::run_worker(&args[2..]) {
            eprintln!("ALPM Worker error: {e}");
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    tracing_subscriber::fmt::init();

    // Initialize multi-threaded Tokio runtime so async tasks, network timers, and sleeps
    // running in glib futures have an active reactor context.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to initialize Tokio runtime");
    let _guard = rt.enter();

    let app = Application::new();
    let exit_code = app.run();
    std::process::exit(i32::from(exit_code));
}

use adw::prelude::*;

pub fn show_about_dialog(parent: &impl IsA<gtk4::Widget>) {
    let comments = pastor_core::tr("A modern, native package management and software center for ParchLinux.\n\nSupporting [world] and [void] repositories, AUR, Flatpak, and Snapper snapshots.");
    let app_name = pastor_core::tr("Parch Store (Pastor)");
    let dev_name = pastor_core::tr("ParchLinux Team");

    let about = adw::AboutDialog::builder()
        .application_name(app_name)
        .application_icon("com.parchlinux.pastor")
        .developer_name(dev_name)
        .version(env!("CARGO_PKG_VERSION"))
        .copyright("© 2026 ParchLinux Team")
        .website("https://parchlinux.com")
        .issue_url("https://github.com/parchlinux/pastor/issues")
        .comments(comments)
        .license_type(gtk4::License::Gpl30)
        .build();

    if let Some(root) = parent.root() {
        if let Ok(w) = root.downcast::<gtk4::Window>() {
            about.present(Some(&w));
            return;
        }
    }
    about.present(gtk4::Window::NONE);
}

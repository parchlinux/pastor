use adw::prelude::*;

pub fn show_about_dialog(parent: &impl IsA<gtk4::Widget>) {
    let about = adw::AboutDialog::builder()
        .application_name("Parch Store (Pastor)")
        .application_icon("system-software-install")
        .developer_name("ParchLinux Team")
        .version("0.1.0")
        .copyright("© 2026 ParchLinux Team")
        .website("https://parchlinux.com")
        .issue_url("https://github.com/parchlinux/pastor/issues")
        .comments("A modern, native package management and software center for ParchLinux.\n\nSupporting [world] and [void] repositories, AUR, Flatpak, and Snapper snapshots.")
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

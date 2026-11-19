use adw::prelude::*;

pub fn show_about_dialog(parent: &impl IsA<gtk4::Widget>) {
    let comments = pastor_core::tr("A complete store for Parch GNU/Linux.");
    let app_name = pastor_core::tr("Parch Store (Pastor)");
    let dev_name = pastor_core::tr("ParchLinux Team");
    let translator_credits = pastor_core::tr("translator-credits");

    let release_notes = "
<p>Version 0.4.0 introduces major new features, diagnostics, and refinements:</p>
<ul>
  <li>Parch Doctor: Diagnostic suite featuring pacman lock detection and release, visual side-by-side pacnew diff review, and intelligent cache management.</li>
  <li>Mobile Adaptive Interface: Fully responsive layout adapting from mobile screens to 4K desktops without layout overflow.</li>
  <li>Multilingual and RTL Support: Complete Persian and Arabic localizations with native right-to-left UI alignment.</li>
  <li>View Switching: Seamless toggle between spacious grid cards and compact list views.</li>
  <li>Launch Reliability: Desktop application launcher resolving exact desktop entries for installed apps.</li>
</ul>
";

    let about = adw::AboutDialog::builder()
        .application_name(app_name)
        .application_icon("com.parchlinux.pastor")
        .developer_name(dev_name)
        .version(env!("CARGO_PKG_VERSION"))
        .copyright("© 2026 ParchLinux Team")
        .website("https://parchlinux.com")
        .issue_url("https://github.com/parchlinux/pastor/issues")
        .support_url("https://forum.parchlinux.com")
        .comments(comments)
        .release_notes(release_notes.trim())
        .release_notes_version("0.4.0")
        .developers(vec![
            "Sohrab Behdani <behdanisohrab@gmail.com>",
            "ParchLinux Team <contact@parchlinux.com>",
        ])
        .designers(vec!["ParchLinux Design Team"])
        .artists(vec!["ParchLinux Artwork Team"])
        .translator_credits(translator_credits)
        .license_type(gtk4::License::Gpl30)
        .build();

    about.add_link(
        &pastor_core::tr("Parch Apps Web"),
        "https://apps.parchlinux.com",
    );
    about.add_link(
        &pastor_core::tr("Parch Wiki"),
        "https://wiki.parchlinux.com",
    );
    about.add_link(
        &pastor_core::tr("Parch Community Forum"),
        "https://forum.parchlinux.com",
    );
    about.add_link(
        &pastor_core::tr("Source Repository"),
        "https://github.com/parchlinux/pastor",
    );

    if let Some(root) = parent.root() {
        if let Ok(w) = root.downcast::<gtk4::Window>() {
            about.present(Some(&w));
            return;
        }
    }
    about.present(gtk4::Window::NONE);
}

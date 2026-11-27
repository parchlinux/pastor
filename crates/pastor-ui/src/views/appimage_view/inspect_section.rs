use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::glib;
use pastor_core::{
    format_size, install_appimage, launch_appimage, tr, AppImageMetadata, Package, PackageSource,
};

fn source_label(source: &PackageSource) -> &'static str {
    match source {
        PackageSource::Parch(_) | PackageSource::Arch(_) => tr("Official Repository"),
        PackageSource::Flatpak { .. } => tr("Flatpak"),
        PackageSource::Aur => tr("AUR"),
        _ => tr("Package"),
    }
}

pub fn populate_inspect_view(
    container: &gtk4::Box,
    appimage_path: PathBuf,
    meta: AppImageMetadata,
    alternatives: Vec<Package>,
    on_select_package: Rc<dyn Fn(Package)>,
    on_back_to_list: Rc<dyn Fn()>,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    // Header Card
    let header_card = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .css_classes(["card"])
        .build();

    let header_inner = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(14)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();

    let icon_widget = if let Some(ref ipath) = meta.icon_path {
        gtk4::Image::builder()
            .file(ipath.to_string_lossy().as_ref())
            .pixel_size(48)
            .build()
    } else {
        gtk4::Image::builder()
            .icon_name(crate::icons::appimage_icon())
            .pixel_size(48)
            .build()
    };
    header_inner.append(&icon_widget);

    let title_col = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(4)
        .hexpand(true)
        .valign(gtk4::Align::Center)
        .build();
    let title_lbl = gtk4::Label::builder()
        .label(&meta.name)
        .halign(gtk4::Align::Start)
        .wrap(true)
        .css_classes(["title-2"])
        .build();
    let ver_part = meta
        .version
        .as_deref()
        .map(|v| format!("{v} • "))
        .unwrap_or_default();
    let subtitle_lbl = gtk4::Label::builder()
        .label(format!(
            "{}{} • {}",
            ver_part,
            meta.appimage_type.label(),
            format_size(meta.file_size_bytes)
        ))
        .halign(gtk4::Align::Start)
        .wrap(true)
        .css_classes(["dim-label"])
        .build();
    title_col.append(&title_lbl);
    title_col.append(&subtitle_lbl);
    header_inner.append(&title_col);

    let back_btn = gtk4::Button::builder()
        .label(tr("All AppImages"))
        .valign(gtk4::Align::Center)
        .height_request(44)
        .css_classes(["flat"])
        .build();
    {
        let back_c = on_back_to_list.clone();
        back_btn.connect_clicked(move |_| back_c());
    }
    header_inner.append(&back_btn);
    header_card.append(&header_inner);
    container.append(&header_card);

    // Smart Alternatives Banner & List (if found in Official Repos, Flatpak, or AUR)
    if !alternatives.is_empty() {
        let alt_banner = adw::Banner::builder()
            .title(tr(
                "This application is available natively in Flatpak, AUR, or Official Repositories. Installing from those sources provides automatic updates and better system integration.",
            ))
            .revealed(true)
            .build();
        container.append(&alt_banner);

        let alt_group = adw::PreferencesGroup::builder()
            .title(tr("Recommended Native & Community Alternatives"))
            .description(tr(
                "You can install directly from these sources instead of using the AppImage.",
            ))
            .build();

        for alt_pkg in &alternatives {
            let row = adw::ActionRow::builder()
                .title(&alt_pkg.name)
                .subtitle(format!(
                    "{} • {} • {}",
                    source_label(&alt_pkg.id.source),
                    alt_pkg.version,
                    alt_pkg.summary
                ))
                .use_markup(false)
                .activatable(true)
                .build();

            let badge = gtk4::Label::builder()
                .label(source_label(&alt_pkg.id.source))
                .valign(gtk4::Align::Center)
                .css_classes(["caption", "accent"])
                .build();
            row.add_suffix(&badge);

            let view_btn = gtk4::Button::builder()
                .label(tr("View & Install"))
                .valign(gtk4::Align::Center)
                .height_request(44)
                .css_classes(["suggested-action"])
                .build();

            let pkg_c = alt_pkg.clone();
            let select_cb = on_select_package.clone();
            view_btn.connect_clicked(move |_| select_cb(pkg_c.clone()));

            let pkg_row_c = alt_pkg.clone();
            let select_row_cb = on_select_package.clone();
            row.connect_activated(move |_| select_row_cb(pkg_row_c.clone()));

            row.add_suffix(&view_btn);
            alt_group.add(&row);
        }

        container.append(&alt_group);
    }

    // AppImage Metadata Details
    let details_group = adw::PreferencesGroup::builder()
        .title(tr("AppImage Details"))
        .build();

    if let Some(ref comment) = meta.comment {
        let desc_row = adw::ActionRow::builder()
            .title(tr("Description"))
            .subtitle(comment)
            .subtitle_lines(4)
            .use_markup(false)
            .build();
        details_group.add(&desc_row);
    }

    let path_row = adw::ActionRow::builder()
        .title(tr("Source File"))
        .subtitle(appimage_path.to_string_lossy().as_ref())
        .use_markup(false)
        .build();
    details_group.add(&path_row);

    let dest_row = adw::ActionRow::builder()
        .title(tr("Install Location"))
        .subtitle(format!("~/Applications/{}.AppImage", meta.slug))
        .use_markup(false)
        .build();
    details_group.add(&dest_row);

    if !meta.categories.is_empty() {
        let cat_row = adw::ActionRow::builder()
            .title(tr("Categories"))
            .subtitle(meta.categories.join(", "))
            .use_markup(false)
            .build();
        details_group.add(&cat_row);
    }
    container.append(&details_group);

    let status_label = gtk4::Label::builder()
        .halign(gtk4::Align::Start)
        .wrap(true)
        .visible(false)
        .css_classes(["dim-label"])
        .build();
    container.append(&status_label);

    let actions_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(12)
        .halign(gtk4::Align::End)
        .margin_top(6)
        .build();

    let run_btn = gtk4::Button::builder()
        .label(tr("Run Once"))
        .height_request(44)
        .build();
    {
        let p = appimage_path.clone();
        let st = status_label.clone();
        run_btn.connect_clicked(move |_| {
            if let Err(e) = launch_appimage(&p) {
                st.set_label(&format!("{}: {}", tr("Launch failed"), e));
                st.set_visible(true);
            } else {
                st.set_label(tr("Application launched."));
                st.set_visible(true);
            }
        });
    }

    let install_btn = gtk4::Button::builder()
        .label(tr("Integrate & Install"))
        .height_request(44)
        .css_classes(["suggested-action"])
        .build();
    {
        let p = appimage_path;
        let st = status_label;
        let ib = install_btn.clone();
        let rb = run_btn.clone();
        let back = on_back_to_list;
        install_btn.connect_clicked(move |_| {
            ib.set_sensitive(false);
            rb.set_sensitive(false);
            st.set_label(tr("Integrating AppImage into ~/Applications..."));
            st.set_visible(true);

            let src_bg = p.clone();
            let st_ui = st.clone();
            let ib_ui = ib.clone();
            let rb_ui = rb.clone();
            let back_ui = back.clone();

            glib::MainContext::default().spawn_local(async move {
                let res = tokio::task::spawn_blocking(move || install_appimage(&src_bg)).await;
                ib_ui.set_sensitive(true);
                rb_ui.set_sensitive(true);
                match res {
                    Ok(Ok(_installed)) => {
                        back_ui();
                    }
                    Ok(Err(e)) => {
                        st_ui.set_label(&format!("{}: {}", tr("Installation failed"), e));
                    }
                    Err(e) => {
                        st_ui.set_label(&format!("{}: {}", tr("Installation failed"), e));
                    }
                }
            });
        });
    }

    actions_box.append(&run_btn);
    actions_box.append(&install_btn);
    container.append(&actions_box);
}

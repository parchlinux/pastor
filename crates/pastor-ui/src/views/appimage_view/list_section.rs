use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::{gio, glib};
use pastor_core::{
    format_size, launch_appimage, list_installed_appimages, tr, uninstall_appimage,
    update_appimage, AppImageOwner, InstalledAppImage,
};

pub fn open_appimage_file_dialog(
    parent_widget: &impl IsA<gtk4::Widget>,
    title: &str,
    on_picked: Rc<dyn Fn(PathBuf)>,
) {
    let filter = gtk4::FileFilter::new();
    filter.set_name(Some(tr("AppImage Applications (*.AppImage)")));
    filter.add_pattern("*.AppImage");
    filter.add_pattern("*.appimage");
    filter.add_mime_type("application/vnd.appimage");
    filter.add_mime_type("application/x-iso9660-appimage");

    let filters = gio::ListStore::new::<gtk4::FileFilter>();
    filters.append(&filter);

    let dialog = gtk4::FileDialog::builder()
        .title(title)
        .modal(true)
        .filters(&filters)
        .default_filter(&filter)
        .build();

    let win = parent_widget
        .root()
        .and_then(|r| r.downcast::<gtk4::Window>().ok());

    dialog.open(win.as_ref(), gio::Cancellable::NONE, move |res| {
        if let Ok(file) = res {
            if let Some(path) = file.path() {
                on_picked(path);
            }
        }
    });
}

fn owner_badge_label(owner: AppImageOwner) -> &'static str {
    match owner {
        AppImageOwner::Pastor => tr("Pastor"),
        AppImageOwner::AppManager => tr("AppManager"),
        AppImageOwner::External => tr("Local"),
    }
}

pub fn populate_installed_list(
    container: &gtk4::Box,
    status_label: &gtk4::Label,
    on_open_new: Rc<dyn Fn(PathBuf)>,
    on_refresh: Rc<dyn Fn()>,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    // Header Card with "Open .AppImage File..." button
    let header_group = adw::PreferencesGroup::builder()
        .title(tr("AppImage Manager"))
        .description(tr(
            "Manage integrated AppImages (including those installed with AppManager), or open a new .AppImage file to inspect, run, or install it.",
        ))
        .build();

    let open_row = adw::ActionRow::builder()
        .title(tr("Install or Run a New .AppImage"))
        .subtitle(tr(
            "Checks Official Repositories, Flatpak, and AUR for native alternatives before integrating.",
        ))
        .use_markup(false)
        .activatable(true)
        .build();

    let pick_btn = gtk4::Button::builder()
        .label(tr("Open .AppImage..."))
        .valign(gtk4::Align::Center)
        .height_request(44)
        .css_classes(["suggested-action"])
        .build();
    {
        let on_open = on_open_new.clone();
        let btn_c = pick_btn.clone();
        pick_btn.connect_clicked(move |_| {
            open_appimage_file_dialog(&btn_c, tr("Select AppImage File"), on_open.clone());
        });
    }
    {
        let on_open = on_open_new;
        let row_c = open_row.clone();
        open_row.connect_activated(move |_| {
            open_appimage_file_dialog(&row_c, tr("Select AppImage File"), on_open.clone());
        });
    }
    open_row.add_suffix(&pick_btn);
    header_group.add(&open_row);
    container.append(&header_group);

    let items = list_installed_appimages();
    let list_group = adw::PreferencesGroup::builder()
        .title(tr("Installed AppImages"))
        .description(format!("{}: {}", tr("Found"), items.len()))
        .build();

    if items.is_empty() {
        let empty_row = adw::ActionRow::builder()
            .title(tr("No Installed AppImages Found"))
            .subtitle(tr(
                "AppImages installed via Pastor or AppManager in ~/Applications will appear here.",
            ))
            .use_markup(false)
            .build();
        list_group.add(&empty_row);
    } else {
        for item in items {
            list_group.add(&build_installed_row(item, status_label, on_refresh.clone()));
        }
    }

    container.append(&list_group);
}

fn build_installed_row(
    item: InstalledAppImage,
    status_label: &gtk4::Label,
    on_refresh: Rc<dyn Fn()>,
) -> adw::ActionRow {
    let ver_str = item
        .version
        .as_deref()
        .map(|v| format!("{v} • "))
        .unwrap_or_default();
    let sub = format!(
        "{}{} • {}",
        ver_str,
        owner_badge_label(item.owner),
        format_size(item.file_size_bytes)
    );

    let row = adw::ActionRow::builder()
        .title(&item.name)
        .subtitle(&sub)
        .use_markup(false)
        .build();

    let icon_widget = if let Some(ref ipath) = item.icon_path {
        gtk4::Image::builder()
            .file(ipath.to_string_lossy().as_ref())
            .pixel_size(32)
            .valign(gtk4::Align::Center)
            .build()
    } else {
        gtk4::Image::builder()
            .icon_name(crate::icons::appimage_icon())
            .pixel_size(32)
            .valign(gtk4::Align::Center)
            .build()
    };
    row.add_prefix(&icon_widget);

    // Launch button
    let launch_btn = gtk4::Button::builder()
        .icon_name("media-playback-start-symbolic")
        .tooltip_text(tr("Launch"))
        .valign(gtk4::Align::Center)
        .height_request(44)
        .css_classes(["flat"])
        .build();
    {
        let exec = item.exec_path.clone();
        let st = status_label.clone();
        launch_btn.connect_clicked(move |_| {
            if let Err(e) = launch_appimage(&exec) {
                st.set_label(&format!("{}: {}", tr("Launch failed"), e));
                st.set_visible(true);
            } else {
                st.set_visible(false);
            }
        });
    }

    // Update / Replace button
    let update_btn = gtk4::Button::builder()
        .icon_name("software-update-available-symbolic")
        .tooltip_text(tr("Update with newer .AppImage..."))
        .valign(gtk4::Align::Center)
        .height_request(44)
        .css_classes(["flat"])
        .build();
    {
        let item_u = item.clone();
        let st = status_label.clone();
        let refresh = on_refresh.clone();
        update_btn.connect_clicked(move |btn| {
            let item_c = item_u.clone();
            let st_c = st.clone();
            let ref_c = refresh.clone();
            open_appimage_file_dialog(
                btn,
                tr("Select Newer .AppImage"),
                Rc::new(move |new_path| {
                    st_c.set_label(tr("Updating AppImage..."));
                    st_c.set_visible(true);
                    let item_bg = item_c.clone();
                    let st_ui = st_c.clone();
                    let ref_ui = ref_c.clone();
                    glib::MainContext::default().spawn_local(async move {
                        let res = tokio::task::spawn_blocking(move || {
                            update_appimage(&item_bg, &new_path)
                        })
                        .await;
                        match res {
                            Ok(Ok(updated)) => {
                                st_ui.set_label(&format!(
                                    "{}: {}",
                                    tr("Updated AppImage"),
                                    updated.name
                                ));
                                ref_ui();
                            }
                            Ok(Err(e)) => {
                                st_ui.set_label(&format!("{}: {}", tr("Update failed"), e));
                            }
                            Err(e) => {
                                st_ui.set_label(&format!("{}: {}", tr("Update failed"), e));
                            }
                        }
                    });
                }),
            );
        });
    }

    // Uninstall button with confirmation dialog
    let remove_btn = gtk4::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text(tr("Uninstall"))
        .valign(gtk4::Align::Center)
        .height_request(44)
        .css_classes(["flat", "destructive-action"])
        .build();
    {
        let item_r = item.clone();
        let st = status_label.clone();
        let refresh = on_refresh;
        remove_btn.connect_clicked(move |btn| {
            let win = btn.root().and_then(|r| r.downcast::<gtk4::Window>().ok());
            let dlg = adw::AlertDialog::builder()
                .heading(tr("Uninstall AppImage?"))
                .body(format!(
                    "{} \"{}\"",
                    tr("This will remove the AppImage file and its desktop launcher for"),
                    item_r.name
                ))
                .build();
            dlg.add_response("cancel", tr("Cancel"));
            dlg.add_response("remove", tr("Uninstall"));
            dlg.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
            dlg.set_default_response(Some("cancel"));

            let item_bg = item_r.clone();
            let st_ui = st.clone();
            let ref_ui = refresh.clone();
            dlg.connect_response(None, move |_, resp| {
                if resp == "remove" {
                    match uninstall_appimage(&item_bg) {
                        Ok(()) => {
                            st_ui.set_label(&format!(
                                "{}: {}",
                                tr("Removed AppImage"),
                                item_bg.name
                            ));
                            st_ui.set_visible(true);
                            ref_ui();
                        }
                        Err(e) => {
                            st_ui.set_label(&format!("{}: {}", tr("Uninstall failed"), e));
                            st_ui.set_visible(true);
                        }
                    }
                }
            });
            dlg.present(win.as_ref());
        });
    }

    row.add_suffix(&launch_btn);
    row.add_suffix(&update_btn);
    row.add_suffix(&remove_btn);
    row
}

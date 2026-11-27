use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk4::{gio, glib};
use pastor_core::{
    convert_deb_to_pkg, format_size, host_arch, inspect_deb, tr, DebMetadata, Package,
    PackageSource, TransactionStep,
};
use pastor_store::Store;

#[derive(Clone)]
pub struct DebViewHandle {
    pub root: gtk4::Box,
    load_fn: Rc<dyn Fn(PathBuf)>,
}

impl DebViewHandle {
    pub fn load_file(&self, path: PathBuf) {
        (self.load_fn)(path);
    }
}

fn source_label(source: &PackageSource) -> &'static str {
    match source {
        PackageSource::Parch(_) | PackageSource::Arch(_) => tr("Official Repository"),
        PackageSource::Flatpak { .. } => tr("Flatpak"),
        PackageSource::Aur => tr("AUR"),
        _ => tr("Package"),
    }
}

fn open_deb_file_dialog(parent_widget: &impl IsA<gtk4::Widget>, on_picked: Rc<dyn Fn(PathBuf)>) {
    let filter = gtk4::FileFilter::new();
    filter.set_name(Some(tr("Debian Packages (*.deb)")));
    filter.add_pattern("*.deb");
    filter.add_mime_type("application/vnd.debian.binary-package");

    let filters = gio::ListStore::new::<gtk4::FileFilter>();
    filters.append(&filter);

    let dialog = gtk4::FileDialog::builder()
        .title(tr("Select Debian Package (.deb)"))
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

fn save_pkg_file_dialog(
    parent_widget: &impl IsA<gtk4::Widget>,
    suggested_name: &str,
    on_picked: Rc<dyn Fn(PathBuf)>,
) {
    let dialog = gtk4::FileDialog::builder()
        .title(tr("Save Converted Arch Package"))
        .initial_name(suggested_name)
        .modal(true)
        .build();

    let win = parent_widget
        .root()
        .and_then(|r| r.downcast::<gtk4::Window>().ok());

    dialog.save(win.as_ref(), gio::Cancellable::NONE, move |res| {
        if let Ok(file) = res {
            if let Some(path) = file.path() {
                on_picked(path);
            }
        }
    });
}

pub fn create_deb_view(
    store: Store,
    on_select_package: Rc<dyn Fn(Package)>,
) -> DebViewHandle {
    let root = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    let stack = gtk4::Stack::builder()
        .transition_type(gtk4::StackTransitionType::Crossfade)
        .hexpand(true)
        .vexpand(true)
        .build();

    // 1. Empty / File Picker State
    let empty_page = adw::StatusPage::builder()
        .icon_name(crate::icons::deb_icon())
        .title(tr("Debian Package (.deb) Installer"))
        .description(tr(
            "Open a .deb file to check for native Official, Flatpak, or AUR alternatives first, or convert and install it cleanly on ParchLinux.",
        ))
        .build();

    let pick_btn = gtk4::Button::builder()
        .label(tr("Choose .deb File..."))
        .halign(gtk4::Align::Center)
        .height_request(44)
        .css_classes(["suggested-action", "pill"])
        .build();
    empty_page.set_child(Some(&pick_btn));
    stack.add_named(&empty_page, Some("empty"));

    // 2. Loading State
    let loading_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(14)
        .valign(gtk4::Align::Center)
        .halign(gtk4::Align::Center)
        .margin_top(32)
        .margin_bottom(32)
        .margin_start(24)
        .margin_end(24)
        .build();
    let spinner = gtk4::Spinner::builder()
        .spinning(true)
        .width_request(36)
        .height_request(36)
        .build();
    let loading_label = gtk4::Label::builder()
        .label(tr("Inspecting .deb package and searching Flatpak, AUR, and Official Repositories..."))
        .wrap(true)
        .justify(gtk4::Justification::Center)
        .css_classes(["title-4"])
        .build();
    loading_box.append(&spinner);
    loading_box.append(&loading_label);
    stack.add_named(&loading_box, Some("loading"));

    // 3. Error State
    let error_page = adw::StatusPage::builder()
        .icon_name("dialog-error-symbolic")
        .title(tr("Could Not Open .deb Package"))
        .build();
    let error_retry_btn = gtk4::Button::builder()
        .label(tr("Choose Another .deb File..."))
        .halign(gtk4::Align::Center)
        .height_request(44)
        .css_classes(["pill"])
        .build();
    error_page.set_child(Some(&error_retry_btn));
    stack.add_named(&error_page, Some("error"));

    // 4. Content State (Loaded .deb + Smart Alternatives)
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .hexpand(true)
        .build();
    let clamp = adw::Clamp::builder()
        .maximum_size(760)
        .tightening_threshold(500)
        .margin_top(20)
        .margin_bottom(28)
        .margin_start(16)
        .margin_end(16)
        .build();
    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(18)
        .build();
    clamp.set_child(Some(&content_box));
    scrolled.set_child(Some(&clamp));
    stack.add_named(&scrolled, Some("content"));

    stack.set_visible_child_name("empty");
    root.append(&stack);

    let current_request_id: Rc<RefCell<u64>> = Rc::new(RefCell::new(0));
    let load_fn_slot: Rc<RefCell<Option<Rc<dyn Fn(PathBuf)>>>> = Rc::new(RefCell::new(None));

    let trigger_load: Rc<dyn Fn(PathBuf)> = {
        let store = store.clone();
        let stack = stack.clone();
        let content_box = content_box.clone();
        let error_page = error_page.clone();
        let on_select_package = on_select_package.clone();
        let req_id_cell = current_request_id.clone();
        let load_fn_slot_c = load_fn_slot.clone();

        Rc::new(move |deb_path: PathBuf| {
            let next_id = {
                let mut g = req_id_cell.borrow_mut();
                *g += 1;
                *g
            };

            stack.set_visible_child_name("loading");

            let store_c = store.clone();
            let stack_c = stack.clone();
            let content_box_c = content_box.clone();
            let error_page_c = error_page.clone();
            let on_select_c = on_select_package.clone();
            let req_id_c = req_id_cell.clone();
            let load_slot_c = load_fn_slot_c.clone();

            glib::MainContext::default().spawn_local(async move {
                let deb_path_c = deb_path.clone();
                let inspect_res = tokio::task::spawn_blocking(move || inspect_deb(&deb_path_c)).await;

                if *req_id_c.borrow() != next_id {
                    return;
                }

                let meta: DebMetadata = match inspect_res {
                    Ok(Ok(m)) => m,
                    Ok(Err(e)) => {
                        error_page_c.set_description(Some(&e.to_string()));
                        stack_c.set_visible_child_name("error");
                        return;
                    }
                    Err(e) => {
                        error_page_c.set_description(Some(&e.to_string()));
                        stack_c.set_visible_child_name("error");
                        return;
                    }
                };

                // Run smart alternative discovery & ALPM dependency resolution concurrently
                let (alternatives, (resolved_deps, unresolved_deps)) = tokio::join!(
                    store_c.find_deb_alternatives(&meta),
                    store_c.resolve_deb_dependencies(&meta.depends),
                );

                if *req_id_c.borrow() != next_id {
                    return;
                }

                while let Some(child) = content_box_c.first_child() {
                    content_box_c.remove(&child);
                }

                // Header Card
                let header_card = gtk4::Box::builder()
                    .orientation(gtk4::Orientation::Vertical)
                    .spacing(12)
                    .css_classes(["card"])
                    .margin_bottom(4)
                    .build();

                let header_inner = gtk4::Box::builder()
                    .orientation(gtk4::Orientation::Horizontal)
                    .spacing(14)
                    .margin_top(16)
                    .margin_bottom(16)
                    .margin_start(16)
                    .margin_end(16)
                    .build();

                let pkg_icon = gtk4::Image::builder()
                    .icon_name(crate::icons::deb_icon())
                    .pixel_size(48)
                    .build();
                header_inner.append(&pkg_icon);

                let title_col = gtk4::Box::builder()
                    .orientation(gtk4::Orientation::Vertical)
                    .spacing(4)
                    .hexpand(true)
                    .valign(gtk4::Align::Center)
                    .build();
                let title_lbl = gtk4::Label::builder()
                    .label(&meta.package)
                    .halign(gtk4::Align::Start)
                    .wrap(true)
                    .css_classes(["title-2"])
                    .build();
                let subtitle_lbl = gtk4::Label::builder()
                    .label(format!(
                        "{} • {} • {}",
                        meta.version,
                        meta.arch_architecture,
                        format_size(meta.file_size_bytes)
                    ))
                    .halign(gtk4::Align::Start)
                    .wrap(true)
                    .css_classes(["dim-label"])
                    .build();
                title_col.append(&title_lbl);
                title_col.append(&subtitle_lbl);
                header_inner.append(&title_col);

                let open_other_btn = gtk4::Button::builder()
                    .label(tr("Open Another..."))
                    .valign(gtk4::Align::Center)
                    .height_request(44)
                    .css_classes(["flat"])
                    .build();
                if let Some(ref f) = *load_slot_c.borrow() {
                    let f_c = f.clone();
                    let btn_c = open_other_btn.clone();
                    open_other_btn.connect_clicked(move |_| {
                        open_deb_file_dialog(&btn_c, f_c.clone());
                    });
                }
                header_inner.append(&open_other_btn);
                header_card.append(&header_inner);
                content_box_c.append(&header_card);

                // Smart Alternative Recommendation Section (if found in Official Repos, Flatpak, or AUR)
                if !alternatives.is_empty() {
                    let alt_banner = adw::Banner::builder()
                        .title(tr(
                            "This application is available natively in Flatpak, AUR, or Official Repositories. Installing from those sources is recommended for automatic updates and better system integration.",
                        ))
                        .revealed(true)
                        .build();
                    content_box_c.append(&alt_banner);

                    let alt_group = adw::PreferencesGroup::builder()
                        .title(tr("Recommended Native & Community Alternatives"))
                        .description(tr(
                            "You can install directly from these sources instead of converting the .deb file.",
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
                        let select_cb = on_select_c.clone();
                        view_btn.connect_clicked(move |_| {
                            select_cb(pkg_c.clone());
                        });

                        let pkg_row_c = alt_pkg.clone();
                        let select_row_cb = on_select_c.clone();
                        row.connect_activated(move |_| {
                            select_row_cb(pkg_row_c.clone());
                        });

                        row.add_suffix(&view_btn);
                        alt_group.add(&row);
                    }

                    content_box_c.append(&alt_group);
                }

                // Warnings (Architecture mismatch or Debian maintainer scripts)
                let current_host = host_arch();
                if meta.arch_architecture != "any" && meta.arch_architecture != current_host {
                    let arch_banner = adw::Banner::builder()
                        .title(format!(
                            "{}: {} (host: {})",
                            tr("Architecture mismatch"),
                            meta.arch_architecture,
                            current_host
                        ))
                        .revealed(true)
                        .build();
                    content_box_c.append(&arch_banner);
                }

                if meta.has_maintainer_scripts {
                    let script_row = adw::ActionRow::builder()
                        .title(tr("Debian Maintainer Scripts Detected"))
                        .subtitle(tr(
                            "Debian-specific postinst/prerm scripts are omitted for safety. Standard Arch ALPM hooks will handle desktop entries and icon caches automatically.",
                        ))
                        .build();
                    let warn_icon = gtk4::Image::builder()
                        .icon_name("dialog-warning-symbolic")
                        .valign(gtk4::Align::Center)
                        .build();
                    script_row.add_prefix(&warn_icon);

                    let notice_group = adw::PreferencesGroup::new();
                    notice_group.add(&script_row);
                    content_box_c.append(&notice_group);
                }

                // Package Metadata Details
                let details_group = adw::PreferencesGroup::builder()
                    .title(tr("Debian Package Details"))
                    .build();

                let desc_text = match &meta.long_description {
                    Some(long) => format!("{}\n\n{}", meta.description, long),
                    None => meta.description.clone(),
                };
                let desc_row = adw::ActionRow::builder()
                    .title(tr("Description"))
                    .subtitle(desc_text)
                    .subtitle_lines(6)
                    .use_markup(false)
                    .build();
                details_group.add(&desc_row);

                let target_row = adw::ActionRow::builder()
                    .title(tr("Converted Arch Package"))
                    .subtitle(format!("{} ({})", meta.arch_pkgname, meta.arch_pkgver))
                    .use_markup(false)
                    .build();
                details_group.add(&target_row);

                if let Some(ref maint) = meta.maintainer {
                    let maint_row = adw::ActionRow::builder()
                        .title(tr("Maintainer"))
                        .subtitle(maint)
                        .use_markup(false)
                        .build();
                    details_group.add(&maint_row);
                }

                if let Some(ref url) = meta.homepage {
                    let url_row = adw::ActionRow::builder()
                        .title(tr("Homepage"))
                        .subtitle(url)
                        .use_markup(false)
                        .build();
                    details_group.add(&url_row);
                }

                // Dependency Mapping Expander
                let dep_expander = adw::ExpanderRow::builder()
                    .title(tr("Mapped Arch Dependencies"))
                    .subtitle(format!(
                        "{}: {} | {}: {}",
                        tr("Resolved"),
                        resolved_deps.len(),
                        tr("Skipped"),
                        unresolved_deps.len()
                    ))
                    .use_markup(false)
                    .build();

                if resolved_deps.is_empty() && unresolved_deps.is_empty() {
                    let none_row = adw::ActionRow::builder()
                        .title(tr("No external dependencies declared"))
                        .use_markup(false)
                        .build();
                    dep_expander.add_row(&none_row);
                } else {
                    for dep in &resolved_deps {
                        let r = adw::ActionRow::builder()
                            .title(dep)
                            .subtitle(tr("Available in Arch/Parch repositories"))
                            .use_markup(false)
                            .build();
                        let ok_icon = gtk4::Image::builder()
                            .icon_name("emblem-ok-symbolic")
                            .build();
                        r.add_prefix(&ok_icon);
                        dep_expander.add_row(&r);
                    }
                    for udep in &unresolved_deps {
                        let r = adw::ActionRow::builder()
                            .title(udep)
                            .subtitle(tr("Debian-specific or unavailable in sync databases (skipped)"))
                            .use_markup(false)
                            .build();
                        let skip_icon = gtk4::Image::builder()
                            .icon_name("dialog-information-symbolic")
                            .build();
                        r.add_prefix(&skip_icon);
                        dep_expander.add_row(&r);
                    }
                }
                details_group.add(&dep_expander);
                content_box_c.append(&details_group);

                // Action Status Label + Buttons
                let status_label = gtk4::Label::builder()
                    .halign(gtk4::Align::Start)
                    .wrap(true)
                    .visible(false)
                    .css_classes(["dim-label"])
                    .build();
                content_box_c.append(&status_label);

                let actions_box = gtk4::Box::builder()
                    .orientation(gtk4::Orientation::Horizontal)
                    .spacing(12)
                    .halign(gtk4::Align::End)
                    .margin_top(6)
                    .build();

                let save_btn = gtk4::Button::builder()
                    .label(tr("Save as .pkg.tar.zst"))
                    .height_request(44)
                    .build();

                let install_label = if alternatives.is_empty() {
                    tr("Convert & Install")
                } else {
                    tr("Convert & Install .deb Anyway")
                };
                let install_btn = gtk4::Button::builder()
                    .label(install_label)
                    .height_request(44)
                    .css_classes(["suggested-action"])
                    .build();

                // Save Only Handler
                {
                    let deb_path_s = deb_path.clone();
                    let deps_s = resolved_deps.clone();
                    let save_btn_c = save_btn.clone();
                    let install_btn_c = install_btn.clone();
                    let status_c = status_label.clone();
                    let meta_s = meta.clone();

                    save_btn.connect_clicked(move |btn| {
                        let suggested = format!(
                            "{}-{}-{}.pkg.tar.zst",
                            meta_s.arch_pkgname,
                            meta_s.arch_pkgver.replace(':', "_"),
                            meta_s.arch_architecture
                        );
                        let deb_p = deb_path_s.clone();
                        let deps_c = deps_s.clone();
                        let sb = save_btn_c.clone();
                        let ib = install_btn_c.clone();
                        let st = status_c.clone();

                        save_pkg_file_dialog(
                            btn,
                            &suggested,
                            Rc::new(move |dest_path| {
                                sb.set_sensitive(false);
                                ib.set_sensitive(false);
                                st.set_label(tr("Converting .deb to .pkg.tar.zst..."));
                                st.set_visible(true);

                                let deb_inner = deb_p.clone();
                                let deps_inner = deps_c.clone();
                                let sb_i = sb.clone();
                                let ib_i = ib.clone();
                                let st_i = st.clone();

                                glib::MainContext::default().spawn_local(async move {
                                    let out_dir = dest_path
                                        .parent()
                                        .map(|p| p.to_path_buf())
                                        .unwrap_or_else(std::env::temp_dir);
                                    let res = tokio::task::spawn_blocking(move || {
                                        let built = convert_deb_to_pkg(&deb_inner, &out_dir, &deps_inner)?;
                                        if built != dest_path {
                                            std::fs::rename(&built, &dest_path).map_err(|e| {
                                                pastor_core::PastorError::InvalidOperation(format!(
                                                    "Failed saving package to destination: {e}"
                                                ))
                                            })?;
                                        }
                                        Ok::<PathBuf, pastor_core::PastorError>(dest_path)
                                    })
                                    .await;

                                    sb_i.set_sensitive(true);
                                    ib_i.set_sensitive(true);

                                    match res {
                                        Ok(Ok(saved)) => {
                                            st_i.set_label(&format!(
                                                "{}: {}",
                                                tr("Saved Arch package"),
                                                saved.display()
                                            ));
                                        }
                                        Ok(Err(e)) => {
                                            st_i.set_label(&format!("{}: {}", tr("Conversion failed"), e));
                                        }
                                        Err(e) => {
                                            st_i.set_label(&format!("{}: {}", tr("Conversion failed"), e));
                                        }
                                    }
                                });
                            }),
                        );
                    });
                }

                // Convert & Install Handler
                {
                    let deb_path_i = deb_path.clone();
                    let deps_i = resolved_deps.clone();
                    let store_i = store_c.clone();
                    let save_btn_c = save_btn.clone();
                    let install_btn_c = install_btn.clone();
                    let status_c = status_label.clone();
                    let pkgname_i = meta.arch_pkgname.clone();

                    install_btn.connect_clicked(move |_| {
                        save_btn_c.set_sensitive(false);
                        install_btn_c.set_sensitive(false);
                        status_c.set_label(tr("Converting .deb package to Arch .pkg.tar.zst..."));
                        status_c.set_visible(true);

                        let deb_inner = deb_path_i.clone();
                        let deps_inner = deps_i.clone();
                        let store_inner = store_i.clone();
                        let sb_i = save_btn_c.clone();
                        let ib_i = install_btn_c.clone();
                        let st_i = status_c.clone();
                        let pname = pkgname_i.clone();

                        glib::MainContext::default().spawn_local(async move {
                            let build_dir = pastor_core::pastor_cache_dir().join("deb-builds");
                            let conv_res = tokio::task::spawn_blocking(move || {
                                convert_deb_to_pkg(&deb_inner, &build_dir, &deps_inner)
                            })
                            .await;

                            let pkg_path = match conv_res {
                                Ok(Ok(p)) => p,
                                Ok(Err(e)) => {
                                    st_i.set_label(&format!("{}: {}", tr("Conversion failed"), e));
                                    sb_i.set_sensitive(true);
                                    ib_i.set_sensitive(true);
                                    return;
                                }
                                Err(e) => {
                                    st_i.set_label(&format!("{}: {}", tr("Conversion failed"), e));
                                    sb_i.set_sensitive(true);
                                    ib_i.set_sensitive(true);
                                    return;
                                }
                            };

                            st_i.set_label(tr("Installing converted package..."));
                            let mut rx = store_inner.install_local_package(pkg_path.clone(), pname);
                            while let Some(evt) = rx.recv().await {
                                match evt.step {
                                    TransactionStep::Completed => {
                                        st_i.set_label(tr("Package converted and installed successfully."));
                                        break;
                                    }
                                    TransactionStep::Failed(err) => {
                                        st_i.set_label(&format!("{}: {}", tr("Installation failed"), err));
                                        break;
                                    }
                                    _ => {
                                        st_i.set_label(&evt.log_message);
                                    }
                                }
                            }

                            let _ = std::fs::remove_file(&pkg_path);
                            sb_i.set_sensitive(true);
                            ib_i.set_sensitive(true);
                        });
                    });
                }

                actions_box.append(&save_btn);
                actions_box.append(&install_btn);
                content_box_c.append(&actions_box);

                stack_c.set_visible_child_name("content");
            });
        })
    };

    *load_fn_slot.borrow_mut() = Some(trigger_load.clone());

    {
        let trigger = trigger_load.clone();
        let btn_c = pick_btn.clone();
        pick_btn.connect_clicked(move |_| {
            open_deb_file_dialog(&btn_c, trigger.clone());
        });
    }

    {
        let trigger = trigger_load.clone();
        let btn_c = error_retry_btn.clone();
        error_retry_btn.connect_clicked(move |_| {
            open_deb_file_dialog(&btn_c, trigger.clone());
        });
    }

    DebViewHandle {
        root,
        load_fn: trigger_load,
    }
}

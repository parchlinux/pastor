use adw::prelude::*;
use pastor_core::{Package, PackageCategory, PackageIcon, PackageSource, PackageState, ParchRepoType};
use pastor_store::{ActiveTransactionEvent, Store};
use super::circular_progress::CircularProgress;

pub fn create_package_details_view(
    pkg: Package,
    store: Store,
    on_select_package: impl Fn(Package) + 'static + Clone,
    on_downgrade: impl Fn(String) + 'static + Clone,
    on_transaction_start: impl Fn(tokio::sync::mpsc::Receiver<pastor_core::TransactionEvent>) + 'static + Clone,
) -> gtk4::Widget {
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build();

    let clamp = adw::Clamp::builder()
        .maximum_size(860)
        .tightening_threshold(620)
        .build();

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(28)
        .margin_top(20)
        .margin_bottom(48)
        .margin_start(24)
        .margin_end(24)
        .build();

    // -------------------------------------------------------------
    // 1. GNOME HIG Hero Section (Icon, Title, Publisher, Badges, Actions)
    // -------------------------------------------------------------
    let hero_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(24)
        .margin_top(8)
        .margin_bottom(6)
        .build();

    // App Icon (clean, native display with subtle shadow)
    let icon_container = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .valign(gtk4::Align::Center)
        .css_classes(["app-icon-hero"])
        .build();

    let icon = match &pkg.icon {
        Some(PackageIcon::LocalPath(path)) => {
            let img = gtk4::Image::from_file(path);
            img.set_pixel_size(100);
            img
        }
        Some(PackageIcon::Themed(name)) => gtk4::Image::builder()
            .icon_name(name)
            .pixel_size(100)
            .build(),
        _ => gtk4::Image::builder()
            .icon_name("application-x-executable")
            .pixel_size(100)
            .build(),
    };
    icon_container.append(&icon);
    hero_box.append(&icon_container);

    // Metadata: Title, Publisher, Badges
    let meta_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .hexpand(true)
        .valign(gtk4::Align::Center)
        .build();

    let title_label = gtk4::Label::builder()
        .label(pkg.display_title())
        .css_classes(["title-1"])
        .halign(gtk4::Align::Start)
        .build();
    meta_box.append(&title_label);

    let dev_name = pkg.maintainer.as_deref().unwrap_or("ParchLinux Contributors");
    let lic_str = pkg.license.as_deref().unwrap_or("Open Source");
    let meta_subtitle = format!("{} • v{} • {}", dev_name, pkg.version, lic_str);

    let developer_label = gtk4::Label::builder()
        .label(&meta_subtitle)
        .css_classes(["caption", "dim-label"])
        .halign(gtk4::Align::Start)
        .build();
    meta_box.append(&developer_label);

    // Badges Row (Repository, Sandbox/Safety, Category)
    let badges_row = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(8)
        .margin_top(6)
        .build();

    let (repo_badge_text, repo_badge_class) = match &pkg.id.source {
        PackageSource::Parch(ParchRepoType::World) => ("Parch [world]", "parch-badge-world"),
        PackageSource::Parch(ParchRepoType::Void) => ("Parch [void]", "parch-badge-void"),
        PackageSource::Arch(_) => ("Arch Repo", "parch-badge-arch"),
        PackageSource::Aur => ("AUR", "parch-badge-aur"),
        PackageSource::Flatpak { .. } => ("Flatpak", "parch-badge-flatpak"),
        PackageSource::Bootc => ("bootc OS", "parch-badge-bootc"),
        PackageSource::Waydroid => ("Waydroid", "parch-badge-waydroid"),
    };

    let repo_badge = gtk4::Label::builder()
        .label(repo_badge_text)
        .css_classes([repo_badge_class])
        .valign(gtk4::Align::Center)
        .build();
    badges_row.append(&repo_badge);

    // Safety badge
    let (safety_text, safety_class) = match &pkg.id.source {
        PackageSource::Flatpak { .. } => ("Sandboxed", "metadata-badge-accent"),
        PackageSource::Bootc => ("Immutable Root", "metadata-badge-accent"),
        _ => ("Snapper Protected", "metadata-badge"),
    };
    let safety_badge = gtk4::Label::builder()
        .label(safety_text)
        .css_classes([safety_class])
        .valign(gtk4::Align::Center)
        .build();
    badges_row.append(&safety_badge);

    if let Some(first_cat) = pkg.categories.first() {
        let cat_badge = gtk4::Label::builder()
            .label(first_cat.title())
            .css_classes(["metadata-badge"])
            .valign(gtk4::Align::Center)
            .build();
        badges_row.append(&cat_badge);
    }

    meta_box.append(&badges_row);

    // Multi-source switcher: Check if alternative package sources exist (e.g. Flatpak and Arch repo)
    let source_switcher_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(8)
        .margin_top(4)
        .visible(false)
        .build();

    let source_lbl = gtk4::Label::builder()
        .label("Source:")
        .css_classes(["caption", "dim-label"])
        .valign(gtk4::Align::Center)
        .build();
    source_switcher_box.append(&source_lbl);

    let source_dropdown = gtk4::DropDown::builder()
        .valign(gtk4::Align::Center)
        .build();
    source_switcher_box.append(&source_dropdown);
    meta_box.append(&source_switcher_box);

    let store_alt = store.clone();
    let current_pkg_clone = pkg.clone();
    let switcher_box_clone = source_switcher_box.clone();
    let dropdown_clone = source_dropdown.clone();
    let on_sel_alt = on_select_package.clone();

    gtk4::glib::spawn_future_local(async move {
        if let Ok(mut alts) = store_alt.get_alternatives(&current_pkg_clone.name).await {
            if !alts.iter().any(|p| p.id == current_pkg_clone.id) {
                alts.insert(0, current_pkg_clone.clone());
            }

            let mut titles: Vec<String> = Vec::new();
            let mut unique_alts: Vec<Package> = Vec::new();
            let mut seen_sources = std::collections::HashSet::new();
            let mut selected_idx = 0;

            for alt_p in alts {
                let source_desc = match &alt_p.id.source {
                    PackageSource::Parch(ParchRepoType::World) => "Parch [world]".to_string(),
                    PackageSource::Parch(ParchRepoType::Void) => "Parch [void]".to_string(),
                    PackageSource::Arch(r) => format!("Arch ({r})"),
                    PackageSource::Aur => "AUR (Community)".to_string(),
                    PackageSource::Flatpak { remote } => format!("Flatpak ({remote})"),
                    PackageSource::Bootc => "bootc (Base OS)".to_string(),
                    PackageSource::Waydroid => "Waydroid (Android)".to_string(),
                };

                let source_key = match &alt_p.id.source {
                    PackageSource::Flatpak { .. } => "flatpak".to_string(),
                    PackageSource::Arch(_) => "arch".to_string(),
                    PackageSource::Parch(_) => "parch".to_string(),
                    PackageSource::Aur => "aur".to_string(),
                    PackageSource::Bootc => "bootc".to_string(),
                    PackageSource::Waydroid => "waydroid".to_string(),
                };

                if seen_sources.insert(source_key) {
                    let label = format!("{} — v{}", source_desc, alt_p.version);
                    if alt_p.id == current_pkg_clone.id {
                        selected_idx = unique_alts.len();
                    }
                    titles.push(label);
                    unique_alts.push(alt_p);
                }
            }

            if unique_alts.len() > 1 {
                let str_slice: Vec<&str> = titles.iter().map(|s| s.as_str()).collect();
                let str_list = gtk4::StringList::new(&str_slice);
                dropdown_clone.set_model(Some(&str_list));
                dropdown_clone.set_selected(selected_idx as u32);
                switcher_box_clone.set_visible(true);

                let alts_rc = std::rc::Rc::new(unique_alts);
                let on_sel = on_sel_alt.clone();
                let current_id = current_pkg_clone.id.clone();
                dropdown_clone.connect_selected_notify(move |dd| {
                    let sel = dd.selected() as usize;
                    if let Some(target_pkg) = alts_rc.get(sel) {
                        if target_pkg.id != current_id {
                            on_sel(target_pkg.clone());
                        }
                    }
                });
            } else {
                switcher_box_clone.set_visible(false);
            }
        }
    });

    hero_box.append(&meta_box);

    // Primary & Secondary Action Controls
    let actions_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(10)
        .valign(gtk4::Align::Center)
        .halign(gtk4::Align::End)
        .build();

    let pkg_id = pkg.id.clone();
    let store_clone = store.clone();
    let on_tx_start = on_transaction_start.clone();
    let state = pkg.state;

    let render_actions_holder: std::rc::Rc<std::cell::RefCell<Option<std::rc::Rc<dyn Fn(PackageState)>>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));

    let render_actions = {
        let target_box = actions_box.clone();
        let pkg = pkg.clone();
        let pkg_id = pkg_id.clone();
        let store = store_clone.clone();
        let on_tx = on_tx_start.clone();
        let on_downgrade = on_downgrade.clone();

        std::rc::Rc::new(move |current_state: PackageState| {
            while let Some(child) = target_box.first_child() {
                target_box.remove(&child);
            }

            match current_state {
                PackageState::Installed => {
                    let is_runtime = pkg_id.name.starts_with("runtime/");
                    if !is_runtime {
                        let launch_btn = gtk4::Button::builder()
                            .label("Open")
                            .icon_name("media-playback-start-symbolic")
                            .css_classes(["suggested-action", "pill"])
                            .build();
                        let app_name = pkg.display_title().to_string();
                        let pkg_for_run = pkg.clone();
                        launch_btn.connect_clicked(move |_| {
                            tracing::info!("Launching application: {}", app_name);
                            if let PackageSource::Flatpak { .. } = &pkg_for_run.id.source {
                                let _ = std::process::Command::new("flatpak")
                                    .args(["run", &pkg_for_run.id.name])
                                    .spawn();
                            } else {
                                let _ = std::process::Command::new("gtk-launch")
                                    .arg(&pkg_for_run.id.name)
                                    .spawn();
                            }
                        });
                        target_box.append(&launch_btn);
                    }

                    // Progress widget for uninstallation
                    let progress_box = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Horizontal)
                        .spacing(10)
                        .css_classes(["transaction-pill"])
                        .valign(gtk4::Align::Center)
                        .visible(false)
                        .build();
                    let circle = CircularProgress::new(26);
                    let pct_label = gtk4::Label::builder()
                        .label("Uninstalling… 0%")
                        .css_classes(["caption", "heading"])
                        .valign(gtk4::Align::Center)
                        .build();
                    let cancel_btn = gtk4::Button::builder()
                        .icon_name("process-stop-symbolic")
                        .tooltip_text("Cancel uninstallation")
                        .css_classes(["flat", "circular"])
                        .build();
                    let store_c = store.clone();
                    let pkg_c = pkg_id.clone();
                    cancel_btn.connect_clicked(move |_| {
                        let s = store_c.clone();
                        let p = pkg_c.clone();
                        glib::spawn_future_local(async move {
                            let _ = s.cancel_installation(&p).await;
                        });
                    });
                    progress_box.append(circle.widget());
                    progress_box.append(&pct_label);
                    progress_box.append(&cancel_btn);

                    let uninstall_btn = gtk4::Button::builder()
                        .icon_name("user-trash-symbolic")
                        .css_classes(["destructive-action", "flat", "circular"])
                        .tooltip_text("Uninstall software")
                        .build();
                    let store_u = store.clone();
                    let pkg_u = pkg_id.clone();
                    let tx_u = on_tx.clone();
                    let pbox_u = progress_box.clone();
                    let ubtn_u = uninstall_btn.clone();
                    uninstall_btn.connect_clicked(move |_| {
                        ubtn_u.set_visible(false);
                        pbox_u.set_visible(true);
                        let rx = store_u.remove(pkg_u.clone());
                        tx_u(rx);
                    });
                    target_box.append(&uninstall_btn);
                    target_box.append(&progress_box);

                    if !is_runtime {
                        let more_menu_btn = gtk4::MenuButton::builder()
                            .icon_name("view-more-symbolic")
                            .css_classes(["flat", "circular"])
                            .tooltip_text("More Options")
                            .build();

                        let popover = gtk4::Popover::new();
                        let menu_vbox = gtk4::Box::builder()
                            .orientation(gtk4::Orientation::Vertical)
                            .spacing(4)
                            .margin_top(6)
                            .margin_bottom(6)
                            .margin_start(6)
                            .margin_end(6)
                            .build();

                        let downgrade_item = gtk4::Button::builder()
                            .label("Downgrade Version…")
                            .icon_name("edit-undo-symbolic")
                            .css_classes(["flat"])
                            .halign(gtk4::Align::Fill)
                            .build();
                        let on_dw = on_downgrade.clone();
                        let name_clone = pkg.name.clone();
                        let pop_clone = popover.clone();
                        downgrade_item.connect_clicked(move |_| {
                            pop_clone.popdown();
                            on_dw(name_clone.clone());
                        });
                        menu_vbox.append(&downgrade_item);

                        if let Some(ref url) = pkg.homepage {
                            let web_item = gtk4::LinkButton::builder()
                                .uri(url)
                                .label("Project Website")
                                .css_classes(["flat"])
                                .halign(gtk4::Align::Fill)
                                .build();
                            menu_vbox.append(&web_item);
                        }

                        popover.set_child(Some(&menu_vbox));
                        more_menu_btn.set_popover(Some(&popover));
                        target_box.append(&more_menu_btn);
                    }
                }
                PackageState::NotInstalled | PackageState::UpdateAvailable => {
                    let is_update = matches!(current_state, PackageState::UpdateAvailable);
                    let btn_label = if is_update { "Update" } else { "Install" };
                    let btn_icon = if is_update {
                        crate::icons::update_icon()
                    } else {
                        "folder-download-symbolic".to_string()
                    };

                    let install_btn = gtk4::Button::builder()
                        .label(btn_label)
                        .icon_name(&btn_icon)
                        .css_classes(["suggested-action", "pill"])
                        .build();

                    let progress_box = gtk4::Box::builder()
                        .orientation(gtk4::Orientation::Horizontal)
                        .spacing(10)
                        .css_classes(["transaction-pill"])
                        .valign(gtk4::Align::Center)
                        .visible(false)
                        .build();

                    let circle = CircularProgress::new(26);
                    let pct_label = gtk4::Label::builder()
                        .label(if is_update { "Updating… 0%" } else { "Installing… 0%" })
                        .css_classes(["caption", "heading"])
                        .valign(gtk4::Align::Center)
                        .build();

                    let cancel_btn = gtk4::Button::builder()
                        .icon_name("process-stop-symbolic")
                        .tooltip_text("Cancel installation")
                        .css_classes(["flat", "circular"])
                        .build();
                    let store_c = store.clone();
                    let pkg_c = pkg_id.clone();
                    cancel_btn.connect_clicked(move |_| {
                        let s = store_c.clone();
                        let p = pkg_c.clone();
                        glib::spawn_future_local(async move {
                            let _ = s.cancel_installation(&p).await;
                        });
                    });

                    progress_box.append(circle.widget());
                    progress_box.append(&pct_label);
                    progress_box.append(&cancel_btn);

                    let store_i = store.clone();
                    let pkg_i = pkg_id.clone();
                    let tx_i = on_tx.clone();
                    let install_btn_clone = install_btn.clone();
                    let progress_box_clone = progress_box.clone();

                    install_btn.connect_clicked(move |_| {
                        install_btn_clone.set_visible(false);
                        progress_box_clone.set_visible(true);
                        let rx_to_bar = store_i.install(pkg_i.clone());
                        tx_i(rx_to_bar);
                    });

                    target_box.append(&install_btn);
                    target_box.append(&progress_box);

                    if is_update {
                        let uninstall_btn = gtk4::Button::builder()
                            .icon_name("user-trash-symbolic")
                            .css_classes(["destructive-action", "flat", "circular"])
                            .tooltip_text("Uninstall application")
                            .build();
                        let store_u = store.clone();
                        let pkg_u = pkg_id.clone();
                        let tx_u = on_tx.clone();
                        let u_btn_c = uninstall_btn.clone();
                        let pbox_c = progress_box.clone();
                        uninstall_btn.connect_clicked(move |_| {
                            u_btn_c.set_visible(false);
                            pbox_c.set_visible(true);
                            let rx = store_u.remove(pkg_u.clone());
                            tx_u(rx);
                        });
                        target_box.append(&uninstall_btn);
                    }
                }
                PackageState::Processing => {
                    let proc_btn = gtk4::Button::builder()
                        .label("Processing…")
                        .css_classes(["pill"])
                        .sensitive(false)
                        .build();
                    target_box.append(&proc_btn);
                }
            }

            if let Some(_active_info) = store.get_active_transaction(&pkg_id) {
                for child in target_box.observe_children().into_iter().flatten() {
                    if let Ok(w) = child.downcast::<gtk4::Widget>() {
                        if w.has_css_class("transaction-pill") {
                            w.set_visible(true);
                        } else {
                            w.set_visible(false);
                        }
                    }
                }
            }
        })
    };

    *render_actions_holder.borrow_mut() = Some(render_actions.clone());
    render_actions(state);

    // Global transaction subscription for this package details view
    {
        let mut tx_sub = store_clone.subscribe_transactions();
        let target_pkg_id = pkg_id.clone();
        let target_clean_name = target_pkg_id.name.strip_suffix(".desktop").unwrap_or(&target_pkg_id.name).to_string();
        let s_chk = store_clone.clone();
        let holder_c = render_actions_holder.clone();

        glib::spawn_future_local(async move {
            while let Ok(event) = tx_sub.recv().await {
                match event {
                    ActiveTransactionEvent::Started(info) | ActiveTransactionEvent::Progress(info) => {
                        let info_clean = info.package_id.name.strip_suffix(".desktop").unwrap_or(&info.package_id.name);
                        if info.package_id == target_pkg_id || info_clean == target_clean_name {
                            // Progress is tracked via the store and global ops center
                        }
                    }
                    ActiveTransactionEvent::Completed(id) => {
                        let id_clean = id.name.strip_suffix(".desktop").unwrap_or(&id.name);
                        if id == target_pkg_id || id_clean == target_clean_name {
                            let s = s_chk.clone();
                            let pid = target_pkg_id.clone();
                            let h = holder_c.clone();
                            glib::timeout_add_local_once(
                                std::time::Duration::from_millis(300),
                                move || {
                                    glib::spawn_future_local(async move {
                                        let updated_pkg = s.get_package(&pid).await.ok().flatten();
                                        let new_state = updated_pkg
                                            .map(|p| p.state)
                                            .unwrap_or(PackageState::NotInstalled);
                                        if let Some(renderer) = h.borrow().as_ref() {
                                            renderer(new_state);
                                        }
                                    });
                                },
                            );
                        }
                    }
                    ActiveTransactionEvent::Failed(id, _) | ActiveTransactionEvent::Cancelled(id) => {
                        let id_clean = id.name.strip_suffix(".desktop").unwrap_or(&id.name);
                        if id == target_pkg_id || id_clean == target_clean_name {
                            let s = s_chk.clone();
                            let pid = target_pkg_id.clone();
                            let h = holder_c.clone();
                            glib::spawn_future_local(async move {
                                let updated_pkg = s.get_package(&pid).await.ok().flatten();
                                let cur_st = updated_pkg
                                    .map(|p| p.state)
                                    .unwrap_or(PackageState::NotInstalled);
                                if let Some(renderer) = h.borrow().as_ref() {
                                    renderer(cur_st);
                                }
                            });
                        }
                    }
                }
            }
        });
    }

    hero_box.append(&actions_box);
    content_box.append(&hero_box);

    let dl_str = match pkg.size_download {
        Some(sz) => format!("{:.1} MB", sz as f64 / 1_048_576.0),
        None => "Varies".to_string(),
    };
    let inst_str = match pkg.size_installed {
        Some(sz) => format!("{:.1} MB", sz as f64 / 1_048_576.0),
        None => "Varies".to_string(),
    };

    // -------------------------------------------------------------
    // 3. Authentic Screenshot Previews (adw::Carousel)
    // -------------------------------------------------------------
    let carousel_group = adw::PreferencesGroup::builder()
        .title("Screenshots")
        .build();

    let carousel = adw::Carousel::builder()
        .spacing(18)
        .allow_scroll_wheel(true)
        .build();

    if !pkg.screenshots.is_empty() {
        for url in &pkg.screenshots {
            let slide_card = create_screenshot_widget(url);
            carousel.append(&slide_card);
        }
    } else {
        let preview_slides = [
            ("Workspace", "Primary interface and active workspace"),
            ("Tools & Workflows", "Comprehensive tools and options"),
            ("Preferences", "User customization and display settings"),
        ];

        for (index, (slide_title, slide_desc)) in preview_slides.into_iter().enumerate() {
            let slide_card = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Vertical)
                .css_classes(["screenshot-preview-card"])
                .build();

            // Modern Linux Application Window Header
            let preview_header = gtk4::Box::builder()
                .orientation(gtk4::Orientation::Horizontal)
                .css_classes(["screenshot-preview-header"])
                .build();

            let header_title = gtk4::Label::builder()
                .label(format!("{} — {}", pkg.display_title(), slide_title))
                .css_classes(["caption", "dim-label"])
                .hexpand(true)
                .halign(gtk4::Align::Center)
                .build();
            preview_header.append(&header_title);
            slide_card.append(&preview_header);

            // Preview Body Canvas tailored to Category
            let preview_body = create_preview_canvas(&pkg, index);
            slide_card.append(&preview_body);

            // Slide caption footer
            let footer_caption = gtk4::Label::builder()
                .label(slide_desc)
                .css_classes(["caption", "dim-label"])
                .margin_bottom(12)
                .halign(gtk4::Align::Center)
                .build();
            slide_card.append(&footer_caption);

            carousel.append(&slide_card);
        }
    }

    let dots = adw::CarouselIndicatorDots::builder()
        .carousel(&carousel)
        .halign(gtk4::Align::Center)
        .margin_top(8)
        .build();

    let carousel_container = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(8)
        .build();
    carousel_container.append(&carousel);
    carousel_container.append(&dots);
    carousel_group.add(&carousel_container);
    content_box.append(&carousel_group);

    // -------------------------------------------------------------
    // 4. About Section (adw::PreferencesGroup)
    // -------------------------------------------------------------
    let about_group = adw::PreferencesGroup::builder()
        .title("About")
        .build();

    let summary_lead = gtk4::Label::builder()
        .label(&pkg.summary)
        .css_classes(["title-4"])
        .wrap(true)
        .halign(gtk4::Align::Start)
        .margin_bottom(6)
        .build();
    about_group.add(&summary_lead);

    let desc_text = pkg.description.as_deref().unwrap_or(&pkg.summary);
    let desc_label = gtk4::Label::builder()
        .label(desc_text)
        .wrap(true)
        .halign(gtk4::Align::Start)
        .css_classes(["body"])
        .margin_bottom(4)
        .build();
    about_group.add(&desc_label);
    content_box.append(&about_group);

    // -------------------------------------------------------------
    // 5. What's New Section (Actual Changelog from AppStream)
    // -------------------------------------------------------------
    let whats_new_group = adw::PreferencesGroup::builder()
        .title(format!("What's New in Version {}", pkg.version))
        .build();

    let changelog_text = pkg.changelog.as_deref().unwrap_or(
        "Optimized for ParchLinux with updated system dependencies, Wayland integration, and performance enhancements."
    );

    let expander = adw::ExpanderRow::builder()
        .title(format!("Version {} Release Notes", pkg.version))
        .subtitle("Click to view changelog and update details")
        .expanded(true)
        .build();
    let rel_icon = gtk4::Image::from_icon_name("software-update-available-symbolic");
    expander.add_prefix(&rel_icon);

    let cl_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .margin_start(16)
        .margin_end(16)
        .margin_top(12)
        .margin_bottom(12)
        .spacing(6)
        .build();

    let cl_label = gtk4::Label::builder()
        .label(changelog_text)
        .wrap(true)
        .halign(gtk4::Align::Start)
        .css_classes(["body"])
        .build();
    cl_box.append(&cl_label);
    expander.add_row(&cl_box);
    whats_new_group.add(&expander);
    content_box.append(&whats_new_group);

    // -------------------------------------------------------------
    // 6. Technical Specifications (adw::PreferencesGroup + ActionRows)
    // -------------------------------------------------------------
    let specs_group = adw::PreferencesGroup::builder()
        .title("Details")
        .build();

    let dev_row = create_spec_row(
        "Developer",
        pkg.maintainer.as_deref().unwrap_or("ParchLinux Contributors"),
        "avatar-default-symbolic",
    );
    specs_group.add(&dev_row);

    let pkg_row = create_spec_row(
        "Package Name",
        &pkg.name,
        "application-x-executable-symbolic",
    );
    specs_group.add(&pkg_row);

    let ver_row = create_spec_row(
        "Version",
        &pkg.version,
        "tag-symbolic",
    );
    specs_group.add(&ver_row);

    let lic_row = create_spec_row(
        "License",
        lic_str,
        "help-about-symbolic",
    );
    specs_group.add(&lic_row);

    let source_title = match &pkg.id.source {
        PackageSource::Parch(repo) => format!("Parch Linux [{}]", repo),
        PackageSource::Arch(repo) => format!("Arch Linux Upstream [{}]", repo),
        PackageSource::Aur => "Arch User Repository (AUR)".to_string(),
        PackageSource::Flatpak { remote } => format!("Flatpak ({})", remote),
        PackageSource::Bootc => "bootc Transactional Base OS".to_string(),
        PackageSource::Waydroid => "Waydroid Android Environment".to_string(),
    };
    let source_row = create_spec_row(
        "Repository and Source",
        &source_title,
        "package-x-generic-symbolic",
    );
    specs_group.add(&source_row);

    let arch_row = create_spec_row(
        "Architecture",
        "x86_64",
        "system-run-symbolic",
    );
    specs_group.add(&arch_row);

    let dl_row = create_spec_row(
        "Download Size",
        &dl_str,
        "folder-download-symbolic",
    );
    specs_group.add(&dl_row);

    let inst_row = create_spec_row(
        "Installed Footprint",
        &inst_str,
        "drive-harddisk-symbolic",
    );
    specs_group.add(&inst_row);

    let format_info = match &pkg.id.source {
        PackageSource::Flatpak { .. } => "Flatpak OCI Bundle (Sandboxed runtime)",
        PackageSource::Bootc => "bootc OCI Container Image Layer",
        PackageSource::Waydroid => "Android Package Container",
        _ => "Native ALPM Package (.pkg.tar.zst)",
    };
    let format_row = create_spec_row(
        "Package Format",
        format_info,
        "package-x-generic-symbolic",
    );
    specs_group.add(&format_row);

    let safety_row = create_spec_row(
        "System Safety",
        "Automatic Snapper Btrfs pre/post snapshots for instant rollback",
        "camera-photo-symbolic",
    );
    specs_group.add(&safety_row);

    if let Some(ref url) = pkg.homepage {
        let home_row = create_spec_row(
            "Project Website",
            url,
            "applications-internet-symbolic",
        );

        let link_btn = gtk4::LinkButton::builder()
            .uri(url)
            .icon_name("insert-link-symbolic")
            .css_classes(["flat", "circular"])
            .tooltip_text("Open in browser")
            .valign(gtk4::Align::Center)
            .build();
        home_row.add_suffix(&link_btn);
        specs_group.add(&home_row);
    }

    content_box.append(&specs_group);

    // -------------------------------------------------------------
    // 7. Dependencies (Clean AdwExpanderRow)
    // -------------------------------------------------------------
    if !pkg.dependencies.is_empty() {
        let dep_group = adw::PreferencesGroup::builder()
            .title("Dependencies")
            .build();

        let expander = adw::ExpanderRow::builder()
            .title(format!("Dependencies ({})", pkg.dependencies.len()))
            .subtitle("Packages and runtimes required for functionality")
            .expanded(true)
            .build();
        let dep_icon = gtk4::Image::from_icon_name("system-run-symbolic");
        expander.add_prefix(&dep_icon);

        let dep_flow = gtk4::FlowBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .max_children_per_line(4)
            .min_children_per_line(1)
            .row_spacing(8)
            .column_spacing(8)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();

        for dep in &pkg.dependencies {
            let chip = gtk4::Label::builder()
                .label(dep)
                .css_classes(["dependency-tag"])
                .build();
            dep_flow.insert(&chip, -1);
        }

        expander.add_row(&dep_flow);
        dep_group.add(&expander);
        content_box.append(&dep_group);
    }

    clamp.set_child(Some(&content_box));
    scrolled.set_child(Some(&clamp));
    scrolled.upcast()
}

/// Helper to create a specification row with an icon prefix
fn create_spec_row(title: &str, subtitle: &str, icon_name: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    let img = gtk4::Image::from_icon_name(icon_name);
    row.add_prefix(&img);
    row
}


/// Helper to generate authentic, category-tailored preview canvas mockups
fn create_preview_canvas(pkg: &Package, slide_index: usize) -> gtk4::Widget {
    let canvas_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(16)
        .css_classes(["screenshot-preview-body"])
        .vexpand(true)
        .build();

    let primary_cat = pkg.categories.first().copied().unwrap_or(PackageCategory::Utilities);

    // Sidebar wireframe
    let sidebar = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(8)
        .css_classes(["mock-sidebar-box"])
        .build();

    let side_icon = match primary_cat {
        PackageCategory::Development => "text-editor-symbolic",
        PackageCategory::Graphics => "image-x-generic-symbolic",
        PackageCategory::Multimedia => "audio-x-generic-symbolic",
        PackageCategory::Games => "input-gaming-symbolic",
        _ => "utilities-terminal-symbolic",
    };
    let s_icon = gtk4::Image::builder()
        .icon_name(side_icon)
        .pixel_size(24)
        .halign(gtk4::Align::Start)
        .margin_bottom(8)
        .build();
    sidebar.append(&s_icon);

    for _ in 0..4 {
        let bar = gtk4::Box::builder().css_classes(["mock-sidebar-bar"]).build();
        sidebar.append(&bar);
    }
    canvas_box.append(&sidebar);

    // Main workspace content panel
    let content_panel = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .hexpand(true)
        .css_classes(["mock-content-panel"])
        .build();

    let panel_header = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(8)
        .build();

    let panel_title = gtk4::Label::builder()
        .label(match slide_index {
            0 => "Active Session",
            1 => "Inspect & Debug",
            _ => "Global Configuration",
        })
        .css_classes(["heading"])
        .halign(gtk4::Align::Start)
        .build();
    panel_header.append(&panel_title);
    content_panel.append(&panel_header);

    for i in 0..3 {
        let data_bar = gtk4::Box::builder()
            .css_classes(["mock-data-bar"])
            .opacity(1.0 - (i as f64 * 0.2))
            .build();
        content_panel.append(&data_bar);
    }

    let footer_tag = gtk4::Label::builder()
        .label(format!("ParchLinux • {} v{}", pkg.name, pkg.version))
        .css_classes(["caption", "dim-label"])
        .halign(gtk4::Align::End)
        .margin_top(8)
        .build();
    content_panel.append(&footer_tag);

    canvas_box.append(&content_panel);
    canvas_box.upcast()
}

fn get_cache_path(url: &str) -> std::path::PathBuf {
    let mut cache_dir = gtk4::glib::user_cache_dir();
    cache_dir.push("pastor");
    cache_dir.push("screenshots");
    let _ = std::fs::create_dir_all(&cache_dir);

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(url, &mut hasher);
    let hash = std::hash::Hasher::finish(&hasher);

    let ext = if url.ends_with(".png") {
        "png"
    } else if url.ends_with(".webp") {
        "webp"
    } else {
        "jpg"
    };

    cache_dir.push(format!("{:x}.{}", hash, ext));
    cache_dir
}

pub fn create_screenshot_widget(url: &str) -> gtk4::Widget {
    let overlay = gtk4::Overlay::builder()
        .height_request(320)
        .width_request(560)
        .css_classes(["screenshot-real-card"])
        .build();

    let picture = gtk4::Picture::builder()
        .keep_aspect_ratio(true)
        .can_shrink(true)
        .vexpand(true)
        .hexpand(true)
        .build();

    let spinner = gtk4::Spinner::builder()
        .spinning(true)
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Center)
        .width_request(36)
        .height_request(36)
        .build();

    let cache_file = get_cache_path(url);
    if cache_file.is_file() {
        picture.set_filename(Some(&cache_file));
        spinner.set_visible(false);
    } else {
        spinner.set_visible(true);
        let pic_clone = picture.clone();
        let spin_clone = spinner.clone();
        let url_string = url.to_string();
        let target_file = cache_file.clone();

        gtk4::glib::spawn_future_local(async move {
            let target_for_thread = target_file.clone();
            let res = tokio::task::spawn_blocking(move || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                let resp = ureq::get(&url_string).timeout(std::time::Duration::from_secs(8)).call()?;
                let mut reader = resp.into_reader();
                let tmp_file = target_for_thread.with_extension("tmp");
                let mut f = std::fs::File::create(&tmp_file)?;
                std::io::copy(&mut reader, &mut f)?;
                std::fs::rename(&tmp_file, &target_for_thread)?;
                Ok(())
            }).await;

            if let Ok(Ok(())) = res {
                pic_clone.set_filename(Some(&target_file));
            }
            spin_clone.set_visible(false);
        });
    }

    overlay.set_child(Some(&picture));
    overlay.add_overlay(&spinner);
    overlay.upcast()
}

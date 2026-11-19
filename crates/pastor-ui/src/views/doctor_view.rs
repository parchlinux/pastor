use std::sync::Arc;
use adw::prelude::*;
use gtk4::glib;
use pastor_core::{format_size, tr, DoctorReport, PacnewFile, TransactionEvent};
use pastor_store::Store;

pub fn create_doctor_view(
    store: Store,
    _on_back: impl Fn() + 'static + Clone,
    on_transaction_start: impl Fn(tokio::sync::mpsc::Receiver<TransactionEvent>) + 'static + Clone,
) -> gtk4::Widget {
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build();

    let content_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(18)
        .margin_top(16)
        .margin_bottom(28)
        .margin_start(16)
        .margin_end(16)
        .build();

    // Title & Header bar
    let header_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(12)
        .halign(gtk4::Align::Fill)
        .build();

    let title_vbox = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(2)
        .hexpand(true)
        .build();

    let title_lbl = gtk4::Label::builder()
        .label(tr("Parch Doctor"))
        .css_classes(["title-2"])
        .halign(gtk4::Align::Start)
        .build();

    let subtitle_lbl = gtk4::Label::builder()
        .label(tr("System health, configuration drift, and maintenance diagnostics"))
        .css_classes(["dim-label", "caption"])
        .halign(gtk4::Align::Start)
        .build();

    title_vbox.append(&title_lbl);
    title_vbox.append(&subtitle_lbl);

    let refresh_btn = gtk4::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text(tr("Refresh Diagnostics"))
        .css_classes(["flat", "circular"])
        .valign(gtk4::Align::Center)
        .build();

    header_box.append(&title_vbox);
    header_box.append(&refresh_btn);
    content_box.append(&header_box);

    // Health Status Banner
    let banner = adw::Banner::builder()
        .title(tr("Scanning system health and configuration status…"))
        .revealed(true)
        .build();
    content_box.append(&banner);

    // Group 1: Configuration Drift (.pacnew & .pacsave)
    let pacnew_group = adw::PreferencesGroup::builder()
        .title(tr("Configuration Drift (.pacnew)"))
        .description(tr("Upstream package updates that installed new configuration templates"))
        .build();
    let pacnew_list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    pacnew_group.add(&pacnew_list);
    content_box.append(&pacnew_group);

    // Group 2: Orphaned Packages
    let orphan_group = adw::PreferencesGroup::builder()
        .title(tr("Orphaned Packages"))
        .description(tr("Unneeded dependencies that are no longer required by any installed package"))
        .build();
    let orphan_list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    orphan_group.add(&orphan_list);
    content_box.append(&orphan_group);

    // Group 3: Package Cache & Storage
    let cache_group = adw::PreferencesGroup::builder()
        .title(tr("Package Cache Storage"))
        .description(tr("Arch Linux package archives stored in /var/cache/pacman/pkg"))
        .build();
    let cache_list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    cache_group.add(&cache_list);
    content_box.append(&cache_group);

    // Group 4: Pacman Database Lock
    let lock_group = adw::PreferencesGroup::builder()
        .title(tr("Database Lock Status"))
        .description(tr("Pacman process lock file at /var/lib/pacman/db.lck"))
        .visible(false)
        .build();
    let lock_list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    lock_group.add(&lock_list);
    content_box.append(&lock_group);

    let clamp = adw::Clamp::builder()
        .maximum_size(1080)
        .tightening_threshold(720)
        .vexpand(true)
        .build();
    clamp.set_child(Some(&content_box));
    scrolled.set_child(Some(&clamp));

    // Diagnostic refresh logic
    let run_diagnostics = {
        let store = store.clone();
        let banner = banner.clone();
        let pacnew_list = pacnew_list.clone();
        let orphan_list = orphan_list.clone();
        let cache_list = cache_list.clone();
        let lock_group = lock_group.clone();
        let lock_list = lock_list.clone();
        let root_widget = scrolled.clone();
        let on_tx = on_transaction_start.clone();

        Arc::new(move || {
            let s = store.clone();
            let b = banner.clone();
            let p_list = pacnew_list.clone();
            let o_list = orphan_list.clone();
            let c_list = cache_list.clone();
            let l_group = lock_group.clone();
            let l_list = lock_list.clone();
            let root_w = root_widget.clone();
            let on_tx_c = on_tx.clone();

            b.set_title(&tr("Scanning system health and configuration status…"));
            b.set_revealed(true);

            glib::spawn_future_local(async move {
                let report = s.run_doctor_diagnostics().await;
                populate_doctor_view(
                    &report,
                    &s,
                    &b,
                    &p_list,
                    &o_list,
                    &c_list,
                    &l_group,
                    &l_list,
                    &root_w,
                    on_tx_c,
                );
            });
        })
    };

    let run_diag_clone = run_diagnostics.clone();
    refresh_btn.connect_clicked(move |_| {
        run_diag_clone();
    });

    // Initial scan on page open
    run_diagnostics();

    scrolled.upcast()
}

fn populate_doctor_view(
    report: &DoctorReport,
    store: &Store,
    banner: &adw::Banner,
    pacnew_list: &gtk4::ListBox,
    orphan_list: &gtk4::ListBox,
    cache_list: &gtk4::ListBox,
    lock_group: &adw::PreferencesGroup,
    lock_list: &gtk4::ListBox,
    parent_widget: &gtk4::ScrolledWindow,
    on_tx_start: impl Fn(tokio::sync::mpsc::Receiver<TransactionEvent>) + 'static + Clone,
) {
    // 1. Update Banner
    let issues = report.total_issues_count();
    if issues == 0 {
        banner.set_title(&tr("System is healthy: No configuration drift, stale locks, or orphaned packages detected."));
    } else {
        banner.set_title(&format!("{} {} {}", tr("Parch Doctor found"), issues, tr("items requiring review.")));
    }
    banner.set_revealed(true);

    // 2. Populate .pacnew List
    while let Some(child) = pacnew_list.first_child() {
        pacnew_list.remove(&child);
    }
    if report.pacnew_files.is_empty() {
        let row = adw::ActionRow::builder()
            .title(tr("All configuration files are up to date"))
            .subtitle(tr("No .pacnew or .pacsave files found in /etc"))
            .sensitive(false)
            .build();
        pacnew_list.append(&row);
    } else {
        for file in &report.pacnew_files {
            let row = create_pacnew_row(file, store, parent_widget);
            pacnew_list.append(&row);
        }
    }

    // 3. Populate Orphan List
    while let Some(child) = orphan_list.first_child() {
        orphan_list.remove(&child);
    }
    if report.orphan_packages.is_empty() {
        let row = adw::ActionRow::builder()
            .title(tr("No orphaned packages found"))
            .subtitle(tr("All installed packages are required by the system or explicitly tracked"))
            .sensitive(false)
            .build();
        orphan_list.append(&row);
    } else {
        let total_size: u64 = report.orphan_packages.iter().map(|p| p.size_bytes).sum();
        let header_row = adw::ActionRow::builder()
            .title(format!("{} {}", report.orphan_packages.len(), tr("orphans detected")))
            .subtitle(format!("{}: {}", tr("Total reclaimable space"), format_size(total_size)))
            .build();

        let clean_orphans_btn = gtk4::Button::builder()
            .label(tr("Remove All Orphans"))
            .css_classes(["destructive-action"])
            .valign(gtk4::Align::Center)
            .build();

        let s_c = store.clone();
        let p_w = parent_widget.clone();
        let on_tx_c = on_tx_start.clone();
        clean_orphans_btn.connect_clicked(move |_| {
            let s = s_c.clone();
            let on_tx = on_tx_c.clone();
            show_repair_confirmation(
                &p_w,
                &tr("Remove Orphaned Packages?"),
                &tr("These packages were installed as dependencies but are no longer needed by any installed software."),
                true,
                move |take_snapshot| {
                    glib::spawn_future_local(async move {
                        if take_snapshot {
                            let _ = s.snapshot_backend().create_pre_snapshot("Parch Doctor: Pre-orphan removal").await;
                        }
                        let rx = s.refresh_databases();
                        on_tx(rx);
                    });
                },
            );
        });

        header_row.add_suffix(&clean_orphans_btn);
        orphan_list.append(&header_row);

        for orphan in &report.orphan_packages {
            let row = adw::ActionRow::builder()
                .title(&orphan.name)
                .subtitle(format!("{} ({})", orphan.version, format_size(orphan.size_bytes)))
                .build();
            orphan_list.append(&row);
        }
    }

    // 4. Populate Cache List
    while let Some(child) = cache_list.first_child() {
        cache_list.remove(&child);
    }
    let total_cache_row = adw::ActionRow::builder()
        .title(tr("Total Cached Packages"))
        .subtitle(format!(
            "{} {} ({})",
            report.cache_health.total_packages_cached,
            tr("packages"),
            format_size(report.cache_health.total_cache_bytes)
        ))
        .build();
    cache_list.append(&total_cache_row);

    let reclaim_row = adw::ActionRow::builder()
        .title(tr("Reclaimable Cache Space"))
        .subtitle(format!(
            "{} {} ({})",
            report.cache_health.reclaimable_packages,
            tr("older versions beyond latest 2"),
            format_size(report.cache_health.reclaimable_bytes)
        ))
        .build();

    if report.cache_health.reclaimable_packages > 0 {
        let trim_btn = gtk4::Button::builder()
            .label(tr("Clean Cache"))
            .css_classes(["suggested-action"])
            .valign(gtk4::Align::Center)
            .build();

        let s_c = store.clone();
        let p_w = parent_widget.clone();
        trim_btn.connect_clicked(move |_| {
            let s = s_c.clone();
            show_repair_confirmation(
                &p_w,
                &tr("Clean Package Cache?"),
                &tr("This will remove obsolete cached package tarballs, keeping the two latest versions for safe rollbacks."),
                false,
                move |_| {
                    glib::spawn_future_local(async move {
                        let _ = s.clean_pacman_cache(2).await;
                    });
                },
            );
        });
        reclaim_row.add_suffix(&trim_btn);
    }
    cache_list.append(&reclaim_row);

    // 5. Populate Lock List
    while let Some(child) = lock_list.first_child() {
        lock_list.remove(&child);
    }
    if let Some(lock) = &report.database_lock {
        if lock.exists {
            lock_group.set_visible(true);
            let lock_row = adw::ActionRow::builder()
                .title(if lock.is_stale {
                    tr("Stale Database Lock Detected")
                } else {
                    tr("Database Lock Active")
                })
                .subtitle(if let Some(proc) = &lock.process_name {
                    format!("{}: {} (PID: {})", tr("Held by process"), proc, lock.active_pid.unwrap_or(0))
                } else {
                    tr("No running pacman process detected. Safe to remove.").to_string()
                })
                .build();

            if lock.is_stale {
                let unlock_btn = gtk4::Button::builder()
                    .label(tr("Remove Stale Lock"))
                    .css_classes(["destructive-action"])
                    .valign(gtk4::Align::Center)
                    .build();

                let s_c = store.clone();
                let p_w = parent_widget.clone();
                unlock_btn.connect_clicked(move |_| {
                    let s = s_c.clone();
                    show_repair_confirmation(
                        &p_w,
                        &tr("Remove Database Lock?"),
                        &tr("This will delete /var/lib/pacman/db.lck so new package installations can proceed."),
                        false,
                        move |_| {
                            glib::spawn_future_local(async move {
                                let _ = s.resolve_database_lock().await;
                            });
                        },
                    );
                });
                lock_row.add_suffix(&unlock_btn);
            }
            lock_list.append(&lock_row);
        } else {
            lock_group.set_visible(false);
        }
    } else {
        lock_group.set_visible(false);
    }
}

fn create_pacnew_row(
    file: &PacnewFile,
    store: &Store,
    parent_widget: &gtk4::ScrolledWindow,
) -> adw::ActionRow {
    let name = file
        .original_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config")
        .to_string();

    let row = adw::ActionRow::builder()
        .title(file.original_path.display().to_string())
        .subtitle(format!(
            "{}: {} ({})",
            tr("Candidate"),
            file.candidate_path.display(),
            format_size(file.candidate_size_bytes)
        ))
        .build();

    let diff_btn = gtk4::Button::builder()
        .label(tr("View Diff"))
        .css_classes(["flat"])
        .valign(gtk4::Align::Center)
        .build();

    let orig = file.original_path.clone();
    let cand = file.candidate_path.clone();
    let parent_clone = parent_widget.clone();
    let s_diff = store.clone();

    diff_btn.connect_clicked(move |_| {
        let s_k = s_diff.clone();
        let s_r = s_diff.clone();
        let orig_k = orig.clone();
        let cand_k = cand.clone();
        let orig_r = orig.clone();
        let cand_r = cand.clone();

        crate::dialogs::show_pacnew_diff_dialog(
            &parent_clone,
            &orig,
            &cand,
            move || {
                glib::spawn_future_local(async move {
                    let _ = s_k.apply_pacnew_resolution(&orig_k, &cand_k, false).await;
                });
            },
            move || {
                glib::spawn_future_local(async move {
                    let _ = s_r.apply_pacnew_resolution(&orig_r, &cand_r, true).await;
                });
            },
        );
    });

    let s_keep = store.clone();
    let orig_k = file.original_path.clone();
    let cand_k = file.candidate_path.clone();
    let keep_btn = gtk4::Button::builder()
        .label(tr("Keep Current"))
        .tooltip_text(tr("Discard .pacnew and keep existing file"))
        .css_classes(["flat"])
        .valign(gtk4::Align::Center)
        .build();
    keep_btn.connect_clicked(move |_| {
        let s = s_keep.clone();
        let orig = orig_k.clone();
        let cand = cand_k.clone();
        glib::spawn_future_local(async move {
            let _ = s.apply_pacnew_resolution(&orig, &cand, false).await;
        });
    });

    let s_rep = store.clone();
    let orig_r = file.original_path.clone();
    let cand_r = file.candidate_path.clone();
    let p_rep = parent_widget.clone();
    let replace_btn = gtk4::Button::builder()
        .label(tr("Replace"))
        .tooltip_text(tr("Overwrite current file with .pacnew"))
        .css_classes(["suggested-action"])
        .valign(gtk4::Align::Center)
        .build();
    replace_btn.connect_clicked(move |_| {
        let s = s_rep.clone();
        let orig = orig_r.clone();
        let cand = cand_r.clone();
        show_repair_confirmation(
            &p_rep,
            &format!("{}: {}", tr("Replace Configuration"), name),
            &tr("This will overwrite your existing configuration file with the new package template."),
            true,
            move |take_snapshot| {
                glib::spawn_future_local(async move {
                    if take_snapshot {
                        let _ = s.snapshot_backend().create_pre_snapshot("Parch Doctor: Pre-pacnew overwrite").await;
                    }
                    let _ = s.apply_pacnew_resolution(&orig, &cand, true).await;
                });
            },
        );
    });

    row.add_suffix(&diff_btn);
    row.add_suffix(&keep_btn);
    row.add_suffix(&replace_btn);
    row
}

fn show_repair_confirmation(
    parent: &impl IsA<gtk4::Widget>,
    title: &str,
    message: &str,
    has_snapshots: bool,
    on_confirm: impl FnOnce(bool) + 'static,
) {
    let dialog = adw::MessageDialog::builder()
        .heading(title)
        .body(message)
        .modal(true)
        .build();

    if let Some(root) = parent.root() {
        if let Ok(w) = root.downcast::<gtk4::Window>() {
            dialog.set_transient_for(Some(&w));
        }
    }

    dialog.add_response("cancel", &tr("Cancel"));
    dialog.add_response("confirm", &tr("Proceed"));
    dialog.set_response_appearance("confirm", adw::ResponseAppearance::Destructive);

    let check = gtk4::CheckButton::builder()
        .label(tr("Create Btrfs system restore point before applying changes"))
        .active(has_snapshots)
        .visible(has_snapshots)
        .halign(gtk4::Align::Start)
        .margin_top(8)
        .build();

    dialog.set_extra_child(Some(&check));

    let cb = std::rc::Rc::new(std::cell::RefCell::new(Some(on_confirm)));
    dialog.connect_response(None, move |_, resp| {
        if resp == "confirm" {
            let take_snap = check.is_active();
            if let Some(f) = cb.borrow_mut().take() {
                f(take_snap);
            }
        }
    });

    dialog.present();
}

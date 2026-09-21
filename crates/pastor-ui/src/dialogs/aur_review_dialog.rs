use adw::prelude::*;
use pastor_aur::{ScanReport, TrustReport, TrustStatus};

pub fn show_aur_review_dialog(
    parent: &impl IsA<gtk4::Widget>,
    pkg_name: &str,
    pkg_version: &str,
    trust: &TrustReport,
    scan_report: &ScanReport,
    diff_text: &str,
    on_confirm: impl FnOnce(bool) + 'static,
) {
    let window = adw::Window::builder()
        .title(&format!("AUR Security Review: {}", pkg_name))
        .default_width(840)
        .default_height(680)
        .modal(true)
        .build();

    if let Some(root) = parent.root() {
        if let Ok(w) = root.downcast::<gtk4::Window>() {
            window.set_transient_for(Some(&w));
        }
    }

    let toolbar_view = adw::ToolbarView::new();
    let header_bar = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header_bar);

    let main_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(16)
        .margin_top(16)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();

    // -------------------------------------------------------------
    // 1. Maintainer Trust Banner
    // -------------------------------------------------------------
    let banner_frame = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(8)
        .css_classes(["card", "p-3"])
        .build();

    let (trust_title, trust_subtitle, trust_icon, trust_css) = match &trust.status {
        TrustStatus::MaintainerChanged { previous, current } => (
            "⚠️ Maintainer Identity Changed!".to_string(),
            format!(
                "Package maintainer changed from '{}' to '{}'. This matches known 2026 supply-chain attack vectors. Please inspect the code changes carefully.",
                previous, current
            ),
            "dialog-warning-symbolic",
            "destructive-action",
        ),
        TrustStatus::FirstInstall => (
            "First Time Install: New Maintainer".to_string(),
            trust.details.clone(),
            "dialog-information-symbolic",
            "accent",
        ),
        TrustStatus::Orphaned { .. } => (
            "⚠️ Orphaned Package".to_string(),
            trust.details.clone(),
            "dialog-warning-symbolic",
            "destructive-action",
        ),
        TrustStatus::UnchangedMaintainer { maintainer } => (
            format!("Verified Maintainer: {}", maintainer),
            "Maintainer identity matches previous install baseline. Showing diff against last known-good commit.".to_string(),
            "emblem-ok-symbolic",
            "success",
        ),
    };

    let banner_header = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(12)
        .build();

    let icon = gtk4::Image::builder()
        .icon_name(trust_icon)
        .pixel_size(28)
        .css_classes([trust_css])
        .build();

    let banner_text_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(4)
        .hexpand(true)
        .build();

    let title_lbl = gtk4::Label::builder()
        .label(&trust_title)
        .css_classes(["heading"])
        .halign(gtk4::Align::Start)
        .build();

    let subtitle_lbl = gtk4::Label::builder()
        .label(&trust_subtitle)
        .wrap(true)
        .css_classes(["caption", "dim-label"])
        .halign(gtk4::Align::Start)
        .build();

    banner_text_box.append(&title_lbl);
    banner_text_box.append(&subtitle_lbl);

    banner_header.append(&icon);
    banner_header.append(&banner_text_box);
    banner_frame.append(&banner_header);
    main_box.append(&banner_frame);

    // -------------------------------------------------------------
    // 2. Static Heuristic Scanner Findings
    // -------------------------------------------------------------
    let scanner_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(8)
        .css_classes(["card", "p-3"])
        .build();

    let scan_status_lbl = gtk4::Label::builder()
        .label(&scan_report.summary_text())
        .css_classes(["heading"])
        .halign(gtk4::Align::Start)
        .build();
    scanner_box.append(&scan_status_lbl);

    if !scan_report.is_clean() {
        let findings_list = gtk4::ListBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();

        for finding in &scan_report.findings {
            let row = adw::ActionRow::builder()
                .title(&format!("{}: {} ({}:{})", finding.severity.badge_name(), finding.title, finding.file, finding.line_number))
                .subtitle(&format!("{}\nLine: {}", finding.description, finding.line_content.trim()))
                .build();

            let badge = gtk4::Label::builder()
                .label(finding.severity.badge_name())
                .css_classes(["caption", finding.severity.css_class()])
                .valign(gtk4::Align::Center)
                .build();
            row.add_suffix(&badge);
            findings_list.append(&row);
        }

        scanner_box.append(&findings_list);
    }
    main_box.append(&scanner_box);

    // -------------------------------------------------------------
    // 3. Diff & Source Code Viewer (Monospace)
    // -------------------------------------------------------------
    let diff_title = gtk4::Label::builder()
        .label("PKGBUILD / Commit Changes")
        .css_classes(["heading"])
        .halign(gtk4::Align::Start)
        .build();
    main_box.append(&diff_title);

    let text_view = gtk4::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .left_margin(12)
        .right_margin(12)
        .top_margin(12)
        .bottom_margin(12)
        .build();

    let buffer = text_view.buffer();
    buffer.set_text(diff_text);

    let scrolled_diff = gtk4::ScrolledWindow::builder()
        .child(&text_view)
        .vexpand(true)
        .min_content_height(260)
        .css_classes(["card"])
        .build();
    main_box.append(&scrolled_diff);

    // -------------------------------------------------------------
    // 4. Sandbox Isolation Option (Bubblewrap vs Host)
    // -------------------------------------------------------------
    let sandbox_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .css_classes(["card", "p-2"])
        .build();

    let sandbox_row = adw::ActionRow::builder()
        .title("Bubblewrap Sandbox (Filesystem & Credential Isolation)")
        .subtitle("Builds inside an isolated unprivileged container (home files and credentials shielded, network enabled for source dependencies).")
        .build();

    let sandbox_switch = gtk4::Switch::builder()
        .active(true)
        .valign(gtk4::Align::Center)
        .build();
    sandbox_row.add_suffix(&sandbox_switch);
    sandbox_box.append(&sandbox_row);

    let host_warning = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(10)
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(8)
        .visible(false)
        .build();

    let host_warn_icon = gtk4::Image::builder()
        .icon_name("dialog-warning-symbolic")
        .pixel_size(20)
        .css_classes(["destructive-action"])
        .build();

    let host_warn_lbl = gtk4::Label::builder()
        .label("⚠️ Warning: Building directly on the host machine without Bubblewrap sandbox. PKGBUILD scripts will execute with direct user privileges and network access.")
        .wrap(true)
        .css_classes(["caption", "destructive-action"])
        .halign(gtk4::Align::Start)
        .build();

    host_warning.append(&host_warn_icon);
    host_warning.append(&host_warn_lbl);
    sandbox_box.append(&host_warning);
    main_box.append(&sandbox_box);

    let proceed_btn = gtk4::Button::builder()
        .label(&format!("Build & Install v{}", pkg_version))
        .css_classes(["suggested-action", "pill"])
        .build();

    let host_warn_clone = host_warning.clone();
    let proceed_btn_clone = proceed_btn.clone();
    let pkg_ver_clone = pkg_version.to_string();
    sandbox_switch.connect_active_notify(move |sw| {
        let is_sandbox = sw.is_active();
        host_warn_clone.set_visible(!is_sandbox);
        if is_sandbox {
            proceed_btn_clone.set_label(&format!("Build in Sandbox & Install v{}", pkg_ver_clone));
        } else {
            proceed_btn_clone.set_label(&format!("Build on Host & Install v{}", pkg_ver_clone));
        }
    });

    // -------------------------------------------------------------
    // 5. Action Buttons & Confirmation
    // -------------------------------------------------------------
    let action_bar = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(12)
        .halign(gtk4::Align::End)
        .build();

    let cancel_btn = gtk4::Button::builder()
        .label("Cancel")
        .css_classes(["pill"])
        .build();

    let win_cancel = window.clone();
    cancel_btn.connect_clicked(move |_| {
        win_cancel.close();
    });

    let win_proceed = window.clone();
    let on_confirm_cell = std::rc::Rc::new(std::cell::RefCell::new(Some(on_confirm)));
    let switch_for_confirm = sandbox_switch.clone();
    proceed_btn.connect_clicked(move |_| {
        let use_sandbox = switch_for_confirm.is_active();
        win_proceed.close();
        if let Some(cb) = on_confirm_cell.borrow_mut().take() {
            cb(use_sandbox);
        }
    });

    action_bar.append(&cancel_btn);
    action_bar.append(&proceed_btn);
    main_box.append(&action_bar);

    toolbar_view.set_content(Some(&main_box));
    window.set_content(Some(&toolbar_view));
    window.present();
}

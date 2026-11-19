use std::path::Path;
use adw::prelude::*;
use pastor_core::doctor::generate_diff;
use pastor_core::tr;

pub fn show_pacnew_diff_dialog(
    parent: &impl IsA<gtk4::Widget>,
    original_path: &Path,
    candidate_path: &Path,
    on_keep: impl FnOnce() + 'static,
    on_replace: impl FnOnce() + 'static,
) {
    let file_name = original_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config");

    let window_title = format!("{}: {}", tr("Configuration Diff"), file_name);

    let window = adw::Window::builder()
        .title(&window_title)
        .default_width(740)
        .default_height(580)
        .width_request(340)
        .modal(true)
        .build();

    if let Some(root) = parent.root() {
        if let Ok(w) = root.downcast::<gtk4::Window>() {
            window.set_transient_for(Some(&w));
        }
    }

    let toolbar_view = adw::ToolbarView::new();
    let header_bar = adw::HeaderBar::new();

    let on_keep_cell = std::rc::Rc::new(std::cell::RefCell::new(Some(on_keep)));
    let on_replace_cell = std::rc::Rc::new(std::cell::RefCell::new(Some(on_replace)));

    let win_weak = window.downgrade();
    let keep_btn = gtk4::Button::builder()
        .label(tr("Keep Current"))
        .tooltip_text(tr("Discard the .pacnew template and preserve your existing configuration"))
        .css_classes(["flat"])
        .valign(gtk4::Align::Center)
        .build();
    let cb_keep = on_keep_cell.clone();
    let w_k = win_weak.clone();
    keep_btn.connect_clicked(move |_| {
        if let Some(cb) = cb_keep.borrow_mut().take() {
            cb();
        }
        if let Some(w) = w_k.upgrade() {
            w.close();
        }
    });

    let replace_btn = gtk4::Button::builder()
        .label(tr("Replace with New"))
        .tooltip_text(tr("Overwrite the existing file with the new configuration template"))
        .css_classes(["suggested-action"])
        .valign(gtk4::Align::Center)
        .build();
    let cb_replace = on_replace_cell.clone();
    let w_r = win_weak.clone();
    replace_btn.connect_clicked(move |_| {
        if let Some(cb) = cb_replace.borrow_mut().take() {
            cb();
        }
        if let Some(w) = w_r.upgrade() {
            w.close();
        }
    });

    header_bar.pack_start(&keep_btn);
    header_bar.pack_end(&replace_btn);
    toolbar_view.add_top_bar(&header_bar);

    // ViewStack with:
    // 1. Changes (Unified Colorful Diff)
    // 2. Current File
    // 3. New Template
    let view_stack = adw::ViewStack::new();

    // 1. Unified Diff View
    let diff_text = generate_diff(original_path, candidate_path);
    let mut additions_count = 0;
    let mut deletions_count = 0;
    for line in diff_text.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            additions_count += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions_count += 1;
        }
    }

    let diff_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .build();

    let stats_bar = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(4)
        .margin_start(12)
        .margin_end(12)
        .build();

    let adds_badge = gtk4::Label::builder()
        .label(format!("+{} {}", additions_count, tr("additions")))
        .css_classes(["badge", "success-badge"])
        .build();

    let dels_badge = gtk4::Label::builder()
        .label(format!("-{} {}", deletions_count, tr("removals")))
        .css_classes(["badge", "destructive-badge"])
        .build();

    let path_label = gtk4::Label::builder()
        .label(format!("{}: {}", tr("File"), original_path.display()))
        .css_classes(["dim-label", "caption"])
        .ellipsize(gtk4::pango::EllipsizeMode::Middle)
        .hexpand(true)
        .halign(gtk4::Align::End)
        .build();

    stats_bar.append(&adds_badge);
    stats_bar.append(&dels_badge);
    stats_bar.append(&path_label);
    diff_box.append(&stats_bar);

    let diff_tv = gtk4::TextView::builder()
        .monospace(true)
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk4::WrapMode::WordChar)
        .left_margin(12)
        .right_margin(12)
        .top_margin(8)
        .bottom_margin(8)
        .build();
    populate_diff_buffer(&diff_tv.buffer(), &diff_text);

    let diff_sw = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .child(&diff_tv)
        .build();
    diff_box.append(&diff_sw);

    let diff_page = view_stack.add_titled(&diff_box, Some("changes"), &tr("Changes"));
    diff_page.set_icon_name(Some("format-text-diff-symbolic"));

    // 2. Current File View
    let orig_text = std::fs::read_to_string(original_path)
        .unwrap_or_else(|_| tr("(Original file does not exist or cannot be read)").to_string());
    let orig_tv = gtk4::TextView::builder()
        .monospace(true)
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk4::WrapMode::WordChar)
        .left_margin(12)
        .right_margin(12)
        .top_margin(8)
        .bottom_margin(8)
        .build();
    orig_tv.buffer().set_text(&orig_text);
    let orig_sw = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .child(&orig_tv)
        .build();
    let orig_page = view_stack.add_titled(&orig_sw, Some("current"), &tr("Current File"));
    orig_page.set_icon_name(Some("document-open-symbolic"));

    // 3. New Candidate View
    let cand_text = std::fs::read_to_string(candidate_path)
        .unwrap_or_else(|_| tr("(Candidate .pacnew file cannot be read)").to_string());
    let cand_tv = gtk4::TextView::builder()
        .monospace(true)
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk4::WrapMode::WordChar)
        .left_margin(12)
        .right_margin(12)
        .top_margin(8)
        .bottom_margin(8)
        .build();
    cand_tv.buffer().set_text(&cand_text);
    let cand_sw = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .child(&cand_tv)
        .build();
    let cand_page = view_stack.add_titled(&cand_sw, Some("incoming"), &tr("New Template"));
    cand_page.set_icon_name(Some("document-new-symbolic"));

    // View Switchers: Top (Desktop) and Bottom Bar (Mobile)
    let top_switcher = adw::ViewSwitcher::builder()
        .stack(&view_stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();
    header_bar.set_title_widget(Some(&top_switcher));

    let bottom_switcher_bar = adw::ViewSwitcherBar::builder()
        .stack(&view_stack)
        .build();
    toolbar_view.add_bottom_bar(&bottom_switcher_bar);

    // Adaptive mobile layout breakpoint (< 540px)
    let mobile_bp = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        540.0,
        adw::LengthUnit::Px,
    ));
    mobile_bp.add_setter(&bottom_switcher_bar, "reveal", Some(&true.into()));
    mobile_bp.add_setter(&top_switcher, "visible", Some(&false.into()));
    window.add_breakpoint(mobile_bp);

    toolbar_view.set_content(Some(&view_stack));
    window.set_content(Some(&toolbar_view));
    window.present();
}

fn populate_diff_buffer(buffer: &gtk4::TextBuffer, diff_text: &str) {
    let tag_table = buffer.tag_table();

    let tag_add = gtk4::TextTag::builder()
        .name("diff-add")
        .foreground("#2ec27e")
        .paragraph_background("rgba(46, 194, 126, 0.15)")
        .build();

    let tag_del = gtk4::TextTag::builder()
        .name("diff-del")
        .foreground("#e01b24")
        .paragraph_background("rgba(224, 27, 36, 0.15)")
        .build();

    let tag_chunk = gtk4::TextTag::builder()
        .name("diff-chunk")
        .foreground("#3584e4")
        .weight(700)
        .build();

    let tag_meta = gtk4::TextTag::builder()
        .name("diff-meta")
        .foreground("#9a9996")
        .weight(700)
        .build();

    let _ = tag_table.add(&tag_add);
    let _ = tag_table.add(&tag_del);
    let _ = tag_table.add(&tag_chunk);
    let _ = tag_table.add(&tag_meta);

    buffer.set_text("");
    let mut iter = buffer.end_iter();

    for line in diff_text.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            buffer.insert_with_tags(&mut iter, &format!("{line}\n"), &[&tag_meta]);
        } else if line.starts_with("@@") {
            buffer.insert_with_tags(&mut iter, &format!("{line}\n"), &[&tag_chunk]);
        } else if line.starts_with('+') {
            buffer.insert_with_tags(&mut iter, &format!("{line}\n"), &[&tag_add]);
        } else if line.starts_with('-') {
            buffer.insert_with_tags(&mut iter, &format!("{line}\n"), &[&tag_del]);
        } else {
            buffer.insert(&mut iter, &format!("{line}\n"));
        }
    }
}

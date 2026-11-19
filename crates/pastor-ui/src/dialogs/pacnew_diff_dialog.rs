use std::path::Path;
use adw::prelude::*;

pub fn show_pacnew_diff_dialog(
    parent: &impl IsA<gtk4::Widget>,
    original_path: &Path,
    candidate_path: &Path,
    on_keep: impl FnOnce() + 'static,
    on_replace: impl FnOnce() + 'static,
) {
    let title = format!(
        "{}: {}",
        pastor_core::tr("Configuration Diff"),
        original_path.file_name().and_then(|n| n.to_str()).unwrap_or("config")
    );

    let window = adw::Window::builder()
        .title(&title)
        .default_width(900)
        .default_height(650)
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
        .label(pastor_core::tr("Keep Current"))
        .tooltip_text(pastor_core::tr("Discard the new .pacnew candidate and preserve existing configuration"))
        .css_classes(["flat"])
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
        .label(pastor_core::tr("Replace with New"))
        .tooltip_text(pastor_core::tr("Overwrite the existing file with the .pacnew configuration"))
        .css_classes(["suggested-action"])
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

    let orig_text = std::fs::read_to_string(original_path)
        .unwrap_or_else(|_| pastor_core::tr("(Original file does not exist or cannot be read)").to_string());
    let cand_text = std::fs::read_to_string(candidate_path)
        .unwrap_or_else(|_| pastor_core::tr("(Candidate .pacnew file cannot be read)").to_string());

    let paned = gtk4::Paned::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .position(440)
        .wide_handle(true)
        .build();

    // Left pane: Current file
    let left_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(4)
        .build();
    let left_lbl = gtk4::Label::builder()
        .label(format!("{}: {}", pastor_core::tr("Current"), original_path.display()))
        .css_classes(["heading", "caption"])
        .halign(gtk4::Align::Start)
        .ellipsize(gtk4::pango::EllipsizeMode::Middle)
        .build();
    let left_tv = gtk4::TextView::builder()
        .monospace(true)
        .editable(false)
        .wrap_mode(gtk4::WrapMode::WordChar)
        .build();
    left_tv.buffer().set_text(&orig_text);
    let left_sw = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .child(&left_tv)
        .build();
    left_box.append(&left_lbl);
    left_box.append(&left_sw);

    // Right pane: .pacnew candidate
    let right_box = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(6)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(4)
        .margin_end(8)
        .build();
    let right_lbl = gtk4::Label::builder()
        .label(format!("{}: {}", pastor_core::tr("New Candidate"), candidate_path.display()))
        .css_classes(["heading", "caption"])
        .halign(gtk4::Align::Start)
        .ellipsize(gtk4::pango::EllipsizeMode::Middle)
        .build();
    let right_tv = gtk4::TextView::builder()
        .monospace(true)
        .editable(false)
        .wrap_mode(gtk4::WrapMode::WordChar)
        .build();
    right_tv.buffer().set_text(&cand_text);
    let right_sw = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .child(&right_tv)
        .build();
    right_box.append(&right_lbl);
    right_box.append(&right_sw);

    paned.set_start_child(Some(&left_box));
    paned.set_end_child(Some(&right_box));

    toolbar_view.set_content(Some(&paned));
    window.set_content(Some(&toolbar_view));
    window.present();
}

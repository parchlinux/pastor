
use adw::prelude::*;
use gtk4::{glib, pango};
use pastor_core::{TransactionEvent, TransactionStep};
use tokio::sync::mpsc::Receiver;

#[derive(Clone)]
pub struct TransactionBar {
    container: gtk4::Box,
    status_label: gtk4::Label,
    progress_bar: gtk4::ProgressBar,
    spinner: gtk4::Spinner,
    log_buffer: gtk4::TextBuffer,
    text_view: gtk4::TextView,
    scrolled: gtk4::ScrolledWindow,
    expander: gtk4::Revealer,
    expand_btn: gtk4::ToggleButton,
    cancel_btn: gtk4::Button,
}

impl TransactionBar {
    pub fn new() -> Self {
        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(6)
            .margin_start(16)
            .margin_end(16)
            .margin_bottom(12)
            .margin_top(8)
            .visible(false)
            .css_classes(["card"])
            .build();

        let top_row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(12)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(12)
            .margin_end(12)
            .build();

        let spinner = gtk4::Spinner::builder()
            .spinning(true)
            .build();
        top_row.append(&spinner);

        let status_label = gtk4::Label::builder()
            .label("Preparing transaction...")
            .css_classes(["heading"])
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .ellipsize(pango::EllipsizeMode::End)
            .build();
        top_row.append(&status_label);

        let cancel_btn = gtk4::Button::builder()
            .icon_name("process-stop-symbolic")
            .tooltip_text("Cancel current transaction")
            .css_classes(["flat", "circular"])
            .visible(false)
            .build();
        top_row.append(&cancel_btn);

        let expand_btn = gtk4::ToggleButton::builder()
            .icon_name("utilities-terminal-symbolic")
            .tooltip_text("Show native transaction console")
            .css_classes(["flat", "circular"])
            .build();
        top_row.append(&expand_btn);

        let close_btn = gtk4::Button::builder()
            .icon_name("window-close-symbolic")
            .tooltip_text("Dismiss transaction")
            .css_classes(["flat", "circular"])
            .build();
        let container_for_close = container.clone();
        close_btn.connect_clicked(move |_| {
            container_for_close.set_visible(false);
        });
        top_row.append(&close_btn);

        let progress_bar = gtk4::ProgressBar::builder()
            .fraction(0.0)
            .margin_start(12)
            .margin_end(12)
            .margin_bottom(8)
            .build();

        // Console expander
        let log_buffer = gtk4::TextBuffer::new(None);
        let text_view = gtk4::TextView::builder()
            .buffer(&log_buffer)
            .editable(false)
            .cursor_visible(false)
            .monospace(true)
            .css_classes(["console-box"])
            .top_margin(8)
            .bottom_margin(8)
            .left_margin(8)
            .right_margin(8)
            .build();

        let scrolled = gtk4::ScrolledWindow::builder()
            .min_content_height(140)
            .max_content_height(200)
            .child(&text_view)
            .build();

        let expander = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .reveal_child(false)
            .child(&scrolled)
            .build();

        let expander_clone = expander.clone();
        expand_btn.connect_toggled(move |btn| {
            expander_clone.set_reveal_child(btn.is_active());
        });

        container.append(&top_row);
        container.append(&progress_bar);
        container.append(&expander);

        Self {
            container,
            status_label,
            progress_bar,
            spinner,
            log_buffer,
            text_view,
            scrolled,
            expander,
            expand_btn,
            cancel_btn,
        }
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    pub fn toggle_console(&self) {
        let is_vis = self.container.is_visible();
        if !is_vis {
            self.container.set_visible(true);
            self.expand_btn.set_active(true);
        } else {
            let active = !self.expand_btn.is_active();
            self.expand_btn.set_active(active);
        }
    }

    #[allow(dead_code)]
    pub fn is_console_expanded(&self) -> bool {
        self.expand_btn.is_active()
    }

    pub fn connect_cancel<F: Fn() + 'static>(&self, f: F) {
        self.cancel_btn.connect_clicked(move |_| {
            f();
        });
    }

    pub fn monitor_transaction(&self, mut rx: Receiver<TransactionEvent>) {
        self.container.set_visible(true);
        self.spinner.start();
        self.cancel_btn.set_visible(true);
        self.log_buffer.set_text("");
        self.progress_bar.set_fraction(0.0);
        self.status_label.set_text("Transaction starting...");

        let status_label = self.status_label.clone();
        let progress_bar = self.progress_bar.clone();
        let spinner = self.spinner.clone();
        let cancel_btn = self.cancel_btn.clone();
        let log_buffer = self.log_buffer.clone();
        let text_view = self.text_view.clone();
        let scrolled = self.scrolled.clone();
        let container = self.container.clone();
        let expander = self.expander.clone();

        glib::spawn_future_local(async move {
            let mut last_message = String::new();
            while let Some(evt) = rx.recv().await {
                let log_text = evt.log_message.trim();

                // If log_message has live details (e.g. download speeds or steps), display it directly in status label
                if !log_text.is_empty() {
                    status_label.set_text(log_text);
                } else {
                    status_label.set_text(&evt.step.description());
                }

                progress_bar.set_fraction(evt.progress_fraction as f64);

                if !log_text.is_empty() && log_text != last_message {
                    last_message = log_text.to_string();
                    let mut end_iter = log_buffer.end_iter();
                    log_buffer.insert(&mut end_iter, &format!("{}\n", log_text));

                    let mark = log_buffer.create_mark(None, &log_buffer.end_iter(), false);
                    text_view.scroll_to_mark(&mark, 0.0, true, 0.0, 1.0);
                    let vadj = scrolled.vadjustment();
                    vadj.set_value(vadj.upper());
                }

                if matches!(evt.step, TransactionStep::Completed | TransactionStep::Cancelled | TransactionStep::Failed(_)) {
                    spinner.stop();
                    cancel_btn.set_visible(false);
                    let container_weak = container.clone();
                    let expander_weak = expander.clone();
                    // Keep visible for at least 15 seconds unless user expanded console, in which case keep until dismissed
                    glib::timeout_add_seconds_local_once(15, move || {
                        if !expander_weak.reveals_child() {
                            container_weak.set_visible(false);
                        }
                    });
                    break;
                }
            }
        });
    }
}

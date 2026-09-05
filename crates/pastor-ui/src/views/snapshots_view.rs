use std::sync::Arc;

use adw::prelude::*;
use gtk4::glib;
use pastor_core::SnapshotBackend;

pub fn create_snapshots_view(
    snapshot_backend: Arc<dyn SnapshotBackend>,
    _on_back: impl Fn() + 'static + Clone,
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
        .margin_start(24)
        .margin_end(24)
        .build();

    // View Title Header
    let title_label = gtk4::Label::builder()
        .label("Snapper System Snapshots")
        .css_classes(["title-2"])
        .halign(gtk4::Align::Start)
        .build();
    content_box.append(&title_label);

    // Banner
    let banner = adw::Banner::builder()
        .title("Parch Store automatically creates a paired Snapper Btrfs snapshot before and after every package transaction.")
        .revealed(true)
        .build();
    content_box.append(&banner);

    // Snapshots List Group
    let list_group = adw::PreferencesGroup::builder()
        .title("Available System Restore Points")
        .description("Snapshots stored on your root Btrfs filesystem subvolume")
        .build();

    let list_box = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();

    let backend_clone = snapshot_backend.clone();
    let list_box_clone = list_box.clone();

    glib::spawn_future_local(async move {
        if let Ok(snaps) = backend_clone.list_snapshots().await {
            for s in snaps {
                let row = adw::ActionRow::builder()
                    .title(format!("#{} — {}", s.number, s.description))
                    .subtitle(format!(
                        "Date: {} • Type: {}",
                        s.date.format("%Y-%m-%d %H:%M:%S UTC"),
                        if s.pre_number.is_some() {
                            "Post-transaction"
                        } else {
                            "Pre-transaction"
                        }
                    ))
                    .build();

                let badge = gtk4::Label::builder()
                    .label(if s.pre_number.is_some() {
                        "post"
                    } else {
                        "pre"
                    })
                    .css_classes(["parch-badge-world"])
                    .valign(gtk4::Align::Center)
                    .build();
                row.add_suffix(&badge);

                list_box_clone.append(&row);
            }
        }
    });

    list_group.add(&list_box);
    content_box.append(&list_group);

    scrolled.set_child(Some(&content_box));
    scrolled.upcast()
}

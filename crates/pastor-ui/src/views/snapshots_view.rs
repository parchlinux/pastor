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
            if snaps.is_empty() {
                let empty_row = adw::ActionRow::builder()
                    .title("No Btrfs restore points found")
                    .subtitle("Snapshots are automatically created during package installations and updates")
                    .sensitive(false)
                    .build();
                list_box_clone.append(&empty_row);
            } else {
                for s in snaps {
                    let is_post = s.pre_number.is_some();
                    let row = adw::ActionRow::builder()
                        .title(format!("#{} - {}", s.number, s.description))
                        .subtitle(format!(
                            "Date: {} • Type: {}",
                            s.date.format("%Y-%m-%d %H:%M:%S UTC"),
                            if is_post {
                                "Post-transaction"
                            } else {
                                "Pre-transaction"
                            }
                        ))
                        .build();

                    let icon = gtk4::Image::from_icon_name("camera-photo-symbolic");
                    row.add_prefix(&icon);

                    // Pre/Post visual distinction (SNAP-002)
                    let (badge_lbl, badge_css) = if is_post {
                        ("Post", "parch-badge-bootc")
                    } else {
                        ("Pre", "parch-badge-world")
                    };

                    let badge = gtk4::Label::builder()
                        .label(badge_lbl)
                        .css_classes([badge_css])
                        .valign(gtk4::Align::Center)
                        .build();
                    row.add_suffix(&badge);

                    // Restore Action Button (SNAP-001)
                    let restore_btn = gtk4::Button::builder()
                        .label("Restore…")
                        .css_classes(["destructive-action", "flat", "pill"])
                        .valign(gtk4::Align::Center)
                        .tooltip_text("Rollback system to this snapshot")
                        .build();

                    let s_copy = s.clone();
                    let row_for_dialog = row.clone();
                    restore_btn.connect_clicked(move |_| {
                        let dialog = adw::AlertDialog::builder()
                            .heading(format!("Restore snapshot #{}?", s_copy.number))
                            .body(format!(
                                "\"{}\"\n\nThis will revert your system to the state at {}.\nAny changes made after this snapshot will be lost.",
                                s_copy.description,
                                s_copy.date.format("%Y-%m-%d %H:%M UTC")
                            ))
                            .build();

                        dialog.add_response("cancel", "Cancel");
                        dialog.add_response("restore", "Restore System");
                        dialog.set_response_appearance("restore", adw::ResponseAppearance::Destructive);
                        dialog.set_default_response(Some("cancel"));
                        dialog.set_close_response("cancel");

                        let s_num = s_copy.number;
                        dialog.choose(Some(&row_for_dialog), gtk4::gio::Cancellable::NONE, move |resp| {
                            if resp == "restore" {
                                tracing::info!("Executing rollback to snapshot #{}", s_num);
                                std::process::Command::new("pkexec")
                                    .args(["snapper", "rollback", &s_num.to_string()])
                                    .spawn()
                                    .ok();
                            }
                        });
                    });

                    row.add_suffix(&restore_btn);
                    list_box_clone.append(&row);
                }
            }
        }
    });

    list_group.add(&list_box);
    content_box.append(&list_group);

    // Uniform AdwClamp constraint (NAV-004)
    let clamp = adw::Clamp::builder()
        .maximum_size(900)
        .child(&content_box)
        .build();

    scrolled.set_child(Some(&clamp));
    scrolled.upcast()
}

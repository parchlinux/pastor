use adw::prelude::*;
use gtk4::gio;

pub const NOTIFICATION_ID_UPDATES: &str = "pastor-updates-available";

/// Sends a desktop notification indicating that system updates are available.
/// Uses org.freedesktop.Notifications via GApplication / GIO.
pub fn send_updates_notification(app: &impl IsA<gio::Application>, update_count: usize) {
    if update_count == 0 {
        return;
    }

    let title = "System Updates Available";
    let body = if update_count == 1 {
        "1 software update is available for your system.".to_string()
    } else {
        format!("{update_count} software updates are available for your system.")
    };

    let notification = gio::Notification::new(title);
    notification.set_body(Some(&body));
    notification.set_default_action("app.open-updates");
    notification.add_button("Review Updates", "app.open-updates");

    let icon = gio::ThemedIcon::new("software-update-available");
    notification.set_icon(&icon);

    app.send_notification(Some(NOTIFICATION_ID_UPDATES), &notification);
}

/// Clears any pending updates notification.
pub fn withdraw_updates_notification(app: &impl IsA<gio::Application>) {
    app.withdraw_notification(NOTIFICATION_ID_UPDATES);
}

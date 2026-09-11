use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc::UnboundedSender;
use ksni::{
    menu::{MenuItem, StandardItem},
    Handle, Status, ToolTip, Tray, TrayMethods,
};

#[derive(Debug, Clone)]
pub enum AppCommand {
    PresentWindow,
    OpenUpdatesView,
    TriggerUpdateCheck,
    NotifyUpdates(usize),
    WithdrawNotification,
    Quit,
}

#[derive(Clone, Default)]
pub struct IndicatorState {
    pub update_count: Arc<AtomicUsize>,
    pub is_checking: Arc<AtomicBool>,
}

pub struct PastorTray {
    state: IndicatorState,
    sender: UnboundedSender<AppCommand>,
}

impl PastorTray {
    pub fn new(state: IndicatorState, sender: UnboundedSender<AppCommand>) -> Self {
        Self { state, sender }
    }
}

impl Tray for PastorTray {
    fn id(&self) -> String {
        "com.parchlinux.pastor".to_string()
    }

    fn title(&self) -> String {
        "Parch Store".to_string()
    }

    fn icon_name(&self) -> String {
        "com.parchlinux.pastor".to_string()
    }

    fn status(&self) -> Status {
        let count = self.state.update_count.load(Ordering::Relaxed);
        if count > 0 {
            Status::NeedsAttention
        } else {
            Status::Active
        }
    }

    fn tool_tip(&self) -> ToolTip {
        let count = self.state.update_count.load(Ordering::Relaxed);
        let is_checking = self.state.is_checking.load(Ordering::Relaxed);

        let description = if is_checking {
            "Checking for system updates…".to_string()
        } else if count == 0 {
            "System is up to date".to_string()
        } else if count == 1 {
            "1 software update available".to_string()
        } else {
            format!("{count} software updates available")
        };

        ToolTip {
            icon_name: "com.parchlinux.pastor".to_string(),
            icon_pixmap: Vec::new(),
            title: "Parch Store".to_string(),
            description,
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.sender.send(AppCommand::PresentWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let count = self.state.update_count.load(Ordering::Relaxed);
        let is_checking = self.state.is_checking.load(Ordering::Relaxed);

        let (updates_label, is_open_action) = if is_checking {
            ("Checking for updates…".to_string(), false)
        } else if count == 0 {
            ("Check for Updates".to_string(), false)
        } else if count == 1 {
            ("Review 1 Update…".to_string(), true)
        } else {
            (format!("Review {count} Updates…"), true)
        };

        let sender_open = self.sender.clone();
        let sender_updates = self.sender.clone();
        let sender_quit = self.sender.clone();

        vec![
            StandardItem {
                label: "Open Parch Store".to_string(),
                icon_name: "com.parchlinux.pastor".to_string(),
                activate: Box::new(move |_| {
                    let _ = sender_open.send(AppCommand::PresentWindow);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: updates_label,
                icon_name: "software-update-available".to_string(),
                enabled: !is_checking,
                activate: Box::new(move |_| {
                    if is_open_action {
                        let _ = sender_updates.send(AppCommand::OpenUpdatesView);
                    } else {
                        let _ = sender_updates.send(AppCommand::TriggerUpdateCheck);
                    }
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".to_string(),
                icon_name: "application-exit".to_string(),
                activate: Box::new(move |_| {
                    let _ = sender_quit.send(AppCommand::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub async fn spawn_indicator(
    state: IndicatorState,
    sender: UnboundedSender<AppCommand>,
) -> Result<Handle<PastorTray>, ksni::Error> {
    let tray = PastorTray::new(state, sender);
    tray.assume_sni_available(true).spawn().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tray_status_and_tooltip_when_up_to_date() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let state = IndicatorState::default();
        let tray = PastorTray::new(state, tx);

        assert_eq!(tray.id(), "com.parchlinux.pastor");
        assert_eq!(tray.title(), "Parch Store");
        assert_eq!(tray.icon_name(), "com.parchlinux.pastor");
        assert_eq!(tray.status(), Status::Active);

        let tip = tray.tool_tip();
        assert_eq!(tip.title, "Parch Store");
        assert_eq!(tip.description, "System is up to date");

        let menu = tray.menu();
        assert_eq!(menu.len(), 4);
    }

    #[test]
    fn test_tray_status_and_tooltip_when_updates_available() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let state = IndicatorState::default();
        state.update_count.store(3, Ordering::SeqCst);
        let tray = PastorTray::new(state, tx);

        assert_eq!(tray.status(), Status::NeedsAttention);

        let tip = tray.tool_tip();
        assert_eq!(tip.description, "3 software updates available");

        let menu = tray.menu();
        assert_eq!(menu.len(), 4);
    }

    #[test]
    fn test_tray_tooltip_when_checking() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let state = IndicatorState::default();
        state.is_checking.store(true, Ordering::SeqCst);
        let tray = PastorTray::new(state, tx);

        let tip = tray.tool_tip();
        assert_eq!(tip.description, "Checking for system updates…");
    }
}

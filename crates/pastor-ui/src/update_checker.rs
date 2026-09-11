use std::sync::atomic::Ordering;
use std::time::Duration;

use ksni::Handle;
use pastor_store::{ActiveTransactionEvent, Store};
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::indicator::{AppCommand, IndicatorState, PastorTray};

pub struct UpdateCheckerOptions {
    pub check_interval: Duration,
    pub startup_delay: Duration,
}

impl Default for UpdateCheckerOptions {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(3600), // 1 hour
            startup_delay: Duration::from_secs(5),     // 5 seconds after startup
        }
    }
}

pub fn spawn_update_checker(
    store: Store,
    state: IndicatorState,
    tray_handle: Option<Handle<PastorTray>>,
    cmd_sender: UnboundedSender<AppCommand>,
    mut manual_trigger_rx: mpsc::Receiver<()>,
    options: UpdateCheckerOptions,
) {
    tokio::spawn(async move {
        // Startup delay before initial check so UI launches immediately
        tokio::time::sleep(options.startup_delay).await;

        let mut tx_receiver = store.subscribe_transactions();
        let mut interval = tokio::time::interval(options.check_interval);
        // interval tick immediately returns on first call, but we already waited startup_delay
        interval.tick().await;

        // Run initial check
        check_now(&store, &state, &tray_handle, &cmd_sender).await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    tracing::info!("Running periodic background update check");
                    check_now(&store, &state, &tray_handle, &cmd_sender).await;
                }
                Some(_) = manual_trigger_rx.recv() => {
                    tracing::info!("Running manual update check triggered from UI or Tray");
                    check_now(&store, &state, &tray_handle, &cmd_sender).await;
                }
                Ok(tx_event) = tx_receiver.recv() => {
                    match tx_event {
                        ActiveTransactionEvent::Completed(_) => {
                            tracing::info!("Package transaction completed; refreshing update status in 3s");
                            tokio::time::sleep(Duration::from_secs(3)).await;
                            check_now(&store, &state, &tray_handle, &cmd_sender).await;
                        }
                        _ => {}
                    }
                }
            }
        }
    });
}

async fn check_now(
    store: &Store,
    state: &IndicatorState,
    tray_handle: &Option<Handle<PastorTray>>,
    cmd_sender: &UnboundedSender<AppCommand>,
) {
    state.is_checking.store(true, Ordering::SeqCst);
    if let Some(handle) = tray_handle {
        let _ = handle.update(|_| {}).await;
    }

    let updates_result = store.get_updates().await;
    state.is_checking.store(false, Ordering::SeqCst);

    match updates_result {
        Ok(updates) => {
            let count = updates.len();
            let prev_count = state.update_count.swap(count, Ordering::SeqCst);

            tracing::info!("Update check finished: {count} update(s) available (previous: {prev_count})");

            if let Some(handle) = tray_handle {
                let _ = handle.update(|_| {}).await;
            }

            if count > 0 && count != prev_count {
                let _ = cmd_sender.send(AppCommand::NotifyUpdates(count));
            } else if count == 0 && prev_count > 0 {
                let _ = cmd_sender.send(AppCommand::WithdrawNotification);
            }
        }
        Err(e) => {
            tracing::warn!("Failed to check for updates: {e}");
            if let Some(handle) = tray_handle {
                let _ = handle.update(|_| {}).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_checker_default_options() {
        let opts = UpdateCheckerOptions::default();
        assert_eq!(opts.check_interval, Duration::from_secs(3600));
        assert_eq!(opts.startup_delay, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_check_now_with_mock_store() {
        let store = Store::new();
        let state = IndicatorState::default();
        let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel();

        // Run check_now with an empty store (no updates initially)
        check_now(&store, &state, &None, &cmd_tx).await;

        assert_eq!(state.update_count.load(Ordering::SeqCst), 0);
        assert_eq!(state.is_checking.load(Ordering::SeqCst), false);
        // With 0 updates, no notification should be sent
        assert!(cmd_rx.try_recv().is_err());
    }
}

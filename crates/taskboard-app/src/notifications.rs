// SPDX-License-Identifier: AGPL-3.0-only
//! The notification sink port (design §6): how a fired reminder
//! notification reaches the user. The v1 implementation is
//! [`LogSink`] — one `tracing` line per delivery; the desktop-toast
//! sink is a later plan and only adds another implementor.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use taskboard_domain::Notification;

/// Channel capacity of the notifier inbox (engine → sink consumer).
/// The engine's tick bursts are bounded by the firing evaluation, so 64
/// is far beyond v1's log-line deliveries; a future OS sink with
/// backpressure policy revisits the number with its design round.
pub const NOTIFIER_CAPACITY: usize = 64;

/// Delivery failure classes of a [`NotificationSink`]. Abstract for
/// now: `LogSink` cannot fail, but the port result must be fallible —
/// an OS toast sink can.
#[derive(Debug, thiserror::Error)]
#[error("notification delivery failed")]
pub struct NotifyError;

/// The port every notification consumer implements. The boxed future
/// keeps async implementors (a desktop sink awaits) possible without a
/// `futures` dependency — the std `Pin<Box<…>>` spelling of the same
/// contract.
pub trait NotificationSink: Send + Sync + std::fmt::Debug {
    /// Delivers one reminder notification.
    ///
    /// # Errors
    ///
    /// [`NotifyError`] when the sink could not deliver; the consumer
    /// logs and continues (one lost toast must not kill the actor).
    fn deliver<'a>(
        &'a self,
        notification: &'a Notification,
    ) -> Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>>;
}

/// The v1 sink: one `tracing::info!` line with typed fields per
/// delivery. It cannot fail.
#[derive(Debug, Default, Clone, Copy)]
pub struct LogSink;

impl NotificationSink for LogSink {
    fn deliver<'a>(
        &'a self,
        notification: &'a Notification,
    ) -> Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>> {
        Box::pin(async move {
            tracing::info!(
                reminder = %notification.reminder.text_form(),
                task = %notification.task.as_uuid(),
                title = %notification.title,
                occurrence = %notification.occurrence.to_rfc3339(),
                "reminder due"
            );
            Ok(())
        })
    }
}

/// Spawns the notifier consumer: drains the engine's notification
/// channel into `sink` until the channel closes (the engine task owns
/// the only sender clone; its exit closes the channel). A failed
/// delivery is logged and skipped — never fatal.
#[must_use]
pub fn spawn_notifier(
    mut notifications: tokio::sync::mpsc::Receiver<Notification>,
    sink: Arc<dyn NotificationSink>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tracing::debug!("notifier: consumer started");
        while let Some(notification) = notifications.recv().await {
            if let Err(err) = sink.deliver(&notification).await {
                tracing::warn!(
                    ?err,
                    reminder = %notification.reminder.text_form(),
                    "notification delivery failed"
                );
            }
        }
        tracing::debug!("notifier: channel closed, consumer exiting");
    })
}

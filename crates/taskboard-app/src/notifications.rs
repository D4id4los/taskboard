// SPDX-License-Identifier: AGPL-3.0-only
//! The notification sink port (design §6): how a fired reminder
//! notification reaches the user. The v1 implementation is
//! [`LogSink`] — one `tracing` line per delivery; [`DesktopSink`]
//! renders the same payload as a passive desktop toast (D-R12) behind
//! the same unmoved port (D-R11); which one a process uses is
//! boot-time config (D-R13).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Local, Utc};

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
            // The typed anchor decides the id field: task firings render
            // `task=`, alarm firings `alarm=` (the DesktopSink of the
            // desktop-toast plan matches the same enum).
            match notification.anchor {
                taskboard_domain::ReminderAnchor::Task(task) => tracing::info!(
                    reminder = %notification.reminder.text_form(),
                    task = %task.as_uuid(),
                    title = %notification.title,
                    occurrence = %notification.occurrence.to_rfc3339(),
                    "reminder due"
                ),
                taskboard_domain::ReminderAnchor::Alarm(alarm) => tracing::info!(
                    reminder = %notification.reminder.text_form(),
                    alarm = %alarm.as_uuid(),
                    title = %notification.title,
                    occurrence = %notification.occurrence.to_rfc3339(),
                    "reminder due"
                ),
            }
            Ok(())
        })
    }
}

/// The desktop-toast sink: renders the payload mapping (D-R11) and
/// hands it to the platform notification service. All DBus/session
/// discovery, per-platform quirks, timeouts, and the app identity
/// live here — the port and its other implementors stay OS-free.
/// Passive toasts only (D-R12): no actions, no sound, no urgency
/// hints, the platform's default timeout — interactive snooze stays
/// in-app. A failed delivery logs the backend cause and surfaces the
/// port's [`NotifyError`] — never retried, never fallback-sent
/// (D-R13).
#[derive(Debug, Default, Clone, Copy)]
pub struct DesktopSink;

impl NotificationSink for DesktopSink {
    fn deliver<'a>(
        &'a self,
        notification: &'a Notification,
    ) -> Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>> {
        Box::pin(async move {
            let payload = toast_payload(notification, Utc::now());
            let mut toast = notify_rust::Notification::new();
            toast
                .appname(payload.appname)
                .summary(&payload.summary)
                .body(&payload.body);
            // The anchor rides in the payload (the `Notification`
            // already carries it) and is rendered nowhere — ready for
            // #203's click-through.
            if let Err(err) = toast.show_async().await {
                // `NotifyError` is a frozen unit struct (port
                // discipline), so the backend cause is logged here
                // before the typed class surfaces at the consumer.
                tracing::warn!(
                    error = %err,
                    reminder = %notification.reminder.text_form(),
                    "desktop toast delivery failed"
                );
                return Err(NotifyError);
            }
            Ok(())
        })
    }
}

/// The typed body decision the tests own: the normal one-tick case
/// (`DueNow`) versus an occurrence still in the future at delivery
/// time — clock drift or an early tick (`AtInstant`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyKind {
    DueNow,
    AtInstant,
}

fn body_kind(occurrence: DateTime<Utc>, now: DateTime<Utc>) -> BodyKind {
    if occurrence <= now {
        BodyKind::DueNow
    } else {
        BodyKind::AtInstant
    }
}

/// The toast payload as the OS boundary receives it: `summary` is the
/// notification's title snapshot (task title or alarm label, D-R6.5),
/// `body` renders the occurrence, `appname` is the fixed identity.
struct ToastPayload {
    summary: String,
    body: String,
    appname: &'static str,
}

/// The pure payload mapping (D-R3 of the stage-1 plan): data-in,
/// data-out, unit-testable without a bus. Anchor-agnostic by
/// construction — both anchors ride the same title snapshot.
fn toast_payload(notification: &Notification, now: DateTime<Utc>) -> ToastPayload {
    let body = match body_kind(notification.occurrence, now) {
        BodyKind::DueNow => "due now".to_owned(),
        // The viewer's local wall time; the phrase text is never
        // asserted (only the typed edge above is).
        BodyKind::AtInstant => format!(
            "fires at {}",
            notification
                .occurrence
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M %Z")
        ),
    };
    ToastPayload {
        summary: notification.title.clone(),
        body,
        appname: "taskboard",
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

#[cfg(test)]
mod tests {
    use super::*;
    use taskboard_domain::{AdhocAlarmId, ReminderAnchor, ReminderRef, TaskId};

    const FIRED_AT: DateTime<Utc> =
        chrono::DateTime::<Utc>::from_timestamp(1_792_000_000, 0).expect("valid instant");

    fn notification(anchor: ReminderAnchor) -> Notification {
        let reminder = match &anchor {
            ReminderAnchor::Task(task) => ReminderRef::Default(*task),
            ReminderAnchor::Alarm(alarm) => ReminderRef::Adhoc(*alarm),
        };
        Notification {
            reminder,
            anchor,
            title: "ship it".into(),
            occurrence: FIRED_AT,
        }
    }

    #[test]
    fn toast_payload_passes_the_title_snapshot_through_for_task_anchors() {
        let payload = toast_payload(
            &notification(ReminderAnchor::Task(TaskId::from_uuid(
                uuid::Uuid::from_u128(4),
            ))),
            FIRED_AT,
        );
        assert_eq!(payload.summary, "ship it");
        assert_eq!(payload.appname, "taskboard");
        assert_ne!(payload.body, "");
    }

    #[test]
    fn toast_payload_passes_the_alarm_label_through_for_alarm_anchors() {
        let payload = toast_payload(
            &notification(ReminderAnchor::Alarm(AdhocAlarmId::from_uuid(
                uuid::Uuid::from_u128(5),
            ))),
            FIRED_AT,
        );
        assert_eq!(payload.summary, "ship it");
        assert_eq!(payload.appname, "taskboard");
        assert_ne!(payload.body, "");
    }

    #[test]
    fn body_kind_edges() {
        // At the instant and in the past: the normal one-tick case.
        assert!(matches!(body_kind(FIRED_AT, FIRED_AT), BodyKind::DueNow));
        assert!(matches!(
            body_kind(FIRED_AT, FIRED_AT + chrono::Duration::minutes(1)),
            BodyKind::DueNow
        ));
        // In the future: clock drift / early tick.
        assert!(matches!(
            body_kind(FIRED_AT, FIRED_AT - chrono::Duration::minutes(1)),
            BodyKind::AtInstant
        ));
    }
}

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

use std::collections::HashMap;

use crate::config::{AppMode, SinkChoice};

use taskboard_domain::{Notification, NotifierMessage, ReminderRef};
// The health snapshot is presentation-side data (one shared shape for
// the app actor and the dumb UI); the watch lane is app-owned.
use taskboard_ui_slint::presentation::DeliveryHealth;

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
    /// Asks the sink to withdraw the OS toast for one dismissed
    /// occurrence (design D-10). Best-effort by contract in BOTH
    /// directions: a sink whose platform cannot withdraw returns
    /// `Ok(())` and does nothing, and a failed attempt surfaces
    /// [`NotifyError`] — logged and dropped by the consumer, never an
    /// error surface. Closing a toast natively is NOT a dismissal (the
    /// in-app surface is the universal source of truth).
    fn withdraw<'a>(
        &'a self,
        reminder: &'a ReminderRef,
        occurrence: &'a chrono::DateTime<chrono::Utc>,
    ) -> Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>>;

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

    fn withdraw<'a>(
        &'a self,
        _reminder: &'a ReminderRef,
        _occurrence: &'a chrono::DateTime<chrono::Utc>,
    ) -> Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
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
///
/// # Withdrawal support (the D-10 platform matrix)
///
/// The sink remembers the platform toast handle of each fire, so a
/// later [`NotificationSink::withdraw`] can close it:
///
/// - **Linux / the BSDs (DBus/FDO)**: withdrawal works where the
///   notification server implements `CloseNotification` — the standard
///   case. Servers that keep toasts fire-and-forget simply ignore the
///   close; the attempt is best-effort and never surfaces.
/// - **Windows / macOS (the other release targets)**: fire-and-forget —
///   the backend's toast handles are not retained closable, so
///   `withdraw` is a typed no-op (the in-app surface clears).
///
/// Tracked handles are capped: past [`DesktopSink::TRACK_CAP`] live
/// toasts the tracked set is dropped and those toasts become
/// fire-and-forget. Closing a toast natively (the user, or the
/// desktop's timeout) is NOT a dismissal — the dismissal is the synced
/// engine fact (design D-10, issue #444 tracks native action buttons).
///
/// # DR-3 fence (round 3, chunk 7)
///
/// Settlement — the delivered-vs-dismissed lifecycle, prune/compaction
/// timing, and the in-app surface's persistence — is DR-3 chunk 1's
/// territory (landed on `main`); this module owns OS delivery and
/// delivery *visibility* only. Neither round re-times the other's fix:
/// the delivered-mark is intentionally unconditional on the sink
/// outcome (the mark is the local receipt guarding the in-app row,
/// the always-on fallback surface). Cross-ref: #474.
#[derive(Default)]
pub struct DesktopSink {
    /// The fire-time toast handles keyed by `(reminder, occurrence)` —
    /// exactly the identity a `Withdraw` message carries.
    recent: std::sync::Mutex<HashMap<ToastKey, notify_rust::NotificationHandle>>,
}

/// The tracked-toast key: the `(reminder, occurrence)` pair.
type ToastKey = (ReminderRef, chrono::DateTime<chrono::Utc>);

impl std::fmt::Debug for DesktopSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopSink")
            .field("tracked", &self.recent.lock().map_or(0, |m| m.len()))
            .finish_non_exhaustive()
    }
}

impl DesktopSink {
    /// The tracked-toast cap (see the platform matrix above).
    const TRACK_CAP: usize = 256;
}

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
            match toast.show_async().await {
                // The handle is the withdrawal permit (the platform
                // matrix above); drop it silently where the OS cannot
                // close toasts (fire-and-forget targets).
                Ok(handle) => {
                    if let Ok(mut recent) = self.recent.lock() {
                        if recent.len() >= Self::TRACK_CAP {
                            recent.clear();
                        }
                        recent.insert((notification.reminder, notification.occurrence), handle);
                    }
                    Ok(())
                }
                Err(err) => {
                    // `NotifyError` is a frozen unit struct (port
                    // discipline), so the backend cause is logged here
                    // before the typed class surfaces at the consumer.
                    tracing::warn!(
                        error = %err,
                        reminder = %notification.reminder.text_form(),
                        "desktop toast delivery failed"
                    );
                    Err(NotifyError)
                }
            }
        })
    }

    fn withdraw<'a>(
        &'a self,
        reminder: &'a ReminderRef,
        occurrence: &'a chrono::DateTime<chrono::Utc>,
    ) -> Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>> {
        Box::pin(async move {
            // Remove first: one withdrawal per tracked toast, whatever
            // the close attempt's outcome (best-effort, never retried).
            let handle = self
                .recent
                .lock()
                .ok()
                .and_then(|mut recent| recent.remove(&(*reminder, *occurrence)));
            if let Some(handle) = handle {
                handle.close_async().await;
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

/// The resolved sink kind — the typed result of the D-R13 matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SinkKind {
    Desktop,
    Log,
}

/// The D-R13 resolution: an explicit choice wins; `auto` consults the
/// mode — desktop toasts, daemon/kiosk keep the log trail. An explicit
/// `desktop` under daemon/kiosk is honored, not refused: the operator
/// who asks for toasts gets a sink whose failures are logged
/// [`NotifyError`]s (degradation governs delivery, not resolution).
pub(crate) fn resolve_sink(choice: SinkChoice, mode: AppMode) -> SinkKind {
    match (choice, mode) {
        (SinkChoice::Auto, AppMode::Desktop) | (SinkChoice::Desktop, _) => SinkKind::Desktop,
        (SinkChoice::Auto, AppMode::Daemon | AppMode::Kiosk) | (SinkChoice::Log, _) => {
            SinkKind::Log
        }
    }
}

/// One delivery-pipeline observation (app-owned, ui-safe data). The
/// consumer emits one event per sink call around every deliver/withdraw;
/// observers turn the stream into logs, health, or test pins. Carries
/// no error cause by design: the backend cause stays where it is
/// readable — the [`DesktopSink`] warn — while this event is the typed
/// record (the chip says *degraded*, the log says *why*).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeliveryEvent {
    Attempted {
        reminder: ReminderRef,
        occurrence: DateTime<Utc>,
    },
    Delivered {
        reminder: ReminderRef,
        occurrence: DateTime<Utc>,
    },
    Failed {
        reminder: ReminderRef,
        occurrence: DateTime<Utc>,
    },
    Withdrawn {
        reminder: ReminderRef,
        occurrence: DateTime<Utc>,
    },
    WithdrawFailed {
        reminder: ReminderRef,
        occurrence: DateTime<Utc>,
    },
}

/// The seam every delivery observation flows through (round-3 D31):
/// implementors may log, tally health, or collect for tests. One
/// stream for every [`NotificationSink`] implementor — the CI fakes
/// included — with no daemon.
pub trait DeliveryObserver: Send + Sync + std::fmt::Debug {
    fn on_event(&self, event: DeliveryEvent);
}

/// The default observer: the seam's optionality — tests and callers
/// that do not care about delivery health compile `spawn_notifier`
/// unchanged with this.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopObserver;

impl DeliveryObserver for NoopObserver {
    fn on_event(&self, _: DeliveryEvent) {}
}

/// The trait-object default ([`NoopObserver`] behind the `Arc`).
#[must_use]
pub fn no_op_observer() -> Arc<dyn DeliveryObserver> {
    Arc::new(NoopObserver)
}

/// The production observer (round-3 D31): tallies the event stream
/// into a [`DeliveryHealth`] snapshot and publishes it on a
/// `tokio::sync::watch` — the status-bar chip's data lane. Withdraw
/// events are best-effort by contract and never move the health.
#[derive(Debug)]
pub(crate) struct HealthTracker {
    health: std::sync::Mutex<DeliveryHealth>,
    tx: tokio::sync::watch::Sender<DeliveryHealth>,
}

impl HealthTracker {
    pub(crate) fn new(tx: tokio::sync::watch::Sender<DeliveryHealth>) -> Self {
        Self {
            health: std::sync::Mutex::new(DeliveryHealth::default()),
            tx,
        }
    }

    /// Applies one event to the tally and publishes the snapshot.
    /// Timestamps are wall-clock telemetry at the consumer (delivery
    /// timing, not engine logic); tests assert the transitions, never
    /// the stamps.
    pub(crate) fn apply(&self, event: &DeliveryEvent) {
        let mut health = self.health.lock().expect("delivery health mutex poisoned");
        match event {
            DeliveryEvent::Attempted { .. } => health.last_attempt = Some(Utc::now()),
            DeliveryEvent::Delivered { .. } => {
                health.last_success = Some(Utc::now());
                health.consecutive_failures = 0;
            }
            DeliveryEvent::Failed { .. } => {
                health.last_failure = Some(Utc::now());
                health.consecutive_failures = health.consecutive_failures.saturating_add(1);
            }
            // Withdraw is best-effort by contract (D-10): neither
            // outcome is a delivery health fact.
            DeliveryEvent::Withdrawn { .. } | DeliveryEvent::WithdrawFailed { .. } => {}
        }
        let _ = self.tx.send(health.clone());
    }
}

impl DeliveryObserver for HealthTracker {
    fn on_event(&self, event: DeliveryEvent) {
        self.apply(&event);
    }
}

/// Spawns the notifier consumer: drains the engine's notification
/// channel into `sink` until the channel closes (the engine task owns
/// the only sender clone; its exit closes the channel). Every sink
/// call is bracketed by [`DeliveryEvent`]s for `observer`; a failed
/// delivery is observed and skipped — never fatal. The consumer keeps
/// no failure log of its own: the sink logs the backend cause, the
/// event stream is the typed record.
#[must_use]
pub fn spawn_notifier(
    mut notifications: tokio::sync::mpsc::Receiver<NotifierMessage>,
    sink: Arc<dyn NotificationSink>,
    observer: Arc<dyn DeliveryObserver>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tracing::debug!("notifier: consumer started");
        while let Some(message) = notifications.recv().await {
            match message {
                NotifierMessage::Fire(notification) => {
                    observer.on_event(DeliveryEvent::Attempted {
                        reminder: notification.reminder,
                        occurrence: notification.occurrence,
                    });
                    if sink.deliver(&notification).await.is_err() {
                        observer.on_event(DeliveryEvent::Failed {
                            reminder: notification.reminder,
                            occurrence: notification.occurrence,
                        });
                    } else {
                        observer.on_event(DeliveryEvent::Delivered {
                            reminder: notification.reminder,
                            occurrence: notification.occurrence,
                        });
                    }
                }
                // Withdrawal failures are observed as best-effort,
                // never surfaced (the in-app notification is the
                // source of truth).
                NotifierMessage::Withdraw {
                    reminder,
                    occurrence,
                } => {
                    if sink.withdraw(&reminder, &occurrence).await.is_err() {
                        observer.on_event(DeliveryEvent::WithdrawFailed {
                            reminder,
                            occurrence,
                        });
                    } else {
                        observer.on_event(DeliveryEvent::Withdrawn {
                            reminder,
                            occurrence,
                        });
                    }
                }
            }
        }
        tracing::debug!("notifier: channel closed, consumer exiting");
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use taskboard_domain::{AdhocAlarmId, NotifierMessage, ReminderAnchor, ReminderRef, TaskId};

    const FIRED_AT: DateTime<Utc> =
        chrono::DateTime::<Utc>::from_timestamp(1_792_000_000, 0).expect("valid instant");

    fn notification(anchor: ReminderAnchor) -> Notification {
        let reminder = match &anchor {
            ReminderAnchor::Task(task) => ReminderRef::Default(*task),
            ReminderAnchor::Alarm(alarm) => ReminderRef::Adhoc(*alarm),
        };
        Notification {
            insistence: taskboard_domain::Insistence::Normal,
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

    #[tokio::test]
    async fn log_sink_withdraw_is_the_contracted_no_op() {
        // D-10: a sink that cannot withdraw answers Ok and does nothing;
        // `LogSink` is that sink on every platform.
        let outcome = LogSink
            .withdraw(
                &ReminderRef::Default(TaskId::from_uuid(uuid::Uuid::from_u128(8))),
                &FIRED_AT,
            )
            .await;
        assert!(outcome.is_ok());
    }

    #[tokio::test]
    async fn the_consumer_routes_withdraw_to_the_sink() {
        // The two-variant channel contract (D-10): Fire reaches
        // `deliver`, Withdraw reaches `withdraw`, in order.
        #[derive(Default)]
        struct CollectingSink {
            fires: std::sync::atomic::AtomicUsize,
            withdrawals: std::sync::atomic::AtomicUsize,
        }
        impl NotificationSink for CollectingSink {
            fn deliver<'a>(
                &'a self,
                _: &'a Notification,
            ) -> Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>> {
                Box::pin(async move {
                    self.fires.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }
            fn withdraw<'a>(
                &'a self,
                _: &'a ReminderRef,
                _: &'a chrono::DateTime<chrono::Utc>,
            ) -> Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>> {
                Box::pin(async move {
                    self.withdrawals.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }
        }
        impl std::fmt::Debug for CollectingSink {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("CollectingSink")
            }
        }

        let sink = Arc::new(CollectingSink::default());
        let (tx, rx) = tokio::sync::mpsc::channel(NOTIFIER_CAPACITY);
        let consumer = spawn_notifier(
            rx,
            Arc::clone(&sink) as Arc<dyn NotificationSink>,
            no_op_observer(),
        );
        tx.send(NotifierMessage::Fire(notification(ReminderAnchor::Task(
            TaskId::from_uuid(uuid::Uuid::from_u128(9)),
        ))))
        .await
        .expect("fire send");
        tx.send(NotifierMessage::Withdraw {
            reminder: ReminderRef::Default(TaskId::from_uuid(uuid::Uuid::from_u128(9))),
            occurrence: FIRED_AT,
        })
        .await
        .expect("withdraw send");
        drop(tx);
        consumer.await.expect("consumer survives both variants");
        assert_eq!(sink.fires.load(Ordering::SeqCst), 1);
        assert_eq!(sink.withdrawals.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn resolve_sink_matrix() {
        use AppMode::{Daemon, Desktop, Kiosk};
        // auto resolves by mode; an explicit choice wins over any mode.
        let cells = [
            (SinkChoice::Auto, Daemon, SinkKind::Log),
            (SinkChoice::Auto, Desktop, SinkKind::Desktop),
            (SinkChoice::Auto, Kiosk, SinkKind::Log),
            (SinkChoice::Log, Daemon, SinkKind::Log),
            (SinkChoice::Log, Desktop, SinkKind::Log),
            (SinkChoice::Log, Kiosk, SinkKind::Log),
            (SinkChoice::Desktop, Daemon, SinkKind::Desktop),
            (SinkChoice::Desktop, Desktop, SinkKind::Desktop),
            (SinkChoice::Desktop, Kiosk, SinkKind::Desktop),
        ];
        for (choice, mode, expected) in cells {
            assert_eq!(
                resolve_sink(choice, mode),
                expected,
                "{choice:?} x {mode:?}"
            );
        }
    }

    #[tokio::test]
    async fn failed_delivery_does_not_kill_the_consumer() {
        // D-R13's "never fatal" contract, pinned at its lowest layer:
        // the first delivery errs, the consumer logs and continues, the
        // second lands. Deterministic: channels + join, no sleeps.
        struct FailingSink {
            calls: std::sync::atomic::AtomicUsize,
            failures: std::sync::atomic::AtomicUsize,
            deliveries: std::sync::atomic::AtomicUsize,
        }
        impl NotificationSink for FailingSink {
            fn deliver<'a>(
                &'a self,
                _notification: &'a Notification,
            ) -> Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>> {
                Box::pin(async move {
                    if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        self.failures.fetch_add(1, Ordering::SeqCst);
                        return Err(NotifyError);
                    }
                    self.deliveries.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }

            fn withdraw<'a>(
                &'a self,
                _: &'a ReminderRef,
                _: &'a chrono::DateTime<chrono::Utc>,
            ) -> Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>> {
                Box::pin(async move { Ok(()) })
            }
        }
        impl std::fmt::Debug for FailingSink {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("FailingSink")
            }
        }

        let sink = Arc::new(FailingSink {
            calls: std::sync::atomic::AtomicUsize::new(0),
            failures: std::sync::atomic::AtomicUsize::new(0),
            deliveries: std::sync::atomic::AtomicUsize::new(0),
        });
        let (tx, rx) = tokio::sync::mpsc::channel(NOTIFIER_CAPACITY);
        let consumer = spawn_notifier(
            rx,
            Arc::clone(&sink) as Arc<dyn NotificationSink>,
            no_op_observer(),
        );
        let fire = |u: u128| {
            NotifierMessage::Fire(notification(ReminderAnchor::Task(TaskId::from_uuid(
                uuid::Uuid::from_u128(u),
            ))))
        };
        tx.send(fire(6)).await.expect("first send");
        tx.send(fire(7)).await.expect("second send");
        drop(tx);
        consumer.await.expect("consumer survives a failed delivery");
        // Both notifications were offered, exactly the first failed,
        // the second was delivered.
        assert_eq!(sink.calls.load(Ordering::SeqCst), 2);
        assert_eq!(sink.failures.load(Ordering::SeqCst), 1);
        assert_eq!(sink.deliveries.load(Ordering::SeqCst), 1);
    }

    /// The seam-test observer: forwards every event into an unbounded
    /// channel, so a test joins the consumer and drains the stream
    /// (channels + join, no sleeps).
    #[derive(Debug)]
    struct CollectingObserver(tokio::sync::mpsc::UnboundedSender<DeliveryEvent>);

    impl DeliveryObserver for CollectingObserver {
        fn on_event(&self, event: DeliveryEvent) {
            let _ = self.0.send(event);
        }
    }

    fn collecting_observer() -> (
        Arc<dyn DeliveryObserver>,
        tokio::sync::mpsc::UnboundedReceiver<DeliveryEvent>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (Arc::new(CollectingObserver(tx)), rx)
    }

    fn fire_of(u: u128) -> NotifierMessage {
        NotifierMessage::Fire(notification(ReminderAnchor::Task(TaskId::from_uuid(
            uuid::Uuid::from_u128(u),
        ))))
    }

    fn expected(
        event: impl FnOnce(ReminderRef, DateTime<Utc>) -> DeliveryEvent,
        u: u128,
    ) -> DeliveryEvent {
        event(
            ReminderRef::Default(TaskId::from_uuid(uuid::Uuid::from_u128(u))),
            FIRED_AT,
        )
    }

    fn attempted(r: ReminderRef, o: DateTime<Utc>) -> DeliveryEvent {
        DeliveryEvent::Attempted {
            reminder: r,
            occurrence: o,
        }
    }

    fn delivered(r: ReminderRef, o: DateTime<Utc>) -> DeliveryEvent {
        DeliveryEvent::Delivered {
            reminder: r,
            occurrence: o,
        }
    }

    fn failed(r: ReminderRef, o: DateTime<Utc>) -> DeliveryEvent {
        DeliveryEvent::Failed {
            reminder: r,
            occurrence: o,
        }
    }

    fn withdrawn(r: ReminderRef, o: DateTime<Utc>) -> DeliveryEvent {
        DeliveryEvent::Withdrawn {
            reminder: r,
            occurrence: o,
        }
    }

    fn withdraw_failed(r: ReminderRef, o: DateTime<Utc>) -> DeliveryEvent {
        DeliveryEvent::WithdrawFailed {
            reminder: r,
            occurrence: o,
        }
    }

    #[tokio::test]
    async fn the_consumer_emits_attempt_then_delivered_for_a_landing_fire() {
        let (observer, mut events) = collecting_observer();
        let (tx, rx) = tokio::sync::mpsc::channel(NOTIFIER_CAPACITY);
        let consumer = spawn_notifier(rx, Arc::new(LogSink), observer);
        tx.send(fire_of(11)).await.expect("fire send");
        drop(tx);
        consumer.await.expect("consumer joins");
        let mut seen = Vec::new();
        while let Ok(event) = events.try_recv() {
            seen.push(event);
        }
        assert_eq!(
            seen,
            vec![expected(attempted, 11), expected(delivered, 11),]
        );
    }

    #[tokio::test]
    async fn the_consumer_emits_attempt_then_failed_for_a_refusing_sink() {
        // The first fire errs, the second lands: exactly one
        // Attempted→Failed bracket, then Attempted→Delivered — and the
        // consumer survives (the never-fatal contract, re-pinned
        // through the seam).
        struct RefusingSink {
            calls: std::sync::atomic::AtomicUsize,
        }
        impl NotificationSink for RefusingSink {
            fn deliver<'a>(
                &'a self,
                _: &'a Notification,
            ) -> Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>> {
                Box::pin(async move {
                    if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        return Err(NotifyError);
                    }
                    Ok(())
                })
            }
            fn withdraw<'a>(
                &'a self,
                _: &'a ReminderRef,
                _: &'a chrono::DateTime<chrono::Utc>,
            ) -> Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>> {
                Box::pin(async move { Ok(()) })
            }
        }
        impl std::fmt::Debug for RefusingSink {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("RefusingSink")
            }
        }

        let (observer, mut events) = collecting_observer();
        let (tx, rx) = tokio::sync::mpsc::channel(NOTIFIER_CAPACITY);
        let consumer = spawn_notifier(
            rx,
            Arc::new(RefusingSink {
                calls: std::sync::atomic::AtomicUsize::new(0),
            }),
            observer,
        );
        tx.send(fire_of(12)).await.expect("first send");
        tx.send(fire_of(13)).await.expect("second send");
        drop(tx);
        consumer.await.expect("consumer survives a failed delivery");
        let mut seen = Vec::new();
        while let Ok(event) = events.try_recv() {
            seen.push(event);
        }
        assert_eq!(
            seen,
            vec![
                expected(attempted, 12),
                expected(failed, 12),
                expected(attempted, 13),
                expected(delivered, 13),
            ]
        );
    }

    #[tokio::test]
    async fn withdraw_outcomes_are_observed_as_best_effort() {
        struct WithdrawSink {
            fail: bool,
        }
        impl NotificationSink for WithdrawSink {
            fn deliver<'a>(
                &'a self,
                _: &'a Notification,
            ) -> Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>> {
                Box::pin(async move { Ok(()) })
            }
            fn withdraw<'a>(
                &'a self,
                _: &'a ReminderRef,
                _: &'a chrono::DateTime<chrono::Utc>,
            ) -> Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>> {
                Box::pin(async move { if self.fail { Err(NotifyError) } else { Ok(()) } })
            }
        }
        impl std::fmt::Debug for WithdrawSink {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("WithdrawSink")
            }
        }

        let (observer, mut events) = collecting_observer();
        let (tx, rx) = tokio::sync::mpsc::channel(NOTIFIER_CAPACITY);
        let consumer = spawn_notifier(rx, Arc::new(WithdrawSink { fail: false }), observer);
        tx.send(NotifierMessage::Withdraw {
            reminder: ReminderRef::Default(TaskId::from_uuid(uuid::Uuid::from_u128(14))),
            occurrence: FIRED_AT,
        })
        .await
        .expect("withdraw send");
        drop(tx);
        consumer.await.expect("consumer joins");
        let mut seen = Vec::new();
        while let Ok(event) = events.try_recv() {
            seen.push(event);
        }
        assert_eq!(seen, vec![expected(withdrawn, 14)]);

        // The failing close surfaces WithdrawFailed — and moves no
        // delivery health (withdraw is best-effort by contract).
        let (observer, mut events) = collecting_observer();
        let (tx, rx) = tokio::sync::mpsc::channel(NOTIFIER_CAPACITY);
        let consumer = spawn_notifier(rx, Arc::new(WithdrawSink { fail: true }), observer);
        tx.send(NotifierMessage::Withdraw {
            reminder: ReminderRef::Default(TaskId::from_uuid(uuid::Uuid::from_u128(15))),
            occurrence: FIRED_AT,
        })
        .await
        .expect("withdraw send");
        drop(tx);
        consumer.await.expect("consumer joins");
        let mut seen = Vec::new();
        while let Ok(event) = events.try_recv() {
            seen.push(event);
        }
        assert_eq!(seen, vec![expected(withdraw_failed, 15)]);

        let (health_tx, health_rx) = tokio::sync::watch::channel(DeliveryHealth::default());
        let tracker = HealthTracker::new(health_tx);
        tracker.apply(&expected(withdraw_failed, 15));
        assert!(!health_rx.borrow().degraded());
    }

    #[test]
    fn health_tracker_transitions_degraded_on_failure_and_clears_on_success() {
        let (health_tx, health_rx) = tokio::sync::watch::channel(DeliveryHealth::default());
        let tracker = HealthTracker::new(health_tx);
        assert!(!health_rx.borrow().degraded());
        tracker.apply(&expected(attempted, 16));
        tracker.apply(&expected(failed, 16));
        {
            let health = health_rx.borrow();
            assert!(health.degraded());
            assert_eq!(health.consecutive_failures, 1);
        }
        tracker.apply(&expected(attempted, 16));
        tracker.apply(&expected(delivered, 16));
        {
            let health = health_rx.borrow();
            assert!(!health.degraded());
            assert_eq!(health.consecutive_failures, 0);
        }
    }

    #[test]
    fn no_op_observer_is_a_no_op() {
        // The seam's optionality: the default observer swallows the
        // stream without effect.
        NoopObserver.on_event(expected(attempted, 17));
    }
}

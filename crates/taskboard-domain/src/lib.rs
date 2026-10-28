// SPDX-License-Identifier: MIT OR Apache-2.0
//! Pure domain types, entities, and inter-actor protocols for `taskboard`.
//!
//! This crate is the functional core of the system. It must remain pure:
//! zero network, zero UI, zero persistence, and zero asynchronous runtime
//! dependencies. All inter-actor communication types ([`StateCommand`]-style
//! message payloads) live here so that every other workspace crate can
//! share them without coupling to each other.
//!
//! Design rules (see `AGENTS.md`):
//! - Domain logic is expressed as pure functions: inputs in, outputs out,
//!   no hidden mutable state. Merge functions never read the clock or
//!   generate ids — time and identity are always data passed in.
//! - IO interfaces (ports, e.g. [`TaskRepository`]) are defined here as
//!   traits returning [`BoxFuture`] (no `async-trait`/`tokio`) and
//!   implemented by adapter crates (`taskboard-storage-sqlite`).
//! - The sync conflict policy ([`apply_sync_report`] and the per-entity
//!   primitives) is executed by the state engine, never by the sync actor.
//!
//! Module map: identifiers ([`ids`]), time seam ([`clock`]), identity
//! seam ([`idgen`]), entities ([`entities`]), state ([`state`]), remote
//! views ([`remote`]), outbox ([`outbox`]), persistence port
//! ([`persistence`]) with the shared batch semantics ([`apply_actions`]),
//! messages ([`messages`]), local command semantics ([`command`]),
//! conflict-policy algebra ([`merge`]) and its fixed-order composition
//! ([`pipeline`]), reminders ([`reminders`]), day boundaries
//! ([`days`]), recurring-series rules ([`recur`]).

#![forbid(unsafe_code)]

pub mod appdata;
pub mod capabilities;
pub mod clock;
pub mod command;
pub mod days;
pub mod entities;
pub mod find;
pub mod habit;
pub mod idgen;
pub mod ids;
pub mod merge;
pub mod messages;
pub mod outbox;
pub mod persistence;
pub mod pipeline;
pub mod push;
pub mod reconcile;
pub mod recur;
pub mod reminders;
pub mod remote;
pub mod state;
pub mod tracking;

/// Kani bounded-model-checking harnesses (compiled only under Kani's
/// own rustc driver — never by tests, clippy, or coverage).
#[cfg(kani)]
mod verify;

/// Compiled for our own tests (dev-dep `proptest`) and for downstream
/// crates that enable the `test-support` feature.
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
pub(crate) mod merge_testutil;

pub use appdata::{
    ANCHOR_CARD_TITLE, APPDATA_FORMAT, AppDataDocument, AppDataSections, DismissalEntry,
    DismissalsEntry, MODELED_TASK_FIELD_KEYS, ModeledValue, RuleEntry, RulesEntry, TaskEntry,
    TaskFieldWrite, TimeEntryRecord, derive_rule_tombstones, dismissal_wire_key,
    dismissals_entry_for, lane_gc_scheduled, modeled_key, prune_absent_cards, rules_entry_for,
    task_entry_for, union,
};
pub use capabilities::{DeckFeatures, flags_for};
pub use clock::{Clock, SystemClock};
pub use command::{
    CommandError, CommandOutcome, CommandPlan, bind_wipe_count, bind_wipe_forecast, plan_command,
    plan_day_rolls,
};
pub use days::{DayCalendar, day_window, window_start};
pub use entities::{
    Board, BoardClocks, Color, Effort, Importance, Label, LabelClocks, Stack, StackClocks, Task,
    TaskClocks, Urgency,
};
pub use find::{MatchQuality, SearchHit, SearchTarget, find};
pub use habit::{
    Habit, HabitClocks, HabitLogEntry, HabitLogKey, HabitState, HabitTarget, InvalidHabitTarget,
    NonZeroCount, PeriodKind, WindowStart, log_window, period_after, period_before, period_end,
    period_start, streak,
};
pub use idgen::{IdGenerator, UuidV7Generator};
pub use ids::{
    AdhocAlarmId, BoardId, ConnectorId, HabitId, LabelId, ReminderRuleId, RemoteBoardBinding,
    RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef, RemoteStackId,
    RemoteStackRef, SeriesId, StackId, TaskId, TimeEntryId,
};
pub use merge::{
    RuleFoldChanges, adopt_board_if_newer, adopt_label_after_push, adopt_remote_board,
    adopt_remote_label, adopt_remote_stack, adopt_remote_task, adopt_stack_after_push,
    adopt_task_after_push, finalize_pushed_task_delete, fold_reminder_dismissals,
    fold_reminder_rules, merge_label, merge_stack, merge_task, tombstone_label, tombstone_stack,
    tombstone_task,
};
pub use messages::{
    AppDataEvidence, AppDataUnreadable, EngineSignal, HabitChanges, LabelChanges, StateCommand,
    SyncCommand, SyncReport, SystemEvent, TaskChanges,
};
pub use outbox::{LocalOp, OpId, PendingOp};
pub use persistence::{
    BoxFuture, PersistedState, PersistenceAction, RepositoryError, SyncStateReader, SyncValidators,
    TaskRepository, ValidatorKey, apply_actions,
};
pub use pipeline::{apply_push_report, apply_sync_report};
pub use push::{
    CardShape, EntityTables, MaterializedOp, NewCardShape, PushGroup, PushTarget, plan_pushes,
};
pub use reconcile::{ReconcileCounts, normalized_title, plan_reconcile};
pub use recur::{
    DayOfMonth, EveryDays, EveryWeeks, InvalidRecur, MonthlyAnchor, NthWeek, OccurrenceCount,
    OccurrenceOutcome, RecurBound, RecurPattern, RecurRule, WeekdaySet, next_after,
    occurrence_outcome, occurs_on, roll_target, schedule,
};
pub use reminders::{
    AdhocAlarm, AdhocWhen, Insistence, InvalidReminderOffset, Notification, NotifierMessage,
    ReminderAnchor, ReminderDefaults, ReminderDelivery, ReminderDismissal, ReminderFiring,
    ReminderOffset, ReminderRef, ReminderRule, ReminderSnooze, ReminderSnoozeKey, ReminderState,
    active_notifications, alarm_occurrence, due_firings, due_soon, effective_occurrence,
    notifications_for, occurrence_of, overdue_tasks, prune_settled_alarms,
};
pub use remote::{
    BoardPullValidators, PushOutcome, PushResult, RemoteBoard, RemoteBoardSnapshot, RemoteEcho,
    RemoteIndex, RemoteLabel, RemoteStack, RemoteTask, resolve_label,
};
pub use state::{
    AppState, SidecarHealth, SyncCounters, SyncErrorKind, SyncPhase, SyncStates, SyncStatus,
    aggregate_rows, attributed_board, derive_sidecar_health, derive_sync_states,
};
pub use tracking::{ClosedSpan, SpanRejected, TimeEntry, TrackingState, tracked_in_window};

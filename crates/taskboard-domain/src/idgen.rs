// SPDX-License-Identifier: MIT OR Apache-2.0
//! Identity seam.
//!
//! Merge and pipeline functions never generate ids: identity is always
//! *data* passed into them. Production wiring uses [`UuidV7Generator`];
//! tests inject a deterministic counter generator
//! (`merge_testutil::CountingIds`). (The time seam lives in
//! [`crate::clock`].)

use std::sync::{Mutex, OnceLock};

use uuid::{ContextV7, Timestamp, Uuid};

use crate::habit_event::HabitEventId;
use crate::ids::{
    AdhocAlarmId, BoardId, BreakRuleId, HabitId, JournalId, LabelId, ReminderRuleId, StackId,
    TaskId, TimeEntryId,
};
use crate::outbox::OpId;

/// The process-global v7 minting context (#1488). `ContextV7` is
/// `!Sync` (a `Cell`-based counter), so the global carries it behind a
/// `Mutex`; minting is rare (user actions), the lock is uncontended in
/// practice, and the counter's monotonicity guarantee is what #1488
/// needs — it survives only if all mints share one context.
///
/// # Behavior identity (#1488)
///
/// Before this context, `Uuid::now_v7()` built
/// `Timestamp::now(NoContext)` — same-millisecond ids ordered by
/// random low bits, so rare `(at, id)` tie inversions were possible.
/// With the shared context, same-millisecond mints increment a 42-bit
/// counter and are **locally totally ordered** (the only guarantee
/// uuid makes: monotonicity per process). Tie *outcomes* corpus-wide
/// change: every persisted `(clock, id)` duel rank minted after this
/// change is counter-ordered. Existing wire rows keep their stored
/// ids — nothing re-mints; `merge_testutil::CountingIds` (the test
/// seam) is untouched.
static V7_CONTEXT: OnceLock<Mutex<ContextV7>> = OnceLock::new();

/// Shared by every v7 mint arm: one process-global counter context,
/// poison-recovered (a poisoned lock still hands the counter back —
/// minting must not panic because a sibling mint panicked while
/// holding it).
fn v7_now() -> Uuid {
    let ctx = V7_CONTEXT.get_or_init(|| Mutex::new(ContextV7::new()));
    let guard = ctx
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Uuid::new_v7(Timestamp::now(&*guard))
}

/// Source of fresh local entity ids (`UUIDv7`).
pub trait IdGenerator: Send + Sync + std::fmt::Debug {
    /// A fresh board id.
    fn new_board_id(&self) -> BoardId;
    /// A fresh stack id.
    fn new_stack_id(&self) -> StackId;
    /// A fresh task id.
    fn new_task_id(&self) -> TaskId;
    /// A fresh reminder-rule id.
    fn new_reminder_rule_id(&self) -> ReminderRuleId;
    /// A fresh time-entry id.
    fn new_time_entry_id(&self) -> TimeEntryId;
    /// A fresh adhoc-alarm id.
    fn new_adhoc_alarm_id(&self) -> AdhocAlarmId;
    /// A fresh break-rule id.
    fn new_break_rule_id(&self) -> BreakRuleId;
    /// A fresh label id.
    fn new_label_id(&self) -> LabelId;
    /// A fresh habit id.
    fn new_habit_id(&self) -> HabitId;
    /// A fresh series id (the recurring-series identity riding a rule).
    fn new_series_id(&self) -> crate::ids::SeriesId;
    /// A fresh journal-entry id.
    fn new_journal_id(&self) -> JournalId;
    /// A fresh habit-time-entry id (#1305, the `new_time_entry_id`
    /// twin).
    fn new_habit_time_entry_id(&self) -> crate::ids::HabitTimeEntryId;
    /// A fresh habit-event id (#1519, the completion/skip-intent
    /// event log's roaming wire key).
    fn new_habit_event_id(&self) -> HabitEventId;
    /// A fresh reflection-entry id.
    fn new_reflection_id(&self) -> crate::ids::ReflectionId;
    /// A fresh outbox-operation id.
    fn new_op_id(&self) -> OpId;
}

/// Production generator: a fresh `v7_now()` per id kind.
#[derive(Debug, Clone, Copy, Default)]
pub struct UuidV7Generator;

impl IdGenerator for UuidV7Generator {
    fn new_board_id(&self) -> BoardId {
        BoardId::from(v7_now())
    }

    fn new_stack_id(&self) -> StackId {
        StackId::from(v7_now())
    }

    fn new_task_id(&self) -> TaskId {
        TaskId::from(v7_now())
    }

    fn new_reminder_rule_id(&self) -> ReminderRuleId {
        ReminderRuleId::from(v7_now())
    }

    fn new_break_rule_id(&self) -> BreakRuleId {
        BreakRuleId::from(v7_now())
    }

    fn new_time_entry_id(&self) -> TimeEntryId {
        TimeEntryId::from(v7_now())
    }

    fn new_adhoc_alarm_id(&self) -> AdhocAlarmId {
        AdhocAlarmId::from(v7_now())
    }

    fn new_label_id(&self) -> LabelId {
        LabelId::from(v7_now())
    }

    fn new_habit_id(&self) -> HabitId {
        HabitId::from(v7_now())
    }

    fn new_series_id(&self) -> crate::ids::SeriesId {
        crate::ids::SeriesId::from(v7_now())
    }

    fn new_journal_id(&self) -> JournalId {
        JournalId::from(v7_now())
    }

    fn new_habit_time_entry_id(&self) -> crate::ids::HabitTimeEntryId {
        crate::ids::HabitTimeEntryId::from(v7_now())
    }

    fn new_habit_event_id(&self) -> HabitEventId {
        HabitEventId::from(v7_now())
    }

    fn new_reflection_id(&self) -> crate::ids::ReflectionId {
        crate::ids::ReflectionId::from(v7_now())
    }

    fn new_op_id(&self) -> OpId {
        OpId(v7_now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore)] // production generator reads the realtime clock (Uuid::now_v7) — environment gate
    fn v7_generator_returns_distinct_correctly_typed_ids() {
        let generator = UuidV7Generator;
        let a = generator.new_task_id();
        let b = generator.new_task_id();
        assert_ne!(a, b, "v7 generation must not repeat");
        let _ = (
            generator.new_board_id(),
            generator.new_stack_id(),
            generator.new_label_id(),
            generator.new_reminder_rule_id(),
            generator.new_time_entry_id(),
        );
        let _ = generator.new_op_id();
        // v7 = time-ordered: the version nibble is 7.
        assert_eq!(a.as_uuid().get_version_num(), 7);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // production generator reads the realtime clock (v7_now) — environment gate
    fn context_v7_mints_locally_total_order() {
        // #1488: under the shared process-global context, consecutive
        // mints strictly increase in uuid byte order — including
        // within one wall-clock millisecond, where the counter leg
        // (not the clock) carries the ordering. No sleep, no timing
        // assertion: a loop fast enough to stay inside one
        // millisecond is exactly the case the counter exists for.
        let generator = UuidV7Generator;
        let mut previous = generator.new_habit_event_id();
        assert_eq!(previous.as_uuid().get_version_num(), 7);
        for _ in 0..10_000 {
            let next = generator.new_habit_event_id();
            assert!(
                previous.as_uuid().as_bytes() < next.as_uuid().as_bytes(),
                "consecutive v7 mints must strictly increase in byte order"
            );
            previous = next;
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)] // production generator reads the realtime clock (v7_now) — environment gate
    fn new_habit_event_id_returns_distinct_v7_ids() {
        let generator = UuidV7Generator;
        let a = generator.new_habit_event_id();
        let b = generator.new_habit_event_id();
        assert_ne!(a, b);
        assert_eq!(a.as_uuid().get_version_num(), 7);
        assert_eq!(b.as_uuid().get_version_num(), 7);
    }
}

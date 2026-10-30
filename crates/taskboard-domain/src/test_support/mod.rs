// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reusable test support: property strategies, an in-memory
//! [`TaskRepository`](crate::persistence::TaskRepository) fake, and the
//! repository contract harness.
//!
//! Ships behind the non-default `test-support` feature so later-phase
//! crates (which cannot share dev-dependencies) can depend on it in tests.
//!
//! Layout mirrors the crate: entity strategies live in [`entities`],
//! remote-view/snapshot strategies in [`remote`], reminder strategies in
//! [`reminders`], state-level strategies in [`state`], and changeset
//! strategies in [`messages`]. The composite
//! strategies assemble from named row structs (not flat tuples) so the
//! referential-consistency logic stays readable; all public functions are
//! re-exported here flat, so consumers import
//! `taskboard_domain::test_support::<name>` regardless of file.

pub mod contract;
pub use memory::InMemoryRepository;
pub mod entities;
pub mod habit_strategies;
pub mod memory;
pub mod messages;
pub mod recur;
pub mod reminders;
pub mod remote;
pub mod state;

pub use entities::{
    effort_strategy, importance_strategy, label_clocks_strategy, membership_map_strategy,
    membership_state_strategy, remote_task_baseline_strategy, stack_clocks_strategy,
    task_clocks_strategy, task_strategy, urgency_strategy,
};
pub use messages::{label_changes_strategy, state_command_strategy, task_changes_strategy};
pub use recur::{
    naive_date_strategy, occurrence_count_strategy, recur_pattern_strategy, recur_rule_strategy,
    weekday_set_strategy, weekday_strategy,
};
pub use reminders::{
    adhoc_alarm_id_strategy, adhoc_alarm_strategy, build_reminders, insistence_strategy,
    reminder_delivery_strategy, reminder_offset_strategy, reminder_ref_strategy,
    reminder_rows_strategy, reminder_rule_id_strategy, reminder_rule_strategy,
    reminder_snooze_strategy, task_map_strategy,
};
pub use remote::{push_outcome_strategy, remote_task_strategy, snapshot_strategy};
pub use state::{app_state_strategy, persisted_state_strategy};
pub mod tracking_strategies;
pub use habit_strategies::{
    HabitFixture, habit_fixture_strategy, habit_strategy, habit_target_strategy,
    period_kind_strategy, quota_strategy,
};
pub use tracking_strategies::{time_entry_strategy, tracking_state_strategy};

use chrono::{DateTime, TimeZone, Utc};
use proptest::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

use crate::idgen::IdGenerator;
use crate::ids::{
    AdhocAlarmId, BoardId, HabitId, JournalId, LabelId, ReminderRuleId, StackId, TaskId,
    TimeEntryId,
};
use crate::outbox::OpId;

/// Deterministic id source: UUIDs from a monotonically increasing
/// counter. Promoted from the domain's `cfg(test)` fixtures so downstream
/// crates (state engine tests and later) reuse one fake instead of
/// re-declaring it. Ids are stable across a scenario: the n-th id request
/// always yields the same UUID.
#[derive(Debug, Default)]
pub struct CountingIds(AtomicU64);

impl CountingIds {
    /// A fresh counter starting at the first UUID.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn next(&self) -> u128 {
        u128::from(self.0.fetch_add(1, Ordering::SeqCst) + 1)
    }
}

impl IdGenerator for CountingIds {
    fn new_board_id(&self) -> BoardId {
        BoardId::from(Uuid::from_u128(self.next()))
    }

    fn new_stack_id(&self) -> StackId {
        StackId::from(Uuid::from_u128(self.next()))
    }

    fn new_task_id(&self) -> TaskId {
        TaskId::from(Uuid::from_u128(self.next()))
    }

    fn new_reminder_rule_id(&self) -> ReminderRuleId {
        ReminderRuleId::from(Uuid::from_u128(self.next()))
    }

    fn new_time_entry_id(&self) -> TimeEntryId {
        TimeEntryId::from(Uuid::from_u128(self.next()))
    }

    fn new_adhoc_alarm_id(&self) -> AdhocAlarmId {
        AdhocAlarmId::from(Uuid::from_u128(self.next()))
    }

    fn new_label_id(&self) -> LabelId {
        LabelId::from(Uuid::from_u128(self.next()))
    }

    fn new_habit_id(&self) -> HabitId {
        HabitId::from(Uuid::from_u128(self.next()))
    }

    fn new_journal_id(&self) -> JournalId {
        JournalId::from(Uuid::from_u128(self.next()))
    }

    fn new_op_id(&self) -> OpId {
        OpId(Uuid::from_u128(self.next()))
    }
}

/// The standard proptest config, Miri-safe: failure persistence
/// canonicalizes the source path at runner startup, which aborts under
/// Miri isolation (`getcwd`). Persistence only remembers shrinking seeds
/// between runs — disabling it under Miri is an environment gate, not a
/// weakened assertion. `PROPTEST_CASES` still overrides `cases` from the
/// environment (slow-interpreter CI capping).
#[must_use]
pub fn proptest_config(cases: u32) -> proptest::test_runner::Config {
    let mut config = proptest::test_runner::Config::with_cases(cases);
    if cfg!(miri) {
        config.failure_persistence = None;
    }
    config
}

/// Bounded, serde-stable strings (no control characters).
pub fn string_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 ._/-]{0,32}"
}

/// Logical timestamps across a sane range (1971–2100), seconds resolution.
pub fn timestamp_strategy() -> impl Strategy<Value = DateTime<Utc>> {
    (31_536_000_i64..=4_102_444_800_i64).prop_map(|secs| Utc.timestamp_opt(secs, 0).unwrap())
}

/// Hard-coded UTC instants of the 2026 DST transitions (EU, US, AU) plus
/// their ±1 s neighbors. The crate is UTC-only by design and no timezone
/// dependency is added: the battery's meaning is that these instants are
/// *ordinary instants* to the timestamp arithmetic — the zone-freeness is
/// the pinned contract (issue #303).
///
/// # Panics
///
/// Panics only if chrono cannot represent the hard-coded 2026 instants —
/// a programming error, not an input condition.
#[must_use]
pub fn dst_boundary_instants() -> Vec<DateTime<Utc>> {
    let bases = [
        (2026, 3, 29, 1, 0, 0),  // EU spring forward
        (2026, 10, 25, 1, 0, 0), // EU fall back
        (2026, 3, 8, 7, 0, 0),   // US spring forward
        (2026, 11, 1, 6, 0, 0),  // US fall back
        (2026, 4, 5, 16, 0, 0),  // AU exit
        (2026, 10, 4, 16, 0, 0), // AU entry
    ];
    let one_second = chrono::TimeDelta::try_seconds(1).unwrap();
    let mut instants = Vec::with_capacity(bases.len() * 3);
    for &(year, month, day, hour, minute, second) in &bases {
        let base = Utc
            .with_ymd_and_hms(year, month, day, hour, minute, second)
            .unwrap();
        instants.push(base - one_second);
        instants.push(base);
        instants.push(base + one_second);
    }
    instants
}

/// Epoch seconds across adversarial ranges (#303): the ordinary bounded
/// band, the seconds adjacent to chrono's own representable bounds
/// (derived from the constants, so a chrono bump shifts the strategy
/// with the types instead of panicking the strategy), and the
/// DST-boundary battery.
pub fn adversarial_timestamp_seconds() -> impl Strategy<Value = i64> {
    let min_ts = DateTime::<Utc>::MIN_UTC.timestamp();
    let max_ts = DateTime::<Utc>::MAX_UTC.timestamp();
    let day = 86_400_i64;
    prop_oneof![
        4 => 31_536_000_i64..=4_102_444_800_i64,
        2 => min_ts..=min_ts + day,
        2 => max_ts - day..=max_ts,
        2 => Just(
            dst_boundary_instants()
                .iter()
                .map(DateTime::timestamp)
                .collect::<Vec<_>>()
        )
        .prop_flat_map(proptest::sample::select),
    ]
}

/// [`adversarial_timestamp_seconds`] as `DateTime<Utc>`.
pub fn adversarial_timestamp_strategy() -> impl Strategy<Value = DateTime<Utc>> {
    adversarial_timestamp_seconds().prop_map(|secs| Utc.timestamp_opt(secs, 0).unwrap())
}

macro_rules! id_strategy {
    ($fn_name:ident, $id_type:ty) => {
        /// A random local id (from a random 128-bit UUID).
        pub fn $fn_name() -> impl Strategy<Value = $id_type> {
            any::<u128>().prop_map(|raw| <$id_type>::from(Uuid::from_u128(raw)))
        }
    };
}

id_strategy!(board_id_strategy, BoardId);
id_strategy!(stack_id_strategy, StackId);
id_strategy!(task_id_strategy, TaskId);
id_strategy!(label_id_strategy, LabelId);
id_strategy!(time_entry_id_strategy, TimeEntryId);
id_strategy!(habit_id_strategy, crate::ids::HabitId);
id_strategy!(series_id_strategy, crate::ids::SeriesId);

/// A positive remote board id (0 is the epoch-0 sentinel of old servers).
pub fn remote_board_id_strategy() -> impl Strategy<Value = crate::ids::RemoteBoardId> {
    remote_number().prop_map(crate::ids::RemoteBoardId)
}

/// The connector tag the test corpus's engine default carries (the legacy
/// scalar world's one synthesized name). Purely a fixture constant.
#[must_use]
pub fn test_connector() -> crate::ids::ConnectorId {
    crate::ids::ConnectorId::from("nextcloud")
}

/// A connector-tagged board binding for fixtures: `test_connector`'s tag
/// on the given remote board. Fixtures that prove tag *preservation*
/// build their own non-default tag instead.
#[must_use]
pub fn test_binding(board: crate::ids::RemoteBoardId) -> Option<crate::ids::RemoteBoardBinding> {
    Some(crate::ids::RemoteBoardBinding {
        connector: test_connector(),
        board,
    })
}

/// A random op id.
pub fn op_id_strategy() -> impl Strategy<Value = OpId> {
    any::<u128>().prop_map(|raw| OpId(Uuid::from_u128(raw)))
}

/// A positive remote deck number (0 is never generated: it is the
/// epoch-0 sentinel of old servers).
pub(crate) fn remote_number() -> impl Strategy<Value = u64> {
    1_u64..=1_000_000
}

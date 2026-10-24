// SPDX-License-Identifier: MIT OR Apache-2.0

//! Day boundaries: the user-configurable "today" window.

use std::collections::BTreeMap;

use chrono::{DateTime, Days, FixedOffset, NaiveDateTime, Offset, Utc};

use crate::entities::Task;
use crate::ids::TaskId;

/// The 24h window `[start, start + 24h)` containing `now`, where
/// `start` is the most recent instant whose wall-clock time-of-day in
/// the fixed UTC offset `offset` equals `day_start_time` (#345's
/// contract; D-F6).
///
/// Pure, total, deterministic: time *and* offset are data — the
/// crate's clock rule ([`crate::clock`]: nothing reads the
/// environment) applies to the offset too, so local-timezone and
/// tz-database lookups never appear here. The consumer passes its
/// *current* fixed offset; on a DST transition day the boundary's
/// wall-clock label may wobble ≤1h from the tz-database answer —
/// accepted by ruling (the instants stay unambiguous; a fixed offset
/// has no transitions, so the instant math is gap-free by
/// construction).
///
/// # Consumers contract (D-F6)
///
/// App bootstrap parses `[preferences] day_start_time` once (typed
/// config validation) and resolves the device's fixed offset from
/// the OS timezone at derivation time; consumers (UI today view, CLI
/// markers, later engine features via bootstrap injection) receive
/// `(day_start_time, offset)` explicitly and call this function.
/// Nothing reads figment mid-flight; nothing mutates
/// [`crate::AppState`] for it.
///
/// Total for every representable `now` at least one day inside
/// chrono's date range (the `Days` arithmetic saturates only at
/// [`chrono::NaiveDate`]'s extremes).
#[must_use]
pub fn day_window(
    now: DateTime<Utc>,
    day_start_time: chrono::NaiveTime,
    offset: FixedOffset,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let local_date = now.with_timezone(&offset).date_naive();
    let candidate = offset_local_to_utc(local_date.and_time(day_start_time), offset);
    let start_date = if candidate > now {
        local_date - Days::new(1)
    } else {
        local_date
    };
    let start = offset_local_to_utc(start_date.and_time(day_start_time), offset);
    let end = offset_local_to_utc((start_date + Days::new(1)).and_time(day_start_time), offset);
    (start, end)
}

/// Back-converts an offset-local wall-clock reading to its instant:
/// `utc = local − offset`, exact for every representable reading
/// because a fixed offset has no transitions, so no reading is
/// ambiguous or missing.
fn offset_local_to_utc(local: NaiveDateTime, offset: FixedOffset) -> DateTime<Utc> {
    DateTime::from_naive_utc_and_offset(local - offset.fix(), Utc)
}

/// The four "today" sections of one day window, disjoint by
/// construction (#346, D-V1): each live task lands in at most one.
///
/// Ids only — the caller reads entities from its own map; no derived
/// "today" bit is ever stored anywhere (every "today" question routes
/// through [`day_window`] + [`today_sections`] at read time).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TodaySections {
    /// Live ∧ open ∧ `duedate < now` — [`crate::reminders::overdue_tasks`]
    /// reused verbatim, canonical `(order, id)` order.
    pub overdue: Vec<TaskId>,
    /// Live ∧ open ∧ `scheduled ∈ [start, end)`, ordered
    /// `(scheduled, order, id)`.
    pub scheduled: Vec<TaskId>,
    /// Live ∧ open ∧ `duedate ∈ [now, end)`, ordered `(order, id)`.
    /// Lower bound `now`: a duedate in `[start, now)` is already
    /// Overdue (first-match precedence).
    pub due_today: Vec<TaskId>,
    /// Live ∧ `done ∈ [start, end)`, ordered `(done, id)` ascending.
    pub done_today: Vec<TaskId>,
}

/// Classifies every live task of `tasks` into the four sections of the
/// day `window` (the `(start, end)` pair [`day_window`] returns), as of
/// `now`. Pure, total, deterministic; board-agnostic by construction
/// (the cross-board desktop is a caller property).
///
/// First-match classification, in display order — the predicates make
/// the sections disjoint:
///
/// 1. **Overdue** — live ∧ open ∧ `duedate < now`.
/// 2. **Scheduled** — live ∧ open ∧ `scheduled ∈ [start, end)`.
/// 3. **Due-today** — live ∧ open ∧ `duedate ∈ [now, end)`.
/// 4. **Done** — live ∧ `done ∈ [start, end)`.
///
/// A task placed in an earlier section is never re-tested for a later
/// one (a task both scheduled and due today lands in Scheduled only; a
/// completed scheduled task moves to Done — completing it closes the
/// open-gate of the earlier sections). Time-passage does not unplace an
/// open task: an open 09:00-scheduled task stays in Scheduled at 15:00.
#[must_use]
pub fn today_sections(
    tasks: &BTreeMap<TaskId, Task>,
    now: DateTime<Utc>,
    window: (DateTime<Utc>, DateTime<Utc>),
) -> TodaySections {
    let (start, end) = window;
    let mut sections = TodaySections {
        // The one section the reminders crate already owns: same live ∧
        // open ∧ `duedate < now` predicate, same `(order, id)` order.
        overdue: crate::reminders::overdue_tasks(tasks, now)
            .into_iter()
            .map(|t| t.id)
            .collect(),
        ..TodaySections::default()
    };
    let mut scheduled: Vec<(DateTime<Utc>, i64, TaskId)> = Vec::new();
    let mut due_today: Vec<(i64, TaskId)> = Vec::new();
    let mut done_today: Vec<(DateTime<Utc>, TaskId)> = Vec::new();
    for task in tasks.values() {
        if !task.is_live() || sections.overdue.contains(&task.id) {
            continue;
        }
        if task.is_open() {
            if task.scheduled.is_some_and(|s| s >= start && s < end) {
                scheduled.push((task.scheduled.unwrap_or(now), task.order, task.id));
                continue;
            }
            if task.duedate.is_some_and(|d| d >= now && d < end) {
                due_today.push((task.order, task.id));
                continue;
            }
        }
        if task.done.is_some_and(|d| d >= start && d < end) {
            done_today.push((task.done.unwrap_or(now), task.id));
        }
    }
    scheduled.sort();
    sections.scheduled = scheduled.into_iter().map(|(_, _, id)| id).collect();
    due_today.sort();
    sections.due_today = due_today.into_iter().map(|(_, id)| id).collect();
    done_today.sort();
    sections.done_today = done_today.into_iter().map(|(_, id)| id).collect();
    sections
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{proptest_config, timestamp_strategy};
    use chrono::{NaiveTime, TimeZone};
    use proptest::prelude::*;

    fn time(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn offset_hours(h: i32) -> FixedOffset {
        FixedOffset::east_opt(h * 3600).unwrap()
    }

    fn utc(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    /// D7a: a midnight start at `+02:00` — the window opens at that
    /// day's local midnight, an earlier UTC instant than a naive UTC
    /// reading of `now`'s date suggests. Hand-computed: local
    /// 2026-06-15 14:00 +02:00 = 12:00 UTC; the day's local midnight
    /// is 2026-06-14 22:00 UTC.
    #[test]
    fn midnight_start_opens_at_the_local_midnight() {
        let now = utc(1_781_524_800); // 2026-06-15 12:00:00 UTC
        let (start, end) = day_window(now, time(0, 0), offset_hours(2));
        assert_eq!(start, utc(1_781_474_400)); // 2026-06-14 22:00:00 UTC
        assert_eq!(end, utc(1_781_560_800)); // 2026-06-15 22:00:00 UTC
    }

    /// D7b: night-owl `"05:00"`, local 03:00 — the window opened
    /// *yesterday* at 05:00 local (the step-back rule).
    #[test]
    fn night_owl_start_rolls_back_to_yesterday() {
        // Local 2026-06-15 03:00 +02:00 = 01:00 UTC. Yesterday 05:00
        // local = 2026-06-14 03:00 UTC; end = 2026-06-15 03:00 UTC.
        let now = utc(1_781_485_200);
        let (start, end) = day_window(now, time(5, 0), offset_hours(2));
        assert_eq!(start, utc(1_781_406_000));
        assert_eq!(end, utc(1_781_492_400));
    }

    /// D7c: `now` exactly at the boundary — the new window opens at
    /// `now` (the equality branch takes the candidate).
    #[test]
    fn now_exactly_at_the_boundary_opens_the_new_window() {
        // Local 2026-06-15 05:00 +02:00 = 03:00 UTC = the boundary.
        let now = utc(1_781_492_400);
        let (start, end) = day_window(now, time(5, 0), offset_hours(2));
        assert_eq!(start, now);
        assert_eq!(end, utc(1_781_578_800)); // +24h
    }

    /// D7d: `"00:00"` at `+00:00` is `[local midnight, +24h)` in UTC.
    #[test]
    fn utc_midnight_start_is_the_plain_day() {
        let now = utc(1_781_524_800);
        let (start, end) = day_window(now, time(0, 0), offset_hours(0));
        assert_eq!(start, utc(1_781_481_600)); // 2026-06-15 00:00 UTC
        assert_eq!(end, utc(1_781_568_000)); // 2026-06-16 00:00 UTC
    }

    fn day_start_time_strategy() -> impl Strategy<Value = NaiveTime> {
        (0_u32..=86_399).prop_map(|s| NaiveTime::from_num_seconds_from_midnight_opt(s, 0).unwrap())
    }

    fn offset_strategy() -> impl Strategy<Value = FixedOffset> {
        (-86_399_i32..=86_399).prop_map(|s| FixedOffset::east_opt(s).unwrap())
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        #[test]
        fn window_contains_now(
            now in timestamp_strategy(),
            day_start_time in day_start_time_strategy(),
            offset in offset_strategy(),
        ) {
            let (start, end) = day_window(now, day_start_time, offset);
            prop_assert!(start <= now && now < end);
        }

        #[test]
        fn window_is_exactly_24_hours(
            now in timestamp_strategy(),
            day_start_time in day_start_time_strategy(),
            offset in offset_strategy(),
        ) {
            let (start, end) = day_window(now, day_start_time, offset);
            prop_assert_eq!(end - start, chrono::TimeDelta::try_hours(24).unwrap());
        }

        #[test]
        fn stepping_now_across_the_boundary_flips_the_window(
            now in timestamp_strategy(),
            day_start_time in day_start_time_strategy(),
            offset in offset_strategy(),
        ) {
            // Independently construct today's offset-local boundary
            // instant `B` and sample both flanks of the flip.
            let local_date = now.with_timezone(&offset).date_naive();
            let boundary = offset
                .from_local_datetime(&local_date.and_time(day_start_time))
                .single()
                .unwrap();
            let boundary_utc: DateTime<Utc> = boundary.into();
            let (start_at, _) = day_window(boundary_utc, day_start_time, offset);
            prop_assert_eq!(start_at, boundary_utc, "at B the new window opens");

            let one_sec = chrono::TimeDelta::try_seconds(1).unwrap();
            let day = chrono::TimeDelta::try_hours(24).unwrap();
            let (start_before, _) =
                day_window((boundary - one_sec).into(), day_start_time, offset);
            prop_assert_eq!(start_before, boundary_utc - day, "before B yesterday's window");
            let (start_after, _) =
                day_window((boundary + one_sec).into(), day_start_time, offset);
            prop_assert_eq!(start_after, boundary_utc, "after B the new window");
        }

        #[test]
        fn one_day_later_the_window_advances_one_day(
            now in timestamp_strategy(),
            day_start_time in day_start_time_strategy(),
            offset in offset_strategy(),
        ) {
            let day = chrono::TimeDelta::try_hours(24).unwrap();
            let (start, end) = day_window(now, day_start_time, offset);
            let (start_later, end_later) =
                day_window(now + day, day_start_time, offset);
            prop_assert_eq!(start_later, start + day);
            prop_assert_eq!(end_later, end + day);
        }
    }
}

#[cfg(test)]
mod today_tests {
    use super::*;
    use crate::entities::TaskClocks;
    use crate::test_support::{self, proptest_config};
    use chrono::{TimeDelta, TimeZone};
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    fn day() -> TimeDelta {
        TimeDelta::try_hours(24).unwrap()
    }

    fn stamp(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn task(
        raw: u128,
        order: i64,
        duedate: Option<DateTime<Utc>>,
        scheduled: Option<DateTime<Utc>>,
        done: Option<DateTime<Utc>>,
    ) -> Task {
        Task {
            id: TaskId::from(uuid::Uuid::from_u128(raw)),
            remote: None,
            title: format!("task {raw}"),
            description: String::new(),
            duedate,
            scheduled,
            done,
            stack: crate::ids::StackId::from(uuid::Uuid::from_u128(1)),
            order,
            labels: BTreeSet::default(),
            effort: None,
            importance: None,
            urgency: None,
            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: stamp(100),
                description: stamp(100),
                duedate: stamp(100),
                scheduled: stamp(100),
                done: stamp(100),
                position: stamp(100),
                labels: stamp(100),
                effort: stamp(100),
                importance: stamp(100),
                urgency: stamp(100),
                archived: stamp(100),
                deleted: stamp(100),
            },
            remote_seen: None,
        }
    }

    fn map(tasks: Vec<Task>) -> BTreeMap<TaskId, Task> {
        tasks.into_iter().map(|t| (t.id, t)).collect()
    }

    /// `(start, now, end)`: a 24h window with `now` inside it.
    fn span_strategy() -> impl Strategy<Value = (DateTime<Utc>, DateTime<Utc>, DateTime<Utc>)> {
        (31_536_000_i64..=4_096_000_000_i64, 0_i64..=86_399).prop_map(|(start, now_off)| {
            let start = stamp(start);
            let now = start + TimeDelta::try_seconds(now_off).unwrap();
            (start, now, start + day())
        })
    }

    /// A live task whose three time stamps wander ±1 day around the
    /// window (the partition property needs timestamps on both flanks).
    fn stamped_task_strategy(
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> impl Strategy<Value = Task> {
        let lo = (start - day()).timestamp();
        let hi = (end + day()).timestamp();
        (
            test_support::task_strategy(),
            prop::option::of(lo..=hi),
            prop::option::of(lo..=hi),
            prop::option::of(lo..=hi),
        )
            .prop_map(|(mut t, duedate, scheduled, done)| {
                t.deleted = false;
                t.duedate = duedate.map(stamp);
                t.scheduled = scheduled.map(stamp);
                t.done = done.map(stamp);
                t
            })
    }

    /// `(start, now, end)` plus live tasks whose stamps wander ±1 day
    /// around that same window (ids shaped by a raw index, so the map
    /// has no collisions).
    /// The span-with-tuples tuple the generated fixtures consume.
    type SpanWithTasks = (
        DateTime<Utc>,
        DateTime<Utc>,
        DateTime<Utc>,
        Vec<(u128, Task)>,
    );

    fn span_with_tasks_strategy() -> impl Strategy<Value = SpanWithTasks> {
        span_strategy().prop_flat_map(|(start, now, end)| {
            (
                Just(start),
                Just(now),
                Just(end),
                prop::collection::vec((0_u128..16, stamped_task_strategy(start, end)), 0..12),
            )
        })
    }

    fn sections_of(
        tasks: &BTreeMap<TaskId, Task>,
        now: DateTime<Utc>,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> TodaySections {
        today_sections(tasks, now, (start, end))
    }

    // P1 — partition: every live task appears in at most one section;
    // tombstoned tasks appear in none.
    proptest! {
        #![proptest_config(proptest_config(256))]

        #[test]
        fn every_live_task_lands_in_at_most_one_section(
            (start, now, end, tasks) in span_with_tasks_strategy(),
        ) {
            // Distinct ids: raw index shapes the id, the strategy shapes
            // the stamps.
            let tasks: Vec<Task> = tasks
                .into_iter()
                .map(|(raw, mut t)| {
                    t.id = TaskId::from(uuid::Uuid::from_u128(raw));
                    t
                })
                .collect();
            let tasks = map(tasks);
            let s = sections_of(&tasks, now, start, end);
            let mut all: Vec<TaskId> = s.overdue.iter().chain(&s.scheduled).chain(&s.due_today).chain(&s.done_today).copied().collect();
            all.sort();
            let distinct = all.len();
            all.dedup();
            prop_assert_eq!(all.len(), distinct, "sections are pairwise disjoint");
            for id in &all {
                prop_assert!(tasks[id].is_live(), "only live tasks are classified");
            }
            for t in tasks.values().filter(|t| !t.is_live()) {
                prop_assert!(!all.contains(&t.id), "tombstones appear nowhere");
            }
        }

        // P2 — first-match: scheduled ∧ due-today lands in Scheduled only.
        #[test]
        fn scheduled_and_due_lands_in_scheduled_only((start, now, end) in span_strategy()) {
            let tasks = map(vec![task(
                1,
                0,
                Some(end - TimeDelta::try_seconds(1).unwrap()),
                Some(start),
                None,
            )]);
            let s = sections_of(&tasks, now, start, end);
            prop_assert_eq!(s.scheduled, vec![tasks[&TaskId::from(uuid::Uuid::from_u128(1))].id]);
            prop_assert_eq!(s.overdue, [] as [TaskId; 0]);
            prop_assert_eq!(s.due_today, [] as [TaskId; 0]);
            prop_assert!(s.done_today.is_empty());
        }

        // P3 — done-move: completing a scheduled task moves it from
        // Scheduled to Done.
        #[test]
        fn completing_a_scheduled_task_moves_it_to_done((start, now, end) in span_strategy()) {
            let id = TaskId::from(uuid::Uuid::from_u128(7));
            let open = map(vec![task(7, 0, None, Some(start), None)]);
            let s = sections_of(&open, now, start, end);
            prop_assert_eq!(s.scheduled, vec![id]);
            let done_at = start + TimeDelta::try_hours(2).unwrap();
            let closed = map(vec![task(7, 0, None, Some(start), Some(done_at))]);
            let s = sections_of(&closed, now, start, end);
            prop_assert_eq!(s.done_today, vec![id]);
            prop_assert!(s.scheduled.is_empty(), "the open gate closed");
        }

        // P5 — the four orderings.
        #[test]
        fn sections_are_canonically_sorted(
            (start, now, end, tasks) in span_with_tasks_strategy(),
        ) {
            let tasks: Vec<Task> = tasks
                .into_iter()
                .map(|(raw, mut t)| {
                    t.id = TaskId::from(uuid::Uuid::from_u128(raw));
                    t
                })
                .collect();
            let tasks = map(tasks);
            let s = sections_of(&tasks, now, start, end);
            let key = |id: &TaskId| {
                let t = &tasks[id];
                (t.order, *id)
            };
            for pair in s.overdue.windows(2) {
                prop_assert!(key(&pair[0]) < key(&pair[1]), "overdue: (order, id)");
            }
            for pair in s.due_today.windows(2) {
                prop_assert!(key(&pair[0]) < key(&pair[1]), "due-today: (order, id)");
            }
            for pair in s.scheduled.windows(2) {
                let a = &tasks[&pair[0]];
                let b = &tasks[&pair[1]];
                prop_assert!(
                    (a.scheduled.unwrap(), a.order, a.id) < (b.scheduled.unwrap(), b.order, b.id),
                    "scheduled: (scheduled, order, id)"
                );
            }
            for pair in s.done_today.windows(2) {
                let a = &tasks[&pair[0]];
                let b = &tasks[&pair[1]];
                prop_assert!(
                    (a.done.unwrap(), a.id) < (b.done.unwrap(), b.id),
                    "done: (done, id)"
                );
            }
        }

        // P6 — totality/determinism: equal inputs, equal outputs.
        #[test]
        fn equal_inputs_give_equal_outputs(
            (start, now, end, tasks) in span_with_tasks_strategy(),
        ) {
            let tasks: Vec<Task> = tasks
                .into_iter()
                .map(|(raw, mut t)| {
                    t.id = TaskId::from(uuid::Uuid::from_u128(raw));
                    t
                })
                .collect();
            let tasks = map(tasks);
            prop_assert_eq!(
                sections_of(&tasks, now, start, end),
                sections_of(&tasks, now, start, end)
            );
        }
    }

    // P4 — the window-boundary edges, pinned as units (seconds
    // resolution makes ±1 s exact).
    #[test]
    fn due_one_second_before_now_is_overdue() {
        let start = stamp(1_781_481_600);
        let now = start + TimeDelta::try_hours(12).unwrap();
        let end = start + day();
        let tasks = map(vec![task(
            1,
            0,
            Some(now - TimeDelta::try_seconds(1).unwrap()),
            None,
            None,
        )]);
        let s = today_sections(&tasks, now, (start, end));
        assert_eq!(s.overdue.len(), 1);
        assert_eq!(s.due_today, [] as [TaskId; 0]);
    }

    #[test]
    fn due_exactly_at_now_is_due_today_not_overdue() {
        let start = stamp(1_781_481_600);
        let now = start + TimeDelta::try_hours(12).unwrap();
        let end = start + day();
        let tasks = map(vec![task(1, 0, Some(now), None, None)]);
        let s = today_sections(&tasks, now, (start, end));
        assert!(s.overdue.is_empty(), "`d < now` is the overdue predicate");
        assert_eq!(s.due_today.len(), 1);
    }

    #[test]
    fn due_exactly_at_end_is_in_neither_section() {
        let start = stamp(1_781_481_600);
        let now = start + TimeDelta::try_hours(12).unwrap();
        let end = start + day();
        let tasks = map(vec![task(1, 0, Some(end), None, None)]);
        let s = today_sections(&tasks, now, (start, end));
        assert_eq!(s.overdue, [] as [TaskId; 0]);
        assert!(s.due_today.is_empty(), "the window is half-open");
    }

    #[test]
    fn done_before_the_window_appears_nowhere() {
        let start = stamp(1_781_481_600);
        let now = start + TimeDelta::try_hours(12).unwrap();
        let end = start + day();
        let tasks = map(vec![task(
            1,
            0,
            None,
            None,
            Some(start - TimeDelta::try_seconds(1).unwrap()),
        )]);
        let s = today_sections(&tasks, now, (start, end));
        assert_eq!(s, TodaySections::default());
    }

    #[test]
    fn a_tombstoned_task_appears_nowhere_even_when_due() {
        let start = stamp(1_781_481_600);
        let now = start + TimeDelta::try_hours(12).unwrap();
        let end = start + day();
        let mut t = task(1, 0, Some(now), None, None);
        t.deleted = true;
        let tasks = map(vec![t]);
        assert_eq!(
            today_sections(&tasks, now, (start, end)),
            TodaySections::default()
        );
    }

    #[test]
    fn time_passage_does_not_unplace_an_open_scheduled_task() {
        // The "done or not" reading: an open 09:00-scheduled task is
        // still in Scheduled at 15:00 — its slot having passed is not
        // a section change.
        let start = stamp(1_781_481_600);
        let end = start + day();
        let scheduled_at = start + TimeDelta::try_hours(9).unwrap();
        let now = start + TimeDelta::try_hours(15).unwrap();
        let tasks = map(vec![task(1, 0, None, Some(scheduled_at), None)]);
        let s = today_sections(&tasks, now, (start, end));
        assert_eq!(s.scheduled.len(), 1);
        assert_eq!(s.due_today, [] as [TaskId; 0]);
    }

    // The overdue section is `overdue_tasks` reused verbatim: an
    // equality pin over a generated state (same predicate, same order).
    proptest! {
        #![proptest_config(proptest_config(256))]

        #[test]
        fn the_overdue_section_is_overdue_tasks_verbatim(
            (start, now, end, tasks) in span_with_tasks_strategy(),
        ) {
            let tasks: Vec<Task> = tasks
                .into_iter()
                .map(|(raw, mut t)| {
                    t.id = TaskId::from(uuid::Uuid::from_u128(raw));
                    t
                })
                .collect();
            let tasks = map(tasks);
            let s = sections_of(&tasks, now, start, end);
            let expected: Vec<TaskId> = crate::reminders::overdue_tasks(&tasks, now)
                .into_iter()
                .map(|t| t.id)
                .collect();
            prop_assert_eq!(s.overdue, expected);
        }
    }
}

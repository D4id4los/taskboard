// SPDX-License-Identifier: AGPL-3.0-only

//! The presentation helpers (design §4.3): pure display-domain
//! mapping over domain types — no slint types, nothing generated, no
//! IO, no mutation (the extraction seam: a second Rust consumer lifts
//! this module into its own crate unchanged; a non-Rust UI reuses the
//! *domain* over IPC and re-implements the mapping natively — the
//! concepts port, the module does not).
//!
//! Domain truth is never reinvented: due classification is
//! set-membership over the domain's own `overdue_tasks`/`due_soon`
//! outputs, and stack display order reads the domain's persisted
//! `Stack.order` field (the domain defines the concept and the write
//! commands; only the reading helper lives here — D10).

use std::collections::BTreeSet;

use chrono::{DateTime, TimeDelta, Utc};
use taskboard_domain::{
    AppState, Board, BoardId, Effort, ReminderRef, SidecarHealth, Stack, SyncPhase, Task, TaskId,
};

/// The due-soon window (design §4): a presentation constant, not
/// config — Q22 fixes the v0.1.0 `[ui]` surface at scale + keymap
/// (the tunable horizon is recorded backlog debt).
pub const DUE_SOON_HORIZON: TimeDelta = TimeDelta::hours(24);

/// The due classification vocabulary (slint-free; `models` maps it
/// 1:1 onto the generated enum for markup conditionals).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueKind {
    None,
    Done,
    Later,
    Soon,
    Overdue,
}

/// The domain classification outputs, computed once per refresh and
/// keyed for membership asks (no predicate duplication anywhere).
#[derive(Debug, Clone, Default)]
pub struct DueSets {
    pub overdue: BTreeSet<TaskId>,
    pub soon: BTreeSet<TaskId>,
}

/// Computes [`DueSets`] for `now` by calling the domain functions
/// collection-side — the overdue/due-soon semantics exist only there.
#[must_use]
pub fn due_sets(state: &AppState, now: DateTime<Utc>) -> DueSets {
    DueSets {
        overdue: task_id_keys(taskboard_domain::overdue_tasks(&state.tasks, now)),
        soon: task_id_keys(taskboard_domain::due_soon(
            &state.tasks,
            now,
            DUE_SOON_HORIZON,
        )),
    }
}

fn task_id_keys<'a>(tasks: impl IntoIterator<Item = &'a Task>) -> BTreeSet<TaskId> {
    tasks.into_iter().map(|t| t.id).collect()
}

/// Classifies one task against the sets: done → `Done`; no duedate →
/// `None`; then set membership — no reimplementation of the domain
/// predicates.
#[must_use]
pub fn due_kind(task: &Task, sets: &DueSets) -> DueKind {
    if task.done.is_some() {
        DueKind::Done
    } else if task.duedate.is_none() {
        DueKind::None
    } else if sets.overdue.contains(&task.id) {
        DueKind::Overdue
    } else if sets.soon.contains(&task.id) {
        DueKind::Soon
    } else {
        DueKind::Later
    }
}

/// The typed numeric bucketing behind due phrases (string assembly is
/// trivial glue over the bucket — tests assert buckets, not text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationBucket {
    Now,
    Minutes(u32),
    Hours(u32),
    Days(u32),
}

/// Buckets a (non-negative) duration. Negative inputs clamp to
/// [`DurationBucket::Now`]; extreme magnitudes saturate their unit's
/// `u32` — total on arbitrary input.
#[must_use]
pub fn duration_bucket(delta: TimeDelta) -> DurationBucket {
    let seconds = delta.num_seconds().max(0);
    if seconds < 60 {
        return DurationBucket::Now;
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return DurationBucket::Minutes(u32::try_from(minutes).unwrap_or(u32::MAX));
    }
    let hours = minutes / 60;
    if hours < 24 {
        return DurationBucket::Hours(u32::try_from(hours).unwrap_or(u32::MAX));
    }
    DurationBucket::Days(u32::try_from(hours / 24).unwrap_or(u32::MAX))
}

/// The display phrase for a due classification (mono role), assembled
/// from the bucket with the direction taken from the sign of
/// `duedate - now`.
#[must_use]
pub fn due_phrase(kind: DueKind, duedate: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    let Some(duedate) = duedate else {
        return String::new();
    };
    let delta = duedate - now;
    let bucket = duration_bucket(delta.abs());
    let quantity = match bucket {
        DurationBucket::Now => return "now".to_owned(),
        DurationBucket::Minutes(n) => format!("{n}m"),
        DurationBucket::Hours(n) => format!("{n}h"),
        DurationBucket::Days(n) => format!("{n}d"),
    };
    let word = match kind {
        DueKind::Overdue | DueKind::Done => "overdue",
        _ => "due",
    };
    if delta < TimeDelta::zero() {
        format!("{quantity} {word}")
    } else {
        format!("in {quantity}")
    }
}

/// The display phrase for a scheduled start (mono role): empty when the
/// field is absent (the `due_phrase` convention); otherwise
/// self-describing — unlike due, no tone carries the direction, so the
/// words do (`starts now` / `starts in {q}` / `started {q} ago`). The
/// bucketing edges are `duration_bucket`'s tested contract.
#[must_use]
pub fn scheduled_phrase(scheduled: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    let Some(scheduled) = scheduled else {
        return String::new();
    };
    let delta = scheduled - now;
    let bucket = duration_bucket(delta.abs());
    match (delta < TimeDelta::zero(), bucket) {
        (_, DurationBucket::Now) => "starts now".to_owned(),
        (false, DurationBucket::Minutes(n)) => format!("starts in {n}m"),
        (false, DurationBucket::Hours(n)) => format!("starts in {n}h"),
        (false, DurationBucket::Days(n)) => format!("starts in {n}d"),
        (true, DurationBucket::Minutes(n)) => format!("started {n}m ago"),
        (true, DurationBucket::Hours(n)) => format!("started {n}h ago"),
        (true, DurationBucket::Days(n)) => format!("started {n}d ago"),
    }
}

/// The display phrase for an estimated effort: empty when absent, else
/// the domain's canonical text form — the UI wraps the domain formatter,
/// never re-implements the grammar.
#[must_use]
pub fn effort_phrase(effort: Option<Effort>) -> String {
    effort.map_or(String::new(), Effort::text_form)
}

/// The anchor-card test (DDF-5, the presentation-side half): an exact
/// byte-equal title match — no trim, no case-fold, no normalization (a
/// trailing space is a visible near-miss, and must stay visible).
/// Never enters the domain crate; resolution keeps resolving the anchor.
#[must_use]
pub fn is_anchor_task(task: &Task) -> bool {
    task.title == taskboard_domain::ANCHOR_CARD_TITLE
}

/// The sync phase as the UI badge vocabulary (slint-free).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPhaseUi {
    Idle,
    Syncing,
    Offline,
    Failed,
}

/// The sync badge model (D11): phase + pending depth; recency text is
/// markup-side trivia over `last_success`, not part of the badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncBadge {
    pub phase: SyncPhaseUi,
    pub pending: u32,
    /// The selected board's derived sidecar health (the loud fence's
    /// subject); `Healthy` (the default) renders nothing.
    pub sidecar: SidecarHealth,
}

/// Maps the *selected* board's sync row onto the badge (design §3.2,
/// option (a)): `Some(board)` reads that board's row — the derivation
/// materializes a default row per known board, so a bound-but-never-
/// synced board reads `Idle`/0; `None` (no boards at all) reads the
/// default. The aggregate fold is not a UI display read (the app-side
/// daemon `status` keeps it).
#[must_use]
pub fn sync_badge_for_board(
    sync: &taskboard_domain::SyncStates,
    board: Option<BoardId>,
) -> SyncBadge {
    let status = board.and_then(|b| sync.get(b)).cloned().unwrap_or_default();
    let phase = match status.phase {
        SyncPhase::Idle => SyncPhaseUi::Idle,
        SyncPhase::Syncing => SyncPhaseUi::Syncing,
        SyncPhase::Offline => SyncPhaseUi::Offline,
        SyncPhase::Failed { .. } => SyncPhaseUi::Failed,
    };
    SyncBadge {
        phase,
        pending: status.pending_ops,
        sidecar: status.sidecar,
    }
}

/// The sidecar warning caption (D-S2's loud fence): empty for healthy
/// (absent evidence is the ruled silence); an unreadable reason renders
/// the "update this app" warning class with the typed reason tag. The
/// wording is presentation-edge text — the `status` CLI's precedent.
#[must_use]
pub fn sidecar_caption(health: SidecarHealth) -> String {
    match health {
        SidecarHealth::Healthy => String::new(),
        SidecarHealth::Unreadable(taskboard_domain::AppDataUnreadable::FormatTooNew { format }) => {
            format!("board data unreadable (format {format} is too new) — update this app")
        }
        SidecarHealth::Unreadable(reason) => {
            let tag = match reason {
                taskboard_domain::AppDataUnreadable::Transport => "transport",
                taskboard_domain::AppDataUnreadable::Decode => "decode",
                taskboard_domain::AppDataUnreadable::ForeignBoard => "foreign-board",
                taskboard_domain::AppDataUnreadable::FormatTooNew { .. } => unreachable!(),
            };
            format!("board data unreadable ({tag}) — update this app")
        }
    }
}

/// A banner snooze/mute target (D4/D12): the task plus the concrete
/// `ReminderRef`/occurrence pair the commands carry. For a task with
/// explicit rules this is the first rule's `occurrence_of` output;
/// rule-less tasks use the implicit default with the duedate anchor
/// (occurrence is opaque link data by engine contract — the *nag*
/// effect of these links lands with the reminders toast plan).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReminderTarget {
    pub task: TaskId,
    pub reminder: ReminderRef,
    pub occurrence: DateTime<Utc>,
}

/// One overdue-banner row (D12): the task truth plus its snooze
/// target. Visibility never consults mute/snooze/deliveries — board
/// truth, not nagging policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BannerRow {
    pub task: TaskId,
    pub kind: DueKind,
    pub target: ReminderTarget,
}

/// The banner rows for `now`: `overdue_tasks` order (canonical), one
/// row per task, each with a well-formed target (D12).
#[must_use]
pub fn banner_rows(state: &AppState, now: DateTime<Utc>) -> Vec<BannerRow> {
    taskboard_domain::overdue_tasks(&state.tasks, now)
        .into_iter()
        // The anchor card is never a banner row (DDF-5: the exact-title
        // filter is presentation-layer; `banner_overflow` follows).
        .filter(|task| !is_anchor_task(task))
        .map(|task| {
            let (reminder, occurrence) = snooze_target(state, task);
            BannerRow {
                task: task.id,
                kind: DueKind::Overdue,
                target: ReminderTarget {
                    task: task.id,
                    reminder,
                    occurrence,
                },
            }
        })
        .collect()
}

/// The snooze target for one overdue task (D12): first explicit rule
/// (rule-id order, the map's canonical order) with `occurrence_of`;
/// rule-less (or a rule whose occurrence arithmetic is degenerate) →
/// `ReminderRef::Default(task)` with the duedate anchor.
fn snooze_target(state: &AppState, task: &Task) -> (ReminderRef, DateTime<Utc>) {
    let rule = state.reminders.rules.values().find(|r| r.task == task.id);
    let occurrence = rule.and_then(|r| taskboard_domain::occurrence_of(r, task.duedate));
    if let (Some(rule), Some(occurrence)) = (rule, occurrence) {
        return (ReminderRef::Rule(rule.id), occurrence);
    }
    let anchor = task
        .duedate
        .unwrap_or_else(taskboard_domain::BoardClocks::epoch);
    (ReminderRef::Default(task.id), anchor)
}

/// The board's live stacks in display order (D10): `(order, id)`
/// ascending — presentation's own reading of the domain's persisted
/// sort position (the domain owns the concept and the write side;
/// this is the display-side reader).
#[must_use]
pub fn stacks_in_board(state: &AppState, board: BoardId) -> Vec<&Stack> {
    let mut stacks: Vec<&Stack> = state
        .stacks
        .values()
        .filter(|s| !s.deleted && s.board == board)
        .collect();
    stacks.sort_by_key(|s| (s.order, s.id));
    stacks
}

/// The live, non-archived boards in canonical order: `state.boards`
/// `BTreeMap` iteration order — board-id ascending, the exact order
/// the CLI's `boards list` renders. The board walk's domain and the
/// seed rule's candidate list.
#[must_use]
pub fn live_boards(state: &AppState) -> Vec<&Board> {
    state
        .boards
        .values()
        .filter(|b| !b.deleted && !b.archived)
        .collect()
}

/// The single live, non-archived board — the view-state seed rule's
/// pick ([`ViewState`](crate::view_state::ViewState)'s `board` when
/// `None`): the first of [`live_boards`]. Its only remaining role;
/// selection lives in the view state.
#[must_use]
pub fn live_board(state: &AppState) -> Option<&Board> {
    live_boards(state).into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use taskboard_domain::{
        BoardClocks, Effort, ReminderOffset, ReminderRule, SidecarHealth, StackId, TaskClocks,
    };

    /// Local fixture clocks (the domain's `test_support` is
    /// feature-gated; these tests need only epoch stamps).
    fn epoch() -> DateTime<Utc> {
        taskboard_domain::BoardClocks::epoch()
    }

    fn task_clocks() -> TaskClocks {
        TaskClocks {
            title: epoch(),
            description: epoch(),
            duedate: epoch(),
            scheduled: epoch(),
            done: epoch(),
            position: epoch(),
            labels: epoch(),
            effort: epoch(),
            importance: epoch(),
            urgency: epoch(),
            archived: epoch(),
            deleted: epoch(),
        }
    }

    fn task(id: TaskId, stack: StackId, order: i64) -> Task {
        Task {
            id,
            remote: None,
            title: format!("task-{order}"),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack,
            order,
            labels: BTreeSet::new(),
            effort: None,
            importance: None,
            urgency: None,
            archived: false,
            deleted: false,
            clocks: task_clocks(),
            remote_seen: None,
        }
    }

    fn stack(id: StackId, board: BoardId, order: i64) -> Stack {
        Stack {
            id,
            remote: None,
            board,
            title: format!("stack-{order}"),
            order,
            archived: false,
            deleted: false,
            clocks: taskboard_domain::StackClocks {
                title: epoch(),
                order: epoch(),
                deleted: epoch(),
            },
            remote_seen: None,
        }
    }

    fn board(id: BoardId) -> Board {
        Board {
            id,
            remote: None,
            title: "board".to_owned(),
            color: taskboard_domain::Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: epoch(),
                color: epoch(),
                archived: epoch(),
                deleted: epoch(),
            },
            remote_seen: None,
        }
    }

    #[test]
    fn due_kind_follows_the_sets() {
        let now = Utc::now();
        let state = AppState::default();
        let sets = due_sets(&state, now);
        let mut overdue = task(
            TaskId::from_uuid(uuid::Uuid::now_v7()),
            StackId::from_uuid(uuid::Uuid::now_v7()),
            0,
        );
        overdue.duedate = Some(now - TimeDelta::hours(2));
        let mut done = task(TaskId::from_uuid(uuid::Uuid::now_v7()), overdue.stack, 1);
        done.done = Some(now);
        done.duedate = Some(now - TimeDelta::hours(5));

        let mut sets = sets;
        sets.overdue.insert(overdue.id);
        assert_eq!(due_kind(&overdue, &sets), DueKind::Overdue);
        assert_eq!(due_kind(&done, &sets), DueKind::Done, "done wins over sets");
        let undated = task(TaskId::from_uuid(uuid::Uuid::now_v7()), overdue.stack, 2);
        assert_eq!(due_kind(&undated, &sets), DueKind::None);
        let mut future = task(TaskId::from_uuid(uuid::Uuid::now_v7()), overdue.stack, 3);
        future.duedate = Some(now + TimeDelta::hours(72));
        assert_eq!(due_kind(&future, &sets), DueKind::Later);
    }

    #[test]
    fn duration_bucket_edges() {
        assert_eq!(duration_bucket(TimeDelta::seconds(59)), DurationBucket::Now);
        assert_eq!(
            duration_bucket(TimeDelta::seconds(60)),
            DurationBucket::Minutes(1)
        );
        assert_eq!(
            duration_bucket(TimeDelta::minutes(59)),
            DurationBucket::Minutes(59)
        );
        assert_eq!(
            duration_bucket(TimeDelta::minutes(60)),
            DurationBucket::Hours(1)
        );
        assert_eq!(
            duration_bucket(TimeDelta::hours(23)),
            DurationBucket::Hours(23)
        );
        assert_eq!(
            duration_bucket(TimeDelta::hours(24)),
            DurationBucket::Days(1)
        );
        assert_eq!(
            duration_bucket(-TimeDelta::hours(24)),
            DurationBucket::Now,
            "negative clamps"
        );
    }

    #[test]
    fn due_phrase_renders_when_dated_and_stays_empty_when_not() {
        // Structural facts only (AGENTS §5): the phrase assembly is
        // glue over `duration_bucket`, whose edges the tests above own
        // — no label-text assertions here.
        let now = Utc::now();
        assert!(due_phrase(DueKind::None, None, now).is_empty(), "undated");
        assert!(
            !due_phrase(DueKind::Later, Some(now + TimeDelta::hours(3)), now).is_empty(),
            "dated future"
        );
        assert!(
            !due_phrase(DueKind::Overdue, Some(now - TimeDelta::hours(3)), now).is_empty(),
            "dated past"
        );
    }

    #[test]
    fn sync_badge_follows_the_named_board() {
        // Two stored rows (Syncing on board 1, Offline on board 2,
        // Failed on board 3) plus derived pending ops attributed to
        // board 1 only: the badge reads the *named* board's row —
        // phase and pending both — not a fold.
        let b1 = BoardId::from_uuid(uuid::Uuid::from_u128(1));
        let b2 = BoardId::from_uuid(uuid::Uuid::from_u128(2));
        let b3 = BoardId::from_uuid(uuid::Uuid::from_u128(3));
        let mut states = taskboard_domain::SyncStates::new();
        states.set_stored(b1, SyncPhase::Syncing, None);
        states.set_stored(b2, SyncPhase::Offline, None);
        states.set_stored(
            b3,
            SyncPhase::Failed {
                last_error: taskboard_domain::SyncErrorKind::Network,
            },
            None,
        );
        let mut state = AppState::default();
        let stack_id = StackId::from_uuid(uuid::Uuid::from_u128(2));
        let task_id = TaskId::from_uuid(uuid::Uuid::from_u128(3));
        state.boards.insert(b1, board(b1));
        state.boards.insert(b2, board(b2));
        state.boards.insert(b3, board(b3));
        state.stacks.insert(stack_id, stack(stack_id, b1, 0));
        state.tasks.insert(task_id, task(task_id, stack_id, 0));
        let ops: Vec<taskboard_domain::PendingOp> = (0..2)
            .map(|i| taskboard_domain::PendingOp {
                op_id: taskboard_domain::OpId(uuid::Uuid::from_u128(10 + i)),
                op: taskboard_domain::LocalOp::UpdateTask(task_id),
                queued_at: chrono::Utc::now(),
            })
            .collect();
        taskboard_domain::derive_sync_states(
            &state.boards,
            &state.stacks,
            &state.tasks,
            &state.labels,
            &ops,
            &mut states,
        );
        assert_eq!(
            sync_badge_for_board(&states, Some(b1)),
            SyncBadge {
                phase: SyncPhaseUi::Syncing,
                pending: 2,
                sidecar: SidecarHealth::Healthy
            }
        );
        assert_eq!(
            sync_badge_for_board(&states, Some(b2)),
            SyncBadge {
                phase: SyncPhaseUi::Offline,
                pending: 0,
                sidecar: SidecarHealth::Healthy
            }
        );
        assert_eq!(
            sync_badge_for_board(&states, Some(b3)),
            SyncBadge {
                phase: SyncPhaseUi::Failed,
                pending: 0,
                sidecar: SidecarHealth::Healthy
            }
        );
        let default = SyncBadge {
            phase: SyncPhaseUi::Idle,
            pending: 0,
            sidecar: SidecarHealth::Healthy,
        };
        assert_eq!(sync_badge_for_board(&states, None), default, "no boards");
        let unknown = BoardId::from_uuid(uuid::Uuid::from_u128(0xFEED));
        assert_eq!(
            sync_badge_for_board(&states, Some(unknown)),
            default,
            "an unknown board reads the default row"
        );
    }

    #[test]
    fn stacks_in_board_filters_and_orders() {
        let b1 = BoardId::from_uuid(uuid::Uuid::now_v7());
        let b2 = BoardId::from_uuid(uuid::Uuid::now_v7());
        let s1 = StackId::from_uuid(uuid::Uuid::now_v7());
        let s2 = StackId::from_uuid(uuid::Uuid::now_v7());
        let s3 = StackId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        state.boards.insert(b1, board(b1));
        // (order, id) order with a tie on order broken by id.
        state.stacks.insert(s2, stack(s2, b1, 1));
        state.stacks.insert(s1, stack(s1, b1, 1));
        state.stacks.insert(s3, stack(s3, b2, 0));
        let mut deleted = stack(s3, b1, 2);
        deleted.deleted = true;
        state.stacks.insert(s3, deleted);

        let ordered: Vec<StackId> = stacks_in_board(&state, b1)
            .into_iter()
            .map(|s| s.id)
            .collect();
        let mut expected = vec![s1, s2];
        expected.sort();
        assert_eq!(ordered, expected, "other boards and tombstones filtered");
    }

    #[test]
    fn live_board_finds_the_single_live_board() {
        let b1 = BoardId::from_uuid(uuid::Uuid::now_v7());
        let b2 = BoardId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        assert_eq!(live_board(&state), None);
        state.boards.insert(b1, board(b1));
        assert_eq!(live_board(&state).map(|b| b.id), Some(b1));
        let mut dead = board(b1);
        dead.deleted = true;
        state.boards.insert(b1, dead);
        state.boards.insert(b2, board(b2));
        let mut archived = board(b2);
        archived.archived = true;
        state.boards.insert(b2, archived);
        assert_eq!(live_board(&state), None, "deleted/archived boards skipped");
    }

    #[test]
    fn live_boards_filters_to_live_boards_in_canonical_order() {
        // Deterministic ids: live low + live high, one deleted and one
        // archived between them; the walk order is board-id ascending
        // (the `boards list` order) and `live_board` is its first.
        let low = BoardId::from_uuid(uuid::Uuid::from_u128(0x01));
        let dead = BoardId::from_uuid(uuid::Uuid::from_u128(0x02));
        let archived = BoardId::from_uuid(uuid::Uuid::from_u128(0x03));
        let high = BoardId::from_uuid(uuid::Uuid::from_u128(0x04));
        let mut state = AppState::default();
        state.boards.insert(high, board(high));
        state.boards.insert(archived, {
            let mut b = board(archived);
            b.archived = true;
            b
        });
        state.boards.insert(dead, {
            let mut b = board(dead);
            b.deleted = true;
            b
        });
        state.boards.insert(low, board(low));

        let ids: Vec<BoardId> = live_boards(&state).into_iter().map(|b| b.id).collect();
        assert_eq!(ids, vec![low, high]);
        assert_eq!(live_board(&state).map(|b| b.id), Some(low));
    }

    #[test]
    fn banner_rows_cover_rule_and_ruleless_targets() {
        let now = Utc::now();
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        let ruled = TaskId::from_uuid(uuid::Uuid::now_v7());
        let ruleless = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();

        let mut t1 = task(ruled, stack, 0);
        t1.duedate = Some(now - TimeDelta::hours(1));
        let rule = ReminderRule {
            id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::now_v7()),
            task: ruled,
            offset: ReminderOffset::before(TimeDelta::minutes(-30)).expect("valid offset"),
            created_at: now,
            updated_at: now,
        };
        state.reminders.rules.insert(rule.id, rule);
        let mut t2 = task(ruleless, stack, 1);
        t2.duedate = Some(now - TimeDelta::hours(2));
        state.tasks.insert(ruled, t1);
        state.tasks.insert(ruleless, t2);

        let rows = banner_rows(&state, now);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].task, ruled, "canonical (order, id) order");
        assert_eq!(
            rows[0].target.reminder,
            ReminderRef::Rule(state.reminders.rules.values().next().expect("rule row").id)
        );
        assert_eq!(
            rows[0].target.occurrence,
            now - TimeDelta::hours(1) - TimeDelta::minutes(30)
        );
        assert_eq!(rows[1].target.reminder, ReminderRef::Default(ruleless));
        assert_eq!(rows[1].target.occurrence, now - TimeDelta::hours(2));

        // Banner truth ignores mute: a muted task still renders.
        state.reminders.muted.insert(ruleless);
        assert_eq!(banner_rows(&state, now).len(), 2);

        // Done tasks drop out.
        let mut t1 = state.tasks.get(&ruled).expect("task").clone();
        t1.done = Some(now);
        state.tasks.insert(ruled, t1);
        let rows = banner_rows(&state, now);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].task, ruleless);
    }

    #[test]
    fn scheduled_phrase_is_empty_when_absent_and_directional_when_set() {
        // Structural facts only (AGENTS §5), the `due_phrase` precedent:
        // the words are presentation wording, the direction is the
        // behavior — pinned as future ≠ past at the same bucket.
        let now = chrono::TimeZone::timestamp_opt(&Utc, 1_700_000_000, 0)
            .single()
            .expect("ts");
        assert_eq!(scheduled_phrase(None, now), "");
        assert!(
            !scheduled_phrase(Some(now), now).is_empty(),
            "the boundary bucket renders"
        );
        let future = scheduled_phrase(Some(now + TimeDelta::hours(2)), now);
        let past = scheduled_phrase(Some(now - TimeDelta::hours(2)), now);
        assert_ne!(future, "", "the future phrase renders");
        assert_ne!(past, "", "the past phrase renders");
        assert_ne!(future, past, "the direction is carried at one bucket");
    }

    #[test]
    fn effort_phrase_wraps_the_domain_text_form() {
        assert_eq!(effort_phrase(None), "");
        assert_eq!(
            effort_phrase(Some(Effort::new(5400))),
            Effort::new(5400).text_form()
        );
        assert_eq!(effort_phrase(Some(Effort::new(0))), "0s");
    }

    #[test]
    fn is_anchor_task_matches_the_exact_title_only() {
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        let mut anchor = task(TaskId::from_uuid(uuid::Uuid::now_v7()), stack, 0);
        anchor.title = taskboard_domain::ANCHOR_CARD_TITLE.to_owned();
        assert!(is_anchor_task(&anchor));
        for near_miss in [
            "[taskboard] app-data ",
            " [taskboard] app-data",
            "[taskboard] app-data.",
            "[TASKBOARD] APP-DATA",
        ] {
            let mut t = task(TaskId::from_uuid(uuid::Uuid::now_v7()), stack, 1);
            t.title = near_miss.to_owned();
            assert!(!is_anchor_task(&t), "{near_miss:?} is a visible near-miss");
        }
    }

    #[test]
    fn banner_rows_exclude_the_anchor() {
        let now = chrono::TimeZone::timestamp_opt(&Utc, 1_700_000_000, 0)
            .single()
            .expect("ts");
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        let mut overdue = task(TaskId::from_uuid(uuid::Uuid::now_v7()), stack, 0);
        overdue.duedate = Some(now - TimeDelta::hours(2));
        state.tasks.insert(overdue.id, overdue);
        let mut anchor = task(TaskId::from_uuid(uuid::Uuid::now_v7()), stack, 1);
        anchor.title = taskboard_domain::ANCHOR_CARD_TITLE.to_owned();
        anchor.duedate = Some(now - TimeDelta::hours(3));
        let anchor_id = anchor.id;
        state.tasks.insert(anchor_id, anchor);
        let rows = banner_rows(&state, now);
        assert_eq!(rows.len(), 1, "the anchor is filtered from the banner");
        assert!(!rows.iter().any(|r| r.task == anchor_id));
    }

    #[test]
    fn sidecar_caption_is_silent_when_healthy_and_typed_when_not() {
        assert_eq!(sidecar_caption(SidecarHealth::Healthy), "");
        let too_new = sidecar_caption(SidecarHealth::Unreadable(
            taskboard_domain::AppDataUnreadable::FormatTooNew { format: 2 },
        ));
        assert!(too_new.contains('2'), "the too-new format rides along");
        for reason in [
            taskboard_domain::AppDataUnreadable::Transport,
            taskboard_domain::AppDataUnreadable::Decode,
            taskboard_domain::AppDataUnreadable::ForeignBoard,
        ] {
            assert_ne!(sidecar_caption(SidecarHealth::Unreadable(reason)), "");
        }
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        /// `due_sets` membership pinned against independently computed
        /// predicates in the test (production keeps one implementation):
        /// overdue ⇔ `duedate < now` (strict), soon ⇔ `(now, now+24h]`.
        #[test]
        fn due_sets_match_the_domain_predicates(
            offset_secs in -172_800i64..=172_800,
            done in prop::bool::ANY,
        ) {
            let now = Utc::now();
            let stack = StackId::from_uuid(uuid::Uuid::now_v7());
            let id = TaskId::from_uuid(uuid::Uuid::now_v7());
            let mut t = task(id, stack, 0);
            t.duedate = Some(now + TimeDelta::seconds(offset_secs));
            if done {
                t.done = Some(now);
            }
            let mut state = AppState::default();
            state.tasks.insert(id, t.clone());

            let sets = due_sets(&state, now);
            let horizon = now + DUE_SOON_HORIZON;
            let overdue = !done && t.duedate.is_some_and(|d| d < now);
            let soon = !done && t.duedate.is_some_and(|d| d > now && d <= horizon);
            prop_assert_eq!(sets.overdue.contains(&id), overdue);
            prop_assert_eq!(sets.soon.contains(&id), soon);
            prop_assert!(!(sets.overdue.contains(&id) && sets.soon.contains(&id)));
        }

        /// `duration_bucket` is monotone and saturates without panic.
        #[test]
        fn duration_bucket_is_monotone(a in -10_000_000i64..=10_000_000, b in -10_000_000i64..=10_000_000) {
            let rank = |b: &DurationBucket| match *b {
                DurationBucket::Now => 0,
                DurationBucket::Minutes(n) => u64::from(n),
                DurationBucket::Hours(n) => u64::from(n) * 60,
                DurationBucket::Days(n) => u64::from(n) * 1440,
            };
            let (low, high) = if a <= b { (a, b) } else { (b, a) };
            prop_assert!(rank(&duration_bucket(TimeDelta::seconds(low)))
                <= rank(&duration_bucket(TimeDelta::seconds(high))));
        }

        /// `banner_rows` over generated states: live+open+overdue only,
        /// canonical order, targets well-formed.
        #[test]
        fn banner_rows_are_canonical_and_well_formed(
            offsets in prop::collection::vec(-48i64..48, 0..8),
            rules in prop::collection::vec(prop::bool::ANY, 0..8),
        ) {
            let now = Utc::now();
            let stack = StackId::from_uuid(uuid::Uuid::now_v7());
            let mut state = AppState::default();
            let rule_this = rules.first().copied().unwrap_or(false);
            for (i, offset) in offsets.iter().enumerate() {
                let id = TaskId::from_uuid(uuid::Uuid::now_v7());
                let mut t = task(id, stack, i64::try_from(i).unwrap_or(i64::MAX));
                t.duedate = Some(now + TimeDelta::hours(*offset));
                if *offset >= 0 {
                    t.done = Some(now); // only overdue tasks belong
                }
                state.tasks.insert(id, t);
                if rule_this {
                    let rule = ReminderRule {
                        id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::now_v7()),
                        task: id,
                        offset: ReminderOffset::before(TimeDelta::minutes(-15))
                            .expect("valid offset"),
                        created_at: now,
                        updated_at: now,
                    };
                    state.reminders.rules.insert(rule.id, rule);
                }
            }
            let rows = banner_rows(&state, now);
            let expected: Vec<TaskId> = taskboard_domain::overdue_tasks(&state.tasks, now)
                .into_iter()
                .map(|t| t.id)
                .collect();
            let got: Vec<TaskId> = rows.iter().map(|r| r.task).collect();
            prop_assert_eq!(got, expected);
            for row in &rows {
                prop_assert_eq!(row.kind, DueKind::Overdue);
                prop_assert_eq!(row.target.task, row.task);
                match row.target.reminder {
                    ReminderRef::Rule(_) => {
                        prop_assert!(state.reminders.rules.values().any(|r| r.task == row.task));
                    }
                    ReminderRef::Default(task) => prop_assert_eq!(task, row.task),
                }
            }
        }

        /// `stacks_in_board` equals the reference `(order, id)` sort of
        /// the board's live stacks (test-side model recomputation).
        #[test]
        fn stacks_in_board_matches_reference_sort(
            orders in prop::collection::vec(0i64..5, 1..6),
        ) {
            let b1 = BoardId::from_uuid(uuid::Uuid::now_v7());
            let b2 = BoardId::from_uuid(uuid::Uuid::now_v7());
            let mut state = AppState::default();
            state.boards.insert(b1, board(b1));
            let mut ids = Vec::new();
            for (i, order) in orders.iter().enumerate() {
                let id = StackId::from_uuid(uuid::Uuid::now_v7());
                ids.push((id, *order));
                state.stacks.insert(id, stack(id, b1, *order));
                if i == 0 {
                    let other = StackId::from_uuid(uuid::Uuid::now_v7());
                    state.stacks.insert(other, stack(other, b2, 0));
                }
            }
            let mut expected: Vec<(i64, StackId)> =
                ids.into_iter().map(|(id, order)| (order, id)).collect();
            expected.sort();
            let got: Vec<(i64, StackId)> = stacks_in_board(&state, b1)
                .into_iter()
                .map(|s| (s.order, s.id))
                .collect();
            prop_assert_eq!(got, expected);
        }
    }
}

/// A connector row's live status (round-1 D8): the down-override wins;
/// otherwise the bound boards' phases fold with the badge ordering
/// (`Syncing > Failed > Offline > Idle`, the `sync_badge_for_board`
/// precedent); no bound boards → `Idle` ("attached, nothing to do").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorStatus {
    Idle,
    Syncing,
    Offline,
    Failed,
    /// The `ConnectorLost` episode override (the liveness map, D8).
    Down,
}

/// The fold. Pure over the state snapshot plus the override flag —
/// unit-pinned (T4).
#[must_use]
pub fn connector_row_status(
    state: &AppState,
    connector: &taskboard_domain::ConnectorId,
    down: bool,
) -> ConnectorStatus {
    if down {
        return ConnectorStatus::Down;
    }
    let mut worst = ConnectorStatus::Idle;
    for board in state.bound_boards_of(connector) {
        let Some(status) = state.sync.get(board) else {
            continue;
        };
        let phase = match status.phase {
            SyncPhase::Syncing => ConnectorStatus::Syncing,
            SyncPhase::Failed { .. } => ConnectorStatus::Failed,
            SyncPhase::Offline => ConnectorStatus::Offline,
            SyncPhase::Idle => ConnectorStatus::Idle,
        };
        if phase_rank(phase) > phase_rank(worst) {
            worst = phase;
        }
    }
    worst
}

/// The badge ordering: `Syncing > Failed > Offline > Idle`.
fn phase_rank(phase: ConnectorStatus) -> u8 {
    match phase {
        ConnectorStatus::Idle => 0,
        ConnectorStatus::Offline => 1,
        ConnectorStatus::Failed => 2,
        ConnectorStatus::Syncing => 3,
        ConnectorStatus::Down => 4,
    }
}

#[cfg(test)]
mod connector_status_tests {
    use super::*;
    use taskboard_domain::{Board, BoardClocks, BoardId, ConnectorId, RemoteBoardBinding};

    fn bound_state(connector: &str, board: BoardId, phase: SyncPhase) -> AppState {
        let mut state = AppState::default();
        let epoch = BoardClocks::epoch();
        state.boards.insert(
            board,
            Board {
                id: board,
                remote: Some(RemoteBoardBinding {
                    board: taskboard_domain::RemoteBoardId(1),
                    connector: ConnectorId::from(connector),
                }),
                title: "b".into(),
                color: taskboard_domain::Color::new(""),
                archived: false,
                deleted: false,
                clocks: BoardClocks {
                    title: epoch,
                    color: epoch,
                    archived: epoch,
                    deleted: epoch,
                },
                remote_seen: None,
            },
        );
        set_stored(&mut state, board, phase);
        state
    }

    fn set_stored(state: &mut AppState, board: BoardId, phase: SyncPhase) {
        let mut states = taskboard_domain::SyncStates::new();
        states.set_stored(board, phase, None);
        state.sync = states;
    }

    /// T4 — the down-override wins; the phases fold with the badge
    /// ordering; empty bindings → Idle.
    #[test]
    fn the_row_status_folds_bound_phases_and_the_down_override_wins() {
        let board = BoardId::from_uuid(uuid::Uuid::now_v7());
        let connector = ConnectorId::from("nextcloud");

        // No boards at all: attached, nothing to do.
        let empty = AppState::default();
        assert_eq!(
            connector_row_status(&empty, &connector, false),
            ConnectorStatus::Idle
        );

        // The down override wins over any fold.
        let mut state = bound_state("nextcloud", board, SyncPhase::Syncing);
        assert_eq!(
            connector_row_status(&state, &connector, false),
            ConnectorStatus::Syncing
        );
        assert_eq!(
            connector_row_status(&state, &connector, true),
            ConnectorStatus::Down
        );

        // The ordering: Failed beats Offline; Syncing beats Failed.
        set_stored(&mut state, board, SyncPhase::Offline);
        assert_eq!(
            connector_row_status(&state, &connector, false),
            ConnectorStatus::Offline
        );
        set_stored(
            &mut state,
            board,
            SyncPhase::Failed {
                last_error: taskboard_domain::SyncErrorKind::Network,
            },
        );
        assert_eq!(
            connector_row_status(&state, &connector, false),
            ConnectorStatus::Failed
        );
    }
}

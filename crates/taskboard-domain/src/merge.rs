// SPDX-License-Identifier: MIT OR Apache-2.0
//! The sync conflict policy's per-entity algebra (ADR 0004, rules R1-R9):
//! pure, total, clock-free functions adopting, merging, and finalizing
//! each entity kind against remote observations and push echoes.
//!
//! Normative rules:
//!
//! - **Per-field LWW**: a remote field value wins iff
//!   `remote.last_modified > field_clock` (strictly); ties keep local.
//! - **Deletion**: boards/stacks/labels carry a real remote `deleted_at`
//!   and LWW against the local tombstone clock. Cards have no remote
//!   delete timestamp, so an observed remote absence always wins
//!   (delete-wins fallback, R3).
//! - **Labels** merge as one field (whole-set LWW); **position**
//!   (`stack` + `order`) is one composite field.
//! - **Push echoes** are adopted unconditionally (the server normalizes
//!   what we just wrote) with clocks advanced to
//!   `max(local, echo.last_modified)`.
//!
//! The fixed-order composition of these primitives is [`pipeline`]'s
//! `apply_sync_report`, which the state engine runs once per
//! `SyncReport::Completed`. All comparisons are cross-machine wall clock;
//! see ADR 0004's skew disclaimer. Tests use logical times and are
//! skew-free by construction.
//!
//! Unmapped-remote-stack strategy (the single rule every path follows):
//! no code path ever fabricates a stack binding. A remote card whose
//! stack has no local binding keeps its whole local position as a unit
//! ([`merge_task`], [`adopt_task_after_push`]) or is not adopted at all
//! ([`adopt_remote_task`] returns `None` — the pipeline defers it to the
//! next cycle, when its stack exists).
//!
//! These primitives are the public API of the sync conflict policy: the
//! state engine may call them directly (e.g. when executing single
//! commands); only `utc_min` and `remote_wins` stay internal.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};

use crate::entities::{Board, Label, Stack, Task};
use crate::ids::{LabelId, RemoteBoardId, RemoteLabelId, RemoteStackId, StackId, TaskId};
use crate::remote::{
    RemoteBoard, RemoteIndex, RemoteLabel, RemoteStack, RemoteTask, resolve_label,
};

/// Smallest representable `DateTime<Utc>`: never-newer than anything.
pub(crate) fn utc_min() -> DateTime<Utc> {
    DateTime::<Utc>::MIN_UTC
}

/// Remote value wins iff strictly newer; ties keep local.
pub(crate) fn remote_wins(remote_ts: DateTime<Utc>, field_clock: DateTime<Utc>) -> bool {
    remote_ts > field_clock
}

fn max_ts(a: DateTime<Utc>, b: DateTime<Utc>) -> DateTime<Utc> {
    if a > b { a } else { b }
}

/// R1 — adopt a remotely-created task under a fresh local id. All clocks
/// are stamped with the remote `last_modified`; remote label ids are
/// resolved through `ctx` (unmapped labels are dropped here; the pipeline
/// adopts the label entities before the tasks, so unmapped means the
/// label was already absent remotely).
///
/// Returns `None` when the card's remote stack has no local binding: the
/// pipeline adopts stacks before tasks, so an unresolved stack is absent
/// remotely too, and adopting would fabricate a dangling position. The
/// caller defers the card to the next sync cycle (see the module docs'
/// unmapped-stack strategy).
#[must_use]
pub fn adopt_remote_task(remote: &RemoteTask, id: TaskId, ctx: &RemoteIndex) -> Option<Task> {
    Some(Task {
        id,
        remote: Some(remote.id),
        title: remote.title.clone(),
        description: remote.description.clone(),
        duedate: remote.duedate,
        done: remote.done,
        stack: resolve_local_stack(ctx, remote.id.board, remote.stack)?,
        order: remote.order,
        labels: map_remote_labels(ctx, remote.id.board, &remote.labels),
        archived: remote.archived,
        deleted: false,
        clocks: clocks_from(remote.last_modified),
        remote_seen: Some(remote.last_modified),
    })
}

fn clocks_from(ts: DateTime<Utc>) -> crate::entities::TaskClocks {
    crate::entities::TaskClocks {
        title: ts,
        description: ts,
        duedate: ts,
        done: ts,
        position: ts,
        labels: ts,
        archived: ts,
        deleted: ts,
    }
}

fn resolve_local_stack(
    ctx: &RemoteIndex,
    board: RemoteBoardId,
    stack: RemoteStackId,
) -> Option<StackId> {
    ctx.stack_by_ref
        .get(&crate::ids::RemoteStackRef { board, stack })
        .copied()
}

fn map_remote_labels(
    ctx: &RemoteIndex,
    board: RemoteBoardId,
    remote: &BTreeSet<RemoteLabelId>,
) -> BTreeSet<LabelId> {
    remote
        .iter()
        .filter_map(|rl| resolve_label(ctx, board, *rl))
        .collect()
}

/// R4/R5/R7/R8 — per-field LWW merge of a bound task against a changed
/// remote snapshot entry (`remote.last_modified > local.remote_seen` is
/// the caller's responsibility; unchanged remotes never reach this).
///
/// The `deleted` field participates like any other: a local tombstone is
/// resurrected iff the remote write is strictly newer than the tombstone
/// (R5); otherwise the tombstone stands and its pending delete re-pushes.
#[must_use]
pub fn merge_task(local: &Task, remote: &RemoteTask, ctx: &RemoteIndex) -> Task {
    let ts_r = remote.last_modified;
    let mut merged = local.clone();

    if remote_wins(ts_r, local.clocks.title) {
        merged.title.clone_from(&remote.title);
        merged.clocks.title = ts_r;
    }
    if remote_wins(ts_r, local.clocks.description) {
        merged.description.clone_from(&remote.description);
        merged.clocks.description = ts_r;
    }
    if remote_wins(ts_r, local.clocks.duedate) {
        merged.duedate = remote.duedate;
        merged.clocks.duedate = ts_r;
    }
    if remote_wins(ts_r, local.clocks.done) {
        merged.done = remote.done;
        merged.clocks.done = ts_r;
    }
    // R8: position is one composite field.
    if remote_wins(ts_r, local.clocks.position)
        && let Some(stack) = resolve_local_stack(ctx, remote.id.board, remote.stack)
    {
        merged.stack = stack;
        merged.order = remote.order;
        merged.clocks.position = ts_r;
    }
    // R7: the label set is one field; unmapped remote labels are dropped
    // (the pipeline adopts label entities first).
    if remote_wins(ts_r, local.clocks.labels) {
        merged.labels = map_remote_labels(ctx, remote.id.board, &remote.labels);
        merged.clocks.labels = ts_r;
    }
    if remote_wins(ts_r, local.clocks.archived) {
        merged.archived = remote.archived;
        merged.clocks.archived = ts_r;
    }

    // R5, `deleted`: resurrect iff the remote write is strictly newer.
    if local.deleted && remote_wins(ts_r, local.clocks.deleted) {
        merged.deleted = false;
        merged.clocks.deleted = ts_r;
    }

    merged.remote = Some(remote.id);
    merged.remote_seen = Some(ts_r);
    merged
}

/// R3 — observed remote absence of a bound card: delete-wins fallback.
/// The tombstone's write time is the *observation* time (`observed_at`,
/// i.e. "now" passed in by the engine); the remote binding is cleared.
#[must_use]
pub fn tombstone_task(local: &Task, observed_at: DateTime<Utc>) -> Task {
    let mut tombstoned = local.clone();
    tombstoned.deleted = true;
    tombstoned.clocks.deleted = max_ts(local.clocks.deleted, observed_at);
    tombstoned.remote = None;
    tombstoned.remote_seen = None;
    tombstoned
}

/// R9a/R9b — unconditional echo adoption after a create/update push. The
/// server normalizes what we just wrote, so values are taken from the
/// echo as-is (NOT LWW) and clocks advance to
/// `max(local, echo.last_modified)` — a seconds-resolution echo can never
/// be "newer" than the just-made local edit, yet its normalization must
/// land.
#[must_use]
pub fn adopt_task_after_push(local: &Task, echo: &RemoteTask, ctx: &RemoteIndex) -> Task {
    let lm = echo.last_modified;
    Task {
        id: local.id,
        remote: Some(echo.id),
        title: echo.title.clone(),
        description: echo.description.clone(),
        duedate: echo.duedate,
        done: echo.done,
        stack: resolve_local_stack(ctx, echo.id.board, echo.stack).unwrap_or(local.stack),
        order: echo.order,
        labels: map_remote_labels(ctx, echo.id.board, &echo.labels),
        archived: echo.archived,
        deleted: false,
        clocks: crate::entities::TaskClocks {
            title: max_ts(local.clocks.title, lm),
            description: max_ts(local.clocks.description, lm),
            duedate: max_ts(local.clocks.duedate, lm),
            done: max_ts(local.clocks.done, lm),
            position: max_ts(local.clocks.position, lm),
            labels: max_ts(local.clocks.labels, lm),
            archived: max_ts(local.clocks.archived, lm),
            deleted: max_ts(local.clocks.deleted, lm),
        },
        remote_seen: Some(lm),
    }
}

/// R9c — the delete push was accepted: finalize the tombstone, clear the
/// remote binding. The next pull cannot resurrect: the tombstone clock is
/// `>=` any stale listing entry (cache-lag safe by construction).
#[must_use]
pub fn finalize_pushed_task_delete(local: &Task) -> Task {
    let mut finalized = local.clone();
    finalized.deleted = true;
    finalized.remote = None;
    finalized.remote_seen = None;
    finalized
}

/// R6 — merge a bound stack against a changed remote entry. The remote
/// `deleted_at` is a timestamped fact and LWWs against the local
/// tombstone clock exactly like the card `deleted` field (R5).
#[must_use]
pub fn merge_stack(local: &Stack, remote: &RemoteStack) -> Stack {
    let ts_r = remote.last_modified;
    let mut merged = local.clone();

    if remote_wins(ts_r, local.clocks.title) {
        merged.title.clone_from(&remote.title);
        merged.clocks.title = ts_r;
    }
    if remote_wins(ts_r, local.clocks.order) {
        merged.order = remote.order;
        merged.clocks.order = ts_r;
    }
    // No local archived commands yet: adopt remote content on change.
    merged.archived = remote.archived;

    // The local side defends with its latest local activity: a rename
    // newer than the remote delete stamp keeps the stack live (worked
    // example 5), while an untouched stack loses to the delete.
    let local_latest = max_ts(
        max_ts(local.clocks.title, local.clocks.order),
        local.clocks.deleted,
    );
    if let Some((deleted, clock)) = soft_delete_verdict(
        local_latest,
        local.deleted,
        local.clocks.deleted,
        remote.deleted_at,
        ts_r,
    ) {
        merged.deleted = deleted;
        merged.clocks.deleted = clock;
    }

    merged.remote = Some(remote.id);
    merged.remote_seen = Some(ts_r);
    merged
}

/// R5/R6 soft-delete defense shared by stacks and labels: decide the
/// tombstone outcome from the local side's latest activity, the remote
/// soft-delete stamp, and the remote entity stamp. Returns `None` when
/// the local `deleted` flag and clock must stay untouched; otherwise the
/// new `(deleted, deleted-clock)` pair.
fn soft_delete_verdict(
    local_latest: DateTime<Utc>,
    local_deleted: bool,
    local_deleted_clock: DateTime<Utc>,
    remote_deleted_at: Option<DateTime<Utc>>,
    ts_r: DateTime<Utc>,
) -> Option<(bool, DateTime<Utc>)> {
    match remote_deleted_at {
        Some(deleted_at) if remote_wins(deleted_at, local_latest) => Some((true, deleted_at)),
        _ if local_deleted && remote_wins(ts_r, local_deleted_clock) => Some((false, ts_r)),
        _ => None,
    }
}

/// R1 for stacks: adopt a remotely-created stack under a fresh local id.
#[must_use]
pub fn adopt_remote_stack(remote: &RemoteStack, id: StackId, board: crate::ids::BoardId) -> Stack {
    Stack {
        id,
        remote: Some(remote.id),
        board,
        title: remote.title.clone(),
        order: remote.order,
        archived: remote.archived,
        deleted: remote.deleted_at.is_some(),
        clocks: crate::entities::StackClocks {
            title: remote.last_modified,
            order: remote.last_modified,
            deleted: remote.deleted_at.unwrap_or(remote.last_modified),
        },
        remote_seen: Some(remote.last_modified),
    }
}

/// R9a for stacks: unconditional echo adoption after create/rename push.
#[must_use]
pub fn adopt_stack_after_push(local: &Stack, echo: &RemoteStack) -> Stack {
    let lm = echo.last_modified;
    Stack {
        id: local.id,
        remote: Some(echo.id),
        board: local.board,
        title: echo.title.clone(),
        order: echo.order,
        archived: echo.archived,
        deleted: false,
        clocks: crate::entities::StackClocks {
            title: max_ts(local.clocks.title, lm),
            order: max_ts(local.clocks.order, lm),
            deleted: max_ts(local.clocks.deleted, lm),
        },
        remote_seen: Some(lm),
    }
}

/// R6 — merge a bound label against a changed remote entry.
#[must_use]
pub fn merge_label(local: &Label, remote: &RemoteLabel) -> Label {
    let ts_r = remote.last_modified;
    let mut merged = local.clone();

    if remote_wins(ts_r, local.clocks.title) {
        merged.title.clone_from(&remote.title);
        merged.clocks.title = ts_r;
    }
    if remote_wins(ts_r, local.clocks.color) {
        merged.color = crate::entities::Color::new(remote.color.clone());
        merged.clocks.color = ts_r;
    }

    // Same defense as stacks: the latest local activity beats an older
    // remote soft delete.
    let local_latest = max_ts(
        max_ts(local.clocks.title, local.clocks.color),
        local.clocks.deleted,
    );
    if let Some((deleted, clock)) = soft_delete_verdict(
        local_latest,
        local.deleted,
        local.clocks.deleted,
        remote.deleted_at,
        ts_r,
    ) {
        merged.deleted = deleted;
        merged.clocks.deleted = clock;
    }

    merged.remote = Some(remote.id);
    merged.remote_seen = Some(ts_r);
    merged
}

/// R1 for labels: adopt a remotely-created label under a fresh local id.
#[must_use]
pub fn adopt_remote_label(remote: &RemoteLabel, id: LabelId, board: crate::ids::BoardId) -> Label {
    Label {
        id,
        remote: Some(remote.id),
        board,
        title: remote.title.clone(),
        color: crate::entities::Color::new(remote.color.clone()),
        deleted: remote.deleted_at.is_some(),
        clocks: crate::entities::LabelClocks {
            title: remote.last_modified,
            color: remote.last_modified,
            deleted: remote.deleted_at.unwrap_or(remote.last_modified),
        },
        remote_seen: Some(remote.last_modified),
    }
}

/// R9a for labels: unconditional echo adoption after create/update push.
#[must_use]
pub fn adopt_label_after_push(local: &Label, echo: &RemoteLabel) -> Label {
    let lm = echo.last_modified;
    Label {
        id: local.id,
        remote: Some(echo.id),
        board: local.board,
        title: echo.title.clone(),
        color: crate::entities::Color::new(echo.color.clone()),
        deleted: false,
        clocks: crate::entities::LabelClocks {
            title: max_ts(local.clocks.title, lm),
            color: max_ts(local.clocks.color, lm),
            deleted: max_ts(local.clocks.deleted, lm),
        },
        remote_seen: Some(lm),
    }
}

/// R6 for boards: adopt changed remote board content. Boards have no
/// local-edit commands in the MVP, so the remote content is authoritative
/// whenever it is strictly newer than `remote_seen`.
///
/// This is deliberately *not* a per-field merge: a true field-level board
/// merge needs `BoardClocks` plus local board-edit commands to stamp them
/// (see `entities.rs`). Until those exist there is nothing local to
/// defend, so every difference would resolve to remote anyway and
/// all-or-nothing adoption is exactly equivalent. Introduce the per-field
/// variant together with board-edit commands, not before.
#[must_use]
pub fn adopt_board_if_newer(local: &Board, remote: &RemoteBoard) -> Board {
    if !remote_wins(
        remote.last_modified,
        local.remote_seen.unwrap_or_else(utc_min),
    ) {
        return local.clone();
    }
    Board {
        id: local.id,
        remote: Some(remote.id),
        title: remote.title.clone(),
        color: crate::entities::Color::new(remote.color.clone()),
        archived: remote.archived,
        deleted: remote.deleted_at.is_some(),
        remote_seen: Some(remote.last_modified),
    }
}

/// R1 for boards: adopt a remotely-created board under a fresh local id.
#[must_use]
pub fn adopt_remote_board(remote: &RemoteBoard, id: crate::ids::BoardId) -> Board {
    Board {
        id,
        remote: Some(remote.id),
        title: remote.title.clone(),
        color: crate::entities::Color::new(remote.color.clone()),
        archived: remote.archived,
        deleted: remote.deleted_at.is_some(),
        remote_seen: Some(remote.last_modified),
    }
}

/// R3/R6 for stacks — observed remote absence of a bound stack (or a
/// local delete push that must be finalized): tombstone it. The
/// tombstone's write time never moves backwards; the remote binding is
/// cleared.
#[must_use]
pub fn tombstone_stack(local: &Stack, observed_at: DateTime<Utc>) -> Stack {
    let mut s = local.clone();
    s.deleted = true;
    s.clocks.deleted = max_ts(local.clocks.deleted, observed_at);
    s.remote = None;
    s.remote_seen = None;
    s
}

/// R3/R6 for labels — observed remote absence of a bound label (or a
/// local delete push that must be finalized): tombstone it. The
/// tombstone's write time never moves backwards; the remote binding is
/// cleared.
#[must_use]
pub fn tombstone_label(local: &Label, observed_at: DateTime<Utc>) -> Label {
    let mut l = local.clone();
    l.deleted = true;
    l.clocks.deleted = max_ts(local.clocks.deleted, observed_at);
    l.remote = None;
    l.remote_seen = None;
    l
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge_testutil::*;
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    fn ts_millis(secs: i64) -> i64 {
        secs
    }

    #[test]
    fn disjoint_field_edits_both_survive() {
        let mut local = bound_task(1);
        local.title = "edited locally".into();
        local.clocks.title = ts(BASE + 330); // 10:05:30
        let mut remote = remote_task(1, 1, ts_millis(BASE + 180)); // 10:03:00
        remote.description = "edited remotely".into();

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx);

        assert_eq!(merged.title, "edited locally");
        assert_eq!(merged.description, "edited remotely");
    }

    #[test]
    fn granularity_trap_remote_entity_touch_overwrites_local_title() {
        let mut local = bound_task(1);
        local.title = "edited locally".into();
        local.clocks.title = ts(BASE + 330); // 10:05:30
        let remote = remote_task(1, 1, BASE + 360); // 10:06:00, description edit

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx);

        // The unchanged remote title wins over the newer local edit —
        // Deck's entity-level granularity. Pinned as intentional (ADR 0004).
        assert_eq!(merged.title, remote.title);
        assert_eq!(merged.description, remote.description);
    }

    #[test]
    fn tie_keeps_local_every_field() {
        let mut local = bound_task(1);
        local.title = "local title".into();
        local.description = "local description".into();
        local.order = 42;
        local.archived = true;
        let mut remote = remote_task(1, 1, BASE); // ts_r == every clock
        remote.title = "remote title".into();
        remote.description = "remote description".into();
        remote.order = 99;
        remote.archived = false;

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx);

        assert_eq!(merged.title, "local title");
        assert_eq!(merged.description, "local description");
        assert_eq!(merged.order, 42);
        assert!(merged.archived);
    }

    #[test]
    fn tombstone_never_moves_an_existing_tombstone_clock_backwards() {
        let mut local = bound_task(1);
        local.deleted = true;
        local.clocks.deleted = ts(BASE + 7_200); // tombstoned earlier offline

        let reobserved = tombstone_task(&local, ts(BASE));
        assert_eq!(
            reobserved.clocks.deleted,
            ts(BASE + 7_200),
            "re-observation must not decrease the tombstone clock"
        );
    }

    #[test]
    fn newer_remote_edit_resurrects_local_tombstone() {
        let mut local = bound_task(1);
        local.deleted = true;
        local.clocks.deleted = ts(BASE + 300); // delete at 10:05
        let remote = remote_task(1, 1, BASE + 360); // remote edit at 10:06

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx);

        assert!(!merged.deleted, "newer remote edit resurrects");
        assert_eq!(merged.remote, Some(card_ref(1, 1)));
    }

    #[test]
    fn older_remote_edit_keeps_tombstone() {
        let mut local = bound_task(1);
        local.deleted = true;
        local.clocks.deleted = ts(BASE + 300); // delete at 10:05
        let remote = remote_task(1, 1, BASE + 180); // remote edit at 10:03

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx);

        assert!(merged.deleted, "older remote edit cannot resurrect");
    }

    #[test]
    fn newer_local_rename_beats_older_stack_delete() {
        let mut local = local_stack();
        local.title = "renamed".into();
        local.clocks.title = ts(BASE + 480); // 10:08
        let mut remote = remote_stack(1, BASE + 420); // 10:07
        remote.deleted_at = Some(ts(BASE + 420));

        let merged = merge_stack(&local, &remote);
        assert!(!merged.deleted, "rename is newer than the soft delete");
        assert_eq!(merged.title, "renamed");
    }

    #[test]
    fn newer_remote_edit_resurrects_locally_tombstoned_stack_and_label() {
        let mut stack = local_stack();
        stack.deleted = true;
        stack.clocks.deleted = ts(BASE + 300);
        let resurrected = merge_stack(&stack, &remote_stack(1, BASE + 360));
        assert!(!resurrected.deleted, "stack: newer remote edit resurrects");

        let mut label = bound_label(1);
        label.deleted = true;
        label.clocks.deleted = ts(BASE + 300);
        let label_revived = merge_label(&label, &remote_label(1, BASE + 360));
        assert!(
            !label_revived.deleted,
            "label: newer remote edit resurrects"
        );
    }

    #[test]
    fn older_remote_edit_keeps_local_stack_and_label_tombstones() {
        let mut stack = local_stack();
        stack.deleted = true;
        stack.clocks.deleted = ts(BASE + 300);
        assert!(merge_stack(&stack, &remote_stack(1, BASE + 60)).deleted);

        let mut label = bound_label(1);
        label.deleted = true;
        label.clocks.deleted = ts(BASE + 300);
        assert!(merge_label(&label, &remote_label(1, BASE + 60)).deleted);
    }

    #[test]
    fn remote_task_with_unmapped_stack_is_not_adopted() {
        // The single unmapped-stack strategy: adoption is deferred (None)
        // instead of fabricating a dangling stack binding.
        let remote = remote_task(1, 9, BASE + 60);
        let adopted = adopt_remote_task(
            &remote,
            TaskId::from(uuid::Uuid::from_u128(778)),
            &RemoteIndex::default(),
        );
        assert!(adopted.is_none());
    }

    #[test]
    fn new_remote_task_adopts_with_resolved_labels() {
        let label_id = LabelId::from(uuid::Uuid::from_u128(700));
        let mut label_map = RemoteIndex::default();
        label_map
            .stack_by_ref
            .insert(stack_ref(1), StackId::from(uuid::Uuid::from_u128(701)));
        label_map.label_by_ref.insert(
            crate::ids::RemoteLabelRef {
                board: board(REMOTE_BOARD_NUM),
                label: crate::ids::RemoteLabelId(5),
            },
            label_id,
        );

        let mut remote = remote_task(1, 9, BASE + 60);
        remote.labels.insert(crate::ids::RemoteLabelId(5));

        let state = base_state(bound_task(1));
        // The index for the merge comes from the state; the label binding
        // arrives through the pipeline after label adoption. Simulate the
        // post-adoption index by merging directly with the extended map.
        let adopted = adopt_remote_task(
            &remote,
            TaskId::from(uuid::Uuid::from_u128(777)),
            &label_map,
        )
        .expect("mapped stack adopts");
        assert_eq!(adopted.labels, BTreeSet::from([label_id]));
        assert_eq!(adopted.clocks.title, remote.last_modified);
        assert_eq!(adopted.remote_seen, Some(remote.last_modified));
        assert!(!adopted.deleted);
        let _ = state;
    }

    #[test]
    fn merge_label_adopts_newer_title_and_color() {
        let merged = merge_label(&bound_label(1), &remote_label(1, BASE + 60));
        assert_eq!(merged.title, "label 1 renamed");
        assert_eq!(merged.color.as_str(), "0000ff");
        assert_eq!(merged.clocks.title, ts(BASE + 60));
        assert!(!merged.deleted);
    }

    #[test]
    fn merge_label_tie_keeps_local() {
        let merged = merge_label(&bound_label(1), &remote_label(1, BASE));
        assert_eq!(merged.title, "label 1");
        assert_eq!(merged.color.as_str(), "00ff00");
    }

    #[test]
    fn newer_local_label_edit_beats_older_remote_delete() {
        let mut local = bound_label(1);
        local.title = "renamed locally".into();
        local.clocks.title = ts(BASE + 480);
        let mut remote = remote_label(1, BASE + 420);
        remote.deleted_at = Some(ts(BASE + 420));

        let merged = merge_label(&local, &remote);
        assert!(!merged.deleted);
        assert_eq!(merged.title, "renamed locally");
    }

    #[test]
    fn newer_remote_label_delete_beats_older_local_edit() {
        let mut local = bound_label(1);
        local.title = "renamed locally".into();
        local.clocks.title = ts(BASE + 300);
        let mut remote = remote_label(1, BASE + 420);
        remote.deleted_at = Some(ts(BASE + 420));

        let merged = merge_label(&local, &remote);
        assert!(merged.deleted);
        assert_eq!(merged.clocks.deleted, ts(BASE + 420));
    }

    #[test]
    fn adopt_board_if_newer_adopts_when_newer_and_keeps_local_on_tie() {
        let local = base_state(bound_task(1))
            .boards
            .values()
            .next()
            .unwrap()
            .clone();

        let merged = adopt_board_if_newer(&local, &remote_board(REMOTE_BOARD_NUM, BASE + 60));
        assert_eq!(merged.title, "board 77");

        let tie = adopt_board_if_newer(&local, &remote_board(REMOTE_BOARD_NUM, BASE));
        assert_eq!(tie, local, "tie keeps local");
    }

    #[test]
    fn adopt_board_if_newer_treats_missing_remote_seen_as_oldest() {
        let mut local = base_state(bound_task(1))
            .boards
            .values()
            .next()
            .unwrap()
            .clone();
        local.remote_seen = None;

        let merged = adopt_board_if_newer(&local, &remote_board(REMOTE_BOARD_NUM, 0));
        assert_eq!(merged.title, "board 77");
        assert_eq!(merged.remote_seen, Some(ts(0)));
    }

    #[test]
    fn adopt_remote_board_stamps_remote_content() {
        let id = crate::ids::BoardId::from(uuid::Uuid::from_u128(1234));
        let adopted = adopt_remote_board(&remote_board(99, BASE + 5), id);
        assert_eq!(adopted.id, id);
        assert_eq!(adopted.remote, Some(board(99)));
        assert_eq!(adopted.remote_seen, Some(ts(BASE + 5)));
        assert!(!adopted.deleted);
    }

    #[test]
    fn adopt_remote_stack_binds_board_and_stamps_clocks() {
        let id = StackId::from(uuid::Uuid::from_u128(4321));
        let board_id = crate::ids::BoardId::from(uuid::Uuid::from_u128(800));
        let adopted = adopt_remote_stack(&remote_stack(3, BASE + 5), id, board_id);
        assert_eq!(adopted.remote, Some(stack_ref(3)));
        assert_eq!(adopted.board, board_id);
        assert_eq!(adopted.clocks.title, ts(BASE + 5));
        assert!(!adopted.deleted);

        let mut deleted = remote_stack(3, BASE + 5);
        deleted.deleted_at = Some(ts(BASE + 5));
        let tombstone = adopt_remote_stack(&deleted, id, board_id);
        assert!(tombstone.deleted);
    }

    #[test]
    fn adopt_stack_after_push_is_unconditional() {
        let mut local = local_stack();
        local.remote = None;
        local.title = "  padded  ".into();
        let mut echo = remote_stack(1, BASE + 60);
        echo.title = "padded".into();

        let merged = adopt_stack_after_push(&local, &echo);
        assert_eq!(merged.title, "padded");
        assert_eq!(merged.remote, Some(stack_ref(1)));
        assert_eq!(merged.clocks.title, echo.last_modified);
    }

    #[test]
    fn adopt_label_after_push_is_unconditional() {
        let mut local = bound_label(1);
        local.remote = None;
        let mut echo = remote_label(1, BASE + 60);
        echo.title = "normalized".into();

        let merged = adopt_label_after_push(&local, &echo);
        assert_eq!(merged.title, "normalized");
        assert_eq!(merged.remote, Some(echo.id));
        assert_eq!(merged.clocks.title, echo.last_modified);
        assert!(!merged.deleted);
    }

    #[test]
    fn finalize_pushed_task_delete_clears_binding() {
        let mut local = bound_task(1);
        local.deleted = true;
        let finalized = finalize_pushed_task_delete(&local);
        assert!(finalized.deleted);
        assert!(finalized.remote.is_none());
        assert!(finalized.remote_seen.is_none());
        assert_eq!(finalized.title, local.title, "content is kept");
    }

    #[test]
    fn merge_task_adopts_position_through_the_stack_index() {
        let mut local = bound_task(1);
        local.order = 42;
        let mut ctx = RemoteIndex::default();
        ctx.stack_by_ref.insert(stack_ref(1), local.stack);

        let mut remote = remote_task(1, 1, BASE + 60);
        remote.order = 77;

        let merged = merge_task(&local, &remote, &ctx);
        assert_eq!(
            merged.stack, local.stack,
            "remote stack maps to the local one"
        );
        assert_eq!(merged.order, 77, "position adopted as one unit");
        assert_eq!(merged.clocks.position, ts(BASE + 60));

        // Unmapped remote stack: the composite position is kept as a unit.
        let mut unmapped = RemoteIndex::default();
        unmapped
            .stack_by_ref
            .insert(stack_ref(9), StackId::from(uuid::Uuid::from_u128(5)));
        let kept = merge_task(&local, &remote, &unmapped);
        assert_eq!(kept.stack, local.stack);
        assert_eq!(kept.order, 42, "unmapped stack keeps the whole position");
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn merge_never_decreases_field_clocks(
            local_ts in 0_i64..10_000,
            remote_ts in 0_i64..10_000,
            card in 1_u64..100,
        ) {
            let mut local = bound_task(card);
            local.clocks = clocks_at(local_ts);
            let remote = remote_task(1, card, remote_ts);

            let ctx = RemoteIndex::default();
            let merged = merge_task(&local, &remote, &ctx);

            prop_assert!(merged.clocks.title >= local.clocks.title);
            prop_assert!(merged.clocks.description >= local.clocks.description);
            prop_assert!(merged.clocks.duedate >= local.clocks.duedate);
            prop_assert!(merged.clocks.done >= local.clocks.done);
            prop_assert!(merged.clocks.position >= local.clocks.position);
            prop_assert!(merged.clocks.labels >= local.clocks.labels);
            prop_assert!(merged.clocks.archived >= local.clocks.archived);
            prop_assert!(merged.clocks.deleted >= local.clocks.deleted);
        }

        #[test]
        fn merge_is_total_and_strictly_newer_wins(
            local_title_clock in 0_i64..10_000,
            remote_ts in 0_i64..10_000,
            card in 1_u64..100,
            local_title in "[a-z]{0,16}",
            remote_title in "[a-z]{0,16}",
        ) {
            let mut local = bound_task(card);
            local.title = local_title.clone();
            local.clocks.title = ts(local_title_clock);
            let mut remote = remote_task(1, card, remote_ts);
            remote.title = remote_title.clone();

            let ctx = RemoteIndex::default();
            let merged = merge_task(&local, &remote, &ctx);
            // Total: no panic. Tie-break: strict `>` — equal timestamps keep
            // the local value (this kills `>=` mutants).
            let expected = if remote_ts > local_title_clock {
                remote_title
            } else {
                local_title
            };
            prop_assert_eq!(merged.title.clone(), expected);

            // Idempotence: merging again with the same remote changes nothing.
            let again = merge_task(&merged, &remote, &ctx);
            prop_assert_eq!(again, merged);
        }
    }
}

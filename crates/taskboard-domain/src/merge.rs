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
//!
//! # Logging contract
//!
//! `trace` events carry entry identities (local id, remote ref, compared
//! timestamps, and the title — the only user content, and only there);
//! `debug` events carry rule verdicts as typed facts (rule tags, outcome
//! discriminants, booleans, ids, timestamps — never prose). Unchanged-
//! remote fast paths are silent by design. Tests never assert this text
//! (AGENTS §5); the events are behavior-neutral by construction.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};

use crate::entities::{Board, Label, Stack, Task};
use crate::ids::{LabelId, RemoteBoardId, RemoteLabelId, RemoteStackId, StackId, TaskId};
use crate::remote::{
    RemoteBoard, RemoteIndex, RemoteLabel, RemoteStack, RemoteTask, resolve_label,
};

/// Rule tags for the conflict-policy `tracing` events: single
/// definitions, because tests are forbidden from asserting log text.
pub(crate) const RULE_R1: &str = "R1";
pub(crate) const RULE_R3: &str = "R3";
pub(crate) const RULE_R4: &str = "R4";
pub(crate) const RULE_R5: &str = "R5";
pub(crate) const RULE_R6: &str = "R6";
pub(crate) const RULE_R9: &str = "R9";

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
    tracing::trace!(
        task_id = ?id,
        card = ?remote.id,
        stack = ?remote.stack,
        remote_ts = ?remote.last_modified,
        title = %remote.title,
        "task adopt entry"
    );
    let Some(stack_id) = resolve_local_stack(ctx, remote.id.board, remote.stack) else {
        tracing::debug!(
            rule = RULE_R1,
            task_id = ?id,
            card = ?remote.id,
            outcome = "deferred_unmapped_stack",
            "task adoption deferred"
        );
        return None;
    };
    tracing::debug!(rule = RULE_R1, task_id = ?id, stack_id = ?stack_id, "task adopted");
    Some(Task {
        id,
        remote: Some(remote.id),
        title: remote.title.clone(),
        description: remote.description.clone(),
        duedate: remote.duedate,
        scheduled: remote.scheduled,
        done: remote.done,
        stack: stack_id,
        order: remote.order,
        labels: map_remote_labels(ctx, remote.id.board, &remote.labels),
        effort: None,
        importance: None,
        urgency: None,
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
        scheduled: ts,
        done: ts,
        position: ts,
        labels: ts,
        effort: ts,
        importance: ts,
        urgency: ts,
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
pub fn merge_task(
    local: &Task,
    remote: &RemoteTask,
    ctx: &RemoteIndex,
    native_scheduled: bool,
) -> Task {
    let ts_r = remote.last_modified;
    tracing::trace!(
        task_id = ?local.id,
        card = ?remote.id,
        remote_ts = ?ts_r,
        remote_seen = ?local.remote_seen,
        title = %local.title,
        "task merge entry"
    );
    let mut merged = local.clone();

    let title_remote_wins = remote_wins(ts_r, local.clocks.title);
    if title_remote_wins {
        merged.title.clone_from(&remote.title);
        merged.clocks.title = ts_r;
    }
    let description_remote_wins = remote_wins(ts_r, local.clocks.description);
    if description_remote_wins {
        merged.description.clone_from(&remote.description);
        merged.clocks.description = ts_r;
    }
    let duedate_remote_wins = remote_wins(ts_r, local.clocks.duedate);
    if duedate_remote_wins {
        merged.duedate = remote.duedate;
        merged.clocks.duedate = ts_r;
    }
    // D-V2's sharp edge: while the connector lacks native `startdate`
    // writes, *nothing native* touches `scheduled` — the always-absent
    // remote value must never clobber local state via "remote wins,
    // value null". The field belongs exclusively to the sidecar fold.
    let scheduled_remote_wins = native_scheduled && remote_wins(ts_r, local.clocks.scheduled);
    if scheduled_remote_wins {
        merged.scheduled = remote.scheduled;
        merged.clocks.scheduled = ts_r;
    }
    let done_remote_wins = remote_wins(ts_r, local.clocks.done);
    if done_remote_wins {
        merged.done = remote.done;
        merged.clocks.done = ts_r;
    }
    // R8: position is one composite field.
    let position_remote_wins = remote_wins(ts_r, local.clocks.position);
    let position_stack = if position_remote_wins {
        resolve_local_stack(ctx, remote.id.board, remote.stack)
    } else {
        None
    };
    if position_remote_wins && let Some(stack) = position_stack {
        merged.stack = stack;
        merged.order = remote.order;
        merged.clocks.position = ts_r;
    }
    // R7: the label set is one field; unmapped remote labels are dropped
    // (the pipeline adopts label entities first).
    let labels_remote_wins = remote_wins(ts_r, local.clocks.labels);
    if labels_remote_wins {
        merged.labels = map_remote_labels(ctx, remote.id.board, &remote.labels);
        merged.clocks.labels = ts_r;
    }
    let archived_remote_wins = remote_wins(ts_r, local.clocks.archived);
    if archived_remote_wins {
        merged.archived = remote.archived;
        merged.clocks.archived = ts_r;
    }

    // R5, `deleted`: resurrect iff the remote write is strictly newer.
    let resurrected = local.deleted && remote_wins(ts_r, local.clocks.deleted);
    if resurrected {
        merged.deleted = false;
        merged.clocks.deleted = ts_r;
    }

    tracing::debug!(
        rule = RULE_R4,
        task_id = ?local.id,
        remote_ts = ?ts_r,
        title_remote_wins,
        description_remote_wins,
        duedate_remote_wins,
        scheduled_remote_wins,
        done_remote_wins,
        archived_remote_wins,
        labels_remote_wins,
        position_remote_wins,
        position_stack_resolved = position_stack.is_some(),
        "task field verdicts"
    );
    if local.deleted {
        tracing::debug!(
            rule = RULE_R5,
            task_id = ?local.id,
            outcome = if resurrected { "resurrected" } else { "tombstone_stands" },
            remote_ts = ?ts_r,
            deleted_clock = ?local.clocks.deleted,
            "task tombstone verdict"
        );
    }

    merged.remote = Some(remote.id);
    merged.remote_seen = Some(ts_r);
    merged
}

/// The sidecar document fold for one card's app fields (D-S3, chunk 4's
/// D1): applies `entry`'s modeled writes onto `merged` (the post
/// `merge_task` + `keep_task_intent` result) under embedded-clock LWW.
///
/// Semantics (all pinned by tests):
///
/// - **Lane gate**: the `scheduled` key folds only while
///   `!native_scheduled` — while the connector natively owns
///   `startdate`, the document's (stale) key must never write. The
///   other three keys fold unconditionally. The gate is a parameter:
///   this function never names [`crate::capabilities::DeckFeatures`]
///   (the lane decision lives in the sync/pipeline layers).
/// - **Clock rule**: the document's embedded clock `d` wins against the
///   local field clock `l` iff `d >= l` (ties to the document — the
///   fold's input is a *read* result, and an equal-stamp remote write
///   the local side has not adopted yet must land); the winner's clock
///   is `max(l, d)`. `null` is a cleared-field write competing by the
///   same clock.
/// - **Guard copy** (D-S6): a set `intent` flag for a key copies the
///   *local* value **and** local clock over the fold result — a queued
///   `UpdateAppData` op the cycle never saw outranks the document.
/// - **Absence is not deletion**: a key absent from `entry.fields`
///   writes nothing. **No presence facts**: the function returns
///   nothing and touches no index (R3 stays snapshot-only). The card
///   stamp (`remote_seen` / the entity stamp) is never read — the two
///   clock domains stay disjoint.
#[allow(dead_code)] // the fold call site lands in commit 3 (chunk 4's pipeline)
pub(crate) fn fold_task_app_data(
    merged: &mut Task,
    local: &Task,
    entry: &crate::appdata::TaskEntry,
    intent: crate::pipeline::PendingTaskIntent,
    native_scheduled: bool,
) {
    for (key, write) in &entry.fields {
        // The lane gate: native ownership skips the document's key.
        if native_scheduled && key == "scheduled" {
            continue;
        }
        let (guard, doc_wins, d, l) = match key.as_str() {
            "effort" => (
                intent.effort,
                matches!(write.value, crate::appdata::ModeledValue::Effort(_)),
                write.clock,
                local.clocks.effort,
            ),
            "importance" => (
                intent.importance,
                matches!(write.value, crate::appdata::ModeledValue::Importance(_)),
                write.clock,
                local.clocks.importance,
            ),
            "urgency" => (
                intent.urgency,
                matches!(write.value, crate::appdata::ModeledValue::Urgency(_)),
                write.clock,
                local.clocks.urgency,
            ),
            "scheduled" => (
                intent.scheduled,
                matches!(write.value, crate::appdata::ModeledValue::Scheduled(_)),
                write.clock,
                local.clocks.scheduled,
            ),
            // Unmodeled content never folds (the codec quarantined it).
            _ => continue,
        };
        if guard {
            // The guard copies the value AND the embedded clock: the
            // next fold's comparison must see the local write's
            // recency (a failed flush must not let a remote write
            // adopt one cycle late).
            match key.as_str() {
                "effort" => {
                    merged.effort = local.effort;
                    merged.clocks.effort = l;
                }
                "importance" => {
                    merged.importance = local.importance;
                    merged.clocks.importance = l;
                }
                "urgency" => {
                    merged.urgency = local.urgency;
                    merged.clocks.urgency = l;
                }
                "scheduled" => {
                    merged.scheduled = local.scheduled;
                    merged.clocks.scheduled = l;
                }
                _ => unreachable!("matched above"),
            }
            continue;
        }
        if !doc_wins {
            continue;
        }
        if d < l {
            continue; // local stands; `merged` already holds it.
        }
        let clock = std::cmp::max(l, d);
        match &write.value {
            crate::appdata::ModeledValue::Effort(value) => {
                merged.effort = *value;
                merged.clocks.effort = clock;
            }
            crate::appdata::ModeledValue::Importance(value) => {
                merged.importance = *value;
                merged.clocks.importance = clock;
            }
            crate::appdata::ModeledValue::Urgency(value) => {
                merged.urgency = *value;
                merged.clocks.urgency = clock;
            }
            crate::appdata::ModeledValue::Scheduled(value) => {
                merged.scheduled = *value;
                merged.clocks.scheduled = clock;
            }
        }
    }
}

/// The typed deltas one [`fold_reminder_rules`] pass produced: the
/// pipeline turns them into persistence actions and applies the same
/// deltas to the memory view, so batch and view cannot drift (the
/// `apply_push_outcomes` style).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuleFoldChanges {
    /// Rules adopted or updated from the document.
    pub upserts: Vec<crate::reminders::ReminderRule>,
    /// Rules removed by a strictly newer tombstone.
    pub deletes: Vec<crate::ids::ReminderRuleId>,
}

/// The sidecar document fold for one card's reminder rules (D-S3's
/// rules counterpart, chunk 4's D2). Keyed by the rule UUID — the
/// document *is* the rule's remote identity (D-S9):
///
/// - A live document rule with clock `>=` the local rule's
///   `updated_at` updates the local rule (and reports an `Upsert`);
///   an unknown UUID adopts the document's id verbatim.
/// - A tombstone removes the local rule iff strictly newer (a rule
///   re-created after a delete carries a newer clock and stands).
/// - [`crate::appdata::RuleEntry::Verbatim`] content never folds.
/// - **Guard** (`protected`): the whole task's rules fold is skipped
///   and no deltas are reported — exactly D-S6's copy-over collapse
///   (adopt→re-add, update→revert, tombstone→restore all reduce to
///   "nothing happened" when the local set is the protected truth).
///
/// Tombstones never materialize locally: a tombstone for a rule this
/// device never had leaves no trace.
pub fn fold_reminder_rules(
    rules: &mut std::collections::BTreeMap<
        crate::ids::ReminderRuleId,
        crate::reminders::ReminderRule,
    >,
    task: TaskId,
    entry: &crate::appdata::RulesEntry,
    protected: bool,
) -> RuleFoldChanges {
    let mut changes = RuleFoldChanges::default();
    if protected {
        return changes;
    }
    for (key, rule_entry) in &entry.rules {
        match rule_entry {
            crate::appdata::RuleEntry::Rule {
                offset,
                created,
                clock,
            } => {
                let Ok(uuid) = uuid::Uuid::parse_str(key) else {
                    continue; // an unparseable key is not a rule identity
                };
                let id = crate::ids::ReminderRuleId::from(uuid);
                match rules.get_mut(&id) {
                    Some(local) if *clock >= local.updated_at => {
                        local.offset = *offset;
                        local.created_at = *created;
                        local.updated_at = std::cmp::max(local.updated_at, *clock);
                        changes.upserts.push(local.clone());
                    }
                    Some(_) => {} // local stands
                    None => {
                        let adopted = crate::reminders::ReminderRule {
                            id,
                            task,
                            offset: *offset,
                            created_at: *created,
                            updated_at: *clock,
                        };
                        rules.insert(id, adopted.clone());
                        changes.upserts.push(adopted);
                    }
                }
            }
            crate::appdata::RuleEntry::Tombstone { clock } => {
                let Ok(uuid) = uuid::Uuid::parse_str(key) else {
                    continue;
                };
                let id = crate::ids::ReminderRuleId::from(uuid);
                if rules
                    .get(&id)
                    .is_some_and(|local| *clock > local.updated_at)
                {
                    rules.remove(&id);
                    changes.deletes.push(id);
                }
            }
            crate::appdata::RuleEntry::Verbatim(_) => {}
        }
    }
    changes
}

/// R3 — observed remote absence of a bound card: delete-wins fallback.
/// The tombstone's write time is the *observation* time (`observed_at`,
/// i.e. "now" passed in by the engine); the remote binding is cleared.
#[must_use]
pub fn tombstone_task(local: &Task, observed_at: DateTime<Utc>) -> Task {
    tracing::trace!(task_id = ?local.id, observed_at = ?observed_at, "task tombstone entry");
    let mut tombstoned = local.clone();
    tombstoned.deleted = true;
    tombstoned.clocks.deleted = max_ts(local.clocks.deleted, observed_at);
    tombstoned.remote = None;
    tombstoned.remote_seen = None;
    tracing::debug!(
        rule = RULE_R3,
        task_id = ?local.id,
        deleted_clock = ?tombstoned.clocks.deleted,
        "task tombstoned"
    );
    tombstoned
}

/// R9a/R9b — unconditional echo adoption after a create/update push. The
/// server normalizes what we just wrote, so values are taken from the
/// echo as-is (NOT LWW) and clocks advance to
/// `max(local, echo.last_modified)` — a seconds-resolution echo can never
/// be "newer" than the just-made local edit, yet its normalization must
/// land.
#[must_use]
pub fn adopt_task_after_push(
    local: &Task,
    echo: &RemoteTask,
    ctx: &RemoteIndex,
    native_scheduled: bool,
) -> Task {
    let lm = echo.last_modified;
    tracing::trace!(
        task_id = ?local.id,
        card = ?echo.id,
        echo_ts = ?lm,
        title = %echo.title,
        "task echo entry"
    );
    let stack_fallback = resolve_local_stack(ctx, echo.id.board, echo.stack).is_none();
    tracing::debug!(
        rule = RULE_R9,
        task_id = ?local.id,
        stack_fallback,
        "task echo adopted"
    );
    // The echo-path half of D-V2's invariant: while the flag is false,
    // the echo's `startdate` is null *because our own capability-gated
    // write sent null* — adopting it (with its clock advanced) would
    // clobber local `scheduled` on every full-send card push echo.
    // The echo-path clobber hazard: value and clock both keep the
    // local truth while the flag is false.
    let (echo_scheduled, echo_scheduled_clock) = if native_scheduled {
        (echo.scheduled, max_ts(local.clocks.scheduled, lm))
    } else {
        (local.scheduled, local.clocks.scheduled)
    };
    Task {
        id: local.id,
        remote: Some(echo.id),
        title: echo.title.clone(),
        description: echo.description.clone(),
        duedate: echo.duedate,
        scheduled: echo_scheduled,
        done: echo.done,
        stack: resolve_local_stack(ctx, echo.id.board, echo.stack).unwrap_or(local.stack),
        order: echo.order,
        labels: map_remote_labels(ctx, echo.id.board, &echo.labels),
        effort: None,
        importance: None,
        urgency: None,
        archived: echo.archived,
        deleted: false,
        clocks: crate::entities::TaskClocks {
            title: max_ts(local.clocks.title, lm),
            description: max_ts(local.clocks.description, lm),
            duedate: max_ts(local.clocks.duedate, lm),
            scheduled: echo_scheduled_clock,
            done: max_ts(local.clocks.done, lm),
            position: max_ts(local.clocks.position, lm),
            labels: max_ts(local.clocks.labels, lm),
            effort: max_ts(local.clocks.effort, lm),
            importance: max_ts(local.clocks.importance, lm),
            urgency: max_ts(local.clocks.urgency, lm),
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
    tracing::trace!(task_id = ?local.id, "task delete finalize entry");
    let mut finalized = local.clone();
    finalized.deleted = true;
    finalized.remote = None;
    finalized.remote_seen = None;
    tracing::debug!(rule = RULE_R9, task_id = ?local.id, "pushed task delete finalized");
    finalized
}

/// R6 — merge a bound stack against a changed remote entry. The remote
/// `deleted_at` is a timestamped fact and LWWs against the local
/// tombstone clock exactly like the card `deleted` field (R5).
#[must_use]
pub fn merge_stack(local: &Stack, remote: &RemoteStack) -> Stack {
    let ts_r = remote.last_modified;
    tracing::trace!(
        stack_id = ?local.id,
        stack = ?remote.id,
        remote_ts = ?ts_r,
        remote_seen = ?local.remote_seen,
        title = %local.title,
        "stack merge entry"
    );
    let mut merged = local.clone();

    let title_remote_wins = remote_wins(ts_r, local.clocks.title);
    if title_remote_wins {
        merged.title.clone_from(&remote.title);
        merged.clocks.title = ts_r;
    }
    let order_remote_wins = remote_wins(ts_r, local.clocks.order);
    if order_remote_wins {
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
    let (remote_delete_wins, resurrected) = apply_remote_soft_delete(
        &mut merged.deleted,
        &mut merged.clocks.deleted,
        local_latest,
        local.deleted,
        local.clocks.deleted,
        remote.deleted_at,
        ts_r,
    );

    tracing::debug!(
        rule = RULE_R6,
        stack_id = ?local.id,
        remote_ts = ?ts_r,
        title_remote_wins,
        order_remote_wins,
        remote_delete_wins,
        resurrected,
        "stack field verdicts"
    );

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

/// Applies the [`soft_delete_verdict`] outcome to the merged entity's
/// `deleted` flag and clock, and expands it into the two verdict facts
/// the R6 debug events report: `(remote_delete_wins, resurrected)`.
fn apply_remote_soft_delete(
    merged_deleted: &mut bool,
    merged_deleted_clock: &mut DateTime<Utc>,
    local_latest: DateTime<Utc>,
    local_deleted: bool,
    local_deleted_clock: DateTime<Utc>,
    remote_deleted_at: Option<DateTime<Utc>>,
    ts_r: DateTime<Utc>,
) -> (bool, bool) {
    match soft_delete_verdict(
        local_latest,
        local_deleted,
        local_deleted_clock,
        remote_deleted_at,
        ts_r,
    ) {
        Some((deleted, clock)) => {
            *merged_deleted = deleted;
            *merged_deleted_clock = clock;
            (deleted, !deleted)
        }
        None => (false, false),
    }
}

/// R1 for stacks: adopt a remotely-created stack under a fresh local id.
#[must_use]
pub fn adopt_remote_stack(remote: &RemoteStack, id: StackId, board: crate::ids::BoardId) -> Stack {
    tracing::trace!(
        stack_id = ?id,
        stack = ?remote.id,
        board_id = ?board,
        remote_ts = ?remote.last_modified,
        title = %remote.title,
        "stack adopt entry"
    );
    tracing::debug!(
        rule = RULE_R1,
        stack_id = ?id,
        adopted_tombstoned = remote.deleted_at.is_some(),
        "stack adopted"
    );
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
    tracing::trace!(
        stack_id = ?local.id,
        stack = ?echo.id,
        echo_ts = ?lm,
        title = %echo.title,
        "stack echo entry"
    );
    tracing::debug!(rule = RULE_R9, stack_id = ?local.id, "stack echo adopted");
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
    tracing::trace!(
        label_id = ?local.id,
        label = ?remote.id,
        remote_ts = ?ts_r,
        remote_seen = ?local.remote_seen,
        title = %local.title,
        "label merge entry"
    );
    let mut merged = local.clone();

    let title_remote_wins = remote_wins(ts_r, local.clocks.title);
    if title_remote_wins {
        merged.title.clone_from(&remote.title);
        merged.clocks.title = ts_r;
    }
    let color_remote_wins = remote_wins(ts_r, local.clocks.color);
    if color_remote_wins {
        merged.color = crate::entities::Color::new(remote.color.clone());
        merged.clocks.color = ts_r;
    }

    // Same defense as stacks: the latest local activity beats an older
    // remote soft delete.
    let local_latest = max_ts(
        max_ts(local.clocks.title, local.clocks.color),
        local.clocks.deleted,
    );
    let (remote_delete_wins, resurrected) = apply_remote_soft_delete(
        &mut merged.deleted,
        &mut merged.clocks.deleted,
        local_latest,
        local.deleted,
        local.clocks.deleted,
        remote.deleted_at,
        ts_r,
    );

    tracing::debug!(
        rule = RULE_R6,
        label_id = ?local.id,
        remote_ts = ?ts_r,
        title_remote_wins,
        color_remote_wins,
        remote_delete_wins,
        resurrected,
        "label field verdicts"
    );

    merged.remote = Some(remote.id);
    merged.remote_seen = Some(ts_r);
    merged
}

/// R1 for labels: adopt a remotely-created label under a fresh local id.
#[must_use]
pub fn adopt_remote_label(remote: &RemoteLabel, id: LabelId, board: crate::ids::BoardId) -> Label {
    tracing::trace!(
        label_id = ?id,
        label = ?remote.id,
        board_id = ?board,
        remote_ts = ?remote.last_modified,
        title = %remote.title,
        "label adopt entry"
    );
    tracing::debug!(rule = RULE_R1, label_id = ?id, "label adopted");
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
    tracing::trace!(
        label_id = ?local.id,
        label = ?echo.id,
        echo_ts = ?lm,
        title = %echo.title,
        "label echo entry"
    );
    tracing::debug!(rule = RULE_R9, label_id = ?local.id, "label echo adopted");
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

/// R6 for boards, whole-entity: adopt changed remote board content
/// whenever it is strictly newer than `remote_seen`. Every editable
/// field then defends its local write through [`merge_board`]'s
/// per-field verdicts (ADR 0004, 2026-10-18 amendment).
///
/// Callers of board pulls go through [`merge_board`], which wraps this
/// result with the field-wise title defense (`BoardClocks.title`); this
/// function alone is the freshness gate every board field shares.
#[must_use]
pub fn adopt_board_if_newer(local: &Board, remote: &RemoteBoard) -> Board {
    tracing::trace!(
        board_id = ?local.id,
        remote_board = ?remote.id,
        remote_ts = ?remote.last_modified,
        remote_seen = ?local.remote_seen,
        title = %local.title,
        "board freshness-gate entry"
    );
    let fresher = remote_wins(
        remote.last_modified,
        local.remote_seen.unwrap_or_else(utc_min),
    );
    if !fresher {
        tracing::debug!(rule = RULE_R6, board_id = ?local.id, outcome = "kept_not_newer", "board kept, remote not newer");
        return local.clone();
    }
    tracing::debug!(rule = RULE_R6, board_id = ?local.id, outcome = "adopted", "board adopted");
    Board {
        id: local.id,
        // The binding is identity, not content: the merge never changes
        // *which* remote board (and owning connector) a local board is
        // bound to — the snapshot came from that very board.
        remote: local.remote.clone(),
        title: remote.title.clone(),
        color: crate::entities::Color::new(remote.color.clone()),
        archived: remote.archived,
        deleted: remote.deleted_at.is_some(),
        clocks: crate::entities::BoardClocks {
            title: remote.last_modified,
            color: remote.last_modified,
            archived: remote.last_modified,
            deleted: remote.last_modified,
        },
        remote_seen: Some(remote.last_modified),
    }
}

/// R6 for boards, title-wise: the whole-entity freshness gate and every
/// field of [`adopt_board_if_newer`], except the title, which is
/// field-wise by the [`merge_stack`] rule — a local rename keeps its
/// title until a strictly newer remote write lands (never-renamed boards
/// carry the epoch clock, so they behave exactly as before).
#[must_use]
pub fn merge_board(local: &Board, remote: &RemoteBoard) -> Board {
    let ts_r = remote.last_modified;
    tracing::trace!(
        board_id = ?local.id,
        remote_board = ?remote.id,
        remote_ts = ?ts_r,
        title = %local.title,
        "board merge entry"
    );
    let mut merged = adopt_board_if_newer(local, remote);

    // Per-field LWW verdicts (the 2026-10-18 amendment): each editable
    // field keeps its local value unless the remote stamp is strictly
    // newer. Epoch clocks (never locally written) always lose — pre-clock
    // rows keep the remote-wins behavior bit-for-bit.
    let title_wins = remote_wins(ts_r, local.clocks.title);
    if !title_wins {
        merged.title.clone_from(&local.title);
        merged.clocks.title = local.clocks.title;
    }
    let color_wins = remote_wins(ts_r, local.clocks.color);
    if !color_wins {
        merged.color = local.color.clone();
        merged.clocks.color = local.clocks.color;
    }
    let archived_wins = remote_wins(ts_r, local.clocks.archived);
    if !archived_wins {
        merged.archived = local.archived;
        merged.clocks.archived = local.clocks.archived;
    }

    // The tombstone defense: a locally-deleted board is never resurrected
    // by pulled live content (no restore verb exists); a strictly newer
    // remote delete stamp only advances the tombstone's clock.
    if local.deleted {
        let remote_deleted_at = remote.deleted_at.unwrap_or(ts_r);
        merged.deleted = true;
        merged.clocks.deleted = max_ts(local.clocks.deleted, remote_deleted_at);
    }

    tracing::debug!(
        rule = RULE_R6,
        board_id = ?local.id,
        remote_ts = ?ts_r,
        title_wins,
        color_wins,
        archived_wins,
        local_tombstone_stands = local.deleted,
        "board field verdicts"
    );
    merged
}

/// R9a for boards: unconditional echo adoption after a rename push
/// (mirrors [`adopt_stack_after_push`]).
#[must_use]
pub fn adopt_board_after_push(local: &Board, echo: &RemoteBoard) -> Board {
    let lm = echo.last_modified;
    tracing::trace!(
        board_id = ?local.id,
        remote_board = ?echo.id,
        echo_ts = ?lm,
        title = %echo.title,
        "board echo entry"
    );
    tracing::debug!(rule = RULE_R9, board_id = ?local.id, "board echo adopted");
    Board {
        id: local.id,
        // Binding preservation, same rule as the pull merges: an echo is
        // this board's own write coming back.
        remote: local.remote.clone(),
        title: echo.title.clone(),
        color: crate::entities::Color::new(echo.color.clone()),
        archived: echo.archived,
        deleted: false,
        clocks: crate::entities::BoardClocks {
            title: max_ts(local.clocks.title, lm),
            color: max_ts(local.clocks.color, lm),
            archived: max_ts(local.clocks.archived, lm),
            deleted: max_ts(local.clocks.deleted, lm),
        },
        remote_seen: Some(lm),
    }
}

/// R9c for boards: the delete push was accepted — the tombstone stands
/// (a delete echo must never adopt `deleted: false`), the deleted clock
/// advances to `max(local, echo.last_modified)`, and the binding clears.
/// The next pull cannot resurrect (the deleted-clock defense, D7/R1
/// guard). Never reuse [`adopt_board_after_push`] for a delete.
#[must_use]
pub fn adopt_board_after_delete_push(local: &Board, echo: &RemoteBoard) -> Board {
    let lm = echo.last_modified;
    tracing::trace!(
        board_id = ?local.id,
        remote_board = ?echo.id,
        echo_ts = ?lm,
        "board delete echo entry"
    );
    tracing::debug!(rule = RULE_R9, board_id = ?local.id, "board delete echo adopted");
    Board {
        id: local.id,
        remote: None,
        title: echo.title.clone(),
        color: crate::entities::Color::new(echo.color.clone()),
        archived: echo.archived,
        deleted: true,
        clocks: crate::entities::BoardClocks {
            title: max_ts(local.clocks.title, lm),
            color: max_ts(local.clocks.color, lm),
            archived: max_ts(local.clocks.archived, lm),
            deleted: max_ts(local.clocks.deleted, lm),
        },
        remote_seen: Some(lm),
    }
}

/// R1 for boards: adopt a remotely-created board under a fresh local id.
#[must_use]
pub fn adopt_remote_board(
    remote: &RemoteBoard,
    connector: crate::ids::ConnectorId,
    id: crate::ids::BoardId,
) -> Board {
    tracing::trace!(
        board_id = ?id,
        connector = %connector,
        remote_board = ?remote.id,
        remote_ts = ?remote.last_modified,
        title = %remote.title,
        "board adopt entry"
    );
    tracing::debug!(rule = RULE_R1, board_id = ?id, "board adopted");
    Board {
        id,
        remote: Some(crate::ids::RemoteBoardBinding {
            connector,
            board: remote.id,
        }),
        title: remote.title.clone(),
        color: crate::entities::Color::new(remote.color.clone()),
        archived: remote.archived,
        deleted: remote.deleted_at.is_some(),
        clocks: crate::entities::BoardClocks {
            title: remote.last_modified,
            color: remote.last_modified,
            archived: remote.last_modified,
            deleted: remote.last_modified,
        },
        remote_seen: Some(remote.last_modified),
    }
}

/// R3/R6 for boards — observed remote absence of a bound board (or a
/// local rename push that must be finalized): tombstone it.
#[must_use]
pub fn tombstone_board(local: &Board, _observed_at: DateTime<Utc>) -> Board {
    tracing::trace!(board_id = ?local.id, "board tombstone entry");
    let mut b = local.clone();
    b.deleted = true;
    b.remote = None;
    b.remote_seen = None;
    tracing::debug!(rule = RULE_R3, board_id = ?local.id, "board tombstoned");
    b
}

/// R3/R6 for stacks — observed remote absence of a bound stack (or a
/// local delete push that must be finalized): tombstone it. The
/// tombstone's write time never moves backwards; the remote binding is
/// cleared.
#[must_use]
pub fn tombstone_stack(local: &Stack, observed_at: DateTime<Utc>) -> Stack {
    tracing::trace!(stack_id = ?local.id, observed_at = ?observed_at, "stack tombstone entry");
    let mut s = local.clone();
    s.deleted = true;
    s.clocks.deleted = max_ts(local.clocks.deleted, observed_at);
    s.remote = None;
    s.remote_seen = None;
    tracing::debug!(
        rule = RULE_R3,
        stack_id = ?local.id,
        deleted_clock = ?s.clocks.deleted,
        "stack tombstoned"
    );
    s
}

/// R3/R6 for labels — observed remote absence of a bound label (or a
/// local delete push that must be finalized): tombstone it. The
/// tombstone's write time never moves backwards; the remote binding is
/// cleared.
#[must_use]
pub fn tombstone_label(local: &Label, observed_at: DateTime<Utc>) -> Label {
    tracing::trace!(label_id = ?local.id, observed_at = ?observed_at, "label tombstone entry");
    let mut l = local.clone();
    l.deleted = true;
    l.clocks.deleted = max_ts(local.clocks.deleted, observed_at);
    l.remote = None;
    l.remote_seen = None;
    tracing::debug!(
        rule = RULE_R3,
        label_id = ?local.id,
        deleted_clock = ?l.clocks.deleted,
        "label tombstoned"
    );
    l
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge_testutil::*;
    use crate::test_support;
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    fn ts_millis(secs: i64) -> i64 {
        secs
    }

    /// A local board row in `merge_testutil`'s fixture shape.
    fn local_board() -> Board {
        crate::merge_testutil::base_board()
    }

    #[test]
    fn every_board_merge_path_preserves_a_non_default_binding_tag() {
        // The binding is identity: a board tagged for a connector that is
        // not the default keeps that tag through every merge path — none
        // of them re-mints (D-8's preserve rule).
        let tag = crate::ids::ConnectorId::from("work");
        let binding = crate::ids::RemoteBoardBinding {
            connector: tag,
            board: board(REMOTE_BOARD_NUM),
        };
        let mut local = local_board();
        local.remote = Some(binding.clone());

        let remote = remote_board(REMOTE_BOARD_NUM, BASE + 120);
        assert_eq!(
            adopt_board_if_newer(&local, &remote).remote,
            Some(binding.clone())
        );
        assert_eq!(merge_board(&local, &remote).remote, Some(binding.clone()));
        assert_eq!(
            adopt_board_after_push(&local, &remote).remote,
            Some(binding.clone())
        );
        // The delete push is the one path that *clears* (finalize): its
        // clearing is asserted by its own test above.
    }

    #[test]
    fn scheduled_merges_per_field_lww_both_directions() {
        // D-F2 branch A: `scheduled` rides the native `startdate` slot,
        // so its verdict is duedate's twin — remote strictly newer
        // adopts, local strictly newer stands (entity-stamp basis).
        let ctx = RemoteIndex::default();

        // Remote newer ⇒ adopt remote value, stamp ts_r.
        let mut local = bound_task(1);
        local.scheduled = Some(ts(BASE - 120));
        local.clocks.scheduled = ts(BASE - 120);
        let mut remote = remote_task(1, 1, BASE + 300);
        remote.scheduled = Some(ts(BASE + 240));
        let merged = merge_task(&local, &remote, &ctx, true);
        assert_eq!(merged.scheduled, Some(ts(BASE + 240)));
        assert_eq!(merged.clocks.scheduled, ts(BASE + 300));

        // Local newer ⇒ local stands.
        let mut local = bound_task(1);
        local.scheduled = Some(ts(BASE + 400));
        local.clocks.scheduled = ts(BASE + 400);
        let mut remote = remote_task(1, 1, BASE + 300);
        remote.scheduled = Some(ts(BASE + 240));
        let merged = merge_task(&local, &remote, &ctx, true);
        assert_eq!(merged.scheduled, Some(ts(BASE + 400)));
        assert_eq!(merged.clocks.scheduled, ts(BASE + 400));
    }

    #[test]
    fn adopt_remote_task_carries_scheduled_with_the_entity_stamp() {
        let mut remote = remote_task(1, 1, BASE + 300);
        remote.scheduled = Some(ts(BASE + 60));
        let mut ctx = RemoteIndex::default();
        ctx.stack_by_ref
            .insert(stack_ref(1), StackId::from(uuid::Uuid::from_u128(701)));
        let id = TaskId::from(uuid::Uuid::from_u128(77));
        let adopted = adopt_remote_task(&remote, id, &ctx).expect("stack resolves");
        assert_eq!(adopted.scheduled, Some(ts(BASE + 60)));
        assert_eq!(adopted.clocks.scheduled, ts(BASE + 300));
    }

    #[test]
    fn adopt_task_after_push_takes_the_echo_value_with_the_max_clock() {
        let ctx = RemoteIndex::default();
        let mut local = bound_task(1);
        local.scheduled = Some(ts(BASE - 120));
        local.clocks.scheduled = ts(BASE - 120);
        let mut echo = remote_task(1, 1, BASE + 300);
        echo.scheduled = Some(ts(BASE - 120)); // server normalization echo
        let adopted = adopt_task_after_push(&local, &echo, &ctx, true);
        assert_eq!(adopted.scheduled, Some(ts(BASE - 120)));
        assert_eq!(
            adopted.clocks.scheduled,
            ts(BASE + 300),
            "clock advances to max(local, echo stamp)"
        );
    }

    #[test]
    fn disjoint_field_edits_both_survive() {
        let mut local = bound_task(1);
        local.title = "edited locally".into();
        local.clocks.title = ts(BASE + 330); // 10:05:30
        let mut remote = remote_task(1, 1, ts_millis(BASE + 180)); // 10:03:00
        remote.description = "edited remotely".into();

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx, true);

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
        let merged = merge_task(&local, &remote, &ctx, true);

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
        let merged = merge_task(&local, &remote, &ctx, true);

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
        let merged = merge_task(&local, &remote, &ctx, true);

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
        let merged = merge_task(&local, &remote, &ctx, true);

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
    fn merge_board_adopts_a_remote_newer_title() {
        let mut local = local_board();
        local.clocks.title = ts(BASE + 60); // a local rename
        let remote = remote_board(REMOTE_BOARD_NUM, BASE + 120);
        let merged = merge_board(&local, &remote);
        assert_eq!(merged.title, remote.title.clone());
        assert_eq!(merged.clocks.title, ts(BASE + 120));
    }

    #[test]
    fn merge_board_keeps_a_local_newer_title() {
        let mut local = local_board();
        local.title = "locally renamed".into();
        local.clocks.title = ts(BASE + 600);
        let remote = remote_board(REMOTE_BOARD_NUM, BASE + 120);
        let merged = merge_board(&local, &remote);
        assert_eq!(merged.title, "locally renamed");
        assert_eq!(merged.clocks.title, ts(BASE + 600));
        // The freshness gate still advances the observation stamp.
        assert_eq!(merged.remote_seen, Some(ts(BASE + 120)));
    }

    #[test]
    fn merge_board_epoch_local_title_always_loses() {
        // A never-locally-renamed board carries the epoch clock and
        // behaves exactly like the pre-clock remote-wins adoption.
        let mut local = local_board();
        local.clocks.title = crate::entities::BoardClocks::epoch();
        assert_eq!(local.clocks.title, crate::entities::BoardClocks::epoch());
        let mut remote = remote_board(REMOTE_BOARD_NUM, BASE + 120);
        remote.title = "remote title".into();
        let merged = merge_board(&local, &remote);
        assert_eq!(merged.title, "remote title");
        assert_eq!(merged.clocks.title, ts(BASE + 120));
    }

    #[test]
    fn adopt_board_after_push_takes_the_echo_title_and_stamp() {
        let mut local = local_board();
        local.title = "renamed".into();
        local.clocks.title = ts(BASE + 60);
        let echo = remote_board(REMOTE_BOARD_NUM, BASE + 120);
        let merged = adopt_board_after_push(&local, &echo);
        assert_eq!(merged.title, echo.title.clone());
        assert_eq!(merged.clocks.title, ts(BASE + 120));
        assert_eq!(merged.remote_seen, Some(ts(BASE + 120)));
        assert!(merged.remote.is_some());
        assert!(!merged.deleted);
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
        let adopted = adopt_remote_board(
            &remote_board(99, BASE + 5),
            crate::test_support::test_connector(),
            id,
        );
        assert_eq!(adopted.id, id);
        assert_eq!(
            adopted.remote,
            crate::test_support::test_binding(board(99)),
            "the adopt-new mint carries the passed connector tag"
        );
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

        let merged = merge_task(&local, &remote, &ctx, true);
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
        let kept = merge_task(&local, &remote, &unmapped, true);
        assert_eq!(kept.stack, local.stack);
        assert_eq!(kept.order, 42, "unmapped stack keeps the whole position");
    }

    #[test]
    fn merge_board_field_verdicts_are_independent() {
        // A newer remote write on the title must not lose a newer local
        // color/archived edit, and vice versa (the 2026-10-18 amendment:
        // boards complete the per-field merge).
        let mut local = local_board();
        local.color = crate::entities::Color::new("123456");
        local.clocks.color = ts(BASE + 480); // local color edit at 10:08
        local.archived = true;
        local.clocks.archived = ts(BASE + 490);
        // The title clock is older than the remote stamp below.
        local.clocks.title = ts(BASE + 60);
        let mut remote = remote_board(REMOTE_BOARD_NUM, BASE + 420); // 10:07
        remote.title = "remote title".into();

        let merged = merge_board(&local, &remote);
        assert_eq!(merged.title, "remote title", "older local title loses");
        assert_eq!(merged.clocks.title, ts(BASE + 420));
        assert_eq!(merged.color.as_str(), "123456", "newer local color stands");
        assert_eq!(merged.clocks.color, ts(BASE + 480));
        assert!(merged.archived, "newer local archived stands");
        assert_eq!(merged.clocks.archived, ts(BASE + 490));
    }

    #[test]
    fn merge_board_epoch_clocks_lose_to_any_remote_stamp() {
        let local = local_board(); // every clock at BASE
        let mut remote = remote_board(REMOTE_BOARD_NUM, BASE + 1);
        remote.color = "00ff00".into();
        remote.archived = true;
        let merged = merge_board(&local, &remote);
        assert_eq!(merged.color.as_str(), "00ff00");
        assert!(merged.archived);
        assert_eq!(merged.clocks.color, ts(BASE + 1));
    }

    #[test]
    fn merge_board_tombstone_stands_against_a_pulled_live_board() {
        // The D7 resurrection guard: a locally-deleted board is never
        // resurrected by pulled live content (no restore verb exists).
        let mut local = local_board();
        local.deleted = true;
        local.clocks.deleted = ts(BASE + 300);
        let remote = remote_board(REMOTE_BOARD_NUM, BASE + 360); // strictly newer live pull
        let merged = merge_board(&local, &remote);
        assert!(merged.deleted, "the tombstone stands");
        assert_eq!(merged.clocks.deleted, ts(BASE + 360), "the stamp advances");

        let older = remote_board(REMOTE_BOARD_NUM, BASE + 120);
        let kept = merge_board(&local, &older);
        assert!(kept.deleted);
        assert_eq!(
            kept.clocks.deleted,
            ts(BASE + 300),
            "an older remote write cannot move the tombstone"
        );
    }

    #[test]
    fn adopt_board_after_push_advances_all_four_clocks() {
        let mut local = local_board();
        local.clocks = crate::entities::BoardClocks {
            title: ts(BASE + 60),
            color: ts(BASE + 60),
            archived: ts(BASE + 60),
            deleted: ts(BASE + 60),
        };
        let echo = remote_board(REMOTE_BOARD_NUM, BASE + 120);
        let merged = adopt_board_after_push(&local, &echo);
        assert_eq!(merged.clocks.title, ts(BASE + 120));
        assert_eq!(merged.clocks.color, ts(BASE + 120));
        assert_eq!(merged.clocks.archived, ts(BASE + 120));
        assert_eq!(merged.clocks.deleted, ts(BASE + 120));
    }

    #[test]
    fn adopt_board_after_delete_push_keeps_the_tombstone_and_clears_the_binding() {
        let mut local = local_board();
        local.deleted = true;
        local.clocks.deleted = ts(BASE + 60);
        local.remote = crate::test_support::test_binding(board(REMOTE_BOARD_NUM));
        let echo = remote_board(REMOTE_BOARD_NUM, BASE + 120);
        let finalized = adopt_board_after_delete_push(&local, &echo);
        assert!(finalized.deleted, "a delete echo must never un-delete");
        assert_eq!(finalized.clocks.deleted, ts(BASE + 120));
        assert!(
            finalized.remote.is_none(),
            "the binding clears on confirmation"
        );
        assert_eq!(finalized.remote_seen, Some(ts(BASE + 120)));
    }

    // ---- chunk 4: the native-scheduled gates (D5 flag-arm matrix) ------

    #[test]
    fn merge_task_scheduled_verdict_runs_only_while_native() {
        // The `true` arm is the pre-chunk pin: the remote stamp adopts.
        let mut local = bound_task(1);
        local.clocks.scheduled = ts(BASE - 120);
        let mut remote = remote_task(1, 1, BASE + 300);
        remote.scheduled = Some(ts(BASE + 240));
        let merged = merge_task(&local, &remote, &ctx_of(), true);
        assert_eq!(merged.scheduled, Some(ts(BASE + 240)));

        // The `false` arm: the always-absent remote value must never
        // clobber local state via "remote wins, value null".
        let mut local = bound_task(1);
        local.scheduled = Some(ts(BASE + 100));
        local.clocks.scheduled = ts(BASE + 100);
        let remote = remote_task(1, 1, BASE + 300); // scheduled: None, stamp newer
        let merged = merge_task(&local, &remote, &ctx_of(), false);
        assert_eq!(merged.scheduled, Some(ts(BASE + 100)));
        assert_eq!(merged.clocks.scheduled, ts(BASE + 100));
    }

    #[test]
    fn adopt_task_after_push_keeps_local_scheduled_while_not_native() {
        let ctx = RemoteIndex::default();
        let mut local = bound_task(1);
        local.scheduled = Some(ts(BASE + 100));
        local.clocks.scheduled = ts(BASE + 100);
        // Our capability-gated write sent null: the echo carries None
        // with a newer stamp — adopting it would clobber the local value.
        let mut echo = remote_task(1, 1, BASE + 300);
        echo.scheduled = None;
        let adopted = adopt_task_after_push(&local, &echo, &ctx, false);
        assert_eq!(adopted.scheduled, Some(ts(BASE + 100)));
        assert_eq!(
            adopted.clocks.scheduled,
            ts(BASE + 100),
            "the clock keeps the local truth too (a later fold must see it)"
        );
        // The `true` arm: the server's normalization lands (existing pin's shape).
        let mut echo = remote_task(1, 1, BASE + 300);
        echo.scheduled = Some(ts(BASE - 120));
        let adopted = adopt_task_after_push(&local, &echo, &ctx, true);
        assert_eq!(adopted.scheduled, Some(ts(BASE - 120)));
        assert_eq!(adopted.clocks.scheduled, ts(BASE + 300));
    }

    fn ctx_of() -> RemoteIndex {
        RemoteIndex::default()
    }

    // ---- chunk 4: the sidecar fold primitives -------------------------

    use crate::appdata::{ModeledValue, RuleEntry, RulesEntry, TaskEntry, TaskFieldWrite};
    use crate::pipeline::PendingTaskIntent;

    fn fw(value: ModeledValue, clock_secs: i64) -> TaskFieldWrite {
        TaskFieldWrite {
            value,
            clock: ts(clock_secs),
        }
    }

    fn rules_entry(entries: &[(&str, RuleEntry)]) -> RulesEntry {
        RulesEntry {
            rules: entries
                .iter()
                .map(|(k, e)| ((*k).to_owned(), e.clone()))
                .collect(),
            unknown: std::collections::BTreeMap::new(),
        }
    }

    fn rule_with_clock(updated_secs: i64) -> crate::reminders::ReminderRule {
        crate::reminders::ReminderRule {
            id: crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(10)),
            task: TaskId::from(uuid::Uuid::from_u128(1)),
            offset: crate::reminders::ReminderOffset::at_due(),
            created_at: ts(BASE + 100),
            updated_at: ts(updated_secs),
        }
    }

    #[test]
    fn fold_adopts_newer_document_values_with_the_embedded_clock() {
        let mut merged = bound_task(1);
        let local = bound_task(1);
        let entry = TaskEntry {
            fields: [
                (
                    "effort".to_owned(),
                    fw(
                        ModeledValue::Effort(Some(crate::entities::Effort::new(900))),
                        BASE + 500,
                    ),
                ),
                (
                    "importance".to_owned(),
                    fw(
                        ModeledValue::Importance(Some(crate::entities::Importance::High)),
                        BASE + 500,
                    ),
                ),
            ]
            .into(),
            unmodeled: std::collections::BTreeMap::new(),
        };
        fold_task_app_data(
            &mut merged,
            &local,
            &entry,
            PendingTaskIntent::default(),
            false,
        );
        assert_eq!(merged.effort, Some(crate::entities::Effort::new(900)));
        assert_eq!(merged.clocks.effort, ts(BASE + 500));
        assert_eq!(merged.importance, Some(crate::entities::Importance::High));
        assert_eq!(merged.clocks.importance, ts(BASE + 500));
        // Untouched native fields stay put.
        assert_eq!(merged.title, local.title);
    }

    #[test]
    fn fold_keeps_newer_local_values_and_ties_lose_to_the_document() {
        let mut local = bound_task(1);
        local.urgency = Some(crate::entities::Urgency::High);
        local.clocks.urgency = ts(BASE + 600);
        let mut merged = local.clone();
        // Document older than local: local stands.
        let entry = TaskEntry {
            fields: [(
                "urgency".to_owned(),
                fw(
                    ModeledValue::Urgency(Some(crate::entities::Urgency::Low)),
                    BASE + 500,
                ),
            )]
            .into(),
            unmodeled: std::collections::BTreeMap::new(),
        };
        fold_task_app_data(
            &mut merged,
            &local,
            &entry,
            PendingTaskIntent::default(),
            false,
        );
        assert_eq!(merged.urgency, Some(crate::entities::Urgency::High));
        assert_eq!(merged.clocks.urgency, ts(BASE + 600));

        // Tie: the document wins (the fold's input is a read result).
        let entry_tie = TaskEntry {
            fields: [(
                "urgency".to_owned(),
                fw(
                    ModeledValue::Urgency(Some(crate::entities::Urgency::Low)),
                    BASE + 600,
                ),
            )]
            .into(),
            unmodeled: std::collections::BTreeMap::new(),
        };
        fold_task_app_data(
            &mut merged,
            &local,
            &entry_tie,
            PendingTaskIntent::default(),
            false,
        );
        assert_eq!(merged.urgency, Some(crate::entities::Urgency::Low));
        assert_eq!(merged.clocks.urgency, ts(BASE + 600));
    }

    #[test]
    fn fold_treats_a_cleared_field_as_an_ordinary_write() {
        let mut merged = bound_task(1);
        let mut local = bound_task(1);
        local.effort = Some(crate::entities::Effort::new(5));
        local.clocks.effort = ts(BASE + 100);
        let entry = TaskEntry {
            fields: [(
                "effort".to_owned(),
                fw(ModeledValue::Effort(None), BASE + 500),
            )]
            .into(),
            unmodeled: std::collections::BTreeMap::new(),
        };
        fold_task_app_data(
            &mut merged,
            &local,
            &entry,
            PendingTaskIntent::default(),
            false,
        );
        assert_eq!(merged.effort, None, "null is a cleared-field write");
        assert_eq!(merged.clocks.effort, ts(BASE + 500));

        // An older clear never deletes a newer local value.
        let older = TaskEntry {
            fields: [(
                "effort".to_owned(),
                fw(ModeledValue::Effort(None), BASE + 50),
            )]
            .into(),
            unmodeled: std::collections::BTreeMap::new(),
        };
        fold_task_app_data(
            &mut merged,
            &local,
            &older,
            PendingTaskIntent::default(),
            false,
        );
        assert_eq!(merged.effort, None, "already cleared");
    }

    #[test]
    fn fold_skips_the_scheduled_key_while_the_connector_owns_it() {
        let mut local = bound_task(1);
        local.scheduled = Some(ts(BASE + 100));
        local.clocks.scheduled = ts(BASE + 100);
        let mut merged = local.clone();
        let entry = TaskEntry {
            fields: [(
                "scheduled".to_owned(),
                fw(ModeledValue::Scheduled(Some(ts(BASE + 900))), BASE + 500),
            )]
            .into(),
            unmodeled: std::collections::BTreeMap::new(),
        };
        fold_task_app_data(
            &mut merged,
            &local,
            &entry,
            PendingTaskIntent::default(),
            true,
        );
        assert_eq!(
            merged.scheduled,
            Some(ts(BASE + 100)),
            "native ownership blocks the fold"
        );
        assert_eq!(merged.clocks.scheduled, ts(BASE + 100));
        // While the flag is false, the same document folds.
        fold_task_app_data(
            &mut merged,
            &local,
            &entry,
            PendingTaskIntent::default(),
            false,
        );
        assert_eq!(merged.scheduled, Some(ts(BASE + 900)));
    }

    #[test]
    fn fold_guard_copies_the_value_and_the_embedded_clock() {
        let mut local = bound_task(1);
        local.effort = Some(crate::entities::Effort::new(7));
        local.clocks.effort = ts(BASE + 400);
        let mut merged = local.clone();
        let entry = TaskEntry {
            fields: [(
                "effort".to_owned(),
                fw(
                    ModeledValue::Effort(Some(crate::entities::Effort::new(900))),
                    BASE + 500,
                ),
            )]
            .into(),
            unmodeled: std::collections::BTreeMap::new(),
        };
        let intent = PendingTaskIntent {
            effort: true,
            ..PendingTaskIntent::default()
        };
        fold_task_app_data(&mut merged, &local, &entry, intent, false);
        assert_eq!(merged.effort, Some(crate::entities::Effort::new(7)));
        assert_eq!(merged.clocks.effort, ts(BASE + 400), "the clock copies too");
    }

    #[test]
    fn fold_absence_writes_nothing_and_the_card_stamp_is_never_read() {
        let mut merged = bound_task(1);
        let mut local = bound_task(1);
        local.effort = Some(crate::entities::Effort::new(3));
        local.clocks.effort = ts(BASE + 300);
        local.remote_seen = Some(ts(9_000));
        // An entry with no modeled keys: nothing folds.
        let entry = TaskEntry::default();
        let before = merged.clone();
        fold_task_app_data(
            &mut merged,
            &local,
            &entry,
            PendingTaskIntent::default(),
            false,
        );
        assert_eq!(merged, before, "absence is not deletion; nothing writes");
    }

    #[test]
    fn fold_guarded_scheduled_copies_local_over_a_newer_document_write() {
        // The scheduled lane guard: while a post-read UpdateAppData op
        // is queued, the fold result takes the local value + clock even
        // though the document's clock is newer.
        let mut local = bound_task(1);
        local.scheduled = Some(ts(BASE + 800));
        local.clocks.scheduled = ts(BASE + 800);
        let mut merged = local.clone();
        let entry = TaskEntry {
            fields: [(
                "scheduled".to_owned(),
                fw(ModeledValue::Scheduled(Some(ts(BASE + 100))), BASE + 900),
            )]
            .into(),
            unmodeled: std::collections::BTreeMap::new(),
        };
        let intent = PendingTaskIntent {
            scheduled: true,
            ..PendingTaskIntent::default()
        };
        fold_task_app_data(&mut merged, &local, &entry, intent, false);
        assert_eq!(merged.scheduled, Some(ts(BASE + 800)));
        assert_eq!(merged.clocks.scheduled, ts(BASE + 800));
    }

    #[test]
    fn rules_fold_adopts_and_updates_by_the_embedded_clock() {
        let mut rules = std::collections::BTreeMap::new();
        let existing = rule_with_clock(BASE + 100);
        rules.insert(existing.id, existing.clone());
        let new_id = crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(11));
        let entry = rules_entry(&[
            (
                "00000000-0000-0000-0000-00000000000a",
                RuleEntry::Rule {
                    offset: crate::reminders::ReminderOffset::before(
                        chrono::TimeDelta::try_minutes(-30).unwrap(),
                    )
                    .unwrap(),
                    created: ts(BASE + 100),
                    clock: ts(BASE + 500),
                },
            ),
            (
                "00000000-0000-0000-0000-00000000000b",
                RuleEntry::Rule {
                    offset: crate::reminders::ReminderOffset::at_due(),
                    created: ts(BASE + 400),
                    clock: ts(BASE + 400),
                },
            ),
        ]);
        let task = TaskId::from(uuid::Uuid::from_u128(1));
        let changes = fold_reminder_rules(&mut rules, task, &entry, false);
        assert_eq!(changes.upserts.len(), 2);
        assert_eq!(changes.deletes, Vec::new());
        // Update: the offset and the clock landed.
        let updated = &rules[&existing.id];
        assert_eq!(
            updated.offset,
            crate::reminders::ReminderOffset::before(chrono::TimeDelta::try_minutes(-30).unwrap())
                .unwrap()
        );
        assert_eq!(updated.updated_at, ts(BASE + 500));
        // Adopt: the document's UUID verbatim.
        assert!(rules.contains_key(&new_id));
        assert_eq!(rules[&new_id].task, task);
        assert_eq!(rules[&new_id].updated_at, ts(BASE + 400));
    }

    #[test]
    fn rules_fold_lets_a_newer_local_rule_stand() {
        let mut rules = std::collections::BTreeMap::new();
        let existing = rule_with_clock(BASE + 600);
        rules.insert(existing.id, existing.clone());
        let entry = rules_entry(&[(
            "00000000-0000-0000-0000-00000000000a",
            RuleEntry::Rule {
                offset: crate::reminders::ReminderOffset::at_due(),
                created: ts(BASE + 100),
                clock: ts(BASE + 500),
            },
        )]);
        let changes = fold_reminder_rules(
            &mut rules,
            TaskId::from(uuid::Uuid::from_u128(1)),
            &entry,
            false,
        );
        assert_eq!(changes.upserts, Vec::new());
        assert_eq!(rules[&existing.id].updated_at, ts(BASE + 600));
    }

    #[test]
    fn rules_fold_tombstones_only_strictly_newer() {
        let mut rules = std::collections::BTreeMap::new();
        let existing = rule_with_clock(BASE + 500);
        rules.insert(existing.id, existing.clone());
        // Equal clock: the tombstone does not win (re-create after delete).
        let entry = rules_entry(&[(
            "00000000-0000-0000-0000-00000000000a",
            RuleEntry::Tombstone {
                clock: ts(BASE + 500),
            },
        )]);
        let changes = fold_reminder_rules(
            &mut rules,
            TaskId::from(uuid::Uuid::from_u128(1)),
            &entry,
            false,
        );
        assert_eq!(changes.deletes, Vec::new());
        assert!(rules.contains_key(&existing.id));
        // Strictly newer: gone.
        let newer = rules_entry(&[(
            "00000000-0000-0000-0000-00000000000a",
            RuleEntry::Tombstone {
                clock: ts(BASE + 501),
            },
        )]);
        let changes = fold_reminder_rules(
            &mut rules,
            TaskId::from(uuid::Uuid::from_u128(1)),
            &newer,
            false,
        );
        assert_eq!(changes.deletes, vec![existing.id]);
        assert!(!rules.contains_key(&existing.id));
        // A tombstone for a rule this device never had leaves no trace.
        let stranger = rules_entry(&[(
            "00000000-0000-0000-0000-00000000000c",
            RuleEntry::Tombstone {
                clock: ts(BASE + 999),
            },
        )]);
        let changes = fold_reminder_rules(
            &mut rules,
            TaskId::from(uuid::Uuid::from_u128(1)),
            &stranger,
            false,
        );
        assert_eq!(changes.deletes, Vec::new());
        assert_eq!(rules.len(), 0);
    }

    #[test]
    fn rules_fold_skips_verbatim_and_unparseable_keys() {
        let mut rules = std::collections::BTreeMap::new();
        let entry = rules_entry(&[
            (
                "not-a-uuid",
                RuleEntry::Tombstone {
                    clock: ts(BASE + 999),
                },
            ),
            (
                "not-a-uuid-either",
                RuleEntry::Rule {
                    offset: crate::reminders::ReminderOffset::at_due(),
                    created: ts(BASE + 1),
                    clock: ts(BASE + 2),
                },
            ),
            (
                "00000000-0000-0000-0000-00000000000d",
                RuleEntry::Verbatim(serde_json::json!({ "future": true })),
            ),
        ]);
        let changes = fold_reminder_rules(
            &mut rules,
            TaskId::from(uuid::Uuid::from_u128(1)),
            &entry,
            false,
        );
        assert_eq!(changes, crate::merge::RuleFoldChanges::default());
    }

    #[test]
    fn rules_fold_guarded_skip_equals_copy_over() {
        // D-S6's collapse: for an adopt+update+tombstone triple, the
        // protected skip must leave exactly the copy-over end state.
        let local_a = rule_with_clock(100); // update target
        let mut rules = std::collections::BTreeMap::new();
        rules.insert(local_a.id, local_a.clone());
        let entry = rules_entry(&[
            (
                "00000000-0000-0000-0000-00000000000a",
                RuleEntry::Rule {
                    offset: crate::reminders::ReminderOffset::at_due(),
                    created: ts(BASE + 100),
                    clock: ts(BASE + 900),
                },
            ),
            (
                "00000000-0000-0000-0000-00000000000b",
                RuleEntry::Rule {
                    offset: crate::reminders::ReminderOffset::at_due(),
                    created: ts(BASE + 1),
                    clock: ts(BASE + 1),
                },
            ),
            (
                "00000000-0000-0000-0000-00000000000a-x",
                RuleEntry::Tombstone {
                    clock: ts(BASE + 900),
                },
            ),
        ]);
        let mut skipped = rules.clone();
        let changes = fold_reminder_rules(
            &mut skipped,
            TaskId::from(uuid::Uuid::from_u128(1)),
            &entry,
            true,
        );
        assert_eq!(changes, crate::merge::RuleFoldChanges::default());
        assert_eq!(skipped, rules, "skip == nothing happened");
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
            let merged = merge_task(&local, &remote, &ctx, true);

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
            let merged = merge_task(&local, &remote, &ctx, true);
            // Total: no panic. Tie-break: strict `>` — equal timestamps keep
            // the local value (this kills `>=` mutants).
            let expected = if remote_ts > local_title_clock {
                remote_title
            } else {
                local_title
            };
            prop_assert_eq!(merged.title.clone(), expected);

            // Idempotence: merging again with the same remote changes nothing.
            let again = merge_task(&merged, &remote, &ctx, true);
            prop_assert_eq!(again, merged);
        }

        #[test]
        fn merge_clocks_are_total_monotone_and_idempotent_at_extremes(
            local_secs in test_support::adversarial_timestamp_seconds(),
            remote_secs in test_support::adversarial_timestamp_seconds(),
            card in 1_u64..100,
        ) {
            // #303: the bounded props above never approach chrono's
            // representable bounds; the same three properties re-asserted
            // where plain `+` would panic.
            let mut local = bound_task(card);
            local.title = "local title".into();
            local.clocks = clocks_at(local_secs);
            let mut remote = remote_task(1, card, remote_secs);
            remote.title = "remote title".into();

            let ctx = RemoteIndex::default();
            let merged = merge_task(&local, &remote, &ctx, true);

            prop_assert!(merged.clocks.title >= local.clocks.title);
            prop_assert!(merged.clocks.description >= local.clocks.description);
            prop_assert!(merged.clocks.duedate >= local.clocks.duedate);
            prop_assert!(merged.clocks.done >= local.clocks.done);
            prop_assert!(merged.clocks.position >= local.clocks.position);
            prop_assert!(merged.clocks.labels >= local.clocks.labels);
            prop_assert!(merged.clocks.archived >= local.clocks.archived);
            prop_assert!(merged.clocks.deleted >= local.clocks.deleted);

            let expected_title = if remote_secs > local_secs {
                "remote title"
            } else {
                "local title"
            };
            prop_assert_eq!(merged.title.clone(), expected_title, "strict-newer-wins tie-break");

            let again = merge_task(&merged, &remote, &ctx, true);
            prop_assert_eq!(again, merged, "idempotent at the extremes");
        }

        #[test]
        fn fold_never_decreases_app_clocks_and_the_guard_dominates(
            doc_secs in 0_i64..10_000,
            local_secs in 0_i64..10_000,
            guarded in proptest::bool::ANY,
            effort in proptest::option::of(0_u64..1_000_000),
        ) {
            let mut local = bound_task(1);
            local.clocks.effort = ts(local_secs);
            let entry = TaskEntry {
                fields: [(
                    "effort".to_owned(),
                    fw(ModeledValue::Effort(effort.map(crate::entities::Effort::new)), doc_secs),
                )]
                .into(),
                unmodeled: std::collections::BTreeMap::new(),
            };
            let intent = PendingTaskIntent {
                effort: guarded,
                ..PendingTaskIntent::default()
            };
            let mut merged = local.clone();
            fold_task_app_data(&mut merged, &local, &entry, intent, false);
            prop_assert!(merged.clocks.effort >= local.clocks.effort);
            if guarded || doc_secs < local_secs {
                prop_assert_eq!(merged.effort, local.effort);
                prop_assert_eq!(merged.clocks.effort, local.clocks.effort);
            } else {
                prop_assert_eq!(merged.effort, effort.map(crate::entities::Effort::new));
                prop_assert_eq!(merged.clocks.effort, ts(doc_secs));
            }
        }
    }
}

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
//! - **Labels** are per-membership OR-set state (ADR 0004's 2026-10-28
//!   amendment, #129): adds survive concurrently, a removal wins only
//!   when strictly newer, removals are remembered (no-flap), and entity
//!   death dominates. **Position** (`stack` + `order`) is one composite
//!   field.
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

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};

use crate::entities::{Board, Label, MembershipState, RemoteTaskBaseline, Stack, Task};
use crate::ids::{
    LabelId, ReminderRuleId, RemoteBoardId, RemoteCardRef, RemoteLabelId, RemoteStackId, StackId,
    TaskId,
};
use crate::outbox::OpId;
use crate::reminders::ReminderRule;
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
/// The crash-window duplicate reconcile's rule tag (#150): collapses are
/// traced, never broadcast — the duplicate was ours.
pub(crate) const RULE_R_DUP: &str = "R-dup";

/// Smallest representable `DateTime<Utc>`: never-newer than anything.
pub(crate) fn utc_min() -> DateTime<Utc> {
    DateTime::<Utc>::MIN_UTC
}

/// Remote value wins iff strictly newer; ties keep local.
pub(crate) fn remote_wins(remote_ts: DateTime<Utc>, field_clock: DateTime<Utc>) -> bool {
    remote_ts > field_clock
}

/// One bool per baseline-covered field: the incoming remote snapshot
/// differs from the last observation. `scheduled`'s consumer still applies
/// its own `native_scheduled` lane gate (D-V2); the diff is lane-blind.
/// The bool fields ARE the data — one predicate per mergeable field.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct BaselineDiff {
    title: bool,
    description: bool,
    duedate: bool,
    scheduled: bool,
    done: bool,
    position: bool,
    labels: bool,
    archived: bool,
}

/// The changed-field diff against the persisted baseline (one traversal,
/// one seam for the property catalogue). Pure: compares field values only.
fn diff_baseline(baseline: &RemoteTaskBaseline, remote: &RemoteTask) -> BaselineDiff {
    BaselineDiff {
        title: remote.title != baseline.title,
        description: remote.description != baseline.description,
        duedate: remote.duedate != baseline.duedate,
        scheduled: remote.scheduled != baseline.scheduled,
        done: remote.done != baseline.done,
        position: remote.stack != baseline.stack || remote.order != baseline.order,
        labels: remote.labels != baseline.labels,
        archived: remote.archived != baseline.archived,
    }
}

fn max_ts(a: DateTime<Utc>, b: DateTime<Utc>) -> DateTime<Utc> {
    if a > b { a } else { b }
}

/// The per-membership OR-set outcome (ADR 0004's labels amendment):
/// the local membership state `l` against the observed remote event
/// `r`. Add-wins-on-tie: a remove beats an add only when strictly
/// newer; an add at an equal stamp survives in both orders.
///
/// | local            | remote event | outcome                                |
/// |------------------|--------------|----------------------------------------|
/// | —                | `Added{t}`   | added                                  |
/// | `Added{a}`       | `Added{t}`   | added; clock `max(a,t)`                |
/// | `Added{a}`       | `Removed{t}` | removed iff `t > a`, else the add stands |
/// | `Removed{r}`     | `Added{t}`   | added iff `t >= r`, else stays removed  |
/// | `Removed{r}`     | `Removed{t}` | removed; clock `max(r,t)`              |
#[must_use]
pub fn merge_membership(l: MembershipState, r: MembershipState) -> MembershipState {
    use MembershipState::{Added, Removed};
    match (l, r) {
        (Added { at: a }, Added { at: t }) => Added { at: max_ts(a, t) },
        (Added { at: a }, Removed { at: t }) if t > a => Removed { at: t },
        // t <= a: the remove is not strictly newer; the add stands.
        (Added { at: a }, Removed { .. }) => Added { at: a },
        (Removed { at: r }, Added { at: t }) if t >= r => Added { at: t },
        // t < r: the stale re-add cannot resurrect behind the tombstone.
        (Removed { at: r }, Added { .. }) => Removed { at: r },
        (Removed { at: r }, Removed { at: t }) => Removed { at: max_ts(r, t) },
    }
}

/// The map-entry form: an event on a membership the map does not know
/// is the table's first row (the "—" row). All callers merge through
/// this so the "—" semantics live in exactly one place.
#[must_use]
pub fn merge_membership_opt(l: Option<MembershipState>, r: MembershipState) -> MembershipState {
    match l {
        None => r,
        Some(l) => merge_membership(l, r),
    }
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
        label_memberships: map_remote_labels(ctx, remote.id.board, &remote.labels)
            .into_iter()
            .map(|id| {
                (
                    id,
                    MembershipState::Added {
                        at: remote.last_modified,
                    },
                )
            })
            .collect(),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,

        archived: remote.archived,
        deleted: false,
        clocks: clocks_from(remote.last_modified),
        remote_seen: Some(remote.last_modified),
        remote_baseline: Some(RemoteTaskBaseline::of(remote)),
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
        effort: ts,
        importance: ts,
        urgency: ts,
        recur: ts,

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

/// Aligns a membership map with an observed live set stamped `ts`: every
/// live id becomes (or stays) `Added` through the outcome table, every
/// local `Added` id absent from the live set is stamped `Removed{max(at,
/// ts)}` — never dropped (the no-flap rule); existing `Removed` entries
/// stay. The shared translation behind the missing-baseline fallback and
/// the push echo (both observe a whole live set).
fn adopt_live_set(
    map: &mut std::collections::BTreeMap<LabelId, MembershipState>,
    live: &BTreeSet<LabelId>,
    ts: DateTime<Utc>,
) {
    for id in live {
        let event = MembershipState::Added { at: ts };
        let entry = map.remove(id);
        map.insert(*id, merge_membership_opt(entry, event));
    }
    let displaced: Vec<LabelId> = map
        .iter()
        .filter(|(id, s)| matches!(s, MembershipState::Added { .. }) && !live.contains(id))
        .map(|(id, _)| *id)
        .collect();
    for id in displaced {
        let local_at = map[&id].at();
        map.insert(
            id,
            MembershipState::Removed {
                at: max_ts(local_at, ts),
            },
        );
    }
}

/// The push echo's membership translation (#129): the echo observes a
/// whole live set, so `adopt_live_set` restamps the echo-present ids at
/// `lm` and tombstones the newly displaced members; an echo-missing id
/// that was *already* `Removed` keeps its tombstone but advances its
/// clock to `max(at, lm)` — the echo is our own PUT's response, so its
/// stamp is still an observation of the membership. A stale
/// (cache-lagged) echo can never resurrect behind a newer tombstone —
/// the outcome table keeps the removal.
fn adopt_echo_memberships(
    local: &std::collections::BTreeMap<LabelId, MembershipState>,
    live: &BTreeSet<LabelId>,
    lm: DateTime<Utc>,
) -> std::collections::BTreeMap<LabelId, MembershipState> {
    let mut memberships = local.clone();
    adopt_live_set(&mut memberships, live, lm);
    for (id, state) in &mut memberships {
        if !live.contains(id) {
            *state = MembershipState::Removed {
                at: max_ts(state.at(), lm),
            };
        }
    }
    memberships
}

/// R7 — applies the OR-set label events of a changed remote snapshot
/// (#129). With a baseline, the remote set diffs against it in remote
/// vocabulary: an observed add stamps `Added{ts_r}`, an observed removal
/// stamps `Removed{ts_r}`, each through the outcome table; unmapped ids
/// are skipped (the pipeline adopts label entities first, so unmapped
/// means absent remotely — entity-death dominance in its quiet form).
/// Without a baseline (local creates, pre-migration rows, freshly bound
/// reconcile survivors) the mapped live set is adopted wholesale; the
/// cycle establishes the record either way. Returns the applied
/// (adds, removes) event counts for the verdict log — the wholesale
/// fallback reports `(0, 0)` (it is not a diff).
fn apply_label_events(
    merged: &mut Task,
    baseline: Option<&RemoteTaskBaseline>,
    remote: &crate::remote::RemoteTask,
    ctx: &RemoteIndex,
    ts_r: DateTime<Utc>,
) -> (usize, usize) {
    let Some(baseline) = baseline else {
        adopt_live_set(
            &mut merged.label_memberships,
            &map_remote_labels(ctx, remote.id.board, &remote.labels),
            ts_r,
        );
        return (0, 0);
    };
    let mut stamp = |events: &[crate::ids::RemoteLabelId], event| {
        let mut applied = 0usize;
        for rl in events {
            let Some(local_id) = resolve_label(ctx, remote.id.board, *rl) else {
                continue;
            };
            let entry = merged.label_memberships.remove(&local_id);
            merged
                .label_memberships
                .insert(local_id, merge_membership_opt(entry, event));
            applied += 1;
        }
        applied
    };
    let add_events: Vec<crate::ids::RemoteLabelId> = remote
        .labels
        .difference(&baseline.labels)
        .copied()
        .collect();
    let remove_events: Vec<crate::ids::RemoteLabelId> = baseline
        .labels
        .difference(&remote.labels)
        .copied()
        .collect();
    let adds = stamp(&add_events, MembershipState::Added { at: ts_r });
    let removes = stamp(&remove_events, MembershipState::Removed { at: ts_r });
    (adds, removes)
}

/// R4/R5/R7/R8 — per-field LWW merge of a bound task against a changed
/// remote snapshot entry (`remote.last_modified > local.remote_seen` is
/// the caller's responsibility; unchanged remotes never reach this).
///
/// The `deleted` field participates like any other: a local tombstone is
/// resurrected iff the remote write is strictly newer than the tombstone
/// (R5); otherwise the tombstone stands and its pending delete re-pushes.
/// This is *adoption* (the card is present), never contention — an
/// *absent* card's delete/edit verdict is R3's
/// [`has_unsynced_work`] carve-out.
#[allow(clippy::too_many_lines)] // one verdict block per field; the length is the schema's
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

    // The changed-field gate (ADR 0004's remote baseline, #128): a field
    // applies only when it also *differs from the last observed remote
    // snapshot*. A missing baseline (local create, pre-migration row)
    // falls back to the blanket rule for this one cycle — and the cycle
    // still establishes the record, so convergence is one pull.
    let changed = match &local.remote_baseline {
        Some(baseline) => diff_baseline(baseline, remote),
        None => BaselineDiff {
            title: true,
            description: true,
            duedate: true,
            scheduled: true,
            done: true,
            position: true,
            labels: true,
            archived: true,
        },
    };

    let title_remote_wins = remote_wins(ts_r, local.clocks.title) && changed.title;
    if title_remote_wins {
        merged.title.clone_from(&remote.title);
        merged.clocks.title = ts_r;
    }
    let description_remote_wins =
        remote_wins(ts_r, local.clocks.description) && changed.description;
    if description_remote_wins {
        merged.description.clone_from(&remote.description);
        merged.clocks.description = ts_r;
    }
    let duedate_remote_wins = remote_wins(ts_r, local.clocks.duedate) && changed.duedate;
    if duedate_remote_wins {
        merged.duedate = remote.duedate;
        merged.clocks.duedate = ts_r;
    }
    // D-V2's sharp edge: while the connector lacks native `startdate`
    // writes, *nothing native* touches `scheduled` — the always-absent
    // remote value must never clobber local state via "remote wins,
    // value null". The field belongs exclusively to the sidecar fold.
    // The diff is lane-blind; the lane gate stays conjunctive here.
    let scheduled_remote_wins =
        native_scheduled && remote_wins(ts_r, local.clocks.scheduled) && changed.scheduled;
    if scheduled_remote_wins {
        merged.scheduled = remote.scheduled;
        merged.clocks.scheduled = ts_r;
    }
    let done_remote_wins = remote_wins(ts_r, local.clocks.done) && changed.done;
    if done_remote_wins {
        merged.done = remote.done;
        merged.clocks.done = ts_r;
    }
    // R8: position is one composite field; the diff treats it as a unit.
    let position_remote_wins = remote_wins(ts_r, local.clocks.position) && changed.position;
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
    // R7: the label memberships are per-membership OR-set state (ADR
    // 0004's labels amendment, #129). An unchanged remote set (against
    // the persisted baseline) emits no events — Deck cache-lag echoes of
    // our own writes are invisible by construction. A changed set diffs
    // against the baseline in remote vocabulary: an observed add stamps
    // `Added{ts_r}`, an observed removal stamps `Removed{ts_r}`, each
    // through the outcome table. Unmapped ids are skipped (the pipeline
    // adopts label entities first, so unmapped means absent remotely —
    // entity-death dominance in its quiet form). A task without a
    // baseline (local creates, pre-migration rows, freshly bound
    // reconcile survivors) falls back to the wholesale live-set
    // adoption; the cycle establishes the record either way.
    let (labels_remote_adds, labels_remote_removes) = if changed.labels {
        apply_label_events(
            &mut merged,
            local.remote_baseline.as_ref(),
            remote,
            ctx,
            ts_r,
        )
    } else {
        (0, 0)
    };
    let archived_remote_wins = remote_wins(ts_r, local.clocks.archived) && changed.archived;
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
        labels_changed = changed.labels,
        labels_remote_adds,
        labels_remote_removes,
        position_remote_wins,
        position_stack_resolved = position_stack.is_some(),
        title_changed = changed.title,
        description_changed = changed.description,
        duedate_changed = changed.duedate,
        scheduled_changed = changed.scheduled,
        done_changed = changed.done,
        position_changed = changed.position,
        archived_changed = changed.archived,
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
    // The merge observes the remote wholly even when it adopts partially —
    // and also in fallback mode (a fallback cycle establishes the record).
    merged.remote_baseline = Some(RemoteTaskBaseline::of(remote));
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
            "recur" => (
                intent.recur,
                matches!(write.value, crate::appdata::ModeledValue::Recur(_)),
                write.clock,
                local.clocks.recur,
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
                "recur" => {
                    merged.recur.clone_from(&local.recur);
                    merged.clocks.recur = l;
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
            crate::appdata::ModeledValue::Recur(value) => {
                merged.recur.clone_from(value);
                merged.clocks.recur = clock;
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
                insistence,
            } => {
                let Ok(uuid) = uuid::Uuid::parse_str(key) else {
                    continue; // an unparseable key is not a rule identity
                };
                let id = crate::ids::ReminderRuleId::from(uuid);
                match rules.get_mut(&id) {
                    Some(local) if *clock >= local.updated_at => {
                        local.offset = *offset;
                        local.created_at = *created;
                        local.insistence = *insistence;
                        local.updated_at = std::cmp::max(local.updated_at, *clock);
                        changes.upserts.push(local.clone());
                    }
                    Some(_) => {} // local stands
                    None => {
                        let adopted = crate::reminders::ReminderRule {
                            insistence: *insistence,
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

/// The sidecar document fold for one card's synced dismissals (design
/// D-4/D-9, the §5b position right after [`fold_reminder_rules`]).
/// Rows fold **additively** into the dismissal set — append-only facts,
/// never edited, so no clock comparison and no tombstones (a dismissal
/// is permanent for its occurrence):
///
/// - A `Dismissed` row whose key parses as a `(rule uuid, occurrence)`
///   pair inserts `ReminderDismissal { Rule(rule), occurrence,
///   dismissed_at }` unless the `(reminder, occurrence)` key is already
///   present (first-write-wins, the deliveries-fold discipline).
/// - **Guard-immune**: no `protected` parameter — the D-S6 intent flag
///   protects local *edits*; there is no "edit a dismissal" verb.
/// - [`crate::appdata::DismissalEntry::Verbatim`] rows never fold.
/// - Unparseable keys — a hand-edited document's `default:`/`alarm:`
///   rows, malformed pairs — are **ignored without being dropped**: the
///   codec carried them as `Verbatim`, and the union's additivity keeps
///   them in the document (the verbatim relay, design D-4's fold side).
///
/// Returns the newly folded rows (the pipeline turns them into
/// `RecordReminderDismissal` actions — batch and view are the same
/// deltas by construction).
pub fn fold_reminder_dismissals(
    dismissals: &mut std::collections::BTreeSet<crate::reminders::ReminderDismissal>,
    entry: &crate::appdata::DismissalsEntry,
) -> Vec<crate::reminders::ReminderDismissal> {
    let mut folded = Vec::new();
    for (key, row) in &entry.dismissals {
        let crate::appdata::DismissalEntry::Dismissed { dismissed_at } = row else {
            continue; // verbatim content never folds
        };
        let Some((rule_id, occurrence)) = crate::appdata::parse_dismissal_wire_key(key) else {
            continue; // not a modeled key: ignored, kept verbatim
        };
        let dismissal = crate::reminders::ReminderDismissal {
            reminder: crate::reminders::ReminderRef::Rule(rule_id),
            occurrence,
            dismissed_at: *dismissed_at,
        };
        if dismissals.insert(dismissal) {
            folded.push(dismissal);
        }
    }
    folded
}

/// The typed deltas one [`fold_time_entries`] pass produced: the
/// pipeline turns them into persistence actions and applies the same
/// deltas to the memory view, so batch and view cannot drift (the
/// [`RuleFoldChanges`] pattern).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrackingFoldChanges {
    /// Entries adopted or updated from the document.
    pub upserts: Vec<crate::tracking::TimeEntry>,
    /// Entries removed by a strictly newer tombstone.
    pub deletes: Vec<crate::ids::TimeEntryId>,
}

/// The sidecar document fold for the board-level `time-entries` section
/// (design D-T12, recipe step 8 — the §5b position right after
/// [`fold_reminder_dismissals`]). Keyed by the entry UUID; per-key LWW
/// against the local `updated_at`:
///
/// - A live `Entry` row whose `clock >=` the local row's `updated_at`
///   overwrites `task`/`task_title`/`start`/`end` with
///   `updated_at = max(local, clock)` (doc wins pull-side ties — the
///   [`fold_reminder_rules`] comparison, the mirror of the flush's
///   tie-to-overlay); an unknown UUID adopts the row verbatim with
///   `updated_at = clock`; an older row leaves local standing.
/// - A tombstone removes the local row iff strictly newer (a delete
///   after an adjust wins; a re-created row carries a newer clock and
///   stands). Tombstones for locally-unknown ids leave no trace.
/// - [`crate::appdata::TimeEntryRecord::Verbatim`] rows never fold.
/// - **Guard-immune** (D-13): no `protected` parameter — the D-S6
///   intent flag protects local *edits* during the copy-over collapse
///   window; entries are independent rows with no such window (the
///   dismissals fold's precedent, but with LWW because entries are
///   editable).
/// - **Fail-closed resolution**: a row with a null `task` or a card
///   absent from `cards` (decimal card key → `TaskId`, board-scoped) is
///   skipped — unresolvable rows are neither imported nor deleted; they
///   age out on the wire instead (D-T12).
pub fn fold_time_entries(
    tracking: &mut crate::tracking::TrackingState,
    rows: &std::collections::BTreeMap<String, crate::appdata::TimeEntryRecord>,
    cards: &std::collections::BTreeMap<String, TaskId>,
) -> TrackingFoldChanges {
    let mut changes = TrackingFoldChanges::default();
    for (key, row) in rows {
        match row {
            crate::appdata::TimeEntryRecord::Entry {
                task: Some(task_card),
                title,
                start,
                end,
                created,
                clock,
            } => {
                let Ok(uuid) = uuid::Uuid::parse_str(key) else {
                    continue; // an unparseable key is not an entry identity
                };
                let Some(&task_id) = cards.get(task_card) else {
                    continue; // unresolvable: neither imported nor deleted
                };
                match tracking
                    .entries
                    .get_mut(&crate::ids::TimeEntryId::from(uuid))
                {
                    Some(local) if *clock >= local.updated_at => {
                        local.task = task_id;
                        local.task_title.clone_from(title);
                        local.start = *start;
                        local.end = *end;
                        local.updated_at = std::cmp::max(local.updated_at, *clock);
                        changes.upserts.push(local.clone());
                    }
                    Some(_) => {} // local stands
                    None => {
                        let adopted = crate::tracking::TimeEntry {
                            id: crate::ids::TimeEntryId::from(uuid),
                            task: task_id,
                            task_title: title.clone(),
                            start: *start,
                            end: *end,
                            created_at: *created,
                            updated_at: *clock,
                        };
                        tracking.entries.insert(adopted.id, adopted.clone());
                        changes.upserts.push(adopted);
                    }
                }
            }
            crate::appdata::TimeEntryRecord::Tombstone { clock } => {
                let Ok(uuid) = uuid::Uuid::parse_str(key) else {
                    continue;
                };
                let id = crate::ids::TimeEntryId::from(uuid);
                if tracking
                    .entries
                    .get(&id)
                    .is_some_and(|local| *clock > local.updated_at)
                {
                    tracking.entries.remove(&id);
                    changes.deletes.push(id);
                }
            }
            crate::appdata::TimeEntryRecord::Entry { task: None, .. }
            | crate::appdata::TimeEntryRecord::Verbatim(_) => {}
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
    tombstoned.remote_baseline = None;
    tracing::debug!(
        rule = RULE_R3,
        task_id = ?local.id,
        deleted_clock = ?tombstoned.clocks.deleted,
        "task tombstoned"
    );
    tombstoned
}

/// D-3a (#130): `true` when the task holds user work the remote has
/// never observed — the delete-contention carve-out's input (R3's
/// resurrect-vs-tombstone fork). Pure; derived from entity state — no
/// persisted flag (ADR 0004's #130 amendment).
///
/// Disjunction of three clauses, each comparing a work stamp against
/// `seen = remote_seen` (the `None` fallback compares against the
/// epoch, resurrect-biased — unreachable for a bound task, defensive
/// only):
///
/// 1. any of the eleven non-`deleted` field clocks strictly newer than
///    `seen` (`deleted` is the tombstone's clock, not user work);
/// 2. any label-membership stamp strictly newer (the OR-set verbs stamp
///    only the membership, never `clocks.labels` — an `Added` *and* a
///    `Removed` both count: a removal is work the server has not seen);
/// 3. any of the task's reminder rules strictly newer (the sidecar
///    lane's user data; its document flush stamps no echo back, so this
///    clause errs toward resurrect until the card's next change — the
///    data-preserving direction, ADR 0004's accepted limitation).
///
/// A fully-synced task is `false` by construction: a drained push's
/// echo maxes all eleven field clocks and every membership stamp to
/// `>= remote_seen`.
#[must_use]
pub fn has_unsynced_work(
    task: &Task,
    reminder_rules: &BTreeMap<ReminderRuleId, ReminderRule>,
) -> bool {
    let seen = task
        .remote_seen
        .unwrap_or_else(crate::entities::BoardClocks::epoch);
    let clocks = &task.clocks;
    let field_work = [
        clocks.title,
        clocks.description,
        clocks.duedate,
        clocks.scheduled,
        clocks.done,
        clocks.position,
        clocks.effort,
        clocks.importance,
        clocks.urgency,
        clocks.recur,
        clocks.archived,
    ]
    .into_iter()
    .any(|clock| clock > seen);
    let membership_work = task
        .label_memberships
        .values()
        .any(|membership| membership.at() > seen);
    let rule_work = reminder_rules
        .values()
        .any(|rule| rule.task == task.id && rule.updated_at > seen);
    field_work || membership_work || rule_work
}

/// R-dup — one crash-window duplicate's create signature: the outbox's
/// pending `CreateTask` op (or the just-landed echo of the replayed one),
/// with the queue stamp the window clause anchors on (#150, D-5b
/// clause 2). No signatures in scope ⇒ the pass cannot fire — the
/// structural no-op is a type-level fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreateSignature {
    /// The task the create op belongs to.
    pub task: TaskId,
    /// The create op's id.
    pub op: OpId,
    /// When the op entered the outbox — the window's lower anchor.
    pub queued_at: DateTime<Utc>,
    /// `true` when the create's echo landed this cycle (the replay arm:
    /// the task is bound to the *newer* card). Pending otherwise.
    pub echoed_this_cycle: bool,
}

/// R-dup — one collapse verdict: the local task, the surviving remote
/// card, every doomed card (each gets a ref-delete push), and the
/// pending create op the collapse satisfies, if any (#150, D-5c).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateCollapse {
    /// The local task the duplicate group belongs to.
    pub task: TaskId,
    /// The surviving card's remote ref (oldest member; an exact stamp
    /// tie keeps the bound card).
    pub survivor: RemoteCardRef,
    /// `false` when the survivor is the already-bound card — no rebind.
    pub rebind: bool,
    /// Every non-survivor member's ref; one ref-delete push each.
    pub doomed: Vec<RemoteCardRef>,
    /// The pending create op the collapse completes (`Some` only in the
    /// pending arm; the echo arm's op already completed in §1).
    pub satisfied_op: Option<OpId>,
}

/// The duplicate-detection window's half-width in seconds (D-5b
/// clause 3, DEC-4): generous by design — a false positive requires a
/// same-title, same-stack, content-equal card created within a minute
/// of our own create.
const DUP_WINDOW_SECS: i64 = 60;

pub(crate) fn dup_window_contains(
    stamp: DateTime<Utc>,
    queued_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> bool {
    stamp >= queued_at - chrono::Duration::seconds(DUP_WINDOW_SECS)
        && stamp <= now + chrono::Duration::seconds(DUP_WINDOW_SECS)
}

/// R-dup — detects whether one *unbound* snapshot card is a
/// crash-window duplicate of ours (#150, D-5b): pure, total, injected
/// data only. The four clauses must all hold against one matching
/// create signature; the first matching signature wins (deterministic
/// in `creates` order). `bound_card` is the matched task's bound card
/// observation when one exists in this cycle's evidence (snapshot
/// listing or this cycle's echo); a bound task without an observation
/// fails clause 3 conservatively — the next full pull retries.
///
/// Clause vocabulary: (1) same normalized title + same resolved stack;
/// (2) the create signature (this function runs per signature); (3) the
/// window `[op.queued_at − 60s, now + 60s]` on both stamps; (4) content
/// equality on the pushed shape (description, duedate, done, archived
/// always; `scheduled` only when the server's native `startdate` lane
/// was on — the create never sent it otherwise).
#[must_use]
pub fn detect_crash_window_duplicates(
    candidate: &RemoteTask,
    bound_card: Option<&RemoteTask>,
    tasks: &BTreeMap<TaskId, Task>,
    stacks: &BTreeMap<StackId, Stack>,
    creates: &[CreateSignature],
    now: DateTime<Utc>,
    native_scheduled: bool,
) -> Option<DuplicateCollapse> {
    for sig in creates {
        // A signature whose task vanished mid-cycle (a local delete racing
        // the collapse) disqualifies that signature only — later
        // signatures still get their verdict.
        let Some(local) = tasks.get(&sig.task) else {
            continue;
        };
        // Clause 1: same normalized title, same resolved stack.
        if crate::reconcile::normalized_title(&local.title)
            != crate::reconcile::normalized_title(&candidate.title)
        {
            continue;
        }
        let Some(local_stack_ref) = stacks.get(&local.stack).and_then(|s| s.remote) else {
            // An unmapped local stack fails the clause: the task could
            // not have pushed, so it has no crash window.
            continue;
        };
        if local_stack_ref.stack != candidate.stack || local_stack_ref.board != candidate.id.board {
            continue;
        }
        // Clause 4: byte-ours content equality (title is clause 1's job;
        // order and labels are excluded — DEC-8).
        let content_equal = local.description == candidate.description
            && local.duedate == candidate.duedate
            && local.done == candidate.done
            && local.archived == candidate.archived
            && (local.scheduled == candidate.scheduled || !native_scheduled);
        if !content_equal {
            continue;
        }
        // Clause 3: the window, on the candidate's stamp and — when the
        // task is bound — the bound card's stamp too.
        if !dup_window_contains(candidate.created_at, sig.queued_at, now) {
            continue;
        }
        let bound_member = match (&local.remote, bound_card) {
            (Some(ref_), Some(obs)) => {
                if obs.id != *ref_ || !dup_window_contains(obs.created_at, sig.queued_at, now) {
                    continue;
                }
                Some((obs.id, obs.created_at))
            }
            (Some(_), None) => continue, // conservative: no bound observation
            (None, _) => None,
        };
        // Clause 2 holds by construction: the caller passes only
        // in-scope create signatures (pending or echoed this cycle).
        let satisfied_op = (!sig.echoed_this_cycle).then_some(sig.op);
        return Some(crate::merge::collapse_for_task(
            sig.task,
            bound_member,
            vec![(candidate.id, candidate.created_at)],
            satisfied_op,
        ));
    }
    None
}

/// R-dup — group formation + survivorship for one task's duplicate
/// group (#150, D-5c, DEC-5/DEC-10): the **oldest** member survives (it
/// owns any remote-side history the crash window could not have), an
/// exact stamp tie keeps the **bound** card (no rebind churn for an
/// undecidable order), and every other member is doomed. Deterministic
/// under candidate arrival order — the fold's inputs carry their own
/// stamps, not their positions.
///
/// # Panics
///
/// Never with a non-empty group (the pipeline only forms verdicts over
/// observed members); an empty `candidates` without `bound` has no
/// survivor and panics — a caller contract, not an input-validation
/// surface.
#[must_use]
pub fn collapse_for_task(
    task: TaskId,
    bound: Option<(RemoteCardRef, DateTime<Utc>)>,
    candidates: Vec<(RemoteCardRef, DateTime<Utc>)>,
    satisfied_op: Option<OpId>,
) -> DuplicateCollapse {
    // (ref, stamp, is_bound): on equal stamps the bound member sorts first.
    let mut members: Vec<(RemoteCardRef, DateTime<Utc>, bool)> = candidates
        .into_iter()
        .map(|(ref_, stamp)| (ref_, stamp, false))
        .collect();
    if let Some((ref_, stamp)) = bound {
        members.push((ref_, stamp, true));
    }
    let &(survivor, _, survivor_is_bound) = members
        .iter()
        // `(stamp, !is_bound)`: on an exact stamp tie the bound member
        // sorts first (DEC-5 — no rebind churn for an undecidable order).
        .min_by(|a, b| (a.1, !a.2).cmp(&(b.1, !b.2)))
        .expect("a collapse group has at least one member");
    let doomed: Vec<RemoteCardRef> = members
        .iter()
        .filter(|(ref_, _, _)| *ref_ != survivor)
        .map(|(ref_, _, _)| *ref_)
        .collect();
    tracing::debug!(
        rule = RULE_R_DUP,
        task_id = ?task,
        survivor = ?survivor,
        doomed = doomed.len(),
        satisfied_op = ?satisfied_op,
        "crash-window duplicate collapse formed"
    );
    DuplicateCollapse {
        task,
        survivor,
        rebind: !survivor_is_bound,
        doomed,
        satisfied_op,
    }
}

/// R-dup — rebinds the local task to the surviving older card: the
/// binding trio writes (ref, `remote_seen`, baseline refresh — D-1c's
/// every-adoption-point rule) and **nothing else**. Content equality
/// was clause 4's job; the survivor's labels/order land via the next
/// pull's R4 compare, where a tie keeps local (the pushed truth).
#[must_use]
pub fn rebind_task_to_survivor(local: &Task, survivor: &RemoteTask) -> Task {
    let mut rebinded = local.clone();
    rebinded.remote = Some(survivor.id);
    rebinded.remote_seen = Some(survivor.last_modified);
    rebinded.remote_baseline = Some(crate::entities::RemoteTaskBaseline::of(survivor));
    tracing::debug!(
        rule = RULE_R_DUP,
        task_id = ?local.id,
        survivor = ?survivor.id,
        remote_seen = ?survivor.last_modified,
        "task rebound to the surviving card"
    );
    rebinded
}

/// The fold-time series dedup's rule tag (§3.6, D-C7): one traced fact
/// per applied collapse, never a broadcast — the spawn race is ours.
pub(crate) const RULE_R_RECUR_DUP: &str = "R-recur-dup";

/// One series-twin group's verdict (§3.6, D-C7): the older-created task
/// survives, every other member of the group is doomed (neutralized as
/// a ref-less tombstone by the applying pass).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesTwinCollapse {
    /// The group's survivor — the minimum `TaskId`, and v7 ids are
    /// time-ordered, so the minimum is the older-created task.
    pub survivor: TaskId,
    /// Every non-survivor member, ascending.
    pub doomed: Vec<TaskId>,
}

/// The fold-time dedup predicate (§3.6, property 7): groups live, open,
/// recur-bearing tasks by `(rule.series, reference local date)`; any
/// group with ≥ 2 members is a spawn race. Pure, total, injected data
/// only. The reference date derives through [`crate::recur::
/// roll_reference_date`] — the roll's exact derivation, one date
/// vocabulary — and a dateless task is unkeyed: it joins no group and
/// can never trigger. Output is group-key ascending, deterministic
/// under input order.
#[must_use]
pub fn detect_series_twins(
    tasks: &BTreeMap<TaskId, Task>,
    day: crate::days::DayCalendar,
) -> Vec<SeriesTwinCollapse> {
    use std::collections::BTreeMap as Groups;
    let mut groups: Groups<(crate::ids::SeriesId, chrono::NaiveDate), Vec<TaskId>> = Groups::new();
    for (id, task) in tasks {
        // Structural membership only: live, open, recur-bearing. A done
        // task is a settled occurrence; a tombstoned one no longer
        // exists — neither is ever a twin candidate (D-5's false-
        // positive class, kept out structurally).
        if !task.is_live() || task.done.is_some() {
            continue;
        }
        let Some(rule) = &task.recur else {
            continue;
        };
        let Some(date) = crate::recur::roll_reference_date(task, day) else {
            continue; // dateless: unkeyed (never groupable)
        };
        groups.entry((rule.series, date)).or_default().push(*id);
    }
    groups
        .into_values()
        .filter_map(|mut members| {
            if members.len() < 2 {
                return None;
            }
            members.sort_unstable();
            let survivor = members[0];
            let doomed = members[1..].to_vec();
            Some(SeriesTwinCollapse { survivor, doomed })
        })
        .collect()
}

/// The fold-time series dedup's survivor merge (§3.6): the survivor
/// absorbs the doomed twin field-wise under LWW clocks (ties to the
/// doomed row — the pair-merge's read-result convention), label
/// memberships OR-set fold key-by-key — and keeps its own identity and
/// binding trio: the first spawn owns any remote history the race
/// window could not have (D-5c).
#[must_use]
pub fn series_survivor_merge(survivor: &Task, doomed: &Task) -> Task {
    let mut merged = crate::reconcile::merge_task_pair(survivor, doomed);
    // `merge_task_pair` fixes the binding from its *bound* argument (the
    // doomed row here); the survivor's own remote history persists.
    merged.remote = survivor.remote;
    merged.remote_seen = survivor.remote_seen;
    merged.remote_baseline.clone_from(&survivor.remote_baseline);
    merged
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
        // The echo is our own PUT's response: its label set is a whole
        // live-set observation, translated never to forget a removal
        // (the no-flap rule, #129).
        label_memberships: adopt_echo_memberships(
            &local.label_memberships,
            &map_remote_labels(ctx, echo.id.board, &echo.labels),
            lm,
        ),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,

        archived: echo.archived,
        deleted: false,
        clocks: crate::entities::TaskClocks {
            title: max_ts(local.clocks.title, lm),
            description: max_ts(local.clocks.description, lm),
            duedate: max_ts(local.clocks.duedate, lm),
            scheduled: echo_scheduled_clock,
            done: max_ts(local.clocks.done, lm),
            position: max_ts(local.clocks.position, lm),
            effort: max_ts(local.clocks.effort, lm),
            importance: max_ts(local.clocks.importance, lm),
            urgency: max_ts(local.clocks.urgency, lm),
            recur: max_ts(local.clocks.recur, lm),
            archived: max_ts(local.clocks.archived, lm),
            deleted: max_ts(local.clocks.deleted, lm),
        },
        remote_seen: Some(lm),
        // The server-normalized echo values are the next pull's diff
        // reference (the baseline-drift guard, design §5).
        remote_baseline: Some(RemoteTaskBaseline::of(echo)),
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
    finalized.remote_baseline = None;
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
    use crate::reminders::Insistence;
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
    fn granularity_trap_closed_remote_touch_keeps_local_title() {
        let mut local = bound_task(1);
        local.title = "edited locally".into();
        local.clocks.title = ts(BASE + 330); // 10:05:30
        // The remote's previous observation establishes the baseline; the
        // new write touches only the description.
        let previous = remote_task(1, 1, BASE + 180);
        let local = with_baseline(local, &previous);
        let mut remote = remote_task(1, 1, BASE + 360); // 10:06:00
        remote.description = "edited remotely".into();

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx, true);

        // The description applies (changed + newer); the title the remote
        // did not touch keeps the local value AND its local clock (#128).
        assert_eq!(merged.title, "edited locally");
        assert_eq!(merged.clocks.title, ts(BASE + 330));
        assert_eq!(merged.description, "edited remotely");
        assert_eq!(merged.clocks.description, ts(BASE + 360));
        assert_eq!(merged.remote_seen, Some(ts(BASE + 360)));
        assert_eq!(merged.remote_baseline, Some(baseline_of(&remote)));
    }

    #[test]
    fn an_unchanged_remote_applies_nothing_but_still_observes() {
        let mut local = bound_task(1);
        local.title = "edited locally".into();
        local.clocks.title = ts(BASE + 330);
        local.order = 42;
        local.archived = true;
        // Baseline equals the incoming snapshot in every covered field.
        let mut remote = remote_task(1, 1, BASE + 360);
        remote.order = 42;
        remote.archived = true;
        let local = with_baseline(local, &remote);

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx, true);

        // Observation without adoption: every value and every clock keep
        // local; only the observation facts advance.
        assert_eq!(merged.title, "edited locally");
        assert_eq!(merged.clocks.title, ts(BASE + 330));
        assert_eq!(merged.order, 42);
        assert_eq!(merged.clocks.position, ts(BASE));
        assert!(merged.archived);
        assert_eq!(merged.clocks.archived, ts(BASE));
        assert_eq!(merged.remote_seen, Some(ts(BASE + 360)));
        assert_eq!(merged.remote_baseline, Some(baseline_of(&remote)));
    }

    #[test]
    fn missing_baseline_falls_back_then_converges() {
        let mut local = bound_task(1);
        local.title = "edited locally".into();
        local.clocks.title = ts(BASE + 330);
        let remote = remote_task(1, 1, BASE + 360); // unchanged title, newer stamp

        let ctx = RemoteIndex::default();
        let first = merge_task(&local, &remote, &ctx, true);

        // The fallback cycle is bit-for-bit today's blanket LWW...
        assert_eq!(first.title, remote.title);
        assert_eq!(first.clocks.title, ts(BASE + 360));
        // ...and it establishes the record.
        assert_eq!(first.remote_baseline, Some(baseline_of(&remote)));

        // The second cycle with the same snapshot is a fixpoint.
        let second = merge_task(&first, &remote, &ctx, true);
        assert_eq!(second, first);
    }

    #[test]
    fn adopt_remote_task_establishes_the_baseline() {
        let mut remote = remote_task(1, 1, BASE + 60);
        remote.labels = BTreeSet::from([RemoteLabelId(7)]);
        let id = TaskId::from(uuid::Uuid::from_u128(9));
        let mut ctx = RemoteIndex::default();
        ctx.stack_by_ref
            .insert(stack_ref(1), StackId::from(uuid::Uuid::from_u128(900)));

        let adopted = adopt_remote_task(&remote, id, &ctx).expect("stack resolves");

        assert_eq!(adopted.remote_baseline, Some(baseline_of(&remote)));
    }

    #[test]
    fn adopt_task_after_push_baseline_holds_the_normalized_echo() {
        let ctx = RemoteIndex::default();
        let local = bound_task(1);
        let mut echo = remote_task(1, 1, BASE + 300);
        echo.title = "  card 1  ".into(); // server-side normalization

        let adopted = adopt_task_after_push(&local, &echo, &ctx, true);

        // The echo's *normalized* values are the next pull's diff
        // reference — the baseline-drift guard (design §5).
        assert_eq!(adopted.remote_baseline, Some(baseline_of(&echo)));
        assert_eq!(
            adopted.remote_baseline.as_ref().map(|b| b.title.as_str()),
            Some("  card 1  ")
        );
    }

    #[test]
    fn tombstone_task_clears_the_baseline() {
        let mut remote = remote_task(1, 1, BASE + 60);
        remote.title = "observed".into();
        let local = with_baseline(bound_task(1), &remote);

        let tombstoned = tombstone_task(&local, ts(BASE + 120));

        assert_eq!(tombstoned.remote_baseline, None);
        assert_eq!(tombstoned.remote, None);
    }

    #[test]
    fn finalize_pushed_task_delete_clears_the_baseline() {
        let local = with_baseline(bound_task(1), &remote_task(1, 1, BASE + 60));

        let finalized = finalize_pushed_task_delete(&local);

        assert_eq!(finalized.remote_baseline, None);
        assert!(finalized.remote.is_none());
    }

    // ---- D-3a: `has_unsynced_work` (#130) --------------------------

    fn rules_map(
        rules: &[crate::reminders::ReminderRule],
    ) -> std::collections::BTreeMap<crate::ids::ReminderRuleId, crate::reminders::ReminderRule>
    {
        rules.iter().map(|rule| (rule.id, rule.clone())).collect()
    }

    fn no_rules()
    -> std::collections::BTreeMap<crate::ids::ReminderRuleId, crate::reminders::ReminderRule> {
        std::collections::BTreeMap::new()
    }

    /// The echo-shaped fixture: every stamp at or below `remote_seen`
    /// (a drained push's echo maxes the clocks and the membership
    /// stamps there) — the 99%-path input, stated as data.
    fn synced_task() -> Task {
        let mut task = bound_task(1);
        task.label_memberships.insert(
            LabelId::from(uuid::Uuid::from_u128(500)),
            crate::entities::MembershipState::Added { at: ts(BASE) },
        );
        task
    }

    #[test]
    fn a_fresh_field_clock_counts_as_unsynced_work() {
        let mut task = synced_task();
        task.clocks.title = ts(BASE + 60);

        assert!(has_unsynced_work(&task, &no_rules()));
    }

    #[test]
    fn a_fresh_added_membership_counts_as_unsynced_work() {
        let mut task = synced_task();
        task.label_memberships.insert(
            LabelId::from(uuid::Uuid::from_u128(501)),
            crate::entities::MembershipState::Added { at: ts(BASE + 60) },
        );

        assert!(has_unsynced_work(&task, &no_rules()));
    }

    #[test]
    fn a_fresh_removed_membership_counts_as_unsynced_work() {
        // The removal-is-work pin: the OR-set verbs stamp only the
        // membership, never `clocks.labels` — a removal the server has
        // not seen is exactly the work this predicate exists to see.
        let mut task = synced_task();
        task.clocks.title = ts(BASE); // all field clocks at `remote_seen`
        task.label_memberships.insert(
            LabelId::from(uuid::Uuid::from_u128(501)),
            crate::entities::MembershipState::Removed { at: ts(BASE + 60) },
        );

        assert!(has_unsynced_work(&task, &no_rules()));
    }

    #[test]
    fn a_fresh_reminder_rule_counts_as_unsynced_work() {
        let task = synced_task();
        let rule = crate::reminders::ReminderRule {
            insistence: Insistence::Normal,
            id: crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(10)),
            task: task.id,
            offset: crate::reminders::ReminderOffset::at_due(),
            created_at: ts(BASE + 60),
            updated_at: ts(BASE + 60),
        };

        assert!(has_unsynced_work(&task, &rules_map(&[rule])));
    }

    #[test]
    fn a_fully_synced_task_holds_no_unsynced_work() {
        let task = synced_task();

        assert!(!has_unsynced_work(&task, &no_rules()));
    }

    #[test]
    fn synced_fields_with_one_fresh_membership_hold_unsynced_work() {
        let mut task = synced_task();
        task.label_memberships.insert(
            LabelId::from(uuid::Uuid::from_u128(501)),
            crate::entities::MembershipState::Added { at: ts(BASE + 60) },
        );

        assert!(has_unsynced_work(&task, &no_rules()));
    }

    #[test]
    fn another_tasks_reminder_rules_do_not_leak_into_the_predicate() {
        let task = synced_task();
        let mut rule = crate::reminders::ReminderRule {
            insistence: Insistence::Normal,
            id: crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(10)),
            task: TaskId::from(uuid::Uuid::from_u128(2)),
            offset: crate::reminders::ReminderOffset::at_due(),
            created_at: ts(BASE + 60),
            updated_at: ts(BASE + 60),
        };
        rule.task = TaskId::from(uuid::Uuid::from_u128(999));

        assert!(!has_unsynced_work(&task, &rules_map(&[rule])));
    }

    #[test]
    fn a_missing_remote_seen_falls_back_to_the_epoch_resurrect_biased() {
        let mut task = synced_task();
        task.remote_seen = None;

        assert!(has_unsynced_work(&task, &no_rules()));
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        /// The design's one-sentence property: the predicate is monotone
        /// in "work newer than `remote_seen`" — shifting every work stamp
        /// strictly below the observation flips it `false`, and raising
        /// any single work stamp strictly above flips it back `true`.
        #[test]
        fn unsynced_work_is_monotone_in_work_newer_than_remote_seen(
            task in crate::test_support::task_strategy(),
            seen_secs in 0_i64..1_000,
        ) {
            let seen = ts(seen_secs);
            let below = seen - chrono::Duration::seconds(1);
            let above = seen + chrono::Duration::seconds(1);

            // The all-below baseline: no work anywhere, `deleted`'s clock
            // excluded from the clause set by construction (never read).
            let mut settled = task;
            settled.remote_seen = Some(seen);
            settled.clocks = crate::entities::TaskClocks {
                title: below,
                description: below,
                duedate: below,
                scheduled: below,
                done: below,
                position: below,
                effort: below,
                importance: below,
                urgency: below,
                recur: below,
                archived: below,
                deleted: below,
            };
            settled.label_memberships = settled
                .label_memberships
                .into_keys()
                .map(|label| (label, crate::entities::MembershipState::Added { at: below }))
                .collect();
            prop_assert!(!has_unsynced_work(&settled, &no_rules()));

            // Raising any single work stamp strictly above flips true.
            let mut field = settled.clone();
            field.clocks.position = above;
            prop_assert!(has_unsynced_work(&field, &no_rules()));

            let mut membership = settled.clone();
            let label = LabelId::from(uuid::Uuid::from_u128(777));
            membership
                .label_memberships
                .insert(label, crate::entities::MembershipState::Removed { at: above });
            prop_assert!(has_unsynced_work(&membership, &no_rules()));

            let rule_task = settled;
            let rule = crate::reminders::ReminderRule {
                insistence: Insistence::Normal,
                id: crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(10)),
                task: rule_task.id,
                offset: crate::reminders::ReminderOffset::at_due(),
                created_at: above,
                updated_at: above,
            };
            prop_assert!(has_unsynced_work(&rule_task, &rules_map(&[rule])));
        }
    }

    #[test]
    fn a_baseline_never_shields_a_tombstone_from_resurrection() {
        let mut local = bound_task(1);
        local.deleted = true;
        local.clocks.deleted = ts(BASE + 300);
        let previous = remote_task(1, 1, BASE + 180);
        let local = with_baseline(local, &previous);
        let remote = remote_task(1, 1, BASE + 360);

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx, true);

        assert!(!merged.deleted, "newer remote edit resurrects");
        assert_eq!(merged.remote, Some(card_ref(1, 1)));
        assert_eq!(merged.remote_baseline, Some(baseline_of(&remote)));
    }

    #[test]
    fn position_diffs_as_one_composite_against_the_baseline() {
        let mut ctx = RemoteIndex::default();
        let local = with_baseline(bound_task(1), &remote_task(1, 1, BASE + 180));
        ctx.stack_by_ref.insert(stack_ref(1), local.stack);

        // The remote only retitled the card: the untouched position keeps
        // local even though ts_r beats the position clock.
        let mut title_touch = remote_task(1, 1, BASE + 360);
        title_touch.title = "moved on".into();
        let merged = merge_task(&local, &title_touch, &ctx, true);
        assert_eq!(merged.title, "moved on");
        assert_eq!(merged.order, local.order);
        assert_eq!(merged.clocks.position, ts(BASE));

        // The remote moved the card: the whole composite applies.
        let mut moved = remote_task(1, 1, BASE + 360);
        moved.order = 77;
        let merged = merge_task(&local, &moved, &ctx, true);
        assert_eq!(merged.order, 77);
        assert_eq!(merged.clocks.position, ts(BASE + 360));
    }

    #[test]
    fn an_unchanged_remote_label_set_emits_no_membership_events() {
        let mut previous = remote_task(1, 1, BASE + 180);
        previous.labels = BTreeSet::from([RemoteLabelId(7)]);
        let mut local = bound_task(1);
        local.label_memberships.insert(
            LabelId::from(uuid::Uuid::from_u128(600)),
            crate::entities::MembershipState::Added { at: ts(BASE + 200) },
        );
        let local = with_baseline(local, &previous);

        let ctx = RemoteIndex::default();

        // Same set as the baseline: no events — the whole map (including
        // the membership's own stamp) is byte-identical despite ts_r.
        let merged = merge_task(&local, &previous, &ctx, true);
        assert_eq!(merged.label_memberships, local.label_memberships);
    }

    #[test]
    fn a_changed_remote_set_tombstones_displaced_local_members_without_a_baseline() {
        let mut previous = remote_task(1, 1, BASE + 180);
        previous.labels = BTreeSet::from([RemoteLabelId(7)]);
        let mut local = bound_task(1);
        local.label_memberships.insert(
            LabelId::from(uuid::Uuid::from_u128(600)),
            crate::entities::MembershipState::Added { at: ts(BASE + 200) },
        );
        local.label_memberships.insert(
            LabelId::from(uuid::Uuid::from_u128(601)),
            crate::entities::MembershipState::Removed { at: ts(BASE + 150) },
        );

        let ctx = RemoteIndex::default();

        // A different set applies: the mapped live set here is empty
        // (unmapped remote labels are skipped), so the local `Added`
        // member is tombstoned — never dropped — and the existing
        // tombstone keeps its older stamp.
        let mut changed = remote_task(1, 1, BASE + 360);
        changed.labels = BTreeSet::from([RemoteLabelId(8)]);
        let merged = merge_task(&local, &changed, &ctx, true);
        assert!(
            merged.live_labels().is_empty(),
            "unmapped remote labels drop"
        );
        assert_eq!(
            merged.label_memberships[&LabelId::from(uuid::Uuid::from_u128(600))],
            crate::entities::MembershipState::Removed { at: ts(BASE + 360) }
        );
        assert_eq!(
            merged.label_memberships[&LabelId::from(uuid::Uuid::from_u128(601))],
            crate::entities::MembershipState::Removed { at: ts(BASE + 150) }
        );
    }

    // ---- the baseline-diff label events (the OR-set R7, #129) --------

    /// A ctx that maps remote labels 7 and 8 on the fixture board to the
    /// given local ids.
    fn label_ctx(x: LabelId, y: LabelId) -> RemoteIndex {
        let mut ctx = RemoteIndex::default();
        ctx.label_by_ref.insert(
            crate::ids::RemoteLabelRef {
                board: board(REMOTE_BOARD_NUM),
                label: RemoteLabelId(7),
            },
            x,
        );
        ctx.label_by_ref.insert(
            crate::ids::RemoteLabelRef {
                board: board(REMOTE_BOARD_NUM),
                label: RemoteLabelId(8),
            },
            y,
        );
        ctx
    }

    fn label_id_n(n: u128) -> LabelId {
        LabelId::from(uuid::Uuid::from_u128(n))
    }

    #[test]
    fn concurrent_two_device_label_adds_both_survive() {
        let mut previous = remote_task(1, 1, BASE + 180);
        previous.labels = BTreeSet::from([RemoteLabelId(7)]);
        let mut local = bound_task(1);
        local.label_memberships.insert(
            label_id_n(600),
            crate::entities::MembershipState::Added { at: ts(BASE + 200) },
        );
        let local = with_baseline(local, &previous);

        let ctx = label_ctx(label_id_n(600), label_id_n(601));
        // The remote (another device) attached label 8 alongside 7.
        let mut remote = remote_task(1, 1, BASE + 360);
        remote.labels = BTreeSet::from([RemoteLabelId(7), RemoteLabelId(8)]);

        let merged = merge_task(&local, &remote, &ctx, true);

        assert_eq!(
            merged.live_labels(),
            BTreeSet::from([label_id_n(600), label_id_n(601)]),
            "both concurrent adds survive"
        );
        assert_eq!(
            merged.label_memberships[&label_id_n(600)],
            crate::entities::MembershipState::Added { at: ts(BASE + 200) },
            "the local add sees no event (7 equals the baseline) and keeps its stamp"
        );
        assert_eq!(
            merged.label_memberships[&label_id_n(601)],
            crate::entities::MembershipState::Added { at: ts(BASE + 360) },
            "the observed remote add lands stamped ts_r"
        );
    }

    #[test]
    fn a_remote_label_removal_wins_only_when_strictly_newer() {
        let mut previous = remote_task(1, 1, BASE + 180);
        previous.labels = BTreeSet::from([RemoteLabelId(7)]);
        let ctx = label_ctx(label_id_n(600), label_id_n(601));
        let mut remote = remote_task(1, 1, BASE + 360);
        remote.labels = BTreeSet::new();

        // Local add older than the remote removal: removed.
        let mut local = bound_task(1);
        local.label_memberships.insert(
            label_id_n(600),
            crate::entities::MembershipState::Added { at: ts(BASE + 200) },
        );
        let local = with_baseline(local, &previous);
        let merged = merge_task(&local, &remote, &ctx, true);
        assert_eq!(
            merged.label_memberships[&label_id_n(600)],
            crate::entities::MembershipState::Removed { at: ts(BASE + 360) }
        );

        // Local add newer than the removal: the add stands, stamp kept.
        let mut local = bound_task(1);
        local.label_memberships.insert(
            label_id_n(600),
            crate::entities::MembershipState::Added { at: ts(BASE + 500) },
        );
        let local = with_baseline(local, &previous);
        let merged = merge_task(&local, &remote, &ctx, true);
        assert_eq!(
            merged.label_memberships[&label_id_n(600)],
            crate::entities::MembershipState::Added { at: ts(BASE + 500) }
        );
    }

    // ---- the echo translation (R9a/R9b labels, #129) ------------------

    #[test]
    fn the_echo_restamps_present_ids_and_tombstones_missing_ones() {
        let ctx = RemoteIndex::default();
        let mut local = bound_task(1);
        // X: added locally at 100 — echo-present, restamped to lm.
        // Y: added locally at 200 — missing from the echo: the server
        //    confirmed the removal. Tombstoned at max(200, lm).
        // Z: removed locally at 150 — missing from the echo: the
        //    tombstone stands with the advanced clock.
        local.label_memberships.insert(
            label_id_n(600),
            crate::entities::MembershipState::Added { at: ts(BASE + 100) },
        );
        local.label_memberships.insert(
            label_id_n(601),
            crate::entities::MembershipState::Added { at: ts(BASE + 200) },
        );
        local.label_memberships.insert(
            label_id_n(602),
            crate::entities::MembershipState::Removed { at: ts(BASE + 150) },
        );

        // An echo with no labels at all: only X's restamp is observable
        // in the live set; every entry is missing and advances to lm.
        let echo = remote_task(1, 1, BASE + 360);
        let adopted = adopt_task_after_push(&local, &echo, &ctx, true);
        assert!(
            adopted.live_labels().is_empty(),
            "X's confirmation removed it"
        );
        for id in [600, 601, 602] {
            assert_eq!(
                adopted.label_memberships[&label_id_n(id)],
                crate::entities::MembershipState::Removed { at: ts(BASE + 360) },
                "missing ids are tombstoned at max(local stamp, lm), never dropped"
            );
        }

        // An echo that still lists a locally-removed id (cache-lag or
        // another device's add riding our push): the outcome table
        // decides — lm >= the tombstone's stamp lands the re-add.
        let mut echo = remote_task(1, 1, BASE + 360);
        let mut ctx = ctx;
        ctx.label_by_ref.insert(
            crate::ids::RemoteLabelRef {
                board: board(REMOTE_BOARD_NUM),
                label: RemoteLabelId(7),
            },
            label_id_n(602),
        );
        echo.labels = BTreeSet::from([RemoteLabelId(7)]);
        let adopted = adopt_task_after_push(&local, &echo, &ctx, true);
        assert_eq!(
            adopted.label_memberships[&label_id_n(602)],
            crate::entities::MembershipState::Added { at: ts(BASE + 360) },
            "an echo presence at lm >= the tombstone re-adds"
        );
    }

    #[test]
    fn the_echo_round_trip_converges_after_a_cache_lagged_pull() {
        // assign → echo → unchanged pull ⇒ no spurious events; then the
        // unassign path: unassign → cache-lagged echo (label still
        // present, stale stamp) → next pull's fresh ts_r converges to
        // removed (the design §5 drift-mitigation property, labels ed.).
        let mut ctx = RemoteIndex::default();
        ctx.label_by_ref.insert(
            crate::ids::RemoteLabelRef {
                board: board(REMOTE_BOARD_NUM),
                label: RemoteLabelId(7),
            },
            label_id_n(600),
        );
        let mut local = bound_task(1);
        local.label_memberships.insert(
            label_id_n(600),
            crate::entities::MembershipState::Added { at: ts(BASE) },
        );

        let mut echo = remote_task(1, 1, BASE + 60);
        echo.labels = BTreeSet::from([RemoteLabelId(7)]);
        let adopted = adopt_task_after_push(&local, &echo, &ctx, true);
        assert_eq!(
            adopted.label_memberships[&label_id_n(600)],
            crate::entities::MembershipState::Added { at: ts(BASE + 60) }
        );

        // The next pull shows exactly the echoed set: no event fires.
        let remote2 = echo.clone();
        let merged = merge_task(&adopted, &remote2, &ctx, true);
        assert_eq!(
            merged.label_memberships[&label_id_n(600)],
            crate::entities::MembershipState::Added { at: ts(BASE + 60) }
        );

        // Local unassign at 100; the push's echo is cache-lagged (still
        // lists the label with a stale BASE+90 stamp, older than the
        // tombstone): the table keeps the removal.
        let mut unassigned = adopted.clone();
        unassigned.label_memberships.insert(
            label_id_n(600),
            crate::entities::MembershipState::Removed { at: ts(BASE + 100) },
        );
        let mut lagged = echo.clone();
        lagged.last_modified = ts(BASE + 90);
        let after_lag = adopt_task_after_push(&unassigned, &lagged, &ctx, true);
        assert_eq!(
            after_lag.label_memberships[&label_id_n(600)],
            crate::entities::MembershipState::Removed { at: ts(BASE + 100) },
            "the stale echo cannot resurrect behind the tombstone"
        );

        // The next pull's fresh snapshot confirms the removal with a
        // newer ts_r: the baseline diff emits the removal event and the
        // tombstone advances — converged.
        let mut confirmed = echo.clone();
        confirmed.last_modified = ts(BASE + 120);
        confirmed.labels = BTreeSet::new();
        let converged = merge_task(&after_lag, &confirmed, &ctx, true);
        assert_eq!(
            converged.label_memberships[&label_id_n(600)],
            crate::entities::MembershipState::Removed { at: ts(BASE + 120) }
        );
    }

    #[test]
    fn adopted_tasks_carry_added_membership_maps() {
        let mut remote = remote_task(1, 1, BASE + 300);
        let mut ctx = RemoteIndex::default();
        ctx.stack_by_ref
            .insert(stack_ref(1), StackId::from(uuid::Uuid::from_u128(900)));
        ctx.label_by_ref.insert(
            crate::ids::RemoteLabelRef {
                board: board(REMOTE_BOARD_NUM),
                label: RemoteLabelId(5),
            },
            label_id_n(700),
        );
        remote.labels = BTreeSet::from([RemoteLabelId(5)]);

        let adopted = adopt_remote_task(&remote, TaskId::from(uuid::Uuid::from_u128(77)), &ctx)
            .expect("stack resolves");
        assert_eq!(
            adopted.label_memberships[&label_id_n(700)],
            crate::entities::MembershipState::Added { at: ts(BASE + 300) },
            "a fresh adoption stamps every mapped id with the entity stamp"
        );
    }

    #[test]
    fn the_missing_baseline_fallback_reproduces_the_wholesale_replacement() {
        // No baseline: the live set after the cycle equals the mapped
        // remote set exactly; displaced members are tombstoned, never
        // dropped; existing tombstones stay.
        let mut local = bound_task(1);
        local.label_memberships.insert(
            label_id_n(600),
            crate::entities::MembershipState::Added { at: ts(BASE + 100) },
        );
        local.label_memberships.insert(
            label_id_n(602),
            crate::entities::MembershipState::Added { at: ts(BASE + 200) },
        );
        local.label_memberships.insert(
            label_id_n(603),
            crate::entities::MembershipState::Removed { at: ts(BASE + 150) },
        );

        let ctx = label_ctx(label_id_n(600), label_id_n(601));
        let mut remote = remote_task(1, 1, BASE + 360);
        remote.labels = BTreeSet::from([RemoteLabelId(7), RemoteLabelId(8)]);

        let merged = merge_task(&local, &remote, &ctx, true);

        assert_eq!(
            merged.live_labels(),
            BTreeSet::from([label_id_n(600), label_id_n(601)]),
            "the live set is the mapped remote set"
        );
        assert_eq!(
            merged.label_memberships[&label_id_n(602)],
            crate::entities::MembershipState::Removed { at: ts(BASE + 360) },
            "the displaced member is remembered, not dropped"
        );
        assert_eq!(
            merged.label_memberships[&label_id_n(603)],
            crate::entities::MembershipState::Removed { at: ts(BASE + 150) },
            "an existing tombstone keeps its older stamp"
        );
    }

    #[test]
    fn an_unmapped_remote_add_creates_no_membership() {
        let mut previous = remote_task(1, 1, BASE + 180);
        previous.labels = BTreeSet::new();
        let local = with_baseline(bound_task(1), &previous);

        let mut remote = remote_task(1, 1, BASE + 360);
        remote.labels = BTreeSet::from([RemoteLabelId(42)]); // unmapped

        let merged = merge_task(&local, &remote, &RemoteIndex::default(), true);
        assert!(
            merged.label_memberships.is_empty(),
            "unmapped adds are skipped (dominance in its quiet form)"
        );
    }

    #[test]
    fn the_scheduled_lane_gate_stays_conjunctive_with_the_diff() {
        let mut previous = remote_task(1, 1, BASE + 180);
        previous.scheduled = Some(ts(BASE - 120));
        let mut local = bound_task(1);
        local.scheduled = Some(ts(BASE + 100)); // a native local edit
        local.clocks.scheduled = ts(BASE + 300);
        let local = with_baseline(local, &previous);

        let ctx = RemoteIndex::default();
        let mut remote = remote_task(1, 1, BASE + 360);
        remote.scheduled = Some(ts(BASE - 120)); // equals the baseline: unchanged

        // Lane open, field unchanged: nothing applies.
        let merged = merge_task(&local, &remote, &ctx, true);
        assert_eq!(merged.scheduled, Some(ts(BASE + 100)));
        assert_eq!(merged.clocks.scheduled, ts(BASE + 300));

        // Lane open, field changed, stamp newer: applies.
        let mut changed = remote_task(1, 1, BASE + 360);
        changed.scheduled = Some(ts(BASE + 90));
        let merged = merge_task(&local, &changed, &ctx, true);
        assert_eq!(merged.scheduled, Some(ts(BASE + 90)));
        assert_eq!(merged.clocks.scheduled, ts(BASE + 360));

        // Lane closed (D-V2): even a changed value never applies.
        let merged = merge_task(&local, &changed, &ctx, false);
        assert_eq!(merged.scheduled, Some(ts(BASE + 100)));
        assert_eq!(merged.clocks.scheduled, ts(BASE + 300));
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
        assert_eq!(adopted.live_labels(), BTreeSet::from([label_id]));
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
            insistence: Insistence::Normal,
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
                    insistence: Insistence::Normal,
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
                    insistence: Insistence::Normal,
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
    fn rules_fold_carries_insistence() {
        // Adopt: an unknown uuid arms with the document's level.
        let mut rules = std::collections::BTreeMap::new();
        let adopted_id = crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(11));
        let entry = rules_entry(&[(
            "00000000-0000-0000-0000-00000000000b",
            RuleEntry::Rule {
                insistence: Insistence::Alarm,
                offset: crate::reminders::ReminderOffset::at_due(),
                created: ts(BASE + 400),
                clock: ts(BASE + 400),
            },
        )]);
        let task = TaskId::from(uuid::Uuid::from_u128(1));
        let changes = fold_reminder_rules(&mut rules, task, &entry, false);
        assert_eq!(changes.upserts.len(), 1);
        assert_eq!(rules[&adopted_id].insistence, Insistence::Alarm);
        // Clock-gated update: a newer document entry overwrites the
        // local row's level.
        let mut local = rule_with_clock(BASE + 100);
        local.insistence = Insistence::Normal;
        let local_id = local.id;
        let mut rules = std::collections::BTreeMap::new();
        rules.insert(local_id, local);
        let newer = rules_entry(&[(
            "00000000-0000-0000-0000-00000000000a",
            RuleEntry::Rule {
                insistence: Insistence::Alarm,
                offset: crate::reminders::ReminderOffset::at_due(),
                created: ts(BASE + 100),
                clock: ts(BASE + 500),
            },
        )]);
        let changes = fold_reminder_rules(&mut rules, task, &newer, false);
        assert_eq!(changes.upserts.len(), 1);
        assert_eq!(rules[&local_id].insistence, Insistence::Alarm);
        // Clock-older: the local level stands.
        let mut local = rule_with_clock(BASE + 600);
        local.insistence = Insistence::Alarm;
        let local_id = local.id;
        let mut rules = std::collections::BTreeMap::new();
        rules.insert(local_id, local);
        let older = rules_entry(&[(
            "00000000-0000-0000-0000-00000000000a",
            RuleEntry::Rule {
                insistence: Insistence::Normal,
                offset: crate::reminders::ReminderOffset::at_due(),
                created: ts(BASE + 100),
                clock: ts(BASE + 500),
            },
        )]);
        let changes = fold_reminder_rules(&mut rules, task, &older, false);
        assert_eq!(changes.upserts, Vec::new());
        assert_eq!(rules[&local_id].insistence, Insistence::Alarm);
    }

    #[test]
    fn rules_fold_lets_a_newer_local_rule_stand() {
        let mut rules = std::collections::BTreeMap::new();
        let existing = rule_with_clock(BASE + 600);
        rules.insert(existing.id, existing.clone());
        let entry = rules_entry(&[(
            "00000000-0000-0000-0000-00000000000a",
            RuleEntry::Rule {
                insistence: Insistence::Normal,
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
                    insistence: Insistence::Normal,
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
                    insistence: Insistence::Normal,
                    offset: crate::reminders::ReminderOffset::at_due(),
                    created: ts(BASE + 100),
                    clock: ts(BASE + 900),
                },
            ),
            (
                "00000000-0000-0000-0000-00000000000b",
                RuleEntry::Rule {
                    insistence: Insistence::Normal,
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
        fn membership_merge_is_commutative_and_idempotent(
            l in crate::test_support::membership_state_strategy(),
            r in crate::test_support::membership_state_strategy(),
        ) {
            prop_assert_eq!(crate::merge::merge_membership(l, r), crate::merge::merge_membership(r, l), "commutative");
            prop_assert_eq!(crate::merge::merge_membership(l, l), l, "idempotent");
        }

        #[test]
        fn a_removal_lands_exactly_when_strictly_newer(
            add_secs in 0_i64..10_000,
            remove_secs in 0_i64..10_000,
        ) {
            let added = crate::entities::MembershipState::Added { at: ts(add_secs) };
            let event = crate::entities::MembershipState::Removed { at: ts(remove_secs) };
            let merged = crate::merge::merge_membership(added, event);
            let removed = matches!(merged, crate::entities::MembershipState::Removed { .. });
            prop_assert_eq!(removed, remove_secs > add_secs);
        }

        #[test]
        fn a_stale_re_add_cannot_resurrect_behind_a_tombstone(
            remove_secs in 0_i64..10_000,
            add_secs in 0_i64..10_000,
            events in 2..6usize,
        ) {
            proptest::prop_assume!(add_secs < remove_secs, "the re-add must be stale");
            let mut state = crate::entities::MembershipState::Removed { at: ts(remove_secs) };
            for _ in 0..events {
                state = crate::merge::merge_membership(
                    state,
                    crate::entities::MembershipState::Added { at: ts(add_secs) },
                );
                let resurrected =
                    matches!(state, crate::entities::MembershipState::Added { .. });
                prop_assert!(!resurrected, "every stale re-add stays behind the tombstone");
            }
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

    // ---- chunk B: the dismissals fold (design D-4/D-9) ----------------

    use crate::appdata::{DismissalEntry, DismissalsEntry, dismissal_wire_key};
    use crate::reminders::{ReminderDismissal, ReminderRef};

    fn dismissals_entry(entries: &[(&str, DismissalEntry)]) -> DismissalsEntry {
        DismissalsEntry {
            dismissals: entries
                .iter()
                .map(|(k, e)| ((*k).to_owned(), e.clone()))
                .collect(),
            unknown: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn dismissals_fold_adopts_rows_and_replays_are_no_ops() {
        let mut dismissals = BTreeSet::new();
        let key = dismissal_wire_key(
            crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(0xa)),
            ts(BASE + 100),
        );
        let entry = dismissals_entry(&[(
            &key,
            DismissalEntry::Dismissed {
                dismissed_at: ts(BASE + 110),
            },
        )]);
        let folded = fold_reminder_dismissals(&mut dismissals, &entry);
        assert_eq!(folded.len(), 1);
        assert_eq!(
            folded[0],
            ReminderDismissal {
                reminder: ReminderRef::Rule(crate::ids::ReminderRuleId::from(
                    uuid::Uuid::from_u128(0xa)
                )),
                occurrence: ts(BASE + 100),
                dismissed_at: ts(BASE + 110),
            }
        );
        // The replayed row (the next cycle re-folds the same document)
        // is the accepted no-op.
        let replayed = fold_reminder_dismissals(&mut dismissals, &entry);
        assert_eq!(replayed, Vec::new());
        assert_eq!(dismissals.len(), 1);
    }

    #[test]
    fn dismissals_fold_ignores_verbatim_and_unmodeled_keys() {
        let mut dismissals = BTreeSet::new();
        let entry = dismissals_entry(&[
            (
                "default:00000000-0000-0000-0000-000000000009|2026-04-01T08:00:00+00:00",
                DismissalEntry::Dismissed {
                    dismissed_at: ts(BASE + 1),
                },
            ),
            (
                "alarm:00000000-0000-0000-0000-000000000009|2026-04-01T08:00:00+00:00",
                DismissalEntry::Dismissed {
                    dismissed_at: ts(BASE + 2),
                },
            ),
            (
                "future-shape",
                DismissalEntry::Verbatim(serde_json::json!(1)),
            ),
            (
                "garbage",
                DismissalEntry::Dismissed {
                    dismissed_at: ts(BASE),
                },
            ),
        ]);
        let folded = fold_reminder_dismissals(&mut dismissals, &entry);
        assert_eq!(folded, Vec::new());
        assert!(dismissals.is_empty());
    }

    // ---- DR-4/S: the skew interleaving pins (D-4b, #133) ---------------

    /// S1 — audit skew interleaving (i)-residual, the echo shadow. Real
    /// time: the local title edit lands at 10:05:00, the push echo comes
    /// back from a server whose clock is 30 s fast and stamps BASE+330
    /// for a BASE+300 edit, a second local edit lands at 10:05:10, and a
    /// web edit that *really happened at 10:05:05* — before that second
    /// edit — is stamped BASE+335 by the fast server. Stamp order is the
    /// only order the algebra sees, so the real-time-earlier web write
    /// wins the second edit's field. Accepted-by-design loss class (audit
    /// F11-02, P2): D-4c's Δ-band guard does *not* cover this interleaving
    /// (it arbitrates only the resurrect comparisons), so this residue
    /// survives even a triggered guard — permanent debt. Self-healing
    /// bound: the next local title edit re-wins (audit §skew (b)).
    /// `currently_pins_`: a changed policy must consciously rename, not
    /// silently pass.
    #[test]
    fn remote_stamp_beats_newer_local_edit_currently_pins_skew_echo_shadow_residual() {
        let ctx = RemoteIndex::default();

        // First local edit at real 10:05:00.
        let mut edited = bound_task(1);
        edited.title = "edited once".into();
        edited.clocks.title = ts(BASE + 300);

        // The push echo: the 30 s-fast server stamps BASE + 330; echo
        // adoption inflates the title clock past the real horizon — C4's
        // `max`, the skew window's opening.
        let echo = remote_task(1, 1, BASE + 330);
        let after_echo = adopt_task_after_push(&edited, &echo, &ctx, true);
        assert_eq!(after_echo.clocks.title, ts(BASE + 330));

        // Second local edit at real 10:05:10: the engine stamps its
        // honest `now` (BASE + 310), below the echo-inflated horizon —
        // local commands stamp unconditionally, no max.
        let mut edited_twice = after_echo;
        edited_twice.title = "edited twice".into();
        edited_twice.clocks.title = ts(BASE + 310);

        // A web edit really made at 10:05:05 (before the second edit),
        // stamped BASE + 335 by the 30 s-fast server. A distinct title so
        // the winner's provenance is unambiguous.
        let mut remote = remote_task(1, 1, BASE + 335);
        remote.title = "edited on the web".into();

        let merged = merge_task(&edited_twice, &remote, &ctx, true);
        assert_eq!(merged.title, "edited on the web");
        assert_eq!(merged.clocks.title, ts(BASE + 335));
    }

    /// S2 — audit skew interleaving (ii), the resurrect flip, both
    /// directions. The R5 comparison (`ts_r > clocks.deleted`) is correct
    /// given honest clocks — `newer_remote_edit_resurrects_local_tombstone`
    /// / `older_remote_edit_keeps_tombstone` pin exactly that — so this
    /// pin shows the same comparison under lying stamps; it doubles as
    /// the acceptance oracle D-4c's Δ-band guard (act A's direction only)
    /// would be measured against.
    #[test]
    fn resurrect_flip_under_skewed_stamps_currently_pins_skew_resurrect_flip() {
        let ctx = RemoteIndex::default();

        // Act A — server fast ⇒ wrongly resurrects. Real time: the card
        // is deleted at 10:06:00 (honest engine stamp, C3) and a remote
        // write really happened at 10:05:50 — *before* the delete; the
        // 30 s-fast server stamps it BASE + 380 (10:06:20), past the
        // tombstone. The resurrect then stamps the tombstone clock with
        // the lying value, compounding the skew for the next comparison
        // (pipeline-side, the pending DeleteTask re-push is dropped —
        // out of scope here).
        let mut deleted = bound_task(1);
        deleted.deleted = true;
        deleted.clocks.deleted = ts(BASE + 360);
        let remote = remote_task(1, 1, BASE + 380);
        let merged = merge_task(&deleted, &remote, &ctx, true);
        assert!(!merged.deleted, "the server-fast stamp resurrects");
        assert_eq!(merged.clocks.deleted, ts(BASE + 380));
        assert_eq!(merged.remote, Some(card_ref(1, 1)));

        // Act B — local fast ⇒ stale tombstone shields itself. The local
        // clock is 60 s fast: the real 10:06:00 delete is stamped
        // 10:07:00. A genuinely fresher remote edit at real 10:06:30
        // carries the honest server stamp BASE + 390 — below the lying
        // tombstone clock, so the delete stands even though the remote
        // change is newer in real time. Bounded self-healing: the change
        // is observed and can land once server stamps pass 10:07:00.
        let mut deleted = bound_task(1);
        deleted.deleted = true;
        deleted.clocks.deleted = ts(BASE + 420);
        let remote = remote_task(1, 1, BASE + 390);
        let merged = merge_task(&deleted, &remote, &ctx, true);
        assert!(merged.deleted, "the local-fast tombstone shields itself");
        assert_eq!(merged.clocks.deleted, ts(BASE + 420));
        assert_eq!(merged.remote_seen, Some(ts(BASE + 390)));
    }

    /// S3 — audit skew interleaving (iii), the seconds tie. Contract, not
    /// debt: ADR 0004's explicit tie-break (strict `>` in `remote_wins`,
    /// ties keep local) interacting with Deck's documented whole-second
    /// wire resolution — the push truncates the engine's sub-second
    /// `Utc::now()`. Real time: local edit at 10:05:30.400, remote write
    /// genuinely later at 10:05:30.900, truncated to the same wire stamp
    /// ⇒ tie ⇒ local stands. The direction is consistently local-favoring
    /// (safe-leaning); changing it is a deliberate ADR amendment, not
    /// tolerated debt.
    #[test]
    fn same_second_remote_write_loses_to_sub_second_local_edit() {
        let ctx = RemoteIndex::default();

        let mut local = bound_task(1);
        local.title = "edited locally".into();
        local.clocks.title = ts(BASE + 330) + chrono::TimeDelta::try_milliseconds(400).unwrap();

        // The remote write at 10:05:30.900, wire-truncated to 10:05:30.
        let remote = remote_task(1, 1, BASE + 330);

        let merged = merge_task(&local, &remote, &ctx, true);
        assert_eq!(merged.title, "edited locally");
        assert_eq!(
            merged.clocks.title,
            ts(BASE + 330) + chrono::TimeDelta::try_milliseconds(400).unwrap()
        );
    }

    // ---- the remote baseline: blind-spot pin and property catalogue ----

    use crate::entities::RemoteTaskBaseline;

    /// The one field the gate cannot see (D-1d): a remote write that
    /// exactly restores the baseline value is indistinguishable from "no
    /// change" and does not land while a local edit holds the field.
    /// Deterministic and bounded — the next genuine remote change to the
    /// field lands it — and accepted as residue in ADR 0004's amendment.
    /// If the policy ever changes, this pin must be consciously renamed.
    #[test]
    fn remote_revert_to_the_baseline_value_stays_hidden_currently_pins_baseline_revert_blind_spot()
    {
        let mut local = bound_task(1);
        local.title = "edited locally".into();
        local.clocks.title = ts(BASE + 330); // 10:05:30
        let previous = remote_task(1, 1, BASE + 180);
        let local = with_baseline(local, &previous);

        // The remote reverts to exactly the baseline's title, newest stamp.
        let mut remote = remote_task(1, 1, BASE + 360);
        remote.title = previous.title.clone();

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx, true);

        assert_eq!(merged.title, "edited locally", "the revert does not land");
        assert_eq!(merged.clocks.title, ts(BASE + 330));
        // The observation still happened.
        assert_eq!(merged.remote_seen, Some(ts(BASE + 360)));
        assert_eq!(merged.remote_baseline, Some(baseline_of(&remote)));
    }

    /// A `RemoteTask` standing in for the incoming snapshot: fields
    /// derived from a baseline (the untouched subset copies it verbatim),
    /// plus the bookkeeping the baseline does not carry.
    #[allow(clippy::too_many_arguments)] // one argument per perturbed field
    fn remote_from_baseline(
        baseline: &RemoteTaskBaseline,
        touched: &Touched,
        title: String,
        description: String,
        duedate: Option<DateTime<Utc>>,
        scheduled: Option<DateTime<Utc>>,
        done: Option<DateTime<Utc>>,
        order: i64,
        label_num: u64,
        archived: bool,
        stack_num: u64,
        stamp_secs: i64,
        card: u64,
    ) -> RemoteTask {
        let (t, d, du, sc, dn, o, labels, arc, stack) = if touched.nothing {
            (
                baseline.title.clone(),
                baseline.description.clone(),
                baseline.duedate,
                baseline.scheduled,
                baseline.done,
                baseline.order,
                baseline.labels.clone(),
                baseline.archived,
                baseline.stack,
            )
        } else {
            (
                if touched.title {
                    title
                } else {
                    baseline.title.clone()
                },
                if touched.description {
                    description
                } else {
                    baseline.description.clone()
                },
                if touched.duedate {
                    duedate
                } else {
                    baseline.duedate
                },
                if touched.scheduled {
                    scheduled
                } else {
                    baseline.scheduled
                },
                if touched.done { done } else { baseline.done },
                if touched.position {
                    order
                } else {
                    baseline.order
                },
                if touched.labels {
                    BTreeSet::from([crate::ids::RemoteLabelId(label_num)])
                } else {
                    baseline.labels.clone()
                },
                if touched.archived {
                    archived
                } else {
                    baseline.archived
                },
                if touched.position {
                    crate::ids::RemoteStackId(stack_num)
                } else {
                    baseline.stack
                },
            )
        };
        RemoteTask {
            id: card_ref(1, card),
            title: t,
            description: d,
            duedate: du,
            scheduled: sc,
            done: dn,
            stack,
            order: o,
            labels,
            archived: arc,
            last_modified: ts(stamp_secs),
            created_at: ts(stamp_secs),
        }
    }

    /// Which baseline-covered fields the generated remote perturbed.
    /// The bool fields ARE the data — one flag per covered field.
    #[allow(clippy::struct_excessive_bools)]
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    struct Touched {
        nothing: bool,
        title: bool,
        description: bool,
        duedate: bool,
        scheduled: bool,
        done: bool,
        position: bool,
        labels: bool,
        archived: bool,
    }

    /// P-G1's assertion shape, shared with P-G3's perturbation variant:
    /// outside `touched` the merge keeps local values AND local clocks;
    /// inside, the remote value lands iff it wins the field's LWW
    /// comparison (scheduled additionally behind its lane gate); the
    /// observation facts advance unconditionally; no clock decreases.
    #[allow(clippy::too_many_arguments)]
    fn assert_baseline_gate(
        local: &Task,
        remote: &RemoteTask,
        merged: &Task,
        touched: Touched,
        native_scheduled: bool,
    ) {
        let ts_r = remote.last_modified;
        let wins = |clock: chrono::DateTime<Utc>| ts_r > clock;

        let (title_wins, description_wins) = if touched.nothing {
            (false, false)
        } else {
            (touched.title, touched.description)
        };
        // Outside the touched set (or untouched): local value + clock.
        if !title_wins || !wins(local.clocks.title) {
            assert_eq!(&merged.title, &local.title);
            assert_eq!(merged.clocks.title, local.clocks.title);
        } else {
            assert_eq!(&merged.title, &remote.title);
            assert_eq!(merged.clocks.title, ts_r);
        }
        if !description_wins || !wins(local.clocks.description) {
            assert_eq!(&merged.description, &local.description);
            assert_eq!(merged.clocks.description, local.clocks.description);
        } else {
            assert_eq!(&merged.description, &remote.description);
            assert_eq!(merged.clocks.description, ts_r);
        }
        let duedate_touched = !touched.nothing && touched.duedate;
        if !duedate_touched {
            assert_eq!(merged.duedate, local.duedate);
            assert_eq!(merged.clocks.duedate, local.clocks.duedate);
        } else if wins(local.clocks.duedate) {
            assert_eq!(merged.duedate, remote.duedate);
            assert_eq!(merged.clocks.duedate, ts_r);
        } else {
            assert_eq!(merged.duedate, local.duedate);
            assert_eq!(merged.clocks.duedate, local.clocks.duedate);
        }
        let scheduled_touched = !touched.nothing && touched.scheduled;
        if !scheduled_touched || !native_scheduled || !wins(local.clocks.scheduled) {
            assert_eq!(merged.scheduled, local.scheduled);
            assert_eq!(merged.clocks.scheduled, local.clocks.scheduled);
        } else {
            assert_eq!(merged.scheduled, remote.scheduled);
            assert_eq!(merged.clocks.scheduled, ts_r);
        }
        let done_touched = !touched.nothing && touched.done;
        if !done_touched {
            assert_eq!(merged.done, local.done);
            assert_eq!(merged.clocks.done, local.clocks.done);
        } else if wins(local.clocks.done) {
            assert_eq!(merged.done, remote.done);
            assert_eq!(merged.clocks.done, ts_r);
        } else {
            assert_eq!(merged.done, local.done);
            assert_eq!(merged.clocks.done, local.clocks.done);
        }
        // The composite keeps local as a unit either way: untouched, or
        // touched-but-suppressed because the property's ctx maps no stack
        // (R8's unit semantics dominate in both branches).
        assert_eq!(merged.stack, local.stack);
        assert_eq!(merged.order, local.order);
        assert_eq!(merged.clocks.position, local.clocks.position);
        // Labels are per-membership OR-set state: an unchanged remote set
        // emits no events; a changed set diffs against the baseline in
        // remote vocabulary and maps each id through the property's ctx
        // — which resolves nothing, so no event applies and the map is
        // byte-identical either way (unmapped ids are skipped, dominance
        // in its quiet form).
        assert_eq!(&merged.label_memberships, &local.label_memberships);
        let archived_touched = !touched.nothing && touched.archived;
        if !archived_touched {
            assert_eq!(merged.archived, local.archived);
            assert_eq!(merged.clocks.archived, local.clocks.archived);
        } else if wins(local.clocks.archived) {
            assert_eq!(merged.archived, remote.archived);
            assert_eq!(merged.clocks.archived, ts_r);
        } else {
            assert_eq!(merged.archived, local.archived);
            assert_eq!(merged.clocks.archived, local.clocks.archived);
        }

        // The observation facts advance unconditionally.
        assert_eq!(merged.remote_seen, Some(ts_r));
        assert_eq!(merged.remote_baseline, Some(RemoteTaskBaseline::of(remote)));

        // An unapplied observation advances nothing: no clock decreases.
        assert!(merged.clocks.title >= local.clocks.title);
        assert!(merged.clocks.description >= local.clocks.description);
        assert!(merged.clocks.duedate >= local.clocks.duedate);
        assert!(merged.clocks.scheduled >= local.clocks.scheduled);
        assert!(merged.clocks.done >= local.clocks.done);
        assert!(merged.clocks.position >= local.clocks.position);
        assert!(merged.clocks.archived >= local.clocks.archived);
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        /// P-G1 — the acceptance property: fields outside the diff's
        /// touched set keep local values AND clocks; fields inside apply
        /// exactly under the (conjunctive) gate.
        #[test]
        fn baseline_gate_is_per_field_anti_clobber(
            local in crate::test_support::task_strategy(),
            baseline in crate::test_support::remote_task_baseline_strategy(),
            touched_title in proptest::bool::ANY,
            touched_description in proptest::bool::ANY,
            touched_duedate in proptest::bool::ANY,
            touched_scheduled in proptest::bool::ANY,
            touched_done in proptest::bool::ANY,
            touched_position in proptest::bool::ANY,
            touched_labels in proptest::bool::ANY,
            touched_archived in proptest::bool::ANY,
            title in "[a-z]{0,16}",
            description in "[a-z]{0,16}",
            duedate in proptest::option::of(0_i64..10_000),
            scheduled in proptest::option::of(0_i64..10_000),
            done in proptest::option::of(0_i64..10_000),
            order in -1_000_i64..=1_000,
            label_num in 1_u64..100,
            archived in proptest::bool::ANY,
            stack_num in 1_u64..100,
            stamp_secs in 0_i64..10_000,
            native_scheduled in proptest::bool::ANY,
        ) {
            let touched = Touched {
                nothing: false,
                title: touched_title,
                description: touched_description,
                duedate: touched_duedate,
                scheduled: touched_scheduled,
                done: touched_done,
                position: touched_position,
                labels: touched_labels,
                archived: touched_archived,
            };
            let remote = remote_from_baseline(
                &baseline, &touched, title, description,
                duedate.map(ts), scheduled.map(ts), done.map(ts),
                order, label_num, archived, stack_num, stamp_secs,
                1,
            );
            let mut local = local;
            local.remote_baseline = Some(baseline);

            let ctx = RemoteIndex::default();
            let merged = merge_task(&local, &remote, &ctx, native_scheduled);

            assert_baseline_gate(&local, &remote, &merged, touched, native_scheduled);
        }

        /// P-G2 — the baseline path is idempotent: re-merging the merged
        /// task with the same snapshot is a fixpoint.
        #[test]
        fn baseline_merge_is_idempotent(
            local in crate::test_support::task_strategy(),
            baseline in crate::test_support::remote_task_baseline_strategy(),
            touched_title in proptest::bool::ANY,
            touched_description in proptest::bool::ANY,
            touched_duedate in proptest::bool::ANY,
            touched_scheduled in proptest::bool::ANY,
            touched_done in proptest::bool::ANY,
            touched_position in proptest::bool::ANY,
            touched_labels in proptest::bool::ANY,
            touched_archived in proptest::bool::ANY,
            title in "[a-z]{0,16}",
            description in "[a-z]{0,16}",
            duedate in proptest::option::of(0_i64..10_000),
            scheduled in proptest::option::of(0_i64..10_000),
            done in proptest::option::of(0_i64..10_000),
            order in -1_000_i64..=1_000,
            label_num in 1_u64..100,
            archived in proptest::bool::ANY,
            stack_num in 1_u64..100,
            stamp_secs in 0_i64..10_000,
            native_scheduled in proptest::bool::ANY,
        ) {
            let touched = Touched {
                nothing: false,
                title: touched_title,
                description: touched_description,
                duedate: touched_duedate,
                scheduled: touched_scheduled,
                done: touched_done,
                position: touched_position,
                labels: touched_labels,
                archived: touched_archived,
            };
            let remote = remote_from_baseline(
                &baseline, &touched, title, description,
                duedate.map(ts), scheduled.map(ts), done.map(ts),
                order, label_num, archived, stack_num, stamp_secs,
                1,
            );
            let mut local = local;
            local.remote_baseline = Some(baseline);

            let ctx = RemoteIndex::default();
            let once = merge_task(&local, &remote, &ctx, native_scheduled);
            let twice = merge_task(&once, &remote, &ctx, native_scheduled);
            prop_assert_eq!(twice, once);
        }

        /// P-G3 — the echo round trip (design §5's drift risk): a pull
        /// carrying exactly the echoed values leaves no spurious diff,
        /// and a later genuine diff reads against the echo reference.
        #[test]
        fn push_echo_pull_leaves_no_spurious_diff(
            local in crate::test_support::task_strategy(),
            echo in crate::test_support::remote_task_strategy(),
            pull_secs in 0_i64..10_000,
            perturb in proptest::bool::ANY,
            perturbed_title in "[a-z]{0,16}",
            native_scheduled in proptest::bool::ANY,
        ) {
            // The echo adoption needs a mapped stack; map the echo's.
            let local_stack = crate::ids::StackId::from(uuid::Uuid::from_u128(900));
            let mut ctx = RemoteIndex::default();
            ctx.stack_by_ref
                .insert(
                    crate::ids::RemoteStackRef {
                        board: echo.id.board,
                        stack: echo.id.stack,
                    },
                    local_stack,
                );

            let adopted = adopt_task_after_push(&local, &echo, &ctx, native_scheduled);

            // The next pull shows the same values with a newer stamp.
            let mut remote2 = echo.clone();
            remote2.last_modified = ts(pull_secs);
            let touched = if perturb {
                remote2.title = perturbed_title;
                Touched { nothing: false, title: true, ..Touched::default() }
            } else {
                Touched { nothing: true, ..Touched::default() }
            };

            let merged = merge_task(&adopted, &remote2, &ctx, native_scheduled);

            assert_baseline_gate(&adopted, &remote2, &merged, touched, native_scheduled);
        }
    }
}

#[cfg(test)]
mod duplicate_reconcile_tests {
    //! R-dup's clause pins (#150, T-5/T-6/T-8): each detection clause
    //! independently, its near-miss negatives, the survivorship rules,
    //! and the structural no-op property.

    use super::*;
    use crate::entities::TaskClocks;
    use crate::merge_testutil::*;
    use proptest::prelude::*;

    use uuid::Uuid;

    fn dup_task(title: &str, bound_card: Option<u64>) -> Task {
        let id = TaskId::from(Uuid::from_u128(500));
        Task {
            id,
            remote: bound_card.map(|card| card_ref(1, card)),
            title: title.into(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack: local_stack().id,
            order: 0,
            label_memberships: BTreeMap::new(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: ts(BASE),
                description: ts(BASE),
                duedate: ts(BASE),
                scheduled: ts(BASE),
                done: ts(BASE),
                position: ts(BASE),
                effort: ts(BASE),
                importance: ts(BASE),
                urgency: ts(BASE),
                recur: ts(BASE),
                archived: ts(BASE),
                deleted: ts(BASE),
            },
            remote_seen: bound_card.map(|_| ts(BASE)),
            remote_baseline: None,
        }
    }

    fn dup_stacks() -> BTreeMap<StackId, Stack> {
        let mut stacks = BTreeMap::new();
        stacks.insert(local_stack().id, local_stack());
        stacks
    }

    fn sig(queued_at: i64, echoed: bool) -> CreateSignature {
        CreateSignature {
            task: TaskId::from(Uuid::from_u128(500)),
            op: OpId(Uuid::from_u128(600)),
            queued_at: ts(queued_at),
            echoed_this_cycle: echoed,
        }
    }

    fn candidate(stack: u64, card: u64, created_at: i64) -> RemoteTask {
        let mut task = remote_task(stack, card, created_at);
        task.id = card_ref(stack, card);
        task.title = "local draft".into();
        task.created_at = ts(created_at);
        task
    }

    fn detect(
        candidate: &RemoteTask,
        bound_card: Option<&RemoteTask>,
        creates: &[CreateSignature],
        now: i64,
    ) -> Option<DuplicateCollapse> {
        let mut tasks = BTreeMap::new();
        let local = dup_task("local draft", bound_card.map(|b| b.id.card.0));
        tasks.insert(local.id, local);
        detect_crash_window_duplicates(
            candidate,
            bound_card,
            &tasks,
            &dup_stacks(),
            creates,
            ts(now),
            true,
        )
    }

    const QUEUED: i64 = BASE;
    const NOW: i64 = BASE + 30;

    // ---- T-5a: clause 1 — same title, same resolved stack ------------

    #[test]
    fn same_title_on_a_different_stack_is_not_ours() {
        let c = candidate(2, 900, QUEUED + 10);
        assert!(detect(&c, None, &[sig(QUEUED, false)], NOW).is_none());
    }

    #[test]
    fn an_unmapped_local_stack_fails_clause_1() {
        let mut tasks = BTreeMap::new();
        let mut local = dup_task("local draft", None);
        let orphan = StackId::from(Uuid::from_u128(901));
        local.stack = orphan;
        tasks.insert(local.id, local);
        let mut stacks = BTreeMap::new();
        let mut orphan_stack = local_stack();
        orphan_stack.id = orphan;
        orphan_stack.remote = None;
        stacks.insert(orphan, orphan_stack);
        let c = candidate(1, 900, QUEUED + 10);
        assert!(
            detect_crash_window_duplicates(
                &c,
                None,
                &tasks,
                &stacks,
                &[sig(QUEUED, false)],
                ts(NOW),
                true
            )
            .is_none()
        );
    }

    // ---- T-5b: clause 2 — no create signature, no collapse -----------

    #[test]
    fn a_strangers_twin_card_without_our_create_is_adopted_not_collapsed() {
        let c = candidate(1, 900, QUEUED + 10);
        assert!(detect(&c, None, &[], NOW).is_none());
    }

    // ---- T-5c: clause 3 — the window ----------------------------------

    #[test]
    fn a_same_titled_card_created_before_the_window_is_not_ours() {
        let c = candidate(1, 900, QUEUED - 61);
        assert!(detect(&c, None, &[sig(QUEUED, false)], NOW).is_none());
    }

    #[test]
    fn a_stamp_beyond_the_skew_tolerated_upper_bound_is_not_ours() {
        let c = candidate(1, 900, NOW + 61);
        assert!(detect(&c, None, &[sig(QUEUED, false)], NOW).is_none());
    }

    #[test]
    fn a_bound_card_outside_the_window_fails_clause_3() {
        let bound = candidate(1, 800, QUEUED - 120); // the crash-window bound card would never be this old
        let c = candidate(1, 900, QUEUED + 10);
        assert!(detect(&c, Some(&bound), &[sig(QUEUED, true)], NOW).is_none());
    }

    // ---- T-5d: clause 4 — byte-ours content ---------------------------

    #[test]
    fn a_description_mismatch_rejects() {
        let mut c = candidate(1, 900, QUEUED + 10);
        c.description = "not ours".into();
        assert!(detect(&c, None, &[sig(QUEUED, false)], NOW).is_none());
    }

    #[test]
    fn a_duedate_mismatch_rejects() {
        let mut c = candidate(1, 900, QUEUED + 10);
        c.duedate = Some(ts(BASE + 9_000));
        assert!(detect(&c, None, &[sig(QUEUED, false)], NOW).is_none());
    }

    #[test]
    fn a_scheduled_mismatch_rejects_only_on_the_native_lane() {
        let mut c = candidate(1, 900, QUEUED + 10);
        c.scheduled = Some(ts(BASE + 9_000));
        // Native lane on: the create carried startdate, so a mismatch
        // is a real content difference.
        assert!(detect(&c, None, &[sig(QUEUED, false)], NOW).is_none());
        // Conservative lane off: the create never sent the field, so
        // the local value must not reject.
        let mut tasks = BTreeMap::new();
        let local = dup_task("local draft", None);
        tasks.insert(local.id, local);
        assert!(
            detect_crash_window_duplicates(
                &c,
                None,
                &tasks,
                &dup_stacks(),
                &[sig(QUEUED, false)],
                ts(NOW),
                false
            )
            .is_some()
        );
    }

    // ---- T-5e: the positive, both arms --------------------------------

    #[test]
    fn the_pending_arm_collapses_the_older_unbound_candidate() {
        let c = candidate(1, 900, QUEUED + 10);
        let collapse = detect(&c, None, &[sig(QUEUED, false)], NOW).expect("ours");
        assert_eq!(collapse.task, TaskId::from(Uuid::from_u128(500)));
        assert_eq!(collapse.survivor, c.id);
        assert!(collapse.rebind, "the unbound survivor means a rebind");
        assert!(collapse.doomed.is_empty(), "no bound card to doom");
        assert_eq!(collapse.satisfied_op, Some(OpId(Uuid::from_u128(600))));
    }

    #[test]
    fn the_echo_arm_dooms_the_bound_newer_card() {
        // Bound card B (created by the replay POST, newer), candidate A
        // (the first POST's card, older).
        let bound = candidate(1, 800, QUEUED + 20);
        let a = candidate(1, 900, QUEUED + 10);
        let collapse = detect(&a, Some(&bound), &[sig(QUEUED, true)], NOW).expect("ours");
        assert_eq!(collapse.survivor, a.id, "the older first POST survives");
        assert!(collapse.rebind);
        assert_eq!(collapse.doomed, vec![bound.id]);
        assert_eq!(
            collapse.satisfied_op, None,
            "the echo arm already completed"
        );
    }

    // ---- T-6: survivorship ---------------------------------------------

    #[test]
    fn an_exact_stamp_tie_keeps_the_bound_card() {
        let bound = candidate(1, 800, QUEUED + 10);
        let c = candidate(1, 900, QUEUED + 10);
        let collapse = detect(&c, Some(&bound), &[sig(QUEUED, true)], NOW).expect("ours");
        assert_eq!(collapse.survivor, bound.id, "tie ⇒ the bound card");
        assert!(!collapse.rebind);
        assert_eq!(collapse.doomed, vec![c.id]);
    }

    #[test]
    fn survivorship_is_order_independent() {
        let bound = candidate(1, 800, QUEUED + 10);
        let a = candidate(1, 900, QUEUED + 5);
        let b = candidate(1, 901, QUEUED + 20);
        let satisfied = Some(OpId(Uuid::from_u128(600)));
        let one = collapse_for_task(
            TaskId::from(Uuid::from_u128(500)),
            Some((bound.id, bound.created_at)),
            vec![(a.id, a.created_at), (b.id, b.created_at)],
            satisfied,
        );
        let two = collapse_for_task(
            TaskId::from(Uuid::from_u128(500)),
            Some((bound.id, bound.created_at)),
            vec![(b.id, b.created_at), (a.id, a.created_at)],
            satisfied,
        );
        assert_eq!(one, two, "arrival order must not decide survivorship");
        assert_eq!(one.survivor, a.id, "the oldest member survives");
        assert!(one.rebind);
        let mut doomed = one.doomed.clone();
        doomed.sort();
        assert_eq!(doomed, vec![bound.id, b.id]);
        assert_eq!(
            one.doomed,
            vec![b.id, bound.id],
            "doom order follows the member order"
        );
    }

    #[test]
    fn a_multi_orphan_group_collapses_to_one_survivor() {
        let bound = candidate(1, 800, QUEUED + 20);
        let a = candidate(1, 900, QUEUED + 5);
        let b = candidate(1, 901, QUEUED + 10);
        let collapse = collapse_for_task(
            TaskId::from(Uuid::from_u128(500)),
            Some((bound.id, bound.created_at)),
            vec![(a.id, a.created_at), (b.id, b.created_at)],
            Some(OpId(Uuid::from_u128(600))),
        );
        assert_eq!(collapse.survivor, a.id);
        assert_eq!(collapse.doomed.len(), 2);
        assert_eq!(collapse.satisfied_op, Some(OpId(Uuid::from_u128(600))));
    }

    // ---- the rebind helper ---------------------------------------------

    #[test]
    fn the_rebind_writes_the_binding_trio_and_nothing_else() {
        let mut local = dup_task("local draft", Some(800));
        local.title = "kept local title".into();
        let before = local.clone();
        let survivor = candidate(1, 900, QUEUED + 10);
        let rebinded = rebind_task_to_survivor(&local, &survivor);
        assert_eq!(rebinded.remote, Some(survivor.id));
        assert_eq!(rebinded.remote_seen, Some(survivor.last_modified));
        assert_eq!(
            rebinded.remote_baseline,
            Some(crate::entities::RemoteTaskBaseline::of(&survivor))
        );
        assert_eq!(rebinded.title, before.title);
        assert_eq!(rebinded.clocks, before.clocks);
        assert_eq!(rebinded.stack, before.stack);
        assert_eq!(rebinded.order, before.order);
        assert_eq!(rebinded.label_memberships, before.label_memberships);
    }

    // ---- T-8: the structural no-op + survivor total order --------------

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn an_empty_create_scope_is_a_structural_no_op(
            candidate in crate::test_support::remote_task_strategy(),
            bound in crate::test_support::remote_task_strategy(),
            now in crate::test_support::timestamp_strategy(),
        ) {
            // No signatures in scope ⇒ the pass cannot fire, whatever
            // the candidate looks like ("trigger only on observed
            // duplicate" is structural, not configured).
            let tasks = BTreeMap::new();
            let stacks = BTreeMap::new();
            prop_assert!(detect_crash_window_duplicates(
                &candidate, Some(&bound), &tasks, &stacks, &[], now, true
            )
            .is_none());
        }

        #[test]
        fn the_oldest_member_survives_and_a_tie_keeps_the_bound_card(
            s1 in crate::test_support::timestamp_strategy(),
            s2 in crate::test_support::timestamp_strategy(),
            b1 in 1_u64..1_000_000,
            b2 in 1_u64..1_000_000,
        ) {
            let bound = (card_ref(1, b1), s1);
            let cand = (card_ref(1, b2), s2);
            let collapse = collapse_for_task(
                TaskId::from(Uuid::from_u128(1)),
                Some(bound),
                vec![cand],
                None,
            );
            let expected_survivor = if (cand.1, false) < (bound.1, false) { cand.0 } else { bound.0 };
            prop_assert_eq!(collapse.survivor, expected_survivor);
            prop_assert_eq!(collapse.rebind, expected_survivor == cand.0);
            prop_assert_eq!(collapse.doomed.len(), 1);
        }
    }

    // ---- DR-5/2: the time-entries fold (D-T12/D-13) -----------------

    use crate::appdata::TimeEntryRecord;

    fn entry_row(task: &str, clock: i64) -> TimeEntryRecord {
        TimeEntryRecord::Entry {
            task: Some(task.to_owned()),
            title: "wire title".to_owned(),
            start: crate::merge_testutil::ts(clock - 1_000),
            end: None,
            created: crate::merge_testutil::ts(clock - 2_000),
            clock: crate::merge_testutil::ts(clock),
        }
    }

    #[test]
    fn fold_time_entries_imports_updates_and_tombstones_by_the_clock() {
        let mut tracking = crate::tracking::TrackingState::default();
        let local_id = crate::ids::TimeEntryId::from(uuid::Uuid::from_u128(1));
        let adjusted = crate::tracking::TimeEntry {
            id: local_id,
            task: crate::merge_testutil::bound_task(1).id,
            task_title: "local title".to_owned(),
            start: crate::merge_testutil::ts(100),
            end: Some(crate::merge_testutil::ts(200)),
            created_at: crate::merge_testutil::ts(50),
            updated_at: crate::merge_testutil::ts(BASE + 300),
        };
        tracking.entries.insert(local_id, adjusted.clone());
        let cards = std::collections::BTreeMap::from([(
            "1".to_owned(),
            crate::merge_testutil::bound_task(1).id,
        )]);
        // The adopt-then-tombstone journey uses two distinct rows in two
        // passes (one map cannot carry both for one key) — the fold call
        // below runs twice, the second seeing the tombstone map.
        let adopt_rows = std::collections::BTreeMap::from([
            // Adopt an unknown UUID.
            (
                uuid::Uuid::from_u128(2).to_string(),
                entry_row("1", BASE + 500),
            ),
            // Tie on the clock: the document wins (pull side).
            (local_id.as_uuid().to_string(), entry_row("1", BASE + 300)),
        ]);
        let rows = std::collections::BTreeMap::from([
            // A strictly newer tombstone removes the adopted row.
            (
                uuid::Uuid::from_u128(2).to_string(),
                TimeEntryRecord::Tombstone {
                    clock: crate::merge_testutil::ts(BASE + 600),
                },
            ),
            // An older tombstone leaves the re-created row standing.
            (
                local_id.as_uuid().to_string(),
                TimeEntryRecord::Tombstone {
                    clock: crate::merge_testutil::ts(BASE + 100),
                },
            ),
            // A tombstone for a locally-unknown id leaves no trace.
            (
                uuid::Uuid::from_u128(3).to_string(),
                TimeEntryRecord::Tombstone {
                    clock: crate::merge_testutil::ts(BASE + 900),
                },
            ),
            // Verbatim, null-task, and unresolvable-card rows skip.
            (
                "not-a-uuid".to_owned(),
                TimeEntryRecord::Verbatim(serde_json::json!(1)),
            ),
            (
                uuid::Uuid::from_u128(4).to_string(),
                TimeEntryRecord::Entry {
                    task: None,
                    title: "x".to_owned(),
                    start: crate::merge_testutil::ts(1),
                    end: None,
                    created: crate::merge_testutil::ts(1),
                    clock: crate::merge_testutil::ts(BASE + 900),
                },
            ),
            (
                uuid::Uuid::from_u128(5).to_string(),
                entry_row("999", BASE + 900),
            ),
        ]);
        let adopted = fold_time_entries(&mut tracking, &adopt_rows, &cards);
        assert_eq!(tracking.entries.len(), 2, "the unknown UUID adopted");
        assert_eq!(adopted.upserts.len(), 2);
        let changes = fold_time_entries(&mut tracking, &rows, &cards);
        assert_eq!(tracking.entries.len(), 1);
        let local = &tracking.entries[&local_id];
        assert_eq!(
            local.task_title, "wire title",
            "the tie went to the document"
        );
        assert_eq!(local.updated_at, crate::merge_testutil::ts(BASE + 300));
        // Deletes: only the adopted row, only by its strictly newer stone.
        assert_eq!(
            changes.deletes,
            vec![crate::ids::TimeEntryId::from(uuid::Uuid::from_u128(2))]
        );
        assert!(
            adopted.upserts.iter().any(|upsert| upsert.id == local_id),
            "the tie updated the local row"
        );
        assert!(
            changes.upserts.iter().all(|upsert| upsert.id == local_id),
            "the older tombstone and the skips report nothing"
        );
    }

    // ---- DR-6/3: the recur fold arm + the series-twin dedup ----------

    use crate::appdata::{ModeledValue, TaskEntry, TaskFieldWrite};
    use crate::pipeline::PendingTaskIntent;

    fn fw(value: ModeledValue, clock_secs: i64) -> TaskFieldWrite {
        TaskFieldWrite {
            value,
            clock: ts(clock_secs),
        }
    }

    fn recur_rule(series_seed: u128) -> crate::recur::RecurRule {
        crate::recur::RecurRule {
            series: crate::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(series_seed)),
            anchor: chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
            pattern: crate::recur::RecurPattern::Daily {
                every: crate::recur::EveryDays::new(1).unwrap(),
            },
            bound: crate::recur::RecurBound::None,
            continued_from: None,
        }
    }

    /// UTC-midnight window calendar: `ts(secs)` instants land on plain
    /// 1970 dates, so fixtures read as `(day, hour)` pairs.
    fn day_utc() -> crate::days::DayCalendar {
        crate::days::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).unwrap(),
        }
    }

    /// A twin candidate: id `id_seed` (v7 order = u128 order), the
    /// series' rule, scheduled at `day` 1970-01-02 (or dateless), open
    /// or done, live or tombstoned.
    fn twin(
        id_seed: u128,
        series_seed: u128,
        scheduled: Option<i64>,
        done: bool,
        deleted: bool,
    ) -> Task {
        let mut task = crate::merge_testutil::bound_task(0);
        task.id = TaskId::from(uuid::Uuid::from_u128(id_seed));
        task.remote = None;
        task.recur = Some(recur_rule(series_seed));
        task.scheduled = scheduled.map(ts);
        task.done = done.then(|| ts(BASE + 100));
        task.deleted = deleted;
        task
    }

    const DAY1: i64 = 86_400; // 1970-01-02, UTC
    const DAY2: i64 = 2 * 86_400;

    #[test]
    fn fold_adopts_a_newer_document_recur_rule_and_clear() {
        let mut merged = bound_task(1);
        let local = bound_task(1);
        let rule = recur_rule(7);
        let entry = TaskEntry {
            fields: [(
                "recur".to_owned(),
                fw(ModeledValue::Recur(Some(rule.clone())), BASE + 500),
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
        assert_eq!(merged.recur, Some(rule));
        assert_eq!(merged.clocks.recur, ts(BASE + 500));

        // A `null` write with a newer clock is the typed clear — an
        // ordinary write competing by the same rule.
        let local = merged.clone();
        let entry = TaskEntry {
            fields: [(
                "recur".to_owned(),
                fw(ModeledValue::Recur(None), BASE + 600),
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
        assert_eq!(merged.recur, None);
        assert_eq!(merged.clocks.recur, ts(BASE + 600));
    }

    #[test]
    fn fold_keeps_a_newer_local_recur_rule() {
        let mut local = bound_task(1);
        local.recur = Some(recur_rule(7));
        local.clocks.recur = ts(BASE + 600);
        let mut merged = local.clone();
        let entry = TaskEntry {
            fields: [(
                "recur".to_owned(),
                fw(ModeledValue::Recur(Some(recur_rule(8))), BASE + 500),
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
        assert_eq!(merged.recur, Some(recur_rule(7)));
        assert_eq!(merged.clocks.recur, ts(BASE + 600));
    }

    #[test]
    fn fold_guarded_recur_copies_local_value_and_clock() {
        let mut local = bound_task(1);
        local.recur = Some(recur_rule(7));
        local.clocks.recur = ts(BASE + 400);
        let mut merged = local.clone();
        let entry = TaskEntry {
            fields: [(
                "recur".to_owned(),
                fw(ModeledValue::Recur(Some(recur_rule(8))), BASE + 900),
            )]
            .into(),
            unmodeled: std::collections::BTreeMap::new(),
        };
        let intent = PendingTaskIntent {
            recur: true,
            ..PendingTaskIntent::default()
        };
        fold_task_app_data(&mut merged, &local, &entry, intent, false);
        assert_eq!(merged.recur, Some(recur_rule(7)));
        assert_eq!(merged.clocks.recur, ts(BASE + 400), "the clock copies too");
    }

    #[test]
    fn fold_recur_absence_never_resurrects_a_locally_cleared_rule() {
        // A truncated view (the recur key missing) writes nothing: a
        // locally cleared rule stays cleared.
        let mut local = bound_task(1);
        local.recur = Some(recur_rule(7));
        local.clocks.recur = ts(BASE + 100);
        let mut merged = local.clone();
        let entry = TaskEntry {
            fields: [(
                "effort".to_owned(),
                fw(
                    ModeledValue::Effort(Some(crate::entities::Effort::new(1))),
                    BASE + 900,
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
        assert_eq!(merged.recur, Some(recur_rule(7)));
        assert_eq!(merged.clocks.recur, ts(BASE + 100));
    }

    #[test]
    fn series_twins_same_series_same_date_collapse_older_id_survives() {
        let tasks = BTreeMap::from([
            (
                TaskId::from(uuid::Uuid::from_u128(10)),
                twin(10, 1, Some(DAY1), false, false),
            ),
            (
                TaskId::from(uuid::Uuid::from_u128(5)),
                twin(5, 1, Some(DAY1), false, false),
            ),
        ]);
        let verdicts = detect_series_twins(&tasks, day_utc());
        assert_eq!(
            verdicts,
            vec![SeriesTwinCollapse {
                survivor: TaskId::from(uuid::Uuid::from_u128(5)),
                doomed: vec![TaskId::from(uuid::Uuid::from_u128(10))],
            }]
        );
    }

    #[test]
    fn series_twin_near_misses_never_trigger() {
        let id = |n: u128| TaskId::from(uuid::Uuid::from_u128(n));
        // Same series, different dates (the reopen-while-successor-lives
        // shape): no verdict.
        let tasks = BTreeMap::from([
            (id(1), twin(1, 1, Some(DAY1), false, false)),
            (id(2), twin(2, 1, Some(DAY2), false, false)),
        ]);
        assert_eq!(detect_series_twins(&tasks, day_utc()), Vec::new());
        // Same date, different series: no verdict.
        let tasks = BTreeMap::from([
            (id(1), twin(1, 1, Some(DAY1), false, false)),
            (id(2), twin(2, 2, Some(DAY1), false, false)),
        ]);
        assert_eq!(detect_series_twins(&tasks, day_utc()), Vec::new());
        // One done member: no verdict.
        let tasks = BTreeMap::from([
            (id(1), twin(1, 1, Some(DAY1), false, false)),
            (id(2), twin(2, 1, Some(DAY1), true, false)),
        ]);
        assert_eq!(detect_series_twins(&tasks, day_utc()), Vec::new());
        // One tombstoned member: no verdict.
        let tasks = BTreeMap::from([
            (id(1), twin(1, 1, Some(DAY1), false, false)),
            (id(2), twin(2, 1, Some(DAY1), false, true)),
        ]);
        assert_eq!(detect_series_twins(&tasks, day_utc()), Vec::new());
        // Both dateless (unkeyed): no verdict.
        let tasks = BTreeMap::from([
            (id(1), twin(1, 1, None, false, false)),
            (id(2), twin(2, 1, None, false, false)),
        ]);
        assert_eq!(detect_series_twins(&tasks, day_utc()), Vec::new());
        // A plain non-recur task with the same title-ish shape: never.
        let mut plain = twin(3, 1, Some(DAY1), false, false);
        plain.recur = None;
        let tasks = BTreeMap::from([
            (id(1), twin(1, 1, Some(DAY1), false, false)),
            (id(3), plain),
        ]);
        assert_eq!(detect_series_twins(&tasks, day_utc()), Vec::new());
    }

    /// A duedate-only twin shares the key through the roll's exact
    /// derivation (scheduled-else-duedate) — a mutant reading only
    /// `scheduled` fails here.
    #[test]
    fn series_twin_key_reads_duedate_when_scheduled_is_absent() {
        let id = |n: u128| TaskId::from(uuid::Uuid::from_u128(n));
        let mut a = twin(1, 1, None, false, false);
        let mut b = twin(2, 1, None, false, false);
        a.duedate = Some(ts(DAY1));
        b.duedate = Some(ts(DAY1));
        let tasks = BTreeMap::from([(id(1), a), (id(2), b)]);
        assert_eq!(detect_series_twins(&tasks, day_utc()).len(), 1);
    }

    proptest! {
        #![proptest_config(crate::test_support::proptest_config(256))]

        /// Property 7, no-twin half: whenever no two open live recur
        /// tasks share `(series, date)`, the predicate returns nothing —
        /// for arbitrary mixes of done, tombstoned, dateless, and
        /// non-recur rows. Dates are unique per row by construction, so
        /// no group can reach size 2 whatever the series ids do.
        #[test]
        fn detect_series_twins_is_a_noop_without_a_twin(
            rules in proptest::collection::vec(crate::test_support::recur_rule_strategy(), 1..5),
            times_of_day in proptest::collection::vec(0_u32..86_400, 1..5),
            flags in proptest::collection::vec((proptest::bool::ANY, proptest::bool::ANY), 1..5),
        ) {
            let tasks: BTreeMap<TaskId, Task> = rules
                .into_iter()
                .zip(times_of_day)
                .zip(flags)
                .enumerate()
                .map(|(index, ((rule, seconds), (done, deleted)))| {
                    let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
                        .unwrap()
                        .checked_add_signed(
                            chrono::TimeDelta::try_days(i64::try_from(index).unwrap()).unwrap(),
                        )
                        .unwrap();
                    let instant = day_utc().instant(
                        date,
                        chrono::NaiveTime::MIN
                            + chrono::TimeDelta::try_seconds(i64::from(seconds)).unwrap(),
                    );
                    let mut task = crate::merge_testutil::bound_task(0);
                    task.id =
                        TaskId::from(uuid::Uuid::from_u128(u128::try_from(index).unwrap() + 1));
                    task.remote = None;
                    task.recur = Some(rule);
                    task.scheduled = Some(instant);
                    task.done = done.then(|| ts(BASE));
                    task.deleted = deleted;
                    (task.id, task)
                })
                .collect();
            prop_assert_eq!(detect_series_twins(&tasks, day_utc()), Vec::new());
        }

        /// Property 7, permutation half: twin groups of arbitrary size,
        /// shuffled in the input map — the verdict set is identical
        /// every shuffle; the survivor is the minimum id, the doomed
        /// list the rest ascending.
        #[test]
        fn series_twin_survivorship_is_permutation_invariant(
            group_seeds in proptest::collection::vec(any::<u128>(), 1..4),
            sizes in proptest::collection::vec(2_u32..5_u32, 1..4),
            shuffle in proptest::collection::vec(any::<u64>(), 24),
        ) {
            let date = chrono::NaiveDate::from_ymd_opt(2026, 5, 4).unwrap();
            let instant = day_utc().instant(date, chrono::NaiveTime::MIN);
            let mut members: Vec<Task> = Vec::new();
            let mut id_seed: u128 = 1;
            for (group, size) in group_seeds.into_iter().zip(sizes) {
                for _ in 0..size {
                    let mut task = twin(id_seed, group, Some(DAY1), false, false);
                    task.scheduled = Some(instant);
                    members.push(task);
                    id_seed += 1;
                }
            }
            // Deterministic shuffle: stable sort by the generated keys.
            let mut order: Vec<usize> = (0..members.len()).collect();
            order.sort_by_key(|&i| shuffle[i % shuffle.len()]);
            let shuffled: BTreeMap<TaskId, Task> =
                order.iter().map(|&i| (members[i].id, members[i].clone())).collect();
            let verdicts = detect_series_twins(&shuffled, day_utc());
            let sorted: BTreeMap<TaskId, Task> =
                members.iter().map(|t| (t.id, t.clone())).collect();
            prop_assert_eq!(&verdicts, &detect_series_twins(&sorted, day_utc()));
            for verdict in &verdicts {
                prop_assert!(verdict.doomed.windows(2).all(|w| w[0] < w[1]));
                prop_assert!(verdict.survivor < verdict.doomed[0]);
            }
        }
    }

    #[test]
    fn series_survivor_merge_keeps_identity_and_binding_and_lwws_fields() {
        let mut survivor = twin(1, 1, Some(DAY1), false, false);
        survivor.remote = Some(crate::merge_testutil::card_ref(1, 50));
        survivor.remote_seen = Some(ts(BASE));
        survivor.remote_baseline = Some(crate::entities::RemoteTaskBaseline::of(
            &crate::merge_testutil::remote_task(1, 50, BASE),
        ));
        survivor.effort = Some(crate::entities::Effort::new(3));
        survivor.clocks.effort = ts(BASE + 600); // survivor's newer effort wins
        let mut doomed = twin(2, 1, Some(DAY1), false, false);
        doomed.remote = Some(crate::merge_testutil::card_ref(1, 51));
        doomed.title = "doomed's newer title".to_owned();
        doomed.clocks.title = ts(BASE + 500); // doomed's newer title wins
        doomed.label_memberships.insert(
            crate::ids::LabelId::from(uuid::Uuid::from_u128(9)),
            crate::entities::MembershipState::Added { at: ts(BASE + 500) },
        );

        let merged = series_survivor_merge(&survivor, &doomed);
        assert_eq!(merged.id, survivor.id);
        assert_eq!(
            merged.remote, survivor.remote,
            "the survivor's card persists"
        );
        assert_eq!(merged.remote_seen, survivor.remote_seen);
        assert_eq!(merged.remote_baseline, survivor.remote_baseline);
        assert_eq!(merged.title, doomed.title);
        assert_eq!(merged.clocks.title, ts(BASE + 500));
        assert_eq!(merged.effort, Some(crate::entities::Effort::new(3)));
        assert_eq!(merged.clocks.effort, ts(BASE + 600));
        assert!(
            matches!(
                merged
                    .label_memberships
                    .get(&crate::ids::LabelId::from(uuid::Uuid::from_u128(9))),
                Some(crate::entities::MembershipState::Added { .. })
            ),
            "the label membership OR-folds onto the survivor"
        );
    }
}

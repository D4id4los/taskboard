// SPDX-License-Identifier: MIT OR Apache-2.0
//! The sidecar flush (chunk 4's D11/D12): the read-modify-write union
//! upload that runs **between the cycle's pull and its report**.
//!
//! The flush is never a blind overwrite: the base is the document this
//! cycle actually read (`Document` evidence, or an empty document when
//! rebuilding over `Absent`/foreign content), the overlay carries the
//! board's local truth as of `read_at`, and [`taskboard_domain::union`]
//! merges per-key embedded-clock LWW (ties to the overlay — the
//! just-stamped local writes). The lane GC and the snapshot-absence GC
//! run on the union, then one PUT lands it (D-S5).
//!
//! Failure semantics (D-S7's fence table): a transport-class upload
//! failure strands (no outcomes, ops retry next cycle, the cycle still
//! reports `Completed`); a permanent `BadRequest` dead-letters the
//! group's primary as a counted `FailOp` (the native bad-request
//! path's convergence cadence, mirrored); the flush never aborts the
//! cycle.

use std::collections::{BTreeMap, BTreeSet};

use taskboard_domain::{
    AppDataDocument, AppDataEvidence, AppDataSections, AppDataUnreadable, BoardId, DeckFeatures,
    EntityTables, HabitState, LocalOp, PendingOp, PushGroup, PushOutcome, PushResult, ReminderRule,
    ReminderRuleId, RemoteBoardId, RemoteBoardSnapshot, Stack, SyncErrorKind, Task, TaskId,
    TimeEntry, TimeEntryId, derive_rule_tombstones, derive_time_entry_tombstones,
    dismissals_entry_for, habit_log_for, habits_for, lane_gc_old_habit_log,
    lane_gc_old_time_entries, lane_gc_scheduled, prune_absent_cards, rules_entry_for,
    task_entry_for, time_entries_for, union,
};

use crate::anchor::{self, ANCHOR_ATTACHMENT_FILENAME, ANCHOR_ATTACHMENT_MIME, AnchorAction};
use crate::client::{AttachmentUpload, DeckClient};
use crate::error::DeckError;
use crate::mapping::{self, PushCtx};

/// The upload response's metadata pair — the `LoopState.documents`
/// cache refresh's change-detection input (the next cycle must not
/// re-GET our own write).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttachmentStamp {
    pub last_modified: i64,
    pub filesize: i64,
}

/// What one flush attempt produced. `Idle` covers both "no work" and
/// "the evidence fences flush nothing" — ops stay queued either way.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FlushVerdict {
    /// Nothing uploaded; no outcomes (transport fence, or no work).
    Idle,
    /// The document landed: one `Applied { echo: None }` per resolved
    /// member op, plus the stamps and content for the cache refresh.
    Applied {
        outcomes: Vec<PushOutcome>,
        stamp: AttachmentStamp,
        document: Box<AppDataDocument>,
    },
    /// The write landed but its echo did not decode: outcomes still
    /// complete (the write is authoritative), the cache stays cold so
    /// the next cycle re-GETs honestly.
    AppliedUncached { outcomes: Vec<PushOutcome> },
    /// Permanent 4xx: the group's primary dead-letters as a counted
    /// `FailOp`; subsumed ops re-form the group next cycle.
    DeadLettered { primary: taskboard_domain::OpId },
}

/// Everything one flush attempt reads. The `group` is the cycle's
/// deferred document group (`None` in the pure-rebuild case: no ops
/// queued, only the rebuild triggers fired).
pub(crate) struct FlushCtx<'a> {
    pub client: &'a DeckClient,
    pub target: RemoteBoardId,
    pub features: DeckFeatures,
    pub read_at: chrono::DateTime<chrono::Utc>,
    pub snapshot: &'a RemoteBoardSnapshot,
    pub evidence: &'a AppDataEvidence,
    pub action: &'a AnchorAction,
    pub group: Option<&'a PushGroup>,
    pub outbox: &'a [PendingOp],
    pub tasks: &'a BTreeMap<TaskId, Task>,
    pub stacks: &'a BTreeMap<taskboard_domain::StackId, Stack>,
    pub rules: &'a BTreeMap<ReminderRuleId, ReminderRule>,
    /// The local dismissal set — the `dismissals` section's overlay
    /// input (the emission filter inside `dismissals_entry_for` keeps
    /// the wire Rule-ref-only, design D-4).
    pub dismissals: &'a BTreeSet<taskboard_domain::ReminderDismissal>,
    /// The local time entries — the board-level `time-entries`
    /// section's overlay input (design D-T10).
    pub entries: &'a BTreeMap<TimeEntryId, TimeEntry>,
    /// The local habit state — the board-level `habits`/`habit-log`
    /// sections' overlay input (DR-6 §4.5).
    pub habits: &'a HabitState,
    /// The local board whose document this is (the actor's loop
    /// variable): the habit builders scope their emission to it.
    pub board: BoardId,
    /// Device-local "today" (the injected day calendar): the habit-log
    /// retention boundary — never a second local-date vocabulary.
    pub today: chrono::NaiveDate,
}

/// D12's lazy gate: any live task of `board` carrying app-data (the
/// three always-app fields, rules, or — while the connector lacks
/// native `startdate` — a scheduled value), or any habit content of
/// `board` (D-H9). Boards whose users touch only native fields never
/// grow an anchor.
#[allow(clippy::too_many_arguments)] // the gate reads every lane's inputs; bundling hides the seams
pub(crate) fn board_has_local_app_data(
    tables: EntityTables<'_>,
    rules: &BTreeMap<ReminderRuleId, ReminderRule>,
    entries: &BTreeMap<TimeEntryId, TimeEntry>,
    habits: &HabitState,
    today: chrono::NaiveDate,
    now: chrono::DateTime<chrono::Utc>,
    board: BoardId,
    native_scheduled: bool,
) -> bool {
    let board_tasks: BTreeSet<TaskId> = tables
        .tasks
        .values()
        .filter(|task| {
            task.is_live()
                && tables
                    .stacks
                    .get(&task.stack)
                    .is_some_and(|stack| stack.board == board)
        })
        .map(|task| task.id)
        .collect();
    let window_start = now
        - chrono::TimeDelta::try_days(taskboard_domain::TRACKING_SYNC_RETENTION_DAYS)
            .expect("180 days is within TimeDelta's range");
    tables.tasks.values().any(|task| {
        board_tasks.contains(&task.id)
            && (task.effort.is_some()
                || task.importance.is_some()
                || task.urgency.is_some()
                || (!native_scheduled && task.scheduled.is_some())
                || task.recur.is_some() // no native lane — always document content
                || rules.values().any(|rule| rule.task == task.id))
    }) || entries
        .values()
        .any(|entry| board_tasks.contains(&entry.task) && entry.start >= window_start)
    // The habit clauses (D-H9): a board whose users touch only habits
    // must still grow the anchor. The retention window applies here too
    // — a long-empty board's aged log does not hold the anchor up on
    // its own; definitions count while any live one exists.
    || habits.habits.values().any(|habit| habit.is_live() && habit.board == board)
    || habit_log_for(&habits.habits, &habits.log, board, today)
        .keys()
        .next()
        .is_some()
}

/// Whether the flush must run this cycle (D11's has-work gate):
/// the board planned a document group, or a rebuild trigger fired and
/// local app-data exists.
#[allow(clippy::too_many_arguments)] // the gate reads every lane's inputs; bundling hides the seams
pub(crate) fn has_flush_work(
    group: Option<&PushGroup>,
    evidence: &AppDataEvidence,
    tables: EntityTables<'_>,
    rules: &BTreeMap<ReminderRuleId, ReminderRule>,
    entries: &BTreeMap<TimeEntryId, TimeEntry>,
    habits: &HabitState,
    today: chrono::NaiveDate,
    now: chrono::DateTime<chrono::Utc>,
    board: BoardId,
    native_scheduled: bool,
) -> bool {
    if group.is_some() {
        return true;
    }
    let rebuild_trigger = matches!(
        evidence,
        AppDataEvidence::Absent | AppDataEvidence::Unreadable(AppDataUnreadable::ForeignBoard)
    );
    rebuild_trigger
        && board_has_local_app_data(
            tables,
            rules,
            entries,
            habits,
            today,
            now,
            board,
            native_scheduled,
        )
}

/// Runs the flush (D11). Call sites: the sync actor's cycle, after
/// `sidecar_evidence`, before the report — the only position satisfying
/// D-S4's three clauses (after the cycle's card pushes, after the pull,
/// before the report; the stamp bump lands in the next cycle's pull).
#[allow(clippy::too_many_lines)] // one ladder, one outcome table; splitting hides the fence rows
pub(crate) async fn run_flush(ctx: FlushCtx<'_>) -> FlushVerdict {
    // The fold/flush fences: a document we could not read must never be
    // re-encoded (D-S7: Transport/Decode/FormatTooNew strand loudly).
    let base = match ctx.evidence {
        AppDataEvidence::Document(document) => (**document).clone(),
        AppDataEvidence::Absent | AppDataEvidence::Unreadable(AppDataUnreadable::ForeignBoard) => {
            // Rebuild content over the silence (or D-S2's moved-card
            // fence: the flush may replace foreign content).
            AppDataDocument {
                format: taskboard_domain::APPDATA_FORMAT,
                board: ctx.target,
                sections: AppDataSections::default(),
                unknown: BTreeMap::new(),
            }
        }
        AppDataEvidence::Unreadable(_) => return FlushVerdict::Idle,
    };

    // ---- overlay assembly -------------------------------------------
    // Normal scope: the deferred group's member tasks (resolved, live).
    // Rebuild scope: ALL live board tasks with app-data — a rebuild is
    // about scope, never about blind overwrite.
    let member_ops: Vec<&PendingOp> = match ctx.group {
        Some(group) => ctx
            .outbox
            .iter()
            .filter(|entry| entry.op_id == group.primary || group.subsumed.contains(&entry.op_id))
            .collect(),
        None => Vec::new(),
    };
    // A rebuild trigger widens the scope even when a group is queued:
    // the base carries no usable content (absence or a foreign key), so
    // flushing only the group's tasks over the empty base would drop
    // every other local task's entries from the uploaded document.
    let rebuild = ctx.group.is_none()
        || matches!(
            ctx.evidence,
            AppDataEvidence::Absent | AppDataEvidence::Unreadable(AppDataUnreadable::ForeignBoard)
        );
    let mut scoped: BTreeMap<TaskId, chrono::DateTime<chrono::Utc>> = BTreeMap::new();
    if rebuild {
        let tables = board_tables(&ctx);
        for task in tables.tasks.values() {
            if task.is_live()
                && task.remote.is_some_and(|r| r.board == ctx.target)
                && task_has_app_data(task, ctx.rules, ctx.features)
            {
                scoped.insert(task.id, ctx.read_at);
            }
        }
    }
    // The queued ops' own stamps (≥ `read_at` by the boundary
    // convention) dominate the rebuild seeding.
    for entry in &member_ops {
        if let LocalOp::UpdateAppData(task_id) = entry.op {
            scoped.insert(task_id, entry.queued_at);
        }
    }

    let mut overlay_tasks: BTreeMap<String, taskboard_domain::TaskEntry> = BTreeMap::new();
    let mut overlay_rules: BTreeMap<String, taskboard_domain::RulesEntry> = BTreeMap::new();
    let mut overlay_dismissals: BTreeMap<String, taskboard_domain::DismissalsEntry> =
        BTreeMap::new();
    for (task_id, stamp) in &scoped {
        let Some(task) = ctx.tasks.get(task_id) else {
            continue;
        };
        // Unresolved (create failed / still unbound) tasks are omitted:
        // no entry, no outcome — they ride the next cycle (D10).
        let Some(binding) = task.remote else {
            continue;
        };
        if binding.board != ctx.target || !task.is_live() {
            continue;
        }
        let card_key = binding.card.get().to_string();
        overlay_tasks.insert(
            card_key.clone(),
            task_entry_for(task, ctx.features.card_startdate),
        );
        let mut rules_entry = rules_entry_for(ctx.rules, *task_id);
        // The removal tombstones derive from the base (D8): only
        // base-present live rules missing locally tombstone, stamped
        // with the removing op's instant (or `read_at` in a rebuild).
        if let Some(base_entry) = base
            .sections
            .reminder_rules
            .as_ref()
            .and_then(|entries| entries.get(&card_key))
        {
            for (key, tombstone) in derive_rule_tombstones(base_entry, ctx.rules, *stamp) {
                rules_entry.rules.insert(key, tombstone);
            }
        }
        overlay_rules.insert(card_key.clone(), rules_entry);
        // The dismissal rows join the same overlay card (design D-1/D-9):
        // the builder's filter emits the task's Rule-ref rows only.
        let dismissals_entry = dismissals_entry_for(ctx.dismissals, ctx.rules, *task_id);
        if !dismissals_entry.dismissals.is_empty() {
            overlay_dismissals.insert(card_key, dismissals_entry);
        }
    }
    // The time-entries section is board-level: it does not join the
    // card-scoped loop (D-T10). The builder emits the in-window,
    // card-bound rows; the derivation adds tombstones for base-present
    // rows missing locally (D-T12's truncated-view guard), stamped with
    // the same scoped stamp the rules derivation uses — the group
    // primary's `queued_at`, or `read_at` in a rebuild.
    let mut overlay_time = time_entries_for(ctx.entries, ctx.tasks, ctx.target, ctx.read_at);
    let entry_stamp = ctx.group.and_then(|group| {
        ctx.outbox
            .iter()
            .find(|entry| entry.op_id == group.primary)
            .map(|entry| entry.queued_at)
    });
    let resolvable_cards: BTreeSet<String> = ctx
        .tasks
        .values()
        .filter_map(|task| {
            let binding = task.remote.as_ref()?;
            (binding.board == ctx.target).then(|| binding.card.get().to_string())
        })
        .collect();
    for (key, tombstone) in derive_time_entry_tombstones(
        base.sections
            .time_entries
            .as_ref()
            .unwrap_or(&BTreeMap::new()),
        ctx.entries,
        &resolvable_cards,
        ctx.read_at,
        entry_stamp.unwrap_or(ctx.read_at),
    ) {
        overlay_time.insert(key, tombstone);
    }
    // The habit sections are board-level too (DR-6 §4.5, D-H9): the
    // definitions emit every live row and tombstone of this board (no
    // retention — deletions must roam forever), the log emits its
    // fail-closed in-window rows. No tombstone *derivation* exists for
    // either section (D-H7): a deleted habit leaves a tombstoned row
    // the overlay emits directly, and log rows are never deleted.
    let overlay_habits = habits_for(&ctx.habits.habits, ctx.board);
    let overlay_log = habit_log_for(&ctx.habits.habits, &ctx.habits.log, ctx.board, ctx.today);
    let overlay = AppDataDocument {
        format: taskboard_domain::APPDATA_FORMAT,
        board: ctx.target,
        sections: AppDataSections {
            tasks: (!overlay_tasks.is_empty()).then_some(overlay_tasks),
            reminder_rules: (!overlay_rules.is_empty()).then_some(overlay_rules),
            dismissals: (!overlay_dismissals.is_empty()).then_some(overlay_dismissals),
            time_entries: (!overlay_time.is_empty()).then_some(overlay_time),
            habits: (!overlay_habits.is_empty()).then_some(overlay_habits),
            habit_log: (!overlay_log.is_empty()).then_some(overlay_log),
            unknown: BTreeMap::new(),
        },
        unknown: BTreeMap::new(),
    };

    // ---- union + GC chain --------------------------------------------
    let merged = union(&base, &overlay);
    let merged = lane_gc_scheduled(&merged, ctx.features.card_startdate);
    let merged = lane_gc_old_time_entries(&merged, ctx.read_at);
    // The habit-log wire GC beside the time-entries one (D-H14):
    // document-side compaction only — the local database never drops.
    let merged = lane_gc_old_habit_log(&merged, ctx.today);
    let present_cards: BTreeSet<String> = ctx
        .snapshot
        .tasks
        .iter()
        .map(|task| task.id.card.get().to_string())
        .collect();
    let merged = prune_absent_cards(&merged, &present_cards);
    let bytes = serde_json::to_vec(&merged).expect(
        "owned serde data serializes infallibly; an empty body would be a corrupt overwrite",
    );
    let upload = AttachmentUpload {
        file_name: ANCHOR_ATTACHMENT_FILENAME.to_owned(),
        mime_type: ANCHOR_ATTACHMENT_MIME.to_owned(),
        content: bytes,
    };

    // ---- the upload ladder (per anchor_action) ------------------------
    let Some(ladder) = ladder_for(ctx.action) else {
        return FlushVerdict::Idle;
    };
    let upload_result = match ladder {
        Ladder::Use(location) => upload_onto(&ctx, &location, &upload, false).await,
        Ladder::RestoreThenReplace(location) => {
            // Restore-then-replace (never create-against-a-tombstone).
            match ctx
                .client
                .restore_attachment(
                    ctx.target.get(),
                    location.stack.get(),
                    location.card.get(),
                    location.attachment_id,
                )
                .await
            {
                Ok(_) => upload_onto(&ctx, &location, &upload, false).await,
                Err(err) => Err(err),
            }
        }
        Ladder::Adopt(location) => upload_onto(&ctx, &location, &upload, true).await,
        Ladder::Create => upload_with_create(&ctx, &upload).await,
    };

    // ---- outcome assembly --------------------------------------------
    let resolved_members: Vec<taskboard_domain::OpId> = member_ops
        .iter()
        .filter_map(|entry| match entry.op {
            LocalOp::UpdateAppData(task_id) => {
                if ctx
                    .tasks
                    .get(&task_id)
                    .and_then(|task| task.remote)
                    .is_some_and(|binding| binding.board == ctx.target)
                    && ctx.tasks.get(&task_id).is_some_and(Task::is_live)
                {
                    Some(entry.op_id)
                } else {
                    None
                }
            }
            // The task-less flush trigger is board-carried: the upload
            // IS its resolution — no binding to check, no row to strand.
            LocalOp::FlushBoardAppData(_) => Some(entry.op_id),
            _ => None,
        })
        .collect();
    match upload_result {
        Ok(Some(stamp)) => FlushVerdict::Applied {
            outcomes: resolved_members
                .iter()
                .map(|op| PushOutcome {
                    op: *op,
                    result: PushResult::Applied { echo: None },
                })
                .collect(),
            stamp,
            document: Box::new(merged),
        },
        Ok(None) => FlushVerdict::AppliedUncached {
            outcomes: resolved_members
                .iter()
                .map(|op| PushOutcome {
                    op: *op,
                    result: PushResult::Applied { echo: None },
                })
                .collect(),
        },
        Err(err) => {
            let classified = mapping::classify_push(
                &err,
                PushCtx {
                    existing: true,
                    delete: false,
                },
            );
            match classified {
                // Permanent 4xx: dead-letter the primary (D-S5's
                // completion clause); subsumed ops re-form next cycle.
                Some(PushResult::Rejected {
                    kind: SyncErrorKind::BadRequest,
                }) => match ctx.group {
                    Some(group) => FlushVerdict::DeadLettered {
                        primary: group.primary,
                    },
                    None => FlushVerdict::Idle,
                },
                Some(_) => {
                    // Other classified failures strand (retryable: auth,
                    // forbidden, server, rate limit…).
                    tracing::warn!(board = ctx.target.get(), %err, "sidecar flush rejected; ops stay queued");
                    FlushVerdict::Idle
                }
                None => {
                    tracing::warn!(board = ctx.target.get(), %err, "sidecar flush failed (transport); ops stay queued");
                    FlushVerdict::Idle
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RemoteCardLocation {
    stack: taskboard_domain::RemoteStackId,
    card: taskboard_domain::RemoteCardId,
    attachment_id: u64,
}

/// The upload ladder (per `anchor_action`): which wire steps land the
/// assembled document.
enum Ladder {
    /// Replace the live carrier's content.
    Use(RemoteCardLocation),
    /// Restore the tombstoned carrier, then replace.
    RestoreThenReplace(RemoteCardLocation),
    /// A first upload onto the unconfirmed title match.
    Adopt(RemoteCardLocation),
    /// The two-step create ladder.
    Create,
}

fn ladder_for(action: &AnchorAction) -> Option<Ladder> {
    match action {
        AnchorAction::Use(candidate) => {
            let location = RemoteCardLocation {
                stack: candidate.card.stack,
                card: candidate.card.card,
                attachment_id: candidate.attachment_id,
            };
            Some(if candidate.tombstoned {
                Ladder::RestoreThenReplace(location)
            } else {
                Ladder::Use(location)
            })
        }
        AnchorAction::Adopt(card_ref) => Some(Ladder::Adopt(RemoteCardLocation {
            stack: card_ref.stack,
            card: card_ref.card,
            attachment_id: 0,
        })),
        AnchorAction::Create => Some(Ladder::Create),
        AnchorAction::None => None,
    }
}

fn board_tables<'a>(ctx: &'a FlushCtx<'a>) -> EntityTables<'a> {
    EntityTables {
        tasks: ctx.tasks,
        stacks: ctx.stacks,
        labels: empty_labels(),
        boards: empty_boards(),
    }
}

fn empty_labels() -> &'static BTreeMap<taskboard_domain::LabelId, taskboard_domain::entities::Label>
{
    static EMPTY: std::sync::LazyLock<
        BTreeMap<taskboard_domain::LabelId, taskboard_domain::entities::Label>,
    > = std::sync::LazyLock::new(BTreeMap::new);
    &EMPTY
}

fn empty_boards() -> &'static BTreeMap<BoardId, taskboard_domain::Board> {
    static EMPTY: std::sync::LazyLock<BTreeMap<BoardId, taskboard_domain::Board>> =
        std::sync::LazyLock::new(BTreeMap::new);
    &EMPTY
}

/// One scoped task's app-data presence (the rebuild scope's filter).
fn task_has_app_data(
    task: &Task,
    rules: &BTreeMap<ReminderRuleId, ReminderRule>,
    features: DeckFeatures,
) -> bool {
    task.effort.is_some()
        || task.importance.is_some()
        || task.urgency.is_some()
        || (!features.card_startdate && task.scheduled.is_some())
        || task.recur.is_some() // recurrence has no native lane — always document content
        || rules.values().any(|rule| rule.task == task.id)
}

/// `Ok(Some(stamp))` on a decoded response, `Ok(None)` when the write
/// landed but the echo did not decode (the cache stays cold), `Err`
/// for the classification table.
async fn upload_onto(
    ctx: &FlushCtx<'_>,
    location: &RemoteCardLocation,
    upload: &AttachmentUpload,
    fresh_card: bool,
) -> Result<Option<AttachmentStamp>, DeckError> {
    let attachment = if fresh_card {
        ctx.client
            .upload_attachment(
                ctx.target.get(),
                location.stack.get(),
                location.card.get(),
                upload,
            )
            .await
    } else {
        ctx.client
            .update_attachment_content(
                ctx.target.get(),
                location.card.get(),
                location.attachment_id,
                upload,
            )
            .await
    }?;
    Ok(attachment_stamp(&attachment))
}

/// The Create ladder: `creation_target` (the first live stack at last
/// position) → the convention-titled card → the first upload. Actor-side
/// and lazy; the next pull adopts the carrier (R6).
async fn upload_with_create(
    ctx: &FlushCtx<'_>,
    upload: &AttachmentUpload,
) -> Result<Option<AttachmentStamp>, DeckError> {
    let Some((stack, order)) = anchor::creation_target(&ctx.snapshot.stacks, &ctx.snapshot.tasks)
    else {
        // No live stack: the flush strands this cycle (the actor never
        // auto-creates stacks); ops stay queued.
        tracing::warn!(
            board = ctx.target.get(),
            "sidecar flush stranded: no live stack for the anchor"
        );
        return Ok(None);
    };
    let created = ctx
        .client
        .create_card(
            ctx.target.get(),
            stack.get(),
            &crate::client::NewCard {
                title: taskboard_domain::ANCHOR_CARD_TITLE.to_owned(),
                order: Some(order),
                description: None,
                duedate: None,
                startdate: None,
                kind: None,
            },
        )
        .await?;
    let attachment = ctx
        .client
        .upload_attachment(ctx.target.get(), stack.get(), created.id, upload)
        .await?;
    Ok(attachment_stamp(&attachment))
}

fn attachment_stamp(attachment: &crate::model::Attachment) -> Option<AttachmentStamp> {
    let filesize = attachment
        .extended_data
        .as_ref()
        .map(|data| data.size)
        .or_else(|| attachment.data.as_ref().map(|data| data.size));
    filesize.map(|filesize| AttachmentStamp {
        last_modified: attachment.last_modified,
        filesize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    /// One live (or soft-deleted) bound task in one stack on `board`:
    /// the owned tables, so each test builds its own `EntityTables`.
    fn fixture(
        deleted: bool,
    ) -> (
        BTreeMap<TaskId, Task>,
        BTreeMap<taskboard_domain::StackId, Stack>,
        TaskId,
    ) {
        let board = BoardId::from(uuid::Uuid::from_u128(1));
        let stack_id = taskboard_domain::StackId::from(uuid::Uuid::from_u128(2));
        let id = TaskId::from(uuid::Uuid::from_u128(1));
        let t = Task {
            id,
            remote: Some(taskboard_domain::RemoteCardRef {
                board: RemoteBoardId(77),
                stack: taskboard_domain::RemoteStackId(7),
                card: taskboard_domain::RemoteCardId(412),
            }),
            title: "task".to_owned(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack: stack_id,
            order: 0,
            label_memberships: BTreeMap::new(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            archived: false,
            deleted,
            clocks: taskboard_domain::TaskClocks {
                title: ts(1),
                description: ts(1),
                duedate: ts(1),
                scheduled: ts(1),
                done: ts(1),
                position: ts(1),
                effort: ts(1),
                importance: ts(1),
                urgency: ts(1),
                recur: ts(1),
                archived: ts(1),
                deleted: ts(1),
            },
            remote_seen: None,
            remote_baseline: None,
        };
        let stack = Stack {
            id: stack_id,
            remote: Some(taskboard_domain::RemoteStackRef {
                board: RemoteBoardId(77),
                stack: taskboard_domain::RemoteStackId(7),
            }),
            board,
            title: "stack".to_owned(),
            order: 0,
            archived: false,
            deleted: false,
            clocks: taskboard_domain::StackClocks {
                title: ts(1),
                order: ts(1),
                deleted: ts(1),
            },
            remote_seen: None,
        };
        (
            BTreeMap::from([(id, t)]),
            BTreeMap::from([(stack_id, stack)]),
            id,
        )
    }

    fn entry(
        id: u128,
        task: TaskId,
        start: chrono::DateTime<chrono::Utc>,
    ) -> (TimeEntryId, TimeEntry) {
        (
            TimeEntryId::from(uuid::Uuid::from_u128(id)),
            TimeEntry {
                id: TimeEntryId::from(uuid::Uuid::from_u128(id)),
                task,
                task_title: "task".to_owned(),
                start,
                end: None,
                created_at: start,
                updated_at: start,
            },
        )
    }

    #[test]
    fn a_tracking_only_board_fires_the_tracking_clause() {
        let (tasks, stacks, task_id) = fixture(false);
        let tables = EntityTables {
            tasks: &tasks,
            stacks: &stacks,
            labels: empty_labels(),
            boards: empty_boards(),
        };
        let board = BoardId::from(uuid::Uuid::from_u128(1));
        let now = ts(10_000_000);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
        let habit_empty = taskboard_domain::HabitState::default();
        // No entries: a task with no app-data has no flush work.
        let empty: BTreeMap<TimeEntryId, TimeEntry> = BTreeMap::new();
        assert!(!has_flush_work(
            None,
            &AppDataEvidence::Absent,
            tables,
            &BTreeMap::new(),
            &empty,
            &habit_empty,
            today,
            now,
            board,
            false
        ));
        // One in-window bound entry: the tracking clause fires alone.
        let entries = BTreeMap::from([entry(1, task_id, ts(9_000_000))]);
        assert!(has_flush_work(
            None,
            &AppDataEvidence::Absent,
            tables,
            &BTreeMap::new(),
            &entries,
            &habit_empty,
            today,
            now,
            board,
            false
        ));
        // An aged entry is not work (the gate agrees with the window).
        let aged_start = now
            - chrono::TimeDelta::try_days(taskboard_domain::TRACKING_SYNC_RETENTION_DAYS).unwrap()
            - chrono::TimeDelta::try_seconds(1).unwrap();
        let aged = BTreeMap::from([entry(2, task_id, aged_start)]);
        assert!(!has_flush_work(
            None,
            &AppDataEvidence::Absent,
            tables,
            &BTreeMap::new(),
            &aged,
            &habit_empty,
            today,
            now,
            board,
            false
        ));
    }

    /// A recurrence rule is document content (no native Deck lane): a
    /// board whose only app-data is a task's recur rule fires the lazy
    /// gate, so the overlay row — which `task_entry_for` emits with the
    /// rule under its clock — actually reaches the wire.
    #[test]
    fn a_recur_only_board_fires_the_appdata_clause() {
        let (mut tasks, stacks, task_id) = fixture(false);
        tasks.get_mut(&task_id).unwrap().recur = Some(taskboard_domain::recur::RecurRule {
            series: taskboard_domain::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(7)),
            anchor: chrono::NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
            pattern: taskboard_domain::recur::RecurPattern::Daily {
                every: taskboard_domain::recur::EveryDays::new(1).unwrap(),
            },
            bound: taskboard_domain::recur::RecurBound::None,
            continued_from: None,
        });
        let tables = EntityTables {
            tasks: &tasks,
            stacks: &stacks,
            labels: empty_labels(),
            boards: empty_boards(),
        };
        let board = BoardId::from(uuid::Uuid::from_u128(1));
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
        let habit_empty = taskboard_domain::HabitState::default();
        let empty: BTreeMap<TimeEntryId, TimeEntry> = BTreeMap::new();
        assert!(
            has_flush_work(
                None,
                &AppDataEvidence::Absent,
                tables,
                &BTreeMap::new(),
                &empty,
                &habit_empty,
                today,
                ts(10_000_000),
                board,
                false
            ),
            "the recur-only board grows its anchor"
        );
        assert!(
            task_has_app_data(
                &tasks[&task_id],
                &BTreeMap::new(),
                taskboard_domain::capabilities::DeckFeatures::CONSERVATIVE
            ),
            "the rebuild scope picks the recur-bearing task"
        );
    }
}

#[allow(clippy::too_many_lines)] // one ladder per clause; splitting hides the gate's shape
#[cfg(test)]
mod habit_gate_tests {
    use super::*;
    use chrono::TimeZone;
    use taskboard_domain::{
        Habit, HabitClocks, HabitLogEntry, HabitLogKey, HabitTarget, NonZeroCount, WindowStart,
    };

    fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    #[test]
    fn a_habit_only_board_fires_the_habit_clauses() {
        let empty_tasks: BTreeMap<TaskId, Task> = BTreeMap::new();
        let empty_stacks: BTreeMap<taskboard_domain::StackId, Stack> = BTreeMap::new();
        let tables = EntityTables {
            tasks: &empty_tasks,
            stacks: &empty_stacks,
            labels: empty_labels(),
            boards: empty_boards(),
        };
        let board = BoardId::from(uuid::Uuid::from_u128(1));
        let foreign = BoardId::from(uuid::Uuid::from_u128(2));
        let now = ts(1_700_000_000);
        let today = chrono::NaiveDate::from_ymd_opt(2023, 11, 14).unwrap();
        let habit = |board: BoardId, deleted: bool| Habit {
            id: taskboard_domain::ids::HabitId::from(uuid::Uuid::from_u128(10)),
            board,
            title: "water".into(),
            target: HabitTarget::PerDay(NonZeroCount::new(3).unwrap()),
            order: 0,
            anchor_time: None,
            clocks: HabitClocks::all(now),
            continued_from: None,
            deleted,
        };
        let mut habits = HabitState::default();
        habits
            .habits
            .insert(habit(board, false).id, habit(board, false));
        // A live definition of this board grows the anchor alone.
        assert!(has_flush_work(
            None,
            &AppDataEvidence::Absent,
            tables,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &habits,
            today,
            now,
            board,
            false
        ));
        // A foreign-board habit never does.
        let mut foreign_habits = HabitState::default();
        let f = habit(foreign, false);
        foreign_habits.habits.insert(f.id, f);
        assert!(!has_flush_work(
            None,
            &AppDataEvidence::Absent,
            tables,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &foreign_habits,
            today,
            now,
            board,
            false
        ));
        // An aged log row alone does not hold the anchor up.
        let aged_date = today
            .checked_sub_days(chrono::Days::new(
                taskboard_domain::HABIT_LOG_SYNC_RETENTION_DAYS as u64 + 1,
            ))
            .unwrap();
        let mut aged_only = HabitState::default();
        let dead = habit(board, true);
        let dead_id = dead.id;
        aged_only.habits.insert(dead.id, dead);
        aged_only.log.insert(
            HabitLogKey {
                habit: dead_id,
                window: WindowStart::from_stored(aged_date),
            },
            HabitLogEntry {
                count: 1,
                clock: now,
            },
        );
        assert!(!has_flush_work(
            None,
            &AppDataEvidence::Absent,
            tables,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &aged_only,
            today,
            now,
            board,
            false
        ));
        // An in-window log row does.
        let mut fresh_log = aged_only.clone();
        fresh_log.log.insert(
            HabitLogKey {
                habit: dead_id,
                window: WindowStart::from_stored(today),
            },
            HabitLogEntry {
                count: 1,
                clock: now,
            },
        );
        assert!(has_flush_work(
            None,
            &AppDataEvidence::Absent,
            tables,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &fresh_log,
            today,
            now,
            board,
            false
        ));
    }
}

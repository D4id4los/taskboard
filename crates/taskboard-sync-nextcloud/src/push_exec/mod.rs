// SPDX-License-Identifier: MIT OR Apache-2.0
//! The push group executor: turns one [`PushGroup`] into client calls,
//! resolving bindings through an in-cycle overlay and classifying every
//! outcome (phase 4 plan §5.3).
//!
//! The overlay is seeded from the read state's bindings and extended as
//! create echoes return — never re-derived by search (decision 7). Every
//! op that touches an existing bound card fetches the card first
//! (fetch-before-write, decision 5): the round-trip PUT is built from the
//! fetched card, and a fetch 404 re-resolves the card once against the
//! fresh listings before classifying — a stale binding (the card moved
//! remotely) rebinds and pushes at the fresh location instead of being
//! misread as `RemoteMissing` (issue #258).

use std::collections::HashMap;

use taskboard_domain::{
    CardShape, EntityTables, LocalOp, MaterializedOp, PendingOp, PushGroup, PushOutcome,
    PushResult, RemoteBoardId, RemoteCardRef, RemoteEcho, RemoteIndex, RemoteLabelId,
    RemoteStackId,
};

use crate::client::DeckClient;
use crate::color::DeckColor;
use crate::error::DeckError;
use crate::mapping::{self, PushCtx};
use crate::model::{Card, StackFilter};

/// Local id → remote id bindings for one cycle, extendable by create
/// echoes. The *inverse* of the domain's read-side [`RemoteIndex`].
#[derive(Debug, Clone)]
pub struct BindingOverlay {
    board: RemoteBoardId,
    stack_by_local: HashMap<taskboard_domain::StackId, RemoteStackId>,
    task_by_local:
        HashMap<taskboard_domain::TaskId, (RemoteStackId, taskboard_domain::RemoteCardId)>,
    label_by_local: HashMap<taskboard_domain::LabelId, RemoteLabelId>,
}

impl BindingOverlay {
    /// Seeds the overlay from the read state's bindings.
    #[must_use]
    pub fn from_index(board: RemoteBoardId, index: &RemoteIndex) -> Self {
        Self {
            board,
            stack_by_local: index
                .stack_by_ref
                .iter()
                .map(|(r, local)| (*local, r.stack))
                .collect(),
            task_by_local: index
                .task_by_ref
                .iter()
                .map(|(r, local)| (*local, (r.stack, r.card)))
                .collect(),
            label_by_local: index
                .label_by_ref
                .iter()
                .map(|(r, local)| (*local, r.label))
                .collect(),
        }
    }

    /// The board every push is addressed to (the cycle's pull target).
    #[must_use]
    pub const fn board(&self) -> RemoteBoardId {
        self.board
    }

    fn card_of(&self, task: taskboard_domain::TaskId) -> Option<RemoteCardRef> {
        self.task_by_local
            .get(&task)
            .map(|(stack, card)| RemoteCardRef {
                board: self.board,
                stack: *stack,
                card: *card,
            })
    }
}

/// What one group's execution produced: the outcomes to report, or the
/// transport failure that aborted the cycle.
#[derive(Debug, Clone, PartialEq)]
pub enum GroupOutcome {
    /// Outcomes for (at least) the group's primary op. An empty vec means
    /// the whole group was deferred to the next cycle (no outcomes).
    Outcomes(Vec<PushOutcome>),
    /// A transport-class failure: stop pushing; nothing is reported for
    /// this group or anything after it (decision 8). The typed error is
    /// logged at the abort site (`DeckError` is not `Clone`). `landed`
    /// carries the outcomes for writes that *did* complete before the
    /// transport failure — most importantly a create whose post-create
    /// sub-call failed: without the evidence the engine would re-POST the
    /// create next cycle, and Deck has no idempotency keys.
    Aborted { landed: Vec<PushOutcome> },
}

/// One push cycle's executor over the injected client.
#[derive(Debug)]
pub struct PushExecutor<'a> {
    client: &'a DeckClient,
    overlay: BindingOverlay,
}

impl<'a> PushExecutor<'a> {
    /// Builds an executor seeded with the read state's bindings.
    #[must_use]
    pub fn new(client: &'a DeckClient, overlay: BindingOverlay) -> Self {
        Self { client, overlay }
    }

    /// The (echo-extended) overlay, for the actor's post-cycle state.
    #[must_use]
    pub const fn overlay(&self) -> &BindingOverlay {
        &self.overlay
    }

    /// Executes one group in planner order, emitting at least the primary
    /// op's outcome — unless a transport failure aborts the cycle or the
    /// group defers itself entirely. The outbox supplies the subsumed
    /// ops' payloads (the planner's groups carry ids only).
    #[allow(clippy::too_many_lines)] // one decision table per materialized kind
    pub async fn execute(
        &mut self,
        group: &PushGroup,
        outbox: &[PendingOp],
        tables: EntityTables<'_>,
    ) -> GroupOutcome {
        tracing::trace!(?group, "executing push group");
        match group.materialized.clone() {
            MaterializedOp::Noop => GroupOutcome::Outcomes(Self::all_applied(group)),
            MaterializedOp::CreateTask {
                task,
                stack,
                new_card,
            } => {
                self.create_task(group, task, stack, &new_card, outbox)
                    .await
            }
            MaterializedOp::UpdateTask { task, card } => {
                self.update_task(group, task, &card, outbox).await
            }
            MaterializedOp::MoveTask { task, to } => self.move_task(group, task, to).await,
            MaterializedOp::DeleteTask { task } => match self.overlay.card_of(task) {
                None => GroupOutcome::Outcomes(Self::all_applied(group)),
                Some(card_ref) => self.delete_card(group, card_ref).await,
            },
            MaterializedOp::CreateStack { stack } => {
                let Some(row) = tables.stacks.get(&stack) else {
                    return GroupOutcome::Outcomes(Self::all_applied(group));
                };
                match self
                    .client
                    .create_stack(self.overlay.board.get(), &row.title, row.order)
                    .await
                {
                    Ok(created) => {
                        self.overlay
                            .stack_by_local
                            .insert(stack, RemoteStackId(created.id));
                        GroupOutcome::Outcomes(Self::primary_outcome(
                            group,
                            PushResult::Applied {
                                echo: Some(mapping::map_stack_echo(&created, self.overlay.board)),
                            },
                        ))
                    }
                    Err(err) => Self::classify(
                        group,
                        &err,
                        PushCtx {
                            existing: false,
                            delete: false,
                        },
                    ),
                }
            }
            MaterializedOp::RenameBoard { board } => {
                let Some(row) = tables.boards.get(&board) else {
                    return GroupOutcome::Outcomes(Self::all_applied(group));
                };
                // Full-send like every Deck write; the lenient stored
                // color falls back (the `publish` precedent) because a
                // malformed color would be a permanent rejection.
                let changes = mapping::board_changes_from(row);
                match self
                    .client
                    .update_board(self.overlay.board.get(), &changes)
                    .await
                {
                    Ok(updated) => GroupOutcome::Outcomes(Self::primary_outcome(
                        group,
                        PushResult::Applied {
                            echo: Some(taskboard_domain::RemoteEcho::Board(mapping::map_board(
                                &updated,
                            ))),
                        },
                    )),
                    Err(err) => Self::classify(
                        group,
                        &err,
                        PushCtx {
                            existing: true,
                            delete: false,
                        },
                    ),
                }
            }
            // A rename and a move share one full-send wire call
            // (`update_stack` carries title + order), so one executor
            // arm serves both materializations.
            MaterializedOp::RenameStack { stack } | MaterializedOp::MoveStack { stack } => {
                let Some(stack_num) = self.overlay.stack_by_local.get(&stack).copied() else {
                    return Self::reject_primary(group);
                };
                let Some(row) = tables.stacks.get(&stack) else {
                    return GroupOutcome::Outcomes(Self::all_applied(group));
                };
                match self
                    .client
                    .update_stack(
                        self.overlay.board.get(),
                        stack_num.get(),
                        &mapping::stack_changes_from(row),
                    )
                    .await
                {
                    Ok(updated) => GroupOutcome::Outcomes(Self::primary_outcome(
                        group,
                        PushResult::Applied {
                            echo: Some(mapping::map_stack_echo(&updated, self.overlay.board)),
                        },
                    )),
                    Err(err) => Self::classify(
                        group,
                        &err,
                        PushCtx {
                            existing: true,
                            delete: false,
                        },
                    ),
                }
            }
            MaterializedOp::DeleteStack { stack } => {
                match self.overlay.stack_by_local.get(&stack).copied() {
                    None => GroupOutcome::Outcomes(Self::all_applied(group)),
                    Some(stack_num) => match self
                        .client
                        .delete_stack(self.overlay.board.get(), stack_num.get())
                        .await
                    {
                        Ok(deleted) => GroupOutcome::Outcomes(Self::primary_outcome(
                            group,
                            PushResult::Applied {
                                echo: Some(mapping::map_stack_echo(&deleted, self.overlay.board)),
                            },
                        )),
                        Err(err) => Self::classify(
                            group,
                            &err,
                            PushCtx {
                                existing: true,
                                delete: true,
                            },
                        ),
                    },
                }
            }
            MaterializedOp::CreateLabel { label } => {
                let Some(row) = tables.labels.get(&label) else {
                    return GroupOutcome::Outcomes(Self::all_applied(group));
                };
                let Ok(color) = DeckColor::from_hex(row.color.as_str()) else {
                    return Self::reject_kind(
                        group,
                        PushResult::Rejected {
                            kind: taskboard_domain::SyncErrorKind::BadRequest,
                        },
                    );
                };
                match self
                    .client
                    .create_label(self.overlay.board.get(), &row.title, &color)
                    .await
                {
                    Ok(created) => {
                        self.overlay
                            .label_by_local
                            .insert(label, RemoteLabelId(created.id));
                        GroupOutcome::Outcomes(Self::primary_outcome(
                            group,
                            PushResult::Applied {
                                echo: Some(mapping::map_label_echo(&created, self.overlay.board)),
                            },
                        ))
                    }
                    Err(err) => Self::classify(
                        group,
                        &err,
                        PushCtx {
                            existing: false,
                            delete: false,
                        },
                    ),
                }
            }
            MaterializedOp::UpdateLabel { label } => {
                let Some(label_num) = self.overlay.label_by_local.get(&label).copied() else {
                    return Self::reject_primary(group);
                };
                let Some(row) = tables.labels.get(&label) else {
                    return GroupOutcome::Outcomes(Self::all_applied(group));
                };
                let Some(changes) = mapping::label_changes_from(row) else {
                    return Self::reject_kind(
                        group,
                        PushResult::Rejected {
                            kind: taskboard_domain::SyncErrorKind::BadRequest,
                        },
                    );
                };
                match self
                    .client
                    .update_label(self.overlay.board.get(), label_num.get(), &changes)
                    .await
                {
                    Ok(updated) => GroupOutcome::Outcomes(Self::primary_outcome(
                        group,
                        PushResult::Applied {
                            echo: Some(mapping::map_label_echo(&updated, self.overlay.board)),
                        },
                    )),
                    Err(err) => Self::classify(
                        group,
                        &err,
                        PushCtx {
                            existing: true,
                            delete: false,
                        },
                    ),
                }
            }
            MaterializedOp::DeleteLabel { label } => {
                match self.overlay.label_by_local.get(&label).copied() {
                    None => GroupOutcome::Outcomes(Self::all_applied(group)),
                    Some(label_num) => match self
                        .client
                        .delete_label(self.overlay.board.get(), label_num.get())
                        .await
                    {
                        Ok(deleted) => GroupOutcome::Outcomes(Self::primary_outcome(
                            group,
                            PushResult::Applied {
                                echo: Some(mapping::map_label_echo(&deleted, self.overlay.board)),
                            },
                        )),
                        Err(err) => Self::classify(
                            group,
                            &err,
                            PushCtx {
                                existing: true,
                                delete: true,
                            },
                        ),
                    },
                }
            }
        }
    }

    /// POST the card, bind the echo, then ride the subsumed ops:
    /// label assignments carry their own outcomes; the archived flag and
    /// the done stamp go through their dedicated endpoints (fields Deck's
    /// create endpoint cannot carry).
    #[allow(clippy::too_many_lines)] // post-create chain, one arm each
    async fn create_task(
        &mut self,
        group: &PushGroup,
        task: taskboard_domain::TaskId,
        stack: taskboard_domain::StackId,
        new_card: &taskboard_domain::NewCardShape,
        outbox: &[PendingOp],
    ) -> GroupOutcome {
        let Some(stack_num) = self.overlay.stack_by_local.get(&stack).copied() else {
            return Self::reject_primary(group);
        };
        let created = match self
            .client
            .create_card(
                self.overlay.board.get(),
                stack_num.get(),
                &mapping::new_card_from(new_card),
            )
            .await
        {
            Ok(created) => created,
            Err(err) => {
                return Self::classify(
                    group,
                    &err,
                    PushCtx {
                        existing: false,
                        delete: false,
                    },
                );
            }
        };
        self.overlay.task_by_local.insert(
            task,
            (
                RemoteStackId(created.stack_id),
                taskboard_domain::RemoteCardId(created.id),
            ),
        );

        let mut outcomes = vec![PushOutcome {
            op: group.primary,
            result: PushResult::Applied {
                echo: Some(mapping::map_echo(&created, self.overlay.board)),
            },
        }];
        for op_id in &group.subsumed {
            let Some(op) = outbox.iter().find(|entry| entry.op_id == *op_id) else {
                continue;
            };
            if Self::cannot_ride_a_task_group(&op.op) {
                continue;
            }
            match op.op {
                LocalOp::AssignLabel(_, label) => {
                    match self.overlay.label_by_local.get(&label).copied() {
                        None => outcomes.push(PushOutcome {
                            op: *op_id,
                            result: PushResult::Rejected {
                                kind: taskboard_domain::SyncErrorKind::LocalData,
                            },
                        }),
                        Some(label_num) => {
                            match self
                                .client
                                .assign_label(
                                    self.overlay.board.get(),
                                    created.stack_id,
                                    created.id,
                                    label_num.get(),
                                )
                                .await
                            {
                                Ok(_) => outcomes.push(PushOutcome {
                                    op: *op_id,
                                    result: PushResult::Applied { echo: None },
                                }),
                                Err(err) => {
                                    let ctx = PushCtx {
                                        existing: true,
                                        delete: false,
                                    };
                                    if let Some(result) = mapping::classify_push(&err, ctx) {
                                        outcomes.push(PushOutcome { op: *op_id, result });
                                    } else {
                                        tracing::warn!(error = %err, "push aborted: transport failure");
                                        // The create DID land: its evidence must
                                        // travel in the Failed report or the next
                                        // cycle re-POSTs the card (no idempotency
                                        // keys — decision 3's whole point).
                                        return GroupOutcome::Aborted { landed: outcomes };
                                    }
                                }
                            }
                        }
                    }
                }
                // Unassignments are synthetic on a fresh card (nothing to
                // remove); updates/moves rode the create's current state.
                LocalOp::UnassignLabel(..) | LocalOp::UpdateTask(_) | LocalOp::MoveTask(_) => {
                    outcomes.push(PushOutcome {
                        op: *op_id,
                        result: PushResult::Applied { echo: None },
                    });
                }
                // `cannot_ride_a_task_group` already continued; nothing
                // else can reach the match.
                _ => {}
            }
        }
        if new_card.archived
            && let Err(err) = self
                .client
                .archive_card(self.overlay.board.get(), created.stack_id, created.id)
                .await
        {
            tracing::warn!(error = %err, "post-create archive failed; the flag repairs on a later cycle");
            if err.is_transport() {
                tracing::warn!(error = %err, "push aborted: transport failure");
                // The create landed; report its evidence (see the
                // assignLabel abort above).
                return GroupOutcome::Aborted { landed: outcomes };
            }
        }
        if let Some(done) = new_card.done {
            let mut body = created.clone();
            body.done = Some(done);
            if let Err(err) = self
                .client
                .update_card(self.overlay.board.get(), &body)
                .await
            {
                tracing::warn!(error = %err, "post-create done stamp failed; it repairs on a later cycle");
                if err.is_transport() {
                    tracing::warn!(error = %err, "push aborted: transport failure");
                    return GroupOutcome::Aborted { landed: outcomes };
                }
            }
        }
        GroupOutcome::Outcomes(outcomes)
    }

    /// Fetches a card's authoritative detail immediately before a write,
    /// recovering a stale binding first (ADR 0007's fetch-before-write,
    /// amended for the move case, issue #258): the overlay's ref carries
    /// the stack component, so a card another client *moved* answers 404
    /// at the stale location while still existing — card identity is
    /// (board, card), the pull-side rule. A fetch 404 therefore
    /// re-resolves the card once against the fresh active+archived
    /// listings (the pull's presence truth): found → the overlay is
    /// rebound and the detail fetched once at the fresh location (the
    /// detail outranks the lagging listing); absent from both listings →
    /// `NotFound`, confirmed-truthful, for the normative classification.
    /// Any other failure — original fetch, listing, or refetch —
    /// classifies as itself.
    ///
    /// Returns the card and the ref at its *fresh* location: the group's
    /// subsequent calls must address it there.
    async fn fetch_card_for_write(
        &mut self,
        group: &PushGroup,
        task: taskboard_domain::TaskId,
        card_ref: RemoteCardRef,
    ) -> Result<(RemoteCardRef, Card), GroupOutcome> {
        let ctx = PushCtx {
            existing: true,
            delete: false,
        };
        match self
            .client
            .card(
                card_ref.board.get(),
                card_ref.stack.get(),
                card_ref.card.get(),
            )
            .await
        {
            Ok(fetched) => return Ok((card_ref, fetched)),
            // Stale binding or a genuine deletion — resolve which.
            Err(DeckError::NotFound) => {}
            Err(err) => return Err(Self::classify(group, &err, ctx)),
        }
        for filter in [StackFilter::Active, StackFilter::Archived] {
            let stacks = match self.client.stacks(card_ref.board.get(), filter).await {
                Ok(stacks) => stacks,
                Err(err) => return Err(Self::classify(group, &err, ctx)),
            };
            let Some(fresh_stack) = stacks
                .into_iter()
                .find(|stack| {
                    stack
                        .cards
                        .iter()
                        .any(|card| taskboard_domain::RemoteCardId(card.id) == card_ref.card)
                })
                .map(|stack| RemoteStackId(stack.id))
            else {
                continue;
            };
            let fresh_ref = RemoteCardRef {
                board: card_ref.board,
                stack: fresh_stack,
                card: card_ref.card,
            };
            self.overlay
                .task_by_local
                .insert(task, (fresh_stack, card_ref.card));
            return match self
                .client
                .card(
                    fresh_ref.board.get(),
                    fresh_stack.get(),
                    card_ref.card.get(),
                )
                .await
            {
                Ok(fetched) => Ok((fresh_ref, fetched)),
                Err(err) => Err(Self::classify(group, &err, ctx)),
            };
        }
        Err(Self::classify(group, &DeckError::NotFound, ctx))
    }

    /// Field PUT built from the fetched card (fetch-before-write),
    /// then the label-set delta through the sub-endpoints. A group
    /// whose only delta is unresolvable labels defers entirely.
    #[allow(clippy::too_many_lines)] // one wire contract; the phases read in order
    async fn update_task(
        &mut self,
        group: &PushGroup,
        task: taskboard_domain::TaskId,
        shape: &CardShape,
        outbox: &[PendingOp],
    ) -> GroupOutcome {
        let Some(card_ref) = self.overlay.card_of(task) else {
            return Self::reject_primary(group);
        };
        let (card_ref, fetched) = match self.fetch_card_for_write(group, task, card_ref).await {
            Ok(pair) => pair,
            Err(outcome) => return outcome,
        };
        let (desired, has_unresolved): (Vec<u64>, bool) = {
            let mut resolved = Vec::new();
            let mut unresolved = false;
            for label in &shape.labels {
                match self.overlay.label_by_local.get(label).copied() {
                    Some(num) => resolved.push(num.get()),
                    None => unresolved = true,
                }
            }
            resolved.sort_unstable();
            (resolved, unresolved)
        };
        let fetched_labels: std::collections::BTreeSet<u64> =
            fetched.labels.iter().map(|l| l.id).collect();
        let fields_differ = fetched.title != shape.title
            || fetched.description != shape.description
            || fetched.duedate != shape.duedate
            || fetched.done != shape.done
            || fetched.order != shape.order
            || fetched.archived != shape.archived;
        if has_unresolved
            && !fields_differ
            && desired.as_slice()
                == fetched_labels
                    .iter()
                    .copied()
                    .collect::<Vec<_>>()
                    .as_slice()
        {
            // Only unresolvable labels would change: defer the whole group.
            tracing::debug!(task = %task.as_uuid(), "deferring push group: unresolvable labels only");
            return GroupOutcome::Outcomes(Vec::new());
        }
        // Tier-2 verified (Deck 1.19): the card-update PUT rejects a
        // non-empty `labels` field with 400 — label assignments ride the
        // dedicated `assignLabel`/`removeLabel` sub-endpoints (the
        // create path's contract). The PUT therefore strips the label
        // list, and the desired set is applied below; the end state
        // always converges to `shape.labels`.
        //
        // Structurally safe against clears, too (v1.19.0
        // `CardApiController::update` / `CardService::update`): the
        // update endpoint's parameter list has no `labels` parameter and
        // the update service path never touches card labels — the field
        // is response-only serialization. A PUT can thus never clear or
        // set server-side labels; the only writes are the sub-endpoints
        // below, so "assigned" is decided against the *fetched* set.
        let body = mapping::apply_card_shape(&fetched, shape, Some(&[]));
        let updated = match self.client.update_card(card_ref.board.get(), &body).await {
            Ok(updated) => updated,
            // A 2xx write whose echo failed to decode still landed.
            Err(DeckError::Envelope(_)) => fetched.clone(),
            Err(err) => {
                return Self::classify(
                    group,
                    &err,
                    PushCtx {
                        existing: true,
                        delete: false,
                    },
                );
            }
        };

        // Apply the label-set delta through the sub-endpoints: only
        // missing assignments are sent (redundant on a PUT-clear would
        // be harmless, but the update path cannot clear — see above), so
        // "assigned" is decided against the *fetched* set.
        let mut current = fetched_labels;
        for label_num in &desired {
            if current.contains(label_num) {
                continue;
            }
            match self
                .client
                .assign_label(
                    card_ref.board.get(),
                    card_ref.stack.get(),
                    card_ref.card.get(),
                    *label_num,
                )
                .await
            {
                Ok(_) => {
                    current.insert(*label_num);
                }
                Err(err) => {
                    return Self::label_delta_failure(group, &updated, card_ref.board, &err);
                }
            }
        }
        for label_num in current {
            if desired.contains(&label_num) {
                continue;
            }
            match self
                .client
                .remove_label(
                    card_ref.board.get(),
                    card_ref.stack.get(),
                    card_ref.card.get(),
                    label_num,
                )
                .await
            {
                Ok(_) => {}
                Err(err) => {
                    return Self::label_delta_failure(group, &updated, card_ref.board, &err);
                }
            }
        }

        // One outcome per op: the primary-only vec is the sole source for
        // the subsumed label ops (the `create_task` precedent) — the
        // delta phase brought the server to the desired set, so a
        // resolvable op is Applied and an unresolvable one rejects.
        // Starting from `primary_outcome` here would double-emit every
        // subsumed label op and shadow an unresolvable one's `Rejected
        // { LocalData }` behind a synthetic `Applied`.
        let mut outcomes = vec![PushOutcome {
            op: group.primary,
            result: PushResult::Applied {
                echo: Some(mapping::map_echo(&updated, card_ref.board)),
            },
        }];
        for op_id in &group.subsumed {
            let Some(op) = outbox.iter().find(|entry| &entry.op_id == op_id) else {
                continue;
            };
            if Self::cannot_ride_a_task_group(&op.op) {
                continue;
            }
            match op.op {
                LocalOp::AssignLabel(_, label) | LocalOp::UnassignLabel(_, label) => {
                    outcomes.push(PushOutcome {
                        op: *op_id,
                        result: if self.overlay.label_by_local.contains_key(&label) {
                            PushResult::Applied { echo: None }
                        } else {
                            PushResult::Rejected {
                                kind: taskboard_domain::SyncErrorKind::LocalData,
                            }
                        },
                    });
                }
                // Updates/moves rode the PUT's field state.
                LocalOp::UpdateTask(_) | LocalOp::MoveTask(_) => {
                    outcomes.push(PushOutcome {
                        op: *op_id,
                        result: PushResult::Applied { echo: None },
                    });
                }
                // `cannot_ride_a_task_group` already continued; nothing
                // else can reach the match.
                _ => {}
            }
        }
        GroupOutcome::Outcomes(outcomes)
    }

    /// The reorder primitive (destination stack in the body); skipped when
    /// the fetched position already matches the local one.
    async fn move_task(
        &mut self,
        group: &PushGroup,
        task: taskboard_domain::TaskId,
        to: (taskboard_domain::StackId, i64),
    ) -> GroupOutcome {
        let Some(card_ref) = self.overlay.card_of(task) else {
            return Self::reject_primary(group);
        };
        let Some(target_stack) = self.overlay.stack_by_local.get(&to.0).copied() else {
            return Self::reject_primary(group);
        };
        let (_, fetched) = match self.fetch_card_for_write(group, task, card_ref).await {
            Ok(pair) => pair,
            Err(outcome) => return outcome,
        };
        if fetched.stack_id == target_stack.get() && fetched.order == to.1 {
            // Already there (an earlier cycle landed it): nothing to send.
            return GroupOutcome::Outcomes(vec![PushOutcome {
                op: group.primary,
                result: PushResult::Applied { echo: None },
            }]);
        }
        // The path carries the card's *fetched* (fresh) location; the body
        // carries the destination — the tier-2-verified wire contract.
        match self
            .client
            .reorder_card(
                card_ref.board.get(),
                fetched.stack_id,
                fetched.id,
                to.1,
                target_stack.get(),
            )
            .await
        {
            Ok(()) => GroupOutcome::Outcomes(vec![PushOutcome {
                op: group.primary,
                result: PushResult::Applied { echo: None },
            }]),
            Err(err) => Self::classify(
                group,
                &err,
                PushCtx {
                    existing: true,
                    delete: false,
                },
            ),
        }
    }

    async fn delete_card(&mut self, group: &PushGroup, card_ref: RemoteCardRef) -> GroupOutcome {
        match self
            .client
            .delete_card(
                card_ref.board.get(),
                card_ref.stack.get(),
                card_ref.card.get(),
            )
            .await
        {
            Ok(deleted) => GroupOutcome::Outcomes(Self::primary_outcome(
                group,
                PushResult::Applied {
                    echo: Some(RemoteEcho::Task(mapping::map_card(
                        &deleted,
                        card_ref.board,
                    ))),
                },
            )),
            Err(err) => Self::classify(
                group,
                &err,
                PushCtx {
                    existing: true,
                    delete: true,
                },
            ),
        }
    }

    // ---- outcome assembly -------------------------------------------

    /// A task group's subsumed bucket carries label, update and move ops
    /// only (`plan_task_groups`); these op classes have no outcome to
    /// report there and stay queued untouched.
    fn cannot_ride_a_task_group(op: &LocalOp) -> bool {
        matches!(
            op,
            LocalOp::CreateTask(_)
                | LocalOp::DeleteTask(_)
                | LocalOp::CreateStack(_)
                | LocalOp::RenameStack(_)
                | LocalOp::MoveStack(_)
                | LocalOp::DeleteStack(_)
                | LocalOp::CreateLabel(_)
                | LocalOp::UpdateLabel(_)
                | LocalOp::DeleteLabel(_)
                | LocalOp::RenameBoard(_)
        )
    }

    /// `Applied` for the primary, synthetic `Applied { echo: None }` for
    /// every subsumed op (decision 6).
    fn primary_outcome(group: &PushGroup, result: PushResult) -> Vec<PushOutcome> {
        let mut outcomes = vec![PushOutcome {
            op: group.primary,
            result,
        }];
        outcomes.extend(group.subsumed.iter().map(|op| PushOutcome {
            op: *op,
            result: PushResult::Applied { echo: None },
        }));
        outcomes
    }

    /// Every member op completes synthetically (unbound-entity delete or
    /// missing-row `Noop` group: the server never knew the entity).
    fn all_applied(group: &PushGroup) -> Vec<PushOutcome> {
        std::iter::once(group.primary)
            .chain(group.subsumed.iter().copied())
            .map(|op| PushOutcome {
                op,
                result: PushResult::Applied { echo: None },
            })
            .collect()
    }

    /// Unresolved dependency: `Rejected { LocalData }` for the primary
    /// only; the subsumed ops stay untouched (queued for the next cycle).
    fn reject_primary(group: &PushGroup) -> GroupOutcome {
        GroupOutcome::Outcomes(vec![PushOutcome {
            op: group.primary,
            result: PushResult::Rejected {
                kind: taskboard_domain::SyncErrorKind::LocalData,
            },
        }])
    }

    fn reject_kind(group: &PushGroup, result: PushResult) -> GroupOutcome {
        GroupOutcome::Outcomes(vec![PushOutcome {
            op: group.primary,
            result,
        }])
    }

    /// Applies the classification table. A *successful* classification
    /// (`Applied`, e.g. the 2xx-but-undecodable-echo case) completes the
    /// subsumed ops with the primary; a failure result (`Rejected` /
    /// `RemoteMissing`) completes the primary only — the subsumed ops stay
    /// queued with their sibling for the next cycle (decision 6). `None`
    /// (transport class) aborts the cycle with the landed evidence.
    fn classify(group: &PushGroup, err: &DeckError, ctx: PushCtx) -> GroupOutcome {
        match mapping::classify_push(err, ctx) {
            Some(result @ PushResult::Applied { .. }) => {
                GroupOutcome::Outcomes(Self::primary_outcome(group, result))
            }
            Some(result) => GroupOutcome::Outcomes(vec![PushOutcome {
                op: group.primary,
                result,
            }]),
            // Transport class: the cycle aborts (decision 8) — no outcome
            // for this group's writes (they did not land) or anything after.
            None => GroupOutcome::Aborted { landed: Vec::new() },
        }
    }

    /// Classify-or-abort tail shared by the label-delta sub-endpoints
    /// (assign/remove): a *classified* failure reports the fields' PUT as
    /// landed — its echo is authoritative for the primary — and the label
    /// ops retry next cycle; a transport-class failure aborts the cycle
    /// with nothing reported: the PUT was a full-send of fields that stay
    /// unchanged across the retry, so re-sending the whole group is safe.
    fn label_delta_failure(
        group: &PushGroup,
        updated: &Card,
        board: RemoteBoardId,
        err: &DeckError,
    ) -> GroupOutcome {
        if mapping::classify_push(
            err,
            PushCtx {
                existing: true,
                delete: false,
            },
        )
        .is_some()
        {
            return GroupOutcome::Outcomes(Self::primary_outcome(
                group,
                PushResult::Applied {
                    echo: Some(mapping::map_echo(updated, board)),
                },
            ));
        }
        tracing::warn!(error = %err, "push aborted: transport failure");
        GroupOutcome::Aborted { landed: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::DeckClient;
    use std::collections::BTreeMap;
    use uuid::Uuid;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn uid(raw: u128) -> Uuid {
        Uuid::from_u128(raw)
    }

    fn task_id(raw: u128) -> taskboard_domain::TaskId {
        taskboard_domain::TaskId::from(uid(raw))
    }

    fn stack_id(raw: u128) -> taskboard_domain::StackId {
        taskboard_domain::StackId::from(uid(raw))
    }

    fn label_id(raw: u128) -> taskboard_domain::LabelId {
        taskboard_domain::LabelId::from(uid(raw))
    }

    fn op(raw: u128, kind: LocalOp) -> PendingOp {
        PendingOp {
            op_id: taskboard_domain::OpId(uid(raw)),
            op: kind,
            queued_at: chrono::Utc::now(),
        }
    }

    fn group(
        primary_raw: u128,
        materialized: MaterializedOp,
        target: taskboard_domain::PushTarget,
        subsumed: &[u128],
    ) -> PushGroup {
        PushGroup {
            target,
            materialized,
            primary: taskboard_domain::OpId(uid(primary_raw)),
            subsumed: subsumed
                .iter()
                .map(|raw| taskboard_domain::OpId(uid(*raw)))
                .collect(),
        }
    }

    fn empty_tables() -> EntityTables<'static> {
        EntityTables {
            tasks: Box::leak(Box::new(BTreeMap::new())),
            stacks: Box::leak(Box::new(BTreeMap::new())),
            labels: Box::leak(Box::new(BTreeMap::new())),
            boards: Box::leak(Box::new(BTreeMap::new())),
        }
    }

    fn card_json(id: u64, stack_id: u64, order: i64) -> String {
        serde_json::json!({
            "id": id, "title": "old", "stackId": stack_id, "type": "plain",
            "order": order, "labels": [], "archived": false,
            "duedate": null, "done": null
        })
        .to_string()
    }

    fn stack_json(id: u64, title: &str, order: i64) -> String {
        serde_json::json!({"id": id, "title": title, "order": order, "archived": false}).to_string()
    }

    fn board_json(id: u64, title: &str) -> String {
        serde_json::json!({
            "id": id, "title": title, "color": "00c2e0",
            "lastModified": 1_700_000_000, "deletedAt": 0,
            "archived": false, "labels": []
        })
        .to_string()
    }

    fn label_json(id: u64, title: &str, color: &str) -> String {
        serde_json::json!({"id": id, "title": title, "color": color}).to_string()
    }

    fn stack_row(raw: u128, title: &str, order: i64) -> taskboard_domain::Stack {
        let now = chrono::Utc::now();
        taskboard_domain::Stack {
            id: stack_id(raw),
            remote: None,
            board: taskboard_domain::BoardId::from(uid(1)),
            title: title.into(),
            order,
            archived: false,
            deleted: false,
            clocks: taskboard_domain::StackClocks {
                title: now,
                order: now,
                deleted: now,
            },
            remote_seen: None,
        }
    }

    fn label_row(raw: u128, title: &str, color: &str) -> taskboard_domain::Label {
        let now = chrono::Utc::now();
        taskboard_domain::Label {
            id: label_id(raw),
            remote: None,
            board: taskboard_domain::BoardId::from(uid(1)),
            title: title.into(),
            color: taskboard_domain::Color::new(color),
            deleted: false,
            clocks: taskboard_domain::LabelClocks {
                title: now,
                color: now,
                deleted: now,
            },
            remote_seen: None,
        }
    }

    fn tables_with(
        stacks: &[taskboard_domain::Stack],
        labels: &[taskboard_domain::Label],
    ) -> EntityTables<'static> {
        EntityTables {
            tasks: Box::leak(Box::new(BTreeMap::new())),
            stacks: Box::leak(Box::new(stacks.iter().map(|s| (s.id, s.clone())).collect())),
            labels: Box::leak(Box::new(labels.iter().map(|l| (l.id, l.clone())).collect())),
            boards: Box::leak(Box::new(BTreeMap::new())),
        }
    }

    fn board_id(raw: u128) -> taskboard_domain::BoardId {
        taskboard_domain::BoardId::from(uid(raw))
    }

    fn board_row(raw: u128, title: &str) -> taskboard_domain::Board {
        let now = chrono::Utc::now();
        taskboard_domain::Board {
            id: taskboard_domain::BoardId::from(uid(raw)),
            remote: Some(RemoteBoardId(42)),
            title: title.into(),
            color: taskboard_domain::Color::new("00c2e0"),
            archived: false,
            deleted: false,
            clocks: taskboard_domain::BoardClocks { title: now },
            remote_seen: None,
        }
    }

    fn tables_with_boards(boards: &[taskboard_domain::Board]) -> EntityTables<'static> {
        EntityTables {
            tasks: Box::leak(Box::new(BTreeMap::new())),
            stacks: Box::leak(Box::new(BTreeMap::new())),
            labels: Box::leak(Box::new(BTreeMap::new())),
            boards: Box::leak(Box::new(boards.iter().map(|b| (b.id, b.clone())).collect())),
        }
    }

    /// A refused local port: deterministically transport-class. The
    /// listener is dropped before use — a bound-but-unaccepted socket
    /// would complete the TCP handshake and hang the client instead of
    /// refusing it. The refusal is verified, not assumed: in the
    /// drop-to-connect window a concurrently starting server can win the
    /// same ephemeral port, so any probe outcome other than
    /// `ConnectionRefused` triggers bounded regeneration with a fresh
    /// port.
    fn refused_client() -> DeckClient {
        const MAX_ATTEMPTS: u32 = 10;
        for attempt in 1..=MAX_ATTEMPTS {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            match std::net::TcpStream::connect_timeout(
                &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
                std::time::Duration::from_millis(100),
            ) {
                Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => {
                    if attempt > 1 {
                        let regenerations = attempt - 1;
                        eprintln!("refused_client: regenerated port {regenerations} time(s)");
                    }
                    return DeckClient::new(&format!("http://127.0.0.1:{port}"), "u", "t").unwrap();
                }
                other => {
                    let why = match &other {
                        Ok(_) => "connect succeeded (port won by another process)".to_string(),
                        Err(err) => format!("probe error: {err}"),
                    };
                    eprintln!("refused_client: port {port} not refusing ({why}), regenerating");
                }
            }
        }
        panic!("refused_client: no refusing port found after {MAX_ATTEMPTS} attempts");
    }

    fn overlay_with(
        board: u64,
        stacks: &[(u128, u64)],
        cards: &[(u128, u64, u64)],
        labels: &[(u128, u64)],
    ) -> BindingOverlay {
        let board = RemoteBoardId(board);
        let index = RemoteIndex::from_bindings(
            cards.iter().map(|(local, stack, card)| {
                (
                    RemoteCardRef {
                        board,
                        stack: RemoteStackId(*stack),
                        card: taskboard_domain::RemoteCardId(*card),
                    },
                    taskboard_domain::TaskId::from(uid(*local)),
                )
            }),
            stacks.iter().map(|(local, stack)| {
                (
                    taskboard_domain::RemoteStackRef {
                        board,
                        stack: RemoteStackId(*stack),
                    },
                    stack_id(*local),
                )
            }),
            labels.iter().map(|(local, label)| {
                (
                    taskboard_domain::RemoteLabelRef {
                        board,
                        label: RemoteLabelId(*label),
                    },
                    label_id(*local),
                )
            }),
        );
        BindingOverlay::from_index(board, &index)
    }

    #[tokio::test]
    async fn create_binds_the_echo_and_rides_label_assignments() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 0)))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/assignLabel",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 0)))
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[(6, 3)]));
        let outbox = vec![
            op(100, LocalOp::CreateTask(task_id(10))),
            op(101, LocalOp::AssignLabel(task_id(10), label_id(6))),
        ];
        let shape = taskboard_domain::NewCardShape {
            title: "created".into(),
            order: 0,
            description: String::new(),
            duedate: None,
            done: None,
            archived: false,
            labels: std::collections::BTreeSet::new(),
        };
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::CreateTask {
                        task: task_id(10),
                        stack: stack_id(5),
                        new_card: shape,
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[101],
                ),
                &outbox,
                empty_tables(),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[0].op, taskboard_domain::OpId(uid(100)));
        assert!(matches!(
            &outcomes[0].result,
            PushResult::Applied { echo: Some(RemoteEcho::Task(view)) }
                if view.id.card == taskboard_domain::RemoteCardId(55)
        ));
        assert_eq!(outcomes[1].op, taskboard_domain::OpId(uid(101)));
        assert!(matches!(
            outcomes[1].result,
            PushResult::Applied { echo: None }
        ));
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            2,
            "one create POST plus one assignLabel PUT"
        );
        // The echo extended the overlay: the created card is now resolvable.
        assert!(exec.overlay().card_of(task_id(10)).is_some());
    }

    #[tokio::test]
    async fn create_with_an_unresolved_stack_rejects_local_data_without_sending() {
        let server = MockServer::start().await;
        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let shape = taskboard_domain::NewCardShape {
            title: "created".into(),
            order: 0,
            description: String::new(),
            duedate: None,
            done: None,
            archived: false,
            labels: std::collections::BTreeSet::new(),
        };
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::CreateTask {
                        task: task_id(10),
                        stack: stack_id(5),
                        new_card: shape,
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            outcomes[0].result,
            PushResult::Rejected {
                kind: taskboard_domain::SyncErrorKind::LocalData
            }
        ));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_of_an_unbound_entity_completes_synthetic_without_sending() {
        let server = MockServer::start().await;
        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outbox = vec![
            op(100, LocalOp::CreateTask(task_id(10))),
            op(101, LocalOp::DeleteTask(task_id(10))),
        ];
        let outcome = exec
            .execute(
                &group(
                    101,
                    MaterializedOp::DeleteTask { task: task_id(10) },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[100],
                ),
                &outbox,
                empty_tables(),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert_eq!(outcomes.len(), 2, "both ops complete synthetically");
        assert!(
            outcomes
                .iter()
                .all(|o| matches!(o.result, PushResult::Applied { echo: None }))
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn update_preflights_the_card_then_puts_the_full_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 9)))
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[(10, 7, 55)], &[]));
        let shape = CardShape {
            title: "edited".into(),
            description: String::new(),
            duedate: None,
            done: None,
            order: 9,
            archived: false,
            labels: std::collections::BTreeSet::new(),
        };
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateTask {
                        task: task_id(10),
                        card: shape,
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            &outcomes[0].result,
            PushResult::Applied { echo: Some(RemoteEcho::Task(view)) } if view.order == 9
        ));
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2, "GET first, then the PUT");
        assert_eq!(requests[0].method.as_str(), "GET");
        assert_eq!(requests[1].method.as_str(), "PUT");
    }

    #[tokio::test]
    async fn label_assignment_on_an_update_rides_the_sub_endpoint() {
        // Tier-2 verified (Deck 1.19): the card-update PUT rejects a
        // non-empty `labels` field with 400 — the PUT carries an empty
        // list and the assignment rides `assignLabel`.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/assignLabel",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
            .expect(1)
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[(10, 7, 55)], &[(6, 3)]));
        let shape = CardShape {
            title: "edited".into(),
            description: String::new(),
            duedate: None,
            done: None,
            order: 3,
            archived: false,
            labels: std::collections::BTreeSet::from([label_id(6)]),
        };
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateTask {
                        task: task_id(10),
                        card: shape,
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(outcomes[0].result, PushResult::Applied { .. }));
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 3, "GET, PUT (fields), PUT (assignLabel)");
        // The field PUT carries no label ids: the wire has no fallback.
        let put_body: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
        assert_eq!(put_body["labels"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn label_removal_on_an_update_rides_the_sub_endpoint() {
        // Mirror of the assign test: a fetched card carrying label 3 and
        // a desired set without it — the removal rides `removeLabel`,
        // never the field PUT (the update path cannot touch labels).
        let server = MockServer::start().await;
        let labelled_card = serde_json::json!({
            "id": 55, "title": "old", "stackId": 7, "type": "plain",
            "order": 3, "labels": [3], "archived": false,
            "duedate": null, "done": null
        })
        .to_string();
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(labelled_card.clone()))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(labelled_card.clone()))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/removeLabel",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(labelled_card))
            .expect(1)
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[(10, 7, 55)], &[(6, 3)]));
        let shape = CardShape {
            title: "edited".into(),
            description: String::new(),
            duedate: None,
            done: None,
            order: 3,
            archived: false,
            labels: std::collections::BTreeSet::new(),
        };
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateTask {
                        task: task_id(10),
                        card: shape,
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(outcomes[0].result, PushResult::Applied { .. }));
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 3, "GET, PUT (fields), PUT (removeLabel)");
        assert_eq!(
            requests[2].url.path(),
            "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/removeLabel"
        );
    }

    #[tokio::test]
    async fn preflight_404_on_an_update_maps_to_remote_missing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[(10, 7, 55)], &[]));
        let shape = CardShape {
            title: "edited".into(),
            description: String::new(),
            duedate: None,
            done: None,
            order: 0,
            archived: false,
            labels: std::collections::BTreeSet::new(),
        };
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateTask {
                        task: task_id(10),
                        card: shape,
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(outcomes[0].result, PushResult::RemoteMissing));
    }

    #[tokio::test]
    async fn update_defers_when_only_unresolvable_labels_differ() {
        // The desired card equals the fetched one in every field; the only
        // delta is label 9, which has no remote binding. The whole group
        // defers: empty outcomes, and the only wire traffic is the
        // pre-flight GET — no mutating request may go out.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[(10, 7, 55)], &[]));
        let shape = CardShape {
            title: "old".into(),
            description: String::new(),
            duedate: None,
            done: None,
            order: 3,
            archived: false,
            labels: std::collections::BTreeSet::from([label_id(9)]),
        };
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateTask {
                        task: task_id(10),
                        card: shape,
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(outcomes.is_empty(), "the deferred group completes nothing");
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "the pre-flight GET alone");
        assert_eq!(requests[0].method.as_str(), "GET");
    }

    #[tokio::test]
    async fn update_completes_subsumed_label_ops_by_resolvability() {
        // One outcome per op, no duplicates (#310): the subsumed label
        // ops are decided by resolvability alone — label 6 is bound to
        // remote 3 (applied by the delta's single `assignLabel(3)`),
        // label 9 has no binding and must surface its `Rejected
        // { LocalData }` instead of being shadowed by a synthetic
        // `Applied`.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/assignLabel",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
            .expect(1)
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[(10, 7, 55)], &[(6, 3)]));
        let shape = CardShape {
            title: "edited".into(),
            description: String::new(),
            duedate: None,
            done: None,
            order: 3,
            archived: false,
            labels: std::collections::BTreeSet::from([label_id(6), label_id(9)]),
        };
        let outbox = [
            op(101, LocalOp::AssignLabel(task_id(10), label_id(6))),
            op(102, LocalOp::AssignLabel(task_id(10), label_id(9))),
        ];
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateTask {
                        task: task_id(10),
                        card: shape,
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[101, 102],
                ),
                &outbox,
                empty_tables(),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert_eq!(outcomes.len(), 3, "one outcome per op, no duplicates");
        assert_eq!(outcomes[0].op, taskboard_domain::OpId(uid(100)));
        assert!(matches!(
            outcomes[0].result,
            PushResult::Applied { echo: Some(_) }
        ));
        assert_eq!(outcomes[1].op, taskboard_domain::OpId(uid(101)));
        assert!(matches!(
            outcomes[1].result,
            PushResult::Applied { echo: None }
        ));
        assert_eq!(outcomes[2].op, taskboard_domain::OpId(uid(102)));
        assert!(
            matches!(
                outcomes[2].result,
                PushResult::Rejected {
                    kind: taskboard_domain::SyncErrorKind::LocalData
                }
            ),
            "the unresolvable label op must not be shadowed"
        );
    }

    #[tokio::test]
    async fn move_same_stack_different_order_sends_the_reorder() {
        // Card sits at stack 7 / order 3; the local intent is the *same*
        // stack at order 9. The fast path must not fire: exactly one
        // reorder carrying order 9 and the destination stack.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/reorder",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
            .expect(1)
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[(10, 7, 55)], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::MoveTask {
                        task: task_id(10),
                        to: (stack_id(5), 9),
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].op, taskboard_domain::OpId(uid(100)));
        assert!(matches!(
            outcomes[0].result,
            PushResult::Applied { echo: None }
        ));
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2, "the pre-flight GET, then one reorder");
        assert_eq!(requests[1].method.as_str(), "PUT");
        let body: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"order": 9, "stackId": 7}),
            "the reorder carries order 9 and the destination stack"
        );
    }

    #[tokio::test]
    async fn move_sends_reorder_only_when_the_position_differs() {
        // Card sits at stack 7 / order 3; the local intent is stack 8 / order 1.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55/reorder",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
            .expect(1)
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(
            &client,
            overlay_with(42, &[(5, 7), (6, 8)], &[(10, 7, 55)], &[]),
        );
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::MoveTask {
                        task: task_id(10),
                        to: (stack_id(6), 1),
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;
        server.verify().await;
        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            outcomes[0].result,
            PushResult::Applied { echo: None }
        ));
        let body: serde_json::Value =
            serde_json::from_slice(&server.received_requests().await.unwrap()[1].body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"order": 1, "stackId": 8}),
            "destination stack travels in the body (tier-2-verified shape)"
        );

        // Same position already: no reorder goes out at all.
        let server2 = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 3)))
            .mount(&server2)
            .await;
        let client2 = DeckClient::new(&server2.uri(), "u", "t").unwrap();
        let mut exec2 =
            PushExecutor::new(&client2, overlay_with(42, &[(5, 7)], &[(10, 7, 55)], &[]));
        let outcome = exec2
            .execute(
                &group(
                    100,
                    MaterializedOp::MoveTask {
                        task: task_id(10),
                        to: (stack_id(5), 3),
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;
        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            outcomes[0].result,
            PushResult::Applied { echo: None }
        ));
        assert!(
            server2.received_requests().await.unwrap().len() == 1,
            "only the pre-flight GET, no reorder"
        );
    }

    // ---- stale-binding pre-write-fetch 404s (#258) ---------------------

    /// The stacks listing carrying card 55 under stack 8 (the card "moved"
    /// there behind our back; the overlay still says stack 7).
    fn moved_listing_json() -> String {
        serde_json::json!([{
            "id": 8, "title": "moved-here", "boardId": 42, "order": 1,
            "lastModified": 1_700_000_000, "deletedAt": 0, "archived": false,
            "cards": [{ "id": 55, "title": "old", "stackId": 8, "type": "plain",
                        "order": 3, "labels": [], "archived": false,
                        "duedate": null, "done": null }]
        }])
        .to_string()
    }

    fn edited_shape() -> CardShape {
        CardShape {
            title: "edited".into(),
            description: String::new(),
            duedate: None,
            done: None,
            order: 9,
            archived: false,
            labels: std::collections::BTreeSet::new(),
        }
    }

    /// E1: a pre-write fetch 404 on a card another client *moved*
    /// re-resolves the card from the fresh listing, rebinds the overlay,
    /// and pushes at the fresh location — the update lands instead of
    /// misclassifying as `RemoteMissing` (which tombstoned the task and
    /// silently cancelled the queued edit; issue #258).
    #[tokio::test]
    async fn update_preflight_404_reresolves_the_card_from_a_fresh_listing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
            .respond_with(ResponseTemplate::new(200).set_body_string(moved_listing_json()))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/8/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 8, 3)))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/8/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 8, 9)))
            .expect(1)
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[(10, 7, 55)], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateTask {
                        task: task_id(10),
                        card: edited_shape(),
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes, got {outcome:?}");
        };
        assert!(matches!(
            &outcomes[0].result,
            PushResult::Applied { echo: Some(RemoteEcho::Task(view)) }
                if view.id.stack == RemoteStackId(8) && view.id.card == taskboard_domain::RemoteCardId(55)
        ));
        assert_eq!(
            exec.overlay().card_of(task_id(10)),
            Some(RemoteCardRef {
                board: RemoteBoardId(42),
                stack: RemoteStackId(8),
                card: taskboard_domain::RemoteCardId(55),
            }),
            "the overlay is rebound to the fresh location"
        );
        let requests = server.received_requests().await.unwrap();
        assert_eq!(
            requests.len(),
            4,
            "stale GET (404), fresh listing, fresh GET, PUT at the fresh stack"
        );
    }

    /// E2: the same recovery on the move path — the reorder's *path* is
    /// the card's fetched (fresh) location while its *body* carries the
    /// distinct destination (the wire contract); the stale stack appears
    /// in no request after the initial 404.
    #[tokio::test]
    async fn move_preflight_404_reresolves_then_reorders_from_the_fresh_location() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
            .respond_with(ResponseTemplate::new(200).set_body_string(moved_listing_json()))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/8/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 8, 3)))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/8/cards/55/reorder",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
            .expect(1)
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(
            &client,
            overlay_with(42, &[(6, 8), (7, 9)], &[(10, 7, 55)], &[]),
        );
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::MoveTask {
                        task: task_id(10),
                        to: (stack_id(7), 5),
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes, got {outcome:?}");
        };
        assert!(matches!(
            outcomes[0].result,
            PushResult::Applied { echo: None }
        ));
        let requests = server.received_requests().await.unwrap();
        assert_eq!(
            requests.len(),
            4,
            "stale GET (404), fresh listing, fresh GET, reorder from the fresh stack"
        );
        let body: serde_json::Value = serde_json::from_slice(&requests[3].body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"order": 5, "stackId": 9}),
            "the body carries the destination (stack 9, order 5); the \
             request path stayed at the fetched stack 8"
        );
    }

    /// E3: a 404 the fresh listings *confirm* (the card is in neither)
    /// still classifies `RemoteMissing` — now truthful absence, and the
    /// genuine-deletion terminal is unchanged.
    #[tokio::test]
    async fn preflight_404_with_the_card_absent_from_both_listings_classifies_remote_missing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        for listing in ["stacks", "stacks/archived"] {
            Mock::given(method("GET"))
                .and(path(format!(
                    "/index.php/apps/deck/api/v1.0/boards/42/{listing}"
                )))
                .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
                .mount(&server)
                .await;
        }

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[(10, 7, 55)], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateTask {
                        task: task_id(10),
                        card: edited_shape(),
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes, got {outcome:?}");
        };
        assert!(matches!(outcomes[0].result, PushResult::RemoteMissing));
    }

    /// E4: a lagging listing that claims the card while the fresh detail
    /// 404s defers to the detail — `RemoteMissing`.
    #[tokio::test]
    async fn preflight_404_with_a_lying_listing_maps_the_refetch_404_to_remote_missing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
            .respond_with(ResponseTemplate::new(200).set_body_string(moved_listing_json()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/8/cards/55",
            ))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[(10, 7, 55)], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateTask {
                        task: task_id(10),
                        card: edited_shape(),
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes, got {outcome:?}");
        };
        assert!(matches!(outcomes[0].result, PushResult::RemoteMissing));
    }

    /// Sleeps zero: the E5 retry exhaustion must not depend on real time.
    #[derive(Debug)]
    struct InstantSleeper;

    impl crate::client::RetrySleep for InstantSleeper {
        fn sleep(
            &self,
            _delay: std::time::Duration,
        ) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>> {
            Box::pin(std::future::ready(()))
        }
    }

    /// E5: a listing that fails transport-class aborts the cycle — the
    /// re-resolve must not turn an unknown listing outcome into a
    /// classification (today it never asks, which this test also pins as
    /// a change).
    #[tokio::test]
    async fn preflight_404_with_a_transport_failing_listing_aborts_the_cycle() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t")
            .unwrap()
            .with_sleeper(std::sync::Arc::new(InstantSleeper));
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[(10, 7, 55)], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateTask {
                        task: task_id(10),
                        card: edited_shape(),
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;
        server.verify().await;

        assert_eq!(
            outcome,
            GroupOutcome::Aborted { landed: Vec::new() },
            "a transport-class listing failure aborts with no outcomes"
        );
    }

    /// E6: a forbidden listing classifies `Rejected { Forbidden }` —
    /// retryable, never a tombstone.
    #[tokio::test]
    async fn preflight_404_with_a_forbidden_listing_rejects_forbidden() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[(10, 7, 55)], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateTask {
                        task: task_id(10),
                        card: edited_shape(),
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes, got {outcome:?}");
        };
        assert!(matches!(
            outcomes[0].result,
            PushResult::Rejected {
                kind: taskboard_domain::SyncErrorKind::Forbidden
            }
        ));
    }

    #[tokio::test]
    async fn transport_failure_aborts_without_outcomes() {
        // Reserve a port and release it: deterministically refused.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let client = DeckClient::new(&format!("http://127.0.0.1:{port}"), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[(10, 7, 55)], &[]));
        let shape = CardShape {
            title: "edited".into(),
            description: String::new(),
            duedate: None,
            done: None,
            order: 0,
            archived: false,
            labels: std::collections::BTreeSet::new(),
        };
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateTask {
                        task: task_id(10),
                        card: shape,
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;
        assert!(
            matches!(outcome, GroupOutcome::Aborted { landed } if landed.is_empty()),
            "a transport failure on the group's own write lands nothing"
        );
    }

    /// F1 regression: a create that landed but whose post-create label ride
    /// then hit a transport failure must carry the create's outcome in the
    /// abort — otherwise the engine re-POSTs the card next cycle (no
    /// idempotency keys).
    #[tokio::test]
    async fn transport_failure_after_a_landed_create_keeps_its_evidence() {
        // A raw TCP server: the first request (the create POST) gets a real
        // HTTP 200 response; every later connection is accepted and dropped,
        // which reqwest reports as a transport error (the client exhausts
        // its three attempts against it).
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                serve_one_create(stream);
            }
            // Subsequent connections: accept and immediately drop.
            for _ in 0..8 {
                if listener.accept().is_err() {
                    break;
                }
            }
        });

        let client = DeckClient::new(&format!("http://{addr}"), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[(6, 3)]));
        let outbox = vec![
            op(100, LocalOp::CreateTask(task_id(10))),
            op(101, LocalOp::AssignLabel(task_id(10), label_id(6))),
        ];
        let shape = taskboard_domain::NewCardShape {
            title: "created".into(),
            order: 0,
            description: String::new(),
            duedate: None,
            done: None,
            archived: false,
            labels: std::collections::BTreeSet::new(),
        };
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::CreateTask {
                        task: task_id(10),
                        stack: stack_id(5),
                        new_card: shape,
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[101],
                ),
                &outbox,
                empty_tables(),
            )
            .await;

        let GroupOutcome::Aborted { landed } = outcome else {
            panic!("expected a transport abort");
        };
        assert_eq!(landed.len(), 1, "the create's evidence survives the abort");
        assert!(matches!(
            &landed[0].result,
            PushResult::Applied { echo: Some(RemoteEcho::Task(view)) }
                if view.id.card == taskboard_domain::RemoteCardId(55)
        ));
    }

    /// F2 regression: a failed materialized write completes only the
    /// primary — the subsumed ops stay queued with their sibling.
    #[tokio::test]
    async fn a_rejected_write_leaves_the_subsumed_ops_queued() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_string(card_json(55, 7, 0)))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path(
                "/index.php/apps/deck/api/v1.0/boards/42/stacks/7/cards/55",
            ))
            .respond_with(ResponseTemplate::new(400))
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[(10, 7, 55)], &[]));
        let shape = CardShape {
            title: "edited".into(),
            description: String::new(),
            duedate: None,
            done: None,
            order: 0,
            archived: false,
            labels: std::collections::BTreeSet::new(),
        };
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateTask {
                        task: task_id(10),
                        card: shape,
                    },
                    taskboard_domain::PushTarget::Task(task_id(10)),
                    &[101, 102],
                ),
                &[],
                empty_tables(),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert_eq!(
            outcomes.len(),
            1,
            "only the primary gets an outcome; the subsumed ops stay queued"
        );
        assert_eq!(outcomes[0].op, taskboard_domain::OpId(uid(100)));
        assert!(matches!(
            outcomes[0].result,
            PushResult::Rejected {
                kind: taskboard_domain::SyncErrorKind::BadRequest
            }
        ));
    }

    // ---- stack push paths --------------------------------------------

    #[tokio::test]
    async fn create_stack_posts_and_applies_the_echo() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
            .respond_with(ResponseTemplate::new(200).set_body_string(stack_json(9, "Todo", 2)))
            .expect(1)
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::CreateStack { stack: stack_id(5) },
                    taskboard_domain::PushTarget::Stack(stack_id(5)),
                    &[],
                ),
                &[],
                tables_with(&[stack_row(5, "Todo", 2)], &[]),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            &outcomes[0].result,
            PushResult::Applied { echo: Some(RemoteEcho::Stack(view)) }
                if view.id.stack == RemoteStackId(9)
                    && view.title == "Todo"
                    && view.order == 2
        ));
        let body: serde_json::Value =
            serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
        assert_eq!(body, serde_json::json!({"title": "Todo", "order": 2}));
    }

    #[tokio::test]
    async fn create_stack_with_a_missing_row_completes_synthetic_without_sending() {
        let server = MockServer::start().await;
        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::CreateStack { stack: stack_id(5) },
                    taskboard_domain::PushTarget::Stack(stack_id(5)),
                    &[101],
                ),
                &[],
                empty_tables(),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert_eq!(outcomes.len(), 2, "primary and subsumed op complete");
        assert!(
            outcomes
                .iter()
                .all(|o| matches!(o.result, PushResult::Applied { echo: None }))
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_stack_400_is_rejected_bad_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::CreateStack { stack: stack_id(5) },
                    taskboard_domain::PushTarget::Stack(stack_id(5)),
                    &[],
                ),
                &[],
                tables_with(&[stack_row(5, "Todo", 0)], &[]),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            outcomes[0].result,
            PushResult::Rejected {
                kind: taskboard_domain::SyncErrorKind::BadRequest
            }
        ));
    }

    #[tokio::test]
    async fn create_stack_transport_failure_aborts() {
        let client = refused_client();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::CreateStack { stack: stack_id(5) },
                    taskboard_domain::PushTarget::Stack(stack_id(5)),
                    &[],
                ),
                &[],
                tables_with(&[stack_row(5, "Todo", 0)], &[]),
            )
            .await;
        assert!(matches!(
            outcome,
            GroupOutcome::Aborted { landed } if landed.is_empty()
        ));
    }

    #[tokio::test]
    async fn rename_stack_puts_the_changeset_and_applies_the_echo() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks/7"))
            .respond_with(ResponseTemplate::new(200).set_body_string(stack_json(7, "Doing", 0)))
            .expect(1)
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::RenameStack { stack: stack_id(5) },
                    taskboard_domain::PushTarget::Stack(stack_id(5)),
                    &[],
                ),
                &[],
                tables_with(&[stack_row(5, "Doing", 0)], &[]),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            &outcomes[0].result,
            PushResult::Applied { echo: Some(RemoteEcho::Stack(view)) } if view.title == "Doing"
        ));
        let body: serde_json::Value =
            serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
        assert_eq!(body["title"], "Doing");
    }

    #[tokio::test]
    async fn move_stack_puts_the_full_row_and_applies_the_echo() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks/7"))
            .respond_with(ResponseTemplate::new(200).set_body_string(stack_json(7, "Doing", 3)))
            .expect(1)
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::MoveStack { stack: stack_id(5) },
                    taskboard_domain::PushTarget::Stack(stack_id(5)),
                    &[],
                ),
                &[],
                tables_with(&[stack_row(5, "Doing", 3)], &[]),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            &outcomes[0].result,
            PushResult::Applied { echo: Some(RemoteEcho::Stack(view)) } if view.order == 3
        ));
        // Full-send: the PUT body carries the whole row (title + order).
        let body: serde_json::Value =
            serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
        assert_eq!(body["title"], "Doing");
        assert_eq!(body["order"], 3);
    }

    #[tokio::test]
    async fn move_of_an_unbound_stack_rejects_local_data_without_sending() {
        let server = MockServer::start().await;
        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::MoveStack { stack: stack_id(5) },
                    taskboard_domain::PushTarget::Stack(stack_id(5)),
                    &[],
                ),
                &[],
                tables_with(&[stack_row(5, "Doing", 0)], &[]),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            outcomes[0].result,
            PushResult::Rejected {
                kind: taskboard_domain::SyncErrorKind::LocalData
            }
        ));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn move_stack_404_maps_to_remote_missing() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks/7"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::MoveStack { stack: stack_id(5) },
                    taskboard_domain::PushTarget::Stack(stack_id(5)),
                    &[],
                ),
                &[],
                tables_with(&[stack_row(5, "Doing", 0)], &[]),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(outcomes[0].result, PushResult::RemoteMissing));
    }

    #[tokio::test]
    async fn move_stack_transport_failure_aborts_the_cycle() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks/7"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::MoveStack { stack: stack_id(5) },
                    taskboard_domain::PushTarget::Stack(stack_id(5)),
                    &[],
                ),
                &[],
                tables_with(&[stack_row(5, "Doing", 0)], &[]),
            )
            .await;
        assert!(matches!(outcome, GroupOutcome::Aborted { landed } if landed.is_empty()));
    }

    #[tokio::test]
    async fn rename_of_an_unbound_stack_rejects_local_data_without_sending() {
        let server = MockServer::start().await;
        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::RenameStack { stack: stack_id(5) },
                    taskboard_domain::PushTarget::Stack(stack_id(5)),
                    &[],
                ),
                &[],
                tables_with(&[stack_row(5, "Doing", 0)], &[]),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            outcomes[0].result,
            PushResult::Rejected {
                kind: taskboard_domain::SyncErrorKind::LocalData
            }
        ));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn rename_board_puts_the_full_changeset_and_applies_the_echo() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(board_json(42, "renamed board")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::RenameBoard { board: board_id(1) },
                    taskboard_domain::PushTarget::Board(board_id(1)),
                    &[],
                ),
                &[],
                tables_with_boards(&[board_row(1, "renamed board")]),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            &outcomes[0].result,
            PushResult::Applied { echo: Some(taskboard_domain::RemoteEcho::Board(view)) }
                if view.title == "renamed board"
        ));
        // Full-send: title + color + archived in the body.
        let body: serde_json::Value =
            serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
        assert_eq!(body["title"], "renamed board");
        assert_eq!(body["color"], "00c2e0");
    }

    #[tokio::test]
    async fn rename_board_falls_back_on_an_invalid_stored_color() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42"))
            .respond_with(ResponseTemplate::new(200).set_body_string(board_json(42, "x")))
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let mut row = board_row(1, "x");
        row.color = taskboard_domain::Color::new("not-hex");
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::RenameBoard { board: board_id(1) },
                    taskboard_domain::PushTarget::Board(board_id(1)),
                    &[],
                ),
                &[],
                tables_with_boards(&[row]),
            )
            .await;
        server.verify().await;

        assert!(matches!(
            outcome,
            GroupOutcome::Outcomes(ref o) if matches!(o[0].result, PushResult::Applied { .. })
        ));
        let body: serde_json::Value =
            serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
        assert_eq!(body["color"], "0082c9", "the fallback color is sent");
    }

    #[tokio::test]
    async fn rename_board_404_maps_to_remote_missing() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::RenameBoard { board: board_id(1) },
                    taskboard_domain::PushTarget::Board(board_id(1)),
                    &[],
                ),
                &[],
                tables_with_boards(&[board_row(1, "x")]),
            )
            .await;
        assert!(matches!(
            outcome,
            GroupOutcome::Outcomes(ref o) if matches!(o[0].result, PushResult::RemoteMissing)
        ));
    }

    #[tokio::test]
    async fn rename_board_transport_failure_aborts_the_cycle() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::RenameBoard { board: board_id(1) },
                    taskboard_domain::PushTarget::Board(board_id(1)),
                    &[],
                ),
                &[],
                tables_with_boards(&[board_row(1, "x")]),
            )
            .await;
        assert!(matches!(outcome, GroupOutcome::Aborted { landed } if landed.is_empty()));
    }

    #[tokio::test]
    async fn delete_stack_unbound_completes_synthetic_without_sending() {
        let server = MockServer::start().await;
        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::DeleteStack { stack: stack_id(5) },
                    taskboard_domain::PushTarget::Stack(stack_id(5)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            outcomes[0].result,
            PushResult::Applied { echo: None }
        ));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_stack_sends_the_delete_and_applies_the_echo() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks/7"))
            .respond_with(ResponseTemplate::new(200).set_body_string(stack_json(7, "Doing", 0)))
            .expect(1)
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::DeleteStack { stack: stack_id(5) },
                    taskboard_domain::PushTarget::Stack(stack_id(5)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            &outcomes[0].result,
            PushResult::Applied { echo: Some(RemoteEcho::Stack(view)) }
                if view.id.stack == RemoteStackId(7)
        ));
    }

    #[tokio::test]
    async fn delete_stack_404_maps_to_remote_missing() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/stacks/7"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[(5, 7)], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::DeleteStack { stack: stack_id(5) },
                    taskboard_domain::PushTarget::Stack(stack_id(5)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(outcomes[0].result, PushResult::RemoteMissing));
    }

    // ---- label push paths --------------------------------------------

    #[tokio::test]
    async fn create_label_posts_and_applies_the_echo() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/labels"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(label_json(4, "bug", "ff0000")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::CreateLabel { label: label_id(6) },
                    taskboard_domain::PushTarget::Label(label_id(6)),
                    &[],
                ),
                &[],
                tables_with(&[], &[label_row(6, "bug", "FF0000")]),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            &outcomes[0].result,
            PushResult::Applied { echo: Some(RemoteEcho::Label(view)) }
                if view.id.label == RemoteLabelId(4) && view.title == "bug"
        ));
        let body: serde_json::Value =
            serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"title": "bug", "color": "ff0000"}),
            "the local lenient color is normalized for the wire"
        );
    }

    #[tokio::test]
    async fn create_label_with_an_invalid_local_color_rejects_bad_request_without_sending() {
        let server = MockServer::start().await;
        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::CreateLabel { label: label_id(6) },
                    taskboard_domain::PushTarget::Label(label_id(6)),
                    &[],
                ),
                &[],
                tables_with(&[], &[label_row(6, "bug", "not-a-color")]),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            outcomes[0].result,
            PushResult::Rejected {
                kind: taskboard_domain::SyncErrorKind::BadRequest
            }
        ));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_label_with_a_missing_row_completes_synthetic_without_sending() {
        let server = MockServer::start().await;
        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::CreateLabel { label: label_id(6) },
                    taskboard_domain::PushTarget::Label(label_id(6)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            outcomes[0].result,
            PushResult::Applied { echo: None }
        ));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_label_transport_failure_aborts() {
        let client = refused_client();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::CreateLabel { label: label_id(6) },
                    taskboard_domain::PushTarget::Label(label_id(6)),
                    &[],
                ),
                &[],
                tables_with(&[], &[label_row(6, "bug", "ff0000")]),
            )
            .await;
        assert!(matches!(
            outcome,
            GroupOutcome::Aborted { landed } if landed.is_empty()
        ));
    }

    #[tokio::test]
    async fn update_label_puts_the_changeset_and_applies_the_echo() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/labels/3"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(label_json(3, "critical", "0000ff")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[(6, 3)]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateLabel { label: label_id(6) },
                    taskboard_domain::PushTarget::Label(label_id(6)),
                    &[],
                ),
                &[],
                tables_with(&[], &[label_row(6, "critical", "0000ff")]),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            &outcomes[0].result,
            PushResult::Applied { echo: Some(RemoteEcho::Label(view)) } if view.title == "critical"
        ));
        let body: serde_json::Value =
            serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
        assert_eq!(body["title"], "critical");
    }

    #[tokio::test]
    async fn update_of_an_unbound_label_rejects_local_data_without_sending() {
        let server = MockServer::start().await;
        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateLabel { label: label_id(6) },
                    taskboard_domain::PushTarget::Label(label_id(6)),
                    &[],
                ),
                &[],
                tables_with(&[], &[label_row(6, "critical", "0000ff")]),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            outcomes[0].result,
            PushResult::Rejected {
                kind: taskboard_domain::SyncErrorKind::LocalData
            }
        ));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn update_label_with_an_unencodable_color_rejects_bad_request_without_sending() {
        let server = MockServer::start().await;
        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[(6, 3)]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::UpdateLabel { label: label_id(6) },
                    taskboard_domain::PushTarget::Label(label_id(6)),
                    &[],
                ),
                &[],
                tables_with(&[], &[label_row(6, "critical", "#0000ff")]),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            outcomes[0].result,
            PushResult::Rejected {
                kind: taskboard_domain::SyncErrorKind::BadRequest
            }
        ));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_label_unbound_completes_synthetic_without_sending() {
        let server = MockServer::start().await;
        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::DeleteLabel { label: label_id(6) },
                    taskboard_domain::PushTarget::Label(label_id(6)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            outcomes[0].result,
            PushResult::Applied { echo: None }
        ));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_label_sends_the_delete_and_applies_the_echo() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/index.php/apps/deck/api/v1.0/boards/42/labels/3"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(label_json(3, "critical", "0000ff")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = DeckClient::new(&server.uri(), "u", "t").unwrap();
        let mut exec = PushExecutor::new(&client, overlay_with(42, &[], &[], &[(6, 3)]));
        let outcome = exec
            .execute(
                &group(
                    100,
                    MaterializedOp::DeleteLabel { label: label_id(6) },
                    taskboard_domain::PushTarget::Label(label_id(6)),
                    &[],
                ),
                &[],
                empty_tables(),
            )
            .await;
        server.verify().await;

        let GroupOutcome::Outcomes(outcomes) = outcome else {
            panic!("expected outcomes");
        };
        assert!(matches!(
            &outcomes[0].result,
            PushResult::Applied { echo: Some(RemoteEcho::Label(view)) }
                if view.id.label == RemoteLabelId(3)
        ));
    }

    /// Writes one minimal HTTP/1.1 response carrying the card JSON, then
    /// closes the connection (`Connection: close` forces the next request
    /// onto a fresh connection, which the test server then drops).
    fn serve_one_create(mut stream: std::net::TcpStream) {
        use std::io::{Read, Write};
        let mut buf = [0_u8; 4096];
        let mut request = Vec::new();
        // Read until the end of the headers, then the content-length body.
        loop {
            let Ok(n) = stream.read(&mut buf) else { return };
            if n == 0 {
                return;
            }
            request.extend_from_slice(&buf[..n]);
            let Some(header_end) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..header_end]).to_lowercase();
            let length: usize = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
            if request.len() >= header_end + 4 + length {
                break;
            }
        }
        let body = card_json(55, 7, 0);
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.flush();
    }
}

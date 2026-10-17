// SPDX-License-Identifier: MIT OR Apache-2.0
//! The per-`MaterializedOp` push decision table and its task arms
//! (stratum 2).

use taskboard_domain::{
    CardShape, EntityTables, LocalOp, MaterializedOp, PendingOp, PushGroup, PushOutcome,
    PushResult, RemoteBoardId, RemoteCardRef, RemoteEcho, RemoteLabelId, RemoteStackId,
};

use crate::client::DeckClient;
use crate::color::DeckColor;
use crate::error::DeckError;
use crate::mapping::{self, PushCtx};
use crate::model::{Card, StackFilter};

use super::GroupOutcome;
use super::outcomes::{
    all_applied, cannot_ride_a_task_group, classify, primary_outcome, reject_kind, reject_primary,
};
use super::overlay::BindingOverlay;

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
            MaterializedOp::Noop => GroupOutcome::Outcomes(all_applied(group)),
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
                None => GroupOutcome::Outcomes(all_applied(group)),
                Some(card_ref) => self.delete_card(group, card_ref).await,
            },
            MaterializedOp::CreateStack { stack } => {
                let Some(row) = tables.stacks.get(&stack) else {
                    return GroupOutcome::Outcomes(all_applied(group));
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
                        GroupOutcome::Outcomes(primary_outcome(
                            group,
                            PushResult::Applied {
                                echo: Some(mapping::map_stack_echo(&created, self.overlay.board)),
                            },
                        ))
                    }
                    Err(err) => classify(
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
                    return GroupOutcome::Outcomes(all_applied(group));
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
                    Ok(updated) => GroupOutcome::Outcomes(primary_outcome(
                        group,
                        PushResult::Applied {
                            echo: Some(taskboard_domain::RemoteEcho::Board(mapping::map_board(
                                &updated,
                            ))),
                        },
                    )),
                    Err(err) => classify(
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
                    return reject_primary(group);
                };
                let Some(row) = tables.stacks.get(&stack) else {
                    return GroupOutcome::Outcomes(all_applied(group));
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
                    Ok(updated) => GroupOutcome::Outcomes(primary_outcome(
                        group,
                        PushResult::Applied {
                            echo: Some(mapping::map_stack_echo(&updated, self.overlay.board)),
                        },
                    )),
                    Err(err) => classify(
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
                    None => GroupOutcome::Outcomes(all_applied(group)),
                    Some(stack_num) => match self
                        .client
                        .delete_stack(self.overlay.board.get(), stack_num.get())
                        .await
                    {
                        Ok(deleted) => GroupOutcome::Outcomes(primary_outcome(
                            group,
                            PushResult::Applied {
                                echo: Some(mapping::map_stack_echo(&deleted, self.overlay.board)),
                            },
                        )),
                        Err(err) => classify(
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
                    return GroupOutcome::Outcomes(all_applied(group));
                };
                let Ok(color) = DeckColor::from_hex(row.color.as_str()) else {
                    return reject_kind(
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
                        GroupOutcome::Outcomes(primary_outcome(
                            group,
                            PushResult::Applied {
                                echo: Some(mapping::map_label_echo(&created, self.overlay.board)),
                            },
                        ))
                    }
                    Err(err) => classify(
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
                    return reject_primary(group);
                };
                let Some(row) = tables.labels.get(&label) else {
                    return GroupOutcome::Outcomes(all_applied(group));
                };
                let Some(changes) = mapping::label_changes_from(row) else {
                    return reject_kind(
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
                    Ok(updated) => GroupOutcome::Outcomes(primary_outcome(
                        group,
                        PushResult::Applied {
                            echo: Some(mapping::map_label_echo(&updated, self.overlay.board)),
                        },
                    )),
                    Err(err) => classify(
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
                    None => GroupOutcome::Outcomes(all_applied(group)),
                    Some(label_num) => match self
                        .client
                        .delete_label(self.overlay.board.get(), label_num.get())
                        .await
                    {
                        Ok(deleted) => GroupOutcome::Outcomes(primary_outcome(
                            group,
                            PushResult::Applied {
                                echo: Some(mapping::map_label_echo(&deleted, self.overlay.board)),
                            },
                        )),
                        Err(err) => classify(
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
            return reject_primary(group);
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
                return classify(
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
            if cannot_ride_a_task_group(&op.op) {
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
            Err(err) => return Err(classify(group, &err, ctx)),
        }
        for filter in [StackFilter::Active, StackFilter::Archived] {
            let stacks = match self.client.stacks(card_ref.board.get(), filter).await {
                Ok(stacks) => stacks,
                Err(err) => return Err(classify(group, &err, ctx)),
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
                Err(err) => Err(classify(group, &err, ctx)),
            };
        }
        Err(classify(group, &DeckError::NotFound, ctx))
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
            return reject_primary(group);
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
                return classify(
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
            if cannot_ride_a_task_group(&op.op) {
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
            return reject_primary(group);
        };
        let Some(target_stack) = self.overlay.stack_by_local.get(&to.0).copied() else {
            return reject_primary(group);
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
            Err(err) => classify(
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
            Ok(deleted) => GroupOutcome::Outcomes(primary_outcome(
                group,
                PushResult::Applied {
                    echo: Some(RemoteEcho::Task(mapping::map_card(
                        &deleted,
                        card_ref.board,
                    ))),
                },
            )),
            Err(err) => classify(
                group,
                &err,
                PushCtx {
                    existing: true,
                    delete: true,
                },
            ),
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
            return GroupOutcome::Outcomes(primary_outcome(
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

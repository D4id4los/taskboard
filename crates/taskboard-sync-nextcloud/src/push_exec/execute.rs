// SPDX-License-Identifier: MIT OR Apache-2.0
//! The per-`MaterializedOp` push decision table and its task arms
//! (stratum 2).

use taskboard_domain::{
    CardShape, DeckFeatures, EntityTables, LocalOp, MaterializedOp, PendingOp, PushGroup,
    PushOutcome, PushResult, RemoteBoardId, RemoteCardRef, RemoteEcho, RemoteLabelId,
    RemoteStackId,
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
    features: DeckFeatures,
}

impl<'a> PushExecutor<'a> {
    /// Builds an executor seeded with the read state's bindings and the
    /// actor's probed [`DeckFeatures`] (the capability-gated `scheduled`
    /// conversion reads them; D-V2/D21).
    #[must_use]
    pub fn new(client: &'a DeckClient, overlay: BindingOverlay, features: DeckFeatures) -> Self {
        Self {
            client,
            overlay,
            features,
        }
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
            // The flush is deferred by design (D-S4 item 4's position
            // ruling): the actor stashes document groups and executes
            // them between the cycle's pull and its report. This arm
            // exists so the match stays total — the actor never relies
            // on it ("empty means deferred to the next cycle").
            MaterializedOp::FlushAppData { .. } => GroupOutcome::Outcomes(Vec::new()),
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
                Some(card_ref) => self.delete_card(group, task, card_ref).await,
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
            MaterializedOp::DeleteBoard { .. } => {
                // The planner only materializes a delete for the bound
                // board; the overlay's board *is* that binding, so the
                // unbound case cannot reach here (the `all_applied` guard
                // mirrors `DeleteStack`'s defensive shape regardless).
                let MaterializedOp::DeleteBoard { board } = group.materialized else {
                    unreachable!("matched arm");
                };
                if !tables.boards.contains_key(&board) {
                    return GroupOutcome::Outcomes(all_applied(group));
                }
                match self.client.delete_board(self.overlay.board.get()).await {
                    Ok(deleted) => GroupOutcome::Outcomes(primary_outcome(
                        group,
                        PushResult::Applied {
                            echo: Some(RemoteEcho::Board(mapping::map_board(&deleted))),
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
            MaterializedOp::UpdateBoard { board } => {
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
                &mapping::new_card_from(new_card, self.features),
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
        match self.relocate_card(group, task, &card_ref, ctx).await {
            Ok(Some(fresh_stack)) => {
                let fresh_ref = RemoteCardRef {
                    board: card_ref.board,
                    stack: fresh_stack,
                    card: card_ref.card,
                };
                // The detail outranks the (possibly lagging) listing.
                match self
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
                }
            }
            Ok(None) => Err(classify(group, &DeckError::NotFound, ctx)),
            Err(outcome) => Err(outcome),
        }
    }

    /// One re-resolve against the fresh active+archived listing pair
    /// (ADR 0007's #258 amendment): finds the stack the card actually
    /// lives under, rebinds the overlay, and returns it — or `None`
    /// when both listings confirm absence. A listing failure
    /// classifies through the normative table with the caller's `ctx`
    /// (the delete path passes the delete ctx, so a listing 403 is
    /// "gone" there).
    async fn relocate_card(
        &mut self,
        group: &PushGroup,
        task: taskboard_domain::TaskId,
        card_ref: &RemoteCardRef,
        ctx: PushCtx,
    ) -> Result<Option<RemoteStackId>, GroupOutcome> {
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
            self.overlay
                .task_by_local
                .insert(task, (fresh_stack, card_ref.card));
            return Ok(Some(fresh_stack));
        }
        Ok(None)
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
        let body = mapping::apply_card_shape(&fetched, shape, Some(&[]), self.features);
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
        // "assigned" is decided against the *fetched* set. `settled`
        // tracks the remote numbers the delta has brought to the desired
        // state, so a later failure can tell an op whose write landed
        // from one the delta never reached.
        let mut settled = std::collections::BTreeSet::new();
        let mut current = fetched_labels;
        for label_num in &desired {
            if current.contains(label_num) {
                settled.insert(*label_num);
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
                    settled.insert(*label_num);
                }
                Err(err) => {
                    return self.label_delta_failure(
                        group,
                        &updated,
                        card_ref.board,
                        &err,
                        *label_num,
                        &settled,
                        outbox,
                    );
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
                Ok(_) => {
                    settled.insert(label_num);
                }
                Err(err) => {
                    return self.label_delta_failure(
                        group,
                        &updated,
                        card_ref.board,
                        &err,
                        label_num,
                        &settled,
                        outbox,
                    );
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
        outcomes.extend(Self::subsumed_task_outcomes(group, outbox, &|label| {
            self.subsumed_label_result(label, None, &settled)
        }));
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

    /// Deletes at the overlay's ref. A delete-path 404 may be a stale
    /// binding (another client moved the card) rather than a deletion,
    /// so it re-resolves the card once against the fresh listings and
    /// retries the delete at the fresh location — at most one
    /// re-resolve and one retried delete, no loop.
    async fn delete_card(
        &mut self,
        group: &PushGroup,
        task: taskboard_domain::TaskId,
        card_ref: RemoteCardRef,
    ) -> GroupOutcome {
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
            Err(DeckError::NotFound) => self.delete_after_re_resolve(group, task, card_ref).await,
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

    /// The delete-path 404's tail, under the delete ctx throughout: a
    /// relocated card is deleted once at its fresh stack (any failure
    /// there — including a second 404, the write path outranking a
    /// lagging listing — classifies as itself); absence from both
    /// listings stays `NotFound` → `RemoteMissing`, which for a delete
    /// converges rather than loses (ADR 0007's amendment).
    async fn delete_after_re_resolve(
        &mut self,
        group: &PushGroup,
        task: taskboard_domain::TaskId,
        card_ref: RemoteCardRef,
    ) -> GroupOutcome {
        let ctx = PushCtx {
            existing: true,
            delete: true,
        };
        let fresh_stack = match self.relocate_card(group, task, &card_ref, ctx).await {
            Ok(Some(fresh_stack)) => fresh_stack,
            Ok(None) => return classify(group, &DeckError::NotFound, ctx),
            Err(outcome) => return outcome,
        };
        match self
            .client
            .delete_card(card_ref.board.get(), fresh_stack.get(), card_ref.card.get())
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
            Err(err) => classify(group, &err, ctx),
        }
    }
    /// One subsumed label op's result after the delta phase. `None`
    /// omits the op from the report (it stays queued): only a failing
    /// delta produces omissions, for labels the delta never reached.
    fn subsumed_label_result(
        &self,
        label: taskboard_domain::LabelId,
        failed: Option<(u64, &PushResult)>,
        settled: &std::collections::BTreeSet<u64>,
    ) -> Option<PushResult> {
        let Some(num) = self.overlay.label_by_local.get(&label).map(|n| n.get()) else {
            return Some(PushResult::Rejected {
                kind: taskboard_domain::SyncErrorKind::LocalData,
            });
        };
        if let Some((failed_num, result)) = failed
            && failed_num == num
        {
            return Some(result.clone());
        }
        if failed.is_none() || settled.contains(&num) {
            return Some(PushResult::Applied { echo: None });
        }
        None // never reached before the failure
    }

    /// The shared subsumed-op emission site: one outcome per subsumed
    /// op that can ride a task group, with label ops decided by
    /// `label_result` (`None` omits — stays queued). Updates/moves rode
    /// the PUT's field state and are always `Applied { echo: None }`.
    fn subsumed_task_outcomes(
        group: &PushGroup,
        outbox: &[PendingOp],
        label_result: &dyn Fn(taskboard_domain::LabelId) -> Option<PushResult>,
    ) -> Vec<PushOutcome> {
        let mut outcomes = Vec::new();
        for op_id in &group.subsumed {
            let Some(op) = outbox.iter().find(|entry| &entry.op_id == op_id) else {
                continue;
            };
            if cannot_ride_a_task_group(&op.op) {
                continue;
            }
            match op.op {
                LocalOp::AssignLabel(_, label) | LocalOp::UnassignLabel(_, label) => {
                    if let Some(result) = label_result(label) {
                        outcomes.push(PushOutcome { op: *op_id, result });
                    }
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
        outcomes
    }

    /// Classify-or-emit tail shared by the label-delta sub-endpoints
    /// (assign/remove). A *classified* failure of the write for remote
    /// number `failed_num` reports the fields' PUT as landed — its echo
    /// is authoritative for the primary — and emits one outcome per
    /// subsumed op by its own class: ops whose write landed (or already
    /// matched in `settled`) are `Applied`, an unresolvable label
    /// rejects `LocalData`, the failing label's ops carry the
    /// classified result per-op, and ops the delta never reached are
    /// omitted — they stay queued and retry next cycle. A
    /// transport-class failure aborts the cycle with nothing reported:
    /// the PUT was a full-send of fields that stay unchanged across the
    /// retry, so re-sending the whole group is safe.
    #[allow(clippy::too_many_arguments)] // the delta's full failure context
    fn label_delta_failure(
        &self,
        group: &PushGroup,
        updated: &Card,
        board: RemoteBoardId,
        err: &DeckError,
        failed_num: u64,
        settled: &std::collections::BTreeSet<u64>,
        outbox: &[PendingOp],
    ) -> GroupOutcome {
        let Some(classified) = mapping::classify_push(
            err,
            PushCtx {
                existing: true,
                delete: false,
            },
        ) else {
            tracing::warn!(error = %err, "push aborted: transport failure");
            return GroupOutcome::Aborted { landed: Vec::new() };
        };
        let mut outcomes = vec![PushOutcome {
            op: group.primary,
            result: PushResult::Applied {
                echo: Some(mapping::map_echo(updated, board)),
            },
        }];
        let failed = Some((failed_num, &classified));
        outcomes.extend(Self::subsumed_task_outcomes(group, outbox, &|label| {
            self.subsumed_label_result(label, failed, settled)
        }));
        GroupOutcome::Outcomes(outcomes)
    }
}

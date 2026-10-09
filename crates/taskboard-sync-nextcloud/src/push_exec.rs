// SPDX-License-Identifier: MIT OR Apache-2.0
//! The push group executor: turns one [`PushGroup`] into client calls,
//! resolving bindings through an in-cycle overlay and classifying every
//! outcome (phase 4 plan §5.3).
//!
//! The overlay is seeded from the read state's bindings and extended as
//! create echoes return — never re-derived by search (decision 7). Every
//! op that touches an existing bound card fetches the card first
//! (fetch-before-write, decision 5): the round-trip PUT is built from the
//! fetched card, and the fetched location protects reorder/delete from
//! stale-binding 404s that would be misread as `RemoteMissing`.

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
    /// logged at the abort site (`DeckError` is not `Clone`).
    Aborted,
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
            MaterializedOp::UpdateTask { task, card } => self.update_task(group, task, &card).await,
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
            MaterializedOp::RenameStack { stack } => {
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
                                        return GroupOutcome::Aborted;
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
                LocalOp::CreateTask(_)
                | LocalOp::DeleteTask(_)
                | LocalOp::CreateStack(_)
                | LocalOp::RenameStack(_)
                | LocalOp::DeleteStack(_)
                | LocalOp::CreateLabel(_)
                | LocalOp::UpdateLabel(_)
                | LocalOp::DeleteLabel(_) => {}
            }
        }
        if new_card.archived
            && let Err(err) = self
                .client
                .archive_card(self.overlay.board.get(), created.stack_id, created.id)
                .await
        {
            tracing::warn!(error = %err, "post-create archive failed; the flag repairs on a later cycle");
            if is_transport(&err) {
                {
                    tracing::warn!(error = %err, "push aborted: transport failure");
                    return GroupOutcome::Aborted;
                }
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
                if is_transport(&err) {
                    {
                        tracing::warn!(error = %err, "push aborted: transport failure");
                        return GroupOutcome::Aborted;
                    }
                }
            }
        }
        GroupOutcome::Outcomes(outcomes)
    }

    /// Full-send PUT built from the fetched card (fetch-before-write).
    /// A group whose only delta is unresolvable labels defers entirely.
    async fn update_task(
        &mut self,
        group: &PushGroup,
        task: taskboard_domain::TaskId,
        shape: &CardShape,
    ) -> GroupOutcome {
        let Some(card_ref) = self.overlay.card_of(task) else {
            return Self::reject_primary(group);
        };
        let fetched = match self
            .client
            .card(
                card_ref.board.get(),
                card_ref.stack.get(),
                card_ref.card.get(),
            )
            .await
        {
            Ok(fetched) => fetched,
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
        let (resolved, has_unresolved): (Vec<u64>, bool) = {
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
            && resolved.as_slice()
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
        let resolved_refs: Vec<u64> = if has_unresolved {
            resolved
        } else {
            shape
                .labels
                .iter()
                .filter_map(|l| self.overlay.label_by_local.get(l).map(|n| n.get()))
                .collect()
        };
        let body = mapping::apply_card_shape(&fetched, shape, Some(&resolved_refs));
        match self.client.update_card(card_ref.board.get(), &body).await {
            Ok(updated) => GroupOutcome::Outcomes(Self::primary_outcome(
                group,
                PushResult::Applied {
                    echo: Some(mapping::map_echo(&updated, card_ref.board)),
                },
            )),
            // A 2xx write whose echo failed to decode still landed.
            Err(DeckError::Envelope(_)) => GroupOutcome::Outcomes(Self::primary_outcome(
                group,
                PushResult::Applied { echo: None },
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
        let fetched = match self
            .client
            .card(
                card_ref.board.get(),
                card_ref.stack.get(),
                card_ref.card.get(),
            )
            .await
        {
            Ok(fetched) => fetched,
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

    /// Applies the classification table; `None` (transport class) aborts.
    fn classify(group: &PushGroup, err: &DeckError, ctx: PushCtx) -> GroupOutcome {
        match mapping::classify_push(err, ctx) {
            Some(result) => GroupOutcome::Outcomes(Self::primary_outcome(group, result)),
            // Transport class: the cycle aborts (decision 8) — no outcome.
            None => GroupOutcome::Aborted,
        }
    }
}

/// Transport-classified: the cycle aborts (no outcome), matching the
/// client's retry semantics ("network lost" = these after retries).
pub(crate) fn is_transport(err: &DeckError) -> bool {
    matches!(
        err,
        DeckError::Transport(_) | DeckError::RateLimited | DeckError::Unavailable
    )
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
        assert!(matches!(outcome, GroupOutcome::Aborted));
    }
}

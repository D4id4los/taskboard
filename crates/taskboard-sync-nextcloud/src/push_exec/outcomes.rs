// SPDX-License-Identifier: MIT OR Apache-2.0
//! Group-outcome assembly and error classification (stratum 3).

use taskboard_domain::{LocalOp, PushGroup, PushOutcome, PushResult};

use crate::error::DeckError;
use crate::mapping::{self, PushCtx};

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

// ---- outcome assembly -------------------------------------------

/// A task group's subsumed bucket carries label, update and move ops
/// only (`plan_task_groups`); these op classes have no outcome to
/// report there and stay queued untouched.
pub(super) fn cannot_ride_a_task_group(op: &LocalOp) -> bool {
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
            | LocalOp::UpdateBoard(_)
            | LocalOp::DeleteBoard(_)
    )
}

/// `Applied` for the primary, synthetic `Applied { echo: None }` for
/// every subsumed op (decision 6).
pub(super) fn primary_outcome(group: &PushGroup, result: PushResult) -> Vec<PushOutcome> {
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
pub(super) fn all_applied(group: &PushGroup) -> Vec<PushOutcome> {
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
pub(super) fn reject_primary(group: &PushGroup) -> GroupOutcome {
    GroupOutcome::Outcomes(vec![PushOutcome {
        op: group.primary,
        result: PushResult::Rejected {
            kind: taskboard_domain::SyncErrorKind::LocalData,
        },
    }])
}

pub(super) fn reject_kind(group: &PushGroup, result: PushResult) -> GroupOutcome {
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
pub(super) fn classify(group: &PushGroup, err: &DeckError, ctx: PushCtx) -> GroupOutcome {
    match mapping::classify_push(err, ctx) {
        Some(result @ PushResult::Applied { .. }) => {
            GroupOutcome::Outcomes(primary_outcome(group, result))
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

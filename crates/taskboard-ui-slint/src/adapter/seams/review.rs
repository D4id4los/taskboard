// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use taskboard_domain::StateCommand;

use super::Adapter;
use crate::AppWindow;
use crate::actions::ActionId;
use crate::adapter::install_seam;
use crate::view_state::{Flow, FlowOutcome};
use slint::ComponentHandle as _;

/// The review walk's seams (DR-7 chunk 3, #638): every button routes
/// the landed registry action — one dispatch path, the adapter's verb
/// table owns the semantics (the focus overlay's rule). The two text
/// mirrors (the answers, the date field) ride the plan buffers'
/// discipline; the candidate click arms the pick directly (the
/// one-shot target pattern).
/// The journal ledger's seams (T-15, #798): the row activation seeds
/// the inline editor from the subject's live state; the commit builds
/// the exact `journal edit` change mask (an all-unchanged commit is
/// the accepted no-op, clocks untouched); the delete stages through
/// the armed confirm (the tombstone is the family's inverse-of-record,
/// DR-7 D-J7 — the family's own verbs are the recovery surface, and
/// this editor IS that surface arriving in the GUI). No new `ActionId`s:
/// the ledger dispatches the engine commands directly (the labels
/// armed-delete precedent), keeping `ContextId::Review` binding-free.
#[allow(clippy::too_many_lines)] // the seam bundle: one block per intent
pub(in crate::adapter) fn install_review_ledger_seams(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    install_seam!(window: window.on_reflection_row_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(id) = a
            .tables
            .reflections
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            return; // a stale index against an older tables pass
        };
        // Seed the buffers from the subject's live state (the echo is
        // the stored row verbatim — never an excerpt).
        let state = a.state.load();
        let Some(entry) = state.reflection.entries.get(&id) else {
            return;
        };
        a.reflection_active = Some(id);
        a.reflection_text_draft.clone_from(&entry.text);
        a.reflection_mood_draft = entry
            .attributes
            .mood
            .map_or_else(String::new, |rating| rating.get().to_string());
        a.reflection_energy_draft = entry
            .attributes
            .energy
            .map_or_else(String::new, |rating| rating.get().to_string());
        a.reflection_flags_add.clear();
        a.reflection_flags_remove.clear();
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_reflection_editor_text_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.reflection_text_draft = text.to_string();

    });
    install_seam!(window: window.on_reflection_editor_mood_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.reflection_mood_draft = text.to_string();

    });
    install_seam!(window: window.on_reflection_editor_energy_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.reflection_energy_draft = text.to_string();

    });
    install_seam!(window: window.on_reflection_editor_flag_toggled, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(flag) = crate::presentation::HEALTH_FLAGS
            .get(usize::try_from(index).unwrap_or(usize::MAX))
        else {
            return;
        };
        let Some(id) = a.reflection_active else {
            return;
        };
        let state = a.state.load();
        let Some(entry) = state.reflection.entries.get(&id) else {
            return;
        };
        // The delta is relative to the subject's live set (the mask's
        // halves, never whole-set replace): a member chip toggles the
        // remove half, a non-member the add half.
        if entry.attributes.flags.contains(flag) {
            if !a.reflection_flags_remove.remove(flag) {
                a.reflection_flags_remove.insert(*flag);
            }
        } else if !a.reflection_flags_add.remove(flag) {
            a.reflection_flags_add.insert(*flag);
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_reflection_editor_committed, adapter: adapter;
    bind mut (a, window), args (), {        let Some(id) = a.reflection_active else {
            return;
        };
        let state = a.state.load();
        let Some(entry) = state.reflection.entries.get(&id) else {
            drop(state);
            a.reflection_active = None; // the subject went stale
            a.refresh_forced(&window);
            return;
        };
        // The text mask: a changed draft rides verbatim (the empty
        // text is the domain's legal clear).
        let text = (a.reflection_text_draft != entry.text)
            .then(|| a.reflection_text_draft.clone());
        // The rating masks: empty = clear (when set), a valid number =
        // set (when changed); an invalid draft refuses the commit (the
        // flavour renders; nothing dispatches).
        let rating_mask = |draft: &str,
                           current: Option<taskboard_domain::reflection::Rating>|
         -> Option<Option<taskboard_domain::reflection::Rating>> {
            if draft.is_empty() {
                current.map(|_| None)
            } else {
                match draft.parse::<u8>().ok().and_then(|n| {
                    taskboard_domain::reflection::Rating::try_new(n).ok()
                }) {
                    Some(rating) if Some(rating) != current => Some(Some(rating)),
                    _ => None,
                }
            }
        };
        let mood_valid = a.reflection_mood_draft.is_empty()
            || a
                .reflection_mood_draft
                .parse::<u8>()
                .ok()
                .and_then(|n| taskboard_domain::reflection::Rating::try_new(n).ok())
                .is_some();
        let energy_valid = a.reflection_energy_draft.is_empty()
            || a
                .reflection_energy_draft
                .parse::<u8>()
                .ok()
                .and_then(|n| taskboard_domain::reflection::Rating::try_new(n).ok())
                .is_some();
        if !mood_valid || !energy_valid {
            return; // an invalid draft never dispatches (the flavour renders)
        }
        // The command's payload carries the raw rating byte (the
        // planner re-mints — the trust boundary is the engine's).
        let mood = rating_mask(&a.reflection_mood_draft, entry.attributes.mood)
            .map(|inner| inner.map(taskboard_domain::reflection::Rating::get));
        let energy = rating_mask(&a.reflection_energy_draft, entry.attributes.energy)
            .map(|inner| inner.map(taskboard_domain::reflection::Rating::get));
        let flags_add = a.reflection_flags_add.clone();
        let flags_remove = a.reflection_flags_remove.clone();
        // An all-unchanged editor commits the accepted no-op (the
        // planner's contract — clocks untouched).
        let command = StateCommand::UpdateReflection {
            id,
            text,
            mood,
            energy,
            flags_add,
            flags_remove,
        };
        if a.commands.try_send(command).is_err() {
            tracing::warn!("journal edit dropped: command channel full or closed");
            return;
        }
        a.reflection_active = None;
        a.reflection_text_draft.clear();
        a.reflection_mood_draft.clear();
        a.reflection_energy_draft.clear();
        a.reflection_flags_add.clear();
        a.reflection_flags_remove.clear();
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_reflection_editor_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.reflection_active.take().is_some() {
            a.reflection_text_draft.clear();
            a.reflection_mood_draft.clear();
            a.reflection_energy_draft.clear();
            a.reflection_flags_add.clear();
            a.reflection_flags_remove.clear();
            a.refresh_forced(&window);
        }

    });
    install_seam!(window: window.on_reflection_remove_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(id) = a
            .tables
            .reflections
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            return; // a stale index against an older tables pass
        };
        // Arming is a UI-state toggle (the settings armed-item
        // letter): one armed subject surface-wide, arming touches no
        // engine state.
        a.reflection_armed = if a.reflection_armed == Some(id) {
            None
        } else {
            Some(id)
        };
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_reflection_remove_confirmed, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(id) = a
            .tables
            .reflections
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            a.reflection_armed = None; // a stale confirm disarms
            return;
        };
        // The armed identity types the verb: clears the flag BEFORE
        // the send; a stale index disarms silently.
        let Some(armed) = a.reflection_armed.take() else {
            return;
        };
        if armed != id {
            return;
        }
        if a
            .commands
            .try_send(StateCommand::DeleteReflection { id })
            .is_err()
        {
            tracing::warn!("journal delete dropped: command channel full or closed");
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_reflection_remove_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.reflection_armed.take().is_some() {
            a.refresh_forced(&window);
        }

    });
}

pub(in crate::adapter) fn install_review_walk_seams(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    install_seam!(window: window.on_walk_abort_arm_requested, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewAbortArm);
    });
    install_seam!(window: window.on_walk_abort_accept_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewAbortAccept);
    });
    install_seam!(window: window.on_walk_abort_dismiss_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewAbortDismiss);
    });
    install_seam!(window: window.on_walk_plan_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewWalkPlanner);
    });
    install_seam!(window: window.on_walk_button_invoked, adapter: adapter; // walk-button-invoked
    bind mut (a, window), args (code), {        let action = match code {
            0 => ActionId::ReviewWalkPrev,
            1 => ActionId::ReviewWalkAdvance,
            2 => ActionId::ReviewFinish,
            3 => ActionId::ReviewWalkComplete,
            _ => return, // a stale code past the pushed row: inert
        };
        a.route(&window, action);
    });
    install_seam!(window: window.on_walk_file_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewInboxFile);
    });
    install_seam!(window: window.on_walk_keep_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewInboxKeep);
    });
    install_seam!(window: window.on_walk_file_prev_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewFileTargetPrev);
    });
    install_seam!(window: window.on_walk_file_next_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewFileTargetNext);
    });
    install_seam!(window: window.on_walk_candidate_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let index = usize::try_from(index).unwrap_or(usize::MAX);
    let state = a.state.load();
    let candidates = crate::presentation::walk_file_candidates(&state);
    let Some(stack) = candidates.get(index).copied() else {
        return; // a stale index past the candidates: inert
    };
    if let Some(Flow::WalkFilePick { armed }) = a.view_state.flow_mut() {
        *armed = Some(stack);
    }
    a.apply_outcomes(&window, &[FlowOutcome::RefreshNow]);
    a.finish_mutation(&window);
    });
    install_seam!(window: window.on_walk_file_commit_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewFileCommit);
    });
    install_seam!(window: window.on_walk_tomorrow_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewCarryTomorrow);
    });
    install_seam!(window: window.on_walk_pick_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewCarryPickDate);
    });
    install_seam!(window: window.on_walk_leave_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewCarryLeave);
    });
    install_seam!(window: window.on_walk_date_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        if let Some(child) = a.view_state.walk_date_mut() {
        child.date_text = text.to_string();
    }
    });
    install_seam!(window: window.on_walk_date_commit_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewCarryDateCommit);
    });
    install_seam!(window: window.on_walk_answer_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (index, text), {        let index = usize::try_from(index).unwrap_or(usize::MAX);
    let Some(walk) = a.view_state.walk_mut() else {
        return;
    };
    if index >= walk.answers.len() {
        walk.answers.resize(index + 1, String::new());
    }
    walk.answers[index] = text.to_string();
    });
    install_seam!(window: window.on_walk_rating_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (kind, text), {        let Some(walk) = a.view_state.walk_mut() else {
        return;
    };
    // Kind 0 = mood, 1 = energy (the overlay's field order); the
    // drafts mirror verbatim — the parse is Record's job (the T-15
    // boundary law: store the text, mint at the trust boundary).
    match kind {
        0 => walk.mood_text = text.to_string(),
        _ => walk.energy_text = text.to_string(),
    }
    });
    install_seam!(window: window.on_walk_record_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewRecord);
    });
    install_seam!(window: window.on_walk_finish_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewFinish);
    });
    // The Today card's activation: the routed opener (the same
    // dispatch as the action — the reference derivation and the
    // kiosk refusal live in the adapter's arms).
    install_seam!(window: window.on_review_card_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReviewOpen);
    });
}

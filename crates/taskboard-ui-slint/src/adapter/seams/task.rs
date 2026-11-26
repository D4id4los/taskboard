// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use taskboard_domain::{ReminderOffset, StateCommand};

use super::super::{EffortEditorState, LabelFormState};
use super::Adapter;
use super::{commit_effort_editor, commit_text, slot_importance, slot_urgency};
use crate::AppWindow;
use crate::actions::ActionId;
use crate::adapter::install_seam;
use slint::ComponentHandle as _;

/// The task-detail title field: mirror + commit (D14). The rule-list
/// composition's seams (chunk 6, D13): index/preset/custom resolution
/// against the id tables and the domain grammar, then the one-shot +
/// route — the banner-seam template.
#[allow(clippy::too_many_lines)] // one wiring sequence, kept flat for review
pub(in crate::adapter) fn install_task_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // The closed title's click (round-3 W-SM-d, F26): routes the
    // `task.title-edit` action — click ≡ key (§7.1); re-arming after
    // one close is always possible.
    // The focus-loss settle (round-3 L1, plan §7): every text flow's
    // field forwards here. The `try_borrow_mut` absorb discipline
    // (the drag_finished echo precedent): a settle's unmount fires
    // the teardown `focus-lost` synchronously inside
    // `apply_outcomes`' borrow — that echo must be absorbed, and it
    // can never mask a legitimate settle (a genuine focus move
    // happens outside any borrow, where the borrow succeeds).
    install_seam!(window: window.on_text_flow_focus_lost, adapter: adapter; [try]
    bind mut (a, window), args (), {        // The clip-focus law's absorb (#1419, operator ruling
        // 2026-11-25, issue comment 7172599685): a focus-lost that
        // fires while an echo window is open is the synthetic unmount
        // echo of the focused field's rebuild — not a user dismissal
        // — and is absorbed; a field that lost its mount cannot lose
        // focus twice, so the FIRST focus-lost of a window is the
        // echo: absorbed and disarmed in one step. Every later
        // focus-lost is a genuine settle. Deliberate leniency
        // (recorded): a genuine blur arriving as the very first
        // event of a window is absorbed once — the flow stays armed
        // and the next focus cycle settles it; Return-commit is
        // unaffected. (The law's remount half — a rebuild-focusless
        // field re-grabbing — is NOT implemented here; its first
        // mechanism was withdrawn and the requirement rides #1490.)
        if a.refresh_push_in_progress.get() {
            a.refresh_push_in_progress.set(false);
            // Flow-aware classification (see the comment head): an
            // armed flow's loss is a genuine dismissal and settles; a
            // flowless loss is the reconcile's teardown echo — settle
            // would only clear the selection and clobber the pass's
            // focus-origin restore (us1_s2).
            if matches!(a.view_state.flow(), crate::view_state::Flow::None) {
                return;
            }
        }
        let state = a.state.load_full();
    a.settle_flow(&window, &state);

    });
    install_seam!(window: window.on_task_title_edit_activated, adapter: adapter;
    bind mut (a, window), args (), {        a
        .route(&window, ActionId::TaskTitleEdit);

    });
    // The priority pickers' seams (UX R-3 D-1, #788): the echo rows'
    // edit affordances route the landed actions (click ≡ key), the
    // slot click commits the exact `UpdateTask` mask immediately
    // (the rule-preset posture — recoverable through `AppUndo`), and
    // the dismissal closes without dispatching.
    install_seam!(window: window.on_task_importance_edit_asked, adapter: adapter;
    bind mut (a, window), args (), {        a
        .route(&window, ActionId::TaskImportanceEdit);

    });
    install_seam!(window: window.on_task_urgency_edit_asked, adapter: adapter;
    bind mut (a, window), args (), {        a
        .route(&window, ActionId::TaskUrgencyEdit);

    });
    install_seam!(window: window.on_priority_slot_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a.view_state.subject_task() else {
            return;
        };
        let Some(field) = a.priority_open else {
            return; // a stale click against a closed picker
        };
        let slot = usize::try_from(index).unwrap_or(usize::MAX);
        let command = match (field, slot) {
            // Slot 0 is the unset twin — the CLI's `none` (the
            // parity contract, D-5); 1..=5 are the domain levels in
            // declaration order.
            (crate::router::PriorityField::Importance, 0) => {
                StateCommand::UpdateTask {
                    id: task,
                    changes: taskboard_domain::TaskChanges {
                        importance: Some(None),
                        ..taskboard_domain::TaskChanges::default()
                    },
                }
            }
            (crate::router::PriorityField::Urgency, 0) => {
                StateCommand::UpdateTask {
                    id: task,
                    changes: taskboard_domain::TaskChanges {
                        urgency: Some(None),
                        ..taskboard_domain::TaskChanges::default()
                    },
                }
            }
            (crate::router::PriorityField::Importance, slot) => {
                let Some(level) = slot_importance(slot) else {
                    return; // a stale index against the closed vocabulary
                };
                StateCommand::UpdateTask {
                    id: task,
                    changes: taskboard_domain::TaskChanges {
                        importance: Some(Some(level)),
                        ..taskboard_domain::TaskChanges::default()
                    },
                }
            }
            (crate::router::PriorityField::Urgency, slot) => {
                let Some(level) = slot_urgency(slot) else {
                    return;
                };
                StateCommand::UpdateTask {
                    id: task,
                    changes: taskboard_domain::TaskChanges {
                        urgency: Some(Some(level)),
                        ..taskboard_domain::TaskChanges::default()
                    },
                }
            }
        };
        // The commit closes the picker (the choice is made — the
        // echo re-renders on the next pass). The forced pass carries
        // the flag flip (the armed form's rule: a view-state change
        // outside the state Arc never rides the identity guard).
        a.priority_open = None;
        if a.commands.try_send(command).is_err() {
            tracing::warn!("priority slot commit dropped: command channel full or closed");
        }
        a.refresh_forced(&window);

    });
    // The labels section/overlay seams (UX R-3 D-2, #789): the add
    // affordance routes `label.manage` (click == key); the chip x
    // and the overlay toggles dispatch the Assign/Unassign pair
    // over the VIEW'S SUBJECT, resolved through the pass's tables
    // (the id-table seam law — a stale index absorbs typed).
    install_seam!(window: window.on_labels_manage_asked, adapter: adapter;
    bind mut (a, window), args (), {        a
        .route(&window, ActionId::LabelManage);

    });
    install_seam!(window: window.on_labels_overlay_dismiss_requested, adapter: adapter;
    bind mut (a, window), args (), {        a
        .route(&window, ActionId::LabelOverlayDismiss);

    });
    install_seam!(window: window.on_chip_unassign_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a.view_state.subject_task() else {
            return;
        };
        let Some(label) = a
            .tables
            .task_labels
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            return; // a stale index against an older tables pass
        };
        if a
            .commands
            .try_send(StateCommand::UnassignLabel { task, label })
            .is_err()
        {
            tracing::warn!("chip unassign dropped: command channel full or closed");
        }
        a.finish_mutation(&window);

    });
    install_seam!(window: window.on_labels_overlay_row_toggled, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(task) = a.view_state.subject_task() else {
            return;
        };
        let Some(label) = a
            .tables
            .label_ids
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            return; // a stale index against an older tables pass
        };
        // The row's pushed state names the pair's arm (D-2): the
        // engine's idempotence owns the rest — the CLI's own
        // contract, no GUI-side guard re-derives it.
        let assigned = a
            .state
            .load()
            .tasks
            .get(&task)
            .is_some_and(|t| t.live_labels().contains(&label));
        let command = if assigned {
            StateCommand::UnassignLabel { task, label }
        } else {
            StateCommand::AssignLabel { task, label }
        };
        if a.commands.try_send(command).is_err() {
            tracing::warn!("label toggle dropped: command channel full or closed");
        }
        a.finish_mutation(&window);

    });
    // The labels form's seams (UX R-3 D-2/D-5, #789): the row verbs
    // arm the in-place draft (the adapter owns it — the due buffers'
    // pattern), the commit funnels through the one sparse-mask
    // assembly (the changed slots only — the LabelChanges
    // discipline; a no-op edit closes without dispatching), and an
    // empty title is the typed inert (the form stays open — the
    // entries commit's absorbed-miss convention).
    install_seam!(window: window.on_labels_overlay_add_requested, adapter: adapter;
    bind mut (a, window), args (), {        if !matches!(a.view_state.flow(), crate::view_state::Flow::Labels { .. }) {
            return;
        }
        a.label_form = Some(LabelFormState {
            seed: None,
            title: String::new(),
            color: crate::presentation::SWATCHES[0].to_owned(),
        });
        a.label_delete_armed = None;
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_labels_overlay_edit_requested, adapter: adapter;
    bind mut (a, window), args (index), {        if !matches!(a.view_state.flow(), crate::view_state::Flow::Labels { .. }) {
            return;
        }
        let Some(label) = a
            .tables
            .label_ids
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            return; // a stale index against an older tables pass
        };
        let state = a.state.load_full();
        let Some(label) = state.labels.get(&label) else {
            return; // the roster pass's label died mid-interaction
        };
        a.label_form = Some(LabelFormState {
            seed: Some(label.id),
            title: label.title.clone(),
            color: label.color.as_str().to_owned(),
        });
        drop(state);
        a.label_delete_armed = None;
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_labels_overlay_title_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        if let Some(form) = a.label_form.as_mut() {
            form.title = text.to_string();
        }

    });
    // The free-form colour draft (tag-color plan D-5, #839): the
    // draft buffer takes the typed text, then the forced refresh
    // re-echoes the validity/preview halves — the field's text is
    // mount-only-read (never push-ward while typing), so the refresh
    // cannot fight the caret (the swatch-clicked shape; the
    // title-edited bare-mut stays bare because no pushed half reads
    // the title buffer).
    // slint: colour-edited
    install_seam!(window: window.on_labels_overlay_colour_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(form) = a.label_form.as_mut() {
            form.color = text.to_string();
            a.refresh_forced(&window);
        }

    });
    // The picker's draft-write seams (journey plan
    // `…-schedule-colour-ux-plan.md` D-3, US-28/S1+S4, the W-3/W-7
    // rows): the projected value is quantized through the conversion
    // the mint owns (`hsv_to_hex`, the 6-hex shape) into the draft
    // buffer, then the forced refresh re-projects the picker — the
    // draft string is the single truth, the picker its projection.
    // No command: the overlay's commit is the funnel.
    // slint: hue-picked
    install_seam!(window: window.on_labels_overlay_hue_picked, adapter: adapter;
    bind mut (a, window), args (hue), {        let hue = f64::from(hue);
        if let Some(form) = a.label_form.as_mut() {
            // The untouched axes ride the last-valid draft's own
            // reading (the projection is only ever re-projected).
            let (_, sat, val) =
                crate::presentation::hex_to_hsv(&form.color).unwrap_or((hue, 1.0, 1.0));
            form.color = crate::presentation::hsv_to_hex(hue, sat, val);
            a.refresh_forced(&window);
        }

    });
    // slint: sat-val-picked
    install_seam!(window: window.on_labels_overlay_sat_val_picked, adapter: adapter;
    bind mut (a, window), args (sat, val), {        let (sat, val) = (f64::from(sat), f64::from(val));
        if let Some(form) = a.label_form.as_mut() {
            let (hue, _, _) =
                crate::presentation::hex_to_hsv(&form.color).unwrap_or((0.0, sat, val));
            form.color = crate::presentation::hsv_to_hex(hue, sat, val);
            a.refresh_forced(&window);
        }

    });
    install_seam!(window: window.on_labels_overlay_swatch_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let palette = crate::presentation::derived_swatches();
        let Some(swatch) = palette.get(usize::try_from(index).unwrap_or(usize::MAX))
        else {
            return; // a stale palette index
        };
        if let Some(form) = a.label_form.as_mut() {
            form.color.clone_from(swatch);
            a.refresh_forced(&window);
        }

    });
    install_seam!(window: window.on_labels_overlay_form_commit_requested, adapter: adapter; [else ( false)]
    bind mut (a, window), args (), {        let Some(form) = a.label_form.clone() else {
            return false; // a stale commit against a disarmed form
        };
        let title = form.title.trim().to_owned();
        if title.is_empty() {
            return false; // the typed inert — the form stays open
        }
        // The mint at the boundary (tag-color plan D-6, US-1/AC-2):
        // an invalid colour is the typed inert — the form stays open,
        // nothing dispatches (the empty-title inert's twin).
        let Ok(color) = taskboard_domain::Color::parse(&form.color) else {
            return false;
        };
        let state = a.state.load_full();
        // Labels are board-scoped (D-2/D-6, tag-color plan): the
        // create board is the flow subject's — the task mount derives
        // the subject task's board; the roster-only mount names its
        // board directly (no task subject needed).
        let board = if let Some(crate::view_state::LabelOverlaySubject::Board(board)) =
            a.view_state.labels_subject()
        {
            board
        } else {
            let Some(task) = a.view_state.subject_task() else {
                return false;
            };
            let Some(task) = state.tasks.get(&task) else {
                return false;
            };
            let Some(board) = state.stacks.get(&task.stack).map(|stack| stack.board) else {
                return false;
            };
            board
        };
        let command = match form.seed {
            None => StateCommand::CreateLabel {
                board,
                title: title.clone(),
                color: color.clone(),
            },
            Some(id) => {
                let Some(label) = state.labels.get(&id) else {
                    return false; // the seeded label died mid-edit
                };
                // The sparse-mask discipline (D-2): only the changed
                // slots set; a no-op edit closes without dispatching.
                let title_change = (label.title != title).then(|| title.clone());
                // The comparison reads the minted draft: a
                // case-only-different draft equals the stored
                // lowercase string, so the sparse mask stays honest
                // (the mint's normalization, plan D-6).
                let color_change = (label.color.as_str() != color.as_str())
                    .then(|| color.clone());
                if title_change.is_none() && color_change.is_none() {
                    a.label_form = None;
                    a.refresh_forced(&window);
                    return true;
                }
                StateCommand::UpdateLabel {
                    id,
                    changes: taskboard_domain::LabelChanges {
                        title: title_change,
                        color: color_change,
                    },
                }
            }
        };
        a.label_form = None;
        if a.commands.try_send(command).is_err() {
            tracing::warn!("label form commit dropped: command channel full or closed");
            return false;
        }
        a.finish_mutation(&window);
        true

    });
    install_seam!(window: window.on_labels_overlay_form_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.label_form.take().is_some() {
            a.refresh_forced(&window);
        }

    });
    // The armed delete (UX R-3 D-2, #789): the r2 grammar on the
    // overlay rows — the first activation arms the row (the
    // consequence-naming strip; nothing dispatches), the Confirm
    // dispatches the exact `DeleteLabel` (the verb is
    // `NotUndoable(RestoreClass)` — staging is the requirement), and
    // Cancel/Escape disarms. Arming a different row re-arms.
    install_seam!(window: window.on_labels_overlay_delete_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(label) = a
            .tables
            .label_ids
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            return; // a stale index against an older tables pass
        };
        // US-6 re-arm guard (#840, D-11): a stale-index/dead-label
        // activation — the id no longer resolving in the live roster —
        // never re-arms (the stale-index guard the edit seam already
        // uses); the fast double-click's second activation against the
        // deleted row cannot resurrect the armed identity.
        {
            let state = a.state.load_full();
            if state.labels.get(&label).is_none_or(|l| l.deleted) {
                return;
            }
        }
        // Arming swaps out an open form (the surfaces are
        // mutually exclusive).
        a.label_form = None;
        if a.label_delete_armed == Some(label) {
            // The second activation IS the confirm.
            a.label_delete_armed = None;
            if a
                .commands
                .try_send(StateCommand::DeleteLabel { id: label })
                .is_err()
            {
                tracing::warn!("label delete dropped: command channel full or closed");
            }
        } else {
            a.label_delete_armed = Some(label);
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_labels_overlay_delete_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.label_delete_armed.take().is_some() {
            a.refresh_forced(&window);
        }

    });
    // The restore sections' seams (the undo-family journey plan
    // W-2/W-9): the armed-confirm grammar over the published roster's
    // row order — the index resolves against the same filtered row
    // list the push derived (the publish lockstep bounds the race,
    // I-2); a stale index is the typed inert (nothing dispatches).
    // The first activation arms (the strip mounts), the Confirm
    // dispatches the exact restore command, Cancel disarms (the
    // D-2 grammar).
    install_seam!(window: window.on_label_restore_requested, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(index_us) = usize::try_from(index).ok() else {
            return;
        };
        // The same board-scoped row source the push derived.
        let overlay_subject = a.view_state.labels_subject();
        let subject_board = match overlay_subject {
            Some(crate::view_state::LabelOverlaySubject::Board(board)) => Some(board),
            _ => a
                .view_state
                .subject_task()
                .and_then(|t| a.state.load_full().tasks.get(&t).cloned())
                .and_then(|t| a.state.load_full().stacks.get(&t.stack).cloned())
                .map(|stack| stack.board),
        };
        let state = a.state.load_full();
        let row_id = state
            .roster
            .tombstoned_labels
            .iter()
            .filter(|row| {
                state
                    .labels
                    .get(&row.id)
                    .is_some_and(|label| Some(label.board) == subject_board)
            })
            .nth(index_us)
            .map(|row| row.id);
        drop(state);
        let Some(label) = row_id else {
            return; // a stale index against an older roster pass
        };
        if a.label_restore_armed == Some(label) {
            // The second activation IS the confirm: the exact
            // `RestoreLabel` (the receipt rides the unchanged
            // outcome-notice lane, W-6).
            a.label_restore_armed = None;
            if a
                .commands
                .try_send(StateCommand::RestoreLabel { id: label })
                .is_err()
            {
                tracing::warn!("label restore dropped: command channel full or closed");
            }
        } else {
            // One strip at a time (the habits twin's law): arming the
            // per-row disarm's the bulk ceremony.
            a.label_restore_all_armed = false;
            a.label_restore_armed = Some(label);
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_label_restore_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.label_restore_armed.take().is_some() {
            a.refresh_forced(&window);
        }

    });
    install_seam!(window: window.on_entry_restore_requested, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(index_us) = usize::try_from(index).ok() else {
            return;
        };
        let state = a.state.load_full();
        let journal_id = state
            .roster
            .deleted_entries
            .get(index_us)
            .map(|row| row.journal_id);
        drop(state);
        let Some(from) = journal_id else {
            return; // a stale index against an older roster pass
        };
        if a.entry_restore_armed == Some(from) {
            // The second activation IS the confirm: the zero-input
            // payload carries the journal id only (D-5) — the engine
            // reads the pre-image and applies the pinned
            // `RestoreTimeEntry` internally (the `EntryRestored`
            // receipt rides the unchanged lane, W-12).
            a.entry_restore_armed = None;
            if a
                .commands
                .try_send(StateCommand::RestoreTimeEntryFromJournal { from })
                .is_err()
            {
                tracing::warn!("entry restore dropped: command channel full or closed");
            }
        } else {
            a.entry_restore_armed = Some(from);
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_entry_restore_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.entry_restore_armed.take().is_some() {
            a.refresh_forced(&window);
        }

    });
    // The deleted-habits section's seams (#1290 enumeration + #1245
    // bulk): the labels-overlay restore seam's exact grammar over the
    // habits roster — the index resolves against the same row list the
    // push derived; a stale index is the typed inert. The first
    // activation arms (the strip mounts), the Confirm dispatches the
    // exact `RestoreHabit { id }`, Cancel disarms.
    install_seam!(window: window.on_habits_habit_restore_requested, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(index_us) = usize::try_from(index).ok() else {
            return;
        };
        let state = a.state.load_full();
        let row_id = state
            .roster
            .tombstoned_habits
            .get(index_us)
            .map(|row| row.id);
        drop(state);
        let Some(habit) = row_id else {
            return; // a stale index against an older roster pass
        };
        a.habit_restore_all_armed = false;
        if a.habit_restore_armed == Some(habit) {
            // The second activation IS the confirm.
            a.habit_restore_armed = None;
            if a
                .commands
                .try_send(StateCommand::RestoreHabit { id: habit })
                .is_err()
            {
                tracing::warn!("habit restore dropped: command channel full or closed");
            }
        } else {
            a.habit_restore_armed = Some(habit);
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_habits_habit_restore_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.habit_restore_armed.take().is_some() {
            a.refresh_forced(&window);
        }

    });
    // The bulk row (#1245): first activation arms (the strip mounts),
    // the Confirm dispatches ONE `RestoreHabitsBulk` (#1345) — the
    // pre-image bundle makes the whole roster one undoable journal
    // fact (one `Undo` walks it back), not N per-row facts.
    install_seam!(window: window.on_habits_habit_restore_all_requested, adapter: adapter;
    bind mut (a, window), args (), {        if a.habit_restore_all_armed {
            a.habit_restore_all_armed = false;
            a.habit_restore_armed = None;
            let state = a.state.load_full();
            let ids: Vec<_> = state.roster.tombstoned_habits.iter().map(|row| row.id).collect();
            drop(state);
            // #1345: ONE `RestoreHabitsBulk` fact — the pre-image
            // bundle makes the bulk one undo step (the per-row
            // `RestoreHabit` stays the single-row verb).
            if a
                .commands
                .try_send(StateCommand::RestoreHabitsBulk { ids })
                .is_err()
            {
                tracing::warn!("habit bulk restore dropped: command channel full or closed");
            }
        } else {
            a.habit_restore_armed = None;
            a.habit_restore_all_armed = true;
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_habits_habit_restore_all_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.habit_restore_all_armed {
            a.habit_restore_all_armed = false;
            a.refresh_forced(&window);
        }

    });
    // The labels overlay's bulk seam (#1245): the same grammar over
    // the board-scoped label roster (the push's exact filter).
    install_seam!(window: window.on_label_restore_all_requested, adapter: adapter;
    bind mut (a, window), args (), {        if a.label_restore_all_armed {
            a.label_restore_all_armed = false;
            let state = a.state.load_full();
            let overlay_board = match a.view_state.labels_subject() {
                Some(crate::view_state::LabelOverlaySubject::Board(board)) => Some(board),
                _ => a
                    .view_state
                    .subject_task()
                    .and_then(|t| state.tasks.get(&t).cloned())
                    .and_then(|t| state.stacks.get(&t.stack).cloned())
                    .map(|stack| stack.board),
            };
            let ids: Vec<_> = state
                .roster
                .tombstoned_labels
                .iter()
                .filter(|row| {
                    state.labels.get(&row.id).is_some_and(|label| {
                        label.deleted && Some(label.board) == overlay_board
                    })
                })
                .map(|row| row.id)
                .collect();
            drop(state);
            // #1345: ONE `RestoreLabelsBulk` fact — the labels twin.
            if a
                .commands
                .try_send(StateCommand::RestoreLabelsBulk { ids })
                .is_err()
            {
                tracing::warn!("label bulk restore dropped: command channel full or closed");
            }
        } else {
            a.label_restore_armed = None;
            a.label_restore_all_armed = true;
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_label_restore_all_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.label_restore_all_armed {
            a.label_restore_all_armed = false;
            a.refresh_forced(&window);
        }

    });
    // The clear-all seam (W-4): the row affordance and the armed
    // strip's Confirm converge here — unarmed it arms (subject-scoped,
    // zero rules is the disabled row), armed it dispatches the exact
    // `ClearReminderRules { task }` once (UF1's pre-images make the
    // clear one undo step, W-7).
    install_seam!(window: window.on_rules_clear_asked, adapter: adapter;
    bind mut (a, window), args (), {        let Some(task) = a.view_state.subject_task() else {
            return;
        };
        let has_rules = {
            let state = a.state.load();
            state.reminders.rules.values().any(|rule| rule.task == task)
        };
        if !has_rules {
            return; // the disabled row's inert (D-2)
        }
        if a.rules_clear_armed {
            a.rules_clear_armed = false;
            if a
                .commands
                .try_send(StateCommand::ClearReminderRules { task })
                .is_err()
            {
                tracing::warn!("rules clear dropped: command channel full or closed");
            }
        } else {
            a.rules_clear_armed = true;
        }
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_rules_clear_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.rules_clear_armed {
            a.rules_clear_armed = false;
            a.refresh_forced(&window);
        }

    });
    // The effort row's seams (task-detail-editor UX plan US-3/D-8,
    // #838). The open funnel: settle any armed date editor (the
    // exclusivity posture — one editor at a time), seed the buffer
    // from the effort's domain text form (empty = the clear seed),
    // refresh. Both fronts land here — the row affordance's seam and
    // nothing else (pointer-grade row verb, the rule-row posture).
    // The buffer mirror (the date-editor buffer pattern verbatim).
    // slint: task-effort-edited
    install_seam!(window: window.on_task_effort_edited, adapter: adapter;
    bind mut (a, window), args (text), {        // The lazy editor (M9): the always-on row has no open seam —
        // the first keystroke arms the buffer over the subject, the
        // seed being the stored value (the D-5 guard's baseline).
        if a.effort_editor.is_none() {
            let Some(task) = a.view_state.subject_task() else {
                return; // the subjectless detail: the typed inert
            };
            let state = a.state.load_full();
            let seed = state
                .tasks
                .get(&task)
                .and_then(|t| t.effort.as_ref())
                .map(|e| e.text_form())
                .unwrap_or_default();
            a.effort_editor = Some(EffortEditorState {
                task,
                buffer: seed.clone(),
                last_committed: seed,
            });
        }
        if let Some(editor) = a.effort_editor.as_mut() {
            editor.buffer = text.to_string();
        }
        a.refresh_forced(&window);

    });

    // The commit funnel: a valid non-empty buffer dispatches the
    // sparse `UpdateTask { effort: Some(Some(e)) }`; the empty buffer
    // commits the clear twin `Some(None)` (the walk placement's
    // empty-clear posture); an invalid non-empty value keeps the
    // buffer and dispatches nothing (the date-editor refusal
    // posture). The commit's consume is the editor's close (D-H2).
    // slint: task-effort-committed
    install_seam!(window: window.on_task_effort_committed, adapter: adapter;
    bind mut (a, window), args (), {        commit_effort_editor(&mut a, &window);

    });

    // The focus-loss settle (the M9 always-on row's commit funnel:
    // parseable buffers commit, unparseable ones refuse in place —
    // the typed text stays for the user to fix).
    install_seam!(window: window.on_task_effort_focus_lost, adapter: adapter; [try] // slint callback: task-effort-focus-lost (the always-on row's commit seam, round-9 W-4)
    bind mut (a, window), args (), {        // The D-5 law: the focus-loss IS the commit — the guard makes a
        // blur of an unchanged value dispatch nothing, so Return +
        // blur never double-dispatch. An unparseable buffer
        // dispatches nothing (the refusal posture; the typed text
        // stays in the field for the user to fix).
        commit_effort_editor(&mut a, &window);

    });

    // `[try]` (round-9 chunk 15): the widget's walk-away close can
    // echo this seam while the adapter is mid-pass holding its own
    // borrow (the focus push that moved the walk away); the echo is
    // absorbed — the mid-pass context settles its own flag, and a
    // later user-driven close converges (the CancelMove arm reads the
    // flag directly).
    install_seam!(window: window.on_priority_dismiss_requested, adapter: adapter; [try]
    bind mut (a, window), args (), {        if a.priority_open.take().is_some() {
            a.refresh_forced(&window);
        }

    });
    install_seam!(window: window.on_task_title_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.view_state.set_buffer(text.to_string());

    });
    install_seam!(window: window.on_task_title_committed, adapter: adapter;
    bind mut (a, window), args (text), {        commit_text(&mut a, &window, &text);

    });

    // The description editor's seams (round-3 D15/#228): the closed
    // body's click arms the multiline field (buffer seeded by the
    // view state); Return and the checkmark funnel through
    // `commit_description` — the accepted text is the field's own
    // buffer read (the funnel re-reads the view-state buffer, kept
    // current by the edited seam).
    install_seam!(window: window.on_task_description_edit_opened, adapter: adapter; // slint: task-description-edit-opened
    bind mut (a, window), args (), {        a.begin_description_edit(&window);

    });
    install_seam!(window: window.on_task_description_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a
        .view_state
        .set_buffer(text.to_string());

    });
    install_seam!(window: window.on_task_description_committed, adapter: adapter;
    bind mut (a, window), args (_text), {        let outcomes = a.view_state.commit_description();
    a.apply_outcomes(&window, &outcomes);

    });

    // The rule cursor's pointer seam (J3 US-6 D-4): a row-body click
    // moves the selected-rule cursor onto that row (id resolved from
    // the tables at the row's index — the remove seam's resolution
    // pattern, one helper shape, two callers). The forced refresh
    // carries the emphasis (the unchanged-refresh guard would eat the
    // pass — the offset-edited seam's posture).
    install_seam!(window: window.on_task_rule_row_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        if let Some(rule) = a
        .tables
        .task_rules
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    {
        a.view_state.select_rule_cursor(rule);
        a.refresh_forced(&window);
    }

    });

    install_seam!(window: window.on_task_rule_remove_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(rule) = a
        .tables
        .task_rules
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return; // a stale index against an older tables pass
    };
    // The pointer coherence write (J3 US-6 D-4): the icon click moves
    // the cursor onto the clicked row first — pointer and cursor can
    // never disagree about which row is selected.
    a.view_state.select_rule_cursor(rule);
    a.rule_target = Some(rule);
    a.route(&window, ActionId::ReminderRuleRemove);

    });

    install_seam!(window: window.on_rule_add_preset_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(dur) = crate::presentation::RULE_PRESETS
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return; // a stale preset index
    };
    a.rule_add_when = Some(crate::presentation::RuleWhen::DueOffset(
        ReminderOffset::before(-dur).expect("preset magnitudes negate into the domain"),
    ));
    a.route(&window, ActionId::ReminderRuleAdd);

    });

    install_seam!(window: window.on_rule_add_custom_accepted, adapter: adapter; [else ( false)]
    bind mut (a, window), args (text), {        // A parse miss dispatches nothing (absorbed — D12): the UI
    // owns no error chrome; the CLI owns typed failures. The miss
    // rides the seam's return (r2 D24): the composition clears its
    // own buffer, so the field never looks stuck. The three-form
    // grammar (r4 D30/D6): bare stays due-anchored, in/at mint the
    // absolute kind — and `In` carries a duration, the planner is
    // the only instant-stamping site.
    let Some(when) =
        crate::presentation::parse_rule_when(&text, a.day_ctx().now, &chrono::Local)
    else {
        a.rule_offset_draft.clear();
        return false;
    };
    a.rule_offset_draft.clear();
    a.rule_add_when = Some(when);
    a.route(&window, ActionId::ReminderRuleAdd);
    true

    });

    install_seam!(window: window.on_rule_offset_edited, adapter: adapter;
    bind mut (a, window), args (text), {        a.rule_offset_draft = text.to_string();
    a.refresh_forced(&window);

    });
}

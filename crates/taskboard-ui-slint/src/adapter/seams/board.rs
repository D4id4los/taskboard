// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use taskboard_domain::{StackId, TaskId};

use super::Adapter;
use super::{
    close_switcher_flyout, commit_effort_editor, send_card_track_toggle, send_toggled_done,
    settle_date_editor,
};
use crate::AppWindow;
use crate::actions::ActionId;
use crate::adapter::install_seam;
use crate::view_state::Flow;
use slint::ComponentHandle as _;

/// A board-bar segment click (design §3.8): resolve the segment's
/// board against the tables pass that pushed the bar, set the one-shot
/// target, and route `board.select` — every click affordance has an
/// action equivalent (§7.1). A stale index against an older pass is
/// absorbed silently. The request-driven flyout's row activation
/// closes it first (the widget path's Return closes — click ≡ key).
pub(in crate::adapter) fn install_board_bar_seam(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    install_seam!(window: window.on_board_bar_segment_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let Ok(index) = usize::try_from(index) else {
        return;
    };
    close_switcher_flyout(&window);
    settle_pointer_conflict(&mut a, &window);
    let Some(board) = a.tables.board_ids.get(index).copied() else {
        return;
    };
    a.board_target = Some(board);
    a.route(&window, ActionId::BoardSelect);

    });
}

/// The archived-boards roster's row activation (UX R-1 D-1, #785):
/// resolve the row's board against the same pass's archived-boards
/// table, dispatch `SetBoardArchived { archived: false }` + the
/// models refresh, and arm the one-shot re-select (consumed at
/// reconcile once the engine's apply makes the board live again). A
/// stale index against an older pass is absorbed silently.
pub(in crate::adapter) fn install_board_bar_archived_seam(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    install_seam!(window: window.on_board_bar_archived_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let Ok(index) = usize::try_from(index) else {
        return;
    };
    settle_pointer_conflict(&mut a, &window);
    let Some(board) = a.tables.archived_board_ids.get(index).copied() else {
        return;
    };
    // Layer 3 (the ToggleFocusMode precedent, UX R-1 D-1's no-new-
    // kiosk-capability non-goal): the kiosk dispatch refuses — the
    // roster renders the disabled affordance (layer 1's visual), the
    // refusal keeps the verb structurally unreachable.
    if a.kiosk {
        return;
    }
    let outcomes = a.view_state.unarchive_board(board);
    a.apply_outcomes(&window, &outcomes);

    });
}

/// The armed-delete strips' seams (UX R-2 D-2, #784): Confirm routes
/// the armed subject's own delete `ActionId` — with the strip armed,
/// the route's interpretation table arm confirms (click ≡ key at
/// every step); Cancel disarms without dispatching. The stack pair's
/// index is the mounting column (the pass the strip rendered from);
/// the confirm routes the armed subject regardless, since the strip
/// only mounts in the armed column of the current pass.
pub(in crate::adapter) fn install_delete_strip_seams(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    // No `settle_pointer_conflict` on any of the four: the armed
    // flow IS the interaction's subject — a settle here would disarm
    // before the route re-arms (the confirm would never commit).
    // One flow lives at a time, so no foreign flow can be settled
    // by a strip interaction.
    // The armed multi-delete strip's pair (UX-R8 chunk-5 D-9, #882):
    // Confirm routes the armed verb (the second stroke's twin — the
    // armed repeat confirms), Cancel disarms with nothing dispatched.
    install_seam!(window: window.on_task_multi_delete_strip_confirmed, adapter: adapter;
    bind mut (a, window), args (), {
        a.route(&window, ActionId::TaskDelete);
    });
    install_seam!(window: window.on_task_multi_delete_strip_cancelled, adapter: adapter;
    bind mut (a, window), args (), {
        let outcomes = a.view_state.cancel_confirm_delete();
        a.apply_outcomes(&window, &outcomes);
    });
    install_seam!(window: window.on_board_delete_strip_confirmed, adapter: adapter;
    bind mut (a, window), args (), {
        a.route(&window, ActionId::BoardDelete);
    });
    install_seam!(window: window.on_board_delete_strip_cancelled, adapter: adapter;
    bind mut (a, window), args (), {
        let outcomes = a.view_state.cancel_confirm_delete();
        a.apply_outcomes(&window, &outcomes);
    });
    // The board ceremony strips' confirm/cancel pair (OP1, W-1/W-3/
    // W-6): one pair — the armed subject surface-wide decides which
    // verb routes (the ConfirmDelete strip's route-the-verb shape;
    // click ≡ key at every step). Cancel is the disarm, nothing
    // dispatched.
    install_seam!(window: window.on_board_ceremony_confirmed, adapter: adapter;
    bind mut (a, window), args (), {
        match a.view_state.flow() {
            crate::view_state::Flow::ConfirmBoardArchive { .. } => {
                a.route(&window, ActionId::BoardArchive);
            }
            crate::view_state::Flow::ConfirmBoardReset { .. } => {
                a.route(&window, ActionId::BoardReset);
            }
            crate::view_state::Flow::ConfirmBoardReconcile { .. } => {
                a.route(&window, ActionId::BoardReconcile);
            }
            _ => {}
        }
    });
    install_seam!(window: window.on_board_ceremony_cancelled, adapter: adapter;
    bind mut (a, window), args (), {
        let outcomes = match a.view_state.flow() {
            crate::view_state::Flow::ConfirmBoardArchive { .. } => {
                a.view_state.cancel_board_archive()
            }
            crate::view_state::Flow::ConfirmBoardReset { .. } => {
                a.view_state.cancel_board_reset()
            }
            crate::view_state::Flow::ConfirmBoardReconcile { .. } => {
                a.view_state.cancel_board_reconcile()
            }
            _ => Vec::new(),
        };
        a.apply_outcomes(&window, &outcomes);
    });
    install_seam!(window: window.on_stack_delete_strip_confirmed, adapter: adapter;
    bind mut (a, window), args (_column), {
        a.route(&window, ActionId::StackDelete);
    });
    install_seam!(window: window.on_stack_delete_strip_cancelled, adapter: adapter;
    bind mut (a, window), args (_column), {
        let outcomes = a.view_state.cancel_confirm_delete();
        a.apply_outcomes(&window, &outcomes);
    });
}

/// The board color strip's seams (UX R-4 D-1, #791): a swatch click
/// commits the armed board's exact `SetBoardColor` through the flow
/// (the palette's hex — never free text) and closes the strip; the
/// dismissal (the underlay's click) ends the flow with nothing
/// dispatched. A stale index against an older palette pass is
/// absorbed silently.
pub(in crate::adapter) fn install_board_color_seams(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    install_seam!(window: window.on_board_color_picked, adapter: adapter;
    bind mut (a, window), args (index), {        let Ok(index) = usize::try_from(index) else {
        return;
    };
    let palette = crate::presentation::derived_swatches();
    let Some(swatch) = palette.get(index) else {
        return; // a stale palette index
    };
    let outcomes = a.view_state.commit_board_color(swatch.as_str());
    a.apply_outcomes(&window, &outcomes);
    });
    install_seam!(window: window.on_board_color_strip_dismissed, adapter: adapter;
    bind mut (a, window), args (), {        let outcomes = a.view_state.cancel_board_color();
    a.apply_outcomes(&window, &outcomes);
    });
}

/// The on-board New-stack button routes the `stack.create` action —
/// the click equivalent of its keyboard binding (§7.1) — and the
/// flow's field mirrors/commits like every other text flow (D9).
pub(in crate::adapter) fn install_stack_create_seams(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    install_seam!(window: window.on_board_new_stack_clicked, adapter: adapter;
    bind mut (a, window), args (), {        settle_pointer_conflict(&mut a, &window);
    a.route(&window, ActionId::StackCreate);

    });
    // The empty-board preset CTA (r3 D5; UX R-4 D-5, #795): click ≡
    // key at every step — the click routes the `board.default-preset`
    // action, whose dispatch arm owns the empty-board gate (the
    // direct command dispatch is retired).
    install_seam!(window: window.on_board_default_preset_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::BoardDefaultPreset);

    });
    install_seam!(window: window.on_board_stack_create_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.view_state.set_buffer(text.to_string());

    });
    install_seam!(window: window.on_board_stack_create_committed, adapter: adapter;
    bind mut (a, window), args (text), {        commit_text(&mut a, &window, &text);

    });
}

/// The stack-detail done checkbox (r3 D6): the toggled command, the
/// board cards' seam verbatim (a stale index absorbed).
pub(in crate::adapter) fn install_stack_done_seam(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    install_seam!(window: window.on_stack_task_done_toggled, adapter: adapter; [bare_ref]
    bind (a), args (row), {        let Ok(row) = usize::try_from(row) else {
        return;
    };
    if let Some(task) = a.tables.stack_task(row) {
        send_toggled_done(&a, task);
    }

    });

    // The card play/pause control (UX-R8 chunk-5 D-6): the stack
    // leg's addressing chain.
    install_seam!(window: window.on_stack_task_track_toggled, adapter: adapter;
    bind mut (a, window), args (row), {        let Ok(row) = usize::try_from(row) else {
        return;
    };
    if let Some(task) = a.tables.stack_task(row) {
        send_card_track_toggle(&mut a, &window, task);
    }

    });
}

/// The seam-wiring decision record (chunk 4.2 of backlog round 3,
/// plan `.artifacts/plans/2026-11-03-install-seam-macro-plan.md` §3.2;
/// closes #595): the standing note here read "a `Fn(&mut Adapter,
/// &AppWindow, Args)` wrapper shape is over generics' worth of closure
/// types; the per-seam helpers stay explicit". The re-adjudication
/// counted 163 installs over 28 `install_*` fns (the trigger of audit
/// finding F7-08 fired ~12.5× over) and re-derived 6 scaffold
/// variants × 9 behavior families. That variety kills the *generic
/// wrapper* — the objection above stands and is decisive against it —
/// but it never reached a declarative token-splicing macro, which
/// hands slint exactly the closure it always received: no generics,
/// no inference change. The scaffold is now `install_seam!` (above);
/// bodies and per-seam design comments stay verbatim at the call
/// site; the six genuinely singular installs stay explicit. Re-open
/// trigger: a seventh scaffold shape or per-site scaffold
/// customization re-adjudicates on the plan's evidence form.
pub(in crate::adapter) fn install_board_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // A card click: focus the (column, row) card (selection state
    // only, §7.1 mouse discipline); activation routes `task.edit`
    // after it.
    install_seam!(window: window.on_board_task_clicked, adapter: adapter;
    bind mut (a, window), args (column, row), {        focus_board_card(&mut a, &window, column, row);

    });
    // The modifier-click grammar (UX-R8 D-5, US-4/S1): the card's
    // modifier release — shift builds the range span, ctrl toggles;
    // the card resolves via the id tables (a stale index absorbed).
    // Nothing dispatches: selection is presentation state (R-2).
    // slint: task-clicked-modifiers
    install_seam!(window: window.on_board_task_clicked_modifiers, adapter: adapter;
    bind mut (a, window), args (column, row, shift, control), {        // The selection's own seam: the pointer-conflict ritual settles
        // a live flow but KEEPS the selection — the gesture builds on
        // the anchor (the D-4 settle-clear law is for foreign settles:
        // Escape, the modal opens, the surface changes).
        settle_pointer_conflict_keeping_selection(&mut a, &window, true);
    let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
        return;
    };
    let Some((_, task)) = a.tables.board_task(column, row) else {
        return;
    };
    let state = a.state.load_full();
    let outcomes = if shift {
        a.view_state.selection_range(&state, task)
    } else if control {
        a.view_state.selection_toggle(task)
    } else {
        Vec::new()
    };
    a.apply_outcomes(&window, &outcomes);

    });
    install_seam!(window: window.on_board_task_activated, adapter: adapter;
    bind mut (a, window), args (column, row), {        if focus_board_card(&mut a, &window, column, row).is_some() {
        a.route(&window, ActionId::TaskEdit);
    }

    });

    // Header and switcher activation are the same selection move (D8:
    // one mechanism per breakpoint; D13: no stack management in UI).
    install_seam!(window: window.on_board_header_activated, adapter: adapter;
    bind mut (a, window), args (column), {        select_column(&mut a, &window, column);

    });
    install_seam!(window: window.on_board_switcher_activated, adapter: adapter;
    bind mut (a, window), args (column), {        select_column(&mut a, &window, column);

    });

    // The Add Task ghost (r2 D8): select the addressed stack, then
    // route task.create — the keyboard's exact action, so click ≡ key
    // by construction (one action, one target-resolution rule).
    install_seam!(window: window.on_board_add_task_activated, adapter: adapter;
    bind mut (a, window), args (column), {        select_column(&mut a, &window, column);
    settle_pointer_conflict(&mut a, &window);
    // The clicked column IS the create target (r4 D4): resolved
    // through the id tables (the D2 index-only convention — a
    // stale index yields `None`, never a wrong stack), so field
    // placement and selection can never diverge.
    if let Ok(index) = usize::try_from(column) {
        a.create_target = a.tables.stack_ids.get(index).copied();
    }
    a.route(&window, ActionId::TaskCreate);

    });

    // The done checkbox (r3 D6): resolve (column, row) against the id
    // tables (a stale index is absorbed, the drag-arm precedent) and
    // raw-send the toggled `SetTaskDone` — the notification Done
    // button's precedent (the registry's complete/reopen act on the
    // focused task; the checkbox addresses an arbitrary card). No
    // local checked state exists anywhere: the refresh push is the
    // single writer of `TaskCardModel.done`.
    install_seam!(window: window.on_board_task_done_toggled, adapter: adapter;
    bind mut (a, window), args (column, row), {        settle_pointer_conflict(&mut a, &window);
    let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
        return;
    };
    let Some((_, task)) = a.tables.board_task(column, row) else {
        return;
    };
    send_toggled_done(&a, task);

    });

    // The card play/pause control (UX-R8 chunk-5 D-6, US-4): the
    // board leg's addressing chain (the regular and compact cards'
    // seam).
    install_seam!(window: window.on_board_task_track_toggled, adapter: adapter;
    bind mut (a, window), args (column, row), {        settle_pointer_conflict(&mut a, &window);
    let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
        return;
    };
    let Some((_, task)) = a.tables.board_task(column, row) else {
        return;
    };
    send_card_track_toggle(&mut a, &window, task);

    });

    // The create-row field: keystrokes mirror into the Rust buffer
    // (D9 rule 1); Return commits.
    install_seam!(window: window.on_board_create_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (column, text), {        let _ = column;
    a.view_state.set_buffer(text.to_string());

    });
    install_seam!(window: window.on_board_create_committed, adapter: adapter;
    bind mut (a, window), args (column, text), {        let _ = column;
    commit_text(&mut a, &window, &text);

    });
}

/// The L2 pointer-seam settle (round-3 R1/R2, plan §3.3): every seam
/// that bypasses `interpret` settles a live flow through the same
/// per-kind rule before its own semantics; idempotent on
/// `Flow::None` (the double-settle guard).
pub(in crate::adapter) fn settle_pointer_conflict(adapter: &mut Adapter, window: &AppWindow) {
    settle_pointer_conflict_keeping_selection(adapter, window, false);
}

/// The context-menu variant (UX-R8 D-11): opening a task's context
/// menu settles a live flow per the pointer rule but *keeps* the
/// selection — the multi-selection menu (US-4/S3) reads it, and the
/// exit restores the invoking card with the selection still driving
/// the menu's grammar on reopen.
pub(in crate::adapter) fn settle_pointer_conflict_keeping_selection(
    adapter: &mut Adapter,
    window: &AppWindow,
    keep_selection: bool,
) {
    // The r4 D24 rule governs the date editor's own exits — a foreign
    // begin (the description field taking focus, journey 13's steal)
    // is one: settle per the commit/discard rule, not the
    // unrelated-settle clean close.
    if matches!(adapter.view_state.flow(), Flow::EditDate { .. }) {
        settle_date_editor(adapter, window);
    }
    // The M9 dirty-close law: the leaving settle commits the armed
    // effort draft (R-9's autosave on leaving the field; the guard
    // makes an unchanged value dispatch nothing). The focus-loss
    // seam's echo here is the `[try]`-absorbed shape — this direct
    // call is the commit's owner.
    commit_effort_editor(adapter, window);
    let state = adapter.state.load_full();
    let selection = adapter.view_state.take_selection();
    adapter.settle_flow(window, &state);
    if keep_selection {
        adapter.view_state.restore_selection(selection);
    }
}

/// A board card at (column, row): resolves the id tables (D2) and
/// applies the click's selection state. Returns the addressed
/// `(stack, task)`, or `None` for a stale index against an older
/// tables pass.
pub(in crate::adapter) fn focus_board_card(
    adapter: &mut Adapter,
    window: &AppWindow,
    column: i32,
    row: i32,
) -> Option<(StackId, TaskId)> {
    settle_pointer_conflict(adapter, window);
    let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
        return None;
    };
    let (stack, task) = adapter.tables.board_task(column, row)?;
    // UX-R8 D-4's plain-click cell: the left click resets the
    // selection (the focus still moves — the open posture rides the
    // action layer). The right-click context funnel calls
    // `activate_card` directly and keeps the selection (D-11).
    adapter.view_state.reset_selection();
    let outcomes = adapter.view_state.activate_card(task, stack);
    adapter.apply_outcomes(window, &outcomes);
    Some((stack, task))
}

/// Header/switcher activation: select the column's stack (D8).
pub(in crate::adapter) fn select_column(adapter: &mut Adapter, window: &AppWindow, column: i32) {
    settle_pointer_conflict(adapter, window);
    let Some(stack) = adapter
        .tables
        .stack_ids
        .get(usize::try_from(column).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    let state = adapter.state.load_full();
    let outcomes = adapter.view_state.select_stack(&state, stack);
    adapter.apply_outcomes(window, &outcomes);
}

/// The text flows' commit path (D9): mirror the final buffer, commit
/// through the view state, apply the outcomes (a commit outside a
/// flow is inert — `commit_edit` ends nothing).
pub(in crate::adapter) fn commit_text(adapter: &mut Adapter, window: &AppWindow, text: &str) {
    // The buffer mirrors first, then the settle: if a live flow is
    // the field's own, the settle commits exactly this text and the
    // trailing commit is the idempotent no-op.
    adapter.view_state.set_buffer(text.to_owned());
    settle_pointer_conflict(adapter, window);
    let state = adapter.state.load_full();
    let outcomes = adapter.view_state.commit_edit(&state);
    adapter.apply_outcomes(window, &outcomes);
}

pub(in crate::adapter) fn install_stack_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // A card click: focus the row's task (selection state only, §7.1);
    // activation routes `task.edit` after it.
    install_seam!(window: window.on_stack_task_clicked, adapter: adapter;
    bind mut (a, window), args (row), {        activate_stack_row(&mut a, &window, row);

    });
    install_seam!(window: window.on_stack_task_activated, adapter: adapter;
    bind mut (a, window), args (row), {        activate_stack_row(&mut a, &window, row);
    a.route(&window, ActionId::TaskEdit);

    });

    install_seam!(window: window.on_stack_create_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.view_state.set_buffer(text.to_string());

    });

    // The search palette (#348): per-keystroke re-rank, outside-click
    // close, and row activation sharing the keyboard Activate path.
    install_seam!(window: window.on_palette_query_edited, adapter: adapter;
    bind mut (a, window), args (text), {        a.apply_palette_query(&window, &text);

    });
    // Quick capture (chunk 10 D8): the buffer mirror, the Return
    // commit (keeps the flow), and the #976 outside-click accept
    // (`commit_capture`'s empty-buffer arm closes without commit —
    // the W1 proof-mount's discard-and-close outside click retired);
    // Escape stays the explicit discard seam below.
    install_seam!(window: window.on_capture_text_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a.view_state.set_buffer(text.to_string());

    });
    install_seam!(window: window.on_capture_accepted, adapter: adapter; // slint callback: capture-accepted (the Return commit AND the #976 edit-kind outside-click accept, US-1)
    bind mut (a, window), args (), {        let outcomes = a.view_state.commit_capture();
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });
    install_seam!(window: window.on_habit_editor_history_clicked, adapter: adapter; // slint callback: habit-editor-history-clicked (the HF3 W-8 editor History row seam, US-7/S1 — the shared opener)
    bind mut (a, window), args (), {        a.open_habit_history_from_subject(&window);

    });
    install_seam!(window: window.on_habit_history_month_flipped, adapter: adapter; // slint callback: habit_history_month_flipped (the HF3 W-8 pager's flip seam, US-7/S3 — the adapter clamps against the can-page facts)
    bind mut (a, window), args (delta), {        let month_open = a.view_state.habit_history().1.is_some();
        if month_open {
            let clamped = {
                let (habit, month) = a.view_state.habit_history();
                let state = a.state.load();
                let day = taskboard_domain::DayCalendar {
                    day_start_time: a.day_start.0,
                    offset: a.day_start.1,
                };
                let now = a.day_ctx().now;
                month
                    .zip(habit)
                    .is_some_and(|(month, habit)| {
                        // The clamp reads the CURRENT month's can-page
                        // facts: a flip is legal exactly when the side
                        // being paged toward has room (US-7/S3 — no
                        // wrap, the ends hold).
                        let habit_row = state.habits.habits.get(&habit).expect("open target");
                        let (counts, posture) =
                            taskboard_domain::habit_event::derived_window_views(
                                habit_row,
                                state.habits.events.values(),
                                day,
                            );
                        let visible = crate::presentation::habit_history_month(
                            habit_row,
                            &counts,
                            &posture,
                            month,
                            now,
                            day,
                        );
                        (delta < 0 && visible.can_page_back)
                            || (delta > 0 && visible.can_page_forward)
                    })
            };
            if clamped {
                let outcomes = a.view_state.flip_habit_history_month(delta);
                a.apply_outcomes(&window, &outcomes);
            }
        }

    });
    install_seam!(window: window.on_habit_history_close_requested, adapter: adapter; // slint callback: habit_history_close_requested (the HF3 W-8 read-role dismissal seam, US-7/S4 — zero state change)
    bind mut (a, window), args (), {        let outcomes = a.view_state.close_habit_history();
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });
    install_seam!(window: window.on_capture_dismiss_requested, adapter: adapter; // slint callback: capture-dismiss-requested (the ModalDialogHost mount's scrim dismissal seam, round-9 W1)
    bind mut (a, window), args (), {        let outcomes = a.view_state.close_capture();
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });
    install_seam!(window: window.on_palette_dismiss_requested, adapter: adapter;
    bind mut (a, window), args (), {        let outcomes = a.view_state.close_palette();
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });
    install_seam!(window: window.on_palette_row_activated, adapter: adapter; // slint callback: palette-row-activated (D3b W-10)
    bind mut (a, window), args (index), {        let target = a
        .tables
        .palette
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied();
    let state = a.state.load();
    let today = a.today_date();
    let outcomes = match target {
        Some(target) => a.view_state.palette_jump(&state, target, today),
        None => a.view_state.close_palette(),
    };
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });
    install_seam!(window: window.on_stack_create_committed, adapter: adapter;
    bind mut (a, window), args (text), {        commit_text(&mut a, &window, &text);

    });
    // The dead-zone click-out (#1543, UX plan R-1/R-4): the board's
    // gated TouchArea layers route here and the one settle funnel
    // (`settle_flow`) owns everything — commit-vs-discard is the
    // pure `settle_by_rule`, and a flowless click is `settle_flow`'s
    // own `Flow::None` early-return (the adapter owns the decision;
    // the UI never branches on flow content). The `[try]` install
    // absorbs the settle's teardown `focus-lost` echo exactly as at
    // every other settle seam (task.rs's borrow discipline).
    install_seam!(window: window.on_board_dead_zone_clicked, adapter: adapter; [try]
    bind mut (a, window), args (), {        let state = a.state.load_full();
    a.settle_flow(&window, &state);

    });
}

/// A stack-detail card click: focus the row's task (selection state
/// only, §7.1).
pub(in crate::adapter) fn activate_stack_row(adapter: &mut Adapter, window: &AppWindow, row: i32) {
    let Some(task) = adapter
        .tables
        .stack_task(usize::try_from(row).unwrap_or(usize::MAX))
    else {
        return;
    };
    let Some(stack) = adapter.view_state.subject_stack() else {
        return;
    };
    let outcomes = adapter.view_state.activate_card(task, stack);
    adapter.apply_outcomes(window, &outcomes);
}

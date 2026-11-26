// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle as _, SharedString};

use crate::AppWindow;
use crate::view_state::Flow;

use super::Adapter;
use crate::adapter::install_seam;

/// The pointer drag-to-move seams (r2 chunk-8, D-B/D-F/D-G): the
/// `DragArea` arm, the `DropArea` hover/drop reports, and the
/// drag-finished outcome. Every handler is flow-guarded — a seam
/// arriving with no active `Flow::DragMove` is a post-teardown echo
/// and is absorbed (the cancel matrix's race arm).
#[allow(clippy::too_many_lines)] // one seam block family, kept flat (the install family's rule)
pub(in crate::adapter) fn install_drag_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // The payload minted once at mount (the plan's P-3): non-empty
    // (the runtime refuses to start a drag without one); the adapter
    // addresses drags through the id tables, never the payload.
    window
        .global::<crate::DragState>()
        .set_payload(slint::DataTransfer::from(SharedString::from(
            "taskboard-task",
        )));

    // The arm: a DragArea crossed the slop. Dismisses the source
    // card's possibly held-open tooltip (the open-menu precedent),
    // then opens the drag flow with the target seeded to the source
    // stack. A stale (column, row) against an older tables pass is
    // absorbed (the `focus_board_card` precedent).
    install_seam!(window: window.on_board_card_drag_armed, adapter: adapter;
    bind mut (a, window), args (column, row), {        let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
        return;
    };
    if a.view_state.flow() != &Flow::None {
        return;
    }
    let Some((_, task)) = a.tables.board_task(column, row) else {
        return;
    };
    Adapter::dismiss_tooltip(&window);
    let state = a.state.load_full();
    let outcomes = a.view_state.begin_drag_move(&state, task);
    a.apply_outcomes(&window, &outcomes);

    });

    // The hover report: a column DropArea sees the DragMove (the
    // runtime's DragMove hit-testing designated it — no geometry
    // crosses to Rust, D-E). Resolves the column against the id
    // tables and reports the target; inert without the drag flow.
    install_seam!(window: window.on_board_drag_hovered, adapter: adapter;
    bind mut (a, window), args (column), {
        a.drag_target_report(&window, column);
    });

    // The drop: re-assert the target from the drop column, then
    // confirm — the shared construction site builds exactly the
    // `MoveTask` the keyboard flow confirms (D-F, equivalence by
    // construction; the journey pins the command).
    install_seam!(window: window.on_board_card_dropped, adapter: adapter;
    bind mut (a, window), args (column), {        let Ok(column) = usize::try_from(column) else {
        return;
    };
    let Flow::DragMove { .. } = a.view_state.flow() else {
        return;
    };
    // Round 3 (R2, F8): a drop that cannot resolve its column —
    // the stale-tables race — settles instead of returning: the
    // move dispatches nothing (a missed drop must not guess a
    // target) and no armed flow/ghost survives the gesture.
    let Some(stack) = a.tables.stack_ids.get(column).copied() else {
        let state = a.state.load_full();
        a.settle_flow(&window, &state);
        return;
    };
    let state = a.state.load_full();
    let mut outcomes = a.view_state.drag_target(&state, stack);
    outcomes.extend(a.view_state.confirm_drag_move(&state));
    a.apply_outcomes(&window, &outcomes);

    });

    // The gap hover report (UX-R8 D-2, US-2/S1): a gap-zone DropArea's
    // hover address, resolved to (stack, gap) through the id tables.
    // An unresolvable (column, gap) — the stale-tables race — is
    // absorbed silently (hover asserts nothing).
    // slint: board-gap-hovered
    install_seam!(window: window.on_board_gap_hovered, adapter: adapter;
    bind mut (a, window), args (column, gap), {        let (Ok(column), Ok(gap)) = (usize::try_from(column), usize::try_from(gap)) else {
        return;
    };
    let Flow::DragMove { .. } = a.view_state.flow() else {
        return;
    };
    let Some(stack) = a.tables.stack_ids.get(column).copied() else {
        return;
    };
    let state = a.state.load_full();
    if gap > state.tasks_in_stack(stack).len() {
        return; // a stale gap index: absorbed (hover asserts nothing)
    }
    let outcomes = a.view_state.drag_gap(&state, stack, gap);
    a.apply_outcomes(&window, &outcomes);

    });

    // The gap drop (UX-R8 D-2, US-2/S2): re-assert the (stack, gap)
    // aim, then confirm through the shared construction site. The
    // stale twins (the `board-card-dropped` F8 law verbatim): an
    // unresolvable column or gap on drop *settles* the flow — the
    // move dispatches nothing and no armed ghost survives.
    // slint: board-gap-dropped
    install_seam!(window: window.on_board_gap_dropped, adapter: adapter;
    bind mut (a, window), args (column, gap), {        let (Ok(column), Ok(gap)) = (usize::try_from(column), usize::try_from(gap)) else {
        return;
    };
    let Flow::DragMove { .. } = a.view_state.flow() else {
        return;
    };
    let Some(stack) = a.tables.stack_ids.get(column).copied() else {
        let state = a.state.load_full();
        a.settle_flow(&window, &state);
        return;
    };
    let state = a.state.load_full();
    if gap > state.tasks_in_stack(stack).len() {
        a.settle_flow(&window, &state);
        return;
    }
    let mut outcomes = a.view_state.drag_gap(&state, stack, gap);
    outcomes.extend(a.view_state.confirm_drag_move(&state));
    a.apply_outcomes(&window, &outcomes);

    });

    // The compact segment's hover report (UX-R8 D-3, US-3/S1): the
    // switcher's DropArea address resolved to its stack — the
    // highlight rides the existing move-target paint (model-driven),
    // so the report is the column-level aim (no gap). A stale segment
    // index is absorbed silently.
    // slint: switcher-drop-hovered
    install_seam!(window: window.on_board_switcher_drop_hovered, adapter: adapter;
    bind mut (a, window), args (column), {
        a.drag_target_report(&window, column);
    });

    // The compact segment drop (UX-R8 D-3, US-3/S2): resolve the
    // segment's stack, then guard — the dragged task's *own* stack's
    // segment is the selected stack's segment in the compact
    // one-column leg (R-3's append-only rule makes a same-stack
    // segment drop meaningless), so it settles the flow and dispatches
    // nothing (AC-2). Otherwise confirm through the shared
    // construction site: the column-level aim appends (R-3 verbatim).
    // A stale segment index settles (never guesses, the F8 law).
    // slint: switcher-card-dropped
    install_seam!(window: window.on_board_switcher_card_dropped, adapter: adapter;
    bind mut (a, window), args (column), {        let Ok(column) = usize::try_from(column) else {
        return;
    };
    let Flow::DragMove { task, .. } = a.view_state.flow() else {
        return;
    };
    let Some(stack) = a.tables.stack_ids.get(column).copied() else {
        let state = a.state.load_full();
        a.settle_flow(&window, &state);
        return;
    };
    let state = a.state.load_full();
    if state.tasks.get(task).is_some_and(|t| t.stack == stack) {
        a.settle_flow(&window, &state);
        return;
    }
    let mut outcomes = a.view_state.drag_target(&state, stack);
    outcomes.extend(a.view_state.confirm_drag_move(&state));
    a.apply_outcomes(&window, &outcomes);

    });

    // The finished outcome (round 3, R2/F8): the flow settles
    // whenever it is still armed — `false` (a cancelled drag —
    // release outside any accepting column, the runtime's Exit
    // rewrite) cancels, and a `true` whose confirm was swallowed by
    // the stale-column race cancels too. `try_borrow_mut`: the
    // adapter's synthetic
    // `PointerExited` teardown (Escape / a leaving effect / an
    // Activate confirm mid-drag) fires `drag-finished(none)`
    // synchronously *inside* an `interpret` borrow; that echo is
    // absorbed here — the view-state flow is already gone, which the
    // flow guard would assert anyway.
    install_seam!(window: window.on_board_card_drag_finished, adapter: adapter; [try]
    bind mut (a, window), args (column, row, _completed), {        let _ = (column, row);
    // Round 3 (R2, F8): the `completed` early-out is gone — the
    // handler settles whenever the flow is still armed,
    // `completed` or not. `completed == true` with the flow still
    // armed means the drop seam's confirm was swallowed (exactly
    // the stale-column race); `completed == false` is today's
    // cancel. Either way the gesture is over and nothing stays
    // armed. The `try_borrow_mut` absorption stays: the synthetic
    // teardown echo fires inside an `interpret` borrow with the
    // flow already gone — absorption is correct there and can
    // never mask a legitimate settle.
    let Flow::DragMove { .. } = a.view_state.flow() else {
        return;
    };
    let state = a.state.load_full();
    a.settle_flow(&window, &state);

    });
}
impl Adapter {
    /// Re-types a row-carried journal id at the trust boundary (the
    /// W-2/W-3 addressing law): a malformed string is absorbed (the
    /// silent law — the stale-row case surfaces as the engine's typed
    /// `TargetNotInLine` only for well-formed but moved ids).
    pub(in crate::adapter) fn parse_journal_id(
        id: &slint::SharedString,
    ) -> Option<taskboard_domain::JournalId> {
        uuid::Uuid::parse_str(id)
            .ok()
            .map(taskboard_domain::JournalId::from_uuid)
    }

    /// The column drag hover report shared by the board `DropArea` and
    /// the compact switcher segment (#1297 dedup): the column resolves
    /// against the id tables and the target is reported; a non-index
    /// column, a non-DragMove flow and a stale column index are
    /// absorbed silently (the bodies were token-identical at both
    /// seams — the incident-law comments stay at each seam).
    pub(in crate::adapter) fn drag_target_report(&mut self, window: &AppWindow, raw_column: i32) {
        let Ok(column) = usize::try_from(raw_column) else {
            return;
        };
        let Flow::DragMove { .. } = self.view_state.flow() else {
            return;
        };
        let Some(stack) = self.tables.stack_ids.get(column).copied() else {
            return;
        };
        let state = self.state.load_full();
        let outcomes = self.view_state.drag_target(&state, stack);
        self.apply_outcomes(window, &outcomes);
    }
}

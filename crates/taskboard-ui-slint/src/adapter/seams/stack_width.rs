// SPDX-License-Identifier: AGPL-3.0-only

use super::Adapter;
use crate::adapter::install_seam;
use slint::ComponentHandle as _;
use std::cell::RefCell;
use std::rc::Rc;

use crate::AppWindow;

/// The column resize divider seams (r4 D6): press materializes the
/// resolved row into the view-state overlay and captures the boundary
/// (left stack, right stack, rendered base of the left column), drag
/// moves exactly the pair at the boundary (the pure
/// [`crate::widths::drag_boundary`]) and re-renders, release persists
/// the full snapshot over the request lane. A stale column index
/// against an older tables pass is absorbed silently (the
/// `board_target` precedent).
pub(in crate::adapter) fn install_stack_width_seams(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    install_seam!(window: window.on_board_stack_width_press, adapter: adapter;
    bind mut (a, window), args (column), {        let Ok(column) = usize::try_from(column) else {
        return;
    };
    let Some(&left) = a.tables.stack_ids.get(column) else {
        return;
    };
    let Some(board) = a.view_state.board() else {
        return;
    };
    let row = crate::models::board_row(&window, &a.state.load(), &a.view_state);
    // A boundary needs a right column; the last column has no
    // divider (the markup's conditional hides it anyway).
    let Some(&right) = row.stacks.get(column + 1) else {
        return;
    };
    let scale = f64::from(window.global::<crate::Theme>().get_base_scale()).max(0.001);
    // Press materializes the resolved row into the view-state
    // overlay (chunk-2 plan §7.2): from here the gesture runs on a
    // total, sum-exact basis — untouched columns cannot wiggle
    // mid-gesture and the released boundary cannot jump when the
    // next derive re-fits the row. View-state only; release still
    // sends the lane's one request.
    for (stack, &width) in row.stacks.iter().zip(&row.widths) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let stored = (f64::from(width) / scale).round() as i64;
        a.view_state
            .set_stack_width(board, *stack, u32::try_from(stored).unwrap_or(u32::MAX));
    }
    #[allow(clippy::cast_possible_truncation)] // a px value
    let base = f64::from(row.widths[column]) as f32;
    a.width_drag = Some((left, right, base));

    });
    install_seam!(window: window.on_board_stack_width_drag, adapter: adapter;
    bind mut (a, window), args (column, delta), {        let Ok(column) = usize::try_from(column) else {
        return;
    };
    let Some((left, right, base)) = a.width_drag else {
        return;
    };
    // A tables pass that reordered the columns makes the index
    // stale: only proceed when it still addresses the dragged
    // boundary's left column (the right StackId rides in the
    // buffer and needs no re-check against the row below).
    if a.tables.stack_ids.get(column) != Some(&left) {
        return;
    }
    let Some(board) = a.view_state.board() else {
        return;
    };
    let row = crate::models::board_row(&window, &a.state.load(), &a.view_state);
    let Some(boundary) = row.stacks.iter().position(|stack| *stack == left) else {
        return;
    };
    if row.stacks.get(boundary + 1) != Some(&right) {
        return;
    }
    let requested = f64::from(base) + f64::from(delta);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let d = (requested - f64::from(row.widths[boundary])).round() as i64;
    let next = crate::widths::drag_boundary(&row.widths, boundary, d, &row.mins);
    let scale = f64::from(window.global::<crate::Theme>().get_base_scale()).max(0.001);
    for index in [boundary, boundary + 1] {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let stored = (f64::from(next[index]) / scale).round() as i64;
        a.view_state.set_stack_width(
            board,
            row.stacks[index],
            u32::try_from(stored).unwrap_or(u32::MAX),
        );
    }
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_board_stack_width_release, adapter: adapter;
    bind mut (a, _window), args (column), {        let Ok(_column) = usize::try_from(column) else {
        return;
    };
    // Persist on release only (the drag path never sends): one
    // full-snapshot request, warn-and-drop on a full inbox.
    if a.width_drag.take().is_none() {
        return;
    }
    let Some(board) = a.view_state.board() else {
        return;
    };
    let request = crate::requests::UiRequest::StackWidthsChanged {
        board,
        widths: a.view_state.stack_widths_of(board),
    };
    if a.requests.try_send(request).is_err() {
        tracing::warn!("request channel full or closed; dropped");
    }

    });
}

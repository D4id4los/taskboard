// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle as _;

use crate::AppWindow;
use crate::actions::{ActionId, ContextId};
use crate::menu::{self};
use crate::router::UiEffect;

use super::Adapter;
use super::install_body_seams;
use super::{MenuAnchor, MenuKind, MenuSubject};
use super::{commit_text, settle_pointer_conflict, settle_pointer_conflict_keeping_selection};
use crate::adapter::install_seam;

/// The dismissal layer's full close (round-1 D2): an outside click
/// routes `menu.dismiss` — a dismissal, not a level-up.
pub(in crate::adapter) fn install_menu_dismiss_seam(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    // An outside click is a full dismissal, not a level-up
    // (round-1 D6: `CloseMenu`, the layer and the toggle share it).
    install_seam!(window: window.on_menu_dismiss_requested, adapter: adapter;
        bind mut (a, window), args (), {
        a.apply_effect(&window, UiEffect::CloseMenu);
    });
}

/// The right-click context menu opener (round-1 D13, round-2 D21):
/// focus the addressed card (right-click selects like left-click),
/// then open the shared menu pipeline over the named object actions —
/// flat, anchored at the pointer. Over `[Board, Global]` (the one
/// chain every object menu opens with) every board-surface verb's
/// binding hint resolves — the F-B7 fix. Stale indices against an
/// older tables pass are absorbed silently.
impl Adapter {
    fn open_context_menu(
        &mut self,
        window: &AppWindow,
        x: f32,
        y: f32,
        ids: &[ActionId],
        bar: Vec<crate::menu::QuickBarEntry>,
        states: &std::collections::BTreeMap<ActionId, bool>,
    ) {
        let chain = [ContextId::Board, ContextId::Global].to_vec();
        self.menu_anchor = MenuAnchor::Pointer(x, y);
        self.menu_kind = MenuKind::Object;
        self.menu_subject = MenuSubject::None;
        self.menu_chain = chain;
        // The id-list menus have no subject (the multi set and the
        // stack/board surfaces' curated lists).
        self.menu_row_subject = None;
        self.root_entries = menu::object_entries(&self.keymap, &self.menu_chain, ids, states);
        self.menu_entries = self.root_entries.clone();
        // The quick bar (UX-R8 chunk-5 D-2): built once at open —
        // the D-1 pin covers it; every other menu consumer passes
        // [] (an empty model renders nothing).
        self.quick_bar_entries = bar;
        self.shell.menu = crate::shell::MenuState {
            open: true,
            selected: 0,
            len: self.menu_entries.len(),
            active_group: None,
            task_group: None,
            root_selected: 0,
        };
        self.sync_menu_models(window);
        self.sync(window);
    }

    /// The task card's menu list (round-2 D21 spelling of round-1
    /// D13): the Task group's rows, reopen and move-cancel included.
    /// `extra` extends the list for the calling surface (the
    /// tracker hide-set filter is the subtraction precedent) — the
    /// today seam passes `TaskDueToday` (UX R-4 D-8, #796: the
    /// verb binds `ContextId::Today` only, so board/stack cards'
    /// menus stay untouched).
    pub(crate) fn open_task_context_menu(
        &mut self,
        window: &AppWindow,
        x: f32,
        y: f32,
        extra: &[ActionId],
        subject: Option<crate::presentation::RowSubject>,
    ) {
        // The habit arm (H2 D1/B1): the union's habit subject routes
        // to the habit page's own opener — the task page's grammar is
        // untouched for task subjects.
        if let Some(crate::presentation::RowSubject::Habit(habit)) = subject {
            let index = self.tables.habits.iter().position(|&row| row == habit);
            if let Some(index) = index {
                self.open_habit_context_menu(window, x, y, index);
            }
            return;
        }
        let task = subject.and_then(|subject| match subject {
            crate::presentation::RowSubject::Task(task) => Some(task),
            crate::presentation::RowSubject::Habit(_) => None,
        });
        // The selection branch (UX-R8 D-6, US-4/S3): a member
        // invocation opens the multi row set (the count caption is
        // D-9); a non-member (or board-surface-foreign) invocation
        // opens the curated single-card menu.
        let selection_covers = task.is_some_and(|task| self.view_state.selection().contains(&task));
        if selection_covers {
            let ids: Vec<ActionId> = menu::TASK_CONTEXT_MULTI.to_vec();
            // The multi bar (J5 US-2, R-3's multi-sensibility): the
            // aggregate facts over the selection's members — this is
            // also the H-1 fix (a member right-click, the common
            // selection-covered case, used to open a barless menu).
            let bar = {
                let state = self.state.load_full();
                let members = self.view_state.selection_anchor_order(&state);
                let tracker = crate::presentation::tracking_open_task(&state);
                let tracked_members = members
                    .iter()
                    .filter(|member| Some(**member) == tracker)
                    .count();
                let any_scheduled = members.iter().any(|member| {
                    state
                        .tasks
                        .get(member)
                        .is_some_and(|task| task.scheduled.is_some())
                });
                let tracking_elsewhere = tracker.is_some_and(|open| !members.contains(&open));
                menu::multi_quick_bar(
                    members.len(),
                    any_scheduled,
                    tracked_members,
                    tracking_elsewhere,
                )
            };
            self.open_context_menu(window, x, y, &ids, bar, &std::collections::BTreeMap::new());
            // The disarm snapshot (#927): recorded after the opener
            // resets it — the multi menu disarms when every member
            // vanished.
            self.menu_subject = MenuSubject::Selection(
                self.view_state
                    .selection_anchor_order(&self.state.load_full()),
            );
            return;
        }
        // Opening another subject's menu disarms the armed habit
        // conversion (H2 D4: one subject armed at a time) — and the
        // armed remove (H4 D4, the same one-subject law).
        if self.habit_convert_armed.is_some() {
            self.habit_convert_armed = None;
            self.habit_convert_target = None;
            self.pattern_draft = None;
        }
        self.habit_remove_armed = None;
        // The curated grammar (UX-R8 chunk-5 D-4/D-5, US-2): the
        // quick bar + the applicable rows + the four families compose
        // through `task_context_page` over the per-subject facts (the
        // #492 tracker hide-set moved into `TaskMenuSubject::is_tracker`
        // — semantics unchanged, the reopen no-op stays).
        // The one chain every object menu opens with (round-2 D21) —
        // set before the composition so the projection resolves the
        // board-surface verbs' hints.
        self.menu_chain = [ContextId::Board, ContextId::Global].to_vec();
        let subject_id = task.or(self.view_state.focused_task());
        self.menu_row_subject = subject_id.map(crate::presentation::RowSubject::Task);
        let (bar, entries) = {
            let state = self.state.load_full();
            let flow = self.view_state.flow().clone();
            let states = self.entry_states(&state, &flow);
            let subject = crate::presentation::task_menu_subject(
                &state,
                subject_id.map(crate::presentation::RowSubject::Task),
            );
            menu::task_context_page(&self.keymap, &self.menu_chain, subject, &states, extra)
        };
        self.open_context_menu_with(window, x, y, entries, bar);
        // The disarm snapshot (#927): recorded after the opener resets
        // it — the pinned subject's death closes the menu on the same
        // reconciliation pass.
        self.menu_subject = subject_id.map_or(MenuSubject::None, MenuSubject::Single);
    }

    /// The open menu's subject's facts (UX-R8 chunk-5 D-4; H2 D1): the
    /// focused task on every card surface (the context seams'
    /// focus-first posture), or the open habit menu's habit — the
    /// submenu machinery reads the same union either way.
    pub(in crate::adapter) fn current_task_subject(&self) -> crate::menu::TaskMenuSubject {
        let state = self.state.load_full();
        crate::presentation::task_menu_subject(&state, self.menu_row_subject)
    }

    /// The context-menu opener over pre-computed rows (UX-R8 chunk-5
    /// D-5): the curated builder's landing — the id-list variant
    /// delegates with the projected rows.
    pub(in crate::adapter) fn open_context_menu_with(
        &mut self,
        window: &AppWindow,
        x: f32,
        y: f32,
        entries: Vec<crate::menu::MenuEntry>,
        bar: Vec<crate::menu::QuickBarEntry>,
    ) {
        self.menu_anchor = MenuAnchor::Pointer(x, y);
        self.menu_kind = MenuKind::Object;
        self.menu_subject = MenuSubject::None;
        // The chain is the opener's (round-2 D21, #1311): the task
        // opener installs `[Board, Global]`, the habit opener the
        // Today-prefixed one — this landing site overwrites nothing.
        self.root_entries = entries;
        self.menu_entries = self.root_entries.clone();
        // The quick bar (UX-R8 chunk-5 D-2): built once at open —
        // the D-1 pin covers it; every other menu consumer passes
        // [] (an empty model renders nothing).
        self.quick_bar_entries = bar;
        self.shell.menu = crate::shell::MenuState {
            open: true,
            selected: 0,
            len: self.menu_entries.len(),
            active_group: None,
            task_group: None,
            root_selected: 0,
        };
        self.sync_menu_models(window);
        self.sync(window);
    }
}

/// A board card's right click (round-1 D13): resolve, focus, open.
pub(in crate::adapter) fn install_board_context_seam(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    install_seam!(window: window.on_board_task_context_requested, adapter: adapter;
    bind mut (a, window), args (column, row, x, y), {        let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row)) else {
        return;
    };
    settle_pointer_conflict_keeping_selection(&mut a, &window, true);
    let Some((stack, task)) = a.tables.board_task(column, row) else {
        return;
    };
    let outcomes = a.view_state.activate_card(task, stack);
    a.apply_outcomes(&window, &outcomes);
    a.open_task_context_menu(
        &window,
        x,
        y,
        &[],
        Some(crate::presentation::RowSubject::Task(task)),
    );

    });
}

/// A stack-detail card's right click (round-1 D13).
pub(in crate::adapter) fn install_stack_context_seam(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    install_seam!(window: window.on_stack_task_context_requested, adapter: adapter;
    bind mut (a, window), args (row, x, y), {        let Ok(row) = usize::try_from(row) else {
        return;
    };
    settle_pointer_conflict(&mut a, &window);
    let Some(task) = a.tables.stack_task(row) else {
        return;
    };
    let Some(stack) = a.view_state.subject_stack() else {
        return;
    };
    let outcomes = a.view_state.activate_card(task, stack);
    a.apply_outcomes(&window, &outcomes);
    // The stack-detail surface carries no selection (R-1's scoping).
    a.open_task_context_menu(&window, x, y, &[], None);

    });
}
/// The stack-context arm, shared by the header and the body seams
/// (r3 D4) so they cannot drift: resolve the column's stack, apply
/// the selection outcomes, open the stack menu at the pointer.
/// Stale indices are absorbed silently (the tables guard).
pub(in crate::adapter) fn open_stack_context(
    a: &mut Adapter,
    window: &AppWindow,
    column: i32,
    x: f32,
    y: f32,
) {
    let Ok(column) = usize::try_from(column) else {
        return;
    };
    let Some(stack) = a.tables.stack_ids.get(column).copied() else {
        return;
    };
    let state = a.state.load_full();
    let outcomes = a.view_state.select_stack(&state, stack);
    a.apply_outcomes(window, &outcomes);
    a.open_context_menu(
        window,
        x,
        y,
        menu::STACK_CONTEXT,
        Vec::new(),
        &std::collections::BTreeMap::new(),
    );
}
/// The r2 D21 object-context seams: right-click selects first (the
/// card precedent), then opens the object's context menu at the
/// pointer. A right-clicked non-selected board-bar segment switches
/// to that board — "the menu addresses what you clicked" (the
/// recorded semantics). Stale indices are absorbed silently.
pub(in crate::adapter) fn install_object_context_seams(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    // The stack header (the regular header and the compact switcher's
    // segments share the callback).
    install_seam!(window: window.on_board_header_context_requested, adapter: adapter;
    bind mut (a, window), args (column, x, y), {        open_stack_context(&mut a, &window, column, x, y);

    });
    // The stack card's bare surface (r3 D4): context on right-down,
    // select on left-click (r4 D3) — the two body seams, one home.
    install_body_seams(window, adapter);
    // The board-bar segment.
    install_seam!(window: window.on_board_segment_context_requested, adapter: adapter;
    bind mut (a, window), args (index, x, y), {        let Ok(index) = usize::try_from(index) else {
        return;
    };
    settle_pointer_conflict(&mut a, &window);
    let Some(board) = a.tables.board_ids.get(index).copied() else {
        return;
    };
    let state = a.state.load_full();
    let outcomes = a.view_state.select_board(&state, board);
    a.apply_outcomes(&window, &outcomes);
    // The ceremony rows' liveness verdicts (OP1 US-1/S1): enabled for
    // a live board, disabled for a tombstone (an honest greyed row —
    // the engine's typed refusal stays the keyboard path's backstop).
    let live = state
        .boards
        .get(&board)
        .is_some_and(|row| !row.deleted);
    let states = std::collections::BTreeMap::from([
        (ActionId::BoardArchive, live),
        (ActionId::BoardReset, live),
        (ActionId::BoardReconcile, live),
    ]);
    a.open_context_menu(&window, x, y, menu::BOARD_CONTEXT, Vec::new(), &states);

    });
    // The stack-rename field (per-column; the flow names the stack —
    // the mirror only needs the buffer, the commit resolves the flow).
    install_seam!(window: window.on_board_stack_rename_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (_column, text), {        a
        .view_state
        .set_buffer(text.to_string());

    });
    install_seam!(window: window.on_board_stack_rename_committed, adapter: adapter;
    bind mut (a, window), args (_column, text), {        commit_text(&mut a, &window, &text);

    });
    // The board create/rename fields: the window-level slot's
    // mirror/commit seams (the create-row pattern).
    install_seam!(window: window.on_board_create_field_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a
        .view_state
        .set_buffer(text.to_string());

    });
    install_seam!(window: window.on_board_create_field_committed, adapter: adapter;
    bind mut (a, window), args (text), {        commit_text(&mut a, &window, &text);

    });
    install_seam!(window: window.on_board_rename_field_edited, adapter: adapter; [bare_mut]
    bind mut (a), args (text), {        a
        .view_state
        .set_buffer(text.to_string());

    });
    install_seam!(window: window.on_board_rename_field_committed, adapter: adapter;
    bind mut (a, window), args (text), {        commit_text(&mut a, &window, &text);

    });
}

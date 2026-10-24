// SPDX-License-Identifier: AGPL-3.0-only

//! The view-state machine (D3/D7/D9): the one view cycle, board
//! focus, compact stack selection, and the edit/move flows as pure
//! state over `AppState` slices. No slint types, no IO, no time
//! (methods take the state snapshot they reason about; the adapter
//! calls them from the interpretation table and applies the returned
//! outcomes verbatim — it never re-decides).
//!
//! Focus discipline (D15): slint element focus is mount-driven (each
//! view scope/field focuses itself when it is created); this module
//! owns *selection* — which task/stack the actions address.

use std::collections::BTreeMap;

use taskboard_domain::{
    AppState, BoardId, SearchTarget, StackId, StateCommand, TaskChanges, TaskId,
};

use crate::actions::ContextId;
use crate::presentation::{live_board, live_boards, stacks_in_board};
use crate::router::Mode;
use crate::widths::{STACK_WIDTH_DEFAULT_PX, STACK_WIDTH_MIN_PX, STACK_WIDTH_STEP_PX, clamp_width};

/// The three v0.1.0 views (D7): one tree, one cycle
/// Board → `StackDetail` → `TaskDetail` → Board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewId {
    #[default]
    Board,
    StackDetail,
    TaskDetail,
    /// The settings surface (round-1 D5/D24b): reachable only through
    /// `open_settings` — never a member of the `step_view` cycle.
    Settings,
}

/// The active flow (D9): at most one — edit (create row or title) XOR
/// move. `refresh` never touches text properties while a flow holds
/// the buffers (D9 rule 4).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Flow {
    #[default]
    None,
    /// The inline create row, appending to `stack`.
    Create { stack: StackId },
    /// The inline create row for a new stack on the live board
    /// (#304): needs no target stack, so it works on an empty board.
    CreateStack,
    /// The task-detail title editor.
    EditTitle { task: TaskId },
    /// The move flow: moving `task` toward `target`.
    Move { task: TaskId, target: StackId },
    /// The jump-to search palette (#348, chunk 9): the palette cursor
    /// lives in the flow (subject-free — `reconcile` leaves it alone;
    /// background refreshes re-rank through the row push instead).
    Palette { selected: usize },
}

/// What a view-state transition asks the adapter to do (D9):
/// dispatch a command, flip the vi mode, or repaint now. The adapter
/// applies outcomes; focus is mount-driven (module docs).
#[derive(Debug, Clone, PartialEq)]
pub enum FlowOutcome {
    Command(StateCommand),
    SetMode(Mode),
    RefreshNow,
    /// The stack column widths changed (round-1 D16/D21): persist the
    /// full per-board snapshot now — the adapter translates it into
    /// the `UiRequest` lane; the view state itself never touches a
    /// channel. Idempotent under re-send (full snapshots).
    StackWidthsChanged,
}

/// The pure view state (D7).
#[derive(Debug, Clone, Default)]
pub struct ViewState {
    view: ViewId,
    /// The selected board (design §3.8): `None` only before the seed
    /// rule first runs or with zero boards — `reconcile` seeds and
    /// repairs it (the normalization is refresh-time, one site).
    board: Option<BoardId>,
    /// The `StackDetail` subject, snapshotted at entry (D7).
    subject_stack: Option<StackId>,
    /// The `TaskDetail` subject, snapshotted at entry (D7).
    subject_task: Option<TaskId>,
    /// The board's focused card (selection, not slint focus).
    focused_task: Option<TaskId>,
    /// The compact switcher's selected stack.
    selected_stack: Option<StackId>,
    flow: Flow,
    /// The Rust mirror of the active text buffer (D9 rule 1): the
    /// adapter updates it on every keystroke callback; commit reads
    /// it — no slint-state dependency.
    buffer: String,
    /// The user-set stack column widths (round-1 D16), stored logical
    /// px; an absent entry = auto layout (today's rendering). UI-local
    /// forever — never a domain type (DF-2); `reconcile` leaves it
    /// alone (liveness pruning is the app forwarder's write-time job).
    stack_widths: BTreeMap<BoardId, BTreeMap<StackId, u32>>,
    /// The view to restore when Settings closes (round-1 D5): only the
    /// three cycle views are ever stored; `close_settings` takes it —
    /// "returns to the exact previous view" is this snapshot, not a
    /// cycle walk.
    settings_return: Option<ViewId>,
}

/// The canonical append position in `stack`: max canonical order + 1.
#[must_use]
pub fn append_order(state: &AppState, stack: StackId) -> i64 {
    state
        .tasks_in_stack(stack)
        .last()
        .map_or(0, |t| t.order + 1)
}

/// The canonical append position on `board`: the board's live stacks'
/// max order + 1 (#304's "sensible default appended after the existing
/// stacks"); 0 on an empty board.
#[must_use]
pub fn append_stack_order(state: &AppState, board: BoardId) -> i64 {
    state
        .stacks
        .values()
        .filter(|s| s.board == board && !s.deleted)
        .map(|s| s.order)
        .max()
        .map_or(0, |max| max + 1)
}

/// The index `current` moved by `direction` (±1), wrapping over
/// `len` — the ring-walk primitive for cycles and switchers.
fn wrapped_index(current: usize, direction: i32, len: usize) -> usize {
    let len = i64::try_from(len).unwrap_or(1).max(1);
    let next = i64::try_from(current).unwrap_or(0) + i64::from(direction);
    usize::try_from(next.rem_euclid(len)).unwrap_or(0)
}

impl ViewState {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            view: ViewId::Board,
            board: None,
            subject_stack: None,
            subject_task: None,
            focused_task: None,
            selected_stack: None,
            flow: Flow::None,
            buffer: String::new(),
            stack_widths: BTreeMap::new(),
            settings_return: None,
        }
    }

    #[must_use]
    pub const fn view(&self) -> ViewId {
        self.view
    }

    /// The selected board (the refresh-time seed/repair keeps it live;
    /// `None` only with zero boards).
    #[must_use]
    pub const fn board(&self) -> Option<BoardId> {
        self.board
    }

    #[must_use]
    pub const fn flow(&self) -> &Flow {
        &self.flow
    }

    /// The flow's write seam (the palette cursor reset rides it —
    /// the adapter's keystroke path zeroes the cursor on re-rank).
    pub(crate) fn flow_mut(&mut self) -> &mut Flow {
        &mut self.flow
    }

    #[must_use]
    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    /// The adapter's keystroke-mirror update (D9 rule 1).
    pub fn set_buffer(&mut self, text: String) {
        self.buffer = text;
    }

    #[must_use]
    pub const fn focused_task(&self) -> Option<TaskId> {
        self.focused_task
    }

    /// The focused stack, always derived (D7): the focused task's
    /// stack; `None` without focus.
    #[must_use]
    pub fn focused_stack(&self, state: &AppState) -> Option<StackId> {
        self.focused_task
            .and_then(|id| state.tasks.get(&id))
            .map(|t| t.stack)
    }

    #[must_use]
    pub const fn selected_stack(&self) -> Option<StackId> {
        self.selected_stack
    }

    #[must_use]
    pub const fn subject_stack(&self) -> Option<StackId> {
        self.subject_stack
    }

    #[must_use]
    pub const fn subject_task(&self) -> Option<TaskId> {
        self.subject_task
    }

    /// Direct view/focus injection (the chunk-2 test seam; real
    /// sessions go through the view cycle and the card clicks).
    pub fn set_view(&mut self, view: ViewId) {
        self.view = view;
    }

    /// Direct focus injection (see [`Self::set_view`]).
    pub fn set_focused(&mut self, task: Option<TaskId>) {
        self.focused_task = task;
    }

    /// The keymap context of the current view (the chunk-2 seam).
    #[must_use]
    pub const fn context(&self) -> ContextId {
        match self.view {
            ViewId::Board => ContextId::Board,
            ViewId::StackDetail => ContextId::StackDetail,
            ViewId::TaskDetail => ContextId::TaskDetail,
            ViewId::Settings => ContextId::Settings,
        }
    }

    /// The selected board's stack ids in display order (D10). No
    /// seeding inside — `reconcile` normalizes before every push; an
    /// unseeded state reads empty.
    #[must_use]
    pub fn stack_ids(&self, state: &AppState) -> Vec<StackId> {
        self.board
            .and_then(|board| state.boards.get(&board))
            .map(|board| stacks_in_board(state, board.id))
            .map(|stacks| stacks.into_iter().map(|s| s.id).collect())
            .unwrap_or_default()
    }

    /// The selected board's task ids in reading order (D7): columns in
    /// stack order, tasks in canonical order.
    #[must_use]
    pub fn reading_order(&self, state: &AppState) -> Vec<TaskId> {
        self.stack_ids(state)
            .into_iter()
            .flat_map(|stack| state.tasks_in_stack(stack))
            .map(|t| t.id)
            .collect()
    }

    fn first_task_of(state: &AppState, stack: StackId) -> Option<TaskId> {
        state.tasks_in_stack(stack).first().map(|t| t.id)
    }

    // -- navigation (D7) ------------------------------------------------

    /// `nav.next-view`: the one cycle with subject validation — a
    /// view without a subject is skipped (an empty board stays put).
    /// Inert while Settings is open (round-1 D5: the cycle never
    /// passes through the settings surface).
    pub fn next_view(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        if self.view == ViewId::Settings {
            return Vec::new();
        }
        self.step_view(state, 1)
    }

    /// `nav.prev-view`: the cycle backwards; inert while Settings is
    /// open (round-1 D5).
    pub fn prev_view(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        if self.view == ViewId::Settings {
            return Vec::new();
        }
        self.step_view(state, -1)
    }

    /// Opens the settings surface (round-1 D6): records the current
    /// view for the exact restore and enters Settings.
    pub fn open_settings(&mut self) -> Vec<FlowOutcome> {
        self.settings_return = Some(self.view);
        self.view = ViewId::Settings;
        vec![FlowOutcome::RefreshNow]
    }

    /// Closes the settings surface: restores the recorded return view,
    /// the board when none was recorded.
    pub fn close_settings(&mut self) -> Vec<FlowOutcome> {
        self.view = self.settings_return.take().unwrap_or(ViewId::Board);
        vec![FlowOutcome::RefreshNow]
    }

    fn step_view(&mut self, state: &AppState, direction: i32) -> Vec<FlowOutcome> {
        let order = [ViewId::Board, ViewId::StackDetail, ViewId::TaskDetail];
        let current = order.iter().position(|v| *v == self.view).unwrap_or(0);
        let next = if direction > 0 {
            order[(current + 1) % order.len()]
        } else {
            order[(current + order.len() - 1) % order.len()]
        };
        if self.enter(state, next) {
            vec![FlowOutcome::RefreshNow]
        } else {
            Vec::new()
        }
    }

    /// Enters `view` if it has a subject; snapshots it (D7). Returns
    /// whether the switch happened.
    fn enter(&mut self, state: &AppState, view: ViewId) -> bool {
        match view {
            ViewId::Board => {
                self.view = ViewId::Board;
                true
            }
            ViewId::StackDetail => {
                let stack = self
                    .focused_stack(state)
                    .or(self.selected_stack)
                    .or_else(|| self.stack_ids(state).first().copied());
                match stack {
                    Some(stack) => {
                        self.view = ViewId::StackDetail;
                        self.subject_stack = Some(stack);
                        true
                    }
                    None => false,
                }
            }
            ViewId::TaskDetail => {
                let task = self.focused_task.or_else(|| {
                    self.subject_stack
                        .and_then(|s| Self::first_task_of(state, s))
                });
                match task {
                    Some(task) => {
                        self.view = ViewId::TaskDetail;
                        self.subject_task = Some(task);
                        true
                    }
                    None => false,
                }
            }
            // The settings surface never enters through the cycle
            // (round-1 D5): only `open_settings` reaches it.
            ViewId::Settings => false,
        }
    }

    /// `nav.focus-board`: back to the board view (D7).
    pub fn focus_board(&mut self) -> Vec<FlowOutcome> {
        self.view = ViewId::Board;
        vec![FlowOutcome::RefreshNow]
    }

    // -- board selection (design §3.8) -----------------------------------

    /// Seeds or repairs the board selection: `None` takes the first
    /// live board; a selection that is no longer live (deleted or
    /// archived under the session) falls back the same way. The one
    /// normalization site — `reconcile` runs it refresh-time; the
    /// board walk and `begin_create_stack` before they read.
    fn normalize_board(&mut self, state: &AppState) {
        let live = self.board.is_some_and(|id| {
            state
                .boards
                .get(&id)
                .is_some_and(|b| !b.deleted && !b.archived)
        });
        if !live {
            self.board = live_board(state).map(|b| b.id);
        }
    }

    /// A board-bar segment click or a targeted select: sets the board,
    /// resets the stack selection to the new board's first stack and
    /// the focus to that stack's first task — the `select_stack`
    /// mirror (D2). Pure view state; no view/subject reset (the
    /// refresh-time subject re-validation pops a stale subject). No
    /// liveness guard on the argument: a stale click cannot name a
    /// dead board through a fresh tables pass, and an interleaved
    /// deletion is repaired by `reconcile`.
    pub fn select_board(&mut self, state: &AppState, board: BoardId) -> Vec<FlowOutcome> {
        self.board = Some(board);
        self.selected_stack = self.stack_ids(state).first().copied();
        self.focused_task = self
            .selected_stack
            .and_then(|stack| Self::first_task_of(state, stack));
        vec![FlowOutcome::RefreshNow]
    }

    /// `board.next`: step to the next live board in canonical order —
    /// no wrap (the ends hold). Inert (`Vec::new()`, no repaint) at
    /// the last board or with fewer than two boards.
    pub fn next_board(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        self.step_board(state, 1)
    }

    /// `board.previous`: the walk backwards, same no-wrap rule.
    pub fn previous_board(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        self.step_board(state, -1)
    }

    fn step_board(&mut self, state: &AppState, direction: i32) -> Vec<FlowOutcome> {
        self.normalize_board(state);
        let boards = live_boards(state);
        let Some(current) = self
            .board
            .and_then(|b| boards.iter().position(|board| board.id == b))
        else {
            return Vec::new();
        };
        let next = i64::from(direction) + i64::try_from(current).unwrap_or(0);
        let Some(neighbour) = boards.get(usize::try_from(next).unwrap_or(usize::MAX)) else {
            return Vec::new(); // no wrap: the ends hold
        };
        self.select_board(state, neighbour.id)
    }

    // -- board focus walk (D7/D8) ----------------------------------------

    /// `nav.next-item` without a move flow: in compact, selection
    /// follows focus at the switcher level first (wrap — segmented
    /// control convention); the task walk itself never wraps (D7).
    pub fn advance_focus(&mut self, state: &AppState, compact: bool) -> Vec<FlowOutcome> {
        self.step_focus(state, 1, compact)
    }

    /// `nav.prev-item` without a move flow; retreating before the
    /// first card in compact returns to the switcher level.
    pub fn retreat_focus(&mut self, state: &AppState, compact: bool) -> Vec<FlowOutcome> {
        self.step_focus(state, -1, compact)
    }

    fn step_focus(&mut self, state: &AppState, direction: i32, compact: bool) -> Vec<FlowOutcome> {
        // Switcher level in compact (no card focused): advance/retreat
        // move the stack selection (wrap) and land on the new stack's
        // first task — one keypress per switch (D8).
        if compact && self.focused_task.is_none() {
            let stacks = self.stack_ids(state);
            if stacks.is_empty() {
                return Vec::new();
            }
            let next = match self.selected_stack {
                None => {
                    if direction > 0 {
                        stacks[0]
                    } else {
                        stacks[stacks.len() - 1]
                    }
                }
                Some(selected) => {
                    let current = stacks.iter().position(|s| *s == selected).unwrap_or(0);
                    stacks[wrapped_index(current, direction, stacks.len())]
                }
            };
            self.selected_stack = Some(next);
            self.focused_task = Self::first_task_of(state, next);
            return vec![FlowOutcome::RefreshNow];
        }
        let domain: Vec<TaskId> = if compact {
            match self
                .selected_stack
                .or_else(|| self.stack_ids(state).first().copied())
            {
                Some(stack) => state
                    .tasks_in_stack(stack)
                    .into_iter()
                    .map(|t| t.id)
                    .collect(),
                None => Vec::new(),
            }
        } else {
            self.reading_order(state)
        };
        let Some(current) = self
            .focused_task
            .and_then(|id| domain.iter().position(|t| *t == id))
        else {
            // No focus yet: entering the walk lands on the first card.
            return match domain.first() {
                Some(first) => {
                    self.focused_task = Some(*first);
                    vec![FlowOutcome::RefreshNow]
                }
                None => Vec::new(),
            };
        };
        let next = i64::from(direction) + i64::try_from(current).unwrap_or(0);
        if next < 0 {
            if compact {
                // Retreat past the first card hands focus back to the
                // switcher level (selection-follows-focus, D8).
                self.focused_task = None;
                return vec![FlowOutcome::RefreshNow];
            }
            return Vec::new(); // no wrap (D7)
        }
        match domain.get(usize::try_from(next).unwrap_or(usize::MAX)) {
            Some(id) => {
                self.focused_task = Some(*id);
                vec![FlowOutcome::RefreshNow]
            }
            None => Vec::new(), // no wrap (D7): the ends hold
        }
    }

    /// The compact switcher's selection move (keys while the switcher
    /// leads, or a segment click — one mechanism, D8). Board focus
    /// resets to the new stack's first task.
    pub fn select_stack(&mut self, state: &AppState, stack: StackId) -> Vec<FlowOutcome> {
        self.selected_stack = Some(stack);
        self.focused_task = Self::first_task_of(state, stack);
        vec![FlowOutcome::RefreshNow]
    }

    /// A card click: sets focus/selection state only (§7.1 mouse
    /// discipline); the action layer does the rest.
    pub fn activate_card(&mut self, task: TaskId, stack: StackId) -> Vec<FlowOutcome> {
        self.focused_task = Some(task);
        self.selected_stack = Some(stack);
        vec![FlowOutcome::RefreshNow]
    }

    /// vi's insert intent inside `TaskDetail` (D5): begin editing the
    /// viewed task's title in place (the view does not switch).
    pub fn edit_subject(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Some(task) = self.subject_task.and_then(|id| state.tasks.get(&id)) else {
            return Vec::new();
        };
        self.flow = Flow::EditTitle { task: task.id };
        self.buffer.clone_from(&task.title);
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// The `Activate` intent with no flow (D9): open the focused
    /// task's detail as a title-edit flow (D14).
    pub fn activate_focused(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Some(task) = self.focused_task.and_then(|id| state.tasks.get(&id)) else {
            return Vec::new();
        };
        self.view = ViewId::TaskDetail;
        self.subject_task = Some(task.id);
        self.flow = Flow::EditTitle { task: task.id };
        self.buffer.clone_from(&task.title);
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    // -- create flow (D8) --------------------------------------------------

    /// The stack a create row opens in: the focused stack (Board) or
    /// the selected/subject stack; `None` on an empty board.
    #[must_use]
    pub fn create_target_stack(&self, state: &AppState) -> Option<StackId> {
        match self.view {
            ViewId::Board => self
                .focused_stack(state)
                .or(self.selected_stack)
                .or_else(|| self.stack_ids(state).first().copied()),
            ViewId::StackDetail => self.subject_stack,
            ViewId::TaskDetail => self
                .subject_task
                .and_then(|t| state.tasks.get(&t))
                .map(|t| t.stack),
            // The settings surface hosts no create flow (round-1 D5).
            ViewId::Settings => None,
        }
    }

    // -- stack column widths (round-1 D16) ---------------------------------

    /// Seeds the device-local widths at mount (the settings' map, uuid
    /// keys already converted). Sub-floor persisted widths heal here
    /// (r2 D2, the single load point — never at reconcile, never at
    /// push, and never written back to config: healing reaches the
    /// stored overlay only when the user next drags/steps). Unknown
    /// boards/stacks are kept — a re-connected board finds its widths
    /// again; liveness pruning happens once, at write time, app-side.
    pub fn seed_stack_widths(&mut self, widths: BTreeMap<BoardId, BTreeMap<StackId, u32>>) {
        self.stack_widths = widths
            .into_iter()
            .map(|(board, stacks)| {
                (
                    board,
                    stacks
                        .into_iter()
                        .map(|(stack, px)| (stack, crate::widths::reclamp_width(px)))
                        .collect::<BTreeMap<StackId, u32>>(),
                )
            })
            .collect();
    }

    /// The stored logical-px width of one column, if the user set one.
    #[must_use]
    pub fn stack_width(&self, board: BoardId, stack: StackId) -> Option<u32> {
        self.stack_widths
            .get(&board)
            .and_then(|m| m.get(&stack))
            .copied()
    }

    /// Stores one column's width (logical px). Clamping is the
    /// caller's duty — the pure math of [`crate::widths`].
    pub fn set_stack_width(&mut self, board: BoardId, stack: StackId, px: u32) {
        self.stack_widths
            .entry(board)
            .or_default()
            .insert(stack, px);
    }

    /// The full per-board width snapshot in `BTreeMap` order (the
    /// `StackWidthsChanged` payload — deterministic, idempotent).
    #[must_use]
    pub fn stack_widths_of(&self, board: BoardId) -> Vec<(StackId, u32)> {
        self.stack_widths
            .get(&board)
            .map(|m| m.iter().map(|(k, v)| (*k, *v)).collect())
            .unwrap_or_default()
    }

    /// The keyboard resize's target stack: focused → selected → first
    /// (the `create_target_stack` Board-view resolution). The adapter
    /// reads it to size the ceiling's "other columns" sum; the resize
    /// verb itself resolves through it too — one home.
    #[must_use]
    pub fn keyboard_resize_target(&self, state: &AppState) -> Option<StackId> {
        self.focused_stack(state)
            .or(self.selected_stack)
            .or_else(|| self.stack_ids(state).first().copied())
    }

    /// The keyboard resize verb (round-1 D18): resolves the target
    /// stack (focused → selected → first, `create_target_stack`'s
    /// Board-view resolution), seeds an absent entry at
    /// [`STACK_WIDTH_DEFAULT_PX`], applies ±[`STACK_WIDTH_STEP_PX`]
    /// clamped to `[STACK_WIDTH_MIN_PX, ceiling]`, and returns
    /// `[StackWidthsChanged, RefreshNow]` — or `Vec::new()` when there
    /// is no stack to address.
    pub fn resize_focused_stack(
        &mut self,
        state: &AppState,
        direction: i32,
        ceiling: u32,
    ) -> Vec<FlowOutcome> {
        let Some(target) = self.keyboard_resize_target(state) else {
            return Vec::new();
        };
        let Some(board) = self.board else {
            return Vec::new();
        };
        let base = self
            .stack_width(board, target)
            .unwrap_or(STACK_WIDTH_DEFAULT_PX);
        let requested = i64::from(base) + i64::from(direction) * i64::from(STACK_WIDTH_STEP_PX);
        let next = clamp_width(requested, STACK_WIDTH_MIN_PX, ceiling);
        self.set_stack_width(board, target, next);
        vec![FlowOutcome::StackWidthsChanged, FlowOutcome::RefreshNow]
    }

    /// `BeginCreate`: opens the row in insert mode (D5).
    pub fn begin_create(&mut self, stack: StackId) -> Vec<FlowOutcome> {
        self.flow = Flow::Create { stack };
        self.buffer.clear();
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// `BeginCreateStack`: opens the stack create row in insert mode
    /// (#304) — no target stack, so an empty board offers it too. The
    /// board-existence refusal lives here (one home): the selection
    /// normalizes first, and a board-less state is refused with an
    /// inert `Vec::new()` — nothing offers a stack create with no
    /// board to create it on.
    pub fn begin_create_stack(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        self.normalize_board(state);
        if self.board.is_none() {
            return Vec::new();
        }
        self.flow = Flow::CreateStack;
        self.buffer.clear();
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// Text commit (D9): the create row appends a task; the title
    /// editor updates it. An empty buffer commits nothing (the flow
    /// simply ends — the CLI owns title validation, the UI dispatches
    /// only well-formed commands). The flow always ends and normal
    /// mode returns.
    pub fn commit_edit(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let title = self.buffer.trim().to_owned();
        let flow = std::mem::take(&mut self.flow);
        self.buffer.clear();
        let mut outcomes = vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow];
        match flow {
            Flow::Create { stack } if !title.is_empty() => {
                outcomes.insert(
                    0,
                    FlowOutcome::Command(StateCommand::CreateTask {
                        title,
                        stack,
                        order: append_order(state, stack),
                    }),
                );
            }
            Flow::CreateStack if !title.is_empty() => {
                // The command names the *selected* board (design §3.8);
                // a board-less state cannot get here —
                // `begin_create_stack` refuses and `reconcile` ends the
                // flow when the last board vanishes.
                if let Some(board) = self.board {
                    outcomes.insert(
                        0,
                        FlowOutcome::Command(StateCommand::CreateStack {
                            board,
                            title,
                            order: append_stack_order(state, board),
                        }),
                    );
                }
            }
            Flow::EditTitle { task } if !title.is_empty() => {
                outcomes.insert(
                    0,
                    FlowOutcome::Command(StateCommand::UpdateTask {
                        id: task,
                        changes: TaskChanges {
                            title: Some(title),
                            ..TaskChanges::default()
                        },
                    }),
                );
            }
            _ => {}
        }
        outcomes
    }

    /// The only non-committing exit (D9): discard the buffer.
    pub fn discard_edit(&mut self) -> Vec<FlowOutcome> {
        self.flow = Flow::None;
        self.buffer.clear();
        vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
    }

    // -- search palette (#348, chunk 9) ------------------------------------

    /// `PaletteSearch`: opens the palette in insert mode with an empty
    /// query (the `begin_create` opening shape). The empty query means
    /// no hits — the palette opens over an empty result list.
    pub fn open_palette(&mut self) -> Vec<FlowOutcome> {
        self.flow = Flow::Palette { selected: 0 };
        self.buffer.clear();
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// Walks the palette cursor by `direction`, wrapping over the
    /// `visible` row count (the menu-cursor ring semantics; inert at
    /// zero rows). The keystroke path resets the cursor to 0 on every
    /// query change (a stale mid-list cursor on a fresh ranking is a
    /// lie); only a refresh-shrunk list clamps here.
    pub fn palette_cursor(&mut self, direction: i32, visible: usize) -> Vec<FlowOutcome> {
        if let Flow::Palette { selected } = &mut self.flow {
            if visible > 0 {
                let len = i64::try_from(visible).unwrap_or(i64::MAX);
                let next = i64::try_from(*selected).unwrap_or(i64::MAX) + i64::from(direction);
                *selected = usize::try_from(next.rem_euclid(len)).unwrap_or(0);
            }
            vec![FlowOutcome::RefreshNow]
        } else {
            Vec::new()
        }
    }

    /// Every palette exit that means close-without-effect: Esc,
    /// `MenuDismiss`, a stale-target jump. The `discard_edit` shape.
    pub fn close_palette(&mut self) -> Vec<FlowOutcome> {
        self.flow = Flow::None;
        self.buffer.clear();
        vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
    }

    /// The jump-to matrix (design D12): retarget board/stack/focus per
    /// hit kind against the live snapshot, then end the flow. A stale
    /// target (the entity died between push and invoke) is inert —
    /// close only, no retarget.
    pub fn palette_jump(&mut self, state: &AppState, target: SearchTarget) -> Vec<FlowOutcome> {
        let mut outcomes = match target {
            SearchTarget::Task(task_id) => {
                let Some(task) = state.tasks.get(&task_id) else {
                    return self.close_palette();
                };
                let Some(stack) = state.stacks.get(&task.stack) else {
                    return self.close_palette();
                };
                let board = stack.board;
                let out = self.select_board(state, board);
                self.focused_task = Some(task_id);
                out
            }
            SearchTarget::Stack(stack_id) => {
                let Some(stack) = state.stacks.get(&stack_id) else {
                    return self.close_palette();
                };
                let board = stack.board;
                let out = self.select_board(state, board);
                self.selected_stack = Some(stack_id);
                self.subject_stack = Some(stack_id);
                self.focused_task = Self::first_task_of(state, stack_id);
                self.view = ViewId::StackDetail;
                out
            }
            SearchTarget::Board(board_id) => {
                if !state.boards.contains_key(&board_id) {
                    return self.close_palette();
                }
                self.select_board(state, board_id)
            }
        };
        // The flow always ends (close_palette's ending; select_board
        // already asked for the repaint).
        self.flow = Flow::None;
        self.buffer.clear();
        outcomes.push(FlowOutcome::SetMode(Mode::Normal));
        outcomes
    }

    // -- move flow (D8) ----------------------------------------------------

    /// `RequestMove` with no flow: the focused task starts moving
    /// toward its own stack. Re-request is the cancel (toggle, zero
    /// new bindings) — the adapter's table routes that to
    /// [`Self::cancel_move`].
    pub fn start_move(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Some(task) = self.focused_task else {
            return Vec::new();
        };
        let Some(target) = self
            .focused_stack(state)
            .or_else(|| self.stack_ids(state).first().copied())
        else {
            return Vec::new();
        };
        self.flow = Flow::Move { task, target };
        vec![FlowOutcome::RefreshNow]
    }

    /// Focus next/prev during a move flow: cycle the target stack
    /// (wrap — in compact this is exactly the switcher highlighting).
    pub fn cycle_move_target(&mut self, state: &AppState, direction: i32) -> Vec<FlowOutcome> {
        let Flow::Move { target, .. } = self.flow else {
            return Vec::new();
        };
        let stacks = self.stack_ids(state);
        if stacks.is_empty() {
            return Vec::new();
        }
        let current = stacks
            .iter()
            .position(|s| *s == target)
            .map_or(0, |i| wrapped_index(i, direction, stacks.len()));
        if let Flow::Move { target, .. } = &mut self.flow {
            *target = stacks[current];
        }
        vec![FlowOutcome::RefreshNow]
    }

    /// `Activate` during a move flow: confirm — `MoveTask` append to
    /// the target (D8).
    pub fn confirm_move(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Flow::Move { task, target } = self.flow else {
            return Vec::new();
        };
        let order = append_order(state, target);
        self.flow = Flow::None;
        vec![
            FlowOutcome::Command(StateCommand::MoveTask {
                id: task,
                stack: target,
                order,
            }),
            FlowOutcome::RefreshNow,
        ]
    }

    /// `RequestMove` during a move flow: cancel, nothing dispatched.
    pub fn cancel_move(&mut self) -> Vec<FlowOutcome> {
        if self.flow == Flow::None {
            return Vec::new();
        }
        self.flow = Flow::None;
        vec![FlowOutcome::RefreshNow]
    }

    // -- refresh-time reconciliation (D7) ---------------------------------

    /// Re-validates subjects and focus against a fresh snapshot: a
    /// deleted task pops `TaskDetail` → Board, a deleted stack pops
    /// `StackDetail` → Board and re-falls-back the compact selection;
    /// flows on vanished entities end without commands.
    pub fn reconcile(&mut self, state: &AppState) {
        self.normalize_board(state);
        let is_live = |id: TaskId| {
            state
                .tasks
                .get(&id)
                .is_some_and(taskboard_domain::Task::is_live)
        };
        if self.focused_task.is_some_and(|t| !is_live(t)) {
            self.focused_task = None;
        }
        let stacks = self.stack_ids(state);
        if self.selected_stack.is_some_and(|s| !stacks.contains(&s)) {
            self.selected_stack = stacks.first().copied();
        }
        if let Some(stack) = self.subject_stack
            && !stacks.contains(&stack)
        {
            self.subject_stack = None;
            if self.view == ViewId::StackDetail {
                self.view = ViewId::Board;
            }
        }
        if let Some(task) = self.subject_task
            && !is_live(task)
        {
            self.subject_task = None;
            if self.view == ViewId::TaskDetail {
                self.view = ViewId::Board;
            }
            if matches!(self.flow, Flow::EditTitle { task: t } if t == task) {
                self.flow = Flow::None;
                self.buffer.clear();
            }
        }
        match self.flow {
            Flow::Create { stack } if !stacks.contains(&stack) => self.flow = Flow::None,
            Flow::CreateStack if state.boards.values().all(|b| b.deleted) => {
                self.flow = Flow::None;
            }
            Flow::Move { task, .. } if !is_live(task) => self.flow = Flow::None,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::BTreeSet;
    use taskboard_domain::{Board, BoardClocks, BoardId, Stack, StackClocks, Task, TaskClocks};

    fn epoch() -> chrono::DateTime<Utc> {
        taskboard_domain::BoardClocks::epoch()
    }

    fn clocks() -> TaskClocks {
        TaskClocks {
            title: epoch(),
            description: epoch(),
            duedate: epoch(),
            scheduled: epoch(),
            done: epoch(),
            position: epoch(),
            labels: epoch(),
            effort: epoch(),
            importance: epoch(),
            urgency: epoch(),
            archived: epoch(),
            deleted: epoch(),
        }
    }

    fn task(id: TaskId, stack: StackId, order: i64) -> Task {
        Task {
            id,
            remote: None,
            title: format!("task-{order}"),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack,
            order,
            labels: BTreeSet::new(),
            effort: None,
            importance: None,
            urgency: None,
            archived: false,
            deleted: false,
            clocks: clocks(),
            remote_seen: None,
        }
    }

    fn stack(id: StackId, board: BoardId, order: i64) -> Stack {
        Stack {
            id,
            remote: None,
            board,
            title: format!("stack-{order}"),
            order,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: epoch(),
                order: epoch(),
                deleted: epoch(),
            },
            remote_seen: None,
        }
    }

    /// A board with stacks s0, s1 (display order) and tasks a, b in s0
    /// and c in s1 (canonical order a, b, c).
    fn fixture() -> (AppState, BoardId, [StackId; 2], [TaskId; 3]) {
        let board_id = BoardId::from_uuid(uuid::Uuid::now_v7());
        let s0 = StackId::from_uuid(uuid::Uuid::now_v7());
        let s1 = StackId::from_uuid(uuid::Uuid::now_v7());
        let a = TaskId::from_uuid(uuid::Uuid::now_v7());
        let b = TaskId::from_uuid(uuid::Uuid::now_v7());
        let c = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        state.boards.insert(
            board_id,
            Board {
                id: board_id,
                remote: None,
                title: "board".to_owned(),
                color: taskboard_domain::Color::new(""),
                archived: false,
                deleted: false,
                clocks: BoardClocks {
                    title: epoch(),
                    color: epoch(),
                    archived: epoch(),
                    deleted: epoch(),
                },
                remote_seen: None,
            },
        );
        state.stacks.insert(s0, stack(s0, board_id, 0));
        state.stacks.insert(s1, stack(s1, board_id, 1));
        state.tasks.insert(a, task(a, s0, 0));
        state.tasks.insert(b, task(b, s0, 1));
        state.tasks.insert(c, task(c, s1, 0));
        (state, board_id, [s0, s1], [a, b, c])
    }

    // -- the view cycle (D7) --------------------------------------------

    #[test]
    fn view_cycle_walks_board_stack_task_and_back() {
        let (state, _, [s0, _], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        assert_eq!(vs.view(), ViewId::Board);

        vs.next_view(&state);
        assert_eq!(vs.view(), ViewId::StackDetail);
        assert_eq!(
            vs.subject_stack(),
            Some(s0),
            "first stack is the fallback subject"
        );

        vs.next_view(&state);
        assert_eq!(vs.view(), ViewId::TaskDetail);
        assert_eq!(
            vs.subject_task(),
            Some(a),
            "first card is the fallback subject"
        );

        vs.next_view(&state);
        assert_eq!(vs.view(), ViewId::Board, "cycle wraps");
        vs.prev_view(&state);
        assert_eq!(vs.view(), ViewId::TaskDetail);
        vs.focus_board();
        assert_eq!(vs.view(), ViewId::Board);
    }

    #[test]
    fn focus_moves_retarget_the_view_cycle_entry() {
        let (state, _, [_, s1], [.., c]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(c, s1);
        vs.next_view(&state);
        assert_eq!(
            vs.subject_stack(),
            Some(s1),
            "StackDetail opens on the focused stack"
        );
        vs.next_view(&state);
        assert_eq!(
            vs.subject_task(),
            Some(c),
            "TaskDetail opens on the focused task"
        );
    }

    #[test]
    fn entering_a_view_snapshots_its_subject() {
        let (state, _, [s0, s1], [a, b, c]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.next_view(&state);
        assert_eq!(vs.subject_stack(), Some(s0));
        // Focus moves in the background view must not retarget it.
        vs.activate_card(c, s1);
        assert_eq!(vs.subject_stack(), Some(s0));
        // …and the TaskDetail subject likewise.
        vs.next_view(&state);
        assert_eq!(vs.subject_task(), Some(c));
        vs.activate_card(b, s0);
        assert_eq!(vs.subject_task(), Some(c));
    }

    #[test]
    fn an_empty_board_keeps_the_cycle_on_board() {
        let mut vs = ViewState::new();
        let state = AppState::default();
        assert_eq!(vs.next_view(&state), Vec::new());
        assert_eq!(vs.view(), ViewId::Board);
    }

    #[test]
    fn cycle_entry_into_task_detail_never_opens_an_edit_flow() {
        // Navigation is not activation: cycling into TaskDetail shows
        // the detail; only `Activate` begins the title edit (D14/D9).
        let (state, _, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.next_view(&state);
        vs.next_view(&state);
        assert_eq!(vs.view(), ViewId::TaskDetail);
        assert_eq!(*vs.flow(), Flow::None);
    }

    // -- the focus walk (D7/D8) -------------------------------------------

    #[test]
    fn reading_order_walk_is_columns_then_cards_without_wrap() {
        let (state, _, _, [a, b, c]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.advance_focus(&state, false);
        assert_eq!(
            vs.focused_task(),
            Some(a),
            "entering the walk takes the first card"
        );
        vs.advance_focus(&state, false);
        vs.advance_focus(&state, false);
        assert_eq!(vs.focused_task(), Some(c));
        vs.advance_focus(&state, false);
        assert_eq!(vs.focused_task(), Some(c), "no wrap at the end");
        vs.retreat_focus(&state, false);
        assert_eq!(vs.focused_task(), Some(b));
        vs.retreat_focus(&state, false);
        vs.retreat_focus(&state, false);
        assert_eq!(vs.focused_task(), Some(a), "no wrap at the start");
    }

    #[test]
    fn compact_walk_covers_the_selected_stack_and_returns_to_the_switcher() {
        let (state, _, [s0, s1], [a, b, c]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        // Entering the walk takes the first stack's first card
        // (one keypress per switch, D8).
        vs.advance_focus(&state, true);
        assert_eq!(vs.selected_stack(), Some(s0));
        assert_eq!(vs.focused_task(), Some(a));
        vs.advance_focus(&state, true);
        assert_eq!(vs.focused_task(), Some(b));
        vs.advance_focus(&state, true);
        assert_eq!(vs.focused_task(), Some(b), "selected stack's end holds");
        vs.retreat_focus(&state, true);
        vs.retreat_focus(&state, true);
        assert_eq!(
            vs.focused_task(),
            None,
            "retreat past the first card yields to the switcher"
        );
        // Switcher level: advance moves the selection and lands on the
        // new stack's first card; selection wraps.
        vs.advance_focus(&state, true);
        assert_eq!(vs.selected_stack(), Some(s1));
        assert_eq!(vs.focused_task(), Some(c));
        vs.retreat_focus(&state, true);
        vs.retreat_focus(&state, true);
        assert_eq!(
            vs.selected_stack(),
            Some(s0),
            "retreat moves the selection back"
        );
        assert_eq!(vs.focused_task(), Some(a));
        vs.retreat_focus(&state, true);
        vs.retreat_focus(&state, true);
        assert_eq!(
            vs.selected_stack(),
            Some(s1),
            "switcher selection wraps backwards"
        );
        assert_eq!(vs.focused_task(), Some(c));
    }

    #[test]
    fn select_stack_resets_focus_to_the_new_stacks_first_task() {
        let (state, _, [s0, s1], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.select_stack(&state, s1);
        vs.select_stack(&state, s0);
        assert_eq!(vs.selected_stack(), Some(s0));
        assert_eq!(vs.focused_task(), Some(a), "first task of the new stack");
    }

    #[test]
    fn activate_card_sets_selection_state_only() {
        let (_state, _, [s0, _], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        let outcomes = vs.activate_card(a, s0);
        assert_eq!(outcomes, vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.focused_task(), Some(a));
        assert_eq!(*vs.flow(), Flow::None, "a click never starts a flow");
    }

    // -- the create flow (D8/D9) -------------------------------------------

    #[test]
    fn create_flow_commits_an_appended_create_task() {
        let (state, _, [s0, _], [_, b, _]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(b, s0);
        let target = vs.create_target_stack(&state).expect("target");
        assert_eq!(target, s0, "the focused stack is the create target");
        vs.begin_create(target);
        assert_eq!(*vs.flow(), Flow::Create { stack: s0 });
        vs.set_buffer("new".to_owned());
        assert_eq!(
            vs.commit_edit(&state)[..1],
            [FlowOutcome::Command(StateCommand::CreateTask {
                title: "new".to_owned(),
                stack: s0,
                order: 2, // append after a(0), b(1)
            })][..]
        );
        assert_eq!(*vs.flow(), Flow::None);
    }

    #[test]
    fn stack_create_flow_commits_the_appended_create_stack() {
        let (state, board_id, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        let outcomes = vs.begin_create_stack(&state);
        assert_eq!(*vs.flow(), Flow::CreateStack);
        assert!(
            outcomes.contains(&FlowOutcome::SetMode(Mode::Insert)),
            "the row opens in insert mode"
        );
        vs.set_buffer("doing".to_owned());
        assert_eq!(
            vs.commit_edit(&state)[..1],
            [FlowOutcome::Command(StateCommand::CreateStack {
                board: board_id,
                title: "doing".to_owned(),
                order: 2, // append after s0(0), s1(1)
            })][..]
        );
        assert_eq!(*vs.flow(), Flow::None, "the flow ends either way");
    }

    #[test]
    fn stack_create_flow_works_on_an_empty_board() {
        // The point of #304: no target stack needed.
        let (mut state, board_id, _, _) = fixture();
        let stacks: Vec<_> = state.stacks.keys().copied().collect();
        for id in stacks {
            state.stacks.remove(&id);
        }
        let tasks: Vec<_> = state.tasks.keys().copied().collect();
        for id in tasks {
            state.tasks.remove(&id);
        }
        let _ = board_id;
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_create_stack(&state);
        vs.set_buffer("first".to_owned());
        assert_eq!(
            vs.commit_edit(&state)[..1],
            [FlowOutcome::Command(StateCommand::CreateStack {
                board: board_id,
                title: "first".to_owned(),
                order: 0,
            })][..]
        );
    }

    #[test]
    fn begin_create_stack_refuses_without_a_board() {
        // The one-home refusal: a board-less state gets no flow at all,
        // so nothing can command against a nonexistent board later.
        let (mut state, _, _, _) = fixture();
        state.boards.clear();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        assert_eq!(
            vs.begin_create_stack(&state),
            Vec::new(),
            "the refusal is inert"
        );
        assert_eq!(*vs.flow(), Flow::None, "no flow opened");
        vs.set_buffer("first".to_owned());
        assert!(
            !vs.commit_edit(&state)
                .iter()
                .any(|o| matches!(o, FlowOutcome::Command(_))),
            "no board commits no command"
        );
    }

    #[test]
    fn an_empty_stack_create_buffer_commits_nothing() {
        let (state, _, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_create_stack(&state);
        vs.set_buffer("   ".to_owned());
        let outcomes = vs.commit_edit(&state);
        assert!(
            !outcomes
                .iter()
                .any(|o| matches!(o, FlowOutcome::Command(_))),
            "whitespace commits nothing"
        );
        assert_eq!(*vs.flow(), Flow::None, "the flow ends either way");
    }

    #[test]
    fn append_stack_order_continues_the_canonical_board_order() {
        let (state, board_id, [s0, s1], _) = fixture();
        assert_eq!(append_stack_order(&state, board_id), 2);
        let mut empty = state.clone();
        let stacks: Vec<_> = empty.stacks.keys().copied().collect();
        for id in stacks {
            empty.stacks.remove(&id);
        }
        assert_eq!(append_stack_order(&empty, board_id), 0);
        let _ = (s0, s1);
    }

    /// The two-board fixture (deterministic ids: board 1 < board 2, so
    /// board 1 is the seed): board one carries the classic two-stack
    /// shape (stack a: tasks a, b; stack b: task c), board two one
    /// stack (c: task d).
    fn two_board_fixture() -> (AppState, BoardId, BoardId, [StackId; 3], [TaskId; 4]) {
        let b1 = BoardId::from_uuid(uuid::Uuid::from_u128(0x1001));
        let b2 = BoardId::from_uuid(uuid::Uuid::from_u128(0x1002));
        let sa = StackId::from_uuid(uuid::Uuid::from_u128(0x2001));
        let sb = StackId::from_uuid(uuid::Uuid::from_u128(0x2002));
        let sc = StackId::from_uuid(uuid::Uuid::from_u128(0x2003));
        let ta = TaskId::from_uuid(uuid::Uuid::from_u128(0x3001));
        let tb = TaskId::from_uuid(uuid::Uuid::from_u128(0x3002));
        let tc = TaskId::from_uuid(uuid::Uuid::from_u128(0x3003));
        let td = TaskId::from_uuid(uuid::Uuid::from_u128(0x3004));
        let mut state = AppState::default();
        for (id, title) in [(b1, "board one"), (b2, "board two")] {
            state.boards.insert(
                id,
                Board {
                    id,
                    remote: None,
                    title: title.to_owned(),
                    color: taskboard_domain::Color::new(""),
                    archived: false,
                    deleted: false,
                    clocks: BoardClocks {
                        title: epoch(),
                        color: epoch(),
                        archived: epoch(),
                        deleted: epoch(),
                    },
                    remote_seen: None,
                },
            );
        }
        state.stacks.insert(sa, stack(sa, b1, 0));
        state.stacks.insert(sb, stack(sb, b1, 1));
        state.stacks.insert(sc, stack(sc, b2, 0));
        state.tasks.insert(ta, task(ta, sa, 0));
        state.tasks.insert(tb, task(tb, sa, 1));
        state.tasks.insert(tc, task(tc, sb, 0));
        state.tasks.insert(td, task(td, sc, 0));
        (state, b1, b2, [sa, sb, sc], [ta, tb, tc, td])
    }

    // -- board selection (design §3.8) -------------------------------------

    #[test]
    fn reconcile_seeds_the_lowest_live_board() {
        let (state, b1, b2, _, _) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        assert_eq!(vs.board(), Some(b1), "the lowest live board seeds");

        // An archived lowest board seeds the next one.
        let mut archived = state.clone();
        archived.boards.get_mut(&b1).expect("board").archived = true;
        let mut vs = ViewState::new();
        vs.reconcile(&archived);
        assert_eq!(vs.board(), Some(b2));

        // Zero boards stays None, and the stack read is empty.
        let mut empty = state;
        empty.boards.clear();
        let mut vs = ViewState::new();
        vs.reconcile(&empty);
        assert_eq!(vs.board(), None);
        assert_eq!(vs.stack_ids(&empty), Vec::new());
    }

    #[test]
    fn reconcile_repairs_a_dead_board_selection_and_pops_the_stale_subject() {
        let (state, b1, b2, [sa, ..], _) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.select_board(&state, b2);
        assert_eq!(vs.board(), Some(b2));

        // A deleted selection falls back to the first live board.
        let mut dead = state.clone();
        dead.boards.get_mut(&b2).expect("board").deleted = true;
        vs.reconcile(&dead);
        assert_eq!(vs.board(), Some(b1), "dead selection re-seeds");

        // An archived selection repairs the same way.
        let mut archived = state.clone();
        archived.boards.get_mut(&b2).expect("board").archived = true;
        vs.select_board(&state, b2);
        vs.reconcile(&archived);
        assert_eq!(vs.board(), Some(b1));

        // A stale StackDetail subject of the old board pops to Board:
        // select_board does not reset the view (the select_stack
        // mirror); the refresh-time subject re-validation does.
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.select_stack(&state, sa);
        vs.next_view(&state);
        assert_eq!(vs.view(), ViewId::StackDetail);
        vs.select_board(&state, b2);
        vs.reconcile(&state);
        assert_eq!(vs.view(), ViewId::Board, "the stale subject pops");
    }

    #[test]
    fn select_board_resets_the_selection_like_select_stack() {
        let (state, b1, b2, [sa, ..], [ta, ..]) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.select_board(&state, b2);
        let outcomes = vs.select_board(&state, b1);
        assert_eq!(outcomes, vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.board(), Some(b1));
        assert_eq!(vs.selected_stack(), Some(sa), "the new board's first stack");
        assert_eq!(vs.focused_task(), Some(ta), "that stack's first task");
        assert!(
            !outcomes
                .iter()
                .any(|o| matches!(o, FlowOutcome::Command(_))),
            "the dumb-client pin: switching dispatches nothing"
        );

        // An empty target board leaves both selections None with the
        // same single outcome (select_stack's task-less-stack shape).
        let mut stripped = state;
        let stacks: Vec<_> = stripped
            .stacks
            .values()
            .filter(|s| s.board == b2)
            .map(|s| s.id)
            .collect();
        for id in stacks {
            stripped.stacks.remove(&id);
        }
        stripped
            .tasks
            .retain(|_, t| stripped.stacks.get(&t.stack).is_none_or(|s| s.board != b2));
        vs.select_board(&stripped, b2);
        assert_eq!(vs.selected_stack(), None);
        assert_eq!(vs.focused_task(), None);
    }

    #[test]
    fn the_board_walk_steps_without_wrapping() {
        let (state, b1, b2, _, _) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        assert_eq!(vs.next_board(&state), vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.board(), Some(b2));
        assert_eq!(vs.next_board(&state), Vec::new(), "the last end holds");
        assert_eq!(vs.board(), Some(b2));
        assert_eq!(vs.previous_board(&state), vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.board(), Some(b1));
        assert_eq!(vs.previous_board(&state), Vec::new(), "the first end holds");
        assert_eq!(vs.board(), Some(b1));
        assert!(
            !vs.next_board(&state)
                .iter()
                .chain(vs.previous_board(&state).iter())
                .any(|o| matches!(o, FlowOutcome::Command(_))),
            "the dumb-client pin: the walk dispatches nothing"
        );

        // A single-board state is inert in both directions.
        let mut single = state.clone();
        single.boards.remove(&b2);
        let mut vs = ViewState::new();
        vs.reconcile(&single);
        assert_eq!(vs.next_board(&single), Vec::new());
        assert_eq!(vs.previous_board(&single), Vec::new());

        // An unseeded state seeds, then steps (normalize-first).
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        assert_eq!(vs.next_board(&state), vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.board(), Some(b2));
    }

    #[test]
    fn the_create_stack_flow_names_the_selected_board() {
        // Replaces the interim first-live-board fill: the command names
        // the board the view selected, with that board's append order.
        let (state, b1, b2, [sa, sb, sc], _) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.select_board(&state, b2);
        let outcomes = vs.begin_create_stack(&state);
        assert!(
            outcomes.contains(&FlowOutcome::SetMode(Mode::Insert)),
            "the row opens in insert mode"
        );
        vs.set_buffer("x".to_owned());
        match &vs.commit_edit(&state)[0] {
            FlowOutcome::Command(StateCommand::CreateStack { board, order, .. }) => {
                assert_eq!(board, &b2);
                assert_eq!(order, &1, "append after board two's single stack");
            }
            other => panic!("the create-stack command must commit, got {other:?}"),
        }

        // And on board one the command names board one.
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_create_stack(&state);
        vs.set_buffer("x".to_owned());
        match &vs.commit_edit(&state)[0] {
            FlowOutcome::Command(StateCommand::CreateStack { board, order, .. }) => {
                assert_eq!(board, &b1);
                assert_eq!(order, &2, "append after board one's two stacks");
            }
            other => panic!("the create-stack command must commit, got {other:?}"),
        }
        let _ = (sa, sb, sc);
    }

    #[test]
    fn create_flow_in_stack_detail_targets_the_subject() {
        let (state, _, [s0, s1], _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.select_stack(&state, s1);
        vs.next_view(&state);
        assert_eq!(vs.view(), ViewId::StackDetail);
        assert_eq!(vs.create_target_stack(&state), Some(s1), "subject stack");
        let _ = s0;
    }

    #[test]
    fn an_empty_create_buffer_commits_nothing_but_ends_the_flow() {
        let (state, _, [s0, _], _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_create(s0);
        vs.set_buffer("   ".to_owned());
        let outcomes = vs.commit_edit(&state);
        assert!(
            !outcomes
                .iter()
                .any(|o| matches!(o, FlowOutcome::Command(_)))
        );
        assert_eq!(*vs.flow(), Flow::None);
    }

    #[test]
    fn discard_edit_is_the_only_non_committing_exit() {
        let (_state, _, [s0, _], _) = fixture();
        let mut vs = ViewState::new();
        vs.begin_create(s0);
        vs.set_buffer("kept".to_owned());
        let outcomes = vs.discard_edit();
        assert_eq!(
            outcomes,
            vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
        );
        assert_eq!(vs.buffer(), "");
        assert_eq!(*vs.flow(), Flow::None);
    }

    // -- the title edit flow (D14) ------------------------------------------

    #[test]
    fn activate_focused_opens_the_title_edit_with_the_mirror_seeded() {
        let (state, _, [s0, _], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.activate_focused(&state);
        assert_eq!(vs.view(), ViewId::TaskDetail);
        assert_eq!(vs.subject_task(), Some(a));
        assert_eq!(*vs.flow(), Flow::EditTitle { task: a });
        assert_eq!(
            vs.buffer(),
            "task-0",
            "the mirror starts at the current title"
        );
        vs.set_buffer("renamed".to_owned());
        assert_eq!(
            vs.commit_edit(&state)[..1],
            [FlowOutcome::Command(StateCommand::UpdateTask {
                id: a,
                changes: TaskChanges {
                    title: Some("renamed".to_owned()),
                    ..TaskChanges::default()
                },
            })][..]
        );
    }

    #[test]
    fn insert_intent_in_task_detail_edits_the_subject_in_place() {
        let (state, _, [s0, _], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.activate_focused(&state);
        vs.discard_edit();
        // Still in TaskDetail (display mode): `i` edits the subject.
        let outcomes = vs.edit_subject(&state);
        assert_eq!(*vs.flow(), Flow::EditTitle { task: a });
        assert_eq!(vs.buffer(), "task-0");
        assert!(matches!(
            outcomes.first(),
            Some(FlowOutcome::SetMode(Mode::Insert))
        ));
        assert_eq!(
            vs.commit_edit(&state)[..1],
            [FlowOutcome::Command(StateCommand::UpdateTask {
                id: a,
                changes: TaskChanges {
                    title: Some("task-0".to_owned()),
                    ..TaskChanges::default()
                },
            })][..]
        );
    }

    // -- the move flow (D8/D9) -----------------------------------------------

    #[test]
    fn move_flow_cycles_the_target_and_confirms_with_append() {
        let (state, _, [s0, s1], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.start_move(&state);
        assert_eq!(
            *vs.flow(),
            Flow::Move {
                task: a,
                target: s0
            },
            "the flow starts on the focused task's own stack"
        );
        vs.cycle_move_target(&state, 1);
        assert_eq!(
            *vs.flow(),
            Flow::Move {
                task: a,
                target: s1
            }
        );
        vs.cycle_move_target(&state, -1);
        assert_eq!(
            *vs.flow(),
            Flow::Move {
                task: a,
                target: s0
            }
        );
        vs.cycle_move_target(&state, -1);
        assert_eq!(
            *vs.flow(),
            Flow::Move {
                task: a,
                target: s1
            },
            "cycle wraps backwards"
        );

        let outcomes = vs.confirm_move(&state);
        assert_eq!(
            outcomes[..1],
            [FlowOutcome::Command(StateCommand::MoveTask {
                id: a,
                stack: s1,
                order: 1, // append after c(0)
            })][..]
        );
        assert_eq!(*vs.flow(), Flow::None);
    }

    #[test]
    fn re_requesting_move_cancels_without_dispatch() {
        let (state, _, [s0, _], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.start_move(&state);
        let outcomes = vs.cancel_move();
        assert_eq!(outcomes, vec![FlowOutcome::RefreshNow]);
        assert_eq!(*vs.flow(), Flow::None);
    }

    #[test]
    fn move_without_focus_is_inert() {
        let (state, _, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        assert_eq!(vs.start_move(&state), Vec::new());
        assert_eq!(vs.confirm_move(&state), Vec::new());
    }

    #[test]
    fn flows_are_exclusive_starting_one_ends_the_other() {
        let (state, _, [s0, _], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.start_move(&state);
        // Beginning a create ends the move flow (one flow at a time).
        vs.begin_create(s0);
        assert!(matches!(vs.flow(), Flow::Create { .. }));
        vs.activate_focused(&state);
        assert!(
            matches!(vs.flow(), Flow::EditTitle { .. }),
            "activation replaces the flow"
        );
    }

    // -- reconciliation (D7) ---------------------------------------------------

    #[test]
    fn reconcile_pops_task_detail_when_the_subject_is_deleted() {
        let (mut state, _, [s0, _], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.activate_focused(&state);
        assert_eq!(vs.view(), ViewId::TaskDetail);
        state.tasks.get_mut(&a).expect("task").deleted = true;
        vs.reconcile(&state);
        assert_eq!(vs.view(), ViewId::Board, "deleted subject pops to Board");
        assert_eq!(vs.focused_task(), None);
        assert_eq!(
            *vs.flow(),
            Flow::None,
            "the orphaned edit flow ends silently"
        );
    }

    #[test]
    fn reconcile_refalls_back_the_compact_selection_and_pops_stack_detail() {
        let (mut state, _, [s0, s1], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.select_stack(&state, s1);
        vs.next_view(&state);
        assert_eq!(vs.view(), ViewId::StackDetail);
        state.stacks.get_mut(&s1).expect("stack").deleted = true;
        vs.reconcile(&state);
        assert_eq!(vs.view(), ViewId::Board);
        assert_eq!(vs.selected_stack(), Some(s0), "selection re-falls-back");
    }

    #[test]
    fn reconcile_ends_flows_on_vanished_targets() {
        let (mut state, _, [s0, s1], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.start_move(&state);
        state.tasks.get_mut(&a).expect("task").deleted = true;
        vs.reconcile(&state);
        assert_eq!(*vs.flow(), Flow::None);

        vs.begin_create(s1);
        state.stacks.get_mut(&s1).expect("stack").deleted = true;
        vs.reconcile(&state);
        assert_eq!(*vs.flow(), Flow::None);
    }

    #[test]
    fn append_order_continues_the_canonical_order() {
        let (state, _, [s0, s1], _) = fixture();
        assert_eq!(append_order(&state, s0), 2);
        assert_eq!(append_order(&state, s1), 1);
        let empty = StackId::from_uuid(uuid::Uuid::now_v7());
        assert_eq!(append_order(&state, empty), 0);
    }

    // -- stack column widths (round-1 D16) ---------------------------------

    /// T-3 — the keyboard semantics: an auto column seeds at
    /// DEFAULT ± STEP, the ends clamp at min/ceiling, and every step
    /// emits `[StackWidthsChanged, RefreshNow]`.
    #[test]
    fn resize_focused_stack_seeds_steps_and_clamps() {
        let (state, board, [s0, _], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);

        let ceiling = 1_000u32;
        let outcomes = vs.resize_focused_stack(&state, 1, ceiling);
        assert_eq!(
            outcomes,
            vec![FlowOutcome::StackWidthsChanged, FlowOutcome::RefreshNow]
        );
        assert_eq!(
            vs.stack_width(board, s0),
            Some(crate::widths::STACK_WIDTH_DEFAULT_PX + crate::widths::STACK_WIDTH_STEP_PX),
            "an auto column seeds at DEFAULT, then steps"
        );

        // Narrower clamps at the readable floor.
        for _ in 0..20 {
            vs.resize_focused_stack(&state, -1, ceiling);
        }
        assert_eq!(
            vs.stack_width(board, s0),
            Some(crate::widths::STACK_WIDTH_MIN_PX),
            "narrowing clamps at the min"
        );

        // Wider clamps at the given ceiling.
        for _ in 0..60 {
            vs.resize_focused_stack(&state, 1, ceiling);
        }
        assert_eq!(vs.stack_width(board, s0), Some(ceiling));
    }

    /// T-3 — target resolution: focused → selected → first; inert
    /// without any stack.
    #[test]
    fn resize_targets_focused_then_selected_then_first_and_is_inert_without_stacks() {
        let (state, _, [s0, s1], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        let ceiling = 1_000u32;

        // No focus, no selection: the first stack.
        vs.resize_focused_stack(&state, 1, ceiling);
        assert_eq!(vs.selected_stack(), None);
        assert!(
            vs.stack_width(vs.board().expect("board"), s0).is_some(),
            "the first stack was addressed"
        );

        // Selection wins over first.
        vs.select_stack(&state, s1);
        vs.resize_focused_stack(&state, 1, ceiling);
        let board = vs.board().expect("board");
        assert_eq!(
            vs.stack_width(board, s1),
            Some(crate::widths::STACK_WIDTH_DEFAULT_PX + crate::widths::STACK_WIDTH_STEP_PX),
            "the selected stack was addressed"
        );

        // Focus wins over selection.
        vs.activate_card(a, s0);
        vs.resize_focused_stack(&state, 1, ceiling);
        assert!(
            vs.stack_width(board, s0).unwrap_or(0) > crate::widths::STACK_WIDTH_DEFAULT_PX,
            "the focused stack was addressed"
        );

        // A stack-less state is inert.
        let mut empty = ViewState::new();
        let bare = AppState::default();
        empty.reconcile(&bare);
        assert_eq!(empty.resize_focused_stack(&bare, 1, ceiling), Vec::new());
    }

    /// T-4 — the restart-apply half: seeding lands, the snapshot reads
    /// back in `BTreeMap` order, and `reconcile` leaves the map alone
    /// (pruning is the app forwarder's write-time job).
    #[test]
    fn seed_and_snapshot_round_trip_and_reconcile_leaves_widths_alone() {
        use std::collections::BTreeMap;
        let (state, b1, b2, [sa, sb, sc], _) = two_board_fixture();
        let mut vs = ViewState::new();
        let mut widths: BTreeMap<BoardId, BTreeMap<StackId, u32>> = BTreeMap::new();
        widths.insert(b2, BTreeMap::from([(sc, 300)]));
        // E-4 (r2 D2): the seed heals at load, so the round-trip seeds
        // floor-respecting values; the sub-floor heal is T-3's subject.
        widths.insert(b1, BTreeMap::from([(sb, 180), (sa, 240)]));
        vs.seed_stack_widths(widths);
        vs.reconcile(&state);

        assert_eq!(vs.stack_width(b1, sa), Some(240));
        assert_eq!(vs.stack_width(b1, sb), Some(180));
        assert_eq!(vs.stack_width(b2, sc), Some(300));
        assert_eq!(
            vs.stack_widths_of(b1),
            vec![(sa, 240), (sb, 180)],
            "BTreeMap order, full per-board snapshot"
        );
        assert_eq!(vs.stack_widths_of(b2), vec![(sc, 300)]);
        assert_eq!(
            vs.stack_widths_of(BoardId::from_uuid(uuid::Uuid::now_v7())),
            Vec::new()
        );
        let _ = b1;
    }

    /// T-3 (r2 D2) — the load-time heal: a sub-floor persisted entry
    /// seeds as the floor; an at-or-above-floor entry passes through
    /// untouched. The reconcile-leaves-alone invariant is the round
    /// trip's (the test above).
    #[test]
    fn seed_stack_widths_heals_below_floor_entries() {
        use std::collections::BTreeMap;
        let (state, b1, _b2, [sa, sb, _sc], _) = two_board_fixture();
        let mut vs = ViewState::new();
        let mut widths: BTreeMap<BoardId, BTreeMap<StackId, u32>> = BTreeMap::new();
        widths.insert(
            b1,
            BTreeMap::from([(sa, 96), (sb, crate::widths::STACK_WIDTH_MIN_PX + 80)]),
        );
        vs.seed_stack_widths(widths);
        vs.reconcile(&state);

        assert_eq!(
            vs.stack_width(b1, sa),
            Some(crate::widths::STACK_WIDTH_MIN_PX),
            "the legacy 96-px entry healed at load"
        );
        assert_eq!(
            vs.stack_width(b1, sb),
            Some(crate::widths::STACK_WIDTH_MIN_PX + 80),
            "an above-floor entry is untouched"
        );
    }

    // -- the search palette (#348, chunk 9) -----------------------------

    fn palette_two_board_fixture() -> (AppState, BoardId, BoardId, StackId, TaskId) {
        let b0 = BoardId::from_uuid(uuid::Uuid::now_v7());
        let b1 = BoardId::from_uuid(uuid::Uuid::now_v7());
        let s1 = StackId::from_uuid(uuid::Uuid::now_v7());
        let needle = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        for (id, title) in [(b0, "board one"), (b1, "board two")] {
            state.boards.insert(
                id,
                Board {
                    id,
                    remote: None,
                    title: title.to_owned(),
                    color: taskboard_domain::Color::new(""),
                    archived: false,
                    deleted: false,
                    clocks: BoardClocks {
                        title: epoch(),
                        color: epoch(),
                        archived: epoch(),
                        deleted: epoch(),
                    },
                    remote_seen: None,
                },
            );
        }
        state.stacks.insert(s1, stack(s1, b1, 0));
        let mut needle_task = task(needle, s1, 0);
        needle_task.title = "zebra needle".to_owned();
        state.tasks.insert(needle, needle_task);
        (state, b0, b1, s1, needle)
    }

    #[test]
    fn the_palette_opens_insert_closes_and_walks_the_cursor() {
        let mut vs = ViewState::new();
        // Opening: insert mode + cleared buffer + cursor at 0.
        vs.set_buffer("stale".to_owned());
        let outcomes = vs.open_palette();
        assert_eq!(
            outcomes,
            vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
        );
        assert_eq!(vs.flow(), &Flow::Palette { selected: 0 });
        assert_eq!(vs.buffer(), "");

        // The cursor wraps over the visible count, both directions.
        let down = vs.palette_cursor(1, 3);
        assert_eq!(down, vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.flow(), &Flow::Palette { selected: 1 });
        vs.palette_cursor(1, 3);
        vs.palette_cursor(1, 3);
        assert_eq!(vs.flow(), &Flow::Palette { selected: 0 });
        vs.palette_cursor(-1, 3);
        assert_eq!(vs.flow(), &Flow::Palette { selected: 2 });

        // Inert at zero visible rows.
        vs.palette_cursor(1, 0);
        assert_eq!(vs.flow(), &Flow::Palette { selected: 2 });

        // Closing: normal mode, cleared buffer.
        let outcomes = vs.close_palette();
        assert_eq!(
            outcomes,
            vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
        );
        assert_eq!(vs.flow(), &Flow::None);
        assert_eq!(vs.buffer(), "");
    }

    #[test]
    fn the_palette_jump_matrix_retargets_per_kind() {
        let (state, b0, b1, s1, needle) = palette_two_board_fixture();

        // Task hit: the needle's board selected, the task focused, the
        // flow ended.
        let mut vs = ViewState::new();
        vs.select_board(&state, b0);
        vs.open_palette();
        let outcomes = vs.palette_jump(&state, SearchTarget::Task(needle));
        assert!(outcomes.contains(&FlowOutcome::SetMode(Mode::Normal)));
        assert_eq!(vs.flow(), &Flow::None);
        assert_eq!(vs.board(), Some(b1));
        assert_eq!(vs.focused_task(), Some(needle));

        // Stack hit: the stack subject + selection, its first task
        // focused, StackDetail entered.
        let mut vs = ViewState::new();
        vs.select_board(&state, b0);
        vs.open_palette();
        vs.palette_jump(&state, SearchTarget::Stack(s1));
        assert_eq!(vs.view(), ViewId::StackDetail);
        assert_eq!(vs.board(), Some(b1));
        assert_eq!(vs.focused_task(), Some(needle));

        // Board hit: plain board select.
        let mut vs = ViewState::new();
        vs.select_board(&state, b1);
        vs.open_palette();
        vs.palette_jump(&state, SearchTarget::Board(b0));
        assert_eq!(vs.view(), ViewId::Board);
        assert_eq!(vs.board(), Some(b0));
        assert_eq!(vs.flow(), &Flow::None);
    }

    #[test]
    fn a_stale_palette_target_is_inert_close_only() {
        let (mut state, b0, _b1, s1, needle) = palette_two_board_fixture();
        let mut vs = ViewState::new();
        vs.select_board(&state, b0);
        vs.open_palette();
        vs.set_buffer("zebra".to_owned());
        vs.palette_cursor(1, 1);

        // The needle dies between push and invoke: close only — the
        // board selection and focus never retarget.
        let dead = SearchTarget::Task(needle);
        state.tasks.remove(&needle);
        let outcomes = vs.palette_jump(&state, dead);
        assert_eq!(
            outcomes,
            vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
        );
        assert_eq!(vs.flow(), &Flow::None);
        assert_eq!(vs.buffer(), "");
        assert_eq!(vs.board(), Some(b0));
        assert_eq!(vs.focused_task(), None);
        let _ = s1;
    }
}

#[cfg(test)]
mod settings_surface_tests {
    use super::*;

    fn one_board() -> AppState {
        let board = BoardId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        let epoch = taskboard_domain::BoardClocks::epoch();
        state.boards.insert(
            board,
            taskboard_domain::Board {
                id: board,
                remote: None,
                title: "board".into(),
                color: taskboard_domain::Color::new(""),
                archived: false,
                deleted: false,
                clocks: taskboard_domain::BoardClocks {
                    title: epoch,
                    color: epoch,
                    archived: epoch,
                    deleted: epoch,
                },
                remote_seen: None,
            },
        );
        state
    }

    fn editing_state(stack: StackId) -> AppState {
        let mut state = one_board();
        let epoch = taskboard_domain::BoardClocks::epoch();
        state.stacks.insert(
            stack,
            taskboard_domain::Stack {
                id: stack,
                remote: None,
                board: *state.boards.keys().next().expect("board"),
                title: "stack".into(),
                order: 0,
                archived: false,
                deleted: false,
                clocks: taskboard_domain::StackClocks {
                    title: epoch,
                    order: epoch,
                    deleted: epoch,
                },
                remote_seen: None,
            },
        );
        state
    }

    /// T2 — open records the return view; close restores it exactly
    /// (the board when nothing was recorded); the cycle verbs are
    /// inert while open; the context is `Settings`; reconcile keeps it
    /// open and still normalizes the board.
    #[test]
    fn the_settings_surface_choreography_is_exact() {
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        let state = editing_state(stack);

        // From StackDetail: open → context + record; close → exact
        // restore.
        let mut vs = ViewState::new();
        vs.reconcile(&state); // seeds the board selection (the refresh-time rule)
        assert!(vs.enter(&state, ViewId::StackDetail));
        assert_eq!(vs.context(), ContextId::StackDetail);
        assert_eq!(vs.open_settings(), vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.view(), ViewId::Settings);
        assert_eq!(vs.context(), ContextId::Settings);
        assert_eq!(vs.close_settings(), vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.view(), ViewId::StackDetail);

        // The board default: closing with nothing recorded lands on
        // the board.
        let mut vs = ViewState::new();
        vs.open_settings();
        vs.close_settings();
        assert_eq!(vs.view(), ViewId::Board);

        // The cycle verbs are inert while open (both directions).
        let mut vs = ViewState::new();
        vs.open_settings();
        assert_eq!(vs.next_view(&state), Vec::new());
        assert_eq!(vs.prev_view(&state), Vec::new());
        assert_eq!(vs.view(), ViewId::Settings);

        // Reconcile while open: stays open, still normalizes the board
        // selection (the seed rule ran — the board is live).
        vs.reconcile(&state);
        assert_eq!(vs.view(), ViewId::Settings);
        assert!(vs.board().is_some());

        // The surface never enters through the cycle (the enter-guard
        // arm): the ring walk from the board never reaches Settings.
        let mut vs = ViewState::new();
        for _ in 0..4 {
            vs.next_view(&state);
            assert_ne!(vs.view(), ViewId::Settings);
        }
    }
}

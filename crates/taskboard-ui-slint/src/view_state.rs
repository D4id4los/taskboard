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

use chrono::{DateTime, Utc};
use taskboard_domain::{
    AppState, BoardId, SearchTarget, StackId, StateCommand, TaskChanges, TaskId,
};

use crate::actions::ContextId;
use crate::presentation::{live_board, live_boards, stacks_in_board};
use crate::router::Mode;
use crate::widths::{STACK_WIDTH_DEFAULT_PX, STACK_WIDTH_MIN_PX, STACK_WIDTH_STEP_PX, clamp_width};

/// The pass's time bundle (chunk 10 D4): the fresh `now` and the
/// `day_window` pair the adapter derives from its held
/// `(day_start_time, offset)` custody — time is data, recomputed on
/// every refresh pass (the rollover rides the existing repaint driver;
/// no new timer, D-V4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayCtx {
    pub now: DateTime<Utc>,
    pub window: (DateTime<Utc>, DateTime<Utc>),
}

/// The v0.1.0 views (D7): one tree, one cycle
/// Today → Board → `StackDetail` → `TaskDetail` → Today (chunk 10 D2
/// added `Today` at the cycle head; the `Default` stays on `Board` —
/// `ViewState::new()` keeps every existing caller's shape, the
/// desktop-default is `new_starting`'s argument).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewId {
    Today,
    #[default]
    Board,
    StackDetail,
    TaskDetail,
    /// The settings surface (round-1 D5/D24b): reachable only through
    /// `open_settings` — never a member of the `step_view` cycle.
    Settings,
}

/// The kiosk ambient display's modes (issue #363): the two
/// degenerations the rotation cycles. Declaration order carries the
/// config mapping; the [`KioskMode::view`] mapping is total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KioskMode {
    /// The pinned board's kanban reading.
    Board,
    /// The today view's upcoming-oriented reading.
    Today,
}

impl KioskMode {
    /// The view the mode renders — the existing compositions, never a
    /// kiosk-specific one.
    #[must_use]
    pub const fn view(self) -> ViewId {
        match self {
            Self::Board => ViewId::Board,
            Self::Today => ViewId::Today,
        }
    }
}

/// Why a [`KioskRotation`] refused to mint. The config validation owns
/// the refusal at the deployment boundary; this constructor is the
/// trust boundary's last gate (the house A1 pattern — fallible
/// minting, no derive crates, no validate-and-forget).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum KioskRotationError {
    /// Fewer than two distinct modes — a one-mode "rotation" is a
    /// no-op wearing a config.
    #[error("kiosk rotation needs at least two distinct modes")]
    TooFewModes,
    /// Duplicate entries — a duplicate splits the dwell cycle
    /// pointlessly.
    #[error("kiosk rotation modes must be distinct")]
    DuplicateModes,
}

/// The kiosk display-mode state machine (KDF-11, D-V3): the enabled
/// modes in their declared rotation order plus the dwell. Pure data —
/// the adapter owns the timer; [`KioskRotation::tick`] is the only
/// transition. No time lives here: the timer IS the dwell state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KioskRotation {
    /// The rotation order; ≥ 2 distinct entries, validated at minting.
    modes: Vec<KioskMode>,
    /// Per-mode display time; ≥ the config floor, validated at the
    /// config boundary before minting.
    dwell: std::time::Duration,
    /// Index into `modes` — the active display mode.
    active: usize,
}

impl KioskRotation {
    /// Mints the rotation over `modes` in declared order. `modes` must
    /// hold ≥ 2 distinct entries; the config validation owns the
    /// refusal upstream and this constructor mirrors it so no caller
    /// can bypass validation silently.
    ///
    /// # Errors
    ///
    /// [`KioskRotationError::TooFewModes`] / [`KioskRotationError::DuplicateModes`]
    /// per their docs.
    pub fn new(
        modes: Vec<KioskMode>,
        dwell: std::time::Duration,
    ) -> Result<Self, KioskRotationError> {
        if modes.len() < 2 {
            return Err(KioskRotationError::TooFewModes);
        }
        let mut seen = std::collections::HashSet::new();
        if !modes.iter().all(|mode| seen.insert(mode)) {
            return Err(KioskRotationError::DuplicateModes);
        }
        Ok(Self {
            modes,
            dwell,
            active: 0,
        })
    }

    /// The display mode currently on the wall.
    #[must_use]
    pub fn active(&self) -> KioskMode {
        self.modes[self.active]
    }

    /// The per-mode display time (the full dwell — each flip re-arms
    /// it; no carry-over, no interaction pause).
    #[must_use]
    pub fn dwell(&self) -> std::time::Duration {
        self.dwell
    }

    /// The one transition: advance to the next mode in declared order
    /// (wrapping) and return it.
    pub fn tick(&mut self) -> KioskMode {
        self.active = (self.active + 1) % self.modes.len();
        self.active()
    }
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
    /// The task-detail due-date editor (r2 D25): typed entry plus the
    /// calendar picker; the commit funnels through the adapter's
    /// `task-due-committed` seam, never through `commit_edit`.
    EditDue { task: TaskId },
    /// The move flow: moving `task` toward `target`.
    Move { task: TaskId, target: StackId },
    /// The jump-to search palette (#348, chunk 9): the palette cursor
    /// lives in the flow (subject-free — `reconcile` leaves it alone;
    /// background refreshes re-rank through the row push instead).
    Palette { selected: usize },
    /// Quick capture (chunk 10 D8, #347): subject-free like the
    /// palette — the buffer is the thought; committing keeps the flow
    /// open (the rapid-fire braindump; `commit_edit`'s "the flow
    /// always ends" contract does not apply).
    Capture,
    // -- round 2 (D18/D21): the board/stack lifecycle flows --
    /// The new-board field (window-level slot).
    CreateBoard,
    /// The stack title editor for `stack`.
    RenameStack { stack: StackId },
    /// The board title editor for `board`.
    RenameBoard { board: BoardId },
    /// The stack-move flow: repositioning `stack` to insertion slot
    /// `slot` (`0..=len`, wrap-cycled).
    MoveStack { stack: StackId, slot: usize },
    /// The pointer drag-to-move flow (r2 chunk-8, D-B): dragging
    /// `task` toward `target` (the hovered column, seeded to the
    /// source stack). A distinct variant, *not* a `Flow::Move` reuse:
    /// the keyboard flow's arms keep byte-identical semantics, the
    /// pointer-shaped intents (set-target-by-column) have no table
    /// precedent, and the model push derives the ghost/dim/highlight
    /// from the flow. Refresh-survival is model-push by construction.
    DragMove { task: TaskId, target: StackId },
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

/// Which end of the focus domain the jump addresses (round 2, D20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusEdge {
    First,
    Last,
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

/// The one `MoveTask` construction site (r2 chunk-8 D-F): the
/// keyboard confirm and the pointer drop both build their end-of-stack
/// append here — command equivalence by construction.
fn append_move_command(state: &AppState, task: TaskId, target: StackId) -> FlowOutcome {
    FlowOutcome::Command(StateCommand::MoveTask {
        id: task,
        stack: target,
        order: append_order(state, target),
    })
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
    /// The constructor with an explicit start view (chunk 10 D2): the
    /// desktop mount starts on `Today`, the kiosk mount on `Board` —
    /// the choice is `install`'s, from `UiSettings.kiosk`; `new()`
    /// keeps the const `Board` default.
    #[must_use]
    pub const fn new_starting(view: ViewId) -> Self {
        let mut state = Self::new();
        state.view = view;
        state
    }

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
            ViewId::Today => ContextId::Today,
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
        let order = [
            ViewId::Today,
            ViewId::Board,
            ViewId::StackDetail,
            ViewId::TaskDetail,
        ];
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
            // Subjectless (chunk 10 D2): `ViewId::Board`'s arm inverted.
            ViewId::Today => {
                self.view = ViewId::Today;
                true
            }
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

    /// `view.today` (chunk 10 D3): the Today view, no engine command —
    /// the `focus_board` sibling.
    pub fn focus_today(&mut self) -> Vec<FlowOutcome> {
        self.view = ViewId::Today;
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
    pub fn advance_focus(
        &mut self,
        state: &AppState,
        compact: bool,
        kiosk: bool,
        day: &DayCtx,
    ) -> Vec<FlowOutcome> {
        self.step_focus(state, 1, compact, kiosk, day)
    }

    /// `nav.prev-item` without a move flow; retreating before the
    /// first card in compact returns to the switcher level.
    pub fn retreat_focus(
        &mut self,
        state: &AppState,
        compact: bool,
        kiosk: bool,
        day: &DayCtx,
    ) -> Vec<FlowOutcome> {
        self.step_focus(state, -1, compact, kiosk, day)
    }

    fn step_focus(
        &mut self,
        state: &AppState,
        direction: i32,
        compact: bool,
        kiosk: bool,
        day: &DayCtx,
    ) -> Vec<FlowOutcome> {
        // The Today view has no stack level: the flat section walk is
        // the domain (chunk 10 D7), even in compact.
        if self.view == ViewId::Today {
            let domain = self.today_domain(state, kiosk, day);
            let outcomes = self.step_flat_domain(direction, compact, &domain);
            self.follow_board(state);
            return outcomes;
        }
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
        let domain = self.focus_domain(state, compact);
        self.step_flat_domain(direction, compact, &domain)
    }

    /// The shared flat-walk mechanics of `step_focus` (chunk 10 D7
    /// factoring): position in `domain`, no wrap at either end, entry
    /// at the first card.
    fn step_flat_domain(
        &mut self,
        direction: i32,
        compact: bool,
        domain: &[TaskId],
    ) -> Vec<FlowOutcome> {
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

    /// The task ids the focus walkers address (round 2, D20 — factored
    /// out of `step_focus` so the first/last jump shares the exact
    /// domain choice, one behaviour one home): compact walks the
    /// effective stack's tasks, the board walks the reading order.
    fn focus_domain(&self, state: &AppState, compact: bool) -> Vec<TaskId> {
        if compact {
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
        }
    }

    /// `nav.first-item`/`nav.last-item` (round 2, D20): focus lands on
    /// the first/last card of the same domain `step_focus` walks;
    /// idempotent at the target (the jump emits the same single
    /// `RefreshNow` from anywhere, including the target itself); inert
    /// on an empty domain.
    pub fn jump_focus(
        &mut self,
        state: &AppState,
        edge: FocusEdge,
        compact: bool,
        kiosk: bool,
        day: &DayCtx,
    ) -> Vec<FlowOutcome> {
        let domain = if self.view == ViewId::Today {
            self.today_domain(state, kiosk, day)
        } else {
            self.focus_domain(state, compact)
        };
        let target = match edge {
            FocusEdge::First => domain.first(),
            FocusEdge::Last => domain.last(),
        };
        match target {
            Some(id) => {
                self.focused_task = Some(*id);
                self.follow_board(state);
                vec![FlowOutcome::RefreshNow]
            }
            None => Vec::new(),
        }
    }

    /// The today walk's domain (chunk 10 D7): the four filtered
    /// sections concatenated in display order — the same filters
    /// `push_today` applies (anchor rows out; on kiosk the focused
    /// board's subset), derived here from `(state, DayCtx)`.
    #[must_use]
    pub fn today_domain(&self, state: &AppState, kiosk: bool, day: &DayCtx) -> Vec<TaskId> {
        let sections = taskboard_domain::days::today_sections(&state.tasks, day.now, day.window);
        let focused_board = self.board;
        let keep = |task: &taskboard_domain::entities::Task| {
            !crate::presentation::is_anchor_task(task)
                && (!kiosk
                    || state
                        .stacks
                        .get(&task.stack)
                        .is_some_and(|s| focused_board.is_some_and(|b| s.board == b)))
        };
        let mut out = Vec::new();
        for id in sections
            .overdue
            .iter()
            .chain(&sections.scheduled)
            .chain(&sections.due_today)
            .chain(&sections.done_today)
        {
            if state.tasks.get(id).is_some_and(keep) {
                out.push(*id);
            }
        }
        out
    }

    /// Selection-follows-focus (chunk 10 D7, D-V3): after each today
    /// focus step the board retargets to the row's board and the
    /// stack selection to the row's stack — keeping `focused_task` (a
    /// lighter touch than `select_board`, which would reset focus and
    /// break the walk). Keeps `task.move`'s ring and detail navigation
    /// board-local. No-op when the board already matches.
    fn follow_board(&mut self, state: &AppState) {
        if self.view != ViewId::Today {
            return;
        }
        let Some(task) = self.focused_task else {
            return;
        };
        let Some(task) = state.tasks.get(&task) else {
            return;
        };
        let Some(stack) = state.stacks.get(&task.stack) else {
            return;
        };
        if self.board != Some(stack.board) {
            self.board = Some(stack.board);
        }
        if self.selected_stack != Some(task.stack) {
            self.selected_stack = Some(task.stack);
        }
    }

    /// A today row's click (chunk 10 D7): the board card's click
    /// (`activate_card`, §7.1 mouse discipline) plus the
    /// selection-follows-focus retarget — the walk's own seam, so a
    /// mouse-picked row leaves the board/stack selection consistent
    /// for the task verbs, exactly as a keyboard step would.
    pub fn activate_today_row(
        &mut self,
        state: &AppState,
        task: TaskId,
        stack: StackId,
    ) -> Vec<FlowOutcome> {
        let outcomes = self.activate_card(task, stack);
        self.follow_board(state);
        outcomes
    }

    /// `stack.next`/`stack.prev` (round 2, D17): the stack selection
    /// steps with wrap (the compact switcher's segmented-control
    /// convention) and focus lands on the new stack's first task —
    /// exactly `select_stack`. A `None` selection enters at the
    /// direction-appropriate end (the `step_focus` switcher rule);
    /// inert with zero stacks.
    pub fn step_stack(&mut self, state: &AppState, direction: i32) -> Vec<FlowOutcome> {
        self.normalize_board(state);
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
        self.select_stack(state, next)
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

    /// The due cell's click (r2 D25): begin the subject task's
    /// due-date editor in place — insert mode (the typed field's
    /// printables), no buffer seeding (an empty entry means clear).
    pub fn begin_edit_due(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Some(task) = self.subject_task.and_then(|id| state.tasks.get(&id)) else {
            return Vec::new();
        };
        self.flow = Flow::EditDue { task: task.id };
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// The due-commit funnel's end (r2 D25): the adapter dispatches
    /// the `UpdateTask` itself; ending the flow returns to normal.
    pub fn end_edit_due(&mut self) -> Vec<FlowOutcome> {
        self.flow = Flow::None;
        vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
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
            // No create row on the today view (chunk 10 D3): capture
            // is the create path there. The settings surface hosts no
            // create flow either (round-1 D5).
            ViewId::Today | ViewId::Settings => None,
            ViewId::Board => self
                .focused_stack(state)
                .or(self.selected_stack)
                .or_else(|| self.stack_ids(state).first().copied()),
            ViewId::StackDetail => self.subject_stack,
            ViewId::TaskDetail => self
                .subject_task
                .and_then(|t| state.tasks.get(&t))
                .map(|t| t.stack),
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
            // Round 2 (D18/D21): the board/stack lifecycle commits.
            Flow::CreateBoard if !title.is_empty() => {
                // The lenient empty color (the fixture convention); a
                // colour picker is recorded debt for a future round.
                outcomes.insert(
                    0,
                    FlowOutcome::Command(StateCommand::CreateBoard {
                        title,
                        color: taskboard_domain::Color::new(""),
                    }),
                );
            }
            Flow::RenameStack { stack } if !title.is_empty() => {
                outcomes.insert(
                    0,
                    FlowOutcome::Command(StateCommand::RenameStack {
                        id: stack,
                        new_title: title,
                    }),
                );
            }
            Flow::RenameBoard { board } if !title.is_empty() => {
                outcomes.insert(
                    0,
                    FlowOutcome::Command(StateCommand::RenameBoard { board, title }),
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

    /// The kiosk rotation flip (issue #363, decision D7): end any flow
    /// without committing (the `discard_edit` exit — an operator's
    /// half-open create row never commits on a tick), reset focus and
    /// the compact stack selection to the mode's canonical head, and
    /// switch the view. Pure view state; the only outcome is the
    /// repaint (the discard's `SetMode(Normal)` folds in when a flow
    /// was active). Scroll positions reset by construction — the
    /// view switch remounts the incoming view's trees at the top.
    pub fn enter_kiosk_mode(&mut self, state: &AppState, mode: KioskMode) -> Vec<FlowOutcome> {
        let mut outcomes = if self.flow == Flow::None {
            Vec::new()
        } else {
            self.discard_edit()
        };
        self.view = mode.view();
        self.focused_task = None;
        self.selected_stack = self.stack_ids(state).first().copied();
        if !outcomes.contains(&FlowOutcome::RefreshNow) {
            outcomes.push(FlowOutcome::RefreshNow);
        }
        outcomes
    }

    /// Quick capture's opener (chunk 10 D8): replaces any in-progress
    /// flow WITHOUT committing it (the overlay is the fire-drill
    /// escape; the XOR rule substitutes, it does not commit), then
    /// opens `Flow::Capture` with a cleared buffer — exactly
    /// `begin_create`'s opening.
    pub fn begin_capture(&mut self) -> Vec<FlowOutcome> {
        self.flow = Flow::Capture;
        self.buffer.clear();
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// Return on the capture buffer (chunk 10 D8): a non-empty
    /// (trimmed) thought dispatches `CaptureTask { board: None }` —
    /// always the global Inbox, never a focused board — clears the
    /// buffer, and KEEPS the flow + Insert mode (commits-and-continues).
    /// An empty buffer closes instead.
    pub fn commit_capture(&mut self) -> Vec<FlowOutcome> {
        if self.flow != Flow::Capture {
            return Vec::new();
        }
        let thought = self.buffer.trim().to_owned();
        if thought.is_empty() {
            return self.close_capture();
        }
        self.buffer.clear();
        vec![
            FlowOutcome::Command(StateCommand::CaptureTask {
                title: thought,
                board: None,
            }),
            FlowOutcome::RefreshNow,
        ]
    }

    /// Escape out of capture (chunk 10 D8): discard-and-close (the
    /// `discard_edit` shape).
    pub fn close_capture(&mut self) -> Vec<FlowOutcome> {
        if self.flow != Flow::Capture {
            return Vec::new();
        }
        self.discard_edit()
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

    /// `view.inbox` (chunk 10 D9, D-V7): jumps to the global Inbox's
    /// canonical-first stack — read-side only. Quietly no-ops (no
    /// repaint, no creation) when no live Inbox board exists: an empty
    /// inbox is triage-done, and read-write separation forbids the
    /// view from creating.
    pub fn open_inbox(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Some(board) = state.inbox_board() else {
            return Vec::new();
        };
        let outcomes = self.select_board(state, board);
        self.subject_stack = state.canonical_first_stack(board);
        self.view = ViewId::StackDetail;
        outcomes
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

    /// The move ring (chunk 10 D6, the deliberate behavior change):
    /// the *flat cross-board* candidate list — every live board's
    /// stacks in `(order, id)` order, boards in canonical `BTreeMap`
    /// order — wrapping across the board edge. No Inbox special-casing:
    /// the Inbox board's stacks appear like any board's.
    #[must_use]
    pub fn move_ring(state: &AppState) -> Vec<StackId> {
        live_boards(state)
            .into_iter()
            .flat_map(|board| stacks_in_board(state, board.id))
            .map(|stack| stack.id)
            .collect()
    }

    /// Focus next/prev during a move flow: cycle the target stack
    /// (wrap — in compact this is exactly the switcher highlighting).
    pub fn cycle_move_target(&mut self, state: &AppState, direction: i32) -> Vec<FlowOutcome> {
        let Flow::Move { target, .. } = self.flow else {
            return Vec::new();
        };
        let stacks = Self::move_ring(state);
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
        let command = append_move_command(state, task, target);
        self.flow = Flow::None;
        vec![command, FlowOutcome::RefreshNow]
    }

    // -- pointer drag-to-move (r2 chunk-8, D-B/D-F) ------------------------

    /// The pointer arm: a card drag crossing the slop starts the
    /// drag flow with `target` seeded to the task's own stack.
    /// Refuses (inert) when any other flow holds (the D9 XOR rule —
    /// the slint side additionally disables the `DragArea`s while a
    /// flow is open, belt and suspenders) or the task is unknown/not
    /// live (the reconcile path re-checks).
    pub fn begin_drag_move(&mut self, state: &AppState, task: TaskId) -> Vec<FlowOutcome> {
        if self.flow != Flow::None {
            return Vec::new();
        }
        let Some(target) = state
            .tasks
            .get(&task)
            .filter(|t| t.is_live())
            .map(|t| t.stack)
        else {
            return Vec::new();
        };
        self.flow = Flow::DragMove { task, target };
        vec![FlowOutcome::RefreshNow]
    }

    /// The pointer's column report (r2 D-E): a `DropArea`'s hover
    /// address, resolved to its stack by the adapter. `RefreshNow` only
    /// on an actual change; inert without the drag flow.
    pub fn drag_target(&mut self, _state: &AppState, stack: StackId) -> Vec<FlowOutcome> {
        let Flow::DragMove { target, .. } = &mut self.flow else {
            return Vec::new();
        };
        if *target == stack {
            return Vec::new();
        }
        *target = stack;
        vec![FlowOutcome::RefreshNow]
    }

    /// The drop (r2 D-F): the same `MoveTask` command the keyboard
    /// flow confirms — both funnel through [`append_move_command`],
    /// the one construction site (equivalence by construction; the
    /// pin test compares the commands).
    pub fn confirm_drag_move(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Flow::DragMove { task, target } = self.flow else {
            return Vec::new();
        };
        let command = append_move_command(state, task, target);
        self.flow = Flow::None;
        vec![command, FlowOutcome::RefreshNow]
    }

    /// `RequestMove` during a move flow: cancel, nothing dispatched.
    pub fn cancel_move(&mut self) -> Vec<FlowOutcome> {
        if self.flow == Flow::None {
            return Vec::new();
        }
        self.flow = Flow::None;
        vec![FlowOutcome::RefreshNow]
    }

    /// The per-kind settle rule (round-3 W-SM-b, plan §2): the one
    /// pure taxonomy every settle trigger shares — L1 focus loss, L2
    /// pointer seams, and the leaving effects — so the triggers
    /// cannot diverge. Idempotent on `Flow::None` (the
    /// double-settle guard). TEXT flows commit a trimmed non-empty
    /// buffer and discard an empty one (F7 dies); the due editor
    /// clean-closes (never commits on an unrelated settle — the r2
    /// D25 rule; the only commit path stays the `commit_due`
    /// funnel); move/drag flows cancel; the modals take their own
    /// close paths.
    pub fn settle_flow(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        match self.flow.clone() {
            Flow::None => Vec::new(),
            Flow::Create { .. }
            | Flow::CreateStack
            | Flow::CreateBoard
            | Flow::EditTitle { .. }
            | Flow::RenameStack { .. }
            | Flow::RenameBoard { .. } => {
                if self.buffer.trim().is_empty() {
                    self.discard_edit()
                } else {
                    self.commit_edit(state)
                }
            }
            Flow::EditDue { .. } => self.end_edit_due(),
            Flow::Move { .. } | Flow::MoveStack { .. } | Flow::DragMove { .. } => {
                self.cancel_move()
            }
            Flow::Palette { .. } => self.close_palette(),
            Flow::Capture => self.close_capture(),
        }
    }

    // -- board/stack lifecycle (round 2, D18/D21) --------------------------

    /// `board.create`: always available (the zero-board first-run
    /// escape — the `begin_create_stack` refusal does not apply);
    /// opens the board field in insert mode.
    pub fn begin_create_board(&mut self) -> Vec<FlowOutcome> {
        self.flow = Flow::CreateBoard;
        self.buffer.clear();
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// `stack.rename`: opens the stack title editor seeded with the
    /// current title (the `edit_subject` pattern); inert without a
    /// target (focused → selected → first).
    pub fn begin_rename_stack(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Some(stack) = self.keyboard_resize_target(state) else {
            return Vec::new();
        };
        let Some(title) = state.stacks.get(&stack).map(|s| s.title.clone()) else {
            return Vec::new();
        };
        self.flow = Flow::RenameStack { stack };
        self.buffer = title;
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// `board.rename`: opens the board title editor seeded with the
    /// current title; inert without a current board.
    pub fn begin_rename_board(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        self.normalize_board(state);
        let Some(board) = self.board else {
            return Vec::new();
        };
        let Some(title) = state.boards.get(&board).map(|b| b.title.clone()) else {
            return Vec::new();
        };
        self.flow = Flow::RenameBoard { board };
        self.buffer = title;
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// `stack.delete`: dispatches `DeleteStack` for the resolved
    /// target (focused → selected → first); the engine owns every
    /// typed refusal. Inert without a target; selection repair is
    /// `reconcile`'s on the next refresh.
    pub fn delete_stack(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Some(stack) = self.keyboard_resize_target(state) else {
            return Vec::new();
        };
        vec![
            FlowOutcome::Command(StateCommand::DeleteStack { id: stack }),
            FlowOutcome::RefreshNow,
        ]
    }

    /// `board.delete`: dispatches `DeleteBoard` for the current board
    /// (cascades; the engine's typed refusals are the only guard — no
    /// confirmation UI this round). Inert without a board.
    pub fn delete_board(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        self.normalize_board(state);
        let Some(board) = self.board else {
            return Vec::new();
        };
        vec![
            FlowOutcome::Command(StateCommand::DeleteBoard { board }),
            FlowOutcome::RefreshNow,
        ]
    }

    /// `stack.move` with no flow: the resolved stack starts moving at
    /// its current display position. Inert without a target.
    pub fn begin_move_stack(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Some(stack) = self.keyboard_resize_target(state) else {
            return Vec::new();
        };
        let stacks = self.stack_ids(state);
        let slot = stacks.iter().position(|s| *s == stack).unwrap_or(0);
        self.flow = Flow::MoveStack { stack, slot };
        vec![FlowOutcome::RefreshNow]
    }

    /// Focus next/prev during a stack move: wrap the insertion slot
    /// over `0..=len` (`len + 1` slots — `len` = append after last).
    pub fn cycle_move_stack_target(
        &mut self,
        state: &AppState,
        direction: i32,
    ) -> Vec<FlowOutcome> {
        let Flow::MoveStack { slot, .. } = self.flow else {
            return Vec::new();
        };
        let slots = self.stack_ids(state).len() + 1;
        let next = wrapped_index(slot, direction, slots);
        if let Flow::MoveStack { slot, .. } = &mut self.flow {
            *slot = next;
        }
        vec![FlowOutcome::RefreshNow]
    }

    /// `Activate` during a stack move: confirm — the re-normalization
    /// semantics (recorded). The desired final order is the current
    /// display order with the moving stack reinserted at `slot`; the
    /// minimal set of `MoveStack { id, order }` commands with target
    /// orders `0..len-1` goes out — only for stacks whose order
    /// changes. One absolute-order command can never express a dense
    /// insertion; renumbering the permutation is deterministic,
    /// idempotent, and tie-free. An in-place confirm dispatches
    /// nothing. The flow always ends.
    pub fn confirm_move_stack(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Flow::MoveStack { stack, slot } = self.flow else {
            return Vec::new();
        };
        self.flow = Flow::None;
        let mut stacks = self.stack_ids(state);
        let Some(from) = stacks.iter().position(|s| *s == stack) else {
            // The moving stack vanished mid-flow: reconcile's job on the
            // next refresh; nothing to command now.
            return vec![FlowOutcome::RefreshNow];
        };
        // `slot == len` is the append-after-last slot; only a
        // board-shrink mid-flow clamps it (never to len-1 — that would
        // turn the append into an in-place no-op).
        let slot = slot.min(stacks.len());
        let moved = stacks.remove(from);
        let insert_at = if from < slot { slot - 1 } else { slot };
        stacks.insert(insert_at.min(stacks.len()), moved);
        let mut outcomes: Vec<FlowOutcome> = stacks
            .iter()
            .enumerate()
            .filter(|(order, id)| {
                state
                    .stacks
                    .get(*id)
                    .is_some_and(|s| s.order != i64::try_from(*order).unwrap_or(i64::MAX))
            })
            .map(|(order, id)| {
                FlowOutcome::Command(StateCommand::MoveStack {
                    id: *id,
                    order: i64::try_from(order).unwrap_or(i64::MAX),
                })
            })
            .collect();
        outcomes.push(FlowOutcome::RefreshNow);
        outcomes
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
            if matches!(
                self.flow,
                Flow::EditTitle { task: t } | Flow::EditDue { task: t } if t == task
            ) {
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
            // The pointer drag (r2 chunk-8): a vanished dragged task
            // *or* target stack ends the flow — the ghost/dim/highlight
            // unpush on the next model pass (the subject-repair rule).
            Flow::DragMove { task, target } if !is_live(task) || !stacks.contains(&target) => {
                self.flow = Flow::None;
            }
            // Round 2 (D18/D21): the lifecycle flows end silently on a
            // vanished target (the `Flow::Move` precedent).
            Flow::RenameStack { stack } if !stacks.contains(&stack) => {
                self.flow = Flow::None;
                self.buffer.clear();
            }
            Flow::RenameBoard { board } if self.board != Some(board) => self.flow = Flow::None,
            Flow::MoveStack { stack, .. } if !stacks.contains(&stack) => self.flow = Flow::None,
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
    /// A fixed day bundle for the focus tests: the window is far from
    /// every fixture timestamp, so the board domains are unaffected.
    fn day() -> DayCtx {
        use chrono::TimeZone as _;
        DayCtx {
            now: Utc.timestamp_opt(1_781_481_600, 0).unwrap(),
            window: (
                Utc.timestamp_opt(1_781_481_600, 0).unwrap(),
                Utc.timestamp_opt(1_781_568_000, 0).unwrap(),
            ),
        }
    }

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

    /// A second live board with two stacks appended to `state` — the
    /// cross-board ring fixture. Returns the board and its stacks in
    /// ring order.
    fn second_board(state: &mut AppState) -> (BoardId, [StackId; 2]) {
        let board = BoardId::from_uuid(uuid::Uuid::now_v7());
        let t0 = StackId::from_uuid(uuid::Uuid::now_v7());
        let t1 = StackId::from_uuid(uuid::Uuid::now_v7());
        state.boards.insert(
            board,
            Board {
                id: board,
                remote: None,
                title: "board two".to_owned(),
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
        state.stacks.insert(t0, stack(t0, board, 0));
        state.stacks.insert(t1, stack(t1, board, 1));
        (board, [t0, t1])
    }

    // -- the move ring (chunk 10 D6) -------------------------------------

    #[test]
    fn the_move_ring_is_the_flat_cross_board_candidate_list() {
        let (mut state, _, [s0, s1], _) = fixture();
        let (_, [t0, t1]) = second_board(&mut state);
        assert_eq!(
            ViewState::move_ring(&state),
            vec![s0, s1, t0, t1],
            "boards in canonical order, each board's stacks in (order, id)"
        );
    }

    #[test]
    fn the_ring_wraps_across_the_board_edge() {
        let (mut state, _, [s0, _], [a, ..]) = fixture();
        let (_, [t0, t1]) = second_board(&mut state);
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.start_move(&state);
        // Walk to the last stack of the last board and wrap forward.
        vs.cycle_move_target(&state, 1);
        vs.cycle_move_target(&state, 1);
        let Flow::Move { target, .. } = vs.flow() else {
            panic!("flow holds");
        };
        assert_eq!(*target, t0, "the forward walk crosses the board edge");
        vs.cycle_move_target(&state, 1);
        let Flow::Move { target, .. } = vs.flow() else {
            panic!("flow holds");
        };
        assert_eq!(*target, t1, "the last board's last stack");
        vs.cycle_move_target(&state, 1);
        let Flow::Move { target, .. } = vs.flow() else {
            panic!("flow holds");
        };
        assert_eq!(
            *target, s0,
            "wrapping past the last board's last stack reaches the first"
        );
    }

    #[test]
    fn a_cross_board_move_confirms_with_the_engine_append() {
        let (mut state, _, [s0, _], [a, ..]) = fixture();
        let (_, [t0, _]) = second_board(&mut state);
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.start_move(&state);
        vs.cycle_move_target(&state, 1);
        vs.cycle_move_target(&state, 1);
        let outcomes = vs.confirm_move(&state);
        assert_eq!(
            outcomes[..1],
            [FlowOutcome::Command(StateCommand::MoveTask {
                id: a,
                stack: t0,
                order: 0,
            })],
            "cross-board targets are already legal engine-side"
        );
    }

    // -- the pointer drag flow (r2 chunk-8, D-B/D-F) ----------------------

    /// A drag journey's helper: select nothing, arm the drag directly.
    #[test]
    fn a_drag_seeds_the_target_to_the_source_stack_and_refuses_when_busy() {
        let (state, _, [s0, _], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        let outcomes = vs.begin_drag_move(&state, a);
        assert_eq!(outcomes, vec![FlowOutcome::RefreshNow], "one repaint");
        assert_eq!(
            *vs.flow(),
            Flow::DragMove {
                task: a,
                target: s0
            },
            "the target is the task's own stack"
        );

        // The D9 XOR rule: no second flow while one holds.
        let mut busy = ViewState::new();
        busy.reconcile(&state);
        busy.activate_card(a, s0);
        busy.start_move(&state);
        assert!(
            busy.begin_drag_move(&state, a).is_empty(),
            "an active flow refuses the drag arm"
        );
        assert!(
            matches!(busy.flow(), Flow::Move { .. }),
            "the refused arm left the existing flow intact"
        );

        // An unknown task refuses.
        let unknown = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut fresh = ViewState::new();
        fresh.reconcile(&state);
        assert_eq!(
            fresh.begin_drag_move(&state, unknown),
            [] as [FlowOutcome; 0]
        );
        assert_eq!(*fresh.flow(), Flow::None);
    }

    #[test]
    fn a_drag_target_reports_only_on_change_and_is_inert_without_the_flow() {
        let (state, _, [s0, s1], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_drag_move(&state, a);

        assert!(
            vs.drag_target(&state, s0).is_empty(),
            "re-reporting the current target is inert"
        );
        assert_eq!(
            vs.drag_target(&state, s1),
            vec![FlowOutcome::RefreshNow],
            "an actual change repaints"
        );
        assert_eq!(
            *vs.flow(),
            Flow::DragMove {
                task: a,
                target: s1
            }
        );

        let mut idle = ViewState::new();
        idle.reconcile(&state);
        assert!(
            idle.drag_target(&state, s1).is_empty(),
            "no drag flow: the target report is inert"
        );
    }

    /// The D-F pin: the drag drop and the keyboard confirm produce
    /// **equal `StateCommand::MoveTask`s** for the same (task → stack)
    /// transition — the commands are compared, not the outcome
    /// vectors' other effects.
    #[test]
    fn the_drag_drop_and_the_keyboard_confirm_produce_the_same_move_command() {
        let (state, _, [s0, s1], [a, ..]) = fixture();
        let keyboard = {
            let mut vs = ViewState::new();
            vs.reconcile(&state);
            vs.activate_card(a, s0);
            vs.start_move(&state);
            vs.cycle_move_target(&state, 1); // ring: s0 → s1
            vs.confirm_move(&state)
        };
        let drag = {
            let mut vs = ViewState::new();
            vs.reconcile(&state);
            vs.begin_drag_move(&state, a);
            vs.drag_target(&state, s1);
            vs.confirm_drag_move(&state)
        };
        let pick = |outcomes: &[FlowOutcome]| {
            outcomes.iter().find_map(|o| match o {
                FlowOutcome::Command(cmd @ StateCommand::MoveTask { .. }) => Some(cmd.clone()),
                _ => None,
            })
        };
        assert_eq!(
            pick(&keyboard),
            pick(&drag),
            "one MoveTask construction site"
        );
    }

    #[test]
    fn cancel_move_resets_a_drag_flow() {
        let (state, _, [_, _], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_drag_move(&state, a);
        let outcomes = vs.cancel_move();
        assert_eq!(outcomes, vec![FlowOutcome::RefreshNow]);
        assert_eq!(*vs.flow(), Flow::None);
        assert!(
            vs.confirm_drag_move(&state).is_empty(),
            "the ended flow cannot confirm afterwards"
        );
    }

    #[test]
    fn reconcile_cancels_a_drag_whose_task_or_target_vanished() {
        let (mut state, _, [_, s1], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_drag_move(&state, a);
        vs.drag_target(&state, s1);

        // The dragged task vanishes.
        let vanished = state.tasks.get_mut(&a).expect("task");
        vanished.deleted = true;
        vs.reconcile(&state);
        assert_eq!(
            *vs.flow(),
            Flow::None,
            "a vanished dragged task ends the flow"
        );

        // The target stack vanishes.
        let mut vs2 = ViewState::new();
        let mut state2 = state.clone();
        state2.tasks.get_mut(&a).expect("task").deleted = false;
        vs2.reconcile(&state2);
        vs2.begin_drag_move(&state2, a);
        vs2.drag_target(&state2, s1);
        state2.stacks.get_mut(&s1).expect("stack").deleted = true;
        vs2.reconcile(&state2);
        assert_eq!(
            *vs2.flow(),
            Flow::None,
            "a vanished target stack ends the flow"
        );
    }

    // -- the capture flow (chunk 10 D8) -----------------------------------

    #[test]
    fn capture_commits_and_continues_then_closes_on_empty() {
        let (state, _, [s0, _], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.start_move(&state);

        // Begin replaces any in-progress flow WITHOUT committing it.
        let outcomes = vs.begin_capture();
        assert_eq!(*vs.flow(), Flow::Capture);
        assert_eq!(vs.buffer(), "");
        assert!(
            outcomes
                .iter()
                .all(|o| !matches!(o, FlowOutcome::Command(_))),
            "the replaced move flow dispatches nothing"
        );
        assert!(outcomes.contains(&FlowOutcome::SetMode(Mode::Insert)));

        // Non-empty Return: the exact CaptureTask, board None, and the
        // flow KEEPS running (commits-and-continues).
        vs.set_buffer("  milk  ".to_owned());
        let outcomes = vs.commit_capture();
        assert_eq!(
            outcomes[..1],
            [FlowOutcome::Command(StateCommand::CaptureTask {
                title: "milk".to_owned(),
                board: None,
            })],
            "trimmed, always the global inbox"
        );
        assert_eq!(*vs.flow(), Flow::Capture, "the flow continues");
        assert_eq!(vs.buffer(), "", "the buffer cleared");

        // Empty Return closes (SetMode(Normal), flow ends).
        let outcomes = vs.commit_capture();
        assert_eq!(*vs.flow(), Flow::None);
        assert!(outcomes.contains(&FlowOutcome::SetMode(Mode::Normal)));
    }

    #[test]
    fn escape_discards_capture_and_reconcile_leaves_the_flow_alone() {
        let (state, _, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_capture();
        vs.set_buffer("halfway".to_owned());
        let outcomes = vs.close_capture();
        assert_eq!(*vs.flow(), Flow::None, "discard-and-close");
        assert_eq!(vs.buffer(), "");
        assert!(outcomes.contains(&FlowOutcome::SetMode(Mode::Normal)));

        // reconcile never ends the capture flow nor touches the buffer
        // (the palette precedent: subject-free).
        vs.begin_capture();
        vs.set_buffer("mid-thought".to_owned());
        vs.reconcile(&state);
        assert_eq!(*vs.flow(), Flow::Capture);
        assert_eq!(vs.buffer(), "mid-thought");
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
        assert_eq!(vs.view(), ViewId::Today, "the cycle wraps through Today");
        vs.next_view(&state);
        assert_eq!(vs.view(), ViewId::Board);
        vs.prev_view(&state);
        assert_eq!(vs.view(), ViewId::Today);
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
        vs.advance_focus(&state, false, false, &day());
        assert_eq!(
            vs.focused_task(),
            Some(a),
            "entering the walk takes the first card"
        );
        vs.advance_focus(&state, false, false, &day());
        vs.advance_focus(&state, false, false, &day());
        assert_eq!(vs.focused_task(), Some(c));
        vs.advance_focus(&state, false, false, &day());
        assert_eq!(vs.focused_task(), Some(c), "no wrap at the end");
        vs.retreat_focus(&state, false, false, &day());
        assert_eq!(vs.focused_task(), Some(b));
        vs.retreat_focus(&state, false, false, &day());
        vs.retreat_focus(&state, false, false, &day());
        assert_eq!(vs.focused_task(), Some(a), "no wrap at the start");
    }

    #[test]
    fn compact_walk_covers_the_selected_stack_and_returns_to_the_switcher() {
        let (state, _, [s0, s1], [a, b, c]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        // Entering the walk takes the first stack's first card
        // (one keypress per switch, D8).
        vs.advance_focus(&state, true, false, &day());
        assert_eq!(vs.selected_stack(), Some(s0));
        assert_eq!(vs.focused_task(), Some(a));
        vs.advance_focus(&state, true, false, &day());
        assert_eq!(vs.focused_task(), Some(b));
        vs.advance_focus(&state, true, false, &day());
        assert_eq!(vs.focused_task(), Some(b), "selected stack's end holds");
        vs.retreat_focus(&state, true, false, &day());
        vs.retreat_focus(&state, true, false, &day());
        assert_eq!(
            vs.focused_task(),
            None,
            "retreat past the first card yields to the switcher"
        );
        // Switcher level: advance moves the selection and lands on the
        // new stack's first card; selection wraps.
        vs.advance_focus(&state, true, false, &day());
        assert_eq!(vs.selected_stack(), Some(s1));
        assert_eq!(vs.focused_task(), Some(c));
        vs.retreat_focus(&state, true, false, &day());
        vs.retreat_focus(&state, true, false, &day());
        assert_eq!(
            vs.selected_stack(),
            Some(s0),
            "retreat moves the selection back"
        );
        assert_eq!(vs.focused_task(), Some(a));
        vs.retreat_focus(&state, true, false, &day());
        vs.retreat_focus(&state, true, false, &day());
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

    // -- the stack wrap-walk and the first/last jumps (r2 D17/D20) -----

    #[test]
    fn step_stack_wraps_and_lands_on_the_new_stacks_first_task() {
        let (state, _, [s0, s1], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.select_stack(&state, s1);
        // Forward wraps to the first stack; focus follows.
        let outcomes = vs.step_stack(&state, 1);
        assert_eq!(outcomes, vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.selected_stack(), Some(s0));
        assert_eq!(vs.focused_task(), Some(a));
        // Backwards wraps to the last stack from the first.
        vs.step_stack(&state, -1);
        assert_eq!(vs.selected_stack(), Some(s1));
        assert_eq!(
            vs.focused_task(),
            state.tasks_in_stack(s1).first().map(|t| t.id)
        );
        assert!(
            !outcomes
                .iter()
                .any(|o| matches!(o, FlowOutcome::Command(_))),
            "the dumb-client pin: the walk dispatches nothing"
        );
    }

    #[test]
    fn step_stack_enters_at_the_direction_end_without_a_selection() {
        let (state, _, [s0, s1], _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.step_stack(&state, 1);
        assert_eq!(vs.selected_stack(), Some(s0), "forward enters at first");
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.step_stack(&state, -1);
        assert_eq!(vs.selected_stack(), Some(s1), "backwards enters at last");
    }

    #[test]
    fn step_stack_is_inert_with_zero_stacks() {
        let mut vs = ViewState::new();
        let state = AppState::default();
        vs.reconcile(&state);
        assert_eq!(vs.step_stack(&state, 1), Vec::new());
        assert_eq!(vs.step_stack(&state, -1), Vec::new());
    }

    #[test]
    fn jump_focus_lands_on_both_ends_and_is_idempotent_at_the_target() {
        let (state, _, _, [a, b, c]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        // First from anywhere (unfocused).
        assert_eq!(
            vs.jump_focus(&state, FocusEdge::First, false, false, &day()),
            vec![FlowOutcome::RefreshNow]
        );
        assert_eq!(vs.focused_task(), Some(a));
        // Last.
        assert_eq!(
            vs.jump_focus(&state, FocusEdge::Last, false, false, &day()),
            vec![FlowOutcome::RefreshNow]
        );
        assert_eq!(vs.focused_task(), Some(c));
        // Last again: a stay — same state, one refresh.
        assert_eq!(
            vs.jump_focus(&state, FocusEdge::Last, false, false, &day()),
            vec![FlowOutcome::RefreshNow]
        );
        assert_eq!(vs.focused_task(), Some(c));
        // First again.
        vs.jump_focus(&state, FocusEdge::First, false, false, &day());
        assert_eq!(vs.focused_task(), Some(a));
        let _ = b;
    }

    #[test]
    fn jump_focus_shares_the_step_focus_domain() {
        // Compact walks the selected stack only, not the reading order.
        let (state, _, [s0, _s1], [.., c]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.select_stack(&state, s0);
        vs.jump_focus(&state, FocusEdge::Last, true, false, &day());
        assert_ne!(
            vs.focused_task(),
            Some(c),
            "compact last is the selected stack's end, not the board's"
        );
        // An empty board is inert in both edges.
        let mut vs = ViewState::new();
        let bare = AppState::default();
        vs.reconcile(&bare);
        assert_eq!(
            vs.jump_focus(&bare, FocusEdge::First, false, false, &day()),
            Vec::new()
        );
        assert_eq!(
            vs.jump_focus(&bare, FocusEdge::Last, false, false, &day()),
            Vec::new()
        );
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

    // -- the board/stack lifecycle flows (r2 D18/D21) ----------------------

    #[test]
    fn board_create_flow_commits_the_lenient_color_command() {
        let (state, _, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        let outcomes = vs.begin_create_board();
        assert_eq!(*vs.flow(), Flow::CreateBoard);
        assert_eq!(vs.buffer(), "", "the field opens empty");
        assert!(
            outcomes.contains(&FlowOutcome::SetMode(Mode::Insert)),
            "the field opens in insert mode"
        );
        vs.set_buffer("second board".to_owned());
        assert_eq!(
            vs.commit_edit(&state)[..1],
            [FlowOutcome::Command(StateCommand::CreateBoard {
                title: "second board".to_owned(),
                color: taskboard_domain::Color::new(""),
            })][..]
        );
        assert_eq!(*vs.flow(), Flow::None);
    }

    #[test]
    fn rename_stack_flow_seeds_the_buffer_and_commits_the_rename() {
        let (state, _, [s0, _], _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_rename_stack(&state);
        assert!(matches!(vs.flow(), Flow::RenameStack { stack } if *stack == s0));
        assert_eq!(vs.buffer(), "stack-0", "seeded from the current title");
        vs.set_buffer("doing".to_owned());
        assert_eq!(
            vs.commit_edit(&state)[..1],
            [FlowOutcome::Command(StateCommand::RenameStack {
                id: s0,
                new_title: "doing".to_owned(),
            })][..]
        );
    }

    #[test]
    fn rename_board_flow_seeds_the_buffer_and_commits_the_rename() {
        let (state, board_id, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_rename_board(&state);
        assert!(matches!(vs.flow(), Flow::RenameBoard { board } if *board == board_id));
        assert_eq!(vs.buffer(), "board");
        vs.set_buffer("renamed".to_owned());
        assert_eq!(
            vs.commit_edit(&state)[..1],
            [FlowOutcome::Command(StateCommand::RenameBoard {
                board: board_id,
                title: "renamed".to_owned(),
            })][..]
        );
    }

    #[test]
    fn delete_stack_and_delete_board_dispatch_the_exact_commands() {
        let (state, board_id, [s0, _], _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        assert_eq!(
            vs.delete_stack(&state)[..1],
            [FlowOutcome::Command(StateCommand::DeleteStack { id: s0 })][..]
        );
        assert_eq!(
            vs.delete_board(&state)[..1],
            [FlowOutcome::Command(StateCommand::DeleteBoard {
                board: board_id
            })][..]
        );
    }

    #[test]
    fn delete_stack_is_inert_without_a_target() {
        let mut vs = ViewState::new();
        let bare = AppState::default();
        vs.reconcile(&bare);
        assert_eq!(vs.delete_stack(&bare), Vec::new());
        assert_eq!(vs.delete_board(&bare), Vec::new());
        assert_eq!(vs.begin_rename_stack(&bare), Vec::new());
        assert_eq!(vs.begin_move_stack(&bare), Vec::new());
    }

    #[test]
    fn the_stack_move_flow_begins_cycles_wrapped_and_confirms_minimally() {
        // A wider board: [A, B, C, D] — move B after C → B:2, C:1.
        let board_id = BoardId::from_uuid(uuid::Uuid::from_u128(0x4001));
        let sa = StackId::from_uuid(uuid::Uuid::from_u128(0x5001));
        let sb = StackId::from_uuid(uuid::Uuid::from_u128(0x5002));
        let sc = StackId::from_uuid(uuid::Uuid::from_u128(0x5003));
        let sd = StackId::from_uuid(uuid::Uuid::from_u128(0x5004));
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
        for (id, order) in [(sa, 0), (sb, 1), (sc, 2), (sd, 3)] {
            state.stacks.insert(id, stack(id, board_id, order));
        }
        let mut vs = ViewState::new();
        vs.reconcile(&state);

        vs.select_stack(&state, sb);
        vs.begin_move_stack(&state);
        assert!(
            matches!(vs.flow(), Flow::MoveStack { stack, slot } if *stack == sb && *slot == 1),
            "begins at the current slot"
        );
        // Cycle wraps over 0..=4 (len + 1 slots).
        vs.cycle_move_stack_target(&state, -1);
        vs.cycle_move_stack_target(&state, -1);
        assert!(
            matches!(vs.flow(), Flow::MoveStack { slot: 4, .. }),
            "wraps backwards"
        );
        vs.cycle_move_stack_target(&state, 1);
        assert!(matches!(vs.flow(), Flow::MoveStack { slot: 0, .. }));
        vs.cycle_move_stack_target(&state, 1);
        vs.cycle_move_stack_target(&state, 1);
        vs.cycle_move_stack_target(&state, 1);
        vs.cycle_move_stack_target(&state, 1);
        vs.cycle_move_stack_target(&state, 1);
        assert!(
            matches!(vs.flow(), Flow::MoveStack { slot: 0, .. }),
            "wraps forwards"
        );

        // Move B (slot 1) after C (insertion slot 3): B:2, C:1.
        vs.cycle_move_stack_target(&state, 1);
        vs.cycle_move_stack_target(&state, 1);
        vs.cycle_move_stack_target(&state, 1); // slot 3
        let outcomes = vs.confirm_move_stack(&state);
        let commands: Vec<_> = outcomes
            .iter()
            .filter_map(|o| match o {
                FlowOutcome::Command(c) => Some(c.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            commands,
            vec![
                // Desired-position order: [A, C, B, D] — C lands at 1
                // (was 2), B at 2 (was 1); A and D keep their orders.
                StateCommand::MoveStack { id: sc, order: 1 },
                StateCommand::MoveStack { id: sb, order: 2 },
            ],
            "the minimal re-normalization set, in desired order"
        );
        assert_eq!(*vs.flow(), Flow::None);
        assert!(
            matches!(outcomes.last(), Some(FlowOutcome::RefreshNow)),
            "the flow's end repaints"
        );
    }

    #[test]
    fn an_in_place_stack_move_confirm_dispatches_nothing() {
        let (state, _, [_s0, _], _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_move_stack(&state);
        let outcomes = vs.confirm_move_stack(&state);
        assert!(
            !outcomes
                .iter()
                .any(|o| matches!(o, FlowOutcome::Command(_))),
            "slot 0 for the first stack is a no-op"
        );
        assert_eq!(*vs.flow(), Flow::None);
    }

    #[test]
    fn the_stack_move_flow_cancels_and_reconciles_on_vanish() {
        let (state, _, [s0, s1], _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_move_stack(&state);
        let outcomes = vs.cancel_move();
        assert_eq!(outcomes, vec![FlowOutcome::RefreshNow]);
        assert_eq!(*vs.flow(), Flow::None);

        vs.begin_move_stack(&state);
        let mut gone = state.clone();
        gone.stacks.get_mut(&s0).expect("stack").deleted = true;
        vs.reconcile(&gone);
        assert_eq!(
            *vs.flow(),
            Flow::None,
            "a vanished moving stack ends the flow"
        );

        // A rename flow ends the same way.
        vs.begin_rename_stack(&state);
        let mut renamed_away = state.clone();
        renamed_away.stacks.get_mut(&s0).expect("stack").deleted = true;
        vs.reconcile(&renamed_away);
        assert_eq!(*vs.flow(), Flow::None);
        let _ = s1;
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

    // -- the kiosk rotation (K13, issue #363) ---------------------------------

    #[test]
    fn kiosk_rotation_mints_only_distinct_mode_pairs() {
        let dwell = std::time::Duration::from_secs(30);
        assert_eq!(
            KioskRotation::new(vec![KioskMode::Board], dwell),
            Err(KioskRotationError::TooFewModes),
            "a one-mode rotation is a no-op wearing a config"
        );
        assert_eq!(
            KioskRotation::new(vec![KioskMode::Board, KioskMode::Board], dwell),
            Err(KioskRotationError::DuplicateModes),
            "a duplicate splits the cycle pointlessly"
        );
        let rotation = KioskRotation::new(vec![KioskMode::Board, KioskMode::Today], dwell)
            .expect("two distinct modes mint");
        assert_eq!(rotation.dwell(), dwell);
    }

    #[test]
    fn kiosk_rotation_ticks_in_declared_order_and_wraps() {
        let mut rotation = KioskRotation::new(
            vec![KioskMode::Today, KioskMode::Board],
            std::time::Duration::from_secs(30),
        )
        .expect("two distinct modes mint");
        assert_eq!(
            rotation.active(),
            KioskMode::Today,
            "the declared order is the rotation order, not a hardcoded one"
        );
        assert_eq!(rotation.tick(), KioskMode::Board);
        assert_eq!(rotation.active(), KioskMode::Board);
        assert_eq!(rotation.tick(), KioskMode::Today, "the cycle wraps");
    }

    #[test]
    fn enter_kiosk_mode_resets_to_the_mode_head() {
        let (state, _, [s0, _], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.begin_create(s0);
        assert_eq!(vs.buffer(), "", "the create flow starts on an empty buffer");
        vs.set_buffer("half-typed".to_owned());

        let outcomes = vs.enter_kiosk_mode(&state, KioskMode::Today);
        assert_eq!(vs.view(), ViewId::Today);
        assert_eq!(*vs.flow(), Flow::None, "the flow ends without committing");
        assert_eq!(vs.buffer(), "", "the buffer is discarded, never committed");
        assert_eq!(vs.focused_task(), None);
        assert_eq!(
            vs.selected_stack(),
            Some(s0),
            "the stack selection lands on the board's first live stack"
        );
        assert_eq!(
            outcomes,
            vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow],
            "the non-committing exit's outcomes fold into the flip"
        );

        // A flip with no flow active carries no SetMode (already Normal).
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        let outcomes = vs.enter_kiosk_mode(&state, KioskMode::Board);
        assert_eq!(outcomes, vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.view(), ViewId::Board);
    }

    #[test]
    fn rotation_transition_never_produces_an_engine_command() {
        let (state, board, [s0, _], [a, ..]) = fixture();
        let flows = [
            Flow::None,
            Flow::Create { stack: s0 },
            Flow::CreateStack,
            Flow::EditTitle { task: a },
            Flow::EditDue { task: a },
            Flow::Move {
                task: a,
                target: s0,
            },
            Flow::Palette { selected: 0 },
            Flow::Capture,
            Flow::CreateBoard,
            Flow::RenameStack { stack: s0 },
            Flow::RenameBoard { board },
            Flow::MoveStack { stack: s0, slot: 0 },
            Flow::DragMove {
                task: a,
                target: s0,
            },
        ];
        for flow in flows {
            for mode in [KioskMode::Board, KioskMode::Today] {
                let mut vs = ViewState::new();
                vs.reconcile(&state);
                vs.flow = flow.clone();
                vs.buffer = "in-flight".to_owned();
                let outcomes = vs.enter_kiosk_mode(&state, mode);
                assert!(
                    !outcomes
                        .iter()
                        .any(|outcome| matches!(outcome, FlowOutcome::Command(_))),
                    "rotation must never dispatch into the engine (flow {flow:?}, mode {mode:?})"
                );
                assert!(
                    outcomes
                        .iter()
                        .any(|o| matches!(o, FlowOutcome::RefreshNow)),
                    "the flip always repaints"
                );
            }
        }
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

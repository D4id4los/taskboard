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

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, Utc};
use taskboard_domain::{
    AppState, BoardId, ReviewCadence, SearchTarget, StackId, StateCommand, TaskChanges, TaskId,
};

use crate::actions::ContextId;
use crate::presentation::{live_board, live_boards, stacks_in_board};
use crate::router::Mode;

/// One row of the today walk's domain (chunk 8 D2, #934): the walk
/// widens from tasks-only to the task sections **then** the active
/// board's live habits in stored `(order, id)` order — the same
/// order `IdTables.habits` keys, so the walk's landing and the
/// section's rows agree by construction. Habit rows are today-only
/// (census R-1): no other view's domain carries them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodayRow {
    Task(TaskId),
    Habit(taskboard_domain::HabitId),
    /// The break-rules roster's row (US-9, W-10, #1066's roster half):
    /// the `TimersSection`'s roster adopts the K4 selected-row cursor —
    /// the walk's landing arms it, the roster id table clamps/prunes
    /// it. Today-only, desktop-only (the section's kiosk absence).
    BreakRule(taskboard_domain::BreakRuleId),
}

/// The focus walk's landing triple (A5a D-2, #1302): the subjects the
/// scroll hint's guard compares. A landing that changes none of the
/// three re-arms nothing — the ends-hold law, widened from the
/// task-only `focused_task` capture to every walkable today row kind
/// (task / habit / break-rule roster). The fields are mutually
/// exclusive while the walk sits on a today row (the stepper's
/// exclusivity law); a foreign-surface task focus clears the twins.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WalkFocus {
    pub task: Option<TaskId>,
    pub habit: Option<taskboard_domain::HabitId>,
    pub roster: Option<taskboard_domain::BreakRuleId>,
    /// The planner's in-pane cursor task (#1320): the fourth walkable
    /// row kind — the pane walk's landing resolves the scroll hint's
    /// `(pane, row)` through `IdTables::plan_position`. `None` while
    /// the walk sits on any other surface (the planner cursor is
    /// cleared on view leave, the clamp/prune law).
    pub plan: Option<TaskId>,
}

/// The pass's time bundle (chunk 10 D4): the fresh `now` and the
/// `day_window` pair the adapter derives from its held
/// `(day_start_time, offset)` custody — time is data, recomputed on
/// every refresh pass (the rollover rides the existing repaint driver;
/// no new timer, D-V4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayCtx {
    pub now: DateTime<Utc>,
    pub window: (DateTime<Utc>, DateTime<Utc>),
    /// The day calendar the window was derived with (the aggregated
    /// today walk's habit rows read the window date through it;
    /// round-6 A1 D-2, 2026-11-24).
    pub calendar: taskboard_domain::DayCalendar,
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
    /// The review/statistics surface (#214, chunk 6.1): the
    /// estimate-vs-actual calibration reading — subjectless, read-only
    /// (a ledger, not a task list). A cycle member like Today;
    /// `KioskMode` maps only Board/Today, so the appliance rotation
    /// can never select it.
    Review,
    /// The settings surface (round-1 D5/D24b): reachable only through
    /// `open_settings` — never a member of the `step_view` cycle.
    Settings,
    /// The plan-next-day surface (#622, D-1): reachable only through
    /// `open_plan_next_day` — never a member of the `step_view`
    /// cycle (the Settings precedent; the wave's only interactive
    /// editing workspace must not become a casual cycle stop).
    PlanNextDay,
    /// The status panel (OP3 W-1, journey plan §2 R-1: the fixed
    /// surface set's app-scoped home, the Settings precedent):
    /// reachable only through `open_status_panel` — never a member of
    /// the `step_view` cycle. Read-only (R-3): no engine command is
    /// reachable from it.
    Status,
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

/// Which date field a [`Flow::EditDate`] edits (round-3 D8): the
/// enum-with-data A1 shape — one flow variant, one commit funnel, the
/// field read only where the `TaskChanges` mask differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateField {
    Due,
    Scheduled,
}

/// The review walk's stage machine (DR-7 D-J9; J4 D-1): a six-stage
/// guided journey — Entry → Inbox → Carry forward → (Planner, opt-in)
/// → Prompts → Completion summary — inside one modal. The plan-tomorrow
/// leg is an **embedded step** (J4 R-1/O-3): the planner mounts as the
/// walk's own child level (D-2), never a close-reopen handoff. The
/// stages carry no sub-state: the armed row affordances (the File-to
/// picker, the Pick-a-date field) are the walk's child levels (chunk
/// 3's ports); the Escape ladder's sub-state rung is the generic
/// Cancelled rung, then abort-with-confirm (J4 R-2), never a silent
/// step-back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkStage {
    /// The guidance stage (J4 D-1): header + stage-intro + the strip,
    /// advanced by the single Next control; never skipped, always the
    /// open stage.
    Entry,
    /// Inbox triage (P1): the armed File-to picker is the
    /// `WalkFilePick` child level (chunk 3's N3a port); the stage
    /// carries no sub-state.
    Inbox,
    /// Carry forward (P9): the armed Pick-a-date field is the
    /// `WalkDatePick` child level (chunk 3's N3b port); the stage
    /// carries no sub-state. Its Next is the embedded planner *choice*
    /// (the verb), never an auto-advance — an empty Carry skips
    /// straight to Prompts (D-3).
    CarryForward,
    /// The embedded planner step (J4 US-3): live only while the
    /// `ReviewPlanner` child level (D-2) is mounted; leaving the child
    /// advances to Prompts (done) or returns to Carry (back).
    Planner,
    /// The prompted entry (P7/P13) — the cadence template's fields
    /// over one `RecordReflection`. Record/Finish both advance to the
    /// completion summary (D-1).
    Prompts,
    /// The terminal completion summary (D-8): presents the session
    /// tally; its single exit closes the walk and restores focus.
    Complete,
}

/// The session tally (J4 D-8): the counts the completion summary
/// presents. They ride the flow — they are *not* derivable from
/// `AppState` (the walk's commands are honest writes; the summary is
/// what *this session* did).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WalkTally {
    /// Inbox rows filed via the File-to picker.
    pub filed: usize,
    /// Carry rows moved to tomorrow (the verb or the picked date).
    pub moved: usize,
    /// Rows left as they were (`Keep in inbox` / `Leave as is`).
    pub left: usize,
    /// A non-blank reflection was recorded this session.
    pub recorded: bool,
    /// The embedded planner step was visited this session (the
    /// stage-position counter counts it only then — D-9).
    pub planner: bool,
}

/// The first open stage at/after `from` under `rows` (D-3's skip law,
/// one function): an empty Inbox skips to Carry, an empty Carry skips
/// to Prompts, both empty land on Prompts — a stage can never
/// *render* empty. Non-row stages pass through unchanged (Entry is
/// never skipped; the planner is opt-in, never a skip destination).
#[must_use]
pub const fn walk_first_open_stage(from: WalkStage, rows: WalkRows) -> WalkStage {
    if matches!(from, WalkStage::Inbox) && rows.inbox == 0 {
        if rows.carry == 0 {
            return WalkStage::Prompts;
        }
        return WalkStage::CarryForward;
    }
    if matches!(from, WalkStage::CarryForward) && rows.carry == 0 {
        return WalkStage::Prompts;
    }
    from
}

/// The walk's per-stage row counts as the pass pushed them (the view
/// state holds only the cursor; every row set is re-derived from
/// `AppState` on every push, so the counts arrive as data — the
/// palette's `visible` argument generalized).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WalkRows {
    pub inbox: usize,
    pub carry: usize,
}

/// What a walk step asked of the adapter (J4 D-3/D-6): the stage moved
/// on, the embedded planner choice landed (the `ReviewPlanner` child
/// is mounted), or nothing happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkStep {
    /// The stage changed (or the cursor moved within it).
    Advanced,
    /// The planner choice landed: the `ReviewPlanner` child level is
    /// live above the walk (D-2) — a repaint mounts the embedded body.
    Planner,
    /// Nothing to advance to (no walk, or a terminal/exit-driven
    /// stage).
    Inert,
}

/// One tally seam's note (D-8): which summary counter a verb seam
/// advances.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkTallyNote {
    /// An inbox row was filed into a stack.
    Filed,
    /// A carry row was moved to tomorrow (verb or picked date).
    Moved,
    /// A row was left as it was.
    Left,
    /// A non-blank reflection was recorded.
    Recorded,
}

/// The active flow (D9): at most one — edit (create row or title) XOR
/// move. `refresh` never touches text properties while a flow holds
/// the buffers (D9 rule 4).
/// The edit flows' widened subject vocabulary (O-2, W-14, #1048):
/// the shared title/description editors mount over a task OR a habit —
/// one flow grammar, two subjects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditSubject {
    /// The task subject (the landed grammar).
    Task(TaskId),
    /// The habit subject (the editor variant's shared flows).
    Habit(taskboard_domain::HabitId),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Flow {
    #[default]
    None,
    /// The inline create row, appending to `stack`.
    Create { stack: StackId },
    /// The inline create row for a new stack on the live board
    /// (#304): needs no target stack, so it works on an empty board.
    CreateStack,
    /// The detail title editor (W-14, #1048: the O-2 widened
    /// subject — the shared editor mounts over a task OR a habit;
    /// task behavior is the regression oracle).
    EditTitle { subject: EditSubject },
    /// The task-detail date editor (r2 D25, round-3 D8): typed entry
    /// plus the calendar picker over one of the task's two date
    /// fields — due or scheduled start — through the *one* commit
    /// funnel (the adapter's `commit_date`), never `commit_edit`.
    /// Which field rides the flow; every flow rule is field-blind.
    EditDate { task: TaskId, field: DateField },
    /// The task-detail recurrence-rule editor (round-7 D-7.1): the
    /// window-level overlay's flow — the draft buffer lives in the
    /// level's `RecurEditorState` (the `EditDate` rule: identity
    /// rides the flow, text rides the level), seeded by the opener
    /// seam, committed through the adapter's funnel over
    /// `assemble_rule`. Ends clean on cancel/Escape (nothing
    /// dispatches); dropped by `reconcile` when the subject dies.
    EditRecur { task: TaskId },
    /// The task-detail description editor (round-3 D15/#228): the
    /// shared `buffer` (the title's), committed verbatim — an empty
    /// buffer commits `Some("")`, which is the clear (the
    /// `TaskChanges.description` mask's documented shape). The
    /// commit funnel is [`Self::commit_description`]; the settle rule
    /// is the text flows' (commit non-empty / discard empty — no
    /// accidental clears on focus loss).
    EditDescription { subject: EditSubject },
    /// The move flow: moving `task` toward `target` at insertion slot
    /// `slot` (UX-R8 D-1, #823): the gap index into
    /// `tasks_in_stack(target)` — `0..=len` (gap `i` = before task
    /// `i`, gap `len` = after the last). One flow carries both aim
    /// coordinates; the confirm mints through [`move_aim_command`]
    /// (B3 2026-11-24 #1247: a same-stack gap is the engine-side
    /// reorder's index, never a presentation-side order value).
    Move {
        task: TaskId,
        target: StackId,
        slot: usize,
    },
    /// The jump-to search palette (#348, chunk 9): the palette cursor
    /// lives in the flow (subject-free — `reconcile` leaves it alone;
    /// background refreshes re-rank through the row push instead).
    Palette { selected: usize },
    /// Quick capture (chunk 10 D8, #347): subject-free like the
    /// palette — the buffer is the thought; committing keeps the flow
    /// open (the rapid-fire braindump; `commit_edit`'s "the flow
    /// always ends" contract does not apply).
    Capture,
    /// The concentration overlay (#496): subject-free like the palette
    /// — `reconcile` leaves it alone (no subject to repair); the
    /// switcher's cursor lives in the flow. Flows are XOR: opening it
    /// replaces any other flow (the workspace suspends — that is the
    /// point of concentration mode), and any other flow open replaces
    /// it back. Closing the overlay never stops a running session
    /// (the session is the open entry; the overlay is presentation).
    FocusMode { selected: usize },
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
    /// The time-entries overlay's list surface (r4 chunk 10, D-TS2):
    /// subject-affine — the mount is gated on the subject matching,
    /// and `reconcile` drops the flow when the view leaves the task
    /// detail or the subject dies (the overlay cannot outlive its
    /// view).
    TrackEntries { task: TaskId },
    /// The overlay's add (`seed: None`) / edit (`Some`) form surface,
    /// the list's child level (chunk 3's N1 port; the sibling
    /// replace retired).
    TrackEntryForm {
        task: TaskId,
        seed: Option<taskboard_domain::TimeEntryId>,
    },
    /// The pointer drag-to-move flow (r2 chunk-8, D-B): dragging
    /// `task` toward `target` (the hovered column, seeded to the
    /// source stack). A distinct variant, *not* a `Flow::Move` reuse:
    /// the keyboard flow's arms keep byte-identical semantics, the
    /// pointer-shaped intents (set-target-by-column) have no table
    /// precedent, and the model push derives the ghost/dim/highlight
    /// from the flow. Refresh-survival is model-push by construction.
    /// UX-R8 D-2 adds `gap`: the aimed insertion slot — `Some(gap)`
    /// from a gap-zone report, `None` = column-only aim (the legacy
    /// column seams; the drop then appends).
    DragMove {
        task: TaskId,
        target: StackId,
        gap: Option<usize>,
    },
    /// The plan-next-day placement form (#622, D-3): subject-affine
    /// like the entries form (the third tenant of that pattern) —
    /// `reconcile` drops it when the subject dies, but a view change
    /// does *not* drop it (the mount gate keys on the subject, not
    /// the view). The time/effort text lives adapter-side (the due
    /// buffers' lifecycle); the flow only carries the addressed box.
    PlanPlace { task: TaskId },
    /// The review walk (DR-7 D-J8/D-J9, #638): a subjectless XOR flow
    /// like the palette/capture/focus overlay — `reconcile` leaves it
    /// alone, any other flow replaces it without committing, opening
    /// it replaces any other flow. `cadence` + `reference` name the
    /// reviewed window, re-derived on every read as
    /// `review_window(cadence, reference, calendar)` (plan D-W1: the
    /// reference date pins the window against a wall-clock boundary
    /// crossing mid-walk — windows are data, never stored instants);
    /// `stage` is the machine, `cursor` the stage's row index (the
    /// palette-cursor-in-flow precedent; clamped against the pushed
    /// row count on refresh). Cadence-generic by construction: only
    /// the openers pin `Day` in v0.2 (#640 widens the openers).
    ReviewWalk {
        cadence: ReviewCadence,
        reference: NaiveDate,
        stage: WalkStage,
        cursor: usize,
        /// The session tally (D-8): rides the flow, presented at the
        /// completion stage.
        tally: WalkTally,
    },
    /// The embedded planner child level (J4 D-2, #716's integration
    /// contract): pushed **above the live walk** while
    /// [`WalkStage::Planner`] — the planner appears as a step in the
    /// same dialog, never a close-reopen handoff. A flow level, not a
    /// pushed view (the D-2 primary considered
    /// `Surface::View(PlanNextDay)`; that would flip `top_view()` and
    /// mount the full-screen planner over the walk dialog — the
    /// minting-mechanics swap the plan pre-declared as the fallback).
    /// Its exit contract rides `on_child_exit`: `Cancelled` (back)
    /// returns the walk to Carry, `Completed(None)` (done) advances
    /// to Prompts. Placement writes are plain `UpdateTask`s in the
    /// published snapshot.
    ReviewPlanner,
    /// The entries form's date grid as a child level (W5b N2, chunk
    /// 3): pushed only by the form's own toggle seam, ended only
    /// through the exit protocol. The grid/cursor machinery lives in
    /// the level's `Picker` family; the completed payload is the
    /// picked day (`LevelResult::Date`), cancellation is the generic
    /// `Cancelled` rung.
    EntryDatePicker,
    /// The walk's File-to picker as a child level (W5b N3a, chunk 3):
    /// pushed by the walk's own `File to…` intent with the first
    /// candidate armed (`None` remains only for the candidate-death
    /// clamp's same-pass window); the completed payload is the armed
    /// target stack. The candidate ring's cursor is the variant data
    /// (D-H2: pure interaction state rides the variant).
    WalkFilePick { armed: Option<StackId> },
    /// The walk's Pick-a-date field as a child level (W5b N3b, chunk
    /// 3): the typed text lives in the level's `WalkDate` family; the
    /// completed payload is the parsed instant (minted adapter-side —
    /// the child's commit parses its own text). A foreign settle ends
    /// it `Cancelled`, discarding half-typed text exactly as the
    /// flattened walk always did (the Q3 ruling, D5).
    WalkDatePick,
    /// The armed-delete confirm strip (UX R-2 D-2, #784): the board/
    /// stack delete verbs arm this flow first — nothing dispatches
    /// until the subject's stroke repeats or the strip's Confirm
    /// routes the exact verb (the stack-move two-step grammar; the
    /// strip is its pointer twin). The shared grammar's escape paths
    /// are the flow machinery's for free: a foreign settle disarms
    /// (the settle rule), Escape resolves ahead of the resolver
    /// (`CancelOnly`), and the subject's vanish ends the flow
    /// (reconcile's rename-vanish pattern). One armed subject: arming
    /// a different subject re-arms.
    ConfirmDelete(DeleteTarget),
    /// The board color picker (UX R-4 D-1, #791): armed-with-no-buffer
    /// over the selected board — the `ConfirmDelete` shape minus the
    /// confirm stage (the verb is self-reversing, so no armed strip
    /// applies). The swatch strip mounts from the flow; a swatch
    /// click dispatches the exact `SetBoardColor` and ends the flow;
    /// Escape / outside click / a foreign settle disarm with nothing
    /// dispatched (the settle law verbatim). Arming a different
    /// board re-arms; the board's death ends the flow (reconcile).
    BoardColorPick { board: BoardId },
    // -- the board ceremony flows (O-5 parity chunk OP1, US-1/US-2/
    // US-3, #945/#946/#947) ------------------------------------------
    //
    // The armed consent strips for the three GUI board-ceremony verbs.
    // The r1 erratum law holds verbatim: `SetBoardArchived`,
    // `ResetBoard`, and `ReconcileByTitle` stay
    // `NotUndoable(BoardCeremony)` in the engine — these flows are
    // *informed-consent confirmations*, not undo machinery; the typed
    // escapes are the verbs' own reversal paths (re-archive for
    // archive, the re-pull for reset, idempotent re-runs for
    // reconcile), never `AppUndo`. The shared grammar is the
    // `ConfirmDelete` shape (one armed subject surface-wide, re-arm
    // swaps, Escape → disarm, a foreign settle disarms, the subject's
    // death ends the flow); the letters ride the flow as typed
    // derivation inputs, never hand-written prose.
    //
    /// The archive consent caption (OP1 US-1/S2–S3, D-1): one
    /// context-menu row with a consent caption — the verb is
    /// self-reversing (the archived roster's restore row is the typed
    /// escape), so the letter is the caption fact alone, no counts
    /// strip. Confirm routes the exact `SetBoardArchived { true }`.
    ConfirmBoardArchive { board: BoardId },
    /// The armed reset strip (OP1 US-2, D-3): the consequence letter
    /// derives its counts from the live model at arm/refresh time
    /// (the board's stack/task/label rows) — no engine query. Confirm
    /// routes the exact `ResetBoard { board }`; the board row and
    /// connect session survive (the letter's re-materialization fact).
    ConfirmBoardReset { board: BoardId },
    /// The armed reconcile strip (OP1 US-3, D-4): the preview letter's
    /// counts are planner-derived facts (the same-title merge pairs
    /// the presentation model does not carry), so they arrive through
    /// the oneshot `UiRequest::BoardReconcilePreview` reply and ride
    /// the flow (the `WalkFilePick` precedent: pure interaction state
    /// rides the variant). `Pending` mounts the strip before the
    /// counts reply lands; `Ready` carries the counts (a zero-count
    /// preview is the letter's "says so" — the confirm routes the
    /// idempotent verb anyway); `Failed` is the typed dead-lane fact
    /// (the disarm follows). Confirm routes the exact
    /// `ReconcileByTitle { board }`.
    ConfirmBoardReconcile {
        board: BoardId,
        preview: ReconcilePreviewState,
    },
    /// The labels overlay (UX R-3 D-2, #789): the task-detail's
    /// modal over the subject task — the entries overlay's pattern
    /// (subject-affine mount; `reconcile` drops it when the view
    /// leaves the task detail or the subject dies). Create/edit/
    /// delete live in its rows; assign/unassign are one click from
    /// the detail chips. Tag-color plan D-3 widens the payload to a
    /// subject enum — one flow, two mounts (the roster-only board
    /// mount carries no assignment toggles).
    Labels { subject: LabelOverlaySubject },
}

/// The labels overlay's subject (tag-color plan D-3, US-2): the
/// detail view's task (the assignment-toggles mount) or a board
/// surface's roster-only mount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelOverlaySubject {
    /// The detail mount over the viewed task.
    Task(TaskId),
    /// The roster-only mount over the selected board (no toggles —
    /// the tag-color plan R-2's letter made visible).
    Board(BoardId),
}

/// The armed delete's subject (UX R-2 D-2, #784): the resolved target
/// the confirmation names — one armed subject per the shared grammar,
/// re-arming swaps to the new subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteTarget {
    Stack(StackId),
    Board(BoardId),
    /// The armed multi delete (UX-R8 D-6, US-4/AC-3): the selection's
    /// anchor-order snapshot at arm time; a member dying mid-arm
    /// prunes via `reconcile` (empty → disarm), and the confirm
    /// dispatches one `DeleteTask` per member (each a journal
    /// `PreImage` — `AppUndo` reverses stepwise, engine truth).
    Tasks(Vec<TaskId>),
}

/// The reconcile preview letter's typed state (OP1 D-4): the
/// oneshot reply's lifecycle as flow data — never prose. The fold
/// tests assert this enum, never letter text (the round law).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcilePreviewState {
    /// The preview request is in flight; the strip is mounted, the
    /// counts are not in yet.
    Pending,
    /// The reply landed: the planner's match counts (the letter's
    /// derivation inputs — no prose rides the wire).
    Ready(taskboard_domain::ReconcileCounts),
    /// The reply lane died (the app half is gone): the typed failure
    /// fact; the disarm follows on the drain.
    Failed,
}

/// The board selection (UX-R8 D-4, US-4/S1, W-4): adapter-owned
/// presentation state — never synced, never journaled (R-2); it is
/// presentation state like `focused_task`. Empty = `anchor.is_none()`
/// (the `BTreeSet` follows the anchor — the anchor is the invariant
/// carrier); display order is derived over board reading order at use
/// time, never stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoardSelection {
    anchor: Option<TaskId>,
    members: BTreeSet<TaskId>,
}

impl BoardSelection {
    /// Empty selection: no anchor (and by the carrier invariant, no
    /// members).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.anchor.is_none()
    }

    /// The range origin.
    #[must_use]
    pub const fn anchor(&self) -> Option<TaskId> {
        self.anchor
    }

    /// Membership (the render fact's input).
    #[must_use]
    pub fn contains(&self, task: &TaskId) -> bool {
        self.members.contains(task)
    }

    fn reset(&mut self, task: TaskId) {
        self.anchor = Some(task);
        self.members.clear();
        self.members.insert(task);
    }
}

/// What a view-state transition asks the adapter to do (D9):
/// dispatch a command, flip the vi mode, or repaint now. The adapter
/// applies outcomes; focus is mount-driven (module docs).
#[allow(clippy::large_enum_variant)]
// StateCommand just crossed the threshold (pause rule); boxing churns every match site for a local lint
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

/// The keyboard step's inputs (r4 D8): the resolved board row plus
/// the scaled step and the conversion scale — the adapter gathers them
/// from the window (an input outside the view state, §1.1's one
/// conversion rule).
pub struct RowStep<'a> {
    /// The tables-order stack ids.
    pub stacks: &'a [StackId],
    /// The resolved rendered widths (Σ == the row width).
    pub widths: &'a [u32],
    /// The content minimums, rendered px.
    pub mins: &'a [u32],
    /// The rendered step (`STACK_WIDTH_STEP_PX × scale`).
    pub step: i64,
    /// The base-scale for the rendered→logical write-back.
    pub scale: f64,
}

/// The pure view state (D7).
#[derive(Debug, Clone, Default)]
pub struct ViewState {
    /// The surface chain (W5b D-H1): the root view, the pushed
    /// over-views, and the flow levels — one owned chain replacing
    /// the flat view/flow/return-slot model (chunk 1).
    surfaces: crate::surfaces::Surfaces,
    /// The selected board (design §3.8): `None` only before the seed
    /// rule first runs or with zero boards — `reconcile` seeds and
    /// repairs it (the normalization is refresh-time, one site).
    board: Option<BoardId>,
    /// The `StackDetail` subject, snapshotted at entry (D7).
    subject_stack: Option<StackId>,
    /// The `TaskDetail` subject, snapshotted at entry (D7).
    subject_task: Option<TaskId>,
    /// The habit variant's subject (H4 D1, US-5): the shared
    /// task-detail surface over a habit — set by
    /// [`Self::open_habit_editor`], cleared whenever the root leaves
    /// the detail (the same one-subject law the task subject's
    /// death pass speaks).
    subject_habit: Option<taskboard_domain::HabitId>,
    /// The habit editor's origin root (#1303, the D7 entry-snapshot
    /// law): `open_habit_editor` mounts by a ROOT SWAP — the origin
    /// view is destroyed, not pushed — so the entry snapshot is the
    /// only memory of where the editor came from. The exit seams
    /// ([`Self::close_habit_editor`]) restore it; never a hard route.
    /// Adapter-owned presentation state — never synced, never
    /// journaled.
    habit_editor_origin: Option<ViewId>,
    /// The history dialog's one-shot subject (HF3 W-8, US-7/S1): set
    /// by [`Self::open_habit_history`], consumed by the adapter's
    /// projection pass, cleared on close (the one-shot grammar). The
    /// paired month anchor is the visible month's any-date; the pager
    /// flips it (US-7/S3) and the clamps live in the presentation's
    /// can-page facts. Adapter-owned presentation state — never
    /// synced, never journaled.
    habit_history_target: Option<taskboard_domain::HabitId>,
    habit_history_month: Option<chrono::NaiveDate>,
    /// The board's focused card (selection, not slint focus).
    focused_task: Option<TaskId>,
    /// The today walk's focused habit row (chunk 8 D2, #934): the
    /// habit half of the `TodayRow` walk domain's cursor. Mutually
    /// exclusive with `focused_task` by construction — the today
    /// stepper clears the twin on every landing — and never set
    /// outside the today walk (habit rows are today-only, census
    /// R-1); a task-side focus write elsewhere (board, detail,
    /// selection) wins at the `walk_subject` read by task
    /// precedence, so no stale habit ever answers a task stroke.
    focused_habit: Option<taskboard_domain::HabitId>,
    /// The rule list's selected-rule cursor (J3 US-6/R-7, round 9
    /// chunk 12): `None` = disarmed; `Some(rule)` = the walked row.
    /// Adapter-owned presentation state like `focused_habit` (the
    /// task.select precedent) — never synced, never journaled. Id-
    /// keyed, NOT index-keyed: the row model carries no ids across
    /// the seam (rule-list.slint D2) and indices go stale on any
    /// roster change; `reconcile` clamps the cursor against the
    /// subject's live roster every pass (the `notification_cursor`
    /// clamp law). Roster-scoped: armed by the first vertical
    /// directional stroke at the task detail, cleared on view exit,
    /// subject change, and every foreign settle.
    rule_cursor: Option<taskboard_domain::ReminderRuleId>,
    /// The disarm latch (US-10, W-11, #1091): set ONLY by the
    /// `RowCursorDisarm` chord, cleared by every other clear site —
    /// while set, the verticals' arming gate declines and the
    /// chunk-9 control ladder owns the stroke again (the dedicated
    /// escape hatch; the flow table's `armed --> disarmed` edge).
    rule_cursor_disarmed: bool,
    /// The break-rules roster's selected-row cursor (US-9, W-10,
    /// #1066's roster half): the K4 shape — id-keyed, armed by the
    /// today walk's landing on a roster row, pruned against the
    /// active board's roster every pass, cleared on view exit (the
    /// reconcile face). Today-only presentation state; the remove
    /// verb rides the row's own armed-confirm seams through it.
    break_rule_cursor: Option<taskboard_domain::BreakRuleId>,
    /// The compact switcher's selected stack.
    selected_stack: Option<StackId>,
    /// The user-set stack column widths (round-1 D16), stored logical
    /// px; an absent entry = auto layout (today's rendering). UI-local
    /// forever — never a domain type (DF-2); `reconcile` leaves it
    /// alone (liveness pruning is the app forwarder's write-time job).
    stack_widths: BTreeMap<BoardId, BTreeMap<StackId, u32>>,
    /// The levels retired since the last drain (D3): every removal —
    /// truncate, XOR replace, root swap — hands its dropped levels
    /// here innermost-first, and the adapter's per-pass drain consumes
    /// them (the exit side-effects ride the drain, never a per-flow
    /// wipe rule).
    retired: Vec<crate::surfaces::Level>,
    /// The board selection (UX-R8 D-4, US-4/S1): the card-grid's
    /// multi-select — pure presentation, cleared by the D-4 paths
    /// (plain click, Escape/foreign settle, reconcile's prune,
    /// surface change). Lives beside `focused_task`: plain walk keeps
    /// the selection but moves focus (the chosen reading of the
    /// origin's US-4/S1 table).
    selection: BoardSelection,
    /// The archived-boards roster's one-shot select target (UX R-1
    /// D-1, issue #785): armed when a roster row dispatches
    /// `SetBoardArchived { archived: false }` — the engine applies the
    /// command asynchronously, so the selection is consumed by
    /// `reconcile` exactly on the first refresh where the board is
    /// live again (an armed target whose board is deleted prunes).
    pending_board_select: Option<BoardId>,
    /// The planner's focused pane (J3 R-5, round 9 chunk 9): `0` =
    /// placed boxes, `1` = candidate pool — the `models.rs`
    /// `plan_position` pane numbering. The directional left/right
    /// keys flip it; the in-pane walk rides the K4 cursor state
    /// (`plan_cursor`, US-15 — the typed inert retired, #1041).
    plan_focus_pane: usize,
    /// The planner's in-pane row cursor (US-15, #1041): `None` =
    /// disarmed; `Some(task)` = the walked row. The K4 selected-row
    /// cursor state's planner adopter (R-9 — the rule cursor's exact
    /// shape: adapter-owned presentation state, id-keyed like the
    /// rule cursor because indices go stale on any roster change).
    /// Armed by the first vertical directional stroke over the
    /// focused pane's rows; cleared by the pane switch, the view
    /// exit, the global cancel, and the refresh clamp (a dead id or
    /// a pane that no longer holds it — the clamp law).
    plan_cursor: Option<TaskId>,
}

/// The `Flow` the flow accessors return when no flow level is live —
/// a unit-variant constant keeps the `&Flow` signature (R2).
static FLOW_NONE: Flow = Flow::None;

// The order-key minting family lives in the domain now (#1351's re-home,
// round 7 chunk B3): the ONE home for "where does an inserted row land"
// is `taskboard_domain::order`, pure over `AppState`. The UI re-exports
// it so the adapter/actions/router call sites read unchanged; the
// multi-move's caller-supplied move ring comes from
// [`ViewState::move_ring`].
pub use taskboard_domain::order::{
    append_order, append_stack_order, move_aim_command, multi_move_commands, slot_order,
};

/// The review-walk's file verb (r2 chunk-8 D-F): a bare append into
/// `target` — a NEW-surface insertion, not a same-stack crossing move,
/// so it stays an absolute-order `MoveTask` at the [`append_order`]
/// (B3 2026-11-24 #1247: the move-aim mint — keyboard confirm and
/// pointer drop — lives in [`move_aim_command`]; this is not it).
fn append_move_command(state: &AppState, task: TaskId, target: StackId) -> FlowOutcome {
    FlowOutcome::Command(StateCommand::MoveTask {
        id: task,
        stack: target,
        order: append_order(state, target),
    })
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
    pub fn new_starting(view: ViewId) -> Self {
        let mut state = Self::new();
        state.surfaces = crate::surfaces::Surfaces::rooted(view);
        state
    }

    #[must_use]
    pub fn new() -> Self {
        Self {
            surfaces: crate::surfaces::Surfaces::rooted(ViewId::Board),
            board: None,
            subject_stack: None,
            subject_task: None,
            subject_habit: None,
            habit_editor_origin: None,
            habit_history_target: None,
            habit_history_month: None,
            focused_task: None,
            focused_habit: None,
            rule_cursor: None,
            rule_cursor_disarmed: false,
            break_rule_cursor: None,
            selected_stack: None,
            stack_widths: BTreeMap::new(),
            retired: Vec::new(),
            selection: BoardSelection::default(),
            pending_board_select: None,
            plan_focus_pane: 0,
            plan_cursor: None,
        }
    }

    #[must_use]
    pub fn view(&self) -> ViewId {
        self.surfaces.top_view()
    }

    /// The chain read seam (the adapter's offer walk reads the levels;
    /// writes stay inside this module).
    #[must_use]
    pub(crate) fn surfaces(&self) -> &crate::surfaces::Surfaces {
        &self.surfaces
    }

    /// The identity of what the main view mounts (r4 chunk 4, the D14
    /// dismiss seam): `(view, board, subject_stack, subject_task)`.
    /// `focused_task`/`selected_stack` are deliberately *not* members
    /// — a selection/focus step leaves every card mounted, and
    /// dismissing on focus steps would kill tooltips during ordinary
    /// pointer hover (the over-fire guard). Any effect whose
    /// application changes this tuple swapped an anchor away; the
    /// adapter's `apply_effect` choke point diffs it per pass.
    #[must_use]
    pub fn view_identity(&self) -> (ViewId, Option<BoardId>, Option<StackId>, Option<TaskId>) {
        (
            self.view(),
            self.board,
            self.subject_stack,
            self.subject_task,
        )
    }

    /// The selected board (the refresh-time seed/repair keeps it live;
    /// `None` only with zero boards).
    #[must_use]
    pub const fn board(&self) -> Option<BoardId> {
        self.board
    }

    #[must_use]
    pub fn flow(&self) -> &Flow {
        self.surfaces.top_flow().unwrap_or(&FLOW_NONE)
    }

    /// The flow's write seam (the palette cursor reset rides it —
    /// the adapter's keystroke path zeroes the cursor on re-rank).
    /// `None` when no flow level is live.
    pub(crate) fn flow_mut(&mut self) -> Option<&mut Flow> {
        self.surfaces.top_flow_mut()
    }

    /// Writes the flow slot (the flat model's `self.flow = …`, chain
    /// encoded): `None` ends the innermost flow level — truncate to
    /// its index, so a flow opened over a pushed over-view never
    /// closes the over-view; a fresh flow replaces the live one
    /// wholesale (the XOR rule — every opener's `self.flow = …` was a
    /// wholesale slot replacement); without a flow level it pushes
    /// one. The push is infallible here: without a live flow level no
    /// flow kind is in the chain, so the once-per-chain rule cannot
    /// refuse (the two over-views are view kinds, not flow kinds).
    /// Every dropped level — the ended one, the XOR-replaced one —
    /// retires with its state (the drop is the wipe, D-H2).
    fn write_flow(&mut self, flow: Flow) {
        if flow == Flow::None {
            let dropped = self.surfaces.end_top_flow();
            self.retired.extend(dropped);
        } else if let Some(replaced) = self.surfaces.replace_top_flow(flow.clone()) {
            self.retired.push(replaced);
        } else {
            // The mount's parent anchor (D-H7): a flat flow's exit
            // restores the view scope (the flowless hand-off's
            // vocabulary); the record rides the mint, before the
            // child mounts.
            self.surfaces
                .record_parent_anchor(crate::level_state::FocusAnchor::View);
            let _ = self.surfaces.push(crate::surfaces::Surface::Flow(flow));
        }
    }

    /// The root swap with its retirement (the flat model's
    /// `self.view = …`).
    fn replace_root(&mut self, view: ViewId) {
        let dropped = self.surfaces.replace_root(view);
        self.retired.extend(dropped);
        // The habit editor's subject is a mount fact, not focus
        // memory (H4 D1): it lives exactly while the detail root
        // mounts the habit variant.
        if view != ViewId::TaskDetail {
            self.subject_habit = None;
        }
        // UX-R8 D-4's surface-change cell: the selection never
        // survives into another surface. Same-root swaps are the
        // reconciliation's repair hops (a dead subject pops back to
        // Board) — the clear there is the same law, benignly.
        self.selection = BoardSelection::default();
    }

    /// The retire drain (D3): every level dropped since the last
    /// drain, innermost-first. The adapter consumes the exit
    /// side-effects (grid disarm, anchor restore) here and drops the
    /// corpses.
    pub(crate) fn drain_retired(&mut self) -> Vec<crate::surfaces::Level> {
        std::mem::take(&mut self.retired)
    }

    /// Takes the flow slot: the live flow out (or [`Flow::None`]),
    /// the level ended. The commit funnels' `mem::take` shape.
    fn take_flow(&mut self) -> Flow {
        let flow = self.flow().clone();
        self.write_flow(Flow::None);
        flow
    }

    /// The test fixture seam: writes the armed date editor's buffer
    /// (the typed-entry journeys).
    #[cfg(test)]
    pub(crate) fn set_buffer_for_date_editor_test(&mut self, text: &str) {
        self.date_editor_mut()
            .expect("the date editor armed")
            .buffer = text.to_owned();
    }

    /// The test fixture seam: arms `flow` as the live flow level
    /// (replace or push — the fixture side of `write_flow`).
    #[cfg(test)]
    pub(crate) fn arm_flow(&mut self, flow: Flow) {
        self.write_flow(flow);
    }

    /// The test fixture seam: pushes `flow` as a child level above the
    /// live flow level (the exit-protocol fixtures' chain builder).
    #[cfg(test)]
    pub(crate) fn arm_child_for_test(
        &mut self,
        flow: Flow,
    ) -> Result<usize, crate::surfaces::AlreadyInChain> {
        self.mount_child(flow)?;
        Ok(self.surfaces.depth() - 1)
    }

    /// The innermost `Walk` level's state (the review walk's answers):
    /// `None` without one.
    #[must_use]
    pub(crate) fn walk(&self) -> Option<&crate::level_state::WalkState> {
        self.surfaces
            .above_levels()
            .iter()
            .rev()
            .find_map(|level| level.state().as_walk())
    }

    /// The walk's write seam.
    pub(crate) fn walk_mut(&mut self) -> Option<&mut crate::level_state::WalkState> {
        self.surfaces
            .above_levels_mut()
            .iter_mut()
            .rev()
            .find_map(|level| level.state_mut().as_walk_mut())
    }

    /// The innermost `Picker` level's state (the standalone calendar
    /// child's read seam, N2): `None` without one. Deliberately blind
    /// to the date editor's embedded picker — that one lives in the
    /// `DateEditor` family.
    #[must_use]
    pub(crate) fn picker(&self) -> Option<&crate::level_state::PickerState> {
        self.surfaces
            .above_levels()
            .iter()
            .rev()
            .find_map(|level| level.state().as_picker())
    }

    /// The picker child's write seam (the grid keystrokes' cursor).
    pub(crate) fn picker_mut(&mut self) -> Option<&mut crate::level_state::PickerState> {
        self.surfaces
            .above_levels_mut()
            .iter_mut()
            .rev()
            .find_map(|level| level.state_mut().as_picker_mut())
    }

    /// The one child-mount seam (chunk 3's ports): pushes `flow` above
    /// the top flow level. The structural admission rule lives at the
    /// callers — only a surface's own handler reaches here, so no
    /// other path can open a child. The parent's anchor rides the
    /// mint (the flat flows' View restore; the drain reads it on the
    /// child's retirement).
    pub(crate) fn mount_child(
        &mut self,
        flow: Flow,
    ) -> Result<(), crate::surfaces::AlreadyInChain> {
        self.surfaces
            .record_parent_anchor(crate::level_state::FocusAnchor::View);
        self.surfaces
            .push(crate::surfaces::Surface::Flow(flow))
            .map(|_| ())
    }

    /// The entries form's date-grid toggle's open half (N2): mounts
    /// the picker child over the live form — the `EntryDatePicker`
    /// level mints clean; the adapter seeds it through
    /// [`Self::picker_mut`] right after. The `AlreadyInChain` refusal
    /// is the defensive no-op (D7: unreachable through the toggle
    /// seam — the kind is at most once per chain and nothing else
    /// opens it).
    pub fn open_entry_date_picker(&mut self) -> Vec<FlowOutcome> {
        if self.track_form().is_none() {
            return Vec::new();
        }
        // The defensive posture (chunk-1 D7): a typed refusal maps to
        // the no-op refresh. Unreachable through the toggle seam —
        // the duplicate-open unit test is the pin.
        let _ = self.mount_child(Flow::EntryDatePicker);
        vec![FlowOutcome::RefreshNow]
    }

    /// The picker child's Cancelled end (the toggle's close half and
    /// the grid's Enter rung, N2): the exit rides the one seam; the
    /// parent (the form) ignores a `Cancelled` exit, so the visible
    /// shape is the refresh that unmounts the grid.
    pub fn close_entry_date_picker(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        if self.picker().is_none() {
            return Vec::new();
        }
        self.end_top_flow_with(state, crate::surfaces::GuestExit::Cancelled)
    }

    /// The chain's flow tags, innermost first (the scope stack's
    /// concatenation input, D9).
    #[must_use]
    pub(crate) fn chain_tags(&self) -> Vec<crate::surfaces::FlowTag> {
        self.surfaces
            .above_levels()
            .iter()
            .rev()
            .filter_map(|level| level.flow().map(Flow::tag))
            .collect()
    }

    /// Whether a flow level matching `pred` sits anywhere in the
    /// chain (the shell's modal-mask booleans' truth): the top-flow
    /// read alone lies when a child level holds the chain's top.
    #[must_use]
    pub(crate) fn chain_has_flow(&self, pred: impl Fn(&Flow) -> bool) -> bool {
        self.surfaces
            .above_levels()
            .iter()
            .rev()
            .any(|level| level.flow().is_some_and(&pred))
    }

    /// The walk level's flow (read through any children above it):
    /// the stage machine's facts ride the walk level even when one of
    /// its children holds the chain's top — the flat `flow()` read is
    /// the innermost level, which under a live child is not the walk.
    #[must_use]
    pub(crate) fn walk_flow(&self) -> Option<Flow> {
        self.surfaces
            .above_levels()
            .iter()
            .rev()
            .find_map(|level| match level.flow() {
                Some(flow @ Flow::ReviewWalk { .. }) => Some(flow.clone()),
                _ => None,
            })
    }

    /// The walk level's write seam (through any children above it).
    pub(crate) fn walk_flow_mut(&mut self) -> Option<&mut Flow> {
        self.surfaces
            .above_levels_mut()
            .iter_mut()
            .rev()
            .find_map(|level| match level.flow_mut() {
                Some(flow @ Flow::ReviewWalk { .. }) => Some(flow),
                _ => None,
            })
    }

    /// The walk's File-to child's armed candidate (N3a): `Some` only
    /// while the child level is live (its `Option` variant data is the
    /// same-pass clamp window, D6/R3).
    #[must_use]
    pub(crate) fn walk_file_armed(&self) -> Option<StackId> {
        self.surfaces
            .above_levels()
            .iter()
            .rev()
            .find_map(|level| match level.flow() {
                Some(Flow::WalkFilePick { armed }) => *armed,
                _ => None,
            })
    }

    /// The walk's Pick-a-date child's read seam (N3b): `None` without
    /// one.
    #[must_use]
    pub(crate) fn walk_date(&self) -> Option<&crate::level_state::WalkDateState> {
        self.surfaces
            .above_levels()
            .iter()
            .rev()
            .find_map(|level| level.state().as_walk_date())
    }

    /// The walk's Pick-a-date child's write seam (the field's typed
    /// text).
    pub(crate) fn walk_date_mut(&mut self) -> Option<&mut crate::level_state::WalkDateState> {
        self.surfaces
            .above_levels_mut()
            .iter_mut()
            .rev()
            .find_map(|level| level.state_mut().as_walk_date_mut())
    }

    /// Whether the walk's Pick-a-date child is live (the `picking`
    /// mirror's truth, N3b).
    #[must_use]
    pub(crate) fn walk_date_child(&self) -> bool {
        self.surfaces
            .above_levels()
            .iter()
            .any(|level| matches!(level.flow(), Some(Flow::WalkDatePick)))
    }

    /// The innermost `PlanPlace` level's state (the placement form's
    /// two mirrors): `None` without one.
    #[must_use]
    pub(crate) fn plan_place(&self) -> Option<&crate::level_state::PlanPlaceState> {
        self.surfaces
            .above_levels()
            .iter()
            .rev()
            .find_map(|level| level.state().as_plan_place())
    }

    /// The placement form's write seam.
    pub(crate) fn plan_place_mut(&mut self) -> Option<&mut crate::level_state::PlanPlaceState> {
        self.surfaces
            .above_levels_mut()
            .iter_mut()
            .rev()
            .find_map(|level| level.state_mut().as_plan_place_mut())
    }

    /// The innermost `TrackForm` level's state (the entries form's
    /// read seam): `None` without one.
    #[must_use]
    pub(crate) fn track_form(&self) -> Option<&crate::level_state::TrackFormState> {
        self.surfaces
            .above_levels()
            .iter()
            .rev()
            .find_map(|level| level.state().as_track_form())
    }

    /// The entries form's write seam.
    pub(crate) fn track_form_mut(&mut self) -> Option<&mut crate::level_state::TrackFormState> {
        self.surfaces
            .above_levels_mut()
            .iter_mut()
            .rev()
            .find_map(|level| level.state_mut().as_track_form_mut())
    }

    /// The innermost `DateEditor` level's state (the due family's
    /// read seam): `None` without one — every reader renders the
    /// wiped state the old enforce pass produced.
    #[must_use]
    pub(crate) fn date_editor(&self) -> Option<&crate::level_state::DateEditorState> {
        self.surfaces
            .above_levels()
            .iter()
            .rev()
            .find_map(|level| level.state().as_date_editor())
    }

    /// The date editor's write seam (the owning flow's keystroke
    /// paths and seeding).
    pub(crate) fn date_editor_mut(&mut self) -> Option<&mut crate::level_state::DateEditorState> {
        self.surfaces
            .above_levels_mut()
            .iter_mut()
            .rev()
            .find_map(|level| level.state_mut().as_date_editor_mut())
    }

    /// The `EditRecur` flow's subject, if that flow holds (the strip
    /// seams' re-arm source — the flow is the live truth).
    #[must_use]
    pub(crate) fn task_of_edit_recur(&self) -> Option<TaskId> {
        match self.flow() {
            Flow::EditRecur { task } => Some(*task),
            _ => None,
        }
    }

    /// The recur editor's read seam (the model push's source).
    #[must_use]
    pub(crate) fn recur_editor(&self) -> Option<&crate::level_state::RecurEditorState> {
        self.surfaces
            .above_levels()
            .iter()
            .rev()
            .find_map(|level| level.state().as_recur_editor())
    }

    /// The recur editor's write seam (the opener's seeding and the
    /// field keystroke mirrors).
    pub(crate) fn recur_editor_mut(&mut self) -> Option<&mut crate::level_state::RecurEditorState> {
        self.surfaces
            .above_levels_mut()
            .iter_mut()
            .rev()
            .find_map(|level| level.state_mut().as_recur_editor_mut())
    }

    /// The innermost `Text` level's buffer (D9 rule 1): `""` without
    /// one — byte-equal to today's cleared shared buffer on every
    /// read (a text flow always holds the only live buffer).
    #[must_use]
    pub fn buffer(&self) -> &str {
        self.surfaces
            .above_levels()
            .iter()
            .rev()
            .find_map(|level| level.state().text_buffer())
            .unwrap_or("")
    }

    /// The adapter's keystroke-mirror update (D9 rule 1): writes the
    /// innermost `Text` level's buffer; a no-op without one (every
    /// post-close write today is a clear — the level is gone, so the
    /// clear is the no-op).
    pub fn set_buffer(&mut self, text: String) {
        let levels = self.surfaces.above_levels_mut();
        if let Some(level) = levels
            .iter_mut()
            .rev()
            .find(|level| matches!(level.state(), crate::level_state::LevelState::Text { .. }))
            && let Some(buffer) = level.state_mut().text_buffer_mut()
        {
            *buffer = text;
        }
    }

    #[must_use]
    pub const fn focused_task(&self) -> Option<TaskId> {
        self.focused_task
    }

    /// The today walk's focused habit row (chunk 8 D2): the cursor's
    /// habit half; `None` while the walk sits on a task row (the
    /// stepper's exclusivity law) or nowhere.
    #[must_use]
    pub const fn focused_habit(&self) -> Option<taskboard_domain::HabitId> {
        self.focused_habit
    }

    /// The walk cursor's subject (chunk 8 D3): the focused task or —
    /// exclusively, the stepper's law — the focused habit. Task
    /// precedence is deliberate: a task-side focus write from any
    /// other surface (board, detail, selection) wins over a stale
    /// habit landing, so a habit stroke can never answer a task
    /// focus. `None` when the walk has not entered yet.
    #[must_use]
    pub fn walk_subject(&self) -> Option<crate::presentation::RowSubject> {
        if let Some(task) = self.focused_task {
            return Some(crate::presentation::RowSubject::Task(task));
        }
        self.focused_habit
            .map(crate::presentation::RowSubject::Habit)
    }

    /// The walk cursor's full landing triple (A5a D-2, #1302): the
    /// scroll hint guard's before/after capture — task, habit, and
    /// break-rule roster in one equality (the exclusivity law makes
    /// at most one `Some` per landing).
    #[must_use]
    pub const fn walk_focus(&self) -> WalkFocus {
        WalkFocus {
            task: self.focused_task,
            habit: self.focused_habit,
            roster: self.break_rule_cursor,
            plan: self.plan_cursor,
        }
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

    /// The habit variant's subject (H4 D1) — `Some` exactly while the
    /// detail surface mounts the habit editor.
    #[must_use]
    pub const fn subject_habit(&self) -> Option<taskboard_domain::HabitId> {
        self.subject_habit
    }

    /// The history dialog's one-shot subject + visible-month anchor
    /// (HF3 W-8). Read accessor — the adapter's projection pass
    /// consumes it.
    #[must_use]
    pub const fn habit_history(
        &self,
    ) -> (Option<taskboard_domain::HabitId>, Option<chrono::NaiveDate>) {
        (self.habit_history_target, self.habit_history_month)
    }

    /// The history dialog's open (US-7/S1): the one-shot subject is
    /// minted, the visible month anchors at `now`'s month. No flow
    /// arms; the dismissal is the host's own (read role).
    pub fn open_habit_history(
        &mut self,
        habit: taskboard_domain::HabitId,
        today: chrono::NaiveDate,
    ) -> Vec<FlowOutcome> {
        self.habit_history_target = Some(habit);
        self.habit_history_month = Some(today);
        vec![FlowOutcome::RefreshNow]
    }

    /// The pager's month flip (US-7/S3): the presentation's
    /// can-page facts clamp the adapter's dispatch, so this flip is
    /// the pure arithmetic (no wrap — the ends hold).
    pub fn flip_habit_history_month(&mut self, delta: i32) -> Vec<FlowOutcome> {
        if let Some(month) = self.habit_history_month {
            self.habit_history_month = Some(crate::presentation::habit_month_flipped(month, delta));
            return vec![FlowOutcome::RefreshNow];
        }
        Vec::new()
    }

    /// The history dialog's close (US-7/S4): clears the one-shot
    /// subject and the month anchor — zero engine state change.
    pub fn close_habit_history(&mut self) -> Vec<FlowOutcome> {
        if self.habit_history_target.is_none() {
            return Vec::new();
        }
        self.habit_history_target = None;
        self.habit_history_month = None;
        vec![FlowOutcome::RefreshNow]
    }

    /// The rule list's selected-rule cursor (J3 US-6, round 9 chunk
    /// 12): `Some(rule)` while armed on the walked row, `None` while
    /// disarmed. Pure presentation — read by the row projection
    /// (`RuleRowModel.selected`) and the Activate resolution.
    #[must_use]
    pub const fn rule_cursor(&self) -> Option<taskboard_domain::ReminderRuleId> {
        self.rule_cursor
    }

    /// The pointer seam's cursor write (D-4 coherence): clicking a
    /// rule row — the row body or its remove affordance — moves the
    /// cursor there, so pointer and cursor can never disagree about
    /// which row is selected. No hover-follow: hover is not selection
    /// (the `focused_task` precedent).
    pub(crate) fn select_rule_cursor(&mut self, rule: taskboard_domain::ReminderRuleId) {
        self.rule_cursor = Some(rule);
    }

    /// The rule cursor's typed clear (the D-2 lifecycle sites): every
    /// site except the disarm chord lands here, so the chord's latch
    /// resets with the cursor — a foreign settle re-opens the arming
    /// gate.
    pub(crate) fn clear_rule_cursor(&mut self) {
        self.rule_cursor = None;
        self.rule_cursor_disarmed = false;
    }

    /// The break-rules roster's cursor read (US-9, W-10): `Some` while
    /// armed on the walked row.
    #[must_use]
    pub const fn break_rule_cursor(&self) -> Option<taskboard_domain::BreakRuleId> {
        self.break_rule_cursor
    }

    /// The pointer seam's cursor write (the K4 coherence law): a
    /// roster row's remove click moves the cursor there, so pointer
    /// and cursor never disagree about the selected row. The walk's
    /// exclusivity law: a roster landing clears the task/habit twins.
    pub(crate) fn select_break_rule_cursor(&mut self, rule: taskboard_domain::BreakRuleId) {
        self.break_rule_cursor = Some(rule);
        self.focused_task = None;
        self.focused_habit = None;
    }

    /// The cursor's typed clear (the K4 lifecycle sites).
    pub(crate) fn clear_break_rule_cursor(&mut self) {
        self.break_rule_cursor = None;
    }

    /// The disarm chord's write (US-10, W-11, #1091): clears the
    /// cursor AND latches the arming gate, so the verticals return to
    /// the chunk-9 control ladder without a view exit or foreign
    /// settle. Idempotent with the cursor disarmed (the adapter arms
    /// the typed inert there).
    pub(crate) fn disarm_rule_cursor(&mut self) {
        self.rule_cursor = None;
        self.rule_cursor_disarmed = true;
    }

    /// The planner's in-pane cursor (US-15, #1041): `Some(task)` while
    /// armed on the walked row, `None` while disarmed. Pure
    /// presentation — read by the Activate resolution (the row's own
    /// seam) and the refresh clamp. Never synced, never journaled.
    #[must_use]
    pub const fn plan_cursor(&self) -> Option<TaskId> {
        self.plan_cursor
    }

    /// The cursor's write (the rule cursor's select twin; the walk's
    /// arming and stepping both land here — one write seam).
    pub(crate) fn select_plan_cursor(&mut self, task: TaskId) {
        self.plan_cursor = Some(task);
    }

    /// The planner cursor's typed clear (the lifecycle sites: the
    /// pane switch, the global cancel, the refresh clamp).
    pub(crate) fn clear_plan_cursor(&mut self) {
        self.plan_cursor = None;
    }

    /// The subject's live rule roster in id-table order — the exact
    /// derivation the row projection speaks (`state.reminders.rules`
    /// filtered to the subject), so the walk's landing and the
    /// rendered rows agree by construction. Empty without a task
    /// subject.
    fn rule_roster(
        state: &AppState,
        subject: Option<TaskId>,
    ) -> Vec<taskboard_domain::ReminderRuleId> {
        subject
            .map(|task| {
                state
                    .reminders
                    .rules
                    .values()
                    .filter(|rule| rule.task == task)
                    .map(|rule| rule.id)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether a vertical directional stroke at the detail is the
    /// rule cursor's (D-5): a task subject with a non-empty roster —
    /// the stroke arms or steps instead of riding the control ladder.
    /// The empty-roster gate's read side: the adapter's
    /// `directional_control_step` must agree exactly with
    /// [`Self::walk_rule_cursor`]'s arming verdict, or the arming
    /// stroke would double-fire (the cursor arms AND the ladder
    /// steps).
    pub(crate) fn rule_walk_owns_stroke(&self, state: &AppState) -> bool {
        // The disarm latch (US-10): while set, the verticals belong to
        // the control ladder again — the declarative row must agree
        // with the walk's arming verdict or the stroke double-fires
        // (the D-5 agreement law).
        !self.rule_cursor_disarmed
            && self.subject_task.is_some()
            && !Self::rule_roster(state, self.subject_task).is_empty()
    }

    /// The walk's mechanics (J3 US-6 D-2, the `step_flat_domain` law):
    /// the first vertical stroke over a non-empty roster arms the
    /// cursor at row 0 (arm-then-step — the arming stroke does not
    /// move past row 0); each further stroke steps within the roster,
    /// clamping at both edges (no wrap — the ends hold). An empty
    /// roster and a subjectless detail are the typed no-op (never
    /// arm).
    pub(crate) fn walk_rule_cursor(
        &mut self,
        state: &AppState,
        direction: i32,
    ) -> Vec<FlowOutcome> {
        let roster = Self::rule_roster(state, self.subject_task);
        if roster.is_empty() || self.rule_cursor_disarmed {
            // The disarm latch (US-10): the stroke is the typed no-op
            // here and `rule_walk_owns_stroke` declines it — the
            // control-ladder row owns it instead.
            return Vec::new();
        }
        let current = self
            .rule_cursor
            .and_then(|cursor| roster.iter().position(|id| *id == cursor));
        let Some(current) = current else {
            self.rule_cursor = Some(roster[0]);
            return vec![FlowOutcome::RefreshNow];
        };
        let next = i64::from(direction) + i64::try_from(current).unwrap_or(0);
        match roster.get(usize::try_from(next).unwrap_or(usize::MAX)) {
            Some(id) => {
                self.rule_cursor = Some(*id);
                vec![FlowOutcome::RefreshNow]
            }
            None => Vec::new(), // clamp: no wrap, the ends hold (D-2)
        }
    }

    /// The habit editor's open (H4 D1, US-5): the `activate_focused`
    /// twin — the shared task-detail surface mounts over the habit
    /// subject (the mount discriminator decides the variant). No flow
    /// arms; the escape discipline is the surface's own.
    pub fn open_habit_editor(&mut self, habit: taskboard_domain::HabitId) -> Vec<FlowOutcome> {
        // The origin snapshot (#1303, D7's entry-snapshot law — the
        // `TaskDetail` subject's exact shape): the mount is a root
        // swap that destroys the origin view, so the exit seams'
        // return-to-origin reads THIS record, never a hard route.
        self.habit_editor_origin = Some(self.view());
        self.replace_root(ViewId::TaskDetail);
        self.subject_habit = Some(habit);
        vec![FlowOutcome::RefreshNow]
    }

    /// The habit editor's exit (#1303): the recorded origin root
    /// restores — Today stays Today, Board stays Board; a root swap
    /// to the same view is the typed no-op it always was. An
    /// unrecorded origin (a mounted variant that slipped past the
    /// opener — the kiosk belt's restored-snapshot posture) falls
    /// back to the board root, the D7 board-repair rule. The
    /// `replace_root` landing clears the habit subject (the
    /// mount-fact law); the origin record is consumed here.
    pub fn close_habit_editor(&mut self) -> Vec<FlowOutcome> {
        let origin = self.habit_editor_origin.take().unwrap_or(ViewId::Board);
        self.replace_root(origin);
        vec![FlowOutcome::RefreshNow]
    }

    /// US-10 (W-14, #1048) adapter predicate: is a shared edit flow
    /// armed over the habit subject (title when `description` is
    /// false, description when true)? Inert without a subject.
    #[must_use]
    pub fn habit_edit_flow_armed(
        &self,
        habit: Option<taskboard_domain::HabitId>,
        description: bool,
    ) -> bool {
        let Some(habit) = habit else {
            return false;
        };
        let want = if description {
            Flow::EditDescription {
                subject: EditSubject::Habit(habit),
            }
        } else {
            Flow::EditTitle {
                subject: EditSubject::Habit(habit),
            }
        };
        *self.flow() == want
    }

    /// The push-side kiosk belt (W-15, #1049): the dead-subject pass's
    /// twin — on kiosk a mounted habit subject (a posture that slipped
    /// past the dispatch-time gates: a restored or pushed snapshot)
    /// steps the view back through the SAME mechanism (`replace_root`,
    /// the subject clear, the board root). Desktop-inert: `kiosk` is
    /// the adapter's held run mode; the dispatch gates stay the first
    /// line, this pass is the second.
    pub fn kiosk_habit_belt(&mut self, kiosk: bool) -> Vec<FlowOutcome> {
        if !kiosk || self.subject_habit.is_none() {
            return Vec::new();
        }
        self.subject_habit = None;
        if self.view() == ViewId::TaskDetail {
            self.replace_root(ViewId::Board);
        }
        vec![FlowOutcome::RefreshNow]
    }

    /// Direct view/focus injection (the chunk-2 test seam; real
    /// sessions go through the view cycle and the card clicks).
    pub fn set_view(&mut self, view: ViewId) {
        self.replace_root(view);
    }

    /// Direct focus injection (see [`Self::set_view`]).
    pub fn set_focused(&mut self, task: Option<TaskId>) {
        self.focused_task = task;
    }

    /// The keymap context of the current view (the chunk-2 seam).
    #[must_use]
    pub fn context(&self) -> ContextId {
        match self.view() {
            ViewId::Today => ContextId::Today,
            ViewId::Board => ContextId::Board,
            ViewId::StackDetail => ContextId::StackDetail,
            ViewId::TaskDetail => ContextId::TaskDetail,
            ViewId::Review => ContextId::Review,
            ViewId::Settings => ContextId::Settings,
            ViewId::Status => ContextId::StatusPanel,
            ViewId::PlanNextDay => ContextId::PlanNextDay,
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
        // The cycle is inert while an over-view is pushed (round-1
        // D5); Settings and the planner are the only pushed view
        // kinds, so this is byte-equal to the old `self.view` match.
        if self.surfaces.has_pushed_view() {
            return Vec::new();
        }
        self.step_view(state, 1)
    }

    /// `nav.prev-view`: the cycle backwards; inert while Settings is
    /// open (round-1 D5).
    pub fn prev_view(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        // The `next_view` guard's twin.
        if self.surfaces.has_pushed_view() {
            return Vec::new();
        }
        self.step_view(state, -1)
    }

    /// Opens the settings surface (round-1 D6): pushes a Settings
    /// view level — the exact restore is truncation (D-H1), the
    /// recorded-return slot is retired. A refusal (Settings already
    /// in the chain — D7) is a no-op that only repaints.
    pub fn open_settings(&mut self) -> Vec<FlowOutcome> {
        match self
            .surfaces
            .push(crate::surfaces::Surface::View(ViewId::Settings))
        {
            Ok(_) | Err(crate::surfaces::AlreadyInChain) => vec![FlowOutcome::RefreshNow],
        }
    }

    /// Closes the settings surface: truncates the pushed Settings
    /// level — the root view is restored exactly. Without a pushed
    /// level (the old no-recorded-return path) the fallback lands on
    /// the board, byte-equal to the retired `unwrap_or(ViewId::Board)`.
    pub fn close_settings(&mut self) -> Vec<FlowOutcome> {
        self.close_over_view();
        vec![FlowOutcome::RefreshNow]
    }

    /// Opens the status panel (OP3 W-1, D-5): the `open_settings`
    /// twin — a pushed view level, exact restore by truncation, the
    /// D7 refusal mapped to repaint-only. Read-only surface (R-3).
    pub fn open_status_panel(&mut self) -> Vec<FlowOutcome> {
        match self
            .surfaces
            .push(crate::surfaces::Surface::View(ViewId::Status))
        {
            Ok(_) | Err(crate::surfaces::AlreadyInChain) => vec![FlowOutcome::RefreshNow],
        }
    }

    /// Closes the status panel: the `close_settings` twin.
    pub fn close_status_panel(&mut self) -> Vec<FlowOutcome> {
        self.close_over_view();
        vec![FlowOutcome::RefreshNow]
    }

    /// Opens the plan-next-day surface (#622, D-4): the `open_settings`
    /// twin — a pushed view level, exact restore by truncation, the
    /// D7 refusal mapped to repaint-only.
    pub fn open_plan_next_day(&mut self) -> Vec<FlowOutcome> {
        // A fresh open anchors the pane cursor on the placed boxes
        // (the day shape's primary pane; J3 R-5's two-pane grammar).
        self.plan_focus_pane = 0;
        match self
            .surfaces
            .push(crate::surfaces::Surface::View(ViewId::PlanNextDay))
        {
            Ok(_) | Err(crate::surfaces::AlreadyInChain) => vec![FlowOutcome::RefreshNow],
        }
    }

    /// The planner's focused pane (J3 R-5, round 9 chunk 9): `0` =
    /// placed boxes, `1` = candidate pool — the `models.rs`
    /// `plan_position` numbering.
    #[must_use]
    pub const fn plan_focus_pane(&self) -> usize {
        self.plan_focus_pane
    }

    /// The directional left/right kept arm on the planner (J3 R-5):
    /// the two-pane grammar's pane switch. Two panes, so both
    /// directions toggle; Up/Down ride the in-pane cursor (US-15,
    /// #1041 — the K4 adopter). The switch is a cursor clear site
    /// (US-15/S1's escape column; the K4 clear law — the cursor is
    /// pane-scoped, never carried across panes, US-15/S4).
    pub fn switch_plan_pane(&mut self, direction: i32) -> Vec<FlowOutcome> {
        if self.view() != ViewId::PlanNextDay {
            return Vec::new(); // the typed no-op: the grammar is the planner's
        }
        self.clear_plan_cursor();
        // Two panes: the step lands if it stays in `{0, 1}`, else it
        // toggles (a second Left from boxes wraps to the pool — the
        // two-value ring is the pane count's own geometry).
        let stepped = i64::try_from(self.plan_focus_pane).unwrap_or(0) + i64::from(direction);
        self.plan_focus_pane = match stepped {
            0 => 0,
            1 => 1,
            _ => 1 - self.plan_focus_pane.min(1),
        };
        vec![FlowOutcome::RefreshNow]
    }

    /// Closes the plan-next-day surface: the `close_settings` twin.
    pub fn close_plan_next_day(&mut self) -> Vec<FlowOutcome> {
        self.close_over_view();
        vec![FlowOutcome::RefreshNow]
    }

    /// US-5 (task-detail-editor UX plan D-10, #706): the over-view
    /// restore's typed subject-liveness guard. A truncation-exact
    /// restore re-reveals the buried root without `reconcile`'s
    /// subject-death pass (that pops a `TaskDetail` only while it is
    /// the *current* view — under a pushed over-view the pop never
    /// runs), so a subject tombstoned under the over-view would
    /// restore an empty detail shell. Called from every over-view
    /// restore seam after the truncate: a restored root `TaskDetail`
    /// whose snapshot subject no longer resolves to a live task falls
    /// back to the board (the selection-repair shape; `reconcile`'s
    /// normalize picks the lowest live board when that one is gone
    /// too). Inert for live subjects (AC-2: the existing restoration
    /// tests stand) — the focus contract composes: the close seam's
    /// focus-restore funnel runs after this swap.
    pub fn repair_restored_root(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        if self.view() == ViewId::TaskDetail {
            // The habit variant's root is dead without a live habit
            // subject (the task subject's twin).
            if let Some(habit) = self.subject_habit
                && !state
                    .habits
                    .habits
                    .get(&habit)
                    .is_some_and(taskboard_domain::habit::Habit::is_live)
            {
                self.replace_root(ViewId::Board);
                return vec![FlowOutcome::RefreshNow];
            }
            let dead = self.subject_task.is_none_or(|id| {
                !state
                    .tasks
                    .get(&id)
                    .is_some_and(taskboard_domain::Task::is_live)
            });
            if dead {
                self.replace_root(ViewId::Board);
                return vec![FlowOutcome::RefreshNow];
            }
        }
        Vec::new()
    }

    /// Ends the topmost pushed view level (the close-over primitive):
    /// truncates to its index; without one, the navigation fallback
    /// swaps the root to the board (the retired
    /// `unwrap_or(ViewId::Board)`).
    fn close_over_view(&mut self) {
        let pushed = self
            .surfaces
            .kinds()
            .iter()
            .rposition(|kind| matches!(kind, crate::surfaces::SurfaceKind::View(_)));
        match pushed {
            Some(0) | None => self.replace_root(ViewId::Board),
            Some(index) => {
                let dropped = self.surfaces.truncate_to(index);
                self.retired.extend(dropped);
            }
        }
    }

    fn step_view(&mut self, state: &AppState, direction: i32) -> Vec<FlowOutcome> {
        let order = [
            ViewId::Today,
            ViewId::Board,
            ViewId::StackDetail,
            ViewId::TaskDetail,
            ViewId::Review,
        ];
        let current = order
            .iter()
            .position(|v| *v == self.surfaces.top_view())
            .unwrap_or(0);
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
                self.replace_root(ViewId::Today);
                true
            }
            ViewId::Board => {
                self.replace_root(ViewId::Board);
                true
            }
            // Subjectless, exactly like Today's arm (#214 D-1): the
            // review surface reads the whole state, no subject.
            ViewId::Review => {
                self.replace_root(ViewId::Review);
                true
            }
            ViewId::StackDetail => {
                let stack = self
                    .focused_stack(state)
                    .or(self.selected_stack)
                    .or_else(|| self.stack_ids(state).first().copied());
                match stack {
                    Some(stack) => {
                        self.replace_root(ViewId::StackDetail);
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
                        self.replace_root(ViewId::TaskDetail);
                        // A fresh open is a new roster (D-2 site 3):
                        // the arm is roster-scoped, never carried
                        // across detail opens.
                        self.clear_rule_cursor();
                        self.subject_task = Some(task);
                        // The task/habit subjects are mutually
                        // exclusive mount facts (H4 D1) — same
                        // retirement as `activate_focused`.
                        self.subject_habit = None;
                        true
                    }
                    None => false,
                }
            }
            // The settings surface never enters through the cycle
            // (round-1 D5): only `open_settings` reaches it. The
            // plan-next-day surface likewise (D-1): only
            // `open_plan_next_day` reaches it. The status panel
            // likewise (OP3): only `open_status_panel` reaches it.
            ViewId::Settings | ViewId::PlanNextDay | ViewId::Status => false,
        }
    }

    /// `nav.focus-board`: back to the board view (D7).
    pub fn focus_board(&mut self) -> Vec<FlowOutcome> {
        self.replace_root(ViewId::Board);
        vec![FlowOutcome::RefreshNow]
    }

    /// `view.today` (chunk 10 D3): the Today view, no engine command —
    /// the `focus_board` sibling.
    pub fn focus_today(&mut self) -> Vec<FlowOutcome> {
        self.replace_root(ViewId::Today);
        vec![FlowOutcome::RefreshNow]
    }

    /// `nav.show-task` (round-3 D23): resolves the task → stack →
    /// board, selects all three, and mounts the board view — the
    /// task's home, always (`ViewId::Board`), from any source view.
    /// A stale id (the entity died between the palette push and the
    /// invoke) is inert: empty outcomes, no repaint — the
    /// `palette_jump` stale rule. Subjects are left alone (the
    /// refresh-time re-validation pops stale ones), and any pushed
    /// over-view level stays pushed (replaced at the next open).
    pub fn show_task(&mut self, state: &AppState, task: TaskId) -> Vec<FlowOutcome> {
        let Some(stack) = state.tasks.get(&task).map(|t| t.stack) else {
            return Vec::new();
        };
        let Some(board) = state.stacks.get(&stack).map(|s| s.board) else {
            return Vec::new();
        };
        self.board = Some(board);
        self.selected_stack = Some(stack);
        self.focused_task = Some(task);
        self.replace_root(ViewId::Board);
        vec![FlowOutcome::RefreshNow]
    }

    /// The toolbar view switcher's pushed index (round-3 D22): the
    /// `[Board, Today]` segment order. The deeper views are the board
    /// branch (`nav.focus-board` returns from them), and while
    /// Settings is open the segment names where settings returns to
    /// (nothing recorded reads as the board).
    #[must_use]
    pub fn switcher_selection(&self) -> usize {
        // While an over-view is pushed, the recorded return of either
        // over-view is always the view the chain keeps at the root
        // (both openers record the then-top view, and a pushed level
        // sits directly above the root), so the root's id is the
        // byte-equal re-expression (chunk 1 R3).
        usize::from(self.surfaces.root() == ViewId::Today)
    }

    // -- board selection (design §3.8) -----------------------------------

    /// Seeds or repairs the board selection: `None` takes the first
    /// live board; a selection that is no longer live (deleted or
    /// archived under the session) falls back the same way. The one
    /// normalization site — `reconcile` runs it refresh-time; the
    /// board walk and `begin_create_stack` before they read.
    /// A board row's liveness for the ceremony strips' prune arms
    /// (OP1): present and not tombstoned (an archived board stays
    /// commandable — the archived board is not a death).
    fn board_live(state: &AppState, board: BoardId) -> bool {
        state.boards.get(&board).is_some_and(|row| !row.deleted)
    }

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

    /// The archived-boards roster's unarchive verb (UX R-1 D-1, issue
    /// #785): dispatches the exact `SetBoardArchived { archived: false
    /// }` plus the models refresh, and arms the one-shot select target
    /// — `reconcile` selects the board on the first refresh where the
    /// engine's apply makes it live again (the async apply makes an
    /// immediate `select_board` repair away). The command is journal-
    /// eligible (`undo.rs:324`), so `AppUndo` is the escape path.
    pub fn unarchive_board(&mut self, board: BoardId) -> Vec<FlowOutcome> {
        self.pending_board_select = Some(board);
        vec![
            FlowOutcome::Command(StateCommand::SetBoardArchived {
                board,
                archived: false,
            }),
            FlowOutcome::RefreshNow,
        ]
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
        // the domain (chunk 10 D7), even in compact. Chunk 8 (D2,
        // #934) widens the domain to `TodayRow` — the task sections
        // then the aggregated live habits (all live boards; round-6
        // A1 D-2) — so the same keys traverse the habit section.
        if self.view() == ViewId::Today {
            let rows = self.today_rows(state, kiosk, day);
            let outcomes = self.step_today_rows(direction, compact, &rows);
            self.follow_board(state);
            return outcomes;
        }
        // The detail's item walk is RETIRED (J3 R-3, 2026-11-19 — the
        // round-8 keymap-parity D-2 supersession, census-dated): no
        // viewed-task change from the detail at all; return-to-board-
        // and-walk is the grammar. The ids still resolve in the mode
        // contexts (vi's H/G, the member rows that keep their board
        // homes), so the guard is the typed no-op — the hidden board
        // selection never moves under the open detail (the silent
        // absorb the D-2 fix fought stays dead).
        if self.view() == ViewId::TaskDetail {
            return Vec::new();
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

    /// The today walk's mechanics over the widened `TodayRow` domain
    /// (chunk 8 D2, #934): the flat-walk rules of `step_flat_domain`
    /// (no wrap, entry at the first row, compact retreat hands the
    /// switcher level back) with the cursor split across the
    /// exclusivity pair — a habit landing clears `focused_task`, a
    /// task landing clears `focused_habit`.
    fn step_today_rows(
        &mut self,
        direction: i32,
        compact: bool,
        rows: &[TodayRow],
    ) -> Vec<FlowOutcome> {
        let current = self.walk_subject().and_then(|subject| {
            rows.iter().position(|row| match (*row, subject) {
                (TodayRow::Task(task), crate::presentation::RowSubject::Task(focused)) => {
                    task == focused
                }
                (TodayRow::Habit(habit), crate::presentation::RowSubject::Habit(focused)) => {
                    habit == focused
                }
                _ => false,
            })
        });
        // The roster cursor's position (US-9, W-10): the armed roster
        // row is the walk's position whenever the task/habit subject
        // is not (the exclusivity law makes them mutually exclusive).
        let current = current.or_else(|| {
            self.break_rule_cursor.and_then(|cursor| {
                rows.iter().position(|row| match *row {
                    TodayRow::BreakRule(rule) => rule == cursor,
                    _ => false,
                })
            })
        });
        let Some(current) = current else {
            // No focus yet: entering the walk lands on the first row.
            return match rows.first() {
                Some(first) => {
                    self.focus_today_row(*first);
                    vec![FlowOutcome::RefreshNow]
                }
                None => Vec::new(),
            };
        };
        let next = i64::from(direction) + i64::try_from(current).unwrap_or(0);
        if next < 0 {
            if compact {
                // Retreat past the first row hands focus back to the
                // switcher level (selection-follows-focus, D8).
                self.focused_task = None;
                self.focused_habit = None;
                return vec![FlowOutcome::RefreshNow];
            }
            return Vec::new(); // no wrap (D7)
        }
        match rows.get(usize::try_from(next).unwrap_or(usize::MAX)) {
            Some(row) => {
                self.focus_today_row(*row);
                vec![FlowOutcome::RefreshNow]
            }
            None => Vec::new(), // no wrap (D7): the ends hold
        }
    }

    /// The today walk's landing write (chunk 8 D2): the exclusivity
    /// law in one place — the twin cursor clears on every landing,
    /// whichever half moves.
    fn focus_today_row(&mut self, row: TodayRow) {
        match row {
            TodayRow::Task(task) => {
                self.focused_task = Some(task);
                self.focused_habit = None;
            }
            TodayRow::Habit(habit) => {
                self.focused_habit = Some(habit);
                self.focused_task = None;
            }
            // The roster row's landing arms the K4 cursor (US-9,
            // W-10): the arming IS the landing (arm-on-first-vertical
            // via the walk's entry rule); the twins clear (the
            // exclusivity law).
            TodayRow::BreakRule(rule) => {
                self.break_rule_cursor = Some(rule);
                self.focused_task = None;
                self.focused_habit = None;
            }
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
        // The detail's first/last jumps are RETIRED with the step walk
        // (J3 R-3, 2026-11-19 — the same census-dated supersession):
        // M-</M-> at the detail are the typed no-op, the subject
        // pinned.
        if self.view() == ViewId::TaskDetail {
            return Vec::new();
        }
        let rows = if self.view() == ViewId::Today {
            // The widened today domain (chunk 8 D2): tasks then habits.
            self.today_rows(state, kiosk, day)
        } else {
            self.focus_domain(state, compact)
                .into_iter()
                .map(TodayRow::Task)
                .collect::<Vec<_>>()
        };
        let target = match edge {
            FocusEdge::First => rows.first(),
            FocusEdge::Last => rows.last(),
        };
        match target {
            Some(row) => {
                self.focus_today_row(*row);
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

    /// The today walk's rows (chunk 8 D2, #934): `today_domain`'s
    /// task rows in display order, then the aggregated live habits
    /// (`presentation::today_habit_rows` — all live boards, canonical
    /// board order outer, stored `(order, id)` inner; round-6 A1
    /// D-2, 2026-11-24, #1301 — the board-scoped walk retired), then
    /// the aggregated break-rule roster (`break_rule_rows(state,
    /// None)`, D-3) — the section model's own derivations, so the
    /// walk's landing and the rendered rows agree by construction.
    /// Zero live boards carry no habit rows (the section renders
    /// empty with them).
    #[must_use]
    pub fn today_rows(&self, state: &AppState, kiosk: bool, day: &DayCtx) -> Vec<TodayRow> {
        let mut rows: Vec<TodayRow> = self
            .today_domain(state, kiosk, day)
            .into_iter()
            .map(TodayRow::Task)
            .collect();
        rows.extend(
            crate::presentation::today_habit_rows(state, day.now, day.calendar)
                .into_iter()
                .map(|row| TodayRow::Habit(row.habit)),
        );
        // The break-rules roster's rows (US-9, W-10, #1066's roster
        // half): after the habit section, in the same canonical order
        // the section's model speaks (`break_rule_rows` — the walk's
        // landing and the rendered rows agree by construction).
        // Desktop-only (the section's kiosk absence); the roster
        // aggregates over all boards (round-6 A1 D-3).
        if !kiosk {
            rows.extend(
                crate::presentation::break_rule_rows(state, None)
                    .into_iter()
                    .map(|row| TodayRow::BreakRule(row.rule)),
            );
        }
        rows
    }

    /// Selection-follows-focus (chunk 10 D7, D-V3): after each today
    /// focus step the board retargets to the row's board and the
    /// stack selection to the row's stack — keeping `focused_task` (a
    /// lighter touch than `select_board`, which would reset focus and
    /// break the walk). Keeps `task.move`'s ring and detail navigation
    /// board-local. No-op when the board already matches.
    fn follow_board(&mut self, state: &AppState) {
        if self.view() != ViewId::Today {
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
        // The task landing clears the habit cursor (chunk 8 D2's
        // exclusivity law at the pointer seam too — the walk's own
        // twin of the stepper's landing write).
        self.focused_habit = None;
        let outcomes = self.activate_card(task, stack);
        self.follow_board(state);
        outcomes
    }

    /// The habit row's pointer landing (#1200, chunk A1 D2): the
    /// walk landing write's twin at the click seam — the exclusivity
    /// law in one place (`focus_today_row`'s Habit arm verbatim) and
    /// the emphasis ring's refresh. The seam opens no editor (habits
    /// have no click-to-open verb; H4's reserved verbs untouched).
    pub fn activate_habit_row(&mut self, habit: taskboard_domain::HabitId) -> Vec<FlowOutcome> {
        self.focused_habit = Some(habit);
        self.focused_task = None;
        vec![FlowOutcome::RefreshNow]
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

    // -- the board selection (UX-R8 D-4, US-4/S1) -------------------------

    /// The selection read (the models pass and the menu branch's
    /// input).
    #[must_use]
    pub const fn selection(&self) -> &BoardSelection {
        &self.selection
    }

    /// The dispatch order (D-4): board reading order — columns in
    /// `stack_ids` order, tasks in canonical order within a column —
    /// with the anchor first, then the remaining members in reading
    /// order. Dead members are pruned against the same pass (the
    /// sequence-build's per-task absorb input).
    #[must_use]
    pub fn selection_anchor_order(&self, state: &AppState) -> Vec<TaskId> {
        let reading: Vec<TaskId> = self.reading_order(state);
        let Some(anchor) = self.selection.anchor else {
            return Vec::new();
        };
        let mut ordered = Vec::new();
        if reading.contains(&anchor) {
            ordered.push(anchor);
        }
        ordered.extend(
            reading
                .into_iter()
                .filter(|id| *id != anchor && self.selection.members.contains(id)),
        );
        // A dead-anchor repair: the anchor's card is gone but members
        // survive — promote the first member (the reconcile pass
        // normally prunes first; this is the sequence-build's belt).
        if ordered.is_empty()
            && let Some(first) = self.selection.members.iter().next().copied()
        {
            ordered.push(first);
        }
        ordered
    }

    /// Ctrl+Click (D-4): toggle the clicked card; on an empty
    /// selection it becomes the anchor. Toggling off the anchor
    /// promotes the first remaining member (the carrier invariant:
    /// non-empty members always carry an anchor — a documented
    /// leniency; the range then spans from the promoted anchor).
    pub fn selection_toggle(&mut self, task: TaskId) -> Vec<FlowOutcome> {
        if self.selection.anchor.is_some() {
            if !self.selection.members.remove(&task) {
                self.selection.members.insert(task);
            } else if self.selection.members.is_empty() {
                self.selection.anchor = None;
            } else if self.selection.anchor == Some(task) {
                self.selection.anchor = self.selection.members.iter().next().copied();
            }
        } else {
            self.selection.reset(task);
        }
        vec![FlowOutcome::RefreshNow]
    }

    /// Shift+Click (D-4): the range span — members become the
    /// reading-order span `[min(anchor_idx, clicked_idx),
    /// max(...)]` over the resolved board; the anchor stays. An
    /// unresolvable clicked card (stale/foreign surface) is absorbed
    /// silently; an empty selection (or a dead anchor) takes the
    /// clicked card as the new anchor's singleton.
    pub fn selection_range(&mut self, state: &AppState, clicked: TaskId) -> Vec<FlowOutcome> {
        let reading = self.reading_order(state);
        let Some(clicked_idx) = reading.iter().position(|id| *id == clicked) else {
            return Vec::new();
        };
        let anchor_idx = self
            .selection
            .anchor
            .and_then(|anchor| reading.iter().position(|id| *id == anchor));
        let Some(anchor_idx) = anchor_idx else {
            self.selection.reset(clicked);
            return vec![FlowOutcome::RefreshNow];
        };
        let (lo, hi) = if anchor_idx <= clicked_idx {
            (anchor_idx, clicked_idx)
        } else {
            (clicked_idx, anchor_idx)
        };
        self.selection.members = reading[lo..=hi].iter().copied().collect();
        vec![FlowOutcome::RefreshNow]
    }

    /// Shift+walk (D-4): the named-key walk with shift extends the
    /// selection over the board walk's domain. An empty selection
    /// anchors at the focused card (the singleton); each newly
    /// focused card joins. The walk itself is the flat reading-order
    /// step (no wrap; the ends hold) — plain walk keeps moving focus
    /// without touching the selection (the chosen reading).
    pub fn extend_selection_walk(&mut self, state: &AppState, direction: i32) -> Vec<FlowOutcome> {
        let domain = self.reading_order(state);
        let Some(current) = self.focused_task else {
            // No focus yet: entering the walk lands on the first card
            // and anchors the singleton (the step_focus entry rule).
            return match domain.first() {
                Some(first) => {
                    self.focused_task = Some(*first);
                    self.selection.reset(*first);
                    vec![FlowOutcome::RefreshNow]
                }
                None => Vec::new(),
            };
        };
        if self.selection.is_empty() {
            self.selection.reset(current);
        }
        let Some(cur_idx) = domain.iter().position(|id| *id == current) else {
            return Vec::new(); // a dead focus: reconcile repairs first
        };
        let next_idx = i64::from(direction) + i64::try_from(cur_idx).unwrap_or(0);
        let Some(next) = usize::try_from(next_idx)
            .ok()
            .and_then(|idx| domain.get(idx).copied())
        else {
            return Vec::new(); // the ends hold (no wrap)
        };
        self.focused_task = Some(next);
        self.selection.members.insert(next);
        vec![FlowOutcome::RefreshNow]
    }

    /// The plain-click reset (D-4): the click seam's selection half —
    /// focus still moves (the open posture rides the action layer).
    pub fn reset_selection(&mut self) {
        self.selection = BoardSelection::default();
    }

    /// The menu-open settle's belt (UX-R8 D-11): the selection handed
    /// across a settle that must keep it (the context-menu funnel —
    /// the exit restores the invoking card with the selection still
    /// driving the menu's grammar on reopen).
    pub(crate) fn take_selection(&mut self) -> BoardSelection {
        std::mem::take(&mut self.selection)
    }

    /// The take's twin (see [`Self::take_selection`]).
    pub(crate) fn restore_selection(&mut self, selection: BoardSelection) {
        self.selection = selection;
    }

    /// The clear (Escape / a foreign settle / the reconcile prune's
    /// empty arm): nothing dispatches.
    pub fn clear_selection(&mut self) -> Vec<FlowOutcome> {
        if self.selection.is_empty() {
            return Vec::new();
        }
        self.selection = BoardSelection::default();
        vec![FlowOutcome::RefreshNow]
    }

    /// vi's insert intent inside `TaskDetail` (D5): begin editing the
    /// viewed task's title in place (the view does not switch).
    pub fn edit_subject(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        match self.subject_task {
            Some(task) => self.begin_edit_title(state, task),
            None => Vec::new(),
        }
    }

    /// A date cell's click (r2 D25, round-3 D8): begin the subject
    /// task's editor for `field` in place — insert mode (the typed
    /// field's printables). The buffer seeding is the adapter's D11
    /// seam (the view state owns no date buffer).
    pub fn begin_edit_date(&mut self, state: &AppState, field: DateField) -> Vec<FlowOutcome> {
        let Some(task) = self.subject_task.and_then(|id| state.tasks.get(&id)) else {
            return Vec::new();
        };
        // The date editor is a foreign settle (D-2 site 4): the rule
        // cursor never survives under a mask. Both fronts land here —
        // the row affordances and the keyboard openers (click ≡ key
        // at the funnel).
        self.clear_rule_cursor();
        self.write_flow(Flow::EditDate {
            task: task.id,
            field,
        });
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// The date-commit funnel's end (r2 D25): the adapter dispatches
    /// the `UpdateTask` itself; ending the flow returns to normal.
    pub fn end_edit_date(&mut self) -> Vec<FlowOutcome> {
        self.write_flow(Flow::None);
        vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
    }

    /// The recurrence editor's opener (round-7 D-7.1, plan D-D1):
    /// arm the window-level overlay flow over the subject task —
    /// insert mode (the form's text fields). The level's draft seeds
    /// through the adapter's opener seam (the view state owns no
    /// recur buffer, the `begin_edit_date` rule).
    pub fn begin_edit_recur(&mut self, state: &AppState) -> Option<TaskId> {
        let task = self.subject_task.filter(|id| {
            state
                .tasks
                .get(id)
                .is_some_and(taskboard_domain::Task::is_open)
        })?;
        // The recur overlay is a foreign settle (D-2 site 4).
        self.clear_rule_cursor();
        self.write_flow(Flow::EditRecur { task });
        Some(task)
    }

    /// The recurrence editor's clean close: nothing dispatches (the
    /// date-editor clean-close rule — cancel discards, the level dies
    /// with its buffers).
    pub fn end_edit_recur(&mut self) -> Vec<FlowOutcome> {
        self.write_flow(Flow::None);
        vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
    }

    /// Arms the description editor for `task` (round-3 D15/#228):
    /// the buffer seeds from the task's current description (the
    /// display mirror, the `begin_edit_title` pattern). Inert
    /// without a live task.
    pub fn begin_edit_description(&mut self, state: &AppState, task: TaskId) -> Vec<FlowOutcome> {
        let Some(task) = state.tasks.get(&task) else {
            return Vec::new();
        };
        self.write_flow(Flow::EditDescription {
            subject: EditSubject::Task(task.id),
        });
        self.set_buffer(task.description.clone());
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// The description commit funnel (round-3 D15/#228): dispatch the
    /// typed `UpdateTask` with the buffer verbatim — no trim, the
    /// description's whitespace is content; an empty buffer commits
    /// `Some("")`, which is the clear (the `TaskChanges.description`
    /// mask: `Some` sets, an empty string *is* the clear). The focus
    /// paths use [`Self::settle_flow`], which never reaches here
    /// empty.
    pub fn commit_description(&mut self) -> Vec<FlowOutcome> {
        let Flow::EditDescription { subject } = self.flow().clone() else {
            return Vec::new();
        };
        let description = self.buffer().to_owned();
        self.write_flow(Flow::None);
        // The habit subject rides the same settle rules, its own
        // command (W-14, #1048: the sparse `UpdateHabit` — the
        // description's whitespace is content, the empty buffer is
        // the clear, exactly the task mask's shape).
        let command = match subject {
            EditSubject::Task(task) => StateCommand::UpdateTask {
                id: task,
                changes: taskboard_domain::TaskChanges {
                    description: Some(description),
                    ..taskboard_domain::TaskChanges::default()
                },
            },
            EditSubject::Habit(habit) => StateCommand::UpdateHabit {
                habit,
                changes: taskboard_domain::HabitChanges {
                    description: Some(description),
                    ..taskboard_domain::HabitChanges::default()
                },
            },
        };
        vec![
            FlowOutcome::Command(command),
            FlowOutcome::SetMode(Mode::Normal),
            FlowOutcome::RefreshNow,
        ]
    }

    /// Arms the title editor for `task` (round-3 W-SM-d, F26): the
    /// one title-edit entry point — the registry action's
    /// `BeginTitleEdit`, the closed title's pointer seam, and vi's
    /// `i`/`a` (via [`Self::edit_subject`]) all funnel here. The
    /// buffer seeds from the task's current title (the display
    /// mirror). Inert without a live task.
    pub fn begin_edit_title(&mut self, state: &AppState, task: TaskId) -> Vec<FlowOutcome> {
        let Some(task) = state.tasks.get(&task) else {
            return Vec::new();
        };
        self.write_flow(Flow::EditTitle {
            subject: EditSubject::Task(task.id),
        });
        self.set_buffer(task.title.clone());
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// The habit variant's title edit (W-14, #1048): the shared flow
    /// arms over the habit subject, the buffer seeded from the stored
    /// row (the same display-mirror rule; the commit funnel routes the
    /// sparse `UpdateHabit`). Inert without a live habit row.
    pub fn begin_habit_title_edit(
        &mut self,
        state: &AppState,
        habit: taskboard_domain::HabitId,
    ) -> Vec<FlowOutcome> {
        let Some(row) = state.habits.habits.get(&habit) else {
            return Vec::new();
        };
        self.write_flow(Flow::EditTitle {
            subject: EditSubject::Habit(habit),
        });
        self.set_buffer(row.title.clone());
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// The habit variant's description edit (W-14, #1048): the
    /// `begin_edit_description` twin over the habit subject.
    pub fn begin_habit_description_edit(
        &mut self,
        state: &AppState,
        habit: taskboard_domain::HabitId,
    ) -> Vec<FlowOutcome> {
        let Some(row) = state.habits.habits.get(&habit) else {
            return Vec::new();
        };
        self.write_flow(Flow::EditDescription {
            subject: EditSubject::Habit(habit),
        });
        self.set_buffer(row.description.clone());
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// The `Activate` intent with no flow (D9; round-3 W-SM-d): open
    /// the focused task's detail — plain cycle-entry parity, NEVER an
    /// edit flow. The title editor arms only through
    /// [`Self::begin_edit_title`] (the F26 auto-arm is gone: after
    /// one close, the title is re-armable via the bound action, the
    /// pointer seam, or vi's `i`/`a`).
    pub fn activate_focused(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Some(task) = self.focused_task.and_then(|id| state.tasks.get(&id)) else {
            return Vec::new();
        };
        self.replace_root(ViewId::TaskDetail);
        self.subject_task = Some(task.id);
        // A task subject and the habit variant are mutually exclusive
        // mount facts (H4 D1): the task detail's entry retires any
        // habit editor the surface carried.
        self.subject_habit = None;
        vec![FlowOutcome::RefreshNow]
    }

    // -- create flow (D8) --------------------------------------------------

    /// The stack a create row opens in: the focused stack (Board) or
    /// the selected/subject stack; `None` on an empty board.
    #[must_use]
    pub fn create_target_stack(&self, state: &AppState) -> Option<StackId> {
        match self.view() {
            // No create row on the today view (chunk 10 D3): capture
            // is the create path there. The settings surface hosts no
            // create flow either (round-1 D5).
            // Review is subjectless like Today (#214 D-1).
            ViewId::Today
            | ViewId::Settings
            | ViewId::Review
            | ViewId::PlanNextDay
            // The status panel hosts no create flow (OP3: read-only).
            | ViewId::Status => None,
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

    /// The keyboard resize verb (r4 D8): resolves the target stack
    /// (focused → selected → first, `keyboard_resize_target`), steps
    /// its boundary against the neighbour via the pure
    /// [`crate::widths::step_column`] — the last column steals from
    /// its *left* neighbour — and materializes the stepped row into
    /// the overlay (the press path's one-code-path rule, plan §7.2).
    /// Returns `[StackWidthsChanged, RefreshNow]`, or `Vec::new()` when
    /// there is no stack, no row position, or no boundary to move (a
    /// one-column board is inert).
    pub fn resize_focused_stack(
        &mut self,
        state: &AppState,
        direction: i32,
        row: &crate::view_state::RowStep<'_>,
    ) -> Vec<FlowOutcome> {
        let Some(target) = self.keyboard_resize_target(state) else {
            return Vec::new();
        };
        let Some(board) = self.board else {
            return Vec::new();
        };
        let Some(index) = row.stacks.iter().position(|stack| *stack == target) else {
            return Vec::new();
        };
        let next = crate::widths::step_column(row.widths, index, direction, row.step, row.mins);
        if next.len() < 2 {
            return Vec::new();
        }
        for (stack, &width) in row.stacks.iter().zip(&next) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let stored = (f64::from(width) / row.scale).round() as i64;
            self.set_stack_width(board, *stack, u32::try_from(stored).unwrap_or(u32::MAX));
        }
        vec![FlowOutcome::StackWidthsChanged, FlowOutcome::RefreshNow]
    }

    /// `BeginCreate`: opens the row in insert mode (D5).
    pub fn begin_create(&mut self, stack: StackId) -> Vec<FlowOutcome> {
        self.write_flow(Flow::Create { stack });
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// The empty-board preset dispatch (r3 D5): one explicit-order
    /// `CreateStack` per preset stack plus a trailing `RefreshNow`.
    /// Orders are explicit 0..n — all commands are built before the
    /// engine applies any of them, so the canonical append arithmetic
    /// would hand them all the same order. The board-existence refusal
    /// mirrors `begin_create_stack` (the selection normalizes first; a
    /// board-less state yields an inert empty vec). Direct command
    /// dispatch: no text flow, no new flow-machine surface.
    pub fn apply_board_preset(
        &mut self,
        state: &AppState,
        preset: &crate::board_presets::BoardPreset,
    ) -> Vec<FlowOutcome> {
        self.normalize_board(state);
        let Some(board) = self.board else {
            return Vec::new();
        };
        preset
            .stacks
            .iter()
            .enumerate()
            .map(|(order, title)| {
                FlowOutcome::Command(StateCommand::CreateStack {
                    board,
                    title: (*title).to_owned(),
                    order: i64::try_from(order).unwrap_or(i64::MAX),
                })
            })
            .chain(std::iter::once(FlowOutcome::RefreshNow))
            .collect()
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
        self.write_flow(Flow::CreateStack);
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// Text commit (D9): the create row appends a task; the title
    /// editor updates it. An empty buffer commits nothing (the flow
    /// simply ends — the CLI owns title validation, the UI dispatches
    /// only well-formed commands). The flow always ends and normal
    /// mode returns.
    pub fn commit_edit(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let title = self.buffer().trim().to_owned();
        let flow = self.take_flow();
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
            Flow::EditTitle { subject } if !title.is_empty() => {
                // W-14 (#1048): the habit subject commits the same
                // sparse shape onto `UpdateHabit` (one flow grammar,
                // two subjects; the settle rule — commit non-empty,
                // discard empty — is subject-blind).
                let command = match subject {
                    EditSubject::Task(task) => StateCommand::UpdateTask {
                        id: task,
                        changes: TaskChanges {
                            title: Some(title),
                            ..TaskChanges::default()
                        },
                    },
                    EditSubject::Habit(habit) => StateCommand::UpdateHabit {
                        habit,
                        changes: taskboard_domain::HabitChanges {
                            title: Some(title),
                            ..taskboard_domain::HabitChanges::default()
                        },
                    },
                };
                outcomes.insert(0, FlowOutcome::Command(command));
            }
            // Round 2 (D18/D21): the board/stack lifecycle commits.
            Flow::CreateBoard if !title.is_empty() => {
                // The CLI's own omission default (UX R-4 D-6, #791):
                // the GUI create mints the exact payload the CLI's
                // create applies when `--color` is omitted — both
                // front ends produce identical creates. The recorded
                // colour-picker debt is retired by the board color
                // picker (UX R-4 D-1).
                outcomes.insert(
                    0,
                    FlowOutcome::Command(StateCommand::CreateBoard {
                        title,
                        color: taskboard_domain::Color::new(
                            taskboard_domain::entities::DEFAULT_BOARD_COLOR,
                        ),
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
        self.write_flow(Flow::None);
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
        let mut outcomes = if *self.flow() == Flow::None {
            Vec::new()
        } else {
            self.discard_edit()
        };
        self.replace_root(mode.view());
        self.focused_task = None;
        self.focused_habit = None;
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
    pub fn begin_capture(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        // The chain-aware Replace pre-pass (cross-surface friction
        // plan D-7, #712): the XOR rule only killed the TOP level, so
        // a walk child survived beneath a `BeginCapture` (the
        // resolver port's recorded deviation). Fold the flow chain
        // innermost-first — the D-H4 letter, `settle_foreign`'s shape
        // with its idempotence guard — then capture opens at depth 1.
        // Non-flow levels (over-views, the settings surface) are
        // untouched: the fold reads flow levels only.
        let mut outcomes = Vec::new();
        while let Some(index) = self.surfaces.top_flow_index() {
            let Some(flow) = self.surfaces.top_flow().cloned() else {
                break;
            };
            // The TEXT mask's precise boundary (the plan D-7): exactly
            // the buffer-commit family takes the fire-drill's
            // replace-without-committing rule — `discard_edit`, never
            // a commit, regardless of buffer content. Every other
            // kind takes its own settle rule (the walk closes
            // commandlessly; the pickers take the Cancelled end; the
            // armed strips cancel without dispatching).
            if matches!(
                flow,
                Flow::Create { .. }
                    | Flow::CreateStack
                    | Flow::CreateBoard
                    | Flow::EditTitle { .. }
                    | Flow::RenameStack { .. }
                    | Flow::RenameBoard { .. }
                    | Flow::EditDescription { .. }
            ) {
                outcomes.extend(self.discard_edit());
            } else {
                outcomes.extend(self.settle_by_rule(&flow, state));
            }
            if self.surfaces.top_flow_index() == Some(index) {
                // Unreachable with the real taxonomy (every rule ends
                // its level); the fold's own end keeps the walk total
                // — the `Surfaces::settle_fold` contract.
                let dropped = self.surfaces.end_top_flow();
                self.retired.extend(dropped);
            }
        }
        self.write_flow(Flow::Capture);
        outcomes.push(FlowOutcome::SetMode(Mode::Insert));
        outcomes.push(FlowOutcome::RefreshNow);
        outcomes
    }

    /// Return on the capture buffer (chunk 10 D8): a non-empty
    /// (trimmed) thought dispatches `CaptureTask { board: None }` —
    /// always the global Inbox, never a focused board — clears the
    /// buffer, and KEEPS the flow + Insert mode (commits-and-continues).
    /// An empty buffer closes instead.
    pub fn commit_capture(&mut self) -> Vec<FlowOutcome> {
        if *self.flow() != Flow::Capture {
            return Vec::new();
        }
        let thought = self.buffer().trim().to_owned();
        if thought.is_empty() {
            return self.close_capture();
        }
        self.set_buffer(String::new());
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
        if *self.flow() != Flow::Capture {
            return Vec::new();
        }
        self.discard_edit()
    }

    // -- the concentration overlay (#496) ----------------------------------

    /// `focus.toggle` when the overlay is closed: opens
    /// `Flow::FocusMode` over an empty switcher selection, replacing
    /// any in-progress flow WITHOUT committing it (the XOR rule —
    /// `begin_capture`'s opening; the workspace suspends beneath).
    pub fn open_focus_mode(&mut self) -> Vec<FlowOutcome> {
        self.write_flow(Flow::FocusMode { selected: 0 });
        vec![FlowOutcome::RefreshNow]
    }

    /// Walks the switcher cursor by `direction`, wrapping over the
    /// `visible` row count (the palette-cursor ring semantics; inert
    /// at zero rows).
    pub fn focus_mode_cursor(&mut self, direction: i32, visible: usize) -> Vec<FlowOutcome> {
        if let Some(Flow::FocusMode { selected }) = self.surfaces.top_flow_mut() {
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

    /// Every overlay exit that means close-without-effect: Escape, the
    /// toggle stroke, the scrim click. The `discard_edit` shape — the
    /// session (the open entry) is untouched; the runtime owns the
    /// phases, the overlay is presentation.
    pub fn close_focus_mode(&mut self) -> Vec<FlowOutcome> {
        self.write_flow(Flow::None);
        vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
    }

    // -- search palette (#348, chunk 9) ------------------------------------

    /// `PaletteSearch`: opens the palette in insert mode with an empty
    /// query (the `begin_create` opening shape). The empty query means
    /// no hits — the palette opens over an empty result list.
    pub fn open_palette(&mut self) -> Vec<FlowOutcome> {
        self.write_flow(Flow::Palette { selected: 0 });
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// Walks the palette cursor by `direction`, wrapping over the
    /// `visible` row count (the menu-cursor ring semantics; inert at
    /// zero rows). The keystroke path resets the cursor to 0 on every
    /// query change (a stale mid-list cursor on a fresh ranking is a
    /// lie); only a refresh-shrunk list clamps here.
    pub fn palette_cursor(&mut self, direction: i32, visible: usize) -> Vec<FlowOutcome> {
        if let Some(Flow::Palette { selected }) = self.surfaces.top_flow_mut() {
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
        self.write_flow(Flow::None);
        vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
    }

    // -- time-entries overlay (r4 chunk 10, D-TS2) -------------------------

    /// `task.track-entries`: opens the list surface over the viewed
    /// task. The kiosk refusal and the no-subject refusal are the
    /// *adapter's* gate (dispatch-time gating, D-TS8); this arm opens
    /// whenever asked.
    pub fn open_track_entries(&mut self, task: TaskId) -> Vec<FlowOutcome> {
        // The entries overlay is a foreign settle (D-2 site 4).
        self.clear_rule_cursor();
        self.write_flow(Flow::TrackEntries { task });
        vec![FlowOutcome::RefreshNow]
    }

    /// Row-activate (or the add button): mounts the form as the
    /// list's CHILD (N1) — edit seeds the entry (`Some`), add does
    /// not (`None`). The structural admission rule lives here: the
    /// list's own handler is the only opener, the form mounts only
    /// over its own live list, and the duplicate-kind refusal is the
    /// defensive no-op (D7).
    pub fn open_track_entry_form(
        &mut self,
        task: TaskId,
        seed: Option<taskboard_domain::TimeEntryId>,
    ) -> Vec<FlowOutcome> {
        if !self.chain_has_flow(
            |flow| matches!(flow, Flow::TrackEntries { task: subject, .. } if *subject == task),
        ) {
            return Vec::new();
        }
        let _ = self.mount_child(Flow::TrackEntryForm { task, seed });
        vec![FlowOutcome::RefreshNow]
    }

    /// The form's completion (N1): the payload-less `Completed` — the
    /// parent list ignores it and stays (the commit's multi-add
    /// posture). Inert without the form child.
    pub fn complete_track_entry_form(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        if !matches!(self.flow(), Flow::TrackEntryForm { .. }) {
            return Vec::new();
        }
        self.end_top_flow_with(state, crate::surfaces::GuestExit::Completed(None))
    }

    /// Every overlay exit that means close-without-effect: the
    /// outside click, the view leaving, the subject dying. The
    /// `close_palette` shape.
    pub fn close_track_entries(&mut self) -> Vec<FlowOutcome> {
        self.write_flow(Flow::None);
        vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
    }

    // -- the labels overlay (UX R-3 D-2, #789) ---------------------------

    /// `label.manage`: opens the labels overlay over the viewed
    /// task. The kiosk refusal and the no-subject refusal are the
    /// *adapter's* gate (dispatch-time gating — the entries
    /// overlay's D-TS8 shape); this arm opens whenever asked. A held
    /// foreign flow settles first (the resolver's overlay class).
    pub fn open_labels(&mut self, subject: LabelOverlaySubject) -> Vec<FlowOutcome> {
        // A task-mounted labels overlay is a foreign settle (D-2
        // site 4); the board mount never holds a rule cursor.
        self.clear_rule_cursor();
        self.write_flow(Flow::Labels { subject });
        vec![FlowOutcome::RefreshNow]
    }

    /// `label.manage` from the board surface (tag-color plan D-3,
    /// US-2): the roster-only mount over the *selected* board — the
    /// same source the board context menu's other rows address
    /// (`normalize_board` + `self.board`, the `begin_board_color`
    /// shape). Inert without a board.
    pub fn open_labels_board(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        self.normalize_board(state);
        let Some(board) = self.board else {
            return Vec::new();
        };
        self.open_labels(LabelOverlaySubject::Board(board))
    }

    /// The overlay's mount facts (tag-color plan D-4): the flow's
    /// subject addresses the current surface — the detail mount
    /// matches the viewed task, the board mount the selected board (a
    /// dead subject renders closed; the reconcile pass drops the flow
    /// anyway).
    #[must_use]
    pub fn labels_mount_open(&self) -> bool {
        match self.flow() {
            Flow::Labels { subject } => match subject {
                LabelOverlaySubject::Task(task) => self.subject_task() == Some(*task),
                LabelOverlaySubject::Board(board) => self.board == Some(*board),
            },
            _ => false,
        }
    }

    /// The board-mount half only (tag-color plan D-4): the roster-only
    /// mount over the selected board — the board surface's mount
    /// condition.
    #[must_use]
    pub fn labels_board_mount_open(&self) -> bool {
        matches!(
            self.flow(),
            Flow::Labels { subject: LabelOverlaySubject::Board(board) }
                if self.board == Some(*board)
        )
    }

    /// The flow's subject, when the labels overlay holds (tag-color
    /// plan D-8): the roster/rows/id-table passes read it.
    #[must_use]
    pub fn labels_subject(&self) -> Option<LabelOverlaySubject> {
        match self.flow() {
            Flow::Labels { subject } => Some(*subject),
            _ => None,
        }
    }

    /// Every overlay exit that means close-without-effect: the
    /// Escape ladder's last rung, the outside click, the view
    /// leaving, the subject dying (the `close_track_entries` shape).
    pub fn close_labels(&mut self) -> Vec<FlowOutcome> {
        self.write_flow(Flow::None);
        vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
    }

    // -- the review walk (DR-7 D-J9, #638; plan D-W2) ----------------------

    /// `review.open` / the Today card: opens the walk over
    /// `(cadence, reference)` at the Entry stage — the guidance stage
    /// is never skipped (J4 D-1: it is the guidance the operator asked
    /// for); the row stages' empty-skip law applies from Entry's Next
    /// via [`walk_first_open_stage`] (D-3). Replaces any in-progress
    /// flow WITHOUT committing it (the XOR rule — `begin_capture`'s
    /// opening).
    pub fn open_review_walk(
        &mut self,
        cadence: ReviewCadence,
        reference: NaiveDate,
    ) -> Vec<FlowOutcome> {
        self.write_flow(Flow::ReviewWalk {
            cadence,
            reference,
            stage: WalkStage::Entry,
            cursor: 0,
            tally: WalkTally::default(),
        });
        vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
    }

    /// Every walk exit that means close-without-effect: the Entry
    /// silent exit (nothing decided, nothing typed — J4 D-6), the
    /// abort-confirm accept, the completion summary's exit, the
    /// post-`Record` legacy shape, a foreign settle. No command rides
    /// it (the `close_palette` shape).
    pub fn close_review_walk(&mut self) -> Vec<FlowOutcome> {
        self.write_flow(Flow::None);
        vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
    }

    /// The current stage's row count under `rows` (the walk's own row
    /// stages only; every other stage shows no walk rows).
    const fn walk_stage_rows(stage: WalkStage, rows: WalkRows) -> usize {
        match stage {
            WalkStage::Inbox => rows.inbox,
            WalkStage::CarryForward => rows.carry,
            WalkStage::Entry | WalkStage::Planner | WalkStage::Prompts | WalkStage::Complete => 0,
        }
    }

    /// `Up`/`Down` in the walk: moves the cursor by `direction` inside
    /// the stage's rows, clamped at both ends (no wrap — the ends hold;
    /// the row verbs are what pass the last row). Inert on Prompts
    /// (no rows) and outside the walk.
    pub fn review_walk_cursor(&mut self, direction: i32, rows: WalkRows) -> Vec<FlowOutcome> {
        let Some(Flow::ReviewWalk { stage, cursor, .. }) = self.walk_flow_mut() else {
            return Vec::new();
        };
        let len = Self::walk_stage_rows(*stage, rows);
        if len == 0 {
            return Vec::new();
        }
        let next = i64::try_from(*cursor).unwrap_or(0) + i64::from(direction);
        let clamped = next.clamp(0, i64::try_from(len - 1).unwrap_or(0));
        *cursor = usize::try_from(clamped).unwrap_or(0);
        vec![FlowOutcome::RefreshNow]
    }

    /// A row verb's cursor step (`Keep in inbox`, `Leave as is`, and
    /// the post-commit step of `Move to tomorrow`/the date pick): the
    /// cursor moves to the next row; passing the last row advances the
    /// stage (the Keep-on-the-last-row rule). The armed child (the
    /// File-to picker) dies with its own commit — the row step carries
    /// no sub-state.
    pub fn review_walk_next_row(&mut self, rows: WalkRows) -> WalkStep {
        let Some(Flow::ReviewWalk { stage, cursor, .. }) = self.walk_flow_mut() else {
            return WalkStep::Inert;
        };
        // The armed child dies with the row step — the adapter's row
        // verbs end it `Cancelled` before stepping (the flattened
        // next-row clear; the child's death is the clear).
        let len = Self::walk_stage_rows(*stage, rows);
        if *cursor + 1 < len {
            *cursor += 1;
            return WalkStep::Advanced;
        }
        self.advance_review_walk(rows)
    }

    /// `review.walk-advance` (the stage Next control, J4 US-1/S2):
    /// Entry → the first open row stage / Prompts (D-3's skip law);
    /// Inbox → the first open stage at/after Carry; Carry → Prompts
    /// (the planner is opt-in — the verb, never an auto-advance);
    /// Prompts/Complete have no Next (their exits are the commit verbs
    /// / the completion control). Inert outside the walk.
    pub fn advance_review_walk(&mut self, rows: WalkRows) -> WalkStep {
        let Some(Flow::ReviewWalk { stage, cursor, .. }) = self.walk_flow_mut() else {
            return WalkStep::Inert;
        };
        match stage {
            WalkStage::Entry | WalkStage::Inbox | WalkStage::CarryForward => {
                // The advance targets the machine's NEXT stage, then
                // the skip law normalizes it (D-3): Entry → Inbox,
                // Inbox → Carry, Carry → Prompts — each landing on its
                // first open shape. The planner is never a skip
                // destination (the verb, D-6).
                let from = match stage {
                    WalkStage::Entry => WalkStage::Inbox,
                    WalkStage::Inbox => WalkStage::CarryForward,
                    // Carry forward and every later stage alike: the
                    // Next beyond Carry is Prompts (the planner is the
                    // verb, D-6).
                    _ => WalkStage::Prompts,
                };
                *stage = walk_first_open_stage(from, rows);
                *cursor = 0;
                WalkStep::Advanced
            }
            WalkStage::Planner | WalkStage::Prompts | WalkStage::Complete => WalkStep::Inert,
        }
    }

    /// The Prev control (J4 US-1/S2, the dialog button row): steps
    /// back one *visited* stage — Prompts → Carry/Inbox (the row
    /// stages behind, first non-empty from behind), the row stages →
    /// behind/close. Back out of the Planner stage is the child's
    /// Cancelled rung (D-6), never this ladder. Inert outside the
    /// walk; Complete is terminal behind the exit control.
    pub fn back_review_walk(&mut self, rows: WalkRows) -> Vec<FlowOutcome> {
        let Some(Flow::ReviewWalk { stage, cursor, .. }) = self.walk_flow_mut() else {
            return Vec::new();
        };
        match stage {
            WalkStage::Prompts => {
                *stage = if rows.carry > 0 {
                    WalkStage::CarryForward
                } else if rows.inbox > 0 {
                    WalkStage::Inbox
                } else {
                    WalkStage::Entry
                };
                *cursor = 0;
                vec![FlowOutcome::RefreshNow]
            }
            WalkStage::CarryForward | WalkStage::Inbox => {
                if matches!(stage, WalkStage::CarryForward) && rows.inbox > 0 {
                    *stage = WalkStage::Inbox;
                } else {
                    *stage = WalkStage::Entry;
                }
                *cursor = 0;
                vec![FlowOutcome::RefreshNow]
            }
            WalkStage::Entry | WalkStage::Planner | WalkStage::Complete => Vec::new(),
        }
    }

    /// The plan-next-day choice out of Carry (J4 US-3/S1): sets the
    /// Planner stage and mounts the embedded `ReviewPlanner` child
    /// level (D-2). Inert outside Carry or when the child kind is
    /// already live (the defensive D7 no-op). The walk stays open
    /// beneath; back ends the child `Cancelled` and returns to Carry.
    pub fn walk_open_planner(&mut self) -> WalkStep {
        if !matches!(
            self.walk_flow().as_ref(),
            Some(Flow::ReviewWalk {
                stage: WalkStage::CarryForward,
                ..
            })
        ) {
            return WalkStep::Inert;
        }
        if let Err(crate::surfaces::AlreadyInChain) = self.mount_child(Flow::ReviewPlanner) {
            return WalkStep::Inert;
        }
        if let Some(Flow::ReviewWalk { stage, tally, .. }) = self.walk_flow_mut() {
            *stage = WalkStage::Planner;
            tally.planner = true;
        }
        WalkStep::Planner
    }

    /// A tally bump (D-8): the verb seams note what the session did —
    /// the counts ride the flow and are presented at the completion
    /// stage. Inert outside the walk.
    pub fn walk_tally_note(&mut self, note: WalkTallyNote) {
        let Some(Flow::ReviewWalk { tally, .. }) = self.walk_flow_mut() else {
            return;
        };
        match note {
            WalkTallyNote::Filed => tally.filed += 1,
            WalkTallyNote::Moved => tally.moved += 1,
            WalkTallyNote::Left => tally.left += 1,
            WalkTallyNote::Recorded => tally.recorded = true,
        }
    }

    /// The walk's stage (read, for the adapter's rung arms).
    #[must_use]
    pub fn walk_stage(&self) -> Option<WalkStage> {
        match self.walk_flow().as_ref() {
            Some(Flow::ReviewWalk { stage, .. }) => Some(*stage),
            _ => None,
        }
    }

    /// `File to…` (N3a): mounts the File-to child over the walk with
    /// the first candidate armed — the child is never minted unarmed
    /// (the arm refuses empty candidate lists). Re-request re-arms to
    /// the first candidate (the flattened model's reset); inert
    /// without candidates, outside the Inbox stage, or outside the
    /// walk. The duplicate-kind refusal is the defensive no-op (D7 —
    /// unreachable: the walk arms at most one child).
    pub fn review_walk_file(&mut self, candidates: &[StackId]) -> Vec<FlowOutcome> {
        let Some(first) = candidates.first() else {
            return Vec::new();
        };
        if let Some(armed) = self.surfaces.top_flow_mut().and_then(|flow| match flow {
            Flow::WalkFilePick { armed } => Some(armed),
            _ => None,
        }) {
            *armed = Some(*first);
            return vec![FlowOutcome::RefreshNow];
        }
        if !matches!(
            self.walk_flow().as_ref(),
            Some(Flow::ReviewWalk {
                stage: WalkStage::Inbox,
                ..
            })
        ) {
            return Vec::new();
        }
        let _ = self.mount_child(Flow::WalkFilePick {
            armed: Some(*first),
        });
        vec![FlowOutcome::RefreshNow]
    }

    /// The picker's left/right: moves the armed candidate by
    /// `direction` over `candidates`, wrapping (the menu-cursor ring).
    /// Inert unless the picker is armed.
    pub fn review_walk_file_target(
        &mut self,
        direction: i32,
        candidates: &[StackId],
    ) -> Vec<FlowOutcome> {
        let Some(Flow::WalkFilePick { armed: Some(armed) }) = self.surfaces.top_flow_mut() else {
            return Vec::new();
        };
        if candidates.is_empty() {
            return Vec::new();
        }
        let current = candidates.iter().position(|s| s == armed).unwrap_or(0);
        *armed = candidates[wrapped_index(current, direction, candidates.len())];
        vec![FlowOutcome::RefreshNow]
    }

    /// `Pick a date…` (N3b): mounts the walk's date child over the
    /// live walk; inert outside the Carry forward stage. The typed
    /// text lives in the child's `WalkDate` family; a foreign settle
    /// ends it `Cancelled`, discarding half-typed text exactly as the
    /// flattened walk always did (the Q3 ruling, D5). The
    /// duplicate-kind refusal is the defensive no-op (D7).
    pub fn review_walk_pick_date(&mut self) -> Vec<FlowOutcome> {
        if !matches!(
            self.walk_flow().as_ref(),
            Some(Flow::ReviewWalk {
                stage: WalkStage::CarryForward,
                ..
            })
        ) {
            return Vec::new();
        }
        let _ = self.mount_child(Flow::WalkDatePick);
        vec![FlowOutcome::RefreshNow]
    }

    /// The refresh-time normalization (the palette's shrunk-list
    /// clamp, generalized): the cursor clamps against the pushed row
    /// count; a row stage whose rows vanished advances structurally
    /// through the symmetric skip law (D-3 — an emptied Inbox moves to
    /// Carry, an emptied Carry to Prompts, via
    /// [`walk_first_open_stage`]); an armed picker whose candidate
    /// died disarms. Pure — called by the push pass with the rows it
    /// just derived; no command, no repaint request (the pass is
    /// already repainting).
    pub fn clamp_review_walk(&mut self, state: &AppState, rows: WalkRows, candidates: &[StackId]) {
        // The armed candidate's death ends the child `Cancelled` —
        // BEFORE the skip law, so no one-pass window can show the next
        // stage with a stale armed-child mirror (R3). The exit is
        // ignored by the walk (the disarm was silent).
        if self
            .walk_file_armed()
            .is_some_and(|armed| !candidates.contains(&armed))
        {
            let _ = self.end_top_flow_with(state, crate::surfaces::GuestExit::Cancelled);
        }
        let Some(Flow::ReviewWalk { stage, cursor, .. }) = self.walk_flow_mut() else {
            return;
        };
        if matches!(stage, WalkStage::Inbox | WalkStage::CarryForward) {
            let first = walk_first_open_stage(*stage, rows);
            if first != *stage {
                *stage = first;
                *cursor = 0;
                return;
            }
            let len = Self::walk_stage_rows(*stage, rows);
            *cursor = (*cursor).min(len.saturating_sub(1));
        }
    }

    // -- the exit protocol (W5b D-H3, chunk 3) ------------------------------

    /// Ends the top flow level, delivering `exit` to the level now
    /// beneath it (its `on_child_exit`). The chain truncates to the
    /// top flow level's index — the subtree goes with it, the parent's
    /// own ancestors stay. **Every child exit rides this one seam**:
    /// the retire drain (`truncate_to`/`end_top_flow`/`settle_fold`/
    /// `reconcile`) never delivers an exit, so subject death (Q4)
    /// truncates silently by construction. Delivery reads the parent's
    /// flow *after* truncation — the parent is `top_flow()` of the
    /// shortened chain, so a payload can never cross an intermediate
    /// level. The adapter applies the returned outcomes.
    pub fn end_top_flow_with(
        &mut self,
        state: &AppState,
        exit: crate::surfaces::GuestExit,
    ) -> Vec<FlowOutcome> {
        let Some(index) = self.surfaces.top_flow_index() else {
            return Vec::new();
        };
        let dropped = self.surfaces.truncate_to(index);
        self.retired.extend(dropped);
        self.on_child_exit(state, exit)
    }

    /// The parent's consumption arm (D-H3), matched over the flow
    /// level beneath the ended subtree. Default: ignore — a parent
    /// with no arm for the exit (or for a payload it did not ask for)
    /// returns no outcomes.
    fn on_child_exit(
        &mut self,
        state: &AppState,
        exit: crate::surfaces::GuestExit,
    ) -> Vec<FlowOutcome> {
        // The planner child's exit contract (D-2): leaving the child
        // advances the walk — `Cancelled` (back, J4 US-3/S1) returns
        // to Carry, `Completed(None)` (done, US-3/S2) advances to
        // Prompts. Only the planner child can be live while the stage
        // is Planner (the XOR children arm at the row stages), so the
        // stage alone discriminates the leg.
        if self.walk_stage() == Some(WalkStage::Planner) {
            let next = match exit {
                crate::surfaces::GuestExit::Cancelled => WalkStage::CarryForward,
                crate::surfaces::GuestExit::Completed(_) => WalkStage::Prompts,
            };
            if let Some(Flow::ReviewWalk { stage, cursor, .. }) = self.walk_flow_mut() {
                *stage = next;
                *cursor = 0;
            }
            return vec![FlowOutcome::RefreshNow];
        }
        let crate::surfaces::GuestExit::Completed(Some(payload)) = exit else {
            return Vec::new();
        };
        match payload {
            crate::surfaces::LevelResult::Stack(stack) => self.walk_files_cursor_row(state, stack),
            crate::surfaces::LevelResult::Date(instant) => {
                // The entries form consumes the picked day (N2): the
                // same write the day-click made — the local date of
                // the noon-anchored instant.
                if self.track_form().is_some() {
                    let text = instant
                        .with_timezone(&chrono::Local)
                        .date_naive()
                        .format("%Y-%m-%d")
                        .to_string();
                    if let Some(form) = self.track_form_mut() {
                        form.date_text = text;
                    }
                    vec![FlowOutcome::RefreshNow]
                } else {
                    Vec::new()
                }
            }
        }
    }

    /// The walk parent's `Stack` consumption (N3a): today's
    /// `FileCommit` body — the `MoveTask` append for the cursor row
    /// into `stack`, the tally's `filed` bump (D-8), then the row
    /// advance (the child's death is the flattened sub-state clear).
    /// R2's order is preserved by construction: payload consumed →
    /// command built → next row.
    fn walk_files_cursor_row(&mut self, state: &AppState, stack: StackId) -> Vec<FlowOutcome> {
        let Flow::ReviewWalk { cursor, .. } = self.flow().clone() else {
            return Vec::new();
        };
        let inbox = crate::presentation::walk_inbox_rows(state);
        let Some(task) = inbox.get(cursor).copied() else {
            return Vec::new();
        };
        let command = append_move_command(state, task, stack);
        let mut outcomes = vec![command];
        self.walk_tally_note(WalkTallyNote::Filed);
        let rows = WalkRows {
            inbox: inbox.len(),
            carry: 0,
        };
        if self.review_walk_next_row(rows) == WalkStep::Advanced {
            outcomes.push(FlowOutcome::RefreshNow);
        }
        outcomes
    }

    /// The Cancelled rung (D-H4's ladder generalization): ends exactly
    /// the top flow level `Cancelled` when it sits above another flow
    /// level; `None` when the top flow level is at depth 1 (no parent
    /// flow — the caller's own close rung applies). Inert without a
    /// flow level. The rung's visible shape is today's step-back: the
    /// refresh-only outcome set (every current parent ignores a
    /// `Cancelled` exit; the repaint is what unmounts the child's
    /// surface).
    pub fn cancel_top_child(&mut self, state: &AppState) -> Option<Vec<FlowOutcome>> {
        match self.surfaces.top_flow_index() {
            Some(index) if index > 1 => {}
            _ => return None,
        }
        let mut outcomes = self.end_top_flow_with(state, crate::surfaces::GuestExit::Cancelled);
        outcomes.push(FlowOutcome::RefreshNow);
        Some(outcomes)
    }

    // -- plan-next-day placement (#622, D-3) -------------------------------

    /// `plan.begin-place`: arms the placement form over the candidate
    /// row's task (re-opening on an already-placed row edits that box
    /// — the same funnel seeds from the row's instant adapter-side).
    pub fn open_plan_place(&mut self, task: TaskId) -> Vec<FlowOutcome> {
        self.write_flow(Flow::PlanPlace { task });
        vec![FlowOutcome::RefreshNow]
    }

    /// The placement form as the embedded planner's own child (J4
    /// D-2): pushed ABOVE the live `ReviewPlanner` level — replacing
    /// the top flow would kill the walk's planner step. The commit /
    /// discard tails end the child through `write_flow(None)` (the
    /// truncate), handing the level back to the planner.
    pub fn open_plan_place_child(&mut self, task: TaskId) -> Vec<FlowOutcome> {
        if let Err(crate::surfaces::AlreadyInChain) = self.mount_child(Flow::PlanPlace { task }) {
            return Vec::new();
        }
        vec![FlowOutcome::RefreshNow]
    }

    /// The placement form's discard (the Escape ladder's form → pane
    /// rung): ends the flow, keeps the surface.
    pub fn close_plan_place(&mut self) -> Vec<FlowOutcome> {
        if matches!(self.flow(), Flow::PlanPlace { .. }) {
            self.write_flow(Flow::None);
            vec![FlowOutcome::RefreshNow]
        } else {
            Vec::new()
        }
    }

    /// Ends an open placement flow whose subject was `task` (the
    /// commit/discard tails call this after their own work; the
    /// adapter owns the parse, so the command is built there).
    pub fn end_plan_place(&mut self) {
        if matches!(self.flow(), Flow::PlanPlace { .. }) {
            self.write_flow(Flow::None);
        }
    }

    /// `plan.remove-box` (#622, D-3): routes `UpdateTask { scheduled:
    /// Some(None) }` — the unplace write, one honest command; an open
    /// placement flow on the same subject ends with it. No new write
    /// vocabulary: this is the landed `UpdateTask` mask.
    #[must_use]
    pub fn remove_plan_box(&mut self, task: TaskId) -> Vec<FlowOutcome> {
        self.end_plan_place();
        vec![
            FlowOutcome::Command(StateCommand::UpdateTask {
                id: task,
                changes: taskboard_domain::TaskChanges {
                    scheduled: Some(None),
                    ..taskboard_domain::TaskChanges::default()
                },
            }),
            FlowOutcome::RefreshNow,
        ]
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
        self.replace_root(ViewId::StackDetail);
        outcomes
    }

    /// The jump-to matrix (design D12): retarget board/stack/focus per
    /// hit kind against the live snapshot, then end the flow. A stale
    /// target (the entity died between push and invoke) is inert —
    /// close only, no retarget.
    pub fn palette_jump(
        &mut self,
        state: &AppState,
        target: SearchTarget,
        today: chrono::NaiveDate,
    ) -> Vec<FlowOutcome> {
        let mut outcomes = match target {
            SearchTarget::Task(task_id) => {
                // The stale check keeps the close-only rule; the live
                // retarget IS `show_task` (round-3 D23: one
                // implementation — board, stack, focus — two dispatch
                // sites, never a parallel path).
                let live = state
                    .tasks
                    .get(&task_id)
                    .is_some_and(|t| state.stacks.contains_key(&t.stack));
                if !live {
                    return self.close_palette();
                }
                self.show_task(state, task_id)
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
                self.replace_root(ViewId::StackDetail);
                out
            }
            SearchTarget::Board(board_id) => {
                if !state.boards.contains_key(&board_id) {
                    return self.close_palette();
                }
                self.select_board(state, board_id)
            }
            // W-10 (#1218): the search row's History affordance — the
            // resolution is id-exact (ids need no title leg), so the
            // tombstone matches by construction (the CLI twin's
            // tombstone-tolerant resolver shape). The dialog is the
            // landed read-role mount; the tombstoned subject projects
            // the retained log.
            SearchTarget::Habit(habit_id) => self.open_habit_history(habit_id, today),
        };
        // The flow always ends (close_palette's ending; select_board
        // already asked for the repaint).
        self.write_flow(Flow::None);
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
        // The armed slot (UX-R8 D-1): the gap *after* the task's
        // current position in its own stack. A vanished task leaves
        // the flow unarmed — the `focused_task` guard above; the
        // `position` fallback only fires on a focus/tables race that
        // `reconcile` repairs (the gap-0 seed is the safe side).
        let slot = state
            .tasks_in_stack(target)
            .iter()
            .position(|t| t.id == task)
            .map_or(0, |i| i + 1);
        self.write_flow(Flow::Move { task, target, slot });
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

    /// The flattened slot enumeration (UX-R8 D-1, US-1/AC-3): every
    /// live stack's insertion gaps — stacks in [`Self::move_ring`]
    /// order, positions top→bottom within a stack, N+1 slots per
    /// stack (a single-task stack offers exactly two). Computed on
    /// demand from the live state, never stored — refresh-survival by
    /// re-derivation, the ring's existing law.
    #[must_use]
    pub fn move_slots(state: &AppState) -> Vec<(StackId, usize)> {
        Self::move_ring(state)
            .into_iter()
            .flat_map(|stack| {
                let len = state.tasks_in_stack(stack).len();
                (0..=len).map(move |gap| (stack, gap))
            })
            .collect()
    }

    /// Focus next/prev during a move flow: advance one insertion slot
    /// per stroke through the flattened [`Self::move_slots`] sequence
    /// (wrap — UX-R8 D-1's walk; in compact this is exactly the
    /// switcher highlighting).
    pub fn cycle_move_target(&mut self, state: &AppState, direction: i32) -> Vec<FlowOutcome> {
        let Flow::Move { target, slot, .. } = self.flow().clone() else {
            return Vec::new();
        };
        let slots = Self::move_slots(state);
        if slots.is_empty() {
            return Vec::new();
        }
        let current = slots
            .iter()
            .position(|(stack, gap)| *stack == target && *gap == slot)
            .map_or(0, |i| wrapped_index(i, direction, slots.len()));
        let (stack, gap) = slots[current];
        if let Some(Flow::Move { target, slot, .. }) = self.surfaces.top_flow_mut() {
            *target = stack;
            *slot = gap;
        }
        vec![FlowOutcome::RefreshNow]
    }

    /// The directional left/right kept arm during a move flow (J3
    /// US-2, round 9 chunk 9): hop the drop *column* one stack over,
    /// keeping the picked gap index (clamped to the target stack's
    /// gap count) — the horizontal geometry of the same flow. Clamp,
    /// not wrap (the ends hold: the column walk's edge is the board's
    /// edge); the move survives every hop.
    pub fn cycle_move_stack_column(
        &mut self,
        state: &AppState,
        direction: i32,
    ) -> Vec<FlowOutcome> {
        let Flow::Move { target, slot, .. } = self.flow().clone() else {
            return Vec::new();
        };
        let stacks = self.stack_ids(state);
        let Some(current) = stacks.iter().position(|s| *s == target) else {
            return Vec::new(); // a vanished target: reconcile repairs first
        };
        let next = usize::try_from(i64::try_from(current).unwrap_or(0) + i64::from(direction))
            .ok()
            .filter(|idx| *idx < stacks.len());
        let Some(next) = next else {
            return Vec::new(); // the edge holds
        };
        let target = stacks[next];
        // The picked row index survives the hop; the target's gap
        // count bounds it (a shorter column clamps the gap — the
        // append gap is index `len`).
        let gap_count = state.tasks_in_stack(target).len().saturating_add(1);
        let slot = slot.min(gap_count.saturating_sub(1));
        if let Some(Flow::Move {
            target: t, slot: s, ..
        }) = self.surfaces.top_flow_mut()
        {
            *t = target;
            *s = slot;
        }
        vec![FlowOutcome::RefreshNow]
    }

    /// `Activate` during a move flow: confirm (UX-R8 D-1, re-signed
    /// B3 2026-11-24 #1247). A SAME-stack aim mints the engine-side
    /// gap-index reorder — `StateCommand::TaskReorder` at the aimed
    /// gap (the engine's in-place gaps are the accepted no-op the
    /// walked-back slot dispatches into; typed, no presentation-side
    /// special case). A cross-stack aim keeps `MoveTask`: gap `0` is
    /// the first-position mint (`slot_order`'s surviving end arm),
    /// anything else appends (interior cross-stack insertion is out of
    /// the reorder law — D-2). The one mint this flow speaks.
    pub fn confirm_move(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Flow::Move { task, target, slot } = self.flow().clone() else {
            return Vec::new();
        };
        let command = FlowOutcome::Command(move_aim_command(state, task, target, Some(slot)));
        self.write_flow(Flow::None);
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
        if *self.flow() != Flow::None {
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
        self.write_flow(Flow::DragMove {
            task,
            target,
            gap: None,
        });
        vec![FlowOutcome::RefreshNow]
    }

    /// The pointer's column report (r2 D-E): a `DropArea`'s hover
    /// address, resolved to its stack by the adapter. `RefreshNow` only
    /// on an actual change; inert without the drag flow.
    pub fn drag_target(&mut self, _state: &AppState, stack: StackId) -> Vec<FlowOutcome> {
        let Some(Flow::DragMove { target, gap, .. }) = self.surfaces.top_flow_mut() else {
            return Vec::new();
        };
        if *target == stack && gap.is_none() {
            return Vec::new();
        }
        *target = stack;
        // Column-level aim: the gap resets (the gap zone's report
        // re-arms it, UX-R8 D-2).
        *gap = None;
        vec![FlowOutcome::RefreshNow]
    }

    /// The pointer's gap report (UX-R8 D-2, US-2): a gap-zone
    /// `DropArea`'s hover address, resolved to (stack, gap) by the
    /// adapter. Updates the drag flow's target *and* armed gap;
    /// `RefreshNow` only on an actual change; inert without the drag
    /// flow.
    pub fn drag_gap(&mut self, _state: &AppState, stack: StackId, gap: usize) -> Vec<FlowOutcome> {
        let Some(Flow::DragMove {
            target, gap: armed, ..
        }) = self.surfaces.top_flow_mut()
        else {
            return Vec::new();
        };
        if *target == stack && *armed == Some(gap) {
            return Vec::new();
        }
        *target = stack;
        *armed = Some(gap);
        vec![FlowOutcome::RefreshNow]
    }

    /// The drop (r2 D-F; re-signed B3 2026-11-24 #1247): the same
    /// command the keyboard flow confirms — both funnel through
    /// [`move_aim_command`], the one construction site (equivalence by
    /// construction; the pin test compares the commands). A same-stack
    /// drop mints the engine-side `TaskReorder` gap; cross-stack keeps
    /// `MoveTask` (D-2). The *in-place* confirm — the confirmed gap on
    /// the dragged card's own position (gap `idx` or `idx + 1` of its
    /// own stack) — dispatches nothing and ends the flow (US-2/AC-2;
    /// the gesture is silent too, so a release back on the card lands
    /// no ghosting refresh).
    pub fn confirm_drag_move(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Flow::DragMove { task, target, gap } = self.flow().clone() else {
            return Vec::new();
        };
        let own_index = state
            .tasks_in_stack(target)
            .iter()
            .position(|t| t.id == task);
        if gap.is_some() && own_index.is_some_and(|idx| gap == Some(idx) || gap == Some(idx + 1)) {
            self.write_flow(Flow::None);
            return vec![FlowOutcome::RefreshNow];
        }
        // The multi-drag branch (UX-R8 D-6, US-4/AC-1): a drag armed
        // from any selected member moves the set — one `MoveTask` per
        // member in anchor order, relative order preserved. A
        // single-task selection behaves exactly as today.
        let members: Vec<TaskId> = if !self.selection.is_empty() && self.selection.contains(&task) {
            self.selection_anchor_order(state)
        } else {
            vec![task]
        };
        let commands: Vec<FlowOutcome> = if members.len() > 1 {
            multi_move_commands(state, &members, target, gap, &ViewState::move_ring(state))
                .into_iter()
                .map(FlowOutcome::Command)
                .collect()
        } else {
            vec![FlowOutcome::Command(move_aim_command(
                state, task, target, gap,
            ))]
        };
        self.write_flow(Flow::None);
        let mut outcomes = commands;
        outcomes.push(FlowOutcome::RefreshNow);
        outcomes
    }

    /// `RequestMove` during a move flow: cancel, nothing dispatched.
    pub fn cancel_move(&mut self) -> Vec<FlowOutcome> {
        if *self.flow() == Flow::None {
            return Vec::new();
        }
        self.write_flow(Flow::None);
        vec![FlowOutcome::RefreshNow]
    }

    /// The LOCAL settle (D-H4): the top flow level only, by its kind
    /// rule. Idempotent without a flow level (the double-settle
    /// guard).
    pub fn settle_flow(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Some(flow) = self.surfaces.top_flow().cloned() else {
            return Vec::new();
        };
        self.settle_by_rule(&flow, state)
    }

    /// The FOREIGN settle (D-H4): the innermost-first fold over the
    /// flow levels, each by its own kind rule — the trigger side of
    /// the leaving effects, the modal settle, and the resolver's
    /// CancelOnly/SettleThenRun/Replace cells. Idempotent: no flow
    /// levels fold to empty, and every rule ends its level, so a
    /// second fold settles nothing.
    pub fn settle_foreign(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let mut outcomes = Vec::new();
        while let Some(index) = self.surfaces.top_flow_index() {
            let Some(flow) = self.surfaces.top_flow().cloned() else {
                break;
            };
            outcomes.extend(self.settle_by_rule(&flow, state));
            if self.surfaces.top_flow_index() == Some(index) {
                // Unreachable with the real taxonomy (every kind rule
                // ends its level); the fold's own end keeps the walk
                // total — the `Surfaces::settle_fold` contract.
                let dropped = self.surfaces.end_top_flow();
                self.retired.extend(dropped);
            }
        }
        outcomes
    }

    /// The per-kind settle rule (round-3 W-SM-b, plan §2): the one
    /// pure taxonomy every settle trigger shares — L1 focus loss, L2
    /// pointer seams, and the leaving effects — so the triggers
    /// cannot diverge. TEXT flows commit a trimmed non-empty
    /// buffer and discard an empty one (F7 dies); the due editor
    /// clean-closes (never commits on an unrelated settle — the r2
    /// D25 rule; the only commit path stays the `commit_due`
    /// funnel); move/drag flows cancel; the modals take their own
    /// close paths.
    fn settle_by_rule(&mut self, flow: &Flow, state: &AppState) -> Vec<FlowOutcome> {
        match flow.clone() {
            Flow::None => Vec::new(),
            // The placement form's settle is its own close path (the
            // overlay cells' shape): a foreign settle discards it
            // without effect.
            Flow::PlanPlace { .. } => self.close_plan_place(),
            Flow::Create { .. }
            | Flow::CreateStack
            | Flow::CreateBoard
            | Flow::EditTitle { .. }
            | Flow::RenameStack { .. }
            | Flow::RenameBoard { .. } => {
                if self.buffer().trim().is_empty() {
                    self.discard_edit()
                } else {
                    self.commit_edit(state)
                }
            }
            Flow::EditDate { .. } => self.end_edit_date(),
            // The recurrence editor clean-closes on a foreign settle
            // (the date-editor rule): the draft dies with the level,
            // nothing dispatches — the only commit path is the
            // editor's own funnel.
            Flow::EditRecur { .. } => self.end_edit_recur(),
            // The description is a TEXT flow on the shared buffer:
            // commit non-empty, discard empty — a focus steal never
            // clears (round-3 D15).
            Flow::EditDescription { .. } => {
                if self.buffer().trim().is_empty() {
                    self.discard_edit()
                } else {
                    self.commit_description()
                }
            }
            Flow::Move { .. } | Flow::MoveStack { .. } | Flow::DragMove { .. } => {
                self.cancel_move()
            }
            Flow::Palette { .. } => self.close_palette(),
            Flow::Capture => self.close_capture(),
            // The concentration overlay closes like the palette: the
            // settle is the bounded re-run's terminator (an inert
            // settle would recurse in the resolver), and the leaving
            // effects must not re-run beneath a live overlay. The
            // multi-control surface's field focus-loss never reaches
            // this settle — the overlay's seam does not wire it (the
            // capture overlay's single-field discipline does not
            // transfer); the close paths stay Escape / the toggle /
            // the scrim. The session is untouched (close carries no
            // command).
            Flow::FocusMode { .. } => self.close_focus_mode(),
            // The entries overlay closes like the palette: the settle
            // is the bounded re-run's terminator; the close paths stay
            // the Escape ladder / the outside click (the
            // close-without-effect shape — no command rides it).
            Flow::TrackEntries { .. } | Flow::TrackEntryForm { .. } => self.close_track_entries(),
            // The labels overlay closes like the palette (the
            // entries-overlay's shape — the close-without-effect
            // family; the escape ladder and the outside click are
            // its exits).
            Flow::Labels { .. } => self.close_labels(),
            // The review walk closes like the palette (DR-7 D-J8: any
            // other flow replaces it without committing — there is
            // nothing to commit; the entry is one `Record` at the end).
            Flow::ReviewWalk { .. } => self.close_review_walk(),
            // The child levels (chunk 3's ports): the modals' row —
            // the child's close paths are its exits, and its settle
            // arm is the Cancelled end (the modal family's
            // terminator). The Q3 ruling scoped correctly (D5): the
            // text rule governs levels whose settle kind is `Text`;
            // the walk's half-typed date child never settled as text —
            // a foreign settle discards it, exactly as the flattened
            // walk always did. The entries form's typed fields
            // likewise keep their rule (the overlay cells).
            // The embedded planner child (J4 D-2): a foreign settle
            // ends it Cancelled — the exit contract returns the walk
            // to Carry (the child-death rule; drafts die with the
            // level).
            Flow::EntryDatePicker
            | Flow::ReviewPlanner
            | Flow::WalkFilePick { .. }
            | Flow::WalkDatePick => {
                self.end_top_flow_with(state, crate::surfaces::GuestExit::Cancelled)
            }
            // The armed-delete strip (UX R-2 D-2): a foreign settle
            // disarms without dispatching — the placement form's
            // close shape (only a confirm or the subject's own repeat
            // stroke dispatches).
            Flow::ConfirmDelete(_) => self.cancel_confirm_delete(),
            // The color picker (UX R-4 D-1): a foreign settle disarms
            // without dispatching — the armed-strip's close shape (the
            // picker's own re-openability is the recovery, so no
            // staged discard either).
            Flow::BoardColorPick { .. } => self.cancel_board_color(),
            // The board ceremony strips (OP1, US-1/2/3): a foreign
            // settle disarms without dispatching — the armed-delete
            // close shape verbatim (the informed-consent letter is
            // consent scaffolding, never a command).
            Flow::ConfirmBoardArchive { .. } => self.cancel_board_archive(),
            Flow::ConfirmBoardReset { .. } => self.cancel_board_reset(),
            Flow::ConfirmBoardReconcile { .. } => self.cancel_board_reconcile(),
        }
    }

    // -- board/stack lifecycle (round 2, D18/D21) --------------------------

    /// `board.create`: always available (the zero-board first-run
    /// escape — the `begin_create_stack` refusal does not apply);
    /// opens the board field in insert mode.
    pub fn begin_create_board(&mut self) -> Vec<FlowOutcome> {
        self.write_flow(Flow::CreateBoard);
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
        self.write_flow(Flow::RenameStack { stack });
        self.set_buffer(title);
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
        self.write_flow(Flow::RenameBoard { board });
        self.set_buffer(title);
        vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
    }

    /// `stack.delete`, two-phase (UX R-2 D-2, #784): the first
    /// invocation arms `Flow::ConfirmDelete` over the resolved target
    /// — nothing dispatches, the strip mounts at the subject; the
    /// invocation repeated (or the strip's Confirm — click ≡ key at
    /// every step) dispatches the exact `DeleteStack` + `RefreshNow`
    /// and clears the flow. Zero command delta, zero undo-class
    /// change: `DeleteStack` keeps its `NotUndoable(RestoreClass)`
    /// class; the confirmation *is* the escape path. Inert without a
    /// target; re-arming over a differently-armed subject swaps.
    pub fn delete_stack(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Some(stack) = self.keyboard_resize_target(state) else {
            return Vec::new();
        };
        if *self.flow() == Flow::ConfirmDelete(DeleteTarget::Stack(stack)) {
            return self.commit_confirm_delete();
        }
        self.write_flow(Flow::ConfirmDelete(DeleteTarget::Stack(stack)));
        vec![FlowOutcome::RefreshNow]
    }

    /// `board.delete`, the stack twin (UX R-2 D-2): arms
    /// `Flow::ConfirmDelete` over the current board first; the repeat
    /// (or the strip's Confirm) dispatches the exact `DeleteBoard`
    /// (cascades) + `RefreshNow` and clears the flow.
    pub fn delete_board(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        self.normalize_board(state);
        let Some(board) = self.board else {
            return Vec::new();
        };
        if *self.flow() == Flow::ConfirmDelete(DeleteTarget::Board(board)) {
            return self.commit_confirm_delete();
        }
        self.write_flow(Flow::ConfirmDelete(DeleteTarget::Board(board)));
        vec![FlowOutcome::RefreshNow]
    }

    /// The confirm half of the armed delete: dispatches the armed
    /// subject's exact command and clears the flow. Only a confirm
    /// reaches here (the two-phase verbs' armed branch and the
    /// adapter's strip seam).
    fn commit_confirm_delete(&mut self) -> Vec<FlowOutcome> {
        let Flow::ConfirmDelete(target) = self.flow().clone() else {
            return Vec::new();
        };
        self.write_flow(Flow::None);
        match target {
            DeleteTarget::Stack(stack) => {
                vec![
                    FlowOutcome::Command(StateCommand::DeleteStack { id: stack }),
                    FlowOutcome::RefreshNow,
                ]
            }
            DeleteTarget::Board(board) => {
                vec![
                    FlowOutcome::Command(StateCommand::DeleteBoard { board }),
                    FlowOutcome::RefreshNow,
                ]
            }
            // The multi delete's confirm (UX-R8 D-6, US-4/AC-3): one
            // `DeleteTask` per member of the arm-time snapshot — each
            // a journal PreImage; `AppUndo` reverses stepwise. The
            // snapshot's prune is reconcile's job; the engine absorbs
            // a vanished id (the typed no-op).
            DeleteTarget::Tasks(members) => {
                let mut outcomes: Vec<FlowOutcome> = members
                    .into_iter()
                    .map(|id| FlowOutcome::Command(StateCommand::DeleteTask { id }))
                    .collect();
                outcomes.push(FlowOutcome::RefreshNow);
                outcomes
            }
        }
    }

    /// The armed delete's disarm (the strip's Cancel, a foreign
    /// settle, Escape): ends the flow without dispatching — the
    /// placement form's close shape.
    pub fn cancel_confirm_delete(&mut self) -> Vec<FlowOutcome> {
        if matches!(self.flow(), Flow::ConfirmDelete(_)) {
            self.write_flow(Flow::None);
            vec![FlowOutcome::RefreshNow]
        } else {
            Vec::new()
        }
    }

    /// The armed multi delete (UX-R8 D-6, US-4/AC-3): `TaskDelete`
    /// with a non-empty selection arms `ConfirmDelete(Tasks)` — the
    /// anchor-order snapshot, nothing dispatches, the armed strip's
    /// grammar verbatim (the invoking verb's second stroke confirms;
    /// Escape / a foreign settle disarm with nothing dispatched).
    pub fn arm_multi_delete(&mut self, members: Vec<TaskId>) -> Vec<FlowOutcome> {
        if matches!(self.flow(), Flow::ConfirmDelete(DeleteTarget::Tasks(_))) {
            return self.commit_confirm_delete();
        }
        self.write_flow(Flow::ConfirmDelete(DeleteTarget::Tasks(members)));
        vec![FlowOutcome::RefreshNow]
    }

    /// `board.color-edit` (UX R-4 D-1, #791): arms the color picker
    /// over the selected board — nothing dispatches, the swatch strip
    /// mounts. Inert without a board; re-arming over the armed board
    /// is the typed no-op (the strip is already up); a different
    /// armed board re-arms.
    pub fn begin_board_color(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        self.normalize_board(state);
        let Some(board) = self.board else {
            return Vec::new();
        };
        if matches!(*self.flow(), Flow::BoardColorPick { board: armed } if armed == board) {
            return Vec::new();
        }
        self.write_flow(Flow::BoardColorPick { board });
        vec![FlowOutcome::RefreshNow]
    }

    /// The picker's commit half (UX R-4 D-1): dispatches the exact
    /// `SetBoardColor` over the armed board with the swatch's hex
    /// (the shared palette's vocabulary — never free text) and ends
    /// the flow. The verb is `NotUndoable(BoardCeremony)`, so the
    /// picker's own re-openability is the mis-pick recovery — the
    /// recorded reason no armed-confirm stage applies.
    pub fn commit_board_color(&mut self, swatch: &'static str) -> Vec<FlowOutcome> {
        let Flow::BoardColorPick { board } = self.flow().clone() else {
            return Vec::new();
        };
        self.write_flow(Flow::None);
        vec![
            FlowOutcome::Command(StateCommand::SetBoardColor {
                board,
                color: taskboard_domain::Color::new(swatch),
            }),
            FlowOutcome::RefreshNow,
        ]
    }

    /// The picker's disarm (the strip's dismissal seam, Escape, a
    /// foreign settle): ends the flow without dispatching — the
    /// armed-delete close shape.
    pub fn cancel_board_color(&mut self) -> Vec<FlowOutcome> {
        if matches!(self.flow(), Flow::BoardColorPick { .. }) {
            self.write_flow(Flow::None);
            vec![FlowOutcome::RefreshNow]
        } else {
            Vec::new()
        }
    }

    // -- the board ceremony verbs (OP1, US-1/US-2/US-3) ---------------

    /// `board.archive` (OP1 US-1, D-1): arms the archive consent
    /// caption over the selected board — nothing dispatches; the
    /// strip's Confirm (or the invoking verb's repeat — click ≡ key
    /// at every step) routes the exact `SetBoardArchived { true }`
    /// and clears the flow. Zero command delta, zero undo-class
    /// change: the verb stays `NotUndoable(BoardCeremony)`; the
    /// confirmation is consent scaffolding, the typed escape is the
    /// re-archive path. Inert without a board; re-arming over the
    /// armed board confirms (the shared grammar's second stroke); a
    /// different armed subject swaps.
    pub fn begin_board_archive(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        self.normalize_board(state);
        let Some(board) = self.board else {
            return Vec::new();
        };
        if *self.flow() == (Flow::ConfirmBoardArchive { board }) {
            return self.commit_board_ceremony_archive();
        }
        self.write_flow(Flow::ConfirmBoardArchive { board });
        vec![FlowOutcome::RefreshNow]
    }

    /// The archive strip's confirm half: dispatches the exact
    /// `SetBoardArchived { archived: true }` over the armed board and
    /// clears the flow (the confirm-delete commit shape; the engine's
    /// typed tombstone refusal is the failure path's fact).
    fn commit_board_ceremony_archive(&mut self) -> Vec<FlowOutcome> {
        let Flow::ConfirmBoardArchive { board } = self.flow().clone() else {
            return Vec::new();
        };
        self.write_flow(Flow::None);
        vec![
            FlowOutcome::Command(StateCommand::SetBoardArchived {
                board,
                archived: true,
            }),
            FlowOutcome::RefreshNow,
        ]
    }

    /// The archive strip's disarm (Cancel, Escape, a foreign settle):
    /// ends the flow without dispatching — the armed-delete close
    /// shape.
    pub fn cancel_board_archive(&mut self) -> Vec<FlowOutcome> {
        if matches!(self.flow(), Flow::ConfirmBoardArchive { .. }) {
            self.write_flow(Flow::None);
            vec![FlowOutcome::RefreshNow]
        } else {
            Vec::new()
        }
    }

    /// `board.reset` (OP1 US-2, D-3): arms `ConfirmBoardReset` over
    /// the selected board — the consequence letter derives its counts
    /// from the live model at push time (the adapter's board rows;
    /// no engine query). Confirm routes the exact `ResetBoard`; the
    /// same grammar as the archive caption (the shared armed-strip
    /// machinery). Inert without a board; re-arm swaps subjects.
    pub fn begin_board_reset(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        self.normalize_board(state);
        let Some(board) = self.board else {
            return Vec::new();
        };
        if *self.flow() == (Flow::ConfirmBoardReset { board }) {
            return self.commit_board_reset();
        }
        self.write_flow(Flow::ConfirmBoardReset { board });
        vec![FlowOutcome::RefreshNow]
    }

    /// The reset strip's confirm half (US-2/S3): dispatches the exact
    /// `ResetBoard { board }` and clears the flow — the board row and
    /// connect session survive engine-side; the typed tombstone
    /// refusal is the failure path's fact.
    fn commit_board_reset(&mut self) -> Vec<FlowOutcome> {
        let Flow::ConfirmBoardReset { board } = self.flow().clone() else {
            return Vec::new();
        };
        self.write_flow(Flow::None);
        vec![
            FlowOutcome::Command(StateCommand::ResetBoard { board }),
            FlowOutcome::RefreshNow,
        ]
    }

    /// The reset strip's disarm (Cancel, Escape, a foreign settle).
    pub fn cancel_board_reset(&mut self) -> Vec<FlowOutcome> {
        if matches!(self.flow(), Flow::ConfirmBoardReset { .. }) {
            self.write_flow(Flow::None);
            vec![FlowOutcome::RefreshNow]
        } else {
            Vec::new()
        }
    }

    /// `board.reconcile` (OP1 US-3, D-4): arms `ConfirmBoardReconcile`
    /// with a `Pending` preview — the adapter sends the oneshot
    /// `UiRequest::BoardReconcilePreview` when the arm is fresh (the
    /// adapter's arm owns the send; the view state never touches a
    /// channel). Confirm routes the exact `ReconcileByTitle`; the
    /// zero-count preview is the accepted idempotent no-op (AC-4).
    pub fn begin_board_reconcile(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        self.normalize_board(state);
        let Some(board) = self.board else {
            return Vec::new();
        };
        if let Flow::ConfirmBoardReconcile {
            board: armed,
            preview,
        } = self.flow().clone()
            && armed == board
        {
            // The second stroke confirms unless the counts are still
            // in flight — a confirm against an unseen letter is not
            // informed consent (D-4's staging law); the stroke is the
            // typed no-op while pending.
            return if matches!(preview, ReconcilePreviewState::Pending) {
                Vec::new()
            } else {
                self.commit_board_reconcile()
            };
        }
        self.write_flow(Flow::ConfirmBoardReconcile {
            board,
            preview: ReconcilePreviewState::Pending,
        });
        vec![FlowOutcome::RefreshNow]
    }

    /// The preview reply's landing (the adapter's drain seam): fills
    /// the armed flow's counts (or the dead-lane fact) — only over
    /// the matching armed subject; a reply for a since-vanished flow
    /// is discarded.
    pub fn settle_board_reconcile_preview(
        &mut self,
        board: BoardId,
        preview: Result<taskboard_domain::ReconcileCounts, ()>,
    ) -> Vec<FlowOutcome> {
        let Flow::ConfirmBoardReconcile {
            board: armed,
            preview: state,
        } = self.flow().clone()
        else {
            return Vec::new();
        };
        if armed != board || state != ReconcilePreviewState::Pending {
            return Vec::new();
        }
        if let Ok(counts) = preview {
            self.write_flow(Flow::ConfirmBoardReconcile {
                board: armed,
                preview: ReconcilePreviewState::Ready(counts),
            });
            return vec![FlowOutcome::RefreshNow];
        }
        // The dead-lane fact (US-3/S1's escape cell): the strip
        // disarms — an unlettered confirm is not informed consent
        // (D-4's staging law); the Failed state is the typed
        // dead-lane marker the fold catalogue names (never
        // mounted — the disarm precedes any render).
        self.write_flow(Flow::None);
        vec![FlowOutcome::RefreshNow]
    }

    /// The reconcile strip's confirm half (US-3/S3): dispatches the
    /// exact `ReconcileByTitle { board }` and clears the flow.
    fn commit_board_reconcile(&mut self) -> Vec<FlowOutcome> {
        let Flow::ConfirmBoardReconcile { board, .. } = self.flow().clone() else {
            return Vec::new();
        };
        self.write_flow(Flow::None);
        vec![
            FlowOutcome::Command(StateCommand::ReconcileByTitle { board }),
            FlowOutcome::RefreshNow,
        ]
    }

    /// The reconcile strip's disarm (Cancel, Escape, a foreign
    /// settle): the preview dies with the flow (US-3 AC-3 — the
    /// discard).
    pub fn cancel_board_reconcile(&mut self) -> Vec<FlowOutcome> {
        if matches!(self.flow(), Flow::ConfirmBoardReconcile { .. }) {
            self.write_flow(Flow::None);
            vec![FlowOutcome::RefreshNow]
        } else {
            Vec::new()
        }
    }

    /// `stack.move` with no flow: the resolved stack starts moving at
    /// its current display position. Inert without a target.
    pub fn begin_move_stack(&mut self, state: &AppState) -> Vec<FlowOutcome> {
        let Some(stack) = self.keyboard_resize_target(state) else {
            return Vec::new();
        };
        let stacks = self.stack_ids(state);
        let slot = stacks.iter().position(|s| *s == stack).unwrap_or(0);
        self.write_flow(Flow::MoveStack { stack, slot });
        vec![FlowOutcome::RefreshNow]
    }

    /// Focus next/prev during a stack move: wrap the insertion slot
    /// over `0..=len` (`len + 1` slots — `len` = append after last).
    pub fn cycle_move_stack_target(
        &mut self,
        state: &AppState,
        direction: i32,
    ) -> Vec<FlowOutcome> {
        let Flow::MoveStack { slot, .. } = self.flow().clone() else {
            return Vec::new();
        };
        let slots = self.stack_ids(state).len() + 1;
        let next = wrapped_index(slot, direction, slots);
        if let Some(Flow::MoveStack { slot, .. }) = self.surfaces.top_flow_mut() {
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
        let Flow::MoveStack { stack, slot } = self.flow().clone() else {
            return Vec::new();
        };
        self.write_flow(Flow::None);
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
    #[allow(clippy::too_many_lines)] // the reconcile fold, one pass
    pub fn reconcile(&mut self, state: &AppState) {
        // The roster's one-shot select (UX R-1 D-1): consumed exactly
        // on the first refresh where the target is live again; a
        // deleted target prunes (a stale arm must not linger forever).
        if let Some(target) = self.pending_board_select {
            match state.boards.get(&target) {
                Some(board) if !board.deleted && !board.archived => {
                    self.pending_board_select = None;
                    self.board = Some(target);
                }
                Some(_) => {}
                None => self.pending_board_select = None,
            }
        }
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
        // The habit cursor prunes like the task cursor (chunk 8 D2):
        // a dead habit's landing clears on the same pass.
        if self.focused_habit.is_some_and(|h| {
            !state
                .habits
                .habits
                .get(&h)
                .is_some_and(taskboard_domain::Habit::is_live)
        }) {
            self.focused_habit = None;
        }
        // The rule cursor's clamp law (J3 US-6 D-2, round 9 chunk 12):
        // id-keyed, pruned — never re-pointed — against this pass's
        // roster (a dead id never re-anchors to a live neighbour, the
        // `pending_board_select` prune law). A live rule on a foreign
        // roster (the subject changed between passes) clears the same
        // way, and the cursor is a detail-surface cursor: leaving the
        // detail clears it (the leaving rule's drain face — the
        // refresh after any exit runs this pass).
        if let Some(cursor) = self.rule_cursor {
            let on_roster = self.subject_task.is_some_and(|subject| {
                state
                    .reminders
                    .rules
                    .get(&cursor)
                    .is_some_and(|rule| rule.task == subject)
            });
            if !on_roster || self.view() != ViewId::TaskDetail {
                self.clear_rule_cursor();
            }
        }
        // UX-R8 D-4's reconcile arm: the selection repairs like the
        // focus — dead members prune against this pass; a dead anchor
        // or an emptied member set clears (nothing dispatches).
        if !self.selection.is_empty() {
            self.selection.members.retain(|id| is_live(*id));
            let anchor_live = self.selection.anchor.is_some_and(is_live);
            if !anchor_live || self.selection.members.is_empty() {
                self.selection = BoardSelection::default();
            }
        }
        let stacks = self.stack_ids(state);
        if self.selected_stack.is_some_and(|s| !stacks.contains(&s)) {
            self.selected_stack = stacks.first().copied();
        }
        if let Some(stack) = self.subject_stack
            && !stacks.contains(&stack)
        {
            self.subject_stack = None;
            // The pop reads the top view — the flat model's single
            // `self.view` slot (a pushed over-view reads the
            // over-view, exactly as today). A matching view is always
            // the root: the only pushed view kinds are Settings and
            // the planner, never a subject view (D8).
            if self.view() == ViewId::StackDetail {
                self.replace_root(ViewId::Board);
            }
        }
        // The habit variant's subject absorbs when its habit dies
        // (the same stale-subject law; the editor closes to Board).
        if let Some(habit) = self.subject_habit
            && !state
                .habits
                .habits
                .get(&habit)
                .is_some_and(taskboard_domain::habit::Habit::is_live)
        {
            self.subject_habit = None;
            if self.view() == ViewId::TaskDetail {
                self.replace_root(ViewId::Board);
            }
            // W-14 (#1048): the widened edit flows die with the habit
            // subject exactly like the task flows die with theirs.
            if matches!(
                *self.flow(),
                Flow::EditTitle {
                    subject: EditSubject::Habit(h)
                }
                | Flow::EditDescription {
                    subject: EditSubject::Habit(h)
                } if h == habit
            ) {
                self.write_flow(Flow::None);
            }
        }
        if let Some(task) = self.subject_task
            && !is_live(task)
        {
            self.subject_task = None;
            if self.view() == ViewId::TaskDetail {
                self.replace_root(ViewId::Board);
            }
            if matches!(
                *self.flow(),
                Flow::EditTitle {
                    subject: EditSubject::Task(t),
                }
                | Flow::EditDescription {
                    subject: EditSubject::Task(t),
                }
                    | Flow::EditDate {
                        task: t, field: _,
                    }
                    | Flow::EditRecur { task: t } if t == task
            ) {
                self.write_flow(Flow::None);
            }
        }
        match self.flow().clone() {
            Flow::Create { stack } if !stacks.contains(&stack) => self.write_flow(Flow::None),
            Flow::CreateStack if state.boards.values().all(|b| b.deleted) => {
                self.write_flow(Flow::None);
            }
            Flow::Move { task, .. } if !is_live(task) => self.write_flow(Flow::None),
            // The pointer drag (r2 chunk-8): a vanished dragged task
            // *or* target stack ends the flow — the ghost/dim/highlight
            // unpush on the next model pass (the subject-repair rule).
            Flow::DragMove { task, target, .. } if !is_live(task) || !stacks.contains(&target) => {
                self.write_flow(Flow::None);
            }
            // Round 2 (D18/D21): the lifecycle flows end silently on a
            // vanished target (the `Flow::Move` precedent).
            Flow::RenameStack { stack } if !stacks.contains(&stack) => {
                self.write_flow(Flow::None);
            }
            Flow::RenameBoard { board } if self.board != Some(board) => self.write_flow(Flow::None),
            Flow::MoveStack { stack, .. } if !stacks.contains(&stack) => {
                self.write_flow(Flow::None);
            }
            // The armed-delete strip (UX R-2 D-2): a vanished subject
            // ends the flow silently — nothing dispatches (the
            // rename-vanish pattern; the strip unmounts on the next
            // model pass).
            // The armed multi delete (UX-R8 D-6): a member dying
            // mid-arm prunes the snapshot (empty → disarm) — the
            // rename-vanish pattern over a member set.
            Flow::ConfirmDelete(DeleteTarget::Tasks(members)) => {
                let pruned: Vec<TaskId> = members.into_iter().filter(|id| is_live(*id)).collect();
                if pruned.is_empty() {
                    self.write_flow(Flow::None);
                } else {
                    self.write_flow(Flow::ConfirmDelete(DeleteTarget::Tasks(pruned)));
                }
            }
            Flow::ConfirmDelete(DeleteTarget::Stack(stack)) if !stacks.contains(&stack) => {
                self.write_flow(Flow::None);
            }
            Flow::ConfirmDelete(DeleteTarget::Board(board)) if self.board != Some(board) => {
                self.write_flow(Flow::None);
            }
            // The board ceremony strips (OP1): the armed board's
            // death ends the flow silently (the reconcile-vanish
            // pattern; the preview dies with the flow).
            Flow::ConfirmBoardArchive { board }
                if self.board != Some(board) || !Self::board_live(state, board) =>
            {
                self.write_flow(Flow::None);
            }
            Flow::ConfirmBoardReset { board }
                if self.board != Some(board) || !Self::board_live(state, board) =>
            {
                self.write_flow(Flow::None);
            }
            Flow::ConfirmBoardReconcile { board, .. }
                if self.board != Some(board) || !Self::board_live(state, board) =>
            {
                self.write_flow(Flow::None);
            }
            // The placement form (#622): a dead subject drops it; a
            // view change does NOT (the mount gate keys on the
            // subject — the entries overlay's predicate minus the
            // view strand, D-3's reconciliation risk).
            Flow::PlanPlace { task } if !is_live(task) => self.write_flow(Flow::None),
            _ => {}
        }
        // The entries overlay (r4 chunk 10, D-TS2's strand-proof;
        // chain-wide since the ports): it cannot outlive the
        // task-detail view or a dead subject. The scan walks the
        // chain because the overlay may sit beneath its own children
        // (the form, the form's picker) — the OUTERMOST overlay level
        // ends and the subtree above it goes with it, silently (Q4:
        // a subject death delivers no exit). Focus falls back through
        // the existing walk.
        while let Some(index) = self
            .surfaces
            .above_levels()
            .iter()
            .position(|level| {
                matches!(
                    level.flow(),
                    Some(
                        Flow::TrackEntries { .. }
                            | Flow::TrackEntryForm { .. }
                            | Flow::Labels { .. }
                    )
                )
            })
            .filter(|index| {
                let dead = match self.surfaces.above_levels()[*index].flow() {
                    Some(Flow::TrackEntries { task } | Flow::TrackEntryForm { task, .. }) => {
                        !is_live(*task)
                    }
                    Some(Flow::Labels {
                        subject: LabelOverlaySubject::Task(task),
                    }) => !is_live(*task),
                    Some(Flow::Labels {
                        subject: LabelOverlaySubject::Board(board),
                    }) => state.boards.get(board).is_none_or(|b| b.deleted),
                    _ => false,
                };
                // The view-leave rule, subject-affine (tag-color plan
                // D-4): the detail mount dies when the view leaves the
                // task detail; the roster-only board mount dies when
                // the view leaves the board surface.
                let view_left = match self.surfaces.above_levels()[*index].flow() {
                    Some(Flow::Labels {
                        subject: LabelOverlaySubject::Board(_),
                    }) => self.view() != ViewId::Board,
                    _ => self.view() != ViewId::TaskDetail,
                };
                dead || view_left
            })
        {
            let dropped = self.surfaces.truncate_to(index + 1);
            self.retired.extend(dropped);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    use taskboard_domain::{Board, BoardClocks, BoardId, Stack, StackClocks, Task, TaskClocks};

    pub(super) fn epoch() -> chrono::DateTime<Utc> {
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
            effort: epoch(),
            importance: epoch(),
            urgency: epoch(),
            recur: epoch(),
            kind: epoch(),

            archived: epoch(),
            deleted: epoch(),
        }
    }

    pub(super) fn task(id: TaskId, stack: StackId, order: i64) -> Task {
        Task {
            id,
            remote: None,
            label_memberships: BTreeMap::new(),
            title: format!("task-{order}"),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack,
            order,
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: taskboard_domain::entities::TaskKind::Standard,

            archived: false,
            deleted: false,
            clocks: clocks(),
            authors: taskboard_domain::entities::TaskAuthors::default(),
            remote_seen: None,
            remote_baseline: None,
        }
    }

    pub(super) fn stack(id: StackId, board: BoardId, order: i64) -> Stack {
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
            authors: taskboard_domain::entities::StackAuthors::default(),
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
            calendar: taskboard_domain::DayCalendar {
                day_start_time: chrono::NaiveTime::MIN,
                offset: chrono::FixedOffset::east_opt(0).expect("utc offset"),
            },
        }
    }

    pub(super) fn fixture() -> (AppState, BoardId, [StackId; 2], [TaskId; 3]) {
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
                authors: taskboard_domain::entities::BoardAuthors::default(),
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
                authors: taskboard_domain::entities::BoardAuthors::default(),
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
        let (mut state, _, [s0, s1], [a, ..]) = fixture();
        let (_, [t0, t1]) = second_board(&mut state);
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.start_move(&state);
        // Armed at (s0, 1); the forward walk crosses the board edge
        // into the second board's (empty) stacks — one slot each.
        let walk = |vs: &mut ViewState, state: &AppState| {
            vs.cycle_move_target(state, 1);
            match vs.flow().clone() {
                Flow::Move { target, slot, .. } => Some((target, slot)),
                _ => None,
            }
        };
        assert_eq!(walk(&mut vs, &state), Some((s0, 2)));
        assert_eq!(walk(&mut vs, &state), Some((s1, 0)));
        assert_eq!(walk(&mut vs, &state), Some((s1, 1)));
        assert_eq!(
            walk(&mut vs, &state),
            Some((t0, 0)),
            "the forward walk crosses the board edge"
        );
        assert_eq!(
            walk(&mut vs, &state),
            Some((t1, 0)),
            "the last board's last stack"
        );
        assert_eq!(
            walk(&mut vs, &state),
            Some((s0, 0)),
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
        // Walk to the second board's first (empty) stack: gap 0 on an
        // empty stack is the canonical 0 — cross-board targets are
        // already legal engine-side.
        vs.cycle_move_target(&state, 1);
        vs.cycle_move_target(&state, 1);
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
            "an empty target stack's only slot is the canonical 0"
        );
    }

    // -- the multi sequences (UX-R8 D-6, US-4/S2) -------------------------

    #[test]
    fn the_multi_delete_confirms_one_task_per_member() {
        let (state, _, [_s0, _s1], [a, b, _c]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        let outcomes = vs.arm_multi_delete(vec![a, b]);
        assert_eq!(
            outcomes,
            vec![FlowOutcome::RefreshNow],
            "the arm dispatches nothing"
        );
        assert_eq!(
            *vs.flow(),
            Flow::ConfirmDelete(DeleteTarget::Tasks(vec![a, b])),
            "the armed strip carries the anchor-order snapshot"
        );
        // The repeat (or the strip's Confirm) dispatches one
        // `DeleteTask` per member (each a journal PreImage).
        let outcomes = vs.arm_multi_delete(vec![a, b]);
        assert_eq!(
            outcomes[..2],
            [
                FlowOutcome::Command(StateCommand::DeleteTask { id: a }),
                FlowOutcome::Command(StateCommand::DeleteTask { id: b }),
            ][..]
        );
        assert_eq!(*vs.flow(), Flow::None);
        // Escape disarms with nothing dispatched.
        let outcomes = vs.arm_multi_delete(vec![a, b]);
        assert_eq!(outcomes, vec![FlowOutcome::RefreshNow]);
        let outcomes = vs.cancel_confirm_delete();
        assert_eq!(outcomes, vec![FlowOutcome::RefreshNow]);
        assert_eq!(*vs.flow(), Flow::None);
    }

    #[test]
    fn the_multi_delete_prunes_a_dead_member_and_disarms_on_empty() {
        let (mut state, _, [_s0, _s1], [a, b, _c]) = fixture();
        state.tasks.get_mut(&b).expect("task").deleted = true;
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.arm_multi_delete(vec![a, b]);
        // The reconcile pass prunes the dead member from the snapshot.
        vs.reconcile(&state);
        assert_eq!(
            *vs.flow(),
            Flow::ConfirmDelete(DeleteTarget::Tasks(vec![a])),
            "the dead member's prune follows the strip"
        );
        // Every member dying disarms.
        state.tasks.get_mut(&a).expect("task").deleted = true;
        vs.reconcile(&state);
        assert_eq!(*vs.flow(), Flow::None, "an empty snapshot disarms");
    }

    // -- the slot grammar (UX-R8 D-1, US-1/AC-3) --------------------------

    #[test]
    fn a_single_task_stack_enumerates_exactly_two_slots() {
        let (state, _, [_s0, s1], _) = fixture();
        let s1_slots: Vec<(StackId, usize)> = ViewState::move_slots(&state)
            .into_iter()
            .filter(|(stack, _)| *stack == s1)
            .collect();
        assert_eq!(
            s1_slots,
            vec![(s1, 0), (s1, 1)],
            "one task, two insertion points: before and after"
        );
    }

    #[test]
    fn the_slot_walk_order_is_stack_canonical_top_to_bottom() {
        let (state, _, [s0, s1], _) = fixture();
        assert_eq!(
            ViewState::move_slots(&state),
            vec![(s0, 0), (s0, 1), (s0, 2), (s1, 0), (s1, 1),],
            "stacks in ring order, gaps 0..=len within each"
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
                target: s0,
                gap: None
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
                target: s1,
                gap: None
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
            // UX-R8 D-1: the walk is slot-shaped — three strokes take
            // (s0, own-gap+1) → (s0, 2) → (s1, 0) → (s1, 1), and gap 1
            // of s1 IS the append (last.order + 1). The drag leg
            // confirms the column-level append; the commands must stay
            // identical (one construction site).
            vs.cycle_move_target(&state, 1);
            vs.cycle_move_target(&state, 1);
            vs.cycle_move_target(&state, 1);
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

    // -- the surface chain (W5b chunk 1, plan D7/D8/D12) -------------------

    #[test]
    fn closing_settings_restores_the_exact_root_view_from_every_cycle_view() {
        for view in [
            ViewId::Today,
            ViewId::Board,
            ViewId::StackDetail,
            ViewId::TaskDetail,
            ViewId::Review,
        ] {
            let mut vs = ViewState::new_starting(view);
            vs.open_settings();
            assert_eq!(vs.view(), ViewId::Settings, "settings pushes over {view:?}");
            vs.close_settings();
            assert_eq!(
                vs.view(),
                view,
                "the exact restore is truncation from {view:?}"
            );
        }
    }

    #[test]
    fn closing_the_planner_restores_the_exact_root_view_from_every_cycle_view() {
        for view in [
            ViewId::Today,
            ViewId::Board,
            ViewId::StackDetail,
            ViewId::TaskDetail,
            ViewId::Review,
        ] {
            let mut vs = ViewState::new_starting(view);
            vs.open_plan_next_day();
            assert_eq!(vs.view(), ViewId::PlanNextDay);
            vs.close_plan_next_day();
            assert_eq!(vs.view(), view);
        }
    }

    #[test]
    fn a_close_over_closes_only_the_top_of_pushed_view_stack() {
        let mut vs = ViewState::new_starting(ViewId::TaskDetail);
        vs.open_settings();
        vs.open_plan_next_day();
        assert_eq!(vs.view(), ViewId::PlanNextDay);
        vs.close_plan_next_day();
        assert_eq!(vs.view(), ViewId::Settings, "only the top level closed");
        vs.close_settings();
        assert_eq!(vs.view(), ViewId::TaskDetail);
    }

    #[test]
    fn a_double_open_refuses_and_the_close_lands_on_the_root() {
        // D7: the second open is the AlreadyInChain refusal mapped to
        // a no-op — one level, so the close restores the root, never
        // the flat model's self-return (the pathological corner the
        // plan pins; the real UI cannot double-open either surface).
        let mut vs = ViewState::new_starting(ViewId::Today);
        vs.open_settings();
        vs.open_settings();
        vs.close_settings();
        assert_eq!(vs.view(), ViewId::Today);
        // The planner twin.
        let mut vs = ViewState::new_starting(ViewId::Today);
        vs.open_plan_next_day();
        vs.open_plan_next_day();
        vs.close_plan_next_day();
        assert_eq!(vs.view(), ViewId::Today);
    }

    #[test]
    fn a_dead_subject_under_a_pushed_over_view_currently_pins_the_flat_pop_guard() {
        let (mut state, _, _, [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.set_focused(Some(a));
        vs.activate_focused(&state);
        assert_eq!(vs.view(), ViewId::TaskDetail);
        vs.open_settings();
        // The subject dies while Settings is pushed: the pop reads the
        // top view (Settings), so it does not fire — byte-equal to the
        // flat model's single-slot guard.
        state.tasks.get_mut(&a).expect("task").deleted = true;
        vs.reconcile(&state);
        assert_eq!(vs.view(), ViewId::Settings);
        vs.close_settings();
        assert_eq!(vs.view(), ViewId::TaskDetail);
        // The pop guard is dead from here: the subject is already
        // `None`, so the restored root never pops — the flat model's
        // single-slot guard reproduced verbatim. (Debt noted: a
        // subject-less TaskDetail root survives until a navigation
        // replaces it; not this chunk's behaviour change to make.)
        vs.reconcile(&state);
        assert_eq!(vs.view(), ViewId::TaskDetail);
    }

    #[test]
    fn reconcile_drops_a_dead_subject_flow_without_a_settle_rule() {
        // Q4: the subject's death truncates the flow level silently —
        // no text commit, no buffer read (the flow's level is gone,
        // the buffer untouched by the drop).
        let (mut state, _, _, [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.set_focused(Some(a));
        vs.activate_focused(&state);
        vs.begin_edit_title(&state, a);
        vs.set_buffer("half-typed".to_owned());
        state.tasks.get_mut(&a).expect("task").deleted = true;
        vs.reconcile(&state);
        assert_eq!(*vs.flow(), Flow::None, "the dead subject drops the flow");
        // The drop clears the mirrored buffer in place — reconcile
        // returns no outcomes, so nothing was committed.
        assert_eq!(vs.buffer(), "");
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
        let outcomes = vs.begin_capture(&AppState::default());
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

    // -- per-level state (W5b D-H2/D3) -------------------------------------

    #[test]
    fn a_closed_text_level_reopens_clean_through_every_open() {
        let (state, _, [s0, _], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        // Create → discard → stack-create: the re-minted level starts
        // empty — the drop was the wipe.
        vs.begin_create(s0);
        vs.set_buffer("half-typed".to_owned());
        assert_eq!(vs.buffer(), "half-typed");
        vs.discard_edit();
        vs.begin_create_stack(&state);
        assert_eq!(vs.buffer(), "", "the re-opened level starts clean");
        vs.discard_edit();
        // The title editor's seeded reopen is still per level.
        vs.begin_edit_title(&state, a);
        assert_eq!(vs.buffer(), "task-0", "the seed rides the fresh level");
        vs.discard_edit();
        vs.begin_capture(&AppState::default());
        assert_eq!(vs.buffer(), "");
    }

    #[test]
    fn the_date_editor_reopens_clean_after_every_close() {
        let (state, _, _, [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.set_focused(Some(a));
        vs.activate_focused(&state);
        // Typed entry, then the Escape discard: the re-opened editor
        // starts clean — the drop was the wipe (the D25 clean close's
        // keep is unobservable past the close).
        vs.begin_edit_date(&state, DateField::Due);
        vs.set_buffer_for_date_editor_test("half-typed");
        vs.discard_edit();
        vs.begin_edit_date(&state, DateField::Scheduled);
        let editor = vs.date_editor().expect("the editor armed");
        assert_eq!(editor.buffer, "");
        assert_eq!(editor.echo, "");
        assert!(editor.picker.is_none());
        assert_eq!(editor.time_text, "--:--");
        // The commit end is the same drop.
        vs.set_buffer_for_date_editor_test("2026-01-02");
        vs.end_edit_date();
        vs.begin_edit_date(&state, DateField::Due);
        assert_eq!(vs.date_editor().expect("the editor armed").buffer, "");
    }

    #[test]
    fn the_entries_form_reopens_clean_after_its_level_ends() {
        let (state, _, _, [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.set_focused(Some(a));
        vs.activate_focused(&state);
        // The list, then the add form typed into, then the ladder
        // step back to the list (the generic Cancelled rung); a fresh
        // form mints clean — the drop was the wipe.
        vs.open_track_entries(a);
        vs.open_track_entry_form(a, None);
        if let Some(form) = vs.track_form_mut() {
            form.date_text = "typed".to_owned();
        }
        assert_eq!(
            vs.cancel_top_child(&state),
            Some(vec![FlowOutcome::RefreshNow])
        );
        let entry = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
        vs.open_track_entry_form(a, Some(entry));
        let form = vs.track_form().expect("the form armed");
        assert_eq!(form.date_text, "");
        assert!(vs.picker().is_none());
        assert!(!form.start_filter_armed);
        // Closing the whole overlay retires the form level with it.
        vs.close_track_entries();
        assert!(vs.track_form().is_none());
    }

    #[test]
    fn the_form_grid_toggle_mounts_the_picker_child_once() {
        let (state, _, _, _, [task, ..]) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.arm_flow(Flow::TrackEntryForm { task, seed: None });
        let outcomes = vs.open_entry_date_picker();
        assert!(matches!(outcomes.as_slice(), [FlowOutcome::RefreshNow]));
        assert!(vs.picker().is_some(), "the child mounts over the form");
        // The duplicate-kind open is the defensive no-op (D7): no
        // second child appears, so ONE Cancelled rung lands the walk
        // back on the form.
        let _ = vs.open_entry_date_picker();
        let outcomes = vs.end_top_flow_with(&state, crate::surfaces::GuestExit::Cancelled);
        assert!(outcomes.is_empty(), "the form ignores a Cancelled exit");
        assert!(matches!(vs.flow(), Flow::TrackEntryForm { .. }));
    }

    #[test]
    fn the_plan_form_reopens_clean_after_its_level_ends() {
        let (state, _, _, [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.set_focused(Some(a));
        vs.activate_focused(&state);
        // Typed placement, then the close: a fresh form mints clean —
        // the drop was the wipe.
        vs.open_plan_place(a);
        if let Some(place) = vs.plan_place_mut() {
            place.time_text = "09:30".to_owned();
            place.effort_text = "2h".to_owned();
        }
        vs.close_plan_place();
        vs.open_plan_place(a);
        let place = vs.plan_place().expect("the form armed");
        assert_eq!(place.time_text, "");
        assert_eq!(place.effort_text, "");
    }

    #[test]
    fn the_walk_reopens_clean_after_its_level_ends() {
        let (state, _, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        let reference = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("a valid date");
        // Prompt answers typed, then the close: a fresh walk mints
        // clean — the drop was the wipe. (The typed date text is the
        // WalkDatePick child's since N3b — its own level, its own
        // wipe.)
        vs.open_review_walk(ReviewCadence::Day, reference);
        if let Some(walk) = vs.walk_mut() {
            walk.answers = vec!["typed".to_owned()];
        }
        vs.close_review_walk();
        vs.open_review_walk(ReviewCadence::Day, reference);
        let walk = vs.walk().expect("the walk armed");
        assert_eq!(walk.answers, Vec::<String>::new());
    }

    #[test]
    fn the_retire_queue_drains_empty_at_rest() {
        let (state, _, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_capture(&AppState::default());
        vs.set_buffer("thought".to_owned());
        vs.discard_edit();
        vs.begin_capture(&AppState::default());
        vs.discard_edit();
        let drained = vs.drain_retired();
        assert_eq!(drained.len(), 2, "one corpse per dropped level");
        assert!(vs.drain_retired().is_empty(), "the queue is empty at rest");
        assert_eq!(*vs.flow(), Flow::None);
        // The root swap retires its pushed views the same way.
        vs.open_settings();
        vs.set_view(ViewId::Board);
        assert_eq!(vs.drain_retired().len(), 1);
    }

    #[test]
    fn escape_discards_capture_and_reconcile_leaves_the_flow_alone() {
        let (state, _, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_capture(&AppState::default());
        vs.set_buffer("halfway".to_owned());
        let outcomes = vs.close_capture();
        assert_eq!(*vs.flow(), Flow::None, "discard-and-close");
        assert_eq!(vs.buffer(), "");
        assert!(outcomes.contains(&FlowOutcome::SetMode(Mode::Normal)));

        // reconcile never ends the capture flow nor touches the buffer
        // (the palette precedent: subject-free).
        vs.begin_capture(&AppState::default());
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

        // The review surface joined the cycle (#214 D-1): subjectless,
        // between TaskDetail and Today.
        vs.next_view(&state);
        assert_eq!(vs.view(), ViewId::Review, "Review entered subjectless");

        vs.next_view(&state);
        assert_eq!(vs.view(), ViewId::Today, "the cycle wraps through Today");
        vs.next_view(&state);
        assert_eq!(vs.view(), ViewId::Board);
        vs.prev_view(&state);
        assert_eq!(vs.view(), ViewId::Today);
        vs.prev_view(&state);
        assert_eq!(
            vs.view(),
            ViewId::Review,
            "wrapping backwards crosses Review"
        );
        vs.prev_view(&state);
        assert_eq!(vs.view(), ViewId::TaskDetail);
        vs.focus_board();
        assert_eq!(vs.view(), ViewId::Board);
    }

    #[test]
    fn the_kiosk_rotation_never_lands_on_review() {
        // D-10(a), structural: KioskMode's view mapping is total over
        // exactly {Board, Today}, so no rotation tick — over every
        // declared mode set — can offer the Review view.
        for modes in [
            vec![KioskMode::Board, KioskMode::Today],
            vec![KioskMode::Today, KioskMode::Board],
        ] {
            let mut rotation =
                KioskRotation::new(modes, std::time::Duration::from_secs(1)).expect("mints");
            for _ in 0..4 {
                let view = rotation.tick().view();
                assert_ne!(
                    view,
                    ViewId::Review,
                    "the kiosk rotation never renders the review surface"
                );
                assert_ne!(
                    view,
                    ViewId::PlanNextDay,
                    "the kiosk rotation never renders the planner surface"
                );
            }
        }
        // And the mapping itself is exhaustive over the modes.
        assert_eq!(KioskMode::Board.view(), ViewId::Board);
        assert_eq!(KioskMode::Today.view(), ViewId::Today);
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

    // -- the detail walk RETIRED (J3 R-3, 2026-11-19; the round-8
    //    keymap-parity D-2 supersession, census-dated) ------------------
    //
    //    `detail_step_walk_moves_the_subject_and_syncs_focus`,
    //    `detail_jump_focus_targets_the_subjects_stack_ends`, and
    //    `detail_walk_is_inert_without_a_subject_or_an_empty_stack`
    //    asserted the subject walk's mechanics — the walk is gone (no
    //    viewed-task change from the detail at all); the guard's pin
    //    test below replaces them.

    /// Opens the detail over `task` the way the shell does: the card
    /// click (focus/selection) then the activation (the detail push).
    fn detail_over(task: TaskId, stack: StackId, state: &AppState) -> ViewState {
        let mut vs = ViewState::new();
        vs.reconcile(state);
        vs.activate_card(task, stack);
        let outcomes = vs.activate_focused(state);
        assert!(!outcomes.is_empty(), "the detail opens");
        vs
    }

    #[test]
    fn the_detail_guard_pins_the_retired_walk_to_the_typed_no_op() {
        let (state, _, [s0, _], [a, ..]) = fixture();
        let mut vs = detail_over(a, s0, &state);
        assert_eq!(vs.subject_task(), Some(a));
        // The step and the jump are the typed no-op: the subject and
        // the hidden board selection never move (J3 R-3 —
        // return-to-board-and-walk is the grammar).
        assert_eq!(vs.advance_focus(&state, false, false, &day()), Vec::new());
        assert_eq!(vs.retreat_focus(&state, false, false, &day()), Vec::new());
        assert_eq!(
            vs.jump_focus(&state, FocusEdge::Last, false, false, &day()),
            Vec::new()
        );
        assert_eq!(vs.subject_task(), Some(a), "the subject is pinned");
        assert_eq!(vs.focused_task(), Some(a), "the selection is pinned");
        assert_eq!(vs.view(), ViewId::TaskDetail, "the view never moves");
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
    fn the_default_kanban_preset_dispatches_explicit_ordered_creates() {
        let (state, board_id, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        let outcomes = vs.apply_board_preset(&state, &crate::board_presets::DEFAULT_KANBAN);
        let commands: Vec<_> = outcomes
            .iter()
            .filter_map(|o| match o {
                FlowOutcome::Command(c) => Some(c.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            commands.len(),
            crate::board_presets::DEFAULT_KANBAN.stacks.len()
        );
        for (order, (command, title)) in commands
            .iter()
            .zip(crate::board_presets::DEFAULT_KANBAN.stacks)
            .enumerate()
        {
            match command {
                StateCommand::CreateStack {
                    board,
                    title: t,
                    order: o,
                } => {
                    assert_eq!(*board, board_id, "the selected board");
                    assert_eq!(t, *title, "table order titles");
                    assert_eq!(*o, i64::try_from(order).unwrap(), "explicit 0..n orders");
                }
                other => panic!("typed outcome expected, got {other:?}"),
            }
        }
        assert!(
            outcomes
                .iter()
                .any(|o| matches!(o, FlowOutcome::RefreshNow)),
            "one trailing refresh"
        );
        // No text flow opened: preset creation is direct dispatch.
        assert_eq!(*vs.flow(), Flow::None);
    }

    #[test]
    fn a_synthetic_preset_proves_the_table_is_the_extensibility_point() {
        let (state, _, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        let duo = crate::board_presets::BoardPreset {
            name: "test-duo",
            stacks: &["A", "B"],
        };
        let outcomes = vs.apply_board_preset(&state, &duo);
        let commands: Vec<_> = outcomes
            .iter()
            .filter_map(|o| match o {
                FlowOutcome::Command(c) => Some(c.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(commands.len(), 2);
        for (order, command) in commands.iter().enumerate() {
            match command {
                StateCommand::CreateStack {
                    title, order: o, ..
                } => {
                    assert_eq!(title, duo.stacks[order]);
                    assert_eq!(*o, i64::try_from(order).unwrap());
                }
                other => panic!("typed outcome expected, got {other:?}"),
            }
        }
    }

    #[test]
    fn the_preset_dispatch_refuses_without_a_board() {
        let (mut state, _, _, _) = fixture();
        state.boards.clear();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        assert_eq!(
            vs.apply_board_preset(&state, &crate::board_presets::DEFAULT_KANBAN),
            Vec::new(),
            "the refusal is inert"
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

    /// The two-board fixture's fixed render date (the clock-discipline
    /// seam for committed artifacts that embed clock-derived text).
    pub(super) fn test_today() -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 23).expect("fixed date")
    }

    /// W-15/#1049 mechanism: the kiosk belt steps a slipped-past
    /// habit-variant mount back to the board (the dead-subject arm's
    /// twin, one mechanism).
    #[test]
    fn the_kiosk_belt_steps_a_mounted_habit_subject_back() {
        let (state, _b1, _, _, _) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        let habit = taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7());
        let _ = vs.open_habit_editor(habit);
        assert_eq!(vs.view(), ViewId::TaskDetail);
        assert_eq!(vs.subject_habit(), Some(habit));
        // The gate slipped: the belt (kiosk) fires on the same pass.
        let outcomes = vs.kiosk_habit_belt(true);
        assert!(matches!(outcomes[..], [FlowOutcome::RefreshNow]));
        assert_eq!(vs.view(), ViewId::Board, "the view stepped back");
        assert_eq!(vs.subject_habit(), None, "the mount is cleared");
        // Idempotent: a second pass is inert.
        assert_eq!(vs.kiosk_habit_belt(true), Vec::new());
    }

    /// W-15/#1049 desktop leg: the belt is desktop-inert — a mounted
    /// habit subject survives the pass untouched.
    #[test]
    fn the_kiosk_belt_is_desktop_inert() {
        let (state, _b1, _, _, _) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        let habit = taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7());
        let _ = vs.open_habit_editor(habit);
        assert_eq!(vs.kiosk_habit_belt(false), Vec::new());
        assert_eq!(vs.view(), ViewId::TaskDetail, "desktop keeps the mount");
        assert_eq!(vs.subject_habit(), Some(habit));
    }

    /// #1303 (the origin-snapshot law): `open_habit_editor` records
    /// the root it mounts over, and `close_habit_editor` restores it
    /// — the Today round-trip and the Board round-trip both return
    /// to where the editor came from, never the board hard-route.
    #[test]
    fn the_habit_editor_round_trips_to_the_recorded_origin_root() {
        let (state, _b1, _, _, _) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        let habit = taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7());
        // The Today origin: open by root swap, close restores Today.
        vs.set_view(ViewId::Today);
        let _ = vs.open_habit_editor(habit);
        assert_eq!(vs.view(), ViewId::TaskDetail);
        assert_eq!(vs.subject_habit(), Some(habit), "the mount carries");
        let outcomes = vs.close_habit_editor();
        assert!(matches!(outcomes[..], [FlowOutcome::RefreshNow]));
        assert_eq!(vs.view(), ViewId::Today, "Today stays Today");
        assert_eq!(vs.subject_habit(), None, "the mount fact cleared");
        // The Board origin: the same round-trip stays on Board.
        vs.set_view(ViewId::Board);
        let _ = vs.open_habit_editor(habit);
        assert_eq!(vs.view(), ViewId::TaskDetail);
        let _ = vs.close_habit_editor();
        assert_eq!(vs.view(), ViewId::Board, "Board stays Board");
        // The unrecorded-origin fallback (the kiosk belt's
        // restored-snapshot posture): the board repair rule.
        let _ = vs.open_habit_editor(habit);
        vs.set_view(ViewId::Status); // a foreign landing retires the record
        let _ = vs.close_habit_editor();
        assert_eq!(vs.view(), ViewId::Board, "no record falls back to Board");
    }

    pub(super) fn two_board_fixture() -> (AppState, BoardId, BoardId, [StackId; 3], [TaskId; 4]) {
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
                    authors: taskboard_domain::entities::BoardAuthors::default(),
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

    /// UX R-1 D-1 (issue #785): the roster's one-shot select — armed
    /// at unarchive, consumed exactly when the engine's apply makes
    /// the board live again, pruned when the board is deleted first.
    #[test]
    fn unarchive_board_arms_a_pending_select_consumed_once_live() {
        let (state, b1, b2, _, _) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.select_board(&state, b1);

        // The verb dispatches the exact unarchive + refresh and arms
        // the one-shot; the selection holds until the apply lands.
        let outcomes = vs.unarchive_board(b2);
        assert_eq!(outcomes.len(), 2);
        assert!(matches!(
            &outcomes[0],
            FlowOutcome::Command(StateCommand::SetBoardArchived {
                archived: false,
                ..
            })
        ));
        assert!(matches!(outcomes[1], FlowOutcome::RefreshNow));
        assert_eq!(vs.board(), Some(b1), "the apply has not landed yet");

        // The engine applies; the next reconcile selects the restored
        // board exactly once (the one-shot is consumed).
        let mut applied = state.clone();
        applied.boards.get_mut(&b2).expect("board").archived = false;
        vs.reconcile(&applied);
        assert_eq!(vs.board(), Some(b2));
        vs.reconcile(&applied);
        assert_eq!(vs.board(), Some(b2), "consumed — a no-op thereafter");

        // A deleted target prunes (a stale arm never lingers).
        let (state, _, b2, _, _) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.unarchive_board(b2);
        let mut deleted = state.clone();
        deleted.boards.get_mut(&b2).expect("board").deleted = true;
        vs.reconcile(&deleted);
        assert_eq!(vs.board(), Some(b1), "the deletion repairs normally");

        // The engine's typed refusals (the escape column): an engine
        // that refuses leaves the board archived forever, so the
        // armed target never consumes — the selection stays put
        // rather than landing on a non-live board.
        let (state, b1, b2, _, _) = two_board_fixture();
        let mut still_archived = state.clone();
        still_archived.boards.get_mut(&b2).expect("board").archived = true;
        let mut vs = ViewState::new();
        vs.reconcile(&still_archived);
        vs.select_board(&still_archived, b1);
        vs.unarchive_board(b2);
        vs.reconcile(&still_archived);
        assert_eq!(vs.board(), Some(b1), "a refused apply never selects");
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
    fn detail_entry_never_arms_and_begin_edit_title_seeds_the_mirror() {
        // Recorded decision (round-3 chunk 1, plan §6 — the F26
        // rework): activate_focused is the PLAIN detail open and
        // never arms an edit flow; the title editor arms only via
        // begin_edit_title.
        let (state, _, [s0, _], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.activate_focused(&state);
        assert_eq!(vs.view(), ViewId::TaskDetail);
        assert_eq!(vs.subject_task(), Some(a));
        assert_eq!(*vs.flow(), Flow::None, "entry never arms (plan §6)");
        let outcomes = vs.begin_edit_title(&state, a);
        assert_eq!(
            *vs.flow(),
            Flow::EditTitle {
                subject: EditSubject::Task(a)
            }
        );
        assert_eq!(
            vs.buffer(),
            "task-0",
            "the mirror starts at the current title"
        );
        assert!(matches!(
            outcomes.first(),
            Some(FlowOutcome::SetMode(Mode::Insert))
        ));
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
        // Recorded decision (round-3 chunk 1, plan §6): entry does
        // not arm, so there is nothing to discard — `i` arms the
        // editor directly from display mode.
        assert_eq!(*vs.flow(), Flow::None);
        // Still in TaskDetail (display mode): `i` edits the subject.
        let outcomes = vs.edit_subject(&state);
        assert_eq!(
            *vs.flow(),
            Flow::EditTitle {
                subject: EditSubject::Task(a)
            }
        );
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

    // UX-R8 D-1: the walk is slot-shaped now — the stack-level cycle
    // assertions below were retargeted to the flattened slot sequence
    // (same walk verb, finer grain; the append confirm moved to
    // `move_flow_confirms_the_aimed_slot_order`).
    #[test]
    fn move_flow_walks_slots_and_wraps() {
        let (state, _, [s0, s1], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.start_move(&state);
        assert_eq!(
            *vs.flow(),
            Flow::Move {
                task: a,
                target: s0,
                slot: 1
            },
            "the flow arms at the slot after the task's own position"
        );
        // Flattened slots: s0 gaps 0..=2 (tasks a, b), s1 gaps 0..=1
        // (task c). Forward from (s0, 1) walks top→bottom, then the
        // next stack.
        vs.cycle_move_target(&state, 1);
        assert_eq!(
            *vs.flow(),
            Flow::Move {
                task: a,
                target: s0,
                slot: 2
            }
        );
        vs.cycle_move_target(&state, 1);
        assert_eq!(
            *vs.flow(),
            Flow::Move {
                task: a,
                target: s1,
                slot: 0
            }
        );
        vs.cycle_move_target(&state, -1);
        assert_eq!(
            *vs.flow(),
            Flow::Move {
                task: a,
                target: s0,
                slot: 2
            },
            "the walk steps back one slot"
        );
        vs.cycle_move_target(&state, -1);
        vs.cycle_move_target(&state, -1);
        vs.cycle_move_target(&state, -1);
        assert_eq!(
            *vs.flow(),
            Flow::Move {
                task: a,
                target: s1,
                slot: 1
            },
            "walking back past (s0, 0) wraps to the last slot"
        );

        let outcomes = vs.confirm_move(&state);
        assert_eq!(
            outcomes[..1],
            [FlowOutcome::Command(StateCommand::MoveTask {
                id: a,
                stack: s1,
                order: 1, // gap after c(0): last.order + 1
            })][..]
        );
        assert_eq!(*vs.flow(), Flow::None);
    }

    #[test]
    fn move_flow_confirms_the_aimed_slot_order() {
        let (state, _, [s0, s1], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.start_move(&state);
        // Walk to s1's gap 0 (before the only task c(0)): the slot
        // order is first.order - 1 — the between/ends grammar of
        // `slot_order`, not the append.
        vs.cycle_move_target(&state, 1);
        vs.cycle_move_target(&state, 1);
        assert_eq!(
            *vs.flow(),
            Flow::Move {
                task: a,
                target: s1,
                slot: 0
            }
        );
        let outcomes = vs.confirm_move(&state);
        assert_eq!(
            outcomes[..1],
            [FlowOutcome::Command(StateCommand::MoveTask {
                id: a,
                stack: s1,
                order: -1,
            })][..]
        );
        assert_eq!(*vs.flow(), Flow::None);
    }

    // -- the same-stack reorder mint (B3, 2026-11-24, #1247) ---------------

    #[test]
    fn move_flow_same_stack_confirm_mints_the_engine_side_gap_reorder() {
        // U1: a same-stack aim mints `TaskReorder` at the aimed gap —
        // never a `MoveTask` order value (the midpoint mint is retired
        // from every move path). The walked-back in-place slot still
        // dispatches; the engine's in-place gaps absorb it as the
        // accepted no-op.
        let (state, _, [s0, _], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.start_move(&state);
        // The flow arms at a's own gap + 1 (the in-place bottom edge):
        // the confirm dispatches `TaskReorder` there and the engine
        // no-ops it.
        let outcomes = vs.confirm_move(&state);
        assert_eq!(
            outcomes[..1],
            [FlowOutcome::Command(StateCommand::TaskReorder {
                id: a,
                target: 1,
            })][..]
        );
        // Gap 0 (above the first row) mints the crossing gap.
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.start_move(&state);
        vs.cycle_move_target(&state, -1); // wrap back to (s0, 0)
        let outcomes = vs.confirm_move(&state);
        assert_eq!(
            outcomes[..1],
            [FlowOutcome::Command(StateCommand::TaskReorder {
                id: a,
                target: 0,
            })][..]
        );
    }

    #[test]
    fn pointer_drop_on_an_adjacent_pair_mints_the_crossing_gap() {
        // U2, #1247's user-visible symptom pinned at the flow level:
        // dragging a card to the gap beyond its adjacent neighbour
        // mints the engine-side reorder that swaps them — the old
        // midpoint mint landed on an occupied order and resolved the
        // tie by mint-order luck (the silent above/below lottery).
        let (state, _, [s0, _], [_a, b, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_drag_move(&state, b); // b is s0's second row
        vs.drag_gap(&state, s0, 0); // the gap above a
        let outcomes = vs.confirm_drag_move(&state);
        assert_eq!(
            outcomes[..1],
            [FlowOutcome::Command(StateCommand::TaskReorder {
                id: b,
                target: 0,
            })][..]
        );
    }

    #[test]
    fn keyboard_confirm_and_pointer_drop_mint_equal_same_stack_reorders() {
        // U3, the one-construction-site law carried onto the reorder
        // grammar: the same (task, same-stack gap) builds ONE command
        // shape through `move_aim_command` — the pins compare the
        // commands, whatever surface produced them.
        let (state, _, [s0, _], [_, b, ..]) = fixture();
        // b is s0's second row: gap 0 is a crossing gap for it (a's
        // own gap 0 would be the in-place silent confirm).
        let keyboard = {
            let mut vs = ViewState::new();
            vs.reconcile(&state);
            vs.activate_card(b, s0);
            vs.start_move(&state);
            vs.cycle_move_target(&state, -1); // (s0, 1) → (s0, 0)
            vs.cycle_move_target(&state, -1);
            vs.confirm_move(&state)
        };
        let drag = {
            let mut vs = ViewState::new();
            vs.reconcile(&state);
            vs.begin_drag_move(&state, b);
            vs.drag_gap(&state, s0, 0);
            vs.confirm_drag_move(&state)
        };
        let pick = |outcomes: &[FlowOutcome]| {
            outcomes.iter().find_map(|o| match o {
                FlowOutcome::Command(cmd @ StateCommand::TaskReorder { .. }) => Some(cmd.clone()),
                _ => None,
            })
        };
        assert_eq!(pick(&keyboard), pick(&drag), "one reorder mint");
        assert!(
            pick(&keyboard).is_some(),
            "the same-stack legs mint the reorder verb, not MoveTask"
        );
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

    // -- the directional kept arms (J3 US-2, round 9 chunk 9) ------------

    #[test]
    fn move_flow_column_hop_keeps_the_row_and_clamps_at_the_edges() {
        let (state, _, [s0, s1], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.start_move(&state);
        assert_eq!(
            *vs.flow(),
            Flow::Move {
                task: a,
                target: s0,
                slot: 1
            },
            "the flow arms at the slot after the task's own position"
        );
        // Right hops the column; the picked row index survives the
        // hop, clamped to the shorter column's gap count (s1: one
        // task, gaps 0..=1).
        vs.cycle_move_stack_column(&state, 1);
        assert_eq!(
            *vs.flow(),
            Flow::Move {
                task: a,
                target: s1,
                slot: 1
            }
        );
        // Left hops back; the edge beyond s0 holds (clamp, no wrap).
        vs.cycle_move_stack_column(&state, -1);
        assert_eq!(
            *vs.flow(),
            Flow::Move {
                task: a,
                target: s0,
                slot: 1
            }
        );
        vs.cycle_move_stack_column(&state, -1);
        assert_eq!(
            *vs.flow(),
            Flow::Move {
                task: a,
                target: s0,
                slot: 1
            },
            "the board's left edge holds"
        );
        // The move survives every hop (confirm still completes).
        assert_eq!(*vs.cancel_move(), [FlowOutcome::RefreshNow][..]);
    }

    #[test]
    fn move_flow_column_hop_without_a_live_target_is_inert() {
        let (state, _, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        assert_eq!(vs.cycle_move_stack_column(&state, 1), Vec::new());
    }

    // -- the planner's pane grammar (J3 R-5, round 9 chunk 9) ------------

    #[test]
    fn planner_pane_switch_toggles_and_a_fresh_open_reanchors_on_boxes() {
        let (state, _, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        // Outside the planner the switch is the typed no-op.
        assert_eq!(vs.switch_plan_pane(1), Vec::new());
        assert_eq!(vs.plan_focus_pane(), 0, "the no-op leaves the pane");
        vs.open_plan_next_day();
        assert_eq!(vs.plan_focus_pane(), 0, "a fresh open anchors on boxes");
        assert_eq!(vs.switch_plan_pane(1), vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.plan_focus_pane(), 1, "Right lands on the pool");
        // The two-value ring: a second Right toggles back.
        vs.switch_plan_pane(1);
        assert_eq!(vs.plan_focus_pane(), 0);
        vs.switch_plan_pane(-1);
        assert_eq!(vs.plan_focus_pane(), 1, "Left from boxes wraps to the pool");
        // A fresh open re-anchors on the boxes pane.
        vs.close_plan_next_day();
        vs.open_plan_next_day();
        assert_eq!(vs.plan_focus_pane(), 0);
    }

    #[test]
    fn planner_pane_switch_with_a_zero_direction_is_a_stay() {
        let (state, _, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.open_plan_next_day();
        // The degenerate direction is an in-range step on both panes:
        // the pane holds (it does not toggle the ring).
        assert_eq!(vs.switch_plan_pane(0), vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.plan_focus_pane(), 0);
        vs.switch_plan_pane(1);
        assert_eq!(vs.plan_focus_pane(), 1);
        assert_eq!(vs.switch_plan_pane(0), vec![FlowOutcome::RefreshNow]);
        assert_eq!(
            vs.plan_focus_pane(),
            1,
            "the degenerate stroke holds the pool"
        );
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
    fn board_create_flow_commits_the_default_board_color() {
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
                color: taskboard_domain::Color::new(
                    taskboard_domain::entities::DEFAULT_BOARD_COLOR,
                ),
            })][..]
        );
        assert_eq!(*vs.flow(), Flow::None);
    }

    #[test]
    fn board_color_picker_arms_over_the_selected_board_and_commits_the_swatch_hex() {
        let (state, b1, _, _) = fixture();
        let mut vs = ViewState::new();
        // Board-less: inert (the adapter's arm refuses with nothing).
        let empty = AppState::default();
        assert_eq!(vs.begin_board_color(&empty), Vec::new());
        assert_eq!(*vs.flow(), Flow::None);
        vs.reconcile(&state);
        vs.select_board(&state, b1);
        // Arming writes the flow and dispatches nothing.
        assert_eq!(vs.begin_board_color(&state), vec![FlowOutcome::RefreshNow]);
        assert_eq!(*vs.flow(), Flow::BoardColorPick { board: b1 });
        // Re-arm over the armed board: the typed no-op (the strip is
        // already up).
        assert_eq!(vs.begin_board_color(&state), Vec::new());
        assert_eq!(*vs.flow(), Flow::BoardColorPick { board: b1 });
        // The commit: the exact SetBoardColor over the armed board
        // with the swatch's hex; the flow ends.
        assert_eq!(
            vs.commit_board_color("31cc7c"),
            vec![
                FlowOutcome::Command(StateCommand::SetBoardColor {
                    board: b1,
                    color: taskboard_domain::Color::new("31cc7c"),
                }),
                FlowOutcome::RefreshNow,
            ]
        );
        assert_eq!(*vs.flow(), Flow::None);
        // A commit without the armed flow is inert.
        assert_eq!(vs.commit_board_color("31cc7c"), Vec::new());
    }

    #[test]
    fn board_color_picker_disarms_on_cancel_and_foreign_settle() {
        let (state, b1, _, _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.select_board(&state, b1);
        // The dismissal (the strip's seam): ends the flow, dispatches
        // nothing.
        vs.begin_board_color(&state);
        assert_eq!(vs.cancel_board_color(), vec![FlowOutcome::RefreshNow]);
        assert_eq!(*vs.flow(), Flow::None);
        // The foreign settle (the settle law): disarms without
        // dispatching.
        vs.begin_board_color(&state);
        assert_eq!(vs.settle_foreign(&state), vec![FlowOutcome::RefreshNow]);
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
    fn delete_stack_arms_then_confirms_dispatching_exactly_once() {
        let (state, _board_id, [s0, _], _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        // The first stroke arms: the flow holds, zero commands.
        assert_eq!(vs.delete_stack(&state), vec![FlowOutcome::RefreshNow]);
        assert!(matches!(
            vs.flow(),
            Flow::ConfirmDelete(DeleteTarget::Stack(s)) if *s == s0
        ));
        // The repeat stroke confirms: the exact command, the flow
        // cleared.
        assert_eq!(
            vs.delete_stack(&state),
            vec![
                FlowOutcome::Command(StateCommand::DeleteStack { id: s0 }),
                FlowOutcome::RefreshNow
            ]
        );
        assert_eq!(*vs.flow(), Flow::None);
    }

    #[test]
    fn delete_board_arms_per_subject_and_rearm_swaps() {
        let (state, board_id, [s0, _], _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        // Arm the board.
        assert_eq!(vs.delete_board(&state), vec![FlowOutcome::RefreshNow]);
        assert!(matches!(
            vs.flow(),
            Flow::ConfirmDelete(DeleteTarget::Board(b)) if *b == board_id
        ));
        assert_eq!(
            vs.delete_board(&state),
            vec![
                FlowOutcome::Command(StateCommand::DeleteBoard { board: board_id }),
                FlowOutcome::RefreshNow
            ]
        );
        // Re-arm over an armed subject swaps to the new one (one
        // armed subject per the shared grammar).
        assert_eq!(vs.delete_board(&state), vec![FlowOutcome::RefreshNow]);
        assert!(matches!(
            vs.flow(),
            Flow::ConfirmDelete(DeleteTarget::Board(_))
        ));
        assert_eq!(vs.delete_stack(&state), vec![FlowOutcome::RefreshNow]);
        assert!(matches!(
            vs.flow(),
            Flow::ConfirmDelete(DeleteTarget::Stack(s)) if *s == s0
        ));
    }

    #[test]
    fn armed_delete_foreign_settle_and_vanish_disarm() {
        let (state, board_id, [s0, _], _) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.delete_board(&state);
        assert!(matches!(vs.flow(), Flow::ConfirmDelete(_)));
        // A foreign settle disarms without dispatching (the settle
        // rule's strip arm).
        assert_eq!(vs.cancel_confirm_delete(), vec![FlowOutcome::RefreshNow]);
        assert_eq!(*vs.flow(), Flow::None);
        // Re-arm the stack, then delete its subject out-of-band: the
        // reconcile vanish ends the flow, nothing dispatched.
        vs.delete_stack(&state);
        assert!(matches!(vs.flow(), Flow::ConfirmDelete(_)));
        let mut vanished = state.clone();
        vanished.stacks.get_mut(&s0).expect("fixture stack").deleted = true;
        vs.reconcile(&vanished);
        assert_eq!(*vs.flow(), Flow::None);
        let _ = board_id;
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
                authors: taskboard_domain::entities::BoardAuthors::default(),
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
        // Recorded decision (round-3 chunk 1, plan §6): activation
        // never arms — it is the plain detail open. At this layer it
        // settles nothing (the adapter's resolver settles the foreign
        // flow before the re-dispatch); the editor arms via
        // begin_edit_title.
        assert!(matches!(vs.flow(), Flow::Create { .. }));
        vs.begin_edit_title(&state, a);
        assert!(
            matches!(vs.flow(), Flow::EditTitle { .. }),
            "the title action arms the editor"
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

    // -- stack column widths (round-1 D16) ---------------------------------

    /// T-3 (r4 D8) — the keyboard semantics: the step moves the
    /// boundary (the neighbour seesaws), the floor clamps at the
    /// content minimum with the neighbour absorbing the rest, the
    /// pair ceiling holds both minimums, and every step emits
    /// `[StackWidthsChanged, RefreshNow]`.
    #[test]
    fn resize_focused_stack_steps_the_pair_and_clamps() {
        use crate::widths::step_column;

        fn row<'a>(stacks: &'a [StackId], mins: &'a [u32], widths: &'a [u32]) -> RowStep<'a> {
            RowStep {
                stacks,
                widths,
                mins,
                step: i64::from(crate::widths::STACK_WIDTH_STEP_PX),
                scale: 1.0,
            }
        }
        let (state, board, [s0, s1], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);

        let stacks = [s0, s1];
        let mins = [120u32, 120];
        let mut widths = vec![456u32, 456];

        let outcomes = vs.resize_focused_stack(&state, 1, &row(&stacks, &mins, &widths));
        assert_eq!(
            outcomes,
            vec![FlowOutcome::StackWidthsChanged, FlowOutcome::RefreshNow]
        );
        widths = step_column(&widths, 0, 1, 32, &mins);
        assert_eq!(
            vs.stack_width(board, s0),
            Some(488),
            "exact step arithmetic"
        );
        assert_eq!(
            vs.stack_width(board, s1),
            Some(424),
            "the neighbour seesaws"
        );

        // Narrowing clamps at the content minimum; the neighbour takes
        // the rest of the row.
        for _ in 0..20 {
            vs.resize_focused_stack(&state, -1, &row(&stacks, &mins, &widths));
            widths = step_column(&widths, 0, -1, 32, &mins);
        }
        assert_eq!(vs.stack_width(board, s0), Some(120), "the floor clamps");
        assert_eq!(vs.stack_width(board, s1), Some(792), "sum preserved");

        // Widening past the neighbour's minimum sticks (the pair
        // ceiling), but the step still emits its outcome pair.
        for _ in 0..60 {
            vs.resize_focused_stack(&state, 1, &row(&stacks, &mins, &widths));
            widths = step_column(&widths, 0, 1, 32, &mins);
        }
        assert_eq!(vs.stack_width(board, s0), Some(792));
        assert_eq!(vs.stack_width(board, s1), Some(120));
        let outcomes = vs.resize_focused_stack(&state, 1, &row(&stacks, &mins, &widths));
        assert_eq!(
            outcomes,
            vec![FlowOutcome::StackWidthsChanged, FlowOutcome::RefreshNow],
            "a clamped step still reports the snapshot"
        );
    }

    /// T-3 — target resolution: focused → selected → first; inert
    /// without any stack and on a one-column board (no boundary).
    #[test]
    fn resize_targets_focused_then_selected_then_first_and_is_inert_without_stacks() {
        use crate::widths::step_column;
        let (state, board, [s0, s1], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        let stacks = [s0, s1];
        let mins = [120u32, 120];
        let widths = [456u32, 456];
        let row = RowStep {
            stacks: &stacks,
            widths: &widths,
            mins: &mins,
            step: 32,
            scale: 1.0,
        };

        // No focus, no selection: the first stack.
        vs.resize_focused_stack(&state, 1, &row);
        assert_eq!(vs.selected_stack(), None);
        assert_eq!(vs.stack_width(board, s0), Some(488));

        // Selection wins over first.
        vs.select_stack(&state, s1);
        vs.resize_focused_stack(&state, 1, &row);
        assert_eq!(vs.stack_width(board, s1), Some(488), "the selected stack");

        // Focus wins over selection.
        vs.activate_card(a, s0);
        vs.resize_focused_stack(&state, -1, &row);
        assert_eq!(
            vs.stack_width(board, s0),
            Some(step_column(&widths, 0, -1, 32, &mins)[0]),
            "the focused stack was addressed"
        );

        // A stack-less state is inert.
        let mut empty = ViewState::new();
        let bare = AppState::default();
        empty.reconcile(&bare);
        assert_eq!(empty.resize_focused_stack(&bare, 1, &row), Vec::new());

        // A one-column board has no boundary: inert, nothing written.
        let (one_stack_state, _, [only, ..], _) = fixture();
        let mut one = ViewState::new();
        one.reconcile(&one_stack_state);
        let single = [only];
        let one_row = RowStep {
            stacks: &single,
            widths: &[600],
            mins: &mins[..1],
            step: 32,
            scale: 1.0,
        };
        assert_eq!(
            one.resize_focused_stack(&one_stack_state, 1, &one_row),
            Vec::new()
        );
        assert_eq!(one.stack_width(board, only), None, "nothing written");
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
            BTreeMap::from([
                (sa, 96),
                (sb, crate::widths::STACK_WIDTH_READABILITY_MIN_PX + 80),
            ]),
        );
        vs.seed_stack_widths(widths);
        vs.reconcile(&state);

        assert_eq!(
            vs.stack_width(b1, sa),
            Some(crate::widths::STACK_WIDTH_READABILITY_MIN_PX),
            "the legacy 96-px entry healed at load"
        );
        assert_eq!(
            vs.stack_width(b1, sb),
            Some(crate::widths::STACK_WIDTH_READABILITY_MIN_PX + 80),
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
                    authors: taskboard_domain::entities::BoardAuthors::default(),
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
        let outcomes = vs.palette_jump(&state, SearchTarget::Task(needle), test_today());
        assert!(outcomes.contains(&FlowOutcome::SetMode(Mode::Normal)));
        assert_eq!(vs.flow(), &Flow::None);
        assert_eq!(vs.board(), Some(b1));
        assert_eq!(vs.focused_task(), Some(needle));

        // Stack hit: the stack subject + selection, its first task
        // focused, StackDetail entered.
        let mut vs = ViewState::new();
        vs.select_board(&state, b0);
        vs.open_palette();
        vs.palette_jump(&state, SearchTarget::Stack(s1), test_today());
        assert_eq!(vs.view(), ViewId::StackDetail);
        assert_eq!(vs.board(), Some(b1));
        assert_eq!(vs.focused_task(), Some(needle));

        // Board hit: plain board select.
        let mut vs = ViewState::new();
        vs.select_board(&state, b1);
        vs.open_palette();
        vs.palette_jump(&state, SearchTarget::Board(b0), test_today());
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
        let outcomes = vs.palette_jump(&state, dead, test_today());
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
            Flow::EditTitle {
                subject: EditSubject::Task(a),
            },
            Flow::EditDate {
                task: a,
                field: crate::view_state::DateField::Due,
            },
            Flow::EditDescription {
                subject: EditSubject::Task(a),
            },
            Flow::Move {
                task: a,
                target: s0,
                slot: 0,
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
                gap: None,
            },
        ];
        for flow in flows {
            for mode in [KioskMode::Board, KioskMode::Today] {
                let mut vs = ViewState::new();
                vs.reconcile(&state);
                vs.arm_flow(flow.clone());
                vs.set_buffer("in-flight".to_owned());
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
    use super::tests::{epoch, stack, task, two_board_fixture};
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
                authors: taskboard_domain::entities::BoardAuthors::default(),
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
                authors: taskboard_domain::entities::StackAuthors::default(),
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

    // -- plan-next-day surface (#622, D-1/D-4): the Settings twin --

    #[test]
    fn the_plan_next_day_surface_choreography_is_exact() {
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        let state = editing_state(stack);

        // From StackDetail: open → context + push; close → exact
        // restore (the settings-surface twin).
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        assert!(vs.enter(&state, ViewId::StackDetail));
        assert_eq!(vs.context(), ContextId::StackDetail);
        assert_eq!(vs.open_plan_next_day(), vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.view(), ViewId::PlanNextDay);
        assert_eq!(vs.context(), ContextId::PlanNextDay);
        assert_eq!(vs.close_plan_next_day(), vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.view(), ViewId::StackDetail);

        // The board default: closing with nothing recorded lands on
        // the board.
        let mut vs = ViewState::new();
        vs.open_plan_next_day();
        vs.close_plan_next_day();
        assert_eq!(vs.view(), ViewId::Board);

        // The cycle verbs are inert while open (both directions).
        let mut vs = ViewState::new();
        vs.open_plan_next_day();
        assert_eq!(vs.next_view(&state), Vec::new());
        assert_eq!(vs.prev_view(&state), Vec::new());
        assert_eq!(vs.view(), ViewId::PlanNextDay);

        // The surface never enters through the cycle (the enter-guard
        // arm): the ring walk from the board never reaches it, from
        // any cycle member.
        let mut vs = ViewState::new();
        for _ in 0..5 {
            vs.next_view(&state);
            assert_ne!(vs.view(), ViewId::PlanNextDay);
            assert_ne!(vs.view(), ViewId::Settings);
        }
        for _ in 0..5 {
            vs.prev_view(&state);
            assert_ne!(vs.view(), ViewId::PlanNextDay);
        }
    }

    #[test]
    fn the_plan_place_flow_reconciles_on_subject_death_only() {
        let (state, _, _, _, [task, ..]) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        assert_eq!(vs.open_plan_place(task), vec![FlowOutcome::RefreshNow]);
        assert!(matches!(vs.flow(), Flow::PlanPlace { task: t } if *t == task));

        // A view change does NOT drop it (the mount gate keys on the
        // subject, not the view — the entries overlay's predicate
        // minus the view strand).
        vs.set_view(ViewId::Board);
        vs.reconcile(&state);
        assert!(matches!(vs.flow(), Flow::PlanPlace { .. }));

        // The subject dying drops it.
        let mut fresh = state.clone();
        let dead = fresh.tasks.get_mut(&task).expect("live");
        dead.deleted = true;
        vs.reconcile(&fresh);
        assert!(matches!(vs.flow(), Flow::None));

        // The discard is the form → pane rung; from no flow it is
        // inert.
        let mut vs = ViewState::new();
        vs.open_plan_place(task);
        assert_eq!(vs.close_plan_place(), vec![FlowOutcome::RefreshNow]);
        assert!(matches!(vs.flow(), Flow::None));
        assert_eq!(vs.close_plan_place(), Vec::new());
    }

    #[test]
    fn remove_plan_box_routes_the_unplace_mask_and_ends_the_flow() {
        let (state, _, _, _, [task, ..]) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.open_plan_place(task);
        let outcomes = vs.remove_plan_box(task);
        assert_eq!(outcomes.len(), 2);
        assert!(matches!(
            &outcomes[0],
            FlowOutcome::Command(StateCommand::UpdateTask { id, changes })
                if *id == task && changes.scheduled == Some(None)
        ));
        assert_eq!(outcomes[1], FlowOutcome::RefreshNow);
        assert!(matches!(vs.flow(), Flow::None));
    }

    // -- nav.show-task (round-3 D23) ---------------------------------------

    #[test]
    fn show_task_targets_the_tasks_board_stack_and_focus() {
        let (state, b1, b2, [sa, sb, sc], [.., tc, td]) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        // Seed a foreign selection: board one, first stack, no focus.
        vs.select_board(&state, b1);

        // A cross-board target: td lives on board two's sc.
        let outcomes = vs.show_task(&state, td);
        assert_eq!(outcomes, vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.view(), ViewId::Board, "the task's home view");
        assert_eq!(vs.board(), Some(b2), "the task's board");
        assert_eq!(vs.selected_stack(), Some(sc), "the task's stack");
        assert_eq!(vs.focused_task(), Some(td));

        // A same-board target retargets stack and focus too: tc lives
        // on board one's second stack.
        vs.show_task(&state, tc);
        assert_eq!(vs.board(), Some(b1));
        assert_eq!(vs.selected_stack(), Some(sb));
        assert_eq!(vs.focused_task(), Some(tc));
        let _ = sa;
    }

    #[test]
    fn show_task_is_inert_on_a_stale_target() {
        let (state, b1, _, _, _) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.select_board(&state, b1);
        let ghost = TaskId::from_uuid(uuid::Uuid::now_v7());
        let focus_before = vs.focused_task();

        assert_eq!(vs.show_task(&state, ghost), Vec::new(), "no outcomes");
        assert_eq!(vs.view(), ViewId::Board, "no repaint, no retarget");
        assert_eq!(vs.focused_task(), focus_before, "the focus is untouched");
        assert_eq!(vs.board(), Some(b1), "the selection is untouched");
    }

    #[test]
    fn show_task_switches_to_the_board_from_any_view() {
        let (state, b1, _, _, [ta, ..]) = two_board_fixture();
        // From Today.
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.focus_today();
        vs.show_task(&state, ta);
        assert_eq!(vs.view(), ViewId::Board);
        assert_eq!(vs.focused_task(), Some(ta));

        // From StackDetail.
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.select_board(&state, b1);
        vs.next_view(&state);
        assert_eq!(vs.view(), ViewId::StackDetail);
        vs.show_task(&state, ta);
        assert_eq!(vs.view(), ViewId::Board);

        // From Settings: one semantics, no per-source branching; the
        // return snapshot stays as-is (overwritten at the next open).
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.select_board(&state, b1);
        vs.open_settings();
        vs.show_task(&state, ta);
        assert_eq!(vs.view(), ViewId::Board);
        assert_eq!(vs.focused_task(), Some(ta));
    }

    #[test]
    fn show_task_lands_on_a_done_task() {
        let (mut state, b1, _, _, [ta, ..]) = two_board_fixture();
        state.tasks.get_mut(&ta).expect("task").done = Some(epoch());
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.focus_today();

        // Done tasks keep their board cards: the landing is the same
        // retarget, no special case.
        assert_eq!(vs.show_task(&state, ta), vec![FlowOutcome::RefreshNow]);
        assert_eq!(vs.view(), ViewId::Board);
        assert_eq!(vs.board(), Some(b1));
        assert_eq!(vs.focused_task(), Some(ta));
    }

    #[test]
    fn palette_jump_task_arm_matches_show_task() {
        let (state, b1, _, [.., sb, _], [.., tc, _]) = two_board_fixture();
        // Two machines, same starting shape: the palette Task arm and
        // a direct show_task must land identically (the seam
        // discipline's equivalence pin).
        let mut jumped = ViewState::new();
        jumped.reconcile(&state);
        jumped.select_board(&state, b1);
        jumped.open_palette();
        let jumped_outcomes = jumped.palette_jump(
            &state,
            SearchTarget::Task(tc),
            chrono::NaiveDate::from_ymd_opt(2026, 11, 23).expect("fixed date"),
        );

        let mut shown = ViewState::new();
        shown.reconcile(&state);
        shown.select_board(&state, b1);
        let show_outcomes = shown.show_task(&state, tc);

        assert_eq!(jumped.focused_task(), shown.focused_task());
        assert_eq!(jumped.board(), shown.board());
        assert_eq!(jumped.selected_stack(), Some(sb));
        assert_eq!(shown.selected_stack(), Some(sb));
        assert_eq!(jumped.view(), shown.view());
        // The palette arm additionally ends the flow (the close
        // ending); the show_task outcomes are its prefix.
        assert_eq!(jumped_outcomes.len(), show_outcomes.len() + 1);
        assert_eq!(&jumped_outcomes[..show_outcomes.len()], &show_outcomes[..]);
        assert!(matches!(
            jumped_outcomes.last(),
            Some(FlowOutcome::SetMode(crate::router::Mode::Normal))
        ));
    }

    // -- the toolbar switcher projection (round-3 D22) ---------------------

    #[test]
    fn switcher_selection_projects_views_and_the_pushed_over_views() {
        let (state, b1, _, _, _) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.select_board(&state, b1);

        // The board branch: the board view and both deeper views keep
        // the board segment lit (nav.focus-board returns from them).
        assert_eq!(vs.switcher_selection(), 0);
        vs.next_view(&state);
        assert_eq!(vs.view(), ViewId::StackDetail);
        assert_eq!(vs.switcher_selection(), 0);
        vs.next_view(&state);
        assert_eq!(vs.view(), ViewId::TaskDetail);
        assert_eq!(vs.switcher_selection(), 0);

        // Today lights the second segment.
        vs.focus_today();
        assert_eq!(vs.switcher_selection(), 1);

        // Settings keeps the pushed-over view's segment lit (the root
        // is read through the push); nothing pushed reads as the
        // board (a fresh machine never opened settings).
        vs.open_settings();
        assert_eq!(vs.switcher_selection(), 1, "the return view is today");
        vs.close_settings();
        let fresh = ViewState::new();
        assert_eq!(fresh.switcher_selection(), 0, "no return recorded: board");

        // A StackDetail return still reads as the board branch.
        vs.close_settings();
        vs.next_view(&state);
        vs.open_settings();
        assert_eq!(vs.switcher_selection(), 0);
    }

    // -- the review walk's stage machine (DR-7 D-J9; plan D-W2) ----

    const DAY: ReviewCadence = ReviewCadence::Day;
    fn reference() -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 5).expect("date")
    }
    fn rows(inbox: usize, carry: usize) -> WalkRows {
        WalkRows { inbox, carry }
    }
    fn walk_flow(vs: &ViewState) -> (ReviewCadence, chrono::NaiveDate, WalkStage, usize) {
        match vs.walk_flow() {
            Some(Flow::ReviewWalk {
                cadence,
                reference,
                stage,
                cursor,
                ..
            }) => (cadence, reference, stage, cursor),
            other => panic!("not a walk: {other:?}"),
        }
    }

    #[test]
    fn the_walk_opens_at_entry_and_steps_to_its_first_open_stage() {
        let (state, _, _, _, [task, ..]) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.open_review_walk(DAY, reference());
        assert!(
            matches!(walk_flow(&vs), (DAY, r, WalkStage::Entry, 0) if r == reference()),
            "the walk always opens at the guidance stage (J4 D-1)"
        );
        // Entry's Next lands on the first open row stage (D-3).
        assert_eq!(vs.advance_review_walk(rows(2, 0)), WalkStep::Advanced);
        assert!(matches!(walk_flow(&vs), (DAY, _, WalkStage::Inbox, 0)));

        // An empty inbox skips to Carry forward; both empty lands on
        // Prompts — a stage never renders empty (D-3's symmetric law).
        vs.open_review_walk(DAY, reference());
        assert_eq!(vs.advance_review_walk(rows(0, 3)), WalkStep::Advanced);
        assert!(matches!(
            walk_flow(&vs),
            (DAY, _, WalkStage::CarryForward, 0)
        ));
        vs.open_review_walk(DAY, reference());
        assert_eq!(vs.advance_review_walk(rows(0, 0)), WalkStep::Advanced);
        assert!(matches!(walk_flow(&vs), (DAY, _, WalkStage::Prompts, 0)));

        // XOR both ways: capture replaces the walk; the walk replaces
        // capture — without committing either.
        vs.begin_capture(&AppState::default());
        assert!(matches!(vs.flow(), Flow::Capture));
        vs.open_review_walk(DAY, reference());
        assert!(matches!(vs.flow(), Flow::ReviewWalk { .. }));

        // `reconcile` leaves it alone (subject-free, the palette rule).
        vs.reconcile(&state);
        assert!(matches!(vs.flow(), Flow::ReviewWalk { .. }));
        let _ = task;
    }

    #[test]
    fn the_walk_cursor_clamps_and_row_verbs_pass_the_last_row() {
        let mut vs = ViewState::new();
        vs.open_review_walk(DAY, reference());
        vs.advance_review_walk(rows(3, 2));
        assert!(matches!(walk_flow(&vs), (DAY, _, WalkStage::Inbox, 0)));

        // Cursor: clamped at both ends, no wrap.
        vs.review_walk_cursor(-1, rows(3, 2));
        assert_eq!(walk_flow(&vs).3, 0, "the top end holds");
        vs.review_walk_cursor(1, rows(3, 2));
        vs.review_walk_cursor(1, rows(3, 2));
        assert_eq!(walk_flow(&vs).3, 2);
        vs.review_walk_cursor(1, rows(3, 2));
        assert_eq!(walk_flow(&vs).3, 2, "the bottom end holds");

        // A row verb on the last row advances the stage.
        assert_eq!(vs.review_walk_next_row(rows(3, 2)), WalkStep::Advanced);
        assert!(matches!(
            walk_flow(&vs),
            (DAY, _, WalkStage::CarryForward, 0)
        ));

        // The armed Pick-a-date child is independent of the row step:
        // it mounts over the carry stage, and the adapter's row verbs
        // end it Cancelled before stepping (the flattened next-row
        // clear; the child's death is the clear).
        vs.review_walk_pick_date();
        assert!(matches!(vs.flow(), Flow::WalkDatePick));
        assert_eq!(vs.review_walk_next_row(rows(3, 2)), WalkStep::Advanced);
        assert!(matches!(
            walk_flow(&vs),
            (DAY, _, WalkStage::CarryForward, 1)
        ));

        // Passing the last carry row advances to Prompts through the
        // skip law — the planner is opt-in (the verb), never an
        // auto-advance (J4 D-3).
        assert_eq!(vs.review_walk_next_row(rows(3, 2)), WalkStep::Advanced);
        assert!(matches!(walk_flow(&vs), (DAY, _, WalkStage::Prompts, 0)));
        // Prompts has no advance (its exits are the commit verbs).
        assert_eq!(vs.review_walk_next_row(rows(0, 0)), WalkStep::Inert);
        assert_eq!(vs.advance_review_walk(rows(0, 0)), WalkStep::Inert);
    }

    #[test]
    fn the_escape_ladder_steps_substate_then_stage_then_closes() {
        let (state, _, _, _, _) = two_board_fixture();
        let mut vs = ViewState::new();
        let candidates = [StackId::from_uuid(uuid::Uuid::now_v7())];

        // Sub-state first: the armed File-to child ends `Cancelled`
        // alone — the stage stays (the D-H4 generic rung; the
        // flattened filing field is gone).
        vs.open_review_walk(DAY, reference());
        vs.advance_review_walk(rows(1, 0));
        vs.review_walk_file(&candidates);
        assert!(matches!(vs.flow(), Flow::WalkFilePick { armed: Some(_) }));
        assert_eq!(
            vs.cancel_top_child(&state),
            Some(vec![FlowOutcome::RefreshNow])
        );
        assert!(matches!(walk_flow(&vs), (DAY, _, WalkStage::Inbox, _)));

        // The armed Pick-a-date child is the same generic rung (the
        // carry rows count is virtual here — the date child only
        // arms over the Carry stage).
        vs.advance_review_walk(rows(1, 1));
        vs.review_walk_pick_date();
        assert!(matches!(vs.flow(), Flow::WalkDatePick));
        assert_eq!(
            vs.cancel_top_child(&state),
            Some(vec![FlowOutcome::RefreshNow])
        );
        // Prev steps visited stages back: Carry forward → Inbox when
        // the inbox has rows (the dialog button row, J4 US-1/S2).
        vs.back_review_walk(rows(1, 0));
        assert!(matches!(walk_flow(&vs), (DAY, _, WalkStage::Inbox, 0)));

        // Prev from the Inbox returns to Entry — the walk itself is
        // never closed by the step-back (the close is the abort/exit
        // law, J4 R-2).
        vs.back_review_walk(rows(1, 0));
        assert!(matches!(walk_flow(&vs), (DAY, _, WalkStage::Entry, 0)));
        assert!(matches!(vs.flow(), Flow::ReviewWalk { .. }));

        // An empty behind-stage is skipped backwards too: Carry
        // forward's Prev with an empty inbox returns to Entry (P12 —
        // never into a dead end).
        vs.open_review_walk(DAY, reference());
        vs.advance_review_walk(rows(0, 1));
        vs.back_review_walk(rows(0, 1));
        assert!(matches!(walk_flow(&vs), (DAY, _, WalkStage::Entry, 0)));

        // Prompts steps back to Carry forward, never into the planner.
        vs.advance_review_walk(rows(0, 1));
        assert!(matches!(
            walk_flow(&vs),
            (DAY, _, WalkStage::CarryForward, 0)
        ));
        vs.advance_review_walk(rows(0, 1));
        assert!(matches!(walk_flow(&vs), (DAY, _, WalkStage::Prompts, 0)));
        vs.back_review_walk(rows(1, 1));
        assert!(matches!(
            walk_flow(&vs),
            (DAY, _, WalkStage::CarryForward, 0)
        ));

        // Carry's Prev steps to the Inbox when it has rows, then to
        // Entry; Entry and Complete are inert behind Prev (Entry is
        // the start; the completion stage's single exit is the
        // control).
        vs.back_review_walk(rows(1, 1));
        assert!(matches!(walk_flow(&vs), (DAY, _, WalkStage::Inbox, 0)));
        vs.back_review_walk(rows(1, 1));
        assert!(matches!(walk_flow(&vs), (DAY, _, WalkStage::Entry, 0)));
        assert_eq!(vs.back_review_walk(rows(1, 1)), Vec::<FlowOutcome>::new());
    }

    #[test]
    fn a_foreign_settle_of_a_half_typed_walk_date_discards_the_text() {
        let (state, _, _, _, _) = two_board_fixture();
        let mut vs = ViewState::new();
        walk_armed(&mut vs);
        vs.open_review_walk(DAY, reference());
        vs.advance_review_walk(rows(0, 1));
        vs.review_walk_pick_date();
        assert!(matches!(vs.flow(), Flow::WalkDatePick));
        if let Some(child) = vs.walk_date_mut() {
            child.date_text = "2030-01-02".to_owned();
        }

        let outcomes = vs.settle_foreign(&state);
        assert!(
            !outcomes
                .iter()
                .any(|o| matches!(o, FlowOutcome::Command(_))),
            "the discarded text dispatches nothing"
        );
        assert!(vs.walk_date().is_none(), "the child's death is the clear");
        assert!(matches!(vs.flow(), Flow::None), "the walk settled after it");
    }

    #[test]
    fn the_walk_file_picker_wraps_and_the_clamp_normalizes() {
        let mut vs = ViewState::new();
        let (state, _, _, [s0, s1, _], _) = two_board_fixture();
        let candidates = [s0, s1];
        vs.open_review_walk(DAY, reference());
        vs.advance_review_walk(rows(2, 0));
        vs.review_walk_file(&candidates);
        assert!(matches!(
            vs.flow(),
            Flow::WalkFilePick {
                armed: Some(f),
            } if *f == s0
        ));
        vs.review_walk_file_target(1, &candidates);
        assert!(matches!(
            vs.flow(),
            Flow::WalkFilePick {
                armed: Some(f),
            } if *f == s1
        ));
        vs.review_walk_file_target(1, &candidates);
        assert!(
            matches!(
                vs.flow(),
                Flow::WalkFilePick {
                    armed: Some(f),
                } if *f == s0
            ),
            "the ring wraps"
        );

        // The refresh clamp: the cursor clamps against the pushed
        // count; a vanished candidate list ends the child `Cancelled`;
        // a vanished inbox advances structurally.
        vs.review_walk_cursor(1, rows(2, 0));
        vs.clamp_review_walk(&state, rows(1, 0), &candidates);
        assert_eq!(walk_flow(&vs).3, 0, "the cursor clamps");
        vs.clamp_review_walk(&state, rows(1, 0), &[]);
        assert!(matches!(walk_flow(&vs), (DAY, _, WalkStage::Inbox, _)));
        assert!(
            !matches!(vs.flow(), Flow::WalkFilePick { .. }),
            "the dead candidate's child is gone"
        );
        vs.clamp_review_walk(&state, rows(0, 2), &candidates);
        assert!(matches!(
            walk_flow(&vs),
            (DAY, _, WalkStage::CarryForward, 0)
        ));
        // Carry forward's cursor clamps against its own rows.
        vs.review_walk_cursor(5, rows(0, 2));
        assert_eq!(walk_flow(&vs).3, 1);
        vs.clamp_review_walk(&state, rows(0, 1), &candidates);
        assert_eq!(walk_flow(&vs).3, 0);
        let _ = state;
    }

    #[test]
    fn the_walk_settles_to_a_commandless_close() {
        let (state, _, _, _, _) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.open_review_walk(DAY, reference());
        let outcomes = vs.settle_flow(&state);
        assert!(matches!(vs.flow(), Flow::None));
        assert!(
            !outcomes
                .iter()
                .any(|o| matches!(o, FlowOutcome::Command(_))),
            "a settle never dispatches — there is nothing to commit"
        );
    }

    // -- the skip law's totality (J4 D-3, T12) ------------------------------

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(512))]

        /// The skip law is total: `walk_first_open_stage` never lands a
        /// row stage on a zero-row rendering — the result is the stage
        /// itself when it has rows, otherwise the next non-empty stage,
        /// otherwise Prompts (the terminal that always "has rows": the
        /// template's prompts). The planner is never a skip
        /// destination.
        #[test]
        fn the_skip_law_never_lands_a_row_stage_on_zero_rows(
            inbox in 0usize..4,
            carry in 0usize..4,
        ) {
            let rows = WalkRows { inbox, carry };
            for from in [WalkStage::Inbox, WalkStage::CarryForward] {
                let landed = crate::view_state::walk_first_open_stage(from, rows);
                prop_assert!(
                    matches!(landed, WalkStage::Prompts)
                        || ViewState::walk_stage_rows(landed, rows) > 0,
                    "from {from:?} over ({inbox}, {carry}) landed on {landed:?} with zero rows"
                );
                prop_assert!(
                    !matches!(landed, WalkStage::Planner | WalkStage::Entry),
                    "the skip law never lands on the opt-in planner or Entry"
                );
                // The landing is idempotent (a stage never skips twice).
                prop_assert_eq!(crate::view_state::walk_first_open_stage(landed, rows), landed);
            }
        }
    }

    // -- the exit protocol (W5b D-H3, chunk 3; plan D8) --------------------

    /// An inbox board with one live stack and two open tasks — the
    /// Stack-consumption fixture's row source.
    fn inbox_fixture() -> (AppState, StackId, [TaskId; 2]) {
        let board = BoardId::from_uuid(uuid::Uuid::from_u128(0x4001));
        let target = StackId::from_uuid(uuid::Uuid::from_u128(0x4002));
        let t0 = TaskId::from_uuid(uuid::Uuid::from_u128(0x4003));
        let t1 = TaskId::from_uuid(uuid::Uuid::from_u128(0x4004));
        let mut state = AppState::default();
        state.boards.insert(
            board,
            taskboard_domain::Board {
                id: board,
                remote: None,
                title: taskboard_domain::state::INBOX_BOARD_TITLE.to_owned(),
                color: taskboard_domain::Color::new(""),
                archived: false,
                deleted: false,
                clocks: taskboard_domain::BoardClocks {
                    title: epoch(),
                    color: epoch(),
                    archived: epoch(),
                    deleted: epoch(),
                },
                authors: taskboard_domain::entities::BoardAuthors::default(),
                remote_seen: None,
            },
        );
        state.stacks.insert(target, stack(target, board, 0));
        for (id, order) in [(t0, 0), (t1, 1)] {
            state.tasks.insert(id, task(id, target, order));
        }
        (state, target, [t0, t1])
    }

    fn walk_armed(vs: &mut ViewState) {
        vs.arm_flow(Flow::ReviewWalk {
            cadence: DAY,
            reference: reference(),
            stage: WalkStage::Inbox,
            cursor: 0,
            tally: WalkTally::default(),
        });
    }

    // -- the Replace pre-pass (cross-surface friction plan D-7,
    // -- #712): capture's chain fold, innermost-first, TEXT-masked --

    /// US-6/AC-1 (the lowest layer): capture over a live walk child
    /// folds the chain innermost-first — the picker child AND the
    /// walk both end (commandlessly), capture opens at depth 1.
    #[test]
    fn us6_ac1_capture_over_a_walk_child_folds_the_chain_innermost_first() {
        let (state, stack, _tasks) = inbox_fixture();
        let mut vs = ViewState::new();
        walk_armed(&mut vs);
        vs.arm_child_for_test(Flow::WalkFilePick { armed: Some(stack) })
            .expect("the child mounts");

        let outcomes = vs.begin_capture(&state);
        assert!(matches!(vs.flow(), Flow::Capture), "capture opens");
        assert_eq!(
            vs.surfaces().depth(),
            2,
            "the chain is the root view plus the capture: no walk or picker level survives"
        );
        assert!(
            outcomes
                .iter()
                .all(|o| !matches!(o, FlowOutcome::Command(_))),
            "the fold closes commandlessly (the walk's settle rule)"
        );
    }

    /// US-6/AC-2 (the TEXT mask's new case): a non-empty TEXT title
    /// editor under `begin_capture` DISCARDS — no commit command in
    /// the outcomes, the fire-drill's replace-without-committing
    /// semantics (today's XOR, extended to deeper TEXT levels).
    #[test]
    fn us6_ac2_capture_over_a_live_text_editor_discards_never_commits() {
        let (state, _, _, _, [a, ..]) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.begin_edit_title(&state, a);
        vs.set_buffer("a half-typed title that must never commit".to_owned());

        let outcomes = vs.begin_capture(&state);
        assert!(matches!(vs.flow(), Flow::Capture));
        assert!(
            outcomes
                .iter()
                .all(|o| !matches!(o, FlowOutcome::Command(_))),
            "the TEXT mask discards; nothing commits under a capture fire-drill"
        );
        assert_eq!(vs.buffer(), "", "the buffer died with the level");
        assert_eq!(
            vs.surfaces().depth(),
            2,
            "the chain is the root view plus the capture"
        );
    }

    #[test]
    fn a_completed_stack_payload_reaches_the_walk_parent() {
        let (state, stack, [first, ..]) = inbox_fixture();
        let mut vs = ViewState::new();
        walk_armed(&mut vs);
        vs.arm_child_for_test(Flow::WalkFilePick { armed: Some(stack) })
            .expect("the child mounts");

        let outcomes = vs.end_top_flow_with(
            &state,
            crate::surfaces::GuestExit::Completed(Some(crate::surfaces::LevelResult::Stack(stack))),
        );
        // R2's order: the command first, then the advance's refresh.
        assert!(matches!(
            outcomes.first(),
            Some(FlowOutcome::Command(taskboard_domain::StateCommand::MoveTask {
                id,
                stack: target,
                ..
            })) if *id == first && *target == stack
        ));
        assert!(matches!(outcomes.last(), Some(FlowOutcome::RefreshNow)));
        // The child is gone, the walk stays and its cursor sat on the
        // next row (one row step on a two-row inbox).
        assert!(!matches!(vs.flow(), Flow::WalkFilePick { .. }));
        assert!(matches!(vs.flow(), Flow::ReviewWalk { cursor: 1, .. }));
    }

    #[test]
    fn a_cancelled_exit_is_the_default_ignore_and_the_parent_stays() {
        let (state, _, _, _, [task, ..]) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.arm_flow(Flow::TrackEntryForm { task, seed: None });
        vs.arm_child_for_test(Flow::EntryDatePicker)
            .expect("the child mounts");

        let outcomes = vs.cancel_top_child(&state).expect("a parent flow exists");
        assert!(
            matches!(outcomes.as_slice(), [FlowOutcome::RefreshNow]),
            "the rung's shape is the refresh-only step-back"
        );
        // The form level is intact; the picker child is gone.
        assert!(matches!(vs.flow(), Flow::TrackEntryForm { .. }));

        // The form itself is at depth 1: no Cancelled rung above it —
        // the caller's own close rung applies.
        assert!(vs.cancel_top_child(&state).is_none());
    }

    #[test]
    fn a_completed_none_is_the_default_ignore() {
        let (state, _, _, _, [task, ..]) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.arm_flow(Flow::TrackEntryForm { task, seed: None });
        vs.arm_child_for_test(Flow::EntryDatePicker)
            .expect("the child mounts");

        let outcomes = vs.end_top_flow_with(&state, crate::surfaces::GuestExit::Completed(None));
        assert!(outcomes.is_empty(), "the default ignore produces nothing");
        assert!(matches!(vs.flow(), Flow::TrackEntryForm { .. }));
    }

    #[test]
    fn a_grandchilds_payload_never_crosses_an_intermediate_level() {
        use chrono::TimeZone as _;
        let (state, _, _, _, [task, ..]) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.arm_flow(Flow::TrackEntries { task });
        vs.arm_child_for_test(Flow::TrackEntryForm { task, seed: None })
            .expect("the form mounts");
        vs.arm_child_for_test(Flow::EntryDatePicker)
            .expect("the picker mounts");

        let day = chrono::NaiveDate::from_ymd_opt(2030, 6, 15).expect("date");
        let instant = chrono::Local
            .from_local_datetime(&day.and_hms_opt(12, 0, 0).expect("noon"))
            .single()
            .expect("a real local noon")
            .with_timezone(&chrono::Utc);
        let outcomes = vs.end_top_flow_with(
            &state,
            crate::surfaces::GuestExit::Completed(Some(crate::surfaces::LevelResult::Date(
                instant,
            ))),
        );
        // The form (the immediate parent) consumed the payload.
        assert_eq!(
            vs.track_form().map(|form| form.date_text.as_str()),
            Some("2030-06-15")
        );
        assert!(matches!(outcomes.as_slice(), [FlowOutcome::RefreshNow]));
        // The list observed nothing: stepping the form back leaves it
        // open.
        let _ = vs.cancel_top_child(&state);
        assert!(matches!(vs.flow(), Flow::TrackEntries { .. }));
    }

    #[test]
    fn subject_death_takes_the_subtree_without_an_exit() {
        let (mut state, _, _, _, [task, ..]) = two_board_fixture();
        let mut vs = ViewState::new();
        vs.arm_flow(Flow::TrackEntryForm { task, seed: None });
        vs.arm_child_for_test(Flow::EntryDatePicker)
            .expect("the child mounts");

        state.tasks.remove(&task);
        vs.reconcile(&state);
        // The subtree is gone — and it left as retirements (the
        // Q4 construction fact: no exit was delivered, so no
        // continuation outcomes exist to observe).
        assert!(matches!(vs.flow(), Flow::None));
        let retired = vs.drain_retired();
        assert_eq!(retired.len(), 2);
    }
}

#[cfg(test)]
mod edit_recur_tests {
    use super::tests::{epoch, fixture};
    use super::*;
    use crate::level_state::RecurPatternKind;

    /// An armed editor over the fixture's first task.
    fn armed() -> (ViewState, AppState, TaskId) {
        let (state, _, [s0, ..], [a, ..]) = fixture();
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.activate_focused(&state);
        let id = vs.begin_edit_recur(&state).expect("an open subject arms");
        (vs, state, id)
    }

    #[test]
    fn begin_arms_over_the_open_subject_and_end_clean_closes() {
        let (mut vs, state, a) = armed();
        assert_eq!(vs.begin_edit_recur(&state), Some(a));
        assert_eq!(*vs.flow(), Flow::EditRecur { task: a });
        assert!(
            vs.recur_editor_mut().is_some(),
            "the level state mints with the flow"
        );
        // The clean close: nothing dispatches, the mode resolves.
        let outcomes = vs.end_edit_recur();
        assert_eq!(
            outcomes,
            vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
        );
        assert_eq!(*vs.flow(), Flow::None);
    }

    #[test]
    fn a_done_subject_refuses_to_arm() {
        let (mut state, _, [s0, ..], [a, ..]) = fixture();
        state.tasks.get_mut(&a).expect("task").done = Some(epoch());
        let mut vs = ViewState::new();
        vs.reconcile(&state);
        vs.activate_card(a, s0);
        vs.activate_focused(&state);
        assert_eq!(
            vs.begin_edit_recur(&state),
            None,
            "the assembly precondition is structural"
        );
        assert_eq!(*vs.flow(), Flow::None);
    }

    #[test]
    fn the_draft_buffer_drops_with_the_flow() {
        let (mut vs, state, _) = armed();
        {
            let editor = vs.recur_editor_mut().expect("the level state");
            editor.every_days = "3".to_owned();
            editor.pattern = RecurPatternKind::Monthly;
        }
        let _ = vs.end_edit_recur();
        assert!(
            vs.recur_editor_mut().is_none(),
            "the level died with the flow"
        );
        // A re-arm starts from the family default — the previous draft
        // never survives (the buffers-never-outlive-the-flow rule).
        assert!(vs.begin_edit_recur(&state).is_some());
        let editor = vs.recur_editor_mut().expect("the re-minted level");
        assert_eq!(editor.every_days, "1");
        assert_eq!(editor.pattern, RecurPatternKind::Daily);
    }

    #[test]
    fn reconcile_drops_the_flow_when_the_subject_dies() {
        let (mut vs, mut state, a) = armed();
        state.tasks.remove(&a);
        vs.reconcile(&state);
        assert_eq!(*vs.flow(), Flow::None);
        assert!(vs.recur_editor_mut().is_none());
    }

    #[test]
    fn a_foreign_settle_clean_closes_without_dispatching() {
        let (mut vs, state, _) = armed();
        let outcomes = vs.settle_flow(&state);
        assert_eq!(
            outcomes,
            vec![FlowOutcome::SetMode(Mode::Normal), FlowOutcome::RefreshNow]
        );
        assert_eq!(
            *vs.flow(),
            Flow::None,
            "the only commit path is the editor's own funnel"
        );
    }

    #[test]
    fn kiosk_rotation_never_dispatches_from_the_editor() {
        let (mut vs, state, _) = armed();
        let outcomes = vs.enter_kiosk_mode(&state, crate::view_state::KioskMode::Today);
        assert!(
            !outcomes
                .iter()
                .any(|o| matches!(o, FlowOutcome::Command(_)))
        );
        assert_eq!(*vs.flow(), Flow::None);
    }

    #[test]
    fn the_flow_tags_as_its_own_kind() {
        let (vs, _, _) = armed();
        assert_eq!(vs.flow().tag(), crate::surfaces::FlowTag::EditRecur);
        assert_eq!(vs.chain_tags(), vec![crate::surfaces::FlowTag::EditRecur]);
    }

    // -- the today walk's habit half (chunk 8 D2, #934) ------------------

    use proptest::prelude::*;
    use taskboard_domain::{Habit, HabitClocks, HabitId, HabitTarget, NonZeroCount};

    fn habit(id: HabitId, board: BoardId, order: i64) -> Habit {
        Habit {
            id,
            board,
            title: format!("habit-{order}"),
            description: String::new(),
            target: HabitTarget::PerDay(NonZeroCount::new(1).unwrap()),
            order,
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: HabitClocks::all(epoch()),
            continued_from: None,
            deleted: false,
            by: None,

            paused_until: None,
        }
    }

    /// The first test module's day bundle, re-stated: this module's
    /// fixtures need the same window the task rows schedule into.
    fn day() -> DayCtx {
        use chrono::TimeZone as _;
        DayCtx {
            now: Utc.timestamp_opt(1_781_481_600, 0).unwrap(),
            window: (
                Utc.timestamp_opt(1_781_481_600, 0).unwrap(),
                Utc.timestamp_opt(1_781_568_000, 0).unwrap(),
            ),
            calendar: taskboard_domain::DayCalendar {
                day_start_time: chrono::NaiveTime::MIN,
                offset: chrono::FixedOffset::east_opt(0).expect("utc offset"),
            },
        }
    }

    /// A today fixture: two tasks scheduled inside the day window
    /// (the walk's task half) and three live habits on the board at
    /// orders 10/20/30 (the walk's habit half).
    fn today_fixture() -> (AppState, BoardId, [TaskId; 2], [HabitId; 3]) {
        use chrono::TimeZone as _;
        let (mut state, board, _, [a, b, _c]) = fixture();
        let window_start = Utc.timestamp_opt(1_781_481_600, 0).unwrap();
        for (i, id) in [a, b].into_iter().enumerate() {
            let t = state.tasks.get_mut(&id).expect("fixture task");
            t.scheduled =
                Some(window_start + chrono::Duration::hours(i64::try_from(i).unwrap_or(0) + 1));
        }
        let habits: [HabitId; 3] =
            std::array::from_fn(|_| HabitId::from_uuid(uuid::Uuid::now_v7()));
        for (i, id) in habits.iter().enumerate() {
            state.habits.habits.insert(
                *id,
                habit(*id, board, 10 * (i64::try_from(i).unwrap_or(0) + 1)),
            );
        }
        (state, board, [a, b], habits)
    }

    /// The today walk mounted the way the shell does: the Today root
    /// over the fixture board.
    fn today_walk(state: &AppState, board: BoardId) -> ViewState {
        let mut vs = ViewState::new_starting(ViewId::Today);
        vs.reconcile(state);
        vs.select_board(state, board);
        vs.focused_task = None; // the walk enters at its own first row
        vs
    }

    /// A1 (round-6 A1 D-2; red list): the walk's habit rows aggregate
    /// over ALL live boards — canonical board order outer, stored
    /// `(order, id)` inner — regardless of the selected board. Red
    /// pre-fix: the walk scoped the habits to `self.board`.
    #[test]
    fn today_rows_walk_aggregates_all_live_boards_habits() {
        let (mut state, board_a, _, fixture_habits) = today_fixture();
        let board_b = BoardId::from_uuid(uuid::Uuid::now_v7());
        state.boards.insert(
            board_b,
            taskboard_domain::Board {
                id: board_b,
                remote: None,
                title: "beta".to_owned(),
                color: taskboard_domain::Color::new(""),
                archived: false,
                deleted: false,
                clocks: taskboard_domain::BoardClocks {
                    title: DateTime::<Utc>::UNIX_EPOCH,
                    color: DateTime::<Utc>::UNIX_EPOCH,
                    archived: DateTime::<Utc>::UNIX_EPOCH,
                    deleted: DateTime::<Utc>::UNIX_EPOCH,
                },
                authors: taskboard_domain::entities::BoardAuthors::default(),
                remote_seen: None,
            },
        );
        let hb0 = HabitId::from_uuid(uuid::Uuid::now_v7());
        state.habits.habits.insert(hb0, habit(hb0, board_b, 10));
        let vs = today_walk(&state, board_a);
        let walked: Vec<HabitId> = vs
            .today_rows(&state, false, &day())
            .into_iter()
            .filter_map(|row| match row {
                TodayRow::Habit(id) => Some(id),
                TodayRow::Task(_) | TodayRow::BreakRule(_) => None,
            })
            .collect();
        // The fixture's three habits lead (board_a, canonical order),
        // board_b's habit trails: canonical board order outer.
        let mut expected = fixture_habits.to_vec();
        expected.push(hb0);
        assert_eq!(
            walked, expected,
            "the walk aggregates both boards' habits in canonical \
             board order"
        );
    }

    /// A2 (round-6 A1 D-2; armor): an archived board contributes no
    /// habit rows under the aggregated contract.
    #[test]
    fn archived_boards_contribute_no_habit_rows() {
        let (mut state, board_a, _, _) = today_fixture();
        let archived = BoardId::from_uuid(uuid::Uuid::now_v7());
        state.boards.insert(
            archived,
            taskboard_domain::Board {
                id: archived,
                remote: None,
                title: "archived".to_owned(),
                color: taskboard_domain::Color::new(""),
                archived: false,
                deleted: false,
                clocks: taskboard_domain::BoardClocks {
                    title: DateTime::<Utc>::UNIX_EPOCH,
                    color: DateTime::<Utc>::UNIX_EPOCH,
                    archived: DateTime::<Utc>::UNIX_EPOCH,
                    deleted: DateTime::<Utc>::UNIX_EPOCH,
                },
                authors: taskboard_domain::entities::BoardAuthors::default(),
                remote_seen: None,
            },
        );
        let dead_habit = HabitId::from_uuid(uuid::Uuid::now_v7());
        state
            .habits
            .habits
            .insert(dead_habit, habit(dead_habit, archived, 10));
        state.boards.get_mut(&archived).expect("board").archived = true;
        let vs = today_walk(&state, board_a);
        let walked: Vec<HabitId> = vs
            .today_rows(&state, false, &day())
            .into_iter()
            .filter_map(|row| match row {
                TodayRow::Habit(id) => Some(id),
                TodayRow::Task(_) | TodayRow::BreakRule(_) => None,
            })
            .collect();
        assert!(
            !walked.contains(&dead_habit),
            "the archived board's habits never render"
        );
    }

    #[test]
    fn us3_s1_today_rows_orders_tasks_then_habits_in_stored_order() {
        let (state, board, [a, b], [h0, h1, h2]) = today_fixture();
        let vs = today_walk(&state, board);
        assert_eq!(
            vs.today_rows(&state, false, &day()),
            vec![
                TodayRow::Task(a),
                TodayRow::Task(b),
                TodayRow::Habit(h0),
                TodayRow::Habit(h1),
                TodayRow::Habit(h2),
            ],
            "the task sections lead, the habits follow in (order, id) order"
        );
    }

    #[test]
    fn us3_s1_the_walk_traverses_habit_rows_with_the_task_keys() {
        let (state, board, [a, b], [h0, h1, h2]) = today_fixture();
        let mut vs = today_walk(&state, board);
        // Forward: two task landings then the three habit rows; the
        // cursor halves stay exclusive on every landing.
        vs.advance_focus(&state, false, false, &day());
        assert_eq!(vs.focused_task(), Some(a));
        assert_eq!(vs.focused_habit(), None);
        vs.advance_focus(&state, false, false, &day());
        vs.advance_focus(&state, false, false, &day());
        assert_eq!(
            vs.focused_habit(),
            Some(h0),
            "the walk crosses into the habit section on the same keys"
        );
        assert_eq!(vs.focused_task(), None, "the task cursor cleared");
        vs.advance_focus(&state, false, false, &day());
        vs.advance_focus(&state, false, false, &day());
        assert_eq!(vs.focused_habit(), Some(h2));
        vs.advance_focus(&state, false, false, &day());
        assert_eq!(vs.focused_habit(), Some(h2), "no wrap at the end");
        // Backward: the habit rows replay in reverse, then the tasks.
        vs.retreat_focus(&state, false, false, &day());
        vs.retreat_focus(&state, false, false, &day());
        assert_eq!(vs.focused_habit(), Some(h0));
        vs.retreat_focus(&state, false, false, &day());
        assert_eq!(vs.focused_task(), Some(b), "back into the task sections");
        assert_eq!(vs.focused_habit(), None);
        let _ = h1;
    }

    #[test]
    fn the_walks_cursor_halves_stay_exclusive_across_the_compact_retreat() {
        let (state, board, _, _) = today_fixture();
        let mut vs = today_walk(&state, board);
        // Enter, step onto a habit, retreat twice: the second retreat
        // hands the switcher level back with BOTH cursor halves clear.
        vs.advance_focus(&state, true, false, &day());
        vs.advance_focus(&state, true, false, &day());
        vs.advance_focus(&state, true, false, &day());
        assert!(vs.focused_habit().is_some());
        vs.retreat_focus(&state, true, false, &day());
        vs.retreat_focus(&state, true, false, &day());
        vs.retreat_focus(&state, true, false, &day());
        assert_eq!(vs.focused_task(), None);
        assert_eq!(vs.focused_habit(), None, "the switcher level holds both");
    }

    #[test]
    fn jump_focus_targets_the_widened_domain_ends() {
        let (state, board, [a, _], [_, _, h2]) = today_fixture();
        let mut vs = today_walk(&state, board);
        vs.jump_focus(&state, FocusEdge::Last, false, false, &day());
        assert_eq!(vs.focused_habit(), Some(h2), "last is the last habit");
        vs.jump_focus(&state, FocusEdge::First, false, false, &day());
        assert_eq!(vs.focused_task(), Some(a), "first is the first task");
        assert_eq!(vs.focused_habit(), None);
    }

    #[test]
    fn a_dead_habit_landing_prunes_on_reconcile() {
        let (mut state, board, _, [h0, _, _]) = today_fixture();
        let mut vs = today_walk(&state, board);
        vs.advance_focus(&state, false, false, &day());
        vs.advance_focus(&state, false, false, &day());
        vs.advance_focus(&state, false, false, &day());
        assert_eq!(vs.focused_habit(), Some(h0));
        let dead = state.habits.habits.get_mut(&h0).expect("habit");
        dead.deleted = true;
        vs.reconcile(&state);
        assert_eq!(vs.focused_habit(), None, "the dead landing cleared");
        assert_eq!(vs.focused_task(), None);
    }

    #[test]
    fn a_today_walk_with_two_tasks_and_three_habits_steps_end_to_end_snapshot() {
        let (state, board, [a, b], [h0, h1, h2]) = today_fixture();
        let mut vs = today_walk(&state, board);
        let mut landings: Vec<Option<crate::presentation::RowSubject>> = vec![vs.walk_subject()];
        for _ in 0..6 {
            vs.advance_focus(&state, false, false, &day());
            landings.push(vs.walk_subject());
        }
        let named: Vec<String> = landings
            .into_iter()
            .map(|s| match s {
                Some(crate::presentation::RowSubject::Task(t)) if t == a => "task-a".into(),
                Some(crate::presentation::RowSubject::Task(t)) if t == b => "task-b".into(),
                Some(crate::presentation::RowSubject::Habit(h)) if h == h0 => "habit-0".into(),
                Some(crate::presentation::RowSubject::Habit(h)) if h == h1 => "habit-1".into(),
                Some(crate::presentation::RowSubject::Habit(h)) if h == h2 => "habit-2".into(),
                other => format!("{other:?}"),
            })
            .collect();
        insta::assert_debug_snapshot!("today_walk_end_to_end", named);
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]
        #[test]
        fn today_rows_habit_order_is_stable_under_arbitrary_insertion(o0 in 0i64..1000, o1 in 0i64..1000, o2 in 0i64..1000) {
                let (mut state, board, _, [h0, h1, h2]) = today_fixture();
                for (id, order) in [(h0, o0), (h1, o1), (h2, o2)] {
                    state.habits.habits.get_mut(&id).unwrap().order = order;
                }
                let vs = today_walk(&state, board);
                let habit_rows: Vec<HabitId> = vs
                    .today_rows(&state, false, &day())
                    .into_iter()
                    .filter_map(|row| match row {
                        TodayRow::Habit(id) => Some(id),
                        // The roster rows (US-9, W-10) are not part
                        // of this property's domain.
                        TodayRow::Task(_) | TodayRow::BreakRule(_) => None,
                    })
                    .collect();
                let mut expected = habit_rows.clone();
                expected.sort_by_key(|id| {
                    let h = state.habits.habits.get(id).unwrap();
                    (h.order, h.id)
                });
            prop_assert_eq!(habit_rows, expected);
        }
    }
    // -- the rule-list cursor (J3 US-6, round 9 chunk 12) --------------

    /// A due-anchored rule row on `task` (always unarmed without a
    /// due date — the unarmed kind the D-3 jump arm reads).
    fn rule(id: taskboard_domain::ReminderRuleId, task: TaskId) -> taskboard_domain::ReminderRule {
        taskboard_domain::ReminderRule {
            insistence: taskboard_domain::Insistence::Normal,
            id,
            task,
            kind: taskboard_domain::ReminderKind::DueOffset(
                taskboard_domain::ReminderOffset::at_due(),
            ),
            created_at: epoch(),
            updated_at: epoch(),
            by: None,
        }
    }

    /// Seeds `count` rules on `task` and returns their ids in roster
    /// (id-table) order.
    fn seed_rules(
        state: &mut AppState,
        task: TaskId,
        count: usize,
    ) -> Vec<taskboard_domain::ReminderRuleId> {
        let mut ids = Vec::new();
        for _ in 0..count {
            let id = taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::now_v7());
            state.reminders.rules.insert(id, rule(id, task));
            ids.push(id);
        }
        ids
    }

    /// The detail mounted the way the shell does: the `TaskDetail` root
    /// over `subject` (the cursor walk's own fixture).
    fn detail_over(state: &AppState, subject: TaskId) -> ViewState {
        let mut vs = ViewState::new_starting(ViewId::TaskDetail);
        vs.subject_task = Some(subject);
        vs.reconcile(state);
        vs
    }

    #[test]
    fn us6_s1_the_rule_cursor_arms_at_row_0_and_steps_clamped() {
        let (mut state, _, _, [a, _, _]) = fixture();
        let roster = seed_rules(&mut state, a, 3);
        let mut vs = detail_over(&state, a);
        assert_eq!(vs.rule_cursor(), None, "a fresh detail is disarmed");

        // The first vertical stroke arms at row 0 (arm-then-step).
        assert_eq!(
            vs.walk_rule_cursor(&state, 1),
            vec![FlowOutcome::RefreshNow]
        );
        assert_eq!(vs.rule_cursor(), Some(roster[0]));

        // Down down walks; the third down clamps (no wrap).
        vs.walk_rule_cursor(&state, 1);
        assert_eq!(vs.rule_cursor(), Some(roster[1]));
        vs.walk_rule_cursor(&state, 1);
        assert_eq!(vs.rule_cursor(), Some(roster[2]));
        assert_eq!(
            vs.walk_rule_cursor(&state, 1),
            Vec::<FlowOutcome>::new(),
            "the last row clamps"
        );
        assert_eq!(vs.rule_cursor(), Some(roster[2]), "the end holds");
        // Up up up: back to row 0, then the clamp.
        vs.walk_rule_cursor(&state, -1);
        vs.walk_rule_cursor(&state, -1);
        assert_eq!(vs.rule_cursor(), Some(roster[0]));
        assert_eq!(vs.walk_rule_cursor(&state, -1), Vec::<FlowOutcome>::new());
        assert_eq!(vs.rule_cursor(), Some(roster[0]), "row 0 clamps too");
    }

    #[test]
    fn us6_n1_an_empty_roster_and_a_subjectless_detail_never_arm() {
        let (state, _, _, [a, _, _]) = fixture();
        let mut vs = detail_over(&state, a);
        assert_eq!(
            vs.walk_rule_cursor(&state, 1),
            Vec::<FlowOutcome>::new(),
            "zero rules: the stroke is the typed no-op (the empty-roster gate)"
        );
        assert_eq!(vs.rule_cursor(), None);
        let mut vs = ViewState::new_starting(ViewId::TaskDetail);
        assert_eq!(
            vs.walk_rule_cursor(&state, 1),
            Vec::<FlowOutcome>::new(),
            "no subject: the stroke is the typed no-op"
        );
    }

    #[test]
    fn us6_s3_a_dead_rule_prunes_and_a_live_one_survives_the_refresh() {
        let (mut state, _, _, [a, _, _]) = fixture();
        let roster = seed_rules(&mut state, a, 2);
        let mut vs = detail_over(&state, a);
        vs.walk_rule_cursor(&state, 1);
        assert_eq!(vs.rule_cursor(), Some(roster[0]));

        // A refresh with the cursor'd rule still live keeps the cursor
        // (D-2: never cleared by refresh alone).
        vs.reconcile(&state);
        assert_eq!(
            vs.rule_cursor(),
            Some(roster[0]),
            "the live cursor survives a refresh (the clamp law prunes, not clears)"
        );

        // A foreign delete (CLI / sync / sibling remove) prunes — the
        // dead id never re-anchors to a live neighbour.
        state.reminders.rules.remove(&roster[0]);
        vs.reconcile(&state);
        assert_eq!(
            vs.rule_cursor(),
            None,
            "the dead cursor pruned to None (not re-pointed)"
        );
    }

    #[test]
    fn us6_s3_subject_change_and_view_exit_clear_the_cursor() {
        let (mut state, _, _, [a, b, _]) = fixture();
        let roster = seed_rules(&mut state, a, 2);
        seed_rules(&mut state, b, 1);
        let mut vs = detail_over(&state, a);
        vs.walk_rule_cursor(&state, 1);
        assert_eq!(vs.rule_cursor(), Some(roster[0]));

        // Opening a different task's detail is a new roster (D-2
        // site 3): the open path clears. The open grammar resolves
        // the subject off `focused_task` (the board focus's open) —
        // focus b, then enter.
        vs.focused_task = Some(b);
        vs.enter(&state, ViewId::TaskDetail);
        assert_eq!(vs.rule_cursor(), None, "the fresh open cleared the arm");
        vs.reconcile(&state);
        assert_eq!(vs.rule_cursor(), None);

        // View exit clears on the leaving pass (D-2 site 1): the
        // cursor never survives under another surface.
        vs.walk_rule_cursor(&state, 1);
        assert!(vs.rule_cursor().is_some());
        vs.replace_root(ViewId::Board);
        vs.reconcile(&state);
        assert_eq!(vs.rule_cursor(), None, "the detail exit cleared");
    }

    #[test]
    fn us6_s3_the_overlay_openers_are_foreign_settles() {
        let (mut state, _, _, [a, _, _]) = fixture();
        let roster = seed_rules(&mut state, a, 2);
        let mut vs = detail_over(&state, a);
        vs.walk_rule_cursor(&state, 1);
        assert!(vs.rule_cursor().is_some());

        // The due editor (the jump arm's own target — the mask host).
        vs.begin_edit_date(&state, DateField::Due);
        assert_eq!(vs.rule_cursor(), None, "the due editor cleared");
        vs.write_flow(Flow::None);

        // The labels overlay.
        vs.walk_rule_cursor(&state, 1);
        vs.open_labels(LabelOverlaySubject::Task(a));
        assert_eq!(vs.rule_cursor(), None, "the labels overlay cleared");
        vs.write_flow(Flow::None);

        // The entries overlay.
        vs.walk_rule_cursor(&state, 1);
        vs.open_track_entries(a);
        assert_eq!(vs.rule_cursor(), None, "the entries overlay cleared");
        vs.write_flow(Flow::None);

        // The recur overlay.
        vs.walk_rule_cursor(&state, 1);
        assert_eq!(vs.rule_cursor(), Some(roster[0]));
        vs.begin_edit_recur(&state);
        assert_eq!(vs.rule_cursor(), None, "the recur overlay cleared");
    }

    #[test]
    fn us6_fold_the_walk_walks_presentation_only() {
        let (mut state, _, _, [a, _, _]) = fixture();
        seed_rules(&mut state, a, 2);
        let mut vs = detail_over(&state, a);
        // Every outcome of the whole walk is RefreshNow — no
        // StateCommand variant ever carries the cursor (the fold's
        // view-state face; the journey pins the boundary stream).
        for direction in [1, 1, -1, 1] {
            assert!(
                vs.walk_rule_cursor(&state, direction)
                    .into_iter()
                    .all(|outcome| outcome == FlowOutcome::RefreshNow)
            );
        }
    }
}

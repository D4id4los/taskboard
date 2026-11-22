// SPDX-License-Identifier: AGPL-3.0-only

//! The flow transition-table test (round-3 chunk-1, plan §9.1): one
//! expectation row per `(FlowKind, EffectClass)` cell plus per-pair
//! overrides for the kept explicit arms, and a driver that runs
//! **every** `(Flow, UiEffect)` pair through the interpretation
//! table and asserts the typed outcome (post-flow kind + commands
//! dispatched at the boundary). A missing row fails; a new
//! `Flow`/`UiEffect` variant without a row fails — the totality is
//! compiler- (no wildcard arm) plus test-enforced.
//!
//! The normative matrix is the audit report §6
//! (`.artifacts/reports/2026-10-26-ui-flow-state-audit-report.md`).

#![allow(clippy::too_many_lines)] // the matrix is the deliverable

use super::mount;
use crate::router::{Mode, UiEffect};
use crate::test_platform::on_slint_thread;
use crate::view_state::Flow;
use crate::{UiHandle, UiSettings};
use slint::{ComponentHandle as _, Model as _};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use taskboard_domain::{
    AppState, Board, BoardClocks, BoardId, Color, Stack, StackClocks, StackId, StateCommand, Task,
    TaskClocks, TaskId,
};
use tokio::sync::mpsc;

// -- the classification mirrors (audit §6) ------------------------------

/// The seven flow kinds the matrix rows range over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FK {
    None,
    Text,
    Due,
    Move,
    Drag,
    Palette,
    Capture,
    FocusOverlay,
    // The entries overlay's two surfaces (r4 chunk 10): the ladder's
    // form→list step is observable, so the list and the form carry
    // distinct kinds.
    TrackList,
    TrackForm,
    /// The plan-next-day placement form (#622): the planner's own
    /// form kind — the kept planner arms own it, settle closes it.
    PlanForm,
    /// The review walk (#638): the kept walk arms own its intents,
    /// settle closes it (the palette family).
    ReviewWalk,
    /// The labels overlay (UX R-3 D-2, #789): the overlay family's
    /// row — its kept intents own the open/dismiss; settle is its
    /// close (the palette terminator rule).
    LabelOverlay,
    /// The recurrence-rule editor overlay (round-7 D-7.1): the
    /// overlay family's row — settle is its clean close (the draft
    /// dies, nothing dispatches).
    RecurEditor,
    /// The child levels (chunk 3's ports): no kept arms of their own —
    /// every effect bubbles outward to the parent (the routing's
    /// innermost-first walk). The fixture arms them at depth 1, where
    /// the bubble lands on the root handler.
    Child,
    /// The embedded planner child (J4 D-2): the child cell's shape,
    /// with the walk's own verb arm and the planner-surface rows —
    /// its kept arms (`PlanBegin` mounts the form as its child, J4
    /// D-2; the walk intents route to the walk beneath).
    PlanChild,
}

/// The eleven effect classes the matrix columns range over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EC {
    Nav,
    Lifecycle,
    Begin,
    OpenPalette,
    FocusOverlay,
    BeginCapture,
    Edit,
    Cancel,
    Menu,
    System,
    Title,
    Picker,
    TrackOverlay,
    LabelsOverlay,
    PlanSurface,
    ReviewWalk,
}

fn kind_of(flow: &Flow) -> FK {
    match flow {
        // The armed-delete strip (UX R-2 D-2) and the color picker
        // (UX R-4 D-1, #791): the modals' row — their seams own the
        // commit/confirm, settle is the disarm.
        // The board ceremony strips (OP1): the armed-delete's row —
        // settle is the disarm, the strips' seams own the confirm.
        Flow::ConfirmBoardArchive { .. }
        | Flow::ConfirmBoardReset { .. }
        | Flow::ConfirmBoardReconcile { .. }
        | Flow::ConfirmDelete(_)
        | Flow::BoardColorPick { .. }
        | Flow::None => FK::None,
        // The child levels (chunk 3's ports): no kept intents of
        // their own; settle is the Cancelled end.
        Flow::EntryDatePicker | Flow::WalkFilePick { .. } | Flow::WalkDatePick => FK::Child,
        // The embedded planner child (J4 D-2): its own kind — the
        // walk-intent arm and the planner rows are its kept arms.
        Flow::ReviewPlanner => FK::PlanChild,
        Flow::Create { .. }
        | Flow::CreateStack
        | Flow::CreateBoard
        | Flow::EditTitle { .. }
        | Flow::EditDescription { .. }
        | Flow::RenameStack { .. }
        | Flow::RenameBoard { .. } => FK::Text,
        Flow::EditDate { .. } => FK::Due,
        Flow::Move { .. } | Flow::MoveStack { .. } => FK::Move,
        Flow::DragMove { .. } => FK::Drag,
        Flow::Palette { .. } => FK::Palette,
        Flow::Capture => FK::Capture,
        Flow::FocusMode { .. } => FK::FocusOverlay,
        Flow::TrackEntries { .. } => FK::TrackList,
        Flow::TrackEntryForm { .. } => FK::TrackForm,
        Flow::PlanPlace { .. } => FK::PlanForm,
        Flow::ReviewWalk { .. } => FK::ReviewWalk,
        Flow::EditRecur { .. } => FK::RecurEditor,
        Flow::Labels { .. } => FK::LabelOverlay,
    }
}

/// Payload-free effect key (the per-pair override table's row key;
/// `SetMode` splits by the mode's meaning).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EK {
    Exit,
    OpenMenu,
    OpenPalette,
    CloseMenu,
    MenuBack,
    MenuNext,
    /// The notification verbs' placement (the menu family's letter;
    /// the matrix never sees them — `apply_effect` applies first).
    NotificationVerb,
    MenuPrevious,
    FocusToday,
    OpenInbox,
    BeginCapture,
    ToggleFocusMode,
    InvokeSelected,
    SetModeNormal,
    SetModeInsert,
    FocusNext,
    FocusPrev,
    /// The directional horizontal (Left/Right) and vertical (Up/Down)
    /// keys (J3 US-1): the kept move-flow arms differ per axis, so
    /// the pair harness carries them separately.
    DirectionH,
    DirectionV,
    /// The selection-extend twin (#875): no kept arms under any flow
    /// (the Nav settle-then-run re-runs it flow-free).
    ExtendSelection,
    NextView,
    PrevView,
    FocusBoard,
    BeginCreate,
    BeginCreateStack,
    Activate,
    RequestMove,
    BoardNext,
    BoardPrevious,
    BoardSelect,
    StackWider,
    StackNarrower,
    CancelMove,
    StackNext,
    StackPrev,
    StackDelete,
    StackRename,
    StackMove,
    BoardCreate,
    BoardDelete,
    BoardRename,
    BoardColorEdit,
    BoardDefaultPreset,
    /// The board ceremony verbs (OP1, US-1/2/3): the lifecycle
    /// family's keys — the arm/confirm split is the delete verbs'
    /// two-phase shape over the selected board.
    BoardCeremony,
    /// Present for the class mapping's totality; the pair harness
    /// never drives it (applied before the table, the `PickerKey`
    /// exclusion verbatim).
    #[allow(dead_code)]
    OpenBoardBarFlyout,
    FocusFirst,
    FocusLast,
    OpenSettings,
    /// The export dialog's pair (the J2 export plan W-1): the open
    /// is a modal takeover, the close the modal's own exit arm.
    ExportOpen,
    ExportClose,
    OpenStatusPanel,
    BeginTitleEdit,
    /// The keymap-parity D-4 editor arms: their own keys — the date
    /// arm posts `Due`, the description arm posts `Text`.
    BeginDateEdit,
    BeginDescriptionEdit,
    /// The effort field's focus opener (J3 US-4, chunk 11): the
    /// begin family's shape, but a pure focus — no editor flow arms.
    BeginEffortEdit,
    ShowTask,
    ControlNext,
    ControlPrev,
    ScrollUp,
    ScrollDown,
    TrackEntriesOpen,
    TrackEntryDismiss,
    /// The labels overlay's intents (UX R-3 D-2, #789): the
    /// entries pair's twin — the open settles-then-runs, the
    /// dismissal closes.
    LabelsOpen,
    LabelsDismiss,
    /// The priority pickers' opener (UX R-3 D-1, #788): the same
    /// flag-flip class — no flow moves, no command routes.
    PriorityOpen,
    /// The recur row's intents (issue #506): flag flips over the
    /// subject — no flow moves, no command routes.
    RecurIntent,
    /// The recurrence editor's opener (round-7 D-7.1): opens the
    /// overlay; inert under its own flow.
    SeriesEditOpen,
    /// The stop-repeating strip's flag flips (round-7 D-7.1): the arm
    /// and the cancel share the cell (symmetric flips over the editor
    /// level).
    SeriesDetach,
    /// The plan-next-day family (#622): the surface toggle and the
    /// placement row verbs.
    PlanOpen,
    PlanBegin,
    PlanCommit,
    PlanBox,
    PlanRemove,
    /// The review walk family (#638): the opener plus three intent
    /// keys — advance keeps the walk, the exit intents close it, the
    /// row verbs keep it (the fixture's stage decides the exits).
    ReviewOpen,
    WalkAdvance,
    WalkExit,
    WalkVerb,
    /// The File-to arm (N3a): the one walk verb that mounts a child
    /// level — its own cell because its post-state is the child.
    WalkFileArm,
    /// The H4 editor opener (H4 D1): the Nav-class view intent with
    /// no pair-table overrides — the class cells decide.
    HabitEditorOpen,
}

fn key_of(effect: &UiEffect) -> EK {
    match effect {
        UiEffect::Exit => EK::Exit,
        // The undo branch picker (UF2): its placement mirrors the
        // menu family's (settled-then-mounted, Escape closes); the
        // matrix key rides the menu class (the picker is a sibling
        // modal, not a flow).
        UiEffect::OpenMenu | UiEffect::UndoBranchesOpen => EK::OpenMenu,
        UiEffect::OpenPalette => EK::OpenPalette,
        UiEffect::CloseMenu => EK::CloseMenu,
        UiEffect::MenuBack => EK::MenuBack,
        UiEffect::MenuNext => EK::MenuNext,
        // The notification verbs ride the menu family's placement:
        // applied in `apply_effect` before the table — the matrix
        // never sees them (the menu-family keys' pattern).
        UiEffect::NotificationVerb(_) => EK::NotificationVerb,
        UiEffect::MenuPrevious => EK::MenuPrevious,
        UiEffect::FocusToday => EK::FocusToday,
        UiEffect::OpenInbox => EK::OpenInbox,
        UiEffect::BeginCapture => EK::BeginCapture,
        UiEffect::ToggleFocusMode => EK::ToggleFocusMode,
        UiEffect::InvokeSelected => EK::InvokeSelected,
        UiEffect::SetMode(Mode::Normal) => EK::SetModeNormal,
        UiEffect::SetMode(Mode::Insert) => EK::SetModeInsert,
        UiEffect::FocusNext => EK::FocusNext,
        UiEffect::FocusPrev => EK::FocusPrev,
        // The directional family's kept-arm split is per axis (the
        // move column hop vs the ring); the extend twin has no kept
        // arms.
        UiEffect::FocusDirectional(
            crate::router::Direction::Left | crate::router::Direction::Right,
        ) => EK::DirectionH,
        UiEffect::FocusDirectional(
            crate::router::Direction::Up | crate::router::Direction::Down,
        ) => EK::DirectionV,
        UiEffect::ExtendSelectionNext | UiEffect::ExtendSelectionPrev => EK::ExtendSelection,
        UiEffect::NextView => EK::NextView,
        UiEffect::PrevView => EK::PrevView,
        UiEffect::FocusBoard => EK::FocusBoard,
        UiEffect::BeginCreate { .. } => EK::BeginCreate,
        UiEffect::BeginCreateStack => EK::BeginCreateStack,
        UiEffect::Activate => EK::Activate,
        UiEffect::RequestMove => EK::RequestMove,
        UiEffect::BoardNext => EK::BoardNext,
        UiEffect::BoardPrevious => EK::BoardPrevious,
        UiEffect::BoardSelect(_) => EK::BoardSelect,
        UiEffect::StackWider => EK::StackWider,
        UiEffect::StackNarrower => EK::StackNarrower,
        UiEffect::CancelMove => EK::CancelMove,
        UiEffect::StackNext => EK::StackNext,
        UiEffect::StackPrev => EK::StackPrev,
        UiEffect::StackDelete => EK::StackDelete,
        UiEffect::StackRename => EK::StackRename,
        UiEffect::StackMove => EK::StackMove,
        UiEffect::BoardCreate => EK::BoardCreate,
        UiEffect::BoardDelete => EK::BoardDelete,
        UiEffect::BoardRename => EK::BoardRename,
        UiEffect::BoardColorEdit => EK::BoardColorEdit,
        UiEffect::BoardDefaultPreset => EK::BoardDefaultPreset,
        // The board ceremony verbs (OP1): the lifecycle family's
        // key — one cell family, the delete verbs' shape.
        UiEffect::BoardArchive | UiEffect::BoardReset | UiEffect::BoardReconcile => {
            EK::BoardCeremony
        }
        UiEffect::FocusFirst => EK::FocusFirst,
        UiEffect::FocusLast => EK::FocusLast,
        UiEffect::OpenSettings => EK::OpenSettings,
        UiEffect::ExportDialogOpen => EK::ExportOpen,
        UiEffect::ExportDialogClose => EK::ExportClose,
        UiEffect::OpenStatusPanel => EK::OpenStatusPanel,
        UiEffect::BeginTitleEdit => EK::BeginTitleEdit,
        UiEffect::BeginDateEdit { .. } => EK::BeginDateEdit,
        UiEffect::BeginDescriptionEdit => EK::BeginDescriptionEdit,
        UiEffect::BeginEffortEdit => EK::BeginEffortEdit,
        UiEffect::ShowTask => EK::ShowTask,
        UiEffect::ControlNext => EK::ControlNext,
        UiEffect::ControlPrev => EK::ControlPrev,
        UiEffect::ScrollUp => EK::ScrollUp,
        UiEffect::ScrollDown => EK::ScrollDown,
        UiEffect::TrackEntriesOpen => EK::TrackEntriesOpen,
        // The priority pickers' opener (UX R-3 D-1): the Nav class
        // flag flip.
        UiEffect::PriorityPickerOpen(_) => EK::PriorityOpen,
        // The recur row's intents (issue #506) and HF1's pause editor
        // opener: flag flips over the subject — no flow moves, no
        // command routes.
        UiEffect::SeriesPauseOpen
        | UiEffect::RowMenuRequested
        | UiEffect::HabitPauseOpen
        | UiEffect::HabitConvertArm
        | UiEffect::HabitConvertCancel
        | UiEffect::HabitRemoveArm
        | UiEffect::HabitRemoveCancel
        | UiEffect::HabitCreateToggle
        // The quota popover's opener (W-5, #961): the RecurIntent
        // class — a view intent over the row subject, no modal
        // takeover beyond the settle.
        | UiEffect::HabitQuotaEdit
        | UiEffect::SeriesConvertArm => EK::RecurIntent,
        // The H4 editor opener (H4 D1): the settle-then-open view
        // intent — the Nav class cells, no pair overrides.
        // The HF3 history opener shares the editor opener's matrix
        // key (the same settle-then-open view-intent class).
        UiEffect::HabitEdit | UiEffect::HabitHistoryOpen => EK::HabitEditorOpen,
        // The recur editor's intents (round-7 D-7.1): the opener and
        // the detach strip's symmetric flag flips.
        UiEffect::SeriesEditorOpen => EK::SeriesEditOpen,
        UiEffect::SeriesDetachArm | UiEffect::SeriesDetachCancel => EK::SeriesDetach,
        UiEffect::TrackEntryDismiss => EK::TrackEntryDismiss,
        UiEffect::LabelsOpen => EK::LabelsOpen,
        UiEffect::LabelsDismiss => EK::LabelsDismiss,
        UiEffect::OpenPlanNextDay => EK::PlanOpen,
        UiEffect::PlanBeginPlace(_) => EK::PlanBegin,
        UiEffect::PlanCommitPlace => EK::PlanCommit,
        UiEffect::PlanBoxEarlier(_) | UiEffect::PlanBoxLater(_) => EK::PlanBox,
        UiEffect::PlanRemoveBox(_) => EK::PlanRemove,
        UiEffect::OpenReviewWalk => EK::ReviewOpen,
        UiEffect::ReviewWalk(crate::router::WalkIntent::Advance) => EK::WalkAdvance,
        UiEffect::ReviewWalk(
            crate::router::WalkIntent::Record | crate::router::WalkIntent::Finish,
        ) => EK::WalkExit,
        UiEffect::ReviewWalk(crate::router::WalkIntent::InboxFile) => EK::WalkFileArm,
        UiEffect::ReviewWalk(_) => EK::WalkVerb,
        // The picker keys are applied in `apply_effect`, before the
        // table — the matrix never sees them (the menu-family
        // pattern). Their class cell is exercised via `class_of`.
        UiEffect::PickerKey(_) => {
            unreachable!("picker keys are applied before the table")
        }
        // The board bar flyout's open request (UX R-4 D-4, #794) is
        // applied in `apply_effect`, before the table (the picker
        // keys' pattern) — the matrix never sees it.
        UiEffect::OpenBoardBarFlyout => {
            unreachable!("the flyout request is applied before the table")
        }
    }
}

fn class_of(effect: &UiEffect) -> EC {
    // Delegates to the production classifier (`UiEffect::class`) —
    // the mirror cannot drift from the code the compiler totals.
    match effect.class() {
        crate::router::EffectClass::Nav => EC::Nav,
        crate::router::EffectClass::Lifecycle => EC::Lifecycle,
        crate::router::EffectClass::Begin => EC::Begin,
        crate::router::EffectClass::OpenPalette => EC::OpenPalette,
        crate::router::EffectClass::FocusOverlay => EC::FocusOverlay,
        crate::router::EffectClass::BeginCapture => EC::BeginCapture,
        crate::router::EffectClass::Edit => EC::Edit,
        crate::router::EffectClass::Cancel => EC::Cancel,
        crate::router::EffectClass::Menu => EC::Menu,
        crate::router::EffectClass::System => EC::System,
        crate::router::EffectClass::Title => EC::Title,
        crate::router::EffectClass::Picker => EC::Picker,
        crate::router::EffectClass::TrackOverlay => EC::TrackOverlay,
        crate::router::EffectClass::LabelsOverlay => EC::LabelsOverlay,
        crate::router::EffectClass::PlanSurface => EC::PlanSurface,
        crate::router::EffectClass::ReviewWalk => EC::ReviewWalk,
    }
}

// -- the expectation matrix ----------------------------------------------

/// The typed outcome a pair pass asserts: commands dispatched at the
/// boundary and the post-pass flow (the `None` target or a flow of
/// the named kind).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Exp {
    cmd: Cmd,
    flow: Post,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cmd {
    Zero,
    NonZero,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Post {
    None,
    Kind(FK),
}

const fn exp(cmd: Cmd, flow: Post) -> Exp {
    Exp { cmd, flow }
}

use Cmd::NonZero as NZ;
use Cmd::Zero as Z;
use FK::Capture as KCap;
use FK::Child as KChild;
use FK::Drag as KDrag;
use FK::Due as KDue;
use FK::FocusOverlay as KFoc;
use FK::LabelOverlay as KLabel;
use FK::Move as KMove;
use FK::Palette as KPal;
use FK::PlanChild as KPlanChild;
use FK::PlanForm as KPlanForm;
use FK::RecurEditor as KRecur;
use FK::ReviewWalk as KWalk;
use FK::Text as KTxt;
use FK::TrackForm as KTrackForm;
use FK::TrackList as KTrackList;
use Post::Kind as K;

// The armed-delete strip (UX R-2 D-2) joins the kind-None family:
// its settle is the disarm (Post::None), its kept intents run.
use FK::None as KNone;
use Post::None as N;

/// The class-level default matrix (audit §6): every `(FK, EC)` cell
/// has exactly one row.
fn class_table() -> Vec<(FK, EC, Exp)> {
    use EC as C;
    let mut t = Vec::new();
    let mut row = |kind: FK, class: EC, e: Exp| t.push((kind, class, e));
    for class in [
        C::Nav,
        C::Lifecycle,
        C::Begin,
        C::OpenPalette,
        C::BeginCapture,
        C::Edit,
        C::Cancel,
        C::Menu,
        C::System,
        C::Title,
        C::Picker,
    ] {
        row(FK::None, class, exp(Z, N));
    }
    // FOCUS OVERLAY (#496): the toggle's kept arm covers every flow —
    // from None it opens the overlay (the capture-Replace family's
    // XOR substitute), under a live overlay it closes.
    row(FK::None, C::FocusOverlay, exp(Z, K(KFoc)));
    for kind in [
        FK::Text,
        FK::Due,
        FK::Move,
        FK::Drag,
        FK::Palette,
        FK::Capture,
        FK::FocusOverlay,
    ] {
        row(kind, C::FocusOverlay, exp(Z, K(KFoc)));
    }
    // TEXT: everything foreign settles (the armed non-empty buffer
    // commits) and the effect re-runs; the capture exception replaces
    // without committing; the kept edit/cancel arms stay.
    for class in [C::Nav, C::Lifecycle, C::Begin, C::OpenPalette, C::Title] {
        row(FK::Text, class, exp(NZ, N));
    }
    row(FK::Text, C::BeginCapture, exp(Z, K(KCap)));
    for class in [C::Edit, C::Cancel, C::Menu, C::System, C::Picker] {
        row(FK::Text, class, exp(Z, K(KTxt)));
    }
    // DUE: the settle is the clean close — never a commit.
    for class in [
        C::Nav,
        C::Lifecycle,
        C::Begin,
        C::OpenPalette,
        C::Title,
        C::Cancel,
    ] {
        row(FK::Due, class, exp(Z, N));
    }
    row(FK::Due, C::BeginCapture, exp(Z, K(KCap)));
    for class in [C::Edit, C::Menu, C::System, C::Picker] {
        row(FK::Due, class, exp(Z, K(KDue)));
    }
    // MOVE / DRAG: settle cancels (no dispatch), then re-runs.
    for kind in [FK::Move, FK::Drag] {
        for class in [
            C::Nav,
            C::Lifecycle,
            C::Begin,
            C::OpenPalette,
            C::Title,
            C::Cancel,
        ] {
            row(kind, class, exp(Z, N));
        }
        row(kind, C::BeginCapture, exp(Z, K(KCap)));
        for class in [C::Edit, C::Menu, C::System, C::Picker] {
            row(kind, class, exp(Z, K(kind)));
        }
    }
    // PALETTE / CAPTURE: settle is their own close.
    for kind in [FK::Palette, FK::Capture] {
        for class in [C::Nav, C::Lifecycle, C::Begin, C::Title, C::Cancel] {
            row(kind, class, exp(Z, N));
        }
        row(kind, C::OpenPalette, exp(Z, K(kind)));
        row(kind, C::BeginCapture, exp(Z, K(KCap)));
        for class in [C::Edit, C::Menu, C::System, C::Picker] {
            row(kind, class, exp(Z, K(kind)));
        }
    }
    // FOCUS OVERLAY (#496): settle is its own close (the palette
    // family's terminator rule); the toggle is the pair override
    // below (the kept arm closes without settling).
    for class in [C::Nav, C::Lifecycle, C::Begin, C::Title, C::Cancel] {
        row(FK::FocusOverlay, class, exp(Z, N));
    }
    row(FK::FocusOverlay, C::OpenPalette, exp(Z, K(KPal)));
    row(FK::FocusOverlay, C::BeginCapture, exp(Z, K(KCap)));
    for class in [C::Edit, C::Menu, C::System, C::Picker] {
        row(FK::FocusOverlay, class, exp(Z, K(KFoc)));
    }
    // REVIEW WALK (#638): the same overlay family — the settle is its
    // own close (the palette terminator rule); the kept walk arms are
    // the pair overrides below.
    for class in [C::Nav, C::Lifecycle, C::Begin, C::Title, C::Cancel] {
        row(FK::ReviewWalk, class, exp(Z, N));
    }
    row(FK::ReviewWalk, C::OpenPalette, exp(Z, K(KPal)));
    row(FK::ReviewWalk, C::BeginCapture, exp(Z, K(KCap)));
    row(FK::ReviewWalk, C::FocusOverlay, exp(Z, K(KFoc)));
    row(FK::ReviewWalk, C::TrackOverlay, exp(Z, N));
    for class in [C::Edit, C::Menu, C::System, C::Picker] {
        row(FK::ReviewWalk, class, exp(Z, K(KWalk)));
    }
    // TRACK OVERLAY (r4 chunk 10, D-TS2): the same settle-then-run
    // family — the settle is the overlay's own close; the kept
    // TrackOverlay arms are the pair overrides below (the open
    // request is inert under a live overlay; the dismissal ladder is
    // per-surface).
    for kind in [FK::TrackList, FK::TrackForm, FK::LabelOverlay] {
        for class in [C::Nav, C::Lifecycle, C::Begin, C::Title, C::Cancel] {
            row(kind, class, exp(Z, N));
        }
        row(kind, C::OpenPalette, exp(Z, K(KPal)));
        row(kind, C::FocusOverlay, exp(Z, K(KFoc)));
        row(kind, C::BeginCapture, exp(Z, K(KCap)));
        for class in [C::Edit, C::Menu, C::System, C::Picker] {
            row(kind, class, exp(Z, K(kind)));
        }
        row(kind, C::TrackOverlay, exp(Z, N));
        // The sibling overlay's open settles-then-runs (the kept
        // open/dismiss cells are the pair rows below).
        row(kind, C::LabelsOverlay, exp(Z, N));
    }
    // PLAN FORM (#622, D-3): the same settle-then-run family — the
    // settle is the form's own close; the kept PlanSurface arms are
    // the pair overrides above.
    for class in [C::Nav, C::Lifecycle, C::Begin, C::Title, C::Cancel] {
        row(FK::PlanForm, class, exp(Z, N));
    }
    row(FK::PlanForm, C::OpenPalette, exp(Z, K(KPal)));
    row(FK::PlanForm, C::FocusOverlay, exp(Z, K(KFoc)));
    row(FK::PlanForm, C::BeginCapture, exp(Z, K(KCap)));
    for class in [C::Edit, C::Menu, C::System, C::Picker] {
        row(FK::PlanForm, class, exp(Z, K(KPlanForm)));
    }
    row(FK::PlanForm, C::TrackOverlay, exp(Z, N));
    row(FK::PlanForm, C::PlanSurface, exp(Z, K(KPlanForm)));
    // RECUR EDITOR (round-7 D-7.1): the same settle-then-run overlay
    // family — the settle is the editor's clean close (the draft dies,
    // nothing dispatches); the kept classes stay under the modal mask.
    for class in [C::Nav, C::Lifecycle, C::Begin, C::Title, C::Cancel] {
        row(FK::RecurEditor, class, exp(Z, N));
    }
    row(FK::RecurEditor, C::OpenPalette, exp(Z, K(KPal)));
    row(FK::RecurEditor, C::FocusOverlay, exp(Z, K(KFoc)));
    row(FK::RecurEditor, C::BeginCapture, exp(Z, K(KCap)));
    for class in [C::Edit, C::Menu, C::System, C::Picker] {
        row(FK::RecurEditor, class, exp(Z, K(KRecur)));
    }
    row(FK::RecurEditor, C::TrackOverlay, exp(Z, N));
    row(FK::None, C::TrackOverlay, exp(Z, N));
    // The labels overlay's class cells (UX R-3 D-2, #789): the
    // TrackOverlay family's mirror — the settle is the overlay's own
    // close everywhere a kept pair cell doesn't own the pair.
    for kind in [
        FK::None,
        FK::Text,
        FK::Due,
        FK::Move,
        FK::Drag,
        FK::Palette,
        FK::Capture,
        FK::FocusOverlay,
        FK::PlanForm,
        FK::ReviewWalk,
        FK::RecurEditor,
    ] {
        row(kind, C::LabelsOverlay, exp(Z, N));
    }
    row(FK::Child, C::LabelsOverlay, exp(Z, K(KChild)));
    row(FK::PlanChild, C::LabelsOverlay, exp(Z, K(KPlanChild)));
    // CHILD LEVELS (chunk 3's ports): the settle is the Cancelled end
    // (the modals' terminator rule); everything else bubbles outward —
    // at the fixture's depth 1 the overlay-open classes replace the
    // child (the XOR at depth 1) and the rest reads the root handler:
    // no kept arms means no arm ends the child but a settle.
    for class in [C::Nav, C::Lifecycle, C::Begin, C::Title, C::Cancel] {
        row(FK::Child, class, exp(Z, N));
    }
    row(FK::Child, C::OpenPalette, exp(Z, K(KPal)));
    row(FK::Child, C::BeginCapture, exp(Z, K(KCap)));
    row(FK::Child, C::FocusOverlay, exp(Z, K(KFoc)));
    row(FK::Child, C::TrackOverlay, exp(Z, K(KChild)));
    row(FK::PlanChild, C::TrackOverlay, exp(Z, K(KPlanChild)));
    for class in [C::Edit, C::Menu, C::System, C::Picker] {
        row(FK::Child, class, exp(Z, K(KChild)));
    }
    row(FK::Child, C::ReviewWalk, exp(Z, K(KChild)));
    row(FK::PlanChild, C::ReviewWalk, exp(Z, K(KPlanChild)));
    // The embedded planner child (J4 D-2): the child cell's shape,
    // plus the walk's kept verb arm and the planner-surface keep.
    for class in [C::Nav, C::Lifecycle, C::Begin, C::Title, C::Cancel] {
        row(FK::PlanChild, class, exp(Z, N));
    }
    row(FK::PlanChild, C::OpenPalette, exp(Z, K(KPal)));
    row(FK::PlanChild, C::BeginCapture, exp(Z, K(KCap)));
    row(FK::PlanChild, C::FocusOverlay, exp(Z, K(KFoc)));
    for class in [C::Edit, C::Menu, C::System, C::Picker] {
        row(FK::PlanChild, class, exp(Z, K(KPlanChild)));
    }
    // A held foreign flow under the plan-surface class: the named
    // inert keeps the flow (the modal discipline; Escape out first).
    for kind in [
        FK::Text,
        FK::Due,
        FK::Move,
        FK::Drag,
        FK::Palette,
        FK::Capture,
        FK::FocusOverlay,
        FK::TrackList,
        FK::TrackForm,
        FK::LabelOverlay,
        FK::ReviewWalk,
        FK::RecurEditor,
        FK::Child,
        FK::PlanChild,
    ] {
        row(kind, C::PlanSurface, exp(Z, held_flow_keep(kind)));
    }
    row(FK::None, C::PlanSurface, exp(Z, N));
    // Every foreign kind under the overlay class: the settle is the
    // terminator; the dismissal re-run against the settled `None` is
    // the honest no-op close (the open request's effect-shaped cells
    // are the pair overrides below).
    for kind in [
        FK::Text,
        FK::Due,
        FK::Move,
        FK::Drag,
        FK::Palette,
        FK::Capture,
        FK::FocusOverlay,
    ] {
        row(kind, C::TrackOverlay, exp(Z, N));
    }
    // REVIEW WALK (#638): the intents are the class default — inert
    // under `None` (no walk to act on) and settle-then-inert under a
    // foreign flow (the settle closes the foreign flow, the re-run
    // finds no walk; a non-empty TEXT buffer commits on the way). The
    // opener's effect-shaped opens/keeps are the pair overrides below.
    for kind in [
        FK::None,
        FK::Due,
        FK::Move,
        FK::Drag,
        FK::Palette,
        FK::Capture,
        FK::FocusOverlay,
        FK::TrackList,
        FK::TrackForm,
        FK::LabelOverlay,
        FK::PlanForm,
        // The walk keeps its own intents (the modal's kept arms run
        // the verb table; the Record exit now lands on the summary —
        // the walk stays live, J4 D-1/D-8).
        FK::ReviewWalk,
        FK::RecurEditor,
    ] {
        row(kind, C::ReviewWalk, exp(Z, N));
    }
    row(FK::Text, C::ReviewWalk, exp(NZ, N));
    t
}

/// The per-pair overrides: the kept explicit arms and the
/// effect-shaped rows a class cell cannot express (each names its
/// audit §6 cell).
/// A held foreign flow's keep (the modal discipline's per-kind cell
/// — the flow's own named inert): shared by the plan-surface,
/// series-detach, and planner-verb loops (the matrix's kept
/// foreign-flow letter; every `FK` arm is total).
fn held_flow_keep(kind: FK) -> Post {
    match kind {
        FK::Text => K(KTxt),
        FK::Due => K(KDue),
        FK::Move => K(KMove),
        FK::Drag => K(KDrag),
        FK::Palette => K(KPal),
        FK::Capture => K(KCap),
        FK::FocusOverlay => K(KFoc),
        FK::TrackList => K(KTrackList),
        FK::TrackForm => K(KTrackForm),
        FK::LabelOverlay => K(KLabel),
        FK::PlanForm => K(KPlanForm),
        FK::ReviewWalk => K(KWalk),
        FK::RecurEditor => K(KRecur),
        FK::Child => K(KChild),
        FK::PlanChild => K(KPlanChild),
        FK::None => unreachable!("no flow held"),
    }
}

fn pair_table() -> Vec<(FK, EK, Exp)> {
    use EK as E;
    let mut t = Vec::new();
    let mut row = |kind: FK, key: EK, e: Exp| t.push((kind, key, e));
    // -- RECUR INTENT (issue #506): the flag flips ride every flow —
    //    the flow itself is preserved (the arm is not a modal; the
    //    strip renders inside the row).
    row(FK::None, E::RecurIntent, exp(Z, N));
    for kind in [
        FK::Text,
        FK::Due,
        FK::Move,
        FK::Drag,
        FK::Palette,
        FK::Capture,
        FK::FocusOverlay,
        FK::TrackList,
        FK::TrackForm,
        FK::PlanForm,
        FK::ReviewWalk,
        FK::RecurEditor,
        FK::Child,
        FK::PlanChild,
    ] {
        let keep = match kind {
            FK::Text => K(KTxt),
            FK::Due => K(KDue),
            FK::Move => K(KMove),
            FK::Drag => K(KDrag),
            FK::Palette => K(KPal),
            FK::Capture => K(KCap),
            FK::FocusOverlay => K(KFoc),
            FK::TrackList => K(KTrackList),
            FK::TrackForm => K(KTrackForm),
            FK::LabelOverlay => K(KLabel),
            FK::PlanForm => K(KPlanForm),
            FK::ReviewWalk => K(KWalk),
            FK::RecurEditor => K(KRecur),
            FK::Child => K(KChild),
            FK::PlanChild => K(KPlanChild),
            FK::None => unreachable!("the loop never names None"),
        };
        row(kind, E::RecurIntent, exp(Z, keep));
    }
    // -- THE MASK'S OWN WALK (the focus-restoration plan US-2/W-2,
    //    D-W2): the control walk inside a masked site is kept — the
    //    clamp consumes it in the post-table tail; the flow holds and
    //    nothing dispatches (the recorded matrix change: the walk was
    //    settle-then-run, which was the Tab leak the clamp closes).
    for kind in [
        FK::Palette,
        FK::Capture,
        FK::TrackList,
        FK::TrackForm,
        FK::RecurEditor,
    ] {
        let keep = match kind {
            FK::Palette => K(KPal),
            FK::Capture => K(KCap),
            FK::TrackList => K(KTrackList),
            FK::TrackForm => K(KTrackForm),
            FK::RecurEditor => K(KRecur),
            _ => unreachable!("the loop names the mask kinds only"),
        };
        row(kind, E::ControlNext, exp(Z, keep));
        row(kind, E::ControlPrev, exp(Z, keep));
    }
    // -- CHILD LEVELS (chunk 3's ports): the settle-then-run cells —
    //    the child's Cancelled end dispatches nothing, then the re-run
    //    opens/acts exactly as it would from no flow (the begin
    //    family's R4 cells); the dismissal arm is the wildcard's
    //    close-only rung at depth 1.
    row(FK::Child, E::BeginCreate, exp(Z, K(KTxt)));
    row(FK::Child, E::BeginCreateStack, exp(Z, K(KTxt)));
    row(FK::Child, E::RequestMove, exp(Z, K(KMove)));
    row(FK::Child, E::StackDelete, exp(Z, K(KNone)));
    row(FK::Child, E::StackRename, exp(Z, K(KTxt)));
    row(FK::Child, E::StackMove, exp(Z, K(KMove)));
    row(FK::Child, E::BoardCreate, exp(Z, K(KTxt)));
    // The preset gate (UX R-4 D-5, #795): settle-then-run; the
    // fixture board is non-empty, so the gate's typed no-op ends
    // flow-free.
    row(FK::Child, E::BoardDefaultPreset, exp(Z, N));
    // The color picker (UX R-4 D-1, #791): settle-then-run ends in
    // the armed picker (a kind-None flow, zero commands).
    row(FK::Child, E::BoardColorEdit, exp(Z, K(KNone)));
    // The preset gate (UX R-4 D-5, #795): settle-then-run; the
    // fixture board is non-empty, so the gate's typed no-op ends
    // flow-free.
    row(FK::Child, E::BoardDelete, exp(Z, K(KNone)));
    row(FK::Child, E::BoardCeremony, exp(Z, K(KNone)));
    row(FK::Child, E::BoardRename, exp(Z, K(KTxt)));
    row(FK::Child, E::BeginTitleEdit, exp(Z, K(KTxt)));
    row(FK::Child, E::TrackEntriesOpen, exp(Z, K(KTrackList)));
    row(FK::Child, E::TrackEntryDismiss, exp(Z, N));
    row(FK::Child, E::ReviewOpen, exp(Z, K(KWalk)));
    // The walk intents under a child route to the walk level beneath
    // (the child dispatch arm); with no walk beneath — the fixture's
    // depth 1 — they are the inert no-op and the child stays.
    row(FK::Child, E::WalkAdvance, exp(Z, K(KChild)));
    row(FK::PlanChild, E::WalkAdvance, exp(Z, K(KPlanChild)));
    row(FK::Child, E::WalkExit, exp(Z, K(KChild)));
    row(FK::PlanChild, E::WalkExit, exp(Z, K(KPlanChild)));
    row(FK::Child, E::WalkVerb, exp(Z, K(KChild)));
    row(FK::PlanChild, E::WalkVerb, exp(Z, K(KPlanChild)));
    row(FK::Child, E::WalkFileArm, exp(Z, K(KChild)));
    row(FK::PlanChild, E::WalkFileArm, exp(Z, K(KPlanChild)));
    // -- LIFECYCLE under every foreign kind: the settle-then-run
    //    re-run's outcome is effect-shaped (a rename arms a fresh
    //    text flow, a delete dispatches with no flow) --
    // The deletes under a text flow: the settle commits the
    // non-empty buffer, the re-run arms the strip (UX R-2 D-2).
    row(FK::Text, E::StackDelete, exp(NZ, K(KNone)));
    row(FK::Text, E::BoardDelete, exp(NZ, K(KNone)));
    row(FK::Text, E::BoardCeremony, exp(NZ, K(KNone)));
    row(FK::Text, E::StackRename, exp(NZ, K(KTxt)));
    row(FK::Text, E::BoardRename, exp(NZ, K(KTxt)));
    row(FK::Text, E::BoardCreate, exp(NZ, K(KTxt)));
    row(FK::Text, E::BoardDefaultPreset, exp(NZ, N));
    row(FK::Text, E::BoardColorEdit, exp(NZ, K(KNone)));
    row(FK::Text, E::StackMove, exp(NZ, K(KMove)));
    for kind in [FK::Due, FK::Move, FK::Drag, FK::Palette, FK::Capture] {
        row(kind, E::StackDelete, exp(Z, K(KNone)));
        row(kind, E::BoardDelete, exp(Z, K(KNone)));
        row(kind, E::BoardCeremony, exp(Z, K(KNone)));
        row(kind, E::StackRename, exp(Z, K(KTxt)));
        row(kind, E::BoardRename, exp(Z, K(KTxt)));
        row(kind, E::BoardCreate, exp(Z, K(KTxt)));
        row(kind, E::BoardDefaultPreset, exp(Z, N));
        row(kind, E::BoardColorEdit, exp(Z, K(KNone)));
        row(kind, E::StackMove, exp(Z, K(KMove)));
    }
    // -- TITLE (BeginTitleEdit, audit §6 last column): settle-then-
    //    run under a foreign flow; under None it arms the viewed
    //    task's editor or is inert without a subject --
    row(FK::None, E::BeginTitleEdit, exp(Z, K(KTxt)));
    // -- EDITOR ARMS (keymap-parity US-3/D-4, audit §6 Begin cell):
    //    settle-then-run like the begin family; the arm posts its
    //    editor's own kind. The scheduled payload is payload-blind
    //    (the same DateField flow).
    row(FK::None, E::BeginDateEdit, exp(Z, K(KDue)));
    row(FK::None, E::BeginDescriptionEdit, exp(Z, K(KTxt)));
    // -- EFFORT (BeginEffortEdit, J3 US-4): settle-then-run like the
    //    begin family, but a pure focus — the always-on M9 field owns
    //    the commit, so the arm posts no editor kind.
    row(FK::None, E::BeginEffortEdit, exp(Z, K(KNone)));
    for kind in [
        FK::Text,
        FK::Due,
        FK::Move,
        FK::Drag,
        FK::Palette,
        FK::Capture,
        FK::FocusOverlay,
        FK::TrackList,
        FK::TrackForm,
        FK::LabelOverlay,
        FK::PlanForm,
        FK::ReviewWalk,
        FK::RecurEditor,
        FK::Child,
        FK::PlanChild,
    ] {
        let cmd = if kind == FK::Text { NZ } else { Z };
        row(kind, E::BeginDateEdit, exp(cmd, K(KDue)));
        row(kind, E::BeginDescriptionEdit, exp(cmd, K(KTxt)));
        row(kind, E::BeginEffortEdit, exp(cmd, K(KNone)));
    }
    row(FK::Text, E::BeginTitleEdit, exp(NZ, K(KTxt)));
    row(FK::Due, E::BeginTitleEdit, exp(Z, K(KTxt)));
    row(FK::Move, E::BeginTitleEdit, exp(Z, K(KTxt)));
    row(FK::Drag, E::BeginTitleEdit, exp(Z, K(KTxt)));
    row(FK::Palette, E::BeginTitleEdit, exp(Z, K(KTxt)));
    row(FK::Capture, E::BeginTitleEdit, exp(Z, K(KTxt)));
    // -- Flow::None: the begin/open arms (audit §6 row 1) --
    row(FK::None, E::BeginCreate, exp(Z, K(KTxt)));
    row(FK::None, E::BeginCreateStack, exp(Z, K(KTxt)));
    row(FK::None, E::RequestMove, exp(Z, K(KMove)));
    // The F26 rework (plan §6): Activate is the plain detail open —
    // no edit flow (the auto-arm row this replaces was the recorded
    // decision in the red commit).
    row(FK::None, E::Activate, exp(Z, N));
    row(FK::None, E::OpenPalette, exp(Z, K(KPal)));
    row(FK::None, E::BeginCapture, exp(Z, K(KCap)));
    // The concentration overlay's toggle (#496): opens from None
    // (the kept arm), closes under a live overlay (the toggle).
    row(FK::None, E::ToggleFocusMode, exp(Z, K(KFoc)));
    // The overlay's settle-then-run re-run is effect-shaped like the
    // other modals': the settle closed it, the begin/rename arms a
    // fresh flow.
    row(FK::FocusOverlay, E::BeginCreate, exp(Z, K(KTxt)));
    row(FK::FocusOverlay, E::BeginCreateStack, exp(Z, K(KTxt)));
    row(FK::FocusOverlay, E::RequestMove, exp(Z, K(KMove)));
    row(FK::FocusOverlay, E::BeginTitleEdit, exp(Z, K(KTxt)));
    row(FK::FocusOverlay, E::StackDelete, exp(Z, K(KNone)));
    row(FK::FocusOverlay, E::BoardDelete, exp(Z, K(KNone)));
    row(FK::FocusOverlay, E::BoardCeremony, exp(Z, K(KNone)));
    row(FK::FocusOverlay, E::StackRename, exp(Z, K(KTxt)));
    row(FK::FocusOverlay, E::BoardRename, exp(Z, K(KTxt)));
    row(FK::FocusOverlay, E::BoardCreate, exp(Z, K(KTxt)));
    row(FK::FocusOverlay, E::BoardDefaultPreset, exp(Z, N));
    row(FK::FocusOverlay, E::BoardColorEdit, exp(Z, K(KNone)));
    row(FK::FocusOverlay, E::StackMove, exp(Z, K(KMove)));
    // The named-inert edits under the live overlay (the resolver's
    // Edit class is inert on the overlay's None-kind mapping).
    row(FK::FocusOverlay, E::Activate, exp(Z, K(KFoc)));
    row(FK::FocusOverlay, E::SetModeNormal, exp(Z, K(KFoc)));
    row(FK::FocusOverlay, E::SetModeInsert, exp(Z, K(KFoc)));
    row(FK::FocusOverlay, E::CancelMove, exp(Z, N)); // CancelOnly: the settle closes
    // The toggle under a live overlay: the kept arm closes (no settle).
    row(FK::FocusOverlay, E::ToggleFocusMode, exp(Z, N));
    row(FK::None, E::SetModeInsert, exp(Z, K(KTxt)));
    row(FK::None, E::StackRename, exp(Z, K(KTxt)));
    row(FK::None, E::BoardRename, exp(Z, K(KTxt)));
    row(FK::None, E::BoardCreate, exp(Z, K(KTxt)));
    row(FK::None, E::BoardDefaultPreset, exp(Z, N));
    row(FK::None, E::BoardColorEdit, exp(Z, K(KNone)));
    row(FK::None, E::StackMove, exp(Z, K(KMove)));
    // The delete verbs arm the confirm strip (UX R-2 D-2): nothing
    // dispatches, the armed flow holds (the kind-None family).
    row(FK::None, E::StackDelete, exp(Z, K(KNone)));
    row(FK::None, E::BoardDelete, exp(Z, K(KNone)));
    row(FK::None, E::BoardCeremony, exp(Z, K(KNone)));
    // -- TEXT rows --
    row(FK::Text, E::Activate, exp(NZ, N)); // kept commit (non-empty)
    row(FK::Text, E::SetModeNormal, exp(Z, N)); // kept discard
    row(FK::Text, E::SetModeInsert, exp(Z, K(KTxt))); // named inert
    row(FK::Text, E::CancelMove, exp(Z, K(KTxt))); // named inert (C-g)
    row(FK::Text, E::BeginCreate, exp(NZ, K(KTxt))); // F9: settle+open
    row(FK::Text, E::BeginCreateStack, exp(NZ, K(KTxt)));
    row(FK::Text, E::RequestMove, exp(NZ, K(KMove)));
    row(FK::Text, E::OpenPalette, exp(NZ, K(KPal)));
    // OpenMenu settles at the application layer (audit §5) — inside
    // the table it is unreachable.
    row(FK::Text, E::OpenMenu, exp(Z, K(KTxt)));
    // -- DUE rows --
    // The Escape ladder (r4 D25): the rig arms the flow with no
    // picker, so SetMode(Normal) is the clean close — the ladder's
    // step-back needs a live picker (the journeys own that leg); the
    // machine can never step into a phase that does not exist.
    row(FK::Due, E::SetModeNormal, exp(Z, N));
    row(FK::Due, E::Activate, exp(Z, K(KDue))); // named inert (the commit funnel owns Return)
    row(FK::Due, E::SetModeInsert, exp(Z, K(KDue)));
    row(FK::Due, E::BeginCreate, exp(Z, K(KTxt)));
    row(FK::Due, E::BeginCreateStack, exp(Z, K(KTxt)));
    row(FK::Due, E::RequestMove, exp(Z, K(KMove)));
    row(FK::Due, E::OpenPalette, exp(Z, K(KPal)));
    row(FK::Due, E::OpenMenu, exp(Z, K(KDue)));
    // -- MOVE rows --
    row(FK::Move, E::FocusNext, exp(Z, K(KMove))); // kept target cycle
    row(FK::Move, E::FocusPrev, exp(Z, K(KMove)));
    // The directional kept arms (J3 US-2, round 9 chunk 9): the
    // horizontal keys hop the drop column, the vertical keys ride the
    // target ring — the move survives both.
    row(FK::Move, E::DirectionH, exp(Z, K(KMove)));
    row(FK::Move, E::DirectionV, exp(Z, K(KMove)));
    // #1033: the legacy stack-walk verbs join the kept arms — the
    // chords behind StackNext/StackPrev hop the drop column mid-move
    // (was: settle-then-run, cancelling the live move).
    row(FK::Move, E::StackNext, exp(Z, K(KMove)));
    row(FK::Move, E::StackPrev, exp(Z, K(KMove)));
    row(FK::Move, E::Activate, exp(NZ, N)); // kept confirm
    row(FK::Move, E::RequestMove, exp(Z, N)); // kept toggle-cancel
    row(FK::Move, E::CancelMove, exp(Z, N)); // kept cancel
    row(FK::Move, E::SetModeNormal, exp(Z, N)); // kept cancel
    row(FK::Move, E::SetModeInsert, exp(Z, K(KTxt))); // kept `i`-begins
    row(FK::Move, E::BeginCreate, exp(Z, K(KTxt)));
    row(FK::Move, E::BeginCreateStack, exp(Z, K(KTxt)));
    row(FK::Move, E::OpenPalette, exp(Z, K(KPal)));
    row(FK::Move, E::OpenMenu, exp(Z, K(KMove)));
    // -- DRAG rows --
    row(FK::Drag, E::Activate, exp(NZ, N)); // kept confirm
    row(FK::Drag, E::SetModeNormal, exp(Z, N)); // kept cancel
    row(FK::Drag, E::RequestMove, exp(Z, N)); // kept cancel
    row(FK::Drag, E::FocusNext, exp(Z, K(KDrag))); // kept named inert
    row(FK::Drag, E::FocusPrev, exp(Z, K(KDrag)));
    // The directional keys are absorbed mid-drag too (the pointer
    // owns the ring — J3 US-2's DragMove absorb).
    row(FK::Drag, E::DirectionH, exp(Z, K(KDrag)));
    row(FK::Drag, E::DirectionV, exp(Z, K(KDrag)));
    // The extend twin (#875): the named inert under the move/drag
    // flows (the flows own the selection semantics; a settle would
    // cancel the move — the bug class US-2 fixes).
    row(FK::Drag, E::ExtendSelection, exp(Z, K(KDrag)));
    row(FK::Move, E::ExtendSelection, exp(Z, K(KMove)));
    row(FK::Drag, E::CancelMove, exp(Z, N)); // kept cancel
    row(FK::Drag, E::BeginCreate, exp(Z, K(KTxt)));
    row(FK::Drag, E::BeginCreateStack, exp(Z, K(KTxt)));
    row(FK::Drag, E::OpenPalette, exp(Z, K(KPal)));
    row(FK::Drag, E::OpenMenu, exp(Z, K(KDrag)));
    // -- PALETTE rows --
    row(FK::Palette, E::OpenPalette, exp(Z, K(KPal))); // already open
    row(FK::Palette, E::BeginCreate, exp(Z, K(KTxt)));
    row(FK::Palette, E::BeginCreateStack, exp(Z, K(KTxt)));
    row(FK::Palette, E::RequestMove, exp(Z, K(KMove)));
    row(FK::Palette, E::OpenMenu, exp(Z, K(KPal)));
    // -- CAPTURE rows --
    row(FK::Capture, E::Activate, exp(NZ, K(KCap))); // kept commit-and-continue
    row(FK::Capture, E::SetModeNormal, exp(Z, N)); // kept close
    row(FK::Capture, E::BeginCapture, exp(Z, K(KCap))); // kept re-open inert
    row(FK::Capture, E::BeginCreate, exp(Z, K(KTxt)));
    row(FK::Capture, E::BeginCreateStack, exp(Z, K(KTxt)));
    row(FK::Capture, E::RequestMove, exp(Z, K(KMove)));
    row(FK::Capture, E::OpenMenu, exp(Z, K(KCap)));
    // -- TRACK OVERLAY rows (r4 chunk 10, D-TS2) --
    // From None: the open request opens the list (the rig views the
    // subject; the kiosk/subject refusals are outside this matrix's
    // cells); the dismissal under None is the honest no-op close.
    row(FK::None, E::TrackEntriesOpen, exp(Z, K(KTrackList)));
    row(FK::None, E::TrackEntryDismiss, exp(Z, N));
    // Under a live overlay: the open request is the kept inert; the
    // dismissal closes the armed level (N1: the form is the list's
    // CHILD — the fixture's depth-1 form has no list beneath, so its
    // dismissal reads the close; the real chain's form→list step is
    // the generic Cancelled rung, pinned by the unit tests).
    row(FK::TrackList, E::TrackEntriesOpen, exp(Z, K(KTrackList)));
    row(FK::TrackForm, E::TrackEntriesOpen, exp(Z, K(KTrackForm)));
    row(FK::TrackList, E::TrackEntryDismiss, exp(Z, N));
    row(FK::TrackForm, E::TrackEntryDismiss, exp(Z, N));
    // Settle-then-run under every foreign kind: the settle closes the
    // foreign flow, the re-run opens the list (the overlay family's
    // open is effect-shaped like the palette's).
    for kind in [
        FK::Text,
        FK::Due,
        FK::Move,
        FK::Drag,
        FK::Palette,
        FK::Capture,
        FK::FocusOverlay,
    ] {
        row(kind, E::TrackEntriesOpen, exp(Z, K(KTrackList)));
    }
    // The begin/rename/deletes under a live overlay: the settle
    // closes it, the re-run arms its fresh flow (the overlay family's
    // effect-shaped set).
    for kind in [FK::TrackList, FK::TrackForm] {
        row(kind, E::BeginCreate, exp(Z, K(KTxt)));
        row(kind, E::BeginCreateStack, exp(Z, K(KTxt)));
        row(kind, E::RequestMove, exp(Z, K(KMove)));
        row(kind, E::BeginTitleEdit, exp(Z, K(KTxt)));
        row(kind, E::StackDelete, exp(Z, K(KNone)));
        row(kind, E::BoardDelete, exp(Z, K(KNone)));
        row(kind, E::BoardCeremony, exp(Z, K(KNone)));
        row(kind, E::StackRename, exp(Z, K(KTxt)));
        row(kind, E::BoardRename, exp(Z, K(KTxt)));
        row(kind, E::BoardCreate, exp(Z, K(KTxt)));
        row(kind, E::BoardColorEdit, exp(Z, K(KNone)));
        row(kind, E::StackMove, exp(Z, K(KMove)));
        row(kind, E::OpenPalette, exp(Z, K(KPal)));
    }
    // The begin/rename/deletes under the armed placement form: the
    // settle closes it, the re-run arms its fresh flow (the overlay
    // family's effect-shaped set).
    row(FK::PlanForm, E::BeginCreate, exp(Z, K(KTxt)));
    row(FK::PlanForm, E::BeginCreateStack, exp(Z, K(KTxt)));
    row(FK::PlanForm, E::RequestMove, exp(Z, K(KMove)));
    row(FK::PlanForm, E::BeginTitleEdit, exp(Z, K(KTxt)));
    row(FK::PlanForm, E::StackDelete, exp(Z, K(KNone)));
    row(FK::PlanForm, E::BoardDelete, exp(Z, K(KNone)));
    row(FK::PlanForm, E::BoardCeremony, exp(Z, K(KNone)));
    row(FK::PlanForm, E::StackRename, exp(Z, K(KTxt)));
    row(FK::PlanForm, E::BoardRename, exp(Z, K(KTxt)));
    row(FK::PlanForm, E::BoardCreate, exp(Z, K(KTxt)));
    row(FK::PlanForm, E::BoardDefaultPreset, exp(Z, N));
    row(FK::PlanForm, E::BoardColorEdit, exp(Z, K(KNone)));
    row(FK::PlanForm, E::StackMove, exp(Z, K(KMove)));
    row(FK::PlanForm, E::TrackEntriesOpen, exp(Z, K(KTrackList)));
    // The begin/rename/deletes under the recur editor (round-7 D-7.1):
    // the settle closes it, the re-run arms its fresh flow (the overlay
    // family's effect-shaped set).
    row(FK::RecurEditor, E::BeginCreate, exp(Z, K(KTxt)));
    row(FK::RecurEditor, E::BeginCreateStack, exp(Z, K(KTxt)));
    row(FK::RecurEditor, E::RequestMove, exp(Z, K(KMove)));
    row(FK::RecurEditor, E::BeginTitleEdit, exp(Z, K(KTxt)));
    row(FK::RecurEditor, E::StackDelete, exp(Z, K(KNone)));
    row(FK::RecurEditor, E::BoardDelete, exp(Z, K(KNone)));
    row(FK::RecurEditor, E::BoardCeremony, exp(Z, K(KNone)));
    row(FK::RecurEditor, E::StackRename, exp(Z, K(KTxt)));
    row(FK::RecurEditor, E::BoardRename, exp(Z, K(KTxt)));
    row(FK::RecurEditor, E::BoardCreate, exp(Z, K(KTxt)));
    row(FK::RecurEditor, E::BoardDefaultPreset, exp(Z, N));
    row(FK::RecurEditor, E::BoardColorEdit, exp(Z, K(KNone)));
    row(FK::RecurEditor, E::StackMove, exp(Z, K(KMove)));
    row(FK::RecurEditor, E::TrackEntriesOpen, exp(Z, K(KTrackList)));
    // -- LABELS OVERLAY rows (UX R-3 D-2, #789): from None the open
    //    request opens the list (the rig views the subject; the
    //    kiosk/subject refusals are outside this matrix's cells); the
    //    dismissal under None is the honest no-op close. Under a
    //    live overlay: the open request is the kept inert; the
    //    dismissal closes (the entries pair's shape). Settle-then-run
    //    under every foreign kind: the settle closes the foreign
    //    flow, the re-run opens the list (a TEXT settle commits —
    //    the auto-settle's rule).
    row(FK::None, E::LabelsOpen, exp(Z, K(KLabel)));
    row(FK::None, E::LabelsDismiss, exp(Z, N));
    row(FK::LabelOverlay, E::LabelsOpen, exp(Z, K(KLabel)));
    row(FK::LabelOverlay, E::LabelsDismiss, exp(Z, N));
    // The open REPLACES the foreign text flow without settling (the
    // entries open's explicit-arm shape — the modal takeover, not
    // the auto-commit).
    row(FK::Text, E::LabelsOpen, exp(Z, K(KLabel)));
    row(FK::Text, E::LabelsDismiss, exp(Z, N));
    for kind in [
        FK::Due,
        FK::Move,
        FK::Drag,
        FK::Palette,
        FK::Capture,
        FK::FocusOverlay,
        FK::TrackList,
        FK::TrackForm,
        FK::PlanForm,
        FK::ReviewWalk,
        FK::RecurEditor,
    ] {
        row(kind, E::LabelsOpen, exp(Z, K(KLabel)));
        row(kind, E::LabelsDismiss, exp(Z, N));
    }
    row(FK::Child, E::LabelsOpen, exp(Z, K(KLabel)));
    row(FK::Child, E::LabelsDismiss, exp(Z, N));
    // -- REVIEW WALK rows (#638): from None the walk does not exist
    //    yet (the open effect lands with the walk verbs commit); under
    //    a live walk the settle-then-run re-run is effect-shaped like
    //    the other modals', the named inerts keep the walk.
    row(FK::ReviewWalk, E::BeginCreate, exp(Z, K(KTxt)));
    row(FK::ReviewWalk, E::BeginCreateStack, exp(Z, K(KTxt)));
    row(FK::ReviewWalk, E::RequestMove, exp(Z, K(KMove)));
    row(FK::ReviewWalk, E::BeginTitleEdit, exp(Z, K(KTxt)));
    row(FK::ReviewWalk, E::StackDelete, exp(Z, K(KNone)));
    row(FK::ReviewWalk, E::BoardDelete, exp(Z, K(KNone)));
    row(FK::ReviewWalk, E::BoardCeremony, exp(Z, K(KNone)));
    row(FK::ReviewWalk, E::StackRename, exp(Z, K(KTxt)));
    row(FK::ReviewWalk, E::BoardRename, exp(Z, K(KTxt)));
    row(FK::ReviewWalk, E::BoardCreate, exp(Z, K(KTxt)));
    row(FK::ReviewWalk, E::BoardDefaultPreset, exp(Z, N));
    row(FK::ReviewWalk, E::BoardColorEdit, exp(Z, K(KNone)));
    row(FK::ReviewWalk, E::StackMove, exp(Z, K(KMove)));
    row(FK::ReviewWalk, E::TrackEntriesOpen, exp(Z, K(KTrackList)));
    row(FK::ReviewWalk, E::Activate, exp(Z, K(KWalk)));
    row(FK::ReviewWalk, E::SetModeNormal, exp(Z, K(KWalk)));
    row(FK::ReviewWalk, E::SetModeInsert, exp(Z, K(KWalk)));
    row(FK::ReviewWalk, E::CancelMove, exp(Z, N)); // CancelOnly: the settle closes
    // -- REVIEW WALK rows (#638): the opener opens over every flow
    //    (the settle is the terminator; the re-run arms the walk);
    //    under the live walk it is the kept inert. The intents: the
    //    advance and the row verbs keep the walk, the exit intents
    //    close it (the fixture's Inbox stage decides — Back closes
    //    there, Record closes gently with blank answers); from None
    //    and under a foreign flow the intents are the class rows.
    row(FK::None, E::ReviewOpen, exp(Z, K(KWalk)));
    row(FK::Text, E::ReviewOpen, exp(NZ, K(KWalk)));
    for kind in [
        FK::Due,
        FK::Move,
        FK::Drag,
        FK::Palette,
        FK::Capture,
        FK::FocusOverlay,
        FK::TrackList,
        FK::TrackForm,
        FK::PlanForm,
        FK::ReviewWalk,
        FK::RecurEditor,
    ] {
        row(kind, E::ReviewOpen, exp(Z, K(KWalk)));
    }
    row(FK::ReviewWalk, E::WalkAdvance, exp(Z, K(KWalk)));
    // The File-to arm mounts the walk's child when the candidate
    // list is non-empty — the fixture's rig always has candidates
    // (the stage's own rows); the empty-list inert is the journeys'
    // shape.
    row(FK::ReviewWalk, E::WalkFileArm, exp(Z, K(KChild)));
    row(FK::ReviewWalk, E::WalkVerb, exp(Z, K(KWalk)));
    // The Record/Finish exits land on the completion summary now —
    // the walk stays live (J4 D-1/D-8).
    row(FK::ReviewWalk, E::WalkExit, exp(Z, K(KWalk)));
    // -- RECUR EDITOR rows (round-7 D-7.1): the opener is the
    //    entries-overlay's shape — an explicit table arm that replaces
    //    any foreign flow without settling it (no commit), opens the
    //    editor over the rig's subject; under the live editor it is
    //    the kept inert (the modal discipline).
    for kind in [
        FK::None,
        FK::Text,
        FK::Due,
        FK::Move,
        FK::Drag,
        FK::Palette,
        FK::Capture,
        FK::FocusOverlay,
        FK::TrackList,
        FK::TrackForm,
        FK::PlanForm,
        FK::ReviewWalk,
        FK::Child,
        FK::PlanChild,
    ] {
        row(kind, E::SeriesEditOpen, exp(Z, K(KRecur)));
    }
    row(FK::RecurEditor, E::SeriesEditOpen, exp(Z, K(KRecur)));
    // The detach strip's flips (round-7 D-7.1): flag flips over the
    // editor level — the flow is preserved under every foreign kind
    // (the recur trio's keep shape), a no-op from None, and the
    // editor stays under its own flow.
    row(FK::None, E::SeriesDetach, exp(Z, N));
    for kind in [
        FK::Text,
        FK::Due,
        FK::Move,
        FK::Drag,
        FK::Palette,
        FK::Capture,
        FK::FocusOverlay,
        FK::TrackList,
        FK::TrackForm,
        FK::PlanForm,
        FK::ReviewWalk,
        FK::Child,
        FK::PlanChild,
    ] {
        row(kind, E::SeriesDetach, exp(Z, held_flow_keep(kind)));
    }
    row(FK::RecurEditor, E::SeriesDetach, exp(Z, K(KRecur)));
    // The Escape ladder (#976 SB1, US-2/S3 — the deliberate
    // class-matrix amendment): the mode.normal resolution under the
    // live editor is the explicit cancel — the clean close, nothing
    // dispatches. This is the kept explicit arm the pair row pins
    // over the Edit class's named-inert default (the old matrix cell
    // pinned a limitation, not a contract); SetModeInsert/Activate
    // stay the named inert via the class expectation.
    row(FK::RecurEditor, E::SetModeNormal, exp(Z, N));
    // -- PLAN SURFACE rows (#622, D-3/D-4): from None the open
    //    toggles the view (flow-free), begin-place arms the form,
    //    remove-box is the one honest write; the swaps are inert
    //    without placed boxes. Under the armed form the open is the
    //    Escape rung (form → pane), the commit with empty buffers is
    //    the typed inert, the other planner verbs keep the form.
    row(FK::None, E::PlanOpen, exp(Z, N));
    row(FK::None, E::PlanBegin, exp(Z, K(KPlanForm)));
    row(FK::None, E::PlanCommit, exp(Z, N));
    row(FK::None, E::PlanBox, exp(Z, N));
    row(FK::None, E::PlanRemove, exp(NZ, N));
    row(FK::PlanForm, E::PlanOpen, exp(Z, N));
    // The embedded planner's form is the planner's own child (J4
    // D-2): the mount keeps the child cell's flow shape at Kind(PlanForm).
    row(FK::Child, E::PlanBegin, exp(Z, K(KChild)));
    // The embedded planner's form mounts as the planner's own child
    // (J4 D-2).
    row(FK::PlanChild, E::PlanBegin, exp(Z, K(KPlanForm)));
    row(FK::PlanForm, E::PlanBegin, exp(Z, K(KPlanForm)));
    row(FK::PlanForm, E::PlanCommit, exp(Z, K(KPlanForm)));
    row(FK::PlanForm, E::PlanBox, exp(Z, K(KPlanForm)));
    row(FK::PlanForm, E::PlanRemove, exp(Z, K(KPlanForm)));
    // The embedded planner's unplace: the honest write lands under
    // the child (J4 D-2); the plain children keep the named inert.
    row(FK::Child, E::PlanRemove, exp(Z, K(KChild)));
    row(FK::PlanChild, E::PlanRemove, exp(NZ, K(KPlanChild)));
    // A held foreign flow keeps every planner intent inert (the
    // modal discipline verbatim: Escape out of the flow first).
    for kind in [
        FK::Text,
        FK::Due,
        FK::Move,
        FK::Drag,
        FK::Palette,
        FK::Capture,
        FK::FocusOverlay,
        FK::TrackList,
        FK::TrackForm,
        FK::LabelOverlay,
        FK::ReviewWalk,
        FK::RecurEditor,
        FK::Child,
        FK::PlanChild,
    ] {
        let keep = held_flow_keep(kind);
        for key in [
            E::PlanOpen,
            E::PlanBegin,
            E::PlanCommit,
            E::PlanBox,
            E::PlanRemove,
        ] {
            row(kind, key, exp(Z, keep));
        }
    }

    // The embedded planner child mirrors the child cells verbatim
    // (the depth-1 XOR replaces it like any child; its kept arms —
    // the walk verbs and the planner rows — are the earlier rows).
    row(FK::PlanChild, E::BoardDefaultPreset, exp(Z, N));
    row(FK::PlanChild, E::TrackEntryDismiss, exp(Z, N));
    row(FK::PlanChild, E::LabelsDismiss, exp(Z, N));
    // The embedded planner child mirrors the child cells verbatim
    // (the depth-1 XOR replaces it like any child; its kept arms —
    // the walk verbs and the planner rows — are the earlier rows).
    row(FK::PlanChild, E::BeginCreate, exp(Z, K(KTxt)));
    row(FK::PlanChild, E::BeginCreateStack, exp(Z, K(KTxt)));
    row(FK::PlanChild, E::RequestMove, exp(Z, K(KMove)));
    row(FK::PlanChild, E::StackDelete, exp(Z, K(KNone)));
    row(FK::PlanChild, E::StackRename, exp(Z, K(KTxt)));
    row(FK::PlanChild, E::StackMove, exp(Z, K(KMove)));
    row(FK::PlanChild, E::BoardCreate, exp(Z, K(KTxt)));
    row(FK::PlanChild, E::BoardColorEdit, exp(Z, K(KNone)));
    row(FK::PlanChild, E::BoardDelete, exp(Z, K(KNone)));
    row(FK::PlanChild, E::BoardCeremony, exp(Z, K(KNone)));
    row(FK::PlanChild, E::BoardRename, exp(Z, K(KTxt)));
    row(FK::PlanChild, E::BeginTitleEdit, exp(Z, K(KTxt)));
    row(FK::PlanChild, E::TrackEntriesOpen, exp(Z, K(KTrackList)));
    row(FK::PlanChild, E::ReviewOpen, exp(Z, K(KWalk)));
    row(FK::PlanChild, E::WalkAdvance, exp(Z, K(KChild)));
    row(FK::PlanChild, E::WalkExit, exp(Z, K(KChild)));
    row(FK::PlanChild, E::WalkVerb, exp(Z, K(KChild)));
    row(FK::PlanChild, E::WalkFileArm, exp(Z, K(KChild)));
    row(FK::PlanChild, E::LabelsOpen, exp(Z, K(KLabel)));
    row(FK::PlanChild, E::PlanBegin, exp(Z, K(KChild)));
    row(FK::PlanChild, E::PlanRemove, exp(Z, K(KChild)));
    t
}

fn lookup(kind: FK, key: EK, class: EC) -> Exp {
    let pair = pair_table()
        .into_iter()
        .find(|(k, e, _)| *k == kind && *e == key);
    if let Some((_, _, e)) = pair {
        return e;
    }
    class_table()
        .into_iter()
        .find(|(k, c, _)| *k == kind && *c == class)
        .map_or_else(
            || panic!("no matrix row for ({kind:?}, {class:?}/{key:?})"),
            |(_, _, e)| e,
        )
}

// -- the harness ----------------------------------------------------------

struct Fixture {
    board: BoardId,
    stacks: [StackId; 2],
    tasks: [TaskId; 3],
}

/// The `view_state` fixture's shape (one live board, two stacks, three
/// tasks a/b/c).
fn fixture() -> (AppState, Fixture) {
    let board = BoardId::from_uuid(uuid::Uuid::now_v7());
    let (s0, s1) = (
        StackId::from_uuid(uuid::Uuid::now_v7()),
        StackId::from_uuid(uuid::Uuid::now_v7()),
    );
    let (a, b, c) = (
        TaskId::from_uuid(uuid::Uuid::now_v7()),
        TaskId::from_uuid(uuid::Uuid::now_v7()),
        TaskId::from_uuid(uuid::Uuid::now_v7()),
    );
    let now = BoardClocks::epoch();
    let mut state = AppState::default();
    state.boards.insert(
        board,
        Board {
            id: board,
            remote: None,
            title: "board".to_owned(),
            color: Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: now,
                color: now,
                archived: now,
                deleted: now,
            },
            authors: taskboard_domain::entities::BoardAuthors::default(),
            remote_seen: None,
        },
    );
    let stack = |id: StackId, order: i64| Stack {
        id,
        remote: None,
        board,
        title: format!("stack-{order}"),
        order,
        archived: false,
        deleted: false,
        clocks: StackClocks {
            title: now,
            order: now,
            deleted: now,
        },
        authors: taskboard_domain::entities::StackAuthors::default(),
        remote_seen: None,
    };
    state.stacks.insert(s0, stack(s0, 0));
    state.stacks.insert(s1, stack(s1, 1));
    let task = |id: TaskId, stk: StackId, order: i64| Task {
        id,
        remote: None,
        label_memberships: BTreeMap::new(),
        title: format!("task-{order}"),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        stack: stk,
        order,
        effort: None,
        importance: None,
        urgency: None,
        recur: None,
        kind: taskboard_domain::entities::TaskKind::Standard,
        archived: false,
        deleted: false,
        clocks: TaskClocks {
            title: now,
            description: now,
            duedate: now,
            scheduled: now,
            done: now,
            position: now,
            effort: now,
            importance: now,
            urgency: now,
            recur: now,
            kind: now,
            archived: now,
            deleted: now,
        },
        authors: taskboard_domain::entities::TaskAuthors::default(),
        remote_seen: None,
        remote_baseline: None,
    };
    state.tasks.insert(a, task(a, s0, 0));
    state.tasks.insert(b, task(b, s0, 1));
    state.tasks.insert(c, task(c, s1, 0));
    (
        state,
        Fixture {
            board,
            stacks: [s0, s1],
            tasks: [a, b, c],
        },
    )
}

fn all_flows(fx: &Fixture) -> Vec<Flow> {
    let [s0, s1] = fx.stacks;
    let [a, _b, _c] = fx.tasks;
    vec![
        Flow::None,
        Flow::Create { stack: s1 },
        Flow::CreateStack,
        Flow::EditTitle { task: a },
        Flow::EditDate {
            task: a,
            field: crate::view_state::DateField::Due,
        },
        Flow::EditDate {
            task: a,
            field: crate::view_state::DateField::Scheduled,
        },
        Flow::EditDescription { task: a },
        Flow::Move {
            task: a,
            target: s1,
            slot: 0,
        },
        Flow::MoveStack { stack: s1, slot: 0 },
        Flow::DragMove {
            task: a,
            target: s1,
            gap: None,
        },
        Flow::Palette { selected: 0 },
        Flow::Capture,
        Flow::CreateBoard,
        Flow::RenameStack { stack: s0 },
        Flow::RenameBoard { board: fx.board },
        // The entries overlay's two surfaces (r4 chunk 10, D-TS2):
        // the rig's view is the task detail over `a` (the strand-proof
        // keeps them alive under reconcile).
        Flow::TrackEntries { task: a },
        Flow::TrackEntryForm {
            task: a,
            seed: None,
        },
        // The placement form (#622): the rig views the subject, so
        // reconcile keeps it alive.
        Flow::PlanPlace { task: a },
        // The recur editor (round-7 D-7.1): the rig views the subject
        // (an open task), so reconcile keeps it alive.
        Flow::EditRecur { task: a },
        // The review walk (#638): subject-free — any stage reconciles
        // fine over the fixture.
        Flow::ReviewWalk {
            cadence: taskboard_domain::ReviewCadence::Day,
            reference: chrono::NaiveDate::from_ymd_opt(2026, 11, 5).expect("date"),
            stage: crate::view_state::WalkStage::Inbox,
            cursor: 0,
            tally: crate::view_state::WalkTally::default(),
        },
        // The embedded planner child (J4 D-2): a child level over the
        // walk fixture — same cell family, no kept intents of its own.
        Flow::ReviewPlanner,
        // The child levels (chunk 3's ports): armed at depth 1 the
        // fixtures exercise their settle cell (the Cancelled end);
        // nested mounts are the journeys' and the proptest's shape.
        Flow::EntryDatePicker,
        Flow::WalkFilePick { armed: None },
        Flow::WalkDatePick,
    ]
}

fn all_effects(fx: &Fixture) -> Vec<UiEffect> {
    vec![
        UiEffect::Exit,
        UiEffect::OpenMenu,
        UiEffect::OpenPalette,
        UiEffect::CloseMenu,
        UiEffect::MenuBack,
        UiEffect::MenuNext,
        UiEffect::MenuPrevious,
        UiEffect::FocusToday,
        UiEffect::OpenInbox,
        UiEffect::BeginCapture,
        UiEffect::InvokeSelected,
        UiEffect::SetMode(Mode::Normal),
        UiEffect::SetMode(Mode::Insert),
        UiEffect::FocusNext,
        UiEffect::FocusPrev,
        // The directional family runs the whole matrix too (J3 US-1):
        // each direction is payload-distinct, but the interpretation
        // is payload-blind at the settle layer (the class decides).
        UiEffect::FocusDirectional(crate::router::Direction::Left),
        UiEffect::FocusDirectional(crate::router::Direction::Right),
        UiEffect::FocusDirectional(crate::router::Direction::Up),
        UiEffect::FocusDirectional(crate::router::Direction::Down),
        UiEffect::ExtendSelectionNext,
        UiEffect::ExtendSelectionPrev,
        UiEffect::NextView,
        UiEffect::PrevView,
        UiEffect::FocusBoard,
        // Both payloads run the whole matrix (r4 D4): the driver
        // pins payload-blindness by construction, not convention.
        UiEffect::BeginCreate { target: None },
        UiEffect::BeginCreate {
            target: Some(fx.stacks[1]),
        },
        UiEffect::BeginCreateStack,
        UiEffect::Activate,
        UiEffect::RequestMove,
        UiEffect::BoardNext,
        UiEffect::BoardPrevious,
        UiEffect::BoardSelect(fx.board),
        UiEffect::StackWider,
        UiEffect::StackNarrower,
        UiEffect::CancelMove,
        UiEffect::StackNext,
        UiEffect::StackPrev,
        UiEffect::StackDelete,
        UiEffect::StackRename,
        UiEffect::StackMove,
        UiEffect::BoardCreate,
        UiEffect::BoardDelete,
        UiEffect::BoardRename,
        UiEffect::BoardColorEdit,
        UiEffect::BoardDefaultPreset,
        // The board ceremony verbs (OP1): the lifecycle family's
        // matrix members.
        UiEffect::BoardArchive,
        UiEffect::BoardReset,
        UiEffect::BoardReconcile,
        UiEffect::FocusFirst,
        // OpenBoardBarFlyout is excluded: applied in `apply_effect`,
        // before the table (the picker keys' exclusion).
        UiEffect::FocusLast,
        UiEffect::OpenSettings,
        UiEffect::ExportDialogOpen,
        UiEffect::ExportDialogClose,
        UiEffect::OpenStatusPanel,
        UiEffect::BeginTitleEdit,
        // The keymap-parity D-4 editor arms (US-3): the subject is
        // entered by the driver, so both arms arm their flows.
        UiEffect::BeginDateEdit {
            field: crate::view_state::DateField::Due,
        },
        UiEffect::BeginDateEdit {
            field: crate::view_state::DateField::Scheduled,
        },
        UiEffect::BeginDescriptionEdit,
        UiEffect::ShowTask,
        UiEffect::ControlNext,
        UiEffect::ControlPrev,
        UiEffect::ScrollUp,
        UiEffect::ScrollDown,
        UiEffect::TrackEntriesOpen,
        UiEffect::TrackEntryDismiss,
        UiEffect::LabelsOpen,
        UiEffect::LabelsDismiss,
        // The priority pickers' opener (UX R-3 D-1, #788): both
        // payloads run the whole matrix (the BoardSelect
        // payload-blindness shape).
        UiEffect::PriorityPickerOpen(crate::router::PriorityField::Importance),
        UiEffect::PriorityPickerOpen(crate::router::PriorityField::Urgency),
        // The recur row's view intents (issue #506) and HF1's pause
        // editor opener: the same view intent class as the entries'
        // open.
        UiEffect::SeriesPauseOpen,
        UiEffect::HabitPauseOpen,
        UiEffect::HabitConvertArm,
        UiEffect::HabitRemoveArm,
        UiEffect::HabitRemoveCancel,
        UiEffect::HabitEdit,
        // The HF3 history opener: the same view-intent class as the
        // editor opener (the read-role dialog's flag flip).
        UiEffect::HabitHistoryOpen,
        UiEffect::HabitCreateToggle,
        UiEffect::SeriesConvertArm,
        // The recur editor's intents (round-7 D-7.1).
        UiEffect::SeriesEditorOpen,
        UiEffect::SeriesDetachArm,
        UiEffect::SeriesDetachCancel,
        // The plan-next-day family (#622).
        UiEffect::OpenPlanNextDay,
        UiEffect::PlanBeginPlace(fx.tasks[0]),
        UiEffect::PlanCommitPlace,
        UiEffect::PlanBoxEarlier(fx.tasks[0]),
        UiEffect::PlanBoxLater(fx.tasks[0]),
        UiEffect::PlanRemoveBox(fx.tasks[0]),
        // The review walk family (#638): the opener plus one intent
        // per matrix cell class (advance keeps, record exits, file
        // arms — the row verbs share the keep cell).
        UiEffect::OpenReviewWalk,
        UiEffect::ReviewWalk(crate::router::WalkIntent::Advance),
        UiEffect::ReviewWalk(crate::router::WalkIntent::Record),
        UiEffect::ReviewWalk(crate::router::WalkIntent::InboxFile),
    ]
}

/// One mounted window + adapter; each pair pass resets the machines,
/// arms the flow, drives the effect, and drains the command lane.
struct Rig {
    handle: UiHandle,
    commands_rx: mpsc::Receiver<StateCommand>,
}

fn dummy_receiver<T: Send + Clone + 'static>() -> tokio::sync::broadcast::Receiver<T> {
    let (tx, rx) = tokio::sync::broadcast::channel(16);
    drop(tx);
    rx
}

fn rig(state: AppState) -> Rig {
    let state = Arc::new(arc_swap::ArcSwap::from_pointee(state));
    let (commands_tx, commands_rx) = mpsc::channel(64);
    let (requests_tx, _requests_rx) = mpsc::channel(16);
    let (_health_tx, health) =
        tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
    let handle = mount(
        state,
        dummy_receiver(),
        dummy_receiver(),
        commands_tx,
        requests_tx,
        UiSettings::default(),
        health,
        None,
    )
    .expect("mount succeeds on the null platform");
    Rig {
        handle,
        commands_rx,
    }
}

/// A rig mounted with an explicit keymap (the hint-projection legs'
/// fixture continuity — the empty-map and per-preset empty legs).
fn keyed_rig(state: AppState, keymap: crate::keymap::Keymap, preset: crate::keymap::Preset) -> Rig {
    let state = Arc::new(arc_swap::ArcSwap::from_pointee(state));
    let (commands_tx, commands_rx) = mpsc::channel(64);
    let (requests_tx, _requests_rx) = mpsc::channel(16);
    let (_health_tx, health) =
        tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
    let handle = mount(
        state,
        dummy_receiver(),
        dummy_receiver(),
        commands_tx,
        requests_tx,
        UiSettings {
            keymap,
            keymap_preset: preset,
            ..UiSettings::default()
        },
        health,
        None,
    )
    .expect("mount succeeds on the null platform");
    Rig {
        handle,
        commands_rx,
    }
}

fn emacs_map_rig(state: AppState) -> Rig {
    keyed_rig(
        state,
        crate::keymap::Keymap::from_preset(crate::keymap::Preset::Emacs),
        crate::keymap::Preset::Emacs,
    )
}

fn empty_map_rig(state: AppState) -> Rig {
    keyed_rig(
        state,
        crate::keymap::Keymap::default(),
        crate::keymap::Preset::Standard,
    )
}
impl Rig {
    fn adapter(&self) -> Rc<RefCell<crate::adapter::Adapter>> {
        self.handle.test_adapter()
    }

    fn drain(&mut self) -> usize {
        self.drain_commands().len()
    }

    fn drain_commands(&mut self) -> Vec<StateCommand> {
        let mut out = Vec::new();
        while let Ok(command) = self.commands_rx.try_recv() {
            out.push(command);
        }
        out
    }
}

#[test]
#[allow(clippy::too_many_lines)] // one driver over the whole matrix
fn every_flow_effect_pair_resolves_to_its_matrix_row() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let mut rig = rig(state);
        let adapter = rig.adapter();
        let effects = all_effects(&fx);
        let flows = all_flows(&fx);
        let mut failures = Vec::new();
        for flow in &flows {
            for effect in &effects {
                let expected = lookup(kind_of(flow), key_of(effect), class_of(effect));
                // Reset, seed focus/selection, arm the exact flow
                // under test (last — reconcile's drops must not see
                // it), and seed a non-empty text buffer for the TEXT
                // rows' commit side.
                {
                    let mut a = adapter.borrow_mut();
                    a.test_reset();
                    let live = rig.handle.test_state();
                    a.view_state.reconcile(&live.load_full());
                    let [s0, _s1] = fx.stacks;
                    let [a_id, _b, _c] = fx.tasks;
                    a.view_state.activate_card(a_id, s0);
                    // Enter the detail (the registry scopes the title
                    // action to TaskDetail — the subject must be
                    // viewed for the TITLE cells to mean anything).
                    a.view_state
                        .activate_focused(&rig.handle.test_state().load_full());
                    a.view_state.arm_flow(flow.clone());
                    if matches!(kind_of(flow), FK::Text | FK::Capture) {
                        // The TEXT rows' commit side and capture's
                        // commit-and-continue thought. Armed first —
                        // the buffer lives in the flow's own level.
                        a.view_state.set_buffer("typed".to_owned());
                    }
                }
                {
                    let mut a = adapter.borrow_mut();
                    a.interpret_for_test(rig.handle.window(), *effect);
                }
                let commands = rig.drain();
                let flow_after = adapter.borrow().view_state.flow().clone();
                let observed = exp(
                    if commands == 0 {
                        Cmd::Zero
                    } else {
                        Cmd::NonZero
                    },
                    match &flow_after {
                        Flow::None => Post::None,
                        f => Post::Kind(kind_of(f)),
                    },
                );
                if observed != expected {
                    failures.push(format!(
                        "({flow:?}, {effect:?}): expected {expected:?}, observed {observed:?} (flow after: {flow_after:?})"
                    ));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} pairs off-matrix:\n{}",
            failures.len(),
            flows.len() * effects.len(),
            failures.join("\n")
        );
    });
}

/// Round-3 D25 — the walk's re-entrancy guard: with the flag held
/// the Tab stroke rejects before the callback borrows (the injected
/// echo can never re-enter the resolver — a hang or a double step is
/// the failure); with the flag clear the bound walk row resolves and
/// dispatches exactly one traversal. The flag is checked ahead of the
/// `borrow_mut`, so the guard holds while the walk's own borrow is
/// live — the borrow would panic otherwise, and the panic IS the
/// assertion's teeth here.
#[test]
fn walk_reentry_rejects_exactly_one_tab() {
    on_slint_thread(|| {
        let (state, _fx) = fixture();
        let rig = rig(state);
        let adapter = rig.adapter();
        {
            let mut a = adapter.borrow_mut();
            a.test_reset();
            // A view context so the Tab binding is chain-visible.
            a.shell.view = Some(crate::actions::ContextId::Board);
        }
        // The guard rejects while held (the injected echo's path).
        adapter.borrow().walk_reentry.set(true);
        let rejected = {
            let mut a = adapter.borrow_mut();
            a.intercept_tab_for_test(rig.handle.window())
        };
        assert!(!rejected, "the held flag rejects the injected Tab");
        adapter.borrow().walk_reentry.set(false);

        // The cleared flag: the bound row resolves, the synthetic
        // traversal fires under the guard, and the flag clears after.
        let consumed = {
            let mut a = adapter.borrow_mut();
            a.intercept_tab_for_test(rig.handle.window())
        };
        assert!(consumed, "the bound walk row resolves");
        assert!(
            !adapter.borrow().walk_reentry.get(),
            "the guard clears when the dispatch returns"
        );
    });
}

/// Round-3 D27c — the greying flip: the adapter's `entry_states` map
/// flips the move-cancel row with the live move flow and the
/// complete/reopen pair with the focused task's done stamp, while a
/// static row (task.create carries no state row) stays off the map.
#[test]
fn entry_states_grey_the_enumerated_rows_per_state() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let rig = rig(state);
        let adapter = rig.adapter();
        {
            let mut a = adapter.borrow_mut();
            a.test_reset();
            let live = rig.handle.test_state();
            a.view_state.reconcile(&live.load_full());
            let [a_id, _b, _c] = fx.tasks;
            a.view_state.activate_card(a_id, fx.stacks[0]);
        }
        let states = |adapter: &Rc<RefCell<crate::adapter::Adapter>>| {
            let a = adapter.borrow();
            let live = rig.handle.test_state();
            let flow = a.view_state.flow().clone();
            a.entry_states(&live.load_full(), &flow)
        };

        // A focused open task: complete on, reopen off, move/edit/
        // delete on, cancel off (no flow), track-start on.
        let s = states(&adapter);
        assert!(!s[&crate::actions::ActionId::TaskMoveCancel]);
        assert!(s[&crate::actions::ActionId::TaskComplete]);
        assert!(!s[&crate::actions::ActionId::TaskReopen]);
        assert!(s[&crate::actions::ActionId::TaskMove]);
        assert!(s[&crate::actions::ActionId::TaskTrackStart]);
        assert!(!s[&crate::actions::ActionId::TaskTrackStop]);
        assert!(!s.contains_key(&crate::actions::ActionId::TaskCreate));

        // A live move flow flips the cancel row on (and the walk is
        // still armed under it).
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::RequestMove);
        let s = states(&adapter);
        assert!(s[&crate::actions::ActionId::TaskMoveCancel]);

        // Mark the focused task done: the complete/reopen pair flips.
        {
            let mut a = adapter.borrow_mut();
            a.interpret_for_test(rig.handle.window(), UiEffect::CancelMove);
        }
        let live = rig.handle.test_state();
        let id = fx.tasks[0];
        let mut fresh = (*live.load_full()).clone();
        fresh.tasks.get_mut(&id).expect("task").done = Some(taskboard_domain::BoardClocks::epoch());
        live.store(std::sync::Arc::new(fresh));
        let s = states(&adapter);
        assert!(!s[&crate::actions::ActionId::TaskComplete]);
        assert!(s[&crate::actions::ActionId::TaskReopen]);
    });
}

/// The class matrix covers each `(FlowKind, EffectClass)` cell
/// exactly once — a new variant without a row fails the driver above
/// via the missing-row panic or the compile-time-exhaustive mirrors.
#[test]
fn the_matrix_covers_every_class_cell() {
    let cells = class_table();
    let mut seen = std::collections::BTreeSet::new();
    for (kind, class, _) in &cells {
        assert!(
            seen.insert((*kind, *class)),
            "duplicate row ({kind:?}, {class:?})"
        );
    }
    assert_eq!(
        cells.len(),
        16 * 16,
        "one row per (FlowKind, EffectClass) cell"
    );
}

// -- the R2 drop-miss journeys (F8) --------------------------------------

use crate::view_state::ViewState;

#[test]
fn a_stale_column_drop_settles_the_drag_without_dispatching() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let mut rig = rig(state);
        let adapter = rig.adapter();
        {
            let mut a = adapter.borrow_mut();
            a.test_reset();
            a.view_state.reconcile(&rig.handle.test_state().load_full());
            let [a_id, _b, _c] = fx.tasks;
            a.view_state.arm_flow(Flow::DragMove {
                task: a_id,
                target: fx.stacks[1],
                gap: None,
            });
        }
        // The drop lands past the id tables (the stale-refresh race):
        // the old early-out left the flow armed here.
        rig.handle
            .window()
            .invoke_board_card_dropped(i32::try_from(usize::MAX).unwrap_or(i32::MAX));
        let flow = adapter.borrow().view_state.flow().clone();
        assert_eq!(flow, Flow::None, "the stale drop settles the drag");
        assert_eq!(rig.drain(), 0, "a missed drop guesses no target");
    });
}

#[test]
fn a_swallowed_completed_confirm_still_settles_the_drag() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let mut rig = rig(state);
        let adapter = rig.adapter();
        {
            let mut a = adapter.borrow_mut();
            a.test_reset();
            a.view_state.reconcile(&rig.handle.test_state().load_full());
            let [a_id, _b, _c] = fx.tasks;
            a.view_state.arm_flow(Flow::DragMove {
                task: a_id,
                target: fx.stacks[1],
                gap: None,
            });
        }
        // `completed == true` with the flow still armed: the drop
        // seam's confirm was swallowed — the finished seam cancels.
        rig.handle
            .window()
            .invoke_board_card_drag_finished(0, 0, true);
        let flow = adapter.borrow().view_state.flow().clone();
        assert_eq!(flow, Flow::None, "the swallowed confirm cancels");
        assert_eq!(rig.drain(), 0, "the swallowed confirm dispatches nothing");
    });
}

#[test]
fn a_live_on_tables_drop_still_confirms_with_one_move() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let mut rig = rig(state);
        let adapter = rig.adapter();
        {
            let mut a = adapter.borrow_mut();
            a.test_reset();
            a.view_state.reconcile(&rig.handle.test_state().load_full());
            let [a_id, _b, _c] = fx.tasks;
            a.view_state.arm_flow(Flow::DragMove {
                task: a_id,
                target: fx.stacks[1],
                gap: None,
            });
        }
        rig.handle.window().invoke_board_card_dropped(1);
        let flow = adapter.borrow().view_state.flow().clone();
        assert_eq!(flow, Flow::None, "the confirm ends the flow");
        let drained: Vec<_> = std::iter::from_fn(|| rig.commands_rx.try_recv().ok()).collect();
        assert!(
            matches!(drained.as_slice(), [StateCommand::MoveTask { .. }]),
            "the on-tables drop dispatches exactly the move, got {drained:?}"
        );
    });
}

#[allow(unused)]
fn _view_state_witness(_: &ViewState) {}

// -- the §8 buffer-lifetime invariants (W-SM-c) --------------------------

mod buffer_invariants {
    use super::*;

    fn arm_due(rig: &Rig, adapter: &Rc<RefCell<crate::adapter::Adapter>>, fx: &Fixture) {
        let mut a = adapter.borrow_mut();
        a.test_reset();
        a.view_state.reconcile(&rig.handle.test_state().load_full());
        let [a_id, _b, _c] = fx.tasks;
        a.view_state.arm_flow(Flow::EditDate {
            task: a_id,
            field: crate::view_state::DateField::Due,
        });
    }

    #[test]
    fn the_due_buffers_die_with_the_flow_on_every_end_path() {
        on_slint_thread(|| {
            let (state, fx) = fixture();
            let rig = rig(state);
            let adapter = rig.adapter();

            // The leaving-settle end path.
            arm_due(&rig, &adapter, &fx);
            {
                let mut a = adapter.borrow_mut();
                a.test_seed_due_buffers();
                a.interpret_for_test(rig.handle.window(), UiEffect::NextView);
            }
            assert!(adapter.borrow().due_buffers_empty(), "leaving wipes");
            assert_eq!(adapter.borrow().view_state.flow().clone(), Flow::None);

            // A plain refresh with the flow gone (the reconcile-drop
            // shape): the level's buffers died with the discard — the
            // refresh must not resurrect anything. (Seeding stale
            // buffers while flowless is unrepresentable: the state
            // lives in the level now.)
            arm_due(&rig, &adapter, &fx);
            adapter.borrow_mut().view_state.discard_edit();
            {
                let mut a = adapter.borrow_mut();
                a.refresh(rig.handle.window());
            }
            assert!(adapter.borrow().due_buffers_empty(), "refresh wipes");

            // While the flow is armed, a refresh keeps the buffers.
            arm_due(&rig, &adapter, &fx);
            {
                let mut a = adapter.borrow_mut();
                a.test_seed_due_buffers();
                a.refresh(rig.handle.window());
            }
            assert!(
                !adapter.borrow().due_buffers_empty(),
                "a live EditDate keeps its buffers across refreshes"
            );
        });
    }

    #[test]
    fn the_gesture_buffers_die_on_a_settle_and_survive_a_refresh() {
        on_slint_thread(|| {
            let (state, fx) = fixture();
            let rig = rig(state);
            let adapter = rig.adapter();
            let [s0, s1] = fx.stacks;

            // A live width gesture survives a refresh (the r2
            // interleaved-refresh survival pin's buffer half).
            adapter.borrow_mut().arm_width_drag(s0, s1);
            adapter.borrow_mut().refresh(rig.handle.window());
            assert!(adapter.borrow().width_drag_armed());

            // A settle (here: a leaving effect) drops it — the header
            // it addresses is about to unmount.
            adapter
                .borrow_mut()
                .interpret_for_test(rig.handle.window(), UiEffect::NextView);
            assert!(!adapter.borrow().width_drag_armed());
        });
    }
}

// -- the unchanged-refresh guard (#463) --------------------------------
//
// The pin seam is the adapter's `derive_passes` counter (incremented
// only where `models::push` is about to run): typed, deterministic,
// zero timing. One behavior, one test, lowest layer — no journey
// duplicates these.

fn guard_rig(state: AppState, settings: UiSettings) -> Rig {
    let state = Arc::new(arc_swap::ArcSwap::from_pointee(state));
    let (commands_tx, commands_rx) = mpsc::channel(64);
    let (requests_tx, _requests_rx) = mpsc::channel(16);
    let (_health_tx, health) =
        tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
    let handle = mount(
        state,
        dummy_receiver(),
        dummy_receiver(),
        commands_tx,
        requests_tx,
        settings,
        health,
        None,
    )
    .expect("mount succeeds on the null platform");
    Rig {
        handle,
        commands_rx,
    }
}

/// T1 — the load-bearing pin: an unchanged second refresh executes no
/// derive+diff pass; a publish derives; an identical-values republish
/// (a guard miss) costs exactly one wasted pass, never staleness.
#[test]
fn the_guard_skips_the_derive_while_the_snapshot_is_unchanged() {
    on_slint_thread(|| {
        let rig = guard_rig(AppState::default(), UiSettings::default());
        // Step 1: mount's first-frame pass derives from the `None` cache.
        assert_eq!(rig.adapter().borrow().derive_passes, 1);
        // Step 2: the unchanged second refresh is skipped.
        rig.handle.refresh();
        assert_eq!(rig.adapter().borrow().derive_passes, 1);
        // Step 3: a publish (a new Arc) derives.
        rig.handle.test_state().store(Arc::new(AppState {
            last_updated: Some(chrono::Utc::now()),
            ..AppState::default()
        }));
        rig.handle.refresh();
        assert_eq!(rig.adapter().borrow().derive_passes, 2);
        // Step 4: unchanged again — skipped.
        rig.handle.refresh();
        assert_eq!(rig.adapter().borrow().derive_passes, 2);
        // Step 5: an identical-values republish is a guard miss — one
        // wasted pass, never a stale UI (the issue's own contract).
        rig.handle.test_state().store(Arc::new(AppState::default()));
        rig.handle.refresh();
        assert_eq!(rig.adapter().borrow().derive_passes, 3);
    });
}

/// T2 — the forced verb bypasses the cache on an unchanged snapshot.
#[test]
fn a_forced_refresh_derives_on_an_unchanged_snapshot() {
    on_slint_thread(|| {
        let rig = guard_rig(AppState::default(), UiSettings::default());
        let adapter = rig.adapter();
        assert_eq!(adapter.borrow().derive_passes, 1);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert_eq!(adapter.borrow().derive_passes, 2);
    });
}

/// T3 — a view intent derives on an unchanged snapshot through the
/// production entrance: the flow machine's `RefreshNow` verdict forces
/// the pass, and the derive actually materializes (`board_create_open`
/// is set only by `models::push`).
#[test]
fn a_view_intent_derives_on_an_unchanged_snapshot() {
    on_slint_thread(|| {
        let (state, _fx) = fixture();
        let rig = guard_rig(state, UiSettings::default());
        let adapter = rig.adapter();
        assert_eq!(adapter.borrow().derive_passes, 1);
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::BoardCreate);
        assert_eq!(adapter.borrow().derive_passes, 2);
        assert!(
            rig.handle.window().get_board_create_open(),
            "the derive materialized the create field onto the models"
        );
    });
}

// -- the tracked-time summary row (r4 D17) ------------------------------

/// A5 — with an open entry the task-detail model carries a non-empty
/// total; once the entry closes, the value is refresh-static: a forced
/// re-derive at a strictly later wall clock rewrites the identical
/// phrase (the closed-entries sum; typed, zero timing — no sleeps).
#[test]
fn the_task_detail_model_carries_the_tracked_total_and_it_freezes_when_closed() {
    on_slint_thread(|| {
        let (mut state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let start = chrono::Utc::now();
        let entry_id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
        state.tracking.entries.insert(
            entry_id,
            taskboard_domain::TimeEntry {
                id: entry_id,
                task: a_id,
                task_title: "task-0".to_owned(),
                start,
                end: None,
                created_at: start,
                updated_at: start,
                kind: taskboard_domain::tracking::TimeEntryKind::Work,
                by: None,
            },
        );
        let rig = rig(state);
        let adapter = rig.adapter();
        {
            let mut a = adapter.borrow_mut();
            // The view-state move is outside the state Arc — the #463
            // audit routes it through the forced verb.
            a.refresh_forced(rig.handle.window());
            a.view_state.activate_card(a_id, s0);
            a.view_state
                .activate_focused(&rig.handle.test_state().load_full());
            a.refresh_forced(rig.handle.window());
        }
        let ticking = rig.handle.window().get_task_model().tracked_total;
        assert!(!ticking.is_empty(), "an open entry renders the total");

        // Close the entry at a fixed end: the total becomes a closed
        // sum, invariant under later `now`s.
        let closed = {
            let mut s = (*rig.handle.test_state().load_full()).clone();
            let e = s.tracking.entries.get_mut(&entry_id).expect("entry");
            e.end = Some(e.start + chrono::TimeDelta::seconds(120));
            Arc::new(s)
        };
        rig.handle.test_state().store(closed);
        adapter.borrow_mut().refresh(rig.handle.window());
        let frozen = rig.handle.window().get_task_model().tracked_total;
        assert!(!frozen.is_empty());
        // A forced pass (a later wall clock behind it) rewrites the
        // same value — the refresh-static contract.
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert_eq!(
            rig.handle.window().get_task_model().tracked_total,
            frozen,
            "a closed sum never moves with `now`"
        );
    });
}

// -- the per-second tick gate (r4 D17, §2 decision 5) -------------------

/// Mints one open entry on `task` into the state (the summary's tick
/// input; the same shape the fixture's `add_open_entry` mints).
fn open_entry_on(state: &mut AppState, task: TaskId) {
    let start = chrono::Utc::now();
    let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
    state.tracking.entries.insert(
        id,
        taskboard_domain::TimeEntry {
            id,
            task,
            task_title: "open".to_owned(),
            start,
            end: None,
            created_at: start,
            updated_at: start,
            kind: taskboard_domain::tracking::TimeEntryKind::Work,
            by: None,
        },
    );
}

/// A1 — the gate's term: the mounted task view's subject holds an
/// open entry. A foreign task's open entry, no open entry, and a
/// left task view all leave it false.
#[test]
fn the_seconds_gate_holds_only_while_the_subjects_open_entry_is_mounted() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let rig = rig(state);
        let adapter = rig.adapter();
        let [a_id, b_id, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;

        // Nothing mounted, no open entry: false (the initial derive's
        // computation).
        assert!(!adapter.borrow().seconds_surfaces_active);

        // An open entry on the future subject while the task view is
        // not mounted: false.
        {
            let mut s = (*rig.handle.test_state().load_full()).clone();
            open_entry_on(&mut s, a_id);
            rig.handle.test_state().store(Arc::new(s));
            adapter.borrow_mut().refresh(rig.handle.window());
        }
        assert!(!adapter.borrow().seconds_surfaces_active);

        // The task view mounted on the subject that holds the entry:
        // true.
        {
            let mut a = adapter.borrow_mut();
            a.refresh_forced(rig.handle.window());
            a.view_state.activate_card(a_id, s0);
            a.view_state
                .activate_focused(&rig.handle.test_state().load_full());
            a.refresh_forced(rig.handle.window());
        }
        assert!(
            adapter.borrow().seconds_surfaces_active,
            "the subject's open entry arms the tick"
        );

        // No open entry on the subject: false (mount the view on the
        // entry-less sibling — Back first, then activate).
        {
            let mut a = adapter.borrow_mut();
            a.interpret_for_test(rig.handle.window(), UiEffect::FocusBoard);
            a.view_state.activate_card(b_id, s0);
            a.view_state
                .activate_focused(&rig.handle.test_state().load_full());
            a.refresh_forced(rig.handle.window());
        }
        assert!(
            !adapter.borrow().seconds_surfaces_active,
            "a foreign task's open entry never arms the tick"
        );
    });
}

/// r4 D32 — the timers term: an unfired (counting) alarm arms the
/// tick on desktop; an awaiting-only map (fired ∧ delivered) does not
/// — its text is clamped zeros plus a fixed instant, static; kiosk
/// stays false even with a live alarm (layer (b), the test-pinned
/// fence).
#[test]
fn the_seconds_gate_arms_on_unfired_timers_only() {
    on_slint_thread(|| {
        let alarm_at = chrono::Utc::now() + chrono::TimeDelta::minutes(3);
        let mint = |state: &mut AppState, label: &str, fire_at| {
            let id = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
            state.reminders.alarms.insert(
                id,
                taskboard_domain::AdhocAlarm {
                    insistence: taskboard_domain::Insistence::Normal,
                    id,
                    label: label.to_owned(),
                    fire_at,
                    created_at: fire_at - chrono::TimeDelta::minutes(1),
                },
            );
        };

        // Desktop, one unfired alarm: true.
        let (live, _fx) = fixture();
        let mut live = live;
        mint(&mut live, "tea", alarm_at);
        {
            let rig = rig(live);
            let adapter = rig.adapter();
            adapter.borrow_mut().refresh_forced(rig.handle.window());
            assert!(
                adapter.borrow().seconds_surfaces_active,
                "an unfired timer's countdown moves with the wall second"
            );
        }

        // Desktop, awaiting-only (fired ∧ delivered): false.
        let (awaiting, _fx) = fixture();
        let mut awaiting = awaiting;
        let fired_at = chrono::Utc::now() - chrono::TimeDelta::minutes(1);
        mint(&mut awaiting, "fired", fired_at);
        let id = *awaiting.reminders.alarms.keys().next().expect("alarm");
        awaiting
            .reminders
            .deliveries
            .insert(taskboard_domain::ReminderDelivery {
                reminder: taskboard_domain::ReminderRef::Adhoc(id),
                occurrence: fired_at,
                delivered_at: fired_at,
            });
        {
            let rig = rig(awaiting);
            let adapter = rig.adapter();
            adapter.borrow_mut().refresh_forced(rig.handle.window());
            assert!(
                !adapter.borrow().seconds_surfaces_active,
                "an awaiting-only map renders static text — no tick owed"
            );
        }

        // Kiosk, live alarm: false (layer (b)). Mounted kiosk-first
        // (the A2 shape) — the flag is a mount-time input.
        let (kiosk_state, _fx) = fixture();
        let mut kiosk_state = kiosk_state;
        mint(&mut kiosk_state, "tea", alarm_at);
        {
            let state = Arc::new(arc_swap::ArcSwap::from_pointee(kiosk_state));
            let (commands_tx, _commands_rx) = mpsc::channel(64);
            let (requests_tx, _requests_rx) = mpsc::channel(16);
            let (_health_tx, health) =
                tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
            let handle = mount(
                state,
                dummy_receiver(),
                dummy_receiver(),
                commands_tx,
                requests_tx,
                UiSettings {
                    kiosk: true,
                    ..UiSettings::default()
                },
                health,
                None,
            )
            .expect("mount succeeds on the null platform");
            let adapter = handle.test_adapter();
            adapter.borrow_mut().refresh_forced(handle.window());
            assert!(
                !adapter.borrow().seconds_surfaces_active,
                "kiosk never ticks, whatever the timers hold"
            );
        }
    });
}

/// A2 — the kiosk pin (layer (b)): the explicit `!self.kiosk` term
/// keeps the gate false even with an open entry and a subject
/// candidate in the view state.
#[test]
fn the_seconds_gate_stays_false_on_kiosk() {
    on_slint_thread(|| {
        let (mut state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        open_entry_on(&mut state, a_id);
        let state = Arc::new(arc_swap::ArcSwap::from_pointee(state));
        let (commands_tx, commands_rx) = mpsc::channel(64);
        let (requests_tx, _requests_rx) = mpsc::channel(16);
        let (_health_tx, health) =
            tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
        let handle = mount(
            state,
            dummy_receiver(),
            dummy_receiver(),
            commands_tx,
            requests_tx,
            UiSettings {
                kiosk: true,
                ..UiSettings::default()
            },
            health,
            None,
        )
        .expect("mount succeeds on the null platform");
        let rig = Rig {
            handle,
            commands_rx,
        };
        let adapter = rig.adapter();
        {
            let mut a = adapter.borrow_mut();
            a.refresh_forced(rig.handle.window());
            a.view_state.activate_card(a_id, s0);
            a.view_state
                .activate_focused(&rig.handle.test_state().load_full());
            a.refresh_forced(rig.handle.window());
        }
        assert!(
            !adapter.borrow().seconds_surfaces_active,
            "kiosk never ticks, whatever the view state holds"
        );
    });
}

// -- the task view's header seams (r4 D16b/D16c) ------------------------

/// Enters the task detail on `task` through the production path
/// (activate + focused + the forced derive; the #463 rule) — the
/// matrix driver's seeding, factored for the header-seam and
/// tracking journeys.
fn mount_task_detail(
    rig: &Rig,
    adapter: &std::rc::Rc<std::cell::RefCell<crate::adapter::Adapter>>,
    task: TaskId,
    stack: StackId,
) {
    let mut a = adapter.borrow_mut();
    a.refresh_forced(rig.handle.window());
    a.view_state.activate_card(task, stack);
    a.view_state
        .activate_focused(&rig.handle.test_state().load_full());
    a.refresh_forced(rig.handle.window());
}

/// The boundary commands drained as values (the `drain` count's twin
/// — the header seams' dispatch pins assert the exact commands).
fn drained_commands(rig: &mut Rig) -> Vec<StateCommand> {
    let mut out = Vec::new();
    while let Ok(command) = rig.commands_rx.try_recv() {
        out.push(command);
    }
    out
}

/// A3 — the view checkbox raw-sends the toggled `SetTaskDone` over the
/// SUBJECT, both directions, and under a mis-focused board the
/// command still names the subject (the raw-send rationale: the
/// registry's complete/reopen act on the focused task; the checkbox
/// must not).
#[test]
fn the_done_checkbox_sends_the_toggled_command_over_the_subject() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let mut rig = rig(state);
        let adapter = rig.adapter();
        let [a_id, b_id, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        mount_task_detail(&rig, &adapter, a_id, s0);

        // Open subject: exactly one command, done: true, naming the
        // subject.
        rig.handle.window().invoke_task_done_toggled();
        let drained = drained_commands(&mut rig);
        assert!(
            matches!(
                drained.as_slice(),
                [StateCommand::SetTaskDone { id, done: true }] if *id == a_id
            ),
            "the open subject's checkbox toggles done, got {drained:?}"
        );

        // Done subject: the inverse command. The checkbox holds no
        // local state — the engine-side done fact decides.
        {
            let mut s = (*rig.handle.test_state().load_full()).clone();
            let t = s.tasks.get_mut(&a_id).expect("task");
            t.done = Some(chrono::Utc::now());
            rig.handle.test_state().store(Arc::new(s));
            adapter.borrow_mut().refresh_forced(rig.handle.window());
        }
        rig.handle.window().invoke_task_done_toggled();
        let drained = drained_commands(&mut rig);
        assert!(
            matches!(
                drained.as_slice(),
                [StateCommand::SetTaskDone { id, done: false }] if *id == a_id
            ),
            "the done subject's checkbox toggles undone, got {drained:?}"
        );

        // Subject-addressing pin (D-TU5's shape): focus on ANOTHER
        // card while the subject stays — the command must still name
        // the subject, never the focused task.
        {
            let mut a = adapter.borrow_mut();
            a.view_state.activate_card(b_id, s0);
            a.refresh_forced(rig.handle.window());
        }
        rig.handle.window().invoke_task_done_toggled();
        let drained = drained_commands(&mut rig);
        assert!(
            matches!(
                drained.as_slice(),
                [StateCommand::SetTaskDone { id, .. }] if *id == a_id
            ),
            "the checkbox addresses the subject, not the focus, got {drained:?}"
        );
    });
}

/// A4 — the moved header tracking buttons route their exact old
/// commands over the subject (the move is markup-only; the seams
/// follow the button).
#[test]
fn the_header_tracking_buttons_route_their_registry_commands() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let mut rig = rig(state);
        let adapter = rig.adapter();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        mount_task_detail(&rig, &adapter, a_id, s0);

        rig.handle.window().invoke_task_track_start_clicked();
        let drained = drained_commands(&mut rig);
        assert!(
            matches!(
                drained.as_slice(),
                [StateCommand::StartTracking { task, .. }] if *task == a_id
            ),
            "Start routes StartTracking over the subject, got {drained:?}"
        );

        rig.handle.window().invoke_task_track_stop_clicked();
        let drained = drained_commands(&mut rig);
        assert!(
            matches!(drained.as_slice(), [StateCommand::StopTracking]),
            "Stop routes StopTracking, got {drained:?}"
        );
    });
}

/// The detail shortcut hints' projection law (J3 US-4 W-4 tail,
/// round 9 chunk 11): the entries opener's and effort row's hints are
/// keymap projections pushed per refresh — non-empty exactly where
/// the mounted preset binds the verb in the detail chain (the D19
/// never-hand-written rule; label content is never asserted), and
/// the emacs board-return hint is non-empty (the gap fix — emacs
/// gained its primary `NavFocusBoard` row in this chunk). The track
/// hint's deliberate Start/Stop (state-correct verb) choice is the
/// plan §7 R-5 ruling, not a coverage miss.
#[test]
fn the_detail_shortcut_hints_project_the_preset_rows() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;

        // Standard: entries (F12) and effort (Ctrl+e) bind at the
        // detail — both projections carry rows.
        let rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        let window = rig.handle.window();
        assert!(
            window.get_task_entries_hint().row_count() > 0,
            "the entries opener's hint projects the bound stroke"
        );
        assert!(
            window.get_task_effort_hint().row_count() > 0,
            "the effort row's hint projects the bound stroke"
        );
        drop(rig);

        // Emacs: the board-return hint is non-empty (the gap fix —
        // this preset bound NO NavFocusBoard row before chunk 11).
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let rig = emacs_map_rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        assert!(
            rig.handle.window().get_nav_back_hint().row_count() > 0,
            "emacs' board-return hint is non-empty (the chunk-11 gap fix)"
        );
        drop(rig);

        // The empty map binds nothing: both new projections are
        // empty (the structural binds-nothing case).
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let rig = empty_map_rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        assert_eq!(
            rig.handle.window().get_task_entries_hint().row_count(),
            0,
            "nothing bound: the entries projection is empty"
        );
        assert_eq!(
            rig.handle.window().get_task_effort_hint().row_count(),
            0,
            "nothing bound: the effort projection is empty"
        );
    });
}

/// J4's harness-level pin — the header hint projections: empty while
/// the task view is mounted under a keymap that binds neither verb in
/// the TaskDetail-visible chain. The keymap-parity round (US-3) gave
/// the tracking verbs preset rows in *every* preset's detail chain
/// (the TE-W-N2 menu-only note retires), so the empty leg mounts the
/// empty map (the structural "binds nothing" case) rather than a
/// shipped preset; the bound leg is unchanged — vi carries
/// `task.complete` in `ViNormal` (chain-visible at `TaskDetail`) and
/// the `task.track-start` override is an add-new, landing in Global.
/// Label content is never asserted.
#[test]
fn the_header_hint_projections_follow_the_mounted_verb() {
    on_slint_thread(|| {
        // Mounted, unbound verbs: both projections empty (the
        // established empty-hint rendering). Every preset now binds
        // the done verb's rows in the detail chain (keymap-parity
        // US-3; round 9 chunk 11 added the Standard `Space` /
        // emacs `C-c C-t` detail rows), so the done hint's empty leg
        // mounts the empty map too — the structural binds-nothing
        // case, like the tracking verbs' leg below.
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let rig = empty_map_rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        assert_eq!(rig.handle.window().get_task_done_hint().row_count(), 0);

        // The track hint's empty leg: the empty map binds nothing.
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let rig = empty_map_rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        assert_eq!(
            rig.handle.window().get_task_track_hint().row_count(),
            0,
            "nothing bound: the projection is empty"
        );
    });

    on_slint_thread(|| {
        // Bound verbs: both projections non-empty. The vi preset
        // binds the done verb in ViNormal, chain-visible at the task
        // detail; the track override is an add-new (Global).
        let raw = crate::keymap::RawKeymap {
            preset: crate::keymap::Preset::Vi,
            overrides: BTreeMap::from([(
                "task.track-start".to_owned(),
                vec![vec!["Ctrl+Shift+t".to_owned()]],
            )]),
        };
        let built = crate::keymap::build(&raw);
        assert!(
            built.findings.is_empty(),
            "the synthetic binding survives validation"
        );
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let state = Arc::new(arc_swap::ArcSwap::from_pointee(state));
        let (commands_tx, commands_rx) = mpsc::channel(64);
        let (requests_tx, _requests_rx) = mpsc::channel(16);
        let (_health_tx, health) =
            tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
        let handle = mount(
            state,
            dummy_receiver(),
            dummy_receiver(),
            commands_tx,
            requests_tx,
            UiSettings {
                keymap: built.keymap,
                keymap_preset: crate::keymap::Preset::Vi,
                ..UiSettings::default()
            },
            health,
            None,
        )
        .expect("mount succeeds on the null platform");
        let rig = Rig {
            handle,
            commands_rx,
        };
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        assert!(
            rig.handle.window().get_task_done_hint().row_count() > 0,
            "vi binds the done verb in the TaskDetail-visible chain"
        );
        assert!(
            rig.handle.window().get_task_track_hint().row_count() > 0,
            "the override binds the tracking verb (add-new lands in Global)"
        );
    });
}

/// T4 — the guarded skip's cheap tail still pushes the settings
/// liveness: a connector episode flips the row's status enum without a
/// derive pass.
#[test]
fn an_unchanged_refresh_still_pushes_the_settings_liveness() {
    on_slint_thread(|| {
        let rig = guard_rig(
            AppState::default(),
            UiSettings {
                connectors: vec![crate::requests::ConnectorInfo {
                    name: "nextcloud".into(),
                    server_url: "https://c.example".into(),
                    username: "alice".into(),
                    backend: crate::requests::ConnectorBackend::Nextcloud,
                }],
                ..UiSettings::default()
            },
        );
        let row_status = |rig: &Rig| {
            rig.handle
                .window()
                .global::<crate::SettingsModel>()
                .get_rows()
                .row_data(0)
                .expect("the connector row is pushed")
                .status
        };
        assert_eq!(row_status(&rig), crate::SettingsRowStatus::Idle);
        assert_eq!(rig.adapter().borrow().derive_passes, 1);
        rig.handle
            .observe_connector_episode(&taskboard_domain::SystemEvent::ConnectorLost(
                taskboard_domain::ConnectorId::from("nextcloud"),
            ));
        assert_eq!(row_status(&rig), crate::SettingsRowStatus::Down);
        // The liveness delivery provably did not need the derive.
        assert_eq!(rig.adapter().borrow().derive_passes, 1);
    });
}

// -- the total-dismiss walk (r4 chunk 4, D14) ----------------------------
//
// One test, two exhaustive lists — compile-enforced totality: the
// `dismiss_class` match ranges over every `UiEffect` variant with no
// wildcard (a new variant is a compile error until it is placed), and
// the behavioural walk drives every listed effect through the real
// `apply_effect` pass with an armed (pending) tooltip. Identity- or
// anchor/modal-class effects must dismiss the tooltip; the explicit
// survive set — the over-fire guard — must leave it armed. A
// listed-survive effect that trips the identity diff fails here:
// either the effect genuinely unmounts the anchor (move it to a
// dismissal list — the AGENTS §5 "a new test failing can be a bug"
// rule) or the identity tuple carries a spurious member.
//
// Recorded exclusion (the plan §3.3): a task deleted by a sync/engine
// event unmounts an anchor through refresh/reconcile, not through an
// effect — out of D14's scope; the next pointer movement re-arms or
// dismisses naturally.

/// The three dismiss classes of the walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DismissClass {
    /// The application swaps the mounted view, board, or subject: the
    /// identity diff at the `apply_effect` choke point dismisses.
    IdentitySwap,
    /// The hovered anchor itself unmounts, or a modal context takes
    /// over, without the identity moving: an explicit per-arm dismiss
    /// (one mechanism, a named non-identity reason).
    AnchorOrModal,
    /// The explicit survive set — identity-preserving effects; the
    /// choke point must not over-fire on them.
    Survive,
}

/// The class of every effect. Exhaustive, no wildcard: adding a
/// variant without a placement is a compile error.
#[allow(clippy::too_many_lines)] // one placement table, kept flat
fn dismiss_class(effect: &UiEffect) -> DismissClass {
    match effect {
        UiEffect::NextView
        | UiEffect::PrevView
        | UiEffect::FocusBoard
        | UiEffect::FocusToday
        | UiEffect::OpenInbox
        | UiEffect::BoardNext
        | UiEffect::BoardPrevious
        | UiEffect::BoardSelect(_)
        | UiEffect::Activate
        | UiEffect::OpenSettings
        | UiEffect::OpenStatusPanel
        // The plan-next-day toggle (#622): it swaps the mounted
        // surface like the settings opener.
        | UiEffect::OpenPlanNextDay
        | UiEffect::ShowTask
        | UiEffect::InvokeSelected
        // The notification verbs: the menu-family placement — the
        // walk/verb arm mutates no mounted surface beyond the
        // surface's own highlight (identity-preserving).
        | UiEffect::NotificationVerb(_) => DismissClass::IdentitySwap,
        UiEffect::OpenMenu
        | UiEffect::UndoBranchesOpen
        // The row-menu opener mounts the modal menu too (US-7, W-8,
        // #1032) — the same anchor-or-modal class.
        | UiEffect::RowMenuRequested
        | UiEffect::CloseMenu
        | UiEffect::BeginCreate { .. }
        | UiEffect::BeginCreateStack
        | UiEffect::OpenPalette
        // The entries overlay is a modal takeover (r2 D13's rule
        // sentence) — its open is the AnchorOrModal class like the
        // palette's; the dismissal itself is identity-preserving.
        | UiEffect::TrackEntriesOpen
            // The labels overlay is a modal takeover (UX R-3 D-2,
            // #789) — the entries overlay's class.
            | UiEffect::LabelsOpen
            // The recur row's intents (issue #506) and HF1's pause
            // editor opener: view intents — no modal takeover, the
            // flags ride the next pass.
            | UiEffect::SeriesPauseOpen
            | UiEffect::HabitPauseOpen
            | UiEffect::HabitConvertArm
            | UiEffect::HabitRemoveArm
            // The H4 editor opener: the settle-then-open view intent
            // (the Nav family's class) — no modal takeover beyond the
            // settle.
            | UiEffect::HabitEdit
            | UiEffect::HabitHistoryOpen
            | UiEffect::HabitCreateToggle
            | UiEffect::HabitQuotaEdit
            | UiEffect::SeriesConvertArm
            // The recur editor's opener (round-7 D-7.1): a modal
            // takeover like the entries overlay's; the detach flips
            // are view intents (the flags ride the next pass).
            | UiEffect::SeriesEditorOpen
            | UiEffect::SeriesDetachArm
            | UiEffect::SeriesDetachCancel
            // The priority pickers' opener (UX R-3 D-1, #788): a
            // view intent — no modal takeover, the flag rides the
            // next pass.
            | UiEffect::PriorityPickerOpen(_)
        | UiEffect::BeginCapture
        | UiEffect::ToggleFocusMode
        | UiEffect::TrackEntryDismiss
        // The labels ladder closes the overlay (UX R-3 D-2): the
        // entries ladder's class.
        | UiEffect::LabelsDismiss
        // The export dialog (the J2 export plan W-1): the open is a
        // modal takeover, the close the modal's own exit arm.
        | UiEffect::ExportDialogOpen
        | UiEffect::ExportDialogClose => DismissClass::AnchorOrModal,
        UiEffect::Exit
        | UiEffect::MenuBack
        | UiEffect::MenuNext
        | UiEffect::MenuPrevious
        | UiEffect::SetMode(_)
        | UiEffect::FocusNext
        | UiEffect::FocusPrev
        // The directional-focus family (J3 US-1) and its extend twin
        // (#875): pure focus/selection steps — no anchor moves (the
        // walk family's Survive class).
        | UiEffect::FocusDirectional(_)
        | UiEffect::ExtendSelectionNext
        | UiEffect::ExtendSelectionPrev
        | UiEffect::FocusFirst
        | UiEffect::FocusLast
        | UiEffect::ControlNext
        | UiEffect::ControlPrev
        | UiEffect::ScrollUp
        | UiEffect::ScrollDown
        | UiEffect::StackNext
        | UiEffect::StackPrev
        | UiEffect::StackWider
        | UiEffect::StackNarrower
        | UiEffect::RequestMove
        | UiEffect::CancelMove
        | UiEffect::BeginTitleEdit
        // The keymap-parity D-4 editor arms: the flow arms inside the
        // mounted detail — no anchor moves.
        | UiEffect::BeginDateEdit { .. }
        | UiEffect::BeginDescriptionEdit
        // The effort field's focus opener (J3 US-4, chunk 11): same
        // editor-arm shape — flow arms inside the mounted detail, no
        // anchor moves.
        | UiEffect::BeginEffortEdit
        | UiEffect::PickerKey(_)
        // The plan-next-day row verbs (#622): they mutate the model
        // in place, no anchor moves.
        | UiEffect::PlanBeginPlace(_)
        | UiEffect::PlanCommitPlace
        | UiEffect::PlanBoxEarlier(_)
        | UiEffect::PlanBoxLater(_)
        | UiEffect::PlanRemoveBox(_)
        // The review walk's intents (#638): the overlay mutates in
        // place; the opener's mount (commit 6) swaps the view — it
        // rides the identity diff when it lands.
        | UiEffect::OpenReviewWalk
        | UiEffect::ReviewWalk(_)
        | UiEffect::StackDelete
        | UiEffect::StackRename
        | UiEffect::StackMove
        | UiEffect::BoardCreate
        | UiEffect::BoardDelete
        | UiEffect::BoardRename
        | UiEffect::BoardColorEdit
        | UiEffect::BoardDefaultPreset
        | UiEffect::BoardArchive
        | UiEffect::BoardReset
        | UiEffect::BoardReconcile
        | UiEffect::HabitConvertCancel
        | UiEffect::HabitRemoveCancel
        | UiEffect::OpenBoardBarFlyout => DismissClass::Survive,
    }
}

/// The walk fixture: the base fixture plus a second board (carrying
/// one task, the palette jumps' target) and an `Inbox` board (the
/// `OpenInbox` row).
fn walk_fixture() -> (AppState, Fixture, BoardId, TaskId) {
    let (mut state, fx) = fixture();
    let second = BoardId::from_uuid(uuid::Uuid::now_v7());
    let now = BoardClocks::epoch();
    state.boards.insert(
        second,
        Board {
            id: second,
            remote: None,
            title: "second".to_owned(),
            color: Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: now,
                color: now,
                archived: now,
                deleted: now,
            },
            authors: taskboard_domain::entities::BoardAuthors::default(),
            remote_seen: None,
        },
    );
    let inbox = BoardId::from_uuid(uuid::Uuid::now_v7());
    state.boards.insert(
        inbox,
        Board {
            id: inbox,
            remote: None,
            title: "Inbox".to_owned(),
            color: Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: now,
                color: now,
                archived: now,
                deleted: now,
            },
            authors: taskboard_domain::entities::BoardAuthors::default(),
            remote_seen: None,
        },
    );
    // A stack and one task on the second board (the palette jump's
    // cross-board retarget).
    let stack = StackId::from_uuid(uuid::Uuid::now_v7());
    state.stacks.insert(
        stack,
        Stack {
            id: stack,
            remote: None,
            board: second,
            title: "second-stack".to_owned(),
            order: 0,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: now,
                order: now,
                deleted: now,
            },
            authors: taskboard_domain::entities::StackAuthors::default(),
            remote_seen: None,
        },
    );
    let task = TaskId::from_uuid(uuid::Uuid::now_v7());
    let mut template = state.tasks.values().next().expect("fixture task").clone();
    template.id = task;
    template.stack = stack;
    template.title = "second-task".to_owned();
    state.tasks.insert(task, template);
    (state, fx, second, task)
}

/// The walk's effect list: every variant exactly once per class
/// (payloads chosen so the effect's precondition can be met). The
/// totality tie: `dismiss_class` is compile-exhaustive, and the test
/// asserts the list's per-variant classes match their list membership.
fn walk_effects(fx: &Fixture, second: BoardId) -> Vec<UiEffect> {
    vec![
        // IdentitySwap
        UiEffect::NextView,
        UiEffect::PrevView,
        UiEffect::FocusBoard,
        UiEffect::FocusToday,
        UiEffect::OpenInbox,
        UiEffect::BoardNext,
        UiEffect::BoardPrevious,
        UiEffect::BoardSelect(second),
        UiEffect::Activate,
        UiEffect::OpenSettings,
        UiEffect::OpenStatusPanel,
        UiEffect::ShowTask,
        UiEffect::InvokeSelected,
        // AnchorOrModal
        UiEffect::OpenMenu,
        UiEffect::CloseMenu,
        UiEffect::BeginCreate { target: None },
        UiEffect::BeginCreate {
            target: Some(fx.stacks[1]),
        },
        UiEffect::BeginCreateStack,
        UiEffect::OpenPalette,
        UiEffect::BeginCapture,
        // Survive
        UiEffect::Exit,
        UiEffect::MenuBack,
        UiEffect::MenuNext,
        UiEffect::MenuPrevious,
        UiEffect::SetMode(Mode::Normal),
        UiEffect::SetMode(Mode::Insert),
        UiEffect::FocusNext,
        UiEffect::FocusPrev,
        UiEffect::FocusFirst,
        UiEffect::FocusLast,
        UiEffect::ControlNext,
        UiEffect::ControlPrev,
        UiEffect::ScrollUp,
        UiEffect::ScrollDown,
        UiEffect::StackNext,
        UiEffect::StackPrev,
        UiEffect::StackWider,
        UiEffect::StackNarrower,
        UiEffect::RequestMove,
        UiEffect::CancelMove,
        UiEffect::BeginTitleEdit,
        UiEffect::PickerKey(crate::due_edit::DatePickerKey::Left),
        UiEffect::StackDelete,
        UiEffect::StackRename,
        UiEffect::StackMove,
        UiEffect::BoardCreate,
        UiEffect::BoardDelete,
        UiEffect::BoardRename,
    ]
}

/// The arm: reset, reconcile, seed the Board view with a focused
/// card, sync the view seam (the walk's traversal reads it), and arm
/// a pending tooltip (the `hover()` arm — the delay never fires on
/// the null platform's clock, `pending` is the observable).
fn arm_walk_pass(adapter: &Rc<RefCell<crate::adapter::Adapter>>, rig: &Rig, fx: &Fixture) {
    let mut a = adapter.borrow_mut();
    a.test_reset();
    let live = rig.handle.test_state();
    a.view_state.reconcile(&live.load_full());
    let [s0, _s1] = fx.stacks;
    let [a_id, _b, _c] = fx.tasks;
    a.view_state.activate_card(a_id, s0);
    a.view_state.select_board(&live.load_full(), fx.board);
    a.shell.view = Some(crate::actions::ContextId::Board);
    rig.handle
        .window()
        .global::<crate::TooltipState>()
        .invoke_hover(
            "walk".into(),
            "".into(),
            slint::ModelRc::default(),
            100.0,
            100.0,
            32.0,
            32.0,
        );
    drop(a);
    let armed = rig
        .handle
        .window()
        .global::<crate::TooltipState>()
        .get_pending();
    assert!(armed, "the walk's tooltip armed");
}

/// The plan's per-effect preconditions: the palette flows arm with a
/// resolved cross-board target, `FocusBoard` seeds the Today view (the
/// Board→Board jump is the identity's no-op).
fn seed_effect_precondition(
    adapter: &Rc<RefCell<crate::adapter::Adapter>>,
    rig: &Rig,
    fx: &Fixture,
    second: BoardId,
    second_task: TaskId,
    effect: &UiEffect,
) {
    let mut a = adapter.borrow_mut();
    let live = rig.handle.test_state();
    match effect {
        UiEffect::ShowTask | UiEffect::InvokeSelected => {
            a.view_state.arm_flow(Flow::Palette { selected: 0 });
            a.test_seed_palette(vec![taskboard_domain::SearchTarget::Task(second_task)]);
            a.shell.palette = true;
        }
        // The board steps don't wrap (the ends hold): stepping needs
        // a neighbour — FocusBoard seeds off-Board, BoardPrevious
        // seeds off the first canonical board (the walk fixture's
        // `fx.board` is first).
        UiEffect::FocusBoard => {
            a.view_state.focus_today();
        }
        UiEffect::BoardPrevious => {
            a.view_state.select_board(&live.load_full(), second);
        }
        _ => {}
    }
    drop(a);
    let _ = fx;
}

#[test]
#[allow(clippy::too_many_lines)] // one driver over the whole partition
fn every_view_swapping_effect_dismisses_the_tooltip_and_every_preserving_effect_spares_it() {
    on_slint_thread(|| {
        let (state, fx, second, second_task) = walk_fixture();
        let rig = rig(state);
        let adapter = rig.adapter();
        let effects = walk_effects(&fx, second);
        // The totality tie: every listed row's class agrees with its
        // list section (the list is ordered IdentitySwap, AnchorOrModal,
        // Survive), and the section sizes are exact — a variant added
        // to the enum fails the `dismiss_class` match; a variant
        // added to a list without a matching placement (or left out)
        // fails here.
        // The status panel's toggle (OP3 W-1) joined the IdentitySwap
        // section (the settings opener's row — the mounted-view swap).
        let sections = [
            (0, 13, DismissClass::IdentitySwap),
            (13, 20, DismissClass::AnchorOrModal),
            (20, effects.len(), DismissClass::Survive),
        ];
        for (start, end, class) in sections {
            for effect in &effects[start..end] {
                assert_eq!(
                    dismiss_class(effect),
                    class,
                    "{effect:?} listed outside its placement's section"
                );
            }
        }
        let mut failures = Vec::new();
        for effect in &effects {
            arm_walk_pass(&adapter, &rig, &fx);
            seed_effect_precondition(&adapter, &rig, &fx, second, second_task, effect);
            let expected_dismiss = dismiss_class(effect) != DismissClass::Survive;
            adapter
                .borrow_mut()
                .apply_effect_for_test(rig.handle.window(), *effect);
            let pending = rig
                .handle
                .window()
                .global::<crate::TooltipState>()
                .get_pending();
            let ok = if expected_dismiss { !pending } else { pending };
            if !ok {
                failures.push(format!(
                    "{effect:?}: expected {} — tooltip {}",
                    if expected_dismiss {
                        "dismissed"
                    } else {
                        "armed"
                    },
                    if pending {
                        "still pending"
                    } else {
                        "dismissed"
                    }
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} walk rows off-partition:\n{}",
            failures.len(),
            effects.len(),
            failures.join("\n")
        );
    });
}

// -- the first-frame hint projection (r4 chunk 4, D15/E1) ----------------

/// The Tier-4 harness (the F4 investigation's E1): at first frame the
/// seven chrome-tooltip hint models equal the keymap projection
/// `first_binding_display(action, mount_chain)` computed independently
/// from the preset — "hints present at start" at the seam level. Any
/// diffing row would be H-a (chain masking at mount) named here; the
/// empty projection is H-b's by-design record (the E2 enumeration
/// pins it per preset). No text assertions on *content* — the models
/// are compared structurally against the projection's own output.
#[test]
fn first_frame_hints_match_the_keymap_projection() {
    use slint::{Model as _, SharedString};
    let (state, _fx) = fixture();
    // The mount touches the Slint thread's testing platform (the walk's
    // rule): the backend exists only on that thread, headless CI
    // included.
    on_slint_thread(move || {
        let rig = rig(state);
        let adapter = rig.adapter();
        let a = adapter.borrow();
        // The chain the first-frame push projected with (the mount's own
        // view context head — desktop default).
        let chain = a.shell.chain();
        let keymap = crate::keymap::Keymap::from_preset(crate::keymap::Preset::Standard);
        let projection = |action: crate::actions::ActionId| -> Vec<SharedString> {
            keymap
                .first_binding_display(action, &chain)
                .unwrap_or_default()
                .iter()
                .map(|stroke| SharedString::from(stroke.as_str()))
                .collect()
        };
        let read = |model: &slint::ModelRc<SharedString>| -> Vec<SharedString> {
            (0..model.row_count())
                .map(|row| model.row_data(row).expect("hint row"))
                .collect()
        };
        let window = rig.handle.window();
        let rows = [
            (
                window.get_toolbar_menu_hint(),
                projection(crate::actions::ActionId::AppMenu),
            ),
            (
                window.get_nav_back_hint(),
                projection(crate::actions::ActionId::NavFocusBoard),
            ),
            (
                window.get_board_segment_hint(),
                projection(crate::actions::ActionId::BoardSelect),
            ),
            (
                window.get_task_card_hint(),
                projection(crate::actions::ActionId::TaskEdit),
            ),
            (
                window.get_stack_header_hint(),
                projection(crate::actions::ActionId::StackWider),
            ),
            (
                window.get_stack_create_hint(),
                projection(crate::actions::ActionId::StackCreate),
            ),
            (
                window.get_task_create_hint(),
                projection(crate::actions::ActionId::TaskCreate),
            ),
        ];
        for (index, (model, expected)) in rows.iter().enumerate() {
            assert_eq!(
                read(model),
                *expected,
                "hint model {index} differs from its keymap projection at first frame"
            );
        }
    });
}

// -- the concentration overlay (#496): journeys and pins ----------------

use slint::SharedString;
use taskboard_domain::focus::FocusTiming;

/// The overlay rigs' timing: the 25/5 pomodoro idiom.
fn focus_timing() -> FocusTiming {
    FocusTiming::try_new(
        chrono::TimeDelta::minutes(25),
        chrono::TimeDelta::minutes(5),
        chrono::TimeDelta::minutes(15),
        4,
    )
    .expect("positive timings")
}

/// A desktop rig carrying the focus timings (the overlay's
/// precondition) and one open entry on task A (a live session).
fn focus_rig(mut state: AppState, fx: &Fixture, kiosk: bool) -> Rig {
    open_entry_on(&mut state, fx.tasks[0]);
    // The timings ride the published settings (round-5 chunk 5), not
    // the hand-over.
    state.settings.focus_enabled = true;
    state.settings.focus = Some(focus_timing());
    let state = Arc::new(arc_swap::ArcSwap::from_pointee(state));
    let (commands_tx, commands_rx) = mpsc::channel(64);
    let (requests_tx, _requests_rx) = mpsc::channel(16);
    let (_health_tx, health) =
        tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
    let handle = mount(
        state,
        dummy_receiver(),
        dummy_receiver(),
        commands_tx,
        requests_tx,
        UiSettings {
            kiosk,
            ..UiSettings::default()
        },
        health,
        None,
    )
    .expect("mount succeeds on the null platform");
    Rig {
        handle,
        commands_rx,
    }
}

/// The seconds gate's mandated extension (#496, D-F12): the overlay's
/// countdown changes with the wall second while the flow holds — THIS
/// gate is the single tick channel; closed overlay, no per-second
/// cost.
#[test]
fn the_seconds_gate_covers_the_focus_overlay() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let rig = focus_rig(state, &fx, false);
        let adapter = rig.adapter();
        assert!(!adapter.borrow().seconds_surfaces_active);

        adapter.borrow_mut().view_state.open_focus_mode();
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert!(
            adapter.borrow().seconds_surfaces_active,
            "the open overlay's countdown arms the per-second tick"
        );

        let mut a = adapter.borrow_mut();
        a.view_state.close_focus_mode();
        a.refresh_forced(rig.handle.window());
        assert!(
            !a.seconds_surfaces_active,
            "the closed overlay costs nothing per second"
        );
    });
}

/// The toggle journey: open, close, and the drift-story pin — closing
/// the overlay never touches the published tracking state (the
/// session is the open entry; the overlay is presentation).
#[test]
fn the_overlay_toggle_opens_closes_and_never_touches_the_session() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let rig = focus_rig(state, &fx, false);
        let adapter = rig.adapter();
        adapter.borrow_mut().test_reset();

        let window = rig.handle.window();
        assert!(!window.get_focus_overlay_open());
        let before = rig.handle.test_state().load_full().tracking.entries.len();

        adapter
            .borrow_mut()
            .interpret_for_test(window, UiEffect::ToggleFocusMode);
        assert!(window.get_focus_overlay_open(), "the overlay opened");
        assert!(
            rig.handle.test_state().load_full().tracking.entries.len() == before,
            "opening touches no tracking state"
        );

        adapter
            .borrow_mut()
            .interpret_for_test(window, UiEffect::ToggleFocusMode);
        assert!(!window.get_focus_overlay_open(), "the second toggle closes");
        assert!(
            rig.handle.test_state().load_full().tracking.entries.len() == before,
            "closing never stops the session (the drift-story pin)"
        );
    });
}

/// The kiosk exclusion's effect layer (D-F13, layer 3): the flow is
/// structurally unreachable even if a future surface bypassed the
/// entry-states gate (layer 2).
#[test]
fn the_focus_overlay_never_opens_on_kiosk() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let rig = focus_rig(state, &fx, true);
        let adapter = rig.adapter();
        adapter.borrow_mut().test_reset();
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::ToggleFocusMode);
        assert!(
            !rig.handle.window().get_focus_overlay_open(),
            "kiosk dispatches are absorbed"
        );
    });
}

/// The entry-states gate (D-F13, layer 2) plus the mode-off posture:
/// the action lights only on a desktop with the timings configured.
#[test]
fn the_focus_action_gates_on_kiosk_and_on_the_disabled_mode() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let states_of = |rig: &Rig| {
            let adapter = rig.adapter();
            let a = adapter.borrow();
            let live = rig.handle.test_state();
            let flow = a.view_state.flow().clone();
            a.entry_states(&live.load_full(), &flow)
        };

        // Desktop + configured: on.
        let rig = focus_rig(state.clone(), &fx, false);
        assert!(states_of(&rig)[&crate::actions::ActionId::FocusModeToggle]);
        drop(rig);

        // Kiosk + configured: off (layer 2).
        let rig = focus_rig(state.clone(), &fx, true);
        assert!(!states_of(&rig)[&crate::actions::ActionId::FocusModeToggle]);
        drop(rig);

        // Desktop + mode off: off (no timing input, no overlay).
        let (state, _fx) = fixture();
        let (commands_tx, commands_rx) = mpsc::channel(64);
        let (requests_tx, _requests_rx) = mpsc::channel(16);
        let (_health_tx, health) =
            tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
        let handle = mount(
            Arc::new(arc_swap::ArcSwap::from_pointee(state)),
            dummy_receiver(),
            dummy_receiver(),
            commands_tx,
            requests_tx,
            UiSettings::default(),
            health,
            None,
        )
        .expect("mount succeeds on the null platform");
        let rig = Rig {
            handle,
            commands_rx,
        };
        assert!(!states_of(&rig)[&crate::actions::ActionId::FocusModeToggle]);
    });
}

/// The quick-note journey: the commit emits the landed inbox-capture
/// command at the forwarder boundary, clears the buffer, and keeps the
/// overlay open; an empty commit emits nothing.
#[test]
fn the_quick_note_commits_into_the_inbox_and_stays_open() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let mut rig = focus_rig(state, &fx, false);
        let adapter = rig.adapter();
        adapter.borrow_mut().test_reset();
        let window = rig.handle.window();
        adapter
            .borrow_mut()
            .interpret_for_test(window, UiEffect::ToggleFocusMode);
        assert!(window.get_focus_overlay_open());

        // An empty commit emits nothing.
        window.invoke_focus_note_accepted();
        assert!(
            rig.commands_rx.try_recv().is_err(),
            "an empty note commits nothing"
        );

        // A non-empty commit: the capture command, the buffer cleared,
        // the overlay still open.
        window.invoke_focus_note_edited(SharedString::from("remember the milk"));
        window.invoke_focus_note_accepted();
        let command = rig
            .commands_rx
            .try_recv()
            .expect("the capture rode the boundary");
        assert_eq!(
            command,
            taskboard_domain::StateCommand::CaptureTask {
                title: "remember the milk".into(),
                board: None,
            },
            "the landed inbox-capture verb, never a new one"
        );
        assert!(window.get_focus_overlay_open(), "rapid-fire: stays open");
        assert!(
            adapter.borrow().view_state.buffer().is_empty(),
            "the buffer cleared"
        );
    });
}

/// The switcher's start routes the landed `task.track-start` with the
/// selected row's task (the `track_target` seam); the stop button
/// routes the payload-free landed stop.
#[test]
fn the_switcher_start_and_the_stop_route_the_tracked_verbs() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let mut rig = focus_rig(state, &fx, false);
        let adapter = rig.adapter();
        adapter.borrow_mut().test_reset();
        let window = rig.handle.window();
        adapter
            .borrow_mut()
            .interpret_for_test(window, UiEffect::ToggleFocusMode);

        // Start on the first switcher row: StartTracking for THAT task
        // (the engine's single-global intent closes the open entry).
        window.invoke_focus_switcher_invoked(0);
        let command = rig
            .commands_rx
            .try_recv()
            .expect("the start rode the boundary");
        let taskboard_domain::StateCommand::StartTracking { task, kind } = command else {
            panic!("expected StartTracking, got {command:?}")
        };
        let expected =
            crate::presentation::focus_switcher_rows(&rig.handle.test_state().load_full())[0].task;
        assert_eq!(task, expected, "the selected row's task");
        assert_eq!(
            kind,
            taskboard_domain::tracking::TimeEntryKind::Work,
            "a focus target is a work block"
        );

        // Stop: the payload-free landed stop.
        window.invoke_focus_stop_clicked();
        assert_eq!(
            rig.commands_rx
                .try_recv()
                .expect("the stop rode the boundary"),
            taskboard_domain::StateCommand::StopTracking
        );
    });
}

/// The dumb-client line at the push: the overlay's derived frame
/// (phase text, countdown, task title snapshot) follows the published
/// state per pass; the visibility follows the flow AND the mode gate
/// (focus `None` forces the overlay closed).
#[test]
fn the_overlay_push_follows_the_published_state_and_the_mode_gate() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let rig = focus_rig(state, &fx, false);
        let adapter = rig.adapter();
        adapter.borrow_mut().test_reset();
        let window = rig.handle.window();
        adapter
            .borrow_mut()
            .interpret_for_test(window, UiEffect::ToggleFocusMode);
        adapter.borrow_mut().refresh_forced(window);
        assert!(window.get_focus_overlay_open());
        assert_eq!(window.get_focus_phase_text(), SharedString::from("Focus"));
        assert_eq!(window.get_focus_task_title(), SharedString::from("open"));
        // The countdown is mm:ss-shaped and reflects the work block's
        // length (the entry just opened; the wall-second bucket is
        // structural, never a fixed wall-clock pin).
        let remaining = window.get_focus_remaining_text().to_string();
        let (mins, secs) = remaining.split_once(':').expect("mm:ss shape");
        let mins: u32 = mins.parse().expect("numeric minutes");
        let secs: u32 = secs.parse().expect("numeric seconds");
        assert!(
            mins <= 25 && secs < 60,
            "the work block's countdown: {remaining}"
        );

        // Closing the flow hides the overlay on the next pass.
        let mut a = adapter.borrow_mut();
        a.view_state.close_focus_mode();
        a.refresh_forced(window);
        assert!(!window.get_focus_overlay_open());
    });
}

// -- the tracking surfaces' commit journeys (r4 chunk 10, D-TS4) -----

/// Mints one CLOSED one-hour entry on `task` at the absolute instant
/// `start` (`[start, start + 1 h)`), no clock read — the fixed-clock
/// sibling of `closed_entry_on` for tests whose render pass is
/// pinned through `UiSettings::clock`.
fn closed_entry_at(
    state: &mut AppState,
    task: TaskId,
    start: chrono::DateTime<chrono::Utc>,
) -> taskboard_domain::TimeEntryId {
    let end = start + chrono::TimeDelta::hours(1);
    let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
    state.tracking.entries.insert(
        id,
        taskboard_domain::TimeEntry {
            id,
            task,
            task_title: "closed".to_owned(),
            start,
            end: Some(end),
            created_at: start,
            updated_at: start,
            kind: taskboard_domain::tracking::TimeEntryKind::Work,
            by: None,
        },
    );
    id
}

/// Mints one CLOSED entry with exact local instants on `task`, at
/// `hour` and `hour + 1` on the *given* local date (the fixed-zone
/// trick: the test asserts exact instants, so the seed and the
/// expectation derive from the same local wall-clock — one capture
/// the test owns, per D-3/R3).
fn closed_entry_on_date(
    state: &mut AppState,
    task: TaskId,
    hour: u32,
    today: chrono::NaiveDate,
) -> taskboard_domain::TimeEntryId {
    use chrono::{Datelike as _, TimeZone as _};
    let start = chrono::Local
        .with_ymd_and_hms(today.year(), today.month(), today.day(), hour.min(22), 0, 0)
        .single()
        .expect("resolvable local wall-clock")
        .with_timezone(&chrono::Utc);
    closed_entry_at(state, task, start)
}

/// `closed_entry_on_date` at the *device's current* local date — for
/// the already-fine call sites whose oracle is relative.
fn closed_entry_on(state: &mut AppState, task: TaskId, hour: u32) -> taskboard_domain::TimeEntryId {
    closed_entry_on_date(state, task, hour, chrono::Local::now().date_naive())
}

/// The add journey (D-TS4): open → add form → typed fields → commit
/// routes exactly one `LogTime` with the exact local→UTC instants,
/// and the surface steps back to the list.
#[test]
fn the_track_form_add_commit_routes_logtime_with_the_exact_instants() {
    on_slint_thread(|| {
        use chrono::{Datelike as _, TimeZone as _};
        // Single-read capture (D-3/R3): `today` is this test's only
        // clock read; every expected instant derives from it. Residual
        // (accepted, R2/R3): the adapter's `seed_track_form` and
        // `commit_track_form` keep their own `Local::now().date_naive()`
        // reads — a local midnight falling between this capture and the
        // adapter's read is the accepted millisecond-wide window.
        let today = chrono::Local::now().date_naive();
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        {
            let mut a = adapter.borrow_mut();
            a.interpret_for_test(
                rig.handle.window(),
                crate::router::UiEffect::TrackEntriesOpen,
            );
        }
        assert!(matches!(
            adapter.borrow().view_state.flow(),
            Flow::TrackEntries { .. }
        ));
        {
            let mut a = adapter.borrow_mut();
            let outcomes = a.view_state.open_track_entry_form(a_id, None);
            a.apply_outcomes_for_test(rig.handle.window(), &outcomes);
            a.seed_track_form_for_test(None);
            a.refresh_forced(rig.handle.window());
        }
        assert!(rig.handle.window().get_track_form_open());
        // The add seed: today's date, both times unset.
        assert_eq!(
            rig.handle.window().get_track_form_date_text(),
            today.format("%Y-%m-%d").to_string(),
            "the empty date resolves today"
        );
        assert_eq!(rig.handle.window().get_track_form_start_text(), "--:--");
        assert_eq!(rig.handle.window().get_track_form_end_text(), "--:--");

        // Type the span: 22:00 → 23:00 local (inside today, rollover
        // safe). The commit must route the exact instants.
        {
            let mut a = adapter.borrow_mut();
            let form = a.track_form_mut_for_test().expect("the form is seeded");
            form.start_text = "22:00".to_owned();
            form.end_text = "23:00".to_owned();
            let ok = a.commit_track_form_for_test(rig.handle.window());
            assert!(ok, "a parsed span commits");
        }
        let expected_start = chrono::Local
            .with_ymd_and_hms(today.year(), today.month(), today.day(), 22, 0, 0)
            .single()
            .expect("resolvable")
            .with_timezone(&chrono::Utc);
        let expected_end = expected_start + chrono::TimeDelta::hours(1);
        let commands = rig.drain_commands();
        assert_eq!(
            commands,
            vec![StateCommand::LogTime {
                task: a_id,
                start: expected_start,
                end: expected_end,
                kind: taskboard_domain::tracking::TimeEntryKind::Work,
            }],
            "exactly one LogTime with the exact local→UTC instants"
        );
        // The commit steps the surface back to the list; the overlay
        // stays open (the multi-add posture).
        assert!(matches!(
            adapter.borrow().view_state.flow(),
            Flow::TrackEntries { .. }
        ));
    });
}

/// The edit journey (closed entry): row-activate seeds the local
/// fields; changing the end routes the exact `AdjustTimeEntry` mask
/// (`start: Some`, `end: Some`, `kind: None`).
#[test]
fn the_track_form_edit_commit_routes_the_exact_adjust_mask() {
    on_slint_thread(|| {
        use chrono::{Datelike as _, TimeZone as _};
        // Single-read capture (D-3/R3): `today` is this test's only
        // clock read; the entry seed and every expected instant derive
        // from it. Residual (accepted, R2/R3): the adapter's
        // `seed_track_form` and `commit_track_form` keep their own
        // `Local::now().date_naive()` reads — a local midnight falling
        // between this capture and the adapter's read is the accepted
        // millisecond-wide window.
        let today = chrono::Local::now().date_naive();
        let (mut state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let entry = closed_entry_on_date(&mut state, a_id, 10, today);
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        {
            let mut a = adapter.borrow_mut();
            a.interpret_for_test(
                rig.handle.window(),
                crate::router::UiEffect::TrackEntriesOpen,
            );
        }
        // Row-activate: the seed resolves through the id table.
        {
            let mut a = adapter.borrow_mut();
            let outcomes = a.view_state.open_track_entry_form(a_id, Some(entry));
            a.apply_outcomes_for_test(rig.handle.window(), &outcomes);
            a.seed_track_form_for_test(Some(entry));
            a.refresh_forced(rig.handle.window());
        }
        // The seeded fields are the entry's local wall-clocks (the
        // echo-local rule); the closed end seeds its time.
        let start_local = chrono::Local
            .with_ymd_and_hms(today.year(), today.month(), today.day(), 10, 0, 0)
            .single()
            .expect("resolvable");
        assert_eq!(
            rig.handle.window().get_track_form_date_text(),
            start_local.format("%Y-%m-%d").to_string()
        );
        assert_eq!(rig.handle.window().get_track_form_start_text(), "10:00");
        assert_eq!(rig.handle.window().get_track_form_end_text(), "11:00");
        // Change the end; the commit routes the exact mask.
        {
            let mut a = adapter.borrow_mut();
            let form = a.track_form_mut_for_test().expect("the form is seeded");
            form.end_text = "12:30".to_owned();
            let ok = a.commit_track_form_for_test(rig.handle.window());
            assert!(ok);
        }
        let expected_start = start_local.with_timezone(&chrono::Utc);
        let expected_end = chrono::Local
            .with_ymd_and_hms(today.year(), today.month(), today.day(), 12, 30, 0)
            .single()
            .expect("resolvable")
            .with_timezone(&chrono::Utc);
        let commands = rig.drain_commands();
        assert_eq!(
            commands,
            vec![StateCommand::AdjustTimeEntry {
                id: entry,
                start: Some(expected_start),
                end: Some(expected_end),
                kind: None,
            }],
            "the mask: start Some, end Some, kind None"
        );
    });
}

/// The running row's edit (stage-3 focus b): an open entry seeds the
/// end EMPTY; an empty-end commit routes `end: None` (the mask's
/// keep-open — the entry stays running, well-defined); a typed end
/// closes it.
#[test]
fn the_running_rows_edit_seeds_an_empty_end_and_the_mask_keeps_it_open() {
    on_slint_thread(|| {
        let (mut state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let start = chrono::Utc::now();
        let entry = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
        state.tracking.entries.insert(
            entry,
            taskboard_domain::TimeEntry {
                id: entry,
                task: a_id,
                task_title: "running".to_owned(),
                start,
                end: None,
                created_at: start,
                updated_at: start,
                kind: taskboard_domain::tracking::TimeEntryKind::Work,
                by: None,
            },
        );
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        {
            let mut a = adapter.borrow_mut();
            a.interpret_for_test(
                rig.handle.window(),
                crate::router::UiEffect::TrackEntriesOpen,
            );
        }
        {
            let mut a = adapter.borrow_mut();
            let outcomes = a.view_state.open_track_entry_form(a_id, Some(entry));
            a.apply_outcomes_for_test(rig.handle.window(), &outcomes);
            a.seed_track_form_for_test(Some(entry));
            a.refresh_forced(rig.handle.window());
        }
        // The open entry's end seeds EMPTY (the keep-open seed), the
        // start seeds its local wall-clock.
        assert_eq!(rig.handle.window().get_track_form_end_text(), "--:--");
        assert_ne!(rig.handle.window().get_track_form_start_text(), "--:--");
        // Empty-end commit: `end: None` in the mask.
        {
            let mut a = adapter.borrow_mut();
            let ok = a.commit_track_form_for_test(rig.handle.window());
            assert!(ok, "the keep-open mask is well-defined");
        }
        let commands = rig.drain_commands();
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            &commands[0],
            StateCommand::AdjustTimeEntry {
                end: None,
                start: Some(_),
                kind: None,
                ..
            }
        ));
        // The typed end closes: `end: Some(_)`.
        {
            let mut a = adapter.borrow_mut();
            let outcomes = a.view_state.open_track_entry_form(a_id, Some(entry));
            a.apply_outcomes_for_test(rig.handle.window(), &outcomes);
            a.seed_track_form_for_test(Some(entry));
            let form = a.track_form_mut_for_test().expect("seeded");
            form.end_text = "23:59".to_owned();
            let ok = a.commit_track_form_for_test(rig.handle.window());
            assert!(ok);
        }
        let commands = rig.drain_commands();
        assert!(matches!(
            &commands[..],
            [StateCommand::AdjustTimeEntry { end: Some(_), .. }]
        ));
    });
}

/// The live validity table gates the commit (D-TS4): an inverted
/// span lands on the end field — the command lane stays empty and
/// the form stays open.
#[test]
fn an_inverted_span_refuses_the_commit_and_lands_on_the_end_field() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        {
            let mut a = adapter.borrow_mut();
            a.interpret_for_test(
                rig.handle.window(),
                crate::router::UiEffect::TrackEntriesOpen,
            );
            let outcomes = a.view_state.open_track_entry_form(a_id, None);
            a.apply_outcomes_for_test(rig.handle.window(), &outcomes);
            a.seed_track_form_for_test(None);
            let form = a.track_form_mut_for_test().expect("seeded");
            form.start_text = "10:00".to_owned();
            form.end_text = "09:00".to_owned();
            let ok = a.commit_track_form_for_test(rig.handle.window());
            assert!(!ok, "the inverted span refuses");
        }
        assert_eq!(rig.drain(), 0, "no command dispatches");
        assert!(
            matches!(
                adapter.borrow().view_state.flow(),
                Flow::TrackEntryForm { .. }
            ),
            "the form stays open — never swallowed silence"
        );
        assert_eq!(
            rig.handle.window().get_track_form_end_validity(),
            crate::FieldValidity::Invalid,
            "the pair's rejection lands on the END field"
        );
    });
}

/// The armed delete (D-TS5): the first activate arms (no command, the
/// armed mirror reads the row's index), the second dispatches
/// `DeleteTimeEntry { id }` verbatim; a dismissal disarms.
#[test]
fn the_delete_affordance_arms_then_confirms_and_dismissal_disarms() {
    on_slint_thread(|| {
        let (mut state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let entry = closed_entry_on(&mut state, a_id, 8);
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        {
            let mut a = adapter.borrow_mut();
            a.interpret_for_test(
                rig.handle.window(),
                crate::router::UiEffect::TrackEntriesOpen,
            );
        }
        // First activate: arms, dispatches nothing.
        rig.handle.window().invoke_track_entry_delete_clicked(0);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert_eq!(rig.drain(), 0, "arming dispatches nothing");
        assert_eq!(rig.handle.window().get_track_armed_index(), 0);
        assert!(adapter.borrow().test_armed_delete_is(entry));
        // Second activate: the confirm dispatches the existing verb.
        rig.handle.window().invoke_track_entry_delete_clicked(0);
        let commands = rig.drain_commands();
        assert_eq!(
            commands,
            vec![StateCommand::DeleteTimeEntry { id: entry }],
            "the confirm dispatches DeleteTimeEntry verbatim"
        );
        // The dismissal disarms (the adapter holds no target after).
        rig.handle.window().invoke_track_entries_dismiss_requested();
        assert!(
            adapter.borrow().test_armed_delete_is_none(),
            "the overlay's close clears the armed target"
        );
    });
}

/// The Tracked-today section (#490, D-TS7): a tracked task renders a
/// row whose activation opens the task; an untracked day collapses
/// (the pushed section model is empty — the view's `length > 0`
/// never renders it).
#[test]
fn the_tracked_today_section_opens_its_task_and_an_empty_day_collapses() {
    on_slint_thread(|| {
        let (mut state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let start = chrono::Utc::now();
        let entry = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
        state.tracking.entries.insert(
            entry,
            taskboard_domain::TimeEntry {
                id: entry,
                task: a_id,
                task_title: "tracked".to_owned(),
                start,
                end: Some(start + chrono::TimeDelta::minutes(5)),
                created_at: start,
                updated_at: start,
                kind: taskboard_domain::tracking::TimeEntryKind::Work,
                by: None,
            },
        );
        let journey = rig(state);
        let adapter = journey.adapter();
        mount_task_detail(&journey, &adapter, a_id, s0);
        // Navigate to today (the section lives there).
        adapter
            .borrow_mut()
            .view_state
            .set_view(crate::view_state::ViewId::Today);
        adapter.borrow_mut().refresh_forced(journey.handle.window());
        // The row is pushed and the table resolves it.
        assert_eq!(
            adapter.borrow().test_tracked_today_rows(),
            vec![a_id],
            "the tracked task renders a row"
        );
        // Activation opens the task (the today-row-activated arm).
        journey.handle.window().invoke_today_tracked_activated(0);
        assert_eq!(
            adapter.borrow().view_state.view(),
            crate::view_state::ViewId::TaskDetail,
        );
        assert_eq!(adapter.borrow().view_state.subject_task(), Some(a_id));

        // The empty day: no entries — the section's model is empty
        // (the collapse's input).
        let (state2, fx2) = fixture();
        let [a2, _b2, _c2] = fx2.tasks;
        let [s02, _s12] = fx2.stacks;
        let rig2 = rig(state2);
        let adapter2 = rig2.adapter();
        {
            let mut a = adapter2.borrow_mut();
            a.refresh_forced(rig2.handle.window());
            a.view_state.activate_card(a2, s02);
            a.view_state
                .activate_focused(&rig2.handle.test_state().load_full());
            a.view_state.set_view(crate::view_state::ViewId::Today);
            a.refresh_forced(rig2.handle.window());
        }
        assert_eq!(
            adapter2.borrow().test_tracked_today_rows(),
            Vec::<TaskId>::new(),
            "an untracked day renders no rows"
        );
    });
}

/// The add commit routes over the FORM'S SUBJECT (the one-shot target
/// seam, D-TS6), not the focus fallback: a task detail entered without
/// a focused card (the stack-first entry path leaves `focused_task`
/// `None`) still logs onto the viewed task — no retarget, no swallow.
#[test]
fn the_add_commit_routes_over_the_form_subject_not_the_focus() {
    on_slint_thread(|| {
        use chrono::{Datelike as _, TimeZone as _};
        // Single-read capture (D-3/R3): `today` is this test's only
        // clock read; every expected instant derives from it. Residual
        // (accepted, R2/R3): the adapter's `seed_track_form` and
        // `commit_track_form` keep their own `Local::now().date_naive()`
        // reads — a local midnight falling between this capture and the
        // adapter's read is the accepted millisecond-wide window.
        let today = chrono::Local::now().date_naive();
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        // The stack-first entry path: the subject without a focused
        // card.
        adapter.borrow_mut().view_state.set_focused(None);
        {
            let mut a = adapter.borrow_mut();
            a.interpret_for_test(
                rig.handle.window(),
                crate::router::UiEffect::TrackEntriesOpen,
            );
        }
        assert!(matches!(
            adapter.borrow().view_state.flow(),
            Flow::TrackEntries { .. }
        ));
        {
            let mut a = adapter.borrow_mut();
            let outcomes = a.view_state.open_track_entry_form(a_id, None);
            a.apply_outcomes_for_test(rig.handle.window(), &outcomes);
            a.seed_track_form_for_test(None);
            a.refresh_forced(rig.handle.window());
            let form = a.track_form_mut_for_test().expect("the form is seeded");
            form.start_text = "20:00".to_owned();
            form.end_text = "21:00".to_owned();
            let ok = a.commit_track_form_for_test(rig.handle.window());
            assert!(ok, "a parsed span commits");
        }
        let expected_start = chrono::Local
            .with_ymd_and_hms(today.year(), today.month(), today.day(), 20, 0, 0)
            .single()
            .expect("resolvable")
            .with_timezone(&chrono::Utc);
        assert_eq!(
            rig.drain_commands(),
            vec![StateCommand::LogTime {
                task: a_id,
                start: expected_start,
                end: expected_start + chrono::TimeDelta::hours(1),
                kind: taskboard_domain::tracking::TimeEntryKind::Work,
            }],
            "the add lands on the viewed task without a focused card"
        );
    });
}

/// The strand-proof (D-TS2, stage-3 focus d): the overlay cannot
/// outlive its view. A dead subject drops the flow and pops the view
/// back to the board; a view flip away from the task detail drops the
/// flow too — the pushed open flag reads closed either way, so no
/// stranded modal context survives.
#[test]
fn the_overlay_cannot_outlive_its_subject_or_view() {
    on_slint_thread(|| {
        // A dead subject underneath the open overlay.
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let strand_rig = rig(state);
        let adapter = strand_rig.adapter();
        mount_task_detail(&strand_rig, &adapter, a_id, s0);
        {
            let mut a = adapter.borrow_mut();
            a.interpret_for_test(
                strand_rig.handle.window(),
                crate::router::UiEffect::TrackEntriesOpen,
            );
        }
        assert!(matches!(
            adapter.borrow().view_state.flow(),
            Flow::TrackEntries { .. }
        ));
        {
            let swap = strand_rig.handle.test_state();
            let mut s = swap.load_full().as_ref().clone();
            s.tasks.get_mut(&a_id).expect("the subject is live").deleted = true;
            swap.store(Arc::new(s));
        }
        adapter
            .borrow_mut()
            .refresh_forced(strand_rig.handle.window());
        {
            let a = adapter.borrow();
            assert!(
                matches!(a.view_state.flow(), Flow::None),
                "a dead subject drops the flow"
            );
            assert_eq!(
                a.view_state.view(),
                crate::view_state::ViewId::Board,
                "a dead subject pops the view"
            );
        }
        assert!(
            !strand_rig
                .handle
                .window()
                .get_task_model()
                .track_entries_open,
            "the overlay's mount flag reads closed"
        );

        // A view flip underneath the open overlay.
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let flip_rig = rig(state);
        let adapter = flip_rig.adapter();
        mount_task_detail(&flip_rig, &adapter, a_id, s0);
        {
            let mut a = adapter.borrow_mut();
            a.interpret_for_test(
                flip_rig.handle.window(),
                crate::router::UiEffect::TrackEntriesOpen,
            );
        }
        assert!(matches!(
            adapter.borrow().view_state.flow(),
            Flow::TrackEntries { .. }
        ));
        {
            let mut a = adapter.borrow_mut();
            let outcomes = a.view_state.focus_board();
            a.apply_outcomes_for_test(flip_rig.handle.window(), &outcomes);
            a.refresh_forced(flip_rig.handle.window());
        }
        {
            let a = adapter.borrow();
            assert!(
                matches!(a.view_state.flow(), Flow::None),
                "a view flip drops the flow"
            );
        }
        assert!(
            !flip_rig.handle.window().get_task_model().track_entries_open,
            "the overlay cannot outlive the view"
        );
    });
}

/// The kiosk gate (D-TS8): the affordance flag reads false and the
/// open intent refuses — no flow, no command (the display-stays,
/// action-empties posture's action half).
#[test]
fn the_kiosk_gate_refuses_the_entries_open() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let mut state = state;
        closed_entry_on(&mut state, a_id, 10);
        let swap = Arc::new(arc_swap::ArcSwap::from_pointee(state));
        let (commands_tx, mut commands_rx) = mpsc::channel(64);
        let (requests_tx, _requests_rx) = mpsc::channel(16);
        let (_health_tx, health) =
            tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
        let handle = mount(
            swap,
            dummy_receiver(),
            dummy_receiver(),
            commands_tx,
            requests_tx,
            UiSettings {
                kiosk: true,
                ..UiSettings::default()
            },
            health,
            None,
        )
        .expect("mount succeeds on the null platform");
        let adapter = handle.test_adapter();
        adapter.borrow_mut().refresh_forced(handle.window());
        assert!(
            !handle.window().get_task_model().track_entries_visible,
            "kiosk hides the affordance"
        );
        adapter
            .borrow_mut()
            .interpret_for_test(handle.window(), crate::router::UiEffect::TrackEntriesOpen);
        assert!(
            matches!(adapter.borrow().view_state.flow(), Flow::None),
            "the open refuses on kiosk"
        );
        assert!(
            commands_rx.try_recv().is_err(),
            "the refusal dispatches nothing"
        );
    });
}

/// The kiosk Tracked section (#490, D-TS7): rows filter to the viewed
/// board and the day total sums the filtered set — what you see is
/// what's summed.
#[test]
fn the_kiosk_tracked_section_filters_to_the_viewed_board() {
    on_slint_thread(|| {
        use taskboard_domain::{Board, BoardClocks, BoardId, Color, Stack, StackClocks, StackId};
        let now = taskboard_domain::BoardClocks::epoch();
        let board = |id: BoardId, title: &str| Board {
            id,
            remote: None,
            title: title.to_owned(),
            color: Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: now,
                color: now,
                archived: now,
                deleted: now,
            },
            authors: taskboard_domain::entities::BoardAuthors::default(),
            remote_seen: None,
        };
        let board_one = BoardId::from_uuid(uuid::Uuid::now_v7());
        let board_two = BoardId::from_uuid(uuid::Uuid::now_v7());
        let stack_one = StackId::from_uuid(uuid::Uuid::now_v7());
        let stack_two = StackId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        state.boards.insert(board_one, board(board_one, "one"));
        state.boards.insert(board_two, board(board_two, "two"));
        for (stack, board, order) in [(stack_one, board_one, 0), (stack_two, board_two, 0)] {
            state.stacks.insert(
                stack,
                Stack {
                    id: stack,
                    remote: None,
                    board,
                    title: "s".to_owned(),
                    order,
                    archived: false,
                    deleted: false,
                    clocks: StackClocks {
                        title: now,
                        order: now,
                        deleted: now,
                    },
                    authors: taskboard_domain::entities::StackAuthors::default(),
                    remote_seen: None,
                },
            );
        }
        let (task_one, task_two) = (
            taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7()),
            taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7()),
        );
        for (id, stack, order) in [(task_one, stack_one, 0), (task_two, stack_two, 1)] {
            state.tasks.insert(
                id,
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
                    clocks: taskboard_domain::TaskClocks {
                        title: now,
                        description: now,
                        duedate: now,
                        scheduled: now,
                        done: now,
                        position: now,
                        effort: now,
                        importance: now,
                        urgency: now,
                        recur: now,
                        kind: now,
                        archived: now,
                        deleted: now,
                    },
                    authors: taskboard_domain::entities::TaskAuthors::default(),
                    remote_seen: None,
                    remote_baseline: None,
                },
            );
        }
        // One tracked hour per board, at absolute instants strictly
        // inside the frozen UTC day window (docs/testing_strategy.org
        // §6.2, #627): task_one 09:00–10:00Z, task_two 11:00–12:00Z.
        // A closed entry counts in full wherever it lies in the window
        // (`tracked_in_window` clamps only open accrual at `now`), so
        // every input to the rows/total oracle is the constant.
        let now = crate::test_platform::PINNED_NOW;
        closed_entry_at(&mut state, task_one, now - chrono::TimeDelta::hours(3));
        closed_entry_at(&mut state, task_two, now - chrono::TimeDelta::hours(1));

        let swap = Arc::new(arc_swap::ArcSwap::from_pointee(state));
        let (commands_tx, _commands_rx) = mpsc::channel(64);
        let (requests_tx, _requests_rx) = mpsc::channel(16);
        let (_health_tx, health) =
            tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
        let handle = mount(
            swap.clone(),
            dummy_receiver(),
            dummy_receiver(),
            commands_tx,
            requests_tx,
            UiSettings {
                kiosk: true,
                // The render pass's clock frozen at the seed instant
                // (`docs/testing_strategy.org` §6.2, #627): every
                // input to the rows/total oracle is the constant.
                clock: Some(now),
                ..UiSettings::default()
            },
            health,
            None,
        )
        .expect("mount succeeds on the null platform");
        let adapter = handle.test_adapter();
        {
            let mut a = adapter.borrow_mut();
            a.refresh_forced(handle.window());
            a.view_state.select_board(&swap.load(), board_two);
            a.view_state.set_view(crate::view_state::ViewId::Today);
            a.refresh_forced(handle.window());
        }
        assert_eq!(
            adapter.borrow().test_tracked_today_rows(),
            vec![task_two],
            "kiosk filters the rows to the viewed board"
        );
        // What you see is what's summed: the day total is the
        // filtered set's phrase (derived, not pinned copy).
        let expected = crate::presentation::tracked_total_phrase(&crate::presentation::part_total(
            chrono::TimeDelta::hours(1),
        ));
        assert_eq!(
            handle.window().get_today_model().tracked_day_total,
            expected,
            "the day total sums the filtered set"
        );
    });
}

/// The placed box's re-open seeds the placement form from the row's
/// instant (#622, D-3): committing with no typing routes the box's
/// own instant back — the seed round-trips as the placement write. A
/// stale scheduled stamp from an earlier day seeds nothing (the
/// commit would re-place to tomorrow), so its commit is the typed
/// inert.
#[test]
fn the_placed_box_reopen_seeds_the_form_from_the_instant() {
    on_slint_thread(|| {
        // The clock pinned (docs/testing_strategy.org §6.2): the
        // placed instant, both rigs' render passes and the stale arm
        // all derive from PINNED_NOW — no wall-clock read anywhere.
        let now = crate::test_platform::PINNED_NOW;
        let day_start = (
            taskboard_domain::SettingsSnapshot::default().day_start_time,
            UiSettings::default().boot_offset,
        );
        let (next_start, _) =
            taskboard_domain::days::next_day_window(now, day_start.0, day_start.1);
        let instant = next_start + chrono::TimeDelta::hours(2);
        let pinned_settings = UiSettings {
            clock: Some(now),
            ..UiSettings::default()
        };

        let (mut state, fx) = fixture();
        state.tasks.get_mut(&fx.tasks[0]).expect("task").scheduled = Some(instant);
        let mut plan_rig = guard_rig(state, pinned_settings.clone());
        let adapter = plan_rig.adapter();
        {
            let mut a = adapter.borrow_mut();
            a.interpret_for_test(
                plan_rig.handle.window(),
                UiEffect::PlanBeginPlace(fx.tasks[0]),
            );
        }
        assert!(matches!(
            adapter.borrow().view_state.flow(),
            Flow::PlanPlace { .. }
        ));
        // The seed round-trips: commit with no typing routes the box's
        // own instant.
        {
            let mut a = adapter.borrow_mut();
            a.interpret_for_test(plan_rig.handle.window(), UiEffect::PlanCommitPlace);
        }
        assert_eq!(
            plan_rig.drain_commands(),
            vec![StateCommand::UpdateTask {
                id: fx.tasks[0],
                changes: taskboard_domain::TaskChanges {
                    scheduled: Some(Some(instant)),
                    ..taskboard_domain::TaskChanges::default()
                },
            }],
            "the seeded instant commits untouched"
        );

        // The stale-stamp guard: a stamp three days before the
        // frozen `now` seeds nothing, the empty time field refuses
        // the commit (typed inert, form kept). Relative to the pin,
        // never the wall clock — a wall-relative stamp drifts into
        // the frozen day's future and stops being stale (#663).
        let (mut state, fx) = fixture();
        state.tasks.get_mut(&fx.tasks[0]).expect("task").scheduled =
            Some(now - chrono::TimeDelta::days(3));
        let mut plan_rig = guard_rig(state, pinned_settings);
        let adapter = plan_rig.adapter();
        {
            let mut a = adapter.borrow_mut();
            a.interpret_for_test(
                plan_rig.handle.window(),
                UiEffect::PlanBeginPlace(fx.tasks[0]),
            );
            a.interpret_for_test(plan_rig.handle.window(), UiEffect::PlanCommitPlace);
        }
        assert!(matches!(
            adapter.borrow().view_state.flow(),
            Flow::PlanPlace { .. }
        ));
        assert!(
            plan_rig.drain_commands().is_empty(),
            "the unseeded commit dispatches nothing"
        );
    });
}

// -- the live settings mirror (GUI review round 5, chunk 5, D-5.7) ------

/// Publishes a copy of the rig's state with `edit` applied to its
/// settings and runs one refresh — the engine's publish shape (a fresh
/// Arc), so the guarded refresh takes the forced pass.
fn publish_settings(rig: &Rig, edit: impl FnOnce(&mut taskboard_domain::SettingsSnapshot)) {
    let shared = rig.handle.test_state();
    let mut next = (**shared.load()).clone();
    edit(&mut next.settings);
    shared.store(Arc::new(next));
    rig.adapter().borrow_mut().refresh(rig.handle.window());
}

/// A day-start change moves the today window's boundary at the next
/// refresh: the pass derives the window from the published settings,
/// never from a hand-over value.
#[test]
fn a_day_start_change_moves_the_window_at_the_next_refresh() {
    on_slint_thread(|| {
        let (state, _fx) = fixture();
        let rig = rig(state);
        rig.adapter().borrow_mut().refresh(rig.handle.window());
        let before = rig.adapter().borrow().day_ctx().window;
        assert_eq!(
            before.0.time(),
            chrono::NaiveTime::MIN,
            "the default day start opens the window at midnight"
        );

        publish_settings(&rig, |s| {
            s.day_start_time = chrono::NaiveTime::from_hms_opt(5, 0, 0).expect("05:00");
        });
        let after = rig.adapter().borrow().day_ctx().window;
        assert_ne!(after.0, before.0, "the window boundary moved");
        assert_eq!(
            after.0.time(),
            chrono::NaiveTime::from_hms_opt(5, 0, 0).expect("05:00"),
            "the window now opens at the live day start"
        );
        assert_eq!(
            rig.adapter().borrow().calendar().day_start_time,
            chrono::NaiveTime::from_hms_opt(5, 0, 0).expect("05:00")
        );
    });
}

/// The due-soon horizon and the reminder defaults follow the published
/// settings at the next refresh — the sets' boundary and the
/// notification surface's derivation input both move.
#[test]
fn the_horizon_and_the_defaults_follow_the_published_settings() {
    on_slint_thread(|| {
        let (state, _fx) = fixture();
        let rig = rig(state);
        rig.adapter().borrow_mut().refresh(rig.handle.window());
        assert_eq!(
            rig.adapter().borrow().due_soon_horizon,
            chrono::TimeDelta::days(1)
        );
        assert_eq!(
            rig.adapter().borrow().reminder_defaults.default_before,
            taskboard_domain::ReminderOffset::at_due()
        );

        let before = taskboard_domain::ReminderOffset::before(chrono::TimeDelta::minutes(-15))
            .expect("a before offset");
        publish_settings(&rig, |s| {
            s.due_soon_days = taskboard_domain::DueSoonDays::new(3).expect("in range");
            s.reminder_before = before;
        });
        assert_eq!(
            rig.adapter().borrow().due_soon_horizon,
            chrono::TimeDelta::days(3),
            "the horizon moved with the setting"
        );
        assert_eq!(
            rig.adapter().borrow().reminder_defaults.default_before,
            before,
            "the defaults moved with the setting"
        );
    });
}

/// The focus timings follow the published settings: a live enable
/// lights the overlay action, a live disable parks it — and a kiosk
/// never carries a timing regardless.
#[test]
fn the_focus_timings_follow_the_published_settings() {
    on_slint_thread(|| {
        let (state, _fx) = fixture();
        let rig = rig(state);
        rig.adapter().borrow_mut().refresh(rig.handle.window());
        assert!(rig.adapter().borrow().focus.is_none());

        publish_settings(&rig, |s| {
            s.focus_enabled = true;
            s.focus = Some(focus_timing());
        });
        assert_eq!(rig.adapter().borrow().focus, Some(focus_timing()));

        publish_settings(&rig, |s| {
            s.focus_enabled = false;
        });
        assert!(
            rig.adapter().borrow().focus.is_none(),
            "a live disable parks the overlay input"
        );
    });
}

// -- the priority pickers (UX remediation chunk 3, D-1/D-9, #788) --

/// The slot vocabulary's exact `UpdateTask` masks (D-9): per level
/// the slot click dispatches `UpdateTask { importance:
/// Some(Some(level)) }` with every other mask `None`; the `unset`
/// slot commits `Some(None)` — the CLI's `none` twin. The urgency
/// row is the symmetric twin.
#[test]
fn the_importance_picker_slots_dispatch_exact_masks() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        // Open the picker (the routed action's view intent).
        adapter.borrow_mut().interpret_for_test(
            rig.handle.window(),
            UiEffect::PriorityPickerOpen(crate::router::PriorityField::Importance),
        );
        let levels = [
            taskboard_domain::Importance::VeryLow,
            taskboard_domain::Importance::Low,
            taskboard_domain::Importance::Normal,
            taskboard_domain::Importance::High,
            taskboard_domain::Importance::VeryHigh,
        ];
        for (slot, level) in levels.iter().enumerate() {
            rig.handle
                .window()
                .invoke_priority_slot_clicked(i32::try_from(slot + 1).unwrap_or(i32::MAX));
            let commands = rig.drain_commands();
            assert!(
                matches!(
                    &commands[..],
                    [StateCommand::UpdateTask {
                        id,
                        changes: taskboard_domain::TaskChanges { importance: Some(Some(_)), .. },
                    }] if *id == a_id
                ),
                "slot {slot} dispatches the set mask"
            );
            // The exact level rides the slot's declaration order.
            if let [StateCommand::UpdateTask { changes, .. }] = &commands[..] {
                assert_eq!(changes.importance, Some(Some(*level)));
                assert_eq!(changes.urgency, None, "the sibling mask stays untouched");
            }
            // Re-open (the commit closed the picker).
            adapter.borrow_mut().interpret_for_test(
                rig.handle.window(),
                UiEffect::PriorityPickerOpen(crate::router::PriorityField::Importance),
            );
        }
        // The unset slot commits the clear twin.
        rig.handle.window().invoke_priority_slot_clicked(0);
        let commands = rig.drain_commands();
        assert!(matches!(
            &commands[..],
            [StateCommand::UpdateTask {
                changes: taskboard_domain::TaskChanges {
                    importance: Some(None),
                    ..
                },
                ..
            }]
        ));
    });
}

/// The urgency picker's symmetric mask (D-9): a slot click routes the
/// `urgency` mask, the importance mask stays `None` — the fields'
/// independence is the dispatch's own shape.
#[test]
fn the_urgency_picker_slots_dispatch_the_symmetric_mask() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        adapter.borrow_mut().interpret_for_test(
            rig.handle.window(),
            UiEffect::PriorityPickerOpen(crate::router::PriorityField::Urgency),
        );
        rig.handle.window().invoke_priority_slot_clicked(4);
        let commands = rig.drain_commands();
        assert!(matches!(
            &commands[..],
            [StateCommand::UpdateTask {
                id,
                changes: taskboard_domain::TaskChanges {
                    urgency: Some(Some(taskboard_domain::Urgency::High)),
                    ..
                },
            }] if *id == a_id
        ));
    });
}

/// Escape on an open picker dispatches nothing and clears both open
/// flags (D-9's settle test); the dismissal seam is the same
/// close-without-effect shape (the outside click).
#[test]
fn escape_and_dismissal_close_the_picker_without_dispatching() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        adapter.borrow_mut().interpret_for_test(
            rig.handle.window(),
            UiEffect::PriorityPickerOpen(crate::router::PriorityField::Importance),
        );
        assert!(rig.handle.window().get_task_model().importance_open);
        // Escape routes CancelMove at this surface; the picker's rung
        // owns it — nothing dispatches, the flag clears.
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::CancelMove);
        assert_eq!(rig.drain(), 0, "the cancel dispatches nothing");
        assert!(!rig.handle.window().get_task_model().importance_open);
        // The dismissal seam (the outside click) is the same shape.
        adapter.borrow_mut().interpret_for_test(
            rig.handle.window(),
            UiEffect::PriorityPickerOpen(crate::router::PriorityField::Urgency),
        );
        assert!(rig.handle.window().get_task_model().urgency_open);
        rig.handle.window().invoke_priority_dismiss_requested();
        assert_eq!(rig.drain(), 0);
        assert!(!rig.handle.window().get_task_model().urgency_open);
        assert!(!rig.handle.window().get_task_model().importance_open);
    });
}

/// The editor exclusivity (D-1): opening one picker settles the
/// other — exactly one open flag at any pass.
#[test]
fn opening_one_picker_settles_the_other() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        adapter.borrow_mut().interpret_for_test(
            rig.handle.window(),
            UiEffect::PriorityPickerOpen(crate::router::PriorityField::Importance),
        );
        adapter.borrow_mut().interpret_for_test(
            rig.handle.window(),
            UiEffect::PriorityPickerOpen(crate::router::PriorityField::Urgency),
        );
        let model = rig.handle.window().get_task_model();
        assert!(model.urgency_open);
        assert!(!model.importance_open, "the sibling picker settled");
        // And the reverse direction.
        adapter.borrow_mut().interpret_for_test(
            rig.handle.window(),
            UiEffect::PriorityPickerOpen(crate::router::PriorityField::Importance),
        );
        let model = rig.handle.window().get_task_model();
        assert!(model.importance_open);
        assert!(!model.urgency_open);
    });
}

/// The stale-index absorb (D-9/R-2's law): a slot index outside the
/// closed vocabulary dispatches nothing and leaves the picker armed
/// (the id-table seam's typed no-op).
#[test]
fn a_stale_slot_index_dispatches_nothing() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        adapter.borrow_mut().interpret_for_test(
            rig.handle.window(),
            UiEffect::PriorityPickerOpen(crate::router::PriorityField::Importance),
        );
        rig.handle.window().invoke_priority_slot_clicked(9);
        assert_eq!(rig.drain(), 0, "the stale slot dispatches nothing");
        assert!(
            rig.handle.window().get_task_model().importance_open,
            "the picker stays armed — never swallowed silence"
        );
    });
}

// -- the labels overlay (UX remediation chunk 3, D-2/D-9, #789) --

/// Seeds two live labels on the fixture's board: `green` assigned to
/// the subject, `red` unassigned — the toggle test's vocabulary.
fn seed_labels(
    state: &mut AppState,
    board: BoardId,
    subject: TaskId,
) -> (taskboard_domain::LabelId, taskboard_domain::LabelId) {
    let (green, red) = (
        taskboard_domain::LabelId::from_uuid(uuid::Uuid::now_v7()),
        taskboard_domain::LabelId::from_uuid(uuid::Uuid::now_v7()),
    );
    let make = |id, title: &str| taskboard_domain::Label {
        id,
        remote: None,
        board,
        title: title.to_owned(),
        color: taskboard_domain::Color::new("31cc7c"),
        deleted: false,
        clocks: taskboard_domain::LabelClocks {
            title: taskboard_domain::BoardClocks::epoch(),
            color: taskboard_domain::BoardClocks::epoch(),
            deleted: taskboard_domain::BoardClocks::epoch(),
        },
        authors: taskboard_domain::entities::LabelAuthors::default(),
        remote_seen: None,
    };
    state.labels.insert(green, make(green, "green"));
    state.labels.insert(red, make(red, "red"));
    if let Some(task) = state.tasks.get_mut(&subject) {
        task.label_memberships.insert(
            green,
            taskboard_domain::entities::MembershipState::Added {
                at: taskboard_domain::BoardClocks::epoch(),
            },
        );
    }
    (green, red)
}

/// The overlay's assignment toggle (D-9): the row's pushed state
/// names the pair's arm — the assigned row's toggle dispatches
/// `UnassignLabel`, the unassigned row's dispatches `AssignLabel`,
/// both over the VIEW'S SUBJECT with the id resolved from
/// `IdTables::label_ids`. A stale index absorbs typed and the
/// overlay stays armed.
#[test]
fn the_overlay_toggle_dispatches_the_assignment_pair() {
    on_slint_thread(|| {
        let (mut state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let (green, red) = seed_labels(&mut state, fx.board, a_id);
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::LabelsOpen);
        assert!(matches!(
            adapter.borrow().view_state.flow(),
            Flow::Labels {
                subject: crate::view_state::LabelOverlaySubject::Task(task),
            } if *task == a_id
        ));
        // Row 0 = green (assigned) → the unassign arm.
        rig.handle.window().invoke_labels_overlay_row_toggled(0);
        assert_eq!(
            rig.drain_commands(),
            vec![StateCommand::UnassignLabel {
                task: a_id,
                label: green,
            }],
        );
        // Row 1 = red (unassigned) → the assign arm.
        rig.handle.window().invoke_labels_overlay_row_toggled(1);
        assert_eq!(
            rig.drain_commands(),
            vec![StateCommand::AssignLabel {
                task: a_id,
                label: red,
            }],
        );
        // The stale index absorbs typed; the overlay stays armed.
        rig.handle.window().invoke_labels_overlay_row_toggled(9);
        assert_eq!(rig.drain(), 0);
        assert!(matches!(
            adapter.borrow().view_state.flow(),
            Flow::Labels { .. }
        ));
    });
}

/// The detail chip's × (D-2/D-9): the chip seam resolves through
/// `IdTables::task_labels` and dispatches `UnassignLabel` verbatim;
/// a stale index absorbs.
#[test]
fn the_chip_unassign_dispatches_the_exact_verb() {
    on_slint_thread(|| {
        let (mut state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let (green, _red) = seed_labels(&mut state, fx.board, a_id);
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        rig.handle.window().invoke_chip_unassign_clicked(0);
        assert_eq!(
            rig.drain_commands(),
            vec![StateCommand::UnassignLabel {
                task: a_id,
                label: green,
            }],
        );
        rig.handle.window().invoke_chip_unassign_clicked(5);
        assert_eq!(rig.drain(), 0, "the stale chip index absorbs");
    });
}

/// The overlay's ladder (D-2/D-9): `label.manage` opens over the
/// viewed task (the routed action's view intent — click ≡ key), the
/// dismissal closes without dispatching, and a second open request
/// under the live overlay is the kept inert (the modal discipline).
#[test]
fn the_labels_overlay_opens_and_the_dismissal_closes() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::LabelsOpen);
        assert!(rig.handle.window().get_task_model().labels_open);
        // The kept inert under the live overlay.
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::LabelsOpen);
        assert!(matches!(
            adapter.borrow().view_state.flow(),
            Flow::Labels { .. }
        ));
        // The dismissal: nothing dispatches, the flow ends.
        rig.handle
            .window()
            .invoke_labels_overlay_dismiss_requested();
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert_eq!(rig.drain(), 0);
        assert_eq!(adapter.borrow().view_state.flow(), &Flow::None);
        assert!(!rig.handle.window().get_task_model().labels_open);
    });
}

/// UX-R8 chunk-5 D-3 (US-1/S2) — the day-law parity twin: the
/// adapter's per-dispatch `tomorrow_window_end` is exactly
/// `days::next_day_window(now, day_start).1` for the adapter's own
/// `now` (single-read capture; no `+24h` arithmetic anywhere).
#[test]
fn tomorrow_window_end_is_the_next_day_window_end() {
    use crate::actions::ActionId;
    // The mount touches the Slint thread's testing platform (the walk's
    // rule): the backend exists only on that thread, headless CI
    // included.
    on_slint_thread(move || {
        let (state, fx) = fixture();
        let mut rig = rig(state);
        let adapter = rig.adapter();
        let expected;
        {
            let mut a = adapter.borrow_mut();
            // Focus the fixture's first task (the card-focused route);
            // the fixture's task a lives on stack s0.
            let outcomes = a.view_state.activate_card(fx.tasks[0], fx.stacks[0]);
            a.apply_outcomes_for_test(rig.handle.window(), &outcomes);
            let now = a.day_ctx().now;
            // The rig mounts UiSettings::default(), so the adapter's held
            // pair is the default day start + boot offset.
            let day_start = (
                taskboard_domain::SettingsSnapshot::default().day_start_time,
                crate::UiSettings::default().boot_offset,
            );
            expected = taskboard_domain::days::next_day_window(now, day_start.0, day_start.1).1;
            a.route(rig.handle.window(), ActionId::TaskScheduleTomorrow);
        }
        let commands = rig.drain_commands();
        assert_eq!(
            commands,
            vec![StateCommand::UpdateTask {
                id: *fx.tasks.first().expect("task"),
                changes: taskboard_domain::TaskChanges {
                    scheduled: Some(Some(expected)),
                    ..taskboard_domain::TaskChanges::default()
                },
            }],
            "tomorrow commits exactly the next day window's end"
        );
    });
}

/// UX-R8 chunk-5 D-6 (US-4/AC-3) — the card control's one-shot flips:
/// exactly one tracking command per state — the play click dispatches
/// Start exactly once, and after the state flips (the settle) the
/// same control dispatches Stop; no double dispatch per state.
#[test]
fn the_card_track_control_flips_one_shot_per_state() {
    use crate::actions::ActionId;
    // The mount touches the Slint thread's testing platform (the walk's
    // rule): the backend exists only on that thread, headless CI
    // included.
    on_slint_thread(move || {
        let (state, fx) = fixture();
        let mut rig = rig(state);
        let adapter = rig.adapter();
        {
            let mut a = adapter.borrow_mut();
            let outcomes = a.view_state.activate_card(fx.tasks[0], fx.stacks[0]);
            a.apply_outcomes_for_test(rig.handle.window(), &outcomes);
            a.refresh_forced(rig.handle.window());
        }
        // The play half: exactly one Start.
        rig.handle.window().invoke_board_task_track_toggled(0, 0);
        assert_eq!(
            rig.drain_commands(),
            vec![StateCommand::StartTracking {
                task: fx.tasks[0],
                kind: taskboard_domain::tracking::TimeEntryKind::Work,
            }],
            "the play half dispatches exactly one Start"
        );
        // The state flips (the engine settle shape): the open entry lands
        // on the card's task, the refresh rebuilds the models.
        let mut settled = rig.handle.test_state().load_full().as_ref().clone();
        let now = chrono::Utc::now();
        let entry = taskboard_domain::tracking::TimeEntry {
            id: taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7()),
            task: fx.tasks[0],
            task_title: String::new(),
            start: now,
            created_at: now,
            updated_at: now,
            by: None,
            end: None,
            kind: taskboard_domain::tracking::TimeEntryKind::Work,
        };
        settled.tracking.entries.insert(entry.id, entry);
        rig.handle.test_state().store(Arc::new(settled));
        rig.handle.refresh();
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        // The pause half: exactly one Stop.
        rig.handle.window().invoke_board_task_track_toggled(0, 0);
        assert_eq!(
            rig.drain_commands(),
            vec![StateCommand::StopTracking],
            "the pause half dispatches exactly one Stop"
        );
        let _ = ActionId::TaskTrackStart;
    });
}

/// `label.manage` routes the open intent from the registry (the
/// `rule_verbs_route_from_their_targets` shape): the dotted id
/// resolves, and the dispatch emits the `LabelsOpen` effect — no
/// command (the view intent's contract).
#[test]
fn label_manage_routes_the_open_intent() {
    use crate::actions::ActionId;
    use crate::router::{ActionRouter, DispatchCtx, DispatchOutcome};
    let ctx = DispatchCtx::default();
    assert!(matches!(
        ActionRouter::dispatch(ActionId::LabelManage, &ctx),
        DispatchOutcome::UiEffect(UiEffect::LabelsOpen)
    ));
    assert!(matches!(
        ActionRouter::dispatch(ActionId::LabelOverlayDismiss, &ctx),
        DispatchOutcome::UiEffect(UiEffect::LabelsDismiss)
    ));
}

// -- the labels form (UX remediation chunk 3, D-2/D-5/D-9, #789) --

/// The create journey (D-9): the add affordance arms the draft, the
/// typed title + the armed swatch commit the exact `CreateLabel` —
/// the subject board's id and the swatch's exact hex (the palette's
/// vocabulary).
#[test]
fn the_label_create_commit_routes_createlabel_with_the_swatch_hex() {
    on_slint_thread(|| {
        let (mut state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let board = fx.board;
        seed_labels(&mut state, board, a_id);
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::LabelsOpen);
        rig.handle.window().invoke_labels_overlay_add_requested();
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert!(rig.handle.window().get_label_overlay_form_open());
        // The empty title is the typed inert: the commit refuses,
        // dispatches nothing, the form stays open.
        rig.handle
            .window()
            .invoke_labels_overlay_title_edited(SharedString::from("  "));
        assert!(
            !rig.handle
                .window()
                .invoke_labels_overlay_form_commit_requested()
        );
        assert_eq!(rig.drain(), 0);
        assert!(rig.handle.window().get_label_overlay_form_open());
        // A swatch pick re-arms the color; the commit lands the hex.
        rig.handle.window().invoke_labels_overlay_swatch_clicked(2);
        rig.handle
            .window()
            .invoke_labels_overlay_title_edited(SharedString::from("urgent"));
        assert!(
            rig.handle
                .window()
                .invoke_labels_overlay_form_commit_requested()
        );
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert_eq!(
            rig.drain_commands(),
            vec![StateCommand::CreateLabel {
                board,
                title: "urgent".to_owned(),
                color: taskboard_domain::Color::new("e05a4e"),
            }],
        );
        assert!(
            !rig.handle.window().get_label_overlay_form_open(),
            "the commit closes the form"
        );
    });
}

// -- the board color picker (UX remediation chunk 4, D-1, #791) --

/// The armed triple (the r2 shape): arming mounts the strip with the
/// current color's ring; a swatch click dispatches the exact
/// `SetBoardColor` (the palette's hex) and closes; Escape's route
/// (the cancel) and a foreign settle dispatch nothing.
#[test]
fn the_board_color_strip_dispatches_the_exact_setboardcolor_and_disarms_clean() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let mut rig = rig(state);
        let adapter = rig.adapter();
        // The fixture's view is the board over the fixture board (the
        // normalization's selection), so the picker arms over it.
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::BoardColorEdit);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert!(rig.handle.window().get_board_color_strip_open());
        // A swatch click: the exact command + the strip closes.
        rig.handle.window().invoke_board_color_picked(1);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert_eq!(
            rig.drain_commands(),
            vec![StateCommand::SetBoardColor {
                board: fx.board,
                color: taskboard_domain::Color::new("3a70d6"),
            }],
        );
        assert!(
            !rig.handle.window().get_board_color_strip_open(),
            "the commit closes the strip"
        );
        // The dismissal (outside click's seam) without an armed flow
        // is the typed inert.
        rig.handle.window().invoke_board_color_strip_dismissed();
        assert_eq!(rig.drain(), 0);
        // The armed dismissal dispatches nothing (escape paths).
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::BoardColorEdit);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert!(rig.handle.window().get_board_color_strip_open());
        rig.handle.window().invoke_board_color_strip_dismissed();
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert_eq!(rig.drain(), 0);
        assert!(!rig.handle.window().get_board_color_strip_open());
        // The foreign settle (a begin intent lands over the armed
        // flow): settles the picker, then re-runs — no command.
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::BoardColorEdit);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert!(rig.handle.window().get_board_color_strip_open());
        adapter
            .borrow_mut()
            .apply_effect_for_test(rig.handle.window(), UiEffect::BeginCreateStack);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert_eq!(rig.drain(), 0);
        assert!(
            !rig.handle.window().get_board_color_strip_open(),
            "the foreign settle disarmed the picker"
        );
    });
}

// -- the empty-board preset gate (UX remediation chunk 4, D-5, #795) --

/// The key twin of the CTA: on an empty board the action dispatches
/// the explicit ordered `CreateStack` set (the r3 D5 assertion, now
/// routed through the action); on a non-empty board the dispatch-time
/// gate is the typed no-op — nothing drains (the R-3 law: a refresh
/// mid-stroke flips the verdict safely to inert).
#[test]
fn the_default_preset_action_dispatches_on_empty_and_is_the_typed_noop_on_content() {
    on_slint_thread(|| {
        // The empty-board leg: the fixture board stripped of stacks.
        let (mut state, fx) = fixture();
        state.stacks.clear();
        state.tasks.clear();
        let mut rig1 = rig(state);
        let adapter = rig1.adapter();
        adapter
            .borrow_mut()
            .interpret_for_test(rig1.handle.window(), UiEffect::BoardDefaultPreset);
        adapter.borrow_mut().refresh_forced(rig1.handle.window());
        let commands = rig1.drain_commands();
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
                    assert_eq!(*board, fx.board, "the selected board");
                    assert_eq!(t, *title, "table order titles");
                    assert_eq!(*o, i64::try_from(order).unwrap(), "explicit 0..n orders");
                }
                other => panic!("typed outcome expected, got {other:?}"),
            }
        }
        // The non-empty leg: the gate refuses, nothing drains.
        let (state, _) = fixture();
        let mut rig2 = rig(state);
        let adapter = rig2.adapter();
        adapter
            .borrow_mut()
            .interpret_for_test(rig2.handle.window(), UiEffect::BoardDefaultPreset);
        adapter.borrow_mut().refresh_forced(rig2.handle.window());
        assert_eq!(rig2.drain(), 0, "the gate's typed no-op dispatches nothing");
    });
}

// -- the target-free BoardSelect flow (UX remediation chunk 4, D-4, #794) --

/// The R-2 settle pins: the bare stroke opens the switcher flyout
/// (the facts global), zero commands drain; a row activation routes
/// the existing selection (a view intent — still zero commands, the
/// selection moves); the outside click's close (the facts-global
/// write the window backdrop performs) clears the request with
/// nothing dispatched, and the flow re-arms (the request protocol's
/// nonce advances).
#[test]
fn the_target_free_board_select_opens_the_flyout_walks_and_disarms_clean() {
    on_slint_thread(|| {
        let (state, _, _, _) = walk_fixture();
        let mut rig = rig(state);
        let adapter = rig.adapter();
        macro_rules! flyout_open {
            () => {{
                rig.handle
                    .window()
                    .global::<crate::BoardBarFacts>()
                    .get_open()
            }};
        }
        macro_rules! flyout_armed {
            () => {{
                rig.handle
                    .window()
                    .global::<crate::BoardBarFacts>()
                    .get_switcher_request()
            }};
        }
        macro_rules! highlight {
            () => {{
                rig.handle
                    .window()
                    .global::<crate::BoardBarFacts>()
                    .get_highlight()
            }};
        }
        let walk = |rig: &Rig, stroke: &str| {
            rig.adapter().borrow_mut().switcher_flyout_key(
                rig.handle.window(),
                crate::keymap::stroke::parse(stroke).expect("a ladder stroke"),
            )
        };
        // Open: the request arms, the panel mounts, zero commands.
        adapter
            .borrow_mut()
            .apply_effect_for_test(rig.handle.window(), UiEffect::OpenBoardBarFlyout);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert!(flyout_open!(), "the flyout opened on the bare stroke");
        assert!(flyout_armed!(), "the request is armed");
        assert_eq!(rig.drain(), 0, "opening dispatches nothing");
        // Walk: the highlight is anchored on the selection and moves
        // within the rows.
        let before = rig.handle.window().get_board_bar_model().selected;
        assert_eq!(highlight!(), before, "the anchor is the selection");
        walk(&rig, "Down");
        assert_ne!(highlight!(), before, "the Down arm moved the walk");
        // Return: the highlighted row's selection routes (the
        // segment seam's route); the flyout closes and disarms.
        let target = highlight!();
        walk(&rig, "Return");
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert_eq!(
            rig.handle.window().get_board_bar_model().selected,
            target,
            "the row activation selected the walked board"
        );
        assert!(!flyout_open!(), "the activation closed the flyout");
        assert!(!flyout_armed!());
        assert_eq!(rig.drain(), 0, "the selection is a view intent");
        // Escape: closes with nothing dispatched, disarmed.
        adapter
            .borrow_mut()
            .apply_effect_for_test(rig.handle.window(), UiEffect::OpenBoardBarFlyout);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert!(flyout_open!());
        walk(&rig, "Escape");
        assert!(!flyout_open!(), "Escape closes");
        assert!(!flyout_armed!(), "the request cleared");
        assert_eq!(rig.drain(), 0);
        // The outside click's underlay (the window backdrop's facts
        // write) + the next key's self-heal: the request flags fall in
        // line, nothing dispatches.
        adapter
            .borrow_mut()
            .apply_effect_for_test(rig.handle.window(), UiEffect::OpenBoardBarFlyout);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert!(flyout_open!());
        rig.handle
            .window()
            .global::<crate::DropdownFacts>()
            .set_open(false);
        walk(&rig, "Escape");
        assert!(!flyout_open!(), "the underlay close landed");
        assert!(!flyout_armed!(), "the request cleared via the self-heal");
        assert_eq!(rig.drain(), 0);
        // Re-arm: the flow re-opens after a clean close.
        adapter
            .borrow_mut()
            .apply_effect_for_test(rig.handle.window(), UiEffect::OpenBoardBarFlyout);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert!(flyout_open!(), "the flow re-arms after a clean close");
    });
}

// -- the today-row context menu's extra row (UX remediation chunk 4, D-8, #796) --

/// The structural composition pin: the today seam's menu includes
/// `task.due-today`; the board/stack cards' menus (the same helper,
/// no extras) do not — the verb binds `ContextId::Today` only, and
/// the today-only filter is what keeps the row honest there.
#[test]
fn the_today_context_menu_extends_with_due_today_and_the_card_menus_do_not() {
    use crate::actions::ActionId;
    on_slint_thread(|| {
        let (state, _) = fixture();
        let rig = rig(state);
        let adapter = rig.adapter();
        let ids_of = |rig: &Rig| -> Vec<String> {
            let rows = rig
                .handle
                .window()
                .global::<crate::KeyboardState>()
                .get_menu_entries();
            (0..rows.row_count())
                .filter_map(|i| rows.row_data(i))
                .map(|row| row.action.to_string())
                .collect()
        };
        // A board card's menu: the Task context rows, no due-today.
        adapter
            .borrow_mut()
            .open_task_context_menu(rig.handle.window(), 10.0, 10.0, &[], None);
        let card_ids = ids_of(&rig);
        assert!(
            !card_ids.iter().any(|id| id == "task.due-today"),
            "a card's menu never offers the today re-commit"
        );
        // The today row's menu: the same rows plus due-today.
        adapter.borrow_mut().open_task_context_menu(
            rig.handle.window(),
            10.0,
            10.0,
            &[ActionId::TaskDueToday],
            None,
        );
        let today_ids = ids_of(&rig);
        assert!(
            today_ids.iter().any(|id| id == "task.due-today"),
            "the today seam's menu offers the today re-commit"
        );
        assert_eq!(
            today_ids.len(),
            card_ids.len() + 1,
            "the extension is exactly the one verb"
        );
    });
}

/// The edit journey's sparse mask (D-2/D-9): the form seeds from the
/// row, and the commit sets only the changed slots — an untouched
/// color stays `None` in the `LabelChanges`; a no-op edit closes
/// without dispatching.
#[test]
fn the_label_edit_commit_routes_the_sparse_update() {
    on_slint_thread(|| {
        let (mut state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let (green, _red) = seed_labels(&mut state, fx.board, a_id);
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::LabelsOpen);
        // The edit arms seeded from the row.
        rig.handle.window().invoke_labels_overlay_edit_requested(0);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert_eq!(
            rig.handle.window().get_label_overlay_form_title(),
            "green",
            "the seed rides the pushed title"
        );
        // The no-op edit: commit closes the form, dispatches nothing.
        assert!(
            rig.handle
                .window()
                .invoke_labels_overlay_form_commit_requested()
        );
        assert_eq!(rig.drain(), 0);
        assert!(!rig.handle.window().get_label_overlay_form_open());
        // The title-only edit: the color slot stays `None`.
        rig.handle.window().invoke_labels_overlay_edit_requested(0);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        rig.handle
            .window()
            .invoke_labels_overlay_title_edited(SharedString::from("verde"));
        assert!(
            rig.handle
                .window()
                .invoke_labels_overlay_form_commit_requested()
        );
        assert_eq!(
            rig.drain_commands(),
            vec![StateCommand::UpdateLabel {
                id: green,
                changes: taskboard_domain::LabelChanges {
                    title: Some("verde".to_owned()),
                    color: None,
                },
            }],
        );
    });
}

/// The cancel discards the draft without dispatching (D-9); the
/// overlay stays open.
#[test]
fn the_label_form_cancel_discards_without_dispatching() {
    on_slint_thread(|| {
        let (mut state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        seed_labels(&mut state, fx.board, a_id);
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::LabelsOpen);
        rig.handle.window().invoke_labels_overlay_add_requested();
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        rig.handle.window().invoke_labels_overlay_form_cancelled();
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert_eq!(rig.drain(), 0);
        assert!(!rig.handle.window().get_label_overlay_form_open());
        assert!(
            matches!(adapter.borrow().view_state.flow(), Flow::Labels { .. }),
            "the overlay stays open"
        );
    });
}

// -- the armed label delete (UX remediation chunk 3, D-2/D-9, #789) --

/// The armed triple (D-9 — the r2
/// `the_delete_affordance_arms_then_confirms_and_dismissal_disarms`
/// shape verbatim): the first activation arms (the consequence strip
/// mounts, the armed mirror reads the row), nothing dispatches; the
/// Confirm dispatches the exact `DeleteLabel`; Cancel disarms
/// without dispatching. `DeleteLabel` is `NotUndoable(RestoreClass)`
/// — the staging is the r2 grammar's requirement.
#[test]
fn the_label_delete_arms_then_confirms_and_cancel_disarms() {
    on_slint_thread(|| {
        let (mut state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        let (green, _red) = seed_labels(&mut state, fx.board, a_id);
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::LabelsOpen);
        // First activation: arms, dispatches nothing.
        rig.handle.window().invoke_labels_overlay_delete_clicked(0);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert_eq!(rig.drain(), 0, "arming dispatches nothing");
        assert_eq!(rig.handle.window().get_label_overlay_armed_index(), 0);
        // The cancel disarms without dispatching.
        rig.handle.window().invoke_labels_overlay_delete_cancelled();
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert_eq!(rig.drain(), 0);
        assert_eq!(rig.handle.window().get_label_overlay_armed_index(), -1);
        // Re-arm; the second activation confirms the exact verb.
        rig.handle.window().invoke_labels_overlay_delete_clicked(0);
        rig.handle.window().invoke_labels_overlay_delete_clicked(0);
        assert_eq!(
            rig.drain_commands(),
            vec![StateCommand::DeleteLabel { id: green }],
            "the confirm dispatches DeleteLabel verbatim",
        );
    });
}

/// The ladder's settle rungs (D-2/D-9): Escape over an armed row
/// disarms it first (nothing dispatches, the overlay stays), and the
/// next Escape closes the overlay; an open form and an armed row are
/// mutually exclusive surfaces (arming one settles the other).
#[test]
fn the_labels_ladder_settles_the_armed_row_before_closing() {
    on_slint_thread(|| {
        let (mut state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        seed_labels(&mut state, fx.board, a_id);
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::LabelsOpen);
        // Arm the row, then arm the form: the form arm settles the
        // armed row (the mutual exclusivity).
        rig.handle.window().invoke_labels_overlay_delete_clicked(0);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        rig.handle.window().invoke_labels_overlay_add_requested();
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert!(rig.handle.window().get_label_overlay_form_open());
        assert_eq!(rig.handle.window().get_label_overlay_armed_index(), -1);
        // Escape rung 1: the form discards, the overlay stays.
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::LabelsDismiss);
        adapter.borrow_mut().refresh_forced(rig.handle.window());
        assert_eq!(rig.drain(), 0);
        assert!(!rig.handle.window().get_label_overlay_form_open());
        assert!(matches!(
            adapter.borrow().view_state.flow(),
            Flow::Labels { .. }
        ));
        // Escape rung 2: the overlay closes.
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::LabelsDismiss);
        assert_eq!(adapter.borrow().view_state.flow(), &Flow::None);
    });
}

/// The stale-arm absorb (D-9/R-2's law): a roster index outside the
/// pass's table dispatches nothing and changes nothing.
#[test]
fn a_stale_delete_index_dispatches_nothing() {
    on_slint_thread(|| {
        let (mut state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;
        seed_labels(&mut state, fx.board, a_id);
        let mut rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        adapter
            .borrow_mut()
            .interpret_for_test(rig.handle.window(), UiEffect::LabelsOpen);
        rig.handle.window().invoke_labels_overlay_delete_clicked(7);
        assert_eq!(rig.drain(), 0);
        assert_eq!(rig.handle.window().get_label_overlay_armed_index(), -1);
    });
}

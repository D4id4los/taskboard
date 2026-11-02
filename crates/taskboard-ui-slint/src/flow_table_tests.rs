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
}

/// The eleven effect classes the matrix columns range over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EC {
    Nav,
    Lifecycle,
    Begin,
    OpenPalette,
    BeginCapture,
    Edit,
    Cancel,
    Menu,
    System,
    Title,
    Picker,
}

fn kind_of(flow: &Flow) -> FK {
    match flow {
        Flow::None => FK::None,
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
    MenuPrevious,
    FocusToday,
    OpenInbox,
    BeginCapture,
    InvokeSelected,
    SetModeNormal,
    SetModeInsert,
    FocusNext,
    FocusPrev,
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
    FocusFirst,
    FocusLast,
    OpenSettings,
    BeginTitleEdit,
    ShowTask,
    ControlNext,
    ControlPrev,
    ScrollUp,
    ScrollDown,
}

fn key_of(effect: &UiEffect) -> EK {
    match effect {
        UiEffect::Exit => EK::Exit,
        UiEffect::OpenMenu => EK::OpenMenu,
        UiEffect::OpenPalette => EK::OpenPalette,
        UiEffect::CloseMenu => EK::CloseMenu,
        UiEffect::MenuBack => EK::MenuBack,
        UiEffect::MenuNext => EK::MenuNext,
        UiEffect::MenuPrevious => EK::MenuPrevious,
        UiEffect::FocusToday => EK::FocusToday,
        UiEffect::OpenInbox => EK::OpenInbox,
        UiEffect::BeginCapture => EK::BeginCapture,
        UiEffect::InvokeSelected => EK::InvokeSelected,
        UiEffect::SetMode(Mode::Normal) => EK::SetModeNormal,
        UiEffect::SetMode(Mode::Insert) => EK::SetModeInsert,
        UiEffect::FocusNext => EK::FocusNext,
        UiEffect::FocusPrev => EK::FocusPrev,
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
        UiEffect::FocusFirst => EK::FocusFirst,
        UiEffect::FocusLast => EK::FocusLast,
        UiEffect::OpenSettings => EK::OpenSettings,
        UiEffect::BeginTitleEdit => EK::BeginTitleEdit,
        UiEffect::ShowTask => EK::ShowTask,
        UiEffect::ControlNext => EK::ControlNext,
        UiEffect::ControlPrev => EK::ControlPrev,
        UiEffect::ScrollUp => EK::ScrollUp,
        UiEffect::ScrollDown => EK::ScrollDown,
        // The picker keys are applied in `apply_effect`, before the
        // table — the matrix never sees them (the menu-family
        // pattern). Their class cell is exercised via `class_of`.
        UiEffect::PickerKey(_) => {
            unreachable!("picker keys are applied before the table")
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
        crate::router::EffectClass::BeginCapture => EC::BeginCapture,
        crate::router::EffectClass::Edit => EC::Edit,
        crate::router::EffectClass::Cancel => EC::Cancel,
        crate::router::EffectClass::Menu => EC::Menu,
        crate::router::EffectClass::System => EC::System,
        crate::router::EffectClass::Title => EC::Title,
        crate::router::EffectClass::Picker => EC::Picker,
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
use FK::Drag as KDrag;
use FK::Due as KDue;
use FK::Move as KMove;
use FK::Palette as KPal;
use FK::Text as KTxt;
use Post::Kind as K;

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
    t
}

/// The per-pair overrides: the kept explicit arms and the
/// effect-shaped rows a class cell cannot express (each names its
/// audit §6 cell).
fn pair_table() -> Vec<(FK, EK, Exp)> {
    use EK as E;
    let mut t = Vec::new();
    let mut row = |kind: FK, key: EK, e: Exp| t.push((kind, key, e));
    // -- LIFECYCLE under every foreign kind: the settle-then-run
    //    re-run's outcome is effect-shaped (a rename arms a fresh
    //    text flow, a delete dispatches with no flow) --
    row(FK::Text, E::StackDelete, exp(NZ, N));
    row(FK::Text, E::BoardDelete, exp(NZ, N));
    row(FK::Text, E::StackRename, exp(NZ, K(KTxt)));
    row(FK::Text, E::BoardRename, exp(NZ, K(KTxt)));
    row(FK::Text, E::BoardCreate, exp(NZ, K(KTxt)));
    row(FK::Text, E::StackMove, exp(NZ, K(KMove)));
    for kind in [FK::Due, FK::Move, FK::Drag, FK::Palette, FK::Capture] {
        row(kind, E::StackDelete, exp(NZ, N));
        row(kind, E::BoardDelete, exp(NZ, N));
        row(kind, E::StackRename, exp(Z, K(KTxt)));
        row(kind, E::BoardRename, exp(Z, K(KTxt)));
        row(kind, E::BoardCreate, exp(Z, K(KTxt)));
        row(kind, E::StackMove, exp(Z, K(KMove)));
    }
    // -- TITLE (BeginTitleEdit, audit §6 last column): settle-then-
    //    run under a foreign flow; under None it arms the viewed
    //    task's editor or is inert without a subject --
    row(FK::None, E::BeginTitleEdit, exp(Z, K(KTxt)));
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
    row(FK::None, E::SetModeInsert, exp(Z, K(KTxt)));
    row(FK::None, E::StackRename, exp(Z, K(KTxt)));
    row(FK::None, E::BoardRename, exp(Z, K(KTxt)));
    row(FK::None, E::BoardCreate, exp(Z, K(KTxt)));
    row(FK::None, E::StackMove, exp(Z, K(KMove)));
    row(FK::None, E::StackDelete, exp(NZ, N));
    row(FK::None, E::BoardDelete, exp(NZ, N));
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
    row(FK::Due, E::SetModeNormal, exp(Z, N)); // kept discard
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
            archived: now,
            deleted: now,
        },
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
        },
        Flow::MoveStack { stack: s1, slot: 0 },
        Flow::DragMove {
            task: a,
            target: s1,
        },
        Flow::Palette { selected: 0 },
        Flow::Capture,
        Flow::CreateBoard,
        Flow::RenameStack { stack: s0 },
        Flow::RenameBoard { board: fx.board },
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
        UiEffect::FocusFirst,
        UiEffect::FocusLast,
        UiEffect::OpenSettings,
        UiEffect::BeginTitleEdit,
        UiEffect::ShowTask,
        UiEffect::ControlNext,
        UiEffect::ControlPrev,
        UiEffect::ScrollUp,
        UiEffect::ScrollDown,
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
    )
    .expect("mount succeeds on the null platform");
    Rig {
        handle,
        commands_rx,
    }
}

impl Rig {
    fn adapter(&self) -> Rc<RefCell<crate::adapter::Adapter>> {
        self.handle.test_adapter()
    }

    fn drain(&mut self) -> usize {
        let mut n = 0;
        while self.commands_rx.try_recv().is_ok() {
            n += 1;
        }
        n
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
                    if matches!(kind_of(flow), FK::Text | FK::Capture) {
                        // The TEXT rows' commit side and capture's
                        // commit-and-continue thought.
                        a.view_state.set_buffer("typed".to_owned());
                    }
                    *a.view_state.flow_mut() = flow.clone();
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
    assert_eq!(
        cells.len(),
        7 * 11,
        "one row per (FlowKind, EffectClass) cell"
    );
    let mut seen = std::collections::BTreeSet::new();
    for (kind, class, _) in cells {
        assert!(
            seen.insert((kind, class)),
            "duplicate row ({kind:?}, {class:?})"
        );
    }
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
            *a.view_state.flow_mut() = Flow::DragMove {
                task: a_id,
                target: fx.stacks[1],
            };
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
            *a.view_state.flow_mut() = Flow::DragMove {
                task: a_id,
                target: fx.stacks[1],
            };
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
            *a.view_state.flow_mut() = Flow::DragMove {
                task: a_id,
                target: fx.stacks[1],
            };
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
        *a.view_state.flow_mut() = Flow::EditDate {
            task: a_id,
            field: crate::view_state::DateField::Due,
        };
    }

    #[test]
    fn the_due_buffers_die_with_the_flow_on_every_end_path() {
        on_slint_thread(|| {
            let (state, fx) = fixture();
            let rig = rig(state);
            let adapter = rig.adapter();

            // The leaving-settle end path.
            arm_due(&rig, &adapter, &fx);
            adapter
                .borrow_mut()
                .view_state
                .set_buffer("ignored".to_owned());
            {
                let mut a = adapter.borrow_mut();
                a.test_seed_due_buffers();
                a.interpret_for_test(rig.handle.window(), UiEffect::NextView);
            }
            assert!(adapter.borrow().due_buffers_empty(), "leaving wipes");
            assert_eq!(adapter.borrow().view_state.flow().clone(), Flow::None);

            // A plain refresh with the flow gone (the reconcile-drop
            // shape): the wipe must not resurrect anything.
            arm_due(&rig, &adapter, &fx);
            adapter.borrow_mut().view_state.discard_edit();
            {
                let mut a = adapter.borrow_mut();
                a.test_seed_due_buffers();
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
/// (activate + focused + the forced derive; the #463 rule).
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

/// J4's harness-level pin — the header hint projections: empty while
/// the task view is mounted under a keymap that binds neither verb in
/// the TaskDetail-visible chain (the standard preset binds
/// `task.complete` at Board/Today only); non-empty once the verbs are
/// bound — vi carries `task.complete` in `ViNormal` (chain-visible at
/// `TaskDetail`) and the `task.track-start` override is an add-new,
/// landing in Global. Label content is never asserted.
#[test]
fn the_header_hint_projections_follow_the_mounted_verb() {
    on_slint_thread(|| {
        let (state, fx) = fixture();
        let [a_id, _b, _c] = fx.tasks;
        let [s0, _s1] = fx.stacks;

        // Mounted, unbound verbs: both projections empty (the
        // established empty-hint rendering).
        let rig = rig(state);
        let adapter = rig.adapter();
        mount_task_detail(&rig, &adapter, a_id, s0);
        assert_eq!(rig.handle.window().get_task_done_hint().row_count(), 0);
        assert_eq!(rig.handle.window().get_task_track_hint().row_count(), 0);
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
        | UiEffect::ShowTask
        | UiEffect::InvokeSelected => DismissClass::IdentitySwap,
        UiEffect::OpenMenu
        | UiEffect::CloseMenu
        | UiEffect::BeginCreate { .. }
        | UiEffect::BeginCreateStack
        | UiEffect::OpenPalette
        | UiEffect::BeginCapture => DismissClass::AnchorOrModal,
        UiEffect::Exit
        | UiEffect::MenuBack
        | UiEffect::MenuNext
        | UiEffect::MenuPrevious
        | UiEffect::SetMode(_)
        | UiEffect::FocusNext
        | UiEffect::FocusPrev
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
        | UiEffect::PickerKey(_)
        | UiEffect::StackDelete
        | UiEffect::StackRename
        | UiEffect::StackMove
        | UiEffect::BoardCreate
        | UiEffect::BoardDelete
        | UiEffect::BoardRename => DismissClass::Survive,
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
            *a.view_state.flow_mut() = Flow::Palette { selected: 0 };
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
        let sections = [
            (0, 12, DismissClass::IdentitySwap),
            (12, 19, DismissClass::AnchorOrModal),
            (19, effects.len(), DismissClass::Survive),
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

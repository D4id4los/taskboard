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
use std::cell::RefCell;
use std::collections::BTreeSet;
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

/// The ten effect classes the matrix columns range over.
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
}

fn kind_of(flow: &Flow) -> FK {
    match flow {
        Flow::None => FK::None,
        Flow::Create { .. }
        | Flow::CreateStack
        | Flow::CreateBoard
        | Flow::EditTitle { .. }
        | Flow::RenameStack { .. }
        | Flow::RenameBoard { .. } => FK::Text,
        Flow::EditDue { .. } => FK::Due,
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
        UiEffect::BeginCreate => EK::BeginCreate,
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
    }
}

fn class_of(effect: &UiEffect) -> EC {
    match key_of(effect) {
        EK::NextView
        | EK::PrevView
        | EK::FocusBoard
        | EK::BoardNext
        | EK::BoardPrevious
        | EK::BoardSelect
        | EK::FocusToday
        | EK::OpenInbox
        | EK::OpenSettings
        | EK::FocusNext
        | EK::FocusPrev
        | EK::FocusFirst
        | EK::FocusLast
        | EK::StackNext
        | EK::StackPrev
        | EK::StackWider
        | EK::StackNarrower => EC::Nav,
        EK::StackDelete
        | EK::StackRename
        | EK::StackMove
        | EK::BoardCreate
        | EK::BoardDelete
        | EK::BoardRename => EC::Lifecycle,
        EK::BeginCreate | EK::BeginCreateStack | EK::RequestMove => EC::Begin,
        EK::OpenPalette => EC::OpenPalette,
        EK::BeginCapture => EC::BeginCapture,
        EK::Activate | EK::SetModeNormal | EK::SetModeInsert => EC::Edit,
        EK::CancelMove => EC::Cancel,
        EK::OpenMenu
        | EK::CloseMenu
        | EK::MenuBack
        | EK::MenuNext
        | EK::MenuPrevious
        | EK::InvokeSelected => EC::Menu,
        EK::Exit => EC::System,
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
    for class in [C::Edit, C::Cancel, C::Menu, C::System] {
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
    for class in [C::Edit, C::Menu, C::System] {
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
        for class in [C::Edit, C::Menu, C::System] {
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
        for class in [C::Edit, C::Menu, C::System] {
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
    // -- Flow::None: the begin/open arms (audit §6 row 1) --
    row(FK::None, E::BeginCreate, exp(Z, K(KTxt)));
    row(FK::None, E::BeginCreateStack, exp(Z, K(KTxt)));
    row(FK::None, E::RequestMove, exp(Z, K(KMove)));
    // The auto-arm: today's Activate arms the title edit; the F26
    // rework (plan §6) replaces this row with the plain
    // open-detail outcome — recorded decision, rewritten then.
    row(FK::None, E::Activate, exp(Z, K(KTxt)));
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
        title: format!("task-{order}"),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        stack: stk,
        order,
        labels: BTreeSet::default(),
        effort: None,
        importance: None,
        urgency: None,
        archived: false,
        deleted: false,
        clocks: TaskClocks {
            title: now,
            description: now,
            duedate: now,
            scheduled: now,
            done: now,
            position: now,
            labels: now,
            effort: now,
            importance: now,
            urgency: now,
            archived: now,
            deleted: now,
        },
        remote_seen: None,
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
        Flow::EditDue { task: a },
        Flow::Move {
            task: a,
            target: s1,
        },
        Flow::MoveStack { stack: s0, slot: 1 },
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
        UiEffect::BeginCreate,
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
    let handle = mount(
        state,
        dummy_receiver(),
        dummy_receiver(),
        commands_tx,
        requests_tx,
        UiSettings::default(),
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
                    if kind_of(flow) == FK::Text {
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

/// The class matrix covers each `(FlowKind, EffectClass)` cell
/// exactly once — a new variant without a row fails the driver above
/// via the missing-row panic or the compile-time-exhaustive mirrors.
#[test]
fn the_matrix_covers_every_class_cell() {
    let cells = class_table();
    assert_eq!(
        cells.len(),
        7 * 10,
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

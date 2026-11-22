// SPDX-License-Identifier: AGPL-3.0-only

//! The board-ceremony journeys (O-5 parity chunk OP1 — the §6 wiring
//! table's nominated evidence file `board_ceremonies_journeys.rs`):
//! the three GUI board-ceremony verbs (archive US-1 #945, reset US-2
//! #946, reconcile US-3 #947) as informed-consent confirmations. The
//! r1 erratum law holds throughout: the verbs stay
//! `NotUndoable(BoardCeremony)` engine-side, the letters carry the
//! blast radius before the confirm routes, and no test asserts letter
//! text — the oracles are the letter's typed fold enum + derivation
//! inputs and the exact commands that reach the fake-engine boundary.

use crate::support;

use support::driver::{DriverOptions, JourneyDriver as _, SlintDriver};
use support::fake_engine::FakeEngine;
use support::fixture;
use support::health::detached_health;

use arc_swap::ArcSwap;
use slint::ComponentHandle as _;
use slint::Model as _;
use std::sync::Arc;
use taskboard_domain::ReconcileCounts;
use taskboard_domain::StateCommand;
use taskboard_ui_slint::CeremonyLetterMode;
use taskboard_ui_slint::UiSettings;
use taskboard_ui_slint::actions::ActionId;
use tokio::sync::mpsc;

/// The ceremony-journey harness: driver + fake engine + board-mounted
/// window, keeping the request lane's receiver half — the test plays
/// the app half of the reconcile preview (the
/// [`UiHandle::drain_setting_replies`] seam pattern).
struct CeremonyHarness {
    driver: SlintDriver,
    handle: taskboard_ui_slint::UiHandle,
    engine: FakeEngine,
    requests_rx: mpsc::Receiver<taskboard_ui_slint::requests::UiRequest>,
}

fn dummy_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::EngineSignal> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

fn dummy_system_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

impl CeremonyHarness {
    /// Mount over `state` with the Standard keymap, bind the driver,
    /// and escape to the board (the shared journey preamble — the
    /// ceremonies' rows act on the selected board).
    fn mount(state: taskboard_domain::AppState, kiosk: bool) -> Self {
        let settings = UiSettings {
            kiosk,
            ..UiSettings::default()
        };
        let state = Arc::new(ArcSwap::from_pointee(state));
        let (commands, engine) = FakeEngine::channel(Arc::clone(&state));
        let (requests_tx, requests_rx) = mpsc::channel(16);
        let handle = taskboard_ui_slint::mount(
            state,
            dummy_receiver(),
            dummy_system_receiver(),
            commands,
            requests_tx,
            settings,
            detached_health(),
            None,
        )
        .expect("mount succeeds on the testing backend");
        let mut harness = Self {
            driver: SlintDriver::default(),
            handle,
            engine,
            requests_rx,
        };
        harness.driver.bind(&harness.handle);
        if !kiosk {
            // The desktop default is Today; the ceremonies act on the
            // selected board (routed, never preset-bound).
            harness.dispatch(ActionId::NavFocusBoard);
        }
        harness
    }

    fn dispatch(&mut self, action: ActionId) {
        self.handle.dispatch_action(action);
        self.advance();
    }

    fn advance(&mut self) {
        self.driver.advance_time(0);
    }

    /// The armed letter's typed fold (the tests' letter oracle: mode
    /// + derivation inputs, never text).
    fn letter(&self) -> taskboard_ui_slint::CeremonyLetterModel {
        self.handle.window().get_ceremony_letter()
    }

    /// The strip's confirm seam (the pointer twin of the invoking
    /// verb's second stroke — click ≡ key at every step).
    fn confirm(&mut self) {
        self.handle.window().invoke_board_ceremony_confirmed();
        self.advance();
    }

    /// The strip's cancel seam (the disarm, nothing dispatched).
    fn cancel(&mut self) {
        self.handle.window().invoke_board_ceremony_cancelled();
        self.advance();
    }

    /// Takes one drained UI request (the app half's read seam).
    fn next_request(&mut self) -> Option<taskboard_ui_slint::requests::UiRequest> {
        self.requests_rx.try_recv().ok()
    }
}

/// The fixture board's id (the armed subject's address).
fn fixture_board(state: &taskboard_domain::AppState) -> taskboard_domain::BoardId {
    *state.boards.keys().next().expect("fixture board")
}

/// Runs one ceremony journey leg over the golden fixture.
fn ceremony_journey(body: impl FnOnce(taskboard_domain::AppState) + Send + 'static) {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            body(state);
        },
    );
}

// -- US-1: archive (#945) ------------------------------------------------

/// US-1/S2 (W-2 fold, D-1): the archive arm mounts the consent
/// caption — the letter's mode is the `ArchiveCaption` fold and the
/// no-AppUndo flag reads `false` (the letter never offers an undo
/// affordance; the typed escape is the re-archive path, R-3) — and
/// the arm dispatches nothing.
#[test]
fn us1_s2_archive_caption_carries_rearchive_escape_fact() {
    ceremony_journey(|state| {
        let board = fixture_board(&state);
        let mut harness = CeremonyHarness::mount(state, false);
        harness.dispatch(ActionId::BoardArchive);
        let letter = harness.letter();
        assert_eq!(letter.mode, CeremonyLetterMode::ArchiveCaption);
        assert!(!letter.undo_offered, "no AppUndo affordance is offered");
        harness
            .engine
            .drain()
            .is_empty()
            .then_some(())
            .expect("arming dispatches nothing");
        let _ = board;
    });
}

/// US-1/S3 (W-1 seam, AC-1): the archive confirm routes exactly one
/// `SetBoardArchived { archived: true }` over the armed subject's
/// board (click ≡ key: the strip's Confirm seam and the invoking
/// verb's repeat both land here); the strip unmounts after.
#[test]
fn us1_s3_archive_row_dispatches_set_board_archived() {
    ceremony_journey(|state| {
        let board = fixture_board(&state);
        let mut harness = CeremonyHarness::mount(state, false);
        harness.dispatch(ActionId::BoardArchive);
        harness.confirm();
        match harness.engine.drain().as_slice() {
            [
                StateCommand::SetBoardArchived {
                    board: subject,
                    archived: true,
                },
            ] => assert_eq!(*subject, board),
            other => panic!("the confirm routes the exact verb once, got {other:?}"),
        }
        assert_eq!(harness.letter().mode, CeremonyLetterMode::None);
    });
}

/// US-1/S3's escape (D-1): Cancel disarms with nothing dispatched —
/// the consent scaffolding, never a command.
#[test]
fn us1_s3_archive_cancel_disarms_without_dispatching() {
    ceremony_journey(|state| {
        let mut harness = CeremonyHarness::mount(state, false);
        harness.dispatch(ActionId::BoardArchive);
        harness.cancel();
        assert_eq!(harness.letter().mode, CeremonyLetterMode::None);
        assert!(
            harness.engine.drain().is_empty(),
            "the disarm dispatches nothing"
        );
    });
}

// -- US-2: reset (#946) --------------------------------------------------

/// US-2/S1 (W-3): the reset arm mounts the armed strip — the letter's
/// mode is the `ResetCounts` fold, nothing dispatches.
#[test]
fn us2_s1_reset_row_arms_confirm_board_reset() {
    ceremony_journey(|state| {
        let mut harness = CeremonyHarness::mount(state, false);
        harness.dispatch(ActionId::BoardReset);
        assert_eq!(harness.letter().mode, CeremonyLetterMode::ResetCounts);
        assert!(
            harness.engine.drain().is_empty(),
            "arming dispatches nothing"
        );
    });
}

/// US-2/S2 (W-4, D-3, AC-1's letter half): the reset letter derives
/// the live model counts — the fixture board's two stacks and three
/// tasks (zero labels) read straight off the pushed letter's typed
/// inputs.
#[test]
fn us2_s2_reset_letter_derives_live_counts() {
    ceremony_journey(|state| {
        let mut harness = CeremonyHarness::mount(state, false);
        harness.dispatch(ActionId::BoardReset);
        let letter = harness.letter();
        assert_eq!(letter.stacks, 2, "the fixture board's live stacks");
        assert_eq!(letter.tasks, 3, "the fixture board's live tasks");
        assert_eq!(letter.labels, 0, "the fixture board's live labels");
    });
}

/// US-2/S3 (W-5 seam, AC-2): the reset confirm routes exactly one
/// `ResetBoard { board }` — and exactly once under a double confirm
/// (the flow cleared after the first).
#[test]
fn us2_s3_reset_confirm_dispatches_exactly_once() {
    ceremony_journey(|state| {
        let board = fixture_board(&state);
        let mut harness = CeremonyHarness::mount(state, false);
        harness.dispatch(ActionId::BoardReset);
        harness.confirm();
        match harness.engine.drain().as_slice() {
            [StateCommand::ResetBoard { board: subject }] => assert_eq!(*subject, board),
            other => panic!("the confirm routes the exact verb once, got {other:?}"),
        }
        assert_eq!(harness.letter().mode, CeremonyLetterMode::None);
        // A repeat confirm over the cleared flow is the typed no-op.
        harness.confirm();
        assert!(harness.engine.drain().is_empty(), "nothing dispatched");
    });
}

/// US-2 AC-3 (W-5 journey cell): Cancel / the foreign settle disarm
/// with nothing dispatched — the armed-delete close shape verbatim
/// (opening the create flow is the foreign settle here).
#[test]
fn us2_s3_escape_and_foreign_settle_disarm_without_dispatching() {
    ceremony_journey(|state| {
        let mut harness = CeremonyHarness::mount(state, false);
        harness.dispatch(ActionId::BoardReset);
        harness.cancel();
        assert_eq!(harness.letter().mode, CeremonyLetterMode::None);
        // The foreign settle: a new flow replaces the armed strip —
        // disarm, nothing dispatched.
        harness.dispatch(ActionId::BoardReset);
        harness.dispatch(ActionId::BoardCreate);
        assert_eq!(harness.letter().mode, CeremonyLetterMode::None);
        assert!(harness.engine.drain().is_empty(), "nothing dispatched");
    });
}

// -- US-3: reconcile (#947) ----------------------------------------------

/// The app half's answer shape — the three reply outcomes the drain
/// can observe (the test plays the app half).
#[derive(Clone, Copy)]
enum PreviewAnswer {
    /// The planner's counts (the letter's derivation inputs).
    Counts(ReconcileCounts),
    /// The not-derivable fact: the state read failed, the planner
    /// never ran — no counts exist to consent to.
    NotDerivable,
    /// The dead lane (the reply dropped unanswered).
    DeadLane,
}

/// Drives the reconcile arm and answers the preview request with
/// `answer` (the test plays the app half); returns the harness and
/// the request's board.
fn arm_reconcile_and_reply(
    state: taskboard_domain::AppState,
    answer: PreviewAnswer,
) -> (CeremonyHarness, taskboard_domain::BoardId) {
    let board = fixture_board(&state);
    let mut harness = CeremonyHarness::mount(state, false);
    harness.dispatch(ActionId::BoardReconcile);
    match harness.next_request() {
        Some(taskboard_ui_slint::requests::UiRequest::BoardReconcilePreview {
            board: subject,
            reply,
        }) => {
            assert_eq!(subject, board);
            match answer {
                PreviewAnswer::Counts(counts) => {
                    let _ = reply.send(Some(counts));
                }
                PreviewAnswer::NotDerivable => {
                    let _ = reply.send(None);
                }
                PreviewAnswer::DeadLane => drop(reply),
            }
        }
        other => panic!("the arm requests the preview, got {other:?}"),
    }
    harness.handle.drain_reconcile_preview();
    harness.advance();
    (harness, board)
}

/// US-3/S1 (W-6): the reconcile arm requests the oneshot preview and
/// arms the strip (the letter reads `ReconcilePending` before the
/// counts reply lands — the staging law: an unlettered confirm is
/// refused, and the arm dispatches no engine command).
#[test]
fn us3_s1_reconcile_row_requests_preview_and_arms() {
    ceremony_journey(|state| {
        let mut harness = CeremonyHarness::mount(state, false);
        harness.dispatch(ActionId::BoardReconcile);
        assert!(
            matches!(
                harness.next_request(),
                Some(taskboard_ui_slint::requests::UiRequest::BoardReconcilePreview { .. })
            ),
            "the arm requests the preview"
        );
        assert_eq!(harness.letter().mode, CeremonyLetterMode::ReconcilePending);
        assert!(
            harness.engine.drain().is_empty(),
            "arming dispatches no engine command"
        );
        // The pending preview's confirm is the typed no-op (D-4's
        // staging law — no confirm against an unseen letter).
        harness.confirm();
        assert!(harness.engine.drain().is_empty(), "nothing dispatched");
    });
}

/// US-3/S1 (W-9, D-4): the settled reply feeds the letter (the mode
/// flips to `ReconcilePreview`); the dead lane is the typed failure
/// fact that disarms the strip with nothing dispatched (US-3/S1's
/// escape cell). The Escape discard leg rides the disarm.
#[test]
fn us3_s1_preview_reply_feeds_letter_or_disarms() {
    ceremony_journey(|state| {
        // The settled reply: the counts land in the letter.
        let counts = ReconcileCounts {
            stacks: 1,
            labels: 2,
            tasks: 3,
            cancelled_ops: 4,
        };
        let (mut harness, _board) =
            arm_reconcile_and_reply(state.clone(), PreviewAnswer::Counts(counts));
        assert_eq!(harness.letter().mode, CeremonyLetterMode::ReconcilePreview);
        assert!(
            harness.engine.drain().is_empty(),
            "the reply lands counts, no engine command"
        );

        // The dead lane (a fresh mount): the strip disarms.
        let (mut harness, _board) = arm_reconcile_and_reply(state.clone(), PreviewAnswer::DeadLane);
        assert_eq!(harness.letter().mode, CeremonyLetterMode::None);
        assert!(harness.engine.drain().is_empty(), "nothing dispatched");

        // The not-derivable reply (the state read failed): the strip
        // disarms — a failed planner never masquerades as a zero-count
        // letter (US-3/S1's escape cell: the failed reply is the typed
        // fact, and no confirm routes against an unseen letter).
        let (mut harness, _board) = arm_reconcile_and_reply(state, PreviewAnswer::NotDerivable);
        assert_eq!(harness.letter().mode, CeremonyLetterMode::None);
        assert!(harness.engine.drain().is_empty(), "nothing dispatched");
    });
}

/// US-3/S2 (W-7 fold, D-4): the preview letter carries the planner's
/// match counts as its typed derivation inputs — the merge pairs and
/// the cancelled ops, zero counts included (AC-4's "letter says so"
/// is the zero-count value, never text).
#[test]
fn us3_s2_reconcile_letter_carries_preview_counts() {
    ceremony_journey(|state| {
        let counts = ReconcileCounts {
            stacks: 1,
            labels: 2,
            tasks: 3,
            cancelled_ops: 4,
        };
        let (harness, _board) =
            arm_reconcile_and_reply(state.clone(), PreviewAnswer::Counts(counts));
        let letter = harness.letter();
        assert_eq!(letter.merge_stacks, 1);
        assert_eq!(letter.merge_tasks, 3);
        assert_eq!(letter.merge_labels, 2);
        assert_eq!(letter.cancelled_ops, 4);
        assert!(!letter.undo_offered, "no AppUndo affordance is offered");

        // The zero-count preview: the letter carries the zeros (the
        // idempotent no-op's typed fact).
        let (harness, _board) =
            arm_reconcile_and_reply(state, PreviewAnswer::Counts(ReconcileCounts::default()));
        let letter = harness.letter();
        assert_eq!(letter.mode, CeremonyLetterMode::ReconcilePreview);
        assert_eq!(letter.merge_stacks, 0);
        assert_eq!(letter.merge_tasks, 0);
        assert_eq!(letter.merge_labels, 0);
    });
}

/// US-3/S3 (W-8 seam, AC-2): the reconcile confirm routes exactly one
/// `ReconcileByTitle { board }` — the zero-count preview included
/// (AC-4: the idempotent second pass is the accepted no-op, the
/// confirm routes anyway).
#[test]
fn us3_s3_reconcile_confirm_dispatches_exactly_once() {
    ceremony_journey(|state| {
        let board = fixture_board(&state);
        // The zero-count preview still routes (AC-4).
        let (mut harness, _board) =
            arm_reconcile_and_reply(state, PreviewAnswer::Counts(ReconcileCounts::default()));
        harness.confirm();
        match harness.engine.drain().as_slice() {
            [StateCommand::ReconcileByTitle { board: subject }] => assert_eq!(*subject, board),
            other => panic!("the confirm routes the exact verb once, got {other:?}"),
        }
        assert_eq!(harness.letter().mode, CeremonyLetterMode::None);
    });
}

/// US-3 AC-3 (W-8 journey cell): the armed strip's Cancel (Escape /
/// outside click's seam twin) disarms with nothing dispatched and the
/// in-flight preview is discarded — a reply arriving after the disarm
/// is the discarded no-op.
#[test]
fn us3_s3_reconcile_cancel_discards_the_preview() {
    ceremony_journey(|state| {
        let mut harness = CeremonyHarness::mount(state, false);
        harness.dispatch(ActionId::BoardReconcile);
        // Take the request but never answer it (the discard leg).
        let _request = harness.next_request();
        harness.cancel();
        assert_eq!(harness.letter().mode, CeremonyLetterMode::None);
        assert!(harness.engine.drain().is_empty(), "nothing dispatched");
        // The drained preview drain over a flowless slot is silent.
        harness.handle.drain_reconcile_preview();
        assert!(harness.engine.drain().is_empty(), "nothing dispatched");
    });
}

/// US-1/S1 (W-1 journey cell): the `Archive` row sits in the board
/// context menu's destructive tail, enabled for a live board — the
/// menu-row projection (US-1's menu-open leg; click-away closes with
/// nothing dispatched).
#[test]
fn us1_s1_the_archive_row_sits_enabled_for_a_live_board() {
    ceremony_journey(|state| {
        let mut harness = CeremonyHarness::mount(state, false);
        // The board-bar segment's context seam opens the board menu.
        harness
            .handle
            .window()
            .invoke_board_segment_context_requested(0, 120.0, 120.0);
        harness.advance();
        let keyboard = harness
            .handle
            .window()
            .global::<taskboard_ui_slint::KeyboardState>();
        assert!(keyboard.get_menu_open(), "the board context menu opens");
        let entries = keyboard.get_menu_entries();
        let archive = (0..entries.row_count())
            .map(|i| entries.row_data(i).expect("row"))
            .find(|row| row.action == "board.archive")
            .expect("the Archive row renders in the board context menu");
        assert!(archive.enabled, "the row is enabled for a live board");
        // Click-away (the menu's dismissal seam) dispatches nothing.
        harness.dispatch(ActionId::MenuDismiss);
        assert!(harness.engine.drain().is_empty(), "nothing dispatched");
    });
}

/// US-1/S4 (W-1 journey cell): the archived-boards roster's restore
/// row is the archive verb's typed escape — activating a roster row
/// routes the exact `SetBoardArchived { archived: false }` and the
/// board returns live (US-11's landed affordance; this journey pins
/// only its role as the `NotUndoable` verb's escape target).
#[test]
fn us1_s4_the_roster_restore_is_the_typed_escape() {
    ceremony_journey(|state| {
        // The pre-archived leg: the fixture board sits in the roster.
        let mut state = state;
        let board = fixture_board(&state);
        state
            .boards
            .get_mut(&board)
            .expect("fixture board")
            .archived = true;
        let mut harness = CeremonyHarness::mount(state, false);
        let bar = harness.handle.window().get_board_bar_model();
        assert_eq!(
            bar.archived_rows.row_count(),
            1,
            "the archived board joins the roster"
        );
        // The roster's restore row routes the exact unarchive.
        harness
            .handle
            .window()
            .invoke_board_bar_archived_activated(0);
        harness.advance();
        match harness.engine.drain().as_slice() {
            [
                StateCommand::SetBoardArchived {
                    board: subject,
                    archived: false,
                },
            ] => assert_eq!(*subject, board),
            other => panic!("the roster restore routes the exact escape, got {other:?}"),
        }
    });
}

/// US-2/S4 (W-4 fold, AC-4): the reset letter's typed fold carries the
/// survival facts — the mode is the `ResetCounts` fold (the board row
/// and connect session survive, the re-materialization fact) and the
/// no-AppUndo flag reads `false` (R-3: no undo affordance is offered
/// on the post-reset surface).
#[test]
fn us2_s4_the_reset_letter_carries_the_survival_facts() {
    ceremony_journey(|state| {
        let mut harness = CeremonyHarness::mount(state, false);
        harness.dispatch(ActionId::BoardReset);
        let letter = harness.letter();
        assert_eq!(letter.mode, CeremonyLetterMode::ResetCounts);
        assert!(!letter.undo_offered, "no AppUndo affordance is offered");
        harness.cancel();
        assert!(harness.engine.drain().is_empty(), "nothing dispatched");
    });
}

/// US-3/S4 (W-8 journey cell): the second reconcile pass routes the
/// accepted idempotent no-op — a full arm/preview/confirm cycle again
/// after the first dispatches a second `ReconcileByTitle`; the engine
/// absorbs the repeat as the typed `NoOp` (AC-4's zero-count letter
/// rides the second arm's preview).
#[test]
fn us3_s4_the_second_reconcile_pass_routes_the_accepted_no_op() {
    ceremony_journey(|state| {
        let board = fixture_board(&state);
        // The first pass: real counts, the merge routes.
        let (mut harness, _board) = arm_reconcile_and_reply(
            state.clone(),
            PreviewAnswer::Counts(ReconcileCounts {
                stacks: 1,
                labels: 0,
                tasks: 2,
                cancelled_ops: 1,
            }),
        );
        harness.confirm();
        // The second pass: the zero-count preview routes anyway (the
        // accepted no-op), exactly once.
        harness.dispatch(ActionId::BoardReconcile);
        match harness.next_request() {
            Some(taskboard_ui_slint::requests::UiRequest::BoardReconcilePreview {
                board: subject,
                reply,
            }) => {
                assert_eq!(subject, board);
                let _ = reply.send(Some(ReconcileCounts::default()));
            }
            other => panic!("the second arm requests the preview, got {other:?}"),
        }
        harness.handle.drain_reconcile_preview();
        harness.advance();
        harness.confirm();
        let drained = harness.engine.drain();
        match drained.as_slice() {
            [
                StateCommand::ReconcileByTitle { .. },
                StateCommand::ReconcileByTitle { .. },
            ] => {}
            other => panic!("both passes route the verb once each, got {other:?}"),
        }
    });
}

/// D-5's structural gate (US-1/US-2/US-3, the platform posture): the
/// ceremony verbs are absent on kiosk — the board surface does not
/// mount there (board-stack plan PC-4 §6), so a kiosk dispatch of any
/// ceremony id is the structural no-op: no armed letter, no preview
/// request, no engine command. No disabled affordances are minted for
/// a surface that does not exist.
#[test]
fn d5_kiosk_gates_the_ceremony_verbs_absent() {
    ceremony_journey(|state| {
        let mut harness = CeremonyHarness::mount(state, true);
        for id in [
            ActionId::BoardArchive,
            ActionId::BoardReset,
            ActionId::BoardReconcile,
        ] {
            harness.dispatch(id);
            assert_eq!(
                harness.letter().mode,
                CeremonyLetterMode::None,
                "no ceremony letter mounts on kiosk"
            );
        }
        while let Some(request) = harness.next_request() {
            let taskboard_ui_slint::requests::UiRequest::BoardReconcilePreview { .. } = request
            else {
                continue;
            };
            panic!("no preview request rides a kiosk dispatch");
        }
        assert!(
            harness.engine.drain().is_empty(),
            "no ceremony command dispatches on kiosk"
        );
    });
}

/// The tombstone seam leg (#1183, AC-3): a tombstoned board is never
/// the ceremony subject. The keyboard path's backstop posture,
/// hand-built: an `AppState` whose only board is a tombstone mounts,
/// and the `BoardArchive` verb's arm/confirm grammar is fully inert —
/// no consent strip arms (the selection normalize repairs a dead
/// selection to no live board), nothing dispatches, and the
/// snapshot's board set is identical (the tombstone stays a
/// tombstone, never archived). The pointer menu's greyed ceremony
/// rows are the same verdict's click twin (OP1 US-1/S1); one verb
/// carries the seam claim — `BoardReset` and `BoardReconcile` read
/// the same liveness posture, and the engine-side typed refusal
/// (`CommandError::BoardDeleted`) stays domain-pinned as the failure
/// path's fact behind the unreachability.
#[test]
fn a_tombstoned_board_is_never_the_ceremony_subject() {
    ceremony_journey(|mut state| {
        let board = fixture_board(&state);
        {
            let row = state.boards.get_mut(&board).expect("fixture board");
            row.deleted = true;
        }
        let mut harness = CeremonyHarness::mount(state, false);
        // The arm stroke and the confirm twin both land: the grammar
        // cannot reach a tombstone.
        harness.dispatch(ActionId::BoardArchive);
        assert_eq!(
            harness.letter().mode,
            CeremonyLetterMode::None,
            "no consent strip arms over a tombstone"
        );
        harness.dispatch(ActionId::BoardArchive); // the confirm twin
        assert_eq!(
            harness.letter().mode,
            CeremonyLetterMode::None,
            "the strip stays unmounted through the confirm stroke"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "no ceremony command dispatches against the tombstone"
        );
        // The board set is identical: the tombstone stayed a tombstone
        // (not archived, not resurrected, not removed).
        let snapshot = harness.engine.snapshot();
        let row = snapshot.boards.get(&board).expect("the tombstone survives");
        assert!(row.deleted, "still a tombstone");
        assert!(!row.archived, "never archived");
        assert_eq!(snapshot.boards.len(), 1, "the board set is identical");
    });
}

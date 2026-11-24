// SPDX-License-Identifier: AGPL-3.0-only

//! The browse-parity journeys (round 5 chunk D1ii — the §3 journey
//! table's nominated evidence file): the picker's keyboard cursor
//! (#1227) and the payload-label enrichment (#1226) over the
//! journaled tree. The UI is the dumb client: rows are the `UndoView`
//! typed projections, the cursor routes through the exact seams the
//! click path uses (US-4/AC-2: invoke ≡ click), and every commit
//! asserts the exact `StateCommand` that crosses the engine boundary
//! (typed payloads, never label text — the oracles' law).

use crate::support;
use crate::undo_picker_journeys_support as prelude;

use support::fixture;

use taskboard_domain::{JournalId, StateCommand, UndoLabel, UndoView};
use taskboard_ui_slint::{KeyboardState, UiSettings};

/// The row ids the fixture projection carries (minted once, shared by
/// the cursor arithmetic and the commit payloads).
fn fixture_ids() -> (JournalId, JournalId, JournalId) {
    (
        JournalId::from_uuid(uuid::Uuid::from_u128(0xB01)),
        JournalId::from_uuid(uuid::Uuid::from_u128(0xB02)),
        JournalId::from_uuid(uuid::Uuid::from_u128(0xB03)),
    )
}

/// The picker-bearing projection: a two-row `recent` window (the
/// newest carrying a payload, the older deliberately subjectless) and
/// one sibling branch with its own payload — the D-3 fold's both
/// shapes and US-1's same-verb siblings in one fixture.
fn picker_state() -> taskboard_domain::AppState {
    let (_fixture, mut state) = fixture::golden_state();
    let (ancestor, head, branch) = fixture_ids();
    state.undo = UndoView {
        can_undo: true,
        can_redo: true,
        head_label: Some("created".into()),
        recent: vec![
            UndoLabel {
                id: head,
                label: "created".into(),
                payload: Some("beta".into()),
            },
            UndoLabel {
                id: ancestor,
                label: "moved up".into(),
                payload: None,
            },
        ],
        branch_children: vec![UndoLabel {
            id: branch,
            label: "created".into(),
            payload: Some("alpha".into()),
        }],
    };
    state
}

/// A row model as `(id, payload)` — the typed enrichment read
/// straight off the pushed global (the label is presentation, never
/// asserted).
fn rows(global: &KeyboardState) -> prelude::PickerRows {
    prelude::picker_rows(global, |row| row.payload.to_string())
}

/// The picker journey harness: driver + fake engine + mounted window.
type BrowseHarness = prelude::PickerHarness;

fn browse_journey(body: impl FnOnce() + Send + 'static) {
    prelude::picker_journey(body);
}

// -- US-1/S1: the picker mounts with the cursor and payload rows -----

/// US-1/S1 (#1227 W-1 + #1226 G2): the chord mounts the picker with
/// the cursor armed at row 0 and the rows carrying the typed payload
/// enrichment — the newest recent row its subject, the older one the
/// empty-string stand-in for `None` (D-3: the UI hides a None
/// payload), the sibling its own. Mounting dispatches nothing.
#[test]
fn us1_s1_picker_mounts_with_cursor_and_payload_rows() {
    browse_journey(|| {
        let mut harness = BrowseHarness::mount(picker_state(), UiSettings::default());
        assert!(!harness.open(), "closed at mount");
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        assert!(harness.open(), "the chord mounts the picker");
        assert_eq!(harness.cursor(), 0, "the cursor arms at the reflex row");
        let (ancestor, head, branch) = fixture_ids();
        let (branches, targets) = rows(&harness.keyboard());
        assert_eq!(
            branches,
            vec![(branch.as_uuid(), "alpha".to_owned())],
            "the sibling carries its typed payload"
        );
        assert_eq!(
            targets,
            vec![
                (head.as_uuid(), "beta".to_owned()),
                (ancestor.as_uuid(), String::new()),
            ],
            "the newest recent row carries its payload; the subjectless row carries none"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "mounting dispatches nothing"
        );
    });
}

// -- US-1/S2: the menu ladder moves the picker cursor ----------------

/// US-1/S2 (#1227, D-1): the shell's existing
/// `menu.next`/`menu.previous` ladder moves the picker-scoped cursor
/// over the selectable rows, wrapping at both ends — the action
/// menu's own cursor stays out of the way (it is closed).
#[test]
fn us1_s2_menu_ladder_moves_picker_cursor() {
    browse_journey(|| {
        let mut harness = BrowseHarness::mount(picker_state(), UiSettings::default());
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        // 2 reflex + 2 recent + 1 branch = 5 selectable rows.
        harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuNext);
        harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuNext);
        assert_eq!(harness.cursor(), 2, "next steps onto the undo-step rows");
        harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuPrevious);
        assert_eq!(harness.cursor(), 1, "previous unwinds");
        harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuPrevious);
        harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuPrevious);
        assert_eq!(harness.cursor(), 4, "previous wraps to the last row");
        harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuNext);
        assert_eq!(harness.cursor(), 0, "next wraps past the end");
        assert!(
            harness.engine.drain().is_empty(),
            "walking dispatches nothing"
        );
    });
}

// -- US-1/S5: Escape closes without dispatch -------------------------

/// US-1/S5 (AC-4's cursor extension): Escape closes the picker from
/// an armed cursor with nothing dispatched, and the cursor resets so
/// a remount arms at row 0 again.
#[test]
fn us1_s5_escape_closes_the_picker_without_dispatch() {
    browse_journey(|| {
        let mut harness = BrowseHarness::mount(picker_state(), UiSettings::default());
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuNext);
        harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuNext);
        assert_eq!(harness.cursor(), 2);
        harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuDismiss);
        assert!(!harness.open(), "Escape closes");
        assert!(harness.engine.drain().is_empty(), "nothing dispatched");
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        assert!(harness.open());
        assert_eq!(harness.cursor(), 0, "the remount arms at row 0");
    });
}

// -- US-1/S3: invoke dispatches the cursor's row verb ----------------

/// US-1/S3 (#1227 W-1c): `menu.invoke` dispatches the cursor's row
/// through the existing seams — the reflex row routes `AppUndo`
/// (`StateCommand::Undo`), an undo-step row routes the exact
/// `UndoTo { target }` naming that row's journal id, and the picker
/// closes (the close-then-dispatch shape).
#[test]
fn us1_s3_invoke_selected_dispatches_the_row_verb() {
    browse_journey(|| {
        let mut harness = BrowseHarness::mount(picker_state(), UiSettings::default());
        let (_ancestor, head, _branch) = fixture_ids();
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        assert!(harness.open(), "the chord mounts the picker");
        // Cursor at 0: the reflex row.
        harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuInvoke);
        match harness.engine.drain().as_slice() {
            [StateCommand::Undo] => {}
            other => panic!("the reflex row routes the plain verb, got {other:?}"),
        }
        assert!(!harness.open(), "the commit closes the picker");
        // Remount, cursor onto the head's undo-step row (index 2).
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuNext);
        harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuNext);
        harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuInvoke);
        match harness.engine.drain().as_slice() {
            [StateCommand::UndoTo { target }] => assert_eq!(*target, head),
            other => panic!("the step row routes the exact verb once, got {other:?}"),
        }
        assert!(!harness.open());
    });
}

/// US-1/S3 (US-4/AC-2's invoke ≡ click law): the keyboard invoke on a
/// branch row crosses the boundary as byte-identical dispatch to the
/// row's click seam — the same verb variant, the same target, the
/// same exactly-once count.
#[test]
fn us1_s3_invoke_matches_click_dispatch() {
    browse_journey(|| {
        let (_ancestor, _head, branch) = fixture_ids();
        // The keyboard path: remount, walk to the branch row (index 4
        // = 2 reflex + 2 recent + 1 branch), invoke.
        let mut keyboard_harness = BrowseHarness::mount(picker_state(), UiSettings::default());
        keyboard_harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        for _ in 0..4 {
            keyboard_harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuNext);
        }
        assert_eq!(keyboard_harness.cursor(), 4);
        keyboard_harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuInvoke);
        let keyboard_dispatch = keyboard_harness.engine.drain();
        // The click path: the row's seam, directly.
        let mut click_harness = BrowseHarness::mount(picker_state(), UiSettings::default());
        click_harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        click_harness
            .handle
            .window()
            .invoke_undo_branch_picked(branch.as_uuid().to_string().into());
        click_harness.advance();
        let click_dispatch = click_harness.engine.drain();
        assert_eq!(
            keyboard_dispatch, click_dispatch,
            "invoke and click dispatch identically"
        );
        match keyboard_dispatch.as_slice() {
            [StateCommand::RedoTo { target }] => assert_eq!(*target, branch),
            other => panic!("the branch row routes the exact verb once, got {other:?}"),
        }
        assert!(!keyboard_harness.open(), "the commit closes the picker");
    });
}

// -- US-1/S4: the presented projection reflects the landed head ------

/// US-1/S4: after the engine publishes a re-derived projection (the
/// walk landed elsewhere), a re-opened picker presents that state's
/// rows and the cursor re-arms at 0 — the presented state is always
/// the published fact, never a stale local.
#[test]
fn us1_s4_presented_state_reflects_the_landed_head() {
    browse_journey(|| {
        let mut harness = BrowseHarness::mount(picker_state(), UiSettings::default());
        let (ancestor, head, _branch) = fixture_ids();
        harness.engine.settle(&harness.handle);
        let (_fixture, mut branch_state) = fixture::golden_state();
        branch_state.undo = UndoView {
            can_undo: true,
            can_redo: false,
            head_label: Some("created".into()),
            recent: vec![UndoLabel {
                id: ancestor,
                label: "moved up".into(),
                payload: None,
            }],
            branch_children: vec![UndoLabel {
                id: head,
                label: "created".into(),
                payload: Some("beta".into()),
            }],
        };
        harness.engine.replace_state(branch_state, &harness.handle);
        harness.advance();
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        let (branches, targets) = rows(&harness.keyboard());
        assert_eq!(branches, vec![(head.as_uuid(), "beta".to_owned())]);
        assert_eq!(targets, vec![(ancestor.as_uuid(), String::new())]);
        assert_eq!(
            harness.cursor(),
            0,
            "the fresh mount arms at the reflex row"
        );
    });
}

// -- US-4/S2: click and invoke dispatch identically (reflex row) -----

/// US-4/S2 (the fence's second half): the plain reflex row's click
/// seam and the keyboard invoke cross the boundary identically — the
/// same single `StateCommand::Undo`, no drift between the pointer
/// and keyboard grammars.
#[test]
fn us4_s2_click_and_invoke_dispatch_identically() {
    browse_journey(|| {
        // The keyboard path: cursor at 0 (the Undo reflex row).
        let mut keyboard_harness = BrowseHarness::mount(picker_state(), UiSettings::default());
        keyboard_harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        keyboard_harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuInvoke);
        let keyboard_dispatch = keyboard_harness.engine.drain();
        // The click path: the reflex row's seam, directly.
        let mut click_harness = BrowseHarness::mount(picker_state(), UiSettings::default());
        click_harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        click_harness.handle.window().invoke_undo_line_undo();
        click_harness.advance();
        let click_dispatch = click_harness.engine.drain();
        assert_eq!(keyboard_dispatch, click_dispatch, "invoke ≡ click");
        assert!(
            matches!(keyboard_dispatch.as_slice(), [StateCommand::Undo]),
            "the reflex row routes the plain verb exactly once"
        );
    });
}

// -- the shrunken-projection strand arm ------------------------------

/// The typed close-nothing-dispatch arm: the cursor rode past the
/// projection's rows (the windows shrank between the stroke and the
/// invoke — the shell's mount-time row count is stale), so the
/// invoke resolves to no row: the picker closes and NOTHING
/// dispatches (the AC-4 shape, not a panic, not a wild target).
#[test]
fn us1_s3_stranded_cursor_closes_without_dispatch() {
    browse_journey(|| {
        let mut harness = BrowseHarness::mount(picker_state(), UiSettings::default());
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        // The cursor walks to the last row (index 4 = 2 reflex + 2
        // recent + 1 branch); the shell holds the mount-time count.
        for _ in 0..4 {
            harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuNext);
        }
        assert_eq!(harness.cursor(), 4);
        // The projection shrinks to empty windows — the published rows
        // drop to none while the picker stays mounted.
        let (_fixture, mut shrunk) = fixture::golden_state();
        shrunk.undo = UndoView::default();
        harness.engine.replace_state(shrunk, &harness.handle);
        harness.advance();
        harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuInvoke);
        assert!(!harness.open(), "the stranded invoke closes the picker");
        assert!(
            harness.engine.drain().is_empty(),
            "the stranded invoke dispatches nothing"
        );
    });
}

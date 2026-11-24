// SPDX-License-Identifier: AGPL-3.0-only

//! The undo branch-picker journeys (round 4 UF2, #353 — the §6 wiring
//! table's nominated evidence file): the v2 picker over the journaled
//! tree. The UI is the dumb client: the picker's rows are the
//! `UndoView` typed projections, and every commit asserts the exact
//! `StateCommand` that crosses the engine boundary (typed payloads,
//! never label text — the oracles' law). The engine-side walk
//! semantics live at the domain layer (`undo/tests.rs`'s UF2 suite);
//! these journeys pin the routing, the mounts, the escapes, and the
//! R-2 kiosk absence.

use crate::support;
use crate::undo_picker_journeys_support as prelude;

use support::fixture;

use taskboard_domain::{JournalId, StateCommand, UndoLabel, UndoView};
use taskboard_ui_slint::UiSettings;

/// The row ids the fixture projection carries (minted once, shared by
/// the mount assertions and the commit payloads).
fn fixture_ids() -> (JournalId, JournalId, JournalId) {
    (
        JournalId::from_uuid(uuid::Uuid::from_u128(0xA01)),
        JournalId::from_uuid(uuid::Uuid::from_u128(0xA02)),
        JournalId::from_uuid(uuid::Uuid::from_u128(0xA03)),
    )
}

/// The picker-bearing projection: a head with one abandoned sibling
/// branch (`branch_children`) and a two-row `recent` window (US-1/AC-1
/// and US-2's row sources). Engine-derived in production; the dumb
/// client takes the projection as pushed.
fn picker_state() -> taskboard_domain::AppState {
    let (_fixture, mut state) = fixture::golden_state();
    let (ancestor, head, branch) = fixture_ids();
    state.undo = UndoView {
        can_undo: true,
        can_redo: true,
        head_label: Some("Mark done".into()),
        recent: vec![
            UndoLabel {
                id: head,
                label: "Mark done".into(),
                payload: Some("Mark done".into()),
            },
            UndoLabel {
                id: ancestor,
                label: "Move task".into(),
                payload: Some("Move task".into()),
            },
        ],
        branch_children: vec![UndoLabel {
            id: branch,
            label: "Create task".into(),
            payload: Some("Create task".into()),
        }],
    };
    state
}

/// The picker's row models as `(id, label)` pairs (the typed
/// addressing reads straight off the pushed global).
fn rows(global: &taskboard_ui_slint::KeyboardState<'_>) -> prelude::PickerRows {
    prelude::picker_rows(global, |row| row.label.to_string())
}

/// The picker journey harness: driver + fake engine + mounted window
/// (the kiosk flag is this suite's only mount divergence).
type PickerHarness = prelude::PickerHarness;

fn mount(state: taskboard_domain::AppState, kiosk: bool) -> PickerHarness {
    PickerHarness::mount(
        state,
        UiSettings {
            kiosk,
            ..UiSettings::default()
        },
    )
}

fn picker_journey(body: impl FnOnce() + Send + 'static) {
    prelude::picker_journey(body);
}

// -- US-1/S1: the picker mounts with its sections (W-1) ------------------

/// US-1/S1 (W-1, AC-1): the branches chord mounts the picker — the
/// sibling section carries the head's children as typed rows
/// (well-formed journal ids, engine-derived labels), the undo-step
/// section carries the `recent` ancestors — and mounting dispatches
/// nothing (pure navigation).
#[test]
fn us1_s1_picker_mounts_with_sections() {
    picker_journey(|| {
        let mut harness = mount(picker_state(), false);
        assert!(!harness.open(), "closed at mount");
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        assert!(harness.open(), "the chord mounts the picker");
        let (branches, targets) = rows(&harness.keyboard());
        let (_ancestor, _head, branch) = fixture_ids();
        assert_eq!(
            branches,
            vec![(branch.as_uuid(), "Create task".to_owned())],
            "the sibling section lists the head's children, newest first"
        );
        assert_eq!(targets.len(), 2, "the undo-step section lists recent");
        assert!(
            harness.engine.drain().is_empty(),
            "mounting dispatches nothing"
        );
    });
}

/// US-1/S1's R-2 gate: on kiosk the picker entry renders absent — the
/// chord is structurally inert (the desktop-qualified E; the v1 reflex
/// keeps its kiosk qualification unchanged).
#[test]
fn us1_s1_kiosk_picker_entry_renders_absent() {
    picker_journey(|| {
        let mut harness = mount(picker_state(), true);
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        assert!(!harness.open(), "the kiosk mount refuses the picker");
        assert!(
            harness.engine.drain().is_empty(),
            "the inert dispatch sends nothing"
        );
    });
}

// -- US-1/S2: the sibling commit (W-2) -----------------------------------

/// US-1/S2 (W-2, AC-2): committing a sibling row routes exactly one
/// `RedoTo { target }` naming the row's journal id, and the picker
/// closes (the close-then-dispatch shape). The walk itself is the
/// domain layer's evidence (`undo::tests::us1_s2`).
#[test]
fn us1_s2_sibling_commit_redo_to() {
    picker_journey(|| {
        let mut harness = mount(picker_state(), false);
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        let (_ancestor, _head, branch) = fixture_ids();
        harness
            .handle
            .window()
            .invoke_undo_branch_picked(branch.as_uuid().to_string().into());
        harness.advance();
        match harness.engine.drain().as_slice() {
            [StateCommand::RedoTo { target }] => assert_eq!(*target, branch),
            other => panic!("the sibling row routes the exact verb once, got {other:?}"),
        }
        assert!(!harness.open(), "the commit closes the picker");
    });
}

/// US-1/S2b (W-6): a stale row — a well-formed journal id that is no
/// longer on the head's line (an intervening edit moved the head
/// between mount and commit) — crosses the boundary as the typed
/// `RedoTo { target }` payload, and the engine's `TargetNotInLine`
/// refusal surfaces on the existing outcome-notice lane (the W-6 row's
/// unchanged handler; the copy is presentation, never asserted). The
/// boundary oracle here is the typed variant and the exactly-once
/// dispatch; the variant itself is `matches!`-pinned at the domain
/// (`us2_ac2`) and the CLI kind map (`target-not-in-line`).
#[test]
fn us1_s2b_stale_target_toasts_typed_refusal() {
    picker_journey(|| {
        let mut harness = mount(picker_state(), false);
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        let stale = JournalId::from_uuid(uuid::Uuid::from_u128(0xDEAD));
        harness
            .handle
            .window()
            .invoke_undo_branch_picked(stale.as_uuid().to_string().into());
        harness.advance();
        match harness.engine.drain().as_slice() {
            [StateCommand::RedoTo { target }] => assert_eq!(*target, stale),
            other => panic!("the stale row still routes the typed payload once, got {other:?}"),
        }
        assert!(!harness.open());
    });
}

/// US-1/S3: after the switch, the published projection re-derives —
/// the picker re-opened over the branch-head state carries that
/// branch's own rows (the same journey re-run; S3's openable-again
/// law).
#[test]
fn us1_s3_post_switch_state_reflects_branch() {
    picker_journey(|| {
        let mut harness = mount(picker_state(), false);
        let (ancestor, head, _branch) = fixture_ids();
        // The engine half: the walk landed on the branch, so the next
        // publish's projection shows the branch's head and its (empty)
        // children with the older line points as undo steps.
        let (_fixture, mut branch_state) = fixture::golden_state();
        branch_state.undo = UndoView {
            can_undo: true,
            can_redo: false,
            head_label: Some("Create task".into()),
            recent: vec![UndoLabel {
                id: ancestor,
                label: "Move task".into(),
                payload: Some("Move task".into()),
            }],
            branch_children: vec![UndoLabel {
                id: head,
                label: "Mark done".into(),
                payload: Some("Mark done".into()),
            }],
        };
        harness.engine.settle(&harness.handle);
        harness.engine.replace_state(branch_state, &harness.handle);
        harness.advance();
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        let (branches, targets) = rows(&harness.keyboard());
        assert_eq!(branches, vec![(head.as_uuid(), "Mark done".to_owned())]);
        assert_eq!(targets, vec![(ancestor.as_uuid(), "Move task".to_owned())]);
    });
}

/// US-1/S4 (AC-4): Escape closes the picker with nothing dispatched,
/// and the outside click (the backdrop seam) does the same — the pure
/// selection menu's dismissal law.
#[test]
fn us1_s4_escape_closes_without_dispatch() {
    picker_journey(|| {
        let mut harness = mount(picker_state(), false);
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        assert!(harness.open());
        harness.dispatch(taskboard_ui_slint::actions::ActionId::MenuDismiss);
        assert!(!harness.open(), "Escape closes");
        assert!(harness.engine.drain().is_empty(), "nothing dispatched");
        // The outside click: the backdrop's dismissal seam.
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        assert!(harness.open());
        harness.handle.window().invoke_undo_picker_dismissed();
        harness.advance();
        assert!(!harness.open(), "the outside click closes");
        assert!(harness.engine.drain().is_empty(), "nothing dispatched");
    });
}

// -- US-2/S1: the ancestor commit (W-3) ----------------------------------

/// US-2/S1 (W-3, AC-1): committing an undo-step row routes exactly one
/// `UndoTo { target }` naming the row's journal id; the N-step walk is
/// the domain layer's evidence (`undo::tests::us2_ac1`).
#[test]
fn us2_s1_ancestor_commit_undo_to() {
    picker_journey(|| {
        let mut harness = mount(picker_state(), false);
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        let (ancestor, _head, _branch) = fixture_ids();
        harness
            .handle
            .window()
            .invoke_undo_target_picked(ancestor.as_uuid().to_string().into());
        harness.advance();
        match harness.engine.drain().as_slice() {
            [StateCommand::UndoTo { target }] => assert_eq!(*target, ancestor),
            other => panic!("the ancestor row routes the exact verb once, got {other:?}"),
        }
        assert!(!harness.open(), "the commit closes the picker");
    });
}

// -- US-3: the v1 reflex composes ----------------------------------------

/// US-3/S1–S2 (the regression fence's UI half): the plain rows route
/// the unchanged v1 commands (click ≡ key — the same `AppUndo` /
/// `AppRedo` dispatch the keyboard strokes take), and they stay first
/// in the picker's row order.
#[test]
fn us3_s1_plain_rows_route_the_unchanged_v1_verbs() {
    picker_journey(|| {
        let mut harness = mount(picker_state(), false);
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        harness.handle.window().invoke_undo_line_undo();
        harness.advance();
        assert!(matches!(
            harness.engine.drain().as_slice(),
            [StateCommand::Undo]
        ));
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndoBranches);
        harness.handle.window().invoke_undo_line_redo();
        harness.advance();
        assert!(matches!(
            harness.engine.drain().as_slice(),
            [StateCommand::Redo]
        ));
        // The fence's structural half: plain `Undo`/`Redo` outside the
        // picker keep their exact dispatch (the v1 semantics the
        // domain corpus pins byte-identically).
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppUndo);
        harness.dispatch(taskboard_ui_slint::actions::ActionId::AppRedo);
        assert!(matches!(
            harness.engine.drain().as_slice(),
            [StateCommand::Undo, StateCommand::Redo]
        ));
    });
}

// SPDX-License-Identifier: AGPL-3.0-only

//! The directional-focus journeys (J3 US-1, round 9 chunk 9 — the §6
//! wiring table's nominated evidence file `focus_law_journeys.rs`):
//! the nav.focus-* per-view readings (the W-2 interpret table), the
//! text-entry guard negatives, the planner's pane-switch grammar, and
//! the #875 extend rows' one-front-per-press seam. Assertion
//! discipline is the journeys suite's: terminal state + the typed
//! commands that reached the fake-engine boundary; titles are
//! element addresses, never text oracles.

mod support;

use support::driver::{DriverOptions, JourneyDriver, SlintDriver};
use support::fake_engine::FakeEngine;
use support::fixture;
use support::health::detached_health;

use arc_swap::ArcSwap;
use slint::ComponentHandle as _;
use slint::Model as _;
use std::sync::Arc;
use taskboard_domain::{AppState, StateCommand};
use taskboard_ui_slint::actions::{ActionId, ContextId};
use taskboard_ui_slint::keymap::stroke::{self, parse};
use taskboard_ui_slint::{KeyboardState, UiSettings, View};

/// The minimal focus-law harness: driver + fake engine + mount (the
/// `habits_journey` shape — this file carries only what these journeys
/// speak).
struct Harness {
    driver: SlintDriver,
    handle: taskboard_ui_slint::UiHandle,
    engine: FakeEngine,
}

fn dummy_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::EngineSignal> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

fn dummy_system_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

impl Harness {
    fn mount_with_settings(driver: SlintDriver, state: AppState, settings: UiSettings) -> Self {
        let state = Arc::new(ArcSwap::from_pointee(state));
        let (commands, engine) = FakeEngine::channel(Arc::clone(&state));
        let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
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
            driver,
            handle,
            engine,
        };
        harness.driver.bind(&harness.handle);
        // The desktop default is Today; the directional journeys that
        // need the board escape centrally (routed, never preset-bound
        // — the escape must not depend on the harness's preset).
        harness
            .handle
            .dispatch_action(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
        harness
    }

    fn advance(&mut self) {
        self.driver.advance_time(0);
    }

    fn stroke(&mut self, stroke: taskboard_ui_slint::keymap::stroke::Stroke) {
        self.driver.press_stroke(stroke);
    }

    /// The visible context chain (the adapter's `shell.chain()`
    /// mirror).
    fn chain(&self) -> Vec<ContextId> {
        let window = self.handle.window();
        let keyboard = window.global::<KeyboardState>();
        if keyboard.get_menu_open() {
            return vec![ContextId::Menu];
        }
        if keyboard.get_vi_insert() {
            return vec![ContextId::ViInsert, ContextId::Global];
        }
        let view = match window.get_current_view() {
            View::Board => Some(ContextId::Board),
            View::Today => Some(ContextId::Today),
            View::StackDetail => Some(ContextId::StackDetail),
            View::TaskDetail => Some(ContextId::TaskDetail),
            View::Review => None,
            View::Settings => Some(ContextId::Settings),
            View::PlanNextDay => Some(ContextId::PlanNextDay),
        };
        let mut chain = Vec::with_capacity(4);
        if keyboard.get_date_picker_grid() {
            chain.push(ContextId::DatePicker);
        }
        chain.extend(view);
        chain.push(ContextId::ViNormal);
        chain.push(ContextId::Global);
        chain
    }

    /// Presses one action through the preset's own chain-visible
    /// binding (the generated menu's lookup — reuse, not reimplement).
    fn press_preset(&mut self, preset: taskboard_ui_slint::keymap::Preset, action: ActionId) {
        let keymap = taskboard_ui_slint::keymap::Keymap::from_preset(preset);
        let chain = self.chain();
        let hint = keymap
            .first_binding_display(action, &chain)
            .unwrap_or_else(|| panic!("{action:?} is bound in the visible chain {chain:?}"));
        for spelling in &hint {
            let stroke = stroke::parse(spelling)
                .unwrap_or_else(|error| panic!("{action:?}'s hint {spelling:?}: {error}"));
            self.driver.press_stroke(stroke);
        }
    }

    /// Drains the boundary and asserts exactly `expected` arrived.
    fn assert_commands(&mut self, expected: &[StateCommand]) {
        let drained = self.engine.drain();
        assert_eq!(&drained, expected, "the boundary captured exactly these");
    }

    /// The emphasized card: `Some((column, row))` of the card whose
    /// model carries the focus bit.
    fn emphasized_card(&self) -> Option<(usize, usize)> {
        let board = self.handle.window().get_board_model();
        for column in 0..board.stacks.row_count() {
            let tasks = board.stacks.row_data(column).expect("column").tasks;
            for row in 0..tasks.row_count() {
                if tasks.row_data(row).expect("card").emphasis {
                    return Some((column, row));
                }
            }
        }
        None
    }
}

fn text_focus(harness: &Harness) -> i32 {
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::EditingFacts>()
        .get_text_focus_count()
}

/// The task view's walk-stop fact, read back through the generated
/// global.
fn walk_stop(harness: &Harness) -> i32 {
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::TaskViewWalkFacts>()
        .get_stop_id()
}

/// The open detail's mounted subject title (the fixture titles are
/// the journeys' element addresses — identity, never label matching).
fn detail_title(harness: &Harness) -> String {
    harness.handle.window().get_task_model().title.to_string()
}

/// A detail open over the fixture's first board card, through the
/// preset's own rows.
fn open_detail_first_card(harness: &mut Harness, preset: taskboard_ui_slint::keymap::Preset) {
    harness.press_preset(preset, ActionId::NavFocusDown);
    harness.press_preset(preset, ActionId::TaskEdit);
    harness.advance();
    assert_eq!(
        harness.handle.window().get_current_view(),
        View::TaskDetail,
        "the detail is open"
    );
}

/// US-1/S1 + AC-2 (J3 W-2, round 9 chunk 9) — the `TaskDetail`
/// directional rows walk the CONTROL ladder: C-n steps down the stop
/// ladder (the walk-stop fact moves), C-p steps back up, and the
/// viewed subject NEVER moves (J3 R-3: no viewed-task change from the
/// detail at all — the census-dated round-8 D-2 supersession). Zero
/// commands — navigation is view-internal.
#[test]
fn us1_s1_taskdetail_directional_rows_walk_the_ladder_subject_pinned() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let preset = taskboard_ui_slint::keymap::Preset::Emacs;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            open_detail_first_card(&mut harness, preset);
            assert_eq!(
                detail_title(&harness),
                "task-0",
                "the detail opens on the first card"
            );

            // C-n steps the control walk onto a stop (the ladder).
            let mut guard = 0;
            while walk_stop(&harness) == 0 && guard < 10 {
                harness.press_preset(preset, ActionId::NavFocusDown);
                harness.advance();
                guard += 1;
            }
            assert_ne!(
                walk_stop(&harness),
                0,
                "C-n steps the detail's control ladder onto a stop"
            );
            // C-p steps back; the walk runs both axes (the degenerate
            // horizontal — the recorded geometry).
            harness.press_preset(preset, ActionId::NavFocusUp);
            harness.advance();
            // The subject is pinned: the walk never retargets the
            // detail, and the walk stays in the view.
            assert_eq!(
                detail_title(&harness),
                "task-0",
                "the directional walk never changes the viewed task (J3 R-3)"
            );
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::TaskDetail,
                "the view never moves"
            );
            harness.assert_commands(&[]);
        },
    );
}

/// US-1/S2 (J3 W-2) — the Today horizontals are the control-walk
/// first cut (J3 R-4): C-f/C-b (nav.focus-right/left) step the
/// control walk, the verticals keep the item walk, all four stay in
/// the view with zero commands.
#[test]
fn today_horizontal_is_the_control_walk_first_cut() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let preset = taskboard_ui_slint::keymap::Preset::Emacs;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            harness.press_preset(preset, ActionId::ViewToday);
            harness.advance();
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Today,
                "today is open"
            );
            for action in [
                ActionId::NavFocusRight,
                ActionId::NavFocusLeft,
                ActionId::NavFocusDown,
                ActionId::NavFocusUp,
            ] {
                harness.press_preset(preset, action);
                harness.advance();
            }
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Today,
                "the directional walk stays in the view"
            );
            harness.assert_commands(&[]);
        },
    );
}

/// US-1/S1 per view (J3 W-2) — the vi motion keys step the board
/// walks: j/k walk the cards, l enters the stack walk (the
/// nav.focus carriers), the focus emphasis follows, nothing
/// dispatches.
#[test]
fn vi_motion_keys_step_the_board_walks() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let preset = taskboard_ui_slint::keymap::Preset::Vi;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            // l enters the stack walk at the aimed column's head (the
            // step_stack bodies behind nav.focus-left/right).
            harness.press_preset(preset, ActionId::NavFocusRight);
            harness.advance();
            assert_eq!(
                harness.emphasized_card(),
                Some((0, 0)),
                "l lands the stack walk on the aimed column's head"
            );
            // j walks the cards within the column; k steps back.
            harness.press_preset(preset, ActionId::NavFocusDown);
            harness.advance();
            assert_eq!(
                harness.emphasized_card(),
                Some((0, 1)),
                "j walks to the next card"
            );
            harness.press_preset(preset, ActionId::NavFocusUp);
            harness.advance();
            assert_eq!(
                harness.emphasized_card(),
                Some((0, 0)),
                "k steps back to the first card"
            );
            harness.assert_commands(&[]);
        },
    );
}

/// US-1/S3 (J3 W-2) — the text-entry guard negatives per preset
/// family: under an engaged field the directional keys reach the
/// field (emacs C-n, standard arrows, vi's j under insert); the walk
/// never runs, the editor stays armed, nothing dispatches. The
/// guards are the inherited scopes — the test proves the vocabulary
/// never competes with them.
#[test]
fn the_text_entry_guards_own_the_directional_keys() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            // Emacs: C-n inside the title editor reaches the field
            // (the insert chain masks the view context — no
            // NavFocusDown row binds there, so the physical C-n is
            // delivered raw and the editor scope owns it).
            let preset = taskboard_ui_slint::keymap::Preset::Emacs;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state.clone(),
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            open_detail_first_card(&mut harness, preset);
            harness.press_preset(preset, ActionId::TaskTitleEdit);
            harness.advance();
            assert!(text_focus(&harness) > 0, "the title editor holds focus");
            harness.stroke(parse("Ctrl+n").expect("Ctrl+n"));
            harness.advance();
            assert!(
                text_focus(&harness) > 0,
                "C-n stays in the field (the guard owns it)"
            );
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::TaskDetail,
                "the view never moves under the guard"
            );
            harness.assert_commands(&[]);
            drop(harness);

            // Standard: the arrows inside the title editor are the
            // field's cursor motions.
            let preset = taskboard_ui_slint::keymap::Preset::Standard;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state.clone(),
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            open_detail_first_card(&mut harness, preset);
            harness.press_preset(preset, ActionId::TaskTitleEdit);
            harness.advance();
            assert!(text_focus(&harness) > 0, "the title editor holds focus");
            harness.stroke(parse("Down").expect("Down"));
            harness.advance();
            harness.stroke(parse("Up").expect("Up"));
            harness.advance();
            assert!(
                text_focus(&harness) > 0,
                "the arrows stay in the field (the guard owns them)"
            );
            harness.assert_commands(&[]);
            drop(harness);

            // Vi: j under insert reaches the field (the shadowing-
            // without-masking law); the insert leg arms on the board
            // (the create field's insert chain).
            let preset = taskboard_ui_slint::keymap::Preset::Vi;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            harness.press_preset(preset, ActionId::ModeInsert);
            harness.advance();
            assert!(
                harness.chain().contains(&ContextId::ViInsert),
                "insert mode is armed"
            );
            harness.stroke(parse("j").expect("j"));
            harness.advance();
            assert!(
                harness.chain().contains(&ContextId::ViInsert),
                "j under insert stays a printable (no walk, no mode flip)"
            );
            harness.assert_commands(&[]);
        },
    );
}

/// The planner's pane-switch grammar (J3 R-5, round 9 chunk 9): the
/// directional left/right flip the focused pane (boxes ↔ pool), the
/// fresh open anchors on the boxes pane, and nothing dispatches (the
/// pane fact is view-internal; the in-pane walk is the recorded
/// out-of-scope follow-up).
#[test]
fn planner_directional_rows_switch_the_pane() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let preset = taskboard_ui_slint::keymap::Preset::Standard;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            harness.press_preset(preset, ActionId::ViewPlanNextDay);
            harness.advance();
            let pane = |harness: &Harness| harness.handle.window().get_plan_model().focus_pane;
            assert_eq!(pane(&harness), 0, "a fresh open anchors on the boxes pane");
            harness.press_preset(preset, ActionId::NavFocusRight);
            harness.advance();
            assert_eq!(pane(&harness), 1, "Right lands on the pool pane");
            harness.press_preset(preset, ActionId::NavFocusLeft);
            harness.advance();
            assert_eq!(pane(&harness), 0, "Left lands back on the boxes pane");
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::PlanNextDay,
                "the pane switch stays in the planner"
            );
            harness.assert_commands(&[]);
        },
    );
}

/// #875 (round 9 chunk 9) — the Standard extend row: one Shift+Down
/// press extends the selection by exactly one member (the
/// double-dispatch seam — the route-level row wins, the named-key
/// intercept stays the fallback front and never also fires; the
/// count of selected cards is the typed oracle).
#[test]
fn the_standard_extend_row_extends_one_member_per_press() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let preset = taskboard_ui_slint::keymap::Preset::Standard;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            // The typed oracle is the selection COUNT over the whole
            // board: exactly the anchor plus the one extension member
            // — no double dispatch, no stray members anywhere.
            let selected_count = |harness: &Harness| {
                let board = harness.handle.window().get_board_model();
                (0..board.stacks.row_count())
                    .map(|column| {
                        let column = board.stacks.row_data(column).expect("column");
                        (0..column.tasks.row_count())
                            .filter(|row| column.tasks.row_data(*row).expect("card").selected)
                            .count()
                    })
                    .sum::<usize>()
            };
            // Seed the walk focus on the first card, then anchor it.
            harness.press_preset(preset, ActionId::NavFocusDown);
            harness.advance();
            harness
                .handle
                .window()
                .invoke_board_task_clicked_modifiers(0, 0, true, false);
            harness.stroke(parse("Shift+Down").expect("Shift+Down"));
            harness.advance();
            assert_eq!(
                selected_count(&harness),
                2,
                "the anchor plus exactly one extension member (one front, one press)"
            );
            harness.assert_commands(&[]);
        },
    );
}

/// #875 (round 9 chunk 9) — the vi and emacs extend rows: a Shift+j
/// press folds the shift into the glyph (the D6 law) and `J` extends
/// the selection; emacs' Ctrl+Shift+n twin rides the chorded case
/// law. One front per press (the same typed oracle).
#[test]
fn the_vi_and_emacs_extend_rows_extend_one_member_per_press() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            // Vi: j seeds the walk, J extends.
            let preset = taskboard_ui_slint::keymap::Preset::Vi;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state.clone(),
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            // The same typed oracle as the Standard twin: the
            // selection COUNT over the whole board is exactly two.
            let selected_count = |harness: &Harness| {
                let board = harness.handle.window().get_board_model();
                (0..board.stacks.row_count())
                    .map(|column| {
                        let column = board.stacks.row_data(column).expect("column");
                        (0..column.tasks.row_count())
                            .filter(|row| column.tasks.row_data(*row).expect("card").selected)
                            .count()
                    })
                    .sum::<usize>()
            };
            harness.press_preset(preset, ActionId::NavFocusDown);
            harness.advance();
            harness.stroke(parse("J").expect("J"));
            harness.advance();
            assert_eq!(
                selected_count(&harness),
                2,
                "the anchor plus exactly one J extension (one front, one press)"
            );
            harness.assert_commands(&[]);
            drop(harness);

            // Emacs: Ctrl+Shift+n extends after seeding the walk.
            let preset = taskboard_ui_slint::keymap::Preset::Emacs;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            harness.press_preset(preset, ActionId::NavFocusDown);
            harness.advance();
            harness.stroke(parse("Ctrl+Shift+n").expect("Ctrl+Shift+n"));
            harness.advance();
            assert_eq!(
                selected_count(&harness),
                2,
                "the anchor plus exactly one Ctrl+Shift+n extension (one front, one press)"
            );
            harness.assert_commands(&[]);
        },
    );
}

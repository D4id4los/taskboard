// SPDX-License-Identifier: AGPL-3.0-only

//! The directional-journeys shared harness (round 9 chunk 9, the
//! Sonar new-code dedup): driver + fake engine + mount + preset-keymap
//! settings, shared by `focus_law_journeys.rs` and
//! `board_move_journeys.rs`. Each journey keeps its own oracle
//! helpers; this module carries only the per-test preamble they all
//! speak (the fixture-continuity shape: one `golden_state` fixture,
//! one driver run, one board-mounted harness per leg).

use super::driver::{DriverOptions, JourneyDriver as _, SlintDriver};
use super::fake_engine::FakeEngine;
use super::fixture;
use super::health::detached_health;

use arc_swap::ArcSwap;
use slint::ComponentHandle as _;
use slint::Model as _;
use std::sync::Arc;
use taskboard_domain::{AppState, StateCommand};
use taskboard_ui_slint::actions::{ActionId, ContextId};
use taskboard_ui_slint::keymap::stroke::{self, Stroke};
use taskboard_ui_slint::keymap::{Keymap, Preset};
use taskboard_ui_slint::{KeyboardState, UiSettings, View};

/// The minimal directional-journey harness: driver + fake engine +
/// board-mounted window (the journeys-suite shape — this module
/// carries only what the directional journeys speak).
pub struct JourneyHarness {
    pub driver: SlintDriver,
    pub handle: taskboard_ui_slint::UiHandle,
    pub engine: FakeEngine,
}

fn dummy_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::EngineSignal> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

fn dummy_system_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

impl JourneyHarness {
    /// Mount over `state` with the preset's own keymap, bind the
    /// driver, and escape to the board (the shared journey preamble).
    pub fn mount(state: AppState, preset: Preset) -> Self {
        let settings = UiSettings {
            keymap: Keymap::from_preset(preset),
            keymap_preset: preset,
            ..UiSettings::default()
        };
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
            driver: SlintDriver::default(),
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

    pub fn advance(&mut self) {
        self.driver.advance_time(0);
    }

    pub fn stroke(&mut self, stroke: Stroke) {
        self.driver.press_stroke(stroke);
    }

    /// The visible context chain (the adapter's `shell.chain()`
    /// mirror).
    pub fn chain(&self) -> Vec<ContextId> {
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
    pub fn press_preset(&mut self, preset: Preset, action: ActionId) {
        let keymap = Keymap::from_preset(preset);
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
    pub fn assert_commands(&mut self, expected: &[StateCommand]) {
        let drained = self.engine.drain();
        assert_eq!(&drained, expected, "the boundary captured exactly these");
    }

    /// The #875 typed oracle: the selection COUNT over the whole
    /// board.
    pub fn selected_count(&self) -> usize {
        let board = self.handle.window().get_board_model();
        (0..board.stacks.row_count())
            .map(|column| {
                let column = board.stacks.row_data(column).expect("column");
                (0..column.tasks.row_count())
                    .filter(|row| column.tasks.row_data(*row).expect("card").selected)
                    .count()
            })
            .sum::<usize>()
    }
}

/// Runs one journey leg over the golden fixture: the shared per-test
/// preamble (`golden_state` + one driver run), handing the leg its
/// state so it can mount one harness per preset family (the
/// multi-harness legs drop each harness before the next). The leg
/// keeps its own dispatch sequence and typed outcomes.
pub fn golden_state_journey(body: impl FnOnce(AppState) + Send + 'static) {
    let (_fixture, state) = fixture::golden_state();
    super::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            body(state);
        },
    );
}

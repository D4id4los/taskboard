// SPDX-License-Identifier: AGPL-3.0-only

//! The keyboard-journey suite (design §7.3/§8.1, chunk plan §5): the
//! ten core verbs as one continuous session plus the cold-start view
//! graph walk, driven through the `tests/support` harness.
//!
//! Assertion discipline (§10.3): terminal state + focus ownership +
//! the typed commands that reached the forwarder boundary. Accessible
//! labels are element *addresses*; status/error message text is never
//! matched.

mod support;

use support::driver::{DriverOptions, JourneyDriver, SlintDriver};
use support::fake_engine::FakeEngine;
use support::fixture;
use support::guard::Guard;

use arc_swap::ArcSwap;
use slint::ComponentHandle as _;
use slint::Model as _;
use std::sync::Arc;
use taskboard_domain::{AppState, StateCommand};
use taskboard_ui_slint::actions::{ActionId, ContextId};
use taskboard_ui_slint::keymap::stroke;
use taskboard_ui_slint::menu;
use taskboard_ui_slint::{UiSettings, View};

/// The harness (chunk plan §5): driver + fake engine + fixture + the
/// keypress guard. Everything runs on the driver thread inside one
/// `driver::run` job.
struct Harness {
    driver: SlintDriver,
    handle: taskboard_ui_slint::UiHandle,
    engine: FakeEngine,
    guard: Guard,
}

impl Harness {
    /// Mounts `state` through the public `mount()` with the fake's
    /// channel halves (dropped broadcast senders, as the lib tests
    /// do) — no engine, no app crate.
    fn mount(driver: SlintDriver, state: AppState) -> Self {
        let state = Arc::new(ArcSwap::from_pointee(state));
        let (commands, engine) = FakeEngine::channel(Arc::clone(&state));
        let handle = taskboard_ui_slint::mount(
            state,
            dummy_receiver(),
            dummy_system_receiver(),
            commands,
            UiSettings::default(), // the standard preset
        )
        .expect("mount succeeds on the testing backend");
        let mut harness = Self {
            driver,
            handle,
            engine,
            guard: Guard::default(),
        };
        harness.driver.bind(&harness.handle);
        harness
    }

    /// The visible context chain (the adapter's `shell.chain()`
    /// mirror, read from the window globals + current view).
    fn chain(&self) -> Vec<ContextId> {
        let window = self.handle.window();
        let keyboard = window.global::<taskboard_ui_slint::KeyboardState>();
        if keyboard.get_menu_open() {
            return vec![ContextId::Menu];
        }
        if keyboard.get_vi_insert() {
            return vec![ContextId::ViInsert, ContextId::Global];
        }
        let view = match window.get_current_view() {
            View::Board => Some(ContextId::Board),
            View::StackDetail => Some(ContextId::StackDetail),
            View::TaskDetail => Some(ContextId::TaskDetail),
        };
        let mut chain = Vec::with_capacity(3);
        chain.extend(view);
        chain.push(ContextId::ViNormal);
        chain.push(ContextId::Global);
        chain
    }

    /// Presses one *action* by resolving its chain-visible binding
    /// through the same lookup the generated menu uses
    /// (`menu::entries`' hint path — reuse, not reimplement) and
    /// injecting each stroke; counts the presses on the open guard
    /// phase (§9.2 accounting).
    fn press(&mut self, action: ActionId) {
        let keymap = UiSettings::default().keymap;
        let entries = menu::entries(&keymap, &self.chain());
        let entry = entries
            .iter()
            .find(|entry| entry.action == action)
            .unwrap_or_else(|| panic!("{action:?} is bound in the visible chain"));
        assert!(entry.enabled, "{action:?} is enabled");
        for spelling in &entry.hint {
            let stroke = stroke::parse(spelling)
                .unwrap_or_else(|error| panic!("{action:?}'s hint {spelling:?}: {error}"));
            let presses = self.driver.press_stroke(stroke);
            self.guard.press_n(presses);
        }
    }

    /// Types raw text (the title-entry convention: the characters go
    /// through the same `press_stroke` path — they count). Callers
    /// land with the create/edit steps.
    #[allow(dead_code)]
    fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            let stroke = taskboard_ui_slint::keymap::Stroke {
                key: ch,
                ctrl: false,
                alt: false,
                shift: false,
                meta: false,
            };
            let presses = self.driver.press_stroke(stroke);
            self.guard.press_n(presses);
        }
    }

    /// Drains the boundary and asserts exactly `expected` arrived
    /// since the last check (the "nothing else dispatched"
    /// convention).
    fn assert_commands(&mut self, expected: &[StateCommand]) {
        let drained = self.engine.drain();
        assert_eq!(&drained, expected, "the boundary captured exactly these");
    }

    /// The board's card counts per column.
    fn board_shape(&self) -> Vec<usize> {
        let stacks = self.handle.window().get_board_model().stacks;
        (0..stacks.row_count())
            .map(|column| stacks.row_data(column).expect("column").tasks.row_count())
            .collect()
    }

    /// The emphasized card, resolved through the id tables' ordering:
    /// `Some((column, row))` of the card whose model carries the
    /// focus bit.
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

fn dummy_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::EngineSignal> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

type SignalReceiver = tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent>;

fn dummy_system_receiver() -> SignalReceiver {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

/// §7.3 — the ten core verbs in one continuous session (one `#[test]`
/// is the contract; per-step asserts inside).
#[test]
fn the_ten_step_keyboard_session() {
    let (_fixture, state) = fixture::journey_state();
    support::driver::run(DriverOptions::default(), move |_driver| {
        // -- step 1: launch → the board view owns focus --------------
        let mut harness = Harness::mount(SlintDriver::default(), state);
        assert_eq!(harness.handle.window().get_current_view(), View::Board);
        assert_eq!(harness.board_shape(), vec![2, 1], "one column per stack");
        assert!(
            harness.driver.find_by_label("Board").is_some(),
            "the board region is addressable"
        );
        // Focus *selection* seeds on the first walk step (the model
        // seam starts unfocused); ownership asserted from step 2 on.
        harness.assert_commands(&[]);

        // -- step 2: navigate items and views ------------------------
        harness.press(ActionId::NavNextItem);
        assert_eq!(
            harness.emphasized_card(),
            Some((0, 0)),
            "the first walk step lands on the first card"
        );
        harness.press(ActionId::NavNextItem);
        assert_eq!(
            harness.emphasized_card(),
            Some((0, 1)),
            "the second task of the first stack takes the emphasis"
        );
        harness.press(ActionId::NavPrevItem);
        assert_eq!(harness.emphasized_card(), Some((0, 0)));
        harness.assert_commands(&[]); // navigation dispatches nothing

        harness.press(ActionId::NavNextView);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::StackDetail
        );
        harness.press(ActionId::NavNextView);
        assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
        harness.press(ActionId::NavNextView);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Board,
            "the view cycle wraps"
        );
        harness.press(ActionId::NavPrevView);
        assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
        harness.press(ActionId::NavPrevView);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::StackDetail
        );
        harness.press(ActionId::NavPrevView);
        assert_eq!(harness.handle.window().get_current_view(), View::Board);
        harness.assert_commands(&[]);
    });
}

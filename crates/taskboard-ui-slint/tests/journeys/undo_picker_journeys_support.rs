// SPDX-License-Identifier: AGPL-3.0-only

//! The undo branch-picker journeys' shared preamble (the picker
//! suite's and the browse-parity suite's mount, cursor, and row-read
//! machinery): the broadcast dummies, the mounted picker harness over
//! caller-built `UiSettings` (the suites' only divergence — the
//! kiosk flag), the standard headless `run` wrapper, and the typed
//! row read off the pushed global. Included once per `journey_suites`
//! binary; both suites route through it so the mount shape is one
//! construction site.

use crate::support;

use support::driver::{DriverOptions, JourneyDriver as _, SlintDriver};
use support::fake_engine::FakeEngine;
use support::health::detached_health;

use arc_swap::ArcSwap;
use slint::ComponentHandle as _;
use slint::Model as _;
use std::sync::Arc;
use taskboard_domain::AppState;
use taskboard_ui_slint::UiSettings;
use taskboard_ui_slint::actions::ActionId;
use tokio::sync::mpsc;

pub fn dummy_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::EngineSignal> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

pub fn dummy_system_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

/// The picker journey harness: driver + fake engine + mounted window.
pub struct PickerHarness {
    pub driver: SlintDriver,
    pub handle: taskboard_ui_slint::UiHandle,
    pub engine: FakeEngine,
}

impl PickerHarness {
    /// The caller builds the `UiSettings` (the kiosk flag — the
    /// suites' only divergence).
    pub fn mount(state: AppState, settings: UiSettings) -> Self {
        let state = Arc::new(ArcSwap::from_pointee(state));
        let (commands, engine) = FakeEngine::channel(Arc::clone(&state));
        let (requests_tx, _requests_rx) = mpsc::channel(16);
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
        harness
    }

    pub fn dispatch(&mut self, action: ActionId) {
        self.handle.dispatch_action(action);
        self.advance();
    }

    pub fn advance(&mut self) {
        self.driver.advance_time(0);
    }

    pub fn keyboard(&self) -> taskboard_ui_slint::KeyboardState<'_> {
        self.handle
            .window()
            .global::<taskboard_ui_slint::KeyboardState>()
    }

    pub fn open(&self) -> bool {
        self.keyboard().get_undo_picker_open()
    }

    /// The picker-scoped cursor (#1227): the shell's selection
    /// mirrored onto the window global.
    pub fn cursor(&self) -> usize {
        usize::try_from(self.keyboard().get_undo_picker_cursor()).unwrap_or(usize::MAX)
    }
}

/// The picker's row models as `(id, cell)` pairs — the typed
/// addressing reads straight off the pushed global (the
/// pair-of-row-lists shape is the assertion's vocabulary).
pub type PickerRows = (Vec<(uuid::Uuid, String)>, Vec<(uuid::Uuid, String)>);

/// The picker's row models as `(id, cell)` pairs — the typed
/// addressing reads straight off the pushed global. The caller picks
/// the projected cell (the label or the payload — the suites' two
/// enrichment reads).
pub fn picker_rows(
    global: &taskboard_ui_slint::KeyboardState<'_>,
    cell: impl Fn(&taskboard_ui_slint::UndoPickerRowModel) -> String,
) -> PickerRows {
    let read = |model: &slint::ModelRc<taskboard_ui_slint::UndoPickerRowModel>| {
        (0..model.row_count())
            .map(|i| {
                let row = model.row_data(i).expect("row");
                (
                    uuid::Uuid::parse_str(row.id.as_str()).expect("typed id"),
                    cell(&row),
                )
            })
            .collect::<Vec<_>>()
    };
    (
        read(&global.get_undo_picker_branch_rows()),
        read(&global.get_undo_picker_target_rows()),
    )
}

pub fn picker_journey(body: impl FnOnce() + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            body();
        },
    );
}

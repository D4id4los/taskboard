// SPDX-License-Identifier: AGPL-3.0-only
//! Slint presentation layer for `taskboard`.
//!
//! The UI is a "dumb presentation client": it reads state via lock-free
//! `arc_swap::ArcSwap<AppState>` loads, maps fields onto widgets, and emits
//! non-blocking [`tokio::sync::mpsc`] commands on user interaction.
//! Business logic and persistence are forbidden here.
//!
//! NOTE: `#![forbid(unsafe_code)]` is intentionally *not* set in this crate:
//! Slint's generated rendering code requires `unsafe`. This is the only
//! workspace crate with that exception (see `AGENTS.md` and the ADR on
//! toolchain & quality gates).

slint::include_modules!();

pub mod actions;
pub mod adapter;
pub mod fonts;
pub mod keymap;
pub mod menu;
pub mod resolver;
pub mod router;
pub mod shell;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use arc_swap::ArcSwap;
use slint::ComponentHandle as _;
use tokio::sync::{broadcast, mpsc};

use taskboard_domain::{AppState, EngineSignal, StateCommand, SystemEvent};

#[cfg(test)]
mod test_platform;
#[cfg(test)]
mod token_tests;

/// Validated configuration values handed over by `taskboard-app` (the
/// UI crate reads no configuration itself — IO-free by design; design
/// §1.2).
#[derive(Debug, Clone, PartialEq)]
pub struct UiSettings {
    /// The `[ui] scale` value: `Theme.base-scale`'s initial value
    /// (§6.1), validated app-side to the 0.5–3.0 range.
    pub scale: f64,
    /// The validated effective keymap (D14): preset ⊕ surviving
    /// overrides, built app-side — the crate receives typed values.
    pub keymap: keymap::Keymap,
    /// The Q15 startup-warning state (count of dropped/lossy keymap
    /// findings), rendered by chunk 3's status bar.
    pub startup: adapter::StartupWarnings,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            scale: 1.0,
            keymap: keymap::Keymap::from_preset(keymap::Preset::Standard),
            startup: adapter::StartupWarnings::default(),
        }
    }
}

/// Mount failures, typed (thiserror, the app-crate error convention).
/// (Font registration is deliberately absent: the shared-collection
/// registration path is infallible by construction — it fails by
/// panicking, which surfaces before window creation and is not a
/// recoverable mount outcome.)
#[derive(Debug, thiserror::Error)]
pub enum UiMountError {
    /// Slint's window/component creation failed (platform init,
    /// adapter creation).
    #[error("window creation failed")]
    WindowCreation(#[source] slint::PlatformError),
}

/// Event-loop failures, typed.
#[derive(Debug, thiserror::Error)]
pub enum UiRunError {
    /// Showing the window before entering the loop failed.
    #[error("window show failed")]
    WindowShow(#[source] slint::PlatformError),
    /// The slint event loop returned an error.
    #[error("event loop failed")]
    EventLoop(#[source] slint::PlatformError),
}

/// The mounted UI: the window instance plus the seam halves chunks 2–3
/// snap into (the resolver dispatches through `commands`, the views
/// read `state`, the status bar reads `system`/`signals`). No
/// machinery is built around them in chunk 1 — undrained broadcast
/// receivers merely go `Lagged`, which is inert.
pub struct UiHandle {
    window: AppWindow,
    /// The keymap adapter (chunk 2): interception, shell state, and
    /// the `set_view_context`/`set_focused_entities` seam (D16/D17).
    adapter: Rc<RefCell<adapter::Adapter>>,
    /// Lock-free state reads for the rendering thread (AGENTS §3).
    _state: Arc<ArcSwap<AppState>>,
    /// Engine repaint triggers (design §1.2; chunk 3's consumers).
    _signals: broadcast::Receiver<EngineSignal>,
    /// System status events (chunk 3's status bar).
    _system: broadcast::Receiver<SystemEvent>,
}

/// Creates the window and applies the theme state (theme init is a
/// window-construction step, §3.1): registers the bundled fonts
/// (idempotent), instantiates [`AppWindow`] on the platform's backend,
/// and pushes `settings.scale` into `Theme.base-scale` through the
/// generated global setter.
///
/// # Errors
///
/// [`UiMountError`] — typed, see the variant docs.
pub fn mount(
    state: Arc<ArcSwap<AppState>>,
    signals: broadcast::Receiver<EngineSignal>,
    system: broadcast::Receiver<SystemEvent>,
    commands: mpsc::Sender<StateCommand>,
    settings: UiSettings,
) -> Result<UiHandle, UiMountError> {
    fonts::register_bundled_fonts();
    let window = AppWindow::new().map_err(UiMountError::WindowCreation)?;
    // The §6.1 root multiplier, one line per window construction — a
    // future second window re-runs it; no global-once init exists.
    // Slint `float` properties are f32; the validated 0.5–3.0 range
    // converts with precision far below any visible scale step.
    #[allow(clippy::cast_possible_truncation)]
    window
        .global::<Theme>()
        .set_base_scale(settings.scale as f32);
    // The keymap is applied one-way at UI start (§5.6): install the
    // root-scope interception over the validated keymap.
    let adapter = adapter::install(&window, settings.keymap, settings.startup, commands);
    Ok(UiHandle {
        window,
        adapter,
        _state: state,
        _signals: signals,
        _system: system,
    })
}

/// Runs the slint event loop on the calling thread — the process main
/// thread (winit's macOS requirement; the multi-thread tokio runtime's
/// workers keep driving every actor while `block_on` is blocked here).
///
/// # Errors
///
/// [`UiRunError`] — typed, see the variant docs.
pub fn run(handle: UiHandle) -> Result<(), UiRunError> {
    // Consumes the handle: the window lives for the loop's duration
    // and drops with it when the loop returns.
    let UiHandle { window, .. } = handle;
    window.show().map_err(UiRunError::WindowShow)?;
    slint::run_event_loop().map_err(UiRunError::EventLoop)
}

/// Requests the event loop to quit (the exit watcher's surface; a
/// no-op when no loop is running).
pub fn quit() {
    let _ = slint::quit_event_loop();
}

#[cfg(test)]
mod mount_tests {
    use super::*;
    use crate::test_platform::on_slint_thread;

    fn handover() -> (
        Arc<ArcSwap<AppState>>,
        broadcast::Receiver<EngineSignal>,
        broadcast::Receiver<SystemEvent>,
        mpsc::Sender<StateCommand>,
    ) {
        let (commands_tx, _commands_rx) = mpsc::channel(64);
        let (system_tx, system_rx) = broadcast::channel(16);
        let (signals_tx, signals_rx) = broadcast::channel(16);
        drop(system_tx);
        drop(signals_tx);
        (
            Arc::new(ArcSwap::from_pointee(AppState::default())),
            signals_rx,
            system_rx,
            commands_tx,
        )
    }

    /// The dispatch acceptance line: a valid scale reaches
    /// `Theme.base-scale` via the generated `Global` setter.
    #[test]
    fn mount_pushes_the_configured_scale_into_the_theme() {
        on_slint_thread(|| {
            let (state, signals, system, commands) = handover();
            let handle = mount(
                state,
                signals,
                system,
                commands,
                UiSettings {
                    scale: 2.0,
                    ..UiSettings::default()
                },
            )
            .expect("mount succeeds on the null platform");
            let base_scale = handle.window.global::<Theme>().get_base_scale();
            assert_eq!(base_scale, 2.0);
        });
    }

    #[test]
    fn mount_defaults_the_theme_scale_to_one() {
        on_slint_thread(|| {
            let (state, signals, system, commands) = handover();
            let handle = mount(state, signals, system, commands, UiSettings::default())
                .expect("mount succeeds on the null platform");
            assert_eq!(handle.window.global::<Theme>().get_base_scale(), 1.0);
        });
    }

    /// Theme init is a window-construction step (§3.1): mount produces
    /// a window instance that `run()` will show — still hidden here
    /// (the event loop never ran).
    #[test]
    fn mount_produces_a_hidden_window_instance() {
        on_slint_thread(|| {
            let (state, signals, system, commands) = handover();
            let handle = mount(state, signals, system, commands, UiSettings::default())
                .expect("mount succeeds on the null platform");
            assert!(!handle.window.window().is_visible());
        });
    }
}

#[cfg(test)]
mod font_smoke_tests {
    use slint::ComponentHandle as _;

    use crate::fonts::register_bundled_fonts;
    use crate::test_platform::on_slint_thread;

    /// D11/D14 smoke: with the null platform registered, the bundled
    /// fonts register, a window component instantiates (its font
    /// properties resolve against the registered families' names), and
    /// the instance drops cleanly — no panic anywhere in the cycle.
    #[test]
    fn registration_and_instantiation_smoke() {
        on_slint_thread(|| {
            register_bundled_fonts(); // idempotent; a second call is a no-op
            register_bundled_fonts();

            let window = crate::AppWindow::new().expect("window instantiates on the null platform");
            // The theme's Typography globals resolve their family names;
            // reading them exercises the generated global accessors.
            let _ = window.global::<crate::Typography>().get_family_sans();
            let _ = window.global::<crate::Typography>().get_family_mono();
            drop(window);
        });
    }
}

#[cfg(test)]
mod keymap_adapter_tests {
    use super::*;
    use crate::keymap::{Preset, RawKeymap, build};
    use crate::test_platform::on_slint_thread;
    use slint::Model as _;
    use slint::platform::WindowEvent;
    use std::collections::BTreeMap;

    fn vi_keymap() -> keymap::Keymap {
        build(&RawKeymap {
            preset: Preset::Vi,
            overrides: BTreeMap::new(),
        })
        .keymap
    }

    fn handover_vi() -> (UiHandle, mpsc::Receiver<StateCommand>) {
        let (commands_tx, commands_rx) = mpsc::channel(8);
        let (system_tx, system_rx) = broadcast::channel(4);
        let (signals_tx, signals_rx) = broadcast::channel(4);
        drop(system_tx);
        drop(signals_tx);
        let handle = mount(
            Arc::new(ArcSwap::from_pointee(AppState::default())),
            signals_rx,
            system_rx,
            commands_tx,
            UiSettings {
                keymap: vi_keymap(),
                ..UiSettings::default()
            },
        )
        .expect("mount succeeds on the null platform");
        (handle, commands_rx)
    }

    /// Injects one key through the same public dispatch path real
    /// backends use (the root `FocusScope`'s capture handler; focused
    /// by the window's `init`).
    fn press(handle: &UiHandle, text: &str) {
        handle
            .window
            .window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
    }

    /// T25 — the generated menu journey: `app.menu` (vi `:`) opens
    /// the menu, `menu.next` (`j`) moves the cursor, and invoking an
    /// entry closes the menu and re-dispatches the entry's action.
    #[test]
    fn injected_keys_open_navigate_and_invoke_the_menu() {
        on_slint_thread(|| {
            let (handle, _commands) = handover_vi();
            let keyboard = handle.window.global::<KeyboardState>();

            press(&handle, ":");
            assert!(
                keyboard.get_menu_open(),
                "app.menu opens the generated menu"
            );
            assert_eq!(keyboard.get_menu_selected_index(), 0);
            assert!(
                !keyboard
                    .get_menu_entries()
                    .row_data(0)
                    .expect("rows")
                    .label
                    .is_empty()
            );

            press(&handle, "j");
            assert_eq!(
                keyboard.get_menu_selected_index(),
                1,
                "menu.next moves the cursor"
            );
            press(&handle, "k");
            assert_eq!(
                keyboard.get_menu_selected_index(),
                0,
                "menu.previous moves back"
            );

            // Invoke the selected entry (app.exit is the first
            // application row): the menu closes; the re-dispatched
            // action is the adapter's to route.
            press(&handle, "\u{a}");
            assert!(!keyboard.get_menu_open(), "invoke closes the menu");
        });
    }

    /// T26 — the chunk-2 end-to-end path: a `task.complete` keystroke
    /// with focused entities set sends the exact `StateCommand` to
    /// the mpsc receiver (the forwarder boundary).
    #[test]
    fn complete_keystroke_sends_the_exact_command() {
        on_slint_thread(|| {
            let (commands_tx, mut commands_rx) = mpsc::channel(8);
            let (system_tx, system_rx) = broadcast::channel(4);
            let (signals_tx, signals_rx) = broadcast::channel(4);
            drop(system_tx);
            drop(signals_tx);
            let handle = mount(
                Arc::new(ArcSwap::from_pointee(AppState::default())),
                signals_rx,
                system_rx,
                commands_tx,
                UiSettings::default(), // standard preset
            )
            .expect("mount succeeds on the null platform");

            // The chunk-3 focus seam, driven from the test side: a
            // board view with a focused task.
            let task = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
            handle.set_view_context(Some(actions::ContextId::Board));
            handle.set_focused_entities(Some(task), None);

            press(&handle, " "); // task.complete = Space in Board
            let command = commands_rx
                .try_recv()
                .expect("the command reached the mpsc");
            assert_eq!(
                command,
                StateCommand::SetTaskDone {
                    id: task,
                    done: true
                }
            );

            press(&handle, "\u{7f}"); // task.delete = Delete in Board
            let command = commands_rx.try_recv().expect("the delete command arrived");
            assert_eq!(command, StateCommand::DeleteTask { id: task });

            assert!(commands_rx.try_recv().is_err(), "nothing else dispatched");
        });
    }

    /// T27 — the §5.7 status signal: a consumed prefix pushes
    /// prefix-pending, a failed continuation pushes prefix-abort, and
    /// an unbound key dispatches nothing (rejected event).
    #[test]
    fn prefix_feedback_and_unhandled_keys() {
        on_slint_thread(|| {
            let (handle, mut commands) = handover_vi();
            let keyboard = handle.window.global::<KeyboardState>();

            press(&handle, "d"); // prefix of `dd` (task.delete in ViNormal)
            assert_eq!(keyboard.get_last_feedback(), KeyFeedback::PrefixPending);

            press(&handle, "x"); // not the second `d`: prefix-abort
            assert_eq!(keyboard.get_last_feedback(), KeyFeedback::PrefixAbort);
            assert!(commands.try_recv().is_err(), "abort dispatches nothing");

            press(&handle, "q"); // unbound everywhere in the vi chain
            assert_eq!(keyboard.get_last_feedback(), KeyFeedback::None);
            assert!(commands.try_recv().is_err());

            // The abort left the resolver idle: `dd` completes after.
            let task = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
            handle.set_focused_entities(Some(task), None);
            press(&handle, "d");
            press(&handle, "d");
            let command = commands.try_recv().expect("the chord completed");
            assert_eq!(command, StateCommand::DeleteTask { id: task });
        });
    }

    /// Mode flips are ordinary effects (D10): `i` enters insert, and
    /// Escape (its only binding) returns — the chain follows the mode
    /// state.
    #[test]
    fn mode_flips_are_ordinary_state() {
        on_slint_thread(|| {
            let (handle, _commands) = handover_vi();
            let keyboard = handle.window.global::<KeyboardState>();
            assert!(!keyboard.get_vi_insert());
            press(&handle, "i");
            assert!(keyboard.get_vi_insert(), "mode.insert flipped the state");
            press(&handle, "\u{1b}");
            assert!(!keyboard.get_vi_insert(), "mode.normal returned");
        });
    }
}

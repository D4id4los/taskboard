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
pub mod fonts;
pub mod keymap;
pub mod menu;
pub mod resolver;
pub mod router;
pub mod shell;

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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiSettings {
    /// The `[ui] scale` value: `Theme.base-scale`'s initial value
    /// (§6.1), validated app-side to the 0.5–3.0 range.
    pub scale: f64,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self { scale: 1.0 }
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
    /// Lock-free state reads for the rendering thread (AGENTS §3).
    _state: Arc<ArcSwap<AppState>>,
    /// Engine repaint triggers (design §1.2; chunk 3's consumers).
    _signals: broadcast::Receiver<EngineSignal>,
    /// System status events (chunk 3's status bar).
    _system: broadcast::Receiver<SystemEvent>,
    /// The UI → engine command sender (chunk 2's resolver).
    _commands: mpsc::Sender<StateCommand>,
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
    Ok(UiHandle {
        window,
        _state: state,
        _signals: signals,
        _system: system,
        _commands: commands,
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
            let handle = mount(state, signals, system, commands, UiSettings { scale: 2.0 })
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

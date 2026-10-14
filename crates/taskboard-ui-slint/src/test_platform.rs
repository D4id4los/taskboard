// SPDX-License-Identifier: AGPL-3.0-only
//! Headless test platform (chunk decision D14): a dev-only
//! [`slint::platform::Platform`] implementation so component
//! instantiation tests run with no display and no GPU — nothing
//! renders. nextest's process-per-test model makes the process-global
//! `set_platform` registration safe; `cargo test --doc` never
//! instantiates components.

#![cfg(test)]

use std::rc::{Rc, Weak};

use slint::platform::software_renderer::SoftwareRenderer;
use slint::platform::{Platform, PlatformError, Renderer, WindowAdapter};
use slint::{PhysicalSize, Window};

/// A platform that hands out null window adapters: every operation is
/// a no-op, sizes are 0×0, and no event loop can run (tests never call
/// `run_event_loop`).
#[derive(Default)]
pub(crate) struct NullPlatform;

/// The null window adapter: a real [`Window`] object (so component
/// instances can bind to it) with a no-op backend behind it. The
/// renderer is the software renderer — never asked to draw (nothing
/// renders), but constructible headlessly, unlike the GL backends.
struct NullWindowAdapter {
    window: Window,
    renderer: SoftwareRenderer,
}

impl NullWindowAdapter {
    fn new() -> Rc<Self> {
        Rc::new_cyclic(|weak: &Weak<Self>| Self {
            window: Window::new(weak.clone()),
            renderer: SoftwareRenderer::new(),
        })
    }
}

impl WindowAdapter for NullWindowAdapter {
    fn window(&self) -> &Window {
        &self.window
    }

    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }

    fn size(&self) -> PhysicalSize {
        PhysicalSize::new(0, 0)
    }
}

impl Platform for NullPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(NullWindowAdapter::new())
    }
}

/// Registers the null platform as Slint's process-global platform.
///
/// # Panics
///
/// Panics if a platform is already registered — under nextest every
/// test binary registers exactly once (the helper is call-idempotent
/// per process via `Once`).
pub(crate) fn ensure_null_platform() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        slint::platform::set_platform(Box::new(NullPlatform))
            .expect("null platform registers exactly once per test process");
    });
}

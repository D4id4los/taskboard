// SPDX-License-Identifier: AGPL-3.0-only
//! Headless test platform (chunk decision D14): a dev-only
//! [`slint::platform::Platform`] implementation so component
//! instantiation tests run with no display and no GPU — nothing
//! renders. nextest's process-per-test model makes the process-global
//! `set_platform` registration safe; `cargo test --doc` never
//! instantiates components.

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

/// Registers the null platform as Slint's process-global platform and
/// runs `f` on the dedicated slint test thread, returning its result.
///
/// Slint's platform (and font context) is process-global *and
/// thread-affine*: it must be registered and used from one thread for
/// the whole process lifetime. A single long-lived worker thread owns
/// it — it registers the null platform before its first job — so
/// component tests are correct under both nextest's process-per-test
/// model and `cargo test`'s threaded runner (llvm-cov).
///
/// # Panics
///
/// Panics if the platform is already registered on the worker (never:
/// the worker registers exactly once) or if the worker died.
pub(crate) fn on_slint_thread<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    use std::sync::mpsc;
    type Job = Box<dyn FnOnce() + Send>;

    static SENDER: std::sync::OnceLock<mpsc::Sender<Job>> = std::sync::OnceLock::new();
    let sender = SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("slint-tests".into())
            .spawn(move || {
                slint::platform::set_platform(Box::new(NullPlatform))
                    .expect("null platform registers exactly once per test process");
                for job in rx {
                    job();
                }
            })
            .expect("the slint test worker spawns");
        tx
    });

    let (reply_tx, reply_rx) = mpsc::channel();
    sender
        .send(Box::new(move || {
            let result = f();
            let _ = reply_tx.send(result);
        }))
        .expect("the slint test worker is alive");
    reply_rx
        .recv()
        .expect("the slint test worker returned a result")
}

// SPDX-License-Identifier: AGPL-3.0-only

//! The alarm window's drag-applier seam suite (W-2, #1308 US-2/S2):
//! the delta accumulator and the platform arms, driven over
//! handwritten fakes — never a real window manager (the round law:
//! the drag pin is the delta-accumulator seam test). Assertions are
//! typed outcomes (`matches!`) — no log/message text (AGENTS §5).
//!
//! Platform postures under test:
//! - **delta arm** (X11/Windows): per-event read-modify-write against
//!   the sink's live position;
//! - **Wayland arm** (R-2 as amended 2026-11-24): the press requests
//!   the compositor-driven interactive move; client deltas are
//!   dropped for the gesture's duration;
//! - **fallback arm**: no placement channel (non-winit backend, or a
//!   Wayland refusal) — the typed no-op, never a wrong position.

use std::cell::Cell;
use std::rc::Rc;

use taskboard_ui_slint::{
    AlarmPlacementSink, CompositorMoveUnsupported, DragApplier, DragApply, DragArm,
};

/// The handwritten fake (AGENTS: fakes over mocks): a position sink
/// recording every `set_position` call, configurable per posture —
/// `compositor_grants` decides whether the interactive-move request
/// succeeds.
struct FakeSink {
    pos: Cell<(f32, f32)>,
    placements: std::cell::RefCell<Vec<(f32, f32)>>,
    compositor_grants: bool,
}

impl FakeSink {
    fn granted(compositor_grants: bool) -> Rc<Self> {
        Rc::new(Self {
            pos: Cell::new((0.0, 0.0)),
            placements: std::cell::RefCell::new(Vec::new()),
            compositor_grants,
        })
    }
}

impl AlarmPlacementSink for FakeSink {
    fn position(&self) -> (f32, f32) {
        self.pos.get()
    }

    fn set_position(&self, x: f32, y: f32) {
        self.pos.set((x, y));
        self.placements.borrow_mut().push((x, y));
    }

    fn request_compositor_move(&self) -> Result<(), CompositorMoveUnsupported> {
        if self.compositor_grants {
            Ok(())
        } else {
            Err(CompositorMoveUnsupported)
        }
    }

    fn placement_available(&self) -> bool {
        true
    }
}

/// Owned-sink adapter over the shared fake (the applier takes the
/// sink by value; the tests share the recording fake through `Rc`).
struct SinkHandle(Rc<FakeSink>);

impl AlarmPlacementSink for SinkHandle {
    fn position(&self) -> (f32, f32) {
        self.0.position()
    }
    fn set_position(&self, x: f32, y: f32) {
        self.0.set_position(x, y);
    }
    fn request_compositor_move(&self) -> Result<(), CompositorMoveUnsupported> {
        self.0.request_compositor_move()
    }
    fn placement_available(&self) -> bool {
        self.0.placement_available()
    }
}

/// The delta arm (X11/Windows): each strip move lands the accumulated
/// delta — every apply is a read-modify-write against the sink's live
/// position, so the sum of the increments is exact and no update is
/// lost (US-2/AC-1).
#[test]
fn us2_s2_drag_by_applies_the_accumulated_delta() {
    let sink = FakeSink::granted(false);
    sink.pos.set((120.0, 80.0));
    let mut applier = DragApplier::with_session(SinkHandle(Rc::clone(&sink)), false);
    assert!(
        matches!(applier.press(), DragArm::Delta),
        "the X11 posture arms the delta accumulator"
    );
    assert!(matches!(
        applier.drag_by(15.0, 0.0),
        DragApply::Delta { x: 135.0, y: 80.0 }
    ));
    assert!(matches!(
        applier.drag_by(0.0, 7.5),
        DragApply::Delta { x: 135.0, y: 87.5 }
    ));
    let placements = sink.placements.borrow();
    assert_eq!(
        placements.as_slice(),
        [(135.0, 80.0), (135.0, 87.5)],
        "each move applied exactly its accumulated delta"
    );
}

/// The Wayland arm (R-2 amended): the press requests the
/// compositor-driven interactive move, and the drag deltas are
/// dropped while the compositor owns the gesture (US-2/AC-2) — the
/// typed outcome, faked seam: the request happened.
#[test]
fn us2_s2_wayland_arm_requests_the_compositor_move() {
    let sink = FakeSink::granted(true);
    sink.pos.set((10.0, 10.0));
    let mut applier = DragApplier::with_session(SinkHandle(Rc::clone(&sink)), true);
    assert!(
        matches!(applier.press(), DragArm::Compositor),
        "the Wayland posture requests the compositor-driven move"
    );
    assert!(
        matches!(applier.drag_by(40.0, 0.0), DragApply::CompositorMove),
        "a client delta under the compositor arm is dropped"
    );
    assert!(
        sink.placements.borrow().is_empty(),
        "the compositor arm never drives client placements"
    );
}

/// The fallback arm: without a placement channel (non-winit backend)
/// the press lands the typed no-op and no placement ever fires —
/// never a panic, never a wrong position.
#[test]
fn us2_s2_wayland_arm_falls_back_to_a_noop() {
    struct NoChannel;
    impl AlarmPlacementSink for NoChannel {
        fn position(&self) -> (f32, f32) {
            (0.0, 0.0)
        }
        fn set_position(&self, _x: f32, _y: f32) {
            panic!("the fallback arm never places");
        }
        fn request_compositor_move(&self) -> Result<(), CompositorMoveUnsupported> {
            Err(CompositorMoveUnsupported)
        }
        fn placement_available(&self) -> bool {
            false
        }
    }
    let mut applier = DragApplier::with_session(NoChannel, true);
    assert!(
        matches!(applier.press(), DragArm::Noop),
        "no placement channel: the fallback arm"
    );
    assert!(matches!(applier.drag_by(5.0, 5.0), DragApply::Noop));
}

/// The Wayland refusal arm: a compositor that refuses the
/// interactive move degrades to the fallback no-op too — never to a
/// client-side delta apply that would no-op into a wrong position
/// (R-2 amended).
#[test]
fn us2_s2_wayland_refusal_degrades_to_the_fallback_noop() {
    let sink = FakeSink::granted(false);
    let mut applier = DragApplier::with_session(SinkHandle(Rc::clone(&sink)), true);
    assert!(
        matches!(applier.press(), DragArm::Noop),
        "a Wayland compositor refusal degrades to the fallback no-op"
    );
    assert!(matches!(applier.drag_by(5.0, 5.0), DragApply::Noop));
    assert!(sink.placements.borrow().is_empty());
}

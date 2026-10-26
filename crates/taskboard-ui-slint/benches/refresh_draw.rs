// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Refresh+draw benches (kiosk chunk 6, K6/S2 — perf-budgets report
//! row 3): the refresh leg and the software draw leg of one full UI
//! pass, **timed separately, never one combined number**, at board
//! scale N ∈ {100, 1 000, 10 000} on the deterministic fixture.
//!
//! Budget context (row 3, stated not asserted): full-frame repaint
//! ≤ 50 ms / partial ≤ 20 ms @ 1366×768 software renderer on one
//! 1.5 GHz in-order core. These are **dev-machine references, never
//! gates** — the on-device verdict belongs to the phase-B spike. The
//! composed input→visible latency stays floored by the 100 ms repaint
//! cadence on any hardware (C2/F13-09, recorded; not measured here —
//! the legs are timer-free by design).
//!
//! View pinning (the 2026-10-26 probe's instrument fix, #460): mount
//! seeds a view from `UiSettings.kiosk` (`adapter.rs`: kiosk → Board,
//! otherwise Today), and the draw lanes measure whatever view is
//! mounted — the pre-pinning lanes ran on the Today view unlabelled,
//! where most fixture flips render nowhere (honest 0×0) and the
//! overdue flips are filtered-list removals. The draw lanes now come
//! in a `today` and a `board` pin each, with a different flip shape
//! per pin: a Today flip is an **overdue-list removal** (the reflow
//! damage of a filtered list), a Board flip is an **in-place card
//! change** on the eagerly-instantiated board. Both need to be
//! performant; neither number subsumes the other.
//!
//! Lanes (plan D8, re-pinned):
//!
//! - `refresh/{n}` — the public `UiHandle::refresh()` pass:
//!   `ArcSwap` load → `view_state.reconcile` → `models::push`
//!   diff-push (`adapter.rs` refresh). Deliberately unpinned: the
//!   pass derives and pushes **every** view's models regardless of
//!   which one is mounted, so there is one refresh cost per N.
//! - `draw-full/{today|board}/{n}` — one software render into a
//!   pre-allocated 1366×768 caller buffer, default `NewBuffer`
//!   repaint type (full window redrawn each call).
//! - `draw-partial/{today|board}/{n}` — `ReusedBuffer` dirty tracking
//!   over the kiosk steady-state framebuffer model: unmeasured
//!   per-iteration setup (one flip-target task's `done` toggled on its
//!   current value → `ArcSwap` store → refresh), measured routine =
//!   one render. The flip targets are the view's rendered head rows
//!   (`support::today_flip_targets` / `board_flip_targets`), and a
//!   preamble guard asserts the first flip's region is nonzero — a
//!   lane whose "guaranteed change" paints nothing is mis-instrumented
//!   (the 0×0 anomaly's lesson: the guaranteed one-model-row change
//!   used to be, unlabelled, an unrendered change on most targets).
//!   The reported region is the finding-carrier (max across
//!   iterations = the honest worst case; the last iteration rides
//!   along for warm-up vs steady-state separation), never hidden.
//!
//! No GL/llvmpipe path anywhere: the software renderer is pure CPU
//! over a caller buffer (`slint::platform::software_renderer`, the
//! linuxkms production path). No tracing subscriber is installed
//! (the mount-tail `taskboard_perf` event stays a
//! disabled-dispatcher no-op — the storage bench's D9 stance). No
//! visual-correctness assertions (the golden lane owns those).

#[allow(dead_code)] // `on_slint_thread` serves the lib's unit tests only
#[path = "../src/test_platform.rs"]
mod seam;

#[path = "support/mod.rs"]
mod support;

use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use chrono::TimeZone as _;
use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use slint::ComponentHandle as _;
use taskboard_domain::TaskId;
use taskboard_ui_slint::requests;
use taskboard_ui_slint::{UiHandle, UiSettings, mount};

use support::{BENCH_HEIGHT, BENCH_WIDTH, N_SIZES, build_state};

/// The view pins of the draw lanes (the lever is the mount's own
/// kiosk bit — `adapter.rs` seeds Board on kiosk, Today otherwise).
/// The Today pin carries the desktop default view; the Today pass's
/// kiosk-side board filter (`adapter.rs` kiosk gate) is inert on this
/// single-board fixture.
const VIEW_PINS: [(&str, bool); 2] = [("today", false), ("board", true)];

/// The fixed `done` stamp the partial lane flips un-done tasks to
/// (arbitrary fixed instant; the fixture never produces it — its own
/// stamps come from `ts(order + 2_000_000)`).
const PARTIAL_STAMP_SECS: i64 = 1_704_067_200 + 9_000_000;

/// Platform guard (plan D9): slint's platform is process-global and
/// thread-affine; criterion (no rayon) runs everything on the main
/// thread, which is where the null platform registers — once.
fn ensure_platform() {
    use std::sync::OnceLock;
    static PLATFORM: OnceLock<()> = OnceLock::new();
    PLATFORM.get_or_init(|| {
        slint::platform::set_platform(Box::new(seam::NullPlatform))
            .expect("the null platform registers exactly once per bench process");
    });
}

/// Mounts the fixture state windowed at the row-3 budget's own
/// 1366×768 (plan D8/D10: the budget row is stated in
/// software-renderer terms at a size, not fullscreen-kiosk terms),
/// pushes the first refresh, and hands back the seam halves with the
/// pinned view's flip targets.
fn mounted(n: usize, kiosk: bool) -> Bench {
    ensure_platform();
    let state = Arc::new(ArcSwap::from_pointee(build_state(n)));
    let (commands_tx, _rx) = tokio::sync::mpsc::channel(64);
    let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
    let (_stx, signals) = tokio::sync::broadcast::channel(16);
    let (_syt, system) = tokio::sync::broadcast::channel(16);
    let handle = mount(
        Arc::clone(&state),
        signals,
        system,
        commands_tx,
        requests_tx,
        UiSettings {
            kiosk,
            ..UiSettings::default()
        },
    )
    .expect("mount succeeds on the null platform");
    handle
        .window()
        .window()
        .set_size(slint::WindowSize::Physical(slint::PhysicalSize::new(
            u32::try_from(BENCH_WIDTH).expect("width fits u32"),
            u32::try_from(BENCH_HEIGHT).expect("height fits u32"),
        )));
    let adapter = seam::window_adapter().expect("the seam registry holds the mounted adapter");
    let flip_targets = if kiosk {
        support::board_flip_targets(&state.load())
    } else {
        support::today_flip_targets(&state.load())
    };
    assert!(
        !flip_targets.is_empty(),
        "the pinned view must render at least one flip target at N={n}"
    );
    handle.refresh();
    Bench {
        handle,
        adapter,
        state,
        flip_targets,
    }
}

/// The seam halves the lanes drive, plus the pinned view's flip
/// targets (the tasks a one-row change provably renders).
struct Bench {
    handle: UiHandle,
    adapter: std::rc::Rc<seam::NullWindowAdapter>,
    state: Arc<ArcSwap<taskboard_domain::AppState>>,
    flip_targets: Vec<TaskId>,
}

impl Bench {
    /// One unmeasured setup step (plan D6's shape, view-pinned): flip
    /// the rotating target's *current* `done` value — a guaranteed
    /// one-model-row change on the pinned view's rendered scene
    /// (membership removal on Today, in-place card change on Board) —
    /// then store and refresh. The stamp
    /// (`PARTIAL_STAMP_SECS`) sits outside the fixture's own stamp
    /// space, so `None → Some` is always a real change.
    fn flip_next(&self, rotate: &std::cell::Cell<usize>) {
        let len = self.flip_targets.len();
        let i = rotate.get() % len;
        rotate.set((i + 1) % len);
        let mut next = (**self.state.load()).clone();
        if let Some(task) = next.tasks.get_mut(&self.flip_targets[i]) {
            task.done = if task.done.is_none() {
                chrono::Utc.timestamp_opt(PARTIAL_STAMP_SECS, 0).single()
            } else {
                None
            };
        }
        self.state.store(Arc::new(next));
        self.handle.refresh();
    }
}

/// Per-N measurement budget (the storage benches' convention): the
/// 10 000 tier drops to criterion's minimum sample size and a bounded
/// measurement time (plan D8; risk 5's deviation threshold: a
/// pathological > 5 s/iter tier would drop to 10 s, recorded). The
/// lower tiers re-pin criterion's defaults explicitly — the group
/// object outlives one (view, N) cell now that both views share it.
fn configure_group(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    n: usize,
) {
    if n >= 10_000 {
        group.sample_size(10);
        group.measurement_time(Duration::from_secs(15));
    } else {
        group.sample_size(100);
        group.warm_up_time(Duration::from_secs(3));
        group.measurement_time(Duration::from_secs(5));
    }
}

#[must_use]
fn element_throughput(n: usize) -> Throughput {
    Throughput::Elements(u64::try_from(n).expect("task count fits u64"))
}

/// The refresh leg (plan D5): the entire audit-defined refresh pass
/// through the public `UiHandle::refresh()` — no production code
/// changes, no `pub(crate)` widening. One lane per N, not per view:
/// the pass derives and pushes every view's models regardless of the
/// mounted one.
fn refresh_lanes(c: &mut Criterion) {
    let mut group = c.benchmark_group("refresh");
    for n in N_SIZES {
        group.throughput(element_throughput(n));
        configure_group(&mut group, n);
        let bench = mounted(n, false);
        group.bench_with_input(criterion::BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| bench.handle.refresh());
        });
    }
    group.finish();
}

/// The full-frame draw leg (plan D6), per view pin: pre-allocated
/// buffer outside the measured closure, default `NewBuffer` repaint
/// type. The setup render's region is the structural guard — a lane
/// that measures anything less than the full window panics loudly
/// rather than silently mis-reporting.
fn draw_full_lanes(c: &mut Criterion) {
    let mut group = c.benchmark_group("draw-full");
    for (view, kiosk) in VIEW_PINS {
        for n in N_SIZES {
            group.throughput(element_throughput(n));
            configure_group(&mut group, n);
            let bench = mounted(n, kiosk);
            let mut buffer = vec![slint::Rgb8Pixel::new(0, 0, 0); BENCH_WIDTH * BENCH_HEIGHT];
            let stride = BENCH_WIDTH;
            let warmup = bench.adapter.render_into(&mut buffer, stride);
            assert_eq!(
                warmup.bounding_box_size(),
                slint::PhysicalSize::new(
                    u32::try_from(BENCH_WIDTH).expect("width fits u32"),
                    u32::try_from(BENCH_HEIGHT).expect("height fits u32"),
                ),
                "draw-full/{view}/{n} must measure a full-frame render (NewBuffer)"
            );
            eprintln!(
                "[refresh_draw] draw-full/{view}/{n}: warmup region {}×{} (full frame)",
                warmup.bounding_box_size().width,
                warmup.bounding_box_size().height
            );
            group.bench_with_input(
                criterion::BenchmarkId::new(view, n.to_string()),
                &n,
                |b, _| b.iter(|| bench.adapter.render_into(&mut buffer, stride)),
            );
        }
    }
    group.finish();
}

/// The partial draw leg (plan D6), per view pin: `ReusedBuffer` dirty
/// tracking over the kiosk steady-state framebuffer. Unmeasured
/// per-iteration setup toggles one flip-target task's `done` on its
/// current value (rotating over the view's rendered head rows),
/// stores a fresh `Arc<AppState>`, and refreshes; the measured
/// routine is one render. The preamble guard is the lane's honesty
/// anchor: the first flip must produce a nonzero region, or the lane
/// panics — an invisible flip measures nothing (the 0×0 anomaly).
/// The reported region area is the finding-carrier (D6).
fn draw_partial_lanes(c: &mut Criterion) {
    let mut group = c.benchmark_group("draw-partial");
    for (view, kiosk) in VIEW_PINS {
        for n in N_SIZES {
            group.throughput(element_throughput(n));
            configure_group(&mut group, n);
            let bench = mounted(n, kiosk);
            bench.adapter.set_repaint_buffer_type(
                slint::platform::software_renderer::RepaintBufferType::ReusedBuffer,
            );
            let mut buffer = vec![slint::Rgb8Pixel::new(0, 0, 0); BENCH_WIDTH * BENCH_HEIGHT];
            let stride = BENCH_WIDTH;
            let full_area = u64::from(
                bench
                    .adapter
                    .render_into(&mut buffer, stride)
                    .bounding_box_size()
                    .width,
            ) * u64::try_from(BENCH_HEIGHT).expect("height fits u64");
            let rotate = std::cell::Cell::new(0_usize);
            let report = std::cell::Cell::new(None::<(u32, u32)>);
            let last = std::cell::Cell::new(None::<(u32, u32)>);

            // The visible-flip guarantee: flip the first target once
            // and require a painted region. On the Today pin the flip
            // is an overdue-list removal; on the Board pin an in-place
            // card change — either way it must render on the pinned
            // view, or this lane's contract ("one rendered one-task
            // change per iteration") is void.
            bench.flip_next(&rotate);
            let guard = bench
                .adapter
                .render_into(&mut buffer, stride)
                .bounding_box_size();
            assert!(
                guard.width > 0 && guard.height > 0,
                "draw-partial/{view}/{n}: the flip target must render on the pinned \
                 view; got a {}×{} region (the 2026-10-26 0×0 anomaly's lesson: an \
                 invisible flip is a mis-instrumented lane, not zero damage)",
                guard.width,
                guard.height
            );
            eprintln!(
                "[refresh_draw] draw-partial/{view}/{n}: visible-flip guard region \
                 {}×{}",
                guard.width, guard.height
            );
            // Flip the same target back and consume the reverse
            // damage unmeasured: the measured loop starts from the
            // pristine baseline.
            bench.flip_next(&rotate);
            bench.adapter.render_into(&mut buffer, stride);

            group.bench_with_input(
                criterion::BenchmarkId::new(view, n.to_string()),
                &n,
                |b, _| {
                    b.iter_batched(
                        || bench.flip_next(&rotate),
                        |()| {
                            let region = bench.adapter.render_into(&mut buffer, stride);
                            let (w, h) = (
                                region.bounding_box_size().width,
                                region.bounding_box_size().height,
                            );
                            // Keep the max-area region across iterations:
                            // the lane's contract is "one rendered changed
                            // row per render", so the max is the honest
                            // worst case; the last iteration is reported
                            // alongside it for warm-up vs steady-state
                            // separation.
                            let prev = report.get().unwrap_or((0, 0));
                            if u64::from(w) * u64::from(h) >= u64::from(prev.0) * u64::from(prev.1)
                            {
                                report.set(Some((w, h)));
                            }
                            last.set(Some((w, h)));
                        },
                        BatchSize::PerIteration,
                    );
                },
            );
            let (w, h) = report.get().expect("at least one measured render ran");
            let area = u64::from(w) * u64::from(h);
            let share = area
                .checked_mul(100)
                .and_then(|v| v.checked_div(full_area))
                .unwrap_or(0);
            eprintln!(
                "[refresh_draw] draw-partial/{view}/{n}: region {w}×{h} ({area} px = \
                 {share}% of the full frame)"
            );
            let (lw, lh) = last.get().expect("at least one measured render ran");
            let larea = u64::from(lw) * u64::from(lh);
            let lshare = larea
                .checked_mul(100)
                .and_then(|v| v.checked_div(full_area))
                .unwrap_or(0);
            eprintln!(
                "[refresh_draw] draw-partial/{view}/{n}: last-iteration region {lw}×{lh} \
                 ({larea} px = {lshare}% of the full frame)"
            );
        }
    }
    group.finish();
}

criterion_group!(benches, refresh_lanes, draw_full_lanes, draw_partial_lanes);
criterion_main!(benches);

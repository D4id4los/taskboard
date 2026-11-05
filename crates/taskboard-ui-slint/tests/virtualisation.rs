// SPDX-License-Identifier: AGPL-3.0-only

//! The virtualisation engagement suite (#373 stage-1 plan D-3): the
//! slint compiler's `ListView` contract is undocumented and
//! version-pinned — a lost element name or a lost length property
//! makes every wrapped `for` silently render **eager**, goldens and
//! journeys cannot see it, and every virtualisation win is forfeited.
//! These probes guard the contract at runtime, forever (the
//! `pin_guard.rs` discipline, applied at runtime instead of manifest
//! level): they must never be `#[ignore]`d or fixture-starved.
//!
//! The observable is the **live-instantiation count** via the
//! backend's per-instance element query (one entry per live
//! repetition instance) — no production-markup hooks. An eager shape
//! renders all N instances; an engaged shape renders only the
//! viewport window. The ratios asserted are failure detectors on the
//! eager garbage, deliberately not success ceilings: strictly better
//! virtualisation (a smaller window) still passes.

mod support;

use support::driver::{
    DriverOptions, SlintDriver, advance_probe_clock, find_all_by_id_on, find_by_label_on,
};

include!(concat!(env!("OUT_DIR"), "/virtualisation_probe.rs"));

use slint::{ComponentHandle as _, Model as _};

fn run_probe(job: impl FnOnce(&mut SlintDriver) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        |driver| {
            job(driver);
        },
    );
}

fn model_of(n: usize) -> slint::ModelRc<slint::SharedString> {
    slint::ModelRc::new(slint::VecModel::from(
        (0..n)
            .map(|i| slint::format!("card-{i}"))
            .collect::<Vec<_>>(),
    ))
}

/// The eager control: 300 delegates in a 400 px viewport all exist —
/// the probe's counting path works and the eager shape is what we
/// think it is.
#[test]
fn the_eager_control_instantiates_every_delegate() {
    run_probe(|_driver| {
        let probe = EagerProbe::new().expect("the eager probe instantiates");
        probe.show().expect("show on the testing platform");
        probe.set_items(model_of(300));
        advance_probe_clock(0);

        let instances = find_all_by_id_on(&probe, "EagerProbe::item");
        eprintln!("eager instances: {}", instances.len());
        assert_eq!(
            instances.len(),
            300,
            "the plain `for` must instantiate every delegate (the eager control)"
        );
        let first = find_by_label_on(&probe, "card-0");
        assert!(first.is_some(), "the counting path addresses delegates");
    });
}

/// The engaged probe: the identical delegate inside the real
/// `ListView` renders a viewport-scale window, not 300 instances. A
/// lost element name or any lost length property re-arms the eager
/// shape and fails the ratio (a failure detector, never a success
/// ceiling: a smaller window passes). The labelled-element guard
/// rejects the degenerate "engaged but renders nothing" breakage.
#[test]
fn the_list_view_engages_the_virtual_repeater() {
    run_probe(|_driver| {
        let probe = ListViewProbe::new().expect("the list-view probe instantiates");
        probe.show().expect("show on the testing platform");
        probe.set_items(model_of(300));
        probe.set_total(300);
        advance_probe_clock(0);

        let instances = find_all_by_id_on(&probe, "ListViewProbe::item");
        assert!(
            !instances.is_empty(),
            "an engaged ListView still renders its viewport"
        );
        assert!(
            instances.len() * 4 < 300,
            "the ListView must engage the compiler's virtual repeater: got {} \
             live instances of 300 — the eager shape means the element name or \
             one of the five length properties (content-y/height/width, \
             visible-height/width) no longer satisfies the compiler contract",
            instances.len()
        );
        assert!(
            find_by_label_on(&probe, "card-0").is_some(),
            "the engaged viewport renders its first delegate"
        );
    });
}

/// The seek smoke: driving the nonce edge to a deep row makes that
/// row live and lands it inside the viewport (the focus-into-view
/// seam's runtime half — the top-anchor seek through the runtime's
/// own average-height divisor).
#[test]
fn the_seek_makes_a_deep_row_live_and_visible() {
    run_probe(|_driver| {
        let probe = ListViewProbe::new().expect("the list-view probe instantiates");
        probe.show().expect("show on the testing platform");
        probe.set_items(model_of(300));
        probe.set_total(300);
        advance_probe_clock(0);

        assert!(
            find_by_label_on(&probe, "card-250").is_none(),
            "before the seek the deep row is off-viewport (not instantiated)"
        );

        probe.set_req_row(250);
        probe.set_req_nonce(1);
        advance_probe_clock(0);

        let target = find_by_label_on(&probe, "card-250")
            .unwrap_or_else(|| panic!("the seek instantiates the target row"));
        let position = target.absolute_position();
        let size = target.size();
        assert!(
            position.y >= 0.0 && position.y + size.height <= 400.0,
            "the sought row sits fully inside the 400 px viewport, got y={} h={}",
            position.y,
            size.height
        );
    });
}

// -- the row-anchored compensation legs (#592 stage 2) ---------------------

/// The probe-side mount for the compensation contract: 300 uniform
/// 64 px rows in a 400 px viewport, seek-driven to a deep window.
fn mount_anchor_probe(
    driver: &mut SlintDriver,
    anchored: bool,
) -> (
    RowAnchoredProbe,
    std::rc::Rc<slint::VecModel<slint::SharedString>>,
) {
    let probe = RowAnchoredProbe::new().expect("the row-anchor probe instantiates");
    probe.show().expect("show on the testing platform");
    let model: std::rc::Rc<slint::VecModel<slint::SharedString>> =
        std::rc::Rc::new(slint::VecModel::from(
            (0..300)
                .map(|i| slint::format!("card-{i}"))
                .collect::<Vec<_>>(),
        ));
    probe.set_items(model.clone().into());
    probe.set_total(300);
    probe.set_anchored(anchored);
    driver.advance_time(0);
    // Seek deep: the window head lands at row 200.
    probe.set_req_row(200);
    probe.set_req_nonce(1);
    driver.advance_time(0);
    (probe, model)
}

/// The push discipline's removal shape at `index`: a `set_row_data`
/// rebind cascade from the change point down plus one tail remove
/// (models.rs's `diff_rows`, the M-A mechanism fact).
fn probe_remove_at(model: &slint::VecModel<slint::SharedString>, index: usize, n: usize) {
    for i in index..n - 1 {
        let successor = slint::format!("card-{}", i + 1);
        model.set_row_data(i, successor);
    }
    model.remove(n - 1);
}

fn probe_witness_y(probe: &RowAnchoredProbe, label: &str) -> f32 {
    find_by_label_on(probe, label)
        .unwrap_or_else(|| panic!("witness {label} must be live"))
        .absolute_position()
        .y
}

/// The mode-off contract at the widget level (#592, the M-B control
/// made permanent): a head-model removal under a stable content
/// offset reflows the live rows up by exactly the removed row's
/// extent — the content-anchored law the default (mode-off) shape is
/// pinned on. With the mode ON and the exact extent supplied, the
/// same change is pixel-still: the compensation write survives the
/// runtime's own per-pass content-y rewrite and settles (two further
/// stable passes move the witness by zero).
#[test]
fn the_row_anchored_mode_compensates_the_above_viewport_removal() {
    run_probe(|driver| {
        const N: usize = 300;
        // Mode off: the content-anchored law.
        let (probe, model) = mount_anchor_probe(driver, false);
        let rest = probe_witness_y(&probe, "card-203");
        probe_remove_at(&model, 0, N);
        driver.advance_time(0);
        let shifted = probe_witness_y(&probe, "card-203") - rest;
        assert_eq!(
            shifted, -64.0,
            "mode off: the head removal reflows up by the extent"
        );

        // Mode on, exact extent: pixel-still, and it settles.
        let (probe, model) = mount_anchor_probe(driver, true);
        let rest = probe_witness_y(&probe, "card-203");
        probe.set_anchor_first_change(0);
        probe.set_anchor_row_delta(-1);
        probe.set_anchor_extent(64.0_f32);
        probe_remove_at(&model, 0, N);
        probe.set_anchor_nonce(1);
        driver.advance_time(0);
        assert_eq!(
            probe_witness_y(&probe, "card-203"),
            rest,
            "mode on with the exact extent: the witness is pixel-still"
        );
        driver.advance_time(0);
        driver.advance_time(0);
        assert_eq!(
            probe_witness_y(&probe, "card-203"),
            rest,
            "the compensation settles: no oscillation on further passes"
        );
    });
}

/// The extent sources: a supplied non-average extent is used verbatim
/// (the compensation is exactly `-delta × extent`); an unobserved
/// extent (0 px) falls back to the runtime average — exact here
/// because the probe's rows are uniform.
#[test]
fn the_compensation_uses_the_supplied_extent_and_falls_back_to_the_average() {
    run_probe(|driver| {
        const N: usize = 300;
        let (probe, model) = mount_anchor_probe(driver, true);
        let rest = probe_witness_y(&probe, "card-203");
        probe.set_anchor_first_change(0);
        probe.set_anchor_row_delta(-1);
        probe.set_anchor_extent(48.0_f32);
        probe_remove_at(&model, 0, N);
        probe.set_anchor_nonce(1);
        driver.advance_time(0);
        // Under-compensated by 64 - 48 = 16 px.
        assert_eq!(
            probe_witness_y(&probe, "card-203") - rest,
            -16.0,
            "the supplied extent drives the compensation verbatim"
        );

        let (probe, model) = mount_anchor_probe(driver, true);
        let rest = probe_witness_y(&probe, "card-203");
        probe.set_anchor_first_change(0);
        probe.set_anchor_row_delta(-1);
        // extent stays 0px: never observed live
        probe_remove_at(&model, 0, N);
        probe.set_anchor_nonce(1);
        driver.advance_time(0);
        assert_eq!(
            probe_witness_y(&probe, "card-203"),
            rest,
            "an unobserved extent falls back to the runtime average"
        );
    });
}

/// The discriminator (G-2's widget half): a change whose first
/// changed row is at or below the estimated window head never
/// compensates — the within-viewport close-up and the boundary band
/// keep the mode-off geometry (ambiguity falls back, never worse).
#[test]
fn within_window_and_boundary_changes_never_compensate() {
    run_probe(|driver| {
        const N: usize = 300;
        // A within-window removal (first change 201, one row into
        // the window): the rows below close up by the extent, no
        // compensation fights it.
        let (probe, model) = mount_anchor_probe(driver, true);
        let below = probe_witness_y(&probe, "card-203");
        probe.set_anchor_first_change(201);
        probe.set_anchor_row_delta(-1);
        probe.set_anchor_extent(64.0_f32);
        probe_remove_at(&model, 201, N);
        probe.set_anchor_nonce(1);
        driver.advance_time(0);
        assert_eq!(
            probe_witness_y(&probe, "card-203") - below,
            -64.0,
            "a within-window removal closes the rows below uncompensated"
        );

        // The boundary guard band: a change exactly at the estimated
        // window head (row 200) does not compensate.
        let (probe, model) = mount_anchor_probe(driver, true);
        let rest = probe_witness_y(&probe, "card-203");
        probe.set_anchor_first_change(200);
        probe.set_anchor_row_delta(-1);
        probe.set_anchor_extent(64.0_f32);
        probe_remove_at(&model, 200, N);
        probe.set_anchor_nonce(1);
        driver.advance_time(0);
        assert_eq!(
            probe_witness_y(&probe, "card-203") - rest,
            -64.0,
            "the one-average-row guard band at the window head never compensates"
        );
    });
}

// -- the board legs (stage-1 plan §3.2, M2) -------------------------------

#[path = "../benches/support/mod.rs"]
#[allow(dead_code)] // each binary consumes its own subset of the bench fixture
mod bench_support;

use std::sync::Arc;

use arc_swap::ArcSwap;
use support::driver::JourneyDriver as _;
use support::health::detached_health;
use taskboard_domain::{AppState, StateCommand};
use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::{UiHandle, UiSettings, mount};

use bench_support::build_state;

fn dummy_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::EngineSignal> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

fn dummy_system_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

/// Mounts the bench fixture through the public `mount()`, escapes to
/// the board (the goldens' idiom — the desktop default view is
/// Today), sizes the window, and instantiates the conditional trees
/// on the mock clock.
fn mount_board(
    driver: &mut SlintDriver,
    n: usize,
    width: u32,
    height: u32,
) -> (UiHandle, support::fake_engine::FakeEngine) {
    let state = Arc::new(ArcSwap::from_pointee(build_state(n)));
    let (commands, engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
    let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
    let handle = mount(
        state,
        dummy_receiver(),
        dummy_system_receiver(),
        commands,
        requests_tx,
        UiSettings::default(),
        detached_health(),
    )
    .expect("mount succeeds on the testing backend");
    handle.dispatch_action(ActionId::NavFocusBoard);
    driver.bind(&handle);
    driver.set_window_size(width, height);
    driver.advance_time(0);
    (handle, engine)
}

/// The bench fixture's card label (the address the board tests use).
fn bench_label(i: usize) -> String {
    format!("Task {i:05} bench fixture")
}

/// Engagement, board regular leg: a 1 000-task fixture in a
/// 1 000×900 window instantiates a viewport-scale window of cards per
/// column, not all 1 000. A failure detector on the eager garbage —
/// strictly better virtualisation still passes.
#[test]
fn the_board_regular_leg_virtualises() {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        |driver| {
            let (_handle, _engine) = mount_board(driver, 1000, 1000, 900);
            let instances = driver.find_all_by_id("BoardLayout::drag-zone");
            assert!(
                !instances.is_empty(),
                "the engaged board still renders its head cards"
            );
            assert!(
                instances.len() * 4 < 1000,
                "the board's task loops must render viewport-scale, got {} live \
                 cards of 1000 — the eager shape means a leg lost the ListView",
                instances.len()
            );
            assert!(
                driver.find_by_label(&bench_label(0)).is_some(),
                "the first card of column 0 renders"
            );
        },
    );
}

/// Engagement, board compact leg: the single switcher column
/// virtualises the same way (D-4b).
#[test]
fn the_board_compact_leg_virtualises() {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        |driver| {
            let (_handle, _engine) = mount_board(driver, 1000, 600, 900);
            let instances = driver.find_all_by_id("BoardLayout::compact-task");
            assert!(
                !instances.is_empty(),
                "the engaged compact column still renders its head cards"
            );
            assert!(
                instances.len() * 4 < 1000,
                "the compact column must render viewport-scale, got {} live \
                 cards of 1000 — the eager shape means the compact leg lost \
                 the ListView",
                instances.len()
            );
        },
    );
}

/// The deep-board render pin (#462): at the recorded i16 crash scale
/// — four columns × 650 rows = 2 600 tasks, the empirical bracket's
/// panic point where the eager board's column geometry overflowed the
/// software renderer's i16 pixel space — the virtualised board
/// rasterizes a frame. The capture itself is the property: the former
/// failure mode was a process abort inside the raster pass. The
/// ×4 instance ratio is a failure detector on the eager shape, never
/// a success ceiling, and the labelled head card rejects the
/// degenerate "engaged but renders nothing" breakage. Regular leg
/// only: the recorded backtrace is the regular leg's drop-zone
/// repeater, and the compact leg's engagement is pinned above.
#[test]
fn a_board_beyond_the_i16_pixel_ceiling_renders() {
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        |driver| {
            let (_handle, _engine) = mount_board(driver, 2600, 1366, 768);
            let snapshot = driver.snapshot();
            assert_eq!(
                (snapshot.width, snapshot.height),
                (1366, 768),
                "renderer-presence contract: the headless software renderer \
                 must capture the deep board at the exact window size"
            );
            let instances = driver.find_all_by_id("BoardLayout::drag-zone");
            assert!(
                !instances.is_empty(),
                "the engaged board still renders its head cards"
            );
            assert!(
                instances.len() * 4 < 2600,
                "the board must stay viewport-scale at the crash scale, got \
                 {} live cards of 2600 — the eager shape re-arms the #462 \
                 i16 overflow",
                instances.len()
            );
            assert!(
                driver.find_by_label(&bench_label(0)).is_some(),
                "the first card of column 0 renders at the crash scale"
            );
        },
    );
}

/// Focus-into-view, board regular leg (D-5's acceptance semantics): a
/// keyboard jump to the last card of the reading order makes the far
/// off-viewport card live and fully visible. Guarded first: the card
/// is not even instantiated before the move, so the move provably
/// scrolled.
#[test]
fn a_keyboard_jump_to_the_last_card_makes_it_fully_visible() {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        |driver| {
            let (handle, _engine) = mount_board(driver, 1000, 1000, 900);
            let last = bench_label(999);
            assert!(
                driver.find_by_label(&last).is_none(),
                "the reading order's last card starts off-viewport"
            );

            handle.dispatch_action(ActionId::NavFirstItem);
            driver.advance_time(0);
            handle.dispatch_action(ActionId::NavLastItem);
            driver.advance_time(0);

            let card = driver
                .find_by_label(&last)
                .unwrap_or_else(|| panic!("the seek made the last card live"));
            let position = card.absolute_position();
            let size = card.size();
            assert!(
                position.y >= 0.0 && position.y + size.height <= 900.0,
                "the focused card is fully visible, got y={} h={}",
                position.y,
                size.height
            );
        },
    );
}

/// Adjacent keyboard steps do not scroll away (the seek's
/// estimated-range guard): from the top, one `FocusNext` moves focus to
/// the second card while the first card stays at its rest position.
#[test]
fn an_adjacent_step_leaves_the_view_resting() {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        |driver| {
            let (handle, _engine) = mount_board(driver, 1000, 1000, 900);
            handle.dispatch_action(ActionId::NavFirstItem);
            driver.advance_time(0);
            let first = driver
                .find_by_label(&bench_label(0))
                .expect("the head card renders");
            let rest = first.absolute_position();

            handle.dispatch_action(ActionId::NavNextItem);
            driver.advance_time(0);

            let first = driver
                .find_by_label(&bench_label(0))
                .expect("the head card stays live");
            assert_eq!(
                rest.y,
                first.absolute_position().y,
                "an adjacent step must not re-anchor the viewport"
            );
        },
    );
}

/// The drag seams keep global model indices through the virtualised
/// viewport: arming on a head card of column 0 and dropping on
/// column 1 dispatches the exact `MoveTask` the id tables predict
/// (`board_task(0, 0)`), at deep N where the columns are
/// virtualised.
#[test]
fn a_pointer_drag_across_virtualised_columns_moves_the_tabled_task() {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        |driver| {
            let (_handle, mut engine) = mount_board(driver, 1000, 1000, 900);
            let card = driver
                .find_by_label(&bench_label(0))
                .expect("the head card of column 0 renders");
            let (cx, cy) = support::driver::centre(&card);
            let header = driver
                .find_all_by_label("Stack 1")
                .into_iter()
                .find(support::driver::Element::is_accessible_button)
                .unwrap_or_else(|| panic!("the Stack 1 header is present"));
            let pos = header.absolute_position();
            let size = header.size();
            let (hx, hy) = (pos.x + size.width / 2.0, pos.y + size.height / 2.0);

            driver.press_at(cx, cy);
            driver.drag_move_to(hx, hy);
            driver.drag_move_to(hx, hy);
            driver.advance_time(0);
            driver.release_at(hx, hy);
            driver.advance_time(0);

            let moved: Vec<_> = engine
                .drain()
                .iter()
                .filter_map(|command| match command {
                    StateCommand::MoveTask { id, stack, .. } => Some((*id, *stack)),
                    _ => None,
                })
                .collect();
            let state = build_state(1000);
            let mut stack_ids: Vec<_> = state.stacks.values().collect();
            stack_ids.sort_by_key(|stack| stack.order);
            let head = tables_state_head(&state);
            assert_eq!(
                moved,
                vec![(head, stack_ids[1].id)],
                "the drop moves the armed card (board_task(0, 0)) to column 1"
            );
        },
    );
}

/// The fixture's head card of column 0 (canonical order): the id the
/// drag prediction names. (A pure re-derivation, not a test
/// double — the same canonical sort the tables are built from.)
fn tables_state_head(state: &AppState) -> taskboard_domain::TaskId {
    let mut column0: Vec<_> = state
        .tasks
        .values()
        .filter(|task| {
            state
                .stacks
                .get(&task.stack)
                .is_some_and(|stack| stack.order == 0)
        })
        .collect();
    column0.sort_by_key(|task| (task.order, task.id));
    column0[0].id
}

// -- the today sections and stack detail (stage-1 plan §3.3, M3) ----------

/// Engagement, Today sections: a deep overdue fixture renders a
/// viewport-scale window of rows, not all of them.
#[test]
fn the_today_sections_virtualise() {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        |driver| {
            // The bench fixture's due dates are in the past: ~N/3 rows
            // land in the Overdue section.
            let state = Arc::new(ArcSwap::from_pointee(build_state(600)));
            let (commands, _engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
            let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
            let handle = mount(
                state,
                dummy_receiver(),
                dummy_system_receiver(),
                commands,
                requests_tx,
                UiSettings::default(),
                detached_health(),
            )
            .expect("mount succeeds on the testing backend");
            driver.bind(&handle);
            driver.set_window_size(1000, 900);
            driver.advance_time(0);

            let rows = driver.find_all_by_id("TodayView::today-row");
            assert!(
                !rows.is_empty(),
                "the engaged Today view still renders its head rows"
            );
            assert!(
                rows.len() * 4 < 600,
                "the Today sections must render viewport-scale, got {} live \
                 rows of 600 — the eager shape means a section lost the \
                 ListView",
                rows.len()
            );
        },
    );
}

/// Engagement, stack detail: the stack's task list virtualises.
#[test]
fn the_stack_detail_list_virtualises() {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        |driver| {
            let (handle, _engine) = mount_board(driver, 1000, 1000, 900);
            // One view step from the board lands on the stack-detail
            // surface (the view cycle Today → Board → StackDetail →
            // TaskDetail).
            handle.dispatch_action(ActionId::NavNextView);
            driver.advance_time(0);

            // The stack delegates' element ids are not addressable on
            // the pinned slint (the id query returns them empty — the
            // board's and the today view's are fine), so the
            // engagement observable here is the labelled-card count:
            // half of the selected stack's 250 cards probed (i ≡ 0
            // mod 4, first 500 canonical positions). An eager list
            // has every probed head live and fails the ratio; an
            // engaged viewport keeps only its window.
            let mut live_heads = 0;
            let mut probed = 0;
            for i in (0..500).step_by(4) {
                probed += 1;
                if driver.find_by_label(&bench_label(i)).is_some() {
                    live_heads += 1;
                }
            }
            assert!(
                live_heads > 0,
                "the engaged stack list still renders its head cards"
            );
            assert!(
                live_heads * 4 < probed,
                "the stack list must render viewport-scale, got {live_heads} \
                 live head cards of the {probed} probed — the eager shape \
                 means the stack view lost the ListView",
            );

            // The focus-into-view seam reaches the stack surface too.
            // The regular stack view's keyboard walk roams the whole
            // reading order, so no stroke addresses a deep row of the
            // subject stack deterministically; the test writes the
            // hint global in the exact shape the adapter writes for a
            // focused subject-stack row (target-a = −1 — the stack
            // surface's shape per focus-scroll.slint; the adapter-side
            // resolution is `IdTables::stack_position`, unit-tested in
            // models.rs, and the board journeys exercise the full
            // adapter path end-to-end).
            assert!(
                driver.find_by_label(&bench_label(996)).is_none(),
                "the stack's last card starts off-viewport"
            );
            let hint = handle
                .window()
                .global::<taskboard_ui_slint::FocusScrollHint>();
            hint.set_target_a(-1);
            hint.set_target_b(249);
            hint.set_nonce(hint.get_nonce() + 1);
            driver.advance_time(0);

            let last = driver
                .find_by_label(&bench_label(996))
                .unwrap_or_else(|| panic!("the seek made the stack's last card live"));
            let position = last.absolute_position();
            let size = last.size();
            assert!(
                position.y >= 0.0 && position.y + size.height <= 900.0,
                "the focused stack card is fully visible, got y={} h={}",
                position.y,
                size.height
            );
        },
    );
}

/// A Done-section insertion must not disturb the Today viewport
/// (#461, landed with #511). Flipping the fixture's task 0 `done`
/// inserts it into the Done-today section — a separate, lower
/// `ListView` — and the pin asserts an in-window Overdue row rests.
/// Scope note (chunk 4.1 review): task 0 is fixture-`done` by the
/// every-10th pattern (`order % 10 == 0`), so this flip is **not** an
/// Overdue removal — the catalogue's real case-1 pin is
/// `a_true_removal_above_the_today_viewport_reflows_by_the_removed_extent`
/// (the slint 1.18.1 repeater is content-anchored, so an above-viewport
/// removal reflows the live rows by the removed extent). This pin's
/// surviving property is the cross-section one: a Done-section
/// insertion must neither move nor scroll the Overdue viewport.
#[test]
fn a_removal_above_the_today_viewport_leaves_the_rows_resting() {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        |driver| {
            let state = Arc::new(ArcSwap::from_pointee(build_state(600)));
            let (commands, _engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
            let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
            let handle = mount(
                Arc::clone(&state),
                dummy_receiver(),
                dummy_system_receiver(),
                commands,
                requests_tx,
                UiSettings::default(),
                detached_health(),
            )
            .expect("mount succeeds on the testing backend");
            driver.bind(&handle);
            driver.set_window_size(1000, 900);
            driver.advance_time(0);

            // Drive the walk deep: each stroke lands on the next row
            // of the today domain; past the viewport the seek
            // top-anchors, so the viewport ends far from row 0. (The
            // walk's 30 strokes end at overdue position 29 — order 99;
            // the position arithmetic is derived on the within-viewport
            // pin below. bench_label(87) is overdue position 26, live
            // mid-window — the pin's resting witness row.)
            for _ in 0..30 {
                handle.dispatch_action(ActionId::NavNextItem);
                driver.advance_time(0);
            }

            let focused = driver
                .find_by_label(&bench_label(87))
                .unwrap_or_else(|| panic!("the witness card is live"));
            let rest = focused.absolute_position();

            // Flip the fixture's first task's `done`: order 0 is
            // fixture-built `done` by the every-10th pattern
            // (`rem_euclid(10) == 0`), so it was never an Overdue row;
            // re-stamping it inside today's window inserts it into the
            // Done-today section and leaves Overdue untouched.
            let mut next = (**state.load()).clone();
            let first = next
                .tasks
                .values()
                .find(|task| task.title == bench_label(0))
                .expect("the fixture's first task")
                .id;
            next.tasks.get_mut(&first).expect("task").done = Some(chrono::Utc::now());
            state.store(Arc::new(next));
            handle.refresh();
            driver.advance_time(0);

            let focused = driver
                .find_by_label(&bench_label(87))
                .unwrap_or_else(|| panic!("the witness card stays live across the flip"));
            let after = focused.absolute_position();
            assert_eq!(
                rest.y, after.y,
                "a Done-section insertion must not move the Overdue viewport's rows"
            );
        },
    );
}

/// Mounts the bench fixture on the desktop default view (Today) — the
/// anchor catalogue's shared fixture and mount (#461 stage-1 plan D-2:
/// `build_state(600)`, 1000×900 window, ~200 overdue rows).
fn mount_today(
    driver: &mut SlintDriver,
    n: usize,
) -> (
    UiHandle,
    support::fake_engine::FakeEngine,
    Arc<ArcSwap<taskboard_domain::AppState>>,
) {
    let state = Arc::new(ArcSwap::from_pointee(build_state(n)));
    let (commands, engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
    let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
    let handle = mount(
        Arc::clone(&state),
        dummy_receiver(),
        dummy_system_receiver(),
        commands,
        requests_tx,
        UiSettings::default(),
        detached_health(),
    )
    .expect("mount succeeds on the testing backend");
    driver.bind(&handle);
    driver.set_window_size(1000, 900);
    driver.advance_time(0);
    (handle, engine, state)
}

/// Anchor-on-insertion (#461 stage-1 plan D-2 case 2; extended by
/// #592's row-anchored mode): with the Today overdue viewport driven
/// deep by the keyboard walk, inserting a new head row into the
/// section (a synthetic task older than every fixture stamp — the
/// shape a capture landing at the Overdue head takes) must not scroll
/// the page (the section header rests) and must not reflow the live
/// rows by the full inserted extent. The mode compensates by the
/// runtime-average extent (the inserted row is new — never live, so
/// no observed extent exists; the C-2 sanctioned fallback), which
/// leaves a small residual: the witness shift is strictly smaller
/// than one ordinary row extent, uniformly across the window (the
/// mode-off shape shifted by the full inserted extent, ~1.3 row
/// extents — the stage-2 measurement's insert-head campaign case).
#[test]
fn an_insertion_above_the_today_viewport_reflows_less_than_one_extent() {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        |driver| {
            let (handle, _engine, state) = mount_today(driver, 600);

            // One ordinary row extent, measured while the section
            // head is on-screen (the residual's comparison scale).
            let head = driver
                .find_by_label(&bench_label(3))
                .unwrap_or_else(|| panic!("the Overdue head renders pre-walk"));
            let successor = driver
                .find_by_label(&bench_label(6))
                .unwrap_or_else(|| panic!("the Overdue head's successor renders pre-walk"));
            let extent = successor.absolute_position().y - head.absolute_position().y;

            // Drive the walk deep (the landed removal pin's setup):
            // 30 strokes enter at overdue row 0 and end at overdue
            // position 29 (order 99 — the arithmetic is derived on the
            // within-viewport pin below), so the window has left the
            // section head behind. bench_label(87) (position 26) is a
            // live mid-window anchor row.
            for _ in 0..30 {
                handle.dispatch_action(ActionId::NavNextItem);
                driver.advance_time(0);
            }

            let focused = driver
                .find_by_label(&bench_label(87))
                .unwrap_or_else(|| panic!("the anchor card is live"));
            let rest = focused.absolute_position();
            let successor = driver
                .find_by_label(&bench_label(93))
                .unwrap_or_else(|| panic!("the successor row is live"));
            let successor_rest = successor.absolute_position().y;
            let header = driver
                .find_by_label("Overdue")
                .unwrap_or_else(|| panic!("the Overdue section header renders"));
            let header_rest = header.absolute_position();

            // Insert a synthetic strictly-older task: clone overdue
            // row 0's shape (live, open, duedate in the past, no
            // scheduled — fixture order 3 satisfies 3 % 4 != 1) with a
            // fresh id, order -1, and a duedate one second older. It
            // lands at Overdue index 0 by the `(order, id)` order —
            // above the viewport. Direct state mutation + `refresh()`
            // is the deterministic lever: no desktop verb inserts
            // above a scrolled viewport.
            let mut next = (**state.load()).clone();
            let head = next
                .tasks
                .values()
                .find(|task| task.title == bench_label(3))
                .expect("the fixture's oldest open overdue task")
                .clone();
            let mut insert = head.clone();
            insert.id = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
            insert.title = "Task probe insertion head".into();
            insert.order = -1;
            insert.duedate = head.duedate.map(|d| d - chrono::TimeDelta::seconds(1));
            insert.clocks = head.clocks;
            next.tasks.insert(insert.id, insert);
            state.store(Arc::new(next));
            handle.refresh();
            driver.advance_time(0);

            let focused = driver
                .find_by_label(&bench_label(87))
                .unwrap_or_else(|| panic!("the focused card stays live across the insertion"));
            let shifted = focused.absolute_position().y - rest.y;
            assert!(
                shifted.abs() < extent,
                "the compensated insertion reflows the live rows by less than one \
                 ordinary row extent (the average-estimator residual), got shift {shifted} \
                 against extent {extent}"
            );
            // The reflow is uniform: the anchor row and its
            // successor shift by the same extent (the section's list
            // moves as one under the fixed content offset).
            let successor = driver
                .find_by_label(&bench_label(93))
                .unwrap_or_else(|| panic!("the successor row stays live"));
            assert_eq!(
                focused.absolute_position().y - rest.y,
                successor.absolute_position().y - successor_rest,
                "the reflow shifts every live row by the same extent"
            );
            let header = driver
                .find_by_label("Overdue")
                .unwrap_or_else(|| panic!("the Overdue section header stays"));
            assert_eq!(
                header_rest.y,
                header.absolute_position().y,
                "the insertion must not scroll the page"
            );
        },
    );
}

/// Anchor-on-removal above the viewport (#461 stage-1 plan D-2 case
/// 1; extended to pixel-still by #592's row-anchored mode): the
/// catalogue's case 1 with a task that really sits in the Overdue
/// section. The landed
/// `a_removal_above_the_today_viewport_leaves_the_rows_resting` pin
/// flips the fixture's task 0, whose order (0) makes it `done` by the
/// fixture's every-10th pattern — it was never an Overdue row, so its
/// flip inserts into Done-today (a separate, lower `ListView`) and
/// pins nothing about this case. This pin flips the true Overdue head
/// (order 3): under the mode the compensation carries the removed
/// row's **observed** extent (it rendered on-screen at mount, so the
/// C-2 height cache is exact), and the live rows are **pixel-still**
/// — the witness rests at exactly its pre-change y (the mode-off
/// shape reflowed up by the removed extent; the stage-2 measurement's
/// remove-head campaign case). The page never scrolls (the header
/// rests).
#[test]
fn a_true_removal_above_the_today_viewport_is_pixel_still() {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        |driver| {
            let (handle, _engine, state) = mount_today(driver, 600);

            // Drive the walk deep: the window ends far from the
            // section head, so the removal is entirely above the
            // viewport.
            for _ in 0..30 {
                handle.dispatch_action(ActionId::NavNextItem);
                driver.advance_time(0);
            }

            let focused = driver
                .find_by_label(&bench_label(87))
                .unwrap_or_else(|| panic!("the anchor card is live"));
            let rest = focused.absolute_position();
            let header = driver
                .find_by_label("Overdue")
                .unwrap_or_else(|| panic!("the Overdue section header renders"));
            let header_rest = header.absolute_position();

            // Flip the Overdue head's `done` to a fixed stamp outside
            // today's window (the bench's pure-removal lever): a
            // removal above the viewport, with no Done-section
            // insertion to confound the geometry.
            let mut next = (**state.load()).clone();
            let first = next
                .tasks
                .values()
                .find(|task| task.title == bench_label(3))
                .expect("the fixture's oldest open overdue task")
                .id;
            next.tasks.get_mut(&first).expect("task").done =
                Some(chrono::DateTime::from_timestamp(1_704_067_200, 0).expect("fixture stamp"));
            state.store(Arc::new(next));
            handle.refresh();
            driver.advance_time(0);

            let focused = driver
                .find_by_label(&bench_label(87))
                .unwrap_or_else(|| panic!("the focused card stays live across the removal"));
            let shifted = rest.y - focused.absolute_position().y;
            assert_eq!(
                shifted, 0.0,
                "the compensated removal above the viewport leaves the witness pixel-still \
                 (the removed row's observed extent is exact — it rendered at mount)"
            );
            let header = driver
                .find_by_label("Overdue")
                .unwrap_or_else(|| panic!("the Overdue section header stays"));
            assert_eq!(
                header_rest.y,
                header.absolute_position().y,
                "the removal must not scroll the page"
            );
        },
    );
}

/// Anchor-on-removal of a live in-window row (#461 stage-1 plan D-2
/// case 3): with the Today overdue viewport driven deep by the
/// keyboard walk, flipping a **live** row's `done` removes it from the
/// section in place — the rows **below** it close up by exactly the
/// removed row's extent (its successor takes its old position), the
/// rows above it rest, and the page does not scroll (the header
/// rests). This is the click-flip geometry: the list closes up under
/// the cursor. The done stamp is a fixed fixture-epoch value outside
/// today's window — the bench's pure-removal lever, with no
/// Done-section insertion to confound the geometry — and focus
/// follow-through is deliberately not asserted here (the walk journey
/// and the state-side focus pins own that behavior; this pin is the
/// geometry contract).
#[test]
fn a_removal_within_the_today_viewport_closes_the_rows_below_without_scrolling() {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        |driver| {
            let (handle, _engine, state) = mount_today(driver, 600);

            // Drive the walk deep (the landed removal pin's setup):
            // 30 strokes enter at overdue row 0. Overdue rows are the
            // fixture's multiples of 3 (duedate set) minus its
            // multiples of 10 (`has_done`), so overdue position p is
            // order 3·m with m − ⌊m/10⌋ = p + 1: the walk's end,
            // position 29, is order 99, and order 87 is position 26 —
            // mid-window, pinned because both a row above it
            // (order 75) and its successor (order 93; order 90 is
            // fixture-done, not a row) are live, so the close-up is
            // measurable on both sides.
            for _ in 0..30 {
                handle.dispatch_action(ActionId::NavNextItem);
                driver.advance_time(0);
            }

            let focused = driver
                .find_by_label(&bench_label(87))
                .unwrap_or_else(|| panic!("the anchor card is live"));
            let rest = focused.absolute_position();
            let above = driver
                .find_by_label(&bench_label(75))
                .unwrap_or_else(|| panic!("a row above the removed one is live"));
            let above_rest = above.absolute_position();
            let header = driver
                .find_by_label("Overdue")
                .unwrap_or_else(|| panic!("the Overdue section header renders"));
            let header_rest = header.absolute_position();

            // Flip the pinned row's `done` to a fixed stamp
            // outside today's window: a removal within the viewport.
            let mut next = (**state.load()).clone();
            let pinned = next
                .tasks
                .values()
                .find(|task| task.title == bench_label(87))
                .expect("the pinned task")
                .id;
            next.tasks.get_mut(&pinned).expect("task").done =
                Some(chrono::DateTime::from_timestamp(1_704_067_200, 0).expect("fixture stamp"));
            state.store(Arc::new(next));
            handle.refresh();
            driver.advance_time(0);

            let successor = driver
                .find_by_label(&bench_label(93))
                .unwrap_or_else(|| panic!("the successor row closes up"));
            assert_eq!(
                rest.y,
                successor.absolute_position().y,
                "the successor row takes exactly the removed row's old position"
            );
            let above = driver
                .find_by_label(&bench_label(75))
                .unwrap_or_else(|| panic!("the row above the removed one stays live"));
            assert_eq!(
                above_rest.y,
                above.absolute_position().y,
                "the rows above the removed row do not move"
            );
            let header = driver
                .find_by_label("Overdue")
                .unwrap_or_else(|| panic!("the Overdue section header stays"));
            assert_eq!(
                header_rest.y,
                header.absolute_position().y,
                "the removal must not scroll the page"
            );
            assert!(
                driver.find_by_label(&bench_label(87)).is_none(),
                "the removed row is gone from the section"
            );
        },
    );
}

/// Compact stack-switch remount (r2 D8): switching stacks by keyboard
/// re-mounts the single column's viewport cleanly — the new stack's
/// head card renders at the rest position, the old stack's cards are
/// gone.
#[test]
fn a_compact_stack_switch_remounts_the_viewport_cleanly() {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        |driver| {
            let (handle, _engine) = mount_board(driver, 1000, 600, 900);
            let old_head = bench_label(0);
            // Retreating from the top hands focus back to the switcher
            // (r2 D8) and the second stroke moves to the previous
            // stack — the last one — landing on its first task.
            let new_head = bench_label(3);
            assert!(
                driver.find_by_label(&old_head).is_some(),
                "the selected stack's head card renders"
            );

            handle.dispatch_action(ActionId::NavPrevItem);
            driver.advance_time(0);
            handle.dispatch_action(ActionId::NavPrevItem);
            driver.advance_time(0);

            let card = driver
                .find_by_label(&new_head)
                .expect("the new stack's head card renders after the switch");
            let position = card.absolute_position();
            assert!(
                position.y > 0.0 && position.y < 900.0,
                "the new stack's head card renders at its rest position"
            );
            assert!(
                driver.find_by_label(&old_head).is_none(),
                "the old stack's cards are gone from the viewport"
            );
        },
    );
}

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

use slint::ComponentHandle as _;

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

/// Anchor-on-removal (#461): with the Today overdue viewport driven
/// deep by the keyboard walk, flipping an above-viewport overdue
/// task's `done` removes it from the section — and the on-screen
/// rows do not move (the repeater's row-anchored offset shift).
/// A pixel-anchored repeater shifts every visible row up by the
/// removed row's height and fails this pin.
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
            // top-anchors, so the viewport ends far from row 0.
            for _ in 0..30 {
                handle.dispatch_action(ActionId::NavNextItem);
                driver.advance_time(0);
            }

            let focused = driver
                .find_by_label(&bench_label(87))
                .unwrap_or_else(|| panic!("the walked-to card is live"));
            let rest = focused.absolute_position();

            // Flip overdue row 0's `done` (the fixture's first task is
            // the oldest overdue): a removal above the viewport.
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
                .unwrap_or_else(|| panic!("the focused card stays live across the removal"));
            let after = focused.absolute_position();
            assert_eq!(
                rest.y, after.y,
                "the removal above the viewport must not move the on-screen rows"
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

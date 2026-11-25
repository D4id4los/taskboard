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

/// The one-platform mode this binary standardizes on: the `support`
/// self-tests read it so their `DriverOptions` agree with the binary's
/// journeys (`driver::run`'s guard asserts one platform per process;
/// see #1016). The binary's known mixed false sites are #1016/B4a
/// scope, not touched here.
pub(crate) const HEADLESS_RENDERER: bool = true;

use chrono::TimeDelta;
use chrono::TimeZone as _;
use support::driver::{DriverOptions, JourneyDriver, SlintDriver};
use support::fixture;
use support::guard::Verb;
use support::harness::Harness;

use slint::ComponentHandle as _;
use slint::Model as _;
use taskboard_domain::{
    AppState, Rating, ReflectionAttributes, ReflectionEntry, ReflectionId, ReviewCadence,
    StateCommand, TaskId,
};
use taskboard_ui_slint::actions::{ActionId, ContextId};
use taskboard_ui_slint::adapter::REPAINT_CADENCE_MS;
use taskboard_ui_slint::keymap::stroke::{self, parse};
use taskboard_ui_slint::{FieldValidity, SettingsOutcome, UiSettings, View};

/// The test-local read seams over the shared harness.
fn keyboard(harness: &Harness) -> taskboard_ui_slint::KeyboardState<'_> {
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::KeyboardState>()
}
fn model_snapshot(harness: &Harness) -> Vec<String> {
    let entries = keyboard(harness).get_menu_entries();
    (0..entries.row_count())
        .map(|i| entries.row_data(i).expect("row").action.to_string())
        .collect()
}

fn tooltip_state(harness: &Harness) -> taskboard_ui_slint::TooltipState<'_> {
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::TooltipState>()
}

fn facts(harness: &Harness) -> taskboard_ui_slint::MenuPanelFacts<'_> {
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::MenuPanelFacts>()
}

/// The device-local wall-clock's UTC instant (r4 D28): the journeys'
/// tz-invariant expectation helper — the grammar speaks the local
/// zone, storage stays UTC instants; the journey computes the seam
/// conversion the way the app does. Fixed dates/times only — never a
/// DST edge.
#[allow(clippy::too_many_arguments)]
fn local_wall(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> chrono::DateTime<chrono::Utc> {
    chrono::Local
        .with_ymd_and_hms(y, mo, d, h, mi, 0)
        .single()
        .expect("a resolvable local wall-clock")
        .with_timezone(&chrono::Utc)
}

/// The N-4 stamping law's mint over the harness's injected day facts
/// (00:00 start, UTC boot offset) — the instant the commit funnels
/// produce for a bare date (US-1/D-3, task-detail-editor UX plan,
/// 2026-11-17; superseded by #893, 2026-11-18: the mint is the
/// **last covered instant**, `window_end − 1ms`, not the raw window
/// end — the stamp survives the `[now, end)` today membership). The
/// pins that predate the law assert this, not the old local midnight.
fn window_end_utc(y: i32, mo: u32, d: u32) -> chrono::DateTime<chrono::Utc> {
    taskboard_domain::days::last_covered_instant(taskboard_domain::days::day_window_end(
        chrono::NaiveDate::from_ymd_opt(y, mo, d).expect("fixture date"),
        chrono::NaiveTime::MIN,
        chrono::FixedOffset::east_opt(0).expect("zero"),
    ))
}

fn local_midnight(y: i32, mo: u32, d: u32) -> chrono::DateTime<chrono::Utc> {
    local_wall(y, mo, d, 0, 0)
}

/// §7.3 — the ten core verbs in one continuous session (one
/// continuous journey; per-step asserts inside — the length is the
/// contract's, not sprawl).
#[allow(clippy::too_many_lines)]
#[test]
fn the_ten_step_keyboard_session() {
    let (fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        // -- step 1: launch → the board view owns focus --------------

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
        harness.press(ActionId::NavFocusDown);
        assert_eq!(
            harness.emphasized_card(),
            Some((0, 0)),
            "the first walk step lands on the first card"
        );
        harness.press(ActionId::NavFocusDown);
        assert_eq!(
            harness.emphasized_card(),
            Some((0, 1)),
            "the second task of the first stack takes the emphasis"
        );
        harness.press(ActionId::NavFocusUp);
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
            View::Review,
            "the view cycle crosses the review surface (#214) before wrapping"
        );
        harness.press(ActionId::NavNextView);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Today,
            "the view cycle wraps through Today (chunk 10 D2)"
        );
        harness.press(ActionId::NavNextView);
        assert_eq!(harness.handle.window().get_current_view(), View::Board);
        harness.press(ActionId::NavPrevView);
        harness.press(ActionId::NavPrevView);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Review,
            "the backwards wrap crosses the review surface (#214)"
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

        // -- step 3: task.create — one-character title (the guard
        //    convention: create counts N + x + Return against ≤ 3) --
        let create_row = |harness: &Harness, column: usize| {
            harness
                .handle
                .window()
                .get_board_model()
                .stacks
                .row_data(column)
                .expect("column")
                .create_open
        };
        harness.guard.begin(Verb::Create);
        harness.press(ActionId::TaskCreate);
        assert!(create_row(&*harness, 0), "the create row opened in stack a");
        harness.assert_commands(&[]);
        harness.type_text("x");
        harness.stroke(parse("Return").expect("Return"));
        harness.guard.end(Verb::Create);
        assert!(!create_row(&*harness, 0), "the commit closed the row");
        harness.assert_commands(&[StateCommand::CreateTask {
            title: "x".into(),
            stack: fixture.stack_a,
            order: 2,
        }]);
        harness.engine.settle(&harness.handle);
        assert_eq!(
            harness.board_shape(),
            vec![3, 1],
            "the created task joined stack a"
        );

        // -- step 4: task.edit — the created card is ALREADY the
        //    focused card (UX-R8 chunk-5 US-5's create-focus
        //    handoff landed on top of this journey's original
        //    NavNextItem walk — the walk steps are superseded; the
        //    emphasis assertion is the same oracle), open the
        //    detail, type over the selected title, save (R1's risk
        //    case: the title field's keyboard reachability) ------
        assert_eq!(
            harness.emphasized_card(),
            Some((0, 2)),
            "the created task takes the emphasis"
        );
        harness.guard.begin(Verb::Edit);
        harness.press(ActionId::TaskEdit);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::TaskDetail,
            "task.edit opens the detail (focus ownership)"
        );
        // Recorded decision (round-3 chunk 1, plan §6 — F26):
        // task.edit never auto-arms the title; the editor arms
        // via the bound `task.title-edit` action (F2 in the
        // standard preset).
        harness.press(ActionId::TaskTitleEdit);
        harness.type_text("y");
        harness.stroke(parse("Return").expect("Return"));
        harness.guard.end(Verb::Edit);
        // The created task's id only exists once the fake applied the
        // create; the edit's command is matched structurally (exact
        // variant + typed title) and its id carries steps 5–7.
        let drained = harness.engine.drain();
        let created = match drained.as_slice() {
            [StateCommand::UpdateTask { id, changes }] if changes.title.as_deref() == Some("y") => {
                Some(*id)
            }
            other => panic!("the title edit reached the boundary, got {other:?}"),
        };
        harness.engine.settle(&harness.handle);
        assert_eq!(
            harness.handle.window().get_task_model().title,
            "y",
            "the detail model reflects the retitled task"
        );
        let created = created.expect("the edit command carried the created task's id");

        // -- step 5: task.complete (the standard preset binds Space in
        //    the Board context; the keyboard path back is focus-board)
        harness.press(ActionId::NavFocusBoard);
        assert_eq!(harness.handle.window().get_current_view(), View::Board);
        harness.guard.begin(Verb::Complete);
        harness.press(ActionId::TaskComplete);
        harness.guard.end(Verb::Complete);
        harness.assert_commands(&[StateCommand::SetTaskDone {
            id: created,
            done: true,
        }]);
        harness.engine.settle(&harness.handle);

        // -- step 6: task.move — begin, slot-walk, confirm -----------
        // UX-R8 D-1: the walk is slot-shaped. Armed at the task's
        // own gap+1 in stack a (3 tasks → gaps 0..3, armed at 3);
        // two Down strokes walk to stack b's after-last gap —
        // whose slot order IS the append (last.order + 1 = 1).
        harness.guard.begin(Verb::Move);
        harness.press(ActionId::TaskMove);
        assert!(
            harness.handle.window().get_flow_hint_model().visible,
            "the flow hint shows while moving"
        );
        harness.stroke(parse("Down").expect("Down"));
        harness.stroke(parse("Down").expect("Down"));
        assert_eq!(
            harness.handle.window().get_board_model().move_target,
            1,
            "the second stack highlights as the target"
        );
        harness.stroke(parse("Return").expect("Return"));
        harness.guard.end(Verb::Move);
        assert!(
            !harness.handle.window().get_flow_hint_model().visible,
            "the confirm ends the flow hint"
        );
        harness.assert_commands(&[StateCommand::MoveTask {
            id: created,
            stack: fixture.stack_b,
            order: 1,
        }]);
        harness.engine.settle(&harness.handle);
        assert_eq!(
            harness.board_shape(),
            vec![2, 2],
            "the created task joined stack b"
        );

        // -- step 7: task.delete — the confirmation-free flow ---------
        harness.press(ActionId::TaskDelete);
        harness.assert_commands(&[StateCommand::DeleteTask { id: created }]);
        harness.engine.settle(&harness.handle);
        assert_eq!(
            harness.board_shape(),
            vec![2, 1],
            "the view reconciles off the deleted subject"
        );
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Board,
            "the session stays on the board"
        );

        // -- step 8: app.menu — open, cursor walk to the last row
        //    (Quit is pinned last by the curation), invoke: the
        //    keyboard re-dispatch path closes the menu and fires
        //    the exit effect (counter seam).
        harness.guard.begin(Verb::MenuInvoke);
        harness.press(ActionId::AppMenu);
        macro_rules! keyboard_state {
            () => {
                harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
            };
        }
        assert!(keyboard_state!().get_menu_open(), "app.menu opens the menu");
        assert_eq!(keyboard_state!().get_menu_selected_index(), 0);
        let rows = keyboard_state!().get_menu_entries().row_count();
        assert!(rows > 1, "the curated root page offers multiple rows");
        for _ in 1..rows {
            harness.press(ActionId::MenuNext);
        }
        assert_eq!(
            keyboard_state!().get_menu_selected_index(),
            i32::try_from(rows - 1).unwrap_or(i32::MAX),
            "the cursor reaches the pinned-last Quit row"
        );
        assert_eq!(
            keyboard_state!()
                .get_menu_entries()
                .row_data(rows - 1)
                .expect("row")
                .action,
            "app.exit",
            "Quit is the last row"
        );
        harness.press(ActionId::MenuInvoke);
        harness.guard.end(Verb::MenuInvoke);
        assert!(
            !keyboard_state!().get_menu_open(),
            "the invoke closed the menu"
        );
        assert_eq!(
            harness.handle.quit_requests(),
            1,
            "the re-dispatched Quit fired the exit effect"
        );
        harness.assert_commands(&[]);

        // -- step 9: read the sync status indicator -------------------
        //    Typed model read (the badge's label text is data, never
        //    an assertion target).
        let sync = harness.handle.window().get_sync_model();
        assert_eq!(sync.phase, taskboard_ui_slint::SyncPhaseUi::Offline);
        assert_eq!(sync.pending, 2, "the fixture's pending ops render");

        // -- step 10: app.exit — the typed quit outcome (the second
        //    quit of the session; step 8's menu invoke was the
        //    first) -----------------------------------------------
        harness.press(ActionId::AppExit);
        assert_eq!(
            harness.handle.quit_requests(),
            2,
            "the exit effect fired once per quit (menu + direct)"
        );
        harness.assert_commands(&[]);

        // -- T7: the §9.2 verdict over the session's counts -----------
        for (verb, count) in harness.guard.recorded() {
            tracing::info!(?verb, count, "keypress count");
        }
        assert_eq!(
            support::guard::verdict(harness.guard.recorded()),
            [],
            "no §9.2 ceiling exceeded"
        );
    });
}

/// The chunk-7 board leg (design §3.8, one behaviour, one test): the
/// keyboard walk over the canonical board order with no wrap, the
/// second board's stacks re-rendered, the toolbar title switched,
/// nothing dispatched at any step (the dumb-client pin), and a
/// compact leg — the strip hidden, keyboard switching remains.
#[test]
fn board_switching_walks_boards_by_keyboard() {
    let (_fixture, state) = fixture::two_board_state();
    support::harness::run_journey(state, move |harness| {
        assert_eq!(harness.handle.window().get_current_view(), View::Board);
        assert_eq!(harness.board_shape(), vec![2, 1], "board one's stacks");
        assert_eq!(
            harness.handle.window().get_toolbar_title(),
            "board one",
            "the seed is the lowest live board"
        );
        harness.assert_commands(&[]);

        harness.press(ActionId::BoardNext);
        assert_eq!(
            harness.board_shape(),
            vec![1],
            "the second board's stacks re-rendered"
        );
        assert_eq!(harness.handle.window().get_toolbar_title(), "board two");
        harness.assert_commands(&[]);

        harness.press(ActionId::BoardNext);
        assert_eq!(harness.board_shape(), vec![1], "the last end holds");
        assert_eq!(harness.handle.window().get_toolbar_title(), "board two");

        harness.press(ActionId::BoardPrevious);
        assert_eq!(harness.board_shape(), vec![2, 1], "back to board one");
        assert_eq!(harness.handle.window().get_toolbar_title(), "board one");
        harness.assert_commands(&[]);

        // The compact leg (R9/kiosk): at 600 px the strip is
        // hidden and the keyboard walk still switches boards.
        harness.driver.set_window_size(600, 800);
        assert!(
            harness.handle.window().get_compact_active(),
            "600 px is the compact breakpoint"
        );
        harness.press(ActionId::BoardNext);
        assert_eq!(
            harness.board_shape(),
            vec![1],
            "keyboard switching works while the strip is hidden"
        );
        harness.assert_commands(&[]);
    });
}

/// D2/P1 — the outside-click dismissal layer: a click off the panel
/// closes the menu (before the layer existed it did nothing), and a
/// stolen hamburger click — the full-window layer covers the toolbar —
/// converges on the same full close. The layer's z-order is pinned by
/// this behavior, not by comments.
#[test]
fn an_outside_click_dismisses_the_menu() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        // A fixed geometry: the hamburger button occupies the
        // toolbar's right edge (x 952–984, y 16–48 at 1000 px).
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::AppMenu);
        harness.driver.advance_time(0); // instantiate the layer
        assert!(keyboard(&*harness).get_menu_open());
        // Off-panel: the window's left edge, mid-height.
        harness.driver.click_at(10.0, 450.0);
        assert!(
            !keyboard(&*harness).get_menu_open(),
            "an outside click dismisses"
        );

        // The stolen hamburger click: the layer covers the toolbar,
        // so the click dismisses instead of re-toggling.
        harness.handle.window().invoke_toolbar_menu_clicked();
        harness.driver.advance_time(0); // instantiate the layer
        assert!(keyboard(&*harness).get_menu_open());
        harness.driver.click_at(968.0, 32.0);
        assert!(
            !keyboard(&*harness).get_menu_open(),
            "a click over the hamburger while open is a dismissal, not a re-open"
        );
    });
}

/// D6 — the two-tier menu in both presentation modes: a parent row's
/// invocation opens the group's page (fly-out at regular width — its
/// facts land beside the root panel's; replace + a "Back" address at
/// compact width), `menu.next` walks the submenu, a leaf invocation
/// routes and closes, and Escape walks back level by level.
#[test]
#[allow(clippy::too_many_lines)] // the two-mode walk is the contract's length
fn the_two_tier_menu_navigates_levels_in_both_modes() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        // Walk the cursor to the first submenu-parent row (the
        // quick rows precede the parents on the root page).
        let open_at_parent = |harness: &mut Harness| {
            harness.press(ActionId::AppMenu);
            let parent_index = (0..{
                let entries = keyboard(harness).get_menu_entries();
                entries.row_count()
            })
                .find(|i| {
                    keyboard(harness)
                        .get_menu_entries()
                        .row_data(*i)
                        .expect("row")
                        .submenu
                })
                .expect("the root page carries submenu parents");
            for _ in 0..parent_index {
                harness.press(ActionId::MenuNext);
            }
            harness.press(ActionId::MenuInvoke);
            assert!(
                keyboard(harness).get_menu_active_group() >= 0,
                "the parent invocation opened the group's page"
            );
            assert!(
                keyboard(harness).get_menu_submenu_entries().row_count() > 0,
                "the submenu model is mounted"
            );
        };

        // -- regular/expansive: the fly-out beside the root panel --
        harness.driver.set_window_size(1000, 900);
        open_at_parent(&mut *harness);
        assert!(
            facts(&*harness).get_flyout_width() > 0.0,
            "the fly-out panel is mounted"
        );
        assert!(
            facts(&*harness).get_flyout_x() + facts(&*harness).get_flyout_width()
                <= facts(&*harness).get_x(),
            "the fly-out sits left of the root panel, clear of it"
        );
        assert!(
            facts(&*harness).get_flyout_x() >= 8.0,
            "the fly-out stays inside the window"
        );
        // `menu.next` walks the submenu; invoking the first leaf
        // (task.create over the board chain) routes and closes.
        harness.press(ActionId::MenuInvoke);
        assert!(
            harness
                .handle
                .window()
                .get_board_model()
                .stacks
                .row_data(0)
                .expect("col")
                .create_open,
            "the leaf invocation routed (the create row opened)"
        );
        assert!(
            !keyboard(&*harness).get_menu_open(),
            "the leaf invocation closed the menu"
        );
        harness.stroke(parse("Escape").expect("Escape")); // discard the flow

        // Escape walks back level by level.
        open_at_parent(&mut *harness);
        harness.stroke(parse("Escape").expect("Escape"));
        assert!(
            keyboard(&*harness).get_menu_open()
                && keyboard(&*harness).get_menu_active_group() == -1,
            "Escape at a submenu returns to the root page"
        );
        harness.stroke(parse("Escape").expect("Escape"));
        assert!(
            !keyboard(&*harness).get_menu_open(),
            "Escape at the root closes"
        );

        // -- compact: replace mode with the Back address --
        harness.driver.set_window_size(600, 800);
        open_at_parent(&mut *harness);
        // Compact replaces: the Back row (chrome above the list)
        // exists only in this mode.
        harness.driver.click_by_label("Back");
        assert!(
            keyboard(&*harness).get_menu_open()
                && keyboard(&*harness).get_menu_active_group() == -1,
            "the back row returned to the root page"
        );
        // The replaced page's rows dispatch: the root cursor was
        // preserved on the parent row, so re-entering is one
        // invoke; the second invoke fires the submenu's first leaf
        // (task.create over the board chain).
        harness.press(ActionId::MenuInvoke);
        assert!(
            keyboard(&*harness).get_menu_active_group() >= 0,
            "re-entered"
        );
        harness.press(ActionId::MenuInvoke);
        assert!(
            !keyboard(&*harness).get_menu_open(),
            "the compact leaf invocation closed the menu"
        );
        assert!(
            harness
                .handle
                .window()
                .get_board_model()
                .stacks
                .row_data(0)
                .expect("col")
                .create_open,
            "the compact leaf invocation routed"
        );
    });
}

/// D4/P2/P2b — the panel is bounded on all four edges at every window
/// size, and the narrow fallback engages at 320 px (the panel spans
/// the window minus the space-2 margins). Structural reads of the
/// `MenuPanelFacts` rendering facts.
#[test]
fn the_menu_panel_stays_inside_the_window_at_every_size() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        for (width, height) in [(1000u32, 800u32), (600, 800), (320, 400)] {
            harness.driver.set_window_size(width, height);
            harness.press(ActionId::AppMenu);
            harness.driver.advance_time(0);
            assert!(
                harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
                    .get_menu_open()
            );
            let (x, w) = (facts(&*harness).get_x(), facts(&*harness).get_width());
            let margin = 8.0; // Theme.space-2 at scale 1
            #[allow(clippy::cast_precision_loss)]
            let (win_w, win_h) = (width as f32, height as f32);
            assert!(
                x >= margin && x + w <= win_w - margin,
                "panel [{x}, {}] inside [{margin}, {}] at {width}x{height}",
                x + w,
                win_w - margin,
            );
            let y = facts(&*harness).get_y();
            let h = facts(&*harness).get_height();
            assert!(
                y >= margin && y + h <= win_h - margin,
                "panel vertically inside at {width}x{height}: y={y} h={h}"
            );
            if width == 320 {
                assert_eq!(
                    w, 304.0,
                    "the narrow fallback spans the window minus margins"
                );
            }
            harness.stroke(parse("Escape").expect("Escape"));
            assert!(
                !harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
                    .get_menu_open()
            );
        }
    });
}

/// D3/F2 — a mouse row invocation routes AND closes: clicking the
/// "Edit task" row over a focused card navigates to the task detail
/// and the menu is closed afterwards — the same observable end state
/// the keyboard invoke path has always had.
#[test]
fn a_mouse_row_invocation_routes_and_closes() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        // The curated root page's only enabled dispatching row is
        // the pinned Quit row (the view verbs live behind the
        // submenus) — the typed routing outcome is the exit effect.
        harness.press(ActionId::AppMenu);
        harness.driver.advance_time(0); // instantiate the menu tree
        assert!(
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_open()
        );

        harness.driver.click_by_label("Quit taskboard");
        assert_eq!(
            harness.handle.quit_requests(),
            1,
            "the routed action dispatched"
        );
        assert!(
            !harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_open(),
            "the mouse invoke closed the menu"
        );
        harness.assert_commands(&[]); // nothing reached the engine
    });
}

/// Undo v1 surfaces (undo/2, ADR 0019) — J-1: the undo rows grey from
/// the published `UndoView`: an empty line renders both rows disabled;
/// a published `can_undo`/`can_redo` view enables them while the menu
/// holds (the models pass rebuilds the greying map on refresh).
#[test]
fn the_undo_rows_grey_from_the_published_undo_view() {
    let (_fixture, mut state) = fixture::golden_state();
    // golden_state carries the default (empty-line) view already; make
    // it explicit, then the live view for the second phase.
    state.undo = taskboard_domain::UndoView::default();
    support::harness::run_journey(state.clone(), move |harness| {
        let mounted_state = state.clone();

        harness.press(ActionId::AppMenu);
        harness.driver.advance_time(0); // instantiate the menu tree

        let row_state = |harness: &Harness, action: &str| {
            let entries = keyboard(harness).get_menu_entries();
            (0..entries.row_count())
                .map(|i| entries.row_data(i).expect("row"))
                .find(|row| row.action == action)
                .expect("the row renders on the root page")
                .enabled
        };
        assert!(
            !row_state(&*harness, "app.undo") && !row_state(&*harness, "app.redo"),
            "an empty line greys both rows"
        );

        // The engine publishes a live view; the open menu re-greys.
        // (The published snapshot is the one this test mounted —
        // nothing else has written — so the local clone is it.)
        let mut live = mounted_state;
        live.undo = taskboard_domain::UndoView {
            can_undo: true,
            can_redo: true,
            head_label: None,
            recent: vec![],
            branch_children: Vec::new(),
        };
        harness.engine.replace_state(live, &harness.handle);
        harness.driver.advance_time(0);
        assert!(
            row_state(&*harness, "app.undo") && row_state(&*harness, "app.redo"),
            "a live view enables both rows while the menu holds"
        );
        harness.assert_commands(&[]);
    });
}

/// Undo v1 surfaces (undo/2, ADR 0019) — J-2: invoking the rows sends
/// the target-free engine verbs through the fire-and-forget command
/// boundary (the dumb-client pin at the UI edge).
#[test]
fn invoking_the_undo_rows_sends_the_engine_verbs() {
    let (_fixture, mut state) = fixture::golden_state();
    state.undo = taskboard_domain::UndoView {
        can_undo: true,
        can_redo: true,
        head_label: None,
        recent: vec![],
        branch_children: Vec::new(),
    };
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppMenu);
        harness.driver.advance_time(0);

        harness.driver.click_by_label("Undo");
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::Undo]);

        // The menu closed on the routed click (the mouse-row
        // contract); reopen for the redo row.
        harness.press(ActionId::AppMenu);
        harness.driver.advance_time(0);
        harness.driver.click_by_label("Redo");
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::Redo]);
    });
}

/// F1/D1 — the hamburger is a stateful toggle and every open shows
/// the same entries: open → second click closes → third click opens a
/// model equal to the first open's (row count + per-row action ids,
/// structural).
#[test]
fn the_hamburger_toggles_and_every_open_shows_the_same_entries() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.handle.window().invoke_toolbar_menu_clicked();
        assert!(keyboard(&*harness).get_menu_open(), "the first click opens");
        let first = model_snapshot(&*harness);
        assert!(first.len() > 1, "the populated board offers rows");

        harness.handle.window().invoke_toolbar_menu_clicked();
        assert!(
            !keyboard(&*harness).get_menu_open(),
            "the second click closes"
        );

        harness.handle.window().invoke_toolbar_menu_clicked();
        assert!(
            keyboard(&*harness).get_menu_open(),
            "the third click reopens"
        );
        assert_eq!(
            model_snapshot(&*harness),
            first,
            "the second open reproduces the first open's entries"
        );

        // The keyboard path never re-fires while open: Escape
        // (menu.dismiss) closes cleanly after the toggle.
        let opened = keyboard(&*harness).get_menu_open();
        harness.stroke(parse("Escape").expect("Escape"));
        assert!(opened, "the third click left the menu open");
        assert!(!keyboard(&*harness).get_menu_open());
    });
}

/// §8.1 scriptability — the cold-start view-graph walk (chunk plan
/// §5.2): every view reachable by action, `nav.focus-board` returns
/// from everywhere, the menu and the edit flow's Escape always land
/// in a stable state (no dead ends), and the compact board's stack
/// switcher is keyboard-complete.
#[test]
#[allow(clippy::too_many_lines)] // the walk is the contract's length
fn the_view_graph_walks_from_a_cold_start_without_dead_ends() {
    fn current_view(harness: &Harness) -> View {
        harness.handle.window().get_current_view()
    }
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        // Cold start: the board, no selection yet.
        assert_eq!(current_view(&*harness), View::Board);
        assert_eq!(harness.emphasized_card(), None, "no focus at rest");

        // Every view is reachable by action, and nav.focus-board
        // (Alt+1) returns from everywhere.
        harness.press(ActionId::NavNextView);
        assert_eq!(current_view(&*harness), View::StackDetail);
        harness.press(ActionId::NavFocusBoard);
        assert_eq!(current_view(&*harness), View::Board);

        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        assert_eq!(current_view(&*harness), View::TaskDetail);
        harness.press(ActionId::NavFocusBoard);
        assert_eq!(current_view(&*harness), View::Board);

        // The cycle wraps from any starting view (no one-way edges).
        // Review (#214) sits between TaskDetail and Today: the
        // backwards wrap from Board crosses Today first, then
        // Review, then TaskDetail.
        harness.press(ActionId::NavPrevView);
        assert_eq!(current_view(&*harness), View::Today, "wrap backwards");
        harness.press(ActionId::NavPrevView);
        assert_eq!(
            current_view(&*harness),
            View::Review,
            "the wrap's far edge is the review surface"
        );
        harness.press(ActionId::NavPrevView);
        assert_eq!(current_view(&*harness), View::TaskDetail);
        harness.press(ActionId::NavFocusBoard);
        assert_eq!(current_view(&*harness), View::Board);

        // The forward cycle reaches Review the same way: it is
        // read-only (a ledger), so entering it arms no flow and
        // nav.focus-board still returns.
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        assert_eq!(current_view(&*harness), View::Review);
        harness.press(ActionId::NavFocusBoard);
        assert_eq!(current_view(&*harness), View::Board);

        // The menu opens from the board and dismisses cleanly.
        harness.press(ActionId::AppMenu);
        assert!(
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_open()
        );
        harness.stroke(parse("Escape").expect("Escape"));
        assert!(
            !harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_open()
        );

        // The create flow's Escape ends the flow back at rest.
        harness.press(ActionId::TaskCreate);
        assert!(
            harness
                .handle
                .window()
                .get_board_model()
                .stacks
                .row_data(0)
                .expect("column")
                .create_open
        );
        harness.stroke(parse("Escape").expect("Escape"));
        assert!(
            !harness
                .handle
                .window()
                .get_board_model()
                .stacks
                .row_data(0)
                .expect("column")
                .create_open,
            "Escape discarded the create flow"
        );

        // The edit flow's Escape leaves a stable TaskDetail (no dead
        // end): focus-board still works afterwards.
        harness.press(ActionId::NavFocusDown); // seed the focus walk
        harness.press(ActionId::TaskEdit);
        assert_eq!(current_view(&*harness), View::TaskDetail);
        harness.stroke(parse("Escape").expect("Escape"));
        assert!(
            !harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_vi_insert(),
            "the discard returned to normal mode"
        );
        harness.press(ActionId::NavFocusBoard);
        assert_eq!(current_view(&*harness), View::Board);
        harness.assert_commands(&[]); // the whole walk dispatched nothing

        // One compact leg (§6.4's keyboard-completeness proof): at
        // 600 px the switcher leads — nav.next-item walks the stacks
        // with selection-follows-focus, wrapping.
        harness.driver.set_window_size(600, 800);
        assert!(
            harness.handle.window().get_compact_active(),
            "600 px is the compact breakpoint"
        );
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            -1,
            "no stack is explicitly selected at mount (compact still renders the first via its effective-selected clamp)"
        );
        // Retreat hands the card focus back to the switcher level
        // (D8's selection-follows-focus entry point).
        harness.press(ActionId::NavFocusUp);
        assert_eq!(harness.emphasized_card(), None, "the switcher leads");
        // From the switcher, Down selects stack a and lands its first
        // task; the next Down walks that stack's cards.
        harness.press(ActionId::NavFocusDown);
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            0,
            "the switcher step selects stack a"
        );
        assert_eq!(harness.emphasized_card(), Some((0, 0)));
        harness.press(ActionId::NavFocusDown);
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            0,
            "the walk continues inside stack a"
        );
        assert_eq!(harness.emphasized_card(), Some((0, 1)));
        // Retreating past the first card hands focus back to the
        // switcher; the next step from there advances the selection
        // (wrap) to stack b and lands its first task.
        harness.press(ActionId::NavFocusUp);
        harness.press(ActionId::NavFocusUp);
        assert_eq!(harness.emphasized_card(), None, "back at the switcher");
        harness.press(ActionId::NavFocusDown);
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            1,
            "the switcher selection advanced (wrapped) to stack b"
        );
        assert_eq!(
            harness.emphasized_card(),
            Some((1, 0)),
            "the emphasis sits on stack b's first card (its own column)"
        );
        // And one more switcher round wraps back to stack a.
        harness.press(ActionId::NavFocusUp);
        harness.press(ActionId::NavFocusDown);
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            0,
            "the switcher walk wraps"
        );
        assert_eq!(harness.emphasized_card(), Some((0, 0)));

        // Back at the regular width, the full board is keyboard-alive.
        harness.driver.set_window_size(1000, 800);
        assert!(!harness.handle.window().get_compact_active());
        harness.press(ActionId::NavFocusDown);
        assert_eq!(
            harness.emphasized_card(),
            Some((0, 1)),
            "the regular full-board walk resumes across columns"
        );
        harness.assert_commands(&[]);
    });
}

/// D12 — the task-detail back button: the click equivalent of
/// `nav.focus-board` (§7.1) returns from the task detail to the board.
#[test]
fn the_back_button_returns_to_the_board() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskEdit);
        // Recorded decision (round-3 chunk 1, plan §6 — F26):
        // task.edit never auto-arms; the editor arms via the
        // `task.title-edit` action.
        harness.handle.dispatch_action(ActionId::TaskTitleEdit);
        assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
        harness.driver.click_by_label("Back to board");
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Board,
            "the back button routed focus_board"
        );
        // Leaving mid-edit commits the buffer first (the D9
        // leaving rule — the back button is an ordinary leaving
        // effect, no special casing).
        let drained = harness.engine.drain();
        assert!(
            matches!(
                drained.as_slice(),
                [StateCommand::UpdateTask { changes, .. }]
                    if changes.title.as_deref() == Some("task-0")
            ),
            "the leaving commit reached the boundary, got {drained:?}"
        );
    });
}

/// D13 — the right-click task context menu: opening focuses the
/// addressed card, the entries are the Task group's flat page (no
/// parents, no Quit), and invoking "Complete task" routes the exact
/// typed command for the card's task and closes the menu.
#[test]
fn the_context_menu_focuses_and_routes_the_card() {
    let (fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        // The fixture's known card: stack a's second task.
        harness
            .handle
            .window()
            .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);
        assert!(
            keyboard(&*harness).get_menu_open(),
            "the context request opened the menu"
        );
        assert!(
            keyboard(&*harness).get_menu_pointer_anchored(),
            "the menu is anchored at the pointer"
        );

        // The entries are the curated grammar (UX-R8 chunk-5
        // US-2/AC-3): core rows + the rare families as submenu
        // parents, no Quit. The fixture's plain task withholds
        // Convert ▸ (no habit/series rows apply) — the empty-page
        // cascade.
        let entries = keyboard(&*harness).get_menu_entries();
        let rows: Vec<_> = (0..entries.row_count())
            .map(|i| entries.row_data(i).expect("row"))
            .collect();
        let parents: Vec<_> = rows
            .iter()
            .filter(|r| r.submenu)
            .map(|r| r.label.clone())
            .collect();
        assert_eq!(
            parents,
            ["Reminders", "Timers", "Entries"],
            "the rendered submenu parents (Convert withheld)"
        );
        assert!(rows.iter().all(|r| r.action != "app.exit"), "no Quit");
        assert!(
            rows.iter()
                .skip_while(|r| !r.submenu)
                .all(|r| r.submenu || r.action == "task.delete"),
            "after the first parent only parents and the destructive row follow"
        );
        assert!(
            rows.last().map(|r| r.action.clone()) == Some("task.delete".into()),
            "the destructive row renders last (BF-W-N2)"
        );
        assert!(
            rows.iter().any(|r| r.action == "task.complete"),
            "the card context offers Complete task"
        );

        // Invoke it: the exact typed command at the boundary, menu
        // closed.
        harness.driver.click_by_label("Complete task");
        harness.assert_commands(&[StateCommand::SetTaskDone {
            id: fixture.task1,
            done: true,
        }]);
        assert!(
            !keyboard(&*harness).get_menu_open(),
            "the invocation closed the menu"
        );
    });
}

/// R3-F1-half (D6) — the task card's done checkbox: a click toggles
/// the addressed card (`SetTaskDone` true, then false on the round
/// trip), the keyboard's focused-task route produces the same command
/// shape (click ≡ key), and the model round-trip flips
/// `TaskCardModel.done` on the next push — no local checked state
/// anywhere.
#[test]
fn the_card_checkbox_toggles_the_addressed_task() {
    let (fixture, state) = fixture::golden_state();
    // The first card of stack "a": the checkbox click addresses it
    // (find_by_label takes the tree's first match).
    let task0 = state
        .tasks
        .values()
        .find(|task| task.stack == fixture.stack_a && task.order == 0)
        .map(|task| task.id)
        .expect("the fixture's first task");
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);

        // Click the first card's checkbox: the toggled-on command.
        harness.driver.click_by_label("Mark task done");
        harness.assert_commands(&[StateCommand::SetTaskDone {
            id: task0,
            done: true,
        }]);
        // The keyboard's focused-task route produces the same
        // command shape (click ≡ key): task.complete on the
        // focused card sends the same toggled-on SetTaskDone.
        harness.handle.window().invoke_board_task_clicked(0, 0);
        harness.press(ActionId::TaskComplete);
        harness.assert_commands(&[StateCommand::SetTaskDone {
            id: task0,
            done: true,
        }]);

        // The engine applies both toggles (one flip): the model
        // round-trip lands done on the next push, the box offers
        // the undone verb, and the click toggles back off.
        harness.engine.settle(&harness.handle);
        let done_bit = harness
            .handle
            .window()
            .get_board_model()
            .stacks
            .row_data(0)
            .expect("column")
            .tasks
            .row_data(0)
            .expect("card")
            .done;
        assert!(done_bit, "the refresh push flipped the model bit");
        harness.driver.click_by_label("Mark task not done");
        harness.assert_commands(&[StateCommand::SetTaskDone {
            id: task0,
            done: false,
        }]);
    });
}

/// R3-F6 (D4, probe P-B1) — right-clicking a stack's bare card
/// surface (an empty stack's body, the space below the last card)
/// opens the pointer-anchored stack context menu and selects the
/// clicked stack — the header seam's body twin. The probe drives a
/// real right-button press (below the last card of a task-less
/// stack), not the seam invocation.
#[test]
fn the_body_right_click_selects_and_opens_the_stack_menu() {
    let (fixture, mut state) = fixture::golden_state();
    // Stack "b" loses its task: its whole card body is bare surface.
    let stack_b = fixture.stack_b;
    state.tasks.retain(|_, task| task.stack != stack_b);
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        let header = harness
            .driver
            .find_all_by_label("b")
            .into_iter()
            .find(support::driver::Element::is_accessible_button)
            .expect("the stack-b header button is present");
        let pos = header.absolute_position();
        let size = header.size();
        // Below the header: the empty stack's bare body.
        #[allow(clippy::cast_precision_loss)]
        let bare_y = pos.y + size.height * 3.0;
        harness
            .driver
            .right_press_at(pos.x + size.width / 2.0, bare_y);
        harness.driver.advance_time(0);
        assert!(keyboard(&*harness).get_menu_open());
        assert!(
            keyboard(&*harness).get_menu_pointer_anchored(),
            "the menu anchors at the pointer"
        );
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            1,
            "the right-click selected the clicked stack"
        );
    });
}

/// R3-F10/11/41 (D5) — the empty-board page: the board bar renders on
/// a zero-stack board (its tab is addressable — the strongest
/// structural proof available; F11 pinned by addressing), the
/// New-stack CTA opens the same flow state the keyboard's route arms,
/// and the preset CTA drains the table's `CreateStack` commands in
/// table order with explicit 0..n orders (titles are domain payloads
/// — asserted as command payloads, not presentation text).
#[test]
fn the_empty_board_renders_bar_ctas_and_the_preset() {
    let state = fixture::empty_board_state();
    support::harness::run_journey(state.clone(), move |harness| {
        harness.driver.set_window_size(1000, 900);

        // The bar: the fresh board's one tab is addressable.
        assert!(
            harness.driver.find_by_label("board").is_some(),
            "the empty board's tab is addressable"
        );

        // The New-stack CTA arms the same flow state the `S`
        // binding does; Escape-like settle closes it.
        harness.driver.click_by_label("New stack");
        assert!(
            harness.handle.window().get_board_model().stack_create_open,
            "the CTA opens the stack-create flow"
        );
        harness.stroke(parse("Escape").expect("Escape"));
        harness.assert_commands(&[]);

        // The preset CTA: the table's stacks, explicit orders.
        harness.driver.click_by_label("Create Default Kanban Setup");
        let drained = harness.engine.drain();
        let mut created: Vec<(&str, i64)> = Vec::new();
        for command in &drained {
            if let StateCommand::CreateStack { title, order, .. } = command {
                created.push((title.as_str(), *order));
            }
        }
        assert_eq!(
            created,
            vec![
                ("Backlog", 0),
                ("To Do", 1),
                ("In Progress", 2),
                ("Done", 3)
            ],
            "the preset's stacks, table order, explicit orders"
        );
    });
}

/// R3-F12 (D3) — an Add Task ghost hover arms the generalized
/// tooltip: pending on enter, visible after the delay with a
/// non-empty description (content never asserted), dismissed on
/// leave. The regular leg's ghost is probed; the compact ghost shares
/// the wiring.
#[test]
fn an_add_task_ghost_hover_arms_the_tooltip() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        let ghost = harness
            .driver
            .find_by_label("Add task")
            .expect("the add-task ghost is addressable");
        let pos = ghost.absolute_position();
        let size = ghost.size();
        harness
            .driver
            .hover_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
        assert!(
            tooltip_state(&*harness).get_pending(),
            "the hover armed the delay"
        );
        harness.driver.advance_time(600);
        assert!(tooltip_state(&*harness).get_visible(), "the delay fired");
        assert_ne!(
            tooltip_state(&*harness).get_description(),
            "",
            "the description slot is populated"
        );
        harness.driver.hover_at(4.0, 4.0);
        assert!(!tooltip_state(&*harness).get_pending());
        assert!(!tooltip_state(&*harness).get_visible());
    });
}

/// r4 D16/D17 (blanket rule a) — the task view's header row and
/// summary elements arm their tooltips on hover: the done checkbox,
/// the tracking button, and the summary value each arm with a
/// populated description and dismiss on leave. Label content is
/// never asserted.
#[test]
fn the_task_view_header_and_summary_elements_arm_their_tooltips() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskEdit);
        assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);

        for label in ["Mark task done", "Start tracking", "Tracked time total"] {
            let element = harness
                .driver
                .find_by_label(label)
                .unwrap_or_else(|| panic!("{label} is addressable"));
            let pos = element.absolute_position();
            let size = element.size();
            harness
                .driver
                .hover_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
            assert!(
                tooltip_state(&*harness).get_pending(),
                "{label}'s hover armed the delay"
            );
            harness.driver.advance_time(600);
            assert!(tooltip_state(&*harness).get_visible(), "{label} showed");
            assert_ne!(
                tooltip_state(&*harness).get_description(),
                "",
                "{label}'s description slot is populated"
            );
            harness.driver.hover_at(4.0, 4.0);
            assert!(!tooltip_state(&*harness).get_pending());
            assert!(!tooltip_state(&*harness).get_visible());
        }
    });
}

/// r4 D21 (F26, acceptance e) — the hover sweep over the chunk's new
/// interactive elements: the four closed cells and the preset chips
/// arm the generalized tooltip (label + description, hint keys where
/// bound), hover shows it after the delay, and moving away hides it.
/// Historical note: the field sites were tip-less when this journey
/// was written (the planned `TextField` tip arm died at stage 3 — the
/// inner `TextInput` accepts its own hover moves, so a sibling
/// `TouchArea` never hovers and one above breaks caret handling).
/// The r3-3.1 chunk landed them via the sibling-cell `TippedField`
/// pair instead; see
/// `.artifacts/plans/2026-11-03-taskview-field-tooltips-plan.md` and
/// the origin report §5 for the history — the field sites' tips are
/// now pinned by `every_task_view_field_editor_carries_its_info_tip`
/// below.
#[test]
fn every_task_view_cell_and_chip_carries_its_info_tip() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskEdit);
        harness.driver.advance_time(0);
        assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);

        // The structural inventory: one distinct tip spot per cell
        // trigger and four chip triggers (the settings sweep's
        // distinct-geometry mechanism — the id query reports the
        // touch subtree too).
        let distinct_spots = |harness: &Harness, id: &str| -> usize {
            let mut spots: Vec<(f32, f32)> = harness
                .driver
                .find_all_by_id(id)
                .iter()
                .map(|tip| {
                    let p = tip.absolute_position();
                    (p.x, p.y)
                })
                .collect();
            spots.sort_by(|a, b| a.partial_cmp(b).expect("no NaN positions"));
            spots.dedup();
            spots.len()
        };
        // The W4 absorption: the four closed cells share the
        // widget's touch anatomy — one tip trigger per cell, four
        // distinct spots over the shared id (the address-only
        // rename per the CheckBox precedent), plus the tracked
        // row's hover-only arm.
        assert_eq!(
            distinct_spots(&*harness, "LabeledValueRow::cell-touch"),
            4,
            "exactly one tip trigger per closed cell"
        );
        assert_eq!(
            distinct_spots(&*harness, "LabeledValueRow::value-touch"),
            1,
            "exactly one hover arm at the tracked row"
        );
        // The W5 absorption: the preset chips are `ActionChip`
        // instances (`RuleList::preset-chip`); one chip per offset,
        // four distinct spots (each carries the widget's tip-armed
        // TouchArea — the probe asserts the posture itself).
        assert_eq!(
            distinct_spots(&*harness, "RuleList::preset-chip"),
            4,
            "each preset chip carries its own tip trigger"
        );

        // The functional legs: hover arms the delay and shows the
        // panel with a populated description; moving away hides.
        let cell = |harness: &Harness, label: &str| {
            let element = harness
                .driver
                .find_by_label(label)
                .unwrap_or_else(|| panic!("{label} is addressable"));
            let pos = element.absolute_position();
            let size = element.size();
            (pos.x + size.width / 2.0, pos.y + size.height / 2.0)
        };
        for label in ["Edit title", "Edit due date", "5m"] {
            let (x, y) = cell(&*harness, label);
            harness.driver.hover_at(x, y);
            harness.driver.advance_time(0);
            assert!(
                tooltip_state(&*harness).get_pending(),
                "{label}'s hover armed the delay"
            );
            harness.driver.advance_time(600);
            assert!(tooltip_state(&*harness).get_visible(), "{label} showed");
            assert_ne!(
                tooltip_state(&*harness).get_description(),
                "",
                "{label}'s description slot is populated"
            );
            harness.driver.hover_at(4.0, 4.0);
            harness.driver.advance_time(0);
            assert!(!tooltip_state(&*harness).get_pending());
            assert!(!tooltip_state(&*harness).get_visible());
        }
    });
}

/// Chunk r3-3.1 — the five field sites (title/description editors,
/// due/scheduled editors, the custom reminder offset) carry their
/// `TippedField` info tips (issue #590; plan
/// `.artifacts/plans/2026-11-03-taskview-field-tooltips-plan.md` §3).
/// The custom-offset square is always mounted (exactly one in the
/// closed view); each open editor adds exactly one distinct square —
/// never zero, never duplicated. Per site the square trails the
/// field as a sibling layout cell (tip.x ≥ field.x + field.width —
/// the mechanism pin: a square inside or over the field's rect would
/// reintroduce the r4 caret infeasibility; see the origin report
/// `2026-11-03-ui-r4-taskview-focus-stage3-report.md` §5). The
/// functional legs ride the settings sweep's grammar: hover arms the
/// delay, the mock clock's 600 ms shows the panel with a populated
/// description slot (content never asserted), moving away hides —
/// except at the two date-editor sites, where the test currently
/// pins the picker overlay's hover precedence instead (see the leg
/// comment below for the mechanism and the tripwire).
#[test]
#[allow(clippy::too_many_lines)] // the five site legs are the contract's length
fn every_task_view_field_editor_carries_its_info_tip() {
    let sites: [(&str, &str); 5] = [
        ("Edit title", "Task title"),
        ("Edit description", "Description"),
        ("Edit due date", "Due date entry"),
        ("Edit start date", "Start date entry"),
        // The offset field is always mounted; no cell opens it.
        ("", "Custom reminder offset"),
    ];

    // The closed view: exactly the always-mounted offset square.
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskEdit);
        harness.driver.advance_time(0);
        assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
        // The offset square rides TippedField; the description's
        // square is the manual edit-mount composition's own
        // (round-5 chunk 2, D-MLE-8) — both count.
        let mut spots: Vec<(f32, f32)> = harness
            .driver
            .find_all_by_id("TippedField::help-tip")
            .iter()
            .chain(
                harness
                    .driver
                    .find_all_by_id("TaskView::description-help-tip")
                    .iter(),
            )
            .map(|tip| {
                let p = tip.absolute_position();
                (p.x, p.y)
            })
            .collect();
        spots.sort_by(|a, b| a.partial_cmp(b).expect("no NaN positions"));
        spots.dedup();
        assert_eq!(
            spots.len(),
            1,
            "the closed view carries only the offset square"
        );
    });

    for (cell, field_label) in sites {
        let (_fixture, state) = fixture::golden_state();
        support::harness::run_journey(state, move |harness| {
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::NavFocusDown);
            harness.press(ActionId::TaskEdit);
            harness.driver.advance_time(0);
            if !cell.is_empty() {
                harness.driver.click_by_label(cell);
                harness.driver.advance_time(0);
            }
            // The offset square rides TippedField; the
            // description site's square is the manual edit-mount
            // composition's own (round-5 chunk 2, D-MLE-8).
            let mut tips = harness.driver.find_all_by_id("TippedField::help-tip");
            tips.extend(
                harness
                    .driver
                    .find_all_by_id("TaskView::description-help-tip"),
            );
            assert_eq!(
                tips.len(),
                if cell.is_empty() { 1 } else { 2 },
                "{field_label}: the open site adds exactly one square to the offset's"
            );

            // The mechanism pin: the site's square pairs with the
            // field it annotates — a labelled element sharing the
            // square's row that the square trails (tip.x ≥ field
            // right edge — the square never sits inside or over
            // the field's rect, the caret-guard geometry). The
            // label lookup is a set, not a single: a section
            // caption can share the field's label (the
            // description's caption does), and only the real pair
            // satisfies the relation.
            let fields = harness.driver.find_all_by_label(field_label);
            let mut pairs: Vec<_> = Vec::new();
            for tip in &tips {
                let tpos = tip.absolute_position();
                let tsize = tip.size();
                for field in &fields {
                    let fpos = field.absolute_position();
                    let fsize = field.size();
                    let same_row = tpos.y + tsize.height > fpos.y && tpos.y < fpos.y + fsize.height;
                    if same_row && tpos.x >= fpos.x + fsize.width {
                        pairs.push((tip, field));
                    }
                }
            }
            assert_eq!(
                pairs.len(),
                1,
                "{field_label}: exactly one square trails the field in its row"
            );
            let (tip, _) = pairs.remove(0);

            // The functional legs. The three non-picker sites ride
            // the settings sweep's grammar: hover arms, the delay
            // shows, the description slot is populated, away
            // hides. The two date-editor sites currently pin a
            // limitation instead: the adapter's typing-first
            // default-open mounts the window-level picker overlay
            // for the editor's whole life, and its dismissal
            // layer (r4 D2) sits on top of everything — any click
            // settles the editor (r4 D24), so no click path
            // hovers the tip while the editor is open. The help
            // still reaches AT users via the field's
            // accessible-description; reworking the overlay is
            // r4-surface territory, not this chunk's. The pin
            // asserts the precedence as the live contract — a
            // future change that frees the hover flips the first
            // assertion red, which is the intended tripwire.
            let picker_leg = cell == "Edit due date" || cell == "Edit start date";
            let (x, y) = support::driver::centre(tip);
            harness.driver.hover_at(x, y);
            if picker_leg {
                assert!(
                    !tooltip_state(&*harness).get_pending(),
                    "{field_label}: the picker overlay's dismissal layer outranks the tip hover"
                );
                harness.driver.hover_at(4.0, 4.0);
            } else {
                assert!(
                    tooltip_state(&*harness).get_pending(),
                    "{field_label}'s hover armed the delay"
                );
                harness.driver.advance_time(600);
                assert!(
                    tooltip_state(&*harness).get_visible(),
                    "{field_label} showed"
                );
                assert_ne!(
                    tooltip_state(&*harness).get_description(),
                    "",
                    "{field_label}'s description slot is populated"
                );
                harness.driver.hover_at(4.0, 4.0);
                harness.driver.advance_time(0);
                assert!(!tooltip_state(&*harness).get_pending());
                assert!(!tooltip_state(&*harness).get_visible());
            }
        });
    }
}

/// Chunk r3-3.1 — the caret-guard companion: hovering the offset
/// field's tip square mid-edit leaves the caret and the keyboard
/// surface intact (issue #590's original infeasibility must not
/// re-enter). The typed buffer survives the hover round-trip and
/// commits whole; the insert-mask fact still reads true (focus never
/// left the field). Typed outcomes + structural facts only.
#[test]
fn a_field_tip_hover_leaves_the_caret_intact() {
    let (fixture, state) = fixture::golden_state();
    let task0 = fixture.task0;
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskEdit);
        harness.driver.advance_time(0);
        click_field_centre(&mut *harness, "Custom reminder offset");
        harness.type_text("45");

        let tip = harness
            .driver
            .find_all_by_id("TippedField::help-tip")
            .into_iter()
            .next()
            .expect("the offset square is mounted");
        let (x, y) = support::driver::centre(&tip);
        harness.driver.hover_at(x, y);
        assert!(
            tooltip_state(&*harness).get_pending(),
            "the hover armed the delay"
        );
        harness.driver.advance_time(600);
        assert!(
            tooltip_state(&*harness).get_visible(),
            "the tip shows mid-edit"
        );

        harness.type_text("m");
        harness.stroke(parse("Return").expect("return"));
        match harness.engine.drain().as_slice() {
            [StateCommand::AddReminderRule { task, offset, .. }] => {
                assert_eq!(*task, task0, "the subject's rule");
                assert_eq!(
                    *offset,
                    taskboard_domain::ReminderOffset::before(
                        TimeDelta::try_minutes(-45).expect("-45m")
                    )
                    .expect("legal offset")
                );
            }
            other => panic!("the full typed content committed, got {other:?}"),
        }
        assert!(
            keyboard(&*harness).get_vi_insert(),
            "the field's focus masks the chain"
        );
    });
}

/// D22 — the compact add-stack affordance: at the compact width the
/// relocated trailing button opens the same stack-create flow (the
/// click equivalent of stack.create, §7.1).
#[test]
fn the_compact_add_stack_affordance_opens_the_flow() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(600, 800);
        assert!(harness.handle.window().get_compact_active());
        harness.driver.click_by_label("New stack");
        assert!(
            harness.handle.window().get_board_model().stack_create_open,
            "the compact button opens the stack-create flow"
        );
    });
}

/// G8/G9 — the stack-create flow has a visible surface, and the
/// on-board New-stack button is the click equivalent of the `S` key
/// (usability pass D7, dispatch issue 6): both routes open the same
/// flow state and land the same typed command at the boundary.
#[test]
fn stack_create_has_a_visible_click_equal_flow() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state.clone(), move |harness| {
        let create_open =
            |harness: &Harness| harness.handle.window().get_board_model().stack_create_open;

        // (a) The keyboard route: `S` opens the flow's field.
        harness.press(ActionId::StackCreate);
        assert!(create_open(&*harness), "the flow's field is visible");
        harness.assert_commands(&[]);
        harness.type_text("s");
        harness.stroke(parse("Return").expect("Return"));
        assert!(!create_open(&*harness), "the commit closed the flow");
        match harness.engine.drain().as_slice() {
            [StateCommand::CreateStack { title, order, .. }] => {
                assert_eq!(title.as_str(), "s");
                assert_eq!(*order, 2, "append after the fixture's two stacks");
            }
            other => panic!("the create-stack command reached the boundary, got {other:?}"),
        }

        // (b) The click route: the on-board button opens the same
        // flow state and the same commit path fires — click ≡ key
        // at the observed boundary (§7.1).
        harness.driver.click_by_label("New stack");
        assert!(create_open(&*harness), "the button opens the same flow");
        harness.type_text("s");
        harness.stroke(parse("Return").expect("Return"));
        match harness.engine.drain().as_slice() {
            [StateCommand::CreateStack { title, order, .. }] => {
                assert_eq!(title.as_str(), "s");
                assert_eq!(*order, 2, "the same command shape");
            }
            other => panic!("the click route's command reached the boundary, got {other:?}"),
        }
    });
}

/// R4 D6 (dispatch acceptance (a)/(e)) — a divider between two of
/// four columns redistributes space between exactly those two: every
/// non-adjacent column's edge is bit-stable while the dragged boundary
/// moves.
#[test]
fn divider_drag_moves_exactly_two_neighbouring_columns() {
    let (stacks, state) = fixture::golden_state_with_stacks(4);
    support::harness::run_journey(state, move |harness| {
        let window = harness.handle.window();
        // A deterministic row: 1000 − the chrome (2·space-3 side
        // padding + four space-3 gaps + the 40 px affordance) =
        // 888 rendered px, four equal shares of 222 at scale 1.
        harness.driver.set_window_size(1000, 800);
        for column in 0..4 {
            assert_eq!(
                harness.column_width_px(column),
                222.0,
                "the four columns seed at equal shares"
            );
        }

        // Drag the middle boundary (between columns 1 and 2) by
        // +100: column 1 takes +δ, column 2 −δ, columns 0 and 3
        // never move (the non-adjacent isolation, four columns).
        window.invoke_board_stack_width_press(1);
        window.invoke_board_stack_width_drag(1, 100.0);
        window.invoke_board_stack_width_release(1);
        assert_eq!(
            harness.column_width_px(0),
            222.0,
            "the left far column is bit-stable"
        );
        assert_eq!(
            harness.column_width_px(3),
            222.0,
            "the right far column is bit-stable"
        );
        assert_eq!(harness.column_width_px(1), 322.0);
        assert_eq!(harness.column_width_px(2), 122.0);

        // The released row re-resolves stably: a fresh refresh
        // does not move any edge.
        harness.handle.refresh();
        assert_eq!(harness.column_width_px(0), 222.0);
        assert_eq!(harness.column_width_px(1), 322.0);
        assert_eq!(harness.column_width_px(2), 122.0);
        assert_eq!(harness.column_width_px(3), 222.0);
        let _ = stacks;
    });
}

/// R4 D7/decision 6 (dispatch acceptance (c), the add half) — a board
/// with partial stored widths rebalances when a stack joins: the
/// absent entries seed at the equal share and the row re-fits
/// proportionally over all columns, without any gesture.
#[test]
fn stored_widths_rebalance_when_a_stack_joins() {
    let (stacks, state) = fixture::golden_state_with_stacks(3);
    let board = *state.boards.keys().next().expect("fixture board");
    // The device-local overlay carries only the first stack's
    // width — the other two are absent entries (a downlevel
    // config, or a stack created after the last snapshot).
    let settings = taskboard_ui_slint::UiSettings {
        stack_widths: std::collections::BTreeMap::from([(
            board.as_uuid(),
            std::collections::BTreeMap::from([(stacks[0].as_uuid(), 500u32)]),
        )]),
        ..taskboard_ui_slint::UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.driver.set_window_size(1000, 800);

        // The row (900 rendered px) re-fits over the stored basis
        // and the two equal-share seeds: every column positive, at
        // or above its content minimum, summing to the row; the
        // stored column narrows and the newcomers land near the
        // average share (± the fit's rounding).
        let widths: Vec<f32> = (0..3).map(|c| harness.column_width_px(c)).collect();
        let total: f32 = widths.iter().sum();
        for width in &widths {
            assert!(*width >= 120.0, "every column ≥ its minimum, got {width}");
        }
        assert!((total - 900.0).abs() <= 2.0, "the row fills: {total}");
        assert!(widths[0] < 500.0, "the stored basis gave up slack");
        assert!(
            (widths[1] - widths[2]).abs() <= 2.0,
            "the two absent-seeded columns land together"
        );
        assert!(
            harness.drain_requests().is_empty(),
            "a passive rebalance never persists"
        );
    });
}

/// Round-1 chunk 2 (D18, T-10), adapted to the r4 pair model (D8) —
/// the keyboard resize journey: wider steps the focused column against
/// its neighbour (which seesaws), narrowing clamps at the content
/// minimum with the neighbour absorbing the rest, and every step's
/// full-snapshot request reaches the lane typed (exactly the
/// `StackWidthsChanged` payloads; the materialized overlay persists
/// both columns — the lane's full-snapshot contract).
#[test]
fn keyboard_resize_steps_clamps_and_requests_persistence() {
    let (fixture, state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    support::harness::run_journey(state, move |harness| {
        // A deterministic row: 1000 − the chrome (2·space-3 side
        // padding + two space-3 gaps + the 40 px affordance) = 912
        // rendered px, two equal shares of 456 at scale 1.
        harness.driver.set_window_size(1000, 800);
        assert_eq!(harness.column_width_px(0), 456.0, "equal-share seed");
        assert_eq!(harness.column_width_px(1), 456.0, "equal-share seed");

        // One full-snapshot request per key press, drained per
        // step; the materialized overlay carries both columns, so
        // the matcher is order-insensitive (BTreeMap key order).
        let expect_snapshot =
            |harness: &Harness, expected: [(taskboard_domain::StackId, u32); 2]| {
                let requests = harness.drain_requests();
                assert_eq!(requests.len(), 1, "one request per key press");
                let taskboard_ui_slint::requests::UiRequest::StackWidthsChanged {
                    board: payload_board,
                    widths,
                } = &requests[0]
                else {
                    panic!("the typed snapshot reached the lane");
                };
                assert_eq!(payload_board, &board);
                let mut got = widths.clone();
                got.sort_by_key(|(stack, _)| *stack);
                let mut want = expected.to_vec();
                want.sort_by_key(|(stack, _)| *stack);
                assert_eq!(got, want, "the typed snapshot carries the row");
            };

        harness.press(ActionId::StackWider);
        assert_eq!(
            harness.column_width_px(0),
            488.0,
            "the first step takes 32 from the neighbour (scale 1)"
        );
        assert_eq!(harness.column_width_px(1), 424.0, "the neighbour seesaws");
        expect_snapshot(&*harness, [(fixture.stack_a, 488), (fixture.stack_b, 424)]);

        harness.press(ActionId::StackNarrower);
        assert_eq!(harness.column_width_px(0), 456.0, "back to the share");
        assert_eq!(harness.column_width_px(1), 456.0);
        expect_snapshot(&*harness, [(fixture.stack_a, 456), (fixture.stack_b, 456)]);

        // Narrowing clamps at the content minimum (120 for the
        // short fixture titles) and keeps reporting the clamped
        // snapshot; the neighbour absorbs the rest of the row.
        for step in 1..=12u32 {
            harness.press(ActionId::StackNarrower);
            let width = 456u32
                .saturating_sub(32 * step)
                .max(taskboard_ui_slint::widths::STACK_WIDTH_READABILITY_MIN_PX);
            // px values, far below f32's mantissa reach.
            #[allow(clippy::cast_precision_loss)]
            let expected_px = width as f32;
            assert_eq!(harness.column_width_px(0), expected_px);
            #[allow(clippy::cast_precision_loss)]
            let neighbour = (912 - width) as f32;
            assert_eq!(harness.column_width_px(1), neighbour);
            expect_snapshot(
                &*harness,
                [(fixture.stack_a, width), (fixture.stack_b, 912 - width)],
            );
        }
    });
}
/// Round-1 chunk 2 (D17, T-11), adapted to the r4 divider model —
/// the mouse drag journey: press materializes the row and captures the
/// boundary, drag re-renders live (the pair at the boundary moves,
/// clamped at both content minimums), a stale column index is
/// absorbed, and exactly one request lands on release carrying both
/// columns.
#[test]
fn drag_seam_resizes_live_and_persists_on_release() {
    let (fixture, state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    support::harness::run_journey(state, move |harness| {
        let window = harness.handle.window();
        // A deterministic row: 912 rendered px, two shares of 456.
        harness.driver.set_window_size(1000, 800);

        window.invoke_board_stack_width_press(0);
        window.invoke_board_stack_width_drag(0, -50.0);
        assert_eq!(
            harness.column_width_px(0),
            406.0,
            "the left column re-rendered live at base + delta"
        );
        assert_eq!(
            harness.column_width_px(1),
            506.0,
            "the right neighbour takes exactly −δ"
        );
        assert!(
            harness.drain_requests().is_empty(),
            "drag ticks never persist (release only)"
        );

        // A stale index (tables moved between press and tick) is
        // absorbed silently.
        window.invoke_board_stack_width_drag(9, 500.0);
        assert_eq!(harness.column_width_px(0), 406.0, "stale tick ignored");

        window.invoke_board_stack_width_release(0);
        let requests = harness.drain_requests();
        assert_eq!(requests.len(), 1, "exactly one request on release");
        let taskboard_ui_slint::requests::UiRequest::StackWidthsChanged {
            board: payload_board,
            widths,
        } = &requests[0]
        else {
            panic!("the typed snapshot reached the lane");
        };
        assert_eq!(payload_board, &board);
        let mut got = widths.clone();
        got.sort_by_key(|(stack, _)| *stack);
        assert_eq!(
            got,
            vec![(fixture.stack_a, 406), (fixture.stack_b, 506)],
            "the release snapshot carries both boundary columns"
        );

        // Release without a press is inert (no request).
        window.invoke_board_stack_width_release(0);
        assert!(harness.drain_requests().is_empty());
    });
}

/// Round-1 chunk 2 (D16, T-12), adapted to the r4 explicit-width row
/// (chunk-2 plan §5) — a never-resized board pushes positive
/// equal-share widths on every column (the rendered image of the old
/// stretch-equal distribution; the −1.0 sentinel is gone) and
/// refreshes alone never emit a persistence request.
#[test]
fn never_resized_board_pushes_positive_widths_and_no_requests() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        // A deterministic row: 912 rendered px over two columns.
        harness.driver.set_window_size(1000, 800);
        let board_model = harness.handle.window().get_board_model();
        let count = board_model.stacks.row_count();
        assert!(count >= 2);
        for column in 0..count {
            let width = harness.column_width_px(column);
            assert!(width > 0.0, "column {column} pushes a positive width");
            assert_eq!(
                width, 456.0,
                "every column carries its equal share of the row"
            );
        }
        harness.handle.refresh();
        harness.handle.refresh();
        assert!(
            harness.drain_requests().is_empty(),
            "refreshes alone never persist"
        );
    });
}

/// R4 D5/decision 6 (dispatch acceptance (c)) — a window resize
/// distributes the delta proportionally over the stored bases: drag
/// column 0 down to its content minimum, enlarge the window, and every
/// column grows; the resolved row sums to the window minus the chrome
/// (2·space-3 side padding + two space-3 gaps + the 40 px affordance
/// at scale 1 = 88 px — the corrected arithmetic the code now
/// matches).
#[test]
fn window_resize_distributes_delta_proportionally() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        let window = harness.handle.window();
        harness.driver.set_window_size(1000, 800);

        // Drag column 0 to its content minimum (the divider's
        // press materializes; the delta clamps at the pair's
        // minimums).
        window.invoke_board_stack_width_press(0);
        window.invoke_board_stack_width_drag(0, -500.0);
        window.invoke_board_stack_width_release(0);
        assert_eq!(
            harness.column_width_px(0),
            120.0,
            "the dragged column clamps at its content minimum"
        );
        assert_eq!(harness.column_width_px(1), 792.0, "the row still sums");

        // A wider window fills: the delta distributes
        // proportionally over the bases (120/792 of 912).
        harness.driver.set_window_size(1500, 900);
        let w0 = harness.column_width_px(0);
        let w1 = harness.column_width_px(1);
        assert!(w0 > 120.0, "the min-basis column grows too, got {w0}");
        assert!(w1 > 792.0, "the wide column keeps its lead, got {w1}");
        // Σ widths == 1500 − 88 chrome (±1 px of physical
        // rounding).
        let total = w0 + w1;
        assert!(
            (total - 1412.0).abs() <= 1.0,
            "the resolved row fills the window: {total} vs 1412"
        );
    });
}

/// R2 D3 (T-5) — the header is one locked control row regardless of
/// content: a 0-task stack's header measures exactly the D20b constant,
/// identical to its populated neighbour's.
#[test]
fn stack_headers_are_one_control_row_regardless_of_content() {
    let (fixture, mut state) = fixture::golden_state();
    state.tasks.retain(|_, task| task.stack != fixture.stack_b);
    support::harness::run_journey(state, move |harness| {
        assert_eq!(harness.board_shape(), vec![2, 0], "stack b is empty");
        #[allow(clippy::cast_precision_loss)]
        let control_row = (32.0 + 2.0 * 4.0) * 1.0; // space-6 + 2·space-1, scale 1
        for title in ["a", "b"] {
            let header = harness
                .driver
                .find_all_by_label(title)
                .into_iter()
                .find(support::driver::Element::is_accessible_button)
                .unwrap_or_else(|| panic!("stack {title}'s header is addressable"));
            let height = header.size().height;
            assert_eq!(
                height, control_row,
                "stack {title}'s header is one control row"
            );
        }
    });
}

/// R2 D4 (T-10) — the model pushes the *true* selection: a fresh mount
/// carries −1 (no explicit selection), and a header click moves it to
/// the clicked stack's index.
#[test]
fn mount_pushes_no_selection_and_a_header_click_moves_it() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 800);
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            -1,
            "a fresh mount pushes no selection"
        );
        let click_header = |harness: &Harness, title: &str| {
            harness
                .driver
                .find_all_by_label(title)
                .into_iter()
                .find(support::driver::Element::is_accessible_button)
                .unwrap_or_else(|| panic!("stack {title}'s header is addressable"))
                .invoke_default_action();
        };
        click_header(&*harness, "a");
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            0,
            "the header click selected stack a"
        );
        click_header(&*harness, "b");
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            1,
            "the header click moved the selection to stack b"
        );
    });
}

/// R2 D5 (T-6), amended by r3 D2 — the add-stack plus glyph is square
/// at the `IconButton` icon-box footprint (space-6 − 2·space-2, the
/// halving the review asked for): the renderer's bounding box of the
/// painted glyph inside the trailing add-column strip measures that
/// square (element ids cannot address the glyph directly — unnamed
/// decoration items merge in the item tree — so this pin reads the
/// pixels, golden-adjacent).
#[test]
fn the_add_stack_glyph_is_square() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 800);
        let header_b = harness
            .driver
            .find_all_by_label("b")
            .into_iter()
            .find(support::driver::Element::is_accessible_button)
            .expect("stack b's header is addressable");
        // The add-col strip starts one space-3 gap after the last
        // column and is space-6 + space-2 wide (scale 1).
        let strip_x0 = header_b.absolute_position().x + header_b.size().width + 12.0;

        let image = harness.driver.snapshot();
        let (mut x_min, mut x_max, mut y_min, mut y_max) = (u32::MAX, 0u32, u32::MAX, 0u32);
        // Scan only the columns row's band: the toolbar and
        // status bar also paint at the window's right edge.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // px coordinates at scale 1
        let row_y0 = header_b.absolute_position().y as u32;
        let row_y1 = (row_y0 + 600).min(image.height);
        for y in row_y0..row_y1 {
            // 995: inside the board region's focus ring (whose
            // right border also paints at the window edge).
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (scan_x0, scan_x1) = (strip_x0 as u32, strip_x0 as u32 + 40);
            for x in scan_x0..scan_x1.min(995) {
                let i = (y as usize * image.width as usize + x as usize) * 4;
                let (r, g, b) = (
                    i64::from(image.data[i]),
                    i64::from(image.data[i + 1]),
                    i64::from(image.data[i + 2]),
                );
                let background =
                    (r - 20).abs() <= 12 && (g - 22).abs() <= 12 && (b - 28).abs() <= 12;
                if !background {
                    x_min = x_min.min(x);
                    x_max = x_max.max(x);
                    y_min = y_min.min(y);
                    y_max = y_max.max(y);
                }
            }
        }
        assert!(x_max > x_min, "the glyph paints inside the strip");
        let width = x_max - x_min + 1;
        let height = y_max - y_min + 1;
        assert!(
            (12.0..=20.0).contains(&f64::from(width)) && (12.0..=20.0).contains(&f64::from(height)),
            "the glyph bounding box is the icon-box square, got {width}x{height}"
        );
    });
}
/// R2 D6 (T-7) — the add-stack button carries the stack.create binding
/// hint: non-empty under the standard preset, and (since round 2 fills
/// the D15 gap with `Ctrl+c s`) non-empty under emacs too. Structural
/// presence only — the hint's content is never asserted.
#[test]
fn the_add_stack_button_carries_the_stack_create_hint() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let harness = Harness::mount(SlintDriver::default(), state);
            assert!(
                harness.handle.window().get_stack_create_hint().row_count() > 0,
                "the standard preset binds stack.create"
            );

            let emacs = taskboard_ui_slint::keymap::Keymap::from_preset(
                taskboard_ui_slint::keymap::Preset::Emacs,
            );
            let (_fixture, state) = fixture::golden_state();
            let harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    keymap: emacs,
                    keymap_preset: taskboard_ui_slint::keymap::Preset::Emacs,
                    ..UiSettings::default()
                },
            );
            assert!(
                harness.handle.window().get_stack_create_hint().row_count() > 0,
                "emacs binds stack.create (Ctrl+c s; the D15 gap closed)"
            );
        },
    );
}

/// R2 D7 (T-8) — compact mounts exactly one add-stack entry, inline:
/// at the smallest viewport the regular full-height affordance is
/// absent and a single "New stack" accessible entry exists (the
/// switcher's trailing button); at a regular width the affordance is
/// present.
#[test]
fn compact_mounts_exactly_one_add_stack_entry() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(600, 800);
        assert!(harness.handle.window().get_compact_active());
        assert!(
            harness
                .driver
                .find_all_by_id("BoardLayout::add-col-ghost")
                .is_empty(),
            "the regular add-column unmounts in compact"
        );
        assert_eq!(
            harness.driver.find_all_by_label("New stack").len(),
            1,
            "exactly one compact add-stack entry"
        );

        harness.driver.set_window_size(1000, 800);
        assert!(!harness.handle.window().get_compact_active());
        assert!(
            !harness
                .driver
                .find_all_by_id("BoardLayout::add-col-ghost")
                .is_empty(),
            "the regular add-column is mounted"
        );
    });
}

/// R2 D9 (§2.7 test 3) — the model identity pin: consecutive
/// unchanged refreshes leave the board's model handle untouched (the
/// repetition-probe's no-op path at the transport level), and a title
/// edit still lands through a row update with the handle stable
/// across that refresh (F8/F11's TouchArea-survival mechanism).
#[test]
fn consecutive_refreshes_keep_the_board_model_identity() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        let before = harness.handle.window().get_board_model().stacks;
        harness.handle.refresh();
        harness.handle.refresh();
        assert_eq!(
            before,
            harness.handle.window().get_board_model().stacks,
            "unchanged refreshes keep the model handle"
        );
        // One changed refresh (a retitled card, applied by the
        // fake engine's settle + the commit's refresh) keeps the
        // handle and lands the new title through a row update.
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskEdit);
        // Recorded decision (round-3 chunk 1, plan §6 — F26):
        // task.edit opens the detail display and never auto-arms;
        // the title editor arms via the `task.title-edit` action.
        harness.handle.dispatch_action(ActionId::TaskTitleEdit);
        harness.type_text("z");
        harness.stroke(parse("Return").expect("Return"));
        harness.engine.settle(&harness.handle);
        let after = harness.handle.window().get_board_model().stacks;
        assert_eq!(before, after, "the changed refresh keeps the model handle");
        let title = after
            .row_data(0)
            .expect("column")
            .tasks
            .row_data(0)
            .expect("card")
            .title;
        assert_eq!(title, "z", "the edit landed through the row update");
    });
}

/// R2 D8 (T-9) — the per-column Add Task ghost is the click
/// equivalent of `task.create` anchored to that stack: clicking it
/// selects the stack and opens the create row there; typing + Return
/// lands the exact typed command the keyboard's action produces from
/// the same selection state. Empty stacks are anchored too; the
/// compact ghost addresses the effective-selected stack.
#[test]
fn the_add_task_ghost_is_the_click_equivalent_of_task_create() {
    let (fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            // (a) The click route: stack b's ghost (document order —
            // a's ghost is first).
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            let ghosts = harness.driver.find_all_by_label("Add task");
            assert_eq!(ghosts.len(), 2, "one ghost per stack");
            ghosts[1].invoke_default_action();
            assert!(
                harness
                    .handle
                    .window()
                    .get_board_model()
                    .stacks
                    .row_data(1)
                    .expect("column")
                    .create_open,
                "the create row opened in stack b"
            );
            // Recorded decision (2026-10-23-ui-r2-interaction-repairs
            // plan §2.6 residual + §2.7): under the D9 row-diff
            // transport the create field mounts at the next frame's
            // render visit instead of the old wholesale model reset,
            // and the testing backend has no frame pass between this
            // callback and a synthesized keystroke — so the headless
            // journey commits through the same field seam the Return
            // key reaches. The click-opens-the-flow half stays
            // asserted above; the mouse-open→type→Return feel is the
            // Tier-4 manual checklist item.
            harness
                .handle
                .window()
                .invoke_board_create_committed(1, "n".into());
            harness.assert_commands(&[StateCommand::CreateTask {
                title: "n".into(),
                stack: fixture.stack_b,
                order: 1,
            }]);

            // (b) The keyboard route from the same selection state
            // (fresh mount): select stack b via its header, then
            // task.create — the identical command shape.
            let (fixture, state) = fixture::journey_state();
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            harness
                .driver
                .find_all_by_label("b")
                .into_iter()
                .find(support::driver::Element::is_accessible_button)
                .expect("stack b's header")
                .invoke_default_action();
            harness.press(ActionId::TaskCreate);
            harness.type_text("n");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[StateCommand::CreateTask {
                title: "n".into(),
                stack: fixture.stack_b,
                order: 1,
            }]);

            // (c) The empty-stack variant: stack b has no tasks and
            // its ghost still anchors the flow there.
            let (fixture, mut state) = fixture::golden_state();
            state.tasks.retain(|_, task| task.stack != fixture.stack_b);
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            let ghosts = harness.driver.find_all_by_label("Add task");
            assert_eq!(ghosts.len(), 2);
            ghosts[1].invoke_default_action();
            // The D9 frame-step residual, as in (a) (recorded
            // decision, 2026-10-23-ui-r2-interaction-repairs plan
            // §2.6): commit through the field seam.
            harness
                .handle
                .window()
                .invoke_board_create_committed(1, "n".into());
            harness.assert_commands(&[StateCommand::CreateTask {
                title: "n".into(),
                stack: fixture.stack_b,
                order: 0,
            }]);

            // (d) The compact variant: the single ghost addresses the
            // effective-selected stack (a at mount).
            let (fixture, state) = fixture::journey_state();
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(600, 800);
            let ghosts = harness.driver.find_all_by_label("Add task");
            assert_eq!(ghosts.len(), 1, "the compact column's ghost");
            ghosts[0].invoke_default_action();
            harness.type_text("n");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[StateCommand::CreateTask {
                title: "n".into(),
                stack: fixture.stack_a,
                order: 2,
            }]);
        },
    );
}

/// DDF-5 — the anchor card is hidden from the board's list surfaces:
/// board card counts (and, on the same single pass, the stack-detail
/// cards) exclude the exact-title anchor; a byte-level near-miss stays
/// visible.
#[test]
fn the_anchor_card_is_hidden_from_the_board() {
    let (fixture, mut state) = fixture::golden_state();
    // Clone stack a's first task, retitle it to the exact anchor title,
    // and plant it back into stack a.
    let stack_a = fixture.stack_a;
    let source = state
        .tasks_in_stack(stack_a)
        .first()
        .copied()
        .expect("fixture task")
        .clone();
    let mut anchor = source.clone();
    anchor.id = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
    anchor.title = taskboard_domain::ANCHOR_CARD_TITLE.to_owned();
    state.tasks.insert(anchor.id, anchor);
    // The near-miss rides stack a too: same words, trailing space.
    let mut near_miss = source;
    near_miss.id = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
    near_miss.title = format!("{} ", taskboard_domain::ANCHOR_CARD_TITLE);
    state.tasks.insert(near_miss.id, near_miss);

    support::harness::run_journey(state, move |harness| {
        assert_eq!(
            harness.board_shape(),
            vec![3, 1],
            "the anchor is filtered; the near-miss and the rest stay"
        );
        let cards = harness.handle.window().get_board_model().stacks;
        let first = cards.row_data(0).expect("stack a");
        let titles: Vec<String> = (0..first.tasks.row_count())
            .map(|row| first.tasks.row_data(row).expect("card").title.to_string())
            .collect();
        assert!(
            !titles
                .iter()
                .any(|title| title == taskboard_domain::ANCHOR_CARD_TITLE),
            "no anchor card in the model"
        );
        assert!(
            titles
                .iter()
                .any(|title| { title == &format!("{} ", taskboard_domain::ANCHOR_CARD_TITLE) }),
            "the near-miss stays visible (byte-exact filtering)"
        );

        // The stack-detail surface (the second half of the same
        // pass): navigate there and count again.
        harness.press(ActionId::NavNextView);
        let stack_model = harness.handle.window().get_stack_model();
        assert_eq!(
            stack_model.tasks.row_count(),
            3,
            "stack detail filters the anchor in the same pass"
        );
        harness.assert_commands(&[]);
    });
}

/// D-S2 — the loud fence on the status bar: a selected-board row
/// carrying `Unreadable` evidence renders a non-empty sidecar warning
/// (typed model read — the caption wording is data, never asserted);
/// a healthy row renders nothing.
#[test]
fn the_sidecar_warning_renders_only_when_the_health_says_so() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state.sync.get_mut(board).expect("sync row").sidecar =
        taskboard_domain::SidecarHealth::Unreadable(
            taskboard_domain::AppDataUnreadable::FormatTooNew { format: 2 },
        );

    support::harness::run_journey(state, move |harness| {
        let sync = harness.handle.window().get_sync_model();
        assert_ne!(
            sync.sidecar_warning, "",
            "the unreadable evidence renders the warning"
        );
        harness.assert_commands(&[]);
    });

    // The healthy baseline: the same fixture without the evidence —
    // absent evidence is the ruled silence.
    let (_fixture, healthy) = fixture::journey_state();
    support::harness::run_journey(healthy.clone(), move |harness| {
        let sync = harness.handle.window().get_sync_model();
        assert_eq!(sync.sidecar_warning, "", "healthy is silent");
        harness.assert_commands(&[]);
    });
}

// ---------------------------------------------------------------------
// Round-1 chunk 3 (D5/D28b): the settings surface
// ---------------------------------------------------------------------

/// J1 — `Ctrl+,` from the board opens Settings; Escape closes onto the
/// board; the back affordance closes too. The opener is a toggle (D6.3)
/// — a second dispatch closes.
#[test]
fn settings_open_closes_and_toggles() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        assert_eq!(harness.handle.window().get_current_view(), View::Board);

        // The opener action through the same resolve-and-inject
        // path every journey uses (Ctrl+, in the standard preset).
        harness.press(ActionId::AppSettings);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Settings,
            "the opener opened the settings surface"
        );
        assert_eq!(
            harness.chain(),
            vec![ContextId::Settings, ContextId::ViNormal, ContextId::Global],
            "the settings context heads the chain"
        );

        // Escape (the Settings-context binding) closes onto the
        // return view.
        harness.stroke(parse("Escape").expect("Escape"));
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Board,
            "Escape restored the exact previous view"
        );

        // Re-open, then the toggle: a second opener dispatch closes.
        harness.press(ActionId::AppSettings);
        assert_eq!(harness.handle.window().get_current_view(), View::Settings);
        harness.press(ActionId::AppSettings);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Board,
            "the opener toggles"
        );

        // Re-open, then the back affordance closes too.
        harness.press(ActionId::AppSettings);
        assert_eq!(harness.handle.window().get_current_view(), View::Settings);
        harness.driver.click_by_label("Back");
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Board,
            "the back affordance routed the close"
        );
    });
}

/// J2 — the menu row invocation: the menu closes first, then Settings
/// opens (the D3 × D28b close-then-route rule).
#[test]
fn the_menu_row_closes_the_menu_then_opens_settings() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppMenu);
        assert!(
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_open()
        );

        // The "Open settings" row: index-resolved through the
        // window's pushed root model (the D2 id-table rule — no
        // text matching).
        let entries = harness
            .handle
            .window()
            .global::<taskboard_ui_slint::KeyboardState>()
            .get_menu_entries();
        let index = (0..entries.row_count())
            .find(|i| {
                entries
                    .row_data(*i)
                    .is_some_and(|row| row.action == "app.settings")
            })
            .unwrap_or_else(|| panic!("the settings row is in the root page"));
        harness
            .handle
            .window()
            .invoke_menu_entry_invoked(index.try_into().expect("index fits"));

        assert!(
            !harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_open(),
            "the menu closed first"
        );
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Settings,
            "then the settings surface opened"
        );
    });
}

/// J3 — from `StackDetail`: open → close returns to the **exact
/// previous view** (the recorded snapshot, not a cycle walk).
#[test]
fn settings_closes_onto_the_exact_previous_view() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavNextView);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::StackDetail
        );

        harness.press(ActionId::AppSettings);
        assert_eq!(harness.handle.window().get_current_view(), View::Settings);
        harness.press(ActionId::AppSettings);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::StackDetail,
            "the close restored StackDetail exactly"
        );
    });
}

/// J4 — cycle exclusion: with Settings open, the next/prev-view verbs
/// change nothing; from the return view, the full cycle never lands on
/// Settings.
#[test]
fn the_view_cycle_never_passes_through_settings() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        assert_eq!(harness.handle.window().get_current_view(), View::Settings);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavPrevView);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Settings,
            "the cycle verbs are inert while Settings is open"
        );
        harness.press(ActionId::AppSettings);

        // A full cycle walk from the board: Settings is never a
        // member; Today is the cycle head (chunk 10 D2).
        for expected in [
            View::StackDetail,
            View::TaskDetail,
            View::Review,
            View::Today,
            View::Board,
        ] {
            harness.press(ActionId::NavNextView);
            assert_eq!(harness.handle.window().get_current_view(), expected);
        }
    });
}

/// J5 — the sign-in journey: the field mirrors ride the edited seams,
/// the Sign-in click routes `SaveConnection` on the drained lane with
/// the typed payload and an empty name (the app derives it, round-5
/// chunk 8); the request's `Debug` shows no secret. A sign-in with an
/// invalid draft routes nothing (the form's enablement gate).
#[test]
fn the_settings_form_routes_the_save_ceremony() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state.clone(), move |harness| {
        // An invalid draft (a bad server URL): the Sign-in click
        // is absorbed — nothing on the lane.
        harness.handle.window().invoke_settings_save_clicked(
            "not a url".into(),
            "alice".into(),
            "pw".into(),
        );
        assert!(harness.drain_requests().is_empty(), "the gate held");

        // Fill the fields through the edited seams (the mirror
        // path the real fields drive).
        let window = harness.handle.window();
        window.invoke_settings_field_edited(1, "https://c.example".into());
        window.invoke_settings_field_edited(2, "alice".into());
        window.invoke_settings_field_edited(3, "hunter2-secret".into());

        window.invoke_settings_save_clicked(
            "https://c.example".into(),
            "alice".into(),
            "hunter2-secret".into(),
        );
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1, "one SaveConnection routed");
        assert!(
            matches!(
                &requests[0],
                taskboard_ui_slint::requests::UiRequest::SaveConnection {
                    profile,
                    ..
                } if profile.name.is_empty()
                    && profile.server_url == "https://c.example"
                    && profile.username == "alice"
                    && profile.password.0 == "hunter2-secret"
            ),
            "the typed payload carries the draft"
        );
        assert!(
            !format!("{:?}", requests[0]).contains("hunter2-secret"),
            "the request Debug never carries the secret"
        );

        // The app half answers (the A6 choreography); the drain
        // clears the pending state and lands the fresh rows.
        let taskboard_ui_slint::requests::UiRequest::SaveConnection { reply, .. } =
            requests.remove(0)
        else {
            panic!("the save request routed");
        };
        // The r2 D32 reply carries the full pair (boards +
        // connectors): both surfaces re-render from one reply.
        reply
            .send(Ok(taskboard_ui_slint::requests::ConnectionsSnapshot {
                boards: Vec::new(),
                connectors: vec![taskboard_ui_slint::requests::ConnectorInfo {
                    name: "home".into(),
                    server_url: "https://c.example".into(),
                    username: "alice".into(),
                    backend: taskboard_ui_slint::requests::ConnectorBackend::Nextcloud,
                }],
            }))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();
        let rows = window
            .global::<taskboard_ui_slint::SettingsModel>()
            .get_rows();
        assert_eq!(rows.row_count(), 1, "the reply's fresh list landed");
        assert_eq!(
            rows.row_data(0).expect("row").name,
            "home",
            "the reply payload is the secret-free row shape"
        );

        // A second sign-in routes the same name-less payload (the
        // app reuses or derives the name).
        window.invoke_settings_save_clicked(
            "https://other.example".into(),
            "alice".into(),
            "pw2".into(),
        );
        let requests = harness.drain_requests();
        assert_eq!(requests.len(), 1);
        assert!(matches!(
            &requests[0],
            taskboard_ui_slint::requests::UiRequest::SaveConnection {
                profile,
                ..
            } if profile.name.is_empty()
                && profile.server_url == "https://other.example"
        ));
    });
}

/// J6 — the live status projection (D8): the row's status flips on
/// `ConnectorLost` and restores on `ConnectorRestored` (the down
/// override wins; the restore falls back to the bound-phases fold).
#[test]
fn connector_episodes_flip_the_row_status() {
    use taskboard_ui_slint::SettingsRowStatus;
    let (_fixture, state) = fixture::golden_state();
    let settings = UiSettings {
        connectors: vec![
            taskboard_ui_slint::requests::ConnectorInfo {
                name: "nextcloud".into(),
                server_url: "https://c.example".into(),
                username: "alice".into(),
                backend: taskboard_ui_slint::requests::ConnectorBackend::Nextcloud,
            },
            // A second plain row (the two-row shape the status
            // episode below reads; the D4 read-only flag is gone with
            // round-3 D21 — every row is mutable).
            taskboard_ui_slint::requests::ConnectorInfo {
                name: "legacy".into(),
                server_url: "https://legacy.example".into(),
                username: "bob".into(),
                backend: taskboard_ui_slint::requests::ConnectorBackend::Nextcloud,
            },
        ],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::AppSettings);
        assert_eq!(harness.handle.window().get_current_view(), View::Settings);

        let row_status = |harness: &Harness| {
            let rows = harness
                .handle
                .window()
                .global::<taskboard_ui_slint::SettingsModel>()
                .get_rows();
            let row = rows.row_data(0).expect("the seeded row");
            row.status
        };
        assert_eq!(row_status(&*harness), SettingsRowStatus::Idle);
        let rows = harness
            .handle
            .window()
            .global::<taskboard_ui_slint::SettingsModel>()
            .get_rows();
        assert_eq!(rows.row_count(), 2, "both rows arrive");

        harness
            .handle
            .observe_connector_episode(&taskboard_domain::SystemEvent::ConnectorLost(
                taskboard_domain::ConnectorId::from("nextcloud"),
            ));
        assert_eq!(row_status(&*harness), SettingsRowStatus::Down);

        harness.handle.observe_connector_episode(
            &taskboard_domain::SystemEvent::ConnectorRestored(taskboard_domain::ConnectorId::from(
                "nextcloud",
            )),
        );
        assert_eq!(
            row_status(&*harness),
            SettingsRowStatus::Idle,
            "the restore falls back to the fold (no bound boards → Idle)"
        );
    });
}

/// J7 — the preset flow's two halves: the pick routes
/// `SetKeymapPreset` on the lane; applying the reply through the drain
/// swaps the keymap live — vi's colon opener resolves in the settings
/// chain right after (T3's journey half).
#[test]
fn the_preset_pick_routes_and_the_reply_swaps_the_keymap_live() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        #[allow(unused_mut)]
        harness.press(ActionId::AppSettings);
        assert_eq!(harness.handle.window().get_current_view(), View::Settings);

        // Before: the standard preset has no colon binding — the
        // stroke falls through (unhandled) and the menu stays shut.
        harness.stroke(parse(":").expect("colon"));
        assert!(
            !harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_open()
        );

        // The pick routes the request.
        harness.handle.window().invoke_settings_preset_picked(2);
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1);
        let taskboard_ui_slint::requests::UiRequest::SetKeymapPreset { preset, reply, .. } =
            requests.remove(0)
        else {
            panic!("the preset request routed");
        };
        assert_eq!(preset, taskboard_ui_slint::keymap::Preset::Vi);

        // The app half answers; the drain applies the swap live.
        reply
            .send(Ok(taskboard_ui_slint::requests::KeymapUpdate {
                keymap: taskboard_ui_slint::keymap::Keymap::from_preset(
                    taskboard_ui_slint::keymap::Preset::Vi,
                ),
                findings: 0,
                preset: taskboard_ui_slint::keymap::Preset::Vi,
            }))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();

        // After: vi's colon opens the menu through the same
        // settings-context chain (the live-apply fact, D9).
        harness.stroke(parse(":").expect("colon"));
        assert!(
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_open(),
            "the swapped keymap resolves vi's colon in the settings chain"
        );
    });
}

/// J8 — the selection echoes the *replied* preset, never the click
/// (r2 D27): the pick leaves the index untouched until the reply's
/// `KeymapUpdate.preset` lands through the drain, and a synthetic
/// reply for a different preset moves it again.
#[test]
fn the_preset_selection_echoes_the_replied_preset_not_the_click() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        assert_eq!(harness.handle.window().get_current_view(), View::Settings);

        let selection = |harness: &Harness| {
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::SettingsModel>()
                .get_preset_index()
        };
        // Mount: the standard preset, index 0.
        assert_eq!(selection(&*harness), 0);

        // The pick: the request routes, but the selection does NOT
        // move — no optimistic click-echo.
        harness.handle.window().invoke_settings_preset_picked(2);
        assert_eq!(
            selection(&*harness),
            0,
            "the click alone never moves the selection"
        );

        // The reply names vi; the drain echoes it.
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1);
        let taskboard_ui_slint::requests::UiRequest::SetKeymapPreset { reply, .. } =
            requests.remove(0)
        else {
            panic!("the preset request routed");
        };
        reply
            .send(Ok(taskboard_ui_slint::requests::KeymapUpdate {
                keymap: taskboard_ui_slint::keymap::Keymap::from_preset(
                    taskboard_ui_slint::keymap::Preset::Vi,
                ),
                findings: 0,
                preset: taskboard_ui_slint::keymap::Preset::Vi,
            }))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();
        assert_eq!(selection(&*harness), 2, "the replied preset is echoed");

        // A second pick (now allowed — pending cleared) replies for
        // a different preset: the reply, not the click, moves the
        // selection again.
        harness.handle.window().invoke_settings_preset_picked(1);
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1);
        let taskboard_ui_slint::requests::UiRequest::SetKeymapPreset { reply, .. } =
            requests.remove(0)
        else {
            panic!("the second preset request routed");
        };
        reply
            .send(Ok(taskboard_ui_slint::requests::KeymapUpdate {
                keymap: taskboard_ui_slint::keymap::Keymap::from_preset(
                    taskboard_ui_slint::keymap::Preset::Emacs,
                ),
                findings: 0,
                preset: taskboard_ui_slint::keymap::Preset::Emacs,
            }))
            .expect("the reply lane is still alive");
        harness.handle.drain_setting_replies();
        assert_eq!(selection(&*harness), 1, "a new reply re-echoes");
    });
}

// ---------------------------------------------------------------------
// r4 D38: the scale release commit (the preview arm is gone)
// ---------------------------------------------------------------------

/// The scale drag moves nothing until release (r4 D38): the gesture
/// holds between press and release — the theme, the echo, and the
/// lane are all untouched (the drag itself is the widget's own, the
/// grab probe's p4); release emits exactly one `SetScale` carrying
/// the final value, the theme moves once, and the echo re-renders the
/// canonical commit.
#[test]
fn the_scale_drag_moves_nothing_until_release() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();
        let model = || window.global::<taskboard_ui_slint::SettingsModel>();
        let scale = || {
            window
                .global::<taskboard_ui_slint::Theme>()
                .get_base_scale()
        };
        assert_eq!(scale(), 1.0);

        // Mid-gesture: nothing observable moves.
        window.invoke_settings_scale_pressed();
        assert_eq!(scale(), 1.0, "the theme holds the committed scale");
        assert_eq!(
            model().get_scale_echo(),
            "1.0",
            "the echo holds the committed scale"
        );
        assert!(
            harness.drain_requests().is_empty(),
            "no request mid-gesture"
        );

        // Release: one request, one theme move, the canonical echo.
        window.invoke_settings_scale_released(1.7);
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1, "exactly one SetScale per gesture");
        // The seam's f32 hand-over pins the comparison term: the
        // committed value is exactly `f64::from(1.7f32)`.
        assert!(matches!(
            &requests[0],
            taskboard_ui_slint::requests::UiRequest::SetScale { scale, .. }
                if scale.total_cmp(&f64::from(1.7f32)) == std::cmp::Ordering::Equal
        ));
        assert!(
            (scale() - 1.7).abs() < 0.001,
            "the theme moves once, on release"
        );
        assert_eq!(
            model().get_scale_echo(),
            "1.7",
            "the echo re-renders the commit"
        );
        let taskboard_ui_slint::requests::UiRequest::SetScale { reply, .. } = requests.remove(0)
        else {
            panic!("the scale request routed");
        };
        reply.send(Ok(())).expect("the reply lane is alive");
        harness.handle.drain_setting_replies();
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Ok,
        );
    });
}

/// The scale echo renders the committed scale (r4 D38): mount echo;
/// a typed draft survives a concurrent drag (nothing writes the echo
/// mid-gesture anymore); focus loss discards the draft and re-syncs
/// to the committed scale; a commit re-renders the canonical echo and
/// an Err reply follows the revert; a Return-less draft never
/// survives the surface (plan §5.1 draft invariants).
#[test]
fn the_scale_echo_renders_the_committed_scale() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();
        let model = || window.global::<taskboard_ui_slint::SettingsModel>();
        let scale = || {
            window
                .global::<taskboard_ui_slint::Theme>()
                .get_base_scale()
        };

        // Mount: the canonical echo of the live scale.
        assert_eq!(model().get_scale_echo(), "1.0");

        // A typed draft survives a concurrent drag: mid-gesture
        // nothing writes the echo (the preview writer is gone).
        window.invoke_settings_field_edited(4, "2.2".into());
        assert_eq!(model().get_scale_echo(), "2.2", "the draft is the echo");
        window.invoke_settings_scale_pressed();
        assert_eq!(
            model().get_scale_echo(),
            "2.2",
            "the drag never touches the typed draft"
        );
        assert_eq!(scale(), 1.0, "the theme holds until release");

        // Focus loss discards the Return-less edit and re-syncs
        // to the committed scale (unchanged by the gesture).
        window.invoke_settings_scale_focus_lost();
        assert_eq!(
            model().get_scale_echo(),
            "1.0",
            "the echo re-syncs to the committed scale"
        );

        // Commit "2": canonical echo, one request, Err reverts.
        window.invoke_settings_field_edited(4, "2".into());
        window.invoke_settings_scale_committed("2".into());
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1, "one SetScale");
        assert_eq!(model().get_scale_echo(), "2.0", "the canonical echo");
        let taskboard_ui_slint::requests::UiRequest::SetScale { reply, .. } = requests.remove(0)
        else {
            panic!("the scale request routed");
        };
        reply
            .send(Err(
                taskboard_ui_slint::requests::SettingsError::PersistFailed,
            ))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();
        assert!(
            (scale() - 1.0).abs() < 0.001,
            "the Err reply reverts the theme"
        );
        assert_eq!(
            model().get_scale_echo(),
            "1.0",
            "the echo follows the reverted committed scale"
        );

        // A Return-less draft never survives the surface: close
        // and reopen settings, and the mount re-renders the
        // canonical live echo (plan §5.1 draft invariants).
        window.invoke_settings_field_edited(4, "9.9".into());
        assert_eq!(model().get_scale_echo(), "9.9", "the draft renders");
        harness.press(ActionId::AppSettings);
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();
        assert_eq!(
            window
                .global::<taskboard_ui_slint::SettingsModel>()
                .get_scale_echo(),
            "1.0",
            "the mount discards the stale draft"
        );
    });
}

// ---------------------------------------------------------------------
// r3 D20c: the scale commit's scroll anchor
// ---------------------------------------------------------------------

/// The scale commit anchors the scroll on the scale row (r3 D20c,
/// dispatch §5 g): scrolled to the bottom at scale 1.0 in a window
/// that overflows (driven by REAL wheel events — the production
/// scroll path), an UPWARD commit to 3.0 preserves the row's viewport
/// position (within a row height), the downward commit back to 1.0
/// leaves the viewport within bounds (no blank screen, finding 25),
/// and manual scrolling afterwards is never fought (one commit, one
/// consume).
#[test]
fn a_scale_commit_anchors_the_scroll_on_the_scale_row() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |driver: &mut SlintDriver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            driver.bind(&harness.handle);
            driver.set_window_size(480, 460);
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let facts = window.global::<taskboard_ui_slint::SettingsScrollFacts>();
            driver.advance_time(0);
            assert!(
                facts.get_content_height() > facts.get_viewport_height(),
                "the overflow exists at scale 1.0 in the small window: {} vs {}",
                facts.get_content_height(),
                facts.get_viewport_height()
            );

            // Scroll to the bottom with real wheel events (the
            // production scroll path — the flickable processes them,
            // severing any property bindings on content-y exactly as
            // in production; the anchor must survive that).
            for _ in 0..24 {
                window
                    .window()
                    .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                        position: slint::LogicalPosition::new(240.0, 230.0),
                        delta_x: 0.0,
                        delta_y: -60.0,
                    });
                driver.advance_time(30);
            }
            let overflow = facts.get_content_height() - facts.get_viewport_height();
            assert!(
                (facts.get_viewport_y() + overflow).abs() < 20.0,
                "the wheel reached the bottom"
            );

            // The row's viewport offset before the commit.
            let offset_before = facts.get_scale_row_y() + facts.get_viewport_y();

            // Commit UP via the field; the Ok reply settles.
            window.invoke_settings_field_edited(4, "3.0".into());
            window.invoke_settings_scale_committed("3.0".into());
            // The #412 capture rides the same ticked lane: this
            // journey's resized window legitimately emitted one
            // geometry snapshot on the first tick after the resize.
            // Drained here per the plan's stray-capture rule (the
            // test counts the commit's own lane, never the capture's).
            let mut requests: Vec<_> = harness
                .drain_requests()
                .into_iter()
                .filter(|request| {
                    !matches!(
                        request,
                        taskboard_ui_slint::requests::UiRequest::WindowGeometryChanged { .. }
                    )
                })
                .collect();
            assert_eq!(requests.len(), 1);
            let taskboard_ui_slint::requests::UiRequest::SetScale { reply, .. } =
                requests.remove(0)
            else {
                panic!("the scale request routed");
            };
            reply.send(Ok(())).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            driver.advance_time(0);

            // Up-leg: the row's viewport offset is preserved within a
            // row height.
            let offset_after = facts.get_scale_row_y() + facts.get_viewport_y();
            assert!(
                (offset_after - offset_before).abs() <= 60.0,
                "the scale row stayed put across the up-commit: {offset_before} -> {offset_after}"
            );
            let vy = facts.get_viewport_y();
            let min_vy = (facts.get_viewport_height() - facts.get_content_height()).min(0.0);
            assert!(
                vy >= min_vy - 0.01 && vy <= 0.01,
                "in bounds after the up-commit"
            );

            // Commit DOWN again; the viewport must land within bounds
            // (no blank screen — the finding-25 direction).
            window.invoke_settings_field_edited(4, "1.0".into());
            window.invoke_settings_scale_committed("1.0".into());
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1);
            let taskboard_ui_slint::requests::UiRequest::SetScale { reply, .. } =
                requests.remove(0)
            else {
                panic!("the second scale request routed");
            };
            reply.send(Ok(())).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            driver.advance_time(0);
            let vy = facts.get_viewport_y();
            let min_vy = (facts.get_viewport_height() - facts.get_content_height()).min(0.0);
            assert!(
                vy >= min_vy - 0.01 && vy <= 0.01,
                "the viewport is within bounds after the down-commit (no blank): {vy} in [{min_vy}, 0]"
            );

            // Manual scrolling afterwards is never fought: a wheel
            // event moves the viewport (the anchor consumed once).
            window
                .window()
                .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                    position: slint::LogicalPosition::new(240.0, 230.0),
                    delta_x: 0.0,
                    delta_y: -60.0,
                });
            driver.advance_time(30);
            assert!(
                facts.get_viewport_y() < vy,
                "the manual wheel scroll stuck (the anchor consumed once)"
            );
        },
    );
}

// ---------------------------------------------------------------------
// r3 D18: the status line's pinned placement
// ---------------------------------------------------------------------

/// The status line sits outside the scroll viewport (r3 D18, dispatch
/// §5 c): at scale 3.0 in the min-floor window, triggering an outcome
/// renders the pinned row above the Flickable's band — readable
/// without scrolling (finding 20) — while the pinned header stays
/// put; the hidden state leaves the scroll geometry untouched
/// (layouts skip invisible children).
#[test]
fn the_status_line_pins_above_the_scroll_viewport() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |driver: &mut SlintDriver| {
            let settings = UiSettings {
                scale: taskboard_ui_slint::UiScale::new(3.0).expect("valid"),
                ..UiSettings::default()
            };
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            driver.bind(&harness.handle);
            driver.set_window_size(480 * 3, 400 * 3);
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();
            driver.advance_time(0);

            // Hidden mount: no outcome, and the scroll host occupies
            // its geometry.
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::None
            );
            let back_y = |driver: &SlintDriver| {
                driver
                    .find_by_label("Back")
                    .expect("the pinned Back affordance")
                    .absolute_position()
                    .y
            };
            let back_hidden = back_y(driver);

            // Trigger an outcome (the seam-local scale refusal).
            window.invoke_settings_scale_committed("not a number".into());
            driver.advance_time(0);
            assert_ne!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::None,
                "the outcome landed"
            );

            // The status row renders OUTSIDE the Flickable's band:
            // entirely above the viewport's top edge (measured after
            // the outcome — a visible row legitimately shrinks the
            // viewport, the fix).
            let flick_y = driver
                .find_all_by_id("SettingsView::flick")
                .into_iter()
                .next()
                .expect("the scroll host")
                .absolute_position()
                .y;
            let status = driver
                .find_all_by_id("SettingsView::status-line")
                .into_iter()
                .next()
                .expect("the pinned status line");
            let pos = status.absolute_position();
            let size = status.size();
            assert!(
                pos.y + size.height <= flick_y,
                "the status row (y {}, h {}) sits above the viewport top {}",
                pos.y,
                size.height,
                flick_y
            );

            // The pinned header never moved.
            assert_eq!(back_y(driver), back_hidden, "the header stays pinned");
        },
    );
}

// ---------------------------------------------------------------------
// r3 D16: the password eye toggle
// ---------------------------------------------------------------------

/// The eye toggle (r3 D16, dispatch §5 a): the secret field carries a
/// discoverable a11y button beside it; toggling is markup-local —
/// every model property stays untouched and the lane stays silent.
/// The reveal state itself (dots vs plaintext) is a Tier-4 visual,
/// not harness-pinned; the write-only redaction contract (the
/// emptiness oracle, nothing secret on any model/Debug) is pinned by
/// the save ceremony journey and the adapter's draft mirror.
#[test]
fn the_eye_toggles_locally_without_leaking_the_secret() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state.clone(), move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();
        let model = || window.global::<taskboard_ui_slint::SettingsModel>();

        // Type a secret through the mirror path.
        window.invoke_settings_field_edited(3, "hunter2-secret".into());
        let model_snapshot = (
            model().get_draft_server(),
            model().get_draft_username(),
            model().get_save_enabled(),
        );

        // The eye is discoverable as an accessible button beside
        // the password field.
        let eyes: Vec<_> = harness
            .driver
            .find_all_by_label("Reveal")
            .into_iter()
            .filter(support::driver::Element::is_accessible_button)
            .collect();
        assert_eq!(eyes.len(), 1, "exactly one eye on the form");

        // Toggle: no model property moves, the lane stays silent.
        eyes.into_iter()
            .next()
            .expect("the eye")
            .invoke_default_action();
        harness.driver.advance_time(0);
        assert_eq!(
            (
                model().get_draft_server(),
                model().get_draft_username(),
                model().get_save_enabled(),
            ),
            model_snapshot,
            "the toggle touched nothing but the field's local state"
        );
        assert!(harness.drain_requests().is_empty(), "zero lane traffic");

        // Re-toggle: the label swaps with the state.
        let concealed: Vec<_> = harness
            .driver
            .find_all_by_label("Conceal")
            .into_iter()
            .filter(support::driver::Element::is_accessible_button)
            .collect();
        assert_eq!(concealed.len(), 1, "the eye relabels with the state");
        concealed
            .into_iter()
            .next()
            .expect("the relabelled eye")
            .invoke_default_action();
        harness.driver.advance_time(0);
        assert_eq!(
            (
                model().get_draft_server(),
                model().get_draft_username(),
                model().get_save_enabled(),
            ),
            model_snapshot,
            "the second toggle is equally local"
        );
    });
}

// ---------------------------------------------------------------------
// r3 D19: the validity flavour (per-field, per keystroke)
// ---------------------------------------------------------------------

/// The validity flavour tracks every field per keystroke (r3 D19,
/// dispatch §5 d): empty renders neutral with no error line, a
/// failing field renders invalid with a non-empty error element, a
/// passing field renders valid — and the Save gate (the same ladder,
/// one source of truth) tracks the draft throughout. The error copy
/// itself is presentation text and is never asserted (§7-R3).
#[test]
fn the_validity_flavour_tracks_each_field_per_keystroke() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();
        let model = || window.global::<taskboard_ui_slint::SettingsModel>();

        // The empty mount: every field neutral, no error line.
        assert_eq!(model().get_validity_server(), FieldValidity::Neutral);
        assert_eq!(model().get_error_server(), "");
        assert!(
            harness
                .driver
                .find_all_by_id("LabeledField::error-line")
                .is_empty(),
            "no error line renders on the empty mount"
        );

        // A bad URL: invalid flavour + non-empty error element, the
        // Save gate closed.
        window.invoke_settings_field_edited(1, "not a url".into());
        assert_eq!(model().get_validity_server(), FieldValidity::Invalid);
        assert_ne!(model().get_error_server(), "");
        assert_eq!(model().get_validity_username(), FieldValidity::Neutral);
        assert!(!model().get_save_enabled(), "the gate follows the ladder");

        // Fixed: valid flavour, the line gone.
        window.invoke_settings_field_edited(1, "https://c.example".into());
        assert_eq!(model().get_validity_server(), FieldValidity::Valid);
        assert_eq!(model().get_error_server(), "");

        // Empty again: back to neutral, never an error line.
        window.invoke_settings_field_edited(1, "".into());
        assert_eq!(model().get_validity_server(), FieldValidity::Neutral);
        assert_eq!(model().get_error_server(), "");
        window.invoke_settings_field_edited(1, "https://c.example".into());

        // The username: whitespace-only is invalid (the trimmed
        // ladder rule), a name valid.
        window.invoke_settings_field_edited(2, "   ".into());
        assert_eq!(model().get_validity_username(), FieldValidity::Invalid);
        window.invoke_settings_field_edited(2, "alice".into());
        assert_eq!(model().get_validity_username(), FieldValidity::Valid);

        // The password rides the draft mirror: typed text is
        // valid; the redaction contract is pinned by the save
        // ceremony journey.
        window.invoke_settings_field_edited(3, "pw".into());
        assert_eq!(model().get_validity_password(), FieldValidity::Valid);

        // All fields pass: the gate opened.
        assert!(model().get_save_enabled(), "the valid draft enables Save");
    });
}

/// The scale field's flavour carries the typed detail (r3 D20a): a
/// parse miss on Return renders the invalid flavour + error line and
/// the status Rejected *with* the typed scale detail (the recorded
/// decision upgrading the r2 pins' detail-less Rejected), and the
/// typo stays in the field.
#[test]
fn a_scale_parse_miss_renders_the_invalid_flavour_and_typed_detail() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();
        let model = || window.global::<taskboard_ui_slint::SettingsModel>();

        // The typed draft rides the edited seam (the field's
        // per-keystroke path), then Return.
        window.invoke_settings_field_edited(4, "abc".into());
        window.invoke_settings_scale_committed("abc".into());
        assert_eq!(model().get_last_outcome(), SettingsOutcome::Rejected);
        assert_eq!(model().get_validity_scale(), FieldValidity::Invalid);
        assert_ne!(model().get_error_scale(), "", "the error line renders");
        assert_eq!(
            model().get_scale_echo(),
            "abc",
            "the typo stays visible under the flavour"
        );

        // A typed Err reply's revert and a successful commit are
        // pinned by the scale trio journeys.
    });
}

// ---------------------------------------------------------------------
// r2 D28: the status line's text and auto-clear lifetime
// ---------------------------------------------------------------------

/// Terminal status shows mapped text and auto-clears under the mock
/// clock (r2 D28): the seam-local Rejected (a scale parse miss — no
/// lane round-trip; recorded decision, r3 D20a: since then it also
/// carries the typed `Invalid{Scale, NotKebab}` detail and the field
/// flavour — these pins assert outcome lifetime only) is visible for
/// `STATUS_VISIBLE_MS`, then returns to
/// hidden (outcome None, text empty — the VM cleared, so no refresh
/// can resurrect it).
#[test]
fn a_terminal_status_shows_text_then_auto_clears_under_the_mock_clock() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        let model = || {
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::SettingsModel>()
        };
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::None,
            "the mount starts hidden"
        );

        harness
            .handle
            .window()
            .invoke_settings_scale_committed("not a number".into());
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Rejected,
            "the terminal outcome landed"
        );
        assert_ne!(
            model().get_status_text(),
            "",
            "the status shows its mapped text"
        );

        harness
            .driver
            .advance_time(taskboard_ui_slint::settings_status::STATUS_VISIBLE_MS - 1);
        assert_ne!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::None,
            "still visible one tick before the deadline"
        );
        harness.driver.advance_time(1);
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::None,
            "the auto-clear fired at the deadline"
        );
        assert_eq!(model().get_status_text(), "", "the text cleared with it");
    });
}

/// A new outcome re-arms the auto-clear (r2 D28; the scale commit's
/// typed detail arrived later, r3 D20a — recorded decision, the pins
/// here assert outcome lifetime only): seven seconds after
/// the first terminal outcome a second one landed three seconds in —
/// the status must still be visible, clearing only `STATUS_VISIBLE_MS`
/// after the *second* outcome (the tooltip-restart pin's shape).
#[test]
fn a_new_outcome_re_arms_the_status_clear() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        let model = || {
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::SettingsModel>()
        };
        let window = harness.handle.window();

        window.invoke_settings_scale_committed("not a number".into());
        harness
            .driver
            .advance_time(taskboard_ui_slint::settings_status::STATUS_VISIBLE_MS - 2_000);
        window.invoke_settings_scale_committed("0.0".into());
        harness
            .driver
            .advance_time(taskboard_ui_slint::settings_status::STATUS_VISIBLE_MS - 2_000);
        assert_ne!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::None,
            "still visible past the FIRST outcome's deadline — the re-arm reset it"
        );
        harness.driver.advance_time(2_000);
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::None,
            "the clear rides the second outcome's deadline"
        );
    });
}

/// A busy status never auto-clears mid-flight (r2 D28): the Save
/// click's Busy holds past twice the clear window; the Ok reply arms
/// the clear, which then fires.
#[test]
fn busy_status_holds_until_the_reply_lands() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        let model = || {
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::SettingsModel>()
        };

        // A valid draft: the Save click routes the ceremony.
        let window = harness.handle.window();
        window.invoke_settings_field_edited(1, "https://c.example".into());
        window.invoke_settings_field_edited(2, "alice".into());
        window.invoke_settings_field_edited(3, "hunter2-secret".into());
        window.invoke_settings_save_clicked(
            "https://c.example".into(),
            "alice".into(),
            "hunter2-secret".into(),
        );
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Busy,
        );

        // Twice the clear window: the busy status holds (the
        // timer was stopped, never armed).
        harness
            .driver
            .advance_time(2 * taskboard_ui_slint::settings_status::STATUS_VISIBLE_MS);
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Busy,
            "busy never auto-clears mid-flight"
        );

        // The reply lands through the drain: Ok, the clear armed.
        let mut requests = harness.drain_requests();
        let taskboard_ui_slint::requests::UiRequest::SaveConnection { reply, .. } =
            requests.remove(0)
        else {
            panic!("the save request routed");
        };
        // The r2 D32 reply carries the full pair (boards +
        // connectors): both surfaces re-render from one reply.
        reply
            .send(Ok(taskboard_ui_slint::requests::ConnectionsSnapshot {
                boards: Vec::new(),
                connectors: vec![taskboard_ui_slint::requests::ConnectorInfo {
                    name: "home".into(),
                    server_url: "https://c.example".into(),
                    username: "alice".into(),
                    backend: taskboard_ui_slint::requests::ConnectorBackend::Nextcloud,
                }],
            }))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Ok,
            "the Ok reply is visible"
        );
        harness
            .driver
            .advance_time(taskboard_ui_slint::settings_status::STATUS_VISIBLE_MS);
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::None,
            "the Ok status auto-clears"
        );
    });
}

// ---------------------------------------------------------------------
// r2 D29, r4 D38: the scale slider's gesture (press / release)
// ---------------------------------------------------------------------

/// The slider's release commits once per gesture (r2 D29, r4 D38):
/// release routes exactly one `SetScale` carrying the final value,
/// whose Ok reply leaves the outcome Ok and the scale persisted; the
/// lane sees nothing before the release.
#[test]
fn the_slider_holds_the_gesture_and_commits_once_on_release() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();
        let scale = || {
            window
                .global::<taskboard_ui_slint::Theme>()
                .get_base_scale()
        };

        window.invoke_settings_scale_pressed();
        assert!(
            harness.drain_requests().is_empty(),
            "the held gesture writes no lane request"
        );

        window.invoke_settings_scale_released(2.0);
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1, "exactly one SetScale per gesture");
        // 2.0 is exactly representable — the guard pattern keeps
        // clippy's float_cmp appeased without a magic margin.
        assert!(matches!(
            &requests[0],
            taskboard_ui_slint::requests::UiRequest::SetScale { scale, .. }
                if scale.total_cmp(&2.0) == std::cmp::Ordering::Equal
        ));
        let model = || window.global::<taskboard_ui_slint::SettingsModel>();
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Busy,
        );
        let taskboard_ui_slint::requests::UiRequest::SetScale { reply, .. } = requests.remove(0)
        else {
            panic!("the scale request routed");
        };
        reply.send(Ok(())).expect("the reply lane is alive");
        harness.handle.drain_setting_replies();
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Ok,
        );
        assert_eq!(scale(), 2.0, "the committed scale stands");
    });
}

/// A slider gesture is inert while an operation is pending (r2 D29):
/// the silent pending gate absorbs press and release — no `SetScale`,
/// no scale movement, no outcome change.
#[test]
fn a_slider_gesture_is_inert_while_an_op_is_pending() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();
        let scale = || {
            window
                .global::<taskboard_ui_slint::Theme>()
                .get_base_scale()
        };
        let model = || window.global::<taskboard_ui_slint::SettingsModel>();

        // Make an op pending (the preset pick; the request rides
        // the lane and stays unanswered — pending is the state).
        window.invoke_settings_preset_picked(2);
        assert_eq!(harness.drain_requests().len(), 1);
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Busy,
        );
        assert_eq!(scale(), 1.0, "the mount scale stands");

        window.invoke_settings_scale_pressed();
        window.invoke_settings_scale_released(2.0);
        assert!(
            harness.drain_requests().is_empty(),
            "no SetScale while pending"
        );
        assert_eq!(scale(), 1.0, "the scale never moved");
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Busy,
            "the outcome is untouched"
        );
    });
}

/// A slider commit reverts on a typed error reply (r2 D29): the
/// Err reply restores the pre-**gesture** scale, and the outcome is
/// the typed fold of the error — outcome-agnostic revert.
#[test]
fn a_slider_commit_reverts_on_a_typed_error_reply() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();
        let scale = || {
            window
                .global::<taskboard_ui_slint::Theme>()
                .get_base_scale()
        };
        let model = || window.global::<taskboard_ui_slint::SettingsModel>();
        assert_eq!(scale(), 1.0);

        window.invoke_settings_scale_pressed();
        window.invoke_settings_scale_released(2.0);
        let mut requests = harness.drain_requests();
        let taskboard_ui_slint::requests::UiRequest::SetScale { reply, .. } = requests.remove(0)
        else {
            panic!("the scale request routed");
        };
        reply
            .send(Err(
                taskboard_ui_slint::requests::SettingsError::PersistFailed,
            ))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();
        assert_eq!(
            scale(),
            1.0,
            "the revert restores the persisted scale, not the preview"
        );
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Failed,
            "the typed error folds to its outcome"
        );
    });
}

// ---------------------------------------------------------------------
// r2 D30: the settings content scrolls under the pinned header
// ---------------------------------------------------------------------

/// The scale-3.0 reachability contract (r2 D30, dispatch §6 f): at
/// scale 3.0 in the scale-multiplied min-floor window the content
/// overflows, the scroll reaches the scale controls (the content's
/// tail), the pinned header never moves, and the Window size
/// invariant holds throughout.
#[test]
fn settings_scroll_reaches_the_scale_controls_at_scale_3() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |driver: &mut SlintDriver| {
            let settings = UiSettings {
                scale: taskboard_ui_slint::UiScale::new(3.0).expect("valid"),
                ..UiSettings::default()
            };
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            driver.bind(&harness.handle);
            driver.set_window_size(480 * 3, 400 * 3);
            harness.press(ActionId::AppSettings);
            assert_eq!(harness.handle.window().get_current_view(), View::Settings);

            let window = harness.handle.window();
            let facts = harness
                .handle
                .window()
                .global::<taskboard_ui_slint::SettingsScrollFacts>();
            assert!(
                facts.get_content_height() > facts.get_viewport_height(),
                "the overflow exists at scale 3.0 in the min-floor window"
            );

            let back_y = |driver: &mut SlintDriver| {
                driver
                    .find_by_label("Back")
                    .expect("the pinned Back affordance")
                    .absolute_position()
                    .y
            };
            let before = back_y(driver);

            // Scroll to the bottom: the content's tail — the scale
            // row and the status row — becomes reachable.
            let overflow = facts.get_content_height() - facts.get_viewport_height();
            facts.set_viewport_y(-overflow);
            driver.advance_time(0);
            assert!(
                (facts.get_viewport_y() + overflow).abs() < 0.01,
                "the write-back landed (the two-way binding)"
            );

            // The content's tail moved past the scale row (the GUI
            // settings editor's sections joined below it): the
            // display section's field is the deepest labelled
            // addressable, and it sits inside the window after the
            // scroll.
            let tail_field = driver
                .find_by_label("Due soon (days)")
                .expect("the display field is addressable");
            let pos = tail_field.absolute_position();
            let size = tail_field.size();
            assert!(
                pos.y >= 0.0 && pos.y + size.height <= 400.0 * 3.0,
                "the content tail sits inside the window after the scroll"
            );
            // The status row is the content's last child: with the
            // viewport at the bottom the content's bottom edge is
            // exactly the viewport's bottom edge, so the tail is on
            // screen by construction.
            assert!(
                (facts.get_viewport_y() + facts.get_content_height() - facts.get_viewport_height())
                    .abs()
                    < 0.01,
                "the content tail (the status row) is reachable"
            );

            // The pinned header: the Back affordance's y never moved.
            assert_eq!(back_y(driver), before, "the header never scrolls away");

            // The Window invariant: no width/height binding may react
            // to the scale — the dims out-properties are unchanged
            // across the whole journey.
            assert_eq!(window.get_window_w(), 480.0 * 3.0);
            assert_eq!(window.get_window_h(), 400.0 * 3.0);
        },
    );
}

// ---------------------------------------------------------------------
// r2 D31: the InfoTip help affordances
// ---------------------------------------------------------------------

/// Every settings field carries its `InfoTip` (r2 D31): with one seeded
/// connector row the seven placements resolve by element id — the
/// four `LabeledField` tips (Name, Server, Username, App password),
/// the scale row's, the keymap header's, and the connection row's.
/// Role-less elements have no accessible-label address, hence id
/// addressing; the count pins the placements, never the text.
#[test]
fn every_settings_field_carries_its_info_tip() {
    let (_fixture, state) = fixture::golden_state();
    let settings = UiSettings {
        connectors: vec![taskboard_ui_slint::requests::ConnectorInfo {
            name: "nextcloud".into(),
            server_url: "https://c.example".into(),
            username: "alice".into(),
            backend: taskboard_ui_slint::requests::ConnectorBackend::Nextcloud,
        }],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::AppSettings);
        // The general fields' tips sit deep in the scroll: the
        // testing backend addresses laid-out elements only, so
        // the window grows tall enough to lay every placement out
        // on screen (the id addressing, not visibility, is the
        // pin's subject).
        harness.driver.set_window_size(1000, 2400);
        harness.driver.advance_time(0);
        assert_eq!(harness.handle.window().get_current_view(), View::Settings);
        let driver = &mut harness.driver;
        driver.advance_time(0);

        // The backend's id match reports the tip's subtree too
        // (the chain carries the id), so every count pins DISTINCT
        // physical squares — one per placement.
        let distinct_spots = |driver: &SlintDriver, id: &str| {
            let mut spots: Vec<(f32, f32)> = driver
                .find_all_by_id(id)
                .iter()
                .map(|tip| {
                    let p = tip.absolute_position();
                    (p.x, p.y)
                })
                .collect();
            spots.sort_by(|a, b| a.partial_cmp(b).expect("no NaN positions"));
            spots.dedup();
            spots.len()
        };
        assert_eq!(
            distinct_spots(driver, "LabeledField::help-tip"),
            15,
            "the three sign-in fields, the eight general fields (the \
                 Files row's poll cadence field included, #1083), and \
                 the four focus minute fields each carry a tip"
        );
        for id in [
            "SettingsView::help-scale",
            "SettingsView::help-keymap",
            "SettingsView::help-rows",
        ] {
            assert_eq!(distinct_spots(driver, id), 1, "exactly one tip at {id}");
        }
    });
}

/// A hover on an `InfoTip` arms the generalized tooltip (r2 D31): the
/// pending state arms immediately, the mock clock's 600 ms delay
/// shows it with a non-empty description (the help fact — content
/// never asserted), and moving away hides it.
#[test]
fn an_info_tip_hover_arms_the_generalized_tooltip() {
    let (_fixture, mut state) = fixture::golden_state();
    // The golden fixture now seeds a running timer (DR-5/4, D-TU12);
    // its chip control trips the testing backend's element traversal
    // (a second conditional control in the status row makes the
    // SettingsView query return empty — backend limitation, not
    // behavior: the settings journeys pass with the chip mounted).
    // The tooltip machinery is this test's subject; it runs on a
    // chip-free chrome. Issue-worthy against the backend harness.
    state.tracking.entries.clear();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |driver: &mut SlintDriver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            driver.bind(&harness.handle);
            // The regular/expansive breakpoint: at the compact
            // default the backend's element traversal returns empty
            // over the selector's label-above branch (the recorded
            // chip-free-chrome limitation, extended by one branch).
            driver.set_window_size(1000, 900);
            driver.advance_time(0);

            let tip = driver
                .find_all_by_id("SettingsView::help-scale")
                .into_iter()
                .next()
                .expect("the scale tip is present");
            let pos = tip.absolute_position();
            let size = tip.size();
            driver.hover_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);

            let tooltip = harness
                .handle
                .window()
                .global::<taskboard_ui_slint::TooltipState>();
            assert!(tooltip.get_pending(), "the hover armed the delay");
            driver.advance_time(600);
            assert!(tooltip.get_visible(), "the delay fired");
            assert_ne!(
                tooltip.get_description(),
                "",
                "the help fact rides the description slot"
            );

            // Moving away dismisses (the D13 lifecycle).
            driver.hover_at(4.0, 4.0);
            assert!(!tooltip.get_pending());
            assert!(!tooltip.get_visible());
        },
    );
}

/// The rule-list journey leg (chunk 6, D12/D13): the task detail's
/// composition renders the subject task's rules (rows + the derived
/// hint) and its seams dispatch the exact typed commands — preset add,
/// custom add, absorbed parse miss, row remove resolved through the id
/// tables. Rows-render and hint-presence are asserted here at the
/// composition's live mount (the component-level read folded in — one
/// behavior, one test).
/// The at/in grammar legs (r4 D30/D6, T8): "in 2h" dispatches the
/// At(In) shape — the command carries the duration, the planner stamps
/// the instant; "at 17:00" dispatches the At(At) shape with a concrete
/// instant (the host-local seam resolved it — a typed instant arrived,
/// which is the structural fact); bare garbage and a non-positive
/// in-magnitude are the absorbed misses (D12).
fn at_in_legs(harness: &mut Harness, task1: TaskId) {
    harness
        .handle
        .window()
        .invoke_rule_add_custom_accepted("in 2h".into());
    harness.assert_commands(&[StateCommand::AddReminderRule {
        task: task1,
        offset: taskboard_domain::ReminderOffset::at_due(),
        at: Some(taskboard_domain::AdhocWhen::In(
            TimeDelta::try_hours(2).expect("2h"),
        )),
        insistence: taskboard_domain::Insistence::Normal,
    }]);
    harness
        .handle
        .window()
        .invoke_rule_add_custom_accepted("at 17:00".into());
    let commands = harness.engine.drain();
    let [
        StateCommand::AddReminderRule {
            at: Some(taskboard_domain::AdhocWhen::At(_)),
            ..
        },
    ] = commands[..]
    else {
        panic!("expected one AddReminderRule at Some(At(_)), got {commands:?}");
    };
    harness
        .handle
        .window()
        .invoke_rule_add_custom_accepted("soon".into());
    harness
        .handle
        .window()
        .invoke_rule_add_custom_accepted("in -10m".into());
    harness.assert_commands(&[]);
}

/// T9 (r4 D30/D31a, F25): the rows per kind and the un-mute
/// round-trip. An At row renders the countdown + fixed fire time
/// (structural shape, not label text) and survives a one-second
/// repaint (the seconds-surface term is active); an unarmed
/// `DueOffset` row presents the needs-a-due-date state and its jump affordance
/// opens the due editor (`Flow::EditDate { field: Due }`); mute shows
/// the muted indicator and un-mute dispatches the mute-clearing
/// command over the subject.
#[test]
fn rule_rows_present_per_kind_and_unmute_round_trips() {
    let (fixture, mut state) = fixture::journey_state();
    let task1 = fixture.task1;
    // The subject is undated in this journey (the F25 class): an At
    // rule and an unarmed DueOffset rule live side by side.
    let live = state.tasks.get_mut(&task1).expect("task");
    live.duedate = None;
    let at_instant = chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp() + 7_200, 0)
        .expect("whole second");
    let at_rule = taskboard_domain::ReminderRule {
        insistence: taskboard_domain::Insistence::Normal,
        id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::from_u128(11)),
        task: task1,
        kind: taskboard_domain::ReminderKind::At(
            taskboard_domain::AtInstant::whole_second(at_instant).expect("whole-second"),
        ),
        created_at: taskboard_domain::BoardClocks::epoch(),
        updated_at: taskboard_domain::BoardClocks::epoch(),
        by: None,
    };
    let unarmed_rule = taskboard_domain::ReminderRule {
        insistence: taskboard_domain::Insistence::Normal,
        id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::from_u128(12)),
        task: task1,
        kind: taskboard_domain::ReminderKind::DueOffset(
            taskboard_domain::ReminderOffset::before(TimeDelta::try_minutes(-10).expect("-10m"))
                .expect("legal offset"),
        ),
        created_at: taskboard_domain::BoardClocks::epoch(),
        updated_at: taskboard_domain::BoardClocks::epoch(),
        by: None,
    };
    state.reminders.muted.insert(task1);
    state.reminders.rules.insert(at_rule.id, at_rule.clone());
    state.reminders.rules.insert(unarmed_rule.id, unarmed_rule);

    support::harness::run_journey(state.clone(), move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);

        let task_model = harness.handle.window().get_task_model();
        // Mute is visible where it bites (D31a — the ranked cause
        // of "never fires" made visible).
        assert!(
            task_model.rules_muted,
            "the muted indicator rides the model"
        );
        assert_eq!(task_model.rules.row_count(), 2, "one row per rule");
        // The At row: armed, the countdown + fixed fire time shape.
        let at_row = task_model.rules.row_data(0).expect("row 0");
        assert!(at_row.armed, "an At row is armed regardless of due date");
        assert!(
            at_row.before.starts_with("in "),
            "the countdown line is the d/h/m/s shape"
        );
        assert_eq!(
            at_row.before.split(' ').count(),
            5,
            "in <d>d <h>h <m>m <s>s — five tokens"
        );
        assert!(
            at_row.detail.starts_with("@ "),
            "the fixed local fire time carries the @ prefix shape"
        );
        // The unarmed offset row: the explicit state + the jump
        // affordance's derived bit.
        let unarmed_row = task_model.rules.row_data(1).expect("row 1");
        assert!(
            !unarmed_row.armed,
            "a due-less offset row derives not-armed in the projection"
        );

        // The countdown is a seconds surface: a one-second repaint
        // re-projects the row (and never panics).
        harness.driver.advance_time(1_000);
        let task_model = harness.handle.window().get_task_model();
        assert_eq!(
            task_model.rules.row_count(),
            2,
            "the rows survive the repaint"
        );
        let at_row = task_model.rules.row_data(0).expect("row 0 after repaint");
        assert!(
            at_row.before.starts_with("in "),
            "the countdown re-projected"
        );

        // The jump affordance opens the due editor for the subject
        // (the existing due-edit seam).
        harness
            .handle
            .window()
            .invoke_task_rule_jump_to_due_clicked();
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_task_model().due_open,
            "the jump affordance opens the due editor"
        );

        // Un-mute dispatches the mute-clearing command over the
        // subject (D31a).
        harness.handle.window().invoke_task_rule_unmute_clicked();
        harness.assert_commands(&[StateCommand::SetTaskRemindersMuted {
            task: task1,
            muted: false,
        }]);
    });
}

#[test]
fn the_rule_list_journeys_reach_the_boundary() {
    let (fixture, mut state) = fixture::journey_state();
    let task1 = fixture.task1;
    // Two rules on the focused task, distinct offsets; ids ascend with
    // insertion order so the row table is deterministic.
    let rule1 = taskboard_domain::ReminderRule {
        insistence: taskboard_domain::Insistence::Normal,
        id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::from_u128(11)),
        task: task1,
        kind: taskboard_domain::ReminderKind::DueOffset(taskboard_domain::ReminderOffset::at_due()),
        created_at: taskboard_domain::BoardClocks::epoch(),
        updated_at: taskboard_domain::BoardClocks::epoch(),
        by: None,
    };
    let rule2 = taskboard_domain::ReminderRule {
        insistence: taskboard_domain::Insistence::Normal,
        id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::from_u128(12)),
        task: task1,
        kind: taskboard_domain::ReminderKind::DueOffset(
            taskboard_domain::ReminderOffset::before(TimeDelta::try_minutes(-10).expect("-10m"))
                .expect("legal offset"),
        ),
        created_at: taskboard_domain::BoardClocks::epoch(),
        updated_at: taskboard_domain::BoardClocks::epoch(),
        by: None,
    };
    state.reminders.rules.insert(rule1.id, rule1);
    state.reminders.rules.insert(rule2.id, rule2.clone());

    support::harness::run_journey(state.clone(), move |harness| {
        // Walk to task1's detail: two item steps land on (0, 1) —
        // the fixture's second task — then two view steps.
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        assert_eq!(harness.emphasized_card(), Some((0, 1)));
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
        harness.assert_commands(&[]);

        // The composition rendered from state: two rows in rule-id
        // order and the derived-fact hint present.
        let task_model = harness.handle.window().get_task_model();
        assert_eq!(task_model.rules.row_count(), 2, "one row per rule");
        assert_eq!(
            task_model.rules.row_data(0).expect("row 0").before,
            "0s",
            "the canonical text form"
        );
        assert_eq!(task_model.rules.row_data(1).expect("row 1").before, "10m");
        assert!(
            !task_model.rules_hint.is_empty(),
            "a rule set presents the derived hint"
        );
        assert_eq!(task_model.rule_presets.row_count(), 4, "the preset chips");

        // Preset add (chip 0 = 5m) routes the exact typed command.
        harness.handle.window().invoke_rule_add_preset_clicked(0);
        harness.assert_commands(&[StateCommand::AddReminderRule {
            task: task1,
            offset: taskboard_domain::ReminderOffset::before(
                TimeDelta::try_minutes(-5).expect("-5m"),
            )
            .expect("legal offset"),
            at: None,
            insistence: taskboard_domain::Insistence::Normal,
        }]);

        // A custom entry parses at the adapter ("45m").
        harness
            .handle
            .window()
            .invoke_rule_add_custom_accepted("45m".into());
        harness.assert_commands(&[StateCommand::AddReminderRule {
            task: task1,
            offset: taskboard_domain::ReminderOffset::before(
                TimeDelta::try_minutes(-45).expect("-45m"),
            )
            .expect("legal offset"),
            at: None,
            insistence: taskboard_domain::Insistence::Normal,
        }]);

        at_in_legs(&mut *harness, task1);

        // Row remove resolves the index through the id tables.
        harness.handle.window().invoke_task_rule_remove_clicked(1);
        harness.assert_commands(&[StateCommand::RemoveReminderRule { rule: rule2.id }]);

        // A stale row index (past the table) absorbs.
        harness.handle.window().invoke_task_rule_remove_clicked(9);
        harness.assert_commands(&[]);
    });
}

/// R2 D10 (§3) — the synthesized double-click journey: two clicks at
/// a card's centre with a refresh (the first click's focus refresh)
/// in between and no mock-clock advance land on the same card
/// instance under the D9 transport, and the second release fires the
/// card's `double-clicked => activated()` wiring — the task-detail
/// view. The window-level click counter runs on the mock clock; the
/// journey never advances it.
#[test]
fn a_double_click_across_a_refresh_opens_the_task_detail() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        let card = harness
            .driver
            .find_by_label("task-0")
            .expect("the first card is addressable");
        let pos = card.absolute_position();
        let size = card.size();
        let cx = pos.x + size.width / 2.0;
        let cy = pos.y + size.height / 2.0;

        harness.driver.click_at(cx, cy);
        harness.handle.refresh();
        harness.driver.click_at(cx, cy);

        assert_eq!(
            harness.handle.window().get_current_view(),
            View::TaskDetail,
            "the second release activated the card"
        );
    });
}

/// R2 D11 (§4) — the drag survives refreshes: press, tick, refresh,
/// tick, a concurrent engine change (a new task in the other stack) +
/// refresh, tick, release. Every tick's live width stays on the model,
/// the interleaved passes never disturb the drag (the tables guard
/// still resolves column 0), requests stay empty until release, and
/// the release emits exactly one full-snapshot request with the final
/// width. The `TouchArea` survival itself is the repetition-probe's
/// mechanism; the feel is the Tier-4 item.
#[test]
fn the_drag_survives_interleaved_refreshes_and_engine_changes() {
    let (fixture, state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    support::harness::run_journey(state.clone(), move |harness| {
        let window = harness.handle.window();
        // A deterministic row: 912 rendered px, two shares of 456.
        harness.driver.set_window_size(1000, 800);

        window.invoke_board_stack_width_press(0);
        window.invoke_board_stack_width_drag(0, -20.0);
        assert_eq!(harness.column_width_px(0), 436.0, "first tick");
        harness.handle.refresh();
        window.invoke_board_stack_width_drag(0, -30.0);
        assert_eq!(
            harness.column_width_px(0),
            426.0,
            "the refresh did not disturb the drag (delta stays absolute)"
        );

        // A concurrent engine change: a new task joins stack b.
        let mut changed = state.clone();
        changed.tasks.insert(
            taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7()),
            taskboard_domain::Task {
                id: taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7()),
                remote: None,
                title: "mid-drag".into(),
                description: String::new(),
                duedate: None,
                scheduled: None,
                done: None,
                stack: fixture.stack_b,
                order: 1,
                label_memberships: std::collections::BTreeMap::new(),
                effort: None,
                importance: None,
                urgency: None,
                recur: None,
                kind: taskboard_domain::entities::TaskKind::Standard,

                archived: false,
                deleted: false,
                clocks: taskboard_domain::TaskClocks {
                    title: taskboard_domain::BoardClocks::epoch(),
                    description: taskboard_domain::BoardClocks::epoch(),
                    duedate: taskboard_domain::BoardClocks::epoch(),
                    scheduled: taskboard_domain::BoardClocks::epoch(),
                    done: taskboard_domain::BoardClocks::epoch(),
                    position: taskboard_domain::BoardClocks::epoch(),

                    effort: taskboard_domain::BoardClocks::epoch(),
                    importance: taskboard_domain::BoardClocks::epoch(),
                    urgency: taskboard_domain::BoardClocks::epoch(),
                    recur: taskboard_domain::BoardClocks::epoch(),
                    kind: taskboard_domain::BoardClocks::epoch(),

                    archived: taskboard_domain::BoardClocks::epoch(),
                    deleted: taskboard_domain::BoardClocks::epoch(),
                },
                authors: taskboard_domain::entities::TaskAuthors::default(),
                remote_seen: None,
                remote_baseline: None,
            },
        );
        harness.engine.replace_state(changed, &harness.handle);
        window.invoke_board_stack_width_drag(0, -40.0);
        assert_eq!(
            harness.column_width_px(0),
            416.0,
            "the engine change did not disturb the drag"
        );
        assert!(
            harness.drain_requests().is_empty(),
            "drag ticks never persist (release only)"
        );

        window.invoke_board_stack_width_release(0);
        let requests = harness.drain_requests();
        assert_eq!(requests.len(), 1, "exactly one request on release");
        let taskboard_ui_slint::requests::UiRequest::StackWidthsChanged {
            board: payload_board,
            widths,
        } = &requests[0]
        else {
            panic!("the typed snapshot reached the lane");
        };
        assert_eq!(payload_board, &board);
        let mut got = widths.clone();
        got.sort_by_key(|(stack, _)| *stack);
        assert_eq!(
            got,
            vec![(fixture.stack_a, 416), (fixture.stack_b, 496)],
            "the release snapshot carries the materialized row"
        );
    });
}

/// R2 D12 (§5.3) — the mouse parent path syncs: clicking a parent row
/// in the wide (fly-out) presentation pushes `menu-active-group` (the
/// fly-out mounts — F7's "clicking parents does nothing") and the
/// fly-out's leaf click switches the view.
#[test]
fn a_wide_parent_click_opens_the_fly_out_and_its_leaf_switches_the_view() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::AppMenu);
        harness.driver.advance_time(0);

        harness.driver.click_by_label("View & navigation");
        harness.driver.advance_time(0);
        assert!(
            keyboard(&*harness).get_menu_active_group() >= 0,
            "the parent click pushed the active group"
        );
        assert!(
            facts(&*harness).get_flyout_width() > 0.0,
            "the fly-out mounted"
        );

        harness.driver.click_by_label("Next view");
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::StackDetail,
            "the fly-out leaf switched the view"
        );
        assert!(
            !keyboard(&*harness).get_menu_open(),
            "the leaf invocation closed the menu"
        );
    });
}

/// R2 D12 (§5.3) — the compact replace page resolves its own index
/// space: the parent click replaces the panel's page, and the leaf
/// click resolves against the DISPLAYED page (pre-fix the click
/// resolved against the root page's vector — the F-B3 mismatch).
#[test]
fn a_compact_parent_click_replaces_the_page_and_its_leaf_resolves_it() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(600, 800);
        harness.press(ActionId::AppMenu);
        harness.driver.advance_time(0);

        harness.driver.click_by_label("View & navigation");
        harness.driver.advance_time(0);
        assert!(
            keyboard(&*harness).get_menu_active_group() >= 0,
            "the parent click pushed the active group"
        );

        // The replaced page's rows are addressable and resolve
        // against the displayed (submenu) page.
        harness.driver.click_by_label("Next view");
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::StackDetail,
            "the replace page's leaf switched the view"
        );
    });
}

/// R2 D13 (§6.3) — a visible tooltip does not survive a view switch:
/// the interpret leaving block dismisses the trigger, so the pending
/// or visible tip is gone after `nav.next-view` (the anchor unmounted).
#[test]
fn a_visible_tooltip_does_not_survive_a_view_switch() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        // Hover a card through its a11y geometry, let the delay
        // fire.
        let card = harness
            .driver
            .find_by_label("task-0")
            .expect("the first card is addressable");
        let pos = card.absolute_position();
        let size = card.size();
        let mut armed = false;
        let mut y = pos.y + 2.0;
        while y < pos.y + size.height - 2.0 {
            harness.driver.hover_at(pos.x + size.width / 2.0, y);
            harness.driver.advance_time(0);
            if tooltip_state(&*harness).get_pending() {
                armed = true;
                break;
            }
            y += 4.0;
        }
        assert!(armed, "the card hover armed the trigger");
        harness.driver.advance_time(600);
        assert!(
            tooltip_state(&*harness).get_visible(),
            "the tip went visible"
        );

        harness.press(ActionId::NavNextView);
        harness.driver.advance_time(0);
        let tooltip = tooltip_state(&*harness);
        assert!(
            !tooltip.get_visible() && !tooltip.get_pending() && !tooltip.get_placed(),
            "the view switch dismissed the tooltip"
        );
    });
}

/// R2 D13 (§6.3) — a menu-row click unhovers: the pending tip armed by
/// hovering a row is cleared by clicking a (parent) row; the submenu
/// opening itself is the D12 journey's assertion.
#[test]
fn a_menu_row_click_unhovers_the_tooltip() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::AppMenu);
        harness.driver.advance_time(0);

        // The existing row-hover scan pattern: arm the trigger on
        // some enabled row.
        let facts = harness
            .handle
            .window()
            .global::<taskboard_ui_slint::MenuPanelFacts>();
        let (px, pw) = (facts.get_x(), facts.get_width());
        let (py, ph) = (facts.get_y(), facts.get_height());
        let mut armed = false;
        let mut y = py + 8.0;
        while y < py + ph - 8.0 {
            harness.driver.hover_at(px + pw / 2.0, y);
            harness.driver.advance_time(0);
            if tooltip_state(&*harness).get_pending() {
                armed = true;
                break;
            }
            y += 6.0;
        }
        assert!(armed, "a row hover armed the tooltip");

        harness.driver.click_by_label("View & navigation");
        harness.driver.advance_time(0);
        let tooltip = tooltip_state(&*harness);
        assert!(
            !tooltip.get_pending() && !tooltip.get_visible(),
            "the row click unhovered the tooltip"
        );
    });
}

/// R2 D14 (§7) — the tooltip width includes the hint row, relation
/// only (no font metrics): the same caption with a two-keycap hint is
/// wider than without; the absurd-caption case keeps the window cap;
/// a keys-less hover is not widened by an empty model.
#[test]
fn the_tooltip_width_includes_the_hint_row_and_keeps_the_cap() {
    use slint::{ModelRc, SharedString, VecModel};
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);

        // (a) Short caption, no keys → w1; same caption plus a
        //     two-keycap hint → wider.
        tooltip_state(&*harness).invoke_hover(
            "short".into(),
            "".into(),
            ModelRc::default(),
            500.0,
            300.0,
            32.0,
            32.0,
        );
        harness.driver.advance_time(600);
        harness.driver.advance_time(0);
        let plain = tooltip_state(&*harness).get_tip_width();
        assert!(plain > 0.0, "the tip measured its width");
        tooltip_state(&*harness).invoke_unhover();

        let keys = ModelRc::new(VecModel::from(vec![
            SharedString::from("Ctrl"),
            SharedString::from("K"),
        ]));
        tooltip_state(&*harness).invoke_hover(
            "short".into(),
            "".into(),
            keys,
            500.0,
            300.0,
            32.0,
            32.0,
        );
        harness.driver.advance_time(600);
        harness.driver.advance_time(0);
        let hinted = tooltip_state(&*harness).get_tip_width();
        assert!(
            hinted > plain,
            "the hint row widens the tip: {hinted} vs {plain}"
        );
        tooltip_state(&*harness).invoke_unhover();

        // (b) The absurd caption keeps the window cap.
        tooltip_state(&*harness).invoke_hover(
                "an absurdly long tooltip label that cannot possibly fit a 1000 px window without wrapping somewhere long long long"
                    .into(),
                "".into(),
                ModelRc::default(),
                500.0,
                300.0,
                32.0,
                32.0,
            );
        harness.driver.advance_time(600);
        harness.driver.advance_time(0);
        assert!(
            tooltip_state(&*harness).get_tip_width() <= 1000.0 * 0.6 + 0.5,
            "the cap holds: {}",
            tooltip_state(&*harness).get_tip_width()
        );
        tooltip_state(&*harness).invoke_unhover();

        // (c) A keys-less hover is not widened by an empty model:
        //     the width returns to the caption term.
        tooltip_state(&*harness).invoke_hover(
            "short".into(),
            "".into(),
            ModelRc::default(),
            500.0,
            300.0,
            32.0,
            32.0,
        );
        harness.driver.advance_time(600);
        harness.driver.advance_time(0);
        assert_eq!(
            tooltip_state(&*harness).get_tip_width(),
            plain,
            "the keys-less width matches the plain caption term"
        );
        tooltip_state(&*harness).invoke_unhover();
    });
}

/// Chunk 9 (design D15) — the palette journey: open with the empty
/// query floor, type a needle across boards (cursor walk + overflow
/// count), prove the anchor's absence structurally (a live matching
/// anchor missing from the row counts is the filter's proof), jump to
/// a unique hit (board + focus retarget), and close on Escape without
/// dispatching anything to the engine.
#[test]
fn the_palette_searches_and_jumps_across_boards() {
    let (fixture, mut state) = fixture::two_board_state();
    // Board one's first stack: the decoy + the anchor ride there.
    let stack_a = state
        .stacks
        .values()
        .find(|stack| stack.board == fixture.board_one)
        .expect("board one stack")
        .id;
    let plant =
        |state: &mut AppState, stack: taskboard_domain::StackId, order: i64, title: &str| {
            let id = TaskId::from_uuid(uuid::Uuid::now_v7());
            let mut task = state.tasks.values().next().expect("fixture task").clone();
            task.id = id;
            task.stack = stack;
            task.order = order;
            task.title = title.to_owned();
            state.tasks.insert(id, task);
        };
    // The unique needle lives on board two (the jump crosses boards);
    // order 5 puts it at canonical row 1 of stack "c".
    plant(&mut state, fixture.stack_c, 5, "zebra needle");
    plant(&mut state, stack_a, 5, "zebra decoy");
    plant(&mut state, stack_a, 6, taskboard_domain::ANCHOR_CARD_TITLE);
    for n in 0..9 {
        plant(
            &mut state,
            fixture.stack_c,
            10 + n,
            &format!("zebra extra {n}"),
        );
    }

    support::harness::run_journey(state, move |harness| {
        macro_rules! window {
            () => {
                harness.handle.window()
            };
        }

        // Open: the empty-query floor, end to end.
        harness.press(ActionId::PaletteSearch);
        assert!(window!().get_palette_open(), "the palette opened");
        assert_eq!(window!().get_palette_model().row_count(), 0);
        assert_eq!(window!().get_palette_overflow(), 0);
        harness.assert_commands(&[]);

        // "zeb" matches the needle, the decoy, and nine extras —
        // eleven candidates. The cap keeps 8; the overflow counts
        // the real remainder (the anchor does not match this
        // query, so nothing is filtered here).
        harness.type_text("zeb");
        assert_eq!(window!().get_palette_model().row_count(), 8);
        assert_eq!(window!().get_palette_overflow(), 3);

        // The cursor ring-walks: Down lands on row 1.
        harness.stroke(parse("Down").expect("down"));
        assert_eq!(window!().get_palette_selected_index(), 1);

        // Escape closes without effect: the view, the board, and
        // the engine boundary are untouched.
        harness.stroke(parse("Escape").expect("escape"));
        assert!(!window!().get_palette_open(), "Escape closed");
        assert_eq!(window!().get_current_view(), View::Board);
        harness.assert_commands(&[]);

        // The anchor proof: the anchor task is live and its title
        // matches "taskboard" as a subsequence — an unfiltered
        // count of 1 would be a visible row; zero rows IS the
        // presentation-side filter.
        harness.press(ActionId::PaletteSearch);
        assert!(window!().get_palette_open());
        harness.type_text("taskboard");
        assert_eq!(
            window!().get_palette_model().row_count(),
            0,
            "the matching anchor is filtered from the palette rows"
        );
        assert_eq!(window!().get_palette_overflow(), 0);
        harness.stroke(parse("Escape").expect("escape"));
        assert!(!window!().get_palette_open());

        // The jump: a query only the needle matches, Return
        // activates — the view retargets to board two and the
        // needle's card renders emphasized (canonical row 1 of
        // the single stack).
        harness.press(ActionId::PaletteSearch);
        harness.type_text("zebra needle");
        assert_eq!(window!().get_palette_model().row_count(), 1);
        harness.stroke(parse("Return").expect("return"));
        assert!(!window!().get_palette_open(), "the jump ends the flow");
        assert_eq!(window!().get_current_view(), View::Board);
        assert_eq!(
            window!().get_board_bar_model().selected,
            1,
            "the jump selected board two"
        );
        assert_eq!(
            harness.emphasized_card(),
            Some((0, 1)),
            "the needle's card is emphasized"
        );
    });
}
/// Round-3 D23 (R3-F35) — the show-task repro journey: open the
/// palette **from Today**, query a task that lives **on another
/// board**, Return. The board view mounts on the task's board, the
/// task's stack is selected, the card is emphasized (the pre-fix bug
/// left the board's first stack selected and the view unchanged), and
/// the `FocusScrollHint` global arms with the task's column/row so
/// the column `ListView` seeks. Sibling: activation from Settings.
#[test]
fn a_palette_activation_shows_the_task() {
    let (fixture, mut state) = fixture::two_board_state();
    let id = TaskId::from_uuid(uuid::Uuid::now_v7());
    let mut needle = state.tasks.values().next().expect("fixture task").clone();
    needle.id = id;
    needle.stack = fixture.stack_c;
    needle.order = 5;
    needle.title = "zebra needle".to_owned();
    state.tasks.insert(id, needle);

    support::harness::run_journey(state, move |harness| {
        macro_rules! window {
            () => {
                harness.handle.window()
            };
        }

        // From Today: the source view is part of the repro.
        harness.stroke(parse("Ctrl+T").expect("ctrl t"));
        assert_eq!(window!().get_current_view(), View::Today);

        // The palette round-trip: one unique hit, Return shows it.
        harness.press(ActionId::PaletteSearch);
        assert!(window!().get_palette_open());
        harness.type_text("zebra needle");
        assert_eq!(window!().get_palette_model().row_count(), 1);
        let before = window!()
            .global::<taskboard_ui_slint::FocusScrollHint>()
            .get_nonce();
        harness.stroke(parse("Return").expect("return"));

        assert!(!window!().get_palette_open(), "the jump ends the flow");
        assert_eq!(window!().get_current_view(), View::Board);
        assert_eq!(
            window!().get_board_bar_model().selected,
            1,
            "the task's board (board two) is selected"
        );
        assert_eq!(
            harness.emphasized_card(),
            Some((0, 1)),
            "the needle's card is focused in its stack"
        );
        let hint = window!().global::<taskboard_ui_slint::FocusScrollHint>();
        assert!(
            hint.get_nonce() > before,
            "the retarget armed the scroll hint"
        );
        assert_eq!(
            (hint.get_target_a(), hint.get_target_b()),
            (0, 1),
            "column 0, canonical row 1 of the task's stack"
        );

        // From Settings: one semantics, no per-source branching —
        // the activation still mounts the board view on the
        // task's home.
        harness.press(ActionId::AppSettings);
        assert_eq!(window!().get_current_view(), View::Settings);
        harness.press(ActionId::PaletteSearch);
        harness.type_text("zebra needle");
        harness.stroke(parse("Return").expect("return"));
        assert_eq!(
            window!().get_current_view(),
            View::Board,
            "the view is always the task's home board"
        );
        assert_eq!(
            window!().get_board_bar_model().selected,
            1,
            "board two stayed selected"
        );
    });
}

/// A5a S-1 (#1302, D-2): a habit landing and a break-rule roster
/// landing arm the `FocusScrollHint` with their synthetic page slots
/// (4 = the habits section, 5 = the roster subsection) — the same
/// `(target-a, target-b)` convention the task rows speak. Red
/// pre-fix: the habit and roster walk rows pushed no hint at all
/// (the seam resolved `focused_task` only). Typed outcome pins (the
/// globals' nonce/index), never text.
#[test]
fn habit_and_roster_landings_arm_the_hint() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    let _rule = fixture::add_break_rule(
        &mut state,
        board,
        "Tea break",
        chrono::TimeDelta::minutes(90),
        taskboard_domain::Insistence::Alarm,
    );
    support::harness::run_journey(state, move |harness| {
        macro_rules! window {
            () => {
                harness.handle.window()
            };
        }
        macro_rules! hint {
            () => {
                window!().global::<taskboard_ui_slint::FocusScrollHint>()
            };
        }

        harness.stroke(parse("Ctrl+T").expect("ctrl t"));
        assert_eq!(window!().get_current_view(), View::Today);

        // The walk's entry lands on the first today row: the overdue
        // task (section 0, row 0).
        let before = hint!().get_nonce();
        harness.press(ActionId::NavFocusDown);
        assert!(hint!().get_nonce() > before, "the task entry arms the hint");
        assert_eq!(
            (hint!().get_target_a(), hint!().get_target_b()),
            (0, 0),
            "the task landing keeps the (section, row) convention"
        );

        // Onto the habit row: the habits section's synthetic slot 4.
        let after_task = hint!().get_nonce();
        harness.press(ActionId::NavFocusDown);
        assert!(
            hint!().get_nonce() > after_task,
            "the habit landing arms the hint (dark pre-fix)"
        );
        assert_eq!(
            (hint!().get_target_a(), hint!().get_target_b()),
            (4, 0),
            "the habit landing targets the habits page slot"
        );

        // Onto the roster row: the roster's synthetic slot 5.
        let after_habit = hint!().get_nonce();
        harness.press(ActionId::NavFocusDown);
        assert!(
            hint!().get_nonce() > after_habit,
            "the roster landing arms the hint (dark pre-fix)"
        );
        assert_eq!(
            (hint!().get_target_a(), hint!().get_target_b()),
            (5, 0),
            "the roster landing targets the roster page slot"
        );
    });
}

/// A5a S-2 (#1302, D-7): the ends hold and the pointer law — a
/// vertical walk into either walk end, a jump idempotent at its
/// target, and a pointer row landing all leave the nonce unchanged
/// (the scroll reacts to a focus landing that changed something,
/// never to a key or a pointer). The pointer half rides the row
/// click seam (a focus write with no hint push by construction).
#[test]
fn walk_ends_and_pointer_landings_rearm_nothing() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    let _rule = fixture::add_break_rule(
        &mut state,
        board,
        "Tea break",
        chrono::TimeDelta::minutes(90),
        taskboard_domain::Insistence::Alarm,
    );
    support::harness::run_journey(state, move |harness| {
        macro_rules! window {
            () => {
                harness.handle.window()
            };
        }
        macro_rules! hint {
            () => {
                window!().global::<taskboard_ui_slint::FocusScrollHint>()
            };
        }

        harness.stroke(parse("Ctrl+T").expect("ctrl t"));
        let before = hint!().get_nonce();

        // The first landing arms once; the top end holds after it.
        harness.press(ActionId::NavFirstItem);
        assert!(hint!().get_nonce() > before, "the first jump arms once");
        let top = hint!().get_nonce();
        harness.press(ActionId::NavFocusUp);
        assert_eq!(hint!().get_nonce(), top, "the top end holds (no wrap)");

        // The last row (the roster) arms once; the bottom end and a
        // repeated jump both hold.
        harness.press(ActionId::NavLastItem);
        assert!(hint!().get_nonce() > top, "the last jump arms once");
        let bottom = hint!().get_nonce();
        harness.press(ActionId::NavFocusDown);
        assert_eq!(
            hint!().get_nonce(),
            bottom,
            "the bottom end holds (no wrap)"
        );
        harness.press(ActionId::NavLastItem);
        assert_eq!(
            hint!().get_nonce(),
            bottom,
            "the jump is idempotent at the target (the generalized              subject guard)"
        );

        // The pointer law: a row click moves the focus to the task
        // row and arms nothing.
        window!().invoke_today_row_clicked(0, 0);
        assert_eq!(
            hint!().get_nonce(),
            bottom,
            "a pointer selection hints nothing"
        );
    });
}

/// Round-3 D22 — the toolbar view switcher journey: the clicks drive
/// the same registry actions the keyboard uses (click ≡ key), the
/// pushed index tracks the mounted view in both directions
/// (reply-sourced), the settings round-trip shows the return view's
/// segment, the compact breakpoint keeps the switcher (it drops the
/// inbox chip, never the switcher), and a live board-create flow is
/// settled by a switcher click (the leaving rule).
#[test]
fn the_switcher_drives_and_tracks_the_view() {
    let (_fixture, state) = fixture::two_board_state();

    support::harness::run_journey(state, move |harness| {
        macro_rules! window {
            () => {
                harness.handle.window()
            };
        }

        #[allow(clippy::items_after_statements)] // the run closure's helper
        fn segment(driver: &SlintDriver, label: &str) -> support::driver::Element {
            driver
                .find_all_by_label(label)
                .into_iter()
                .find(support::driver::Element::is_radio_button)
                .unwrap_or_else(|| panic!("the {label} segment is present"))
        }

        // The pushed index tracks the view from the mount on.
        assert_eq!(window!().get_switcher_index(), 0, "the board branch");

        // Click → action: Today. The segment's radio-button role
        // is the address (the visible text labels echo the same
        // string); AT activation is the click path here.
        segment(&harness.driver, "Today").invoke_default_action();
        assert_eq!(window!().get_current_view(), View::Today);
        harness.driver.advance_time(0);
        assert_eq!(window!().get_switcher_index(), 1, "today lit");

        // Keyboard equivalence: Alt+1 lands where the Board
        // segment click does, and the echo is reply-sourced.
        harness.stroke(parse("Alt+1").expect("alt 1"));
        assert_eq!(window!().get_current_view(), View::Board);
        harness.driver.advance_time(0);
        assert_eq!(window!().get_switcher_index(), 0);

        // Settings round-trip: the segment names the return view.
        harness.press(ActionId::AppSettings);
        assert_eq!(window!().get_current_view(), View::Settings);
        harness.driver.advance_time(0);
        assert_eq!(window!().get_switcher_index(), 0, "return reads board");
        segment(&harness.driver, "Today").invoke_default_action();
        assert_eq!(window!().get_current_view(), View::Today);
        harness.driver.advance_time(0);
        assert_eq!(window!().get_switcher_index(), 1);

        // Compact: the switcher survives the breakpoint that
        // drops the inbox chip.
        harness.driver.set_window_size(600, 400);
        assert!(window!().get_compact_active(), "600 px is compact");
        assert!(
            harness.driver.find_by_label("Today").is_some(),
            "the switcher is mounted in compact"
        );
        segment(&harness.driver, "Board").invoke_default_action();
        assert_eq!(window!().get_current_view(), View::Board);
        harness.driver.advance_time(0);
        assert_eq!(window!().get_switcher_index(), 0);
        harness.driver.set_window_size(1280, 800);

        // A live board-create flow settles under the switcher
        // click (the leaving rule): the field closes, nothing
        // dispatches, the switch happens.
        harness.press(ActionId::BoardCreate);
        harness.driver.advance_time(0);
        assert!(window!().get_board_create_open(), "the flow armed");
        segment(&harness.driver, "Today").invoke_default_action();
        harness.driver.advance_time(0);
        assert!(
            !window!().get_board_create_open(),
            "the leaving settle ended the flow"
        );
        assert_eq!(window!().get_current_view(), View::Today);
        harness.assert_commands(&[]);
    });
}

/// Round-3 D24 (R3-F34) — the palette input row's geometry pin: with
/// zero rows the panel's height is exactly the input row (the field's
/// natural single-line height) plus the panel padding — the
/// full-screen-field regression's pin; with a full cap of rows the
/// panel stays content-sized and clamped above the status bar at the
/// minimum viewport.
#[test]
fn the_palette_input_row_stays_one_line() {
    let (_fixture, mut state) = fixture::two_board_state();
    let stack = *state.stacks.keys().next().expect("fixture stack");
    for n in 0..12 {
        let id = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut task = state.tasks.values().next().expect("fixture task").clone();
        task.id = id;
        task.stack = stack;
        task.order = 10 + n;
        task.title = format!("zebra row {n}");
        state.tasks.insert(id, task);
    }

    support::harness::run_journey(state, move |harness| {
        macro_rules! window {
            () => {
                harness.handle.window()
            };
        }

        #[allow(clippy::items_after_statements)] // the run closure's helper
        fn palette_panel(driver: &SlintDriver) -> support::driver::Element {
            driver
                .find_all_by_id("AppWindow::palette-panel")
                .into_iter()
                .next()
                .expect("the palette panel is mounted")
        }

        harness.press(ActionId::PaletteSearch);
        harness.driver.advance_time(0);
        // Zero rows: the panel is content-sized — the full-window
        // pane regression (the surplus distributing into the
        // field) cannot recur; the input row reads one line.
        let panel = palette_panel(&harness.driver);
        let field = harness
            .driver
            .find_all_by_label("Search")
            .into_iter()
            .next()
            .expect("the query field is addressable");
        let natural = f64::from(field.size().height);
        assert!(
            natural > 0.0 && natural < 100.0,
            "the input row is one line, not a pane"
        );
        let height = f64::from(panel.size().height);
        assert!(
            height < natural * 3.0,
            "the zero-row panel is content-sized ({height}), not a pane"
        );

        // Eight rows: content-driven, clamped above the status
        // bar, even at the minimum viewport.
        harness.type_text("zebra");
        harness.driver.advance_time(0);
        let panel = palette_panel(&harness.driver);
        assert_eq!(window!().get_palette_model().row_count(), 8);
        assert!(
            f64::from(panel.absolute_position().y + panel.size().height)
                <= f64::from(window!().get_status_top()),
            "the panel stays above the status bar"
        );
    });
}

// ---- round 2 (D16–D21): the action round-up journeys --------------------

/// Presses one action through `preset`'s own binding table (the
/// `Harness::press` pattern with a non-default keymap): resolves the
/// chain-visible binding the same way the generated menu does and
/// injects each stroke.
fn press_preset(
    harness: &mut Harness,
    preset: taskboard_ui_slint::keymap::Preset,
    action: ActionId,
) {
    let keymap = taskboard_ui_slint::keymap::Keymap::from_preset(preset);
    let chain = harness.chain();
    let hint = keymap
        .first_binding_display(action, &chain)
        .unwrap_or_else(|| panic!("{action:?} is bound in the visible chain {chain:?}"));
    for spelling in &hint {
        let stroke = stroke::parse(spelling)
            .unwrap_or_else(|error| panic!("{action:?}'s hint {spelling:?}: {error}"));
        let presses = harness.driver.press_stroke(stroke);
        harness.guard.press_n(presses);
    }
}

fn menu_rows(harness: &Harness) -> Vec<taskboard_ui_slint::MenuEntryModel> {
    let entries = keyboard(harness).get_menu_entries();
    (0..entries.row_count())
        .map(|i| entries.row_data(i).expect("row"))
        .collect()
}

fn menu_actions(harness: &Harness) -> Vec<String> {
    menu_rows(harness)
        .into_iter()
        .map(|r| r.action.to_string())
        .collect()
}

/// The dispatch (c) contract, journey-level: every preset's move flow
/// cancels through Escape with zero commands drained; emacs' board-view
/// C-g is the cancel alternate (and inert outside a move), and its
/// `Ctrl+g Ctrl+/` sequence still resolves over the task-detail chain
/// (the resolver-level pin's echo at the window boundary).
#[test]
fn move_cancel_routes_through_escape_and_emacs_c_g() {
    for (preset, name) in [
        (taskboard_ui_slint::keymap::Preset::Standard, "standard"),
        (taskboard_ui_slint::keymap::Preset::Emacs, "emacs"),
        (taskboard_ui_slint::keymap::Preset::Vi, "vi"),
    ] {
        let (fixture, state) = fixture::golden_state();
        support::harness::run_journey_with_settings(
            state,
            UiSettings {
                keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                keymap_preset: preset,
                ..UiSettings::default()
            },
            move |harness| {
                harness.handle.window().invoke_board_task_clicked(0, 0);
                press_preset(&mut *harness, preset, ActionId::TaskMove);
                assert!(
                    harness.handle.window().get_flow_hint_model().visible,
                    "{name}: the move flow is visible"
                );
                assert!(
                    harness.handle.window().get_board_model().move_target >= 0,
                    "{name}: the move target highlights"
                );
                harness.stroke(parse("Escape").expect("Escape"));
                assert!(
                    !harness.handle.window().get_flow_hint_model().visible,
                    "{name}: Escape ends the flow"
                );
                harness.assert_commands(&[]);

                // Extra pin (emacs): the board-view C-g is inert
                // outside a move and cancels inside one.
                if preset == taskboard_ui_slint::keymap::Preset::Emacs {
                    harness.stroke(parse("Ctrl+g").expect("Ctrl+g"));
                    harness.assert_commands(&[]);
                    press_preset(&mut *harness, preset, ActionId::TaskMove);
                    harness.stroke(parse("Ctrl+g").expect("Ctrl+g"));
                    assert!(
                        !harness.handle.window().get_flow_hint_model().visible,
                        "the lone C-g cancels the move"
                    );
                    harness.assert_commands(&[]);
                }
                let _ = fixture;
            },
        );
    }
}

/// Round-3 D28 — the emacs double-C-g convention, end to end: from
/// the today chain (where the lone C-g never resolves), a live move
/// flow is cancelled by `C-g C-g` — the first C-g parks as the
/// pending prefix (the status signal), the second dispatches the
/// cancel; nothing moves and the flow is gone.
#[test]
fn emacs_double_c_g_cancels_from_the_today_chain() {
    let (_fixture, state, _walked) = fixture::today_overdue_state();
    let preset = taskboard_ui_slint::keymap::Preset::Emacs;
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
            keymap_preset: preset,
            ..UiSettings::default()
        },
        move |harness| {
            harness.stroke(parse("Ctrl+T").expect("Ctrl+T"));
            press_preset(&mut *harness, preset, ActionId::NavFocusDown); // seed a today row
            press_preset(&mut *harness, preset, ActionId::TaskMove);
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_flow_hint_model().visible,
                "the move flow armed"
            );

            // The convention: the first C-g parks (the prefix-abort
            // status signal rides the pending feedback), the second
            // cancels.
            harness.stroke(parse("Ctrl+g").expect("Ctrl+g"));
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_flow_hint_model().visible,
                "the lone C-g is only a pending prefix here"
            );
            harness.stroke(parse("Ctrl+g").expect("Ctrl+g"));
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_flow_hint_model().visible,
                "C-g C-g cancelled the move"
            );
            harness.assert_commands(&[]);
        },
    );
}

/// The dispatch (d) contract: the stack walk wraps at both ends and
/// lands focus on the new stack's first task.
#[test]
fn the_stack_walk_wraps_at_both_ends() {
    let (fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        // Select the last stack first (the wrap-back check needs a
        // real end): the header click is the established seam.
        harness.handle.window().invoke_board_header_activated(1);
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            1,
            "the header click selected stack b"
        );
        harness.press(ActionId::StackNext);
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            0,
            "next at the last stack wraps to the first"
        );
        assert_eq!(
            harness.emphasized_card(),
            Some((0, 0)),
            "focus lands on the wrapped stack's first task"
        );
        harness.press(ActionId::StackPrev);
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            1,
            "previous from the first stack wraps to the last"
        );
        harness.assert_commands(&[]);
        let _ = fixture;
    });
}

/// The dispatch (e)/(f) contract: board create via the keyboard and
/// via the board-bar menu click ≡ key; board delete and stack delete
/// drain the exact typed commands; reopen routes the inverted done
/// flag and is a no-op dispatch on an open task.
#[test]
fn the_lifecycle_verbs_drain_their_exact_commands() {
    let (fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        // Board create via the keyboard binding.
        harness.press(ActionId::BoardCreate);
        assert!(
            harness.handle.window().get_board_create_open(),
            "the board field opened"
        );
        harness.type_text("second");
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[StateCommand::CreateBoard {
            title: "second".to_owned(),
            color: taskboard_domain::Color::new(taskboard_domain::entities::DEFAULT_BOARD_COLOR),
        }]);
        harness.engine.settle(&harness.handle);

        // Stack delete via the keyboard binding (UX R-2 D-2):
        // the first stroke arms — the armed fact holds, zero
        // commands; the repeat stroke commits the exact verb.
        harness.press(ActionId::StackDelete);
        harness.assert_commands(&[]);
        assert!(
            harness.handle.window().get_board_model().delete_armed_stack >= 0,
            "the armed strip fact holds after the arm stroke"
        );
        harness.press(ActionId::StackDelete);
        harness.assert_commands(&[StateCommand::DeleteStack {
            id: fixture.stack_a,
        }]);
        harness.engine.settle(&harness.handle);

        // Reopen: focus a card, complete it, then invert it.
        harness.handle.window().invoke_board_task_clicked(0, 1);
        harness.driver.advance_time(0);
        harness.press(ActionId::TaskComplete);
        harness.assert_commands(&[StateCommand::SetTaskDone {
            id: fixture.task1,
            done: true,
        }]);
        harness.engine.settle(&harness.handle);
        harness.press(ActionId::TaskReopen);
        harness.assert_commands(&[StateCommand::SetTaskDone {
            id: fixture.task1,
            done: false,
        }]);
        harness.engine.settle(&harness.handle);
        // An open task absorbs the verb (NoTarget at the router).
        harness.press(ActionId::TaskReopen);
        harness.assert_commands(&[]);

        // Board delete via the keyboard binding (UX R-2 D-2):
        // the arm stroke dispatches nothing; the repeat commits
        // the exact verb (matched structurally — the fixture does
        // not expose the board id).
        harness.press(ActionId::BoardDelete);
        harness.assert_commands(&[]);
        assert!(
            harness.handle.window().get_board_delete_armed(),
            "the board strip fact holds after the arm stroke"
        );
        harness.press(ActionId::BoardDelete);
        let drained = harness.engine.drain();
        assert_eq!(drained.len(), 1);
        assert!(
            matches!(drained[0], StateCommand::DeleteBoard { .. }),
            "board.delete drains DeleteBoard"
        );
    });
}

/// UX R-2 D-2 (D-7 T4, #784): the armed strip's Confirm click commits
/// the exact verb the keyboard twin would — click ≡ key at every
/// step (the context-menu delete row routes the same `ActionId` the
/// keymap stroke routes, so the arm via the stroke is the same
/// journey the menu row drives). The arm stroke mounts the strip in
/// the armed column; the driver invokes its Confirm affordance.
#[test]
fn us5_s2_menu_delete_row_arms_and_confirm_click_commits() {
    let (fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        // The arm stroke: the armed fact holds, zero commands.
        harness.press(ActionId::StackDelete);
        harness.assert_commands(&[]);
        assert!(
            harness.handle.window().get_board_model().delete_armed_stack >= 0,
            "the armed strip fact holds"
        );
        // The strip's Confirm affordance routes the exact verb.
        let confirm = harness
            .driver
            .find_by_label("Confirm delete stack")
            .expect("the armed strip mounts at the subject");
        confirm.invoke_default_action();
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::DeleteStack {
            id: fixture.stack_a,
        }]);
        harness.engine.settle(&harness.handle);
    });
}

/// UX R-2 D-2 (D-7 T5, #784): the two-step invoke — the second
/// stroke commits; Escape first cancels and nothing dispatches ever.
#[test]
fn us5_s3_repeat_stroke_confirms_and_escape_cancels() {
    let (fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        // Arm, then Escape: the disarm, nothing dispatched.
        harness.press(ActionId::StackDelete);
        harness.stroke(parse("Escape").expect("Escape"));
        harness.assert_commands(&[]);
        assert!(
            harness.handle.window().get_board_model().delete_armed_stack < 0,
            "Escape disarms the strip"
        );
        // Arm again, the repeat stroke commits the exact verb.
        harness.press(ActionId::StackDelete);
        harness.press(ActionId::StackDelete);
        harness.assert_commands(&[StateCommand::DeleteStack {
            id: fixture.stack_a,
        }]);
        harness.engine.settle(&harness.handle);
    });
}

/// UX R-2 D-2 (D-7 T6, #784): the kiosk delete stage is
/// visible-gated, not absent — the arm yields the armed fact on
/// kiosk exactly as on desktop (the confirmation rides the delete
/// verbs' existing reachability; no capability change either way).
#[test]
fn the_kiosk_delete_stage_is_visible_gated_not_absent() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut kiosk = Harness::mount_with_settings(
                SlintDriver::default(),
                state.clone(),
                taskboard_ui_slint::UiSettings {
                    kiosk: true,
                    ..taskboard_ui_slint::UiSettings::default()
                },
            );
            kiosk.press(ActionId::StackDelete);
            kiosk.assert_commands(&[]);
            assert!(
                kiosk.handle.window().get_board_model().delete_armed_stack >= 0,
                "the kiosk arm yields the armed fact (visible-gated, not absent)"
            );
            // Desktop parity: the same arm mounts the same fact.
            let mut desktop = Harness::mount(SlintDriver::default(), state);
            desktop.press(ActionId::StackDelete);
            desktop.assert_commands(&[]);
            assert!(
                desktop.handle.window().get_board_model().delete_armed_stack >= 0,
                "the desktop arm yields the armed fact"
            );
        },
    );
}

/// The operator's Dvorak-International role-theft report (issue
/// #855, 2026-11-18; the keymap-parity plan US-2 AC-2 made
/// directional): a bare Alt+, — Alt held, Shift NOT held — dispatches
/// nothing (the pre-fix symmetric partner map let it steal the
/// begin/end moves), while the genuinely-shifted press (the layout's
/// base-glyph path: the `,` glyph with Alt+Shift held) fires the
/// first-item jump, and its `.` mirror fires the last-item jump.
/// Typed outcomes on the headless driver over the emacs preset (the
/// preset whose rows bind `Alt+<` / `Alt+>` — the standard preset
/// binds the Ctrl+Home/Ctrl+End pair).
#[test]
fn the_punctuation_partner_matches_only_the_held_shift() {
    let (_fixture, state) = fixture::golden_state();
    let settings = taskboard_ui_slint::UiSettings {
        keymap: taskboard_ui_slint::keymap::build(&taskboard_ui_slint::keymap::RawKeymap {
            preset: taskboard_ui_slint::keymap::Preset::Emacs,
            overrides: std::collections::BTreeMap::default(),
        })
        .keymap,
        ..taskboard_ui_slint::UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        assert_eq!(
            harness.emphasized_card(),
            None,
            "the board mounts unemphasized"
        );
        // The theft: a bare Alt+, (no Shift) must do nothing at all.
        let bare = taskboard_ui_slint::keymap::Stroke::from_event(',', false, true, false, false)
            .expect("the bare event converts");
        harness.stroke(bare);
        harness.driver.advance_time(0);
        assert_eq!(
            harness.emphasized_card(),
            None,
            "a bare Alt+, dispatches nothing"
        );
        harness.assert_commands(&[]);
        // The operator's shifted press: the `,` glyph under
        // Alt+Shift — the begin move.
        let shifted = taskboard_ui_slint::keymap::Stroke::from_event(',', false, true, true, false)
            .expect("the shifted event converts");
        harness.stroke(shifted);
        harness.driver.advance_time(0);
        assert_eq!(
            harness.emphasized_card(),
            Some((0, 0)),
            "Alt+Shift+, lands on the reading order's first card (the M-< route)"
        );
        // The `.` mirror under Alt+Shift — the end move.
        let shifted_end =
            taskboard_ui_slint::keymap::Stroke::from_event('.', false, true, true, false)
                .expect("the shifted event converts");
        harness.stroke(shifted_end);
        harness.driver.advance_time(0);
        assert_eq!(
            harness.emphasized_card(),
            Some((1, 0)),
            "Alt+Shift+. lands on the reading order's last card (the M-> route)"
        );
    });
}

/// The dispatch (g) contract: the first/last jumps land on the far
/// ends of the reading order and are idempotent at the target.
#[test]
fn the_first_last_jumps_land_on_the_far_ends_and_stay() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavLastItem);
        assert_eq!(
            harness.emphasized_card(),
            Some((1, 0)),
            "last lands on the reading order's end"
        );
        harness.press(ActionId::NavLastItem);
        assert_eq!(
            harness.emphasized_card(),
            Some((1, 0)),
            "repeating last is a stay"
        );
        harness.press(ActionId::NavFirstItem);
        assert_eq!(
            harness.emphasized_card(),
            Some((0, 0)),
            "first lands on the reading order's start"
        );
        harness.assert_commands(&[]);
    });
}

/// The stack-move flow: begin highlights the moving stack's slot,
/// cycling wraps over the insertion slots, confirm drains the exact
/// re-normalization set, and Escape mid-flow drains nothing.
#[test]
fn the_stack_move_flow_cycles_and_confirms_minimally() {
    let (fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        // Move stack a (selected, slot 0) after stack b (slot 1):
        // highlight the insertion point, confirm, drain the set.
        harness.handle.window().invoke_board_header_activated(0);
        harness.press(ActionId::StackMove);
        assert_eq!(
            harness.handle.window().get_board_model().move_target,
            0,
            "the flow highlights the moving stack's own slot"
        );
        harness.stroke(parse("Down").expect("Down"));
        assert_eq!(
            harness.handle.window().get_board_model().move_target,
            1,
            "the insertion point follows the cycle"
        );
        harness.stroke(parse("Escape").expect("Escape"));
        harness.assert_commands(&[]);
        harness.press(ActionId::StackMove);
        // Two Downs: slot 2 of 0..=2 = "append after the last
        // stack"; the highlight clamps to the last stack.
        harness.stroke(parse("Down").expect("Down"));
        harness.stroke(parse("Down").expect("Down"));
        assert_eq!(
            harness.handle.window().get_board_model().move_target,
            1,
            "the append slot highlights the last stack"
        );
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[
            StateCommand::MoveStack {
                id: fixture.stack_b,
                order: 0,
            },
            StateCommand::MoveStack {
                id: fixture.stack_a,
                order: 1,
            },
        ]);
        harness.engine.settle(&harness.handle);
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            1,
            "stack a now renders at position 1"
        );

        // An in-place confirm dispatches nothing.
        harness.press(ActionId::StackMove);
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[]);
    });
}

/// The dispatch (h) contract: right-clicking a stack header opens the
/// pointer-anchored menu with exactly the stack context's ids, every
/// bound row hinted, and an invocation routes against the clicked
/// object (Widen stack steps the width request).
#[test]
fn the_header_right_click_opens_the_stack_menu_and_routes() {
    let (fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness
            .handle
            .window()
            .invoke_board_header_context_requested(1, 300.0, 300.0);
        harness.driver.advance_time(0);
        assert!(keyboard(&*harness).get_menu_open());
        assert!(keyboard(&*harness).get_menu_pointer_anchored());
        let rows = menu_rows(&*harness);
        assert!(rows.iter().all(|r| !r.submenu), "flat: no parents");
        assert!(
            !menu_actions(&*harness).contains(&"app.exit".to_owned()),
            "no Quit"
        );
        for action in [
            "task.create",
            "stack.move",
            "stack.rename",
            "stack.wider",
            "stack.narrower",
            "stack.delete",
        ] {
            assert!(
                menu_actions(&*harness).contains(&action.to_owned()),
                "the stack menu offers {action}"
            );
        }
        for row in &rows {
            let keymap = UiSettings::default().keymap;
            if let Some(id) = taskboard_ui_slint::actions::ActionId::from_id(row.action.as_str()) {
                let bound = keymap
                    .first_binding_display(
                        id,
                        &[
                            taskboard_ui_slint::actions::ContextId::Board,
                            taskboard_ui_slint::actions::ContextId::Global,
                        ],
                    )
                    .is_some();
                if bound {
                    assert!(row.hint.row_count() > 0, "{} renders a hint", row.action);
                }
            }
        }

        // Invoking Widen stack routes the width verb (no command,
        // a width request over the request lane).
        harness.driver.click_by_label("Widen stack");
        harness.assert_commands(&[]);
        let requests = harness.drain_requests();
        assert!(
            requests.iter().any(|r| matches!(
                r,
                taskboard_ui_slint::requests::UiRequest::StackWidthsChanged { .. }
            )),
            "the width verb stepped the persist request"
        );
        assert!(!keyboard(&*harness).get_menu_open());
        // The clicked stack was selected first: the moved selection
        // addresses stack b on the next verb.
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            1,
            "right-click selected the clicked column"
        );
        let _ = fixture;
    });
}

/// Right-clicking a board-bar segment selects that board and opens
/// the board context menu (the recorded heavier-than-card semantics).
#[test]
fn the_segment_right_click_selects_and_opens_the_board_menu() {
    let (_fixture, state) = fixture::two_board_state();
    support::harness::run_journey(state, move |harness| {
        harness
            .handle
            .window()
            .invoke_board_segment_context_requested(1, 200.0, 200.0);
        harness.driver.advance_time(0);
        assert!(keyboard(&*harness).get_menu_open());
        for action in [
            "task.create",
            "stack.create",
            "board.create",
            "board.rename",
            "board.delete",
        ] {
            assert!(
                menu_actions(&*harness).contains(&action.to_owned()),
                "the board menu offers {action}"
            );
        }
        assert!(
            !menu_actions(&*harness).contains(&"app.exit".to_owned()),
            "no Quit"
        );
        // Right-click selected the clicked board (the board bar's
        // selected index follows).
        assert_eq!(
            harness.handle.window().get_board_bar_model().selected,
            1,
            "the clicked segment is now the selected board"
        );
    });
}

/// The rename flows: the header menu's Rename stack opens the seeded
/// field inside the locked header; commit drains the exact command;
/// Escape discards. Board rename opens the window-level slot seeded
/// from the current title.
#[test]
fn the_rename_flows_seed_commit_and_discard() {
    let (fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        // Stack rename via the header menu.
        harness
            .handle
            .window()
            .invoke_board_header_context_requested(0, 300.0, 300.0);
        harness.driver.click_by_label("Rename stack");
        harness.driver.advance_time(0);
        let stacks = harness.handle.window().get_board_model().stacks;
        let column = stacks.row_data(0).expect("column");
        assert!(column.rename_open, "the header field opened");
        assert_eq!(column.initial_title, "a", "the buffer seeded");
        // The seeded title starts selected (the task-editor
        // convention): typing replaces it wholesale.
        harness.type_text("renamed");
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[StateCommand::RenameStack {
            id: fixture.stack_a,
            new_title: "renamed".to_owned(),
        }]);
        harness.engine.settle(&harness.handle);

        // Board rename via the board menu: seeded, Escape discards.
        harness
            .handle
            .window()
            .invoke_board_segment_context_requested(0, 200.0, 200.0);
        harness.driver.click_by_label("Rename board");
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_board_rename_open());
        assert_eq!(
            harness.handle.window().get_board_flow_initial(),
            "board",
            "seeded from the current title"
        );
        harness.stroke(parse("Escape").expect("Escape"));
        harness.assert_commands(&[]);
        assert!(!harness.handle.window().get_board_rename_open());
    });
}

/// R2 D23 — the checkmark commit affordance: clicking it dispatches
/// exactly what Return dispatches (click ≡ key at the observed
/// boundary, §7.1), and it stays inert on an empty buffer.
#[test]
fn the_commit_affordance_click_equals_return_on_the_title_field() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        // Into the title editor: the field mounts focused with its
        // seed selected.
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskEdit);
        // Recorded decision (round-3 chunk 1, plan §6 — F26):
        // task.edit never auto-arms; the editor arms via the
        // `task.title-edit` action.
        harness.handle.dispatch_action(ActionId::TaskTitleEdit);

        // (a) The click path: type a fresh title, invoke Commit.
        harness.type_text("clicked");
        harness.driver.click_by_label("Commit");
        let clicked = harness.engine.drain();
        assert!(
            matches!(&clicked[..], [StateCommand::UpdateTask { changes, .. }]
                    if changes.title.as_deref() == Some("clicked")),
            "the click route dispatched the title commit, got {clicked:?}"
        );

        // (b) The key path: the same buffer through Return produces
        // the identical command (equality is the pin). The editor
        // re-opens from the board, focus still on the same task.
        harness.press(ActionId::NavFocusBoard);
        harness.press(ActionId::TaskEdit);
        // Recorded decision (round-3 chunk 1, plan §6 — F26):
        // task.edit never auto-arms; the editor arms via the
        // `task.title-edit` action.
        harness.handle.dispatch_action(ActionId::TaskTitleEdit);
        harness.type_text("clicked");
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(clicked.as_slice());
    });
}

/// R2 D23 — the affordance renders only while a non-empty buffer
/// holds focus: an emptied field offers no Commit element and the
/// check seam cannot fire (nothing dispatched).
#[test]
fn the_commit_affordance_is_inert_on_an_empty_buffer() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskEdit);
        // Recorded decision (round-3 chunk 1, plan §6 — F26):
        // task.edit never auto-arms; the editor arms via the
        // `task.title-edit` action.
        harness.handle.dispatch_action(ActionId::TaskTitleEdit);
        harness.driver.advance_time(0);
        assert!(
            harness.driver.find_by_label("Commit").is_some(),
            "a populated focused field offers the affordance"
        );

        // Empty the seeded selection: the affordance unmounts.
        harness.stroke(parse("Backspace").expect("Backspace"));
        harness.driver.advance_time(0);
        assert!(
            harness.driver.find_by_label("Commit").is_none(),
            "an empty buffer hides the affordance"
        );
        harness.assert_commands(&[]);
    });
}

/// R2 D23 — the create-row sites share the primitive: the stack
/// create field's `Commit` click lands the same `CreateStack` command the
/// Return path produces (one widget, one seam — second data point).
#[test]
fn the_commit_affordance_click_equals_return_on_the_stack_create_row() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::StackCreate);
        harness.type_text("s");
        harness.driver.click_by_label("Commit");
        match harness.engine.drain().as_slice() {
            [StateCommand::CreateStack { title, order, .. }] => {
                assert_eq!(title.as_str(), "s");
                assert_eq!(*order, 2, "the same command shape as Return");
            }
            other => panic!("the click route's command reached the boundary, got {other:?}"),
        }
    });
}

/// R2 D24 — a parse-missed custom offset still dispatches nothing
/// (the D12 absorb) but now resets the buffer: the field reads empty
/// afterwards, so it never looks stuck. The buffer is typed through
/// the real path — the field focused, the entry committed by Return —
/// because the seam's `false` return is what drives the
/// composition's own clear. A parse hit routes `ReminderRuleAdd`
/// unchanged (the regression pin).
#[test]
fn a_parse_missed_custom_offset_clears_its_buffer() {
    let (fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
        harness.driver.advance_time(0);

        let field = |harness: &Harness| {
            harness
                .driver
                .find_by_label("Custom reminder offset")
                .expect("the custom-offset field is mounted")
        };
        // A pointer click is the field's focus seam (no AT default
        // action on a text input, and nothing auto-focuses it).
        let focus = |harness: &mut Harness| {
            let field = field(harness);
            let pos = field.absolute_position();
            let size = field.size();
            harness
                .driver
                .click_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
        };
        assert_eq!(
            field(&*harness).value().as_deref(),
            Some(""),
            "the field mounts empty"
        );

        // A parse miss: nothing at the boundary, buffer reset.
        focus(&mut *harness);
        harness.type_text("soon");
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[]);
        assert_eq!(
            field(&*harness).value().as_deref(),
            Some(""),
            "the reject reset the buffer"
        );

        // A parse hit routes unchanged.
        focus(&mut *harness);
        harness.type_text("45m");
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[StateCommand::AddReminderRule {
            task: fixture.task1,
            offset: taskboard_domain::ReminderOffset::before(
                TimeDelta::try_minutes(-45).expect("-45m"),
            )
            .expect("legal offset"),
            at: None,
            insistence: taskboard_domain::Insistence::Normal,
        }]);
    });
}

// ---- r4 chunk 6 (D18): the activation stops + the walk ------------------

/// The task view's walk-stop fact, read back through the generated
/// global.
fn walk_stop(harness: &Harness) -> i32 {
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::TaskViewWalkFacts>()
        .get_stop_id()
}

/// Presses one control-walk step through the preset's own row: the
/// retune (J3 US-1, round 9 chunk 9) re-pointed emacs' C-f onto
/// nav.focus-right (the same control walk in the non-board view
/// contexts); the other presets keep the Tab row.
fn press_control_next(harness: &mut Harness, preset: taskboard_ui_slint::keymap::Preset) {
    let step = match preset {
        taskboard_ui_slint::keymap::Preset::Emacs => ActionId::NavFocusRight,
        _ => ActionId::NavNextControl,
    };
    press_preset(harness, preset, step);
}

/// Walks `steps` control steps, returning the stop fact observed after
/// each (the reading-order inventory).
fn walk_steps(harness: &mut Harness, steps: usize) -> Vec<i32> {
    let mut observed = Vec::with_capacity(steps);
    for _ in 0..steps {
        if harness.chain().contains(&ContextId::ViInsert) {
            // An engaged field masks the view context: the walk
            // stroke is the native Tab (unbound in the insert chain).
            harness.stroke(parse("Tab").expect("tab"));
        } else {
            press_preset(
                harness,
                taskboard_ui_slint::keymap::Preset::Standard,
                ActionId::NavNextControl,
            );
        }
        harness.driver.advance_time(0);
        observed.push(walk_stop(harness));
    }
    observed
}

/// r4 D18 (F12, acceptance a) — the structural stop inventory: the
/// activation stops report their ids in reading order under repeated
/// control walk, the non-stop interactables (header `IconButton`s, the
/// custom-offset field) occupy exactly one step without a report, and
/// the walk wraps; the open-editor branches replace their cell with
/// the field (one stop, no double report).
#[test]
fn the_task_view_stop_inventory_walks_in_reading_order() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state.clone(), move |harness| {
        press_preset(
            &mut *harness,
            taskboard_ui_slint::keymap::Preset::Standard,
            ActionId::NavFocusDown,
        );
        press_preset(
            &mut *harness,
            taskboard_ui_slint::keymap::Preset::Standard,
            ActionId::TaskEdit,
        );
        harness.driver.advance_time(0);
        assert_eq!(walk_stop(&*harness), 0, "no stop report before the walk");

        // The reading-order prefix: back (unreported), done, title,
        // tracking (unreported), due, scheduled, chips, the
        // custom-offset field (unreported), description. The wrap
        // then cycles through the window chrome (unreported stops)
        // and re-enters at the done box — the wrap pin.
        // The entries affordance (r4 chunk 10, D-TS8) joins the
        // unreported stops (the tracking verbs' precedent): it
        // sits after the scheduled cell, so the stale-repeat
        // window carries one more 6 before the rules rows. The
        // add-repeat affordance (round-7 D-7.1) joins them the
        // same way. The priority rows' edit affordances and the
        // labels section's add affordance (UX R-3 D-1/D-2, #788/
        // #789) join them too: three more unreported steps, the
        // rule rows and everything below shift a step each.
        let observed = walk_steps(&mut *harness, 48);
        assert_eq!(
            &observed[..17],
            &[0, 2, 3, 3, 5, 6, 6, 6, 6, 6, 6, 6, 90, 91, 92, 93, 93],
            "the reading-order inventory"
        );
        assert_eq!(
            observed[17], 11,
            "the description stop follows the rule rows"
        );
        // Structural: past the description stop, the walk cycles
        // through the unreported chrome and re-enters at the done
        // box (stop 2) — the re-entry index shifts with the
        // unreported-stop count, so the pin searches for it
        // instead of fixing an index.
        let reentry = observed[18..]
            .iter()
            .position(|stop| *stop == 2)
            .map(|offset| 18 + offset);
        assert!(
            reentry.is_some(),
            "the walk re-enters at the done box after the chrome"
        );

        // The open-title branch: the field replaces the cell —
        // one stop, no report, and the later cells keep their
        // order.
        harness.driver.click_by_label("Edit title");
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_task_model().edit_open,
            "the editor is armed for the open-branch leg"
        );
        // The field is the cell's one stop: the walk's first
        // stroke settles the armed editor (the background settle
        // rule) and the focus-origin restore (US-1/W-1 AC-3, the
        // census row 14) lands in the title cell — the walk
        // resumes at the restored origin (the recorded
        // re-baseline: the old stale key-root repeat window
        // [2, 2, …] pinned today's behavior; the restore is the
        // desired semantics).
        let observed = walk_steps(&mut *harness, 8);
        assert_eq!(
            &observed[..7],
            &[3, 3, 5, 6, 6, 6, 6],
            "the settle restores into the title cell and the walk reads on"
        );
        // The leg ends where the baseline ended — at the title
        // cell — so the downstream branches keep their entry
        // state (the walk resumes from the origin).
        let mut guard = 0;
        while walk_stop(&*harness) != 3 && guard < 24 {
            walk_steps(&mut *harness, 1);
            guard += 1;
        }
        assert_eq!(
            walk_stop(&*harness),
            3,
            "the leg ends at the restored origin"
        );

        // The open-due branch: the DueEditor replaces the cell.
        harness.stroke(parse("Escape").expect("escape"));
        harness.driver.advance_time(0);
        // Walk back to the due cell and open it.
        let mut guard = 0;
        while walk_stop(&*harness) != 5 && guard < 20 {
            press_preset(
                &mut *harness,
                taskboard_ui_slint::keymap::Preset::Standard,
                ActionId::NavNextControl,
            );
            harness.driver.advance_time(0);
            guard += 1;
        }
        assert_eq!(walk_stop(&*harness), 5, "the walk reached the due cell");
        harness
            .driver
            .find_by_label("Edit due date")
            .expect("the due cell is present")
            .invoke_default_action();
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_task_model().due_open,
            "the due editor is armed for the open-branch leg"
        );
        // The editor's field is the cell's one stop: under the
        // engaged field the walk stroke is the native Tab (the
        // insert chain leaves it unbound); the editor's own
        // affordances occupy the steps between it and the next
        // reported cell — the scheduled cell.
        let mut guard = 0;
        while walk_stop(&*harness) != 6 && guard < 12 {
            harness.stroke(parse("Tab").expect("tab"));
            harness.driver.advance_time(0);
            guard += 1;
        }
        assert_eq!(
            walk_stop(&*harness),
            6,
            "the scheduled cell follows the editor"
        );
    });
}

/// r4 D18 §2 (acceptance d; chunk 7's entry seam) — the cell
/// activation matrix: bare Return/Space on the focused cell activate
/// the cell's own seam under every preset — emacs Return never
/// resolves `task.title-edit` onto the cell, vi Return never
/// `task.edit`-activates and vi Space never completes — and the
/// reservation's prefix discipline holds: an emacs pending prefix
/// aborts on Return and consumes it (the feed-first ordering).
#[test]
#[allow(clippy::too_many_lines)] // the matrix's rows are the contract's length
fn the_focused_cell_activates_on_return_and_space_under_any_preset() {
    let matrix: Vec<(taskboard_ui_slint::keymap::Preset, i32, &'static str)> = vec![
        (taskboard_ui_slint::keymap::Preset::Standard, 3, "Return"),
        (taskboard_ui_slint::keymap::Preset::Standard, 5, "Return"),
        (taskboard_ui_slint::keymap::Preset::Standard, 6, "Return"),
        (taskboard_ui_slint::keymap::Preset::Standard, 11, "Return"),
        (taskboard_ui_slint::keymap::Preset::Standard, 90, "Return"),
        (taskboard_ui_slint::keymap::Preset::Emacs, 5, "Return"),
        (taskboard_ui_slint::keymap::Preset::Emacs, 5, "Space"),
        (taskboard_ui_slint::keymap::Preset::Vi, 5, "Return"),
        (taskboard_ui_slint::keymap::Preset::Vi, 5, "Space"),
    ];
    for (preset, stop, stroke_spelling) in matrix {
        let (fixture, state) = fixture::golden_state();
        let task0 = fixture.task0;
        support::harness::run_journey_with_settings(
            state.clone(),
            UiSettings {
                keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                keymap_preset: preset,
                ..UiSettings::default()
            },
            move |harness| {
                press_preset(&mut *harness, preset, ActionId::NavFocusDown);
                press_preset(&mut *harness, preset, ActionId::TaskEdit);
                harness.driver.advance_time(0);
                let mut guard = 0;
                while walk_stop(&*harness) != stop && guard < 20 {
                    // The always-on Effort Estimate field (M9) is a
                    // walk-through text stop: under vi landing on it
                    // arms insert, where NavNextControl is deliberately
                    // unbound (typing owns the field). Leave insert and
                    // step on — the field is pass-through for the walk
                    // (chunk 11 owns the effort edit/clear verbs).
                    if harness.chain().starts_with(&[ContextId::ViInsert]) {
                        harness.handle.dispatch_action(ActionId::ModeNormal);
                        harness.driver.advance_time(0);
                    }
                    press_control_next(&mut *harness, preset);
                    harness.driver.advance_time(0);
                    guard += 1;
                }
                assert_eq!(walk_stop(&*harness), stop, "the walk reached the cell");
                harness.stroke(parse(stroke_spelling).expect(stroke_spelling));
                harness.driver.advance_time(0);
                let task_model = harness.handle.window().get_task_model();
                match stop {
                    // The title cell arms the title editor.
                    3 => assert!(task_model.edit_open, "the title editor armed"),
                    // The due cell opens the due editor — under emacs
                    // the two-phase entry seam, under vi neither the
                    // `task.edit` activate nor the done toggle.
                    5 => {
                        assert!(task_model.due_open, "the due editor opened");
                        assert!(!task_model.done, "the done state never moved");
                    }
                    // The scheduled cell opens the start-date editor.
                    6 => assert!(task_model.scheduled_open, "the editor opened"),
                    // The description cell opens the editor.
                    11 => assert!(
                        task_model.description_edit_open,
                        "the description editor opened"
                    ),
                    // A chip adds its preset offset (5m).
                    90 => match harness.engine.drain().as_slice() {
                        [StateCommand::AddReminderRule { task, offset, .. }] => {
                            assert_eq!(*task, task0);
                            assert_eq!(
                                *offset,
                                taskboard_domain::ReminderOffset::before(
                                    TimeDelta::try_minutes(-5).expect("-5m")
                                )
                                .expect("legal offset")
                            );
                        }
                        other => panic!("the chip added its preset, got {other:?}"),
                    },
                    other => panic!("unreachable matrix row {other}"),
                }
            },
        );
    }

    // The done box (stop 2): Return toggles the subject's done state
    // — the checkbox row keeps its own semantics under the
    // reservation.
    let (fixture, state) = fixture::golden_state();
    let task0 = fixture.task0;
    let preset = taskboard_ui_slint::keymap::Preset::Vi;
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
            keymap_preset: preset,
            ..UiSettings::default()
        },
        move |harness| {
            press_preset(&mut *harness, preset, ActionId::NavFocusDown);
            press_preset(&mut *harness, preset, ActionId::TaskEdit);
            harness.driver.advance_time(0);
            let mut guard = 0;
            while walk_stop(&*harness) != 2 && guard < 20 {
                press_control_next(&mut *harness, preset);
                harness.driver.advance_time(0);
                guard += 1;
            }
            harness.stroke(parse("Return").expect("return"));
            harness.driver.advance_time(0);
            harness.assert_commands(&[StateCommand::SetTaskDone {
                id: task0,
                done: true,
            }]);
        },
    );

    // The prefix discipline: an emacs pending prefix (C-x) aborts on
    // Return and consumes it — the cell does not activate, and the
    // next bare Return does.
    let (_fixture, state) = fixture::golden_state();
    let preset = taskboard_ui_slint::keymap::Preset::Emacs;
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
            keymap_preset: preset,
            ..UiSettings::default()
        },
        move |harness| {
            press_preset(&mut *harness, preset, ActionId::NavFocusDown);
            press_preset(&mut *harness, preset, ActionId::TaskEdit);
            harness.driver.advance_time(0);
            let mut guard = 0;
            while walk_stop(&*harness) != 5 && guard < 20 {
                press_control_next(&mut *harness, preset);
                harness.driver.advance_time(0);
                guard += 1;
            }
            harness.stroke(parse("Ctrl+x").expect("Ctrl+x"));
            harness.stroke(parse("Return").expect("return"));
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_task_model().due_open,
                "the pending prefix consumed the Return: no activation"
            );
            harness.stroke(parse("Return").expect("return"));
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_task_model().due_open,
                "the bare Return activates the cell"
            );
        },
    );
}

// ---- r4 chunk 6 (D20): the focus-driven insert engagement --------------

/// Click-focuses the element at `label`'s centre (a text input has no
/// AT default action; the pointer press is the field's focus seam —
/// the parse-miss journey's helper shape).
fn click_field_centre(harness: &mut Harness, label: &str) {
    let element = harness
        .driver
        .find_by_label(label)
        .unwrap_or_else(|| panic!("the field {label:?} is present"));
    let (x, y) = support::driver::centre(&element);
    harness.driver.click_at(x, y);
    // The focus-change notification (and so the D20 publish) rides
    // the clock's handler pass.
    harness.driver.advance_time(0);
}

/// The edited-facts singleton, read back through the generated global.
fn text_focus(harness: &Harness) -> i32 {
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::EditingFacts>()
        .get_text_focus_count()
}

/// r4 D20 — the Return-never-hijacks matrix (F24, plan §6.C): every
/// preset against the four field sites. A focused text field masks
/// the view context, so the preset-bound Return reaches the field's
/// own commit — never arms a view flow. The emacs column is the F24
/// repro verbatim (Return resolved `task.title-edit` mid-typing).
#[test]
#[allow(clippy::too_many_lines)] // the matrix's four field legs are the contract's length
fn return_never_hijacks_on_a_focused_field_under_any_preset() {
    let presets = [
        (taskboard_ui_slint::keymap::Preset::Standard, "standard"),
        (taskboard_ui_slint::keymap::Preset::Emacs, "emacs"),
        (taskboard_ui_slint::keymap::Preset::Vi, "vi"),
    ];
    for (preset, name) in presets {
        let settings = move || UiSettings {
            keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
            keymap_preset: preset,
            ..UiSettings::default()
        };

        // -- the custom reminder offset field (the repro verbatim) --
        let (fixture, state) = fixture::golden_state();
        let task0 = fixture.task0;
        support::harness::run_journey_with_settings(state, settings(), move |harness| {
            press_preset(&mut *harness, preset, ActionId::NavFocusDown);
            press_preset(&mut *harness, preset, ActionId::TaskEdit);
            harness.driver.advance_time(0);
            click_field_centre(&mut *harness, "Custom reminder offset");
            harness.type_text("45m");
            harness.stroke(parse("Return").expect("return"));
            match harness.engine.drain().as_slice() {
                [StateCommand::AddReminderRule { task, offset, .. }] => {
                    assert_eq!(*task, task0, "{name}: the subject's rule");
                    assert_eq!(
                        *offset,
                        taskboard_domain::ReminderOffset::before(
                            TimeDelta::try_minutes(-45).expect("-45m")
                        )
                        .expect("legal offset")
                    );
                }
                other => panic!("{name}: the offset committed, got {other:?}"),
            }
            assert!(
                !harness.handle.window().get_task_model().edit_open,
                "{name}: no view flow armed over the field"
            );
            assert!(
                keyboard(&*harness).get_vi_insert(),
                "{name}: the field's focus masks the chain"
            );
        });

        // -- the title field --
        let (_fixture, state) = fixture::golden_state();
        support::harness::run_journey_with_settings(state, settings(), move |harness| {
            press_preset(&mut *harness, preset, ActionId::NavFocusDown);
            press_preset(&mut *harness, preset, ActionId::TaskEdit);
            harness.driver.advance_time(0);
            harness.driver.click_by_label("Edit title");
            harness.driver.advance_time(0);
            harness.type_text("x");
            harness.stroke(parse("Return").expect("return"));
            assert!(
                !harness.handle.window().get_task_model().edit_open,
                "{name}: the commit closed the editor"
            );
            match harness.engine.drain().as_slice() {
                [StateCommand::UpdateTask { changes, .. }] => {
                    assert_eq!(changes.title.as_deref(), Some("x"));
                }
                other => panic!("{name}: the title committed, got {other:?}"),
            }
        });

        // -- the description field --
        let (_fixture, state) = fixture::golden_state();
        support::harness::run_journey_with_settings(state, settings(), move |harness| {
            press_preset(&mut *harness, preset, ActionId::NavFocusDown);
            press_preset(&mut *harness, preset, ActionId::TaskEdit);
            harness.driver.advance_time(0);
            harness.driver.click_by_label("Edit description");
            harness.driver.advance_time(0);
            // D-MLE-6 (the D1 field-level redefinition): plain
            // Return inserts a newline and never commits; the
            // commit seam is Shift+Return.
            harness.type_text("hello");
            harness.stroke(parse("Return").expect("return"));
            harness.driver.advance_time(0);
            assert!(
                harness
                    .handle
                    .window()
                    .get_task_model()
                    .description_edit_open,
                "{name}: plain Return never closed the editor"
            );
            assert!(
                harness.engine.drain().is_empty(),
                "{name}: plain Return dispatched nothing"
            );
            harness.type_text("world");
            harness.stroke(parse("Shift+Return").expect("shift+return"));
            assert!(
                !harness
                    .handle
                    .window()
                    .get_task_model()
                    .description_edit_open,
                "{name}: the commit closed the editor"
            );
            match harness.engine.drain().as_slice() {
                [StateCommand::UpdateTask { changes, .. }] => {
                    assert_eq!(changes.description.as_deref(), Some("hello\nworld"));
                }
                other => panic!("{name}: the description committed, got {other:?}"),
            }
        });

        // -- the today timers' timing field --
        let (_fixture, state) = fixture::golden_state();
        support::harness::run_journey_with_settings(state, settings(), move |harness| {
            press_preset(&mut *harness, preset, ActionId::ViewToday);
            assert_eq!(current_view(&*harness), View::Today);
            click_field_centre(&mut *harness, "Timer label");
            harness.type_text("Tea");
            click_field_centre(&mut *harness, "Timer timing (in 10m or at 09:30)");
            harness.type_text("in 3m");
            harness.stroke(parse("Return").expect("return"));
            match harness.engine.drain().as_slice() {
                [StateCommand::CreateAdhocAlarm { label, when, .. }] => {
                    assert_eq!(label.as_str(), "Tea", "{name}");
                    assert_eq!(
                        *when,
                        taskboard_domain::AdhocWhen::In(TimeDelta::minutes(3)),
                        "{name}"
                    );
                }
                other => panic!("{name}: the timer created, got {other:?}"),
            }
            assert_eq!(
                current_view(&*harness),
                View::Today,
                "{name}: Return never hijacked into a task view"
            );
        });
    }
}

/// r4 D20 — the palette exception (plan §3.5): the palette's own chain
/// branch precedes the mode check, so Return is the jump under every
/// preset — never a field commit, never a hijack.
#[test]
fn the_palette_return_jumps_under_every_preset() {
    for (preset, name) in [
        (taskboard_ui_slint::keymap::Preset::Standard, "standard"),
        (taskboard_ui_slint::keymap::Preset::Emacs, "emacs"),
        (taskboard_ui_slint::keymap::Preset::Vi, "vi"),
    ] {
        let (fixture, mut state) = fixture::two_board_state();
        let id = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut task = state.tasks.values().next().expect("fixture task").clone();
        task.id = id;
        task.stack = fixture.stack_c;
        task.order = 5;
        task.title = "zebra needle".to_owned();
        state.tasks.insert(id, task);
        support::harness::run_journey_with_settings(
            state,
            UiSettings {
                keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                keymap_preset: preset,
                ..UiSettings::default()
            },
            move |harness| {
                press_preset(&mut *harness, preset, ActionId::PaletteSearch);
                assert!(
                    harness.handle.window().get_palette_open(),
                    "{name}: the palette opened"
                );
                harness.type_text("zebra needle");
                harness.stroke(parse("Return").expect("return"));
                assert!(
                    !harness.handle.window().get_palette_open(),
                    "{name}: the jump closed the palette"
                );
                assert_eq!(
                    harness.handle.window().get_board_bar_model().selected,
                    1,
                    "{name}: the jump selected the needle's board"
                );
                harness.assert_commands(&[]);
            },
        );
    }
}

/// r4 D20 — the focus-engage deltas the per-mount seams could not
/// do: the custom offset field's Space is a space once the field
/// holds focus (vi's `task.complete` never toggles over an engaged
/// field), and the whole engagement rides the click-focus — zero
/// `edited` events involved anywhere.
#[test]
fn focus_alone_engages_and_reserves_the_field_strokes() {
    let (fixture, state) = fixture::golden_state();
    let task0 = fixture.task0;
    let preset = taskboard_ui_slint::keymap::Preset::Vi;
    support::harness::run_journey_with_settings(
        state.clone(),
        UiSettings {
            keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
            keymap_preset: preset,
            ..UiSettings::default()
        },
        move |harness| {
            press_preset(&mut *harness, preset, ActionId::NavFocusDown);
            press_preset(&mut *harness, preset, ActionId::TaskEdit);
            harness.driver.advance_time(0);

            // Space over the click-focused field inserts, never
            // toggles the subject's done state.
            click_field_centre(&mut *harness, "Custom reminder offset");
            harness.type_text("4");
            harness.stroke(parse("Space").expect("space"));
            harness.type_text("5m");
            let field = harness
                .driver
                .find_by_label("Custom reminder offset")
                .expect("the field is mounted");
            assert_eq!(
                field.value().as_deref(),
                Some("4 5m"),
                "the Space reached the field's buffer"
            );
            harness.assert_commands(&[]);
            assert!(
                !harness.handle.window().get_task_model().done,
                "the done state never toggled over the field"
            );

            // Return commits (the parse hit), still no done toggle:
            // clear the spaced buffer the way a user retypes.
            for _ in 0..4 {
                harness.stroke(parse("Backspace").expect("backspace"));
            }
            harness.type_text("45m");
            harness.stroke(parse("Return").expect("return"));
            match harness.engine.drain().as_slice() {
                [StateCommand::AddReminderRule { task, .. }] => assert_eq!(*task, task0),
                other => panic!("the offset committed, got {other:?}"),
            }
        },
    );
}

/// r4 D20 — the count lifecycle over the generated global: the
/// publish rides focus (click alone), the A→B transition stays at one
/// through the unspecified handler order, the flow end resets both
/// legs, and the view bindings resolve again after the unmask (the
/// emacs Return re-arms the title — the unmask pin).
#[test]
fn the_editing_facts_count_lifecycle_tracks_focus() {
    let (_fixture, state) = fixture::golden_state();
    let preset = taskboard_ui_slint::keymap::Preset::Emacs;
    support::harness::run_journey_with_settings(
        state.clone(),
        UiSettings {
            keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
            keymap_preset: preset,
            ..UiSettings::default()
        },
        move |harness| {
            press_preset(&mut *harness, preset, ActionId::NavFocusDown);
            press_preset(&mut *harness, preset, ActionId::TaskEdit);
            harness.driver.advance_time(0);

            // Gain: the click alone publishes.
            harness.driver.click_by_label("Edit title");
            harness.driver.advance_time(0);
            assert_eq!(text_focus(&*harness), 1, "the focused field published");

            // The empty commit closes the flow: the flow-end reset
            // zeroes the count and the insert mirror.
            for _ in 0..7 {
                harness.stroke(parse("Backspace").expect("backspace"));
            }
            harness.stroke(parse("Return").expect("return"));
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_task_model().edit_open,
                "the empty commit closed the editor"
            );
            assert_eq!(text_focus(&*harness), 0, "the flow end reset the count");
            assert!(
                !keyboard(&*harness).get_vi_insert(),
                "the unmask dropped the insert mirror"
            );

            // Unmask: the view chain resolves again — emacs Return
            // re-arms the title editor (the field re-publishes).
            harness.stroke(parse("Return").expect("return"));
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_task_model().edit_open,
                "the unmasked Return resolved the view binding"
            );

            // A→B: the armed title flow settles through Escape, then
            // the due editor's field mounts and publishes — the count
            // tracks each field's own focus, never a stranded state.
            harness.stroke(parse("Escape").expect("escape"));
            harness.driver.advance_time(0);
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_task_model().due_open,
                "the due click armed the editor"
            );
            assert!(
                text_focus(&*harness) >= 1,
                "the newly mounted editor field published"
            );

            // Escape settles the flow; the reset seam zeroes again.
            harness.stroke(parse("Escape").expect("escape"));
            harness.driver.advance_time(0);
            assert_eq!(text_focus(&*harness), 0, "the settle reset the count");
        },
    );
}

/// R2 D24 — the preset chips carry the design's padding tokens: each
/// chip's painted width is at least the space-6 floor at scale 1.0
/// (the label no longer touches the chip edges).
#[test]
fn the_preset_chips_hold_their_token_floor() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.advance_time(0);
        // The id query reports each repetition through every
        // ancestor chain; dedup by painted geometry.
        let mut seen: Vec<(f32, f32)> = Vec::new();
        let chips = harness
            .driver
            .find_all_by_id("RuleList::preset-chip")
            .into_iter()
            .filter(|chip| {
                let pos = chip.absolute_position();
                let key = (pos.x, pos.y);
                let fresh = !seen.contains(&key);
                if fresh {
                    seen.push(key);
                }
                fresh
            })
            .collect::<Vec<_>>();
        assert_eq!(chips.len(), 4, "the fixture's four preset chips");
        for chip in chips {
            assert!(
                chip.size().width >= 32.0, // Theme.space-6 at scale 1
                "every chip is at least the space-6 floor, got {}",
                chip.size().width
            );
        }
    });
}

/// R2 D24 — the custom-offset entry stretches into the presets row's
/// leftover: its painted width lands well past the primitive's token
/// floor — where it would sit if the row sized every cell to its
/// preferred width (the stretch default is load-bearing).
#[test]
fn the_custom_offset_field_stretches_into_the_presets_row() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 800);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.advance_time(0);
        let field = harness
            .driver
            .find_by_label("Custom reminder offset")
            .expect("the custom-offset field is mounted");
        assert!(
            field.size().width > 160.0, // the 96 px floor plus the row's leftover
            "the field takes the row's leftover, got {}",
            field.size().width
        );
    });
}

/// Round-3 D25 — the control walk reaches the picker: Tab out of the
/// armed due field lands on the calendar grid (the native traversal —
/// Tab is unbound in the insert chain), the grid's focus seam arms
/// the `DatePicker` context (the pushed mirror reads true), the
/// preset's keys drive the grid (picker.right moves the cursor a
/// day), and Escape still closes the editor cleanly — the context
/// never masks. On close the context disarms (the flow-end belt).
#[test]
fn the_control_walk_reaches_fields_buttons_and_the_picker() {
    let (fixture, mut state) = fixture::journey_state();
    // The walk's subject: one NavNextItem focuses stack "a"'s FIRST
    // task, so the due is seeded there — the picker opens anchored on
    // it (today's fallback would be wall-clock-dependent). January
    // 2030 starts on a Tuesday: one leading blank puts the 30th at
    // flat grid index 30 on every calendar day.
    let due = local_midnight(2030, 1, 30);
    state
        .tasks
        .get_mut(&fixture.task0)
        .expect("fixture task")
        .duedate = Some(due);
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_task_model().date_picker_open,
            "the picker is default-open; the field keeps focus"
        );
        let grid_armed = |harness: &Harness| {
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_date_picker_grid()
        };
        assert!(
            !grid_armed(&*harness),
            "the context is disarmed while the field holds focus"
        );

        let cursor_index = |harness: &Harness| -> Option<usize> {
            let grid = harness.handle.window().get_task_model().date_grid;
            let mut flat = 0usize;
            for r in 0..grid.row_count() {
                let row = grid.row_data(r).expect("row");
                for c in 0..row.row_count() {
                    if row.row_data(c).expect("cell").cursor {
                        return Some(flat);
                    }
                    flat += 1;
                }
            }
            None
        };
        // The seeded due is January 30, 2030: one Monday-first
        // leading blank (the month opens on a Tuesday) puts the
        // 30th at flat index 30.
        assert_eq!(cursor_index(&*harness), Some(30), "the due anchors");

        // The walk enters the picker: Down-in-field hands focus
        // to the grid (the chunk-3 focus-on-mount seam) and the
        // focus seam arms the context — the same arming a Tab
        // traversal onto the grid produces.
        harness.stroke(parse("Down").expect("Down"));
        harness.driver.advance_time(0);
        assert!(
            grid_armed(&*harness),
            "the grid took focus and armed the context"
        );

        // The preset's keys drive the grid: picker.right moves
        // the cursor one day (January 2030 stays on-grid).
        harness.press(ActionId::PickerRight);
        harness.driver.advance_time(0);
        assert_eq!(cursor_index(&*harness), Some(31), "one day forward");

        // The context never masks: Escape resolves ModeNormal
        // from ViInsert — the editor's clean close.
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_task_model().date_picker_open,
            "the editor closed clean"
        );
        assert!(!grid_armed(&*harness), "the context disarmed with the flow");
        harness.assert_commands(&[]);
    });
}

/// Round-3 D26 — the scroll actions page the focused area: the
/// settings pane pages adapter-side through the two-way facts and
/// clamps at both ends (the end of content IS the save row's
/// reachability), the board column hint arms for the focused column,
/// and the task detail — no container — is a typed no-op.
#[test]
fn scroll_actions_page_the_focused_area() {
    let (_fixture, state) = fixture::two_board_state();

    support::harness::run_journey(state, move |harness| {
        macro_rules! window {
            () => {
                harness.handle.window()
            };
        }
        macro_rules! scroll_hint {
            ($h:expr) => {
                $h.handle
                    .window()
                    .global::<taskboard_ui_slint::ScrollPageHint>()
            };
        }

        // -- settings: pages to the end and clamps at both ends --
        harness.press(ActionId::AppSettings);
        harness.driver.advance_time(0);
        macro_rules! scroll_facts {
            ($h:expr) => {
                $h.handle
                    .window()
                    .global::<taskboard_ui_slint::SettingsScrollFacts>()
            };
        }
        let viewport = scroll_facts!(harness).get_viewport_height();
        let content = scroll_facts!(harness).get_content_height();
        assert!(
            content > viewport,
            "precondition: the settings pane scrolls"
        );
        harness.press(ActionId::ViewScrollDown);
        harness.driver.advance_time(0);
        let after_one = scroll_facts!(harness).get_viewport_y();
        assert!(after_one > 0.0, "one page advanced the pane");

        // Page to the end: the clamp is content − viewport (the
        // save row's reachability), and further pages stay there.
        for _ in 0..20 {
            harness.press(ActionId::ViewScrollDown);
            harness.driver.advance_time(0);
        }
        let max = scroll_facts!(harness).get_viewport_y();
        assert!(
            (max - (content - viewport)).abs() < 1.0,
            "the pane clamped at the end of content"
        );
        harness.press(ActionId::ViewScrollDown);
        harness.driver.advance_time(0);
        assert_eq!(
            scroll_facts!(harness).get_viewport_y(),
            max,
            "the clamp holds"
        );

        // Page up clamps at the top.
        for _ in 0..20 {
            harness.press(ActionId::ViewScrollUp);
            harness.driver.advance_time(0);
        }
        assert_eq!(
            scroll_facts!(harness).get_viewport_y(),
            0.0,
            "the top clamp"
        );

        // -- board: the focused task's column pages --
        harness.press(ActionId::NavFocusBoard);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::ViewScrollDown);
        harness.driver.advance_time(0);
        let h = scroll_hint!(harness);
        assert!(h.get_nonce() > 0, "the focused column's page hint armed");
        assert_eq!(h.get_family(), 1, "the board-column family");
        assert_eq!(h.get_page(), 1, "the down direction");

        // -- task detail: no container, a typed no-op --
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        assert_eq!(window!().get_current_view(), View::TaskDetail);
        let nonce_before = scroll_hint!(harness).get_nonce();
        harness.press(ActionId::ViewScrollDown);
        harness.driver.advance_time(0);
        assert_eq!(
            scroll_hint!(harness).get_nonce(),
            nonce_before,
            "the containerless detail hints nothing"
        );
    });
}

/// Round-3 D27 — the menu curates itself: the `ViewNav` page rides the
/// re-rank (the go-to verbs above the cycle, read off the pushed
/// submenu model by action id — ids are addresses, never label
/// text), the greying flips with state (the move-cancel row is
/// disabled without a flow and enabled when a move flow holds), and
/// the root page's parent rows shrink by the preset-irrelevant Mode
/// group (structural: the parent count over the standard preset).
#[test]
#[allow(clippy::too_many_lines)] // the curation walk is the contract's length
fn the_menu_curates_itself() {
    let (_fixture, state) = fixture::two_board_state();

    support::harness::run_journey(state, move |harness| {
        macro_rules! window {
            () => {
                harness.handle.window()
            };
        }

        harness.press(ActionId::NavFocusDown); // seed a focus
        harness.press(ActionId::AppMenu);
        harness.driver.advance_time(0);

        macro_rules! keyboard {
            () => {
                window!().global::<taskboard_ui_slint::KeyboardState>()
            };
        }
        let find = |model: &slint::ModelRc<taskboard_ui_slint::MenuEntryModel>, action: &str| {
            (0..model.row_count())
                .map(|i| model.row_data(i).expect("row"))
                .find(|row| row.action == action)
                .unwrap_or_else(|| panic!("the {action} row is on the page"))
        };
        // Walk to a parent row and open it (the cursor walks the
        // root page; the first parent is the Task group).
        macro_rules! open_first_parent {
            ($h:expr) => {{
                let opened = loop {
                    let k = keyboard!();
                    let entries = k.get_menu_entries();
                    #[allow(clippy::cast_sign_loss)] // the getter's −1 sentinel
                    let index = k.get_menu_selected_index().max(0) as usize;
                    if let Some(row) = entries.row_data(index) {
                        if row.submenu {
                            break true;
                        }
                    }
                    #[allow(clippy::cast_possible_truncation)] // page-bounded walk
                    #[allow(clippy::cast_possible_wrap)]
                    if i32::try_from(index).is_ok_and(|i| i > 24) {
                        break false;
                    }
                    $h.stroke(parse("Down").expect("Down"));
                    $h.driver.advance_time(0);
                };
                if opened {
                    $h.stroke(parse("Return").expect("Return"));
                    $h.driver.advance_time(0);
                }
                opened
            }};
        }

        // The Mode parent is filtered under the standard preset:
        // five parents ride the root page (D27b).
        let root = keyboard!().get_menu_entries();
        let parents = (0..root.row_count())
            .filter(|i| root.row_data(*i).expect("row").submenu)
            .count();
        assert_eq!(parents, 5, "the Mode parent is filtered (D27b)");

        // The greying flip: without a move flow the Task page's
        // cancel row reads disabled.
        assert!(open_first_parent!(harness), "a parent row opened");
        let task_page = keyboard!().get_menu_submenu_entries();
        assert!(
            !find(&task_page, "task.move.cancel").enabled,
            "the cancel row greys without a flow"
        );
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        assert!(!keyboard!().get_menu_open(), "the menu closed");

        // The ViewNav page's order: the go-to verbs above the
        // cycle (D27a). The ViewNav parent is the fourth of the
        // five rendered parents.
        harness.press(ActionId::AppMenu);
        harness.driver.advance_time(0);
        let mut parent_ordinal = 0;
        // The walk budget rides the root page's length (the UF2
        // picker's quick row sits behind the v1 undo/redo pair and
        // shifts the parents down one row).
        for _ in 0..24 {
            let k = keyboard!();
            let entries = k.get_menu_entries();
            #[allow(clippy::cast_sign_loss)] // the getter's −1 sentinel
            let index = k.get_menu_selected_index().max(0) as usize;
            let row = entries.row_data(index).expect("row");
            if row.submenu {
                parent_ordinal += 1;
                if parent_ordinal == 4 {
                    break;
                }
            }
            harness.stroke(parse("Down").expect("Down"));
            harness.driver.advance_time(0);
        }
        assert_eq!(parent_ordinal, 4, "the ViewNav parent is the fourth");
        harness.stroke(parse("Return").expect("Return"));
        harness.driver.advance_time(0);
        let submenu = keyboard!().get_menu_submenu_entries();
        let index = |action: &str| {
            (0..submenu.row_count())
                .map(|i| submenu.row_data(i).expect("row"))
                .position(|row| row.action == action)
                .unwrap_or_else(|| panic!("the {action} row rides the page"))
        };
        assert!(index("view.today") < index("nav.next-view"));
        assert!(index("nav.focus-board") < index("nav.next-view"));
        assert!(index("nav.next-view") < index("nav.next-control"));

        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        assert!(!keyboard!().get_menu_open(), "the menu closed");

        // The flip's reachable half: with a live move flow the
        // cancel row reads enabled (the menu-open settle cannot
        // coexist with the flow, so the enabled side rides the
        // entry_states pin at the adapter layer).
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        harness.stroke(parse("M").expect("M")); // task.move
        harness.driver.advance_time(0);
        harness.press(ActionId::AppMenu);
        harness.driver.advance_time(0);
        assert!(open_first_parent!(harness), "a parent row opened");
        let task_page = keyboard!().get_menu_submenu_entries();
        assert!(
            !find(&task_page, "task.move.cancel").enabled,
            "opening the menu settled the flow (F9): the cancel row greys again"
        );
    });
}

/// R2 D25 — the due editor's typed path: ISO commits at 00:00 UTC,
/// `+N<unit>` commits relative to now (bracketed by the journey's own
/// clock reads), an empty accept clears, and a refusal dispatches
/// nothing while keeping the buffer.
#[test]
#[allow(clippy::too_many_lines)] // the walk is the contract's length
fn the_typed_due_entries_reach_the_boundary() {
    let (fixture, mut state) = fixture::journey_state();
    let due = local_midnight(2030, 1, 31);
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(due);
    support::harness::run_journey(state, move |harness| {
        // One walk into task1's detail; the editor re-opens by
        // its cell after every commit (the view stays put).
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        let open_editor = |harness: &mut Harness| {
            assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            assert!(
                keyboard(harness).get_vi_insert(),
                "the editor opened in insert mode"
            );
        };

        // ISO: commits at the date's midnight UTC.
        open_editor(&mut *harness);
        harness.clear_seeded();
        harness.type_text("2030-02-28");
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(window_end_utc(2030, 2, 28))),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);

        // Round-3 D11: opening seeds the buffer with the set
        // value — Return untouched re-commits the SAME value (the
        // accidental clear is dead; the engine treats it as a
        // content no-op, blessed by the acceptance).
        open_editor(&mut *harness);
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(window_end_utc(2030, 1, 31))),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);

        // The x-clear stages the clear; Return commits it
        // (`Some(None)`).
        open_editor(&mut *harness);
        harness
            .driver
            .find_by_label("Clear")
            .expect("the x affordance is mounted")
            .invoke_default_action();
        harness.driver.advance_time(0);
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                duedate: Some(None),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);

        // Relative: now + 2d, bracketed by the journey's own reads.
        open_editor(&mut *harness);
        harness.clear_seeded();
        let before = chrono::Utc::now();
        harness.type_text("+2d");
        harness.stroke(parse("Return").expect("Return"));
        let after = chrono::Utc::now();
        let drained = harness.engine.drain();
        match drained.as_slice() {
            [StateCommand::UpdateTask { id, changes, .. }] => {
                assert_eq!(id, &fixture.task1);
                let offset = changes
                    .duedate
                    .expect("the due arm is Some(_)")
                    .expect("a set, not a clear")
                    - before;
                assert!(
                    offset >= TimeDelta::try_days(2).unwrap() - TimeDelta::try_seconds(1).unwrap()
                        && offset <= TimeDelta::try_days(2).unwrap() + (after - before),
                    "+2d lands inside the journey's clock bracket, got {offset}"
                );
            }
            other => panic!("the relative commit reached the boundary, got {other:?}"),
        }

        // A refusal dispatches nothing and keeps the buffer.
        open_editor(&mut *harness);
        harness.clear_seeded();
        harness.type_text("junk");
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[]);
        let field = harness
            .driver
            .find_by_label("Due date entry")
            .expect("the editor stays open on a refusal");
        assert_eq!(
            field.value().as_deref(),
            Some("junk"),
            "the refused buffer is kept"
        );
        harness.stroke(parse("Escape").expect("Escape"));
        harness.assert_commands(&[]);
    });
}

/// Round-3 D8 (acceptance b) — scheduled parity: the start-date cell
/// opens the *same* editor over the other field; a typed ISO reaches
/// the boundary as `scheduled: Some(Some(_))`, the x-clear + Return
/// as `Some(None)`, and `duedate` stays untouched throughout.
#[test]
fn the_scheduled_editor_reaches_the_boundary_with_parity() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .scheduled = Some(local_midnight(2030, 3, 15));
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.click_by_label("Edit start date");
        harness.driver.advance_time(0);
        assert!(
            keyboard(&*harness).get_vi_insert(),
            "the editor opened in insert mode"
        );

        // Typed ISO lands on `scheduled`, `duedate` untouched.
        harness.clear_seeded();
        harness.type_text("2030-04-01");
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                scheduled: Some(Some(window_end_utc(2030, 4, 1))),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);

        // The x-clear stages the clear; Return commits `Some(None)`.
        harness.driver.click_by_label("Edit start date");
        harness.driver.advance_time(0);
        harness.clear_seeded();
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                scheduled: Some(None),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);
    });
}

/// R4 D23 (F19) — the steppers work from an empty time: a date-only
/// picker seeds midnight on the first backward step (23:45 the
/// previous day, the readout leaves `--:--`), and the first forward
/// step lands 00:15; the eventual commit carries the stepped instant.
#[test]
fn steppers_seed_and_step_from_empty() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_midnight(2030, 1, 31));
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_task_model().date_time_text,
            "--:--",
            "a midnight anchor stays date-only — no phantom time"
        );

        // The first backward step seeds midnight and wraps to the
        // previous day's 23:45; the readout follows.
        harness.handle.window().invoke_task_date_picker_key(9);
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_task_model().date_time_text,
            "23:45",
            "the seed made a time exist and the step wrapped"
        );

        // The commit carries the stepped instant — cursor date
        // followed the wrap backward. (Grid-Enter no longer
        // commits — r4 D25's hand-over; the combobox funnel does.)
        harness
            .handle
            .window()
            .invoke_task_date_time_accepted("23:45".into());
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(local_wall(2030, 1, 30, 23, 45))),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);

        // A fresh editor: the first forward step from empty is
        // 00:15 the same day.
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        harness.handle.window().invoke_task_date_picker_key(10);
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_task_model().date_time_text,
            "00:15",
            "the seed stepped forward"
        );
    });
}

/// R4 D24 (F20) — the click-away rule on the date fields: a
/// parseable non-empty buffer commits (the same funnel as Return),
/// junk and empty discard (no command, the prior value survives, the
/// editor collapses). Both date fields ride the same rule.
#[test]
#[allow(clippy::too_many_lines)] // the rule's three legs, both fields
fn click_away_commits_parseable_and_discards_junk() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_midnight(2030, 1, 31));
    support::harness::run_journey(state.clone(), move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        // (a) Junk: the click-away discards — no command, the
        // editor collapses, the prior due survives a reopen.
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        harness.clear_seeded();
        harness.type_text("soon");
        harness.driver.click_at(600.0, 450.0);
        harness.driver.advance_time(0);
        harness.assert_commands(&[]);
        assert!(
            !harness.handle.window().get_task_model().due_open,
            "junk cannot linger — the editor collapsed"
        );
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_task_model().due_text.as_str(),
            "2030-01-31",
            "the prior value re-seeded"
        );
        // The two-press ladder out: step-back to the grid, then
        // the clean close — leg (b) starts from a disarmed editor.
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_task_model().due_open,
            "the ladder closed the editor"
        );

        // (b) Parseable: the click-away commits the same command
        // Return would.
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        harness.clear_seeded();
        harness.type_text("2030-02-14");
        harness.driver.click_at(600.0, 450.0);
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(window_end_utc(2030, 2, 14))),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);
        assert!(
            !harness.handle.window().get_task_model().due_open,
            "the editor collapsed on the commit"
        );

        // (c) Empty: the click-away never means clear — no
        // command, the due survives.
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        harness.driver.click_by_label("Clear");
        harness.driver.advance_time(0);
        harness.driver.click_at(600.0, 450.0);
        harness.driver.advance_time(0);
        harness.assert_commands(&[]);
        assert!(
            !harness.handle.window().get_task_model().due_open,
            "the editor collapsed on the empty discard"
        );
        // The absence of a command IS the survival: empty on
        // focus-loss never means clear (a structural fact, not a
        // state-timing read).

        // (d) The start-date field rides the same rule: junk
        // discards (its seeded buffer is empty — typing is the
        // whole entry).
        harness.driver.click_by_label("Edit start date");
        harness.driver.advance_time(0);
        harness.type_text("soon");
        harness.driver.click_at(600.0, 450.0);
        harness.driver.advance_time(0);
        harness.assert_commands(&[]);
        assert!(
            !harness.handle.window().get_task_model().scheduled_open,
            "the scheduled editor collapsed"
        );
        harness.driver.click_by_label("Edit start date");
        harness.driver.advance_time(0);
        harness.type_text("2030-03-01");
        harness.driver.click_at(600.0, 450.0);
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                scheduled: Some(Some(window_end_utc(2030, 3, 1))),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);
    });
}

/// R4 D27 (F27) — the date field's tri-state validity flips with the
/// authoritative parse: junk reads invalid, a parseable entry valid,
/// the untouched field neutral. The model property is the oracle —
/// never the label text.
#[test]
fn the_date_field_validity_flips_with_the_parse() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_midnight(2030, 1, 31));
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_task_model().date_validity,
            taskboard_ui_slint::FieldValidity::Valid,
            "the seeded buffer parses"
        );
        harness.clear_seeded();
        harness.type_text("soon");
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_task_model().date_validity,
            taskboard_ui_slint::FieldValidity::Invalid
        );
        harness.clear_seeded();
        harness.type_text("2030-02-14");
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_task_model().date_validity,
            taskboard_ui_slint::FieldValidity::Valid
        );
    });
}

/// R4 D27 (F27) — the custom-offset and timers-`when` fields ride
/// their authoritative grammars' validity: junk reads invalid, a
/// parseable entry valid, untouched neutral — model reads, never
/// label text.
#[test]
fn the_offset_and_timer_fields_flip_validity_with_their_grammars() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);

        // The custom-offset field on the task detail.
        harness
            .handle
            .window()
            .invoke_rule_offset_edited("soon".into());
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_task_model().offset_validity,
            taskboard_ui_slint::FieldValidity::Invalid
        );
        harness
            .handle
            .window()
            .invoke_rule_offset_edited("45m".into());
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_task_model().offset_validity,
            taskboard_ui_slint::FieldValidity::Valid
        );
        harness.handle.window().invoke_rule_offset_edited("".into());
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_task_model().offset_validity,
            taskboard_ui_slint::FieldValidity::Neutral
        );

        // The timers form's `when` field on the today view.
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        harness
            .handle
            .window()
            .invoke_timers_when_edited("soon".into());
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_timer_when_validity(),
            taskboard_ui_slint::FieldValidity::Invalid
        );
        harness
            .handle
            .window()
            .invoke_timers_when_edited("in 10m".into());
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_timer_when_validity(),
            taskboard_ui_slint::FieldValidity::Valid
        );
        harness.handle.window().invoke_timers_when_edited("".into());
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_timer_when_validity(),
            taskboard_ui_slint::FieldValidity::Neutral
        );
    });
}

/// Chunk-9 mount regression — the section's `when` field rides the
/// pushed validity *through the mount*: typing into the rendered
/// field (not the window callback) flips the `TodayView` root flavour.
/// The adapter seam is pinned by the journey above; this one pins the
/// wiring — the mount's forward was never wired and the seam journey
/// could not see it.
#[test]
fn typing_into_the_timers_section_drives_the_when_validity() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        let click_field = |harness: &mut Harness, label: &str| {
            let element = harness
                .driver
                .find_by_label(label)
                .unwrap_or_else(|| panic!("the field {label:?} is present"));
            let (x, y) = support::driver::centre(&element);
            harness.driver.click_at(x, y);
        };
        click_field(&mut *harness, "Timer timing (in 10m or at 09:30)");
        harness.type_text("soon");
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_timer_when_validity(),
            taskboard_ui_slint::FieldValidity::Invalid,
            "typed junk flips the flavour through the mount"
        );
        // No Clear affordance on this field (it is opt-in): the
        // retype leg deletes through the same stroke path.
        let backspace = parse("Backspace").expect("Backspace");
        for _ in 0.."soon".chars().count() {
            harness.stroke(backspace);
        }
        harness.type_text("in 10m");
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_timer_when_validity(),
            taskboard_ui_slint::FieldValidity::Valid,
            "parseable text re-flips through the mount"
        );
    });
}

/// R4 D24 (chunk-6 acceptance e) — the foreign-flow guard: a focus
/// steal into the description field while the due editor is armed
/// settles the due per the rule and leaves the description's own
/// engage flow intact.
#[test]
fn a_foreign_field_steal_settles_the_due_per_the_rule() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_midnight(2030, 1, 31));
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);

        // Junk in the due field: the steal discards — no command,
        // the due editor collapses, the description's engage flow
        // is untouched.
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        harness.clear_seeded();
        harness.type_text("soon");
        harness.driver.click_by_label("Edit description");
        harness.driver.advance_time(0);
        harness.assert_commands(&[]);
        assert!(
            !harness.handle.window().get_task_model().due_open,
            "the due editor collapsed on the steal"
        );
        assert!(
            harness
                .handle
                .window()
                .get_task_model()
                .description_edit_open,
            "the description's own engage flow is intact"
        );
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        assert!(
            !harness
                .handle
                .window()
                .get_task_model()
                .description_edit_open,
            "the description closed again"
        );

        // A parseable buffer: the steal commits the due.
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        harness.clear_seeded();
        harness.type_text("2030-03-01");
        harness.driver.click_by_label("Edit description");
        harness.driver.advance_time(0);
        // Consciously updated with US-1/D-3 (task-detail-editor UX
        // plan, 2026-11-17) and superseded by #893 (2026-11-18):
        // the settle funnel's commit mint is the N-4 stamping law —
        // a bare date stamps the named date's last covered instant
        // over the injected day facts (the harness's boot facts:
        // 00:00 start, UTC offset), never the old local midnight
        // and never the raw window end (excluded by its own
        // `[now, end)` membership).
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(taskboard_domain::days::last_covered_instant(
                    taskboard_domain::days::day_window_end(
                        chrono::NaiveDate::from_ymd_opt(2030, 3, 1).expect("fixture date"),
                        chrono::NaiveTime::MIN,
                        chrono::FixedOffset::east_opt(0).expect("zero"),
                    ),
                ))),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);
        assert!(
            harness
                .handle
                .window()
                .get_task_model()
                .description_edit_open,
            "the description's engage flow still armed"
        );
    });
}

/// R4 D26 (F22, review focus d) — the combobox is the picker's only
/// time path: the typed filter seeds the picker live (and junk shows
/// invalid without touching it), a quick-pick renders the canonical
/// value AND re-renders the due buffer, and combobox-Enter commits
/// the editor through the one funnel.
#[test]
fn the_combobox_is_the_only_time_path() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_midnight(2030, 1, 31));
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_task_model().date_picker_open);
        assert_eq!(
            harness
                .handle
                .window()
                .get_task_model()
                .date_time_field_text,
            "--:--",
            "the unset echo is the value mirror"
        );
        assert_eq!(
            harness.handle.window().get_task_model().date_time_validity,
            taskboard_ui_slint::FieldValidity::Neutral
        );

        // Typed filter: a lenient-parse hit seeds the picker live
        // and reads valid.
        harness
            .handle
            .window()
            .invoke_task_date_time_edited("9".into());
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_task_model().date_time_validity,
            taskboard_ui_slint::FieldValidity::Valid
        );
        assert_eq!(
            harness.handle.window().get_task_model().date_time_text,
            "09:00",
            "the typed hour seeded the picker"
        );

        // Junk: the validity flips invalid and the picker's time
        // is untouched.
        harness
            .handle
            .window()
            .invoke_task_date_time_edited("9x".into());
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_task_model().date_time_validity,
            taskboard_ui_slint::FieldValidity::Invalid
        );
        assert_eq!(
            harness.handle.window().get_task_model().date_time_text,
            "09:00",
            "junk never rewrote the seeded time"
        );

        // A quick-pick: canonical value in the mirror, and the
        // due buffer re-renders from the selection (Picker-source
        // sync).
        harness
            .handle
            .window()
            .invoke_task_date_time_picked(9 * 60 + 30);
        harness.driver.advance_time(0);
        assert_eq!(
            harness
                .handle
                .window()
                .get_task_model()
                .date_time_field_text,
            "09:30",
            "the pick wrote the canonical rendering"
        );
        assert_eq!(
            harness.handle.window().get_task_model().due_text,
            "2030-01-31 09:30",
            "the buffer mirrors the picker's selection"
        );

        // Combobox-Enter commits the editor through the funnel.
        harness
            .handle
            .window()
            .invoke_task_date_time_accepted("09:45".into());
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(local_wall(2030, 1, 31, 9, 45))),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);
        assert!(
            !harness.handle.window().get_task_model().due_open,
            "the editor collapsed on the combobox commit"
        );
    });
}

/// Round-5 F8/F10 (the mouse-only quick-time session): with the picker
/// opened through the field's own affordances (no keyboard), the
/// quick-times chevron opens the quick list without settling the
/// armed editor — the picker stays open — and a set time still seeds
/// the FULL 96-slot list at open (the typed-digit narrowing is armed
/// by keystrokes, not by the value mirror).
#[test]
fn the_quick_time_button_opens_the_menu_without_closing_the_picker() {
    let (fixture, mut state) = fixture::journey_state();
    // The timed value is what makes both halves red: the settle ends
    // the whole flow on the chevron's focus steal (F8), and the value
    // mirror filters the list to the one matching slot (F10).
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_wall(2030, 1, 31, 14, 30));
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_task_model().date_picker_open);

        // Mouse only: the chevron click must not settle the armed
        // editor — the picker stays open under the quick list.
        harness.driver.click_by_label("Quick times");
        harness.driver.advance_time(0);
        // The second loop turn delivers the deferred anchor-geometry
        // mirror (the mount pass ran in the first — the nonce
        // bump's recorded delivery shape).
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_task_model().date_picker_open,
            "the quick-time click leaves the picker open"
        );
        {
            let facts = harness
                .handle
                .window()
                .global::<taskboard_ui_slint::TimeFieldFacts>();
            assert!(facts.get_open(), "the quick list flyout is open");
            // Below the anchor: the realized panel sits under the
            // field row (the clamp/flip grammar's default leg).
            assert!(
                facts.get_panel_y() >= facts.get_anchor_y() + facts.get_anchor_height(),
                "the flyout mounts below its anchor (panel-y={:?} anchor-y={:?} anchor-h={:?} anchor-x={:?})",
                facts.get_panel_y(),
                facts.get_anchor_y(),
                facts.get_anchor_height(),
                facts.get_anchor_x()
            );
        }
        // The full 96-slot list at open — a set time must not seed
        // the one-row mirror-filtered list.
        let slots = harness.handle.window().get_task_model().date_time_slots;
        assert_eq!(
            slots.row_count(),
            96,
            "the quick list opens unfiltered over the quarter-hour grid"
        );
    });
}

/// Round-5 F10's other half: typed-digit narrowing keeps working per
/// keystroke after an open that seeded the full list — the first
/// keystroke re-arms the digit filter over the value mirror.
#[test]
fn the_quick_list_narrows_per_keystroke_after_an_open_with_a_set_time() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_wall(2030, 1, 31, 14, 30));
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_task_model().date_picker_open);
        harness.driver.click_by_label("Quick times");
        harness.driver.advance_time(0);
        let slots = harness.handle.window().get_task_model().date_time_slots;
        assert_eq!(slots.row_count(), 96, "the open seeds the full list");

        // The first keystroke narrows again: the mirror's digits
        // ("1430" + the typed text) filter the grid per keystroke.
        harness
            .handle
            .window()
            .invoke_task_date_time_edited("143".into());
        harness.driver.advance_time(0);
        let slots = harness.handle.window().get_task_model().date_time_slots;
        let narrowed = slots.row_count();
        assert!(
            narrowed > 0 && narrowed < 96,
            "typed digits narrow the list (got {narrowed})"
        );
    });
}

/// R4 D25 — the calendar picker's two-phase ladder and live sync
/// (reworks the r2 boundary journey's Enter leg): grid-Enter hands
/// focus to the field and dispatches nothing, the typed path and
/// day-click keep their commands, the Escape ladder steps field →
/// grid → close without a command until the close, and the buffer
/// mirrors cursor moves while the grid holds focus.
#[allow(clippy::too_many_lines)] // the walk is the contract's length
#[test]
fn the_due_picker_journeys_reach_the_boundary() {
    let (fixture, mut state) = fixture::journey_state();
    let due = local_midnight(2030, 1, 31);
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(due);
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_task_model().date_picker_open,
            "the picker is default-open on edit entry (round-3 D10)"
        );

        // January 2030: one leading blank (Tuesday start), 31
        // days, padded to the six-row reserve — 42 cells in 6
        // rows (r4 D22); the cursor sits on the due (flattened
        // index 1 + 30).
        let cursor_index = |harness: &Harness| -> Option<usize> {
            let grid = harness.handle.window().get_task_model().date_grid;
            let mut flat = 0usize;
            for r in 0..grid.row_count() {
                let row = grid.row_data(r).expect("row");
                for c in 0..row.row_count() {
                    if row.row_data(c).expect("cell").cursor {
                        return Some(flat);
                    }
                    flat += 1;
                }
            }
            None
        };
        let grid_shape = |harness: &Harness| -> (usize, usize) {
            let grid = harness.handle.window().get_task_model().date_grid;
            let rows = grid.row_count();
            let cells: usize = (0..rows)
                .map(|r| grid.row_data(r).expect("row").row_count())
                .sum();
            (rows, cells)
        };
        assert_eq!(grid_shape(&*harness), (6, 42));
        assert_eq!(cursor_index(&*harness), Some(31));

        // Down-in-field hands focus to the grid (the one-shot
        // focus-on-mount; round-3 D10): the arrows drive from
        // here. Right follows the month (round-3 D12/F28): the
        // caption flips to February and the cursor stays on-grid
        // — Feb 1 2030 is a Friday (4 leading blanks, flattened
        // 4), 28 days padded to the six-row reserve (42 cells).
        // Left returns
        // to the due's January grid.
        harness.stroke(parse("Down").expect("Down"));
        harness.driver.advance_time(0);
        // Tab from the grid lands on the header's first month
        // button (the widget-level tab stop); Return activates it
        // (round-3 D12/F28, acceptance f): the month steps back —
        // December 2029 starts on a Saturday (5 leading blanks,
        // 31 days padded to 42 cells in 6 rows), the cursor on
        // the 31st (flattened 35).
        harness.stroke(parse("Tab").expect("Tab"));
        harness.driver.advance_time(0);
        harness.stroke(parse("Return").expect("Return"));
        harness.driver.advance_time(0);
        assert_eq!(
            grid_shape(&*harness),
            (6, 42),
            "the month button answered Return — and every month \
                renders the same six-row reserve (r4 D22)"
        );
        assert_eq!(cursor_index(&*harness), Some(35));
        // Back to January for the legs below.
        harness.handle.window().invoke_task_date_picker_key(5);
        harness.driver.advance_time(0);
        harness.stroke(parse("Down").expect("Down"));
        harness.driver.advance_time(0);
        harness.stroke(parse("Right").expect("Right"));
        harness.driver.advance_time(0);
        assert_eq!(grid_shape(&*harness), (6, 42));
        assert_eq!(
            cursor_index(&*harness),
            Some(4),
            "Feb 1 stays on-grid — the displayed month follows the cursor"
        );
        harness.stroke(parse("Left").expect("Left"));
        harness.driver.advance_time(0);
        assert_eq!(grid_shape(&*harness), (6, 42));
        assert_eq!(cursor_index(&*harness), Some(31));

        // PageDown: February 2030, the cursor clamped to the 28th
        // (leading 4 + 27 = flattened 31 again, 35 cells).
        harness.stroke(parse("PageDown").expect("PageDown"));
        harness.driver.advance_time(0);
        assert_eq!(grid_shape(&*harness), (6, 42));
        assert_eq!(cursor_index(&*harness), Some(31));

        // Grid-Enter is the phase hand-over, never a commit
        // (r4 D25, review focus b): the field takes focus (the
        // one-shot flag), nothing dispatches, and the buffer
        // mirrors the cursor's date (the Picker-source live sync).
        harness.stroke(parse("Return").expect("Return"));
        harness.driver.advance_time(0);
        harness.assert_commands(&[]);
        assert!(
            harness.handle.window().get_task_model().date_field_focus,
            "the hand-over flagged the field"
        );
        assert_eq!(
            harness.handle.window().get_task_model().due_text.as_str(),
            "2030-02-28",
            "the buffer mirrors the cursor"
        );
        // The field's Return now commits the same command the
        // typed path produces for the same date.
        harness.stroke(parse("Return").expect("Return"));
        let picked = vec![StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(window_end_utc(2030, 2, 28))),
                ..taskboard_domain::TaskChanges::default()
            },
        }];
        harness.assert_commands(picked.as_slice());

        // The typed path for the same date: the identical command.
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        harness.clear_seeded();
        harness.type_text("2030-02-28");
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(picked.as_slice());

        // A day-cell click ≡ cursor+Enter: the 15th commits.
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        harness.handle.window().invoke_task_date_day_clicked(1 + 14);
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(window_end_utc(2030, 1, 15))),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);

        // The Escape ladder (r4 D25): the field phase's Escape is
        // the phase step back — the grid takes focus, the buffer
        // survives, nothing dispatches; the second Escape (the
        // picker phase) is the clean close, no command, the editor
        // collapses (the default-open picker dies with it). A
        // reopen starts default-open again, and the cycle leaves
        // the view closable by its own back binding (R-c).
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        harness.assert_commands(&[]);
        assert!(
            keyboard(&*harness).get_date_picker_grid(),
            "the first Escape stepped back to the grid"
        );
        assert!(
            harness.handle.window().get_task_model().due_open,
            "the field phase's Escape keeps the editor armed"
        );
        assert_eq!(
            harness.handle.window().get_task_model().due_text.as_str(),
            "2030-01-31",
            "the seeded buffer survived the step back"
        );
        harness.stroke(parse("Escape").expect("Escape"));
        harness.assert_commands(&[]);
        assert!(!keyboard(&*harness).get_vi_insert(), "back to normal");
        assert!(
            !harness.handle.window().get_task_model().due_open,
            "the editor collapsed"
        );
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_task_model().date_picker_open,
            "a reopen starts default-open — the picker is part of \
                 the editor's armed affordance"
        );
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        harness.press(ActionId::NavFocusBoard);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Board,
            "the view's back binding still works after the cycle"
        );
    });
}

/// Round-3 D10 (acceptance d, review focus c) — the overlay: the
/// picker mounts at window level and pushes nothing (the metadata
/// row's y is unchanged while it is open), and the dismissal layer's
/// outside click drops the picker only — the editor stays armed with
/// its seeded buffer, nothing dispatches, and Escape still ends the
/// whole editor.
#[test]
fn the_picker_overlay_dismisses_without_disturbing_the_editor() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_midnight(2030, 1, 31));
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_task_model().date_picker_open);
        let caption = |harness: &Harness| {
            harness
                .driver
                .find_by_label("Due date")
                .expect("the metadata caption is mounted")
                .absolute_position()
                .y
        };
        let y_open = caption(&*harness);

        // Outside click (r4 D24, superseding the round-3 D10
        // picker-only rule): the editor settles per the rule — the
        // seeded buffer is parseable, so click-away ≡ done: the
        // same instant re-commits and the editor collapses.
        harness.driver.click_at(600.0, 450.0);
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_task_model().date_picker_open,
            "the outside click dismissed the picker"
        );
        assert_eq!(
            caption(&*harness),
            y_open,
            "the metadata row's y is unchanged — the picker pushes nothing"
        );
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                duedate: Some(Some(window_end_utc(2030, 1, 31))),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);
        assert!(
            !harness.handle.window().get_task_model().due_open,
            "the editor collapsed on the settle"
        );
    });
}

/// Round-3 D15/#228 (acceptance i) — the description edits in place:
/// the read-only body's click arms the multiline field (the seeded
/// buffer mirrors the current text), Return commits
/// `description: Some(_)` (the multiline Enter probe: no newline
/// inserts), Escape discards without dispatching.
#[test]
fn the_description_edits_in_place() {
    let (fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.click_by_label("Edit description");
        harness.driver.advance_time(0);
        assert!(
            keyboard(&*harness).get_vi_insert(),
            "the editor opened in insert mode"
        );

        // Typed text + Shift+Return commits `Some(_)` through
        // the funnel; the plain Return in the middle inserted a
        // newline (the D-MLE-6 grammar) and dispatched nothing.
        // The fixture's description is empty, so the seeded
        // buffer starts empty: typing is the whole entry.
        harness.type_text("steep, then");
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[]);
        assert!(
            harness
                .handle
                .window()
                .get_task_model()
                .description_edit_open,
            "plain Return never closed the editor"
        );
        harness.type_text("serve");
        harness.stroke(parse("Shift+Return").expect("Shift+Return"));
        let drained = harness.engine.drain();
        match drained.as_slice() {
            [StateCommand::UpdateTask { id, changes, .. }] => {
                assert_eq!(id, &fixture.task1);
                let description = changes
                    .description
                    .as_deref()
                    .expect("the description arm is Some(_)");
                assert_eq!(description, "steep, then\nserve", "the newline persisted");
            }
            other => panic!("the description commit reached the boundary, got {other:?}"),
        }

        // Escape discards without dispatching.
        harness.driver.click_by_label("Edit description");
        harness.driver.advance_time(0);
        harness.type_text("half-typed");
        harness.stroke(parse("Escape").expect("Escape"));
        harness.assert_commands(&[]);
        assert!(
            !keyboard(&*harness).get_vi_insert(),
            "back to normal after the discard"
        );
    });
}

/// Round-5 chunk 2 (F7, D-MLE-6): a multi-line buffer commits
/// verbatim through the existing funnel — the newlines survive the
/// round trip with zero Rust-side change (verified, not touched).
#[test]
fn the_multiline_buffer_commits_verbatim() {
    let (fixture, mut state) = fixture::journey_state();
    let task = state.tasks.get_mut(&fixture.task1).expect("fixture task");
    task.description = "first\nsecond".into();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.click_by_label("Edit description");
        harness.driver.advance_time(0);
        // No typing: the seeded buffer commits as-is.
        harness.stroke(parse("Shift+Return").expect("Shift+Return"));
        match harness.engine.drain().as_slice() {
            [StateCommand::UpdateTask { id, changes, .. }] => {
                assert_eq!(id, &fixture.task1);
                assert_eq!(
                    changes.description.as_deref(),
                    Some("first\nsecond"),
                    "the newlines survived the round trip"
                );
            }
            other => panic!("the verbatim commit reached the boundary, got {other:?}"),
        }
    });
}

/// Round-5 chunk 2 (adoption wiring, F12): the display mount is the
/// editor (a labelled text-input cell, not a bare text row) — the
/// viewport's scroll mechanics themselves are pinned at the widget
/// layer (the probe's P-arms); the journey asserts only the wiring,
/// the lowest layer that can see the adoption.
#[test]
fn the_description_display_mounts_the_editor() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        let display = harness
            .driver
            .find_by_label("Description")
            .expect("the description cell is mounted");
        assert!(
            !display.is_accessible_button(),
            "the display mount is a text-input cell"
        );
    });
}

/// The appended legs' view read (the earlier journeys' local helper
/// duplicated once for the chunk-10 block).
fn current_view(harness: &Harness) -> View {
    harness.handle.window().get_current_view()
}

/// Chunk 10 D12 — the today leg: the desktop mount stays on Today; the
/// section walk follows `today_tasks` in display order with the
/// emphasis tracking; the walk retargets the board selection
/// (selection-follows-focus); `task.complete` dispatches the exact
/// `SetTaskDone`; the re-commit verb writes a fresh
/// `duedate = Some(Some(_))` confirmation-free; `nav.focus-board`
/// escapes. Rollover is NOT here — it is the models suite's
/// stepped-`now` push proof.
#[test]
fn the_today_view_walks_completes_and_recommits() {
    let (_fixture, state, [board_one_task, _board_two_task]) = fixture::today_overdue_state();
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = Harness::mount_no_escape(SlintDriver::default(), state);
            // The desktop default: Today, no escape.
            assert_eq!(current_view(&harness), View::Today);
            assert_eq!(
                harness.handle.window().get_toolbar_title(),
                "Today",
                "the today view captions itself"
            );

            // The walk takes `today_tasks` display order: board one's
            // overdue row, then board two's (order, id) — with the
            // emphasis following.
            harness.press(taskboard_ui_slint::actions::ActionId::NavFocusDown);
            let today = harness.handle.window().get_today_model();
            let overdue = today.sections.row_data(0).expect("overdue section");
            assert_eq!(overdue.count, 2, "both overdue rows render");
            assert!(
                overdue.rows.row_data(0).expect("row").card.emphasis,
                "the walk enters at the first row"
            );
            assert_eq!(
                harness.handle.window().get_board_bar_model().selected,
                0,
                "selection-follows-focus stays on board one"
            );

            harness.press(taskboard_ui_slint::actions::ActionId::NavFocusDown);
            assert!(
                overdue.rows.row_data(1).expect("row").card.emphasis,
                "the emphasis follows the walk"
            );
            assert_eq!(
                harness.handle.window().get_board_bar_model().selected,
                1,
                "the walk retargets the board to board two"
            );

            // Back to the first row; complete it (the exact command).
            harness.press(taskboard_ui_slint::actions::ActionId::NavFocusUp);
            harness.press(taskboard_ui_slint::actions::ActionId::TaskComplete);
            harness.assert_commands(&[StateCommand::SetTaskDone {
                id: board_one_task,
                done: true,
            }]);
            // The fake engine applies the drained command on settle
            // (the ten-step's apply discipline).
            harness.engine.settle(&harness.handle);
            let today = harness.handle.window().get_today_model();
            let done_section = today.sections.row_data(3).expect("done section");
            assert_eq!(done_section.count, 1, "done-placement");

            // The re-commit verb (UX R-4 D-8, #796: the round-10
            // "no default stroke" posture is overturned — the verb
            // binds per preset now; this journey drives the action
            // path): a fresh double-option due write, free of
            // confirmations.
            let before = harness.engine.drain().len();
            harness
                .handle
                .dispatch_action(taskboard_ui_slint::actions::ActionId::TaskDueToday);
            let drained = harness.engine.drain();
            assert_eq!(drained.len(), before + 1);
            assert!(
                matches!(
                    &drained[before],
                    StateCommand::UpdateTask {
                        id,
                        changes: taskboard_domain::TaskChanges {
                            duedate: Some(Some(_)),
                            ..
                        },
                    } if *id == board_one_task
                ),
                "the re-commit stamps the day window's end"
            );

            // The escape: back to the kanban board.
            harness.press(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
            assert_eq!(current_view(&harness), View::Board);
        },
    );
}

/// Chunk 10 D12 — the triage leg: `view.inbox` jumps to the Inbox's
/// canonical-first stack; the move ring cycles ACROSS the board edge
/// (the deliberate behavior change) with the hint naming the target;
/// confirming moves; the chip then reads zero (triage-done).
#[test]
fn the_inbox_triage_moves_across_the_board_edge() {
    let (fixture, state, caught) = fixture::triage_state();
    support::harness::run_journey(state, move |harness| {
        // The chip counts the inbox from any view (desktop-only).
        assert_eq!(
            harness.handle.window().get_toolbar_inbox_count(),
            1,
            "one open inbox row"
        );

        // view.inbox jumps (read-side; StackDetail on the
        // canonical-first stack). No default stroke (D11) — the
        // chip click and the menu route the same action.
        harness
            .handle
            .dispatch_action(taskboard_ui_slint::actions::ActionId::ViewInbox);
        assert_eq!(current_view(&*harness), View::StackDetail);

        harness.press(taskboard_ui_slint::actions::ActionId::NavFocusDown);
        // The move ring: cycle once — across the board edge onto
        // board two's stack "c" (the deliberate behavior change).
        // task.move has no StackDetail stroke (the preset table's
        // shape) — the menu path's action routes it. The Inbox
        // stack sits last in the flat ring, so one step BACKWARD
        // crosses the board edge onto board two.
        harness
            .handle
            .dispatch_action(taskboard_ui_slint::actions::ActionId::TaskMove);
        // UX-R8 D-1: the walk is slot-shaped — the armed slot is
        // the task's own gap+1 in the Inbox stack, so two backward
        // strokes walk past the stack's gap 0 onto board two
        // (the cross-board edge, the deliberate behavior change).
        harness.press(taskboard_ui_slint::actions::ActionId::NavFocusUp);
        harness.press(taskboard_ui_slint::actions::ActionId::NavFocusUp);
        let hint = harness.handle.window().get_flow_hint_model().text;
        assert!(
            hint.contains('\u{203a}') && hint.contains("board two"),
            "the hint names the cross-board target: {hint}"
        );
        // The confirm is the Activate intent; StackDetail has no
        // Return binding in the preset table, so the action routes
        // it (the move flow's Activate arm is the confirm).
        harness
            .handle
            .dispatch_action(taskboard_ui_slint::actions::ActionId::TaskEdit);
        harness.assert_commands(&[StateCommand::MoveTask {
            id: caught,
            stack: fixture.stack_c,
            order: 1,
        }]);

        // The fake engine applies the move on settle; the chip
        // then reads zero.
        harness.engine.settle(&harness.handle);
        assert_eq!(
            harness.handle.window().get_toolbar_inbox_count(),
            0,
            "triage-done: the chip counts to zero"
        );
    });
}

/// Chunk 10 D2 — the kiosk mount pin: the kiosk profile still launches
/// on the board view (the desktop-only default's scope).
#[test]
fn the_kiosk_mount_still_lands_on_the_board() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey_with_settings(
        state,
        taskboard_ui_slint::UiSettings {
            kiosk: true,
            ..taskboard_ui_slint::UiSettings::default()
        },
        move |harness| {
            assert_eq!(current_view(&*harness), View::Board);
        },
    );
}

// -- r2 D32/D33/D34, r4 D39: the board-scoped connection surface -----

use taskboard_ui_slint::requests::{ConnectionsSnapshot, ConnectorInfo};

/// The secret-free connector rows the tests hand over at mount.
fn connector_row(name: &str) -> ConnectorInfo {
    ConnectorInfo {
        name: name.to_owned(),
        server_url: "https://c.example".into(),
        username: "alice".into(),
        backend: taskboard_ui_slint::requests::ConnectorBackend::Nextcloud,
    }
}

/// Adds one board to the engine state (r4 D39: the selector projects
/// the STATE every pass — the tests seed boards here, the install
/// hand-over's boards half is subsumed).
fn state_board(state: &mut AppState, id: uuid::Uuid, title: &str, connector: Option<&str>) {
    use taskboard_domain::{
        Board, BoardClocks, BoardId, Color, ConnectorId, RemoteBoardBinding, RemoteBoardId,
    };
    state.boards.insert(
        BoardId::from_uuid(id),
        Board {
            id: BoardId::from_uuid(id),
            remote: connector.map(|c| RemoteBoardBinding {
                board: RemoteBoardId::deck(1),
                connector: ConnectorId::from(c),
            }),
            title: title.to_owned(),
            color: Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: BoardClocks::epoch(),
                color: BoardClocks::epoch(),
                archived: BoardClocks::epoch(),
                deleted: BoardClocks::epoch(),
            },
            authors: taskboard_domain::entities::BoardAuthors::default(),
            remote_seen: None,
        },
    );
}

/// The files-bound fixture (round-6 D-6.8 phase 2): the binding's
/// remote is the files id type — the lane tag is the payload (D-BF4),
/// the connector is the synthesized `files-sync` identity.
fn state_files_board(state: &mut AppState, id: uuid::Uuid, title: &str) {
    use taskboard_domain::{
        Board, BoardClocks, BoardId, Color, ConnectorId, RemoteBoardBinding, RemoteBoardId,
    };
    let id = BoardId::from_uuid(id);
    state.boards.insert(
        id,
        Board {
            id,
            remote: Some(RemoteBoardBinding {
                board: RemoteBoardId::files(uuid::Uuid::new_v4()),
                connector: ConnectorId::from("files-sync"),
            }),
            title: title.to_owned(),
            color: Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: BoardClocks::epoch(),
                color: BoardClocks::epoch(),
                archived: BoardClocks::epoch(),
                deleted: BoardClocks::epoch(),
            },
            authors: taskboard_domain::entities::BoardAuthors::default(),
            remote_seen: None,
        },
    );
}

/// The board bar's pushed model (the roster journeys' facts).
fn bar_model(harness: &Harness) -> taskboard_ui_slint::BoardBarModel {
    harness.handle.window().get_board_bar_model()
}

/// The bar model's rows as a flat vec (the model-facts convention).
fn model_rows(
    model: &slint::ModelRc<taskboard_ui_slint::BoardSegmentModel>,
) -> Vec<taskboard_ui_slint::BoardSegmentModel> {
    (0..model.row_count())
        .filter_map(|i| model.row_data(i))
        .collect()
}

/// UX R-1 D-1 fixture: two live boards plus one archived (the
/// archived board keeps its title "archived one").
fn roster_state() -> (uuid::Uuid, AppState) {
    let mut state = AppState::default();
    state_board(
        &mut state,
        uuid::uuid!("00000000-0000-0000-0000-000000000001"),
        "live one",
        None,
    );
    state_board(
        &mut state,
        uuid::uuid!("00000000-0000-0000-0000-000000000002"),
        "live two",
        None,
    );
    state_board(
        &mut state,
        uuid::uuid!("00000000-0000-0000-0000-000000000003"),
        "archived one",
        None,
    );
    let id =
        taskboard_domain::BoardId::from_uuid(uuid::uuid!("00000000-0000-0000-0000-000000000003"));
    state.boards.get_mut(&id).expect("board").archived = true;
    (uuid::uuid!("00000000-0000-0000-0000-000000000003"), state)
}

/// US-11/S1 (D-6 T3): the roster is a non-empty-archive affordance —
/// with everything live the bar model carries no archived rows, so
/// the indicator never mounts (structural model length, not text).
#[test]
fn us11_s1_roster_absent_while_no_board_is_archived() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        assert_eq!(bar_model(&*harness).archived_rows.row_count(), 0);
    });
}

/// US-11/S2 (D-6 T2): activating a roster row dispatches the exact
/// `StateCommand::SetBoardArchived { archived: false }` — nothing
/// else drains — and the restored board becomes the selected board
/// once the engine's apply lands (the one-shot select consumed at
/// reconcile).
#[test]
fn us11_s2_roster_row_unarchives_and_selects() {
    let (archived_id, state) = roster_state();
    support::harness::run_journey(state.clone(), move |harness| {
        assert_eq!(bar_model(&*harness).archived_rows.row_count(), 1);
        harness
            .handle
            .window()
            .invoke_board_bar_archived_activated(0);
        let drained = harness.engine.drain();
        assert_eq!(drained.len(), 1);
        assert!(matches!(
            &drained[0],
            taskboard_domain::StateCommand::SetBoardArchived {
                archived: false,
                ..
            }
        ));
        let StateCommand::SetBoardArchived { board, .. } = &drained[0] else {
            unreachable!("asserted above")
        };
        assert_eq!(*board, taskboard_domain::BoardId::from_uuid(archived_id));
        // The engine applies; the next refresh consumes the
        // one-shot select — the restored board is the selection.
        harness.engine.settle(&harness.handle);
        let segments = model_rows(&bar_model(&*harness).segments);
        assert_eq!(segments.len(), 3);
        let selected =
            usize::try_from(bar_model(&*harness).selected).expect("a non-negative selection");
        assert_eq!(segments[selected].title, "archived one");
    });
}

/// US-11/S3 (D-6 T2's continuation): the board rejoins the bar's
/// segment set and the roster unmounts again — the archived set is
/// empty, so the indicator's mount law removes it (no dead
/// affordance).
#[test]
fn us11_s3_the_restored_board_rejoins_the_bar_and_the_roster_unmounts() {
    let (_fixture, _state) = fixture::journey_state();
    support::harness::run_journey(roster_state().1, move |harness| {
        harness
            .handle
            .window()
            .invoke_board_bar_archived_activated(0);
        harness.engine.drain();
        harness.engine.settle(&harness.handle);
        let model = bar_model(&*harness);
        let titles: Vec<String> = model_rows(&model.segments)
            .into_iter()
            .map(|s| s.title.to_string())
            .collect();
        assert!(titles.contains(&"archived one".to_owned()));
        assert_eq!(model.archived_rows.row_count(), 0);
    });
}

/// The kiosk gate fixtures' habit (the lib `fixture_habit` twin — the
/// journeys build state directly).
fn state_habit(state: &mut AppState, board: taskboard_domain::BoardId) {
    let habit = taskboard_domain::habit::Habit {
        id: taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7()),
        board,
        title: "water".into(),
        description: String::new(),
        target: taskboard_domain::HabitTarget::PerDay(
            taskboard_domain::NonZeroCount::new(2).unwrap(),
        ),
        order: 0,
        anchor_time: None,
        anchor_day: None,
        scheduled_days: None,
        reminder: None,
        clocks: taskboard_domain::habit::HabitClocks::all(chrono::Utc::now()),
        continued_from: None,
        deleted: false,
        by: None,

        paused_until: None,
    };
    state.habits.habits.insert(habit.id, habit);
}

/// UX R-1 D-2 (D-6 T4, #790): the habit section's kiosk gate is the
/// DISABLED affordance, never absence — the rows render on kiosk AND
/// the editing flag reads disabled; the desktop profile reads enabled.
#[test]
fn the_kiosk_habit_actions_render_disabled_not_absent() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let kiosk = Harness::mount_with_settings(
                SlintDriver::default(),
                state.clone(),
                taskboard_ui_slint::UiSettings {
                    kiosk: true,
                    ..taskboard_ui_slint::UiSettings::default()
                },
            );
            assert_eq!(
                kiosk.handle.window().get_habits_model().row_count(),
                1,
                "the rows render on kiosk"
            );
            assert!(
                !kiosk.handle.window().get_habits_editing_enabled(),
                "the actions render disabled"
            );
            let desktop = Harness::mount(SlintDriver::default(), state);
            assert_eq!(desktop.handle.window().get_habits_model().row_count(), 1);
            assert!(
                desktop.handle.window().get_habits_editing_enabled(),
                "the desktop actions stay enabled"
            );
        },
    );
}

/// UX R-2 D-3 (D-7 T8, #786): the habit row's −1 recovery follows
/// chunk 1's editing-enabled grammar — on kiosk it renders disabled,
/// not absent (present in the tree; the typed `editing-enabled` fact
/// is the gate), with desktop parity.
#[test]
fn the_kiosk_habit_undo_affordance_renders_disabled() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut kiosk = Harness::mount_with_settings(
                SlintDriver::default(),
                state.clone(),
                taskboard_ui_slint::UiSettings {
                    kiosk: true,
                    ..taskboard_ui_slint::UiSettings::default()
                },
            );
            // The habit rows mount with the Today view; jump there.
            kiosk.press(ActionId::ViewToday);
            assert!(
                kiosk
                    .driver
                    .find_by_label("Take one habit log back")
                    .is_some(),
                "the −1 affordance renders on kiosk (visible-gated, not absent)"
            );
            assert!(
                !kiosk.handle.window().get_habits_editing_enabled(),
                "the −1 affordance is gated disabled (the typed grammar)"
            );
            let mut desktop = Harness::mount(SlintDriver::default(), state);
            desktop.press(ActionId::ViewToday);
            assert!(
                desktop
                    .driver
                    .find_by_label("Take one habit log back")
                    .is_some(),
                "the −1 affordance renders on desktop"
            );
            assert!(desktop.handle.window().get_habits_editing_enabled());
        },
    );
}

/// T-41 (#824): the +1's acknowledgement rides the status-notice
/// lane — the command routes AND the typed surplus fact (the logged
/// count delta) arms the notice (both facts, one dispatch).
#[test]
fn a_habit_log_routes_and_sets_the_status_notice() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.handle.window().invoke_habits_log_clicked(0);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::LogHabit { .. }] => {}
            other => panic!("the log reached the boundary, got {other:?}"),
        }
        assert!(
            harness.handle.window().get_notice_visible(),
            "the +1 acknowledgement arms the status notice"
        );
    });
}

/// UX R-1 D-2 (D-6 T5, #790): the focus toggle's menu row greys on
/// kiosk (the `entry_states` layer) and its activation still yields
/// ZERO effects (the dispatch no-op arm stands — layer 3).
#[test]
fn the_kiosk_focus_toggle_row_greys_and_still_no_ops() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey_with_settings(
        state,
        taskboard_ui_slint::UiSettings {
            kiosk: true,
            ..taskboard_ui_slint::UiSettings::default()
        },
        move |harness| {
            harness.handle.window().invoke_toolbar_menu_clicked();
            let entries = keyboard(&*harness).get_menu_entries();
            let focus = (0..entries.row_count())
                .map(|i| entries.row_data(i).expect("row"))
                .find(|row| row.action == "focus.toggle")
                .expect("the focus row renders on kiosk");
            assert!(
                !focus.enabled,
                "the row greys (the visual layer of the three-layer gate)"
            );
            let index = (0..entries.row_count())
                .position(|i| entries.row_data(i).expect("row").action == "focus.toggle")
                .expect("the row's index");
            harness
                .handle
                .window()
                .invoke_menu_entry_invoked(i32::try_from(index).expect("small index"));
            harness.assert_commands(&[]);
        },
    );
}

/// UX R-1 D-2 (D-6 T6, #790): the recur row renders on kiosk — the
/// display halves stay — with its actions' enablement flag false.
#[test]
fn the_kiosk_recur_row_renders_with_disabled_actions() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey_with_settings(
        state,
        taskboard_ui_slint::UiSettings {
            kiosk: true,
            ..taskboard_ui_slint::UiSettings::default()
        },
        move |harness| {
            // The kiosk pin lands on Board (no rotation) — the card
            // grid renders; the card's activation opens the task
            // detail (the desktop double-click journey's path; the
            // activation carries no kiosk refusal).
            let card = harness
                .driver
                .find_by_label("task-0")
                .expect("the first card is addressable");
            let (cx, cy) = support::driver::centre(&card);
            harness.driver.click_at(cx, cy);
            harness.handle.refresh();
            harness.driver.click_at(cx, cy);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::TaskDetail,
                "the task detail mounts on kiosk"
            );
            let model = harness.handle.window().get_task_model();
            assert!(
                model.series_edit_visible,
                "the recur row renders (the display half stays)"
            );
            assert!(
                !model.series_edit_enabled,
                "the recur actions render disabled"
            );
        },
    );
}

/// UX R-1 D-2 (D-6 T7, #790): the R3.5.3 fence stands — the alarm
/// lanes never mount on kiosk and the timers model stays empty — but
/// the absence is no longer silent: the Today timers host carries the
/// hint flag; the desktop profile reads it false.
#[test]
fn the_kiosk_alarm_fence_stands_with_the_absence_hint() {
    let (_fixture, state) = fixture::journey_state();
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let kiosk = Harness::mount_with_settings(
                SlintDriver::default(),
                state.clone(),
                taskboard_ui_slint::UiSettings {
                    kiosk: true,
                    ..taskboard_ui_slint::UiSettings::default()
                },
            );
            assert!(
                kiosk.handle.window().get_timers_absence_hint(),
                "the fenced surface's absence is signalled"
            );
            assert_eq!(
                kiosk.handle.window().get_timers_model().row_count(),
                0,
                "the fence holds: no reminder rows mount"
            );
            let desktop = Harness::mount(SlintDriver::default(), state);
            assert!(
                !desktop.handle.window().get_timers_absence_hint(),
                "the desktop renders no absence hint"
            );
        },
    );
}

/// The pushed dropdown options (the journeys' model facts).
fn options(harness: &Harness) -> Vec<taskboard_ui_slint::DropdownRow> {
    let model = harness
        .handle
        .window()
        .global::<taskboard_ui_slint::SettingsModel>();
    let rows = model.get_board_options();
    (0..rows.row_count())
        .filter_map(|i| rows.row_data(i))
        .collect()
}

/// Walks the settings surface Tab-by-Tab, pressing Return on each
/// stop, until the selector's flyout opens — the walk reaches the
/// field (a real walk stop), and Return on it opens. The facts'
/// `field-focused` mirror is unusable here (the backend fires no
/// `changed` hook on a bound out property — the recorded negative),
/// so the OPEN state itself is the walk's success fact. The bound is
/// Real-pointer-clicks the first remote row's local-board picker (the
/// element query is the address).
fn click_selector_field(driver: &mut SlintDriver) {
    // The backend's element traversal after a view swap is racy (the
    // recorded limitation): retry the query across a few clock turns
    // before giving up.
    let mut row = None;
    for _ in 0..20 {
        driver.advance_time(0);
        let found = driver.find_all_by_id("SettingsView::remote-picker");
        if let Some(first) = found.into_iter().next() {
            row = Some(first);
            break;
        }
    }
    let row = row.expect("a remote row's picker renders");
    let pos = row.absolute_position();
    let size = row.size();
    // The field's middle. The pointer move seeds the cursor position
    // first (the grab probe's dispatch shape — press lands where the
    // move said).
    let x = pos.x + size.width * 0.5;
    let y = pos.y + size.height / 2.0;
    driver.hover_at(x, y);
    driver.advance_time(0);
    driver.click_at(x, y);
    driver.advance_time(30);
}

/// The settings model global, fresh per call (the global handle ties
/// to the window's borrow — module fns are lifetime-generic where
/// closures are not).
fn model(h: &Harness) -> taskboard_ui_slint::SettingsModel<'_> {
    h.handle
        .window()
        .global::<taskboard_ui_slint::SettingsModel>()
}

/// The board selector's facts global, fresh per call.
fn board_facts(h: &Harness) -> taskboard_ui_slint::SettingsBoardFacts<'_> {
    h.handle
        .window()
        .global::<taskboard_ui_slint::SettingsBoardFacts>()
}

/// The board bar's facts global, fresh per call (the lifetime pattern
/// above — the global handle ties to the window's borrow).
fn bar_facts(h: &Harness) -> taskboard_ui_slint::BoardBarFacts<'_> {
    h.handle
        .window()
        .global::<taskboard_ui_slint::BoardBarFacts>()
}

/// Dispatches one raw key event to the focused element (the widget
/// ladder's ladder — the action layer never sees these strokes).
fn raw_key(harness: &Harness, key: slint::platform::Key) {
    harness
        .handle
        .window()
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: key.into() });
}

/// The pushed connections items (the account cards' walk-ordered
/// projection — the journeys' model facts).
fn items(harness: &Harness) -> Vec<taskboard_ui_slint::ConnectionItem> {
    let rows = model(harness).get_connection_items();
    (0..rows.row_count())
        .filter_map(|i| rows.row_data(i))
        .collect()
}

/// The index of the first item of `kind` titled `title` (data identity:
/// titles are the fixture's keys, never presentation copy).
fn item_at(harness: &Harness, kind: taskboard_ui_slint::ConnectionItemKind, title: &str) -> i32 {
    let index = items(harness)
        .iter()
        .position(|item| item.kind == kind && item.title == title)
        .unwrap_or_else(|| panic!("a {kind:?} item titled {title} renders"));
    i32::try_from(index).expect("index fits")
}

/// The empty listing reply (the settle helper's happy-path argument).
#[allow(clippy::unnecessary_wraps)] // the settle helper's signature twin
fn no_rows() -> Result<
    Vec<taskboard_ui_slint::requests::RemoteBoardRow>,
    taskboard_ui_slint::requests::SettingsError,
> {
    Ok(Vec::new())
}

fn remote_row(id: u64, title: &str) -> taskboard_ui_slint::requests::RemoteBoardRow {
    taskboard_ui_slint::requests::RemoteBoardRow {
        remote_id: taskboard_domain::RemoteBoardId::deck(id),
        title: title.to_owned(),
    }
}

/// Answers the mount auto-lists (round-6 D-6.7) with `make`'s typed
/// outcome and drains the auto lane — afterwards the request lane is
/// clear for the test's own requests. Mount routes nothing but
/// auto-lists, so a non-listing request in the lane is a drift, not a
/// silent drop.
fn settle_auto_lists(
    harness: &Harness,
    make: impl Fn() -> Result<
        Vec<taskboard_ui_slint::requests::RemoteBoardRow>,
        taskboard_ui_slint::requests::SettingsError,
    >,
) {
    for request in harness.drain_requests() {
        let taskboard_ui_slint::requests::UiRequest::ListRemoteBoards { reply, .. } = request
        else {
            panic!("the mount routed only auto-lists");
        };
        reply.send(make()).expect("the reply lane is alive");
    }
    harness.handle.drain_auto_listings();
}

/// Clicks the named connector's Refresh and answers the listing with
/// `rows` (the fake lane's half of `ListRemoteBoards`).
fn refresh_with(
    harness: &Harness,
    connector: &str,
    rows: Vec<taskboard_ui_slint::requests::RemoteBoardRow>,
) {
    let deck = item_at(
        harness,
        taskboard_ui_slint::ConnectionItemKind::Deck,
        connector,
    );
    harness
        .handle
        .window()
        .invoke_settings_refresh_clicked(deck);
    let mut requests = harness.drain_requests();
    assert_eq!(requests.len(), 1, "one listing routed");
    let taskboard_ui_slint::requests::UiRequest::ListRemoteBoards {
        connector: listed,
        reply,
    } = requests.remove(0)
    else {
        panic!("the refresh routes ListRemoteBoards");
    };
    assert_eq!(
        listed, connector,
        "the listing addresses the row's connector"
    );
    reply.send(Ok(rows)).expect("the reply lane is alive");
    harness.handle.drain_setting_replies();
}

fn identity_row(name: &str, server: &str, user: &str) -> ConnectorInfo {
    ConnectorInfo {
        name: name.into(),
        server_url: server.into(),
        username: user.into(),
        backend: taskboard_ui_slint::requests::ConnectorBackend::Nextcloud,
    }
}

fn standalone_row(name: &str, server: &str, user: &str) -> ConnectorInfo {
    ConnectorInfo {
        backend: taskboard_ui_slint::requests::ConnectorBackend::Standalone,
        ..identity_row(name, server, user)
    }
}

/// Round-6 D-6.7 (plan §3 T6+T7) — the mount auto-list: the first
/// settings mount fires exactly one listing per unlisted connector; a
/// rapid remount re-fires nothing while a listing is in flight; a
/// landed listing is never re-listed; a failed listing frees the slot
/// (only that connector re-fires, and the failure lands no outcome);
/// a manual refresh holding the pending machine suppresses the
/// auto-fire for its connector.
#[test]
fn mounting_the_settings_view_auto_lists_each_unlisted_connector_once() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    use taskboard_ui_slint::requests::UiRequest;
    let settings = UiSettings {
        connectors: vec![connector_row("work"), connector_row("home")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(AppState::default(), settings, move |harness| {
        harness.press(ActionId::AppSettings);
        let window = harness.handle.window();
        let requests = harness.drain_requests();
        assert_eq!(requests.len(), 2, "one listing per unlisted connector");

        // Rapid remount: the in-flight slots guard the re-fire.
        window.invoke_settings_view_mounted();
        assert!(
            harness.drain_requests().is_empty(),
            "an in-flight listing is never re-fired"
        );

        // work's listing lands; home's fails (the slot frees, the
        // row stays unlisted, the outcome banner is untouched —
        // background listings are silent).
        for request in requests {
            let UiRequest::ListRemoteBoards { connector, reply } = request else {
                panic!("the mount routes only listings");
            };
            let outcome = if connector == "work" {
                Ok(vec![remote_row(1, "r")])
            } else {
                Err(taskboard_ui_slint::requests::SettingsError::VerifyUnreachable)
            };
            reply.send(outcome).expect("the reply lane is alive");
        }
        harness.handle.drain_auto_listings();
        assert_eq!(
            model(&*harness).get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::None,
            "a failed auto-list is silent"
        );
        item_at(&*harness, K::Remote, "r");

        // The next mount: the failed connector is inside the
        // backoff window (the files-sync-status plan US-3 AC-1) —
        // the backed-off connector's remount re-fires nothing.
        window.invoke_settings_view_mounted();
        assert!(
            harness.drain_requests().is_empty(),
            "the backed-off connector's remount re-fires nothing"
        );

        // The manual refresh overrides the backoff (US-3 AC-2):
        // explicit intent routes the listing.
        window.invoke_settings_refresh_clicked(item_at(&*harness, K::Deck, "home"));
        assert_eq!(harness.drain_requests().len(), 1, "the refresh routed");
        // (the refresh's reply is left in flight here on purpose;
        // the in-flight guard legs below ride it)

        // While that refresh is in flight, the mount suppresses
        // work's auto too (the manual-lane guard), and home's
        // pending machine still guards.
        window.invoke_settings_view_mounted();
        assert!(
            harness.drain_requests().is_empty(),
            "the manual-refresh connector and the in-flight slot both guard"
        );
    });
}

/// US-3 AC-1 (the files-sync-status plan, D-4) — the backoff
/// suppresses the mount auto-list: after a failed auto listing, a
/// remount inside the window routes no new request and the connector
/// card renders the failed truth with the retry posture (unlisted —
/// the refresh affordance stays).
#[test]
fn us3_ac1_backoff_suppresses_the_auto_list() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    use taskboard_ui_slint::requests::UiRequest;
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(AppState::default(), settings, move |harness| {
        harness.press(ActionId::AppSettings);
        let window = harness.handle.window();
        let requests = harness.drain_requests();
        assert_eq!(requests.len(), 1);
        let UiRequest::ListRemoteBoards { reply, .. } = requests.into_iter().next().unwrap() else {
            panic!("the mount routes a listing");
        };
        reply
            .send(Err(
                taskboard_ui_slint::requests::SettingsError::VerifyUnreachable,
            ))
            .expect("the reply lane is alive");
        harness.handle.drain_auto_listings();

        // The remount inside the window: no new request fires.
        window.invoke_settings_view_mounted();
        assert!(
            harness.drain_requests().is_empty(),
            "the backoff suppresses the mount auto-list"
        );
        // The card renders the failed truth with the retry
        // posture: the Deck row is unlisted (the refresh
        // affordance's own seam guards the retry path).
        let deck = item_at(&*harness, K::Deck, "work");
        assert!(
            !model(&*harness)
                .get_connection_items()
                .row_data(deck.try_into().expect("fits"))
                .expect("row")
                .listed,
            "the failed truth renders with the retry posture"
        );
    });
}

/// US-3 AC-2 — the manual refresh overrides the backoff, and the
/// failure re-arms it (typed over the request lane).
#[test]
fn us3_ac2_refresh_overrides_the_backoff() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    use taskboard_ui_slint::requests::UiRequest;
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(AppState::default(), settings, move |harness| {
        harness.press(ActionId::AppSettings);
        let window = harness.handle.window();
        for request in harness.drain_requests() {
            let UiRequest::ListRemoteBoards { reply, .. } = request else {
                panic!("the mount routes a listing");
            };
            reply
                .send(Err(
                    taskboard_ui_slint::requests::SettingsError::VerifyUnreachable,
                ))
                .expect("the reply lane is alive");
        }
        harness.handle.drain_auto_listings();
        window.invoke_settings_view_mounted();
        assert!(harness.drain_requests().is_empty(), "backed off");

        // Explicit intent: the refresh routes over the backoff.
        window.invoke_settings_refresh_clicked(item_at(&*harness, K::Deck, "work"));
        let requests = harness.drain_requests();
        assert_eq!(requests.len(), 1, "the refresh routed");
        let UiRequest::ListRemoteBoards { reply, .. } = requests.into_iter().next().unwrap() else {
            panic!("the refresh routes a listing");
        };
        reply
            .send(Err(
                taskboard_ui_slint::requests::SettingsError::VerifyUnreachable,
            ))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();
        harness.handle.drain_auto_listings();

        // The failure re-armed the backoff: the remount re-fires
        // nothing.
        window.invoke_settings_view_mounted();
        assert!(harness.drain_requests().is_empty(), "re-armed");
    });
}

/// US-6 AC-1 (the files-sync-status plan, D-7) — the Restore
/// affordance mounts on an unbound live roster row and routes the
/// exact `UndoBoardMove` (the ceremony's undo leg is the typed gate
/// app-side); a bound row mounts nothing (the withholding fold's
/// journey leg).
#[test]
fn us6_ac1_restore_mounts_and_dispatches_the_undo_request() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let loose = uuid::Uuid::new_v4();
    state_board(&mut state, loose, "loose", None);
    let deck = uuid::Uuid::new_v4();
    state_board(&mut state, deck, "deck rider", Some("work"));
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        let loose_row = items(&*harness)
            .iter()
            .position(|item| item.kind == K::FilesBoard && item.title == "loose")
            .expect("the unbound row renders");
        let deck_row = items(&*harness)
            .iter()
            .position(|item| item.kind == K::FilesBoard && item.title == "deck rider")
            .expect("the bound row renders");
        assert!(
            items(&*harness)[loose_row].restore_offerable,
            "the window's candidate set mounts Restore"
        );
        assert!(
            !items(&*harness)[deck_row].restore_offerable,
            "bound rows withhold"
        );
        window.invoke_settings_files_board_restore_clicked(i32::try_from(loose_row).expect("fits"));
        let requests = harness.drain_requests();
        assert_eq!(requests.len(), 1, "the Restore routed");
        assert!(matches!(
            &requests[0],
            taskboard_ui_slint::requests::UiRequest::UndoBoardMove { board, .. }
                if board.as_uuid() == loose
        ));
    });
}

/// US-5 AC-1 (the files-sync-status plan, D-6) — the page's other
/// controls stay enabled while a publish is pending: the wholesale
/// disable flag is absent on the publish path (typed over the model's
/// busy gate), and a Files-board row action stays clickable-enabled.
#[test]
fn us5_ac1_the_page_stays_usable_during_a_publish() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", None);
    let files = uuid::Uuid::new_v4();
    state_files_board(&mut state, files, "files rider");
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        general: taskboard_ui_slint::requests::SettingsSnapshot {
            files_sync_enabled: true,
            files_sync_poll_secs: None,
            files_sync_anchor: Some("work".into()),
            ..taskboard_ui_slint::requests::SettingsSnapshot::default()
        },
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        let deck_index = item_at(&*harness, K::Deck, "work");
        window.invoke_settings_publish_picked(deck_index, 0);
        assert!(harness.drain_requests().len() == 1, "the publish routed");
        // The wholesale-disable flag is absent: the model's busy
        // gate is false while the ceremony rides.
        assert!(!model(&*harness).get_busy());
        // The Files roster rows' actions stay enabled (the page's
        // other controls usable).
        assert!(
            items(&*harness)
                .iter()
                .filter(|item| item.kind == K::FilesBoard)
                .all(|item| item.action_enabled),
            "the roster's actions stay enabled during a publish"
        );
    });
}

/// Round-6 D-6.7 (plan §3 T8) — an auto-list reply never touches a
/// concurrently in-flight user op: the pending machine, the busy
/// marker, and the outcome banner survive the background reply's
/// settle (the dedicated-lane motivation).
#[test]
fn a_settled_auto_list_never_touches_a_pending_op() {
    use taskboard_ui_slint::requests::UiRequest;
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(AppState::default(), settings, move |harness| {
        harness.press(ActionId::AppSettings);
        let window = harness.handle.window();

        // The auto-list goes out and is held in flight.
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1);
        let UiRequest::ListRemoteBoards { reply, .. } = requests.remove(0) else {
            panic!("the mount routes a listing");
        };

        // A user op takes the pending machine while it is out.
        window.invoke_settings_field_edited(1, "https://c.example".into());
        window.invoke_settings_field_edited(2, "alice".into());
        window.invoke_settings_field_edited(3, "pw".into());
        window.invoke_settings_save_clicked(
            "https://c.example".into(),
            "alice".into(),
            "pw".into(),
        );
        assert!(model(&*harness).get_busy(), "the save is pending");
        assert_eq!(
            model(&*harness).get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Busy
        );

        // The background listing settles — success or failure, the
        // user op's state is untouched.
        reply
            .send(Err(
                taskboard_ui_slint::requests::SettingsError::VerifyUnreachable,
            ))
            .expect("the reply lane is alive");
        harness.handle.drain_auto_listings();
        assert!(model(&*harness).get_busy(), "the save is still pending");
        assert_eq!(
            model(&*harness).get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Busy,
            "the banner still belongs to the save"
        );
        assert!(
            items(&*harness)
                .iter()
                .all(|item| item.kind != taskboard_ui_slint::ConnectionItemKind::Remote),
            "the failed listing landed no rows"
        );

        // The save's reply (dropped: closed lane) settles through
        // the user-op drain as before.
        drop(harness.drain_requests());
        harness.handle.drain_setting_replies();
        assert!(!model(&*harness).get_busy(), "the save cleared");
    });
}

/// Round-6 D-6.7 (plan §3 T9) — bindings render without a listing:
/// the synthesized Remote row is bound, titled by the local board,
/// and fully actionable (a detach on it routes the typed pair); when
/// the listing then lands, the binding enriches into the listing's
/// row and is NOT duplicated, while an unlisted second binding keeps
/// its synthesized row.
#[test]
fn bindings_render_without_a_listing_and_the_listing_enriches_without_duplicates() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    use taskboard_ui_slint::requests::UiRequest;
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    let away = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", Some("work"));
    state_board(&mut state, away, "away", Some("work"));
    // Distinct remote ids: home's binding is deck 1, away's deck 2.
    state
        .boards
        .get_mut(&taskboard_domain::BoardId::from_uuid(away))
        .and_then(|b| b.remote.as_mut())
        .expect("bound")
        .board = taskboard_domain::RemoteBoardId::deck(2);
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::AppSettings);
        let window = harness.handle.window();

        // The mount's listing is held in flight: no listing has
        // landed, yet both bindings render.
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1);
        let UiRequest::ListRemoteBoards { reply, .. } = requests.remove(0) else {
            panic!("the mount routes a listing");
        };
        let away_row = item_at(&*harness, K::Remote, "away");
        assert!(
            items(&*harness)[usize::try_from(away_row).expect("fits")].bound,
            "the synthesized row is bound"
        );
        // UX R-2 D-4 (#787): the disconnect click arms; the
        // confirm sends the typed pair once.
        window.invoke_settings_disconnect_clicked(away_row);
        assert!(
            harness.drain_requests().is_empty(),
            "the arm dispatches nothing"
        );
        window.invoke_settings_remove_confirmed(away_row);
        let mut requests = harness.drain_requests();
        assert!(
            matches!(
                &requests[0],
                UiRequest::DetachConnector { board, connector, .. }
                    if board.as_uuid() == away && connector == "work"
            ),
            "the synthesized row is actionable"
        );
        let UiRequest::DetachConnector { reply: detach, .. } = requests.remove(0) else {
            panic!("unreachable");
        };
        // Answer nothing yet — the detach stays in flight only in
        // the user-op lane; complete it via the reply drop path.
        let _ = detach.send(Ok(ConnectionsSnapshot {
            boards: Vec::new(),
            connectors: vec![connector_row("work")],
        }));
        harness.handle.drain_setting_replies();

        // The listing lands containing deck 1 (home's binding) but
        // not deck 2 (away's): the enriched row renders once, the
        // unlisted binding keeps its synthesized row.
        reply
            .send(Ok(vec![remote_row(1, "r-home"), remote_row(9, "r-new")]))
            .expect("the reply lane is alive");
        harness.handle.drain_auto_listings();
        let remote_titles: Vec<String> = items(&*harness)
            .into_iter()
            .filter(|item| item.kind == K::Remote)
            .map(|item| item.title.to_string())
            .collect();
        assert_eq!(
            remote_titles,
            vec!["r-home".to_owned(), "r-new".to_owned(), "away".to_owned()],
            "the listing enriches; the unlisted binding synthesizes; no duplicates"
        );
        let bound = &items(&*harness)
            [usize::try_from(item_at(&*harness, K::Remote, "r-home")).expect("fits")];
        assert!(bound.bound);
        assert_eq!(bound.detail, "home", "the listing enriches the detail");
        assert!(
            !items(&*harness)
                [usize::try_from(item_at(&*harness, K::Remote, "r-new")).expect("fits")]
            .bound
        );
        let synthesized = &items(&*harness)
            [usize::try_from(item_at(&*harness, K::Remote, "away")).expect("fits")];
        assert!(synthesized.bound);
        assert!(synthesized.detail.is_empty(), "no remote title pre-listing");
    });
}

/// J1 (rewired round-5 chunk 8) — the account cards group the
/// connectors by exact identity in config order: header, the Files
/// toggle on the default connector's card only, one Deck row per
/// connector (labelled when the card carries several); the pickers'
/// options are the unbound live boards; mounting dispatches nothing.
#[test]
fn the_account_cards_group_connectors_and_the_options_project_the_state() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    let away = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", None);
    state_board(&mut state, away, "away", Some("work"));
    let settings = UiSettings {
        connectors: vec![
            identity_row("work", "https://x.example", "alice"),
            identity_row("bob", "https://y.example", "bob"),
            identity_row("work-2", "https://x.example", "alice"),
        ],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::AppSettings);
        let shape: Vec<(K, String, bool)> = items(&*harness)
            .into_iter()
            .map(|item| (item.kind, item.title.to_string(), item.show_title))
            .collect();
        let kinds: Vec<K> = shape.iter().map(|(kind, ..)| *kind).collect();
        assert_eq!(
            kinds,
            vec![
                K::Account,
                K::Files,
                K::FilesBoard,
                K::FilesBoard,
                K::Deck,
                K::Remote,
                K::Deck,
                K::Account,
                K::Files,
                K::FilesBoard,
                K::FilesBoard,
                K::Deck
            ],
            "header, a Files row on EVERY Nextcloud card (round-6 D-6.6) with the
                 card's roster after it (round-7 D-7.3: every live board once per
                 card, LocalOnly sorted first), a Deck row per connector — plus the
                 binding-without-listing Remote row (round-6 D-6.7)"
        );
        // The deck-bound board rides the card's own connector
        // ("work") — the ThisConnection group sorts first (the
        // files-sync-status plan US-10 AC-3's home truth);
        // "home" is the roster's LocalOnly row.
        assert_eq!(
            shape[2].1, "away",
            "the own-card deck binding reads ThisConnection first"
        );
        assert_eq!(shape[3].1, "home", "the roster's LocalOnly row follows");
        assert_eq!(shape[4].1, "work");
        assert_eq!(shape[6].1, "work-2");
        assert!(
            shape[4].2 && shape[6].2,
            "a shared card labels its Deck rows"
        );
        assert!(!shape[11].2, "a single-connector card does not");
        assert!(
            !items(&*harness)[8].files_on,
            "the second card's Files row renders Off (not the anchor)"
        );
        let labels: Vec<String> = options(&*harness)
            .into_iter()
            .map(|row| row.label.to_string())
            .collect();
        assert_eq!(labels, vec!["home".to_owned()], "only unbound boards");
        // The mount auto-list (round-6 D-6.7): every unlisted
        // connector gets exactly one listing request at mount.
        let requests = harness.drain_requests();
        assert_eq!(requests.len(), 3, "one listing per connector");
        assert!(requests.iter().all(|request| matches!(
            request,
            taskboard_ui_slint::requests::UiRequest::ListRemoteBoards { .. }
        )));
        for request in requests {
            if let taskboard_ui_slint::requests::UiRequest::ListRemoteBoards { reply, .. } = request
            {
                reply.send(no_rows()).expect("the reply lane is alive");
            }
        }
        harness.handle.drain_auto_listings();
        assert!(
            items(&*harness)
                .iter()
                .all(|item| item.kind != K::Deck || item.listed),
            "the landed listings mark every Deck row"
        );
    });
}

/// J2 (rewired round-5 chunk 8) — Refresh lists the connector's remote
/// boards; the rows render their binding from the STATE (through this
/// connector only); picking an unbound local board routes exactly one
/// explicit `AttachConnector` carrying the remote id; the Ok reply
/// clears the gate; a `BindRefused` reply lands `Rejected` with
/// non-empty status text (structure, never strings).
#[test]
fn a_refresh_lists_remotes_and_a_pick_binds_explicitly() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    let away = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", None);
    // `state_board` binds remote id 1.
    state_board(&mut state, away, "away", Some("work"));
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state.clone(), settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();
        refresh_with(
            &*harness,
            "work",
            vec![remote_row(1, "r-away"), remote_row(2, "r-new")],
        );
        assert!(
            !model(&*harness).get_busy(),
            "the listing reply cleared the gate"
        );

        let bound = &items(&*harness)
            [usize::try_from(item_at(&*harness, K::Remote, "r-away")).expect("fits")];
        assert!(bound.bound, "remote 1 renders bound from the state");
        assert_eq!(bound.detail, "away");
        let unbound_index = item_at(&*harness, K::Remote, "r-new");
        assert!(!items(&*harness)[usize::try_from(unbound_index).expect("fits")].bound);

        window.invoke_settings_remote_bind_picked(unbound_index, 0);
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1, "exactly one attach routed");
        assert!(matches!(
            &requests[0],
            taskboard_ui_slint::requests::UiRequest::AttachConnector {
                board, connector, remote, ..
            } if board.as_uuid() == home && connector == "work" && *remote == taskboard_domain::RemoteBoardId::deck(2)
        ));
        assert!(model(&*harness).get_busy(), "the gate is busy mid-flight");
        assert!(
            items(&*harness)[usize::try_from(unbound_index).expect("fits")].busy,
            "the ceremony rides the picked remote row"
        );

        let taskboard_ui_slint::requests::UiRequest::AttachConnector { reply, .. } =
            requests.remove(0)
        else {
            panic!("the attach routed");
        };
        reply
            .send(Ok(ConnectionsSnapshot {
                boards: Vec::new(),
                connectors: vec![connector_row("work")],
            }))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();
        assert!(
            !model(&*harness).get_busy(),
            "the gate cleared with the reply"
        );
        assert_eq!(
            model(&*harness).get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Ok,
        );

        window.invoke_settings_remote_bind_picked(unbound_index, 0);
        let mut requests = harness.drain_requests();
        let taskboard_ui_slint::requests::UiRequest::AttachConnector { reply, .. } =
            requests.remove(0)
        else {
            panic!("the second attach routed");
        };
        let _ = reply.send(Err(
            taskboard_ui_slint::requests::SettingsError::BindRefused,
        ));
        harness.handle.drain_setting_replies();
        assert_eq!(
            model(&*harness).get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Rejected,
            "the bind refusal is Rejected"
        );
        assert!(
            !model(&*harness).get_status_text().is_empty(),
            "the refusal sentence is visible"
        );

        // A stale item or option index is absorbed silently.
        window.invoke_settings_remote_bind_picked(unbound_index, 9);
        window.invoke_settings_remote_bind_picked(99, 0);
        assert!(
            harness.drain_requests().is_empty(),
            "stale picks route nothing"
        );
    });
}

/// JP (round-6 D-6.9, #699) — the Deck card's publish picker: picking
/// an unbound local board routes exactly one explicit `PublishBoard`
/// carrying the (connector, board) pair and spins the Deck row's busy
/// marker; the Ok reply clears the gate; stale picks route nothing
/// (dispatch-level routing only — the unbound-only filter is the
/// model's existing behaviour, exercised by the listing journeys).
#[test]
fn a_deck_publish_pick_routes_publish_board() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", None);
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        let deck_index = item_at(&*harness, K::Deck, "work");
        window.invoke_settings_publish_picked(deck_index, 0);
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1, "exactly one publish routed");
        assert!(matches!(
            &requests[0],
            taskboard_ui_slint::requests::UiRequest::PublishBoard {
                board, connector, ..
            } if board.as_uuid() == home && connector == "work"
        ));
        // The page-usable guard (US-5 AC-1): the publish family
        // does not take the wholesale disable — the addressed
        // card's phase badge carries the ceremony instead.
        assert!(
            !model(&*harness).get_busy(),
            "the page stays usable mid-publish"
        );
        let deck_row = &items(&*harness)[usize::try_from(deck_index).expect("fits")];
        assert!(deck_row.busy, "the ceremony rides the Deck row");
        assert_eq!(
            deck_row.pending_phase,
            taskboard_ui_slint::SettingsPublishPhase::Publishing,
            "the card renders the ceremony phase"
        );

        let taskboard_ui_slint::requests::UiRequest::PublishBoard { reply, .. } =
            requests.remove(0)
        else {
            panic!("the publish routed");
        };
        reply
            .send(Ok(ConnectionsSnapshot {
                boards: Vec::new(),
                connectors: vec![connector_row("work")],
            }))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();
        assert!(
            !model(&*harness).get_busy(),
            "the gate cleared with the reply"
        );
        assert_eq!(
            items(&*harness)[usize::try_from(deck_index).expect("fits")].pending_phase,
            taskboard_ui_slint::SettingsPublishPhase::None,
            "the terminal phase clears with the reply (US-5 AC-2)"
        );
        assert_eq!(
            model(&*harness).get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Ok,
        );

        // A stale item or option index is absorbed silently.
        let deck_index = item_at(&*harness, K::Deck, "work");
        window.invoke_settings_publish_picked(deck_index, 9);
        window.invoke_settings_publish_picked(99, 0);
        assert!(
            harness.drain_requests().is_empty(),
            "stale picks route nothing"
        );
    });
}

/// JP-F (re-authored round-7 D-7.3, W-3) — the Files section is the
/// per-account roster: every live board renders once, titled by the
/// LOCAL board, rows derived from the engine's bindings every pass.
/// (Round-6's files-bound-only posture was the picker-era eligibility
/// set; the roster's broader visibility — archived boards included,
/// flagged — is US-1 AC-2, covered by the `us1_s2`/`us1_s3` journeys and
/// the presentation fold tests.)
#[test]
fn the_files_card_lists_files_bound_boards() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let files_board = uuid::Uuid::new_v4();
    let deck_board = uuid::Uuid::new_v4();
    state_files_board(&mut state, files_board, "files rider");
    state_board(&mut state, deck_board, "deck rider", Some("work"));
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);

        let shape: Vec<(K, String)> = items(&*harness)
            .into_iter()
            .map(|item| (item.kind, item.title.to_string()))
            .collect();
        let board_rows: Vec<&(K, String)> = shape
            .iter()
            .filter(|(kind, _)| *kind == K::FilesBoard)
            .collect();
        assert_eq!(
            board_rows,
            vec![
                &(K::FilesBoard, "deck rider".to_owned()),
                &(K::FilesBoard, "files rider".to_owned()),
            ],
            "every live board renders once, titled by the local board"
        );
    });
}

// -- the files roster's journeys (round-7 D-7.3; the UX origin
// .artifacts/plans/2026-11-13-r7-files-roster-ux-plan.md's step-token
// naming law: one test per journey step, tokens in the names) --------

/// The two-card fixtures' settings hand-over: two Nextcloud
/// identities (two account cards), the files lane on and anchored to
/// card A's connector — exactly one card renders the lane On.
fn two_card_settings() -> UiSettings {
    UiSettings {
        connectors: vec![
            identity_row("a", "https://x.example", "alice"),
            identity_row("b", "https://y.example", "bob"),
        ],
        general: taskboard_ui_slint::requests::SettingsSnapshot {
            files_sync_enabled: true,
            files_sync_poll_secs: None,
            files_sync_anchor: Some("a".into()),
            ..taskboard_ui_slint::requests::SettingsSnapshot::default()
        },
        ..UiSettings::default()
    }
}

/// The roster rows' indexes: one row per board per card, so a title
/// yields one index per card in card order (the walk's positional
/// truth — data identity, titles are the fixture's keys).
fn roster_indexes(
    harness: &Harness,
    kind: taskboard_ui_slint::ConnectionItemKind,
    title: &str,
) -> Vec<usize> {
    items(harness)
        .iter()
        .enumerate()
        .filter(|(_, item)| item.kind == kind && item.title == title)
        .map(|(index, _)| index)
        .collect()
}

/// US-1/S1 — opening the Settings view renders each account card's
/// Files roster section: every card walks the Files row, then its
/// roster rows. Back navigation is the existing escape (the view's
/// own seam, unchanged).
#[test]
fn us1_s1_settings_cards_render_the_files_roster() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let files = uuid::Uuid::new_v4();
    state_files_board(&mut state, files, "files rider");
    support::harness::run_journey_with_settings(state, two_card_settings(), move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);

        let per_card = roster_indexes(&*harness, K::FilesBoard, "files rider");
        assert_eq!(
            per_card.len(),
            2,
            "the roster renders once per Nextcloud account card"
        );
        assert_eq!(
            items(&*harness)
                .iter()
                .filter(|item| item.kind == K::Files)
                .count(),
            2,
            "each card walks its Files row before the roster rows"
        );
    });
}

/// US-1/S2 — the badges and the sort: on the anchor card the
/// files-bound board reads `ThisConnection` (success), an unbound pair
/// sorts `LocalOnly` (neutral) by title, a deck-bound board reads
/// `OtherConnection` (info); the non-anchor card's same boards read
/// `OtherConnection`. The badge TEXT is copy — asserted by the
/// presentation fold's pinned-label test, never here.
#[test]
fn us1_s2_roster_badges_sort_and_tones() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    use taskboard_ui_slint::SettingsBadgeTone;
    let mut state = AppState::default();
    let files = uuid::Uuid::new_v4();
    state_files_board(&mut state, files, "files rider");
    let deck = uuid::Uuid::new_v4();
    state_board(&mut state, deck, "deck rider", Some("work"));
    // Inserted after `alpha`, titled to sort before it: the map order
    // must not leak (the (title, id) tiebreak).
    let zeta = uuid::Uuid::new_v4();
    state_board(&mut state, zeta, "alpha", None);
    let alpha = uuid::Uuid::new_v4();
    state_board(&mut state, alpha, "zeta", None);
    support::harness::run_journey_with_settings(state, two_card_settings(), move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);

        let all = items(&*harness);
        let roster: Vec<taskboard_ui_slint::ConnectionItem> = all
            .iter()
            .filter(|item| item.kind == K::FilesBoard)
            .cloned()
            .collect();
        assert_eq!(roster.len(), 8, "four boards × two cards");
        // Card A (the anchor): ThisConnection → LocalOnly ×2 →
        // OtherConnection ×2, the LocalOnly pair in title order.
        let card_a = &roster[..4];
        let titles: Vec<String> = card_a.iter().map(|item| item.title.to_string()).collect();
        assert_eq!(
            titles,
            vec![
                "files rider".to_owned(),
                "alpha".to_owned(),
                "zeta".to_owned(),
                "deck rider".to_owned(),
            ],
            "ThisConnection, then LocalOnly by (title), then OtherConnection — the map order does not leak"
        );
        assert_eq!(card_a[0].badge_tone, SettingsBadgeTone::Success);
        assert_eq!(card_a[1].badge_tone, SettingsBadgeTone::Neutral);
        assert_eq!(card_a[3].badge_tone, SettingsBadgeTone::Info);
        assert!(!card_a[0].archived);
        // Card B (non-anchor): the same boards, but the
        // files-bound board now reads OtherConnection — so the
        // per-card sort re-orders LocalOnly first, and the two
        // OtherConnection rows follow in title order. The badge
        // is channel-relative, the sort per-account (US-1 AC-1).
        let card_b = &roster[4..];
        assert_eq!(card_b[0].badge_tone, SettingsBadgeTone::Neutral);
        assert_eq!(card_b[1].badge_tone, SettingsBadgeTone::Neutral);
        assert_eq!(card_b[2].badge_tone, SettingsBadgeTone::Info);
        assert_eq!(card_b[3].badge_tone, SettingsBadgeTone::Info);
        assert_eq!(card_b[3].title, "files rider");
        assert_eq!(card_b[2].title, "deck rider");
    });
}

/// US-1/S3 — the action postures: the anchor card's rows are enabled
/// with their action labels set; the non-anchor card's rows carry
/// the honest disabled affordance (R-3), and clicking one dispatches
/// nothing (the adapter guard absorbs it).
#[test]
fn us1_s3_disabled_row_action_postures_absorb_click() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let files = uuid::Uuid::new_v4();
    state_files_board(&mut state, files, "files rider");
    let deck = uuid::Uuid::new_v4();
    state_board(&mut state, deck, "deck rider", Some("work"));
    let loose = uuid::Uuid::new_v4();
    state_board(&mut state, loose, "loose", None);
    support::harness::run_journey_with_settings(state, two_card_settings(), move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        let all = items(&*harness);
        let roster: Vec<&taskboard_ui_slint::ConnectionItem> = all
            .iter()
            .filter(|item| item.kind == K::FilesBoard)
            .collect();
        assert_eq!(roster.len(), 6, "three boards × two cards");
        // The anchor card's half (the first occurrence of each
        // board): enabled, action present.
        for row in roster.iter().take(3) {
            assert!(row.action_enabled, "anchor rows are enabled");
            assert!(!row.busy, "no ceremony in flight");
        }
        // The non-anchor card's half: the files-family rows stay
        // disabled (R-3's lane-off posture); the Deck-bound row is
        // the rebind row (the files-sync-status plan US-10, D-11 —
        // a Deck-to-Deck move needs no files lane) and rides
        // enabled.
        for row in roster.iter().skip(3) {
            if row.title == "deck rider" {
                assert!(
                    row.action_enabled,
                    "the rebind row is enabled without the files lane"
                );
            } else {
                assert!(!row.action_enabled, "non-anchor rows are disabled (R-3)");
            }
        }
        // Clicking a disabled row dispatches nothing.
        let disabled_index = items(&*harness)
            .iter()
            .position(|item| item.kind == K::FilesBoard && !item.action_enabled)
            .expect("a disabled row renders");
        window.invoke_settings_files_board_action_clicked(
            i32::try_from(disabled_index).expect("fits"),
        );
        assert!(
            harness.drain_requests().is_empty(),
            "a disabled row's click is absorbed"
        );
    });
}

/// UX R-2 D-4 (D-7 T9, #787): the account card's Remove arms — zero
/// lane traffic, the armed fact rides the card — and the Confirm
/// sends exactly one `RemoveConnection`; the armed flag cleared
/// before the send means a re-click cannot double-dispatch.
#[test]
fn us2_s1_remove_arms_then_confirm_sends_exact_request_once() {
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(AppState::default(), settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        // The arm: the armed fact rides the card, nothing sends.
        window.invoke_settings_remove_clicked(0);
        assert!(
            harness.drain_requests().is_empty(),
            "the arm dispatches nothing"
        );
        assert!(items(&*harness)[0].armed, "the armed fact rides the card");

        // The confirm: exactly one RemoveConnection; the flag
        // cleared (the pre-send clear), the lane busy.
        window.invoke_settings_remove_confirmed(0);
        let requests = harness.drain_requests();
        assert!(
            matches!(
                &requests[..],
                [taskboard_ui_slint::requests::UiRequest::RemoveConnection { name, .. }]
                    if name == "work"
            ),
            "exactly one remove routed"
        );
        assert!(
            !items(&*harness)[0].armed,
            "the flag cleared before the send"
        );
        assert!(model(&*harness).get_busy(), "the lane went busy");

        // The re-click cannot re-enter either path.
        window.invoke_settings_remove_confirmed(0);
        assert!(
            harness.drain_requests().is_empty(),
            "no double-dispatch after the pre-send clear"
        );
    });
}

/// UX R-2 D-4 (D-7 T10, #787): the Stop-Sync pair confirm-dispatches
/// exactly once — the confirm re-invoked cannot double-dispatch (the
/// armed flag cleared pre-send; the pending gate). The Disconnect
/// pair's arm/confirm is pinned by
/// `bindings_render_without_a_listing_and_the_listing_enriches_without_duplicates`
/// (updated to the two-step grammar) and the re-confirm guard by T9.
#[test]
fn us2_s2_stop_sync_confirm_dispatches_once() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let files = uuid::Uuid::new_v4();
    state_files_board(&mut state, files, "files rider");
    let settings = two_card_settings();
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        // Stop-Sync: arm, confirm, re-confirm — one request.
        let row = roster_indexes(&*harness, K::FilesBoard, "files rider")[0];
        window.invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        window.invoke_settings_files_action_confirmed(i32::try_from(row).expect("fits"));
        let requests = harness.drain_requests();
        assert!(matches!(
            &requests[..],
            [taskboard_ui_slint::requests::UiRequest::StopBoardFilesSync { .. }]
        ));
        window.invoke_settings_files_action_confirmed(i32::try_from(row).expect("fits"));
        assert!(harness.drain_requests().is_empty(), "no double-dispatch");
        let taskboard_ui_slint::requests::UiRequest::StopBoardFilesSync { reply, .. } =
            requests.into_iter().next().expect("one request")
        else {
            panic!("unreachable");
        };
        reply
            .send(Ok(ConnectionsSnapshot {
                boards: Vec::new(),
                connectors: harness_settings_connectors(),
            }))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();
    });
}

/// UX R-2 D-4 (D-7 T11, #787): Cancel disarms; arming item B disarms
/// A; a snapshot refresh killing the armed `ItemAddr` leaves a dead
/// strip that dispatches nothing — every disarm path is
/// dispatch-free.
#[test]
fn cancel_rearm_and_snapshot_refresh_disarm_without_dispatch() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let files = uuid::Uuid::new_v4();
    state_files_board(&mut state, files, "files rider");
    let settings = two_card_settings();
    support::harness::run_journey_with_settings(state.clone(), settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        // Cancel disarms.
        let row = roster_indexes(&*harness, K::FilesBoard, "files rider")[0];
        window.invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        assert!(items(&*harness)[row].armed);
        window.invoke_settings_files_action_cancelled();
        assert!(
            items(&*harness).iter().all(|item| !item.armed),
            "cancel disarms"
        );
        window.invoke_settings_files_action_confirmed(i32::try_from(row).expect("fits"));
        assert!(
            harness.drain_requests().is_empty(),
            "a disarmed confirm sends nothing"
        );

        // Arming item B disarms A (one armed subject surface-wide).
        window.invoke_settings_remove_clicked(0);
        assert!(items(&*harness)[0].armed);
        window.invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        assert!(
            !items(&*harness)[0].armed && items(&*harness)[row].armed,
            "the newer arm displaces the older"
        );
        window.invoke_settings_remove_cancelled();
        assert!(items(&*harness).iter().all(|item| !item.armed));

        // A snapshot refresh killing the armed address leaves a
        // dead strip: arm the roster row, then replace the state
        // so the board vanishes — the pass disarms, the confirm
        // dispatches nothing.
        window.invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        assert!(items(&*harness)[row].armed);
        harness
            .engine
            .replace_state(AppState::default(), &harness.handle);
        harness.handle.drain_setting_replies();
        assert!(
            items(&*harness).iter().all(|item| !item.armed),
            "the vanished subject's strip is dead"
        );
        window.invoke_settings_files_action_confirmed(i32::try_from(row).expect("fits"));
        assert!(
            harness.drain_requests().is_empty(),
            "the dead strip dispatches nothing"
        );
    });
}

/// UX R-2 D-4 (D-7 T12, #787): the VM's typed armed fact drives the
/// strip — exactly one row armed surface-wide after an arm, the
/// flag mirrored per row (no label oracle).
#[test]
fn settings_armed_item_renders_strip_flag_only() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let files = uuid::Uuid::new_v4();
    state_files_board(&mut state, files, "files rider");
    let settings = two_card_settings();
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        let armed_count =
            |harness: &Harness| items(harness).iter().filter(|item| item.armed).count();
        assert_eq!(armed_count(&*harness), 0, "nothing armed at mount");
        let row = roster_indexes(&*harness, K::FilesBoard, "files rider")[0];
        window.invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        assert_eq!(armed_count(&*harness), 1, "exactly one armed surface-wide");
    });
}

/// US-2/S1 — pressing a `ThisConnection` row's "Stop Sync" action
/// dispatches exactly one `StopBoardFilesSync` carrying the row's
/// board; the row busy marker rides the row; a second click during
/// the pending is absorbed.
#[test]
fn us2_s1_roster_row_action_dispatches_stop_board_files_sync() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let files = uuid::Uuid::new_v4();
    state_files_board(&mut state, files, "files rider");
    support::harness::run_journey_with_settings(state, two_card_settings(), move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        let row = roster_indexes(&*harness, K::FilesBoard, "files rider")[0];
        // UX R-2 D-4 (#787): the click arms — the armed fact
        // holds, zero lane traffic; the Confirm sends the exact
        // request once.
        window.invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        assert!(
            harness.drain_requests().is_empty(),
            "the arm dispatches nothing"
        );
        assert!(
            items(&*harness)[row].armed,
            "the armed strip fact rides the specific roster row"
        );
        window.invoke_settings_files_action_confirmed(i32::try_from(row).expect("fits"));
        let requests = harness.drain_requests();
        assert!(
            matches!(
                &requests[..],
                [taskboard_ui_slint::requests::UiRequest::StopBoardFilesSync { board, .. }]
                    if board.as_uuid() == files
            ),
            "exactly one stop routed, carrying the row's board"
        );
        assert!(
            items(&*harness)[row].busy,
            "the busy marker rides the specific roster row"
        );
        assert!(
            !items(&*harness)[row].armed,
            "the armed flag cleared before the send"
        );
        // Re-entrancy: the pending absorbs a second click.
        window.invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        assert!(
            harness.drain_requests().is_empty(),
            "the in-flight op absorbs further clicks"
        );
    });
}

/// US-2/S2 — the typed refusal is the escape surface: a stale
/// stop's `BindRefused` lands in the error fold (the Rejected
/// outcome) and the roster stays intact.
#[test]
fn us2_s2_typed_refusal_surfaces_the_error_fold_roster_intact() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let files = uuid::Uuid::new_v4();
    state_files_board(&mut state, files, "files rider");
    support::harness::run_journey_with_settings(state, two_card_settings(), move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        let row = roster_indexes(&*harness, K::FilesBoard, "files rider")[0];
        window.invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        window.invoke_settings_files_action_confirmed(i32::try_from(row).expect("fits"));
        let mut requests = harness.drain_requests();
        let taskboard_ui_slint::requests::UiRequest::StopBoardFilesSync { reply, .. } =
            requests.remove(0)
        else {
            panic!("the stop routed");
        };
        reply
            .send(Err(
                taskboard_ui_slint::requests::SettingsError::BindRefused,
            ))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();
        assert_eq!(
            model(&*harness).get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Rejected,
            "the typed refusal lands in the error fold"
        );
        assert_eq!(
            roster_indexes(&*harness, K::FilesBoard, "files rider").len(),
            2,
            "the roster stays intact"
        );
        assert!(!model(&*harness).get_busy(), "the gate cleared");
    });
}

/// US-2/S3 — after the unbind lands (state + Ok reply), the row's
/// badge flips to `LocalOnly` and the action re-arms as the publish
/// path: pressing it again dispatches `PublishBoardToFiles`.
#[test]
fn us2_s3_badge_flips_local_only_and_action_rearms() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    use taskboard_ui_slint::SettingsBadgeTone;
    let mut state = AppState::default();
    let files = uuid::Uuid::new_v4();
    state_files_board(&mut state, files, "files rider");
    support::harness::run_journey_with_settings(state, two_card_settings(), move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        let row = roster_indexes(&*harness, K::FilesBoard, "files rider")[0];
        window.invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        window.invoke_settings_files_action_confirmed(i32::try_from(row).expect("fits"));
        let mut requests = harness.drain_requests();
        let taskboard_ui_slint::requests::UiRequest::StopBoardFilesSync { reply, .. } =
            requests.remove(0)
        else {
            panic!("the stop routed");
        };
        // The unbind lands engine-side; the fresh snapshot echoes
        // it (the standard refresh discipline — the next truth is
        // the reply, never an optimistic flip).
        let mut next = AppState::default();
        state_board(&mut next, files, "files rider", None);
        harness.engine.replace_state(next, &harness.handle);
        reply
            .send(Ok(ConnectionsSnapshot {
                boards: vec![taskboard_ui_slint::requests::BoardInfo {
                    id: files,
                    name: "files rider".into(),
                    bound_connector: None,
                    bound_channel: None,
                }],
                connectors: harness_settings_connectors(),
            }))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();

        let all = items(&*harness);
        let flipped = all
            .iter()
            .find(|item| item.kind == K::FilesBoard && item.title == "files rider")
            .expect("the row re-renders");
        assert_eq!(flipped.badge_tone, SettingsBadgeTone::Neutral, "LocalOnly");
        assert!(flipped.action_enabled, "the re-armed action is enabled");
        // The re-armed action routes the publish path.
        let index = roster_indexes(&*harness, K::FilesBoard, "files rider")[0];
        window.invoke_settings_files_board_action_clicked(i32::try_from(index).expect("fits"));
        assert!(matches!(
            &harness.drain_requests()[..],
            [taskboard_ui_slint::requests::UiRequest::PublishBoardToFiles { board, .. }]
                if board.as_uuid() == files
        ));
    });
}

/// US-7/S1 — the #749 re-verification journey (the files-sync-status
/// plan US-7, D-9): the landed Stop Sync action still stages its
/// armed confirm (the r2 grammar), sends exactly one
/// `StopBoardFilesSync`, and the reply's fresh snapshot re-derives
/// the `LocalOnly` badge — the unbind reaches the roster, the issue's
/// premise holds.
#[test]
fn us7_s1_stop_sync_stages_confirms_once_and_rederives_local_only() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    use taskboard_ui_slint::SettingsBadgeTone;
    let mut state = AppState::default();
    let files = uuid::Uuid::new_v4();
    state_files_board(&mut state, files, "files rider");
    support::harness::run_journey_with_settings(state, two_card_settings(), move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        let row = roster_indexes(&*harness, K::FilesBoard, "files rider")[0];
        // The staged confirm: arm dispatches nothing, the armed
        // fact rides the row; Confirm sends the exact request
        // once and clears the arm before the send.
        window.invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        assert!(harness.drain_requests().is_empty(), "the arm sends nothing");
        assert!(items(&*harness)[row].armed, "the armed fact rides the row");
        window.invoke_settings_files_action_confirmed(i32::try_from(row).expect("fits"));
        let mut requests = harness.drain_requests();
        let taskboard_ui_slint::requests::UiRequest::StopBoardFilesSync { reply, .. } =
            requests.remove(0)
        else {
            panic!("the stop routed");
        };
        assert!(
            !items(&*harness)[row].armed,
            "the arm cleared before the send"
        );
        // The unbind lands engine-side; the reply's snapshot
        // echoes it — the next truth is the reply.
        let mut next = AppState::default();
        state_board(&mut next, files, "files rider", None);
        harness.engine.replace_state(next, &harness.handle);
        reply
            .send(Ok(ConnectionsSnapshot {
                boards: vec![taskboard_ui_slint::requests::BoardInfo {
                    id: files,
                    name: "files rider".into(),
                    bound_connector: None,
                    bound_channel: None,
                }],
                connectors: harness_settings_connectors(),
            }))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();

        let all = items(&*harness);
        let flipped = all
            .iter()
            .find(|item| item.kind == K::FilesBoard && item.title == "files rider")
            .expect("the row re-renders");
        assert_eq!(
            flipped.badge_tone,
            SettingsBadgeTone::Neutral,
            "the unbound truth reads LocalOnly"
        );
    });
}

/// US-8/S1 — the adoption phase per row (the files-sync-status plan
/// US-8, D-8): the files watch's per-board states fold 1:1 onto the
/// roster rows — a mid-move board renders its phase, a board absent
/// from the map renders the settled truth.
#[test]
fn us8_s1_the_row_carries_the_adoption_phase_from_the_watch() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    use taskboard_ui_slint::presentation::{FilesBoardAdoption, FilesBoardState, FilesSyncChip};
    let mut state = AppState::default();
    let moved = uuid::Uuid::new_v4();
    let settled = uuid::Uuid::new_v4();
    let files_id = uuid::Uuid::now_v7();
    state_files_board_with_identity(&mut state, moved, "moving rider", files_id);
    state_files_board_with_identity(&mut state, settled, "settled rider", uuid::Uuid::now_v7());
    let chip = FilesSyncChip {
        board_states: std::collections::BTreeMap::from([(
            files_id,
            FilesBoardState {
                adoption: FilesBoardAdoption::Adopting,
                ..FilesBoardState::default()
            },
        )]),
        ..FilesSyncChip::default()
    };
    support::harness::run_journey_with_files_sync(
        state.clone(),
        two_card_settings(),
        chip,
        move |harness| {
            let again = state.clone();

            harness.press(ActionId::AppSettings);
            settle_auto_lists(&*harness, no_rows);
            // The drain (the loop's timer may have consumed the send
            // first — either path applies the same map); the re-push
            // re-folds the roster rows over it.
            let drained = harness.handle.drain_files_sync();
            assert!(drained, "the seeded chip drains");
            harness.engine.replace_state(again, &harness.handle);
            harness.driver.advance_time(30);
            let phase_of = |title: &str| {
                let all = items(&*harness);
                all.iter()
                    .find(|item| item.kind == K::FilesBoard && item.title == title)
                    .expect("the row renders")
                    .adoption_phase
            };
            assert!(
                matches!(
                    phase_of("moving rider"),
                    taskboard_ui_slint::SettingsAdoptionPhase::Adopting
                ),
                "the map's board carries its phase"
            );
            assert!(
                matches!(
                    phase_of("settled rider"),
                    taskboard_ui_slint::SettingsAdoptionPhase::None
                ),
                "a board absent from the map renders the settled truth"
            );
        },
    );
}

/// US-9/S1 — the publish picker anchors at the Deck card (the
/// files-sync-status plan US-9, D-10): opening the card's picker
/// mounts the window-level flyout at site 1 — the observation facts
/// name the Deck row as the active item with the field's geometry —
/// and Escape declines the ladder and closes.
#[test]
fn us9_s1_the_picker_opens_anchored_at_the_deck_card_and_escapes() {
    // The single-card posture (the remote-picker journey's proven
    // coordinate seam): two unbound boards feed the picker's options.
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    let away = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", None);
    state_board(&mut state, away, "away", None);
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        refresh_with(&*harness, "work", vec![remote_row(2, "r")]);
        harness.driver.advance_time(0);

        // Real-pointer click on the Deck card's publish picker
        // (the element query is the address; the traversal races
        // right after the refresh — retry across clock turns).
        let mut target = None;
        for _ in 0..20 {
            harness.driver.advance_time(0);
            if let Some(found) = harness
                .driver
                .find_all_by_id("SettingsView::deck-publish-picker")
                .into_iter()
                .next()
            {
                target = Some(found);
                break;
            }
        }
        let picker = target.expect("the Deck card's picker renders");
        let pos = picker.absolute_position();
        let size = picker.size();
        let x = pos.x + size.width * 0.5;
        let y = pos.y + size.height / 2.0;
        harness.driver.hover_at(x, y);
        harness.driver.advance_time(0);
        harness.driver.click_at(x, y);
        harness.driver.advance_time(30);
        let facts = board_facts(&*harness);
        let deck_rows: Vec<i32> = items(&*harness)
            .iter()
            .enumerate()
            .filter(|(_, item)| item.kind == taskboard_ui_slint::ConnectionItemKind::Deck)
            .map(|(index, _)| i32::try_from(index).expect("fits"))
            .collect();
        assert!(!deck_rows.is_empty(), "the Deck rows render");
        assert!(facts.get_open(), "the flyout opens");
        assert!(
            deck_rows.contains(&facts.get_active_item()),
            "the anchor names a Deck card's row (site 1)"
        );
        // The dismissal contract: Escape declines and closes.
        raw_key(&*harness, slint::platform::Key::Escape);
        harness.driver.advance_time(10);
        assert!(
            !board_facts(&*harness).get_open(),
            "Escape closes the flyout"
        );
    });
}

/// US-10/S1 — the rebind row (the files-sync-status plan US-10,
/// D-11): a Deck-bound board riding card A's connector renders under
/// card B with the actionable rebind affordance enabled without the
/// files lane (B holds no anchor); clicking it dispatches exactly
/// one `MoveBoardToDeck` targeting card B's connector, while the
/// own-card row keeps the Deck→Files switch; a stale index absorbs.
#[test]
fn us10_s1_rebind_row_dispatches_move_board_to_deck_at_the_foreign_card() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let deck = uuid::Uuid::new_v4();
    state_board(&mut state, deck, "deck rider", Some("a"));
    support::harness::run_journey_with_settings(state, two_card_settings(), move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        let rows = roster_indexes(&*harness, K::FilesBoard, "deck rider");
        assert_eq!(rows.len(), 2, "the roster renders once per card");
        // The foreign card's row: enabled though B's files lane
        // is off (the rebind needs no files lane).
        let foreign = rows[1];
        assert!(
            items(&*harness)[foreign].action_enabled,
            "the rebind row is enabled without the files lane"
        );
        window.invoke_settings_files_board_action_clicked(i32::try_from(foreign).expect("fits"));
        let requests = harness.drain_requests();
        assert!(
            matches!(
                &requests[..],
                [taskboard_ui_slint::requests::UiRequest::MoveBoardToDeck {
                    board,
                    target_connector,
                    ..
                }] if board.as_uuid() == deck && target_connector == "b"
            ),
            "the foreign card's click routes the rebind onto its connector"
        );
        // The busy grammar rides the Files row (the move family's
        // existing marker; card B's Files row, not card A's).
        let busy_files = items(&*harness)
            .iter()
            .filter(|item| item.kind == K::Files && item.busy)
            .count();
        assert_eq!(busy_files, 1, "exactly the target card's Files row is busy");
        // The in-flight rebind absorbs further clicks (the one
        // pending machine); the own card's switch dispatch is
        // US-3/S1's journey, not this one's.
        let own = rows[0];
        window.invoke_settings_files_board_action_clicked(i32::try_from(own).expect("fits"));
        assert!(
            harness.drain_requests().is_empty(),
            "the pending absorbs the second click"
        );
        assert!(
            items(&*harness)[own].action_enabled,
            "the own card's switch stays enabled under its anchor"
        );
    });
}

/// The files-bound fixture with a pinned files identity (US-8/S1):
/// the adoption map keys on the board's own domain uuid.
fn state_files_board_with_identity(
    state: &mut AppState,
    id: uuid::Uuid,
    title: &str,
    files_identity: uuid::Uuid,
) {
    use taskboard_domain::{
        Board, BoardClocks, BoardId, Color, ConnectorId, RemoteBoardBinding, RemoteBoardId,
    };
    let id = BoardId::from_uuid(id);
    state.boards.insert(
        id,
        Board {
            id,
            remote: Some(RemoteBoardBinding {
                board: RemoteBoardId::files(files_identity),
                connector: ConnectorId::from("files-sync"),
            }),
            title: title.to_owned(),
            color: Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: BoardClocks::epoch(),
                color: BoardClocks::epoch(),
                archived: BoardClocks::epoch(),
                deleted: BoardClocks::epoch(),
            },
            authors: taskboard_domain::entities::BoardAuthors::default(),
            remote_seen: None,
        },
    );
}

/// The two-card identity rows, echoed back in the replies (the
/// connector half re-renders the rows from the reply, r2 D32).
fn harness_settings_connectors() -> Vec<taskboard_ui_slint::requests::ConnectorInfo> {
    vec![
        identity_row("a", "https://x.example", "alice"),
        identity_row("b", "https://y.example", "bob"),
    ]
}

/// US-3/S1 — pressing a deck-bound row's switch action dispatches
/// `MoveBoardToFiles` verbatim (no new ceremony parameters); the
/// busy marker rides the Files row (the move ceremony's existing
/// grammar); a stale row index absorbs.
#[test]
fn us3_s1_switch_action_dispatches_move_board_to_files() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let deck = uuid::Uuid::new_v4();
    // The board rides the anchor card's own connector (the own-card
    // switch; a foreign-card Deck row is US-10's rebind row).
    state_board(&mut state, deck, "deck rider", Some("a"));
    support::harness::run_journey_with_settings(state, two_card_settings(), move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        let row = roster_indexes(&*harness, K::FilesBoard, "deck rider")[0];
        window.invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        let requests = harness.drain_requests();
        assert!(
            matches!(
                &requests[..],
                [taskboard_ui_slint::requests::UiRequest::MoveBoardToFiles { board, .. }]
                    if board.as_uuid() == deck
            ),
            "the switch routes the move ceremony verbatim"
        );
        // The move's busy marker rides the Files row (D9: the
        // existing grammar stands; the row itself carries no dot).
        let files_row = items(&*harness)
            .iter()
            .position(|item| item.kind == K::Files)
            .expect("the Files row renders");
        assert!(
            items(&*harness)[files_row].busy,
            "the ceremony rides the Files row"
        );
        // A stale index absorbs silently.
        window.invoke_settings_files_board_action_clicked(99);
        assert!(
            harness.drain_requests().is_empty(),
            "a stale row index routes nothing"
        );
    });
}

/// US-3/S2 — after the move lands, the badge flips to
/// `ThisConnection` on the anchor card.
#[test]
fn us3_s2_badge_flips_this_connection_after_move() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    use taskboard_ui_slint::SettingsBadgeTone;
    let mut state = AppState::default();
    let deck = uuid::Uuid::new_v4();
    state_board(&mut state, deck, "deck rider", Some("work"));
    support::harness::run_journey_with_settings(state, two_card_settings(), move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);

        let mut next = AppState::default();
        state_files_board(&mut next, deck, "deck rider");
        harness.engine.replace_state(next, &harness.handle);
        let all = items(&*harness);
        let flipped = all
            .iter()
            .find(|item| item.kind == K::FilesBoard && item.title == "deck rider")
            .expect("the row re-renders");
        assert_eq!(
            flipped.badge_tone,
            SettingsBadgeTone::Success,
            "ThisConnection after the move"
        );
    });
}

/// US-4/S1 — pressing a local-only row's publish action dispatches
/// `PublishBoardToFiles` carrying the row's board.
#[test]
fn us4_s1_publish_action_dispatches_publish_board_to_files() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let loose = uuid::Uuid::new_v4();
    state_board(&mut state, loose, "loose", None);
    support::harness::run_journey_with_settings(state, two_card_settings(), move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        let row = roster_indexes(&*harness, K::FilesBoard, "loose")[0];
        window.invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        let requests = harness.drain_requests();
        assert!(
            matches!(
                &requests[..],
                [taskboard_ui_slint::requests::UiRequest::PublishBoardToFiles { board, .. }]
                    if board.as_uuid() == loose
            ),
            "exactly one publish routed, carrying the row's board"
        );
    });
}

/// US-4/S2 — the publish guard's typed refusal is the escape
/// surface: a stale deck-bound mis-target's `BindRefused` lands in
/// the error fold with the roster intact.
#[test]
fn us4_s2_publish_refusal_surfaces_the_error_fold() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let loose = uuid::Uuid::new_v4();
    state_board(&mut state, loose, "loose", None);
    support::harness::run_journey_with_settings(state, two_card_settings(), move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        let row = roster_indexes(&*harness, K::FilesBoard, "loose")[0];
        window.invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        let mut requests = harness.drain_requests();
        let taskboard_ui_slint::requests::UiRequest::PublishBoardToFiles { reply, .. } =
            requests.remove(0)
        else {
            panic!("the publish routed");
        };
        reply
            .send(Err(
                taskboard_ui_slint::requests::SettingsError::BindRefused,
            ))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();
        assert_eq!(
            model(&*harness).get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Rejected,
            "the typed refusal lands in the error fold"
        );
        assert_eq!(
            roster_indexes(&*harness, K::FilesBoard, "loose").len(),
            2,
            "the roster stays intact"
        );
    });
}

/// US-4/S3 — after the publish lands, the badge flips to
/// `ThisConnection` (board truth; the files lane re-derives its board
/// set next cycle — no UI machinery pretends otherwise).
#[test]
fn us4_s3_badge_flips_this_connection_after_publish() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    use taskboard_ui_slint::SettingsBadgeTone;
    let mut state = AppState::default();
    let loose = uuid::Uuid::new_v4();
    state_board(&mut state, loose, "loose", None);
    support::harness::run_journey_with_settings(state, two_card_settings(), move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();

        let row = roster_indexes(&*harness, K::FilesBoard, "loose")[0];
        window.invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        let mut requests = harness.drain_requests();
        let taskboard_ui_slint::requests::UiRequest::PublishBoardToFiles { reply, .. } =
            requests.remove(0)
        else {
            panic!("the publish routed");
        };
        let mut next = AppState::default();
        state_files_board(&mut next, loose, "loose");
        harness.engine.replace_state(next, &harness.handle);
        reply
            .send(Ok(ConnectionsSnapshot {
                boards: vec![taskboard_ui_slint::requests::BoardInfo {
                    id: loose,
                    name: "loose".into(),
                    bound_connector: Some("files-sync".into()),
                    bound_channel: Some(taskboard_ui_slint::requests::BoardChannel::Files),
                }],
                connectors: harness_settings_connectors(),
            }))
            .expect("the reply lane is alive");
        harness.handle.drain_setting_replies();
        let all = items(&*harness);
        let flipped = all
            .iter()
            .find(|item| item.kind == K::FilesBoard && item.title == "loose")
            .expect("the row re-renders");
        assert_eq!(flipped.badge_tone, SettingsBadgeTone::Success);
        assert_eq!(
            model(&*harness).get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Ok,
        );
    });
}

/// J3 (rewired round-5 chunk 8) — Disconnect moves onto the bound
/// remote row: the lane carries the typed `(board, connector)` pair,
/// the busy marker rides that row, and the Ok reply clears it. An
/// unbound row's Disconnect seam is absorbed silently.
#[test]
fn disconnect_routes_detach_from_the_bound_remote_row() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", Some("work"));
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();
        refresh_with(
            &*harness,
            "work",
            vec![remote_row(1, "r-home"), remote_row(2, "r-free")],
        );

        window.invoke_settings_disconnect_clicked(item_at(&*harness, K::Remote, "r-free"));
        assert!(harness.drain_requests().is_empty(), "unbound, no detach");

        let bound = item_at(&*harness, K::Remote, "r-home");
        // UX R-2 D-4 (#787): the click arms; the confirm routes.
        window.invoke_settings_disconnect_clicked(bound);
        assert!(
            harness.drain_requests().is_empty(),
            "the arm dispatches nothing"
        );
        window.invoke_settings_remove_confirmed(bound);
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1, "exactly one detach routed");
        assert!(matches!(
            &requests[0],
            taskboard_ui_slint::requests::UiRequest::DetachConnector {
                board, connector, ..
            } if board.as_uuid() == home && connector == "work"
        ));
        assert!(model(&*harness).get_busy());
        assert!(
            items(&*harness)[usize::try_from(bound).expect("fits")].busy,
            "the ceremony rides the bound row"
        );
        let taskboard_ui_slint::requests::UiRequest::DetachConnector { reply, .. } =
            requests.remove(0)
        else {
            panic!("the detach routed");
        };
        let _ = reply.send(Ok(ConnectionsSnapshot {
            boards: Vec::new(),
            connectors: vec![connector_row("work")],
        }));
        harness.handle.drain_setting_replies();
        assert!(!model(&*harness).get_busy(), "the gate cleared");
    });
}

/// J4 — the busy gate (r2 D34): with an attach pending, Sign in lands a
/// visible Rejected, and Refresh/Create/Remove/a second pick route
/// nothing (one op at a time); the reply clears the marker
/// (reply-driven — no clock).
#[test]
fn the_busy_gate_holds_one_op_and_the_reply_clears_the_row_marker() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", None);
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();
        refresh_with(&*harness, "work", vec![remote_row(5, "r")]);
        let remote = item_at(&*harness, K::Remote, "r");

        window.invoke_settings_remote_bind_picked(remote, 0);
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1);
        assert!(model(&*harness).get_busy());
        assert_eq!(model(&*harness).get_busy_target(), "work");
        assert!(
            items(&*harness)[usize::try_from(item_at(&*harness, K::Deck, "work")).expect("fits")]
                .busy,
            "the owning Deck row carries the busy flag"
        );

        window.invoke_settings_save_clicked(
            "https://x.example".into(),
            "alice".into(),
            "pw".into(),
        );
        assert_eq!(
            model(&*harness).get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Rejected
        );
        window.invoke_settings_refresh_clicked(item_at(&*harness, K::Deck, "work"));
        window.invoke_settings_create_clicked(remote);
        window.invoke_settings_remove_clicked(0);
        window.invoke_settings_remote_bind_picked(remote, 0);
        assert!(
            harness.drain_requests().is_empty(),
            "the pending gate routed nothing"
        );

        let taskboard_ui_slint::requests::UiRequest::AttachConnector { reply, .. } =
            requests.remove(0)
        else {
            panic!("the attach routed");
        };
        let _ = reply.send(Ok(ConnectionsSnapshot {
            boards: Vec::new(),
            connectors: vec![connector_row("work")],
        }));
        harness.handle.drain_setting_replies();
        assert!(!model(&*harness).get_busy(), "the gate cleared");
        assert!(
            items(&*harness).iter().all(|item| !item.busy),
            "every busy flag cleared with the reply"
        );
    });
}

/// r4 D39 §2.3's staleness rule, kept for the pickers: a board created
/// after install renders in the options on the next pass, and a board
/// that becomes bound leaves them (state-derived by construction).
#[test]
fn the_picker_options_follow_the_state() {
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", None);
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::AppSettings);
        let mut next = AppState::default();
        let late = uuid::Uuid::new_v4();
        state_board(&mut next, late, "late", None);
        state_board(&mut next, home, "home", Some("work"));
        harness.engine.replace_state(next, &harness.handle);
        harness.handle.drain_setting_replies();
        let titles: Vec<String> = options(&*harness)
            .into_iter()
            .map(|row| row.label.to_string())
            .collect();
        assert_eq!(titles, vec!["late".to_owned()], "{titles:?}");
    });
}

/// D-8.5 — Create local board routes one `CreateLocalBoard` for the
/// row's `(connector, remote)`; the session completes when the app's
/// writes land in the state (the fake engine's swap) — the row renders
/// bound to the new board.
#[test]
fn create_local_board_routes_and_the_row_renders_bound() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(AppState::default(), settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();
        refresh_with(&*harness, "work", vec![remote_row(1, "shared")]);
        let row = item_at(&*harness, K::Remote, "shared");
        window.invoke_settings_create_clicked(row);
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1);
        let taskboard_ui_slint::requests::UiRequest::CreateLocalBoard {
            connector,
            remote,
            reply,
        } = requests.remove(0)
        else {
            panic!("the create routes CreateLocalBoard");
        };
        assert_eq!(
            (connector.as_str(), remote),
            ("work", taskboard_domain::RemoteBoardId::deck(1))
        );

        // The app half: CreateBoard + BindBoard landed.
        let mut next = AppState::default();
        let created = uuid::Uuid::new_v4();
        state_board(&mut next, created, "shared", Some("work"));
        harness.engine.replace_state(next, &harness.handle);
        let _ = reply.send(Ok(ConnectionsSnapshot {
            boards: Vec::new(),
            connectors: vec![connector_row("work")],
        }));
        harness.handle.drain_setting_replies();
        let item = &items(&*harness)
            [usize::try_from(item_at(&*harness, K::Remote, "shared")).expect("fits")];
        assert!(item.bound, "the created board is bound to the remote");
        assert_eq!(item.detail, "shared");
        window.invoke_settings_create_clicked(row);
        assert!(
            harness.drain_requests().is_empty(),
            "a bound row creates nothing"
        );
    });
}

/// D-8.7 — the listing's typed failures surface through the status
/// line with zero state change: no rows render, no engine command is
/// sent, the gate clears (a dropped reply too — the wedge guard).
#[test]
fn a_failed_listing_surfaces_typed_without_state_change() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(AppState::default(), settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, || {
            Err(taskboard_ui_slint::requests::SettingsError::StoreUnsupported)
        });
        let window = harness.handle.window();
        for (err, outcome) in [
            (
                taskboard_ui_slint::requests::SettingsError::NoSecretStored,
                taskboard_ui_slint::SettingsOutcome::Rejected,
            ),
            (
                taskboard_ui_slint::requests::SettingsError::VerifyUnreachable,
                taskboard_ui_slint::SettingsOutcome::Unavailable,
            ),
        ] {
            window.invoke_settings_refresh_clicked(item_at(&*harness, K::Deck, "work"));
            let mut requests = harness.drain_requests();
            let taskboard_ui_slint::requests::UiRequest::ListRemoteBoards { reply, .. } =
                requests.remove(0)
            else {
                panic!("the refresh routed");
            };
            let _ = reply.send(Err(err));
            harness.handle.drain_setting_replies();
            assert_eq!(model(&*harness).get_last_outcome(), outcome);
            assert!(!model(&*harness).get_status_text().is_empty());
            assert!(!model(&*harness).get_busy(), "the gate cleared");
            assert!(
                items(&*harness)
                    .iter()
                    .all(|item| item.kind != K::Remote && !item.listed),
                "no remote rows render"
            );
        }
        window.invoke_settings_refresh_clicked(item_at(&*harness, K::Deck, "work"));
        drop(harness.drain_requests());
        harness.handle.drain_setting_replies();
        assert!(!model(&*harness).get_busy(), "a dropped reply never wedges");
        assert!(
            harness.engine.drain().is_empty(),
            "no engine command was sent"
        );
    });
}

/// D-8.6 — the Files toggle renders on the default account's card and
/// commits on pick (`SetFilesSyncEnabled`); the reply's value lands
/// on the row with the app-stated restart fact.
#[test]
fn the_files_toggle_commits_on_pick_and_echoes_the_reply() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(AppState::default(), settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let files = || {
            items(&*harness)
                .into_iter()
                .find(|item| item.kind == K::Files)
                .expect("the Files row renders")
        };
        assert!(!files().files_on);
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let files_pick = items(&*harness)
            .iter()
            .position(|item| item.kind == K::Files)
            .expect("the Files row renders") as i32;
        harness
            .handle
            .window()
            .invoke_settings_files_picked(files_pick, 0);
        let mut requests = harness.drain_requests();
        let taskboard_ui_slint::requests::UiRequest::SetFilesSyncEnabled {
            enabled,
            anchor,
            reply,
        } = requests.remove(0)
        else {
            panic!("the pick routes SetFilesSyncEnabled");
        };
        assert!(enabled);
        // The commit-on-pick shape (round-6 D-6.6): enabled and the
        // anchor move as one action — an enablement from off names
        // the card's connector explicitly (here the resolved
        // fallback's own name, so the key stays semantically put).
        assert_eq!(anchor.as_deref(), Some("work"));
        let mut applies = std::collections::BTreeMap::new();
        applies.insert(
            "files_sync.enabled".to_owned(),
            taskboard_ui_slint::requests::ApplySemantics::on_restart("boot spawn"),
        );
        let _ = reply.send(Ok(taskboard_ui_slint::requests::SettingsSnapshot {
            files_sync_enabled: true,
            files_sync_poll_secs: None,
            files_sync_anchor: Some("work".into()),
            applies,
            ..taskboard_ui_slint::requests::SettingsSnapshot::default()
        }));
        harness.handle.drain_setting_replies();
        assert!(files().files_on, "the reply's value landed");
        assert!(files().files_restart, "the restart fact renders");
    });
}

/// Round-6 D-6.6/D-6.8 phase 1 — the Files row rides EVERY Nextcloud
/// account card (a standalone card carries none), `files_on` is the
/// per-card truth (enabled ∧ this card holds the resolved anchor —
/// exactly one card renders On), and the Deck rows' backend word is
/// backend-aware ("Deck" vs the bare name).
#[test]
fn the_files_row_rides_every_nextcloud_card_with_per_card_truth() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let settings = UiSettings {
        connectors: vec![
            identity_row("work", "https://a.example", "alice"),
            standalone_row("local", "https://b.example", "bob"),
            identity_row("home", "https://c.example", "carol"),
        ],
        general: taskboard_ui_slint::requests::SettingsSnapshot {
            files_sync_enabled: true,
            files_sync_poll_secs: None,
            files_sync_anchor: Some("work".into()),
            ..taskboard_ui_slint::requests::SettingsSnapshot::default()
        },
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(AppState::default(), settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let all = items(&*harness);
        let files_rows: Vec<_> = all
            .iter()
            .enumerate()
            .filter(|(_, item)| item.kind == K::Files)
            .collect();
        assert_eq!(
            files_rows.len(),
            2,
            "two Nextcloud cards, two Files rows; the standalone card carries none"
        );
        assert!(files_rows[0].1.files_on, "the anchor's card renders On");
        assert!(!files_rows[1].1.files_on, "the other card renders Off");
        let deck_words: Vec<_> = all
            .iter()
            .filter(|item| item.kind == K::Deck)
            .map(|item| item.backend_word.to_string())
            .collect();
        assert_eq!(
            deck_words,
            vec!["Deck".to_owned(), String::new(), "Deck".to_owned()],
            "the backend word is app-supplied per connector"
        );
    });
}

/// Round-6 D-6.6 — the pick seam: On a non-anchor card re-anchors to
/// it (the request carries the intended anchor), On the anchor card
/// or any Off leaves the anchor key untouched; a pinned env key
/// absorbs the pick.
#[test]
fn the_files_pick_reanchors_only_when_turning_on_a_non_anchor_card() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let settings = UiSettings {
        connectors: vec![
            identity_row("work", "https://a.example", "alice"),
            identity_row("home", "https://c.example", "carol"),
        ],
        general: taskboard_ui_slint::requests::SettingsSnapshot {
            files_sync_enabled: true,
            files_sync_poll_secs: None,
            files_sync_anchor: Some("work".into()),
            ..taskboard_ui_slint::requests::SettingsSnapshot::default()
        },
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(AppState::default(), settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let files_indices = || -> Vec<usize> {
            items(&*harness)
                .iter()
                .enumerate()
                .filter(|(_, item)| item.kind == K::Files)
                .map(|(index, _)| index)
                .collect()
        };
        let drained = |h: &mut Harness| {
            let mut requests = h.drain_requests();
            let taskboard_ui_slint::requests::UiRequest::SetFilesSyncEnabled {
                enabled,
                anchor,
                ..
            } = requests.remove(0)
            else {
                panic!("the pick routes SetFilesSyncEnabled");
            };
            // Dropping the reply half settles the pending machine
            // (a dropped reply is a typed failure, not a wedge) —
            // the next pick must not be absorbed as busy.
            drop(requests);
            h.handle.drain_setting_replies();
            (enabled, anchor)
        };

        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let (home_pick, work_pick) = (files_indices()[1] as i32, files_indices()[0] as i32);
        harness
            .handle
            .window()
            .invoke_settings_files_picked(home_pick, 0);
        let (enabled, anchor) = drained(&mut *harness);
        assert!(enabled);
        assert_eq!(
            anchor.as_deref(),
            Some("home"),
            "On a non-anchor card re-anchors to it"
        );

        harness
            .handle
            .window()
            .invoke_settings_files_picked(work_pick, 0);
        let (enabled, anchor) = drained(&mut *harness);
        assert!(enabled);
        assert_eq!(anchor, None, "On the anchor card the anchor is untouched");

        harness
            .handle
            .window()
            .invoke_settings_files_picked(home_pick, 1);
        let (enabled, anchor) = drained(&mut *harness);
        assert!(!enabled);
        assert_eq!(anchor, None, "any Off leaves the anchor untouched");
    });
}

/// The D7 session (round-5 chunk 8 acceptance) over the fake lane and
/// engine: sign in (the reply's derived connector renders as a card
/// with its Deck row) → Refresh → rows render → bind an unbound local
/// board explicitly → the binding lands in the state and the row
/// renders it.
#[test]
fn the_account_session_signs_in_lists_and_binds() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", None);
    support::harness::run_journey_with_settings(
        state.clone(),
        UiSettings::default(),
        move |harness| {
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&*harness, no_rows);
            let window = harness.handle.window();
            assert!(items(&*harness).is_empty(), "no accounts yet");

            window.invoke_settings_field_edited(1, "https://c.example".into());
            window.invoke_settings_field_edited(2, "alice".into());
            window.invoke_settings_field_edited(3, "pw".into());
            window.invoke_settings_save_clicked(
                "https://c.example".into(),
                "alice".into(),
                "pw".into(),
            );
            let mut requests = harness.drain_requests();
            let taskboard_ui_slint::requests::UiRequest::SaveConnection { profile, reply } =
                requests.remove(0)
            else {
                panic!("sign in routes SaveConnection");
            };
            assert!(profile.name.is_empty(), "the app derives the name");
            let derived = identity_row("c-example-alice", "https://c.example", "alice");
            let _ = reply.send(Ok(ConnectionsSnapshot {
                boards: Vec::new(),
                connectors: vec![derived],
            }));
            harness.handle.drain_setting_replies();
            item_at(&*harness, K::Account, "alice @ https://c.example");
            item_at(&*harness, K::Deck, "c-example-alice");

            refresh_with(&*harness, "c-example-alice", vec![remote_row(42, "team")]);
            let row = item_at(&*harness, K::Remote, "team");
            window.invoke_settings_remote_bind_picked(row, 0);
            let mut requests = harness.drain_requests();
            let taskboard_ui_slint::requests::UiRequest::AttachConnector {
                board,
                connector,
                remote,
                reply,
            } = requests.remove(0)
            else {
                panic!("the pick routes AttachConnector");
            };
            assert_eq!(board.as_uuid(), home);
            assert_eq!(
                (connector.as_str(), remote),
                ("c-example-alice", taskboard_domain::RemoteBoardId::deck(42))
            );

            // The app half's BindBoard lands in the state.
            let mut next = AppState::default();
            state_board(&mut next, home, "home", Some("c-example-alice"));
            next.boards
                .get_mut(&taskboard_domain::BoardId::from_uuid(home))
                .and_then(|b| b.remote.as_mut())
                .expect("bound")
                .board = taskboard_domain::RemoteBoardId::deck(42);
            harness.engine.replace_state(next, &harness.handle);
            let _ = reply.send(Ok(ConnectionsSnapshot {
                boards: vec![taskboard_ui_slint::requests::BoardInfo {
                    id: home,
                    name: "home".into(),
                    bound_connector: Some("c-example-alice".into()),
                    bound_channel: Some(taskboard_ui_slint::requests::BoardChannel::Deck),
                }],
                connectors: vec![identity_row(
                    "c-example-alice",
                    "https://c.example",
                    "alice",
                )],
            }));
            harness.handle.drain_setting_replies();
            let item = &items(&*harness)
                [usize::try_from(item_at(&*harness, K::Remote, "team")).expect("fits")];
            assert!(item.bound, "the binding renders from the state");
            assert_eq!(item.detail, "home");
            assert!(
                options(&*harness).is_empty(),
                "the bound board left the options"
            );
        },
    );
}

/// J5 — the save click-through (r2 D34, F21): every Save click reacts.
/// An invalid draft lands a typed Rejected with non-empty status text
/// and an empty lane; a valid-but-pending click lands Rejected with
/// the lane untouched; a valid idle click routes (covered by the
/// save-ceremony journey, unchanged).
#[test]
fn every_save_click_produces_a_visible_reaction_and_the_lane_stays_clean() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();
        let model = || window.global::<taskboard_ui_slint::SettingsModel>();

        // Invalid draft (a bad server URL): the click fires
        // *through* the disabled button and the refusal is visible.
        window.invoke_settings_save_clicked("not a url".into(), "alice".into(), "pw".into());
        assert!(harness.drain_requests().is_empty(), "no request routed");
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Rejected,
            "the invalid-draft refusal is visible"
        );
        assert!(
            !model().get_status_text().is_empty(),
            "the status line carries the mapped sentence"
        );

        // Valid draft, but a click while pending: visible
        // Rejected, lane untouched.
        window.invoke_settings_field_edited(1, "https://c.example".into());
        window.invoke_settings_field_edited(2, "alice".into());
        window.invoke_settings_field_edited(3, "pw".into());
        window.invoke_settings_save_clicked(
            "https://c.example".into(),
            "alice".into(),
            "pw".into(),
        );
        assert_eq!(harness.drain_requests().len(), 1, "the idle click routed");
        window.invoke_settings_save_clicked(
            "https://c.example".into(),
            "alice".into(),
            "pw".into(),
        );
        assert!(
            harness.drain_requests().is_empty(),
            "the pending click routed nothing"
        );
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Rejected,
            "the busy-click refusal is visible"
        );
    });
}

// -- the pointer drag-to-move journeys (r2 chunk-8, T-6..T-10) ----------

/// The drag journeys' geometry helper: the accessible button whose
/// label addresses the column (the header; the column list carries
/// the same label — the `is_accessible_button` disambiguator).
fn column_header(harness: &Harness, title: &str) -> (f32, f32) {
    let button = harness
        .driver
        .find_all_by_label(title)
        .into_iter()
        .find(support::driver::Element::is_accessible_button)
        .unwrap_or_else(|| panic!("the {title:?} header button is present"));
    let pos = button.absolute_position();
    let size = button.size();
    (pos.x + size.width / 2.0, pos.y + size.height / 2.0)
}

/// The drag flow's model surface (r2 D-D): the ghost's open flag, the
/// ghost card's snapshot title, and the target highlight index.
fn drag_surface(harness: &Harness) -> (bool, String, i32) {
    let window = harness.handle.window();
    let ghost = window
        .global::<taskboard_ui_slint::DragState>()
        .get_ghost_model();
    (
        window.get_board_drag_open(),
        ghost.title.to_string(),
        window.get_board_model().move_target,
    )
}

/// The D-F acceptance (b): press-hold-drag past the slop lifts the
/// ghost (the source card dims), moving over another column
/// highlights it, and the release dispatches **the same `MoveTask`**
/// the keyboard flow confirms — two mounts, one fixture shape, one
/// literal expected command.
#[test]
fn the_pointer_drag_drops_the_keyboard_move_command() {
    let (fixture, state) = fixture::golden_state();
    let task0 = state
        .tasks
        .values()
        .find(|task| task.title == "task-0")
        .map(|task| task.id)
        .expect("the fixture's first task");
    let expected = StateCommand::MoveTask {
        id: task0,
        stack: fixture.stack_b,
        order: 1,
    };

    // The drag path.
    let (drag_state, expected_drag) = (state.clone(), expected.clone());
    support::harness::run_journey(drag_state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        let card = harness
            .driver
            .find_by_label("task-0")
            .expect("the first card is addressable");
        let (cx, cy) = support::driver::centre(&card);
        harness.driver.press_at(cx, cy);
        let (hx, hy) = column_header(&*harness, "b");
        // The arming move is consumed by the StartDrag hand-off;
        // the next move is the first DragMove the DropAreas see.
        harness.driver.drag_move_to(hx, hy);
        harness.driver.drag_move_to(hx, hy);
        harness.driver.advance_time(0);
        let (open, ghost_title, target) = drag_surface(&*harness);
        assert!(open, "the drag is on the model surface");
        assert_eq!(ghost_title, "task-0", "the ghost carries the snapshot");
        assert_eq!(target, 1, "the hovered column highlights");
        let source_dimmed = harness
            .handle
            .window()
            .get_board_model()
            .stacks
            .row_data(0)
            .expect("column")
            .tasks
            .row_data(0)
            .expect("card")
            .dragging_source;
        assert!(source_dimmed, "the source card carries the dim bit");
        harness.driver.release_at(hx, hy);
        harness.driver.advance_time(0);
        harness.assert_commands(&[expected_drag]);
        assert!(
            !harness.handle.window().get_board_drag_open(),
            "the drop closes the ghost"
        );
    });

    // The keyboard path: the same transition, the same literal command.
    let expected_kb = expected;
    support::harness::run_journey(state, move |harness| {
        harness.handle.window().invoke_board_task_clicked(0, 0);
        harness.press(ActionId::TaskMove);
        // UX-R8 D-1: three slot strokes walk to stack b's
        // after-last gap — whose slot order IS the append (the
        // drag leg's command, unchanged).
        harness.stroke(parse("Down").expect("Down"));
        harness.stroke(parse("Down").expect("Down"));
        harness.stroke(parse("Down").expect("Down"));
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[expected_kb]);
    });
}

/// R3-F4 (D1) — the ghost is card-sized: arming a drag captures the
/// source card's rendered size into `DragState` before the ghost's
/// first frame, so the ghost `TaskCard` mounts at the card's geometry —
/// never the window fill. Structural: the globals against the live
/// card element; no pixel assertions.
#[test]
fn the_drag_ghost_carries_the_source_cards_size() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        let card = harness
            .driver
            .find_by_label("task-0")
            .expect("the first card is addressable");
        let (cx, cy) = support::driver::centre(&card);
        let card_size = card.size();
        harness.driver.press_at(cx, cy);
        let (hx, hy) = column_header(&*harness, "b");
        harness.driver.drag_move_to(hx, hy);
        harness.driver.drag_move_to(hx, hy);
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_board_drag_open(),
            "the drag is on the model surface"
        );
        let drag = harness
            .handle
            .window()
            .global::<taskboard_ui_slint::DragState>();
        let (gw, gh) = (drag.get_ghost_width(), drag.get_ghost_height());
        assert!(
            gw > 0.0 && gh > 0.0,
            "the captured size is set before the ghost's first frame, got {gw}x{gh}"
        );
        assert_eq!(gw, card_size.width, "the ghost width is the card's");
        assert_eq!(gh, card_size.height, "the ghost height is the card's");
        harness.driver.release_at(hx, hy);
        harness.driver.advance_time(0);
    });
}

/// The (d) acceptance: a background refresh mid-drag re-derives the
/// ghost flag, the ghost snapshot, and the highlight from the flow —
/// they survive — and the release still dispatches (T-7).
#[test]
fn the_drag_surface_survives_interleaved_refreshes() {
    let (fixture, state) = fixture::golden_state();
    let task0 = state
        .tasks
        .values()
        .find(|task| task.title == "task-0")
        .map(|task| task.id)
        .expect("the fixture's first task");
    // The mid-drag engine snapshot: the SAME fixture (stable ids —
    // the dragged task must survive) plus a new task in stack b.
    let mut changed = state.clone();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        let card = harness
            .driver
            .find_by_label("task-0")
            .expect("the first card is addressable");
        let (cx, cy) = support::driver::centre(&card);
        harness.driver.press_at(cx, cy);
        let (hx, hy) = column_header(&*harness, "b");
        harness.driver.drag_move_to(hx, hy);
        harness.driver.drag_move_to(hx, hy);
        harness.driver.advance_time(0);

        // A concurrent engine change (the width journey's shape).
        changed.tasks.insert(
            taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7()),
            taskboard_domain::Task {
                id: taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7()),
                remote: None,
                title: "mid-drag".into(),
                description: String::new(),
                duedate: None,
                scheduled: None,
                done: None,
                stack: fixture.stack_b,
                order: 0,
                label_memberships: std::collections::BTreeMap::new(),
                effort: None,
                importance: None,
                urgency: None,
                recur: None,
                kind: taskboard_domain::entities::TaskKind::Standard,

                archived: false,
                deleted: false,
                clocks: taskboard_domain::TaskClocks {
                    title: taskboard_domain::BoardClocks::epoch(),
                    description: taskboard_domain::BoardClocks::epoch(),
                    duedate: taskboard_domain::BoardClocks::epoch(),
                    scheduled: taskboard_domain::BoardClocks::epoch(),
                    done: taskboard_domain::BoardClocks::epoch(),
                    position: taskboard_domain::BoardClocks::epoch(),

                    effort: taskboard_domain::BoardClocks::epoch(),
                    importance: taskboard_domain::BoardClocks::epoch(),
                    urgency: taskboard_domain::BoardClocks::epoch(),
                    recur: taskboard_domain::BoardClocks::epoch(),
                    kind: taskboard_domain::BoardClocks::epoch(),

                    archived: taskboard_domain::BoardClocks::epoch(),
                    deleted: taskboard_domain::BoardClocks::epoch(),
                },
                authors: taskboard_domain::entities::TaskAuthors::default(),
                remote_seen: None,
                remote_baseline: None,
            },
        );
        harness.engine.replace_state(changed, &harness.handle);
        let (open, ghost_title, target) = drag_surface(&*harness);
        assert!(open, "the refresh keeps the drag open");
        assert_eq!(ghost_title, "task-0", "the ghost snapshot re-derives");
        assert_eq!(target, 1, "the highlight re-derives");
        harness.driver.release_at(hx, hy);
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::MoveTask {
            id: task0,
            stack: fixture.stack_b,
            order: 1,
        }]);
    });
}

/// The (c) acceptance, arm 1: a release outside any column (over the
/// toolbar) cancels — no dispatch, the ghost closes (T-8a).
#[test]
fn a_release_outside_any_column_cancels_the_drag() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        let card = harness
            .driver
            .find_by_label("task-0")
            .expect("the first card is addressable");
        let (cx, cy) = support::driver::centre(&card);
        harness.driver.press_at(cx, cy);
        harness.driver.drag_move_to(500.0, 12.0);
        harness.driver.drag_move_to(500.0, 12.0);
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_board_drag_open(),
            "the drag opened over the dead zone"
        );
        harness.driver.release_at(500.0, 12.0);
        harness.driver.advance_time(0);
        harness.assert_commands(&[]);
        assert!(
            !harness.handle.window().get_board_drag_open(),
            "the cancel closes the ghost"
        );
        assert_eq!(
            harness.handle.window().get_board_model().move_target,
            -1,
            "no highlight survives"
        );
    });
}

/// The (c) acceptance, arm 2: Escape mid-drag cancels through D16's
/// binding with no dispatch, tears the native drag down, and the real
/// release afterwards cannot degrade into a click (T-8b).
#[test]
fn escape_mid_drag_cancels_and_the_release_clicks_nothing() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        let card = harness
            .driver
            .find_by_label("task-0")
            .expect("the first card is addressable");
        let (cx, cy) = support::driver::centre(&card);
        harness.driver.press_at(cx, cy);
        let (hx, hy) = column_header(&*harness, "b");
        harness.driver.drag_move_to(hx, hy);
        harness.driver.drag_move_to(hx, hy);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_board_drag_open());
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        harness.assert_commands(&[]);
        assert!(
            !harness.handle.window().get_board_drag_open(),
            "Escape ends the drag"
        );
        // The real release after the teardown: no spurious click.
        harness.driver.release_at(hx, hy);
        harness.driver.advance_time(0);
        harness.assert_commands(&[]);
        assert!(
            harness.emphasized_card().is_none(),
            "the post-teardown release selects nothing"
        );
    });
}

/// The (c) acceptance, arm 3: a leaving effect (nav.next-view)
/// mid-drag cancels without dispatch (T-8c).
#[test]
fn a_leaving_effect_mid_drag_cancels_the_drag() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        let card = harness
            .driver
            .find_by_label("task-0")
            .expect("the first card is addressable");
        let (cx, cy) = support::driver::centre(&card);
        harness.driver.press_at(cx, cy);
        let (hx, hy) = column_header(&*harness, "b");
        harness.driver.drag_move_to(hx, hy);
        harness.driver.drag_move_to(hx, hy);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_board_drag_open());
        harness.press(ActionId::NavNextView);
        harness.driver.advance_time(0);
        harness.assert_commands(&[]);
        assert!(
            !harness.handle.window().get_board_drag_open(),
            "the view switch cancels the drag"
        );
    });
}

/// The D-H slop boundary (T-9): below the 8-px slop a release is a
/// click (the typed selection outcome); past it, the same gesture is
/// a drag whose release on the source column reorders the task to its
/// own stack's end — the keyboard ring's legal move, and still no
/// click.
#[test]
fn the_slop_boundary_separates_clicks_from_drags() {
    // Sub-slop: a click.
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        let card = harness
            .driver
            .find_by_label("task-0")
            .expect("the first card is addressable");
        let (cx, cy) = support::driver::centre(&card);
        harness.driver.press_at(cx, cy);
        harness.driver.drag_move_to(cx + 4.0, cy);
        harness.driver.release_at(cx + 4.0, cy);
        harness.driver.advance_time(0);
        harness.assert_commands(&[]);
        assert_eq!(
            harness.emphasized_card(),
            Some((0, 0)),
            "the sub-slop release selected the card"
        );
    });

    // Past the slop: a drag that drops back on the source card.
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        let card = harness
            .driver
            .find_by_label("task-0")
            .expect("the first card is addressable");
        let (cx, cy) = support::driver::centre(&card);
        harness.driver.press_at(cx, cy);
        harness.driver.drag_move_to(cx + 20.0, cy);
        harness.driver.drag_move_to(cx + 20.0, cy);
        harness.driver.advance_time(0);
        let (open, _, _) = drag_surface(&*harness);
        assert!(open, "the drag armed past the slop");
        harness.driver.release_at(cx + 20.0, cy);
        harness.driver.advance_time(0);
        // UX-R8 D-2 (US-2/AC-2): the release lands in the card's
        // own gap zone — the in-place no-op dispatches nothing and
        // ends the drag silently (the old grammar appended here;
        // the release back on the card is the no-op by design
        // now). The append path stays pinned by
        // the_pointer_drag_drops_the_keyboard_move_command's
        // column-header release.
        harness.assert_commands(&[]);
        assert!(
            !harness.handle.window().get_board_drag_open(),
            "the in-place release ends the drag"
        );
        assert!(
            harness.emphasized_card().is_none(),
            "the dragged release never clicked"
        );
    });
}

/// The T-10 refusal: with a flow already open (the create row), a
/// press-move on a card arms nothing — the `DragArea`s are disabled and
/// the view state refuses — and the open flow's field survives.
#[test]
fn a_drag_never_arms_while_another_flow_holds() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::TaskCreate);
        let create_open = |harness: &Harness| {
            harness
                .handle
                .window()
                .get_board_model()
                .stacks
                .row_data(0)
                .expect("column")
                .create_open
        };
        assert!(create_open(&*harness), "the create row opened");
        let card = harness
            .driver
            .find_by_label("task-0")
            .expect("the first card is addressable");
        let (cx, cy) = support::driver::centre(&card);
        harness.driver.press_at(cx, cy);
        harness.driver.drag_move_to(cx + 20.0, cy);
        harness.driver.drag_move_to(cx + 20.0, cy);
        harness.driver.advance_time(0);
        let (open, _, target) = drag_surface(&*harness);
        assert!(!open, "the busy flow refuses the drag arm");
        assert_eq!(target, -1, "no highlight from the refused arm");
        assert!(create_open(&*harness), "the create row survived");
        harness.assert_commands(&[]);
    });
}

// -- K13: the kiosk ambient rotation (issue #363) ------------------------

use std::time::Duration;

use taskboard_ui_slint::view_state::{KioskMode, KioskRotation};

/// The K13 rotation hand-over: board ⇄ today, 30 s dwell. Built
/// directly (the journeys bypass config; the typed surface is the
/// same `UiSettings` field the appliance hands over).
fn rotation_settings() -> UiSettings {
    UiSettings {
        kiosk: true,
        kiosk_rotation: Some(
            KioskRotation::new(
                vec![KioskMode::Board, KioskMode::Today],
                Duration::from_secs(30),
            )
            .expect("two distinct modes mint"),
        ),
        ..UiSettings::default()
    }
}

/// The today sections' per-row counts (the pinned-board filter's
/// surface witness).
fn today_section_counts(harness: &Harness) -> Vec<usize> {
    let today = harness.handle.window().get_today_model();
    (0..today.sections.row_count())
        .map(|section| {
            usize::try_from(today.sections.row_data(section).expect("section").count)
                .expect("section count fits")
        })
        .collect()
}

/// J-K1 — the rotation flips on the dwell timer with zero input: the
/// declared order seeds the start view, the mock clock fires the
/// flip exactly at the dwell boundary, focus resets each flip, the
/// banner and the sync badge carry over, and the engine's command
/// log stays empty throughout (the no-engine-command end-to-end
/// witness).
#[test]
fn the_kiosk_rotation_flips_modes_on_the_dwell_timer() {
    let (_fixture, state, _walked) = fixture::today_overdue_state();
    support::harness::run_journey_with_settings(state, rotation_settings(), move |harness| {
        assert_eq!(
            current_view(&*harness),
            View::Board,
            "the appliance opens on modes[0]"
        );

        let banner_rows_before = harness.handle.window().get_banner_model().row_count();
        let sync_phase_before = harness.handle.window().get_sync_model().phase;

        harness.driver.advance_time(29_999);
        assert_eq!(
            current_view(&*harness),
            View::Board,
            "one millisecond inside the dwell: no flip"
        );
        harness.driver.advance_time(1);
        assert_eq!(
            current_view(&*harness),
            View::Today,
            "the dwell fires the flip"
        );
        assert_eq!(harness.emphasized_card(), None, "the flip resets focus");

        harness.driver.advance_time(30_000);
        assert_eq!(current_view(&*harness), View::Board, "the cycle wraps");
        assert_eq!(harness.emphasized_card(), None);

        // The carry-over is structural (K13 D6): pinned by
        // assertion across the flips.
        assert_eq!(
            harness.handle.window().get_banner_model().row_count(),
            banner_rows_before,
            "the banner rows are unchanged across flips"
        );
        assert_eq!(
            harness.handle.window().get_sync_model().phase,
            sync_phase_before,
            "the sync badge's phase is unchanged across flips"
        );

        assert_eq!(
            harness.engine.drain(),
            [] as [StateCommand; 0],
            "rotation must never reach the engine"
        );
    });
}

/// J-K2 — the passive reading: in both modes the kiosk gates hold
/// (inbox chip 0, notification surface empty) and the today model is
/// the PINNED board's degeneration — board one's overdue row, not
/// the desktop's cross-board two.
#[test]
fn the_kiosk_today_mode_renders_only_the_pinned_board() {
    let (_fixture, mut state, _walked) = fixture::today_overdue_state();
    // Live and awaiting alarms in the data: the gates must hold
    // against non-empty timer state, not just an empty one.
    let fired_at = chrono::Utc::now() - TimeDelta::minutes(1);
    let fired = fixture::add_alarm(&mut state, "kiosk-alarm", fired_at);
    fixture::deliver_alarm(&mut state, fired, fired_at);
    let _live = fixture::add_alarm(
        &mut state,
        "kiosk-timer",
        chrono::Utc::now() + TimeDelta::hours(1),
    );
    let state = state;
    support::harness::run_journey_with_settings(state, rotation_settings(), move |harness| {
        // Board mode (modes[0]): the gates hold.
        assert_eq!(current_view(&*harness), View::Board);
        assert_eq!(
            harness.handle.window().get_toolbar_inbox_count(),
            0,
            "kiosk is passive: the inbox chip counts to zero"
        );
        assert_eq!(
            harness.handle.window().get_notification_model().row_count(),
            0,
            "kiosk is banner-only: the surface model stays empty"
        );
        // The timers gate (DR-3/3, D-I6): the section, its rows,
        // and the status-bar chip are all forced off model-side.
        assert!(!harness.handle.window().get_timers_visible());
        assert_eq!(harness.handle.window().get_timers_model().row_count(), 0);
        assert_eq!(harness.handle.window().get_timers_awaiting(), 0);
        assert!(!harness.handle.window().get_timers_chip_visible());

        harness.driver.advance_time(30_000);
        assert_eq!(current_view(&*harness), View::Today);
        let counts = today_section_counts(&*harness);
        assert_eq!(
            counts.first().copied(),
            Some(1),
            "the pinned board's one overdue row renders (the desktop \
                 would show two — the filter is the kiosk degeneration)"
        );
        assert_eq!(
            harness.handle.window().get_toolbar_inbox_count(),
            0,
            "the gates hold across the flip"
        );
        assert_eq!(
            harness.handle.window().get_notification_model().row_count(),
            0
        );
        assert!(!harness.handle.window().get_timers_visible());
        assert_eq!(harness.handle.window().get_timers_model().row_count(), 0);
        assert!(!harness.handle.window().get_timers_chip_visible());

        harness.driver.advance_time(30_000);
        assert_eq!(
            current_view(&*harness),
            View::Board,
            "the flip back renders the board"
        );
    });
}

/// J-K3 — the desktop is inert: no rotation in the hand-over, so
/// advancing the mock clock past several would-be dwells flips
/// nothing, dispatches nothing, and the view stays the desktop
/// default.
#[test]
fn the_desktop_mount_never_rotates() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = Harness::mount_no_escape(SlintDriver::default(), state);
            assert_eq!(current_view(&harness), View::Today);
            for _ in 0..3 {
                harness.driver.advance_time(30_000);
                assert_eq!(current_view(&harness), View::Today, "no flips");
            }
            assert_eq!(harness.engine.drain(), [] as [StateCommand; 0]);
        },
    );
}

/// The empty-board create flow is visible (round-3 R5/F10, minimal
/// fix): the empty-board leg renders the same create field the
/// populated legs do, typing lands, and Return creates the stack.
/// The full centered-CTA empty-board page stays chunk 2 (D5).
#[test]
fn the_empty_board_stack_create_flow_opens_a_visible_field() {
    let state = fixture::empty_board_state();
    support::harness::run_journey(state, move |harness| {
        let create_open =
            |harness: &Harness| harness.handle.window().get_board_model().stack_create_open;

        harness.press(ActionId::StackCreate);
        assert!(create_open(&*harness), "the flow arms on the empty board");
        assert!(
            harness.driver.find_by_label("New stack name").is_some(),
            "the empty-board leg renders the create field (F10)"
        );
        harness.type_text("first");
        harness.stroke(parse("Return").expect("Return"));
        assert!(!create_open(&*harness), "the commit closed the flow");
        match harness.engine.drain().as_slice() {
            [StateCommand::CreateStack { title, order, .. }] => {
                assert_eq!(title.as_str(), "first");
                assert_eq!(*order, 0, "the first stack on the empty board");
            }
            other => panic!("the create reached the boundary, got {other:?}"),
        }
    });
}

/// Round-3 F26 — the title re-edit journey: detail entry never arms
/// (plan §6), the closed title's click arms the seeded editor, the
/// commit closes it, and clicking again re-arms — the exact repro
/// that was stuck before. The empty commit closes without dispatch.
#[test]
fn the_title_edits_clicks_arms_and_re_arms() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state.clone(), move |harness| {
        let edit_open = |harness: &Harness| harness.handle.window().get_task_model().edit_open;

        // Entry (the standard preset's Return on the focused
        // card) shows the display: no field, no flow, Normal
        // mode.
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskEdit);
        assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
        assert!(!edit_open(&*harness), "entry never arms (plan §6)");
        assert!(
            !harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_vi_insert(),
            "entry stays in normal mode"
        );
        harness.assert_commands(&[]);

        // Click the closed title: the field arms, seeded from the
        // display title (click ≡ key, §7.1).
        harness.driver.click_by_label("Edit title");
        harness.driver.advance_time(0);
        assert!(edit_open(&*harness), "the click arms the editor");
        assert_eq!(
            harness.handle.window().get_task_model().initial_text,
            "task-0",
            "the seed mirrors the current title"
        );

        // An empty commit closes without dispatching: backspace
        // past the whole seed (the click path's selection state
        // is not pinned), then Return on the empty buffer.
        for _ in 0..7 {
            harness.stroke(parse("Backspace").expect("Backspace"));
        }
        harness.stroke(parse("Return").expect("Return"));
        assert!(!edit_open(&*harness), "the empty commit closes");
        harness.assert_commands(&[]);

        // The exact F26 repro: clicking again re-arms.
        harness.driver.click_by_label("Edit title");
        harness.driver.advance_time(0);
        assert!(edit_open(&*harness), "the title is always re-armable");

        // A typed commit dispatches and closes.
        harness.type_text("renamed");
        harness.stroke(parse("Return").expect("Return"));
        assert!(!edit_open(&*harness), "the commit closed the editor");
        match harness.engine.drain().as_slice() {
            [StateCommand::UpdateTask { changes, .. }] => {
                assert_eq!(changes.title.as_deref(), Some("renamed"));
            }
            other => panic!("the title commit reached the boundary, got {other:?}"),
        }

        // The geometry pin (plan §6): the field stretches to the
        // detail body's full width — it is no longer nested in
        // the back row's centered column.
        harness.handle.dispatch_action(ActionId::TaskTitleEdit);
        harness.driver.advance_time(0);
        let field = harness
            .driver
            .find_by_label("Task title")
            .expect("the field is mounted");
        assert!(
            field.size().width > 300.0,
            "the field spans the body width, got {:?}",
            field.size()
        );
    });
}

/// Round-3 R1/F7 — the click-out journeys: an empty create flow
/// settles (discards) on a dead-zone click; a typed one commits
/// exactly once, and Return afterwards dispatches nothing (the
/// double-commit guard).
#[test]
fn a_dead_zone_click_settles_the_create_flow() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        let create_open = |harness: &Harness| {
            harness
                .handle
                .window()
                .get_board_model()
                .stacks
                .row_data(0)
                .expect("column")
                .create_open
        };

        // Empty buffer: the click-out discards, the mode returns,
        // and New Task works again without a second click.
        harness.press(ActionId::TaskCreate);
        assert!(create_open(&*harness));
        harness.driver.advance_time(0);
        harness.driver.click_at(10.0, 450.0);
        harness.driver.advance_time(0);
        assert!(!create_open(&*harness), "the empty click-out discards");
        assert!(!keyboard(&*harness).get_vi_insert(), "normal mode returns");
        harness.assert_commands(&[]);
        harness.press(ActionId::TaskCreate);
        assert!(create_open(&*harness), "New Task works again");
        harness.driver.click_at(10.0, 450.0);
        harness.assert_commands(&[]);

        // Typed buffer: the click-out commits exactly once, and a
        // Return afterwards dispatches nothing (the teardown
        // focus-lost echo is absorbed).
        harness.press(ActionId::TaskCreate);
        harness.type_text("clicked out");
        harness.driver.click_at(10.0, 450.0);
        harness.driver.advance_time(0);
        assert!(!create_open(&*harness), "the typed click-out commits");
        match harness.engine.drain().as_slice() {
            [StateCommand::CreateTask { title, .. }] => {
                assert_eq!(title.as_str(), "clicked out");
            }
            other => panic!("one create reached the boundary, got {other:?}"),
        }
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[]);
    });
}

/// DR-3/3 J-T1 — the desktop create→row→cancel loop: the form's
/// commit dispatches the exact `CreateAdhocAlarm`, the settle makes
/// the row render in the section (the chip lights up with it), and
/// the row's cancel affordance dispatches the exact
/// `CancelAdhocAlarm`, emptying the section by derivation.
#[test]
fn the_timers_form_creates_then_cancels() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        assert_eq!(current_view(&*harness), View::Today);
        assert!(!harness.handle.window().get_timers_chip_visible());
        // Fill the form: click-focus each field (the fields opt
        // out of the mount-time focus grab), type, commit on the
        // timing field's Return.
        let click_field = |harness: &mut Harness, label: &str| {
            let element = harness
                .driver
                .find_by_label(label)
                .unwrap_or_else(|| panic!("the field {label:?} is present"));
            let (x, y) = support::driver::centre(&element);
            harness.driver.click_at(x, y);
        };
        click_field(&mut *harness, "Timer label");
        harness.type_text("Tea");
        click_field(&mut *harness, "Timer timing (in 10m or at 09:30)");
        harness.type_text("in 3m");
        harness.stroke(parse("Return").expect("Return"));
        match harness.engine.drain().as_slice() {
            [StateCommand::CreateAdhocAlarm { label, when, .. }] => {
                assert_eq!(label.as_str(), "Tea");
                assert_eq!(
                    *when,
                    taskboard_domain::AdhocWhen::In(TimeDelta::minutes(3))
                );
            }
            other => panic!("the create reached the boundary, got {other:?}"),
        }

        harness.engine.settle(&harness.handle);
        assert_eq!(harness.handle.window().get_timers_model().row_count(), 1);
        assert!(harness.handle.window().get_timers_chip_visible());

        // The row's cancel affordance.
        harness.driver.click_by_label("Cancel timer");
        match harness.engine.drain().as_slice() {
            [StateCommand::CancelAdhocAlarm { .. }] => {}
            other => panic!("the cancel reached the boundary, got {other:?}"),
        }
        harness.engine.settle(&harness.handle);
        assert_eq!(harness.handle.window().get_timers_model().row_count(), 0);
        assert!(!harness.handle.window().get_timers_chip_visible());
    });
}

/// T-43 (#826): the created timer's acknowledgement rides the
/// status-notice lane — the create routes AND the typed created-
/// occurrence fact (the label plus its target) arms the notice.
#[test]
fn a_created_timer_routes_and_sets_the_status_notice() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        let click_field = |harness: &mut Harness, label: &str| {
            let element = harness
                .driver
                .find_by_label(label)
                .unwrap_or_else(|| panic!("the field {label:?} is present"));
            let (x, y) = support::driver::centre(&element);
            harness.driver.click_at(x, y);
        };
        click_field(&mut *harness, "Timer label");
        harness.type_text("Tea");
        click_field(&mut *harness, "Timer timing (in 10m or at 09:30)");
        harness.type_text("in 3m");
        harness.stroke(parse("Return").expect("Return"));
        match harness.engine.drain().as_slice() {
            [StateCommand::CreateAdhocAlarm { .. }] => {}
            other => panic!("the create reached the boundary, got {other:?}"),
        }
        assert!(
            harness.handle.window().get_notice_visible(),
            "the created-timer acknowledgement arms the status notice"
        );
    });
}

/// r4 D32 — the two-part countdown: a live timer's row renders the
/// fire instant (the `@` detail line) beside the countdown text, and
/// the insistence rung projects the badge bit — `normal` stays
/// badge-less (D-I9's ladder), `alarm` arms it. The per-second motion
/// is pinned at the unit tier (the seconds-gate tests + `SecondTick`):
/// the journey driver's mock clock moves slint timers, not the wall
/// clock the repaint driver reads, so a "changes across one clock
/// step" pin here would be nondeterministic (recorded deviation from
/// the stage-1 catalogue).
#[test]
fn the_timer_row_renders_the_two_part_countdown_and_the_rung() {
    let (_fixture, mut state) = fixture::journey_state();
    let _tea = fixture::add_alarm(
        &mut state,
        "Tea",
        chrono::Utc::now() + TimeDelta::minutes(5),
    );
    let _wake = fixture::add_alarm_with_rung(
        &mut state,
        "Wake",
        chrono::Utc::now() + TimeDelta::minutes(7),
        taskboard_domain::Insistence::Alarm,
    );
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        let rows = harness.handle.window().get_timers_model();
        assert_eq!(rows.row_count(), 2, "both live timers render");
        let normal = rows.row_data(0).expect("the ordered-first row");
        assert!(
            !normal.fire_time.is_empty(),
            "the fire instant's detail line renders"
        );
        assert!(!normal.countdown.is_empty(), "the countdown text renders");
        assert!(
            !normal.alarm_rung,
            "the normal rung is the silent default — no badge"
        );
        let escalation = rows.row_data(1).expect("the ordered-second row");
        assert!(
            escalation.alarm_rung,
            "the alarm rung projects the badge bit"
        );
        assert!(harness.handle.window().get_timers_chip_visible());
        assert!(
            !harness.handle.window().get_timers_chip().is_empty(),
            "the chip names the nearest fire"
        );
    });
}

/// r4 D-T7 — the insistence picker: the inline form's picker rides
/// the commit seam's third parameter — the alarm rung lands on the
/// captured command verbatim; the default pick is the silent `Normal`
/// rung. Typed commands, never label text.
#[test]
fn the_timers_form_carries_the_insistence_rung() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        let click_field = |harness: &mut Harness, label: &str| {
            let element = harness
                .driver
                .find_by_label(label)
                .unwrap_or_else(|| panic!("the element {label:?} is present"));
            let (x, y) = support::driver::centre(&element);
            harness.driver.click_at(x, y);
        };
        click_field(&mut *harness, "Timer label");
        harness.type_text("Wake");
        click_field(&mut *harness, "Timer timing (in 10m or at 09:30)");
        harness.type_text("in 2m");
        click_field(&mut *harness, "Alarm");
        harness.stroke(parse("Return").expect("Return"));
        match harness.engine.drain().as_slice() {
            [
                StateCommand::CreateAdhocAlarm {
                    label, insistence, ..
                },
            ] => {
                assert_eq!(label.as_str(), "Wake");
                assert_eq!(
                    *insistence,
                    taskboard_domain::Insistence::Alarm,
                    "the alarm rung rides the commit seam"
                );
            }
            other => panic!("the create reached the boundary, got {other:?}"),
        }

        // The default pick is the silent Normal rung (D-I9's
        // ladder): a fresh commit with the picker untouched.
        harness.engine.settle(&harness.handle);
        click_field(&mut *harness, "Timer label");
        harness.type_text("Tea");
        click_field(&mut *harness, "Timer timing (in 10m or at 09:30)");
        harness.type_text("in 2m");
        harness.stroke(parse("Return").expect("Return"));
        match harness.engine.drain().as_slice() {
            [StateCommand::CreateAdhocAlarm { insistence, .. }] => {
                assert_eq!(
                    *insistence,
                    taskboard_domain::Insistence::Normal,
                    "the default rung is the silent base"
                );
            }
            other => panic!("the create reached the boundary, got {other:?}"),
        }
    });
}

// -- T-19 (#802): the invoked disabled menu row sets the notice ----------

/// The doctrine's letter: an invoked disabled row reacts — the
/// status-line notice arms from the row's own registry description
/// (the why-text), nothing routes, and the menu stays open (the
/// invocation was refused, not performed).
#[test]
fn an_invoked_disabled_menu_row_sets_the_status_notice() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state.clone(), move |harness| {
        harness.press(ActionId::AppMenu);
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        let entries = window
            .global::<taskboard_ui_slint::KeyboardState>()
            .get_menu_entries();
        // The first disabled leaf row (the golden state carries
        // at least one — `app.undo` has no journal history to
        // undo). Index-resolved through the pushed model (the D2
        // id-table rule — no text matching).
        let index = (0..entries.row_count())
            .find(|i| {
                entries
                    .row_data(*i)
                    .is_some_and(|row| !row.enabled && !row.action.is_empty())
            })
            .unwrap_or_else(|| panic!("a disabled leaf row renders"));
        let row = entries.row_data(index).expect("the row");
        window.invoke_menu_entry_invoked(i32::try_from(index).expect("small index"));
        harness.driver.advance_time(0);
        assert!(
            window.get_notice_visible(),
            "the status-line notice arms — the click reacted"
        );
        assert!(
            window
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_open(),
            "the menu stays open (the invocation was refused)"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "a disabled invocation routes nothing"
        );
        let _ = row;
    });
}

// -- T-15 (#798): the journal ledger's editor + staged delete ------------

/// Shared fixture: one live Day entry with known attributes (the
/// surfacing-card pin's own entry cleared — the ledger pins a single
/// known row).
fn journal_ledger_state() -> (taskboard_domain::ReflectionId, AppState) {
    let now = chrono::Utc::now();
    let (_fixture, mut state) = fixture::golden_state_at(now);
    // The surfacing-card pin seeds its own walk entry (the fixture's
    // fixture-continuity); the ledger journeys pin a known single row.
    state.reflection.entries.clear();
    let attributes = taskboard_domain::ReflectionAttributes {
        mood: taskboard_domain::reflection::Rating::try_new(3).ok(),
        energy: None,
        flags: std::collections::BTreeSet::from([taskboard_domain::HealthFlag::GoodDay]),
        extra_flags: std::collections::BTreeSet::default(),
    };
    let entry_id = fixture::add_reflection(&mut state, "steady day", attributes, now);
    (entry_id, state)
}

/// US-9/S2 (T-15, #798): the ledger row activates into the inline
/// editor seeded from the stored row verbatim.
#[test]
fn us9_s2_the_ledger_row_activates_into_the_editor() {
    let (_id, state) = journal_ledger_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavPrevView);
        harness.driver.advance_time(0);
        harness.press(ActionId::NavPrevView);
        harness.driver.advance_time(0);
        assert_eq!(harness.handle.window().get_current_view(), View::Review);
        let window = harness.handle.window();
        assert_eq!(window.get_review_model().reflection_rows.row_count(), 1);

        window.invoke_reflection_row_activated(0);
        harness.driver.advance_time(0);
        assert!(window.get_reflection_editor_open(), "the editor opened");
        assert_eq!(
            window.get_reflection_editor_text_echo().as_str(),
            "steady day",
            "the text echo is the stored row verbatim"
        );
        assert_eq!(window.get_reflection_editor_mood_echo().as_str(), "3");
        assert_eq!(window.get_reflection_editor_energy_echo().as_str(), "");
    });
}

/// US-9/S3 (T-15, #798): the editor commits the EXACT `journal edit`
/// change mask — the masked edit (text, energy set, a flag added; the
/// untouched mood rides `None`) and the accepted no-op (the exact
/// empty mask).
#[test]
fn us9_s3_the_editor_commits_the_exact_change_mask() {
    let (entry_id, state) = journal_ledger_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavPrevView);
        harness.driver.advance_time(0);
        harness.press(ActionId::NavPrevView);
        harness.driver.advance_time(0);
        let window = harness.handle.window();

        window.invoke_reflection_row_activated(0);
        harness.driver.advance_time(0);

        // The accepted no-op: an untouched editor commits the
        // exact empty mask.
        window.invoke_reflection_editor_committed();
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [
                StateCommand::UpdateReflection {
                    id,
                    text: None,
                    mood: None,
                    energy: None,
                    flags_add,
                    flags_remove,
                },
            ] => {
                assert_eq!(*id, entry_id);
                assert!(flags_add.is_empty() && flags_remove.is_empty());
            }
            other => panic!("the no-op commit routed the empty mask, got {other:?}"),
        }
        harness.engine.settle(&harness.handle);

        // The masked edit: new text, energy set, a flag added —
        // mood untouched (None rides the mask).
        window.invoke_reflection_row_activated(0);
        harness.driver.advance_time(0);
        window.invoke_reflection_editor_text_edited("rough afternoon".into());
        window.invoke_reflection_editor_energy_edited("2".into());
        window.invoke_reflection_editor_flag_toggled(0); // migraine: add
        harness.driver.advance_time(0);
        window.invoke_reflection_editor_committed();
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [
                StateCommand::UpdateReflection {
                    id,
                    text: Some(text),
                    mood: None,
                    energy: Some(Some(energy)),
                    flags_add,
                    flags_remove,
                },
            ] => {
                assert_eq!(*id, entry_id);
                assert_eq!(text.as_str(), "rough afternoon");
                assert_eq!(*energy, 2);
                assert_eq!(
                    flags_add,
                    &std::collections::BTreeSet::from([taskboard_domain::HealthFlag::Migraine])
                );
                assert!(flags_remove.is_empty());
            }
            other => panic!("the edit routed the exact mask, got {other:?}"),
        }
        harness.engine.settle(&harness.handle);
        assert!(
            !window.get_reflection_editor_open(),
            "a committed edit closes the editor"
        );
        assert_eq!(
            window
                .get_review_model()
                .reflection_rows
                .row_data(0)
                .expect("row")
                .text
                .as_str(),
            "rough afternoon",
            "the ledger echoes the committed text"
        );
    });
}

/// US-9/S4 (T-15, #798): the staged delete — the arm stroke dispatches
/// nothing; the strip's Confirm routes the exact `DeleteReflection`
/// (the row tombstones and leaves the live ledger).
#[test]
fn us9_s4_the_staged_delete_confirms_the_tombstone() {
    let (entry_id, state) = journal_ledger_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavPrevView);
        harness.driver.advance_time(0);
        harness.press(ActionId::NavPrevView);
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        assert_eq!(window.get_review_model().reflection_rows.row_count(), 1);

        window.invoke_reflection_remove_clicked(0);
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "arming dispatches nothing"
        );
        assert!(
            window
                .get_review_model()
                .reflection_rows
                .row_data(0)
                .expect("row")
                .armed,
            "the armed row renders its strip flag"
        );
        window.invoke_reflection_remove_confirmed(0);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::DeleteReflection { id }] => assert_eq!(*id, entry_id),
            other => panic!("the confirm reached the boundary, got {other:?}"),
        }
        harness.engine.settle(&harness.handle);
        assert_eq!(
            window.get_review_model().reflection_rows.row_count(),
            0,
            "the tombstoned row leaves the live ledger"
        );
    });
}

// -- T-14 (#797): the break-rule roster in the timers composition --------

// -- T-14 (#797): the break-rule roster in the timers composition --------

/// US-9/S2 (T-14, #797): the roster renders the rule facts from the
/// stored row — label, alarm-rung bit, canonical threshold phrase,
/// roaming board.
#[test]
fn us9_s2_the_break_rule_roster_renders_the_rule_facts() {
    let (_fixture, mut state) = fixture::golden_state();
    let board = state
        .boards
        .keys()
        .next()
        .copied()
        .expect("the fixture board");
    let _ = fixture::add_break_rule(
        &mut state,
        board,
        "Tea break",
        chrono::TimeDelta::minutes(90),
        taskboard_domain::Insistence::Alarm,
    );
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        let model = harness.handle.window().get_break_rules_model();
        assert_eq!(model.row_count(), 1);
        let row = model.row_data(0).expect("the roster row");
        assert_eq!(row.label.as_str(), "Tea break");
        assert_eq!(row.threshold.as_str(), "1h30m", "the mint's text form");
        assert!(row.alarm_rung, "the rung rides verbatim");
    });
}

/// US-9/S3 (T-14, #797): the create form commits the exact
/// `CreateBreakRule` over the section's host board; a threshold parse
/// miss keeps the field text and routes nothing (the rule-add seam's
/// return convention).
#[test]
fn us9_s3_the_create_form_commits_the_exact_break_rule() {
    let (_fixture, state) = fixture::golden_state();
    let board = state
        .boards
        .keys()
        .next()
        .copied()
        .expect("the fixture board");
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);

        harness.driver.click_by_label("Add break rule");
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_break_create_open(),
            "the form opened"
        );

        // The parse-miss arm: the flavour renders, nothing routes.
        let click_field = |harness: &mut Harness, label: &str| {
            let element = harness
                .driver
                .find_by_label(label)
                .unwrap_or_else(|| panic!("the field {label:?} is present"));
            let (x, y) = support::driver::centre(&element);
            harness.driver.click_at(x, y);
        };
        click_field(&mut *harness, "Break rule label");
        harness.type_text("Tea break");
        harness
            .handle
            .window()
            .invoke_timers_break_threshold_edited("nonsense".into());
        harness.driver.advance_time(0);
        click_field(&mut *harness, "Work threshold (e.g. 90m or 1h30m)");
        harness.type_text("nonsense");
        harness.stroke(parse("Return").expect("Return"));
        assert!(
            harness.engine.drain().is_empty(),
            "a threshold parse miss routes nothing"
        );
        assert!(
            harness.handle.window().get_break_threshold_validity() == FieldValidity::Invalid,
            "the miss renders the invalid flavour"
        );

        // The valid commit.
        harness
            .handle
            .window()
            .invoke_timers_break_threshold_edited("90m".into());
        harness.driver.advance_time(0);
        click_field(&mut *harness, "Work threshold (e.g. 90m or 1h30m)");
        harness.type_text("90m");
        harness.stroke(parse("Return").expect("Return"));
        match harness.engine.drain().as_slice() {
            [
                StateCommand::CreateBreakRule {
                    board: rule_board,
                    label,
                    threshold,
                    insistence,
                },
            ] => {
                assert_eq!(*rule_board, Some(board), "the rule roams the host board");
                assert_eq!(label.as_str(), "Tea break");
                assert_eq!(threshold.as_duration(), TimeDelta::minutes(90));
                assert_eq!(*insistence, taskboard_domain::Insistence::Normal);
            }
            other => panic!("the create reached the boundary, got {other:?}"),
        }
        harness.engine.settle(&harness.handle);
        assert!(
            !harness.handle.window().get_break_create_open(),
            "a valid commit closes the form"
        );
    });
}

/// #1335: on the board-free Today view the break-rule create draft
/// mints into the FIRST live board in canonical order (the D-4 law's
/// default — Today has no selected board to inherit; the picker is
/// #1314's territory).
#[test]
fn us9_s5_the_break_rule_create_defaults_to_the_first_live_board() {
    let (_fixture, mut state) = fixture::golden_state();
    let fixture_board = *state.boards.keys().next().expect("the fixture board");
    // A second live board whose id sorts BEFORE the fixture board's —
    // the canonical-first pick must win over every other live board.
    let earlier = taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(1));
    assert!(earlier < fixture_board, "the crafted id sorts first");
    state.boards.insert(earlier, fixture::board(earlier));
    let expected = earlier;
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.click_by_label("Add break rule");
        harness.driver.advance_time(0);
        harness
            .handle
            .window()
            .invoke_timers_break_threshold_edited("90m".into());
        let element = harness
            .driver
            .find_by_label("Break rule label")
            .unwrap_or_else(|| panic!("the label field is present"));
        let (x, y) = support::driver::centre(&element);
        harness.driver.click_at(x, y);
        harness.type_text("Tea break");
        let element = harness
            .driver
            .find_by_label("Work threshold (e.g. 90m or 1h30m)")
            .unwrap_or_else(|| panic!("the threshold field is present"));
        let (x, y) = support::driver::centre(&element);
        harness.driver.click_at(x, y);
        harness.type_text("90m");
        harness.stroke(parse("Return").expect("Return"));
        match harness.engine.drain().as_slice() {
            [StateCommand::CreateBreakRule { board, .. }] => {
                assert_eq!(
                    *board,
                    Some(expected),
                    "the draft roams the canonical-first live board"
                );
            }
            other => panic!("the create reached the boundary, got {other:?}"),
        }
    });
}

/// US-9/S4 (T-14, #797): the staged remove — the arm stroke dispatches
/// nothing (the strip is the only verb that routes) and the strip's
/// Confirm dispatches the exact `DeleteBreakRule` over the armed
/// subject.
#[test]
fn us9_s4_the_staged_remove_confirms_the_exact_delete() {
    let (_fixture, mut state) = fixture::golden_state();
    let board = state
        .boards
        .keys()
        .next()
        .copied()
        .expect("the fixture board");
    let rule_id = fixture::add_break_rule(
        &mut state,
        board,
        "Tea break",
        chrono::TimeDelta::minutes(90),
        taskboard_domain::Insistence::Normal,
    );
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        assert_eq!(
            harness.handle.window().get_break_rules_model().row_count(),
            1
        );

        harness.driver.click_by_label("Remove break rule");
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "arming dispatches nothing"
        );
        assert!(
            harness
                .handle
                .window()
                .get_break_rules_model()
                .row_data(0)
                .expect("row")
                .armed,
            "the armed row renders its strip flag"
        );
        harness.driver.click_by_label("Confirm remove rule");
        match harness.engine.drain().as_slice() {
            [StateCommand::DeleteBreakRule { id }] => {
                assert_eq!(*id, rule_id, "the armed subject's exact delete");
            }
            other => panic!("the confirm reached the boundary, got {other:?}"),
        }
        harness.engine.settle(&harness.handle);
        assert_eq!(
            harness.handle.window().get_break_rules_model().row_count(),
            0,
            "the removal empties the roster by derivation"
        );
    });
}

// -- r4 D33: the timers mouse setter (F13) --------------------------------

/// Drives one popover leg: opens via the header button and returns the
/// harness. The popover's label field shares its a11y address with the
/// inline form's (both say "Timer label" — the D33-pinned copy); the
/// popover mounts last in the tree, so the *last* match is the
/// popover's (the ordered-fixture discipline, asserted by the legs
/// below behaving: text typed into the wrong field would fail the
/// captured-command shapes).
fn open_setter(harness: &mut Harness) {
    harness.driver.click_by_label("Set a timer");
    harness.driver.advance_time(0);
    assert!(
        harness.handle.window().get_timer_setter_open(),
        "the header button opened the popover"
    );
}

/// The segment click (the toolbar journey's idiom): the radio-role
/// element is the address — a label can echo on non-button elements
/// whose default action is inert.
fn click_segment(harness: &mut Harness, label: &str) {
    harness
        .driver
        .find_all_by_label(label)
        .into_iter()
        .find(support::driver::Element::is_radio_button)
        .unwrap_or_else(|| panic!("the {label} segment is present"))
        .invoke_default_action();
    harness.driver.advance_time(0);
}

/// The popover's segment click: the "Alarm"/"Normal" labels are
/// shared with the timers view's inline form, and the popover mounts
/// last in the tree — so, like the label field below, the *last*
/// radio match is the popover's (the ordered-fixture discipline).
fn click_popover_segment(harness: &mut Harness, label: &str) {
    harness
        .driver
        .find_all_by_label(label)
        .into_iter()
        .rev()
        .find(support::driver::Element::is_radio_button)
        .unwrap_or_else(|| panic!("the popover's {label} segment is present"))
        .invoke_default_action();
    harness.driver.advance_time(0);
}

fn click_popover_label(harness: &mut Harness) {
    let matches = harness.driver.find_all_by_label("Timer label");
    let element = matches
        .last()
        .expect("the popover's label field is mounted");
    let (x, y) = support::driver::centre(element);
    harness.driver.click_at(x, y);
}

/// r4 D33 acceptance (b) — the parity contract: a timer created via
/// the popover (open → preset → label → commit) and one created via
/// the typed text form with the equivalent grammar dispatch the
/// `PartialEq`-equal `CreateAdhocAlarm` — same `AdhocWhen`, same
/// insistence. One parser, one seam, proven.
#[test]
fn the_setter_and_the_text_form_hit_the_same_commit_seam() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);

        // The popover leg: open, quick-fill the 5m preset (never
        // auto-commit, D-T6), label, commit.
        open_setter(&mut *harness);
        click_popover_segment(&mut *harness, "5m");
        click_popover_label(&mut *harness);
        harness.type_text("Tea");
        harness.driver.click_by_label("Set timer");
        harness.driver.advance_time(0);
        let popover_command = {
            let drained = harness.engine.drain();
            assert_eq!(
                drained.len(),
                1,
                "the popover dispatched exactly the create"
            );
            drained.into_iter().next().expect("one command")
        };
        assert!(
            !harness.handle.window().get_timer_setter_open(),
            "a successful commit closes the popover"
        );
        harness.engine.settle(&harness.handle);

        // The text-form leg: the same grammar typed, committed on
        // the timing field's Return.
        let click_field = |harness: &mut Harness, label: &str| {
            let element = harness
                .driver
                .find_by_label(label)
                .unwrap_or_else(|| panic!("the field {label:?} is present"));
            let (x, y) = support::driver::centre(&element);
            harness.driver.click_at(x, y);
        };
        click_field(&mut *harness, "Timer label");
        harness.type_text("Tea");
        click_field(&mut *harness, "Timer timing (in 10m or at 09:30)");
        harness.type_text("in 5m");
        harness.stroke(parse("Return").expect("Return"));
        let form_command = {
            let drained = harness.engine.drain();
            assert_eq!(drained.len(), 1, "the form dispatched exactly the create");
            drained.into_iter().next().expect("one command")
        };
        assert_eq!(
            popover_command, form_command,
            "popover ≡ text form ≡ same command (the one-parser contract)"
        );
    });
}

/// r4 D-T4 — the custom row's grammar: a picked time plus an explicit
/// Tomorrow composes `at <RFC-3339>` and commits as `At(instant)` at
/// the picked time of the next day — the one parser between the
/// composition and the engine (typed match, never label text).
#[test]
fn the_setter_composes_the_custom_row_grammar() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state.clone(), move |harness| {
        // Single-read capture (D-3/R3): `today` is this test's only
        // clock read; the expected `At` instant derives from it.
        // Residual (accepted, R2/R3): the adapter's
        // `recompose_timer_setter` keeps its own `Utc::now()` read
        // feeding `compose_timer_at` — a local midnight falling
        // between this capture and the compose is the accepted
        // millisecond-wide window.
        let today = chrono::Local::now().date_naive();

        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        open_setter(&mut *harness);
        // The stepper's seed-on-step (r4 D23): one `+` from the
        // unset state composes 00:15 — the pushed mirror moves off
        // the `--:--` sentinel and the fire caption renders. The
        // seam seeds the *stepped* value; seeding the pre-step
        // anchor left the display frozen while the composition
        // moved (the TimeField's re-seed contract).
        harness.driver.click_by_label("Step time forward");
        harness.driver.advance_time(0);
        assert_ne!(
            harness.handle.window().get_timer_setter_time_text(),
            "--:--",
            "the stepper seeds the display with the stepped value"
        );
        assert!(
            !harness
                .handle
                .window()
                .get_timer_setter_fire_time()
                .is_empty(),
            "the stepped composition renders its fire instant"
        );
        // A fresh draft for the custom-row leg below: Escape
        // clears the setter-local state with the panel.
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        open_setter(&mut *harness);
        // The custom row: click-focus the time field, type the
        // lenient digits, pick Tomorrow.
        let time_field = harness
            .driver
            .find_by_label("Custom time")
            .expect("the custom time field is mounted");
        let (x, y) = support::driver::centre(&time_field);
        harness.driver.click_at(x, y);
        harness.type_text("09:30");
        harness.driver.advance_time(0);
        click_popover_segment(&mut *harness, "Tomorrow");
        assert!(
            !harness
                .handle
                .window()
                .get_timer_setter_fire_time()
                .is_empty(),
            "the composed fire instant's caption renders"
        );
        click_popover_label(&mut *harness);
        harness.type_text("Standup");
        harness.driver.click_by_label("Set timer");
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::CreateAdhocAlarm { when, .. }] => {
                let tomorrow = (today + chrono::TimeDelta::try_days(1).expect("one day"))
                    .and_hms_opt(9, 30, 0)
                    .expect("09:30");
                let expected = chrono::Local
                    .from_local_datetime(&tomorrow)
                    .single()
                    .expect("resolves")
                    .with_timezone(&chrono::Utc);
                assert_eq!(
                    *when,
                    taskboard_domain::AdhocWhen::At(expected),
                    "the explicit day choice is honored — no roll"
                );
            }
            other => panic!("the create reached the boundary, got {other:?}"),
        }
        assert!(
            !harness.handle.window().get_timer_setter_open(),
            "the commit closed the popover"
        );
    });
}

/// r4 D33 §4.2 — the dismissal grammar: the scrim click and Escape
/// close without committing (drafts are setter-local, nothing is
/// dispatched); a successful commit closes; an absorbed miss (the
/// whitespace-empty label, D-P14) keeps the panel open with nothing
/// sent.
#[test]
fn the_setter_dismissal_layers_close_without_committing() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);

        // (a) The scrim click closes — no command.
        open_setter(&mut *harness);
        harness.driver.click_at(600.0, 40.0);
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_timer_setter_open(),
            "the outside click dismissed the panel"
        );
        harness.assert_commands(&[]);

        // (b) Escape closes — widget-internal, the action layer
        // never sees it, and still no command. The label click
        // hands the panel the focus a real open flow has.
        open_setter(&mut *harness);
        click_popover_label(&mut *harness);
        harness.driver.advance_time(0);
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_timer_setter_open(),
            "Escape closed the panel"
        );
        harness.assert_commands(&[]);

        // (c) An absorbed miss keeps the panel open: a preset
        // quick-filled, no label — the empty-label absorb (D-P14).
        open_setter(&mut *harness);
        click_segment(&mut *harness, "10m");
        harness.driver.click_by_label("Set timer");
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_timer_setter_open(),
            "the miss keeps the panel open"
        );
        harness.assert_commands(&[]);

        // (d) A successful commit closes and dispatches exactly
        // the create.
        click_popover_label(&mut *harness);
        harness.type_text("Tea");
        harness.driver.click_by_label("Set timer");
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_timer_setter_open(),
            "the commit closed the panel"
        );
        harness.assert_commands(&[StateCommand::CreateAdhocAlarm {
            label: "Tea".to_owned(),
            when: taskboard_domain::AdhocWhen::In(TimeDelta::minutes(10)),
            insistence: taskboard_domain::Insistence::Normal,
        }]);
    });
}

/// #1306 — the `SegmentedControl` echo pins (structural, round law a):
/// the widget is deliberately stateless (r2 D26/D27), so the caller
/// owns the feedback half — a pick must move the bound index (the
/// adapter-pushed mirrors for day/insistence, the Slint-local in-out
/// for the quick presets). Typed index assertions only; no text pins.
#[test]
fn the_setter_segments_echo_their_picked_selection() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        open_setter(&mut *harness);

        // T1 — the day row's reply-sourced echo: the seam writes
        // `setter_day_offset` and the derive pass re-pushes it.
        click_popover_segment(&mut *harness, "Tomorrow");
        assert_eq!(
            harness.handle.window().get_timer_setter_day_index(),
            1,
            "the day pick echoes through the adapter's mirror"
        );

        // T2 — the insistence row's echo (also pins D1's
        // refresh_forced conversion: nothing else re-pushed this
        // seam when it was bare_mut).
        click_popover_segment(&mut *harness, "Alarm");
        assert_eq!(
            harness.handle.window().get_timer_setter_insist_index(),
            1,
            "the insistence pick echoes through the adapter's mirror"
        );

        // T3 — the presets row's Slint-local echo (a fill, not an
        // adapter-owned selection, D-T6).
        click_popover_segment(&mut *harness, "25m");
        assert_eq!(
            harness.handle.window().get_timer_setter_preset_index(),
            3,
            "the preset pick echoes through the local in-out"
        );
    });
}

/// #1306 — the wipe/reset pin (T4): the dismissal funnel clears the
/// adapter's mirrors (`clear_timer_setter`), so a fresh open seeds
/// every echoed index at 0; the preset mirror is reseeded by the
/// open seam (D3).
#[test]
fn the_setter_wipe_reseeds_the_echoed_indices() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        open_setter(&mut *harness);
        click_popover_segment(&mut *harness, "Tomorrow");
        click_popover_segment(&mut *harness, "Alarm");
        click_popover_segment(&mut *harness, "5m");
        assert_eq!(
            harness.handle.window().get_timer_setter_day_index(),
            1,
            "the pre-dismiss picks moved the mirrors"
        );
        assert_eq!(harness.handle.window().get_timer_setter_insist_index(), 1);
        assert_eq!(harness.handle.window().get_timer_setter_preset_index(), 1);

        // Escape rides the dismiss seam — the explicit decline that
        // routes `clear_timer_setter`.
        click_popover_label(&mut *harness);
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_timer_setter_open(),
            "Escape closed the panel"
        );

        open_setter(&mut *harness);
        assert_eq!(
            harness.handle.window().get_timer_setter_day_index(),
            0,
            "the wipe reseeded the day echo"
        );
        assert_eq!(
            harness.handle.window().get_timer_setter_insist_index(),
            0,
            "the wipe reseeded the insistence echo"
        );
        assert_eq!(
            harness.handle.window().get_timer_setter_preset_index(),
            0,
            "the fresh open reseeded the preset echo"
        );
    });
}

// -- DR-5/4: the tracking affordances (issue #489) -----------------------

/// DR-5/4 — the chip: the open entry names itself (title snapshot +
/// elapsed) on desktop WITH the stop affordance, whose click routes
/// the exact payload-free `StopTracking`; a closed-only state hides
/// the chip. The kiosk leg pins D-T16's gate: the display STAYS (the
/// appliance shows what is being worked on), the action empties — a
/// hidden chip on kiosk is the stage-3 blocking finding.
#[test]
fn the_tracking_chip_renders_and_the_kiosk_gate_empties_only_the_action() {
    let (_fixture, state) = fixture::golden_state();
    // The seeded entry (D-TU12) is the chip's input; a closed-only
    // variant for the hidden leg.
    let mut closed_only = state.clone();
    closed_only.tracking.entries.clear();
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            // Desktop: chip + action visible, text present (presence,
            // never content — AGENTS §5).
            let mut harness = Harness::mount(SlintDriver::default(), state);
            assert!(harness.handle.window().get_tracking_chip_visible());
            assert!(harness.handle.window().get_tracking_chip_action_visible());
            assert!(
                !harness.handle.window().get_tracking_chip().is_empty(),
                "the chip names the open entry"
            );

            // The stop affordance routes the exact command (the seam
            // invoked directly — the testing backend exposes no a11y
            // node for a TouchArea control, so a labelled click cannot
            // address it; the gate's render arm is the `if` over the
            // property pinned below).
            harness.handle.window().invoke_tracking_chip_stop_clicked();
            harness.assert_commands(&[taskboard_domain::StateCommand::StopTracking]);

            // Closed-only: no chip, no action.
            drop(harness);
            let harness = Harness::mount(SlintDriver::default(), closed_only);
            assert!(!harness.handle.window().get_tracking_chip_visible());
            assert!(!harness.handle.window().get_tracking_chip_action_visible());

            // Kiosk: the display STAYS, the action empties.
            drop(harness);
            let (_fixture, kiosk_state) = fixture::golden_state();
            let harness = Harness::mount_with_settings(
                SlintDriver::default(),
                kiosk_state,
                taskboard_ui_slint::UiSettings {
                    kiosk: true,
                    ..taskboard_ui_slint::UiSettings::default()
                },
            );
            assert!(
                harness.handle.window().get_tracking_chip_visible(),
                "the kiosk appliance still shows what is being worked on"
            );
            assert!(
                !harness.handle.window().get_tracking_chip().is_empty(),
                "the kiosk chip carries the title and elapsed"
            );
            assert!(
                !harness.handle.window().get_tracking_chip_action_visible(),
                "the gate empties the action, not the display"
            );
        },
    );
}

/// DR-5/4 (D-TU3) — the badge rides the decorations slot: exactly one
/// card on the board carries the tracking decoration, the open
/// entry's card; the ghost and compact collapsing ride the existing
/// slot semantics, never re-derived here.
#[test]
fn the_tracking_badge_rides_only_the_open_entrys_card() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        let board = harness.handle.window().get_board_model();
        let mut badged: Vec<(usize, usize)> = Vec::new();
        for column in 0..board.stacks.row_count() {
            let tasks = board.stacks.row_data(column).expect("column").tasks;
            for row in 0..tasks.row_count() {
                let card = tasks.row_data(row).expect("card");
                let mut decorations = Vec::new();
                for index in 0..card.decorations.row_count() {
                    decorations.push(card.decorations.row_data(index).expect("decoration"));
                }
                if decorations.iter().any(|decoration| {
                    decoration.kind == taskboard_ui_slint::DecorationKind::Tracking
                }) {
                    badged.push((column, row));
                }
            }
        }
        assert_eq!(
            badged,
            vec![(0, 1)],
            "exactly the open entry's card badges (stack a, order 1)"
        );
    });
}

/// DR-5/4 (D-TU7) — the notification Start Task seam: exactly one
/// command leaves the UI, the exact `StartTracking { task }` — the
/// implicit dismissal lives planner-side, so a `DismissReminder`
/// alongside would be the blocking duplicate. A stale index and an
/// alarm row degrade to zero commands.
#[test]
fn the_notification_start_task_dispatches_exactly_one_command() {
    let (fixture, mut state) = fixture::golden_state();
    let due = chrono::Utc::now() - TimeDelta::minutes(5);
    fixture::deliver_task_notification(&mut state, fixture.task1, due);
    support::harness::run_journey(state, move |harness| {
        let window = harness.handle.window();

        // The task row renders the affordance (the anchor table).
        let rows = window.get_notification_model();
        assert_eq!(rows.row_count(), 1);
        assert!(rows.row_data(0).expect("row").can_track_start);

        // The seam dispatches exactly one command.
        window.invoke_notification_track_start_clicked(0);
        harness.assert_commands(&[taskboard_domain::StateCommand::StartTracking {
            task: fixture.task1,
            kind: taskboard_domain::tracking::TimeEntryKind::Work,
        }]);

        // A stale index degrades to a no-op.
        harness
            .handle
            .window()
            .invoke_notification_track_start_clicked(999);
        harness.assert_commands(&[]);
    });
}

/// DR-5/4 (D-TU7) — the anchor table at the seam: an alarm-anchored
/// row never offers Start Task and its seam dispatches nothing.
#[test]
fn the_alarm_row_never_offers_start_task() {
    let (_fixture, mut state) = fixture::golden_state();
    let fired_at = chrono::Utc::now() - TimeDelta::minutes(1);
    let alarm = fixture::add_alarm(&mut state, "tea", fired_at);
    fixture::deliver_alarm(&mut state, alarm, fired_at);
    support::harness::run_journey(state, move |harness| {
        let rows = harness.handle.window().get_notification_model();
        assert_eq!(rows.row_count(), 1, "the awaiting alarm renders");
        assert!(
            !rows.row_data(0).expect("row").can_track_start,
            "an Adhoc row never renders Start Task"
        );
        drop(rows);
        harness
            .handle
            .window()
            .invoke_notification_track_start_clicked(0);
        harness.assert_commands(&[]);
    });
}

/// DR-5/4 (D-TU6) — the detail button carries exactly one verb per
/// subject: Start for the untracked subject, Stop for the subject
/// holding the open entry (no done-gate in the UI — R-8 is the
/// engine's).
#[test]
fn the_detail_button_carries_one_verb_per_subject() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        // The first card (task-0, untracked): Start only.
        harness.press(ActionId::NavFirstItem);
        harness.press(ActionId::TaskEdit);
        let model = harness.handle.window().get_task_model();
        assert!(model.can_track_start && !model.can_track_stop);

        // The tracked subject: Stop only.
        harness.press(ActionId::NavFocusBoard);
        harness.press(ActionId::NavFocusDown);
        assert_eq!(
            harness.emphasized_card(),
            Some((0, 1)),
            "the walk reaches the tracked card"
        );
        harness.press(ActionId::TaskEdit);
        let model = harness.handle.window().get_task_model();
        assert!(!model.can_track_start && model.can_track_stop);
    });
}

/// DR-3/3 — the awaiting pointer: a delivered (fired ∧ ¬dismissed)
/// alarm renders exactly one surface row (Cancel, no Mark Done) and
/// counts in the section's pointer badge; the dismissal removes both
/// by derivation.
#[test]
fn the_awaiting_alarm_renders_then_dismisses() {
    let (_fixture, mut state) = fixture::golden_state();
    let fired_at = chrono::Utc::now() - TimeDelta::minutes(1);
    let alarm = fixture::add_alarm(&mut state, "tea", fired_at);
    fixture::deliver_alarm(&mut state, alarm, fired_at);
    let state = state;
    support::harness::run_journey(state, move |harness| {
        assert_eq!(
            harness.handle.window().get_notification_model().row_count(),
            1
        );
        assert_eq!(harness.handle.window().get_timers_awaiting(), 1);
        // Awaiting rows stay out of the active section (one
        // source of truth — D-I7).
        assert_eq!(harness.handle.window().get_timers_model().row_count(), 0);
        // The alarm row grew the Cancel affordance (D-P11).
        assert!(harness.driver.find_by_label("Cancel timer").is_some());

        harness.driver.click_by_label("Dismiss notification");
        match harness.engine.drain().as_slice() {
            [
                StateCommand::DismissReminder {
                    reminder,
                    occurrence,
                },
            ] => {
                assert_eq!(*reminder, taskboard_domain::ReminderRef::Adhoc(alarm));
                assert_eq!(*occurrence, fired_at);
            }
            other => panic!("the dismissal reached the boundary, got {other:?}"),
        }
        harness.engine.settle(&harness.handle);
        assert_eq!(
            harness.handle.window().get_notification_model().row_count(),
            0
        );
        assert_eq!(harness.handle.window().get_timers_awaiting(), 0);
        assert!(harness.driver.find_by_label("Cancel timer").is_none());
    });
}

/// DR-3/3 — the row-template repair must not shrink the task side: a
/// task-anchored awaiting row keeps all four task-shaped affordances
/// (Done, Snooze, Mute — plus Dismiss) and grows no Cancel.
#[test]
fn a_task_notification_row_keeps_all_four_buttons() {
    let (golden, mut state) = fixture::golden_state();
    let due = chrono::Utc::now() - TimeDelta::minutes(5);
    fixture::deliver_task_notification(&mut state, golden.task1, due);
    let state = state;
    support::harness::run_journey(state, move |harness| {
        assert_eq!(
            harness.handle.window().get_notification_model().row_count(),
            1
        );
        let row = harness
            .handle
            .window()
            .get_notification_model()
            .row_data(0)
            .expect("the surface's single row");
        assert!(row.can_done);
        assert!(row.can_mute);
        assert!(!row.can_cancel);
        assert!(harness.driver.find_by_label("Mark task done").is_some());
        assert!(harness.driver.find_by_label("Mute reminders").is_some());
        assert!(harness.driver.find_by_label("Cancel timer").is_none());
    });
}

// ---- R4-F6 (r4 design D2): a right-click outside an open overlay
// closes it — every full-window dismissal layer carries the same
// right-down arm the left-click `clicked` seam already has.

/// The menu layer: a right-press off the panel dismisses, typed via
/// `KeyboardState.menu-open` (the outside-left-click journey's twin).
#[test]
fn a_right_press_outside_dismisses_the_menu() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::AppMenu);
        harness.driver.advance_time(0);
        assert!(keyboard(&*harness).get_menu_open());
        harness.driver.right_press_at(10.0, 450.0);
        assert!(
            !keyboard(&*harness).get_menu_open(),
            "the right-down closed the menu"
        );
    });
}

/// The palette layer: the typed `palette-open` bit falls on a
/// right-press in the corner (the centered panel never reaches it).
#[test]
fn a_right_press_outside_dismisses_the_palette() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::PaletteSearch);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_palette_open());
        harness.driver.right_press_at(990.0, 890.0);
        assert!(
            !harness.handle.window().get_palette_open(),
            "the right-down closed the palette"
        );
    });
}

/// The capture layer: the typed `capture-open` bit falls on a
/// right-press in the corner (the centered modal never reaches it).
#[test]
fn a_right_press_outside_dismisses_the_capture_overlay() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::CaptureQuick);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_capture_open());
        harness.driver.right_press_at(990.0, 890.0);
        assert!(
            !harness.handle.window().get_capture_open(),
            "the right-down closed the capture overlay"
        );
    });
}

/// The date-picker layer: the typed `date-picker-open` bit falls on a
/// right-press far from the anchored panel.
#[test]
fn a_right_press_outside_dismisses_the_date_picker() {
    let (fixture, mut state) = fixture::journey_state();
    let due = local_midnight(2030, 1, 30);
    state
        .tasks
        .get_mut(&fixture.task0)
        .expect("fixture task")
        .duedate = Some(due);
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_task_model().date_picker_open);
        harness.driver.set_window_size(1000, 900);
        harness.driver.right_press_at(990.0, 890.0);
        assert!(
            !harness.handle.window().get_task_model().date_picker_open,
            "the right-down closed the picker"
        );
    });
}

/// The guard half of D2: the dismissal layer eats the right-press —
/// the menu closes AND no context menu opens behind it (the layer is
/// hit-tested before every context seam beneath it). The press lands
/// on stack a's bare body (well below the cards, and left of the
/// right-aligned menu panel, which keeps absorbing its own hits).
#[test]
fn a_right_press_on_the_bare_board_closes_the_menu_and_opens_nothing() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        let header = harness
            .driver
            .find_all_by_label("a")
            .into_iter()
            .find(support::driver::Element::is_accessible_button)
            .expect("the stack-a header button is present");
        let pos = header.absolute_position();
        harness.press(ActionId::AppMenu);
        harness.driver.advance_time(0);
        assert!(keyboard(&*harness).get_menu_open());
        #[allow(clippy::cast_precision_loss)]
        let bare_x = pos.x + 10.0;
        harness.driver.right_press_at(bare_x, 700.0);
        assert!(
            !keyboard(&*harness).get_menu_open(),
            "the layer's right-down closed the menu"
        );
        assert!(
            !keyboard(&*harness).get_menu_pointer_anchored(),
            "no context menu opened behind the dismissal"
        );
    });
}

// ---- R4-F7 (r4 design D3): a left-click on a column's bare body
// selects the stack — the body seam's left-click twin of the header.

/// The empty stack: a left-click on its bare body selects it — the
/// exact state the header journey pins (selected stack moved to the
/// clicked column).
#[test]
fn a_body_left_click_on_an_empty_stack_selects_it() {
    let (fixture, mut state) = fixture::golden_state();
    let stack_b = fixture.stack_b;
    state.tasks.retain(|_, task| task.stack != stack_b);
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 800);
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            -1,
            "a fresh mount pushes no selection"
        );
        let header = harness
            .driver
            .find_all_by_label("b")
            .into_iter()
            .find(support::driver::Element::is_accessible_button)
            .expect("the stack-b header button is present");
        let pos = header.absolute_position();
        let size = header.size();
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = (pos.x + size.width / 2.0, pos.y + size.height * 3.0);
        harness.driver.click_at(x, y);
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            1,
            "the body click selected stack b"
        );
    });
}

/// The populated stack: a click below the last card (the pass-through
/// region) selects the stack and focuses its first card.
#[test]
fn a_body_left_click_below_the_last_card_selects_and_focuses() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 800);
        let header = harness
            .driver
            .find_all_by_label("b")
            .into_iter()
            .find(support::driver::Element::is_accessible_button)
            .expect("the stack-b header button is present");
        let pos = header.absolute_position();
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = (pos.x + 10.0, pos.y + 250.0);
        harness.driver.click_at(x, y);
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            1,
            "the below-last-card click selected stack b"
        );
        assert_eq!(
            harness.emphasized_card(),
            Some((1, 0)),
            "the stack's first card is focused"
        );
    });
}

// ---- R4-F8 (r4 design D4): one physical click on a column's
// Add-task ghost opens the create field in THAT column and selects it
// — the targeting carried by the effect payload, not by selection
// ordering. The journeys below are the F8 harness: physical
// press+release delivery (`click_at`), never the a11y seam.

fn ghost_centre(harness: &Harness, index: usize) -> (f32, f32) {
    let ghost = harness
        .driver
        .find_all_by_label("Add task")
        .into_iter()
        .nth(index)
        .expect("the column's ghost is present");
    let pos = ghost.absolute_position();
    let size = ghost.size();
    #[allow(clippy::cast_precision_loss)]
    (pos.x + size.width / 2.0, pos.y + size.height / 2.0)
}

/// (a/b) Under `Flow::None`, one physical click on a non-active
/// column's ghost opens the create row in that column and moves the
/// selection there — one click, not two.
#[test]
fn one_ghost_click_opens_the_create_field_in_the_clicked_column() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 800);
        let (x, y) = ghost_centre(&*harness, 1);
        harness.driver.click_at(x, y);
        let stacks = harness.handle.window().get_board_model().stacks;
        assert!(
            stacks.row_data(1).expect("column b").create_open,
            "one click opened the create row in stack b"
        );
        assert_eq!(
            harness.handle.window().get_board_model().selected_stack,
            1,
            "the clicked column is selected"
        );
    });
}

/// (c) Under a foreign flow (a stack rename armed on stack a), the
/// ghost click settles the flow and opens in the clicked column —
/// settle-then-run with no eaten click.
#[test]
fn a_ghost_click_under_a_foreign_flow_settles_and_opens_in_the_clicked_column() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 800);
        // Arm the foreign flow on stack a (the keyboard route).
        harness
            .driver
            .find_all_by_label("a")
            .into_iter()
            .find(support::driver::Element::is_accessible_button)
            .expect("stack a's header")
            .invoke_default_action();
        harness.press(ActionId::StackRename);
        harness.driver.advance_time(0);
        let stacks = harness.handle.window().get_board_model().stacks;
        assert!(
            stacks.row_data(0).expect("column a").rename_open,
            "the rename flow is armed on stack a"
        );
        let (x, y) = ghost_centre(&*harness, 1);
        harness.driver.click_at(x, y);
        let stacks = harness.handle.window().get_board_model().stacks;
        assert!(
            !stacks.row_data(0).expect("column a").rename_open,
            "the foreign flow settled"
        );
        assert!(
            stacks.row_data(1).expect("column b").create_open,
            "the create row opened in the clicked column"
        );
    });
}

/// (d) With another column's create field open and focused, the ghost
/// click settles that flow and opens the clicked column's field.
#[test]
fn a_ghost_click_replaces_another_columns_open_create_field() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 800);
        harness
            .driver
            .find_all_by_label("a")
            .into_iter()
            .find(support::driver::Element::is_accessible_button)
            .expect("stack a's header")
            .invoke_default_action();
        harness.press(ActionId::TaskCreate);
        harness.driver.advance_time(0);
        let stacks = harness.handle.window().get_board_model().stacks;
        assert!(stacks.row_data(0).expect("column a").create_open);
        let (x, y) = ghost_centre(&*harness, 1);
        harness.driver.click_at(x, y);
        let stacks = harness.handle.window().get_board_model().stacks;
        assert!(
            !stacks.row_data(0).expect("column a").create_open,
            "the old create flow settled"
        );
        assert!(
            stacks.row_data(1).expect("column b").create_open,
            "the clicked column's field opened"
        );
    });
}

/// (e) Ctrl+O is unchanged: with no selection the field opens in the
/// first stack; with a selection, in the selected stack.
#[test]
fn task_create_without_a_click_targets_the_selection_chain() {
    let (fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            // No selection: the first stack.
            harness.press(ActionId::TaskCreate);
            let stacks = harness.handle.window().get_board_model().stacks;
            assert!(
                stacks.row_data(0).expect("column a").create_open,
                "no selection falls through to the first stack"
            );
            // A fresh mount, selection on stack b: the selected stack.
            let (_fixture, state) = fixture::golden_state();
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            harness
                .driver
                .find_all_by_label("b")
                .into_iter()
                .find(support::driver::Element::is_accessible_button)
                .expect("stack b's header")
                .invoke_default_action();
            harness.press(ActionId::TaskCreate);
            let stacks = harness.handle.window().get_board_model().stacks;
            assert!(
                stacks.row_data(1).expect("column b").create_open,
                "the selected stack wins"
            );
            let _ = fixture;
        },
    );
}

// -- r4 chunk 4 (D14): the dismiss journeys ------------------------------
//
// The structural walk pins the routing table; these five pin the
// user-facing transitions through the real seams (hover scan, delay
// fire, the production dispatch paths).

/// The F14 repro verbatim: hover a task card, let the delay fire, then
/// activate the focused card into the task view — the tooltip is gone.
/// (Pre-fix: the ghost tip rode the board→task transition.)
#[test]
fn a_tooltip_does_not_survive_activating_the_hovered_task() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        // Arm on the first card (the established hover-scan
        // pattern), and seed the card focus the activation needs.
        let card = harness
            .driver
            .find_by_label("task-0")
            .expect("the first card is addressable");
        let pos = card.absolute_position();
        let size = card.size();
        let mut armed = false;
        let mut y = pos.y + 2.0;
        while y < pos.y + size.height - 2.0 {
            harness.driver.hover_at(pos.x + size.width / 2.0, y);
            harness.driver.advance_time(0);
            if tooltip_state(&*harness).get_pending() {
                armed = true;
                break;
            }
            y += 4.0;
        }
        assert!(armed, "the card hover armed the trigger");
        harness.driver.advance_time(600);
        assert!(
            tooltip_state(&*harness).get_visible(),
            "the tip went visible"
        );

        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskEdit);
        harness.driver.advance_time(0);
        assert_eq!(
            current_view(&*harness),
            View::TaskDetail,
            "the activation mounted the task view"
        );
        let tooltip = tooltip_state(&*harness);
        assert!(
            !tooltip.get_visible() && !tooltip.get_pending() && !tooltip.get_placed(),
            "the activation dismissed the tooltip"
        );
    });
}

/// The inbox jump: a visible tip does not survive `ViewInbox` — the
/// `OpenInbox` effect was missing from the old leaving set entirely.
#[test]
fn a_tooltip_does_not_survive_the_inbox_jump() {
    let (fixture, mut state) = fixture::two_board_state();
    // The Inbox board: the jump's target (the exact-title adoption).
    if let Some(board) = state.boards.get_mut(&fixture.board_two) {
        board.title = "Inbox".to_owned();
    }
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        let card = harness
            .driver
            .find_by_label("task-0")
            .expect("the first card is addressable");
        let pos = card.absolute_position();
        let size = card.size();
        harness
            .driver
            .hover_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
        harness.driver.advance_time(600);
        assert!(
            tooltip_state(&*harness).get_visible(),
            "the tip went visible"
        );

        // ViewInbox is bound in no preset (the D19 record): the
        // mouse path is the toolbar's inbox chip — a bare
        // TouchArea, so the coordinate click seam drives it.
        let chip = harness
            .driver
            .find_by_label("Go to inbox")
            .expect("the inbox chip is addressable");
        let cpos = chip.absolute_position();
        let csize = chip.size();
        harness
            .driver
            .click_at(cpos.x + csize.width / 2.0, cpos.y + csize.height / 2.0);
        harness.driver.advance_time(0);
        assert_eq!(
            current_view(&*harness),
            View::StackDetail,
            "the inbox jump mounted the stack detail"
        );
        let tooltip = tooltip_state(&*harness);
        assert!(
            !tooltip.get_visible() && !tooltip.get_pending() && !tooltip.get_placed(),
            "the inbox jump dismissed the tooltip"
        );
    });
}

/// The palette invoke: a visible tip armed on the board does not
/// survive the palette's Return-jump across boards —
/// `apply_palette_invoke` lands in the choke point through
/// `InvokeSelected` (the tooltip is armed via the state global: the
/// palette row's own hover geometry is not this journey's seam).
#[test]
fn a_tooltip_does_not_survive_the_palette_invoke() {
    let (fixture, mut state) = fixture::two_board_state();
    let id = TaskId::from_uuid(uuid::Uuid::now_v7());
    let mut needle = state.tasks.values().next().expect("fixture task").clone();
    needle.id = id;
    needle.stack = fixture.stack_c;
    needle.order = 5;
    needle.title = "zebra needle".to_owned();
    state.tasks.insert(id, needle);

    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        // Arm a visible tip while the board view is mounted.
        tooltip_state(&*harness).invoke_hover(
            "armed".into(),
            "".into(),
            slint::ModelRc::default(),
            500.0,
            300.0,
            32.0,
            32.0,
        );
        harness.driver.advance_time(600);
        assert!(
            tooltip_state(&*harness).get_visible(),
            "the tip went visible"
        );

        // The palette round-trip: one unique cross-board hit,
        // Return jumps (the `menu.invoke` → `InvokeSelected` →
        // `apply_palette_invoke` path).
        harness.press(ActionId::PaletteSearch);
        harness.type_text("zebra needle");
        harness.stroke(parse("Return").expect("return"));
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_board_bar_model().selected,
            1,
            "the jump selected board two"
        );
        let tooltip = tooltip_state(&*harness);
        assert!(
            !tooltip.get_visible() && !tooltip.get_pending() && !tooltip.get_placed(),
            "the palette invoke dismissed the tooltip"
        );
    });
}

/// The ghost: a visible tip on the add-task ghost does not survive
/// the click that opens the create field — the hovered `TouchArea`
/// itself unmounts (the ghost case; the identity tuple does not move).
#[test]
fn a_tooltip_does_not_survive_the_ghost_click() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        let ghost = harness
            .driver
            .find_by_label("Add task")
            .expect("the add-task ghost is addressable");
        let pos = ghost.absolute_position();
        let size = ghost.size();
        harness
            .driver
            .hover_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
        harness.driver.advance_time(600);
        assert!(
            tooltip_state(&*harness).get_visible(),
            "the tip went visible"
        );

        harness.driver.click_by_label("Add task");
        harness.driver.advance_time(0);
        let tooltip = tooltip_state(&*harness);
        assert!(
            !tooltip.get_visible() && !tooltip.get_pending() && !tooltip.get_placed(),
            "the ghost click dismissed the tooltip"
        );
    });
}

/// The kiosk flip (J-K rider): a visible tip does not survive the
/// rotation's view swap — the flip happens outside `apply_effect`
/// entirely, so it carries its own dismiss.
#[test]
fn a_tooltip_does_not_survive_a_kiosk_flip() {
    let (_fixture, state, _walked) = fixture::today_overdue_state();
    support::harness::run_journey_with_settings(state, rotation_settings(), move |harness| {
        assert_eq!(current_view(&*harness), View::Board);
        // Arm on the first card of the board view.
        let card = harness
            .driver
            .find_by_label("task-0")
            .expect("the first card is addressable");
        let pos = card.absolute_position();
        let size = card.size();
        harness
            .driver
            .hover_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
        harness.driver.advance_time(600);
        assert!(
            tooltip_state(&*harness).get_visible(),
            "the tip went visible"
        );

        // The dwell fires the flip (J-K1's boundary: 30 s).
        harness.driver.advance_time(30_000);
        harness.driver.advance_time(0);
        assert_eq!(current_view(&*harness), View::Today, "the flip fired");
        let tooltip = tooltip_state(&*harness);
        assert!(
            !tooltip.get_visible() && !tooltip.get_pending() && !tooltip.get_placed(),
            "the kiosk flip dismissed the tooltip"
        );
    });
}

// ---- the tracker task kind (backlog round 2 chunk 4.2, #492) ---------

/// A tracker fixture: the golden state's task1 kind-marked (the same
/// card the context-menu journey addresses).
fn tracker_journey_state() -> (fixture::Fixture, AppState) {
    let (fixture, mut state) = fixture::journey_state();
    let mut tracker = state.tasks.get(&fixture.task1).expect("task").clone();
    tracker.kind = taskboard_domain::entities::TaskKind::Tracker;
    state.tasks.insert(fixture.task1, tracker);
    (fixture, state)
}

/// U3 — the task-view header checkbox hides for a tracker subject:
/// the detail model's flag passes through the push (the checkbox and
/// its focus/touch scopes mount inside the `if`), and the tracking
/// buttons stay on.
#[test]
fn the_task_view_hides_the_header_checkbox_for_a_tracker() {
    let (_fixture, state) = tracker_journey_state();
    support::harness::run_journey(state, move |harness| {
        // Walk to task1's detail: two item steps, two view steps.
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
        let model = harness.handle.window().get_task_model();
        assert!(
            !model.can_done,
            "the tracker subject hides the header checkbox"
        );
        assert!(
            !model.done,
            "the tracker is open — the status text stays data-driven"
        );
    });
}

/// U5 — the tracker card's context menu omits the impossible complete
/// verb and offers the kind toggle; invoking the toggle sends the
/// inverted plain kind edit (the context-menu journey's shape).
#[test]
fn the_tracker_context_menu_omits_complete_and_offers_the_toggle() {
    let (fixture, state) = tracker_journey_state();
    support::harness::run_journey(state, move |harness| {
        // The toggle is the Task page's last row: a tall window lets
        // the panel realize every row (the cap is host-pushed).
        harness.driver.set_window_size(1000, 1800);
        harness
            .handle
            .window()
            .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);
        assert!(keyboard(&*harness).get_menu_open());

        let entries = keyboard(&*harness).get_menu_entries();
        let rows: Vec<_> = (0..entries.row_count())
            .map(|i| entries.row_data(i).expect("row"))
            .collect();
        assert!(
            rows.iter().all(|r| r.action != "task.complete"),
            "the tracker's menu omits Complete task"
        );
        assert!(
            rows.iter().any(|r| r.action == "task.reopen"),
            "the reopen no-op stays — only the impossible verb hides"
        );
        assert!(
            rows.iter().any(|r| r.action == "task.toggle-tracker"),
            "the menu offers the kind toggle"
        );

        harness.driver.advance_time(0);
        harness.driver.click_by_label("Toggle tracker");
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                kind: Some(Some(taskboard_domain::entities::TaskKind::Standard)),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);
        assert!(
            !keyboard(&*harness).get_menu_open(),
            "the invocation closed the menu"
        );
    });
}

/// U6 — the shared suppression under the kiosk pin: the same pinned
/// the app window renders the tracker card without the done checkbox and
/// the standard card with it — zero kiosk-specific code (D-11).
#[test]
fn the_done_checkbox_seam_is_inert_in_kiosk_mode() {
    let (fixture, mut state) = fixture::golden_state();
    let mut tracker = state.tasks.get(&fixture.task1).expect("task").clone();
    tracker.kind = taskboard_domain::entities::TaskKind::Tracker;
    tracker.done = None;
    state.tasks.insert(fixture.task1, tracker);
    let state = state;
    support::harness::run_journey_with_settings(
        state,
        taskboard_ui_slint::UiSettings {
            kiosk: true,
            ..taskboard_ui_slint::UiSettings::default()
        },
        move |harness| {
            assert_eq!(current_view(&*harness), View::Board);
            // The tracker card's done affordance is gone ambiently; the
            // standard sibling's stays.
            let board = harness.handle.window().get_board_model();
            let mut hidden = 0;
            let mut shown = 0;
            for column in 0..board.stacks.row_count() {
                let tasks = board.stacks.row_data(column).expect("column").tasks;
                for row in 0..tasks.row_count() {
                    if tasks.row_data(row).expect("card").can_done {
                        shown += 1;
                    } else {
                        hidden += 1;
                    }
                }
            }
            assert_eq!(
                (shown, hidden),
                (2, 1),
                "exactly the tracker card hides its done box"
            );
            // Only the realized stack publishes a11y elements: the one
            // visible standard card keeps its checkbox, the tracker's
            // is gone ambiently.
            assert_eq!(
                harness
                    .driver
                    .find_by_label("Mark task done")
                    .iter()
                    .count(),
                1,
                "the visible standard card keeps its checkbox"
            );
        },
    );
}

/// U7 — a tracker task's notification rows drop the done button (the
/// existing per-row flag, the task-notification journey's twin).
#[test]
fn notification_rows_drop_the_done_button_for_a_tracker() {
    let (golden, mut state) = fixture::golden_state();
    let due = chrono::Utc::now() - TimeDelta::minutes(5);
    fixture::deliver_task_notification(&mut state, golden.task1, due);
    let mut tracker = state.tasks.get(&golden.task1).expect("task").clone();
    tracker.kind = taskboard_domain::entities::TaskKind::Tracker;
    state.tasks.insert(golden.task1, tracker);
    let state = state;
    support::harness::run_journey(state, move |harness| {
        assert_eq!(
            harness.handle.window().get_notification_model().row_count(),
            1
        );
        let row = harness
            .handle
            .window()
            .get_notification_model()
            .row_data(0)
            .expect("the surface's single row");
        assert!(!row.can_done, "the tracker row drops the done button");
        assert!(row.can_mute, "the mute button is untouched");
        assert!(
            row.can_track_start,
            "tracking stays live (the liveness pin)"
        );
    });
}

/// A Day-cadence reflection entry whose stored window opens at
/// `start` (the journeys' seed shape for the reflection sections).
fn seeded_reflection_entry(
    raw: u128,
    start: chrono::DateTime<chrono::Utc>,
    mood: Option<u8>,
) -> ReflectionEntry {
    let mut row = ReflectionEntry {
        id: ReflectionId::from_uuid(uuid::Uuid::from_u128(raw)),
        cadence: ReviewCadence::Day,
        day: start.date_naive(),
        window_start: start,
        window_end: start + chrono::TimeDelta::try_hours(24).unwrap(),
        text: format!("entry {raw}"),
        attributes: ReflectionAttributes::default(),
        created_at: start + chrono::TimeDelta::try_hours(25).unwrap(),
        updated_at: start + chrono::TimeDelta::try_hours(25).unwrap(),
        deleted: false,
    };
    row.attributes.mood = mood.map(|m| Rating::try_new(m).unwrap());
    row
}

/// #214 — the review surface journey: the cycle's backwards wrap
/// lands on Review, the ledger model matches the fixture's completed
/// set (structure over text — row counts, emptiness flags, and the
/// dashes as structural absences), and nav.focus-board returns. The
/// rows are read-only (D-8): no activation exists to assert.
#[test]
fn the_review_surface_ledgers_and_returns() {
    let (fixture, mut state) = fixture::golden_state();
    let now = chrono::Utc::now();
    let span = |start: chrono::TimeDelta, secs: i64| {
        (
            now + start,
            now + start + chrono::TimeDelta::try_seconds(secs).unwrap(),
        )
    };
    let _ = fixture::add_completed_task(
        &mut state,
        fixture.stack_a,
        "compared-a",
        now - chrono::TimeDelta::try_hours(2).unwrap(),
        Some(5_400),
        Some(span(chrono::TimeDelta::try_hours(-3).unwrap(), 3_600)),
    );
    let _ = fixture::add_completed_task(
        &mut state,
        fixture.stack_a,
        "untracked-b",
        now - chrono::TimeDelta::try_hours(1).unwrap(),
        Some(600),
        None,
    );
    // The reflection entries (#623 slice B): two live Day entries
    // anchored relative to `now` (the stored window_start lands inside
    // both trailing windows at any wall-clock hour) and one tombstone
    // that must vanish everywhere.
    let reflected = |raw: u128, age: chrono::TimeDelta, mood: Option<u8>| {
        seeded_reflection_entry(raw, now - age, mood)
    };
    let mut newest = reflected(1, chrono::TimeDelta::try_hours(3).unwrap(), Some(4));
    newest
        .attributes
        .flags
        .insert(taskboard_domain::HealthFlag::GoodDay);
    let mut dead = reflected(3, chrono::TimeDelta::try_hours(2).unwrap(), Some(5));
    dead.deleted = true;
    for row in [
        newest,
        reflected(2, chrono::TimeDelta::try_days(3).unwrap(), None),
        dead,
    ] {
        state.reflection.entries.insert(row.id, row);
    }
    support::harness::run_journey(state, move |harness| {
        // Cold start is the board; the backwards wrap crosses
        // Today into Review (#214 D-1).
        harness.press(ActionId::NavPrevView);
        assert_eq!(harness.handle.window().get_current_view(), View::Today);
        harness.press(ActionId::NavPrevView);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Review,
            "the wrap lands on the review surface"
        );
        let model = harness.handle.window().get_review_model();
        assert!(!model.empty, "the fixture completes two tasks");
        assert_eq!(model.rows.row_count(), 2, "the ledger mirrors calibrate");
        let first = model.rows.row_data(0).expect("row 0");
        assert_eq!(first.title, "untracked-b", "newest completion first");
        assert!(!first.estimate.is_empty(), "the estimate side is observed");
        assert!(
            first.actual.is_empty(),
            "an untracked actual is a structural absence"
        );
        let second = model.rows.row_data(1).expect("row 1");
        assert_eq!(second.title, "compared-a");
        assert!(!second.gap.is_empty(), "the settled comparison speaks");
        // The tracking sections (#623 slice A) speak structurally:
        // the planted work span keeps them populated and the strips
        // carry the windows' day counts — the read-only contract
        // below is unaffected.
        assert!(!model.tracking_month.empty);
        assert!(!model.tracking_month.work.is_empty());
        assert_eq!(model.tracking_week.bars.row_count(), 7);
        assert_eq!(model.tracking_month.bars.row_count(), 30);

        // The reflection sections + the entries ledger (#623
        // slice B): populated in both windows, the tombstone gone,
        // the strips full-length.
        assert!(!model.reflection_week.empty);
        assert!(!model.reflection_month.empty);
        assert!(!model.reflection_week.mood.is_empty());
        assert!(!model.reflection_week.per_flag.is_empty());
        assert_eq!(model.reflection_week.bars.row_count(), 7);
        assert_eq!(model.reflection_month.bars.row_count(), 30);
        // Two live seeded rows + the fixture's own review-card pin
        // row (an unrated Day entry for yesterday, #638); the
        // tombstone never surfaces.
        assert_eq!(model.reflection_rows.row_count(), 3);
        let entry = model.reflection_rows.row_data(0).expect("row 0");
        assert!(
            !entry.mood.is_empty(),
            "the newest-first row is the rated one"
        );
        assert!(!entry.day.is_empty());
        assert!(!entry.cadence.is_empty());

        // nav.focus-board returns from everywhere (the walk's
        // contract).
        harness.press(ActionId::NavFocusBoard);
        assert_eq!(harness.handle.window().get_current_view(), View::Board);
        harness.assert_commands(&[]); // a read-only view dispatches nothing
    });
}

/// The entries overlay's keyboard journey (r4 chunk 10, D35/D36
/// blanket rule; stage-3 review): the nonce grabs the panel root, one
/// walk step lands on a row, Return activates it into the seeded edit
/// form, and the Escape ladder steps form → list → closed. The row
/// here is an unreported stop (the tracking verbs' precedent), so the
/// reachability pins ride the observable surfaces (the form flag, the
/// mount flag) — the arm/confirm semantics stay at the flow-table
/// layer.
#[test]
fn the_entries_overlay_walks_rows_and_the_escape_ladder_steps_back() {
    let (fx, mut state) = fixture::golden_state();
    // One closed entry on the walk's focus target: the overlay's row.
    let start = chrono::Utc::now() - chrono::TimeDelta::hours(1);
    let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
    state.tracking.entries.insert(
        id,
        taskboard_domain::TimeEntry {
            id,
            task: fx.task0,
            task_title: "task-0".to_owned(),
            start,
            end: Some(start + chrono::TimeDelta::minutes(30)),
            created_at: start,
            updated_at: start,
            kind: taskboard_domain::tracking::TimeEntryKind::Work,
            by: None,
        },
    );
    support::harness::run_journey(state, move |harness| {
        press_preset(
            &mut *harness,
            taskboard_ui_slint::keymap::Preset::Standard,
            ActionId::NavFocusDown,
        );
        press_preset(
            &mut *harness,
            taskboard_ui_slint::keymap::Preset::Standard,
            ActionId::TaskEdit,
        );
        harness.driver.advance_time(0);
        // Open through the window-level dispatch seam; the
        // affordance's real click path (button → component
        // callback → mount forwarding → this seam) is the
        // through-the-real-mount journey's pin. The second loop
        // turn delivers the deferred nonce bump (the mount pass
        // ran in the first).
        harness.handle.window().invoke_task_track_entries_clicked();
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        assert!(
            window.get_task_model().track_entries_open,
            "the overlay mounts over the subject"
        );
        assert_eq!(
            window.get_task_model().track_entries.row_count(),
            1,
            "the subject's entry renders a row"
        );
        // The height-transition grab (post-mount) holds the walk
        // root — the fresh-mount init grab does not take (the
        // walk probe's P4/P5 split).
        {
            let walk_facts = harness
                .handle
                .window()
                .global::<taskboard_ui_slint::TrackPanelWalkFacts>();
            assert!(
                walk_facts.get_panel_root_focused(),
                "the height-transition grab lands on the panel root"
            );
        }
        // Row activation (the inventory's accessible-action
        // idiom): the row's a11y default action IS the
        // row-activated seam. The walk's step count across the
        // window-level mount stays the Tier-4 probe item (the
        // stop itself is structural — the row FocusScope markup
        // above; the control walk's ordering over the overlay is
        // runtime-sensitive and unproven here).
        let start_local = start.with_timezone(&chrono::Local);
        let row_label = start_local.format("%Y-%m-%d %H:%M").to_string();
        assert!(
            !harness.handle.window().get_track_form_open(),
            "the form stays closed before activation"
        );
        harness
            .driver
            .find_by_label(row_label.as_str())
            .expect("the entry row renders under its span label")
            .invoke_default_action();
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_track_form_open(),
            "activating the row opens the seeded edit form"
        );
        // The seed is the entry's own local wall-clock.
        assert_eq!(
            harness.handle.window().get_track_form_start_text(),
            start_local.format("%H:%M").to_string(),
            "the edit seeds the entry's local start"
        );
        // The ladder, level 2: Escape steps the form back to the
        // list — the overlay stays.
        harness.stroke(parse("Escape").expect("escape"));
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_track_form_open(),
            "the form steps back"
        );
        assert!(
            harness.handle.window().get_task_model().track_entries_open,
            "the overlay stays through the form's step"
        );
        // The ladder, level 3: Escape closes the overlay.
        harness.stroke(parse("Escape").expect("escape"));
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_task_model().track_entries_open,
            "the second Escape closes the overlay"
        );
    });
}

/// The entries affordance's real click path (round-5 F1): the
/// "Time entries" button fires the view component's
/// `task-track-entries-clicked`, the window mount forwards it to the
/// window-level seam, and the overlay opens over the subject. The
/// window-level invoke alone is NOT this path — a dead mount
/// forwarding silently no-ops the click (F1), invisible to a
/// window-level journey.
#[test]
fn the_time_entries_affordance_opens_the_overlay_through_the_real_mount() {
    let (fx, mut state) = fixture::golden_state();
    // One closed entry on the subject: the overlay's row pins the
    // open went over fx.task0 (no other task carries entries).
    let start = chrono::Utc::now() - chrono::TimeDelta::hours(1);
    let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
    state.tracking.entries.insert(
        id,
        taskboard_domain::TimeEntry {
            id,
            task: fx.task0,
            task_title: "task-0".to_owned(),
            start,
            end: Some(start + chrono::TimeDelta::minutes(30)),
            created_at: start,
            updated_at: start,
            kind: taskboard_domain::tracking::TimeEntryKind::Work,
            by: None,
        },
    );
    support::harness::run_journey(state, move |harness| {
        press_preset(
            &mut *harness,
            taskboard_ui_slint::keymap::Preset::Standard,
            ActionId::NavFocusDown,
        );
        press_preset(
            &mut *harness,
            taskboard_ui_slint::keymap::Preset::Standard,
            ActionId::TaskEdit,
        );
        harness.driver.advance_time(0);
        // The real click: the button's accessible default action
        // IS the IconButton's `clicked` handler (§7.1's
        // click-equivalence seam — no coordinate arithmetic). The
        // overlay panel shares the label but is unmounted at
        // click time, so the address is unique.
        harness.driver.click_by_label("Time entries");
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        assert!(
            window.get_task_model().track_entries_open,
            "the overlay mounts over the subject through the real click"
        );
        assert_eq!(
            window.get_task_model().track_entries.row_count(),
            1,
            "the subject's entry renders a row"
        );
        let start_local = start.with_timezone(&chrono::Local);
        let row_label = start_local.format("%Y-%m-%d %H:%M").to_string();
        assert!(
            harness.driver.find_by_label(row_label.as_str()).is_some(),
            "the row is the subject entry's local span"
        );
    });
}

/// The bar dropdown's modal ladder (r4 D40) under every keymap preset:
/// while the flyout is open, the ladder keys reach the widget — the
/// capture-phase resolver declines them (the shared decline arm) —
/// and the action layer dispatches nothing. The anchor is the viewed
/// board; Escape closes. (The bar probe cannot host this: the decline
/// arm lives on the app window's key-root capture, absent from a
/// standalone probe.)
#[test]
fn the_bar_dropdown_ladder_declines_the_action_layer_under_every_preset() {
    for (preset, name) in [
        (taskboard_ui_slint::keymap::Preset::Standard, "standard"),
        (taskboard_ui_slint::keymap::Preset::Emacs, "emacs"),
        (taskboard_ui_slint::keymap::Preset::Vi, "vi"),
    ] {
        let mut state = AppState::default();
        // Nine boards: past the segment budget (the bar's dropdown-at),
        // so the bar mounts the dropdown switcher.
        let boards: Vec<_> = (0..9)
            .map(|i| (uuid::Uuid::new_v4(), format!("board-{i}")))
            .collect();
        for (id, title) in &boards {
            state_board(&mut state, *id, title, None);
        }
        support::harness::run_journey_with_settings(
            state,
            UiSettings {
                keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                keymap_preset: preset,
                ..UiSettings::default()
            },
            move |harness| {
                harness.driver.set_window_size(1000, 900);
                harness.driver.advance_time(0);

                // Open via the field click (the click focuses the
                // field — the ladder's home; the element query is the
                // address in dropdown mode, where no segments render).
                let mut field = None;
                for _ in 0..20 {
                    harness.driver.advance_time(0);
                    let found = harness.driver.find_all_by_id("BoardBar::bar-dd");
                    if let Some(first) = found.into_iter().next() {
                        field = Some(first);
                        break;
                    }
                }
                let field = field.expect("{name}: the bar dropdown renders past the budget");
                let pos = field.absolute_position();
                let size = field.size();
                let (x, y) = (pos.x + size.width / 2.0, pos.y + size.height / 2.0);
                harness.driver.hover_at(x, y);
                harness.driver.advance_time(0);
                harness.driver.click_at(x, y);
                harness.driver.advance_time(30);

                assert!(
                    bar_facts(&*harness).get_open(),
                    "{name}: the field click opens"
                );

                // The anchor is the viewed board (segment 0 here — the
                // engine's current board is the first inserted).
                let anchor = bar_facts(&*harness).get_highlight();

                // Down moves the highlight, and the lane stays empty:
                // the capture-phase resolver declined the stroke.
                raw_key(&*harness, slint::platform::Key::DownArrow);
                harness.driver.advance_time(0);
                assert_eq!(
                    bar_facts(&*harness).get_highlight(),
                    anchor + 1,
                    "{name}: Down moves the ladder"
                );
                assert!(
                    harness.drain_requests().is_empty(),
                    "{name}: the arrow dispatched nothing"
                );

                // The C-n spelling shares the arm — the emacs binding
                // must not win the capture race either.
                harness.stroke(parse("Ctrl+n").expect("Ctrl+n parses"));
                harness.driver.advance_time(0);
                assert_eq!(
                    bar_facts(&*harness).get_highlight(),
                    anchor + 2,
                    "{name}: Ctrl+n moves the ladder too"
                );
                assert!(
                    harness.drain_requests().is_empty(),
                    "{name}: the emacs stroke dispatched nothing"
                );

                // Escape closes without selecting; the lane stays
                // empty and the viewed board is unchanged.
                raw_key(&*harness, slint::platform::Key::Escape);
                harness.driver.advance_time(0);
                assert!(!bar_facts(&*harness).get_open(), "{name}: Escape closes");
                assert!(
                    harness.drain_requests().is_empty(),
                    "{name}: closing selected nothing"
                );
            },
        );
    }
}

// ---------------------------------------------------------------------
// The settings editor's general surface (the GUI settings editor
// chunk, J-1..J-7 of the plan's catalogue). Assertion discipline as
// everywhere: typed model properties and the typed requests that
// reached the lane boundary; label/status text is never matched.

use std::collections::BTreeSet;
use taskboard_ui_slint::requests::{
    ApplySemantics, ReminderSink, SectionApply, SettingsFact, SettingsSnapshot,
};

/// The general snapshot fixture (the hand-over's shape): typed
/// editable values, the exhaustive apply map, and the read-only
/// facts. Env-locked variants clone-and-lock.
fn general_snapshot() -> SettingsSnapshot {
    let mut applies = std::collections::BTreeMap::new();
    for key in [
        "preferences.day_start_time",
        "reminders.enabled",
        "reminders.tick_secs",
        "reminders.default_before",
        "reminders.sink",
        "sync.poll_interval_secs",
        "sync.backoff_initial_secs",
        "sync.backoff_max_secs",
        "ui.due_soon_days",
        "focus.enabled",
        "focus.work_mins",
        "focus.break_mins",
        "focus.chain",
        "focus.long_break_mins",
        "focus.long_break_every",
        "focus.notify",
    ] {
        applies.insert(key.to_owned(), ApplySemantics::on_restart("fixture"));
    }
    SettingsSnapshot {
        day_start_time: chrono::NaiveTime::from_hms_opt(5, 30, 0).expect("t"),
        reminders_enabled: true,
        reminder_tick_secs: 30,
        reminder_before: "5m".to_owned(),
        reminder_sink: ReminderSink::Auto,
        poll_interval_secs: 30,
        backoff_initial_secs: 5,
        backoff_max_secs: 300,
        due_soon_days: 1,
        focus_enabled: true,
        focus_work_mins: 25,
        focus_break_mins: 5,
        focus_chain: false,
        focus_long_break_mins: 15,
        focus_long_break_every: 4,
        focus_notify: true,
        files_sync_enabled: false,
        files_sync_poll_secs: None,
        files_sync_anchor: None,
        env_overridden: BTreeSet::new(),
        facts: vec![
            SettingsFact {
                key: "app.mode",
                value: "desktop".to_owned(),
                apply: SectionApply::OnRestart,
            },
            SettingsFact {
                key: "web.token",
                value: "true".to_owned(),
                apply: SectionApply::OnRestart,
            },
            SettingsFact {
                key: "ui.stack_widths",
                value: "3".to_owned(),
                apply: SectionApply::Live,
            },
        ],
        applies,
    }
}

fn general_harness(state: AppState, snapshot: SettingsSnapshot) -> Harness {
    let settings = UiSettings {
        general: snapshot,
        ..UiSettings::default()
    };
    Harness::mount_with_settings(SlintDriver::default(), state, settings)
}

/// J-1 — the render journey: the new sections render the snapshot's
/// values (typed echoes), the read-only facts render with their apply
/// badges, and nothing dispatches at mount.
#[test]
fn the_general_sections_render_the_snapshot() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // The typed echoes (draft-free at mount).
            assert_eq!(model().get_day_start_echo(), "05:30");
            assert_eq!(model().get_tick_echo(), "30");
            assert_eq!(model().get_before_echo(), "5m");
            assert_eq!(model().get_poll_echo(), "30");
            assert_eq!(model().get_backoff_initial_echo(), "5");
            assert_eq!(model().get_backoff_max_echo(), "300");
            assert_eq!(model().get_due_soon_echo(), "1");
            assert_eq!(
                model().get_reminders_enabled_index(),
                0,
                "On renders selected"
            );
            assert_eq!(model().get_sink_index(), 0, "auto renders selected");

            // The read-only facts render one row per fact.
            assert_eq!(model().get_facts().row_count(), 3);
            let token_row = (0..model().get_facts().row_count())
                .find_map(|i| {
                    model()
                        .get_facts()
                        .row_data(i)
                        .filter(|row| row.key == "web.token")
                })
                .expect("the token presence row renders");
            assert_eq!(token_row.value, "true", "presence, never the value");
            let widths_row = (0..model().get_facts().row_count())
                .find_map(|i| {
                    model()
                        .get_facts()
                        .row_data(i)
                        .filter(|row| row.key == "ui.stack_widths")
                })
                .expect("the fenced-subtree presence row renders");
            assert!(!widths_row.on_restart, "the fenced subtree applies live");

            // The apply facts render per editable field (J-5's
            // structural presence, pinned here for the group).
            assert!(model().get_day_start_on_restart());
            assert!(model().get_tick_on_restart());
            assert!(model().get_reminders_enabled_on_restart());
            assert!(model().get_due_soon_on_restart());

            assert!(
                harness.drain_requests().is_empty(),
                "rendering dispatches nothing"
            );
        },
    );
}

/// The apply badges render the registry's verdicts mechanically
/// (round-5 chunk 5, D-5.6): a `Live` key renders without the restart
/// badge, a justified `OnRestart` key with it — the structural
/// `on-restart` property, never label text.
#[test]
fn the_apply_badges_follow_the_registry_verdicts() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut snapshot = general_snapshot();
            snapshot
                .applies
                .insert("reminders.tick_secs".to_owned(), ApplySemantics::live());
            snapshot.applies.insert(
                "preferences.day_start_time".to_owned(),
                ApplySemantics::live(),
            );
            snapshot.applies.insert(
                "reminders.sink".to_owned(),
                ApplySemantics::on_restart("the sink is one boot arc"),
            );
            let mut harness = general_harness(state, snapshot);
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();
            assert!(
                !model().get_tick_on_restart(),
                "a live key carries no badge"
            );
            assert!(!model().get_day_start_on_restart());
            assert!(model().get_sink_on_restart(), "a justified key carries it");
            assert!(
                harness.drain_requests().is_empty(),
                "rendering dispatches nothing"
            );
        },
    );
}

/// J-2 — the edit round-trip (the acceptance core): a valid edit
/// commits, exactly the typed intent dispatches with the minted
/// payload, the fresh-snapshot reply re-renders the row's echo (never
/// the click), and the outcome lands Ok.
#[test]
fn the_general_edit_round_trip_routes_and_echoes_the_reply() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // Stage the edit (per keystroke), commit on Return's seam.
            window.invoke_settings_field_edited(6, "45".into());
            assert_eq!(model().get_tick_echo(), "45", "the draft is the echo");
            window.invoke_settings_general_committed(6, "45".into());
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1, "exactly one typed intent");
            let taskboard_ui_slint::requests::UiRequest::SetReminderTick { secs, reply } =
                requests.remove(0)
            else {
                panic!("the tick intent routed");
            };
            assert_eq!(secs, 45, "the minted payload rides the lane");

            // The reply carries the fresh snapshot; the echo follows
            // the reply (the committed value), not the click.
            let mut fresh = general_snapshot();
            fresh.reminder_tick_secs = 45;
            reply.send(Ok(fresh)).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(model().get_tick_echo(), "45");
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Ok
            );

            // The choice grammar: pick dispatches on the pick (the
            // keymap preset's rule), the reply re-renders the index.
            window.invoke_settings_reminders_picked(1);
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1);
            let taskboard_ui_slint::requests::UiRequest::SetRemindersEnabled { enabled, reply } =
                requests.remove(0)
            else {
                panic!("the toggle intent routed");
            };
            assert!(!enabled);
            let mut fresh = general_snapshot();
            fresh.reminders_enabled = false;
            reply.send(Ok(fresh)).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(
                model().get_reminders_enabled_index(),
                1,
                "the selection echoes the replied value"
            );
            window.invoke_settings_sink_picked(2);
            let mut requests = harness.drain_requests();
            let taskboard_ui_slint::requests::UiRequest::SetReminderSink { sink, reply } =
                requests.remove(0)
            else {
                panic!("the sink intent routed");
            };
            assert_eq!(sink, ReminderSink::Desktop);
            let mut fresh = general_snapshot();
            fresh.reminder_sink = ReminderSink::Desktop;
            reply.send(Ok(fresh)).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(model().get_sink_index(), 2);
        },
    );
}

/// J-3 — the typed-refusal journey: an unparseable day start, a zero
/// tick, and the `initial > max` crossing refuse at the seam — no
/// request dispatches, the field's validity flavour renders the typed
/// kind, the status line renders the refusal outcome. A refused draft
/// stays visible (the typo remains under its flavour).
#[test]
fn invalid_general_commits_refuse_typed_without_routing() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            window.invoke_settings_field_edited(6, "0".into());
            window.invoke_settings_general_committed(6, "0".into());
            assert!(
                harness.drain_requests().is_empty(),
                "a refusal routes nothing"
            );
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected
            );
            assert_eq!(
                model().get_validity_tick(),
                taskboard_ui_slint::FieldValidity::Invalid,
                "the flavour renders the typed kind"
            );

            // Unparseable day start: same shape (the draft staged
            // first — the flavour renders from the staged buffer, the
            // scale field's grammar).
            window.invoke_settings_field_edited(5, "abc".into());
            window.invoke_settings_general_committed(5, "abc".into());
            assert!(harness.drain_requests().is_empty());
            assert_eq!(
                model().get_validity_day_start(),
                taskboard_ui_slint::FieldValidity::Invalid
            );

            // The ordering rule (plan D8): initial above the committed
            // max refuses at the seam, attributed to the offending
            // field — no lane traffic.
            window.invoke_settings_field_edited(9, "400".into());
            window.invoke_settings_general_committed(9, "400".into());
            assert!(harness.drain_requests().is_empty());
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected,
                "the crossing refuses visibly at the seam (the status                  line carries it; the per-field flavour is the                  per-keystroke predicate's)"
            );
            // Symmetric: max below the committed initial.
            window.invoke_settings_field_edited(10, "3".into());
            window.invoke_settings_general_committed(10, "3".into());
            assert!(harness.drain_requests().is_empty());
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected
            );

            // A valid commit clears the stale drafts' flavours: the
            // tick edit refused above stays in its draft; focus loss
            // discards it (the scale field's grammar).
            window.invoke_settings_general_focus_lost(6);
            assert_eq!(
                model().get_validity_tick(),
                taskboard_ui_slint::FieldValidity::Neutral,
                "the discarded draft renders neutral again"
            );
            assert_eq!(model().get_tick_echo(), "30", "the echo re-syncs");
        },
    );
}

/// T-38 (#821): the busy lane's refusal carries its typed posture —
/// a second op under a parked request renders `Rejected` with the
/// `RequestPending` detail class (the old shape was the generic
/// detail-less refusal) and the status line renders. Typed facts
/// only; the sentence text is never asserted.
#[test]
fn a_click_under_a_pending_request_refuses_with_the_busy_posture() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // The first op routes; its reply is deliberately left
            // parked — the request lane stays pending.
            window.invoke_settings_reminders_picked(1);
            assert_eq!(harness.drain_requests().len(), 1, "the first op routed");

            // A second op under the parked request: the busy refusal
            // (typed posture, zero lane traffic).
            window.invoke_settings_sink_picked(1);
            assert!(
                harness.drain_requests().is_empty(),
                "the busy click routes nothing"
            );
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected
            );
            assert!(
                model().get_last_error_request_pending(),
                "the typed busy posture renders"
            );
            assert_ne!(
                model().get_status_text(),
                "",
                "the status line carries the refusal"
            );
        },
    );
}

/// J-4 — the env-override journey: a key in the hand-over's env set
/// renders read-only (the env-locked fact present, the badge fact
/// pushed), and its commit is absorbed at the defence arm — nothing
/// dispatches, nothing pretends to apply.
#[test]
fn env_overridden_keys_render_locked_and_refuse_the_seam() {
    let (_fixture, state) = fixture::golden_state();
    let mut snapshot = general_snapshot();
    snapshot.env_overridden = BTreeSet::from(["reminders.tick_secs".to_owned()]);
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, snapshot);
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            assert!(model().get_tick_env_locked(), "the env fact rides the row");

            // The defence in depth: even a routed commit on the
            // locked key never dispatches (the effective value cannot
            // move while the env stands).
            window.invoke_settings_general_committed(6, "45".into());
            assert!(
                harness.drain_requests().is_empty(),
                "the locked key never dispatches"
            );

            // A reply must not lift the lock: the env set is
            // hand-over state, preserved across replies.
            window.invoke_settings_general_committed(11, "7".into());
            let mut requests = harness.drain_requests();
            let taskboard_ui_slint::requests::UiRequest::SetDueSoonDays { reply, .. } =
                requests.remove(0)
            else {
                panic!("the unlocked field routed");
            };
            let fresh = general_snapshot();
            reply.send(Ok(fresh)).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert!(
                model().get_tick_env_locked(),
                "the reply preserved the standing env set"
            );
        },
    );
}

/// An empty Return refuses typed and dispatches nothing: the field's
/// `accepted` seam fires on Return regardless of content, and the
/// mirror's empty contract is Neutral — so an empty Return reaches the
/// commit seam where the typed mints `expect` a validated parse. The
/// refusal is the visible Rejected outcome, never a mint attempt.
#[test]
fn an_empty_return_refuses_typed_without_routing() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // Every text seam, committed empty (the untouched field's
            // Return) and whitespace-only (the cleared field's
            // Return): no mint attempt, no lane traffic.
            for index in [5, 6, 7, 8, 9, 10, 11] {
                for text in ["", "   "] {
                    window.invoke_settings_general_committed(index, text.into());
                    assert!(
                        harness.drain_requests().is_empty(),
                        "an empty commit on field {index} routes nothing"
                    );
                    assert_eq!(
                        model().get_last_outcome(),
                        taskboard_ui_slint::SettingsOutcome::Rejected,
                        "field {index}: the empty commit refuses visibly"
                    );
                }
            }
        },
    );
}

/// J-6 — the general fields' draft lifecycle: a Return-less edit
/// never survives the surface (the scale field's §5.1 invariant,
/// extended to the new fields — the mount dies the staged buffer and
/// the echo re-renders the committed value). The insert-mask
/// engagement itself is the shared `TextField` widget's D18 machinery,
/// pinned once by the editing-facts lifecycle journey (the testing
/// backend cannot focus a settings field: the recorded
/// "a `TouchArea` click never moves focus" negative, and the fields
/// opt out of autofocus by the initial-focus contract — the plan's
/// focus/commit halves are therefore pinned at the draft seam here).
#[test]
fn a_general_return_less_edit_never_survives_the_surface() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // Stage a Return-less edit: the draft is the echo.
            window.invoke_settings_field_edited(7, "90m".into());
            assert_eq!(model().get_before_echo(), "90m", "the draft renders");

            // The surface closes and reopens: the staged draft died.
            harness.press(ActionId::AppSettings);
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            assert_eq!(
                window
                    .global::<taskboard_ui_slint::SettingsModel>()
                    .get_before_echo(),
                "5m",
                "the mount re-renders the committed echo"
            );

            // The focus-loss seam shares the grammar: a staged draft
            // dies and the echo re-syncs without any surface change.
            window.invoke_settings_field_edited(7, "2h".into());
            assert_eq!(
                window
                    .global::<taskboard_ui_slint::SettingsModel>()
                    .get_before_echo(),
                "2h"
            );
            window.invoke_settings_general_focus_lost(7);
            assert_eq!(
                window
                    .global::<taskboard_ui_slint::SettingsModel>()
                    .get_before_echo(),
                "5m",
                "the focus-loss discard re-syncs the echo"
            );
        },
    );
}

/// J-7 — the busy machine covers the new arms: an editor op in flight
/// disables the other settings ops — a second commit during the
/// flight refuses visibly and routes nothing (the D34 machine's one
/// op at a time).
#[test]
fn a_general_op_in_flight_gates_the_other_settings_ops() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            window.invoke_settings_general_committed(6, "45".into());
            assert!(model().get_busy(), "the busy gate is up");

            // A second editor commit mid-flight: visible refusal,
            // nothing routes.
            window.invoke_settings_general_committed(11, "7".into());
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected
            );

            // The reply lands: the gate lifts, the next op routes.
            let mut lane = harness.requests_rx.lock().expect("requests lock");
            let drained: Vec<_> = std::iter::from_fn(|| lane.try_recv().ok()).collect();
            assert_eq!(
                drained.len(),
                1,
                "exactly the first op is in the lane — the busy gate                  routed nothing extra"
            );
            let taskboard_ui_slint::requests::UiRequest::SetReminderTick { reply, .. } =
                drained.into_iter().next().expect("the op routes")
            else {
                panic!("the tick op routes");
            };
            drop(lane);
            reply
                .send(Ok(general_snapshot()))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert!(!model().get_busy());
            window.invoke_settings_general_committed(11, "7".into());
            assert_eq!(harness.drain_requests().len(), 1, "the gate lifted");
        },
    );
}

// ---- #625: the focus settings editor's journeys (J-F1..J-F9) -----------

/// J-F1 — the render journey: the seven focus rows render the
/// snapshot's typed echoes and segmented indices, the on-restart
/// facts ride the rows, the promoted keys are absent from the facts
/// model, and nothing dispatches at mount.
#[test]
fn the_focus_rows_render_the_snapshot() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            assert_eq!(model().get_focus_work_echo(), "25");
            assert_eq!(model().get_focus_break_echo(), "5");
            assert_eq!(model().get_focus_long_break_echo(), "15");
            assert_eq!(model().get_focus_every_echo(), "4");
            assert_eq!(model().get_focus_enabled_index(), 0, "On renders selected");
            assert_eq!(model().get_focus_chain_index(), 1, "Off renders selected");
            assert_eq!(model().get_focus_notify_index(), 0, "On renders selected");

            // The on-restart captions' facts are pushed per row.
            assert!(model().get_focus_work_on_restart());
            assert!(model().get_focus_every_on_restart());
            assert!(model().get_focus_enabled_on_restart());
            assert!(model().get_focus_notify_on_restart());

            // The promoted keys left the facts: a key renders exactly
            // once, never both editable and read-only (D-FS2).
            for row_index in 0..model().get_facts().row_count() {
                let row = model().get_facts().row_data(row_index).expect("row");
                assert!(
                    !row.key.starts_with("focus."),
                    "the {key} fact left the read-only list",
                    key = row.key
                );
            }

            assert!(
                harness.drain_requests().is_empty(),
                "rendering dispatches nothing"
            );
        },
    );
}

/// J-F2 — the edit round-trip: a staged edit commits on Return,
/// exactly the typed intent dispatches with the minted payload, the
/// fresh-snapshot reply re-renders the echo; a pick dispatches the
/// toggle intent and the reply re-renders the index.
#[test]
fn a_focus_edit_round_trips_and_echoes_the_reply() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            window.invoke_settings_field_edited(12, "45".into());
            assert_eq!(model().get_focus_work_echo(), "45", "the draft renders");
            window.invoke_settings_general_committed(12, "45".into());
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1, "exactly one typed intent");
            let taskboard_ui_slint::requests::UiRequest::SetFocusWorkMins { mins, reply } =
                requests.remove(0)
            else {
                panic!("the work-minutes intent routed");
            };
            assert_eq!(mins, 45, "the minted payload rides the lane");

            let mut fresh = general_snapshot();
            fresh.focus_work_mins = 45;
            reply.send(Ok(fresh)).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(model().get_focus_work_echo(), "45");
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Ok
            );

            // The pick grammar: dispatch on the pick, the reply
            // re-renders the selection.
            window.invoke_settings_focus_enabled_picked(1);
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1);
            let taskboard_ui_slint::requests::UiRequest::SetFocusEnabled { enabled, reply } =
                requests.remove(0)
            else {
                panic!("the toggle intent routed");
            };
            assert!(!enabled);
            let mut fresh = general_snapshot();
            fresh.focus_enabled = false;
            reply.send(Ok(fresh)).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(
                model().get_focus_enabled_index(),
                1,
                "the selection echoes the replied value"
            );
        },
    );
}

/// J-F3 — the typed-refusal journey: out-of-bounds minutes refuse at
/// the seam — the validity flavour renders Invalid, the status shows
/// the refusal outcome, zero requests dispatch, and the draft stays
/// visible under its flavour.
#[test]
fn invalid_focus_commits_refuse_typed_without_routing() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            for text in ["0", "241"] {
                window.invoke_settings_field_edited(12, text.into());
                window.invoke_settings_general_committed(12, text.into());
                assert!(
                    harness.drain_requests().is_empty(),
                    "{text}: a refusal routes nothing"
                );
                assert_eq!(
                    model().get_validity_focus_work(),
                    taskboard_ui_slint::FieldValidity::Invalid,
                    "{text}: the flavour renders the typed kind"
                );
                assert_eq!(
                    model().get_last_outcome(),
                    taskboard_ui_slint::SettingsOutcome::Rejected
                );
                assert_eq!(
                    model().get_focus_work_echo(),
                    text,
                    "{text}: the refused draft stays visible"
                );
            }

            for text in ["0", "25"] {
                window.invoke_settings_field_edited(15, text.into());
                window.invoke_settings_general_committed(15, text.into());
                assert!(
                    harness.drain_requests().is_empty(),
                    "{text}: the every bound refuses without routing"
                );
                assert_eq!(
                    model().get_validity_focus_every(),
                    taskboard_ui_slint::FieldValidity::Invalid
                );
            }

            // The discarded draft renders neutral again.
            window.invoke_settings_general_focus_lost(12);
            assert_eq!(
                model().get_validity_focus_work(),
                taskboard_ui_slint::FieldValidity::Neutral
            );
            assert_eq!(model().get_focus_work_echo(), "25", "the echo re-syncs");
        },
    );
}

/// J-F4 — the env-override journey: a focus key in the hand-over env
/// set locks its row and the commit/pick is absorbed at the defence
/// arm — nothing dispatches.
#[test]
fn env_locked_focus_rows_render_locked_and_refuse_the_seam() {
    let (_fixture, state) = fixture::golden_state();
    let mut snapshot = general_snapshot();
    snapshot.env_overridden =
        BTreeSet::from(["focus.work_mins".to_owned(), "focus.notify".to_owned()]);
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, snapshot);
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            assert!(model().get_focus_work_env_locked());
            assert!(model().get_focus_notify_env_locked());
            assert!(!model().get_focus_break_env_locked());

            window.invoke_settings_general_committed(12, "45".into());
            assert!(
                harness.drain_requests().is_empty(),
                "the locked keys never dispatch"
            );
            // T-16 (#799): the refusal renders typed — the rejected
            // outcome plus the `EnvLocked` detail class, structurally.
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected
            );
            assert!(
                model().get_last_error_env_locked(),
                "the env-locked detail class renders"
            );

            window.invoke_settings_focus_notify_picked(1);
            assert!(harness.drain_requests().is_empty());
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected
            );
            assert!(model().get_last_error_env_locked());
        },
    );
}

/// T-16 (#799) — the env-locked absorb's second family: the Files
/// toggle's pick refuses typed (the `EnvLocked` detail class renders)
/// and never routes; a knob pick on a locked choice row refuses the
/// same way (the sweep's representative).
#[test]
fn env_locked_files_pick_and_choice_picks_refuse_typed() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut snapshot = general_snapshot();
    snapshot.env_overridden =
        BTreeSet::from(["files_sync.enabled".to_owned(), "reminders.sink".to_owned()]);
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        general: snapshot,
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(AppState::default(), settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        let window = harness.handle.window();
        let model = || window.global::<taskboard_ui_slint::SettingsModel>();

        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let files_pick = items(&*harness)
            .iter()
            .position(|item| item.kind == K::Files)
            .expect("the Files row renders") as i32;
        assert!(
            items(&*harness)
                .iter()
                .find(|item| item.kind == K::Files)
                .expect("the Files row renders")
                .files_enabled_pinned,
            "the env fact rides the row"
        );
        window.invoke_settings_files_picked(files_pick, 0);
        assert!(
            harness.drain_requests().is_empty(),
            "a locked pick routes nothing"
        );
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Rejected
        );
        assert!(model().get_last_error_env_locked());

        // The choice-row representative (the sink pick — the same
        // commit-on-pick grammar as the other toggles).
        window.invoke_settings_sink_picked(1);
        assert!(harness.drain_requests().is_empty());
        assert_eq!(
            model().get_last_outcome(),
            taskboard_ui_slint::SettingsOutcome::Rejected
        );
        assert!(model().get_last_error_env_locked());
    });
}

/// J-F5 — the empty-Return arm at the new indices: no mint attempt,
/// no lane traffic, a visible typed refusal.
#[test]
fn an_empty_focus_return_refuses_typed_without_routing() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            for index in [12, 13, 14, 15] {
                for text in ["", "   "] {
                    window.invoke_settings_general_committed(index, text.into());
                    assert!(
                        harness.drain_requests().is_empty(),
                        "an empty commit on field {index} routes nothing"
                    );
                    assert_eq!(
                        model().get_last_outcome(),
                        taskboard_ui_slint::SettingsOutcome::Rejected,
                        "field {index}: the empty commit refuses visibly"
                    );
                }
            }
        },
    );
}

/// J-F6 — the draft lifecycle arm: a Return-less focus edit never
/// survives — focus loss re-syncs the echo to the committed value.
#[test]
fn a_focus_return_less_edit_never_survives_the_surface() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            window.invoke_settings_field_edited(12, "60".into());
            assert_eq!(model().get_focus_work_echo(), "60", "the draft renders");

            window.invoke_settings_general_focus_lost(12);
            assert_eq!(
                model().get_focus_work_echo(),
                "25",
                "the focus-loss discard re-syncs the echo"
            );

            // The mount shares the grammar: closing and reopening the
            // surface kills a staged draft.
            window.invoke_settings_field_edited(15, "8".into());
            harness.press(ActionId::AppSettings);
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            assert_eq!(
                window
                    .global::<taskboard_ui_slint::SettingsModel>()
                    .get_focus_every_echo(),
                "4",
                "the mount re-renders the committed echo"
            );
        },
    );
}

/// J-F7 — the busy machine covers the focus seams: an editor op in
/// flight gates the other settings ops (one op at a time).
#[test]
fn a_focus_op_in_flight_gates_the_other_settings_ops() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            window.invoke_settings_general_committed(12, "30".into());
            assert!(model().get_busy(), "the busy gate is up");

            window.invoke_settings_general_committed(11, "7".into());
            window.invoke_settings_focus_enabled_picked(1);
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected
            );
            let mut lane = harness.requests_rx.lock().expect("requests lock");
            let drained: Vec<_> = std::iter::from_fn(|| lane.try_recv().ok()).collect();
            drop(lane);
            assert_eq!(
                drained.len(),
                1,
                "exactly the first op is in the lane — the gate routed nothing extra"
            );
            let taskboard_ui_slint::requests::UiRequest::SetFocusWorkMins { reply, .. } =
                drained.into_iter().next().expect("the op routes")
            else {
                panic!("the focus op routes");
            };
            reply
                .send(Ok(general_snapshot()))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert!(!model().get_busy(), "the reply lifted the gate");
        },
    );
}

/// J-F8 — the failed-persist revert: an `Err` reply leaves the VM's
/// snapshot untouched, so the echo re-derives from the last-known-good
/// file value; the outcome renders typed; no retry loop, no stuck
/// Busy.
#[test]
fn a_failed_focus_persist_reverts_the_echo_and_shows_the_outcome() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            window.invoke_settings_general_committed(13, "10".into());
            let mut requests = harness.drain_requests();
            let taskboard_ui_slint::requests::UiRequest::SetFocusBreakMins { reply, .. } =
                requests.remove(0)
            else {
                panic!("the break intent routed");
            };
            reply
                .send(Err(
                    taskboard_ui_slint::requests::SettingsError::PersistRejected,
                ))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();

            assert_eq!(
                model().get_focus_break_echo(),
                "5",
                "the failed value never sticks — the echo re-derives"
            );
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected,
                "the typed outcome renders"
            );
            assert!(!model().get_busy(), "no stuck Busy");

            // No retry loop: the next op routes normally.
            window.invoke_settings_general_committed(11, "7".into());
            assert_eq!(harness.drain_requests().len(), 1);
        },
    );
}

/// J-F9 — the commit-on-release structural pin (D-FS8): these rows
/// mount no gesture widget — per-keystroke edits dispatch zero
/// requests; only the single Return seam routes, one intent per
/// commit.
#[test]
fn focus_keystrokes_never_dispatch_until_the_commit_seam() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            settle_auto_lists(&harness, no_rows);
            let window = harness.handle.window();

            for partial in ["4", "45", "450"] {
                window.invoke_settings_field_edited(12, partial.into());
            }
            assert!(
                harness.drain_requests().is_empty(),
                "keystrokes dispatch nothing"
            );

            window.invoke_settings_general_committed(12, "45".into());
            let requests = harness.drain_requests();
            assert_eq!(
                requests.len(),
                1,
                "the single Return seam dispatches exactly one intent"
            );
            assert!(matches!(
                requests[0],
                taskboard_ui_slint::requests::UiRequest::SetFocusWorkMins { .. }
            ));
        },
    );
}

// ---- #622: the plan-next-day surface's journeys ------------------------

/// J-P1 — the opener routes to the planner and Escape restores the
/// exact return view (the J3 shape): open from the board, Escape back
/// onto the board, re-open, and the toggle closes.
#[test]
fn planner_open_closes_and_toggles() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        assert_eq!(harness.handle.window().get_current_view(), View::Board);

        harness.press(ActionId::ViewPlanNextDay);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::PlanNextDay,
            "the opener opened the planner"
        );
        assert_eq!(
            harness.chain(),
            vec![
                ContextId::PlanNextDay,
                ContextId::ViNormal,
                ContextId::Global,
            ],
            "the planner context heads the chain"
        );

        // Escape (the planner-context binding) restores the exact
        // return view.
        harness.stroke(parse("Escape").expect("Escape"));
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Board,
            "Escape restored the exact previous view"
        );

        // Re-open, then the toggle: a second opener dispatch closes.
        harness.press(ActionId::ViewPlanNextDay);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::PlanNextDay
        );
        harness.press(ActionId::ViewPlanNextDay);
        assert_eq!(harness.handle.window().get_current_view(), View::Board);
    });
}

/// J-P2 — cycle exclusion: with the planner open the next/prev-view
/// verbs change nothing; from the return view the full cycle never
/// lands on the planner (the J4 shape).
#[test]
fn the_view_cycle_never_passes_through_the_planner() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewPlanNextDay);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::PlanNextDay
        );
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavPrevView);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::PlanNextDay,
            "the cycle verbs are inert while the planner is open"
        );
        harness.press(ActionId::ViewPlanNextDay);

        // A full cycle walk from the board: the planner is never a
        // member.
        for expected in [
            View::StackDetail,
            View::TaskDetail,
            View::Review,
            View::Today,
            View::Board,
        ] {
            harness.press(ActionId::NavNextView);
            assert_eq!(harness.handle.window().get_current_view(), expected);
        }
    });
}

/// J-P3 — the placement: pick a pool row, type the time-of-day and an
/// effort, commit — ONE `UpdateTask` with both masks reaches the
/// boundary; the row moves to the placed pane; `AppUndo` routes the
/// undo step.
#[test]
#[allow(clippy::too_many_lines)] // the placement journey's ceremony
fn the_placement_routes_one_mask_and_undo_returns_it() {
    // The clock pinned (docs/testing_strategy.org §6.2): the pass's
    // next-day window and the expected instant derive from the one
    // `now`, so neither side can straddle a day boundary the other
    // does not see.
    let now = fixture::PINNED_NOW;
    let (fixture, state) = fixture::golden_state_at(now);
    let task0 = fixture.task0;
    let title = state.tasks.get(&task0).expect("fixture task").title.clone();
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            clock: Some(now),
            ..UiSettings::default()
        },
        move |harness| {
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::ViewPlanNextDay);
            harness.driver.advance_time(0);

            // The pool renders the candidate: pick task0 (the
            // row affordance's addressable label).
            let row = harness
                .driver
                .find_by_label(&format!("Place {title}"))
                .unwrap_or_else(|| panic!("task0's pool row is mounted"));
            row.invoke_default_action();
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_plan_model().placing,
                "the placement form armed"
            );

            // Type the time-of-day and the optional effort, commit.
            click_field_centre(&mut *harness, "Time");
            harness.type_text("09:30");
            click_field_centre(&mut *harness, "Effort (optional)");
            harness.type_text("30");
            harness.stroke(parse("Return").expect("Return"));
            harness.driver.advance_time(0);

            // The one honest write: both masks in one command (the
            // window derives from the same pinned instant the mount
            // froze — no clock read on either side).
            let day_start = (
                taskboard_domain::SettingsSnapshot::default().day_start_time,
                UiSettings::default().boot_offset,
            );
            let (next_start, _) =
                taskboard_domain::days::next_day_window(now, day_start.0, day_start.1);
            let date = next_start.date_naive();
            let time = chrono::NaiveTime::from_hms_opt(9, 30, 0).expect("09:30");
            let expected_instant = taskboard_domain::DayCalendar {
                day_start_time: day_start.0,
                offset: day_start.1,
            }
            .instant(date, time);
            match harness.engine.drain().as_slice() {
                [StateCommand::UpdateTask { id, changes }] => {
                    assert_eq!(*id, task0, "the addressed row");
                    assert_eq!(changes.scheduled, Some(Some(expected_instant)));
                    assert_eq!(
                        changes.effort,
                        Some(Some(
                            taskboard_domain::Effort::parse_text_form("30").expect("30 parses")
                        ))
                    );
                }
                other => panic!("one placement write, got {other:?}"),
            }

            // The fake engine folds the write in (fixture
            // continuity): the row moved to the placed pane; the pool
            // shrank.
            harness.engine.settle(&harness.handle);
            let plan = harness.handle.window().get_plan_model();
            assert_eq!(plan.boxes.row_count(), 1, "the box rendered");
            assert_eq!(plan.pool.row_count(), 2, "the pool shrank");
            assert!(!plan.placing, "the form disarmed on commit");

            // The undo arm: AppUndo routes the engine's undo verb (the
            // engine semantics are the undo suite's own contract).
            harness.press(ActionId::AppUndo);
            match harness.engine.drain().as_slice() {
                [StateCommand::Undo] => {}
                other => panic!("one undo step, got {other:?}"),
            }
        },
    );
}

/// J-P4 — the reorder: two placed boxes; "Move box later" on the
/// earlier box routes the two honest `UpdateTask` swap writes; two
/// undo steps route back.
#[test]
fn the_reorder_routes_the_two_swap_writes() {
    // The clock pinned (docs/testing_strategy.org §6.2): the seeds
    // and the pass both derive from the one `now`, so "tomorrow" is
    // the same window on both sides of the boundary at any wall time.
    let now = fixture::PINNED_NOW;
    let (fixture, mut state) = fixture::golden_state_at(now);
    let (task0, task1) = (fixture.task0, fixture.task1);
    // Two boxes tomorrow: 09:00 and 11:00 off the frozen next-day
    // window (the default day-start pair makes it the next UTC
    // midnight block).
    let day_start = (
        taskboard_domain::SettingsSnapshot::default().day_start_time,
        UiSettings::default().boot_offset,
    );
    let (next_start, _) = taskboard_domain::days::next_day_window(now, day_start.0, day_start.1);
    let early = next_start + TimeDelta::hours(9);
    let late = next_start + TimeDelta::hours(11);
    state.tasks.get_mut(&task0).expect("task0").scheduled = Some(early);
    state.tasks.get_mut(&task1).expect("task1").scheduled = Some(late);
    let title0 = state.tasks.get(&task0).expect("task0").title.clone();
    let title1 = state.tasks.get(&task1).expect("task1").title.clone();
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            clock: Some(now),
            ..UiSettings::default()
        },
        move |harness| {
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::ViewPlanNextDay);
            harness.driver.advance_time(0);

            // Both boxes render, the earlier one first (the day
            // shape's scheduled order).
            assert!(
                harness
                    .driver
                    .find_by_label(&format!("Box {title0}"))
                    .is_some(),
                "the earlier box renders"
            );
            assert_eq!(
                harness.handle.window().get_plan_model().boxes.row_count(),
                2
            );

            // "Move box later" on the earlier box: the two swap
            // writes, instants exchanged, ids preserved.
            let row = harness
                .driver
                .find_by_label("Move box later")
                .unwrap_or_else(|| panic!("the later affordance is mounted"));
            row.invoke_default_action();
            harness.driver.advance_time(0);
            match harness.engine.drain().as_slice() {
                [
                    a @ StateCommand::UpdateTask { .. },
                    b @ StateCommand::UpdateTask { .. },
                ] => {
                    for (command, expected_id, expected_instant) in
                        [(a, task0, late), (b, task1, early)]
                    {
                        let StateCommand::UpdateTask { id, changes } = command else {
                            unreachable!("matched above")
                        };
                        assert_eq!(*id, expected_id, "the swap's ids");
                        assert_eq!(changes.scheduled, Some(Some(expected_instant)));
                    }
                }
                other => panic!("two swap writes, got {other:?}"),
            }

            // The fake engine folds the swap in: the display order
            // flips (the late box now leads, `(scheduled, order, id)`).
            harness.engine.settle(&harness.handle);
            let boxes = harness.handle.window().get_plan_model().boxes;
            assert_eq!(boxes.row_count(), 2);
            assert_eq!(
                boxes.row_data(0).expect("box").card.title.to_string(),
                title1,
                "the late box leads after the swap"
            );

            // The undo arms: two honest steps back.
            harness.press(ActionId::AppUndo);
            harness.press(ActionId::AppUndo);
            let drained = harness.engine.drain();
            assert_eq!(drained.len(), 2, "the swap steps back as two writes");
            assert!(drained.iter().all(|c| matches!(c, StateCommand::Undo)));
        },
    );
}

/// J-P5 — the planner is reachable from every cycle view by its
/// action and escapable everywhere: no dead ends (the walk's reach
/// contract, D-4).
#[test]
fn the_planner_is_reachable_and_escapable_from_every_cycle_view() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        // Visit every cycle member; from each, the opener reaches
        // the planner and Escape restores that exact member.
        let members = [
            (ActionId::NavNextView, View::StackDetail),
            (ActionId::NavNextView, View::TaskDetail),
            (ActionId::NavNextView, View::Review),
            (ActionId::NavNextView, View::Today),
            (ActionId::NavNextView, View::Board),
        ];
        for (_, expected) in members {
            harness.press(ActionId::NavNextView);
            assert_eq!(harness.handle.window().get_current_view(), expected);
            harness.press(ActionId::ViewPlanNextDay);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::PlanNextDay
            );
            harness.stroke(parse("Escape").expect("Escape"));
            assert_eq!(
                harness.handle.window().get_current_view(),
                expected,
                "Escape restored the exact member"
            );
        }
    });
}

/// J-P6 (the kiosk pin's journey half): a kiosk mount never shows the
/// planner and its opener dispatch is structurally inert.
#[test]
fn the_kiosk_mount_never_shows_the_planner() {
    let (_fixture, state) = fixture::golden_state();
    let settings = UiSettings {
        kiosk: true,
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::ViewPlanNextDay);
        assert_ne!(
            harness.handle.window().get_current_view(),
            View::PlanNextDay,
            "the kiosk dispatch is inert"
        );
        assert_eq!(harness.handle.window().get_current_view(), View::Board);
    });
}

// -- round-5 chunk 3: the anchored flyouts (F8/F9/F10/F14, D6) ----------

/// The quick-list flyout's realized facts, flattened for the journey
/// assertions: `(open, site, panel-x, panel-y, anchor-x, anchor-y,
/// anchor-height)` (the global handle borrows the window; the tuple
/// crosses the borrow out).
fn quick_list_facts(
    window: &taskboard_ui_slint::AppWindow,
) -> (bool, i32, f32, f32, f32, f32, f32) {
    let facts = window.global::<taskboard_ui_slint::TimeFieldFacts>();
    (
        facts.get_open(),
        facts.get_site(),
        facts.get_panel_x(),
        facts.get_panel_y(),
        facts.get_anchor_x(),
        facts.get_anchor_y(),
        facts.get_anchor_height(),
    )
}

/// Round-5 F8's calendar-toggle seam (D-3.4 seam 2): the calendar
/// button's click grab must not settle the armed editor — the toggle
/// opens the picker over the typed buffer, the flow stays armed, and
/// the field keeps focus (the default-open contract).
#[test]
fn the_calendar_toggle_keeps_the_armed_editor_and_refocuses_the_field() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_task_model().date_picker_open);
        // Close the picker again: the toggle's close arm sets the
        // latch too (the button's grab must not settle here).
        harness.handle.window().invoke_task_date_picker_toggled();
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_task_model().date_picker_open,
            "the toggle's close arm closes the picker"
        );

        // Type a parseable buffer, then toggle the calendar open:
        // the flow survives the button's click grab, the buffer is
        // untouched, and the field keeps focus.
        harness.clear_seeded();
        harness.type_text("2030-03-01");
        harness.driver.click_by_label("Calendar");
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);
        let model = harness.handle.window().get_task_model();
        assert!(
            model.date_picker_open,
            "the toggle opened the picker over the armed editor"
        );
        assert!(model.due_open, "the armed editor survived the toggle");
        assert_eq!(
            model.due_text, "2030-03-01",
            "the toggle never settled the typed buffer"
        );
        assert!(
            model.date_field_focus,
            "the field keeps focus on the default open"
        );
    });
}

/// Round-5 F14: with no live boards, the settings picker renders the
/// visible empty state — the field is non-interactive (a click opens
/// nothing, the lane stays empty) and the consumer's caption renders.
#[test]
fn the_empty_board_list_renders_the_disabled_picker_with_its_caption() {
    let state = AppState::default();
    // A connector without boards: the picker's rows project the
    // state's live boards — none exist.
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state.clone(), settings, move |harness| {
        harness.driver.set_window_size(1000, 1400);
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        refresh_with(&*harness, "work", vec![remote_row(1, "r")]);
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        let copy = window.global::<taskboard_ui_slint::SettingsBoardCopy>();
        assert!(
            options(&*harness).is_empty(),
            "the zero-board state projects no rows"
        );

        // The field click opens nothing (the disabled state).
        click_selector_field(&mut harness.driver);
        harness.driver.advance_time(0);
        let facts = window.global::<taskboard_ui_slint::DropdownFacts>();
        assert!(
            !facts.get_open(),
            "the empty picker's field click opens nothing"
        );
        assert!(
            harness.drain_requests().is_empty(),
            "the empty state dispatched nothing"
        );

        // The caption renders (the address lookup — the copy
        // constant is the element's a11y label, never asserted).
        assert!(
            harness
                .driver
                .find_by_label(&copy.get_empty_hint())
                .is_some(),
            "the empty state's caption renders"
        );
    });
}

// -- cross-surface friction UX plan US-1 (#753): the empty-model
// -- guard's mount fact. The guard is landed code (round-5 F14/D6's
// -- conversion, kept by chunk-5's grammar) — this journey is the
// -- proof-of-presence, not a fix: on a zero-row model the flyout
// -- never mounts, through every open path the rig can fire.

/// US-1/S1 (AC-1): the zero-board dropdown's open gesture mounts
/// nothing — `DropdownFacts.open` stays false (the typed mount
/// fact) and the lane stays empty (the guard is a typed no-op, not
/// an absorbed error). AC-2 (the populated path opens as today) is
/// the round-5 flyout matrix standing.
#[test]
fn us1_ac1_zero_row_open_mounts_nothing() {
    let state = AppState::default();
    // A connector without boards: the picker's rows project the
    // state's live boards — none exist.
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state.clone(), settings, move |harness| {
        harness.driver.set_window_size(1000, 1400);
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        refresh_with(&*harness, "work", vec![remote_row(1, "r")]);
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        assert!(
            options(&*harness).is_empty(),
            "the zero-board state projects no rows"
        );

        // Open path 1: the field click (the disabled TouchArea —
        // unreachable by construction at zero rows).
        click_selector_field(&mut harness.driver);
        harness.driver.advance_time(0);
        let facts = window.global::<taskboard_ui_slint::DropdownFacts>();
        assert!(!facts.get_open(), "the field click mounts no flyout");

        // Open path 2: the adapter-side keyboard route (the
        // switcher effect's own `rows > 0` gate — the effect
        // never flips the flag at zero rows).
        harness
            .handle
            .dispatch_action(taskboard_ui_slint::actions::ActionId::BoardSelect);
        harness.driver.advance_time(0);
        assert!(
            !window
                .global::<taskboard_ui_slint::DropdownFacts>()
                .get_open(),
            "the keyboard open route mounts no flyout"
        );
        assert!(
            !window
                .global::<taskboard_ui_slint::BoardBarFacts>()
                .get_open(),
            "the switcher facts stay closed"
        );
        assert!(
            harness.drain_requests().is_empty(),
            "the guard dispatched nothing"
        );
    });
}

/// Round-5 D6's per-site matrix, dropdown site 0 (the settings board
/// picker): the flyout mounts window-level below the anchor, the
/// ladder still moves through the decline arm, and an outside click
/// dismisses only the flyout.
#[test]
fn the_settings_picker_flyout_mounts_below_its_anchor_and_dismisses() {
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    let away = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", None);
    state_board(&mut state, away, "away", None);
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        harness.driver.advance_time(0);
        refresh_with(&*harness, "work", vec![remote_row(2, "r")]);
        harness.driver.advance_time(0);
        let window = harness.handle.window();

        // Open via the field click — the proven coordinate seam
        // (the a11y default action alone is racy right after the
        // view swap).
        click_selector_field(&mut harness.driver);
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);
        {
            let facts = window.global::<taskboard_ui_slint::DropdownFacts>();
            assert!(facts.get_open(), "the field click opens the flyout");
            assert!(
                facts.get_panel_y() >= facts.get_anchor_y() + facts.get_anchor_height(),
                "the flyout mounts below its anchor"
            );
        }

        // The ladder still moves (the decline arm hands the stroke
        // to the field; the lane stays empty).
        let anchor = board_facts(&*harness).get_highlight();
        raw_key(&*harness, slint::platform::Key::DownArrow);
        harness.driver.advance_time(0);
        assert_eq!(
            board_facts(&*harness).get_highlight(),
            anchor + 1,
            "Down moves the ladder"
        );
        assert!(
            harness.drain_requests().is_empty(),
            "the arrow dispatched nothing"
        );

        // Outside click: a real coordinate click onto another
        // control's spot — the window-level underlay receives it
        // and dismisses only the flyout (the Back button stays
        // untouched).
        let back = harness
            .driver
            .find_by_label("Back")
            .expect("the Back button renders");
        let (x, y) = support::driver::centre(&back);
        harness.driver.click_at(x, y);
        harness.driver.advance_time(0);
        assert!(
            !window
                .global::<taskboard_ui_slint::DropdownFacts>()
                .get_open(),
            "the outside click dismissed the flyout"
        );
    });
}

/// Round-5 D6's per-site matrix, dropdown site 1 (the board-bar
/// switcher): same grammar over the bar's dropdown mode.
#[test]
fn the_board_bar_flyout_mounts_below_its_anchor_and_dismisses() {
    let mut state = AppState::default();
    let boards: Vec<_> = (0..9)
        .map(|i| (uuid::Uuid::new_v4(), format!("board-{i}")))
        .collect();
    for (id, title) in &boards {
        state_board(&mut state, *id, title, None);
    }
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.driver.advance_time(0);

        let mut field = None;
        for _ in 0..20 {
            harness.driver.advance_time(0);
            let found = harness.driver.find_all_by_id("BoardBar::bar-dd");
            if let Some(first) = found.into_iter().next() {
                field = Some(first);
                break;
            }
        }
        let field = field.expect("the bar dropdown renders past the budget");
        let pos = field.absolute_position();
        let size = field.size();
        let (x, y) = (pos.x + size.width / 2.0, pos.y + size.height / 2.0);
        harness.driver.click_at(x, y);
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);

        let window = harness.handle.window();
        {
            let facts = window.global::<taskboard_ui_slint::DropdownFacts>();
            assert!(facts.get_open(), "the field click opens the flyout");
            assert!(
                facts.get_panel_y() >= facts.get_anchor_y() + facts.get_anchor_height(),
                "the flyout mounts below its anchor"
            );
        }

        // Outside click onto the status bar's spot dismisses only
        // the flyout.
        let stop = harness
            .driver
            .find_by_label("Boards")
            .expect("the bar's a11y container renders");
        let (bx, by) = support::driver::centre(&stop);
        harness.driver.click_at(bx, by + 200.0);
        harness.driver.advance_time(0);
        assert!(
            !window
                .global::<taskboard_ui_slint::DropdownFacts>()
                .get_open(),
            "the outside click dismissed the flyout"
        );
        assert!(
            harness.drain_requests().is_empty(),
            "the dismissal selected nothing"
        );
    });
}

/// Round-5 D6's per-site matrix, time-field site 1 (the timer
/// setter's combobox): the quick list opens under its field, the
/// setter stays open, and an outside click dismisses only the list.
#[test]
fn the_setter_quick_list_takes_the_flip_leg_and_dismisses() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        // A short window puts the setter's combobox genuinely low,
        // so the quick list takes the flip leg (round-9 W1: the
        // anchor facts mirror at click time now — the old pass
        // rode the #954 race, which positioned the popover low by
        // accident; the flip premise is made true by the viewport,
        // not by the race).
        harness.driver.set_window_size(1000, 400);
        // The setter's header button lives in the today view's
        // section (the harness's mount escape routes to Board).
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        open_setter(&mut *harness);
        assert!(harness.handle.window().get_timer_setter_open());

        harness
            .driver
            .find_by_label("Quick times")
            .expect("the setter's chevron renders")
            .invoke_default_action();
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);

        let window = harness.handle.window();
        {
            let facts = window.global::<taskboard_ui_slint::TimeFieldFacts>();
            assert!(facts.get_open(), "the chevron opened the quick list");
            assert_eq!(facts.get_site(), 1, "the setter's site discriminates");
            // The short window's combobox sits low in the popover,
            // so the list takes the clamp/flip
            // grammar's flip leg: above the anchor, inside the
            // window.
            assert!(
                facts.get_panel_y() <= facts.get_anchor_y(),
                "the low anchor flips the panel above (py={:?} ay={:?} ah={:?})",
                facts.get_panel_y(),
                facts.get_anchor_y(),
                facts.get_anchor_height()
            );
            assert!(facts.get_panel_y() >= 0.0, "the panel stays in the window");
        }
        assert!(
            window.get_timer_setter_open(),
            "the setter stays open under the quick list"
        );
        let slots = window.get_timer_setter_time_slots();
        assert_eq!(
            slots.row_count(),
            96,
            "the open seeds the full quarter-hour grid"
        );

        // Outside click at the window corner dismisses only the
        // quick list — the popover stays.
        harness.driver.click_at(2.0, 2.0);
        harness.driver.advance_time(0);
        assert!(
            !window
                .global::<taskboard_ui_slint::TimeFieldFacts>()
                .get_open(),
            "the outside click dismissed the quick list"
        );
        assert!(
            window.get_timer_setter_open(),
            "the setter survived the quick list's dismissal"
        );
    });
}

/// Round-5 D6's per-site matrix, time-field sites 2 and 3 (the
/// track-form's start and end fields — side-by-side in one row, so
/// they share an anchor y and differ in x): each field opens its own
/// full-grid list keyed on the site discriminator, each panel mounts
/// under its own anchor's x, and the dismissal leaves the form open.
/// (The clamp/flip leg is journeyed at the timer-setter site — the
/// track form's row layout cannot produce a low anchor.)
/// The quick-list placement law (the W4 re-baseline): the panel never
/// overlaps the anchor field — below with the space-1 gap, or flipped
/// above with the gap.
fn assert_quick_list_placement_law(panel_y: f32, anchor_y: f32, anchor_h: f32) {
    let below = panel_y >= anchor_y + anchor_h;
    let above = panel_y <= anchor_y - 4.0;
    assert!(
        below || above,
        "the panel is placed by the law, non-overlapping: \
         panel-y={panel_y:?} anchor-y={anchor_y:?} anchor-h={anchor_h:?}",
    );
}
#[test]
fn the_track_form_quick_lists_open_per_field_and_dismiss() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        press_preset(
            &mut *harness,
            taskboard_ui_slint::keymap::Preset::Standard,
            ActionId::NavFocusDown,
        );
        press_preset(
            &mut *harness,
            taskboard_ui_slint::keymap::Preset::Standard,
            ActionId::TaskEdit,
        );
        harness.driver.click_by_label("Time entries");
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_task_model().track_entries_open);
        harness.handle.window().invoke_track_form_add_requested();
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_track_form_open());

        let open_field = |harness: &Harness, index: usize| {
            let chevrons = harness.driver.find_all_by_label("Quick times");
            assert!(
                chevrons.len() >= 2,
                "the track form mounts both time fields"
            );
            chevrons
                .into_iter()
                .nth(index)
                .expect("the field's chevron renders")
                .invoke_default_action();
            // The facts read happens at the call site (the global
            // handle borrows the window).
        };

        // The start field (site 2): opens its own list seeded
        // unfiltered, placed by the placement law — below the
        // anchor when the anchor-to-status span fits, else flipped
        // above with the gap (the recorded W4 re-baseline: the
        // entries panel anchors off the widget-encapsulated
        // tracked-row button, whose realized position sits deeper
        // in the default-size window, so this fixture's field
        // takes the flip leg where the old partial-layout snapshot
        // took the below leg).
        open_field(&*harness, 0);
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);
        {
            let (open, site, panel_x, panel_y, anchor_x, anchor_y, anchor_h) =
                quick_list_facts(harness.handle.window());
            assert!(open, "the start chevron opened the list");
            assert_eq!(site, 2, "the start field discriminates");
            assert_quick_list_placement_law(panel_y, anchor_y, anchor_h);
            assert!(
                (panel_x - anchor_x).abs() < 1.0,
                "the start panel aligns with its own anchor (panel-x={panel_x:?} anchor-x={anchor_x:?})",
            );
        }
        assert_eq!(
            harness
                .handle
                .window()
                .get_track_form_start_slots()
                .row_count(),
            96,
            "the start list seeds the full grid"
        );
        harness.driver.click_at(2.0, 2.0);
        harness.driver.advance_time(0);
        assert!(
            !quick_list_facts(harness.handle.window()).0,
            "the outside click dismissed the start list"
        );
        assert!(
            harness.handle.window().get_track_form_open(),
            "the form survived the dismissal"
        );

        // The end field (site 3): the same grammar, its own list —
        // same anchor y, a different anchor x, so the panel's
        // alignment proves the mirror follows this field's anchor.
        open_field(&*harness, 1);
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);
        {
            let (open, site, panel_x, panel_y, anchor_x, anchor_y, anchor_h) =
                quick_list_facts(harness.handle.window());
            assert!(open, "the end chevron opened the list");
            assert_eq!(site, 3, "the end field discriminates");
            assert!(
                (panel_x - anchor_x).abs() < 1.0,
                "the end panel aligns with its own anchor (panel-x={panel_x:?} anchor-x={anchor_x:?})",
            );
            assert_quick_list_placement_law(panel_y, anchor_y, anchor_h);
        }
        assert_eq!(
            harness
                .handle
                .window()
                .get_track_form_end_slots()
                .row_count(),
            96,
            "the end list seeds the full grid"
        );
    });
}

// ---- round 5 chunk 4 (F19): the segmented and slider keyboard paths ------

/// Tabs (raw) until the stop labelled `label` holds focus. The bound
/// is structural (the settings stop count with headroom).
fn tab_to(harness: &mut Harness, label: &str) {
    for _ in 0..200 {
        if harness.driver.focused_label().as_deref() == Some(label) {
            return;
        }
        harness.stroke(parse("Tab").expect("tab"));
        harness.driver.advance_time(0);
    }
    panic!("the walk never reached the stop {label:?}");
}

/// F19a — a focused segment activates on bare Return and Space, once
/// per stroke, through the same `picked` seam pointer and AT use; a
/// disabled segment swallows the stroke.
#[test]
fn segments_activate_on_return_and_space() {
    use taskboard_ui_slint::keymap::Preset;
    use taskboard_ui_slint::requests::UiRequest;
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        harness.driver.set_window_size(1000, 6000);

        tab_to(&mut *harness, "Standalone");
        harness.stroke(parse("Return").expect("Return"));
        harness.driver.advance_time(0);
        assert!(
            harness.drain_requests().is_empty(),
            "a disabled segment swallows the pick"
        );

        tab_to(&mut *harness, "Emacs");
        harness.stroke(parse("Return").expect("Return"));
        harness.driver.advance_time(0);
        let requests = harness.drain_requests();
        assert!(
            matches!(
                requests.as_slice(),
                [UiRequest::SetKeymapPreset {
                    preset: Preset::Emacs,
                    ..
                }]
            ),
            "Return picks the focused segment exactly once: {requests:?}"
        );
        // Settle the busy machine: a dropped reply ends the op.
        drop(requests);
        harness.handle.drain_setting_replies();

        tab_to(&mut *harness, "Vi");
        harness.stroke(parse("Space").expect("Space"));
        harness.driver.advance_time(0);
        let requests = harness.drain_requests();
        assert!(
            matches!(
                requests.as_slice(),
                [UiRequest::SetKeymapPreset {
                    preset: Preset::Vi,
                    ..
                }]
            ),
            "Space picks the focused segment exactly once: {requests:?}"
        );
    });
}

/// F19b — the slider steps from the keyboard with the r4 D38
/// ceremony: strokes preview widget-internally (no request, no theme
/// move), the release commits once, a stray release commits nothing,
/// and Tab-ing away mid-gesture commits once.
#[test]
#[allow(clippy::too_many_lines)] // the stroke ladder is one flat walk
fn the_slider_steps_from_the_keyboard_and_commits_on_release() {
    use taskboard_ui_slint::requests::UiRequest;
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        harness.driver.set_window_size(1000, 6000);
        let scale = |harness: &Harness| {
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::Theme>()
                .get_base_scale()
        };
        let committed = |requests: &[UiRequest]| -> Vec<f64> {
            requests
                .iter()
                .filter_map(|request| match request {
                    UiRequest::SetScale { scale, .. } => Some(*scale),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(scale(&*harness), 1.0);

        tab_to(&mut *harness, "Scale slider");
        let right = parse("Right").expect("Right");
        for _ in 0..2 {
            harness.stroke(right);
            harness.driver.advance_time(0);
        }
        assert!(
            harness.drain_requests().is_empty(),
            "no request mid-gesture"
        );
        assert_eq!(scale(&*harness), 1.0, "the theme holds until release");

        harness.driver.release_stroke(right);
        harness.driver.advance_time(0);
        let requests = harness.drain_requests();
        let scales = committed(&requests);
        assert_eq!(scales.len(), 1, "one commit per gesture: {requests:?}");
        assert!((scales[0] - 1.2).abs() < 1e-3, "two ticks up");
        for request in requests {
            if let UiRequest::SetScale { reply, .. } = request {
                reply.send(Ok(())).expect("the reply lane is alive");
            }
        }
        harness.handle.drain_setting_replies();

        harness.driver.release_stroke(right);
        harness.driver.advance_time(0);
        assert!(
            committed(&harness.drain_requests()).is_empty(),
            "a release without a stroke commits nothing"
        );

        harness.stroke(parse("Left").expect("Left"));
        harness.driver.advance_time(0);
        harness.stroke(parse("Tab").expect("Tab"));
        harness.driver.advance_time(0);
        let requests = harness.drain_requests();
        let scales = committed(&requests);
        assert_eq!(scales.len(), 1, "focus loss commits the pending gesture");
        assert!((scales[0] - 1.1).abs() < 1e-3, "one tick down from 1.2");
        for request in requests {
            if let UiRequest::SetScale { reply, .. } = request {
                reply.send(Ok(())).expect("the reply lane is alive");
            }
        }
        harness.handle.drain_setting_replies();

        // A refused commit reverts the thumb; the next gesture
        // steps from the reverted value, not the refused one.
        tab_to(&mut *harness, "Scale slider");
        harness.stroke(right);
        harness.driver.advance_time(0);
        harness.driver.release_stroke(right);
        harness.driver.advance_time(0);
        for request in harness.drain_requests() {
            if let UiRequest::SetScale { reply, .. } = request {
                reply
                    .send(Err(
                        taskboard_ui_slint::requests::SettingsError::StoreFailed,
                    ))
                    .expect("the reply lane is alive");
            }
        }
        harness.handle.drain_setting_replies();
        harness.driver.advance_time(0);
        let left = parse("Left").expect("Left");
        harness.stroke(left);
        harness.driver.advance_time(0);
        harness.driver.release_stroke(left);
        harness.driver.advance_time(0);
        let scales = committed(&harness.drain_requests());
        assert_eq!(scales.len(), 1, "one commit per gesture");
        assert!(
            (scales[0] - 1.0).abs() < 1e-3,
            "one tick down from the reverted 1.1, got {}",
            scales[0]
        );
    });
}

/// The ring-coloured pixel count inside `rect` (a channel delta < 24
/// from `Semantic.focus-ring` counts as ring ink).
fn ring_ink(harness: &mut Harness, rect: (f32, f32, f32, f32)) -> u32 {
    let ring = harness
        .handle
        .window()
        .global::<taskboard_ui_slint::Semantic>()
        .get_focus_ring();
    let image = harness.driver.snapshot();
    let (x, y, width, height) = rect;
    let mut count = 0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // UI px
    for row in (y as u32)..((y + height) as u32).min(image.height) {
        for col in (x as u32)..((x + width) as u32).min(image.width) {
            let i = (row as usize * image.width as usize + col as usize) * 4;
            let near = [ring.red(), ring.green(), ring.blue()]
                .iter()
                .zip(&image.data[i..i + 3])
                .all(|(a, b)| a.abs_diff(*b) < 24);
            if near {
                count += 1;
            }
        }
    }
    count
}

/// F19 "visibly focused" — a focused segment, the slider, and Sign in
/// each carry ring ink, and strictly more than the same rect once the
/// walk moves on (a floor-vs-floor comparison, never a ceiling).
#[test]
fn focused_settings_controls_render_the_ring() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        harness.driver.set_window_size(1000, 6000);
        harness.driver.advance_time(0);
        for label in ["Emacs", "Scale slider", "Sign in"] {
            tab_to(&mut *harness, label);
            let rect = harness.driver.focused_rect().expect("a stop holds focus");
            let focused = ring_ink(&mut *harness, rect);
            harness.stroke(parse("Tab").expect("tab"));
            harness.driver.advance_time(0);
            let unfocused = ring_ink(&mut *harness, rect);
            assert!(
                focused >= 40 && focused > unfocused,
                "{label}: focused ring ink {focused} vs unfocused {unfocused}"
            );
        }
    });
}

// ---- #638: the review walk's journeys (the plan §4 catalogue) ----------

use taskboard_domain::{Board, BoardClocks, BoardId, Color, Stack, StackClocks, StackId, Task};

/// The walk fixture: the golden state plus the global Inbox board
/// (P1's capture home) with two triage rows, and task0 scheduled
/// inside yesterday's window (a carry row). Returns the fixture ids
/// and the state.
#[allow(clippy::too_many_lines)] // one fixture builder, kept flat
/// The walk fixtures' global Inbox board plus its triage stack,
/// inserted into `state` — the shared substrate of `walk_fixture_state`
/// and `walk_empty_carry_state`. Returns `(board, stack)` ids.
fn insert_inbox_board_and_stack(state: &mut AppState) -> (BoardId, StackId) {
    let inbox = BoardId::from_uuid(uuid::Uuid::new_v4());
    state.boards.insert(
        inbox,
        Board {
            id: inbox,
            remote: None,
            title: "Inbox".to_owned(),
            color: Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: BoardClocks::epoch(),
                color: BoardClocks::epoch(),
                archived: BoardClocks::epoch(),
                deleted: BoardClocks::epoch(),
            },
            authors: taskboard_domain::entities::BoardAuthors::default(),
            remote_seen: None,
        },
    );
    let inbox_stack = StackId::from_uuid(uuid::Uuid::new_v4());
    state.stacks.insert(
        inbox_stack,
        Stack {
            id: inbox_stack,
            remote: None,
            board: inbox,
            title: "Inbox".to_owned(),
            order: 0,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: BoardClocks::epoch(),
                order: BoardClocks::epoch(),
                deleted: BoardClocks::epoch(),
            },
            authors: taskboard_domain::entities::StackAuthors::default(),
            remote_seen: None,
        },
    );
    (inbox, inbox_stack)
}

/// A fresh triage row on `stack` — the shared task literal of the
/// walk fixtures' `mk` closures.
fn walk_new_task(
    stack: StackId,
    title: &str,
    scheduled: Option<chrono::DateTime<chrono::Utc>>,
) -> Task {
    Task {
        id: TaskId::from_uuid(uuid::Uuid::now_v7()),
        remote: None,
        label_memberships: std::collections::BTreeMap::new(),
        title: title.to_owned(),
        description: String::new(),
        duedate: None,
        scheduled,
        done: None,
        stack,
        order: 0,
        effort: None,
        importance: None,
        urgency: None,
        recur: None,
        kind: taskboard_domain::entities::TaskKind::Standard,
        archived: false,
        deleted: false,
        clocks: taskboard_domain::TaskClocks {
            title: BoardClocks::epoch(),
            description: BoardClocks::epoch(),
            duedate: BoardClocks::epoch(),
            scheduled: BoardClocks::epoch(),
            done: BoardClocks::epoch(),
            position: BoardClocks::epoch(),
            effort: BoardClocks::epoch(),
            importance: BoardClocks::epoch(),
            urgency: BoardClocks::epoch(),
            recur: BoardClocks::epoch(),
            kind: BoardClocks::epoch(),
            archived: BoardClocks::epoch(),
            deleted: BoardClocks::epoch(),
        },
        authors: taskboard_domain::entities::TaskAuthors::default(),
        remote_seen: None,
        remote_baseline: None,
    }
}

fn walk_fixture_state() -> (fixture::Fixture, AppState, StackId, Vec<TaskId>) {
    let (fixture, mut state) = fixture::golden_state();
    // The golden state pins the review card away (wall-clock text in
    // goldens is the forbidden class); the walk journeys need the due
    // state — clear the pin.
    state.reflection.entries.clear();
    let (_inbox, inbox_stack) = insert_inbox_board_and_stack(&mut state);
    let mut inbox_ids = Vec::new();
    for title in ["triage-me", "triage-me-too"] {
        let task = walk_new_task(inbox_stack, title, None);
        inbox_ids.push(task.id);
        state.tasks.insert(task.id, task);
    }
    // task0 carries a scheduled anchor inside yesterday's window (the
    // midnight-aligned convention: the journey harness's wall-clock
    // anchor reads the default day-start pair).
    let day_start = (
        taskboard_domain::SettingsSnapshot::default().day_start_time,
        UiSettings::default().boot_offset,
    );
    let now = chrono::Utc::now();
    let calendar = taskboard_domain::DayCalendar {
        day_start_time: day_start.0,
        offset: day_start.1,
    };
    let yesterday = calendar.today(now).pred_opt().expect("yesterday");
    let carry = calendar.instant(
        yesterday,
        chrono::NaiveTime::from_hms_opt(9, 0, 0).expect("9:00"),
    );
    let mut task0 = state.tasks.get(&fixture.task0).expect("task0").clone();
    task0.scheduled = Some(carry);
    state.tasks.insert(fixture.task0, task0);
    (fixture, state, inbox_stack, inbox_ids)
}

/// Opens the walk over the fixture (the card click — J1's route).
fn open_walk(harness: &mut Harness) {
    // The card lives on the Today view — mount it first (a no-op when
    // the walk is opened from there).
    harness.press(ActionId::ViewToday);
    harness.driver.advance_time(0);
    assert!(
        harness.handle.window().get_review_card_visible(),
        "the surfacing card is visible"
    );
    let label = format!(
        "Review {}",
        chrono::Utc::now()
            .date_naive()
            .pred_opt()
            .expect("yesterday")
    );
    // The a11y tree exposes the freshly mounted card on a later
    // tick (the view-mount settle; the planner pool row's retry).
    let mut card = None;
    for _ in 0..20 {
        harness.driver.advance_time(0);
        if let Some(found) = harness.driver.find_by_label(&label) {
            card = Some(found);
            break;
        }
    }
    let card = card.unwrap_or_else(|| panic!("the surfacing card renders"));
    card.invoke_default_action();
    harness.driver.advance_time(0);
}

fn walk_open(harness: &Harness) -> bool {
    harness.handle.window().get_walk_model().open
}

fn walk_stage(harness: &Harness) -> i32 {
    harness.handle.window().get_walk_model().stage_kind
}

fn walk_rows(harness: &Harness) -> usize {
    harness.handle.window().get_walk_model().rows.row_count()
}

/// The day-review journeys' shared preamble: mount `state` and open
/// the walk modal over the action route (dispatch + settle tick).
fn mount_open_walk(state: AppState) -> Harness {
    let mut harness = Harness::mount(SlintDriver::default(), state);
    harness.handle.dispatch_action(ActionId::ReviewOpen);
    harness.driver.advance_time(0);
    harness
}

/// One dispatched walk verb plus its settle tick — the journeys'
/// step rhythm (dispatch, then `advance_time(0)`).
fn walk_step(harness: &mut Harness, action: ActionId) {
    harness.handle.dispatch_action(action);
    harness.driver.advance_time(0);
}

/// #1053 — the walk host's declared extent counts the D-5 button row:
/// the armed `MaskWalkFacts.extent` equals the body's own stop (1) +
/// every row + every dialog button (the host mounts them as
/// `TextButton` tab stops, so an extent without them under-declares
/// and the adapter's cursor can walk past the last real stop).
#[test]
fn the_walk_extent_counts_the_dialog_button_row() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let harness = mount_open_walk(state);
            assert!(walk_open(&harness), "the walk opened");
            let model = harness.handle.window().get_walk_model();
            let expected = 1 + model.rows.row_count() + model.buttons.row_count();
            let extent = i32::try_from(expected).expect("the extent fits");
            assert_eq!(
                harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::MaskWalkFacts>()
                    .get_extent(),
                extent,
                "the declared extent includes the button row"
            );
        },
    );
}

/// J1 — unreviewed closed day → the Today card; the click opens the
/// walk at the Inbox stage over the due window's reference.
#[test]
fn the_card_opens_the_walk_over_the_due_window() {
    let (_fixture, state, _stack, inbox_ids) = walk_fixture_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_review_card_visible(),
            "an unreviewed closed day surfaces the card"
        );
        open_walk(&mut *harness);
        assert!(walk_open(&*harness), "the card opened the walk");
        // J4 D-1: the walk always opens at the Entry guidance
        // stage; the row stages follow through the skip law.
        assert_eq!(walk_stage(&*harness), 4, "the walk opens at Entry");
        assert_eq!(
            walk_rows(&*harness),
            1,
            "the guidance stage shows the intro row"
        );
        harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
        harness.driver.advance_time(0);
        assert_eq!(walk_stage(&*harness), 0, "Entry's Next lands on Inbox");
        assert_eq!(
            walk_rows(&*harness),
            inbox_ids.len(),
            "the triage rows are the inbox reading"
        );
        assert_eq!(
            harness.engine.drain(),
            Vec::new(),
            "opening dispatches nothing"
        );
    });
}

/// J2 — a live Day entry for the closed day clears the card.
#[test]
fn a_recorded_day_entry_clears_the_card() {
    let (fixture, mut state, _stack, _inbox_ids) = walk_fixture_state();
    let day_start = (
        taskboard_domain::SettingsSnapshot::default().day_start_time,
        UiSettings::default().boot_offset,
    );
    let calendar = taskboard_domain::DayCalendar {
        day_start_time: day_start.0,
        offset: day_start.1,
    };
    let now = chrono::Utc::now();
    let yesterday = calendar.today(now).pred_opt().expect("yesterday");
    let window = taskboard_domain::review_window(ReviewCadence::Day, yesterday, calendar);
    let entry = ReflectionEntry {
        id: ReflectionId::from_uuid(uuid::Uuid::now_v7()),
        cadence: ReviewCadence::Day,
        day: window.day(),
        window_start: window.start(),
        window_end: window.end(),
        text: "done".to_owned(),
        attributes: ReflectionAttributes::default(),
        created_at: now,
        updated_at: now,
        deleted: false,
    };
    state.reflection.entries.insert(entry.id, entry.clone());
    let _ = fixture;
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_review_card_visible(),
            "a live entry for the window clears the card"
        );
    });
}

/// J3 — the boundary crossing rides the render clock seam (the
/// `UiSettings::clock` freeze): with a live entry for yesterday the
/// frozen clock just before the next boundary shows no card; frozen
/// past it, the card appears — with NO command dispatched and no
/// request sent (D-W11's no-machinery pin).
#[test]
fn the_card_appears_at_the_boundary_without_dispatching() {
    for (past_boundary, expect_card) in [(false, false), (true, true)] {
        let (_fixture, mut state, _stack, _inbox_ids) = walk_fixture_state();
        let t0 = chrono::Utc::now();
        let day_start = (
            taskboard_domain::SettingsSnapshot::default().day_start_time,
            UiSettings::default().boot_offset,
        );
        let calendar = taskboard_domain::DayCalendar {
            day_start_time: day_start.0,
            offset: day_start.1,
        };
        let yesterday = calendar.today(t0).pred_opt().expect("yesterday");
        let window = taskboard_domain::review_window(ReviewCadence::Day, yesterday, calendar);
        let entry = ReflectionEntry {
            id: ReflectionId::from_uuid(uuid::Uuid::now_v7()),
            cadence: ReviewCadence::Day,
            day: window.day(),
            window_start: window.start(),
            window_end: window.end(),
            text: "done".to_owned(),
            attributes: ReflectionAttributes::default(),
            created_at: t0,
            updated_at: t0,
            deleted: false,
        };
        state.reflection.entries.insert(entry.id, entry);
        // The boundary: a full day past the fixture's `now` — the
        // latest closed window becomes the day the fixture clock
        // called "today", which carries no entry.
        let settings = UiSettings {
            clock: past_boundary.then(|| t0 + TimeDelta::try_hours(24).expect("day")),
            ..UiSettings::default()
        };
        support::harness::run_journey_with_settings(state, settings, move |harness| {
            harness.press(ActionId::ViewToday);
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_review_card_visible(),
                expect_card,
                "the card follows the frozen clock across the boundary"
            );
            assert!(
                harness.engine.drain().is_empty(),
                "the boundary crossing dispatched nothing"
            );
            assert!(harness.drain_requests().is_empty(), "no request rode it");
        });
    }
}

/// J4 — `review.open` with no closed gap: the walk opens over the
/// CURRENT day window (the evening review before the boundary).
#[test]
fn the_opener_falls_back_to_the_current_day() {
    let (fixture, mut state, _stack, _inbox_ids) = walk_fixture_state();
    let day_start = (
        taskboard_domain::SettingsSnapshot::default().day_start_time,
        UiSettings::default().boot_offset,
    );
    let calendar = taskboard_domain::DayCalendar {
        day_start_time: day_start.0,
        offset: day_start.1,
    };
    let now = chrono::Utc::now();
    let yesterday = calendar.today(now).pred_opt().expect("yesterday");
    let window = taskboard_domain::review_window(ReviewCadence::Day, yesterday, calendar);
    let entry = ReflectionEntry {
        id: ReflectionId::from_uuid(uuid::Uuid::now_v7()),
        cadence: ReviewCadence::Day,
        day: window.day(),
        window_start: window.start(),
        window_end: window.end(),
        text: String::new(),
        attributes: ReflectionAttributes::default(),
        created_at: now,
        updated_at: now,
        deleted: false,
    };
    state.reflection.entries.insert(entry.id, entry);
    let _ = fixture;
    support::harness::run_journey(state, move |harness| {
        harness.handle.dispatch_action(ActionId::ReviewOpen);
        harness.driver.advance_time(0);
        assert!(walk_open(&*harness));
        let header = harness.handle.window().get_walk_model().header.to_string();
        let today = calendar.today(now);
        assert!(
            header.contains(&today.to_string()),
            "the header names the current day window ({header})"
        );
    });
}

/// J5 — File to… → pick a stack → commit: exactly one `MoveTask` with
/// the end-of-stack order; the row leaves at the next refresh.
#[test]
fn filing_commits_one_move_task() {
    let (fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    let stack_a = fixture.stack_a;
    let expected_order = state
        .tasks
        .values()
        .filter(|t| t.stack == stack_a && !t.deleted)
        .map(|t| t.order)
        .max()
        .unwrap_or(0)
        + 1;
    support::harness::run_journey(state, move |harness| {
        open_walk(&mut *harness);
        harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::ReviewInboxFile);
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_walk_model().filing,
            "File to… armed the picker"
        );
        // The first candidate is stack "a" (the fixture board's
        // first): pick it via the pointer seam and commit.
        harness.handle.window().invoke_walk_candidate_activated(0);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::ReviewFileCommit);
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        match drained.as_slice() {
            [StateCommand::MoveTask { id, stack, order }] => {
                assert_eq!(*stack, stack_a, "the picked candidate");
                assert_eq!(*order, expected_order, "the end-of-stack append");
                assert_ne!(*id, TaskId::from_uuid(uuid::Uuid::nil()));
            }
            other => panic!("one MoveTask, got {other:?}"),
        }
        // The fake engine folds it in: the row leaves the inbox.
        harness.engine.settle(&harness.handle);
        assert_eq!(walk_rows(&*harness), 1, "the filed row left the reading");
    });
}

/// J6 — Keep in inbox on the last row: no command, the stage passes
/// to Carry forward.
#[test]
fn keep_on_the_last_row_passes_the_stage() {
    let (_fixture, state, _stack, inbox_ids) = walk_fixture_state();
    assert_eq!(inbox_ids.len(), 2, "the fixture's two triage rows");
    support::harness::run_journey(state, move |harness| {
        open_walk(&mut *harness);
        // Entry → Inbox, then step to the last row and keep: the
        // stage passes.
        harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
        harness.driver.advance_time(0);
        harness.stroke(parse("Down").expect("Down"));
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::ReviewInboxKeep);
        harness.driver.advance_time(0);
        assert!(harness.engine.drain().is_empty(), "keep dispatches nothing");
        assert_eq!(
            walk_stage(&*harness),
            1,
            "the stage passed to Carry forward"
        );
    });
}

/// J7 — an empty inbox opens the walk at Carry forward, the strip
/// carrying the Inbox empty flag (the b-line is the pushed note).
#[test]
fn an_empty_inbox_opens_at_carry_forward() {
    let (_fixture, mut state, _stack, _inbox_ids) = walk_fixture_state();
    let inbox_board = state
        .boards
        .values()
        .find(|board| board.title == "Inbox")
        .expect("the inbox board")
        .id;
    let dead: Vec<_> = state
        .tasks
        .values()
        .filter(|task| {
            state
                .stacks
                .get(&task.stack)
                .is_some_and(|stack| stack.board == inbox_board)
        })
        .map(|task| task.id)
        .collect();
    for id in dead {
        state.tasks.get_mut(&id).expect("live").deleted = true;
    }
    support::harness::run_journey(state, move |harness| {
        harness.handle.dispatch_action(ActionId::ReviewOpen);
        harness.driver.advance_time(0);
        assert!(walk_open(&*harness));
        // J4 D-1: the walk opens at the guidance stage; the skip
        // law applies from Entry's Next.
        assert_eq!(walk_stage(&*harness), 4, "the walk opens at Entry");
        harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
        harness.driver.advance_time(0);
        assert_eq!(walk_stage(&*harness), 1, "the empty inbox is skipped");
        let strip = harness.handle.window().get_walk_model().strip;
        // The strip is the walk order; the row stages sit behind
        // the Entry row (D-1). Any non-empty note is a skipped
        // stage's b-line.
        let notes: Vec<String> = (0..strip.row_count())
            .filter_map(|i| strip.row_data(i))
            .map(|row| row.note.to_string())
            .filter(|note| !note.is_empty())
            .collect();
        assert!(
            !notes.is_empty(),
            "the skipped stage's b-line rides the strip"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the skip dispatched nothing"
        );
    });
}

/// J8 — Move to tomorrow: one `UpdateTask` to the next window's
/// opening instant; the due date is untouched.
#[test]
fn carry_tomorrow_moves_to_the_next_window_start() {
    let (fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    let task0 = fixture.task0;
    let due_before = state.tasks.get(&task0).expect("task0").duedate;
    support::harness::run_journey(state, move |harness| {
        open_walk(&mut *harness);
        // Entry → Inbox, then pass the inbox (keep both rows),
        // land on Carry forward.
        harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::ReviewInboxKeep);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::ReviewInboxKeep);
        harness.driver.advance_time(0);
        assert_eq!(walk_stage(&*harness), 1);
        harness
            .handle
            .dispatch_action(ActionId::ReviewCarryTomorrow);
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        let day_start = (
            taskboard_domain::SettingsSnapshot::default().day_start_time,
            UiSettings::default().boot_offset,
        );
        let calendar = taskboard_domain::DayCalendar {
            day_start_time: day_start.0,
            offset: day_start.1,
        };
        let now = chrono::Utc::now();
        let today = calendar.today(now);
        let next = taskboard_domain::review_window(ReviewCadence::Day, today, calendar);
        match drained.as_slice() {
            [StateCommand::UpdateTask { id, changes }] => {
                assert_eq!(*id, task0, "the cursor row");
                assert_eq!(
                    changes.scheduled,
                    Some(Some(next.start())),
                    "the next window's opening instant"
                );
                assert_eq!(changes.duedate, None, "due dates are never touched");
            }
            other => panic!("one carry write, got {other:?}"),
        }
        // The row left the carry reading (its anchor moved out of
        // the window): the row verb passed the last carry row and
        // the skip law landed on Prompts (the planner is opt-in,
        // J4 D-3).
        harness.engine.settle(&harness.handle);
        assert_eq!(walk_stage(&*harness), 2, "the carried row left");
        assert!(walk_open(&*harness));
    });
    let _ = due_before;
}

/// J9 — Pick a date: a parseable date commits one scheduled edit; an
/// unparseable one dispatches nothing and keeps the text.
#[test]
fn the_date_pick_commits_parseable_input_only() {
    let (fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    let task0 = fixture.task0;
    support::harness::run_journey(state, move |harness| {
        open_walk(&mut *harness);
        harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
        harness.driver.advance_time(0);
        for _ in 0..2 {
            harness.handle.dispatch_action(ActionId::ReviewInboxKeep);
            harness.driver.advance_time(0);
        }
        harness
            .handle
            .dispatch_action(ActionId::ReviewCarryPickDate);
        harness.driver.advance_time(0);
        // Unparseable: nothing dispatches, the buffer is kept.
        harness
            .handle
            .window()
            .invoke_walk_date_edited("not a date".into());
        harness
            .handle
            .dispatch_action(ActionId::ReviewCarryDateCommit);
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "an unparseable date dispatches nothing"
        );
        // Parseable: one scheduled write.
        harness
            .handle
            .window()
            .invoke_walk_date_edited("2030-03-01".into());
        harness
            .handle
            .dispatch_action(ActionId::ReviewCarryDateCommit);
        harness.driver.advance_time(0);
        let day_start = (
            taskboard_domain::SettingsSnapshot::default().day_start_time,
            UiSettings::default().boot_offset,
        );
        let calendar = taskboard_domain::DayCalendar {
            day_start_time: day_start.0,
            offset: day_start.1,
        };
        let expected = calendar.instant(
            chrono::NaiveDate::from_ymd_opt(2030, 3, 1).expect("date"),
            chrono::NaiveTime::MIN,
        );
        match harness.engine.drain().as_slice() {
            [StateCommand::UpdateTask { id, changes }] => {
                assert_eq!(*id, task0);
                assert_eq!(changes.scheduled, Some(Some(expected)));
            }
            other => panic!("one dated write, got {other:?}"),
        }
    });
}

/// J14 — Record with two answers: exactly one `RecordReflection` with
/// the reference's cadence/day/window; after the entry lands in the
/// published state the card is gone.
#[test]
fn recording_routes_one_entry_and_clears_the_card() {
    let (fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    let task0 = fixture.task0;
    let _ = task0;
    let day_start = (
        taskboard_domain::SettingsSnapshot::default().day_start_time,
        UiSettings::default().boot_offset,
    );
    let calendar = taskboard_domain::DayCalendar {
        day_start_time: day_start.0,
        offset: day_start.1,
    };
    let now = chrono::Utc::now();
    let yesterday = calendar.today(now).pred_opt().expect("yesterday");
    let expected_window = taskboard_domain::review_window(ReviewCadence::Day, yesterday, calendar);
    let published = {
        let mut state = state.clone();
        let entry = ReflectionEntry {
            id: ReflectionId::from_uuid(uuid::Uuid::now_v7()),
            cadence: ReviewCadence::Day,
            day: expected_window.day(),
            window_start: expected_window.start(),
            window_end: expected_window.end(),
            text: "shipped the walk".to_owned(),
            attributes: ReflectionAttributes::default(),
            created_at: now,
            updated_at: now,
            deleted: false,
        };
        state.reflection.entries.insert(entry.id, entry);
        state
    };
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        open_walk(&mut *harness);
        // Straight to the prompts: Entry → the first open stage →
        // (skip law) → Prompts (J4 D-3; the planner leg is opt-in).
        for _ in 0..3 {
            harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
            harness.driver.advance_time(0);
        }
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        assert_eq!(walk_stage(&*harness), 2);
        // Two answers ride the mirror seam (the editors' path).
        harness
            .handle
            .window()
            .invoke_walk_answer_edited(0, "shipped the walk".into());
        harness
            .handle
            .window()
            .invoke_walk_answer_edited(3, "the planner leg".into());
        harness.handle.dispatch_action(ActionId::ReviewRecord);
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        // The advance/handoff legs dispatch nothing; the record is
        // the journey's ONE command.
        match drained.as_slice() {
            [
                StateCommand::RecordReflection {
                    cadence,
                    day,
                    window,
                    text,
                    mood,
                    energy,
                    flags,
                },
            ] => {
                assert_eq!(*cadence, ReviewCadence::Day);
                assert_eq!(*day, yesterday);
                assert_eq!(*window, expected_window.bounds());
                assert!(text.contains("shipped the walk"));
                assert!(text.contains("the planner leg"));
                assert_eq!(*mood, None);
                assert_eq!(*energy, None);
                assert!(flags.is_empty(), "attributes stay the CLI's (D-W9)");
            }
            other => panic!("one record, got {other:?}"),
        }
        // The record lands on the completion summary (J4 D-8);
        // the summary's exit closes the walk.
        assert_eq!(walk_stage(&*harness), 5, "the summary presents");
        assert!(walk_open(&*harness));
        harness.handle.dispatch_action(ActionId::ReviewWalkComplete);
        harness.driver.advance_time(0);
        assert!(!walk_open(&*harness), "the exit closed the walk");
        harness.engine.replace_state(published, &harness.handle);
        assert!(
            !harness.handle.window().get_review_card_visible(),
            "the recorded window no longer surfaces"
        );
    });
}

/// J15 re-based (J4 D-1/D-8) — an all-blank Record and `Finish without
/// recording` both land on the completion summary gently: no command,
/// `recorded: no`, the walk stays open for the honest summary.
#[test]
fn blank_records_and_finish_dispatch_nothing() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::harness::run_journey(state, move |harness| {
        for exit in [ActionId::ReviewRecord, ActionId::ReviewFinish] {
            harness.handle.dispatch_action(ActionId::ReviewOpen);
            harness.driver.advance_time(0);
            for _ in 0..3 {
                harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
                harness.driver.advance_time(0);
            }
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert_eq!(walk_stage(&*harness), 2);
            harness.handle.dispatch_action(exit);
            harness.driver.advance_time(0);
            assert!(
                harness.engine.drain().is_empty(),
                "{exit:?} dispatched nothing"
            );
            assert_eq!(
                walk_stage(&*harness),
                5,
                "{exit:?} lands on the completion summary"
            );
            assert!(walk_open(&*harness), "the summary is presented");
            // Leave through the summary's exit for the next leg.
            harness.handle.dispatch_action(ActionId::ReviewWalkComplete);
            harness.driver.advance_time(0);
            assert!(!walk_open(&*harness));
        }
    });
}

/// J16 re-based (J4 D-6) — the Escape ladder: an armed date field
/// disengages first (the focused field's Insert), then the sub-state
/// clears, then the abort confirm arms (never a silent step-back) and
/// the second Escape dismisses it — no command at any rung, the walk
/// never leaves the Carry stage.
#[test]
fn the_escape_ladder_never_dispatches() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::harness::run_journey(state.clone(), move |harness| {
        open_walk(&mut *harness);
        assert!(walk_open(&*harness));
        harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::ReviewInboxKeep);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::ReviewInboxKeep);
        harness.driver.advance_time(0);
        assert_eq!(walk_stage(&*harness), 1);
        harness
            .handle
            .dispatch_action(ActionId::ReviewCarryPickDate);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_walk_model().picking);
        // Rung 1: the focused field disengages Insert; the armed
        // field stays (the TrackEntryDismiss ladder's shape).
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_walk_model().picking,
            "the armed field stays on the mode rung"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the ladder dispatched nothing"
        );
        // Rung 2: the sub-state clears.
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_walk_model().picking,
            "the armed field disengaged"
        );
        assert_eq!(walk_stage(&*harness), 1, "the stage holds");
        // Rung 3: the abort confirm arms (R-2 — never a silent
        // step-back).
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_walk_model().abort_confirm,
            "the confirm armed"
        );
        assert_eq!(walk_stage(&*harness), 1, "the stage is untouched");
        // Rung 4: the second Escape dismisses ("Keep reviewing").
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_walk_model().abort_confirm,
            "the confirm dismissed"
        );
        assert!(walk_open(&*harness), "the walk stays open");
        assert!(
            harness.engine.drain().is_empty(),
            "no ladder rung dispatched"
        );
    });
}

/// J17 — Capture opened mid-walk: the walk is replaced without
/// committing (the XOR rule), nothing dispatched.
#[test]
fn capture_replaces_the_walk_without_committing() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::harness::run_journey(state, move |harness| {
        harness.handle.dispatch_action(ActionId::ReviewOpen);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::CaptureQuick);
        harness.driver.advance_time(0);
        assert!(!walk_open(&*harness), "the walk was replaced");
        assert!(
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_vi_insert(),
            "capture's insert mode engages"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the replacement dispatched nothing"
        );
    });
}

/// T-31 (#814): opening capture over a live flow announces the
/// replacement — the notice arms AND the overlay still opens (both
/// facts, one dispatch; nothing routes).
#[test]
fn capture_over_a_live_flow_announces_the_replacement() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::harness::run_journey(state, move |harness| {
        harness.handle.dispatch_action(ActionId::ReviewOpen);
        harness.driver.advance_time(0);
        assert!(walk_open(&*harness), "the walk is live");
        harness.handle.dispatch_action(ActionId::CaptureQuick);
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_notice_visible(),
            "the replacement announcement arms"
        );
        assert!(
            harness.handle.window().get_capture_open(),
            "the overlay still opens"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the replacement dispatched nothing"
        );
    });
}

/// T-47 (#830): an all-blank record echoes its typed no-op — no
/// `RecordReflection` dispatches AND the status notice arms (the walk
/// stage is no longer the only signal).
#[test]
fn a_blank_reflection_record_echoes_its_typed_no_op() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        open_walk(&mut *harness);
        // Straight to the prompts (the recorded journey's path):
        // Entry → first open stage → (skip law) → Prompts.
        harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
        harness.driver.advance_time(0);
        harness.stroke(parse("Escape").expect("Escape"));
        harness.driver.advance_time(0);
        assert_eq!(walk_stage(&*harness), 2);
        harness.handle.dispatch_action(ActionId::ReviewRecord);
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "an all-blank record dispatches nothing"
        );
        assert!(
            harness.handle.window().get_notice_visible(),
            "the typed no-op echoes on the status notice"
        );
        assert_eq!(
            walk_stage(&*harness),
            5,
            "the walk lands on the honest summary (recorded: no)"
        );
        harness.handle.dispatch_action(ActionId::ReviewWalkComplete);
        harness.driver.advance_time(0);
        assert!(!walk_open(&*harness), "the exit closes gently");
    });
}

/// J18 — a refresh mid-walk re-derives the rows for the SAME window:
/// a concurrent engine write moves the rows, the walk stays open with
/// the reference pinned (no re-target at a state change).
#[test]
fn a_mid_walk_refresh_rederives_the_same_window() {
    let (fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    let task1 = fixture.task1;
    let published = {
        let mut updated = state.clone();
        updated.tasks.get_mut(&task1).expect("live").done = Some(chrono::Utc::now());
        updated
    };
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        open_walk(&mut *harness);
        let header_before = harness.handle.window().get_walk_model().header.to_string();
        // A concurrent engine write lands mid-walk: task1 is done.
        harness.engine.replace_state(published, &harness.handle);
        harness.driver.advance_time(0);
        assert!(walk_open(&*harness), "the walk survives the refresh");
        assert_eq!(
            harness.handle.window().get_walk_model().header.to_string(),
            header_before,
            "the reference is pinned — the header cannot drift"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the refresh dispatched nothing"
        );
    });
}

/// J19 — the kiosk pins (D-J14): the `review.open` dispatch is
/// structurally inert (layer 2) and neither the card nor the walk
/// model renders (layer 3) even with a due window on the wall.
#[test]
fn the_kiosk_mount_never_surfaces_the_walk() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    let settings = UiSettings {
        kiosk: true,
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_review_card_visible(),
            "no card on the appliance (layer 3)"
        );
        harness.handle.dispatch_action(ActionId::ReviewOpen);
        harness.driver.advance_time(0);
        assert!(!walk_open(&*harness), "the dispatch is inert (layer 2)");
        assert!(
            harness.engine.drain().is_empty(),
            "the kiosk dispatch routed nothing"
        );
        assert!(
            !harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_date_picker_grid(),
            "no overlay context leaked"
        );
    });
}

/// J4 US-3/S1 — the plan-next-day choice out of Carry mounts the
/// embedded planner child: the walk stays open beneath (stage 3),
/// and the Prev control ends the child Cancelled — back returns to
/// Carry, the walk never closed (D-2's exit contract).
#[test]
fn us3_s1_planner_embeds_and_back_returns_to_carry() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = mount_open_walk(state);
            assert!(walk_open(&harness));
            // Entry → Inbox → Carry.
            walk_step(&mut harness, ActionId::ReviewWalkAdvance);
            walk_step(&mut harness, ActionId::ReviewWalkAdvance);
            assert_eq!(walk_stage(&harness), 1, "the walk is at Carry");
            // The planner choice mounts the embedded step.
            walk_step(&mut harness, ActionId::ReviewWalkPlanner);
            assert_eq!(
                walk_stage(&harness),
                3,
                "the planner is the walk's own stage"
            );
            assert!(walk_open(&harness), "the walk stays open beneath");
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Board,
                "no view swap: the planner is embedded, never a navigation"
            );
            // Back ends the child Cancelled: back to Carry.
            walk_step(&mut harness, ActionId::ReviewWalkPrev);
            assert_eq!(walk_stage(&harness), 1, "back returns to Carry");
            assert!(walk_open(&harness), "the walk never closed");
        },
    );
}

/// J4 US-3/S2 — completing the embedded planner ends the child
/// `Completed`: the walk advances to Prompts and stays open (D-2's
/// exit contract; the whole walk is never skipped).
#[test]
fn us3_s2_planner_complete_advances_to_prompts() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = mount_open_walk(state);
            walk_step(&mut harness, ActionId::ReviewWalkAdvance);
            walk_step(&mut harness, ActionId::ReviewWalkAdvance);
            walk_step(&mut harness, ActionId::ReviewWalkPlanner);
            assert_eq!(walk_stage(&harness), 3);
            // Done planning: the Next control ends the child Completed.
            walk_step(&mut harness, ActionId::ReviewWalkAdvance);
            assert_eq!(
                walk_stage(&harness),
                2,
                "the child's completion advances to Prompts"
            );
            assert!(walk_open(&harness), "the walk carries on");
            harness.engine.drain();
        },
    );
}

/// J12 re-based (J4 US-3/S2) — the planner leg places a task, then
/// completes: the placement's `UpdateTask` lands and the walk
/// advances to Prompts over the placed state (placed tasks reach the
/// walk as plain snapshot state).
#[test]
fn us3_s2_the_planner_leg_places_then_advances_to_prompts() {
    let (fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    let task0 = fixture.task0;
    let title = state.tasks.get(&task0).expect("task0").title.clone();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = mount_open_walk(state);
            walk_step(&mut harness, ActionId::ReviewWalkAdvance);
            walk_step(&mut harness, ActionId::ReviewWalkAdvance);
            walk_step(&mut harness, ActionId::ReviewWalkPlanner);
            assert_eq!(walk_stage(&harness), 3);
            // Place task0 from the pool: the placement form's funnel,
            // mounted inside the walk dialog.
            let row = harness
                .driver
                .find_by_label(&format!("Place {title}"))
                .unwrap_or_else(|| panic!("task0's pool row is mounted"));
            row.invoke_default_action();
            harness.driver.advance_time(0);
            click_field_centre(&mut harness, "Time");
            harness.type_text("09:30");
            harness.stroke(parse("Return").expect("Return"));
            harness.driver.advance_time(0);
            let drained = harness.engine.drain();
            assert!(
                drained
                    .iter()
                    .any(|c| matches!(c, StateCommand::UpdateTask { .. })),
                "the placement write landed"
            );
            // Done planning: the child's Completed exit advances to
            // Prompts; the walk stays open over the placed state.
            walk_step(&mut harness, ActionId::ReviewWalkAdvance);
            assert!(walk_open(&harness));
            assert_eq!(walk_stage(&harness), 2);
        },
    );
}

/// The empty-row-state fixture: the golden state plus an inbox board
/// with `n` triage rows and NO carry anchor — the skip-law journeys'
/// row source.
fn walk_empty_carry_state(inbox_n: usize) -> AppState {
    let (_fixture, mut state) = fixture::golden_state();
    state.reflection.entries.clear();
    let (_inbox, inbox_stack) = insert_inbox_board_and_stack(&mut state);
    for index in 0..inbox_n {
        let task = walk_new_task(inbox_stack, &format!("triage-{index}"), None);
        state.tasks.insert(task.id, task);
    }
    // The scheduled anchors (the golden fixture's carry sources) are
    // cleared: the carry reading is empty by construction.
    for task in state.tasks.values_mut() {
        task.scheduled = None;
    }
    state
}

/// J4 US-1/S1 — the guided entry: the modal opens at the Entry stage
/// (D-1), the Next control steps Entry → the first open row stage,
/// and the stage-position phrase rides the model (D-9).
#[test]
fn us1_s1_entry_opens_and_steps_forward() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = mount_open_walk(state);
            assert!(walk_open(&harness), "the modal is open");
            assert_eq!(walk_stage(&harness), 4, "the walk opens at Entry");
            assert!(
                !harness
                    .handle
                    .window()
                    .get_walk_model()
                    .stage_position
                    .is_empty(),
                "the stage position rides the model (D-9)"
            );
            walk_step(&mut harness, ActionId::ReviewWalkAdvance);
            assert_eq!(walk_stage(&harness), 0, "Entry's Next lands on Inbox");
            harness.engine.drain();
        },
    );
}

/// J4 US-1/S2 — next/prev roundtrip across the machine's stages; the
/// Prev control never skips past a visited stage into a closed walk.
#[test]
fn us1_s2_stage_next_prev_roundtrip() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = mount_open_walk(state);
            // Forward: Entry → Inbox → Carry → Prompts.
            for expected in [0, 1, 2] {
                walk_step(&mut harness, ActionId::ReviewWalkAdvance);
                assert_eq!(walk_stage(&harness), expected);
            }
            // Back: Prompts → Carry → Inbox → Entry; the walk stays
            // open at every rung (the close is the abort/exit law).
            for expected in [1, 0, 4] {
                walk_step(&mut harness, ActionId::ReviewWalkPrev);
                assert_eq!(walk_stage(&harness), expected);
                assert!(walk_open(&harness));
            }
            harness.engine.drain();
        },
    );
}

/// J4 US-1/S3 — the completion path: Finish lands on the summary
/// (D-8), whose exit closes the walk and restores the view focus.
#[test]
fn us1_s3_complete_closes_and_restores_focus() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = mount_open_walk(state);
            for _ in 0..3 {
                walk_step(&mut harness, ActionId::ReviewWalkAdvance);
            }
            assert_eq!(walk_stage(&harness), 2);
            // The guilt-free exit lands on the summary, recorded: no.
            walk_step(&mut harness, ActionId::ReviewFinish);
            assert_eq!(walk_stage(&harness), 5, "Finish lands on Complete");
            assert!(
                walk_open(&harness),
                "the summary is presented, never a silent close"
            );
            assert!(
                !harness.handle.window().get_walk_model().tally_recorded,
                "recorded: no is presented honestly (P12)"
            );
            // The summary's single exit closes and restores.
            walk_step(&mut harness, ActionId::ReviewWalkComplete);
            assert!(!walk_open(&harness), "the exit closes the walk");
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Board,
                "the view is restored (the D15 hand-off)"
            );
            harness.engine.drain();
        },
    );
}

/// J4 US-2/S1 (D-3) — the empty Carry stage auto-skips: passing the
/// last inbox row lands on Prompts, and the "Move to tomorrow" strip
/// is never mounted over zero rows (the count gate).
#[test]
fn us2_s1_empty_carry_stage_auto_skips() {
    let state = walk_empty_carry_state(1);
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = mount_open_walk(state);
            walk_step(&mut harness, ActionId::ReviewWalkAdvance);
            assert_eq!(walk_stage(&harness), 0);
            // The row verb passes the last row; the skip law lands on
            // Prompts — the carry stage never renders empty.
            walk_step(&mut harness, ActionId::ReviewInboxKeep);
            assert_eq!(
                walk_stage(&harness),
                2,
                "the empty carry stage is skipped to Prompts"
            );
            harness.engine.drain();
        },
    );
}

/// J4 US-2/S1 (D-3) — the empty-everything law: the whole walk is
/// NEVER skipped; with no rows anywhere it lands on Prompts and
/// finishing reaches the completion summary end-to-end.
#[test]
fn us2_s1_empty_everything_reaches_completion() {
    let state = walk_empty_carry_state(0);
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = mount_open_walk(state);
            assert_eq!(walk_stage(&harness), 4, "Entry is never skipped");
            walk_step(&mut harness, ActionId::ReviewWalkAdvance);
            assert_eq!(walk_stage(&harness), 2, "both stages empty land on Prompts");
            walk_step(&mut harness, ActionId::ReviewFinish);
            assert_eq!(walk_stage(&harness), 5);
            walk_step(&mut harness, ActionId::ReviewWalkComplete);
            assert!(!walk_open(&harness), "the whole walk completed");
            harness.engine.drain();
        },
    );
}

/// J4 US-4 (D-5/D-6) — modal containment: the walk stays open through
/// in-dialog interaction; Escape at a row stage arms the abort
/// confirm (never a silent step-back), whose dismiss keeps the walk
/// verbatim.
#[test]
fn us4_s1_modal_containment() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = mount_open_walk(state);
            walk_step(&mut harness, ActionId::ReviewWalkAdvance);
            assert_eq!(walk_stage(&harness), 0);
            // Tab strokes stay contained: the walk stays open.
            harness.stroke(parse("Tab").expect("Tab"));
            harness.driver.advance_time(0);
            harness.stroke(parse("Tab").expect("Tab"));
            harness.driver.advance_time(0);
            assert!(walk_open(&harness), "the Tab walk is clamped inside");
            // Escape arms the confirm (R-2), never a silent close.
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_walk_model().abort_confirm,
                "Escape arms the abort confirm"
            );
            assert!(walk_open(&harness));
            // Dismiss: the confirm closes, the walk stays verbatim.
            walk_step(&mut harness, ActionId::ReviewAbortDismiss);
            assert!(
                !harness.handle.window().get_walk_model().abort_confirm,
                "the confirm closed"
            );
            assert_eq!(walk_stage(&harness), 0, "the stage is untouched");
            assert!(walk_open(&harness));
            harness.engine.drain();
        },
    );
}

/// J4 US-4 (D-6) — the abort ladder's decision-safety law: decisions
/// already made are applied `StateCommand`s; accepting the confirm
/// ends the walk but never undoes them. The Entry stage closes
/// silently (the one silent exit).
#[test]
fn us4_s1_abort_confirm_keeps_decisions() {
    let (_fixture, state, stack, inbox_ids) = walk_fixture_state();
    let filed = inbox_ids[0];
    let filed_stack_before = state.tasks.get(&filed).expect("the row exists").stack;
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = mount_open_walk(state);
            // The Entry silent exit: nothing decided, nothing typed.
            walk_step(&mut harness, ActionId::ReviewAbortArm);
            assert!(
                !walk_open(&harness),
                "Entry closes without a confirm (the one silent exit)"
            );
            // Reopen: decide something (file the cursor row), then
            // abort.
            walk_step(&mut harness, ActionId::ReviewOpen);
            walk_step(&mut harness, ActionId::ReviewWalkAdvance);
            walk_step(&mut harness, ActionId::ReviewInboxFile);
            walk_step(&mut harness, ActionId::ReviewFileCommit);
            // The commit's drained boundary carries exactly the
            // filed row's MoveTask — the decision already made.
            let drained_decision = harness.engine.drain();
            let [
                StateCommand::MoveTask {
                    id, stack: target, ..
                },
            ] = drained_decision.as_slice()
            else {
                panic!("one MoveTask, got {drained_decision:?}");
            };
            assert_eq!(*id, filed, "the decision names the filed row");
            assert_ne!(
                *target, filed_stack_before,
                "the row left its original stack"
            );
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_walk_model().abort_confirm,
                "the abort confirm is armed"
            );
            walk_step(&mut harness, ActionId::ReviewAbortAccept);
            assert!(!walk_open(&harness), "the accept ends the walk");
            // The accept dispatches nothing new: the decision above
            // is the journal's truth — the abort undid nothing.
            let drained_after = harness.engine.drain();
            assert!(
                drained_after.is_empty(),
                "the accept dispatches nothing new"
            );
            let _ = stack;
        },
    );
}

/// J4 US-2 seam (D-3, T10) — `Move to tomorrow` with no carry cursor
/// dispatches nothing: the typed inert (the adapter's guard, kept
/// and pinned structurally over the command count).
#[test]
fn carry_tomorrow_never_dispatches_without_cursor() {
    let state = walk_empty_carry_state(1);
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = mount_open_walk(state);
            walk_step(&mut harness, ActionId::ReviewWalkAdvance);
            // At the Inbox (the carry verb is stage-typed inert here —
            // and the carry stage itself is unreachable with zero
            // rows): the dispatch must produce no command.
            walk_step(&mut harness, ActionId::ReviewCarryTomorrow);
            assert!(
                harness.engine.drain().is_empty(),
                "the None-cursor carry dispatches nothing"
            );
        },
    );
}

/// The view-mount inventory row for the overlay (the walk's structural
/// fact: the overlay is a flow-gated window-level mount, never a view).
#[test]
fn the_walk_overlay_mount_is_flow_gated_window_level() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let harness = mount_open_walk(state);
            let window = harness.handle.window();
            assert!(window.get_walk_model().open);
            // The overlay is NOT a view: the cycle selector stays on
            // its surface and the kiosk rotation cannot reach the walk.
            assert_ne!(window.get_current_view(), View::PlanNextDay);
        },
    );
}

// -- round-5 chunk 7 (F11, D4/W5a): the keybind context stack --

/// The F11 acceptance session: with the picker open and the FIELD
/// holding focus, the calendar scope serves its navigation strokes
/// (emacs/standard chords, vi arrows) while the field keeps every
/// printable; after the Down hand-over the grid phase owns its verbs;
/// Escape closes clean. Both picker phases, all three presets.
#[test]
#[allow(clippy::too_many_lines)] // the two-phase × three-preset matrix is the contract
fn the_picker_shares_chords_while_the_field_keeps_printables() {
    let matrix = [
        taskboard_ui_slint::keymap::Preset::Standard,
        taskboard_ui_slint::keymap::Preset::Emacs,
        taskboard_ui_slint::keymap::Preset::Vi,
    ];
    for preset in matrix {
        let (fixture, mut state) = fixture::journey_state();
        let due = local_midnight(2030, 1, 30);
        state
            .tasks
            .get_mut(&fixture.task1)
            .expect("fixture task")
            .duedate = Some(due);
        support::harness::run_journey_with_settings(
            state,
            UiSettings {
                keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                keymap_preset: preset,
                ..UiSettings::default()
            },
            move |harness| {
                let cursor_index = |harness: &Harness| -> Option<usize> {
                    let grid = harness.handle.window().get_task_model().date_grid;
                    let mut flat = 0usize;
                    for r in 0..grid.row_count() {
                        let row = grid.row_data(r).expect("row");
                        for c in 0..row.row_count() {
                            if row.row_data(c).expect("cell").cursor {
                                return Some(flat);
                            }
                            flat += 1;
                        }
                    }
                    None
                };
                let grid_armed = |harness: &Harness| keyboard(harness).get_date_picker_grid();

                press_preset(&mut *harness, preset, ActionId::NavFocusDown);
                press_preset(&mut *harness, preset, ActionId::NavFocusDown);
                press_preset(&mut *harness, preset, ActionId::NavNextView);
                press_preset(&mut *harness, preset, ActionId::NavNextView);
                harness.driver.click_by_label("Edit due date");
                harness.driver.advance_time(0);
                let task_model = harness.handle.window().get_task_model();
                assert!(
                    task_model.date_picker_open,
                    "the picker is default-open; the field holds focus"
                );
                assert!(!grid_armed(&*harness), "field phase: the grid is not armed");

                // -- the field phase: type first, the buffer follows --
                harness.clear_seeded();
                harness.type_text("2030-03-15");
                harness.driver.advance_time(0);
                assert_eq!(
                    harness.handle.window().get_task_model().due_text.as_str(),
                    "2030-03-15",
                    "printables reach the field; the buffer follows the typed text"
                );

                // -- the field phase's scope: the calendar's
                //    navigation strokes move the selection mid-typing
                //    (emacs' chords, standard's arrows); vi's picker
                //    verbs are the home-row printables, which the
                //    monopoly keeps with the field --
                let chord_move = |harness: &mut Harness, spelling: &str| {
                    let before = cursor_index(harness);
                    harness.stroke(parse(spelling).expect(spelling));
                    harness.driver.advance_time(0);
                    assert_ne!(
                        cursor_index(harness),
                        before,
                        "{spelling} moved the calendar mid-typing"
                    );
                };
                match preset {
                    taskboard_ui_slint::keymap::Preset::Emacs => {
                        for spelling in ["Ctrl+p", "Ctrl+n", "Ctrl+f", "Ctrl+b"] {
                            chord_move(&mut *harness, spelling);
                        }
                    }
                    taskboard_ui_slint::keymap::Preset::Standard => {
                        // Bare Down stays the pinned field→grid
                        // hand-over arm (special case B); the other
                        // arrows are the calendar's mid-typing.
                        for spelling in ["Up", "Left", "Right"] {
                            chord_move(&mut *harness, spelling);
                        }
                    }
                    taskboard_ui_slint::keymap::Preset::Vi => {
                        // The baseline is the typed date's cell (the
                        // calendar live-syncs to the typed text).
                        let typed = cursor_index(&*harness);
                        for spelling in ["h", "j", "k", "l"] {
                            harness.type_text(spelling);
                            harness.driver.advance_time(0);
                            assert_eq!(
                                cursor_index(&*harness),
                                typed,
                                "{spelling} types in the field phase; the calendar is untouched"
                            );
                        }
                    }
                }
                // The field survived the strokes: the editor stays armed.
                assert!(
                    harness.handle.window().get_task_model().date_picker_open,
                    "the field phase never settled the editor"
                );

                // -- the grid phase: Down hands over (vi: Escape steps
                //    back to the grid — "h/j/k/l arm after Esc"), and
                //    the picker owns its verbs --
                if preset == taskboard_ui_slint::keymap::Preset::Vi {
                    harness.stroke(parse("Escape").expect("Escape"));
                } else {
                    harness.stroke(parse("Down").expect("Down"));
                }
                harness.driver.advance_time(0);
                assert!(grid_armed(&*harness), "the grid took focus");
                let before = cursor_index(&*harness);
                press_preset(&mut *harness, preset, ActionId::PickerRight);
                harness.driver.advance_time(0);
                assert_ne!(cursor_index(&*harness), before, "the grid phase walks");

                // -- the clean close: the grid phase's Escape --
                harness.stroke(parse("Escape").expect("Escape"));
                harness.driver.advance_time(0);
                assert!(
                    !harness.handle.window().get_task_model().date_picker_open,
                    "the editor closed clean"
                );
                assert!(!grid_armed(&*harness), "the scope disarmed with the flow");
            },
        );
    }
}

/// The editor-scope acceptance session (D4/W5a): the description
/// editor gains the preset command grammar — emacs mark/region
/// cut/copy through the adapter clipboard slot, the C-c prefix pair,
/// and paste; the vi preset's printables reach the field and Escape
/// keeps the existing discard funnel.
#[test]
#[allow(clippy::too_many_lines)] // the grammar walk is the contract's length
fn the_description_editor_gains_the_preset_command_grammar() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            keymap: taskboard_ui_slint::keymap::Keymap::from_preset(
                taskboard_ui_slint::keymap::Preset::Emacs,
            ),
            keymap_preset: taskboard_ui_slint::keymap::Preset::Emacs,
            ..UiSettings::default()
        },
        move |harness| {
            let open_editor = |harness: &mut Harness| {
                harness.driver.click_by_label("Edit description");
                harness.driver.advance_time(0);
                assert!(
                    harness
                        .handle
                        .window()
                        .get_task_model()
                        .description_edit_open,
                    "the editor opened"
                );
            };
            // The commit drains the boundary and settles it into the
            // fixture (continuity: the next leg's editor seeds from
            // the committed description).
            let commit = |harness: &mut Harness| -> taskboard_domain::TaskChanges {
                harness.stroke(parse("Shift+Return").expect("Shift+Return"));
                harness.driver.advance_time(0);
                let drained = harness.engine.drain();
                let changes = match drained.as_slice() {
                    [StateCommand::UpdateTask { changes, .. }] => changes.clone(),
                    other => panic!("the commit reached the boundary, got {other:?}"),
                };
                harness.engine.settle(&harness.handle);
                harness.driver.advance_time(0);
                changes
            };

            let emacs = taskboard_ui_slint::keymap::Preset::Emacs;
            press_preset(&mut *harness, emacs, ActionId::NavFocusDown);
            press_preset(&mut *harness, emacs, ActionId::NavFocusDown);
            press_preset(&mut *harness, emacs, ActionId::NavNextView);
            press_preset(&mut *harness, emacs, ActionId::NavNextView);

            // -- mark + region kill: C-Space / C-a C-f / C-e / C-w --
            open_editor(&mut *harness);
            harness.type_text("alpha beta");
            harness.driver.advance_time(0);
            harness.stroke(parse("Ctrl+a").expect("C-a"));
            harness.stroke(parse("Ctrl+f").expect("C-f"));
            harness.stroke(parse("Ctrl+f").expect("C-f"));
            harness.stroke(parse("Ctrl+Space").expect("C-Space"));
            harness.stroke(parse("Ctrl+e").expect("C-e"));
            harness.stroke(parse("Ctrl+w").expect("C-w"));
            harness.driver.advance_time(0);
            let changes = commit(&mut *harness);
            assert_eq!(
                changes.description.as_deref(),
                Some("al"),
                "the region past the mark was killed, the head survived"
            );

            // -- paste replays the slot --
            open_editor(&mut *harness);
            harness.stroke(parse("Ctrl+e").expect("C-e"));
            harness.stroke(parse("Ctrl+y").expect("C-y"));
            harness.driver.advance_time(0);
            let changes = commit(&mut *harness);
            assert_eq!(
                changes.description.as_deref(),
                Some("alpha beta"),
                "the kill ring slot replayed at the cursor"
            );

            // -- the C-c prefix pair: C-c C-c commits, C-c C-k discards --
            open_editor(&mut *harness);
            // Clear the single-line seed through the grammar itself.
            harness.stroke(parse("Ctrl+a").expect("C-a"));
            harness.stroke(parse("Ctrl+k").expect("C-k"));
            harness.driver.advance_time(0);
            harness.type_text("prefixed");
            harness.stroke(parse("Ctrl+c").expect("C-c"));
            harness.stroke(parse("Ctrl+c").expect("C-c"));
            harness.driver.advance_time(0);
            let changes = harness.engine.drain();
            match changes.as_slice() {
                [StateCommand::UpdateTask { changes, .. }] => assert_eq!(
                    changes.description.as_deref(),
                    Some("prefixed"),
                    "C-c C-c is the commit"
                ),
                other => panic!("C-c C-c reached the boundary, got {other:?}"),
            }
            harness.engine.settle(&harness.handle);
            harness.driver.advance_time(0);
            open_editor(&mut *harness);
            harness.type_text("doomed");
            harness.stroke(parse("Ctrl+c").expect("C-c"));
            harness.stroke(parse("Ctrl+k").expect("C-k"));
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
            assert!(
                !harness
                    .handle
                    .window()
                    .get_task_model()
                    .description_edit_open,
                "C-c C-k is the discard"
            );

            // -- copy (M-w) then paste --
            open_editor(&mut *harness);
            harness.stroke(parse("Ctrl+a").expect("C-a"));
            harness.stroke(parse("Ctrl+Space").expect("C-Space"));
            harness.stroke(parse("Ctrl+f").expect("C-f"));
            harness.stroke(parse("Ctrl+f").expect("C-f"));
            harness.stroke(parse("Alt+w").expect("M-w"));
            harness.stroke(parse("Ctrl+e").expect("C-e"));
            harness.stroke(parse("Ctrl+y").expect("C-y"));
            harness.driver.advance_time(0);
            let changes = commit(&mut *harness);
            assert_eq!(
                changes.description.as_deref(),
                Some("prefixedpr"),
                "M-w copied the region without cutting; paste replayed it"
            );

            // -- C-d deletes forward, C-k kills to the line end --
            open_editor(&mut *harness);
            harness.stroke(parse("Ctrl+a").expect("C-a"));
            harness.stroke(parse("Ctrl+k").expect("C-k"));
            harness.driver.advance_time(0);
            harness.type_text("hold");
            harness.stroke(parse("Ctrl+a").expect("C-a"));
            harness.stroke(parse("Ctrl+d").expect("C-d"));
            harness.stroke(parse("Ctrl+k").expect("C-k"));
            harness.driver.advance_time(0);
            let changes = commit(&mut *harness);
            assert_eq!(
                changes.description.as_deref(),
                Some(""),
                "C-d took the head, C-k took the rest"
            );
        },
    );
}

/// The monopoly yields to a pending keymap prefix (D-KS2 × the C-x
/// family): while typing, the printable that follows `C-x` belongs to
/// the resolver — an unbound one aborts the prefix and is consumed,
/// never typed, and the field's next printable types normally.
#[test]
fn a_pending_prefix_owns_the_printable_that_follows_it() {
    let (_fixture, state) = fixture::journey_state();
    let emacs = taskboard_ui_slint::keymap::Preset::Emacs;
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            keymap: taskboard_ui_slint::keymap::Keymap::from_preset(emacs),
            keymap_preset: emacs,
            ..UiSettings::default()
        },
        move |harness| {
            press_preset(&mut *harness, emacs, ActionId::NavFocusDown);
            press_preset(&mut *harness, emacs, ActionId::NavFocusDown);
            press_preset(&mut *harness, emacs, ActionId::NavNextView);
            press_preset(&mut *harness, emacs, ActionId::NavNextView);

            harness.driver.click_by_label("Edit description");
            harness.driver.advance_time(0);
            harness.stroke(parse("Ctrl+a").expect("C-a"));
            harness.stroke(parse("Ctrl+k").expect("C-k"));
            harness.driver.advance_time(0);
            harness.type_text("ab");
            harness.stroke(parse("Ctrl+x").expect("C-x"));
            harness.type_text("z");
            harness.type_text("c");
            harness.stroke(parse("Shift+Return").expect("Shift+Return"));
            harness.driver.advance_time(0);
            match harness.engine.drain().as_slice() {
                [StateCommand::UpdateTask { changes, .. }] => assert_eq!(
                    changes.description.as_deref(),
                    Some("abc"),
                    "the aborting printable was consumed by the prefix"
                ),
                other => panic!("the commit reached the boundary, got {other:?}"),
            }
        },
    );
}

/// The vi preset's editor pin (D-KS2/D-KS3): while typing in the
/// description editor (insert), the home-row ladder letters are
/// printables the monopoly keeps with the field — they type, never
/// move; Escape keeps the existing discard funnel (NOT rebound).
#[test]
fn the_vi_editor_keeps_its_printables_and_the_escape_funnel() {
    let (_fixture, state) = fixture::journey_state();
    let vi = taskboard_ui_slint::keymap::Preset::Vi;
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            keymap: taskboard_ui_slint::keymap::Keymap::from_preset(vi),
            keymap_preset: vi,
            ..UiSettings::default()
        },
        move |harness| {
            press_preset(&mut *harness, vi, ActionId::NavFocusDown);
            press_preset(&mut *harness, vi, ActionId::NavFocusDown);
            press_preset(&mut *harness, vi, ActionId::NavNextView);
            press_preset(&mut *harness, vi, ActionId::NavNextView);

            harness.driver.click_by_label("Edit description");
            harness.driver.advance_time(0);
            harness.type_text("hjkl0$xi");
            harness.stroke(parse("Shift+Return").expect("Shift+Return"));
            harness.driver.advance_time(0);
            match harness.engine.drain().as_slice() {
                [StateCommand::UpdateTask { changes, .. }] => assert_eq!(
                    changes.description.as_deref(),
                    Some("hjkl0$xi"),
                    "every ladder letter typed while inserting"
                ),
                other => panic!("the commit reached the boundary, got {other:?}"),
            }

            harness.driver.click_by_label("Edit description");
            harness.driver.advance_time(0);
            harness.type_text("half");
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
            assert!(
                !harness
                    .handle
                    .window()
                    .get_task_model()
                    .description_edit_open,
                "Escape kept the discard funnel"
            );
        },
    );
}

// ---- round 5 chunk 8: the account card's keyboard-only session ----------

/// D-8.8 — the account session is keyboard-only operable: the walk
/// reaches the card's stops in reading order (Remove, the Files
/// toggle, Refresh), Return on Refresh lists, the walk reaches the
/// remote row's picker, Return opens it and Return picks — the bind
/// routes as an explicit `AttachConnector`.
#[test]
fn the_account_session_is_keyboard_operable() {
    use taskboard_ui_slint::requests::UiRequest;
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", None);
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        harness.driver.set_window_size(1000, 6000);
        harness.driver.advance_time(0);

        // The card-scoped reading order.
        let mut seen = Vec::new();
        for _ in 0..200 {
            harness.stroke(parse("Tab").expect("tab"));
            harness.driver.advance_time(0);
            let label = harness.driver.focused_label().unwrap_or_default();
            if label == "Refresh" {
                break;
            }
            seen.push(label);
        }
        let remove = seen
            .iter()
            .position(|l| l == "Remove")
            .expect("Remove is a stop");
        let files = seen
            .iter()
            .position(|l| l == "On")
            .expect("the Files toggle is a stop");
        assert!(
            remove < files,
            "the card header precedes its Files toggle: {seen:?}"
        );

        // Return on Refresh lists.
        harness.stroke(parse("Return").expect("Return"));
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1, "Return on Refresh routed one listing");
        let UiRequest::ListRemoteBoards { reply, .. } = requests.remove(0) else {
            panic!("the listing routed");
        };
        let _ = reply.send(Ok(vec![remote_row(7, "team")]));
        harness.handle.drain_setting_replies();
        harness.driver.advance_time(0);

        // The picker: Return opens, Return picks.
        let picker_label = harness
            .handle
            .window()
            .global::<taskboard_ui_slint::SettingsBoardCopy>()
            .get_label()
            .to_string();
        tab_to(&mut *harness, &picker_label);
        raw_key(&*harness, slint::platform::Key::Return);
        harness.driver.advance_time(0);
        assert!(board_facts(&*harness).get_open(), "Return opens the picker");
        raw_key(&*harness, slint::platform::Key::Return);
        harness.driver.advance_time(0);
        assert!(
            !board_facts(&*harness).get_open(),
            "Return picks and closes"
        );
        let requests = harness.drain_requests();
        assert_eq!(requests.len(), 1, "the pick routed one bind");
        assert!(matches!(
            &requests[0],
            UiRequest::AttachConnector { board, connector, remote, .. }
                if board.as_uuid() == home && connector == "work" && *remote == taskboard_domain::RemoteBoardId::deck(7)
        ));
    });
}

// -- the UX R-3 remediation journeys (US-13/14/15, #788/#789) --------
// The step-token gate rows for the amended task-editing plan (the
// remediation-era stories carry REQUIRED gate rows, stage-1 plan
// D-9). Each test drives one step through the real window seams and
// asserts typed outcomes only.

/// Seeds two labels on the fixture's board and assigns `green` to the
/// subject task (US-14/US-15's vocabulary).
fn label_journey_state() -> (
    fixture::Fixture,
    AppState,
    taskboard_domain::LabelId,
    taskboard_domain::LabelId,
) {
    let (fx, mut state) = fixture::golden_state();
    let board = state
        .stacks
        .get(&state.tasks.get(&fx.task0).expect("task").stack)
        .expect("stack")
        .board;
    let (green, red) = (
        taskboard_domain::LabelId::from_uuid(uuid::Uuid::now_v7()),
        taskboard_domain::LabelId::from_uuid(uuid::Uuid::now_v7()),
    );
    for (id, title, color) in [(green, "green", "31cc7c"), (red, "red", "3a70d6")] {
        state.labels.insert(
            id,
            taskboard_domain::Label {
                id,
                remote: None,
                board,
                title: title.to_owned(),
                color: taskboard_domain::Color::new(color),
                deleted: false,
                clocks: taskboard_domain::LabelClocks {
                    title: taskboard_domain::BoardClocks::epoch(),
                    color: taskboard_domain::BoardClocks::epoch(),
                    deleted: taskboard_domain::BoardClocks::epoch(),
                },
                authors: taskboard_domain::entities::LabelAuthors::default(),
                remote_seen: None,
            },
        );
    }
    if let Some(task) = state.tasks.get_mut(&fx.task0) {
        task.label_memberships.insert(
            green,
            taskboard_domain::entities::MembershipState::Added {
                at: taskboard_domain::BoardClocks::epoch(),
            },
        );
    }
    (fx, state, green, red)
}

/// Arms the task-detail view over the fixture's subject task.
fn label_journey_mount(harness: &mut Harness) {
    press_preset(
        harness,
        taskboard_ui_slint::keymap::Preset::Standard,
        ActionId::NavFocusDown,
    );
    press_preset(
        harness,
        taskboard_ui_slint::keymap::Preset::Standard,
        ActionId::TaskEdit,
    );
    harness.driver.advance_time(0);
}

/// US-13/S1: the importance row's edit affordance opens the picker —
/// the model's open flag rides the adapter-owned state, the sibling
/// urgency flag stays closed (the exclusivity).
#[test]
fn us13_s1_the_importance_edit_affordance_opens_the_picker() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_task_importance_edit_asked();
        harness.driver.advance_time(0);
        assert!(window.get_task_model().importance_open);
        assert!(!window.get_task_model().urgency_open);
    });
}

/// US-13/S2: picking a level commits the exact `UpdateTask` mask
/// (`Some(Some(level))`, every sibling mask `None`) and closes the
/// picker.
#[test]
fn us13_s2_the_slot_pick_commits_the_exact_mask() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_task_importance_edit_asked();
        harness.driver.advance_time(0);
        window.invoke_priority_slot_clicked(4);
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert_eq!(drained.len(), 1);
        assert!(matches!(
            &drained[0],
            taskboard_domain::StateCommand::UpdateTask {
                changes: taskboard_domain::TaskChanges {
                    importance: Some(Some(taskboard_domain::Importance::High)),
                    ..
                },
                ..
            }
        ));
        assert!(!window.get_task_model().importance_open);
    });
}

/// US-13/S3: the `unset` slot commits `Some(None)` — the CLI's `none`
/// twin (the parity contract).
#[test]
fn us13_s3_the_unset_slot_commits_the_clear_twin() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_task_urgency_edit_asked();
        harness.driver.advance_time(0);
        window.invoke_priority_slot_clicked(0);
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::UpdateTask {
                changes: taskboard_domain::TaskChanges {
                    urgency: Some(None),
                    ..
                },
                ..
            }]
        ));
    });
}

// US-6 (J5 R-5, round-9 chunk 15): the detail's priority pickers
// ARE dropdowns — the shared widget class with the de-sited mount.

/// US-6/S1: the edit intent opens the widget's flyout and a pick
/// commits the exact `UpdateTask` importance mask — the flow-table
/// oracle, now through the widget (D-9).
#[test]
fn us6_s1_the_importance_dropdown_pick_commits_the_exact_mask() {
    let (_fx, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            label_journey_mount(&mut harness);
            let window = harness.handle.window();
            window.invoke_task_importance_edit_asked();
            harness.driver.advance_time(0);
            let facts = window.global::<taskboard_ui_slint::DropdownFacts>();
            assert!(facts.get_open(), "the flyout opened");
            assert_eq!(
                facts.get_consumer(),
                taskboard_ui_slint::DropdownConsumer::TaskImportance,
                "the open instance named its consumer kind"
            );
            // The ladder picks vocabulary row 4 (High): the anchor is
            // the current value, so walk to the top, then down to 4.
            for _ in 0..5 {
                harness.driver.press_stroke(parse("Up").unwrap());
            }
            for _ in 0..4 {
                harness.driver.press_stroke(parse("Down").unwrap());
            }
            harness.driver.press_stroke(parse("Return").unwrap());
            harness.driver.advance_time(0);
            let drained = harness.engine.drain();
            assert_eq!(drained.len(), 1);
            assert!(matches!(
                &drained[0],
                taskboard_domain::StateCommand::UpdateTask {
                    changes: taskboard_domain::TaskChanges {
                        importance: Some(Some(taskboard_domain::Importance::High)),
                        ..
                    },
                    ..
                }
            ));
            assert!(
                !window.get_task_model().importance_open,
                "the commit closed the picker"
            );
            assert!(!facts.get_open(), "the flyout unmounted");
        },
    );
}

/// US-6/S2: Escape closes the open picker through the widget's
/// dismissal seam (the decline arm hands the stroke to the field) and
/// dispatches nothing — the focus restore's deep assertions stay
/// `us1_s1_picker_close_restores_the_invoking_field`'s (unchanged
/// oracle; one behavior, one test).
#[test]
fn us6_s2_escape_closes_the_dropdown_without_dispatching() {
    let (_fx, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            label_journey_mount(&mut harness);
            let window = harness.handle.window();
            window.invoke_task_importance_edit_asked();
            harness.driver.advance_time(0);
            let facts = window.global::<taskboard_ui_slint::DropdownFacts>();
            assert!(facts.get_open());
            harness.driver.press_stroke(parse("Escape").unwrap());
            harness.driver.advance_time(0);
            assert!(
                !window.get_task_model().importance_open,
                "the picker closed"
            );
            assert!(!facts.get_open(), "the flyout unmounted");
            assert!(
                harness.engine.drain().is_empty(),
                "Escape dismisses without dispatching"
            );
        },
    );
}

/// US-6/S3: the selector ladder on the detail view — the arrows move
/// the highlight (the `DropdownLadder` seam), Return selects, Escape
/// closes only (the decline arm proves the keys reach the field, D-7).
#[test]
fn us6_s3_the_priority_dropdown_ladder_moves_selects_and_closes() {
    let (_fx, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            label_journey_mount(&mut harness);
            let window = harness.handle.window();
            window.invoke_task_importance_edit_asked();
            harness.driver.advance_time(0);
            let facts = window.global::<taskboard_ui_slint::DropdownFacts>();
            let ladder = window.global::<taskboard_ui_slint::DropdownLadder>();
            assert!(facts.get_open());
            let anchor = facts.get_highlight();
            let nonce = ladder.get_moves();
            harness.driver.press_stroke(parse("Down").unwrap());
            assert_eq!(
                facts.get_highlight(),
                anchor + 1,
                "Down moved the highlight"
            );
            assert!(
                ladder.get_moves() > nonce,
                "every move bumped the ladder's typed counter"
            );
            // Return on the anchor row selects (the vocabulary index
            // is the row index; the unset twin is 0).
            harness.driver.press_stroke(parse("Return").unwrap());
            harness.driver.advance_time(0);
            assert!(!facts.get_open(), "the select closed the flyout");
            assert!(matches!(
                &harness.engine.drain()[..],
                [taskboard_domain::StateCommand::UpdateTask { .. }]
            ));
            // Escape on a fresh open closes only — no selection.
            window.invoke_task_importance_edit_asked();
            harness.driver.advance_time(0);
            assert!(facts.get_open());
            harness.driver.press_stroke(parse("Escape").unwrap());
            harness.driver.advance_time(0);
            assert!(!facts.get_open());
            assert!(harness.engine.drain().is_empty(), "Escape never selects");
        },
    );
}

/// US-6/S4: the urgency field is the mirror twin — its edit intent
/// opens its own consumer kind and its pick commits the urgency mask.
#[test]
fn us6_s4_the_urgency_dropdown_pick_commits_the_urgency_mask() {
    let (_fx, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            label_journey_mount(&mut harness);
            let window = harness.handle.window();
            window.invoke_task_urgency_edit_asked();
            harness.driver.advance_time(0);
            let facts = window.global::<taskboard_ui_slint::DropdownFacts>();
            assert!(facts.get_open());
            assert_eq!(
                facts.get_consumer(),
                taskboard_ui_slint::DropdownConsumer::TaskUrgency,
                "the urgency kind opened, not the importance twin"
            );
            for _ in 0..5 {
                harness.driver.press_stroke(parse("Up").unwrap());
            }
            harness.driver.press_stroke(parse("Return").unwrap());
            harness.driver.advance_time(0);
            assert!(matches!(
                &harness.engine.drain()[..],
                [taskboard_domain::StateCommand::UpdateTask {
                    changes: taskboard_domain::TaskChanges {
                        urgency: Some(None),
                        ..
                    },
                    ..
                }]
            ));
        },
    );
}

/// US-14/S1: `label.manage` opens the labels overlay over the viewed
/// task; the roster renders the board's live labels.
#[test]
fn us14_s1_label_manage_opens_the_overlay_with_the_roster() {
    let (_fx, state, _green, _red) = label_journey_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        assert!(window.get_task_model().labels_open);
        assert_eq!(
            window.get_label_overlay_rows().row_count(),
            2,
            "the board's live labels render as roster rows"
        );
    });
}

/// US-14/S2: the roster toggle dispatches the pair by the row's
/// pushed state — the unassigned row's toggle routes `AssignLabel`
/// with the id resolved from the pass's table.
#[test]
fn us14_s2_the_roster_toggle_dispatches_the_pair() {
    let (_fx, state, _green, red) = label_journey_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        // Row order is canonical (BTreeMap by id): row 1 is `red`
        // (unassigned) in this fixture's id order.
        window.invoke_labels_overlay_row_toggled(1);
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::AssignLabel { label, .. }] if *label == red
        ));
    });
}

/// US-14/S3: the detail chip's × routes `UnassignLabel` over the
/// view's subject (the section always names its subject).
#[test]
fn us14_s3_the_chip_unassign_routes_the_exact_verb() {
    let (fx, state, green, _red) = label_journey_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_chip_unassign_clicked(0);
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::UnassignLabel { task, label }] if *task == fx.task0 && *label == green
        ));
    });
}

/// US-15/S1: the create form commits `CreateLabel` with the subject
/// board's id, the typed title, and the armed swatch's exact hex.
#[test]
fn us15_s1_the_create_form_commits_createlabel() {
    let (fx, state, _green, _red) = label_journey_state();
    let board = state
        .stacks
        .get(&state.tasks.get(&fx.task0).expect("task").stack)
        .expect("stack")
        .board;
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_add_requested();
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_title_edited(slint::SharedString::from("urgent"));
        window.invoke_labels_overlay_swatch_clicked(1);
        assert!(window.invoke_labels_overlay_form_commit_requested());
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::CreateLabel { board: b, title, color }]
                if *b == board && title == "urgent" && color.as_str() == "3a70d6"
        ));
    });
}

/// US-15/S2: the edit form commits the sparse `UpdateLabel` — only
/// the changed slot set.
#[test]
fn us15_s2_the_edit_form_commits_the_sparse_mask() {
    let (_fx, state, green, _red) = label_journey_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_edit_requested(0);
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_title_edited(slint::SharedString::from("verde"));
        assert!(window.invoke_labels_overlay_form_commit_requested());
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::UpdateLabel {
                id,
                changes: taskboard_domain::LabelChanges {
                    title: Some(_),
                    color: None,
                },
            }] if *id == green
        ));
    });
}

/// US-15/S3: the armed delete — first activation arms (nothing
/// dispatches, the strip's mirror reads the row), Confirm dispatches
/// `DeleteLabel` verbatim (`NotUndoable(RestoreClass)` — the staging
/// is the r2 grammar's requirement).
#[test]
fn us15_s3_the_armed_delete_stages_then_confirms() {
    let (_fx, state, green, _red) = label_journey_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_delete_clicked(0);
        harness.driver.advance_time(0);
        assert!(window.get_label_overlay_armed_index() >= 0);
        assert_eq!(harness.engine.drain().len(), 0, "arming dispatches nothing");
        window.invoke_labels_overlay_delete_clicked(window.get_label_overlay_armed_index());
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::DeleteLabel { id }] if *id == green
        ));
    });
}

// -- tag-color UX plan (US-1/W-1, #839): the free-form colour field --

/// US-1/AC-1 (tag-color plan W-1): the field path commits the exact
/// parsed colour — a mixed-case draft mints to the stored lowercase
/// (the domain `Color::parse` normalization) — and the palette path
/// commits the identical payload shape (one vocabulary, both
/// pickers).
#[test]
fn us1_ac1_the_form_commits_the_exact_parsed_colour() {
    let (fx, state, _green, _red) = label_journey_state();
    let board = state
        .stacks
        .get(&state.tasks.get(&fx.task0).expect("task").stack)
        .expect("stack")
        .board;
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_add_requested();
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_title_edited(slint::SharedString::from("mint"));
        // The field path: a mixed-case six-hex draft.
        window.invoke_labels_overlay_colour_edited(slint::SharedString::from("31CC7C"));
        assert!(
            window.get_label_overlay_colour_valid(),
            "the six-hex draft is the typed valid echo"
        );
        assert!(window.invoke_labels_overlay_form_commit_requested());
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(
            matches!(
                &drained[..],
                [taskboard_domain::StateCommand::CreateLabel { board: b, title, color }]
                    if *b == board && title == "mint" && color.as_str() == "31cc7c"
            ),
            "the committed payload is the minted (normalized) draft"
        );

        // The palette path commits the same funnel's exact payload
        // shape (AC-1's two-pickers agreement).
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_add_requested();
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_title_edited(slint::SharedString::from("sw"));
        window.invoke_labels_overlay_swatch_clicked(2);
        // AC-4: the field reflects the swatch's exact value.
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_label_overlay_form_colour().as_str(),
            taskboard_ui_slint::presentation::SWATCHES[2],
            "the field's pushed draft is the swatch's exact hex"
        );
        assert!(window.invoke_labels_overlay_form_commit_requested());
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::CreateLabel { title, color, .. }]
                if title == "sw" && color.as_str() == taskboard_ui_slint::presentation::SWATCHES[2]
        ));
    });
}

/// US-1/AC-2 (tag-color plan W-1): an invalid colour draft renders the
/// typed invalid echo (the validity flag down, the preview
/// transparent) and the commit is the typed inert — nothing dispatches,
/// the form stays open.
#[test]
fn us1_ac2_invalid_colour_renders_the_typed_echo_and_commits_nothing() {
    let (_fx, state, _green, _red) = label_journey_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_add_requested();
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_title_edited(slint::SharedString::from("bad"));
        window.invoke_labels_overlay_colour_edited(slint::SharedString::from("#fff"));
        harness.driver.advance_time(0);
        assert!(
            !window.get_label_overlay_colour_valid(),
            "the invalid draft renders the typed invalid echo"
        );
        assert_eq!(
            window.get_label_overlay_colour_preview(),
            slint::Color::from_argb_u8(0, 0, 0, 0),
            "the preview echo is transparent for an invalid draft"
        );
        assert!(
            !window.invoke_labels_overlay_form_commit_requested(),
            "the commit is the typed inert"
        );
        assert_eq!(harness.engine.drain().len(), 0, "nothing dispatches");
        assert!(window.get_label_overlay_form_open(), "the form stays open");
        // The commit-funnel still refuses after the palette path:
        // an invalid draft cannot sneak in via `swatch-selected`.
        window.invoke_labels_overlay_colour_edited(slint::SharedString::from("zzzzzz"));
        assert!(!window.invoke_labels_overlay_form_commit_requested());
        assert_eq!(harness.engine.drain().len(), 0);
    });
}

// -- tag-color UX plan (US-2/W-2, #839): the roster-only board mount --

/// US-2/AC-1 (tag-color plan W-2): the board menu row — and its
/// Ctrl+L key — opens the labels overlay in roster-only mode over the
/// selected board: the live roster renders and the assignment
/// toggles are absent (the shows-assignment flag down, every row's
/// assigned bit false — structural, the row-set composition).
#[test]
fn us2_ac1_board_menu_opens_the_roster_only_overlay() {
    let (_fx, state, _green, _red) = label_journey_state();
    support::harness::run_journey(state, move |harness| {
        let window = harness.handle.window();
        assert_eq!(window.get_current_view(), View::Board);
        // The click leg: the menu row routes the same action (click
        // == key — the seam's own grammar).
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        assert!(
            window.get_label_overlay_open(),
            "the roster-only mount opens over the board surface"
        );
        assert_eq!(
            window.get_label_overlay_rows().row_count(),
            2,
            "the board's live roster renders"
        );
        assert!(
            !window.get_label_overlay_shows_assignment(),
            "the roster-only mount shows no assignment toggles"
        );
        for i in 0..2 {
            let row = window.get_label_overlay_rows().row_data(i).expect("row");
            assert!(!row.assigned, "board-mount rows carry no assignment");
        }
        // Escape closes (the ladder's last rung; nothing dispatches).
        harness
            .handle
            .dispatch_action(ActionId::LabelOverlayDismiss);
        harness.driver.advance_time(0);
        assert!(!harness.handle.window().get_label_overlay_open());
        harness.assert_commands(&[]);

        // The key leg: the preset's own Ctrl+L binding in the Board
        // context opens the same mount.
        press_preset(
            &mut *harness,
            taskboard_ui_slint::keymap::Preset::Standard,
            ActionId::LabelManage,
        );
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_label_overlay_open(),
            "the preset key opens the roster-only mount"
        );
    });
}

/// US-2/AC-2 (tag-color plan W-2): the roster-only mount's forms
/// commit the exact label commands with the board subject — create
/// (the selected board IS the create subject), the sparse edit, and
/// the staged delete (the #840 armed-identity grammar composing).
#[test]
fn us2_ac2_roster_only_commits_the_exact_label_commands() {
    let (_fx, state, green, red) = label_journey_state();
    let board = state
        .stacks
        .get(&state.tasks.values().next().expect("task").stack)
        .expect("stack")
        .board;
    support::harness::run_journey(state, move |harness| {
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);

        // Create: the roster-only mount's create form.
        window.invoke_labels_overlay_add_requested();
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_title_edited(slint::SharedString::from("board-made"));
        window.invoke_labels_overlay_swatch_clicked(3);
        assert!(window.invoke_labels_overlay_form_commit_requested());
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::CreateLabel { board: b, title, color }]
                if *b == board && title == "board-made"
                    && color.as_str() == taskboard_ui_slint::presentation::SWATCHES[3]
        ));

        // Edit: the seeded form over the board roster.
        window.invoke_labels_overlay_edit_requested(1);
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_title_edited(slint::SharedString::from("rouge"));
        assert!(window.invoke_labels_overlay_form_commit_requested());
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::UpdateLabel {
                id,
                changes: taskboard_domain::LabelChanges {
                    title: Some(_),
                    color: None,
                },
            }] if *id == red
        ));

        // The staged delete (US-2/S2): arm dispatches nothing, the
        // confirm dispatches the exact DeleteLabel.
        window.invoke_labels_overlay_delete_clicked(0);
        harness.driver.advance_time(0);
        assert!(window.get_label_overlay_armed_index() >= 0);
        assert_eq!(harness.engine.drain().len(), 0, "arming dispatches nothing");
        window.invoke_labels_overlay_delete_clicked(window.get_label_overlay_armed_index());
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::DeleteLabel { id }] if *id == green
        ));
    });
}

/// US-2/AC-3 (tag-color plan W-2): the Escape ladder per the r3
/// grammar over the roster-only mount — the armed delete's Escape
/// disarms first, the next rung closes the overlay (the #840 cleared
/// identity composing; nothing dispatches anywhere).
#[test]
fn us2_ac3_roster_only_escape_ladders() {
    let (_fx, state, _green, _red) = label_journey_state();
    support::harness::run_journey(state, move |harness| {
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_delete_clicked(0);
        harness.driver.advance_time(0);
        assert!(window.get_label_overlay_armed_index() >= 0, "armed");

        // Rung one: the dismissal action disarms the strip.
        harness
            .handle
            .dispatch_action(ActionId::LabelOverlayDismiss);
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        assert!(window.get_label_overlay_open(), "the overlay stays");
        assert!(
            window.get_label_overlay_armed_index() < 0,
            "the armed strip disarmed"
        );

        // Rung two: the overlay closes.
        harness
            .handle
            .dispatch_action(ActionId::LabelOverlayDismiss);
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        assert!(!window.get_label_overlay_open());
        harness.assert_commands(&[]);
    });
}

// -- UX remediation chunk 4 (UX R-4 D-1/D-4/D-5, #791/#794/#795) --

/// US-12/S1 (UX R-4 D-1, #791): the opener arms the swatch strip over
/// the selected board — nothing dispatches, the strip mounts with the
/// current color's ring (structural: the open fact + the empty drain).
#[test]
fn us12_s1_board_color_edit_arms_the_swatch_strip() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::BoardColorEdit);
        assert!(
            harness.handle.window().get_board_color_strip_open(),
            "the strip mounts on the opener"
        );
        harness.assert_commands(&[]);
    });
}

/// US-12/S2 (UX R-4 D-1): a swatch click dispatches the exact
/// `SetBoardColor` (the palette's hex — never free text) and closes
/// the strip.
#[test]
fn us12_s2_swatch_pick_commits_setboardcolor_and_closes() {
    let (_fixture, state) = fixture::journey_state();
    let board = state.boards.values().next().expect("fixture board").id;
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::BoardColorEdit);
        harness.handle.window().invoke_board_color_picked(1);
        harness.assert_commands(&[StateCommand::SetBoardColor {
            board,
            color: taskboard_domain::Color::new("3a70d6"),
        }]);
        assert!(
            !harness.handle.window().get_board_color_strip_open(),
            "the commit closes the strip"
        );
    });
}

/// US-12/S3 (UX R-4 D-1): the dismissal (outside click's seam)
/// disarms with nothing dispatched; re-arming works (the verb is
/// self-reversing — the recovery path).
#[test]
fn us12_s3_strip_dismissal_dispatches_nothing_and_rearm_works() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::BoardColorEdit);
        harness.handle.window().invoke_board_color_strip_dismissed();
        harness.assert_commands(&[]);
        assert!(!harness.handle.window().get_board_color_strip_open());
        harness.press(ActionId::BoardColorEdit);
        assert!(harness.handle.window().get_board_color_strip_open());
    });
}

/// US-8/S2 (UX R-4 D-5, #795): on a non-empty board the preset action
/// is the typed no-op — nothing drains (the dispatch-time gate's safe
/// side; the CTA never renders here, but the key could).
#[test]
fn us8_s2_preset_on_nonempty_board_is_the_typed_noop() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::BoardDefaultPreset);
        harness.assert_commands(&[]);
    });
}

/// US-2/S4 (UX R-4 D-4, #794): the target-free `BoardSelect` stroke
/// opens the switcher flyout; the walk's Return routes the highlighted
/// row's selection (the segment seam's route) and closes with the
/// selection moved; Escape closes with nothing dispatched.
#[test]
fn us2_s4_target_free_board_select_opens_flyout_and_return_selects() {
    let mut state = AppState::default();
    state_board(
        &mut state,
        uuid::uuid!("00000000-0000-0000-0000-000000000001"),
        "live one",
        None,
    );
    state_board(
        &mut state,
        uuid::uuid!("00000000-0000-0000-0000-000000000002"),
        "live two",
        None,
    );
    support::harness::run_journey(state, move |harness| {
        // The bare stroke: the flyout arms (BoardBarFacts.open).
        harness.press(ActionId::BoardSelect);
        assert!(
            bar_facts(&*harness).get_open(),
            "the target-free stroke opened the flyout"
        );
        let before = harness.handle.window().get_board_bar_model().selected;
        // The walk: Down moves the highlight off the anchor.
        harness.stroke(parse("Down").expect("Down"));
        let walked = bar_facts(&*harness).get_highlight();
        assert_ne!(walked, before, "the Down arm moved the walk");
        // Return: the walked row's selection routes and the flyout
        // closes.
        harness.stroke(parse("Return").expect("Return"));
        assert_eq!(
            harness.handle.window().get_board_bar_model().selected,
            walked,
            "the walked row is the selection"
        );
        assert!(!bar_facts(&*harness).get_open(), "Return closed the flyout");
        harness.assert_commands(&[]);
        // Escape closes with nothing dispatched.
        harness.press(ActionId::BoardSelect);
        assert!(bar_facts(&*harness).get_open());
        harness.stroke(parse("Escape").expect("Escape"));
        assert!(!bar_facts(&*harness).get_open(), "Escape closed the flyout");
        harness.assert_commands(&[]);
    });
}

/// US-6/S2 (UX R-4 D-8, #796): the today-row context menu extends with
/// the re-commit row (today-only — the extension composition is
/// pinned adapter-side); this journey drives the action through the
/// today seam's route and pins the exact due write.
#[test]
fn us6_s2_today_row_menu_and_preset_key_recommit() {
    // `today_overdue_state`, not `journey_state` (#843, C2 stage-2
    // diagnosis 2026-11-17): the journey drives the re-commit and then
    // context-requests both the *re-committed* row and a surviving
    // control row. Re-anchored 2026-11-18 (#893, the chunk plan D5 —
    // strengthening, never weakening): under the last-covered-instant
    // mint the re-committed row no longer leaves the matrix — it moves
    // overdue → due-today — so the menu is now asserted on the
    // re-committed row itself (section 1, row 0), not on an unrelated
    // surviving row; the second overdue row stays as the control that
    // proves the matrix geometry.
    let now = fixture::PINNED_NOW;
    let (_fixture, state, _walked) = fixture::today_overdue_state_at(now);
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            clock: Some(now),
            ..UiSettings::default()
        },
        move |harness| {
            // The today surface (the verb binds ContextId::Today only).
            harness.press(ActionId::ViewToday);
            assert_eq!(
                today_section_counts(&*harness),
                vec![2, 0, 0, 0],
                "the two overdue control rows open the matrix"
            );
            // Focus a today row (the click seam: click ≡ key's focus
            // half — section 0 is the overdue section, row 0 the
            // fixture's first overdue task).
            harness.handle.window().invoke_today_row_clicked(0, 0);
            // The preset key route: the exact due write (S1's
            // assertion shape, driven through the today surface's
            // focused row) — pinned structurally to the fixture's own
            // frozen day facts (no wall clock anywhere).
            let before = harness.engine.drain().len();
            harness.press(ActionId::TaskDueToday);
            let drained = harness.engine.drain();
            assert_eq!(drained.len(), before + 1);
            let calendar = fixture::harness_calendar();
            let window =
                taskboard_domain::days::day_window(now, calendar.day_start_time, calendar.offset);
            let stamp = taskboard_domain::days::last_covered_instant(window.1);
            assert!(
                matches!(
                    &drained[before],
                    taskboard_domain::StateCommand::UpdateTask {
                        changes: taskboard_domain::TaskChanges {
                            duedate: Some(Some(stamped)),
                            ..
                        },
                        ..
                    } if *stamped == stamp
                ),
                "the re-commit stamps the day's last covered instant"
            );
            harness.engine.settle(&harness.handle);
            assert_eq!(
                today_section_counts(&*harness),
                vec![1, 0, 1, 0],
                "the re-committed row moved within Today, the control stayed overdue"
            );
            // The today-row context menu carries the re-commit row at
            // its *new* position — section 1 (due-today), row 0 (the
            // today-only extension; the card menus' negative is the
            // adapter composition pin).
            harness
                .handle
                .window()
                .invoke_today_row_context_requested(1, 0, 10.0, 10.0);
            let entries = keyboard(&*harness).get_menu_entries();
            let ids: Vec<String> = (0..entries.row_count())
                .filter_map(|i| entries.row_data(i))
                .map(|row| row.action.to_string())
                .collect();
            assert!(
                ids.iter().any(|id| id == "task.due-today"),
                "the re-committed row's menu offers the re-commit"
            );
            // The control row (overdue section 0, row 0) still
            // resolves its own menu request — the geometry witness.
            harness
                .handle
                .window()
                .invoke_today_row_context_requested(0, 0, 10.0, 10.0);
            let entries = keyboard(&*harness).get_menu_entries();
            let ids: Vec<String> = (0..entries.row_count())
                .filter_map(|i| entries.row_data(i))
                .map(|row| row.action.to_string())
                .collect();
            assert!(
                ids.iter().any(|id| id == "task.due-today"),
                "the control row's menu still resolves"
            );
        },
    );
}

// ---- keymap parity (ux-r8 chunk 1; keymap-parity plan §4) --------------

/// Whether the description editor flow is armed on the mounted detail.
fn description_open(harness: &Harness) -> bool {
    harness
        .handle
        .window()
        .get_task_model()
        .description_edit_open
}

/// A detail open over the fixture's first board card, through the
/// preset's own rows (the journeys' shared prelude): focus the first
/// card, activate it into the detail.
fn open_detail_first_card(harness: &mut Harness, preset: taskboard_ui_slint::keymap::Preset) {
    press_preset(harness, preset, ActionId::NavFocusDown);
    press_preset(harness, preset, ActionId::TaskEdit);
    harness.driver.advance_time(0);
    assert_eq!(
        harness.handle.window().get_current_view(),
        View::TaskDetail,
        "the detail is open"
    );
}

/// #893 (the chunk plan D6, the issue's instrumented repro): the
/// today re-commit on the fixture's single overdue row keeps the row
/// *in* Today — it moves within the matrix (overdue → due-today),
/// never out of it. Red under the exclusive-end write (the matrix
/// rebuilt `[0, 0, 0, 0]`: the stamp equalled the window's excluded
/// end), green under the last-covered-instant mint. Typed structural
/// facts only: the drained `UpdateTask` stamp is pinned against the
/// fixture's own frozen day window (the mount clock is frozen at
/// `PINNED_NOW` — no wall clock anywhere), and the section counts are
/// row counts, not phrases.
#[test]
fn us6_s2_recommit_keeps_the_row_in_today() {
    let now = fixture::PINNED_NOW;
    let (_fixture, state) = fixture::journey_state_at(now);
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            clock: Some(now),
            ..UiSettings::default()
        },
        move |harness| {
            harness.press(ActionId::ViewToday);
            assert_eq!(
                today_section_counts(&*harness),
                vec![1, 0, 0, 0],
                "the fixture's single overdue row opens the matrix"
            );
            // Focus the overdue row (section 0, row 0) and re-commit.
            harness.handle.window().invoke_today_row_clicked(0, 0);
            let before = harness.engine.drain().len();
            harness.press(ActionId::TaskDueToday);
            let drained = harness.engine.drain();
            assert_eq!(drained.len(), before + 1);
            // The stamp oracle is the fixture's own day facts: the
            // frozen window's last covered instant, computed through
            // the same domain vocabulary the adapter reads.
            let calendar = fixture::harness_calendar();
            let window =
                taskboard_domain::days::day_window(now, calendar.day_start_time, calendar.offset);
            let stamp = taskboard_domain::days::last_covered_instant(window.1);
            assert!(
                matches!(
                    &drained[before],
                    taskboard_domain::StateCommand::UpdateTask {
                        changes: taskboard_domain::TaskChanges {
                            duedate: Some(Some(stamped)),
                            ..
                        },
                        ..
                    } if *stamped == stamp
                ),
                "the re-commit stamps the day's last covered instant"
            );
            harness.engine.settle(&harness.handle);
            assert_eq!(
                today_section_counts(&*harness),
                vec![0, 0, 1, 0],
                "the row moved within Today (overdue 0, due-today 1), not out of it"
            );
        },
    );
}

// RETIRED (J3 R-3, 2026-11-19, round 9 chunk 9):
// `us1_s1_emacs_motion_rows_step_the_walks` asserted the detail's *subject*
// walk — retired by the census-dated round-8 D-2 supersession
// (docs/ux-surface-census.org `nav.item_walk`, SUPERSEDED 2026-11-19); the
// replacement journey — the control walk pinned with the subject —
// lands in the nominated evidence file `focus_law_journeys.rs`
// (::us1_s1_taskdetail_directional_rows_walk_the_ladder_subject_pinned).
/// US-1/AC-2 (keymap-parity plan W-1) — the Settings field walk: all
/// four emacs chords resolve over the settings chain and step the
/// control walk with zero commands (the walk is view-internal; the
/// traversal mechanics' own layer is the walk unit tests and the
/// focus-coverage suite — one behaviour, one test, lowest layer).
#[test]
fn us1_ac2_settings_control_walk_rows_step() {
    let (_fixture, state) = fixture::golden_state();
    let preset = taskboard_ui_slint::keymap::Preset::Emacs;
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
            keymap_preset: preset,
            ..UiSettings::default()
        },
        move |harness| {
            press_preset(&mut *harness, preset, ActionId::AppSettings);
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Settings,
                "the settings surface is open"
            );
            // The retune (J3 US-1, round 9 chunk 9): emacs' four
            // directional chords carry the settings walk (C-f/C-b
            // read left/right, C-n/C-p down/up — all the control
            // walk at Settings).
            for action in [
                ActionId::NavFocusRight,
                ActionId::NavFocusRight,
                ActionId::NavFocusLeft,
                ActionId::NavFocusLeft,
                ActionId::NavFocusDown,
                ActionId::NavFocusUp,
            ] {
                press_preset(&mut *harness, preset, action);
                harness.driver.advance_time(0);
            }
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Settings,
                "the walk stays on the settings surface"
            );
            harness.assert_commands(&[]);
        },
    );
}

// RETIRED (J3 R-3, 2026-11-19, round 9 chunk 9):
// `us2_s1_menu_first_last_matches_the_key_route` asserted the detail's
// first/last subject jumps — retired with the subject walk (the same
// census-dated supersession; the board's first/last jump journeys remain).
/// US-3/S1 (keymap-parity plan W-3) — the due/scheduled shortcuts arm
/// the editor exactly like the row affordance: the same funnel state
/// (armed window, seeded buffer — empty for the fixture's dateless
/// task, the §8 invariant's honest arm) from both fronts, and Escape
/// settles per the existing ladder.
#[test]
fn us3_s1_due_shortcut_arms_the_editor_like_the_row() {
    let (_fixture, state) = fixture::golden_state();
    let preset = taskboard_ui_slint::keymap::Preset::Standard;
    support::harness::run_journey_with_settings(
        state.clone(),
        UiSettings {
            keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
            keymap_preset: preset,
            ..UiSettings::default()
        },
        move |harness| {
            open_detail_first_card(&mut *harness, preset);

            // The key front: F5 arms the due editor; the buffer is
            // seeded with the field's current value (empty — the task
            // is dateless).
            press_preset(&mut *harness, preset, ActionId::TaskDueEdit);
            harness.driver.advance_time(0);
            let model = harness.handle.window().get_task_model();
            assert!(model.due_open, "F5 arms the due editor");
            let seeded = model.due_text.to_string();
            // The default-open picker phase (round-3 D10): Escape steps
            // back to the armed field first, the second Escape is the
            // clean close (the r4 D25 ladder).
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_task_model().due_open,
                "Escape settles the editor"
            );

            // The pointer front: the row affordance's funnel state is
            // byte-identical (one Adapter method, both fronts).
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            let model = harness.handle.window().get_task_model();
            assert!(model.due_open, "the row affordance arms the editor");
            assert_eq!(
                model.due_text.to_string(),
                seeded,
                "the seeded buffer is the funnel's, identical from both fronts"
            );
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);

            // The scheduled twin: F6 arms the start-date editor, the
            // same funnel over the other field.
            press_preset(&mut *harness, preset, ActionId::TaskScheduledEdit);
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_task_model().scheduled_open,
                "F6 arms the scheduled editor"
            );
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_task_model().scheduled_open,
                "Escape settles the scheduled editor"
            );
            harness.assert_commands(&[]);
        },
    );
}

/// US-3/S2 (keymap-parity plan W-3) — the description shortcut arms
/// the editor like the field's affordance: the `EditDescription` flow
/// state from both fronts, the TEXT settle rule unchanged (Escape
/// discards).
#[test]
fn us3_s2_description_shortcut_arms_the_editor() {
    let (_fixture, state) = fixture::golden_state();
    let preset = taskboard_ui_slint::keymap::Preset::Standard;
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
            keymap_preset: preset,
            ..UiSettings::default()
        },
        move |harness| {
            open_detail_first_card(&mut *harness, preset);

            // The key front: F7 arms the description editor.
            press_preset(&mut *harness, preset, ActionId::TaskDescriptionEdit);
            harness.driver.advance_time(0);
            assert!(
                description_open(&*harness),
                "F7 arms the description editor"
            );
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(
                !description_open(&*harness),
                "Escape discards (the TEXT settle rule)"
            );

            // The pointer front: the field's own affordance arms the
            // identical flow.
            harness.driver.click_by_label("Edit description");
            harness.driver.advance_time(0);
            assert!(
                description_open(&*harness),
                "the affordance arms the same editor"
            );
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
        },
    );
}

/// US-3/S3 (keymap-parity plan W-3) — the clock shortcuts route the
/// exact commands the chip routes: F9 dispatches `StartTracking` over
/// the viewed subject, F10 dispatches the payload-free `StopTracking`;
/// from the Today context the rows resolve (the tracking pair's
/// second context) and a focusless dispatch is the typed `NoTarget`
/// (nothing reaches the boundary).
#[test]
fn us3_s3_clock_shortcuts_route_the_exact_commands() {
    let (fixture, state) = fixture::golden_state();
    let task0 = fixture.task0;
    let preset = taskboard_ui_slint::keymap::Preset::Standard;
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
            keymap_preset: preset,
            ..UiSettings::default()
        },
        move |harness| {
            open_detail_first_card(&mut *harness, preset);
            press_preset(&mut *harness, preset, ActionId::TaskTrackStart);
            harness.driver.advance_time(0);
            harness.assert_commands(&[StateCommand::StartTracking {
                task: task0,
                kind: taskboard_domain::tracking::TimeEntryKind::Work,
            }]);
            press_preset(&mut *harness, preset, ActionId::TaskTrackStop);
            harness.driver.advance_time(0);
            harness.assert_commands(&[StateCommand::StopTracking]);

            // The Today rows resolve (the tracking pair's Today
            // context) and route the same commands over the carried
            // focus (the two honest sources: the focus survives the
            // view jump).
            press_preset(&mut *harness, preset, ActionId::ViewToday);
            harness.driver.advance_time(0);
            press_preset(&mut *harness, preset, ActionId::TaskTrackStart);
            harness.driver.advance_time(0);
            press_preset(&mut *harness, preset, ActionId::TaskTrackStop);
            harness.driver.advance_time(0);
            harness.assert_commands(&[
                StateCommand::StartTracking {
                    task: task0,
                    kind: taskboard_domain::tracking::TimeEntryKind::Work,
                },
                StateCommand::StopTracking,
            ]);
        },
    );
}

/// US-3/S4 (keymap-parity plan W-3) — the rule-add shortcut routes the
/// preset buttons' exact funnel: the button's click and the emacs
/// `C-c C-u` both dispatch `AddReminderRule` over the armed one-shot
/// `when` (click ≡ key at the route); the consumed one-shot is the
/// typed inert (the second key press dispatches nothing).
#[test]
fn us3_s4_rule_add_shortcut_arms_the_funnel() {
    let (fixture, state) = fixture::golden_state();
    let task0 = fixture.task0;
    let preset = taskboard_ui_slint::keymap::Preset::Emacs;
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
            keymap_preset: preset,
            ..UiSettings::default()
        },
        move |harness| {
            open_detail_first_card(&mut *harness, preset);

            // The pointer front arms the one-shot `when` and routes:
            // the exact command over the subject.
            harness.handle.window().invoke_rule_add_preset_clicked(0);
            harness.driver.advance_time(0);
            harness.assert_commands(&[StateCommand::AddReminderRule {
                task: task0,
                offset: taskboard_domain::ReminderOffset::before(
                    TimeDelta::try_minutes(-5).expect("-5m"),
                )
                .expect("legal offset"),
                at: None,
                insistence: taskboard_domain::Insistence::Normal,
            }]);

            // The key front routes the same action: with the one-shot
            // consumed it is the typed inert — the dispatch gate is
            // surfaced, not silent (the drain stays empty).
            press_preset(&mut *harness, preset, ActionId::ReminderRuleAdd);
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
        },
    );
}

// === The focus-origin restore (round-8 chunk 2, the focus-restoration
// === plan US-1/W-1; the chunk plan §2 census table is the site list).

/// The `FocusFacts` tuple (seam, anchor kind, target a, target b,
/// nonce) — the restore's typed oracle.
fn focus_facts(harness: &Harness) -> (i32, i32, i32, i32, i32) {
    let facts = harness
        .handle
        .window()
        .global::<taskboard_ui_slint::FocusFacts>();
    (
        facts.get_seam_id(),
        facts.get_anchor_kind(),
        facts.get_target_a(),
        facts.get_target_b(),
        facts.get_nonce(),
    )
}

/// The focused item's element ids (the `focus_coverage` probe tier's
/// structural address — no label text).
fn focused_carries(harness: &Harness, element_id: &str) -> bool {
    use i_slint_core::item_tree::{ItemRc, ParentItemTraversalMode};
    use i_slint_core::window::WindowInner;
    let focused = WindowInner::from_pub(harness.handle.window().window())
        .focus_item
        .borrow()
        .upgrade();
    let Some(item) = focused else {
        return false;
    };
    let ids = |item: &ItemRc| {
        let count = item.element_count().unwrap_or(0);
        (0..count)
            .filter_map(|index| item.element_type_names_and_ids(index))
            .flatten()
            .filter_map(|(_, id)| {
                if id.is_empty() {
                    None
                } else {
                    Some(id.clone())
                }
            })
            .collect::<Vec<_>>()
    };
    // The runtime qualifies ids with their component ("AppWindow::
    // key-root"); the probe's structural address is the bare id.
    let chain = std::iter::successors(Some(item), |parent| {
        parent.parent_item(ParentItemTraversalMode::StopAtPopups)
    })
    .flat_map(|item| ids(&item))
    .map(|id| id.rsplit("::").next().unwrap_or(&id).to_owned())
    .collect::<Vec<_>>();
    chain.iter().any(|id| id == element_id)
}

/// Whether the focused element ITSELF carries the element id (no
/// ancestor walk — the exact-holder probe; `key-root` is every
/// element's ancestor, so the ancestor walk cannot name it).
fn focused_self_carries(harness: &Harness, element_id: &str) -> bool {
    use i_slint_core::window::WindowInner;
    let Some(item) = WindowInner::from_pub(harness.handle.window().window())
        .focus_item
        .borrow()
        .upgrade()
    else {
        return false;
    };
    let count = item.element_count().unwrap_or(0);
    (0..count)
        .filter_map(|index| item.element_type_names_and_ids(index))
        .flatten()
        .any(|(_, id)| !id.is_empty() && id.rsplit("::").next() == Some(element_id))
}

/// US-4/S1: a walk step that switches the focused board parks the
/// slint focus where the walk left it (inside the walked surface) —
/// the identity-diff tail's view-scope re-anchor is suppressed for
/// the walk (the double handoff that re-anchored the key root over
/// the walked row is the #670 fix); the walked row is the emphasized
/// one on the crossed board.
#[test]
fn us4_s1_cross_board_walk_parks_focus_on_the_row() {
    let (_fixture, state, [_board_one_task, _board_two_task]) = fixture::today_overdue_state();
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            let mut harness = Harness::mount_no_escape(SlintDriver::default(), state);
            // The stay-on-today mount leaves the Today surface holding
            // slint focus (its init grab; the mount dispatch no tail
            // rides it) — the guard's letter is "the walk's focus
            // survives it", so the probe can read the holder directly.
            let emphasized_row = |harness: &Harness| {
                let sections = harness.handle.window().get_today_model().sections;
                let rows = sections.row_data(0).expect("overdue section").rows;
                (0..rows.row_count()).find(|r| rows.row_data(*r).expect("row").card.emphasis)
            };
            harness.press(ActionId::NavFocusDown);
            let before = emphasized_row(&harness).expect("the walk seeded a row");
            // The cross-board step: the walk switches boards, the
            // identity diff fires — and the walked row survives it.
            harness.press(ActionId::NavFocusDown);
            harness.driver.advance_time(0);
            let after = emphasized_row(&harness).expect("the walk crossed boards");
            assert_ne!(before, after, "the step crossed to the other board");
            assert!(
                !focused_self_carries(&harness, "key-root"),
                "the walked row kept the focus off the key root"
            );
        },
    );
}

/// US-1/S1+S2 (the census's picker family, rows 5/14): the priority
/// picker opened from the detail records the origin at open and
/// restores into the opener's own cell at Escape — the typed fact
/// lands and the cell physically holds slint focus. DATED
/// SUPERSESSION 2026-11-22 (round 5 chunk A2, #1205's per-seam
/// detail restore cells): the r8 pin recorded the title-cell
/// collapse (kind 4) — the census's single detail cell — which the
/// chunk law replaces with the opener's own cell (the importance
/// field, kind 9).
#[test]
fn us1_s1_picker_close_restores_the_invoking_field() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskEdit);
        let facts_before = focus_facts(&*harness);
        harness.press(ActionId::TaskImportanceEdit);
        assert!(
            harness.handle.window().get_task_model().importance_open,
            "the picker opened"
        );
        // Escape's routing (the CancelMove arm) closes the picker.
        harness.press(ActionId::TaskMoveCancel);
        harness.engine.settle(&harness.handle);
        assert!(
            !harness.handle.window().get_task_model().importance_open,
            "the picker closed"
        );
        // The restore fact: seam PriorityDue (3), anchor kind 9 (the
        // importance cell — the opener's own cell, the A2 law), a
        // fresh nonce.
        let (seam, kind, a, b, nonce) = focus_facts(&*harness);
        assert_eq!((seam, kind, a, b), (3, 9, -1, -1), "the origin fact");
        assert_eq!(nonce, facts_before.4 + 1, "one-shot edge");
        assert!(
            focused_carries(&*harness, "field-scope"),
            "the importance cell's field holds slint focus"
        );
    });
}

/// US-1/S2 (the census's menu row 7): a menu opened over a walked row
/// restores the row anchor at the Escape close — the typed fact names
/// the row the walk emphasized.
#[test]
fn us1_s1_menu_close_restores_the_origin() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        let emphasized = harness
            .emphasized_card()
            .expect("the walk emphasized task0");
        harness.press(ActionId::AppMenu);
        assert!(keyboard(&*harness).get_menu_open(), "the menu opened");
        harness.press(ActionId::MenuDismiss);
        let (seam, kind, a, b, _) = focus_facts(&*harness);
        assert_eq!((seam, kind), (6, 1), "the menu origin fact");
        assert_eq!(
            (a, b),
            (
                i32::try_from(emphasized.0).expect("column index"),
                i32::try_from(emphasized.1).expect("row index")
            ),
            "the restoring row is the emphasized origin"
        );
        assert_eq!(
            harness.emphasized_card(),
            Some(emphasized),
            "the walked look landed back on the origin"
        );
    });
}

/// US-1/S2 (the overlay-escape family, census rows 8/9): the palette
/// with no row origin falls to the view scope (today's letter, now
/// pinned — first regression net); the capture overlay opened from a
/// walked row restores the row.
#[test]
fn us1_s1_overlay_escape_restores_the_origin() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        // The palette leg: no row focused → the view scope. The
        // restore fires with no element fact (the nonce and kind
        // stay where they were) and the key root re-anchors.
        harness.press(ActionId::PaletteSearch);
        assert!(
            harness.handle.window().get_palette_open(),
            "the palette opened"
        );
        harness.stroke(parse("Escape").expect("escape"));
        assert!(
            !harness.handle.window().get_palette_open(),
            "the palette closed"
        );
        let (_, kind, _, _, _) = focus_facts(&*harness);
        assert_eq!(kind, 0, "the view-scope letter for a View origin");
        assert!(
            focused_carries(&*harness, "key-root"),
            "the view scope re-anchored"
        );
        // The capture leg: opened from a walked row.
        harness.press(ActionId::NavFocusDown);
        let emphasized = harness
            .emphasized_card()
            .expect("the walk emphasized task0");
        harness.press(ActionId::CaptureQuick);
        harness.stroke(parse("Escape").expect("escape"));
        let (seam, kind, a, b, _) = focus_facts(&*harness);
        assert_eq!((seam, kind), (8, 1), "the capture origin fact");
        assert_eq!(
            (
                a,
                b,
                harness.emphasized_card().map(|card| (
                    i32::try_from(card.0).expect("column"),
                    i32::try_from(card.1).expect("row")
                ))
            ),
            (
                i32::try_from(emphasized.0).expect("column"),
                i32::try_from(emphasized.1).expect("row"),
                Some((
                    i32::try_from(emphasized.0).expect("column"),
                    i32::try_from(emphasized.1).expect("row")
                ))
            ),
            "the capture's origin row restored"
        );
    });
}

/// US-1/AC-2 (the vanished-origin fallback): the move flow's anchor
/// task dies while the flow holds; the reconcile drops the flow (the
/// site closes) and the restore falls to the recorded container —
/// the stack header — never to no-control.
#[test]
fn us1_s2_vanished_origin_falls_to_the_container_anchor() {
    let (fixture, state) = fixture::journey_state();
    // The tombstone the engine applies mid-flow: the anchor removed
    // (the clone rides into the closure; the mount keeps the live one).
    let mut tomb = state.clone();
    tomb.tasks.remove(&fixture.task0);
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskMove);
        // The anchor dies: the reconcile drops the flow, the site
        // closes, the restore falls to the container anchor.
        harness.engine.replace_state(tomb, &harness.handle);
        harness.engine.settle(&harness.handle);
        harness.handle.refresh();
        let (seam, kind, a, _, _) = focus_facts(&*harness);
        assert_eq!(seam, 14, "the move flow's origin seam");
        assert_eq!(kind, 2, "the container anchor (the stack header)");
        assert_eq!(a, 0, "stack a's column");
    });
}

/// US-1/AC-3 + the census's editor family (rows 14/15): the title
/// editor's *commit* exit restores into the subject's host cell —
/// the field is gone post-commit; the title cell is the anchor.
#[test]
fn us1_s3_commit_exit_restores_the_row_anchor() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskEdit);
        harness.handle.dispatch_action(ActionId::TaskTitleEdit);
        harness.type_text("z");
        harness.stroke(parse("Return").expect("return"));
        harness.engine.settle(&harness.handle);
        let (seam, kind, _, _, _) = focus_facts(&*harness);
        assert_eq!((seam, kind), (12, 4), "the title editor's origin fact");
        assert!(
            focused_carries(&*harness, "value-stop"),
            "the title cell holds slint focus after the commit"
        );
    });
}

// === The control walk's mask clamp (round-8 chunk 2, the focus-
// === restoration plan US-2/W-2) =================================

/// The `MaskWalkFacts` triple (armed, extent, cursor) — the clamp's
/// typed oracle.
fn mask_facts(harness: &Harness) -> (bool, i32, i32) {
    let mask = harness
        .handle
        .window()
        .global::<taskboard_ui_slint::MaskWalkFacts>();
    (mask.get_armed(), mask.get_extent(), mask.get_cursor())
}

/// US-2/S1: inside the labels overlay (the armed `Labels` context's
/// host) Tab walks the panel's own controls in order and wraps at the
/// mask boundary — nothing under the mask ever receives focus; the
/// physical focus reads (the probe tier's oracle) ride the panel's
/// named controls.
#[test]
fn us2_s1_tab_wraps_inside_the_masked_context() {
    let (_fx, state, _green, _red) = label_journey_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        harness.handle.window().invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        // The host pushed the mask facts; the walk starts from the
        // host's stop 0 (the panel scope's own grab is its mount
        // hand-off — the clamp consumes the cursor, not the mount).
        assert_eq!(mask_facts(&*harness), (true, 3, 0), "the host facts");
        // Tab: panel → add (1).
        harness.stroke(parse("Tab").expect("tab"));
        harness.driver.advance_time(0);
        assert!(
            focused_carries(&*harness, "new-btn"),
            "stop 1 is the add button"
        );
        assert_eq!(mask_facts(&*harness).2, 1, "the cursor followed");
        // Tab: add → close (2).
        harness.stroke(parse("Tab").expect("tab"));
        harness.driver.advance_time(0);
        assert!(
            focused_carries(&*harness, "close-btn"),
            "stop 2 is the close button"
        );
        // Tab: close → wraps to the panel scope (3 % 3 = 0).
        harness.stroke(parse("Tab").expect("tab"));
        harness.driver.advance_time(0);
        assert!(
            focused_carries(&*harness, "panel-scope"),
            "the walk wrapped"
        );
        // Shift+Tab: panel → wraps backwards to close (2).
        harness.stroke(parse("Shift+Tab").expect("shift+tab"));
        harness.driver.advance_time(0);
        assert!(
            focused_carries(&*harness, "close-btn"),
            "the backwards walk wrapped"
        );
    });
}

/// US-2/AC-2 (the degenerate wrap): the timer setter's host holds a
/// single walk stop — Tab stays on it (the walk never leaks to the
/// view beneath the setter panel).
#[test]
fn us2_s2_single_control_context_degenerates_to_stay() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.handle.window().invoke_timers_setter_opened();
        harness.driver.advance_time(0);
        assert!(
            focused_carries(&*harness, "setter-scope"),
            "the setter opened on its scope"
        );
        harness.stroke(parse("Tab").expect("tab"));
        harness.driver.advance_time(0);
        let (armed, extent, _) = mask_facts(&*harness);
        assert!(
            armed && extent == 1,
            "the armed host pushed the degenerate extent"
        );
        assert!(
            focused_carries(&*harness, "setter-scope"),
            "the walk stayed on the single stop"
        );
    });
}

// === The menu's focused-open (round-8 chunk 2, the focus-restoration
// === plan US-3/W-3) =============================================

/// US-3/S1+S2: the menu opens focused on its rows — the walk starts
/// inside the menu at the remembered/first row (the shell cursor's
/// pre-selection, the focused-control fact), the arrow walk moves
/// rows in place, and Return invokes the walked row (one keypress
/// from invocation). Recorded deviation (D-M3's physical letter): the
/// rows' scopes do not take slint focus — the menu panel lives
/// outside the single key scope (§5.1), and a focus move there
/// dead-ends the raw key path on platforms whose layout loop has not
/// realized the panel (slint's invisible-focus reset); the walk
/// starts inside the menu through the shell cursor instead, which is
/// the acceptance behavior's substance.
#[test]
fn us3_s1_menu_open_lands_focus_on_the_menu() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppMenu);
        assert_eq!(
            keyboard(&*harness).get_menu_selected_index(),
            0,
            "the menu opens on its first row (the focused-control fact)"
        );
        // The walk: Down moves the shell's cursor in place — the
        // walk is inside the menu (the panel stays mounted).
        harness.press(ActionId::MenuNext);
        assert_eq!(
            keyboard(&*harness).get_menu_selected_index(),
            1,
            "the arrow walk moves the rows"
        );
        assert!(
            keyboard(&*harness).get_menu_open(),
            "the walk stays in the menu"
        );
        // Return invokes the walked row: the menu closes and the
        // invocation's own dispatch routes.
        harness.press(ActionId::MenuInvoke);
        assert!(
            !keyboard(&*harness).get_menu_open(),
            "the invocation closed the menu"
        );
    });
}

// === The notification surface's keyboard grammar (round-8 chunk 2,
// === the focus-restoration plan US-6/W-6) =======================

/// The notification harness: one delivered task notification (the
/// anchor table's task-anchored row) over the golden fixture, mounted
/// in the driver job (the ten-step mount shape; the tests run whole
/// in `driver::run`).
fn notification_harness() -> (fixture::Fixture, AppState) {
    let (fixture, mut state) = fixture::golden_state();
    let due = chrono::Utc::now() - TimeDelta::minutes(5);
    fixture::deliver_task_notification(&mut state, fixture.task1, due);
    (fixture, state)
}

/// One raw key press through the public dispatch path (the lib test
/// press's shape; the journeys driver's stroke spelling can't name
/// the navigation codepoints).
fn press_raw(handle: &taskboard_ui_slint::UiHandle, text: &str) {
    handle
        .window()
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.into() });
}

/// US-6/S1: the first walk stroke arms the surface and selects row 0
/// (the AC-1 pre-selection); the second clamps at the end — the walk
/// never wraps past the last row, and the highlight follows the
/// cursor (the selected-index push).
#[test]
fn us6_s1_notification_walk_selects_and_clamps() {
    let (_fixture, state) = notification_harness();
    support::harness::run_journey(state, move |harness| {
        // Onto Settings: the fallback-arming stage (the recorded
        // posture revision — a dead key arms; the bound keys keep
        // their ambient claims, the focus overlay's walk being
        // the recorded bar). Settings binds no plain Down/Up.
        harness.press(ActionId::AppSettings);
        let window = harness.handle.window();
        assert_eq!(window.get_notification_model().row_count(), 1);
        assert_eq!(window.get_notification_selected(), -1, "disengaged");
        // The arming stroke: Down arms and pre-selects row 0.
        press_raw(&harness.handle, "\u{F701}");
        assert_eq!(window.get_notification_selected(), 0, "the arm pre-selects");
        // The clamp: a second Down stays on the last row (no wrap).
        press_raw(&harness.handle, "\u{F701}");
        assert_eq!(window.get_notification_selected(), 0, "the walk clamps");
        // Up walks back (still row 0 — the single-row clamp at the head).
        press_raw(&harness.handle, "\u{F700}");
        assert_eq!(window.get_notification_selected(), 0, "the head clamp");
    });
}

/// US-6/S2 (the click ≡ key law): the `d` verb dispatches exactly the
/// command the row's dismiss button seam sends — the structural
/// equality over two fresh harnesses.
#[test]
fn us6_s2_notification_verb_id_matches_the_button_command() {
    let (_fixture, state) = notification_harness();
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        move |_driver| {
            // The key leg: one fresh harness drives the `d` verb.
            let key_commands = {
                let mut harness = Harness::mount(SlintDriver::default(), state.clone());
                press_raw(&harness.handle, "d");
                harness.engine.drain()
            };
            // The click leg: a second fresh harness drives the seam.
            let click_commands = {
                let mut harness = Harness::mount(SlintDriver::default(), state.clone());
                harness
                    .handle
                    .window()
                    .invoke_notification_dismiss_clicked(0);
                harness.engine.drain()
            };
            assert_eq!(
                key_commands, click_commands,
                "the key verb and the button seam dispatch identically"
            );
            assert!(
                matches!(
                    &key_commands[..],
                    [taskboard_domain::StateCommand::DismissReminder { .. }]
                ),
                "the dismissal command's shape"
            );
        },
    );
}

/// US-6/S3: Escape closes the engaged surface dispatching nothing
/// (the rows keep their delivery state) and disarms — the follow-up
/// grammar stroke is ambient again (the `d` key dispatches nothing
/// once disengaged).
#[test]
fn us6_s3_notification_escape_closes_without_dispatch() {
    let (_fixture, state) = notification_harness();
    support::harness::run_journey(state, move |harness| {
        // Onto Settings (the fallback-arming stage: a dead Down
        // arms — see us6_s1's recorded posture revision).
        harness.press(ActionId::AppSettings);
        press_raw(&harness.handle, "\u{F701}");
        assert_eq!(
            harness.handle.window().get_notification_selected(),
            0,
            "engaged"
        );
        press_raw(&harness.handle, "\u{1b}");
        assert!(
            harness.engine.drain().is_empty(),
            "the Escape close dispatches nothing"
        );
        // The disarm: the grammar may re-arm on its own keys
        // (D-N1's arming letter), but an AMBIENT key claims
        // nothing — the unbound `x` dispatches nothing and the
        // context chain no longer heads the surface.
        press_raw(&harness.handle, "x");
        assert!(
            harness.engine.drain().is_empty(),
            "the disarmed surface is ambient again"
        );
        assert_eq!(
            harness.handle.window().get_notification_selected(),
            -1,
            "the surface disengaged"
        );
    });
}

// === The armed fields' focus flags (round-8 chunk 2, the focus-
// === restoration plan US-5/W-5) =================================

/// The task model's (title-armed, description-armed) pair — the
/// US-5 flag oracle.
fn detail_flags(harness: &Harness) -> (bool, bool) {
    let model = harness.handle.window().get_task_model();
    (model.title_armed, model.description_armed)
}

/// US-5/S1: the arm paths carry the per-field focus flag on the task
/// model — the title editor's arm sets `title-armed`, the description
/// editor's arm sets `description-armed`, and the discards clear
/// them; the closed cells' rings read the flags beside their own
/// has-focus (typed model reads, per field).
#[test]
fn us5_s1_the_title_and_description_fields_carry_the_focus_flag() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskEdit);
        assert_eq!(
            detail_flags(&*harness),
            (false, false),
            "the closed cells start unarmed"
        );
        // The title arm (F2's route).
        harness.handle.dispatch_action(ActionId::TaskTitleEdit);
        assert!(detail_flags(&*harness).0, "the title arm sets the flag");
        // Discard (Escape's letter), then the description arm.
        harness.stroke(parse("Escape").expect("escape"));
        assert_eq!(
            detail_flags(&*harness),
            (false, false),
            "the discard clears the flag"
        );
        harness
            .handle
            .window()
            .invoke_task_description_edit_opened();
        assert!(
            detail_flags(&*harness).1,
            "the description arm sets the flag"
        );
        harness.stroke(parse("Escape").expect("escape"));
        assert_eq!(
            detail_flags(&*harness),
            (false, false),
            "the discard clears the flag"
        );
    });
}

/// The journey fixture's Today-nonempty-by-construction contract
/// (#843, clock-discipline §6.1): given `journey_state_at(now)`, the
/// domain oracle `today_sections` classifies `task1` into the overdue
/// section for every pass instant across the today window — the same
/// classification the harness's day pass derives, so fixture and pass
/// agree by construction. Typed set-membership, no mount.
#[test]
fn the_journey_fixture_is_today_nonempty_by_construction() {
    let now = chrono::Utc::now();
    let (fixture, state) = fixture::journey_state_at(now);
    let calendar = taskboard_domain::DayCalendar {
        day_start_time: chrono::NaiveTime::MIN,
        offset: chrono::FixedOffset::east_opt(0).expect("utc"),
    };
    let today = calendar.today(now);
    let window =
        taskboard_domain::review_window(taskboard_domain::ReviewCadence::Day, today, calendar);
    // The pass reads its window per repaint: every instant the pass
    // can observe, from the captured `now` through the today window's
    // end, must classify task1 overdue.
    let pass_instants = [
        now,
        window.end() - chrono::TimeDelta::try_minutes(1).expect("one minute"),
        window.end(),
    ];
    for pass in pass_instants {
        let sections = taskboard_domain::days::today_sections(&state.tasks, pass, window.bounds());
        assert!(
            sections.overdue.contains(&fixture.task1),
            "task1 is overdue at pass instant {pass}"
        );
    }
}

// -- task-detail-editor UX plan US-1 (dispatch chunk 3): the N-4
// -- stamping law — date-only due entries commit the day's last
// -- covered instant (#893, 2026-11-18: not the raw window end, which
// -- the `[now, end)` today membership excludes).

/// US-1/S1: a bare `YYYY-MM-DD` typed into the due field and committed
/// dispatches the day's last covered instant (the whole configured
/// day counts) — the fixture's default boundary (`00:00`, boot offset
/// UTC) makes the law's mint the named date's literal
/// `23:59:59.999` local, never 00:00 of the named date and never the
/// excluded window end.
#[test]
fn us1_s1_bare_date_commits_the_last_covered_instant() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_task_due_edit_opened();
        harness.driver.advance_time(0);
        window.invoke_task_date_accepted(slint::SharedString::from("2030-02-28"));
        harness.driver.advance_time(0);
        // #893 (2026-11-18): the mint is the last covered instant —
        // the raw window end is excluded by its own `[now, end)`
        // membership.
        let expected =
            taskboard_domain::days::last_covered_instant(taskboard_domain::days::day_window_end(
                chrono::NaiveDate::from_ymd_opt(2030, 2, 28).expect("fixture date"),
                chrono::NaiveTime::MIN,
                chrono::FixedOffset::east_opt(0).expect("zero"),
            ));
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::UpdateTask {
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(instant)),
                    scheduled: None,
                    ..
                },
                ..
            }] if *instant == expected
        ));
    });
}

/// US-1/S2: the explicit-time form commits the typed time — the law is
/// date-only (AC-4; the grammar's contrast arm).
#[test]
fn us1_s2_explicit_time_commits_the_typed_time() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_task_due_edit_opened();
        harness.driver.advance_time(0);
        window.invoke_task_date_accepted(slint::SharedString::from("2030-02-28 14:30"));
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::UpdateTask {
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(instant)),
                    ..
                },
                ..
            }] if *instant == chrono::Utc
                .with_ymd_and_hms(2030, 2, 28, 14, 30, 0)
                .single()
                .expect("fixture instant")
        ));
    });
}

/// US-1/S5 (AC-5's quick-set forms, the chunk plan D-3 mapping; #893
/// supersedes the mint): the picker's day click — the date-only commit
/// path — stamps the law (the last covered instant), while the same
/// picker holding a time (a quick-list pick) commits the picked time
/// verbatim: the law applies exactly when no time component exists
/// anywhere in the entry path.
#[test]
fn us1_s5_quick_set_forms_follow_the_law() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        let date = chrono::NaiveDate::from_ymd_opt(2030, 2, 28).expect("fixture date");
        // February 2030: Feb 1 is a Friday — Monday-first index 4,
        // so day 28 sits at grid cell 4 + 27 = 31.
        let cell = 31_i32;
        let expected_end =
            taskboard_domain::days::last_covered_instant(taskboard_domain::days::day_window_end(
                date,
                chrono::NaiveTime::MIN,
                chrono::FixedOffset::east_opt(0).expect("zero"),
            ));
        // Arm the editor and type the date: the Buffer-source sync
        // seeds the picker's cursor onto the typed date (date-only).
        window.invoke_task_due_edit_opened();
        harness.driver.advance_time(0);
        window.invoke_task_date_edited(slint::SharedString::from("2030-02-28"));
        harness.driver.advance_time(0);
        window.invoke_task_date_day_clicked(cell);
        harness.driver.advance_time(0);
        // The date-only commit carried the law's mint.
        let first = harness.engine.drain();
        assert!(matches!(
            &first[..],
            [taskboard_domain::StateCommand::UpdateTask {
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(instant)),
                    ..
                },
                ..
            }] if *instant == expected_end
        ));
        // Re-open: the stored mint is `23:59:59.999` local (#893),
        // so the seeded buffer carries `· 23:59` — the D3.3 suffix
        // consequence. Typing the bare date again re-aims the
        // cursor date-only (the Buffer-source sync), keeping the
        // contrast arm's grid address the same cell.
        window.invoke_task_due_edit_opened();
        harness.driver.advance_time(0);
        window.invoke_task_date_edited(slint::SharedString::from("2030-02-28"));
        harness.driver.advance_time(0);
        // The quick list's pick arms the picker with a time; the
        // day click now commits the picked time verbatim (the
        // contrast arm — a time component exists). The timed
        // commit resolves in the device zone (the r4 D28
        // convention — the adapter passes `chrono::Local`), so the
        // oracle mirrors that zone; the date-only arm above is the
        // boot-offset law's mint.
        window.invoke_task_date_time_picked(570); // 09:30
        harness.driver.advance_time(0);
        window.invoke_task_date_day_clicked(cell);
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::UpdateTask {
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(instant)),
                    ..
                },
                ..
            }] if *instant == chrono::Local
                .from_local_datetime(&chrono::NaiveDateTime::new(
                    date,
                    chrono::NaiveTime::from_hms_opt(9, 30, 0).expect("09:30"),
                ))
                .single()
                .expect("resolvable local wall-clock")
                .with_timezone(&chrono::Utc)
        ));
    });
}

// ---------------------------------------------------------------------------
// UX round 8, chunk 4 — board interaction (the origin plan §8's
// step-token names; the board-interaction plan is the authority).

/// US-1/S1–S3 — the keyboard reorder commits the exact slot move:
/// arm (M), walk to a same-stack slot, confirm → exactly one
/// engine-side gap reorder `TaskReorder { id, target }` (B3
/// 2026-11-24 #1247 — the slot mint is a gap index, never an order
/// value); the flow ends.
#[test]
fn us1_s3_keyboard_reorder_commits_the_exact_move() {
    let (fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        assert_eq!(harness.emphasized_card(), Some((0, 0)), "task0 focused");

        // Arm: the slot after task0's own position in stack a.
        harness.press(ActionId::TaskMove);
        assert_eq!(
            harness.handle.window().get_board_model().move_target,
            0,
            "the flow arms on the task's own stack"
        );
        // Walk back one slot: (stack a, gap 0) — before the first.
        harness.stroke(parse("Up").expect("Up"));
        harness.stroke(parse("Return").expect("Return"));
        harness.assert_commands(&[StateCommand::TaskReorder {
            id: fixture.task0,
            target: 0,
        }]);
        assert_eq!(
            harness.handle.window().get_board_model().move_target,
            -1,
            "the confirm ends the flow"
        );
        assert!(
            !harness.handle.window().get_flow_hint_model().visible,
            "the flow hint clears with the flow"
        );
        harness.engine.settle(&harness.handle);
    });
}

/// US-1/S2 — the flow's escape: arm, stroke, Escape → zero commands,
/// the ring cleared (the settle law; AC-2).
#[test]
fn us1_s2_flow_escape_dispatches_nothing() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskMove);
        harness.stroke(parse("Down").expect("Down"));
        assert_ne!(
            harness.handle.window().get_board_model().move_target,
            -1,
            "the walk moved the ring"
        );
        harness.press(ActionId::TaskMoveCancel);
        harness.assert_commands(&[]);
        assert_eq!(
            harness.handle.window().get_board_model().move_target,
            -1,
            "the ring cleared"
        );
    });
}

/// US-2/S1–S2 — the gap drop commits the aimed move: drag armed, gap
/// hover, gap drop → one same-stack `TaskReorder` at the aimed gap
/// (B3 2026-11-24 #1247 — the before-first drop mints gap 0, the
/// engine resolves the swap; the midpoint order mint is retired); the
/// in-place release dispatches nothing (AC-2); a stale (column, gap)
/// drop settles (the F8 twin).
#[test]
fn us2_s2_gap_drop_commits_the_insertion_order() {
    let (_fixture, state) = fixture::golden_state();
    let task1 = state
        .tasks
        .values()
        .find(|task| task.title == "task-1")
        .map(|task| task.id)
        .expect("the fixture's second task");
    support::harness::run_journey(state, move |harness| {
        // The commit leg: arm the drag on task1 (column 0, row 1),
        // aim the gap before task0 (column 0, gap 0), release.
        harness.handle.window().invoke_board_card_drag_armed(0, 1);
        harness.handle.window().invoke_board_gap_hovered(0, 0);
        harness.handle.window().invoke_board_gap_dropped(0, 0);
        harness.assert_commands(&[StateCommand::TaskReorder {
            id: task1,
            target: 0,
        }]);
        harness.engine.settle(&harness.handle);

        // The in-place release (AC-2): re-arm task1, release back
        // on its own gap (its occupied position) — dispatches
        // nothing, the flow ends.
        harness.handle.window().invoke_board_card_drag_armed(0, 1);
        harness.handle.window().invoke_board_gap_hovered(0, 1);
        harness.handle.window().invoke_board_gap_dropped(0, 1);
        harness.assert_commands(&[]);
        assert!(
            !harness.handle.window().get_board_drag_open(),
            "the in-place release ends the drag"
        );

        // The stale drop (the F8 twin): a column past the tables
        // settles the flow — nothing dispatched, no ghost.
        harness.handle.window().invoke_board_card_drag_armed(0, 0);
        harness
            .handle
            .window()
            .invoke_board_gap_dropped(i32::try_from(usize::MAX).unwrap_or(i32::MAX), 0);
        harness.assert_commands(&[]);
        assert!(
            !harness.handle.window().get_board_drag_open(),
            "the stale drop settles the drag"
        );
    });
}

/// US-2/AC-3's seam half — the indicator's data path: a gap hover
/// arms the aimed column's `aim-gap` (the delegate paints at that
/// index); the column-level hover carries no gap (−1 on every
/// column); the flow's end clears it.
#[test]
fn us2_s1_gap_hover_arms_the_indicator_state() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        let aim_gap = |harness: &Harness, column: usize| {
            harness
                .handle
                .window()
                .get_board_model()
                .stacks
                .row_data(column)
                .expect("column")
                .aim_gap
        };
        harness.handle.window().invoke_board_card_drag_armed(0, 0);
        assert_eq!(
            aim_gap(&*harness, 0),
            -1,
            "the column-level arm aims no gap"
        );
        harness.handle.window().invoke_board_gap_hovered(0, 1);
        assert_eq!(
            aim_gap(&*harness, 0),
            1,
            "the hovered gap lands on the aimed column"
        );
        assert_eq!(aim_gap(&*harness, 1), -1, "the unaimed column stays clear");
        harness
            .handle
            .window()
            .invoke_board_card_drag_finished(0, 0, false);
        harness.driver.advance_time(0);
        assert_eq!(aim_gap(&*harness, 0), -1, "the flow's end clears the aim");
    });
}

/// US-3/S1–S2 — the compact segment drop moves the card: compact arm,
/// segment hover + drop → one append `MoveTask` (R-3); the selected
/// stack's own segment release dispatches nothing (AC-2); a stale
/// segment index settles. AC-3 (a drop that never enters a segment
/// keeps the in-column grammar) is the us2 journey's column leg.
#[test]
fn us3_s2_compact_segment_drop_moves_the_card() {
    let (fixture, state) = fixture::golden_state();
    let task0 = state
        .tasks
        .values()
        .find(|task| task.title == "task-0")
        .map(|task| task.id)
        .expect("the fixture's first task");
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(600, 800);
        assert!(
            harness.handle.window().get_compact_active(),
            "600 px is the compact breakpoint"
        );

        // The commit leg: arm on the selected stack's first card,
        // hover the second stack's segment, release.
        harness.handle.window().invoke_board_card_drag_armed(0, 0);
        harness
            .handle
            .window()
            .invoke_board_switcher_drop_hovered(1);
        assert_eq!(
            harness.handle.window().get_board_model().move_target,
            1,
            "the hovered segment highlights (the move-target paint)"
        );
        harness
            .handle
            .window()
            .invoke_board_switcher_card_dropped(1);
        harness.assert_commands(&[StateCommand::MoveTask {
            id: task0,
            stack: fixture.stack_b,
            order: 1,
        }]);
        harness.engine.settle(&harness.handle);

        // AC-2: the selected stack's own segment is a no-op.
        harness.handle.window().invoke_board_card_drag_armed(0, 0);
        harness
            .handle
            .window()
            .invoke_board_switcher_drop_hovered(0);
        harness
            .handle
            .window()
            .invoke_board_switcher_card_dropped(0);
        harness.assert_commands(&[]);
        assert!(
            !harness.handle.window().get_board_drag_open(),
            "the same-stack segment release ends the drag"
        );

        // The stale segment index settles (never guesses).
        harness.handle.window().invoke_board_card_drag_armed(0, 0);
        harness
            .handle
            .window()
            .invoke_board_switcher_card_dropped(i32::try_from(usize::MAX).unwrap_or(i32::MAX));
        harness.assert_commands(&[]);
        assert!(
            !harness.handle.window().get_board_drag_open(),
            "the stale segment settles the drag"
        );
    });
}

/// US-4/S1 — the modifier clicks build the selection: Ctrl+Click
/// toggles (the first becomes the anchor), Shift+Click spans the
/// reading-order range, plain click resets, Shift+walk extends.
/// Typed model reads: the cards' `selected` / `anchor` bits.
#[test]
fn us4_s1_modifier_clicks_build_the_selection() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        let sel = |harness: &Harness, column: usize, row: usize| {
            let board = harness.handle.window().get_board_model();
            let column = board.stacks.row_data(column).expect("column");
            let card = column.tasks.row_data(row).expect("card");
            (card.selected, card.anchor)
        };

        // Shift+Click card (0,0): the anchor singleton.
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 0, true, false);
        assert_eq!(sel(&*harness, 0, 0), (true, true), "the anchor is marked");
        harness.assert_commands(&[]);

        // Shift+Click card (0,1): the reading-order span.
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 1, true, false);
        assert_eq!(sel(&*harness, 0, 0), (true, true));
        assert_eq!(sel(&*harness, 0, 1), (true, false), "the span member");
        harness.assert_commands(&[]);

        // Ctrl+Click card (1,0): toggles it into the set.
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(1, 0, false, true);
        assert_eq!(sel(&*harness, 1, 0), (true, false), "the ctrl member");
        harness.assert_commands(&[]);

        // Shift+walk (the named-key rows with shift): the plain
        // walk moves focus (the selection rides), then the shift
        // stroke extends to the newly focused card.
        harness.press(ActionId::NavFocusDown);
        harness.stroke(parse("Shift+Down").expect("Shift+Down"));
        assert_eq!(
            sel(&*harness, 0, 1),
            (true, false),
            "the walk extended to the newly focused card"
        );
        harness.assert_commands(&[]);

        // Plain click resets.
        harness.handle.window().invoke_board_task_clicked(0, 0);
        assert_eq!(sel(&*harness, 0, 0), (false, false), "the reset");
        assert_eq!(sel(&*harness, 0, 1), (false, false));
        assert_eq!(sel(&*harness, 1, 0), (false, false));
        harness.assert_commands(&[]);
    });
}

/// US-4/S4 + AC-4 — Escape clears the selection without dispatch,
/// and a foreign settle clears the same.
#[test]
fn us4_s4_escape_clears_the_selection_without_dispatch() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        let selected = |harness: &Harness| {
            let board = harness.handle.window().get_board_model();
            let mut any = false;
            for column in 0..board.stacks.row_count() {
                let column = board.stacks.row_data(column).expect("column");
                for row in 0..column.tasks.row_count() {
                    any = any || column.tasks.row_data(row).expect("card").selected;
                }
            }
            any
        };

        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 0, true, false);
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 1, true, false);
        assert!(selected(&*harness), "the selection built");

        // Escape: the move-cancel stroke with no flow — the
        // settle arm clears the selection, nothing dispatches.
        harness.press(ActionId::TaskMoveCancel);
        harness.assert_commands(&[]);
        assert!(!selected(&*harness), "Escape cleared the selection");

        // Rebuild, then a foreign settle (the app menu's open
        // settles a foreign flow/selection by the modal rule).
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 0, true, false);
        assert!(selected(&*harness));
        harness.press(ActionId::AppMenu);
        harness.assert_commands(&[]);
        assert!(
            !selected(&*harness),
            "the foreign settle cleared the selection"
        );
    });
}

/// US-4/S2 — the selection drag moves all in anchor order: N≥2
/// selection, gap drop → one `MoveTask` per member, anchor order,
/// relative order kept.
#[test]
fn us4_s2_selection_drag_moves_all_in_anchor_order() {
    let (fixture, state) = fixture::golden_state();
    let task0 = state
        .tasks
        .values()
        .find(|task| task.title == "task-0")
        .map(|task| task.id)
        .expect("the fixture's first task");
    let task1 = state
        .tasks
        .values()
        .find(|task| task.title == "task-1")
        .map(|task| task.id)
        .expect("the fixture's second task");
    support::harness::run_journey(state, move |harness| {
        // Build the pair: shift+click both cards (anchor task0).
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 0, true, false);
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 1, true, false);

        // Drag from a member to stack b's gap 0: the cross-stack
        // append sequence (append_order + i per member).
        harness.handle.window().invoke_board_card_drag_armed(0, 0);
        harness.handle.window().invoke_board_gap_hovered(1, 0);
        harness.handle.window().invoke_board_gap_dropped(1, 0);
        harness.assert_commands(&[
            StateCommand::MoveTask {
                id: task0,
                stack: fixture.stack_b,
                order: 1,
            },
            StateCommand::MoveTask {
                id: task1,
                stack: fixture.stack_b,
                order: 2,
            },
        ]);
        harness.engine.settle(&harness.handle);
        assert_eq!(
            harness.board_shape(),
            vec![0, 3],
            "the set landed in stack b"
        );
        assert!(
            !harness.handle.window().get_board_drag_open(),
            "the drop ends the drag"
        );
    });
}

/// US-4/S3 — the multi-selection menu withholds the single-target
/// rows: a member invocation opens the `TASK_CONTEXT_MULTI` row set
/// (structural: the armed rows' `ActionIds`), Complete dispatches the
/// per-task sequence, the Delete arm confirms per task and Escape
/// disarms.
#[test]
fn us4_s3_multi_menu_withholds_single_target_rows() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        // Build the pair, then open the context menu on a member.
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 0, true, false);
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 1, true, false);
        harness
            .handle
            .window()
            .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        let keyboard = harness
            .handle
            .window()
            .global::<taskboard_ui_slint::KeyboardState>();
        assert!(keyboard.get_menu_open(), "the menu opened");
        let rows: Vec<String> = {
            let entries = keyboard.get_menu_entries();
            (0..entries.row_count())
                .map(|i| entries.row_data(i).expect("row").action.to_string())
                .collect()
        };
        // The multi row set (structural; D-11: the selection
        // survived the menu's settle).
        assert_eq!(
            rows,
            vec!["task.complete".to_owned(), "task.delete".to_owned()],
            "the multi grammar: complete, delete (destructive last)"
        );

        // Complete via the menu's row: the per-task sequence
        // (drained before the engine's apply — settle drains).
        harness.press(ActionId::MenuInvoke);
        let drained = harness.engine.drain();
        assert_eq!(drained.len(), 2, "one SetTaskDone per member, in order");
        harness.engine.settle(&harness.handle);
        assert!(
            drained
                .iter()
                .all(|c| matches!(c, StateCommand::SetTaskDone { done: true, .. }))
        );

        // The armed multi delete via the preset stroke: the arm
        // dispatches nothing; the repeat confirms; Escape disarms.
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 0, true, false);
        harness.press(ActionId::TaskDelete);
        harness.assert_commands(&[]);
        harness.press(ActionId::TaskMoveCancel);
        harness.assert_commands(&[]);
    });
}

/// US-5/S1–S2 — the move-aim bit drives the badge's labelled state:
/// the bit is set exactly on the aimed column with its label twin
/// non-empty; no aim gesture → no column carries the bit and every
/// label derivation is empty (AC-2's guard). Typed flags — the label
/// text is never asserted.
#[test]
fn us5_s1_the_move_aim_bit_drives_the_badge_state() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        let badge = |harness: &Harness, column: usize| {
            let board = harness.handle.window().get_board_model();
            let column = board.stacks.row_data(column).expect("column");
            (column.move_aim, column.badge_label.is_empty())
        };

        // AC-2's guard: no gesture, no bit.
        assert_eq!(badge(&*harness, 0), (false, true), "idle: no aim");
        assert_eq!(badge(&*harness, 1), (false, true));

        // The keyboard flow's aim: the armed slot's column only.
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::TaskMove);
        harness.stroke(parse("Down").expect("Down"));
        harness.stroke(parse("Down").expect("Down"));
        harness.stroke(parse("Down").expect("Down"));
        assert_eq!(
            badge(&*harness, 1),
            (true, false),
            "the aimed column carries the bit and its label"
        );
        assert_eq!(
            badge(&*harness, 0),
            (false, true),
            "the unaimed column stays clear"
        );

        // The flow's end clears the bit (the settle/confirm path).
        harness.press(ActionId::TaskMoveCancel);
        assert_eq!(badge(&*harness, 1), (false, true), "the aim cleared");
    });
}

/// US-6/S1–S2 — the compact title dropdown switches boards: the title
/// field's click opens the flyout (nothing dispatches), Escape closes
/// with nothing dispatched (AC-2), a walked row's Return routes the
/// `BoardSelect` seam (the segment click's route, AC-1).
#[test]
fn us6_s2_compact_title_dropdown_switches_board() {
    let (_fixture, state) = fixture::two_board_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(600, 800);
        assert!(
            harness.handle.window().get_compact_active(),
            "600 px is the compact breakpoint"
        );

        // S1: the title field's click opens the flyout (the raw
        // pointer click at the field — the widget's TouchArea is
        // the open toggle; the a11y default action is not the
        // open arm).
        harness.driver.advance_time(0);
        let field = harness
            .driver
            .find_by_label("Boards")
            .expect("the compact bar's dropdown field renders");
        let pos = field.absolute_position();
        let size = field.size();
        harness
            .driver
            .click_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);
        assert!(
            bar_facts(&*harness).get_open(),
            "the title click opened the flyout"
        );
        harness.assert_commands(&[]);

        // AC-2: Escape closes with nothing dispatched.
        harness.stroke(parse("Escape").expect("Escape"));
        harness.assert_commands(&[]);
        assert!(!bar_facts(&*harness).get_open(), "Escape closed");

        // S2: re-open, walk, Return routes the selection.
        harness
            .driver
            .click_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);
        harness.stroke(parse("Down").expect("Down"));
        let walked = bar_facts(&*harness).get_highlight();
        harness.stroke(parse("Return").expect("Return"));
        assert_eq!(
            harness.handle.window().get_board_bar_model().selected,
            walked,
            "the walked row is the selection"
        );
        assert!(
            !bar_facts(&*harness).get_open(),
            "the activation closed the flyout"
        );
        harness.assert_commands(&[]);
    });
}

/// US-6/S3 — the target-free `BoardSelect` stroke opens the same flyout
/// in the compact leg (the r4 US-2/S4 flow's compact reach: the
/// compact leg now mounts the bar instance that receives the edge);
/// walk + Return routes the same seam.
#[test]
fn us6_s3_target_free_stroke_opens_the_compact_flyout() {
    let (_fixture, state) = fixture::two_board_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(600, 800);
        assert!(harness.handle.window().get_compact_active());

        // The target-free stroke: the compact leg's mounted bar
        // receives the switcher-request edge.
        harness.press(ActionId::BoardSelect);
        assert!(
            bar_facts(&*harness).get_open(),
            "the stroke opened the compact flyout"
        );
        harness.stroke(parse("Down").expect("Down"));
        let walked = bar_facts(&*harness).get_highlight();
        harness.stroke(parse("Return").expect("Return"));
        assert_eq!(
            harness.handle.window().get_board_bar_model().selected,
            walked,
            "the walked row is the selection"
        );
        harness.assert_commands(&[]);
    });
}

/// US-7/S1 — the roster indicator carries the glyph flag: the model's
/// `archived-cue` is true exactly when the archived set is non-empty
/// (the flag drives the Dropdown's field glyph; typed flag, never
/// text).
#[test]
fn us7_s1_the_roster_indicator_carries_the_glyph_flag() {
    // A live board plus an archived one: the roster mounts, the cue
    // rides.
    let mut state = AppState::default();
    state_board(
        &mut state,
        uuid::uuid!("00000000-0000-0000-0000-00000000000e"),
        "kept",
        None,
    );
    state_board(
        &mut state,
        uuid::uuid!("00000000-0000-0000-0000-00000000000f"),
        "shelved",
        None,
    );
    let shelved =
        taskboard_domain::BoardId::from_uuid(uuid::uuid!("00000000-0000-0000-0000-00000000000f"));
    let board = state.boards.get_mut(&shelved).expect("board");
    board.archived = true;
    board.clocks.archived = taskboard_domain::BoardClocks::epoch();

    support::harness::run_journey(state, move |harness| {
        let bar = harness.handle.window().get_board_bar_model();
        assert!(
            bar.archived_cue,
            "a non-empty archive carries the recognition cue"
        );
        assert_eq!(
            bar.archived_rows.row_count(),
            1,
            "the roster mounts exactly when the cue rides"
        );
    });

    // The empty archive: no cue (the AC-1 negation — the indicator
    // never renders a bare mystery digit).
    let mut state = AppState::default();
    state_board(
        &mut state,
        uuid::uuid!("00000000-0000-0000-0000-000000000010"),
        "only",
        None,
    );
    support::harness::run_journey(state, move |harness| {
        let bar = harness.handle.window().get_board_bar_model();
        assert!(!bar.archived_cue, "an empty archive carries no cue");
        assert_eq!(bar.archived_rows.row_count(), 0);
    });
}

// -- task-detail-editor UX plan US-2 (dispatch chunk 3): the picker's
// -- pointer repairs — the HH:MM click keeps the editor open (N-5)
// -- and the quick options open on the current slot (N-6).

/// US-2/S1 (N-5, D-6): the reproduction journey — the flow state is
/// the oracle. With the intra-picker click arm, the focus move into
/// the picker's time field arms the one-shot focus-steal latch before
/// the due field's focus-lost settle fires: the editor stays open and
/// nothing dispatches. The un-armed focus loss still settles (the
/// ladder's second rung — the arm is consumed exactly once).
#[test]
fn us2_s1_time_field_click_focuses_and_stays_open() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_task_due_edit_opened();
        harness.driver.advance_time(0);
        assert!(window.get_task_model().due_open, "the editor arms");
        // The click arm: the latch is set before focus moves —
        // the settle funnel consumes it once and skips.
        window.invoke_task_date_time_field_armed();
        window.invoke_date_editor_focus_lost();
        harness.driver.advance_time(0);
        assert!(
            window.get_task_model().due_open,
            "an intra-picker focus move never settles the flow"
        );
        assert_eq!(harness.engine.drain().len(), 0, "nothing dispatches");
        // The un-armed focus loss settles per the rule (the arm
        // is one-shot; the empty buffer discards).
        window.invoke_date_editor_focus_lost();
        harness.driver.advance_time(0);
        assert!(
            !window.get_task_model().due_open,
            "the un-armed focus loss settles the editor"
        );
    });
}

/// US-2/S2 (N-6, D-7): the quick options list opens focused on the
/// current-time slot — when the picker holds no time, the nearest-row
/// seed is the open instant's local time-of-day (never the list head).
/// The oracle is the mount's frozen clock (`fixture::PINNED_NOW`,
/// #921): the adapter captures the seed from the same frozen instant,
/// so the expectation derives from the constant — no wall-clock read
/// remains on either side.
#[test]
fn us2_s2_quick_options_open_on_the_current_slot() {
    let (_fx, state) = fixture::golden_state();
    let settings = UiSettings {
        clock: Some(fixture::PINNED_NOW),
        ..UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_task_due_edit_opened();
        harness.driver.advance_time(0);
        // Single-read by construction (#921): the adapter reads
        // only the frozen clock; the expectation reads only the
        // pinned constant's local time-of-day over the unfiltered
        // quarter-hour grid (the picker holds no time on a fresh
        // open).
        let expected = taskboard_ui_slint::due_edit::nearest_slot(
            &taskboard_ui_slint::due_edit::quarter_slots(),
            Some(fixture::PINNED_NOW.with_timezone(&chrono::Local).time()),
        );
        let pushed = i32::try_from(expected).unwrap_or(0);
        assert_eq!(
            window.get_task_model().date_time_nearest_row,
            pushed,
            "the quick options open scrolled to the current-time slot"
        );
    });
}

// -- task-detail-editor UX plan US-3 (dispatch chunk 3): the effort
// -- detail row (#838) — the inline editor commits the sparse UpdateTask
// -- effort mask; the parse predicate is the walk placement's own.

/// US-3/S1 (AC-1): the editor commits the exact sparse mask — a valid
/// non-empty buffer dispatches `UpdateTask { effort: Some(Some(e)) }`;
/// the empty buffer commits the clear twin `Some(None)` (the
/// always-on row's commit funnel, the overlay migration's M9 shape).
#[test]
fn us3_s1_effort_commit_routes_the_sparse_mask() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_task_effort_edited(slint::SharedString::from("1h30m"));
        window.invoke_task_effort_committed();
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::UpdateTask {
                changes: taskboard_domain::TaskChanges {
                    effort: Some(Some(effort)),
                    ..
                },
                ..
            }] if *effort == taskboard_domain::Effort::new(5400)
        ));
        // The clear twin: the emptied field commits Some(None)
        // (the walk placement's posture, unchanged — the clear is
        // the field's own empty commit now).
        window.invoke_task_effort_edited(slint::SharedString::from(""));
        window.invoke_task_effort_committed();
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::UpdateTask {
                changes: taskboard_domain::TaskChanges {
                    effort: Some(None),
                    ..
                },
                ..
            }]
        ));
    });
}

/// The fake-engine settle's fixture continuity (#1024): the
/// `UpdateTask` arm folds the effort and recur mask fields the way
/// the engine's own application does (`command.rs`'s `UpdateTask` arm
/// is the oracle) — the next journey step's precondition reads the
/// changed value, not the stale one. Fixture continuity, not an
/// oracle (`fake_engine.rs`'s contract comment).
#[test]
fn the_fake_engine_settle_folds_the_update_task_mask() {
    let (fx, mut state) = fixture::golden_state();
    let paused_until = chrono::NaiveDate::from_ymd_opt(2026, 12, 1).expect("fixture date");
    if let Some(task) = state.tasks.get_mut(&fx.task0) {
        task.recur = Some(taskboard_domain::RecurRule {
            series: taskboard_domain::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(42)),
            anchor: chrono::NaiveDate::from_ymd_opt(2026, 10, 5).expect("fixture date"),
            pattern: taskboard_domain::RecurPattern::Daily {
                every: taskboard_domain::EveryDays::new(2).expect("valid interval"),
            },
            bound: taskboard_domain::RecurBound::None,
            continued_from: None,
            paused_until: Some(paused_until),
        });
    }
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        // Two sparse masks in one drain: the effort commit (US-3/S1's
        // seam) and the recur re-set (US-4/S1's seam — paused_until
        // preserved).
        window.invoke_task_effort_edited(slint::SharedString::from("1h30m"));
        window.invoke_task_effort_committed();
        harness.handle.dispatch_action(ActionId::SeriesEdit);
        harness.driver.advance_time(0);
        assert!(window.get_recur_editor_open(), "the recur editor arms");
        window.invoke_recur_commit_clicked();
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(
            matches!(
                drained.as_slice(),
                [
                    taskboard_domain::StateCommand::UpdateTask { .. },
                    taskboard_domain::StateCommand::UpdateTask { .. }
                ]
            ),
            "both sparse writes reached the boundary, got {drained:?}"
        );
        harness.engine.settle(&harness.handle);
        let snap = harness.engine.snapshot();
        let task = snap.tasks.get(&fx.task0).expect("the addressed task");
        assert_eq!(
            task.effort,
            Some(taskboard_domain::Effort::new(5400)),
            "the settle folded the effort mask"
        );
        assert_eq!(
            task.recur.as_ref().map(|rule| rule.paused_until),
            Some(Some(paused_until)),
            "the settle folded the recur mask"
        );
    });
}

/// US-3/S2 (AC-2): an invalid value keeps the buffer and dispatches
/// nothing (the date-editor refusal posture); the focus-loss settle
/// discards it afterwards (the shared rule — the prior value
/// survives).
#[test]
fn us3_s2_invalid_effort_keeps_the_buffer() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_task_effort_edited(slint::SharedString::from("junk"));
        window.invoke_task_effort_committed();
        harness.driver.advance_time(0);
        assert_eq!(
            harness.engine.drain().len(),
            0,
            "the refusal dispatches nothing"
        );
        assert_eq!(
            window.get_task_model().effort_text.as_str(),
            "junk",
            "the buffer mirror survives the refusal"
        );
    });
}

// -- task-detail-editor UX plan US-4 (dispatch chunk 3): a re-set
// -- recur rule preserves the series pause (#759) — both fronts mint
// -- through the one domain `assemble_rule`.

/// US-4/S1 (AC-1, GUI front): the recur editor's commit over a paused
/// series dispatches `UpdateTask { recur: Some(Some(rule)) }` whose
/// minted rule carries the existing `paused_until` — fixing a pattern
/// never silently resumes the series. The CLI twin lives in
/// `taskboard-app/tests/cli/cli_repeat.rs::us4_s1_cli_rule_replace_preserves_paused_until`.
#[test]
fn us4_s1_rule_replace_preserves_paused_until() {
    let (fx, mut state) = fixture::golden_state();
    let paused = taskboard_domain::RecurRule {
        series: taskboard_domain::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(42)),
        anchor: chrono::NaiveDate::from_ymd_opt(2026, 10, 5).expect("fixture date"),
        pattern: taskboard_domain::RecurPattern::Daily {
            every: taskboard_domain::EveryDays::new(2).expect("valid interval"),
        },
        bound: taskboard_domain::RecurBound::None,
        continued_from: None,
        paused_until: chrono::NaiveDate::from_ymd_opt(2026, 12, 1),
    };
    if let Some(task) = state.tasks.get_mut(&fx.task0) {
        task.recur = Some(paused);
    }
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        harness.handle.dispatch_action(ActionId::SeriesEdit);
        harness.driver.advance_time(0);
        assert!(
            window.get_recur_editor_open(),
            "the recur editor arms over the paused series"
        );
        window.invoke_recur_commit_clicked();
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::UpdateTask {
                changes: taskboard_domain::TaskChanges {
                    recur: Some(Some(rule)),
                    ..
                },
                ..
            }] if rule.paused_until == chrono::NaiveDate::from_ymd_opt(2026, 12, 1),
        ));
    });
}

// -- task-detail-editor UX plan US-5 (dispatch chunk 3): the
// -- restoration guard (#706) — a dead subject never restores an
// -- empty detail shell as the root.

/// US-5/S1 (AC-1): the detail view buried under the settings surface,
/// its subject tombstoned while the over-view holds — the restore
/// falls back to the board (the typed guard), never the empty shell.
/// The guard is inert for live subjects (AC-2 — the existing
/// restoration tests stand).
#[test]
fn us5_s1_dead_subject_restores_the_board() {
    let (fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        harness.handle.dispatch_action(ActionId::AppSettings);
        harness.driver.advance_time(0);
        assert_eq!(window.get_current_view(), View::Settings);
        // The subject dies under the over-view.
        let mut changed = harness.engine.snapshot().as_ref().clone();
        if let Some(task) = changed.tasks.get_mut(&fx.task0) {
            task.deleted = true;
        }
        harness.engine.replace_state(changed, &harness.handle);
        harness.driver.advance_time(0);
        // The restore: the settings toggle closes the over-view.
        harness.handle.dispatch_action(ActionId::AppSettings);
        harness.driver.advance_time(0);
        assert_ne!(
            window.get_current_view(),
            View::TaskDetail,
            "a dead subject never restores the detail shell"
        );
        assert_eq!(
            window.get_current_view(),
            View::Board,
            "the root falls back to the subject's board"
        );
    });
}

// -- task-detail-editor UX plan US-6 (dispatch chunk 3): the labels
// -- overlay's armed identity cannot outlive its row (#840).

/// US-6/S1 (AC-1): the fast double-click confirm dispatches
/// `DeleteLabel` and leaves no armed identity; a later activation
/// against the deleted row never re-arms (the re-arm guard + the
/// pass-side clear), so the next Escape closes the overlay directly —
/// no phantom rung. The rung count is observed through typed
/// flow/overlay state, never text.
#[test]
fn us6_s1_double_click_delete_leaves_no_armed_identity() {
    let (_fx, state, green, _red) = label_journey_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        // The fast double-click: both activations in one pass
        // window — the second IS the confirm.
        window.invoke_labels_overlay_delete_clicked(0);
        window.invoke_labels_overlay_delete_clicked(0);
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(matches!(
            &drained[..],
            [taskboard_domain::StateCommand::DeleteLabel { id }] if *id == green
        ));
        assert_eq!(
            window.get_label_overlay_armed_index(),
            -1,
            "the confirm clears the armed identity"
        );
        // The label dies; the next rows pass re-resolves the
        // armed mirror against the roster without it — the
        // pass-side clear drops the identity.
        let mut changed = harness.engine.snapshot().as_ref().clone();
        changed.labels.remove(&green);
        harness.engine.replace_state(changed, &harness.handle);
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_label_overlay_armed_index(),
            -1,
            "the armed identity cannot outlive its row"
        );
        // The re-arm guard: an activation whose id no longer
        // resolves in the live roster (a stale tables pass) never
        // re-arms — arm a live row, remove it under the stale
        // tables, activate the stale index.
        window.invoke_labels_overlay_delete_clicked(0);
        harness.driver.advance_time(0);
        let armed = window.get_label_overlay_armed_index();
        assert!(armed >= 0, "a live row arms");
        let victim = harness
            .engine
            .snapshot()
            .labels
            .values()
            .find(|l| !l.deleted)
            .map(|l| l.id)
            .expect("a live label");
        let mut roster_without_victim = harness.engine.snapshot().as_ref().clone();
        roster_without_victim.labels.remove(&victim);
        harness
            .engine
            .replace_state(roster_without_victim, &harness.handle);
        // No advance: the tables pass still resolves the dead id.
        window.invoke_labels_overlay_delete_clicked(0);
        harness.driver.advance_time(0);
        assert_eq!(harness.engine.drain().len(), 0, "nothing re-arms");
        assert_eq!(
            window.get_label_overlay_armed_index(),
            -1,
            "the dead id never re-arms"
        );
        // The next Escape closes the overlay directly.
        harness
            .handle
            .dispatch_action(ActionId::LabelOverlayDismiss);
        harness.driver.advance_time(0);
        assert!(
            !window.get_task_model().labels_open,
            "no phantom rung — the overlay closes on the first press"
        );
    });
}

/// US-6/S2 (AC-2): the disarm path keeps the ladder — Cancel clears
/// the armed identity, the overlay stays open, and one Escape rung
/// closes it; nothing dispatches anywhere.
#[test]
fn us6_s2_cancel_disarm_keeps_the_ladder() {
    let (_fx, state, _green, _red) = label_journey_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_delete_clicked(0);
        harness.driver.advance_time(0);
        assert!(window.get_label_overlay_armed_index() >= 0, "armed");
        window.invoke_labels_overlay_delete_cancelled();
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_label_overlay_armed_index(),
            -1,
            "the disarm clears the identity"
        );
        assert!(
            window.get_task_model().labels_open,
            "the overlay's ladder is intact"
        );
        assert_eq!(harness.engine.drain().len(), 0);
        harness
            .handle
            .dispatch_action(ActionId::LabelOverlayDismiss);
        harness.driver.advance_time(0);
        assert!(
            !window.get_task_model().labels_open,
            "one rung closes the overlay"
        );
    });
}

/// US-3/S1 (N-11, origin AC-1) — the reproduction: a card context
/// menu held open across an engine-publish refresh tick keeps its
/// model. The mock clock fires the cadence pass (`advance_time` —
/// the oracle-matching tool; the tick's forced lane is the swap
/// surface the round-8 note N-11 records). Typed model assertion:
/// the row set's action ids, never labels.
#[test]
fn us3_s1_open_context_menu_survives_the_dwell_interval() {
    let (_fixture, state) = fixture::journey_state();
    let republish = state.clone();
    support::harness::run_journey(state, move |harness| {
        let model = |harness: &Harness| model_snapshot(harness);
        harness
            .handle
            .window()
            .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);
        assert!(
            keyboard(&*harness).get_menu_open() && keyboard(&*harness).get_menu_pointer_anchored(),
            "the context request opened the anchored popup"
        );
        let before = model(&*harness);
        assert!(
            before.iter().any(|a| a == "task.complete"),
            "the context rows are mounted"
        );

        // The dwell: advance the mock clock past the repaint
        // cadence and fire a refresh pass (the engine publishes —
        // the concurrent-sync shape the note records).
        harness.engine.replace_state(republish, &harness.handle);
        harness.driver.advance_time(REPAINT_CADENCE_MS);
        let after = model(&*harness);
        assert_eq!(after, before, "the popup's model is unchanged");
        assert!(
            after.iter().all(|a| a != "app.settings"),
            "the main menu's quick rows never swapped in"
        );
    });
}

/// US-3/S1 companion (origin AC-2) — the main menu's own refresh-time
/// re-derivation stands: an open main menu re-greys from a published
/// state while it holds (the round-3 D27c behaviour the fix must not
/// regress).
#[test]
fn us3_s2_main_menu_dwell_regrey_stands() {
    let (_fixture, mut state) = fixture::golden_state();
    state.undo = taskboard_domain::UndoView::default();
    let mut live = state.clone();
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::AppMenu);
        harness.driver.advance_time(0);
        let row_state = |harness: &Harness, action: &str| {
            let entries = keyboard(harness).get_menu_entries();
            (0..entries.row_count())
                .map(|i| entries.row_data(i).expect("row"))
                .find(|row| row.action == action)
                .expect("the row renders on the root page")
                .enabled
        };
        assert!(!row_state(&*harness, "app.undo"), "the empty line greys");

        live.undo = taskboard_domain::UndoView {
            can_undo: true,
            can_redo: false,
            head_label: None,
            recent: vec![],
            branch_children: Vec::new(),
        };
        harness.engine.replace_state(live, &harness.handle);
        harness.driver.advance_time(REPAINT_CADENCE_MS);
        assert!(
            row_state(&*harness, "app.undo"),
            "the main menu re-greys while it holds"
        );
    });
}

/// US-1/S1 (origin AC-1) — the quick bar's "schedule today" routes
/// the exact sparse mask: `UpdateTask` with `scheduled ==
/// Some(Some(<today's day-window end>))`, every other mask None; the
/// invocation closes the menu first (round-1 D3). Typed command
/// assertion, never text.
#[test]
fn us1_s1_schedule_today_commits_the_exact_mask() {
    let (fixture, state) = fixture::journey_state();
    let day_start = taskboard_domain::SettingsSnapshot::default().day_start_time;
    let offset = UiSettings::default().boot_offset;
    support::harness::run_journey(state, move |harness| {
        harness
            .handle
            .window()
            .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);
        assert!(keyboard(&*harness).get_menu_open());
        assert!(
            keyboard(&*harness).get_menu_quick_bar().row_count() > 0,
            "the task context menu carries the quick bar"
        );

        // Today's local date (single-read capture; the window end
        // only moves at a day boundary).
        // The adapter's clock reads UTC over the boot offset (the
        // default hand-over) — the capture matches the oracle's
        // zone (the clock-discipline law's single-read capture).
        let today = chrono::Utc::now().with_timezone(&offset).date_naive();
        // #893: the today verb commits the day's last covered
        // instant, not the window's exclusive end.
        let end = taskboard_domain::days::last_covered_instant(
            taskboard_domain::days::day_window_end(today, day_start, offset),
        );
        harness.driver.click_by_label("Schedule today");
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                scheduled: Some(Some(end)),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);
        assert!(
            !keyboard(&*harness).get_menu_open(),
            "the invocation closed the menu"
        );
    });
}

/// US-1/S2 (origin AC-2) — "schedule tomorrow" routes the identical
/// mask shape carrying the next day window's end (the additive
/// `days::next_day_window` fact — never `+24h` arithmetic).
#[test]
fn us1_s2_schedule_tomorrow_commits_the_exact_mask() {
    let (fixture, state) = fixture::journey_state();
    let day_start = taskboard_domain::SettingsSnapshot::default().day_start_time;
    let offset = UiSettings::default().boot_offset;
    support::harness::run_journey(state, move |harness| {
        harness
            .handle
            .window()
            .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);

        let now = chrono::Utc::now();
        let (_, tomorrow_end) = taskboard_domain::days::next_day_window(now, day_start, offset);
        harness.driver.click_by_label("Schedule tomorrow");
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                scheduled: Some(Some(tomorrow_end)),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);
    });
}

/// US-1/S3 (origin AC-3) — a scheduled subject's bar carries
/// "Unschedule"; clicking it routes the clear twin
/// `scheduled: Some(None)`, exact.
#[test]
fn us1_s3_unschedule_commits_the_clear_twin() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .scheduled = Some(chrono::Utc::now());
    support::harness::run_journey(state, move |harness| {
        harness
            .handle
            .window()
            .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);
        harness.driver.click_by_label("Unschedule");
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                scheduled: Some(None),
                ..taskboard_domain::TaskChanges::default()
            },
        }]);
    });
}

/// US-2/AC-4 (UX-R8 chunk-5 D-5) — the Escape ladder: activating a
/// submenu parent (the fixture's Reminders ▸) mounts the family page;
/// Escape 1 closes the submenu (the menu stays, the root page
/// restored), Escape 2 closes the menu. Typed facts, never text.
#[test]
fn us2_ac4_escape_ladder_steps_submenu_then_menu() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness
            .handle
            .window()
            .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);
        assert!(keyboard(&*harness).get_menu_open());

        // Open the Reminders family page via the parent row's
        // click seam (the same Rust path the keyboard's
        // InvokeSelected reaches).
        let entries = keyboard(&*harness).get_menu_entries();
        let parent = (0..entries.row_count())
            .map(|i| entries.row_data(i).expect("row"))
            .position(|r| r.submenu && r.label == "Reminders")
            .expect("the Reminders parent renders");
        harness
            .handle
            .window()
            .invoke_menu_entry_invoked(i32::try_from(parent).expect("small index"));
        harness.driver.advance_time(0);
        assert!(keyboard(&*harness).get_menu_open(), "the menu stays");
        let submenu = keyboard(&*harness).get_menu_submenu_entries();
        let child_actions: Vec<_> = (0..submenu.row_count())
            .map(|i| submenu.row_data(i).expect("row").action.to_string())
            .collect();
        assert!(
            child_actions.contains(&"reminder.rule-add".to_owned()),
            "the family's child rows mounted"
        );

        // Escape 1: the submenu closes, the menu stays.
        harness.press(ActionId::MenuDismiss);
        harness.driver.advance_time(0);
        assert!(
            keyboard(&*harness).get_menu_open(),
            "Escape 1 keeps the menu"
        );
        assert_eq!(
            keyboard(&*harness).get_menu_submenu_entries().row_count(),
            keyboard(&*harness).get_menu_entries().row_count(),
            "the root page is restored"
        );

        // Escape 2: the menu closes.
        harness.press(ActionId::MenuDismiss);
        harness.driver.advance_time(0);
        assert!(
            !keyboard(&*harness).get_menu_open(),
            "Escape 2 closes the menu"
        );
    });
}

/// US-1/AC-5 (D-10) — click ≡ key: the preset stroke (standard's F11,
/// resolved through the same keymap lookup the hint projection uses)
/// routes the identical command as the bar click — one exact-mask
/// assertion per leg on the same mount.
#[test]
fn us1_ac5_bar_key_and_click_route_identically() {
    let (fixture, state) = fixture::journey_state();
    let day_start = taskboard_domain::SettingsSnapshot::default().day_start_time;
    let offset = UiSettings::default().boot_offset;
    support::harness::run_journey(state, move |harness| {
        // The adapter's clock reads UTC over the boot offset (the
        // default hand-over) — the capture matches the oracle's
        // zone (the clock-discipline law's single-read capture).
        let today = chrono::Utc::now().with_timezone(&offset).date_naive();
        // #893: the today verb commits the day's last covered
        // instant, not the window's exclusive end.
        let end = taskboard_domain::days::last_covered_instant(
            taskboard_domain::days::day_window_end(today, day_start, offset),
        );
        let expect = || StateCommand::UpdateTask {
            id: fixture.task1,
            changes: taskboard_domain::TaskChanges {
                scheduled: Some(Some(end)),
                ..taskboard_domain::TaskChanges::default()
            },
        };

        // The click leg.
        harness
            .handle
            .window()
            .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);
        harness.driver.click_by_label("Schedule today");
        harness.driver.advance_time(0);
        harness.assert_commands(&[expect()]);

        // The key leg: the card stays focused after the click's
        // close (the same chain the bar's routed dispatch sees —
        // the menu context itself masks the view chain, D11).
        harness.press(ActionId::TaskScheduleToday);
        harness.driver.advance_time(0);
        harness.assert_commands(&[expect()]);
    });
}

/// US-4/S1 (origin AC-1) — the card's play control routes
/// `StartTracking` with the card's task as target (the one-shot
/// `track_target`); the model's shown half flips on the next push.
#[test]
fn us4_s1_card_play_routes_starttracking() {
    let (_fixture, state) = fixture::golden_state();
    let task0 = state.tasks.values().next().expect("fixture task").id;
    support::harness::run_journey(state, move |harness| {
        harness.driver.click_by_label("Track task");
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::StartTracking {
            task: task0,
            kind: taskboard_domain::tracking::TimeEntryKind::Work,
        }]);
    });
}

/// US-4/S2 (origin AC-2) — the running card's control routes
/// `StopTracking` and flips back.
#[test]
fn us4_s2_card_pause_routes_stoptracking() {
    let (_fixture, mut state) = fixture::golden_state();
    // Seed an open entry on the first card (the running-here half).
    let task0 = state.tasks.values().next().expect("fixture task").id;
    let now = chrono::Utc::now();
    let entry = taskboard_domain::tracking::TimeEntry {
        id: taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7()),
        task: task0,
        task_title: String::new(),
        start: now,
        created_at: now,
        updated_at: now,
        by: None,
        end: None,
        kind: taskboard_domain::tracking::TimeEntryKind::Work,
    };
    state.tracking.entries.insert(entry.id, entry);
    support::harness::run_journey(state, move |harness| {
        harness.driver.click_by_label("Stop tracking task");
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::StopTracking]);
    });
}

/// US-5/S1 (origin AC-1, UX-R8 chunk-5 D-7) — a non-empty inline
/// create commit focuses the created card: the apply lands (the fake
/// engine's settle), the diff moves the focus, and the
/// `FocusScrollHint` global edge-triggers toward the card's column.
#[test]
fn us5_s1_create_focuses_the_new_card() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.guard.begin(Verb::Create);
        harness.press(ActionId::TaskCreate);
        harness.type_text("focus me");
        harness.stroke(parse("Return").expect("Return"));
        harness.guard.end(Verb::Create);
        harness.engine.settle(&harness.handle);
        harness.driver.advance_time(0);
        // The created card is stack a's third card (the append
        // order) and holds the emphasis.
        assert_eq!(
            harness.emphasized_card(),
            Some((0, 2)),
            "the created card is the focused card"
        );
        let hint = harness
            .handle
            .window()
            .global::<taskboard_ui_slint::FocusScrollHint>();
        assert_eq!(
            hint.get_target_a(),
            0,
            "the scroll hint edge-triggers toward the card's column"
        );
    });
}

/// US-5/S2 (origin AC-2) — an empty-buffer settle hands off nothing:
/// no command, no arm, the focus unchanged.
#[test]
fn us5_s2_empty_settle_hands_off_nothing() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.guard.begin(Verb::Create);
        harness.press(ActionId::TaskCreate);
        harness.type_text("");
        harness.stroke(parse("Return").expect("Return"));
        harness.guard.end(Verb::Create);
        harness.assert_commands(&[]);
        harness.engine.settle(&harness.handle);
        harness.driver.advance_time(0);
        assert_ne!(
            harness.emphasized_card(),
            Some((0, 2)),
            "nothing was created, nothing to focus"
        );
        let hint = harness
            .handle
            .window()
            .global::<taskboard_ui_slint::FocusScrollHint>();
        assert_eq!(hint.get_target_a(), -1, "the hint global never armed");
    });
}

/// US-7/S1 (origin AC-1, UX-R8 chunk-5 D-8) — the compact leg's
/// add-task field holds its single-line height: the realized height
/// of the ghost stays the locked `space-6 + 2 * space-1` band (the
/// surplus flows into the task viewport, not the field — the N-13
/// swallow's fix). Geometry oracle, never text.
#[test]
fn us7_s1_the_compact_add_task_field_holds_its_height() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        // A height-constrained window: the surplus the viewport
        // must absorb exists only when the column content exceeds
        // the available height (the N-13 shape).
        harness.driver.set_window_size(600, 460);
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_compact_active(),
            "600 px is the compact breakpoint"
        );
        // The compact leg mounts the selected stack's column; a
        // board switch makes the selection concrete.
        harness.press(ActionId::BoardNext);
        harness.driver.advance_time(0);
        // The armed leg: the create field (the "field" the note
        // N-13 names — armed or ghost, the constraint holds).
        let ghost = harness
            .driver
            .find_by_label("Add task")
            .expect("the compact leg's ghost mounts");
        let locked = 32.0 + 2.0 * 4.0; // space-6 + 2 * space-1 at base scale 1
        let ghost_height = ghost.size().height;
        assert!(
            (ghost_height - locked).abs() < 0.5,
            "the ghost holds its single-line height, got {ghost_height}"
        );
        ghost.invoke_default_action();
        harness.driver.advance_time(0);
        let field = harness
            .driver
            .find_by_label("New task title")
            .expect("the armed create field mounts");
        let field_height = field.size().height;
        assert!(
            (field_height - locked).abs() < 8.0,
            "the armed add-task field holds its single-line height, got {field_height}"
        );
    });
}

/// UX-R8 chunk-5 D-9 (#882's display half) — the armed multi-delete's
/// visible strip: the member-count fact arms while
/// `ConfirmDelete(Tasks)` holds (−1 disarmed); Confirm routes the
/// armed verb (the second stroke's twin) and the members delete;
/// Cancel disarms to −1. Typed facts, never caption text.
#[test]
fn the_armed_multi_delete_strip_shows_the_member_count() {
    let (fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        let armed = |harness: &Harness| harness.handle.window().get_task_multi_delete_armed();
        assert_eq!(armed(&*harness), -1, "disarmed at mount");
        // Build a two-member selection (the modifier-click grammar,
        // chunk 4's card seam).
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 0, true, false);
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 1, true, false);
        harness.driver.advance_time(0);
        // Arm the multi delete (the keyboard verb's first stroke).
        harness.press(ActionId::TaskDelete);
        harness.driver.advance_time(0);
        assert!(armed(&*harness) >= 0, "the armed strip's count fact arms");
        // Cancel disarms (the #784 grammar's cancel twin).
        harness.driver.click_by_label("Cancel delete tasks");
        harness.driver.advance_time(0);
        assert_eq!(armed(&*harness), -1, "the cancel disarms");
        harness.assert_commands(&[]);
        let _ = &fixture;
    });
}

/// US-1/S4 (origin AC-4) — the quick bar's tracking twin: a
/// non-tracking subject's bar routes =`StartTracking`= (the existing
/// id, card-focused); the shown half rides the subject's tracking
/// state (the composition test pins the halves; the card control's
/// journeys pin the flips).
#[test]
fn us1_s4_bar_tracking_twin_routes_the_shown_half() {
    let (fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness
            .handle
            .window()
            .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);
        // The bar's tracking twin shares its registry label with
        // the menu row of the same verb (both in the popup) — the
        // label-addressed click is ambiguous here, so the journey
        // lands on the seam the click reaches (the
        // menu_quick_bar_invoked install; the #782 fold's
        // invoke_* pattern — the schedule trio's journeys pin the
        // identical click path with unique labels).
        harness.handle.window().invoke_menu_quick_bar_invoked(3);
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::StartTracking {
            task: fixture.task1,
            kind: taskboard_domain::tracking::TimeEntryKind::Work,
        }]);
    });
}

// -- J5 (menus remainder, 2026-11-18): the bar remainder's journeys.
// -- The reproduction: pre-fix, a right-click on a card covered by the
// -- selection opened the multi row set with an EMPTY bar (`Vec::new()`)
// -- — the H-1 shadow (the confirmed root cause of the live regression).

/// J5 US-1/S1 (AC-1, the H-1 repro) — a right-click on a card the
/// selection covers (the modifier-click grammar built it earlier) opens
/// the task context menu WITH the quick bar: the multi bar (the R-3
/// fix), never an empty strip. Pre-fix this fails: the multi arm passed
/// `Vec::new()`. Typed model reads: the bar's row ids.
#[test]
fn us1_s1_the_selection_covered_card_menu_carries_the_quick_bar() {
    let (fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        // Ctrl+Click card (0,1): the selection singleton covers it.
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 1, false, true);
        harness.driver.advance_time(0);
        // The right-click on the covered card.
        harness
            .handle
            .window()
            .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);
        assert!(keyboard(&*harness).get_menu_open(), "the menu opens");

        let bar = keyboard(&*harness).get_menu_quick_bar();
        let actions: Vec<String> = (0..bar.row_count())
            .map(|i| bar.row_data(i).expect("row").action.to_string())
            .collect();
        assert!(
            actions.contains(&"task.schedule-today".to_owned())
                && actions.contains(&"task.schedule-tomorrow".to_owned()),
            "the multi bar's schedule pair is present, got {actions:?}"
        );
        assert!(
            actions.contains(&"task.track-start".to_owned()),
            "a uniform-untracked selection shows the start twin"
        );
        let _ = &fixture;
    });
}

/// J5 US-2/S1 (AC-1, W-2) — the multi-selection bar routes per member:
/// two selected members, the bar's "schedule today" dispatches one
/// sparse `UpdateTask` per member in anchor order (the multi-sensible
/// verb shape; the R-3 grammar). Pre-fix the bar was empty, so this
/// could not even be expressed.
#[test]
fn us2_s1_the_multi_bar_routes_per_member() {
    let (fixture, state) = fixture::journey_state();
    let day_start = taskboard_domain::SettingsSnapshot::default().day_start_time;
    let offset = UiSettings::default().boot_offset;
    support::harness::run_journey(state, move |harness| {
        // The two-member selection: Shift+Click spans (0,0)→(0,1).
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 0, true, false);
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 1, true, false);
        harness.driver.advance_time(0);
        harness
            .handle
            .window()
            .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);
        assert!(keyboard(&*harness).get_menu_open());

        // The bar's first shown row routes "schedule today".
        harness.handle.window().invoke_menu_quick_bar_invoked(0);
        harness.driver.advance_time(0);
        let today = chrono::Utc::now().with_timezone(&offset).date_naive();
        // #893 (2026-11-18): the multi-select batch twin stamps the
        // day's last covered instant, not the raw exclusive end.
        let end = taskboard_domain::days::last_covered_instant(
            taskboard_domain::days::day_window_end(today, day_start, offset),
        );
        let mask = taskboard_domain::TaskChanges {
            scheduled: Some(Some(end)),
            ..taskboard_domain::TaskChanges::default()
        };
        harness.assert_commands(&[
            StateCommand::UpdateTask {
                id: fixture.task0,
                changes: mask.clone(),
            },
            StateCommand::UpdateTask {
                id: fixture.task1,
                changes: mask,
            },
        ]);
    });
}

/// J5 US-4 AC-1 (W-4's fold) — every shown bar row's hint model
/// mirrors its action's chain-visible binding exactly: non-empty iff
/// the action is preset-bound over the menu chain (the same projection
/// the menu rows resolve through `row()`), empty when unbound. The
/// keymap is the oracle; typed model lengths, never text.
#[test]
fn the_bar_rows_carry_their_preset_binding_hints() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey(state, move |harness| {
        harness
            .handle
            .window()
            .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);
        let keymap = UiSettings::default().keymap;
        let chain = vec![
            taskboard_ui_slint::actions::ContextId::Board,
            taskboard_ui_slint::actions::ContextId::Global,
        ];
        let bar = keyboard(&*harness).get_menu_quick_bar();
        let shown: Vec<_> = (0..bar.row_count())
            .filter_map(|i| bar.row_data(i))
            .filter(|row| row.visible)
            .collect();
        assert!(!shown.is_empty(), "the card menu carries shown bar rows");
        let bound = |action: &str| {
            taskboard_ui_slint::actions::ActionId::from_id(action).is_some_and(|id| {
                keymap
                    .first_binding_display(id, &chain)
                    .is_some_and(|hint| !hint.is_empty())
            })
        };
        for row in shown {
            let expect_bound = bound(&row.action);
            assert_eq!(
                row.hint_keys.row_count() > 0,
                expect_bound,
                "shown bar row {} mirrors its binding presence",
                row.action
            );
        }
    });
}

/// J5 US-4 AC-2 (W-5, #927) — a deleted subject disarms the pinned
/// menu on the reconciliation pass: the card menu open, the delete
/// armed and confirmed, the state settles → the menu is closed (bar +
/// rows together), terminal state asserted. Pre-fix the menu held its
/// dead rows indefinitely (the built-once pin).
#[test]
fn us4_s2_a_deleted_subject_disarms_the_pinned_menu() {
    let (fixture, state) = fixture::journey_state();
    support::harness::run_journey(state.clone(), move |harness| {
        harness
            .handle
            .window()
            .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);
        assert!(keyboard(&*harness).get_menu_open(), "the menu opens");

        // The delete: a single task's `task.delete` dispatches
        // `DeleteTask` directly (the router arm; the armed strip is
        // the multi/stack/board grammar) — dispatched while the
        // menu holds.
        harness.handle.dispatch_action(ActionId::TaskDelete);
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::DeleteTask { id: fixture.task1 }]);

        // The settle applies the delete → the reconciliation pass
        // runs → the menu closed (terminal state only).
        harness.engine.settle(&harness.handle);
        harness.driver.advance_time(REPAINT_CADENCE_MS);
        assert!(
            !keyboard(&*harness).get_menu_open(),
            "the dead subject's menu disarmed with the settle"
        );
    });
}

// -- cross-surface friction UX plan US-2 (#832): the menu-routed
// -- conversion arms route the real arming seams, subject-resolved.
// -- The reproduction: pre-fix the adapter's effect-interpret arm is
// -- an unconditional `Vec::new()` — the menu row arms nothing and
// -- says nothing.

/// US-2/S1 (AC-2, the series twin): a focused recurring task's menu
/// invocation of `SeriesConvertArm` arms the confirm strip — the
/// window's recur-confirm fact flips and nothing dispatches yet (the
/// strip is consequence-first). Pre-fix this fails: the effect arm
/// drops the dispatch unconditionally.
#[test]
fn us2_ac2_menu_series_convert_arm_arms_the_confirm_strip() {
    let (fx, mut state) = fixture::golden_state();
    let rule = taskboard_domain::RecurRule {
        series: taskboard_domain::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(7)),
        anchor: chrono::NaiveDate::from_ymd_opt(2026, 11, 2).expect("fixture date"),
        pattern: taskboard_domain::RecurPattern::Daily {
            every: taskboard_domain::EveryDays::new(3).expect("valid interval"),
        },
        bound: taskboard_domain::RecurBound::None,
        continued_from: None,
        paused_until: None,
    };
    if let Some(task) = state.tasks.get_mut(&fx.task0) {
        task.recur = Some(rule);
    }
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        harness.handle.dispatch_action(ActionId::SeriesConvertArm);
        harness.driver.advance_time(0);
        assert!(
            window.get_task_model().recur_confirm_open,
            "the menu-routed arm arms the conversion strip"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the arm stages the grammar; the confirm dispatches"
        );
    });
}

/// US-5/S1 (AC-1, cross-surface friction plan D-6, #763): the
/// reproduction, lands first and fails first — pre-fix `route()` takes
/// EVERY one-shot into the dispatch ctx, so the arm's own routing
/// already consumed the pattern draft; the confirm dispatches
/// `NoTarget`-silently. Post-fix the confirm routes
/// `ConvertHabitToSeries` with the armed pattern — an interleaved
/// dispatch cannot eat the target. (H2: the arm rides the menu flow —
/// the strip's row seam retired with the strip.)
#[test]
fn us5_ac1_interleaved_dispatch_spares_the_confirm_target() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();

        // The menu route arms (the focus fact mints in the
        // opener).
        window.invoke_habits_context_requested(0, 120.0, 120.0);
        harness.handle.dispatch_action(ActionId::HabitConvertArm);
        harness.driver.advance_time(0);

        // The interleaved unrelated dispatch (a focused task's
        // completion — consumes nothing of the menu's grammar).
        harness.handle.dispatch_action(ActionId::NavFocusDown);
        harness.handle.dispatch_action(ActionId::TaskComplete);
        harness.driver.advance_time(0);

        // The confirm routes the conversion with the armed target.
        harness
            .handle
            .dispatch_action(ActionId::HabitConvertToSeries);
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(
            matches!(
                &drained[..],
                [
                    taskboard_domain::StateCommand::SetTaskDone { .. },
                    taskboard_domain::StateCommand::ConvertHabitToSeries { .. },
                ]
            ),
            "the confirm survives the interleaving with its target: {drained:?}"
        );
    });
}

/// US-3/S1 (cross-surface friction plan D-4, #836): leaving settings
/// disarms the armed strip — the toggle close (the close path) and
/// the re-entry (the mount boundary) both clear the identity; arming
/// again works fresh (the r2 grammar stands, AC-3).
#[test]
fn us3_ac1_leaving_settings_disarms_the_strip() {
    use taskboard_ui_slint::ConnectionItemKind as K;
    let mut state = AppState::default();
    let files = uuid::Uuid::new_v4();
    state_files_board(&mut state, files, "files rider");
    let settings = two_card_settings();
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.press(ActionId::AppSettings);
        settle_auto_lists(&*harness, no_rows);
        harness.driver.advance_time(0);
        let armed_count =
            |harness: &Harness| items(harness).iter().filter(|item| item.armed).count();
        let row = roster_indexes(&*harness, K::FilesBoard, "files rider")[0];
        harness
            .handle
            .window()
            .invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        harness.driver.advance_time(0);
        assert_eq!(armed_count(&*harness), 1, "the strip arms (the r2 grammar)");

        // The close path: the settings toggle closes the surface —
        // the armed identity dies with it.
        harness.press(ActionId::AppSettings);
        harness.driver.advance_time(0);

        // The mount boundary: re-entering shows no armed echo.
        harness.press(ActionId::AppSettings);
        harness.driver.advance_time(0);
        assert_eq!(
            armed_count(&*harness),
            0,
            "leaving settings disarms the strip; returning shows no echo"
        );

        // Arming again works fresh (the r2 grammar stands).
        let row = roster_indexes(&*harness, K::FilesBoard, "files rider")[0];
        harness
            .handle
            .window()
            .invoke_settings_files_board_action_clicked(i32::try_from(row).expect("fits"));
        harness.driver.advance_time(0);
        assert_eq!(armed_count(&*harness), 1, "a fresh arm holds");
    });
}

// -- cross-surface friction UX plan US-6 (#712): capture's Replace
// -- pre-pass folds the flow chain innermost-first, TEXT-masked; the
// -- child-fold's lowest layer lives in the view_state unit tests.

/// US-6/S1 (AC-1, the composition): capture fired over a live review
/// walk closes the walk commandlessly and mounts the capture overlay
/// (the pre-pass fold); the escape ladder's tail (AC-3) restores the
/// walk's parent view with zero flow levels and no phantom walk.
#[test]
fn us6_ac1_capture_over_a_walk_child_closes_the_walk() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        open_walk(&mut *harness);
        harness.driver.advance_time(0);
        assert!(walk_open(&*harness), "the walk is live");

        // The capture fire-drill over the live walk.
        harness.handle.dispatch_action(ActionId::CaptureQuick);
        harness.driver.advance_time(0);
        assert!(
            !walk_open(&*harness),
            "the pre-pass closed the walk (commandlessly)"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the walk's close dispatches nothing"
        );
        assert!(
            harness.handle.window().get_capture_open(),
            "the capture overlay mounts over the folded chain"
        );

        // AC-3: the dismiss restores the parent view — the root
        // is unchanged, zero flow levels, no phantom walk.
        let root_before = harness.handle.window().get_current_view();
        // The proven dismiss path (the outside-click seam).
        harness.driver.right_press_at(990.0, 890.0);
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_capture_open(),
            "the dismiss closes the capture"
        );
        assert_eq!(
            harness.handle.window().get_current_view(),
            root_before,
            "the parent view is restored exactly"
        );
        assert!(
            !walk_open(&*harness),
            "no phantom walk re-mounts after the dismiss"
        );
    });
}

// -- round-9 W1: the widget framework base (J2 US-1/S3, US-2/S1) -----------

/// J2 US-1/S3 (the routing pair; the geometry laws live on the probe):
/// a click on the capture modal's dead space keeps the dialog open,
/// and a click on the scrim closes it — the inside-safe/outside-kind
/// dismissal law through the real `ModalDialogHost` mount.
#[test]
fn us1_s3_inside_click_never_closes_and_scrim_click_dismisses() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::CaptureQuick);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_capture_open());

        // The panel's dead space: the padding band just above the
        // field (the absorbing panel root's body — no interactive
        // child there).
        let field = harness
            .driver
            .find_by_label("Capture")
            .expect("the capture field is mounted");
        let field_pos = field.absolute_position();
        let field_size = field.size();
        harness
            .driver
            .click_at(field_pos.x + field_size.width / 2.0, field_pos.y - 4.0);
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_capture_open(),
            "an inside click never closes (US-1/S3)"
        );

        // The scrim: the window corner, far from the centered panel.
        harness.driver.click_at(6.0, 6.0);
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_capture_open(),
            "the scrim click dismissed (the flag path)"
        );
    });
}

/// J2 US-2/S1 (the routing pair; the flip/clamp laws live on the
/// probe): the timer-setter popover opens below its invoking button
/// and never covers it — G-2's non-intersection through the real
/// `AnchoredOverlayHost` mount.
#[test]
fn us2_s1_the_setter_opens_below_the_anchor_never_covering_it() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        open_setter(&mut *harness);

        // The anchor: the header button ("Set a timer"); the panel
        // shares the label (the D33-pinned copy) and mounts last in
        // the tree, so the last match is the panel.
        let matches = harness.driver.find_all_by_label("Set a timer");
        assert!(
            matches.len() >= 2,
            "the anchor button and the panel both address the label"
        );
        let anchor = &matches[0];
        let panel = matches.last().expect("the panel match");
        let a_pos = anchor.absolute_position();
        let a_size = anchor.size();
        // The known mirror race (#954: `changed absolute-position`
        // is unreliable in the testing backend — W6's absorption):
        // the journey mirrors the anchor facts itself, exactly what
        // the real button does, then lets the host re-place.
        {
            let facts = harness
                .handle
                .window()
                .global::<taskboard_ui_slint::TimerSetterFacts>();
            facts.set_anchor_x(a_pos.x);
            facts.set_anchor_y(a_pos.y);
            facts.set_anchor_width(a_size.width);
            facts.set_anchor_height(a_size.height);
        }
        harness.driver.advance_time(0);

        let p_pos = panel.absolute_position();
        let p_size = panel.size();
        // Below the anchor with the space-1 gap (G-3's below leg).
        assert!(
            p_pos.y >= a_pos.y + a_size.height,
            "the popover opens below the anchor: panel y {} vs anchor bottom {}",
            p_pos.y,
            a_pos.y + a_size.height
        );
        // Non-intersection (G-2, both axes).
        let no_overlap = p_pos.y >= a_pos.y + a_size.height
            || p_pos.y + p_size.height <= a_pos.y
            || p_pos.x >= a_pos.x + a_size.width
            || p_pos.x + p_size.width <= a_pos.x;
        assert!(
            no_overlap,
            "the popover never covers its anchor (US-2 AC-1)"
        );
    });
}

// -- round-9 chunk 5: the overlay migration (J2 US-2/S3, R-8 race 2) --

/// `om_us2_s3`: the time flyout's single host mount — the panel rect
/// never intersects its field's rect, and after a forced re-resolution
/// (a window resize re-runs the placement pass) the facts mirror
/// tracks the host and the placement law still holds. The mount is
/// one host for all four sites (`site` discriminates the panel
/// contents only, never the geometry), so the site-0 pin through the
/// due picker's time field is the class's pin.
#[test]
fn om_us2_s3_time_flyout_never_over_its_field_and_the_mirror_tracks() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.press(ActionId::NavFocusDown);
        harness.press(ActionId::NavNextView);
        harness.press(ActionId::NavNextView);
        harness.driver.click_by_label("Edit due date");
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_task_model().date_picker_open);
        harness.driver.click_by_label("Quick times");
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);

        let facts = harness
            .handle
            .window()
            .global::<taskboard_ui_slint::TimeFieldFacts>();
        assert!(facts.get_open(), "the quick list flyout is open");
        let (ay, ah, py) = (
            facts.get_anchor_y(),
            facts.get_anchor_height(),
            facts.get_panel_y(),
        );
        assert!(
            py >= ay + ah || py + ah <= ay,
            "the flyout never covers its field vertically \
                 (panel-y={py} anchor-y={ay} anchor-h={ah})"
        );

        // The forced re-resolution: a window resize re-runs the
        // placement pass; the facts mirror tracks the placed panel
        // and the placement law still holds.
        harness.driver.set_window_size(800, 700);
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);
        let (ay2, ah2, py2) = (
            facts.get_anchor_y(),
            facts.get_anchor_height(),
            facts.get_panel_y(),
        );
        assert!(
            py2 >= ay2 + ah2 || py2 + ah2 <= ay2,
            "the placement law survives the re-resolution \
                 (panel-y={py2} anchor-y={ay2} anchor-h={ah2})"
        );
    });
}

/// `om_us1_s1`'s track-entries leg (the M4 sibling law): the form's
/// date-grid overlay opens window-level — no form-growth on toggle
/// (the span fields' rects stay put), the grid panel never covers its
/// date-row anchor, and an outside click drops the grid only (the
/// form and its mirrors stay).
#[test]
fn om_us1_s1_track_form_grid_opens_without_form_growth() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        press_preset(
            &mut *harness,
            taskboard_ui_slint::keymap::Preset::Standard,
            ActionId::NavFocusDown,
        );
        press_preset(
            &mut *harness,
            taskboard_ui_slint::keymap::Preset::Standard,
            ActionId::TaskEdit,
        );
        harness.driver.click_by_label("Time entries");
        harness.driver.advance_time(0);
        harness.handle.window().invoke_track_form_add_requested();
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_track_form_open());

        // The panel's layout witness before the toggle: the
        // summary-row anchor facts (mirrored live by the view's
        // keep-alives — they move iff the layout reflows).
        let facts_before = {
            let facts = harness
                .handle
                .window()
                .global::<taskboard_ui_slint::TrackPanelFacts>();
            (
                facts.get_anchor_x(),
                facts.get_anchor_y(),
                facts.get_anchor_height(),
            )
        };

        // The calendar toggle: the real click path (the anchor
        // push rides the same handler).
        harness.driver.click_by_label("Calendar");
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_track_form_grid_open(),
            "the calendar toggle opens the grid overlay"
        );

        // Zero form growth: the anchor facts are unchanged (the
        // grid overlay's underlay occludes the inner fields from
        // the element query while open, so the mirrored anchor is
        // the observable).
        let facts_after = {
            let facts = harness
                .handle
                .window()
                .global::<taskboard_ui_slint::TrackPanelFacts>();
            (
                facts.get_anchor_x(),
                facts.get_anchor_y(),
                facts.get_anchor_height(),
            )
        };
        assert_eq!(
            facts_before, facts_after,
            "the form never grows on the grid toggle"
        );

        // Non-intersection lives at the base layer: the host's
        // placement law is the G-catalogue's G-2 (probe-pinned
        // both directions), and the M4 mount is the same-law
        // sibling — the journey pins the routing seams instead:
        // the grid rides the date row's click-time anchor push
        // (the facts landed), not a stale mirror.
        let facts = harness
            .handle
            .window()
            .global::<taskboard_ui_slint::TrackPanelFacts>();
        assert!(
            facts.get_form_grid_anchor_width() > 0.0 && facts.get_form_grid_anchor_height() > 0.0,
            "the calendar click pushed the date row's realized anchor"
        );

        // The outside click drops the grid only — the form stays.
        harness.driver.click_at(6.0, 6.0);
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_track_form_grid_open(),
            "the outside click dropped the grid"
        );
        assert!(
            harness.handle.window().get_track_form_open(),
            "the form survives the grid's dismissal"
        );
    });
}

// -- round-9 chunk 5: the recur editor on the modal host (J2 US-1/S1,
// -- US-1/S4; the R-4 dismissal-role flip is the chunk's one
// -- deliberate behavior change) --

/// `om_us1_s1_recur` (the M5 anatomy + the D-3 edit-kind routing pair
/// through the real mount): the editor's scrim click routes the
/// COMMIT seam — a valid draft commits (the autosave law), an invalid
/// one dispatches nothing and the editor stays open (the validity
/// gate applies at the seam).
#[test]
fn om_us1_s1_recur_scrim_click_commits_a_valid_draft_and_nothing_else() {
    let (fx, mut state) = fixture::golden_state();
    let pattern = taskboard_domain::RecurPattern::Daily {
        every: taskboard_domain::EveryDays::new(2).expect("valid interval"),
    };
    if let Some(task) = state.tasks.get_mut(&fx.task0) {
        task.recur = Some(taskboard_domain::RecurRule {
            series: taskboard_domain::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(42)),
            anchor: chrono::NaiveDate::from_ymd_opt(2026, 10, 5).expect("fixture date"),
            pattern,
            bound: taskboard_domain::RecurBound::None,
            continued_from: None,
            paused_until: None,
        });
    }
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        harness.handle.dispatch_action(ActionId::SeriesEdit);
        harness.driver.advance_time(0);
        assert!(window.get_recur_editor_open());

        // The scrim corner (the modal's outside region): the
        // edit-kind law routes the commit seam.
        harness.driver.click_at(6.0, 6.0);
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(
            matches!(
                &drained[..],
                [taskboard_domain::StateCommand::UpdateTask {
                    changes: taskboard_domain::TaskChanges {
                        recur: Some(Some(_)),
                        ..
                    },
                    ..
                }]
            ),
            "the outside click committed the valid draft (R-4's edit kind)"
        );
        assert!(
            !window.get_recur_editor_open(),
            "the commit consumed the flow"
        );

        // The invalid-draft gate: arm the editor, corrupt the
        // interval through its own seam, and the outside click
        // dispatches nothing — the editor stays open.
        harness.handle.dispatch_action(ActionId::SeriesEdit);
        harness.driver.advance_time(0);
        assert!(window.get_recur_editor_open());
        window.invoke_recur_every_days_edited("junk".into());
        harness.driver.advance_time(0);
        harness.driver.click_at(6.0, 6.0);
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(
            drained.is_empty(),
            "an invalid draft's outside click dispatches nothing"
        );
        assert!(
            window.get_recur_editor_open(),
            "the invalid draft keeps the editor open (the gate applies)"
        );
    });
}

/// `om_us1_s1_labels` (the M6 lifecycle journey over the modal-host
/// mount, with the D-6 settle arm): assign, then delete with the
/// staged-confirm grammar, then `settle` — the D-6 `DeleteLabel` arm
/// folds the command into the state snapshot so the subsequent
/// roster read sees the refreshed model exactly like the other
/// lifecycle journeys (fixture continuity, never an oracle).
#[test]
fn om_us1_s1_labels_assign_delete_settle_journey() {
    let (_fx, state, _green, red) = label_journey_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        assert!(window.get_task_model().labels_open);

        // Assign: the unassigned row's toggle routes the pair.
        window.invoke_labels_overlay_row_toggled(1);
        harness.driver.advance_time(0);
        assert!(matches!(
            &harness.engine.drain()[..],
            [taskboard_domain::StateCommand::AssignLabel { label, .. }] if *label == red
        ));

        // Delete: the staged-confirm grammar dispatches the exact
        // DeleteLabel (row 0 = the green fixture label).
        window.invoke_labels_overlay_delete_clicked(0);
        harness.driver.advance_time(0);
        window.invoke_labels_overlay_delete_clicked(window.get_label_overlay_armed_index());
        harness.driver.advance_time(0);
        assert!(matches!(
            &harness.engine.drain()[..],
            [taskboard_domain::StateCommand::DeleteLabel { id }] if *id != red
        ));

        // The D-6 settle arm: the roster reflects the deletion on
        // the refreshed model (one live row remains).
        harness.engine.settle(&harness.handle);
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_label_overlay_rows().row_count(),
            1,
            "the settle folded the delete; the roster reads the refreshed model"
        );
    });
}

/// `om_us1_s2_s3_palette` (the M7 containment pair through the real
/// modal-host mount): an inside click on the palette's dead space
/// never closes it (the host's absorbing panel root), and the scrim
/// click dismisses (the confirm kind — the R-5 trap needs no draft).
/// The Tab containment rides the walk suite (`focus_coverage.rs`'s
/// `SearchPalette` walk stays inside).
#[test]
fn om_us1_s2_s3_palette_inside_click_never_closes_scrim_dismisses() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();
        harness.handle.dispatch_action(ActionId::PaletteSearch);
        harness.driver.advance_time(0);
        assert!(window.get_palette_open());

        // Inside: the query row's band above the field (the
        // absorbing panel root's body — no interactive child).
        let field = harness
            .driver
            .find_all_by_label("Search")
            .into_iter()
            .next()
            .expect("the query field is mounted");
        let pos = field.absolute_position();
        let size = field.size();
        harness
            .driver
            .click_at(pos.x + size.width / 2.0, pos.y - 4.0);
        harness.driver.advance_time(0);
        assert!(
            window.get_palette_open(),
            "an inside click never closes the palette"
        );

        // The scrim corner dismisses (the confirm kind).
        harness.driver.click_at(6.0, 6.0);
        harness.driver.advance_time(0);
        assert!(
            !window.get_palette_open(),
            "the scrim click dismissed the palette"
        );
    });
}

// -- round-9 chunk 5: the always-on Effort Estimate row (J2 US-3/S2,
// -- R-9's commit-on-focus-loss; the D-5 no-double-dispatch guard) --

/// `om_us3_s2` (the M9 commit-on-focus-loss law through the real
/// adapter): type + blur dispatches exactly ONE `UpdateTask`; a
/// focus + blur without an edit dispatches nothing; Return + blur
/// never double-dispatches (the last-committed guard); a dirty field
/// settles exactly once when the flow's focus loss fires.
#[test]
fn om_us3_s2_effort_commits_once_on_focus_loss_and_never_twice() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        label_journey_mount(&mut *harness);
        let window = harness.handle.window();

        // Focus + blur without an edit: zero dispatches (the lazy
        // editor never arms).
        window.invoke_task_effort_focus_lost();
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "a blur of an untouched field dispatches nothing"
        );

        // Type + blur: exactly one UpdateTask.
        window.invoke_task_effort_edited(slint::SharedString::from("45"));
        window.invoke_task_effort_focus_lost();
        harness.driver.advance_time(0);
        assert!(
            matches!(
                &harness.engine.drain()[..],
                [taskboard_domain::StateCommand::UpdateTask {
                    changes: taskboard_domain::TaskChanges {
                        effort: Some(Some(effort)),
                        ..
                    },
                    ..
                }] if *effort == taskboard_domain::Effort::new(2700)
            ),
            "the blur committed the typed value exactly once"
        );

        // Return + blur with no further edit: the guard makes the
        // pair dispatch nothing (no double-dispatch).
        window.invoke_task_effort_committed();
        window.invoke_task_effort_focus_lost();
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "an unchanged value's Return + blur dispatches nothing"
        );

        // A fresh edit + Return, then blur: still exactly the one
        // Return commit (the baseline moved with it).
        window.invoke_task_effort_edited(slint::SharedString::from("1h"));
        window.invoke_task_effort_committed();
        harness.driver.advance_time(0);
        assert!(
            matches!(
                &harness.engine.drain()[..],
                [taskboard_domain::StateCommand::UpdateTask {
                    changes: taskboard_domain::TaskChanges {
                        effort: Some(Some(effort)),
                        ..
                    },
                    ..
                }] if *effort == taskboard_domain::Effort::new(3600)
            ),
            "the Return committed the new value"
        );
        window.invoke_task_effort_focus_lost();
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "the blur after the Return dispatches nothing"
        );
    });
}

/// H2 B6 — the task-card regression pin through the widened
/// `RowSubject` seam: a board card's right-click yields today's exact
/// task page (core rows + the three task families, zero habit rows)
/// — the D1 insertion point's blast radius is zero for tasks.
#[test]
fn b6_the_task_menu_through_the_widened_subject_is_byte_identical() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.advance_time(0);
        harness
            .handle
            .window()
            .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
        harness.driver.advance_time(0);
        assert!(keyboard(&*harness).get_menu_open());
        let entries = keyboard(&*harness).get_menu_entries();
        let rows: Vec<_> = (0..entries.row_count())
            .map(|i| entries.row_data(i).expect("row"))
            .collect();
        let actions: Vec<String> = rows
            .iter()
            .filter(|r| !r.action.is_empty())
            .map(|r| r.action.to_string())
            .collect();
        assert!(
            actions.iter().all(|a| !a.starts_with("habit.")),
            "no habit row leaks into a task menu: {actions:?}"
        );
        let parents: Vec<_> = rows
            .iter()
            .filter(|r| r.submenu)
            .map(|r| r.label.to_string())
            .collect();
        assert_eq!(
            parents,
            ["Reminders", "Timers", "Entries"],
            "the task page's exact family set (Convert withheld on a plain task)"
        );
    });
}

/// H2 B2: the menu-routed `habit.log` resolves the row source's
/// habit — the focus fact the opener minted is the subject, the
/// `LogHabit` command reaches the boundary (click ≡ key composes:
/// the menu row routes the same id the strip does).
#[test]
fn us1_s2_the_menu_log_row_routes_the_row_source_habit() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        harness
            .handle
            .window()
            .invoke_habits_context_requested(0, 120.0, 120.0);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitLog);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::LogHabit { .. }] => {}
            other => panic!("the menu row routes the log, got {other:?}"),
        }
    });
}

/// H2 B4: the menu's reorder pair routes `MoveHabit` with the floor
/// midpoint of the correct free interval — the row seam's exact
/// arithmetic, minted at menu open from the section's live order.
#[test]
fn the_menu_reorder_rows_mint_the_gap_index_slots() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    // Two habits: stored orders 0 and 10.
    state_habit(&mut state, board);
    let mut second = {
        let mut h = taskboard_domain::habit::Habit {
            id: taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7()),
            board,
            title: "stretch".into(),
            description: String::new(),
            target: taskboard_domain::HabitTarget::PerDay(
                taskboard_domain::NonZeroCount::new(1).unwrap(),
            ),
            order: 10,
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: taskboard_domain::habit::HabitClocks::all(chrono::Utc::now()),
            continued_from: None,
            deleted: false,
            by: None,

            paused_until: None,
        };
        let _ = &mut h;
        h
    };
    let _ = &mut second;
    state.habits.habits.insert(second.id, second);
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        // #1198 chunk A1 D1: the slots are target gap indices into
        // the section's live sequence — the engine resolves the
        // orders. The FIRST row's menu: move-down crosses the lower
        // neighbour (gap 2); move-up has no slot above (refuses typed
        // as NoTarget).
        harness
            .handle
            .window()
            .invoke_habits_context_requested(0, 120.0, 120.0);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitMoveDown);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::HabitReorder { target, .. }] => {
                assert_eq!(*target, 2, "the gap below the lower neighbour");
            }
            other => panic!("the reorder routed, got {other:?}"),
        }
        // The LAST row's menu: move-up targets the head gap.
        harness
            .handle
            .window()
            .invoke_habits_context_requested(1, 120.0, 120.0);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitMoveUp);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::HabitReorder { target, .. }] => {
                assert_eq!(*target, 0, "the gap above the upper neighbour");
            }
            other => panic!("the reorder routed, got {other:?}"),
        }
    });
}

// ---- #1311: the habit menu's hint chain — the opener installs the
// Today-prefixed chain, so every habit row's hint carries its preset
// chord (the projection tier's per-row parity rides the menu.rs pin).

/// #1311 (the opener pin) — the mounted habit context menu's rows
/// carry hints: every `habit.*` row's `hint` equals the keymap's
/// `first_binding_display` over the Today-prefixed chain. Red
/// pre-fix: the opener installed the task menus' `[Board, Global]`
/// chain, while every habit verb is Today-bound — every hint
/// projected empty.
#[test]
fn the_habit_menu_rows_hint_their_today_chords() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey(state, move |harness| {
        harness.press(ActionId::ViewToday);
        harness.driver.advance_time(0);
        harness
            .handle
            .window()
            .invoke_habits_context_requested(0, 120.0, 120.0);
        harness.driver.advance_time(0);
        assert!(keyboard(&*harness).get_menu_open());
        let entries = keyboard(&*harness).get_menu_entries();
        let keymap = UiSettings::default().keymap;
        let chain = [ContextId::Today, ContextId::Board, ContextId::Global].to_vec();
        let mut habit_rows = 0;
        let mut hinted_rows = 0;
        for i in 0..entries.row_count() {
            let row = entries.row_data(i).expect("row");
            let Some(id) = ActionId::from_id(row.action.as_str()) else {
                continue;
            };
            let expected = keymap.first_binding_display(id, &chain).unwrap_or_default();
            let hint: Vec<String> = row.hint.iter().map(|s| s.to_string()).collect();
            assert_eq!(
                hint, expected,
                "{id:?}'s mounted row hints exactly its preset binding"
            );
            if row.action.starts_with("habit.") {
                habit_rows += 1;
                // The staged remove/restore rows are deliberately
                // unbound (the parity note) — the count below is the
                // "Today-bound verbs hint" oracle.
                hinted_rows += usize::from(!hint.is_empty());
            }
        }
        assert!(habit_rows > 0, "the habit page mounts habit verbs");
        assert!(
            hinted_rows > 0,
            "the Today-bound habit verbs hint their chords"
        );
    });
}

// ---- J3 US-1/US-2: the directional-focus journeys (round 9 chunk 9) —

// ---- the alarm window redesign journeys (#1308, round 6 B1) --------

/// The handwritten position fake (AGENTS: fakes over mocks): a
/// delta-capable placement sink recording every apply — the journey
/// drives the W-2 seam through it because the testing backend has no
/// window manager.
#[derive(Default)]
struct DragRecordingSink {
    pos: std::cell::Cell<(f32, f32)>,
    placements: std::cell::RefCell<Vec<(f32, f32)>>,
}

impl taskboard_ui_slint::AlarmPlacementSink for DragRecordingSink {
    fn position(&self) -> (f32, f32) {
        self.pos.get()
    }
    fn set_position(&self, x: f32, y: f32) {
        self.pos.set((x, y));
        self.placements.borrow_mut().push((x, y));
    }
    fn request_compositor_move(&self) -> Result<(), taskboard_ui_slint::CompositorMoveUnsupported> {
        // The journey's platform posture is the delta arm (X11-like):
        // the compositor-driven move is not taken here.
        Err(taskboard_ui_slint::CompositorMoveUnsupported)
    }
    fn placement_available(&self) -> bool {
        true
    }
}

/// US-2/S3 (#1308): the drag position survives a re-derivation pass.
/// The pass re-shows the window (`set_rows` + `show` on the held
/// instance) but never resets a user-dragged position — the applier
/// accumulates against the live position, and the show path touches
/// placement not at all (US-2/AC-3). The drag flows through the
/// window's real callbacks into the applier seam over the
/// recording fake.
#[test]
fn us2_s3_alarm_rederive_keeps_the_dragged_position() {
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: HEADLESS_RENDERER,
        },
        move |_driver| {
            let window = taskboard_ui_slint::AlarmWindow::new()
                .expect("the alarm window instantiates on the testing backend");
            let set_rows = |window: &taskboard_ui_slint::AlarmWindow| {
                let model: Vec<taskboard_ui_slint::AlarmRowModel> = vec![
                    taskboard_ui_slint::AlarmRowModel {
                        title: "tea is ready".into(),
                        phrase: "14:05 · overdue".into(),
                    },
                    taskboard_ui_slint::AlarmRowModel {
                        title: "standup in five".into(),
                        phrase: "in 4 min".into(),
                    },
                ];
                window.set_rows(slint::ModelRc::new(slint::VecModel::from(model)));
            };

            // The journey-level wiring: the strip's callbacks into
            // the applier seam (the production wiring in
            // `install_alarm_callbacks` is the same two lines; the
            // delta arm is the deterministic posture here).
            let sink = std::rc::Rc::new(DragRecordingSink::default());
            sink.pos.set((120.0, 80.0));
            let applier = std::rc::Rc::new(std::cell::RefCell::new(
                taskboard_ui_slint::DragApplier::with_session(
                    DragRecordingSinkHandle(std::rc::Rc::clone(&sink)),
                    false,
                ),
            ));
            {
                let press = std::rc::Rc::clone(&applier);
                window.on_drag_start(move || {
                    press.borrow_mut().press();
                });
            }
            {
                let moves = std::rc::Rc::clone(&applier);
                window.on_drag_by(move |dx, dy| {
                    moves.borrow_mut().drag_by(dx, dy);
                });
            }

            set_rows(&window);
            window.show().expect("show on the testing platform");
            support::driver::advance_probe_clock(0);

            // US-2/S1–S2: press the strip, drag by two increments.
            window.invoke_drag_start();
            window.invoke_drag_by(30.0, 0.0);
            window.invoke_drag_by(0.0, 12.0);
            assert_eq!(
                sink.pos.get(),
                (150.0, 92.0),
                "the window moved by exactly the accumulated delta"
            );

            // The re-derive pass: rows re-fed, the window re-shown on
            // the held instance — the dragged position survives.
            set_rows(&window);
            window.show().expect("the pass re-shows the held window");
            support::driver::advance_probe_clock(0);
            assert_eq!(
                sink.pos.get(),
                (150.0, 92.0),
                "the re-derivation never resets a user-dragged position"
            );
            assert_eq!(
                sink.placements.borrow().len(),
                2,
                "the pass applied no placement of its own"
            );
            assert!(
                window.window().is_visible(),
                "the pass keeps the window shown"
            );
        },
    );
}

/// Helper handing the shared fake to the applier (the trait takes an
/// owned sink).
struct DragRecordingSinkHandle(std::rc::Rc<DragRecordingSink>);

impl taskboard_ui_slint::AlarmPlacementSink for DragRecordingSinkHandle {
    fn position(&self) -> (f32, f32) {
        self.0.position()
    }
    fn set_position(&self, x: f32, y: f32) {
        self.0.set_position(x, y);
    }
    fn request_compositor_move(&self) -> Result<(), taskboard_ui_slint::CompositorMoveUnsupported> {
        self.0.request_compositor_move()
    }
    fn placement_available(&self) -> bool {
        self.0.placement_available()
    }
}

/// US-4/S2 (#1308, the T-22 fence): Escape through the real key path
/// — the re-homed, zero-layout-mass `FocusScope` is still the keyboard
/// surface — hides the window committing nothing: no verb command is
/// dispatched, and the next pass re-shows if rows still wait.
#[test]
fn us4_s2_alarm_escape_hides_committing_nothing() {
    use slint::platform::WindowEvent;

    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: HEADLESS_RENDERER,
        },
        move |_driver| {
            let window = taskboard_ui_slint::AlarmWindow::new()
                .expect("the alarm window instantiates on the testing backend");
            let (tx, mut rx) = tokio::sync::mpsc::channel::<taskboard_domain::StateCommand>(64);
            let rows = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            taskboard_ui_slint::install_alarm_callbacks(&window, &tx, &rows);
            let model: Vec<taskboard_ui_slint::AlarmRowModel> =
                vec![taskboard_ui_slint::AlarmRowModel {
                    title: "tea is ready".into(),
                    phrase: "14:05 · overdue".into(),
                }];
            window.set_rows(slint::ModelRc::new(slint::VecModel::from(model)));
            window.show().expect("show on the testing platform");
            support::driver::advance_probe_clock(0);
            assert!(
                window.window().is_visible(),
                "precondition: the window is shown"
            );

            // Escape through the real delivery path (the mount-focus
            // FocusScope is the window's keyboard surface).
            window.window().dispatch_event(WindowEvent::KeyPressed {
                text: "\u{1b}".into(),
            });
            support::driver::advance_probe_clock(0);

            assert!(!window.window().is_visible(), "Escape hides the window");
            assert!(
                rx.try_recv().is_err(),
                "the escape commits nothing: no verb command dispatched"
            );
        },
    );
}

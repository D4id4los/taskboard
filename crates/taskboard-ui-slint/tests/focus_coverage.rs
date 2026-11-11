// SPDX-License-Identifier: AGPL-3.0-only

//! The focus-coverage invariant (round-5 D5, chunk-4 plan §2): every
//! interactive element of a mounted view is keyboard-reachable.
//!
//! Two independent halves over the instantiated item tree:
//!
//! - **The coverage scan** (the guarantee): every enabled, visible
//!   `TouchArea` carrying a pointer handler must sit inside a tab
//!   stop that is its sibling (or a sibling of one of its ancestors)
//!   — the house FocusScope+TouchArea idiom — unless its qualified
//!   element id is in the reasoned exemption registry. A new
//!   pointer-only control fails here by default.
//! - **The walk** (the order pin): synthetic Tab from the view's
//!   mount focus visits every stop the scan found inside the view
//!   (a floor: no stop skipped), in reading order, and wraps.
//!
//! Coupling note: the scan reads `i-slint-core` item-tree internals
//! (`ItemRc`, `WindowInner::focus_item`) — the probe tier's recorded
//! Slint-bump churn surface (the exact `=1.18.1` pin rides the root
//! manifest). `WindowInner::focus_item` is a public field on the pin,
//! so the walk reads the focused item directly; no facts global is
//! needed.

mod support;

use std::ops::ControlFlow;
use std::sync::Arc;

use arc_swap::ArcSwap;
use i_slint_core::item_tree::{ItemRc, ParentItemTraversalMode};
use i_slint_core::items::{FocusScope, TextInput, TouchArea};
use i_slint_core::lengths::{LogicalPoint, LogicalRect};
use i_slint_core::window::WindowInner;
use slint::ComponentHandle as _;
use slint::Model as _;
use support::driver::{DriverOptions, JourneyDriver, SlintDriver};
use support::fixture;
use support::health::detached_health;
use taskboard_domain::AppState;
use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::keymap::stroke::parse;
use taskboard_ui_slint::{UiHandle, UiSettings, View};

/// The exemption registry: qualified `TouchArea` ids that are not
/// tab-stop-covered on purpose. Each entry names why; adding one is
/// a review-visible event.
const EXEMPT: &[(&str, &str)] = &[
    (
        "TouchArea:Toolbar::inbox-chip",
        "chrome chip, AT-operable; a focused chrome FocusScope is a key \
         sink (the D-T16 recorded deviation) — `view.inbox` is its key path",
    ),
    (
        "TouchArea:StatusBar::stop-touch",
        "chrome chip under the same D-T16 deviation; the tracking stop has \
         its routed action binding",
    ),
    // D-6.1 (round 6): the notification row's double-click open. The
    // row's verbs (dismiss/done/snooze/mute) are all key-reachable;
    // the open's keyboard parity (focus → Enter) is the round-6
    // declared non-goal, tracked as #724.
    (
        "TouchArea:NotificationSurface::row-touch",
        "the notification row's double-click open; keyboard activation \
         parity on notification rows is #724 (round-6 non-goal)",
    ),
    // The modal dismissal backdrops (#665): a full-window outside-click
    // layer under each modal. Dismissal is a pointer convenience; the
    // keyboard closes every modal by its Escape/toggle binding, and a
    // walk must never tab into a dismissal backstop.
    (
        "TouchArea:AppWindow::date-picker-backdrop",
        "modal dismissal surface; the picker closes on Escape",
    ),
    (
        "TouchArea:AppWindow::timer-setter-backdrop",
        "modal dismissal surface; the setter closes on Escape",
    ),
    (
        "TouchArea:AppWindow::track-entries-backdrop",
        "modal dismissal surface; the panel closes on Escape",
    ),
    (
        "TouchArea:AppWindow::dropdown-backdrop",
        "modal dismissal surface; the flyout closes on Escape",
    ),
    (
        "TouchArea:AppWindow::time-field-backdrop",
        "modal dismissal surface; the flyout closes on Escape",
    ),
    (
        "TouchArea:AppWindow::menu-backdrop",
        "modal dismissal surface; the menu closes on Escape",
    ),
    (
        "TouchArea:AppWindow::palette-backdrop",
        "modal dismissal surface; the palette closes on Escape",
    ),
    (
        "TouchArea:AppWindow::capture-backdrop",
        "modal dismissal surface; the capture overlay closes on Escape",
    ),
    (
        "TouchArea:AppWindow::focus-overlay-backdrop",
        "modal dismissal surface; the overlay closes on its toggle/Escape",
    ),
];

/// The item-walk hosts (#665 plan D-V1): a delegate the host's item
/// walk (arrows / `nav.next-item`, focus staying on the host) reaches
/// is keyboard-reachable through that walk, not through Tab — making
/// every card a tab stop is the round-5 F13 non-goal. Each entry
/// names the delegate element and the closed set of touch ids it
/// covers (the R8 tightening: a new own-verb button placed inside a
/// delegate is still reported), plus the test proving the walk.
/// Not an exemption: the mechanism is named and proven, per host.
struct ItemWalkHost {
    delegate: &'static str,
    touches: &'static [&'static str],
    walk_test: &'static str,
}

const ITEM_WALK_HOSTS: &[ItemWalkHost] = &[
    ItemWalkHost {
        // The column: header select, body select, and the divider
        // (its keyboard path is stack.wider/narrower on the selected
        // column — the deviation comment at the divider).
        delegate: "BoardLayout::drop-zone",
        touches: &[
            "BoardLayout::header-hit",
            "BoardLayout::body-hit",
            "BoardLayout::divider",
        ],
        walk_test: "board_item_walk_visits_every_column",
    },
    ItemWalkHost {
        delegate: "BoardLayout::drag-card",
        touches: &["TaskCard::touch", "TaskCard::done-hit"],
        walk_test: "board_item_walk_visits_every_card",
    },
    ItemWalkHost {
        delegate: "BoardLayout::compact-task",
        touches: &["TaskCard::touch", "TaskCard::done-hit"],
        walk_test: "board_compact_item_walk_visits_every_card",
    },
    ItemWalkHost {
        delegate: "TodayView::today-row",
        touches: &["TaskCard::touch", "TaskCard::done-hit"],
        walk_test: "today_item_walk_visits_every_row",
    },
    ItemWalkHost {
        delegate: "StackView::detail-task",
        touches: &["TaskCard::touch", "TaskCard::done-hit"],
        walk_test: "stack_item_walk_visits_every_card",
    },
    ItemWalkHost {
        delegate: "SearchPalette::palette-row",
        touches: &["SearchPalette::row-touch"],
        walk_test: "palette_item_walk_visits_every_row",
    },
    ItemWalkHost {
        delegate: "FocusOverlay::switcher-row",
        touches: &["FocusOverlay::row-touch"],
        walk_test: "focus_overlay_item_walk_visits_every_row",
    },
    ItemWalkHost {
        delegate: "DatePicker::day-cell",
        touches: &["DatePicker::touch"],
        walk_test: "date_picker_item_walk_visits_every_day",
    },
    ItemWalkHost {
        delegate: "DropdownFlyout::flyout-row",
        touches: &["DropdownFlyout::row-touch"],
        walk_test: "dropdown_item_walk_visits_every_row",
    },
    ItemWalkHost {
        delegate: "QuickListFlyout::flyout-row",
        touches: &["QuickListFlyout::pick"],
        walk_test: "quick_list_item_walk_visits_every_row",
    },
];

/// The item-walk tests, by name: the registry-completeness guard
/// (D-V7) checks every host names one of these.
const ITEM_WALK_TESTS: &[&str] = &[
    "board_item_walk_visits_every_column",
    "board_item_walk_visits_every_card",
    "board_compact_item_walk_visits_every_card",
    "today_item_walk_visits_every_row",
    "stack_item_walk_visits_every_card",
    "palette_item_walk_visits_every_row",
    "focus_overlay_item_walk_visits_every_row",
    "date_picker_item_walk_visits_every_day",
    "dropdown_item_walk_visits_every_row",
    "quick_list_item_walk_visits_every_row",
];

/// One pointer-interactive element the scan found uncovered (read
/// through `Debug` in the failure message).
#[derive(Debug)]
#[allow(dead_code)]
struct Finding {
    ids: String,
    rect: LogicalRect,
}

/// A mounted fixture: the handle plus the driver bound to it.
struct Mounted {
    driver: SlintDriver,
    handle: UiHandle,
    _engine: support::fake_engine::FakeEngine,
    requests: tokio::sync::mpsc::Receiver<taskboard_ui_slint::requests::UiRequest>,
}

fn mount(state: AppState, settings: UiSettings, size: (u32, u32)) -> Mounted {
    let state = Arc::new(ArcSwap::from_pointee(state));
    let (commands, engine) = support::fake_engine::FakeEngine::channel(Arc::clone(&state));
    let (requests_tx, requests) = tokio::sync::mpsc::channel(16);
    let (_system_tx, system_rx) = tokio::sync::broadcast::channel(4);
    let (_signals_tx, signals_rx) = tokio::sync::broadcast::channel(4);
    let handle = taskboard_ui_slint::mount(
        state,
        signals_rx,
        system_rx,
        commands,
        requests_tx,
        settings,
        detached_health(),
        None,
    )
    .expect("mount succeeds on the testing backend");
    let mut driver = SlintDriver::default();
    driver.bind(&handle);
    // Each fixture picks a size that lays its whole surface out inside
    // the viewport: the scan's visibility filter then needs no
    // scrolling to reach the lower sections.
    driver.set_window_size(size.0, size.1);
    driver.advance_time(0);
    Mounted {
        driver,
        handle,
        _engine: engine,
        requests,
    }
}

/// The settings precedent's size: every section laid out unscrolled.
const TALL: (u32, u32) = (1000, 6000);

/// Answers the named connector's Refresh with `rows` (the account
/// card's remote rows instantiate their picker/Create/Disconnect stops).
/// Answers the mount auto-lists (round-6 D-6.7) with empty listings
/// and drains the auto lane, clearing the request lane for the test's
/// own requests.
fn settle_auto_lists(mounted: &mut Mounted) {
    while let Ok(request) = mounted.requests.try_recv() {
        let taskboard_ui_slint::requests::UiRequest::ListRemoteBoards { reply, .. } = request
        else {
            panic!("the mount routed only auto-lists");
        };
        reply.send(Ok(Vec::new())).expect("the reply lane is alive");
    }
    mounted.handle.drain_auto_listings();
}

fn list_remotes(mounted: &mut Mounted, connector: &str, rows: &[(u64, &str)]) {
    let items = mounted
        .handle
        .window()
        .global::<taskboard_ui_slint::SettingsModel>()
        .get_connection_items();
    let deck = (0..items.row_count())
        .find(|&i| {
            items.row_data(i).is_some_and(|item| {
                item.kind == taskboard_ui_slint::ConnectionItemKind::Deck && item.title == connector
            })
        })
        .expect("the connector's Deck row renders");
    mounted
        .handle
        .window()
        .invoke_settings_refresh_clicked(i32::try_from(deck).expect("fits"));
    let Ok(taskboard_ui_slint::requests::UiRequest::ListRemoteBoards { reply, .. }) =
        mounted.requests.try_recv()
    else {
        panic!("the refresh routed a listing");
    };
    let rows = rows
        .iter()
        .map(|(id, title)| taskboard_ui_slint::requests::RemoteBoardRow {
            remote_id: taskboard_domain::RemoteBoardId::deck(*id),
            title: (*title).to_owned(),
        })
        .collect();
    reply.send(Ok(rows)).expect("the reply lane is alive");
    mounted.handle.drain_setting_replies();
    mounted.driver.advance_time(0);
}

/// Settings with two connectors (the per-card buttons instantiate) and
/// a listing on the first: one remote row bound through it (Disconnect)
/// and one unbound (the picker + Create) — every stop kind renders.
fn settings_fixture() -> Mounted {
    let (_fixture, mut state) = fixture::golden_state();
    let bound = state
        .boards
        .values_mut()
        .find(|board| !board.deleted && !board.archived)
        .expect("a live board");
    bound.remote = Some(taskboard_domain::RemoteBoardBinding {
        board: taskboard_domain::RemoteBoardId::deck(900),
        connector: taskboard_domain::ConnectorId::from("nextcloud"),
    });
    let settings = UiSettings {
        connectors: ["nextcloud", "legacy"]
            .into_iter()
            .map(|name| taskboard_ui_slint::requests::ConnectorInfo {
                name: name.into(),
                server_url: format!("https://{name}.example"),
                username: "alice".into(),
            })
            .collect(),
        ..UiSettings::default()
    };
    let mut mounted = mount(state, settings, TALL);
    mounted.handle.dispatch_action(ActionId::NavFocusBoard);
    mounted.handle.dispatch_action(ActionId::AppSettings);
    mounted.driver.advance_time(0);
    assert_eq!(mounted.handle.window().get_current_view(), View::Settings);
    list_remotes(&mut mounted, "nextcloud", &[(900, "bound"), (901, "free")]);
    mounted
}

/// The task detail over the golden fixture's first card.
fn task_view_fixture() -> Mounted {
    let (_fixture, state) = fixture::golden_state();
    let mut mounted = mount(state, UiSettings::default(), TALL);
    for action in [
        ActionId::NavFocusBoard,
        ActionId::NavNextItem,
        ActionId::TaskEdit,
    ] {
        mounted.handle.dispatch_action(action);
        mounted.driver.advance_time(0);
    }
    assert_eq!(mounted.handle.window().get_current_view(), View::TaskDetail);
    mounted
}

fn press_all(mounted: &mut Mounted, actions: &[ActionId]) {
    for action in actions {
        mounted.handle.dispatch_action(*action);
        mounted.driver.advance_time(0);
    }
    // Two turns: deferred mounts (nonce-driven refocus, anchor
    // geometry mirrors) land on the second.
    mounted.driver.advance_time(0);
}

fn assert_view(mounted: &Mounted, view: View) {
    assert_eq!(mounted.handle.window().get_current_view(), view);
}

/// Three stacks with two cards each (the item walk needs at least two
/// delegates to pin an order).
fn three_stack_state() -> AppState {
    let (_stacks, mut state) = fixture::golden_state_with_stacks(3);
    let seeds: Vec<taskboard_domain::Task> = state.tasks.values().cloned().collect();
    for seed in seeds {
        let mut second = seed;
        second.id = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
        second.order = 1;
        "task-1".clone_into(&mut second.title);
        state.tasks.insert(second.id, second);
    }
    state
}

/// The desktop board over three stacks: every column side by side,
/// each column's cards laid out unscrolled.
fn board_fixture() -> Mounted {
    let state = three_stack_state();
    let mut mounted = mount(state, UiSettings::default(), (1800, 2000));
    press_all(&mut mounted, &[ActionId::NavFocusBoard]);
    assert_view(&mounted, View::Board);
    assert!(!mounted.handle.window().get_compact_active());
    mounted
}

/// The same board below the compact breakpoint: the stack switcher
/// replaces the columns.
fn board_compact_fixture() -> Mounted {
    let state = three_stack_state();
    let mut mounted = mount(state, UiSettings::default(), (480, 2000));
    press_all(&mut mounted, &[ActionId::NavFocusBoard]);
    assert_view(&mounted, View::Board);
    assert!(
        mounted.handle.window().get_compact_active(),
        "the fixture sits below the compact breakpoint"
    );
    mounted
}

/// A live board with zero stacks: the empty-state CTAs.
fn board_empty_fixture() -> Mounted {
    let mut mounted = mount(
        fixture::empty_board_state(),
        UiSettings::default(),
        (1000, 1200),
    );
    press_all(&mut mounted, &[ActionId::NavFocusBoard]);
    assert_view(&mounted, View::Board);
    mounted
}

/// Today with overdue rows, a running timer, a tracked-today entry, an
/// alarm (the timers section and chip) and a delivered notification.
fn today_fixture() -> Mounted {
    today_fixture_with(taskboard_domain::SettingsSnapshot::default())
}

fn today_fixture_with(settings: taskboard_domain::SettingsSnapshot) -> Mounted {
    let (two, mut state, [overdue, _]) = fixture::today_overdue_state();
    state.settings = settings;
    let now = chrono::Utc::now();
    let _ = fixture::add_completed_task(
        &mut state,
        two.stack_c,
        "finished",
        now - chrono::TimeDelta::minutes(30),
        None,
        None,
    );
    let _ = fixture::add_alarm(&mut state, "tea", now + chrono::TimeDelta::hours(2));
    fixture::deliver_task_notification(&mut state, overdue, now - chrono::TimeDelta::minutes(5));
    let mut mounted = mount(state, UiSettings::default(), TALL);
    press_all(&mut mounted, &[ActionId::ViewToday]);
    assert_view(&mounted, View::Today);
    mounted
}

fn stack_fixture() -> Mounted {
    let (_fixture, state) = fixture::golden_state();
    let mut mounted = mount(state, UiSettings::default(), (1000, 4000));
    press_all(
        &mut mounted,
        &[ActionId::NavFocusBoard, ActionId::NavNextView],
    );
    assert_view(&mounted, View::StackDetail);
    mounted
}

fn review_fixture() -> Mounted {
    let (fixture, mut state) = fixture::golden_state();
    let now = chrono::Utc::now();
    let _ = fixture::add_completed_task(
        &mut state,
        fixture.stack_a,
        "shipped",
        now - chrono::TimeDelta::minutes(30),
        Some(1800),
        None,
    );
    let mut mounted = mount(state, UiSettings::default(), (1000, 4000));
    press_all(
        &mut mounted,
        &[
            ActionId::NavFocusBoard,
            ActionId::NavNextView,
            ActionId::NavNextView,
            ActionId::NavNextView,
        ],
    );
    assert_view(&mounted, View::Review);
    mounted
}

/// Two placed boxes tomorrow (09:00, 11:00) and the third card in the
/// pool (the reorder journey's grammar).
fn plan_next_day_fixture() -> Mounted {
    let (fixture, mut state) = fixture::golden_state();
    let (next_start, _) = taskboard_domain::days::next_day_window(
        chrono::Utc::now(),
        state.settings.day_start_time,
        UiSettings::default().boot_offset,
    );
    state
        .tasks
        .get_mut(&fixture.task0)
        .expect("task0")
        .scheduled = Some(next_start + chrono::TimeDelta::hours(9));
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("task1")
        .scheduled = Some(next_start + chrono::TimeDelta::hours(11));
    let mut mounted = mount(state, UiSettings::default(), (1000, 4000));
    press_all(&mut mounted, &[ActionId::ViewPlanNextDay]);
    assert_view(&mounted, View::PlanNextDay);
    assert_eq!(
        mounted.handle.window().get_plan_model().boxes.row_count(),
        2
    );
    mounted
}

/// Clicks the centre of the first visible item whose element type is
/// `type_name` (a structural address, no label text).
fn click_first(mounted: &mut Mounted, type_name: &str) {
    let prefix = format!("{type_name}:");
    let target = visible_items(&root_item(&mounted.handle))
        .into_iter()
        .find(|item| ids_of(item).iter().any(|id| id.starts_with(&prefix)))
        .unwrap_or_else(|| panic!("a visible {type_name} to click"));
    let rect = absolute_rect(&target);
    mounted.driver.click_at(
        rect.origin.x + rect.size.width / 2.0,
        rect.origin.y + rect.size.height / 2.0,
    );
    mounted.driver.advance_time(0);
    mounted.driver.advance_time(0);
}

fn palette_fixture() -> Mounted {
    let mut mounted = board_fixture();
    press_all(&mut mounted, &[ActionId::PaletteSearch]);
    assert!(mounted.handle.window().get_palette_open());
    // A query that matches the golden cards, so result rows mount.
    for stroke in ["t", "a", "s", "k"] {
        mounted.driver.press_stroke(parse(stroke).expect("letter"));
    }
    press_all(&mut mounted, &[]);
    mounted
}

fn capture_fixture() -> Mounted {
    let mut mounted = board_fixture();
    press_all(&mut mounted, &[ActionId::CaptureQuick]);
    mounted
}

fn menu_fixture() -> Mounted {
    let mut mounted = board_fixture();
    press_all(&mut mounted, &[ActionId::AppMenu]);
    mounted
}

fn timer_setter_fixture() -> Mounted {
    let mut mounted = today_fixture();
    // The header button's a11y label is its address (the journeys'
    // opener).
    mounted.driver.click_by_label("Set a timer");
    press_all(&mut mounted, &[]);
    assert!(mounted.handle.window().get_timer_setter_open());
    mounted
}

fn focus_overlay_fixture() -> Mounted {
    // The overlay's precondition: configured focus timings plus a live
    // session (the today fixture's running entry).
    let timing = taskboard_domain::focus::FocusTiming::try_new(
        chrono::TimeDelta::minutes(25),
        chrono::TimeDelta::minutes(5),
        chrono::TimeDelta::minutes(15),
        4,
    )
    .expect("positive timings");
    let mut mounted = today_fixture_with(taskboard_domain::SettingsSnapshot {
        focus_enabled: true,
        focus: Some(timing),
        ..taskboard_domain::SettingsSnapshot::default()
    });
    press_all(&mut mounted, &[ActionId::FocusModeToggle]);
    assert!(mounted.handle.window().get_focus_overlay_open());
    mounted
}

fn track_entries_fixture() -> Mounted {
    let mut mounted = task_view_fixture();
    press_all(&mut mounted, &[ActionId::TaskTrackEntries]);
    mounted
}

fn date_picker_fixture() -> Mounted {
    date_picker_fixture_by(false)
}

/// The picker opened by the opener's click (the journeys' path). The
/// default open keeps the due field focused (round-3 D10,
/// typing-first); `grid == true` then presses Down in the field — the
/// one-shot hand-over that gives the grid focus.
fn date_picker_fixture_by(grid: bool) -> Mounted {
    let mut mounted = task_view_fixture();
    // The opener's a11y label is its address (the journeys' path).
    mounted.driver.click_by_label("Edit due date");
    press_all(&mut mounted, &[]);
    assert!(mounted.handle.window().get_task_model().date_picker_open);
    if grid {
        step(&mut mounted, Step::Key("Down"));
        assert!(mounted.handle.window().get_task_model().date_grid_focus);
    }
    mounted
}

/// Settings over two boards, a remote row's picker flyout open (two
/// rows to walk).
fn dropdown_fixture() -> Mounted {
    let (_fixture, state) = fixture::two_board_state();
    let settings = UiSettings {
        connectors: vec![taskboard_ui_slint::requests::ConnectorInfo {
            name: "work".into(),
            server_url: "https://work.example".into(),
            username: "alice".into(),
        }],
        ..UiSettings::default()
    };
    let mut mounted = mount(state, settings, TALL);
    press_all(
        &mut mounted,
        &[ActionId::NavFocusBoard, ActionId::AppSettings],
    );
    assert_view(&mounted, View::Settings);
    settle_auto_lists(&mut mounted);
    list_remotes(&mut mounted, "work", &[(1, "remote")]);
    click_first(&mut mounted, "Dropdown");
    assert!(
        mounted
            .handle
            .window()
            .global::<taskboard_ui_slint::DropdownFacts>()
            .get_open()
    );
    mounted
}

fn quick_list_fixture() -> Mounted {
    let mut mounted = date_picker_fixture();
    mounted.driver.click_by_label("Quick times");
    press_all(&mut mounted, &[]);
    assert!(
        mounted
            .handle
            .window()
            .global::<taskboard_ui_slint::TimeFieldFacts>()
            .get_open()
    );
    mounted
}

fn root_item(handle: &UiHandle) -> ItemRc {
    window_root(handle.window().window())
}

fn window_root(window: &slint::Window) -> ItemRc {
    ItemRc::new_root(WindowInner::from_pub(window).component())
}

/// Every `type,id` pair the item's elements carry (inlined component
/// roots stack several), joined — the scan's structural address.
fn ids_of(item: &ItemRc) -> Vec<String> {
    let count = item.element_count().unwrap_or(0);
    (0..count)
        .filter_map(|index| item.element_type_names_and_ids(index))
        .flatten()
        .map(|(type_name, id)| format!("{type_name}:{id}"))
        .collect()
}

/// The element ids (`Component::id`, the type stripped) an item carries.
fn element_ids(item: &ItemRc) -> Vec<String> {
    ids_of(item)
        .into_iter()
        .filter_map(|pair| pair.split_once(':').map(|(_, id)| id.to_owned()))
        .collect()
}

fn carries(item: &ItemRc, element_id: &str) -> bool {
    element_ids(item).iter().any(|id| id == element_id)
}

fn absolute_rect(item: &ItemRc) -> LogicalRect {
    let geometry = item.geometry();
    LogicalRect::new(item.map_to_window(geometry.origin), geometry.size)
}

fn ancestors(item: &ItemRc) -> Vec<ItemRc> {
    std::iter::successors(
        item.parent_item(ParentItemTraversalMode::StopAtPopups),
        |parent| parent.parent_item(ParentItemTraversalMode::StopAtPopups),
    )
    .collect()
}

/// A keyboard tab stop: an enabled `focus-on-tab-navigation` scope
/// or an enabled `TextInput` (which accepts tab focus natively).
fn is_stop(item: &ItemRc) -> bool {
    if let Some(scope) = item.downcast::<FocusScope>() {
        let scope = scope.as_pin_ref();
        return scope.enabled() && scope.focus_on_tab_navigation();
    }
    item.downcast::<TextInput>()
        .is_some_and(|input| input.as_pin_ref().enabled())
}

/// A pointer-interactive element: an enabled `TouchArea` with a
/// click, double-click, or raw pointer handler (the slider's gesture
/// rides `pointer-event`, not `clicked`). Hover-only owners have none.
fn is_interactive(item: &ItemRc) -> bool {
    item.downcast::<TouchArea>().is_some_and(|touch| {
        let touch = touch.as_pin_ref();
        touch.enabled()
            && (touch.clicked.has_handler()
                || touch.double_clicked.has_handler()
                || touch.pointer_event.has_handler())
    })
}

fn area(rect: &LogicalRect) -> f32 {
    rect.size.width * rect.size.height
}

fn has_area(rect: &LogicalRect) -> bool {
    rect.size.width > 0.0 && rect.size.height > 0.0
}

/// All visible items under `root`, in tree (reading) order.
fn visible_items(root: &ItemRc) -> Vec<ItemRc> {
    let mut items = Vec::new();
    root.visit_descendants(|item| {
        if item.is_visible() && has_area(&absolute_rect(item)) {
            items.push(item.clone());
        }
        ControlFlow::<()>::Continue(())
    });
    items
}

/// The subtree root of the mounted view (the element whose type is
/// `view_type`), or the window root when absent.
fn view_root(handle: &UiHandle, view_type: &str) -> ItemRc {
    let root = root_item(handle);
    root.visit_descendants(|item| {
        if ids_of(item)
            .iter()
            .any(|id| id.starts_with(&format!("{view_type}:")))
        {
            ControlFlow::Break(item.clone())
        } else {
            ControlFlow::Continue(())
        }
    })
    .unwrap_or(root)
}

/// The coverage scan (half b) over every visible item under `root`,
/// with the item-walk registry.
fn scan(root: &ItemRc, exempt: &[(&str, &str)]) -> Vec<Finding> {
    scan_with(root, exempt, ITEM_WALK_HOSTS)
}

/// The item-walk arm: the touch's own id is in a host's closed set and
/// one of its ancestors is that host's delegate.
fn item_walk_covered(touch: &ItemRc, chain: &[ItemRc], hosts: &[ItemWalkHost]) -> bool {
    let own = element_ids(touch);
    hosts.iter().any(|host| {
        host.touches.iter().any(|t| own.iter().any(|id| id == t))
            && chain
                .iter()
                .any(|ancestor| carries(ancestor, host.delegate))
    })
}

/// The exemption entries a visible interactive touch under `root`
/// matched — the modal tests' "only the backdrop" check.
fn exempt_hits(root: &ItemRc, exempt: &[(&'static str, &str)]) -> Vec<&'static str> {
    let mut hits: Vec<&'static str> = visible_items(root)
        .iter()
        .filter(|item| is_interactive(item))
        .flat_map(|touch| {
            let ids = ids_of(touch);
            exempt
                .iter()
                .filter(move |(id, _)| ids.iter().any(|own| own == id))
                .map(|(id, _)| *id)
        })
        .collect();
    hits.sort_unstable();
    hits.dedup();
    hits
}

fn scan_with(root: &ItemRc, exempt: &[(&str, &str)], hosts: &[ItemWalkHost]) -> Vec<Finding> {
    let items = visible_items(root);
    let stops: Vec<&ItemRc> = items.iter().filter(|item| is_stop(item)).collect();
    let containers: Vec<&ItemRc> = stops
        .iter()
        .copied()
        .filter(|stop| {
            stops
                .iter()
                .any(|other| other != stop && ancestors(other).contains(stop))
        })
        .collect();
    items
        .iter()
        .filter(|item| is_interactive(item))
        .filter(|touch| {
            let ids = ids_of(touch);
            !exempt
                .iter()
                .any(|(id, _reason)| ids.iter().any(|own| own == id))
        })
        .filter(|touch| {
            let rect = absolute_rect(touch);
            let centre = LogicalPoint::new(
                rect.origin.x + rect.size.width / 2.0,
                rect.origin.y + rect.size.height / 2.0,
            );
            let chain = ancestors(touch);
            if item_walk_covered(touch, &chain, hosts) {
                return false;
            }
            !stops.iter().any(|stop| {
                let stop_rect = absolute_rect(stop);
                if chain.contains(stop) {
                    // An enclosing stop covers only when it *is* the
                    // control (the Dropdown field's scope-wraps-touch
                    // shape): the touch fills at least half of it and
                    // no other stop nests inside it. A view-sized or
                    // window-sized container scope (the key root)
                    // never covers a button or a backdrop.
                    return area(&rect) * 2.0 >= area(&stop_rect) && !containers.contains(stop);
                }
                stop.parent_item(ParentItemTraversalMode::StopAtPopups)
                    .is_some_and(|parent| chain.contains(&parent))
                    && stop_rect.contains(centre)
            })
        })
        .map(|touch| Finding {
            ids: ids_of(touch).join(" / "),
            rect: absolute_rect(touch),
        })
        .collect()
}

fn focused(handle: &UiHandle) -> Option<ItemRc> {
    WindowInner::from_pub(handle.window().window())
        .focus_item
        .borrow()
        .upgrade()
}

fn is_within(item: &ItemRc, root: &ItemRc) -> bool {
    item == root || ancestors(item).contains(root)
}

/// The walk (half a): Tab from the mount focus until the focus
/// returns to where it started; the visited stops, in order.
/// `budget` is the structural bound (a stop count with headroom), not
/// a timing bound — the traversal is finite by construction.
fn walk(mounted: &mut Mounted, backwards: bool, budget: usize) -> Vec<ItemRc> {
    let start = focused(&mounted.handle).expect("the view holds focus at the walk's start");
    let stroke = parse(if backwards { "Shift+Tab" } else { "Tab" }).expect("tab");
    let mut visited = vec![start.clone()];
    for _ in 0..budget {
        mounted.driver.press_stroke(stroke);
        mounted.driver.advance_time(0);
        let now = focused(&mounted.handle).expect("a tab always lands on a stop");
        if address(&now) == address(&start) {
            return visited;
        }
        visited.push(now);
    }
    let trail: Vec<String> = visited.iter().map(address).collect();
    panic!("the walk did not wrap within {budget} steps (a focus trap): {trail:#?}");
}

fn assert_walk_covers_the_view(mut mounted: Mounted, view_type: &str) {
    let root = view_root(&mounted.handle, view_type);
    let start = focused(&mounted.handle).expect("the mounted view holds focus");
    assert!(
        start == root,
        "mount focus parity: the {view_type} root holds focus at mount (holder: {:?})",
        ids_of(&start)
    );

    let stops: Vec<ItemRc> = visible_items(&root).into_iter().filter(is_stop).collect();
    let forward = walk(&mut mounted, false, stops.len() * 4 + 64);
    let in_view: Vec<&ItemRc> = forward
        .iter()
        .filter(|item| is_within(item, &root))
        .collect();

    let missed: Vec<String> = stops
        .iter()
        .filter(|stop| !in_view.iter().any(|seen| address(seen) == address(stop)))
        .map(|stop| ids_of(stop).join(" / "))
        .collect();
    assert!(missed.is_empty(), "stops the walk skipped: {missed:#?}");

    assert_reading_order(&in_view);

    // The backwards walk visits the same stops in mirrored order.
    let backward = walk(&mut mounted, true, stops.len() * 4 + 64);
    let mut mirrored: Vec<&ItemRc> = backward[1..].iter().collect();
    mirrored.reverse();
    assert_mirrored(&forward, &mirrored);
}

/// A step of an item walk: a routed action (the host binds it in the
/// action layer) or a raw key the host's own scope handles.
#[derive(Clone, Copy)]
enum Step {
    Action(ActionId),
    Key(&'static str),
}

fn step(mounted: &mut Mounted, how: Step) {
    match how {
        Step::Action(action) => mounted.handle.dispatch_action(action),
        Step::Key(key) => {
            mounted
                .driver
                .press_stroke(parse(key).expect("a key stroke"));
        }
    }
    mounted.driver.advance_time(0);
    mounted.driver.advance_time(0);
}

/// The delegate ids the host renders, visible, in tree order.
fn visible_delegates(root: &ItemRc, delegate: &str) -> Vec<ItemRc> {
    visible_items(root)
        .into_iter()
        .filter(|item| carries(item, delegate))
        .collect()
}

/// The R1 guard: every model row the walk expects is instantiated and
/// visible (a clip or a too-small window cannot hide one and leave a
/// false green).
fn assert_visible_count(root: &ItemRc, delegate: &str, expected: usize) {
    assert_eq!(
        visible_delegates(root, delegate).len(),
        expected,
        "every {delegate} delegate is laid out visible"
    );
}

/// The item walk (D-V1 pin 2): from the first delegate, `next` visits
/// every delegate exactly once in model (reading) order; `prev` from
/// the last mirrors it; the focus holder never changes. The walk is
/// bounded by the delegate count — the terminal clamp-or-wrap is the
/// keymap contract's, never pressed here. `read` returns the selected
/// delegate's model index.
fn assert_item_walk_covers(
    mounted: &mut Mounted,
    next: Step,
    prev: Step,
    read: &dyn Fn(&UiHandle) -> Option<usize>,
    count: usize,
) {
    assert!(
        count >= 2,
        "the walk needs at least two delegates to pin order"
    );
    let holder = focused(&mounted.handle);
    let check_holder = |mounted: &Mounted| {
        let now = focused(&mounted.handle);
        assert!(
            now == holder,
            "the item walk keeps focus on its host (was {:?}, now {:?})",
            holder.as_ref().map(ids_of),
            now.as_ref().map(ids_of)
        );
    };
    if read(&mounted.handle).is_none() {
        step(mounted, next);
        check_holder(mounted);
    }
    // Rewind to the first delegate (structural bound: the count).
    for _ in 0..count {
        if read(&mounted.handle) == Some(0) {
            break;
        }
        step(mounted, prev);
        check_holder(mounted);
    }
    let mut forward = vec![read(&mounted.handle)];
    while forward.len() < count {
        step(mounted, next);
        check_holder(mounted);
        forward.push(read(&mounted.handle));
    }
    let expected: Vec<Option<usize>> = (0..count).map(Some).collect();
    assert_eq!(
        forward, expected,
        "next visits every delegate in model order"
    );
    let mut backward = vec![read(&mounted.handle)];
    while backward.len() < count {
        step(mounted, prev);
        check_holder(mounted);
        backward.push(read(&mounted.handle));
    }
    backward.reverse();
    assert_eq!(backward, expected, "prev mirrors next");
}

/// The modal walk: mount focus sits inside the modal, and Tab and
/// Shift+Tab each return to the start without focus ever leaving the
/// modal subtree (the focus-trap contract), visiting every stop the
/// modal lays out visible in reading order, mirrored.
fn assert_modal_walk_stays_inside(mounted: &mut Mounted, modal_type: &str) {
    let modal = view_root(&mounted.handle, modal_type);
    assert!(
        modal != root_item(&mounted.handle),
        "the {modal_type} subtree is mounted"
    );
    let start = focused(&mounted.handle).expect("the modal holds focus");
    assert!(
        is_within(&start, &modal),
        "mount focus parity: focus is inside the {modal_type} at open (holder: {:?})",
        ids_of(&start)
    );
    let stops: Vec<ItemRc> = visible_items(&modal).into_iter().filter(is_stop).collect();
    let budget = stops.len() * 4 + 16;
    let forward = walk(mounted, false, budget);
    let escaped: Vec<String> = forward
        .iter()
        .filter(|item| !is_within(item, &modal))
        .map(|item| ids_of(item).join(" / "))
        .collect();
    assert!(
        escaped.is_empty(),
        "Tab left the {modal_type}: {escaped:#?}"
    );
    let missed: Vec<String> = stops
        .iter()
        .filter(|stop| !forward.iter().any(|seen| address(seen) == address(stop)))
        .map(|stop| ids_of(stop).join(" / "))
        .collect();
    assert!(
        missed.is_empty(),
        "stops the modal walk skipped: {missed:#?}"
    );
    assert_reading_order(&forward.iter().collect::<Vec<_>>());
    let backward = walk(mounted, true, budget);
    let mut mirrored: Vec<&ItemRc> = backward[1..].iter().collect();
    mirrored.reverse();
    assert_mirrored(&forward, &mirrored);
}

/// A stop's structural address: its element ids plus its window rect.
/// Item identity is not stable across a walk — a repeater that
/// re-instantiates its rows (a model refresh) mints new items for the
/// same stops.
fn address(item: &ItemRc) -> String {
    format!("{} {:?}", ids_of(item).join(" / "), absolute_rect(item))
}

/// The backwards walk visits the forward walk's stops, mirrored.
fn assert_mirrored(forward: &[ItemRc], mirrored: &[&ItemRc]) {
    let forward: Vec<String> = forward[1..].iter().map(address).collect();
    let mirrored: Vec<String> = mirrored.iter().map(|item| address(item)).collect();
    assert_eq!(forward, mirrored, "Shift+Tab mirrors the Tab order");
}

/// Each visited stop starts no higher than the previous one's top, or
/// sits on the same line to its right — reading order as geometry.
fn assert_reading_order(visited: &[&ItemRc]) {
    for pair in visited.windows(2) {
        let (a, b) = (absolute_rect(pair[0]), absolute_rect(pair[1]));
        let same_line =
            b.origin.y < a.origin.y + a.size.height && b.origin.y + b.size.height > a.origin.y;
        assert!(
            b.origin.y >= a.origin.y || (same_line && b.origin.x >= a.origin.x),
            "reading order broken: {:?} after {:?}",
            ids_of(pair[1]),
            ids_of(pair[0])
        );
    }
}

/// The modal scan: no finding over the modal subtree, none over the
/// window either, and the backdrops the window scan matched among the
/// exemptions are exactly the open modal stack's (the backdrop is the
/// only uncovered touch a modal adds, and it is the one named in the
/// registry).
fn assert_modal_scan(mounted: &Mounted, modal_type: &str, backdrops: &[&str]) {
    let modal = view_root(&mounted.handle, modal_type);
    assert!(
        modal != root_item(&mounted.handle),
        "the {modal_type} subtree is mounted"
    );
    assert_no_findings(&scan(&modal, EXEMPT), modal_type);
    let window = root_item(&mounted.handle);
    assert_no_findings(&scan(&window, EXEMPT), &format!("{modal_type} window"));
    let mut matched: Vec<&str> = exempt_hits(&window, EXEMPT)
        .into_iter()
        .filter(|hit| hit.ends_with("-backdrop"))
        .collect();
    matched.sort_unstable();
    let mut expected = backdrops.to_vec();
    expected.sort_unstable();
    assert_eq!(
        matched, expected,
        "the {modal_type} stack's backdrops, exactly"
    );
}

fn assert_no_findings(findings: &[Finding], view: &str) {
    assert!(
        findings.is_empty(),
        "{} pointer-only control(s) in the {view} view (no sibling tab stop, no exemption):\n{findings:#?}",
        findings.len()
    );
}

fn run(job: impl FnOnce() + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| job(),
    );
}

#[test]
fn settings_has_no_pointer_only_controls() {
    run(|| {
        let mounted = settings_fixture();
        let findings = scan(&root_item(&mounted.handle), EXEMPT);
        assert_no_findings(&findings, "settings");
    });
}

#[test]
fn task_view_has_no_pointer_only_controls() {
    run(|| {
        let mounted = task_view_fixture();
        let findings = scan(&root_item(&mounted.handle), EXEMPT);
        assert_no_findings(&findings, "task");
    });
}

#[test]
fn settings_walk_reaches_every_stop_in_reading_order() {
    run(|| assert_walk_covers_the_view(settings_fixture(), "SettingsView"));
}

slint::slint! {
    // The scanner's self-guards (plan §2.4): one pointer-only control
    // (must be reported) and the house sibling idiom (must not be).
    export component CoverageNegativePanel inherits Window {
        width: 200px;
        height: 100px;
        bare := TouchArea {
            width: 40px;
            height: 40px;
            clicked => {}
        }
    }
    export component CoveragePositivePanel inherits Window {
        width: 200px;
        height: 100px;
        Rectangle {
            width: 40px;
            height: 40px;
            scope := FocusScope {
                focus-on-tab-navigation: true;
            }
            covered := TouchArea {
                clicked => { scope.focus(); }
            }
        }
    }
}

#[test]
fn the_scan_reports_a_pointer_only_control() {
    run(|| {
        let panel = CoverageNegativePanel::new().expect("the panel instantiates");
        panel.show().expect("show on the testing platform");
        support::driver::advance_probe_clock(0);
        let findings = scan(&window_root(panel.window()), &[]);
        assert_eq!(
            findings.len(),
            1,
            "the bare TouchArea is reported: {findings:#?}"
        );
    });
}

#[test]
fn the_scan_accepts_the_sibling_stop_idiom() {
    run(|| {
        let panel = CoveragePositivePanel::new().expect("the panel instantiates");
        panel.show().expect("show on the testing platform");
        support::driver::advance_probe_clock(0);
        let findings = scan(&window_root(panel.window()), &[]);
        assert!(
            findings.is_empty(),
            "the sibling scope covers the touch: {findings:#?}"
        );
    });
}

// ---- the readers: the selected delegate's model index --------------------

fn emphasized(cards: impl Iterator<Item = taskboard_ui_slint::TaskCardModel>) -> Option<usize> {
    cards
        .enumerate()
        .find(|(_, card)| card.emphasis)
        .map(|(i, _)| i)
}

fn selected_column(handle: &UiHandle) -> Option<usize> {
    usize::try_from(handle.window().get_board_model().selected_stack).ok()
}

fn selected_card(handle: &UiHandle) -> Option<usize> {
    let board = handle.window().get_board_model();
    let column = board.stacks.row_data(selected_column(handle)?)?;
    emphasized(column.tasks.iter())
}

fn column_card_count(handle: &UiHandle, column: usize) -> usize {
    handle
        .window()
        .get_board_model()
        .stacks
        .row_data(column)
        .map_or(0, |c| c.tasks.row_count())
}

fn today_rows(handle: &UiHandle) -> Vec<taskboard_ui_slint::TaskCardModel> {
    handle
        .window()
        .get_today_model()
        .sections
        .iter()
        .flat_map(|section| section.rows.iter().map(|row| row.card).collect::<Vec<_>>())
        .collect()
}

fn selected_today_row(handle: &UiHandle) -> Option<usize> {
    emphasized(today_rows(handle).into_iter())
}

fn selected_stack_card(handle: &UiHandle) -> Option<usize> {
    emphasized(handle.window().get_stack_model().tasks.iter())
}

fn palette_selection(handle: &UiHandle) -> Option<usize> {
    usize::try_from(handle.window().get_palette_selected_index()).ok()
}

fn focus_switcher_selection(handle: &UiHandle) -> Option<usize> {
    usize::try_from(handle.window().get_focus_switcher_selected()).ok()
}

/// The in-month days of the picker's grid, flattened in reading order.
fn picker_days(handle: &UiHandle) -> Vec<taskboard_ui_slint::DueDayModel> {
    handle
        .window()
        .get_task_model()
        .date_grid
        .iter()
        .flat_map(|row| row.iter().collect::<Vec<_>>())
        .filter(|cell| !cell.blank)
        .collect()
}

fn picker_cursor(handle: &UiHandle) -> Option<usize> {
    picker_days(handle).iter().position(|cell| cell.cursor)
}

fn dropdown_highlight(handle: &UiHandle) -> Option<usize> {
    usize::try_from(
        handle
            .window()
            .global::<taskboard_ui_slint::DropdownFacts>()
            .get_highlight(),
    )
    .ok()
}

fn quick_list_highlight(handle: &UiHandle) -> Option<usize> {
    usize::try_from(
        handle
            .window()
            .global::<taskboard_ui_slint::TimeFieldFacts>()
            .get_highlight(),
    )
    .ok()
}

// ---- the registry's own guards (D-V7) ------------------------------------

#[test]
fn every_item_walk_host_names_a_walk_test() {
    for host in ITEM_WALK_HOSTS {
        assert!(
            ITEM_WALK_TESTS.contains(&host.walk_test),
            "{} names an item-walk test",
            host.delegate
        );
        assert!(
            !host.touches.is_empty(),
            "{} covers named touches",
            host.delegate
        );
    }
    assert_eq!(
        ITEM_WALK_HOSTS.len(),
        ITEM_WALK_TESTS.len(),
        "one item-walk test per host"
    );
}

include!(concat!(env!("OUT_DIR"), "/focus_coverage_probe.rs"));

#[test]
fn the_item_walk_arm_checks_host_membership_and_the_touch_set() {
    run(|| {
        let panel = ItemWalkPanel::new().expect("the panel instantiates");
        panel.show().expect("show on the testing platform");
        support::driver::advance_probe_clock(0);
        let root = window_root(panel.window());
        assert_eq!(
            scan(&root, &[]).len(),
            4,
            "an unregistered delegate covers nothing"
        );
        let host = [ItemWalkHost {
            delegate: "ItemWalkPanel::row",
            touches: &["ItemWalkPanel::hit"],
            walk_test: "",
        }];
        let findings = scan_with(&root, &[], &host);
        assert_eq!(
            findings.len(),
            2,
            "the own-verb touches stay reported: {findings:#?}"
        );
        assert!(findings.iter().all(|f| f.ids.contains("own-verb")));
    });
}

slint::slint! {
    // The container guard: a window-sized touch inside a scope that
    // nests other stops is not covered by that scope (the key-root
    // shape that masked every modal backdrop).
    export component ContainerPanel inherits Window {
        width: 200px;
        height: 100px;
        FocusScope {
            backdrop := TouchArea {
                clicked => {}
            }
            Rectangle {
                width: 40px;
                height: 40px;
                inner := FocusScope {
                    focus-on-tab-navigation: true;
                }
            }
        }
    }
}

#[test]
fn a_container_scope_does_not_cover_a_window_sized_touch() {
    run(|| {
        let panel = ContainerPanel::new().expect("the panel instantiates");
        panel.show().expect("show on the testing platform");
        support::driver::advance_probe_clock(0);
        let findings = scan(&window_root(panel.window()), &[]);
        assert_eq!(findings.len(), 1, "the backdrop is reported: {findings:#?}");
    });
}

// ---- Board -----------------------------------------------------------------

#[test]
fn board_has_no_pointer_only_controls() {
    run(|| {
        let mounted = board_fixture();
        let root = root_item(&mounted.handle);
        assert_visible_count(&root, "BoardLayout::drop-zone", 3);
        assert_visible_count(&root, "BoardLayout::drag-card", 6);
        assert_no_findings(&scan(&root, EXEMPT), "board");
    });
}

#[test]
fn board_compact_has_no_pointer_only_controls() {
    run(|| {
        let mounted = board_compact_fixture();
        let root = root_item(&mounted.handle);
        assert_visible_count(&root, "BoardLayout::compact-task", 2);
        assert_no_findings(&scan(&root, EXEMPT), "compact board");
    });
}

#[test]
fn board_empty_has_no_pointer_only_controls() {
    run(|| {
        let mounted = board_empty_fixture();
        assert_no_findings(&scan(&root_item(&mounted.handle), EXEMPT), "empty board");
    });
}

#[test]
fn board_walk_reaches_every_stop_in_reading_order() {
    run(|| assert_walk_covers_the_view(board_fixture(), "BoardView"));
}

#[test]
fn board_compact_walk_reaches_every_stop_in_reading_order() {
    run(|| assert_walk_covers_the_view(board_compact_fixture(), "BoardView"));
}

#[test]
fn board_empty_walk_reaches_every_stop_in_reading_order() {
    run(|| assert_walk_covers_the_view(board_empty_fixture(), "BoardView"));
}

#[test]
fn board_item_walk_visits_every_column() {
    run(|| {
        let mut mounted = board_fixture();
        assert_item_walk_covers(
            &mut mounted,
            Step::Action(ActionId::StackNext),
            Step::Action(ActionId::StackPrev),
            &selected_column,
            3,
        );
    });
}

#[test]
fn board_item_walk_visits_every_card() {
    run(|| {
        let mut mounted = board_fixture();
        // No column is selected at mount; each stack.next lands on the
        // next one.
        for column in 0..3 {
            step(&mut mounted, Step::Action(ActionId::StackNext));
            assert_eq!(selected_column(&mounted.handle), Some(column));
            let count = column_card_count(&mounted.handle, column);
            assert_item_walk_covers(
                &mut mounted,
                Step::Action(ActionId::NavNextItem),
                Step::Action(ActionId::NavPrevItem),
                &selected_card,
                count,
            );
        }
    });
}

#[test]
fn board_compact_item_walk_visits_every_card() {
    run(|| {
        let mut mounted = board_compact_fixture();
        let count = column_card_count(&mounted.handle, 0);
        assert_item_walk_covers(
            &mut mounted,
            Step::Action(ActionId::NavNextItem),
            Step::Action(ActionId::NavPrevItem),
            &selected_card,
            count,
        );
    });
}

// ---- Today -----------------------------------------------------------------

#[test]
fn today_has_no_pointer_only_controls() {
    run(|| {
        let mounted = today_fixture();
        let root = root_item(&mounted.handle);
        assert_visible_count(
            &root,
            "TodayView::today-row",
            today_rows(&mounted.handle).len(),
        );
        assert_no_findings(&scan(&root, EXEMPT), "today");
    });
}

#[test]
fn today_walk_reaches_every_stop_in_reading_order() {
    run(|| assert_walk_covers_the_view(today_fixture(), "TodayView"));
}

#[test]
#[ignore = "#670: a cross-board row selection re-runs refocus-view, parking focus on the key root"]
fn today_item_walk_visits_every_row() {
    run(|| {
        let mut mounted = today_fixture();
        let count = today_rows(&mounted.handle).len();
        assert_item_walk_covers(
            &mut mounted,
            Step::Action(ActionId::NavNextItem),
            Step::Action(ActionId::NavPrevItem),
            &selected_today_row,
            count,
        );
    });
}

// ---- Stack detail ----------------------------------------------------------

#[test]
fn stack_has_no_pointer_only_controls() {
    run(|| {
        let mounted = stack_fixture();
        let root = root_item(&mounted.handle);
        assert_visible_count(
            &root,
            "StackView::detail-task",
            mounted.handle.window().get_stack_model().tasks.row_count(),
        );
        assert_no_findings(&scan(&root, EXEMPT), "stack");
    });
}

#[test]
fn stack_walk_reaches_every_stop_in_reading_order() {
    run(|| assert_walk_covers_the_view(stack_fixture(), "StackView"));
}

/// The create leg: the conditional field mounts, takes focus inside the
/// view, and adds no pointer-only control. No Tab walk from the field —
/// leaving it ends the create flow (the text-flow contract), so the
/// field is not a wrap anchor.
#[test]
fn stack_create_field_takes_focus_and_adds_no_pointer_only_control() {
    run(|| {
        let mut mounted = stack_fixture();
        press_all(&mut mounted, &[ActionId::TaskCreate]);
        assert!(mounted.handle.window().get_stack_model().create_open);
        let root = view_root(&mounted.handle, "StackView");
        let holder = focused(&mounted.handle).expect("the field holds focus");
        assert!(holder.downcast::<TextInput>().is_some() && is_within(&holder, &root));
        assert_no_findings(&scan(&root_item(&mounted.handle), EXEMPT), "stack create");
    });
}

#[test]
fn stack_item_walk_visits_every_card() {
    run(|| {
        let mut mounted = stack_fixture();
        let count = mounted.handle.window().get_stack_model().tasks.row_count();
        assert_item_walk_covers(
            &mut mounted,
            Step::Action(ActionId::NavNextItem),
            Step::Action(ActionId::NavPrevItem),
            &selected_stack_card,
            count,
        );
    });
}

// ---- Review ----------------------------------------------------------------

#[test]
fn review_has_no_pointer_only_controls() {
    run(|| {
        let mounted = review_fixture();
        assert_no_findings(&scan(&root_item(&mounted.handle), EXEMPT), "review");
    });
}

#[test]
fn review_walk_reaches_every_stop_in_reading_order() {
    run(|| assert_walk_covers_the_view(review_fixture(), "ReviewView"));
}

// ---- Plan-next-day ---------------------------------------------------------

#[test]
fn plan_next_day_has_no_pointer_only_controls() {
    run(|| {
        let mounted = plan_next_day_fixture();
        assert_no_findings(&scan(&root_item(&mounted.handle), EXEMPT), "plan-next-day");
    });
}

#[test]
fn plan_next_day_walk_reaches_every_stop_in_reading_order() {
    run(|| assert_walk_covers_the_view(plan_next_day_fixture(), "PlanNextDayView"));
}

// ---- Modal surfaces ---------------------------------------------------------

#[test]
fn palette_has_no_pointer_only_controls() {
    run(|| {
        let mounted = palette_fixture();
        assert_modal_scan(
            &mounted,
            "SearchPalette",
            &["TouchArea:AppWindow::palette-backdrop"],
        );
    });
}

#[test]
#[ignore = "#668: no modal focus trap — Tab leaves the palette field into the key root and the chrome"]
fn palette_walk_stays_inside() {
    run(|| assert_modal_walk_stays_inside(&mut palette_fixture(), "SearchPalette"));
}

#[test]
fn palette_item_walk_visits_every_row() {
    run(|| {
        let mut mounted = palette_fixture();
        let count = mounted.handle.window().get_palette_model().row_count();
        assert_item_walk_covers(
            &mut mounted,
            Step::Key("Down"),
            Step::Key("Up"),
            &palette_selection,
            count,
        );
    });
}

#[test]
fn capture_has_no_pointer_only_controls() {
    run(|| {
        let mounted = capture_fixture();
        assert_modal_scan(
            &mounted,
            "CaptureOverlay",
            &["TouchArea:AppWindow::capture-backdrop"],
        );
    });
}

#[test]
#[ignore = "#668: no modal focus trap — Tab leaves the capture field into the key root and the board"]
fn capture_walk_stays_inside() {
    run(|| assert_modal_walk_stays_inside(&mut capture_fixture(), "CaptureOverlay"));
}

#[test]
fn menu_has_no_pointer_only_controls() {
    run(|| {
        let mounted = menu_fixture();
        assert_modal_scan(
            &mounted,
            "ActionMenu",
            &["TouchArea:AppWindow::menu-backdrop"],
        );
    });
}

#[test]
#[ignore = "#669: the menu opens with focus left on the view root (its rows are reached by menu.next only)"]
fn menu_walk_stays_inside() {
    run(|| assert_modal_walk_stays_inside(&mut menu_fixture(), "ActionMenu"));
}

#[test]
fn timer_setter_has_no_pointer_only_controls() {
    run(|| {
        let mounted = timer_setter_fixture();
        assert_modal_scan(
            &mounted,
            "TimerSetter",
            &["TouchArea:AppWindow::timer-setter-backdrop"],
        );
    });
}

#[test]
#[ignore = "#668: no modal focus trap — Tab leaves the setter's declared walk into the key root and the today view"]
fn timer_setter_walk_stays_inside() {
    run(|| assert_modal_walk_stays_inside(&mut timer_setter_fixture(), "TimerSetter"));
}

#[test]
fn focus_overlay_has_no_pointer_only_controls() {
    run(|| {
        let mounted = focus_overlay_fixture();
        assert_modal_scan(
            &mounted,
            "FocusOverlay",
            &["TouchArea:AppWindow::focus-overlay-backdrop"],
        );
    });
}

#[test]
#[ignore = "#668: no modal focus trap — Tab leaves the overlay's note field into the key root"]
fn focus_overlay_walk_stays_inside() {
    run(|| assert_modal_walk_stays_inside(&mut focus_overlay_fixture(), "FocusOverlay"));
}

#[test]
fn focus_overlay_item_walk_visits_every_row() {
    run(|| {
        let mut mounted = focus_overlay_fixture();
        let count = mounted
            .handle
            .window()
            .get_focus_switcher_model()
            .row_count();
        assert_item_walk_covers(
            &mut mounted,
            Step::Key("Down"),
            Step::Key("Up"),
            &focus_switcher_selection,
            count,
        );
    });
}

#[test]
fn track_entries_has_no_pointer_only_controls() {
    run(|| {
        let mounted = track_entries_fixture();
        assert_modal_scan(
            &mounted,
            "TrackEntriesPanel",
            &["TouchArea:AppWindow::track-entries-backdrop"],
        );
    });
}

#[test]
#[ignore = "#668: no modal focus trap — Tab leaves the panel's add stop into the key root and the task view"]
fn track_entries_walk_stays_inside() {
    run(|| assert_modal_walk_stays_inside(&mut track_entries_fixture(), "TrackEntriesPanel"));
}

#[test]
fn date_picker_has_no_pointer_only_controls() {
    run(|| {
        let mounted = date_picker_fixture();
        let root = root_item(&mounted.handle);
        let cells: usize = mounted
            .handle
            .window()
            .get_task_model()
            .date_grid
            .iter()
            .map(|row| row.row_count())
            .sum();
        assert_visible_count(&root, "DatePicker::day-cell", cells);
        assert_modal_scan(
            &mounted,
            "DatePicker",
            &["TouchArea:AppWindow::date-picker-backdrop"],
        );
    });
}

#[test]
#[ignore = "#668: no modal focus trap — Tab leaves the picker's last header button into the key root and the task view"]
fn date_picker_walk_stays_inside() {
    run(|| assert_modal_walk_stays_inside(&mut date_picker_fixture_by(true), "DatePicker"));
}

#[test]
fn date_picker_item_walk_visits_every_day() {
    run(|| {
        let mut mounted = date_picker_fixture_by(true);
        let count = picker_days(&mounted.handle).len();
        assert_item_walk_covers(
            &mut mounted,
            Step::Action(ActionId::PickerRight),
            Step::Action(ActionId::PickerLeft),
            &picker_cursor,
            count,
        );
    });
}

#[test]
fn dropdown_has_no_pointer_only_controls() {
    run(|| {
        let mounted = dropdown_fixture();
        assert_modal_scan(
            &mounted,
            "DropdownFlyout",
            &["TouchArea:AppWindow::dropdown-backdrop"],
        );
    });
}

#[test]
fn dropdown_item_walk_visits_every_row() {
    run(|| {
        let mut mounted = dropdown_fixture();
        let flyout = view_root(&mounted.handle, "DropdownFlyout");
        let count = visible_delegates(&flyout, "DropdownFlyout::flyout-row").len();
        assert_item_walk_covers(
            &mut mounted,
            Step::Key("Down"),
            Step::Key("Up"),
            &dropdown_highlight,
            count,
        );
    });
}

#[test]
fn quick_list_has_no_pointer_only_controls() {
    run(|| {
        let mounted = quick_list_fixture();
        assert_modal_scan(
            &mounted,
            "QuickListFlyout",
            &[
                "TouchArea:AppWindow::date-picker-backdrop",
                "TouchArea:AppWindow::time-field-backdrop",
            ],
        );
    });
}

#[test]
fn quick_list_item_walk_visits_every_row() {
    run(|| {
        let mut mounted = quick_list_fixture();
        let flyout = view_root(&mounted.handle, "QuickListFlyout");
        let count = visible_delegates(&flyout, "QuickListFlyout::flyout-row").len();
        assert_item_walk_covers(
            &mut mounted,
            Step::Key("Down"),
            Step::Key("Up"),
            &quick_list_highlight,
            count,
        );
    });
}

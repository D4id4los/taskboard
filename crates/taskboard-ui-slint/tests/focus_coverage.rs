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
    _requests: tokio::sync::mpsc::Receiver<taskboard_ui_slint::requests::UiRequest>,
}

fn mount(state: AppState, settings: UiSettings) -> Mounted {
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
    )
    .expect("mount succeeds on the testing backend");
    let mut driver = SlintDriver::default();
    driver.bind(&handle);
    // Tall enough that the whole settings column is laid out inside
    // the viewport: the scan's visibility filter then needs no
    // scrolling to reach the lower sections.
    driver.set_window_size(1000, 6000);
    driver.advance_time(0);
    Mounted {
        driver,
        handle,
        _engine: engine,
        _requests: requests,
    }
}

/// Settings with two connectors (the per-row buttons instantiate).
fn settings_fixture() -> Mounted {
    let (_fixture, state) = fixture::golden_state();
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
    let mut mounted = mount(state, settings);
    mounted.handle.dispatch_action(ActionId::NavFocusBoard);
    mounted.handle.dispatch_action(ActionId::AppSettings);
    mounted.driver.advance_time(0);
    assert_eq!(mounted.handle.window().get_current_view(), View::Settings);
    mounted
}

/// The task detail over the golden fixture's first card.
fn task_view_fixture() -> Mounted {
    let (_fixture, state) = fixture::golden_state();
    let mut mounted = mount(state, UiSettings::default());
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

/// The coverage scan (half b) over every visible item under `root`.
fn scan(root: &ItemRc, exempt: &[(&str, &str)]) -> Vec<Finding> {
    let items = visible_items(root);
    let stops: Vec<&ItemRc> = items.iter().filter(|item| is_stop(item)).collect();
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
            !stops.iter().any(|stop| {
                let stop_rect = absolute_rect(stop);
                if chain.contains(stop) {
                    // An enclosing stop covers only when it *is* the
                    // control (the Dropdown field's scope-wraps-touch
                    // shape): the touch fills at least half of it. A
                    // view-sized container scope never covers a button.
                    return area(&rect) * 2.0 >= area(&stop_rect);
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
        if now == start {
            return visited;
        }
        visited.push(now);
    }
    panic!("the walk did not wrap within {budget} steps (a focus trap)");
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
        .filter(|stop| !in_view.contains(stop))
        .map(|stop| ids_of(stop).join(" / "))
        .collect();
    assert!(missed.is_empty(), "stops the walk skipped: {missed:#?}");

    // Reading order: each visited stop starts no higher than the
    // previous one's top, or sits on the same line to its right.
    for pair in in_view.windows(2) {
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

    // The backwards walk visits the same stops in mirrored order.
    let backward = walk(&mut mounted, true, stops.len() * 4 + 64);
    let mut mirrored: Vec<&ItemRc> = backward[1..].iter().collect();
    mirrored.reverse();
    assert_eq!(
        forward[1..].len(),
        mirrored.len(),
        "the backwards walk visits as many stops as the forward one"
    );
    assert!(
        forward[1..].iter().zip(mirrored).all(|(a, b)| a == b),
        "Shift+Tab mirrors the Tab order"
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

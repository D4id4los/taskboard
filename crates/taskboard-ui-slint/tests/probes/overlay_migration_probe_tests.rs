// SPDX-License-Identifier: AGPL-3.0-only

//! The overlay-migration probe's om catalogue (round-9 chunk 5, plan
//! §3): the geometry/journey assertions over the migrated mounts —
//! the real `TaskView` plus the due-editor/date-picker
//! `AnchoredOverlayHost` mounts on the probe fixture. Every test name
//! embeds its J2 journey token; every observation is a typed root
//! seam; no text assertions, no clocks, no quality ceilings.

use crate::support;

use slint::ComponentHandle as _;

use support::driver::{DriverOptions, advance_probe_clock};

include!(concat!(env!("OUT_DIR"), "/overlay_migration_probe.rs"));

fn run_probe(job: impl FnOnce(&OverlayMigrationProbe) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = OverlayMigrationProbe::new().expect("the probe window instantiates");
            window.set_task(subject_model(false));
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
}

/// The probe's subject fixture: a rule-less open task with due and
/// scheduled phrases, the detail surface's stable geometry base.
/// `due_open` arms the due editor; everything else stays fixed.
fn subject_model(due_open: bool) -> TaskDetailModel {
    TaskDetailModel {
        title: "Suppe fertigkochen".into(),
        stack_title: "To Do".into(),
        due_phrase: "in 9h · 23:45".into(),
        scheduled_phrase: "tomorrow".into(),
        tracked_total: "0s".into(),
        can_done: true,
        due_open,
        due_text: "2026-12-31".into(),
        due_echo: "in 9h · 23:45".into(),
        scheduled_text: "tomorrow".into(),
        scheduled_echo: "tomorrow".into(),
        date_caption: "December".into(),
        date_syntax_hint: "YYYY-MM-DD or +Nd".into(),
        ..Default::default()
    }
}

/// Forces the testing backend's geometry pass so the realized rects
/// are current (the widget-framework probe's recorded idiom).
fn settle_geometry<T>(window: &T)
where
    T: slint::ComponentHandle,
{
    advance_probe_clock(0);
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(1.0, 1.0),
        });
    advance_probe_clock(0);
}

/// The metadata block's sibling rects (the zero-layout-shift oracle).
struct SiblingRects {
    stack_x: f32,
    stack_y: f32,
    stack_w: f32,
    stack_h: f32,
    state_x: f32,
    state_y: f32,
    state_w: f32,
    state_h: f32,
    effort_y: f32,
    effort_h: f32,
    description_y: f32,
}

impl SiblingRects {
    fn read(window: &OverlayMigrationProbe) -> Self {
        Self {
            stack_x: window.get_stack_x(),
            stack_y: window.get_stack_y(),
            stack_w: window.get_stack_width(),
            stack_h: window.get_stack_height(),
            state_x: window.get_state_x(),
            state_y: window.get_state_y(),
            state_w: window.get_state_width(),
            state_h: window.get_state_height(),
            effort_y: window.get_effort_y(),
            effort_h: window.get_effort_height(),
            description_y: window.get_description_y(),
        }
    }
}

/// `om_us1_s1` (J2 US-1/S1, R-7): opening and closing the due editor
/// shifts no sibling — every metadata-row sibling's rect is equal
/// before, during, and after the open (the swap's re-flow is dead).
#[test]
fn om_us1_s1_due_editor_open_shifts_no_sibling_rect() {
    run_probe(|window| {
        settle_geometry(window);
        let before = SiblingRects::read(window);

        window.set_task(subject_model(true));
        settle_geometry(window);
        let during = SiblingRects::read(window);
        assert_eq!(
            during.stack_x, before.stack_x,
            "stack column x stable across open"
        );
        assert_eq!(
            during.stack_y, before.stack_y,
            "stack column y stable across open"
        );
        assert_eq!(
            during.stack_w, before.stack_w,
            "stack column width stable across open"
        );
        assert_eq!(
            during.stack_h, before.stack_h,
            "stack column height stable across open"
        );
        assert_eq!(
            during.state_x, before.state_x,
            "state column x stable across open"
        );
        assert_eq!(
            during.state_y, before.state_y,
            "state column y stable across open"
        );
        assert_eq!(
            during.state_w, before.state_w,
            "state column width stable across open"
        );
        assert_eq!(
            during.state_h, before.state_h,
            "state column height stable across open"
        );
        assert_eq!(
            during.effort_y, before.effort_y,
            "effort row y stable across open"
        );
        assert_eq!(
            during.effort_h, before.effort_h,
            "effort row height stable across open"
        );
        assert_eq!(
            during.description_y, before.description_y,
            "description section y stable across open"
        );

        window.set_task(subject_model(false));
        settle_geometry(window);
        let after = SiblingRects::read(window);
        assert_eq!(
            after.stack_y, before.stack_y,
            "stack column y stable across close"
        );
        assert_eq!(
            after.state_y, before.state_y,
            "state column y stable across close"
        );
        assert_eq!(
            after.effort_y, before.effort_y,
            "effort row y stable across close"
        );
        assert_eq!(
            after.effort_h, before.effort_h,
            "effort row height stable across close"
        );
        assert_eq!(
            after.description_y, before.description_y,
            "description section y stable across close"
        );
    });
}

/// The editor-arm pass' picker placement fixture: the editor arm and
/// the picker open in the same layout pass (the M1 overlay world —
/// the swap is dead), the anchor facts land via the click-time push
/// the real cell handler performs, and the picker never covers the
/// anchor.
fn om_us2_s1_fixture(window: &OverlayMigrationProbe, anchor_x: f32, anchor_y: f32, floor: f32) {
    window.set_task(TaskDetailModel {
        due_open: true,
        date_picker_open: true,
        date_caption: "December".into(),
        date_syntax_hint: "YYYY-MM-DD or +Nd".into(),
        ..subject_model(true)
    });
    window.set_viewport_floor(floor);
    // Realize the view's layout first: the AnchorMirror keep-alive
    // (round-9 W6's #954 conversion) publishes the real due-column
    // geometry during this pass — the replay below must land after
    // it, or the keep-alive would clobber the injected anchor.
    settle_geometry(window);
    // The click-time push (the real handler's writes, replayed — the
    // setter journey's recorded mirror idiom). The second pass
    // realizes the host's placement; the mirror's sources are
    // untouched by it, so the injected anchor survives.
    let facts = window.global::<DatePickerFacts>();
    facts.set_anchor_x(anchor_x);
    facts.set_anchor_y(anchor_y);
    facts.set_anchor_width(120.0);
    facts.set_anchor_height(28.0);
    settle_geometry(window);
}

/// `om_us2_s1` (J2 US-2/S1, R-8 race 1): the picker opens in the same
/// layout pass as the editor arm — the panel rect never intersects
/// the anchor rect, and the flip verdict matches the below-fits law
/// for the fixture geometry (anchor high in the window: below fits).
#[test]
fn om_us2_s1_picker_in_the_editor_pass_never_covers_the_anchor_below() {
    run_probe(|window| {
        // Anchor high: below fits under the floor (600).
        om_us2_s1_fixture(window, 100.0, 60.0, 600.0);
        assert!(
            !window.get_picker_flipped(),
            "the anchor high in the window places below (no flip)"
        );
        let px = window.get_picker_panel_x();
        let py = window.get_picker_panel_y();
        let pw = window.get_picker_panel_width();
        let ph = window.get_picker_panel_height();
        let (ax, ay, aw, ah) = (
            window.get_facts_anchor_x(),
            window.get_facts_anchor_y(),
            window.get_facts_anchor_width(),
            window.get_facts_anchor_height(),
        );
        let no_overlap = py >= ay + ah || py + ph <= ay || px >= ax + aw || px + pw <= ax;
        assert!(no_overlap, "the picker never covers its anchor (US-2 AC-1)");
    });
}

/// `om_us2_s1`'s flip leg (J2 US-2/S2): the anchor near the viewport
/// floor — the panel flips above, still clear of the anchor.
#[test]
fn om_us2_s1_picker_flips_above_near_the_floor_still_clear() {
    run_probe(|window| {
        // Anchor just above the floor (850): below does not fit.
        om_us2_s1_fixture(window, 100.0, 800.0, 850.0);
        assert!(
            window.get_picker_flipped(),
            "the anchor near the floor flips the panel above"
        );
        let px = window.get_picker_panel_x();
        let py = window.get_picker_panel_y();
        let pw = window.get_picker_panel_width();
        let ph = window.get_picker_panel_height();
        let (ax, ay, aw, ah) = (
            window.get_facts_anchor_x(),
            window.get_facts_anchor_y(),
            window.get_facts_anchor_width(),
            window.get_facts_anchor_height(),
        );
        // Above: the panel's bottom stays at or above the anchor's top.
        assert!(
            py + ph <= ay + 1.0,
            "the flipped panel sits above the anchor: panel bottom {} vs anchor top {}",
            py + ph,
            ay
        );
        let no_overlap = py >= ay + ah || py + ph <= ay || px >= ax + aw || px + pw <= ax;
        assert!(
            no_overlap,
            "the flipped picker still never covers its anchor"
        );
    });
}

/// `om_us3_s1` (J2 US-3/S1, the M8 stretch fix): the description
/// section keeps its realized height across the display/edit mounts —
/// the section is the view's flexible element, the editor fills it
/// (`height: 100%` in the edit mount; the old swap collapsed the
/// editor to its natural single-line height and re-flowed the
/// section). The section's typed rect seam is the oracle (the
/// conditional remount's element geometry is unreliable in the
/// testing backend).
#[test]
fn om_us3_s1_description_section_keeps_its_height_across_the_edit_mount() {
    run_probe(|window| {
        window.set_task(subject_model(false));
        settle_geometry(window);
        let display_h = window.get_description_height();
        assert!(
            display_h > 100.0,
            "the display mount realized the flexible section (h={display_h})"
        );

        window.set_task(TaskDetailModel {
            description_edit_open: true,
            description_initial: "A body".into(),
            ..subject_model(false)
        });
        settle_geometry(window);
        let edit_h = window.get_description_height();
        assert!(
            (edit_h - display_h).abs() <= 1.0,
            "the edit mount keeps the section's height \
             (edit={edit_h} display={display_h})"
        );

        // And the mount swap back is symmetric.
        window.set_task(subject_model(false));
        settle_geometry(window);
        assert_eq!(
            window.get_description_height(),
            display_h,
            "closing the edit flow restores the display geometry"
        );
    });
}

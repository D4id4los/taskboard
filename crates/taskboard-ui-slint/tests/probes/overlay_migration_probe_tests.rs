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

/// `om_us2_s1`'s promised fold row (#1027): the two overlay hosts'
/// open bits read the distinct model flags exclusively — arming the
/// editor realizes the editor host alone, arming the picker realizes
/// the picker host alone; both directions pinned on the hosts' own
/// open seams plus the realized panels. The adapter-side funnel half
/// (the `begin_date_edit` settle arming both, the toggle's close arm)
/// is the journeys' cover — the om probe binds no adapter, so this
/// pin states the observable mount facts (the plan's recorded
/// fallback; no new production seam).
#[test]
fn om_us2_s1_arming_the_editor_closes_the_picker_and_vice_versa() {
    run_probe(|window| {
        // Leg A: the editor armed (Due), the picker closed.
        window.set_task(TaskDetailModel {
            due_open: true,
            date_picker_open: false,
            date_caption: "December".into(),
            date_syntax_hint: "YYYY-MM-DD or +Nd".into(),
            ..subject_model(true)
        });
        settle_geometry(window);
        assert!(
            window.get_editor_open(),
            "the armed editor realizes its host"
        );
        assert!(
            !window.get_picker_open(),
            "the closed picker flag leaves its host shut"
        );
        assert!(
            window.get_editor_panel_width() > 0.0,
            "the editor host realizes a panel"
        );

        // Leg B: the picker armed, the editor closed — the fold's
        // mirror.
        window.set_task(TaskDetailModel {
            due_open: false,
            date_picker_open: true,
            date_caption: "December".into(),
            date_syntax_hint: "YYYY-MM-DD or +Nd".into(),
            ..subject_model(false)
        });
        settle_geometry(window);
        assert!(
            window.get_picker_open(),
            "the armed picker realizes its host"
        );
        assert!(
            !window.get_editor_open(),
            "the closed editor flag leaves its host shut"
        );
        assert!(
            window.get_picker_panel_width() > 0.0,
            "the picker host realizes a panel"
        );
    });
}

// ---- #1206: the picker must never cover the *editor panel* ----------

/// A window-space rect (the structural geometry oracle for #1206).
#[derive(Debug)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn read(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    /// Interval intersection on both axes is empty (the disjointness
    /// law, stated once).
    fn disjoint(&self, other: &Rect) -> bool {
        self.y >= other.y + other.h
            || self.y + self.h <= other.y
            || self.x >= other.x + other.w
            || self.x + self.w <= other.x
    }
}

fn picker_panel(window: &OverlayMigrationProbe) -> Rect {
    Rect::read(
        window.get_picker_panel_x(),
        window.get_picker_panel_y(),
        window.get_picker_panel_width(),
        window.get_picker_panel_height(),
    )
}

fn editor_panel(window: &OverlayMigrationProbe) -> Rect {
    Rect::read(
        window.get_editor_panel_x(),
        window.get_editor_panel_y(),
        window.get_editor_panel_width(),
        window.get_editor_panel_height(),
    )
}

/// The staged-clear fixture: editor and picker both armed below the
/// mid-window anchor (P1/P4's geometry base). The focus hand-over
/// rides the `changed` edge (r4 D25's one-shot mechanism), a separate
/// `set_task` so the edge actually fires.
fn clip_fixture(window: &OverlayMigrationProbe, focused: bool) {
    om_us2_s1_fixture(window, 100.0, 60.0, 600.0);
    if focused {
        window.set_task(TaskDetailModel {
            due_open: true,
            date_picker_open: true,
            date_field_focus: true,
            date_caption: "December".into(),
            date_syntax_hint: "YYYY-MM-DD or +Nd".into(),
            ..subject_model(true)
        });
        settle_geometry(window);
    }
}

/// `om_us2_s2` (P1, #1206's acceptance): with the editor armed and
/// the picker open, the picker panel and the editor host's realized
/// panel are disjoint — the field, echo, and clear affordance live on
/// that panel, so the clip is dead by construction (below arm).
#[test]
fn om_us2_s2_picker_panel_and_editor_panel_are_disjoint_below() {
    run_probe(|window| {
        clip_fixture(window, false);
        let editor = editor_panel(window);
        let picker = picker_panel(window);
        assert!(
            editor.w > 0.0 && editor.h > 0.0,
            "the editor panel realizes"
        );
        assert!(
            picker.w > 0.0 && picker.h > 0.0,
            "the picker panel realizes"
        );
        assert!(
            picker.disjoint(&editor),
            "the picker panel covers the editor panel \
             (picker {picker:?} vs editor {editor:?})"
        );
    });
}

/// `om_us2_s2`'s flip leg (P2): the closed-cell anchor near the
/// viewport floor flips the editor panel above; the picker re-anchors
/// to it and flips too — disjoint, and its bottom stays at or above
/// the editor panel's top (the field never re-clips in the flip arm).
#[test]
fn om_us2_s2_flipped_picker_stays_clear_of_the_editor_panel() {
    run_probe(|window| {
        // Anchor just above the floor (850): both panels flip above.
        om_us2_s1_fixture(window, 100.0, 800.0, 850.0);
        assert!(
            window.get_picker_flipped(),
            "the picker resolves the flip arm near the floor"
        );
        let editor = editor_panel(window);
        let picker = picker_panel(window);
        assert!(
            picker.disjoint(&editor),
            "the flipped picker covers the editor panel \
             (picker {picker:?} vs editor {editor:?})"
        );
        assert!(
            picker.y + picker.h <= editor.y + 1.0,
            "the flipped picker's bottom crosses the editor panel's top: \
             picker bottom {} vs editor top {}",
            picker.y + picker.h,
            editor.y
        );
    });
}

/// `om_us2_s2`'s field band (P3): the field row — the element the
/// user types into, addressed by its accessible label (labels are
/// element addresses, never assertion text) — lies fully outside the
/// picker panel. Guards against a fix that moves the overlap off the
/// panel but back onto the field.
#[test]
fn om_us2_s2_the_field_band_stays_outside_the_picker_panel() {
    run_probe(|window| {
        clip_fixture(window, false);
        let field = support::driver::find_by_label_on(window, "Due date entry")
            .expect("the editor's field mounts");
        let pos = field.absolute_position();
        let size = field.size();
        let field_rect = Rect::read(pos.x, pos.y, size.width, size.height);
        let picker = picker_panel(window);
        assert!(
            field_rect.disjoint(&picker),
            "the picker panel covers the field band \
             (field {field_rect:?} vs picker {picker:?})"
        );
    });
}

/// `om_us2_s2`'s staged-clear leg (P4, symptom 2, typed outcome):
/// with the picker open, the clear affordance's activation routes one
/// `edited("")` through the probe's payload counter. The light-dismiss
/// grammar (an outside click dismisses the picker first — r4 D24)
/// makes the *raw* click path dismissal by design, so the pin drives
/// the §7.1 click-equivalence seam; the raw-path reachability is
/// P1/P3's disjointness itself.
#[test]
fn om_us2_s2_the_clear_affordance_routes_the_staged_empty_edit_with_the_picker_open() {
    run_probe(|window| {
        clip_fixture(window, true);
        let clear = support::driver::find_by_label_on(window, "Clear")
            .expect("the clear affordance mounts on the focused, non-empty field");
        clear.invoke_default_action();
        settle_geometry(window);
        assert_eq!(
            window.get_date_edited_empty_count(),
            1,
            "the staged clear routes exactly one edited(\"\")"
        );
    });
}

/// `om_us2_s2`'s keep-alive leg (P5): the picker anchor re-publishes
/// when the editor panel's source moves while the picker is open —
/// the mirror's `changed` edge (the R3 echo-growth analogue: a
/// re-layout of the editor panel) — and the disjointness survives.
#[test]
fn om_us2_s2_the_picker_anchor_tracks_the_moved_editor_panel() {
    run_probe(|window| {
        clip_fixture(window, false);
        let before = picker_panel(window);
        // Move the closed-cell anchor: the editor host re-places its
        // panel (the same keep-alive chain a scroll/echo-growth
        // re-layout drives), the M1 mirror re-publishes, the picker
        // re-places.
        let facts = window.global::<DatePickerFacts>();
        facts.set_anchor_x(400.0);
        settle_geometry(window);
        let editor = editor_panel(window);
        let picker = picker_panel(window);
        assert!(
            (picker.x - editor.x).abs() <= 1.0,
            "the picker anchor did not re-publish: picker x {} vs editor x {} \
             (moved from {})",
            picker.x,
            editor.x,
            before.x
        );
        assert!(
            picker.disjoint(&editor),
            "the moved picker covers the editor panel \
             (picker {picker:?} vs editor {editor:?})"
        );
    });
}

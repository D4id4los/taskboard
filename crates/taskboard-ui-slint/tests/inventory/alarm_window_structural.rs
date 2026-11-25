// SPDX-License-Identifier: AGPL-3.0-only

//! The alarm window's W-3 tooltip-host structural fold (US-3/S1–S2,
//! #1308 — the `view_mount_inventory` precedent: a source assertion
//! over the mount markup, whose failure mode is a missing line of
//! markup). Two-sided: the window imports the tooltip mechanism and
//! mounts the panel under the same `pending || visible` condition the
//! main window uses, and the Rust side arms the alarm tooltip driver
//! (the delay + size hand-off + placement) — without which a pending
//! state would never become visible.

const ALARM_WINDOW_SOURCE: &str = include_str!("../../ui/alarm-window.slint");

/// #1353 (D-1353a/D-1353b): the drag strip's at-rest grip cue — a
/// three-dot row whose binding states the token transition (rest
/// `Theme.border`, pressed `Theme.surface-raised`; never transparent,
/// never a raw color). The markup fold the element API cannot read
/// (a background color is not an accessible property) rides the
/// module's source-assertion precedent; the instantiated cue's
/// count/size facts are the probe mount's
/// (`alarm_window_max_probe_tests::alarm_window_carries_the_grip_dot_
/// cue`).
#[test]
fn the_alarm_drag_strip_carries_the_grip_cue_binding() {
    assert!(
        ALARM_WINDOW_SOURCE
            .contains("background: strip-ta.pressed ? Theme.surface-raised : Theme.border;"),
        "the grip dots state the pressed-state token transition"
    );
    assert!(
        ALARM_WINDOW_SOURCE.contains("for _dot in 3: grip-dot := Rectangle {"),
        "the cue is the three-dot row"
    );
}
const LIB_SOURCE: &str = include_str!("../../src/lib.rs");

/// US-3/S1 (W-3): the alarm window mounts its own `TooltipPanel` —
/// the shared `TooltipState` global is the seam (the verb
/// `IconButton`s already raise it), and the panel's mount condition
/// matches the main window's host (`pending || visible`), so a hover
/// renders inside the alarm window itself and unmounts with the
/// state (US-3/S2's clear arm rides the same condition).
#[test]
fn us3_s1_alarm_window_mounts_its_own_tooltip_panel() {
    assert!(
        ALARM_WINDOW_SOURCE
            .contains("import { TooltipState, TooltipPanel } from \"widgets/tooltip.slint\";"),
        "the alarm window imports the tooltip mechanism"
    );
    assert!(
        ALARM_WINDOW_SOURCE
            .contains("if (TooltipState.pending || TooltipState.visible): TooltipPanel {"),
        "the window mounts the panel under the pending||visible host condition"
    );
    // The Rust half of the fold: the alarm driver arms the delay
    // timer, the size hand-off and the placement — the wiring that
    // turns a pending hover into a placed, visible tooltip.
    assert!(
        LIB_SOURCE.contains("fn install_alarm_tooltip(window: &AlarmWindow)"),
        "the alarm tooltip delay driver exists"
    );
    assert!(
        LIB_SOURCE.contains("install_alarm_tooltip(window);"),
        "the callback installer arms the alarm tooltip driver"
    );
}

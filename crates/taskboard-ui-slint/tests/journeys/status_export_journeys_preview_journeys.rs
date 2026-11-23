// SPDX-License-Identifier: AGPL-3.0-only

//! The export preview/clipboard journeys (the D2 preview plan §4/§6's
//! nominated evidence file): the controls' family-gated rendering
//! (US-1/S1, US-2/S1, US-2/S4), the Preview dispatch's spec equality
//! with the dialog state (US-1/S2), the pane's cap fact and refusal
//! folds (US-1/S3/S4), and the clipboard copy's dispatch and typed
//! outcome folds (US-2/S2/S3). Assertion discipline: the dialog's
//! flags and indices are the only oracles; the payload identity's
//! app-side half lives in `crates/taskboard-app/tests/
//! it_export_preview.rs` (W-5). No size-ceiling assertions (the cap
//! is a presentation fact asserted via its flag).

use crate::support;

use support::driver::JourneyDriver as _;
use support::focus_journey::JourneyHarness;

use crate::status_export_journeys_support as prelude;
use taskboard_domain::AppState;
use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::keymap::{Keymap, Preset};
use taskboard_ui_slint::requests::{
    ExportClipboardOutcome, ExportDialogDefaults, ExportPreviewOutcome, ExportRefusal,
};
use taskboard_ui_slint::{ExportDialogModel, UiSettings};

/// The journey harness with the request lane *held* (the export
/// journeys' shape): the test plays the app half.
fn mount_holding_requests(
    state: AppState,
) -> (
    JourneyHarness,
    tokio::sync::mpsc::Receiver<taskboard_ui_slint::requests::UiRequest>,
) {
    let settings = UiSettings {
        keymap: Keymap::from_preset(Preset::Standard),
        keymap_preset: Preset::Standard,
        export: ExportDialogDefaults {
            pdf_available: true,
            pdf_dir: "/tmp/taskboard-preview-journeys".into(),
            export_dir: "/tmp/taskboard-preview-journeys".into(),
            open_after: false,
        },
        ..UiSettings::default()
    };
    prelude::mount_holding_settings(state, settings)
}

fn run(body: impl FnOnce(AppState) + Send + 'static) {
    prelude::run(body);
}

/// Mounts the dialog on the tracking (csv/json) family and applies a
/// filter edit — the spec state the dispatches must echo.
fn mount_dialog_with_filter(
    harness: &mut support::focus_journey::JourneyHarness,
) -> ExportDialogModel {
    harness.handle.dispatch_action(ActionId::ExportDialogOpen);
    harness.driver.advance_time(0);
    let window = harness.handle.window();
    window.invoke_export_field_edited(2, slint::SharedString::from("alpha"));
    harness.driver.advance_time(0);
    window.get_export_model()
}

/// US-1/S1 — the csv/json family renders the preview/copy controls
/// (D-5's family flag on) and the filter fields echo the state; the
/// dialog dispatches nothing until Preview/Copy/Run.
#[test]
fn us1_s1_the_csv_json_family_renders_the_preview_controls() {
    run(|state| {
        let (mut harness, mut requests) = mount_holding_requests(state);
        harness.driver.advance_time(0);
        let model = mount_dialog_with_filter(&mut harness);
        assert!(model.preview_enabled, "the csv/json family gates on (D-5)");
        assert_eq!(model.task, "alpha", "the filter fields echo the state");
        assert!(
            requests.try_recv().is_err(),
            "the controls render — nothing dispatches (US-1/S1)"
        );
    });
}

/// US-1/S2 (AC-1) — Preview dispatches exactly one `ExportPreview`
/// whose spec fields equal the dialog state, and the pane enters its
/// own busy posture (the per-affordance flag, D-4); no file-side
/// `ExportRun` rides (D-4's exclusive law).
#[test]
fn us1_s2_preview_dispatches_the_dialog_spec() {
    run(|state| {
        let (mut harness, mut requests) = mount_holding_requests(state);
        harness.driver.advance_time(0);
        mount_dialog_with_filter(&mut harness);
        let window = harness.handle.window();
        window.invoke_export_preview();
        harness.driver.advance_time(0);
        let request = requests
            .try_recv()
            .expect("the preview dispatches one ExportPreview");
        let taskboard_ui_slint::requests::UiRequest::ExportPreview { spec, reply } = request else {
            panic!("the preview dispatches ExportPreview");
        };
        assert_eq!(
            spec.what,
            taskboard_ui_slint::requests::ExportFamily::Tracking
        );
        assert_eq!(
            spec.task.as_deref(),
            Some("alpha"),
            "the spec fields equal the dialog state (AC-1)"
        );
        let model = window.get_export_model();
        assert!(model.preview_busy, "the pane's own busy posture (D-4)");
        assert!(!model.busy, "the Run button's flag stays clear (D-4)");

        // The app half answers a payload; the W-3 fold lands the
        // pane's facts.
        reply
            .send(ExportPreviewOutcome::Payload {
                text: "a,b\n1,2".into(),
                format: taskboard_ui_slint::requests::ExportFileFormat::Csv,
                truncated: false,
            })
            .expect("reply accepted");
        harness.handle.drain_export_preview();
        harness.driver.advance_time(0);
        let model = window.get_export_model();
        assert!(model.preview_shown, "the pane renders the payload (W-3)");
        assert!(!model.preview_truncated, "the cap fact rides the fold");
        assert!(!model.preview_busy, "the busy flag cleared");
    });
}

/// US-1/S3 (AC-3) — an over-cap payload folds the typed `truncated`
/// fact (the flag is the oracle; no sizes).
#[test]
fn us1_s3_over_cap_payload_folds_the_truncated_fact() {
    run(|state| {
        let (mut harness, mut requests) = mount_holding_requests(state);
        harness.driver.advance_time(0);
        mount_dialog_with_filter(&mut harness);
        let window = harness.handle.window();
        window.invoke_export_preview();
        harness.driver.advance_time(0);
        let taskboard_ui_slint::requests::UiRequest::ExportPreview { reply, .. } =
            requests.try_recv().expect("the dispatch")
        else {
            panic!("ExportPreview");
        };
        reply
            .send(ExportPreviewOutcome::Payload {
                text: "he".into(),
                format: taskboard_ui_slint::requests::ExportFileFormat::Json,
                truncated: true,
            })
            .expect("reply accepted");
        harness.handle.drain_export_preview();
        harness.driver.advance_time(0);
        let model = window.get_export_model();
        assert!(
            model.preview_truncated,
            "the typed cap fact folds (D-2's flag law)"
        );
        assert!(model.preview_shown);
    });
}

/// US-1/S4 (AC-4) — a typed refusal folds (the pane's refusal index),
/// the dialog stays mounted with the filter state intact, and a
/// re-Preview re-dispatches (the journey's escape cell).
#[test]
fn us1_s4_preview_refusal_folds_and_the_dialog_state_stays() {
    run(|state| {
        let (mut harness, mut requests) = mount_holding_requests(state);
        harness.driver.advance_time(0);
        mount_dialog_with_filter(&mut harness);
        let window = harness.handle.window();
        window.invoke_export_preview();
        harness.driver.advance_time(0);
        let taskboard_ui_slint::requests::UiRequest::ExportPreview { reply, .. } =
            requests.try_recv().expect("the dispatch")
        else {
            panic!("ExportPreview");
        };
        reply
            .send(ExportPreviewOutcome::Refused(ExportRefusal::BadTaskFilter))
            .expect("reply accepted");
        harness.handle.drain_export_preview();
        harness.driver.advance_time(0);
        let model = window.get_export_model();
        assert!(
            model.preview_refusal >= 0,
            "the refusal kind folds (the pane's index)"
        );
        assert!(!model.preview_shown, "no payload pane on a refusal");
        assert_eq!(
            model.task, "alpha",
            "the filter state stays intact (D-7's fold law)"
        );
        assert!(
            window.get_export_dialog_mounted(),
            "the dialog stays mounted"
        );
        // The re-Preview after correcting the failing field.
        window.invoke_export_field_edited(2, slint::SharedString::from("beta"));
        harness.driver.advance_time(0);
        window.invoke_export_preview();
        harness.driver.advance_time(0);
        let request = requests.try_recv().expect("the re-Preview dispatches");
        let taskboard_ui_slint::requests::UiRequest::ExportPreview { spec, .. } = request else {
            panic!("ExportPreview");
        };
        assert_eq!(
            spec.task.as_deref(),
            Some("beta"),
            "the corrected spec rides"
        );
    });
}

/// US-2/S1 — the same spec state renders the copy control (D-5's
/// flag) with nothing dispatched.
#[test]
fn us2_s1_the_copy_control_renders_on_the_family_flag() {
    run(|state| {
        let (mut harness, mut requests) = mount_holding_requests(state);
        harness.driver.advance_time(0);
        let model = mount_dialog_with_filter(&mut harness);
        assert!(model.preview_enabled, "the family flag gates the copy on");
        assert!(requests.try_recv().is_err(), "rendering dispatches nothing");
    });
}

/// US-2/S2 (AC-1/AC-2) — Copy dispatches exactly one
/// `ExportClipboardCopy` with the dialog's spec; the typed `Copied`
/// outcome folds (the copied-byte fact); the busy flags are the
/// copy's own.
#[test]
fn us2_s2_copy_dispatches_the_dialog_spec() {
    run(|state| {
        let (mut harness, mut requests) = mount_holding_requests(state);
        harness.driver.advance_time(0);
        mount_dialog_with_filter(&mut harness);
        let window = harness.handle.window();
        window.invoke_export_clipboard_copy();
        harness.driver.advance_time(0);
        let request = requests
            .try_recv()
            .expect("the copy dispatches one ExportClipboardCopy");
        let taskboard_ui_slint::requests::UiRequest::ExportClipboardCopy { spec, reply } = request
        else {
            panic!("ExportClipboardCopy");
        };
        assert_eq!(
            spec.task.as_deref(),
            Some("alpha"),
            "the spec fields equal the dialog state (AC-1)"
        );
        let model = window.get_export_model();
        assert!(model.copy_busy, "the copy control's own busy posture");
        reply
            .send(ExportClipboardOutcome::Copied { bytes: 7 })
            .expect("reply accepted");
        harness.handle.drain_export_copy();
        harness.driver.advance_time(0);
        let model = window.get_export_model();
        assert!(model.copy_settled, "the copied fact folds (AC-2)");
        assert!(model.copy_refusal < 0, "no refusal under the copied fact");
        assert_eq!(model.copy_bytes, 7, "the copied-byte fact");
    });
}

/// US-2/S3 (AC-3) — an unavailable clipboard folds the typed
/// `ClipboardUnavailable` refusal and the dialog stays mounted with
/// state intact (D-3).
#[test]
fn us2_s3_clipboard_unavailable_folds_the_typed_refusal() {
    run(|state| {
        let (mut harness, mut requests) = mount_holding_requests(state);
        harness.driver.advance_time(0);
        mount_dialog_with_filter(&mut harness);
        let window = harness.handle.window();
        window.invoke_export_clipboard_copy();
        harness.driver.advance_time(0);
        let taskboard_ui_slint::requests::UiRequest::ExportClipboardCopy { reply, .. } =
            requests.try_recv().expect("the dispatch")
        else {
            panic!("ExportClipboardCopy");
        };
        reply
            .send(ExportClipboardOutcome::Refused(
                ExportRefusal::ClipboardUnavailable,
            ))
            .expect("reply accepted");
        harness.handle.drain_export_copy();
        harness.driver.advance_time(0);
        let model = window.get_export_model();
        assert!(model.copy_settled);
        assert!(
            model.copy_refusal >= 0,
            "the typed environment refusal folds"
        );
        assert_eq!(model.task, "alpha", "the filter state stays intact");
        assert!(
            window.get_export_dialog_mounted(),
            "the dialog stays mounted (D-3)"
        );
    });
}

/// US-2/S4 (AC-4) — the pdf family gates the preview controls OFF
/// (D-5: a fold flag, never hidden lanes), and the family swap
/// invalidates the pane (the gating fold clears the stale family's
/// facts).
#[test]
fn us2_s4_pdf_family_gates_the_preview_controls_off() {
    run(|state| {
        let (mut harness, mut requests) = mount_holding_requests(state);
        harness.driver.advance_time(0);
        mount_dialog_with_filter(&mut harness);
        let window = harness.handle.window();
        // Land a payload first, then swap the family.
        window.invoke_export_preview();
        harness.driver.advance_time(0);
        let taskboard_ui_slint::requests::UiRequest::ExportPreview { reply, .. } =
            requests.try_recv().expect("the dispatch")
        else {
            panic!("ExportPreview");
        };
        reply
            .send(ExportPreviewOutcome::Payload {
                text: "a,b".into(),
                format: taskboard_ui_slint::requests::ExportFileFormat::Csv,
                truncated: false,
            })
            .expect("reply accepted");
        harness.handle.drain_export_preview();
        harness.driver.advance_time(0);
        assert!(window.get_export_model().preview_shown);

        // The pdf family pick: the gating fold clears the pane and
        // the copy fold (the D-5 law), `preview_enabled` goes false.
        window.invoke_export_spec_picked(0, 1);
        harness.driver.advance_time(0);
        let model = window.get_export_model();
        assert_eq!(model.family, 1, "the pdf family picked");
        assert!(
            !model.preview_enabled,
            "the gating flag is off on pdf (D-5's fold law)"
        );
        assert!(!model.preview_shown, "the stale family's pane cleared");
        assert!(!model.copy_settled, "the stale copy fold cleared");
    });
}

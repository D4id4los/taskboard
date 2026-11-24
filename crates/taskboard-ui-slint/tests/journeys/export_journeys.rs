// SPDX-License-Identifier: AGPL-3.0-only

//! The export dialog's journeys (the J2 export plan §6's nominated
//! evidence file): the dialog's mount from the three entry points
//! (US-1), the tracking csv/json leg's echo/run/outcome (US-2), the
//! pdf leg's typed controls and disabled fold (US-3), and the kiosk
//! absence gate (R-2). Assertion discipline is the journeys suite's:
//! terminal state + the typed request that reached the lane boundary;
//! the dialog's flags and indices are the only oracles — caption text
//! is never matched (US-3/AC-3's fold law).

use crate::support;
use support::driver::JourneyDriver as _;
use support::fixture;
use support::harness::{Harness, run_journey_no_escape, run_journey_no_escape_with_settings};

use slint::ComponentHandle as _;
use slint::Model as _;
use taskboard_domain::AppState;
use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::keymap::stroke;
use taskboard_ui_slint::requests::{ExportDialogDefaults, ExportRefusal, ExportRunOutcome};
use taskboard_ui_slint::{FieldValidity, UiSettings};

/// The export defaults' desktop seed for the pdf-available legs: the
/// feature fact on, both homes pointed at a scratch path, the config
/// open-after posture on.
fn pdf_available_defaults() -> ExportDialogDefaults {
    ExportDialogDefaults {
        pdf_available: true,
        pdf_dir: "/tmp/taskboard-export-journey".into(),
        export_dir: "/tmp/taskboard-export-journey".into(),
        open_after: true,
    }
}

fn settings_with_defaults(defaults: ExportDialogDefaults) -> UiSettings {
    UiSettings {
        export: defaults,
        ..UiSettings::default()
    }
}

/// The dialog's model read (the journeys' one oracle surface).
fn export_model(harness: &Harness) -> taskboard_ui_slint::ExportDialogModel {
    harness.handle.window().get_export_model()
}

/// US-1/S1 (AC-1): the dialog mounts from all three entry points with
/// the defaults loaded and NOTHING dispatched — the action route, the
/// Settings row's seam, and the mount's idempotent second dispatch.
/// US-1/S2: the mounted dialog's control set carries the typed
/// defaults (tracking family, csv format, fold layout, the Run gate
/// enabled — the pdf leg's flag rides the hand-over, a default
/// hand-over's pdf controls arrive honestly unavailable).
#[test]
fn us1_s1_dialog_mounts_from_all_entry_points() {
    let (_fixture, state) = fixture::journey_state();
    run_journey_no_escape_with_settings(
        state,
        settings_with_defaults(pdf_available_defaults()),
        move |harness| {
            harness.driver.advance_time(0);
            let window = harness.handle.window();
            assert!(
                !window.get_export_dialog_mounted(),
                "the dialog starts unmounted"
            );

            // Entry point 1 — the action route (the action-menu item
            // and the keymap stroke both land here).
            harness.handle.dispatch_action(ActionId::ExportDialogOpen);
            harness.driver.advance_time(0);
            assert!(
                window.get_export_dialog_mounted(),
                "the action mounts the dialog"
            );

            // Entry point 1b — the window-level open callback (W-1's
            // `export-dialog-open` front): the same route (idempotent
            // on the mounted dialog — the state keeps).
            window.invoke_export_dialog_open();
            harness.driver.advance_time(0);
            assert!(
                window.get_export_dialog_mounted(),
                "the window-level open callback keeps the dialog mounted"
            );
            let model = export_model(harness);
            assert_eq!(model.family, 0, "the tracking family is the default");
            assert_eq!(model.format, 0, "csv is the default format");
            assert_eq!(model.layout, 0, "fold is the default layout");
            assert!(
                model.run_enabled,
                "the Run gate starts enabled (a fresh draft mints)"
            );
            assert!(
                model.pdf_available,
                "the hand-over's feature fact rides the model (D-4's fold flag)"
            );
            assert!(
                !model.busy && model.outcome_kind == 0,
                "a fresh mount carries no busy posture and no outcome"
            );
            assert!(
                harness.engine.drain().is_empty() && harness.drain_requests().is_empty(),
                "the mount dispatches nothing (US-1/AC-1)"
            );

            // The close (US-1/AC-2's Escape rung drives the close in
            // its own test); the re-open from the Settings row's seam.
            harness.handle.window().invoke_export_dialog_close();
            harness.driver.advance_time(0);
            assert!(!window.get_export_dialog_mounted(), "the close unmounts");
            // Entry point 2 — the Settings row's seam (W-2): the same
            // mount arm the action routes (D-1's convergence).
            harness.handle.window().invoke_export_settings_open();
            harness.driver.advance_time(0);
            assert!(
                window.get_export_dialog_mounted(),
                "the Settings row mounts the same dialog"
            );

            // Entry point 3 — the idempotent re-dispatch: an
            // already-mounted dialog keeps its state (no reset).
            harness
                .handle
                .window()
                .invoke_export_field_edited(2, slint::SharedString::from("alpha"));
            harness.handle.window().invoke_export_settings_open();
            harness.driver.advance_time(0);
            assert!(
                export_model(harness).task == "alpha",
                "a re-entry dispatch never resets a mounted dialog's state"
            );
            assert!(
                harness.engine.drain().is_empty() && harness.drain_requests().is_empty(),
                "the whole journey dispatches nothing app-side"
            );
        },
    );
}

/// US-1/AC-2: Escape closes the mounted dialog with NOTHING
/// dispatched — the dialog context's total mask owns the stroke (the
/// preset row), the close drops the VM, and the engine/lane boundary
/// stays silent.
#[test]
fn us1_s1_escape_closes_with_nothing_dispatched() {
    let (_fixture, state) = fixture::journey_state();
    run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::ExportDialogOpen);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_export_dialog_mounted());

        // The raw Escape stroke (the context's own row — the dialog
        // context's mask makes it total).
        let escape = stroke::parse("Escape").expect("escape");
        harness.driver.press_stroke(escape);
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_export_dialog_mounted(),
            "the Escape rung closes the dialog"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the close dispatches no engine command"
        );
        assert!(
            harness.drain_requests().is_empty(),
            "the close dispatches no request"
        );
    });
}

/// US-2/S1 (AC-1): the fields' echo — every edit mirrors into the
/// pushed model the moment it happens (the adapter owns the buffer),
/// the date grammar's invalid fact rides the field's validity flag
/// (never caption text), and an invalid date disables the Run gate.
#[test]
fn us2_s1_filters_echo_dialog_state() {
    let (_fixture, state) = fixture::journey_state();
    run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        harness.handle.dispatch_action(ActionId::ExportDialogOpen);
        harness.driver.advance_time(0);

        window.invoke_export_field_edited(0, slint::SharedString::from("2026-11-01"));
        window.invoke_export_field_edited(1, slint::SharedString::from("2026-11-21"));
        window.invoke_export_field_edited(2, slint::SharedString::from("alpha"));
        window.invoke_export_field_edited(3, slint::SharedString::from("work board"));
        window.invoke_export_spec_picked(1, 1); // format → json
        harness.driver.advance_time(0);
        let model = export_model(harness);
        assert!(model.since == "2026-11-01", "the since field echoes");
        assert!(model.until == "2026-11-21", "the until field echoes");
        assert!(model.task == "alpha", "the task selector echoes");
        assert!(model.board == "work board", "the board selector echoes");
        assert_eq!(model.format, 1, "the json pick mirrors");
        assert!(
            model.since_validity == FieldValidity::Valid
                && model.until_validity == FieldValidity::Valid,
            "the parsed dates carry the valid flag"
        );
        assert!(model.run_enabled, "a well-formed draft enables the Run");

        // The invalid date's fact: the validity flag flips and the
        // Run gate disables (US-2/S1's invalid-draft posture).
        window.invoke_export_field_edited(0, slint::SharedString::from("not-a-date"));
        harness.driver.advance_time(0);
        let model = export_model(harness);
        assert!(
            model.since_validity == FieldValidity::Invalid,
            "the invalid date carries the invalid flag (the echo keeps the text)"
        );
        assert!(
            !model.run_enabled,
            "an invalid draft never enables the Run gate"
        );

        // Clearing the field drops the filter (US-2/S1's escape law).
        window.invoke_export_field_edited(0, slint::SharedString::from(""));
        harness.driver.advance_time(0);
        let model = export_model(harness);
        assert!(
            model.since_validity == FieldValidity::Neutral && model.run_enabled,
            "a cleared field drops the filter and re-enables the Run"
        );
        assert!(
            harness.engine.drain().is_empty() && harness.drain_requests().is_empty(),
            "the editing journey dispatches nothing (display only until Run)"
        );
    });
}

/// US-2/S2 (AC-1): the Run dispatch carries the spec verbatim — the
/// dialog's state IS the spec's fields — and the busy posture holds
/// between the dispatch and the typed reply (the Run gate disables
/// while busy).
#[test]
fn us2_s2_run_dispatches_export_run_spec() {
    let (_fixture, state) = fixture::journey_state();
    run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        harness.handle.dispatch_action(ActionId::ExportDialogOpen);
        harness.driver.advance_time(0);
        window.invoke_export_field_edited(0, slint::SharedString::from("2026-11-01"));
        window.invoke_export_field_edited(2, slint::SharedString::from("alpha"));
        window.invoke_export_field_edited(3, slint::SharedString::from("work board"));
        window.invoke_export_spec_picked(1, 1); // json
        harness.driver.advance_time(0);

        window.invoke_export_run();
        harness.driver.advance_time(0);
        let mut requests = harness.drain_requests();
        let reply = match requests.pop().expect("one request") {
            taskboard_ui_slint::requests::UiRequest::ExportRun { spec, reply } => {
                assert_eq!(
                    spec.what,
                    taskboard_ui_slint::requests::ExportFamily::Tracking,
                    "the family is the dialog's pick"
                );
                assert_eq!(
                    spec.format,
                    taskboard_ui_slint::requests::ExportFileFormat::Json,
                    "the format is the dialog's pick"
                );
                assert_eq!(
                    spec.since,
                    Some(chrono::NaiveDate::from_ymd_opt(2026, 11, 1).expect("valid")),
                    "the since filter is the dialog's field"
                );
                assert!(
                    spec.until.is_none(),
                    "the cleared until field mints no filter"
                );
                assert_eq!(
                    spec.task.as_deref(),
                    Some("alpha"),
                    "the task selector is the dialog's field"
                );
                assert_eq!(
                    spec.board.as_deref(),
                    Some("work board"),
                    "the board selector is the dialog's field"
                );
                assert!(
                    spec.layout.is_none() && spec.out.is_none(),
                    "the tracking leg carries no pdf options and no override —
                     the empty destination is the engine-default contract"
                );
                reply
            }
            other => panic!("the Run dispatches an ExportRun, got {other:?}"),
        };
        // The app half answers; the drain folds the reply.
        let _ = reply.send(ExportRunOutcome::Written {
            path: "/tmp/exports/tracking.json".into(),
        });
        assert!(
            export_model(harness).busy,
            "the busy posture holds before the reply"
        );
        assert!(
            !export_model(harness).run_enabled,
            "the Run gate disables under the busy posture"
        );

        harness.handle.drain_export_run();
        harness.driver.advance_time(0);
        let model = export_model(harness);
        assert!(!model.busy, "the reply lifts the busy posture");
        assert_eq!(
            model.outcome_kind, 1,
            "the written fact lands in the outcome area"
        );
        assert!(
            model.outcome_path == "/tmp/exports/tracking.json",
            "the outcome renders the written path (the file fact)"
        );
    });
}

/// US-2/S2's escape law: Escape during the run closes the dialog and
/// DROPS the pending reply — the app-side run completes or refuses
/// independently (the late reply settles into a drained slot: no
/// panic, no orphaned wait).
#[test]
fn us2_s2_escape_during_the_run_drops_the_pending_reply() {
    let (_fixture, state) = fixture::journey_state();
    run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        harness.handle.dispatch_action(ActionId::ExportDialogOpen);
        harness.driver.advance_time(0);
        window.invoke_export_run();
        harness.driver.advance_time(0);
        let mut requests = harness.drain_requests();
        assert_eq!(requests.len(), 1, "the run dispatches exactly one request");
        let reply = match requests.pop().expect("one request") {
            taskboard_ui_slint::requests::UiRequest::ExportRun { reply, .. } => reply,
            other => panic!("the run dispatches an ExportRun, got {other:?}"),
        };
        assert!(export_model(harness).busy, "the run is in flight");

        let escape = stroke::parse("Escape").expect("escape");
        harness.driver.press_stroke(escape);
        harness.driver.advance_time(0);
        assert!(
            !window.get_export_dialog_mounted(),
            "the escape closes the dialog mid-run"
        );

        // The app half answers late; the drain consumes the reply
        // against the dropped slot (no panic, no stranded wait).
        let _ = reply.send(ExportRunOutcome::Written {
            path: "/tmp/x".into(),
        });
        harness.handle.drain_export_run();
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "the whole escape journey dispatches no engine command"
        );
    });
}

/// US-2/S3 (AC-3): a typed refusal renders the kind and keeps the
/// dialog mounted with the filter state intact — the re-run path.
#[test]
fn us2_s3_typed_outcome_renders_file_fact_or_refusal() {
    let (_fixture, state) = fixture::journey_state();
    run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        harness.handle.dispatch_action(ActionId::ExportDialogOpen);
        harness.driver.advance_time(0);
        window.invoke_export_field_edited(0, slint::SharedString::from("2026-11-01"));
        window.invoke_export_field_edited(2, slint::SharedString::from("alpha"));
        harness.driver.advance_time(0);

        window.invoke_export_run();
        harness.driver.advance_time(0);
        let mut requests = harness.drain_requests();
        let reply = match requests.pop().expect("one request") {
            taskboard_ui_slint::requests::UiRequest::ExportRun { reply, .. } => reply,
            other => panic!("the run dispatches an ExportRun, got {other:?}"),
        };
        let _ = reply.send(ExportRunOutcome::Refused(ExportRefusal::BadTaskFilter));
        harness.handle.drain_export_run();
        harness.driver.advance_time(0);
        let model = export_model(harness);
        assert!(
            window.get_export_dialog_mounted(),
            "a refusal keeps the dialog mounted"
        );
        assert_eq!(model.outcome_kind, 2, "the refusal kind lands");
        assert!(
            model.refusal
                == taskboard_ui_slint::requests::refusal_index(ExportRefusal::BadTaskFilter),
            "the typed refusal's canonical index renders (the kind, never text)"
        );
        assert!(
            model.since == "2026-11-01" && model.task == "alpha",
            "the filter state stays intact on refusal"
        );

        // The re-run after the (typed) diagnosis dispatches again.
        window.invoke_export_run();
        harness.driver.advance_time(0);
        match harness.drain_requests().as_slice() {
            [taskboard_ui_slint::requests::UiRequest::ExportRun { .. }] => {}
            other => panic!("the re-run dispatches again, got {other:?}"),
        }
    });
}

/// US-3/S1 (AC-1/AC-3): the pdf leg's controls — the family pick
/// mounts the pdf controls, the open-after checkbox toggles the
/// posture, and the D-4 fold: a build without the feature renders the
/// controls DISABLED (the flag is the oracle) and cannot dispatch.
#[test]
fn us3_s1_pdf_controls_pre_fill_and_the_fold_disabled_flag() {
    let (_fixture, state) = fixture::journey_state();

    // The D-4 arm: a default hand-over (no pdf feature) — the pdf
    // family's Run gate never enables and the pick cannot dispatch.
    run_journey_no_escape(state.clone(), move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        harness.handle.dispatch_action(ActionId::ExportDialogOpen);
        harness.driver.advance_time(0);
        assert!(
            !export_model(harness).pdf_available,
            "the default hand-over carries the honest unavailable flag"
        );
        window.invoke_export_spec_picked(0, 1); // family → pdf
        window.invoke_export_field_edited(4, slint::SharedString::from("/tmp/override.pdf"));
        harness.driver.advance_time(0);
        let model = export_model(harness);
        assert_eq!(model.family, 1, "the pdf family pick mirrors");
        assert!(
            !model.run_enabled,
            "the unavailable pdf leg never enables the Run (the flag, not the caption)"
        );
        window.invoke_export_run();
        harness.driver.advance_time(0);
        assert!(
            harness.drain_requests().is_empty(),
            "the disabled pdf leg cannot dispatch (US-3/AC-3)"
        );
    });

    // The available arm: the controls mirror and the open-after
    // checkbox toggles (D-2's config default pre-fill rides the
    // hand-over's open_after fact).
    let (_fixture, state) = fixture::journey_state();
    run_journey_no_escape_with_settings(
        state,
        settings_with_defaults(pdf_available_defaults()),
        move |harness| {
            harness.driver.advance_time(0);
            let window = harness.handle.window();
            harness.handle.dispatch_action(ActionId::ExportDialogOpen);
            harness.driver.advance_time(0);
            window.invoke_export_spec_picked(0, 1); // family → pdf
            harness.driver.advance_time(0);
            let model = export_model(harness);
            assert!(model.run_enabled, "the available pdf leg enables the Run");
            assert!(
                model.open_after,
                "the open-after checkbox pre-fills from the config posture (D-2)"
            );
            window.invoke_export_open_after_toggled();
            harness.driver.advance_time(0);
            assert!(
                !export_model(harness).open_after,
                "the checkbox toggles the posture"
            );
            window.invoke_export_spec_picked(2, 2); // layout → a5
            harness.driver.advance_time(0);
            assert_eq!(
                export_model(harness).layout,
                2,
                "the typed layout pick mirrors (never free text)"
            );
        },
    );
}

/// US-3/S2 (AC-2): the pdf run's spec carries the typed layout, the
/// destination override, and the open-after posture verbatim.
#[test]
fn us3_s2_run_dispatches_the_pdf_spec_with_the_override() {
    let (_fixture, state) = fixture::journey_state();
    run_journey_no_escape_with_settings(
        state,
        settings_with_defaults(pdf_available_defaults()),
        move |harness| {
            harness.driver.advance_time(0);
            let window = harness.handle.window();
            harness.handle.dispatch_action(ActionId::ExportDialogOpen);
            harness.driver.advance_time(0);
            window.invoke_export_spec_picked(0, 1); // family → pdf
            window.invoke_export_spec_picked(2, 1); // layout → a4
            window.invoke_export_field_edited(4, slint::SharedString::from("/tmp/override.pdf"));
            window.invoke_export_open_after_toggled(); // → false
            harness.driver.advance_time(0);

            window.invoke_export_run();
            harness.driver.advance_time(0);
            let mut requests = harness.drain_requests();
            match requests.pop().expect("one request") {
                taskboard_ui_slint::requests::UiRequest::ExportRun { spec, reply } => {
                    assert_eq!(
                        spec.what,
                        taskboard_ui_slint::requests::ExportFamily::Pdf,
                        "the pdf family rides the spec"
                    );
                    assert_eq!(
                        spec.layout,
                        Some(taskboard_ui_slint::requests::PdfLayoutChoice::A4),
                        "the typed layout rides the spec (US-3/AC-2)"
                    );
                    assert_eq!(
                        spec.out.as_deref(),
                        Some(std::path::Path::new("/tmp/override.pdf")),
                        "the explicit destination override rides the spec"
                    );
                    assert!(
                        !spec.open_after,
                        "the toggled open-after posture rides the spec"
                    );
                    let _ = reply.send(ExportRunOutcome::Written {
                        path: "/tmp/override.pdf".into(),
                    });
                }
                other => panic!("the pdf run dispatches, got {other:?}"),
            }
            harness.handle.drain_export_run();
            harness.driver.advance_time(0);
            assert_eq!(
                export_model(harness).outcome_kind,
                1,
                "the written fact lands (US-3/S3)"
            );
        },
    );
}

/// US-1/S2: the mounted dialog's control set — the pickers, the
/// filter fields, and the destination — is the CLI verb's grammar
/// verbatim (R-3); the dialog is cancellable at every point below.
#[test]
fn us1_s2_the_mounted_dialog_carries_the_control_set() {
    let (_fixture, state) = fixture::journey_state();
    run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        harness.handle.dispatch_action(ActionId::ExportDialogOpen);
        harness.driver.advance_time(0);
        let model = export_model(harness);
        assert_eq!(model.family, 0, "the family picker mounts");
        assert_eq!(model.format, 0, "the format picker mounts (csv default)");
        assert_eq!(model.layout, 0, "the layout picker mounts (fold default)");
        // The dialog stays cancellable at every point (US-1/S2's
        // escape law): the close unmounts with nothing dispatched.
        window.invoke_export_dialog_close();
        harness.driver.advance_time(0);
        assert!(
            !window.get_export_dialog_mounted(),
            "the dialog stays cancellable at every point (US-1/S2)"
        );
        assert_eq!(harness.engine.drain(), Vec::new());
    });
}

/// US-3/S3: the pdf leg's outcome area — the written path or the
/// refusal kind — and the pdf state stays intact across a refusal
/// (US-3/AC-4's re-run path).
#[test]
fn us3_s3_the_pdf_outcome_renders_the_written_path_or_refusal() {
    let (_fixture, state) = fixture::journey_state();
    run_journey_no_escape_with_settings(
        state,
        settings_with_defaults(pdf_available_defaults()),
        move |harness| {
            harness.driver.advance_time(0);
            let window = harness.handle.window();
            harness.handle.dispatch_action(ActionId::ExportDialogOpen);
            harness.driver.advance_time(0);
            window.invoke_export_spec_picked(0, 1); // family → pdf
            window.invoke_export_spec_picked(2, 3); // layout → a6
            harness.driver.advance_time(0);

            // The refusal path: the typed kind lands, the pdf state
            // stays intact (US-3/AC-4).
            window.invoke_export_run();
            harness.driver.advance_time(0);
            let mut requests = harness.drain_requests();
            let reply = match requests.pop().expect("one request") {
                taskboard_ui_slint::requests::UiRequest::ExportRun { reply, .. } => reply,
                other => panic!("the pdf run dispatches, got {other:?}"),
            };
            let _ = reply.send(ExportRunOutcome::Refused(
                ExportRefusal::DestinationUnwritable,
            ));
            harness.handle.drain_export_run();
            harness.driver.advance_time(0);
            let model = export_model(harness);
            assert_eq!(model.outcome_kind, 2, "the refusal kind lands");
            assert!(
                model.refusal
                    == taskboard_ui_slint::requests::refusal_index(
                        ExportRefusal::DestinationUnwritable
                    ),
                "the typed refusal renders (US-3/S3)"
            );
            assert_eq!(
                model.layout, 3,
                "the layout pick survives the refusal (the state stays)"
            );

            // The written path: the re-run answers with the file fact.
            window.invoke_export_run();
            harness.driver.advance_time(0);
            let mut requests = harness.drain_requests();
            let reply = match requests.pop().expect("one request") {
                taskboard_ui_slint::requests::UiRequest::ExportRun { reply, .. } => reply,
                other => panic!("the re-run dispatches, got {other:?}"),
            };
            let _ = reply.send(ExportRunOutcome::Written {
                path: "/tmp/today-sheet.pdf".into(),
            });
            harness.handle.drain_export_run();
            harness.driver.advance_time(0);
            let model = export_model(harness);
            assert_eq!(model.outcome_kind, 1, "the written fact lands");
            assert!(
                model.outcome_path == "/tmp/today-sheet.pdf",
                "the outcome renders the written path (US-3/S3)"
            );
        },
    );
}

/// US-1's R-2 gate: the kiosk hand-over carries NO export affordance
/// — the dispatch is structurally inert, the action-menu root page
/// has no export row, and the Settings view's mount gate is false.
/// The kiosk gate asserts absence (the structural facts), never
/// caption text.
#[test]
fn us1_s1_the_kiosk_gate_keeps_every_entry_point_absent() {
    let (_fixture, state) = fixture::journey_state();
    let kiosk = UiSettings {
        kiosk: true,
        ..UiSettings::default()
    };
    run_journey_no_escape_with_settings(state, kiosk, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        harness.handle.dispatch_action(ActionId::ExportDialogOpen);
        harness.driver.advance_time(0);
        assert!(
            !window.get_export_dialog_mounted(),
            "the kiosk dispatch never mounts the dialog (R-2)"
        );

        // The action-menu root page: no export row on the appliance.
        harness.handle.dispatch_action(ActionId::AppMenu);
        harness.driver.advance_time(0);
        let entries = window
            .global::<taskboard_ui_slint::KeyboardState>()
            .get_menu_entries();
        let rows: Vec<String> = (0..entries.row_count())
            .map(|i| entries.row_data(i).expect("row").action.to_string())
            .collect();
        assert!(
            !rows.iter().any(|action| action == "app.export-dialog"),
            "the kiosk root menu carries no export row; got {rows:?}"
        );
        harness.handle.dispatch_action(ActionId::AppMenu); // close
        harness.driver.advance_time(0);

        // The Settings view's mount gate: model-side absence.
        assert!(
            !window
                .global::<taskboard_ui_slint::SettingsModel>()
                .get_export_affordance(),
            "the kiosk Settings view carries no export section (R-2)"
        );
    });
}

/// A fresh `AppState` (unused in this file's fixture set — the
/// journeys mount the shared fixture; kept for the docs' shape).
#[allow(dead_code)]
fn unused_state(_: &AppState) {}

/// #1190: the presentation labels' table is structurally pinned to
/// the canonical refusal kinds — one non-empty label per typed kind,
/// no drift between the two tables. The label text itself is
/// presentation and never asserted (round law (a): the journeys keep
/// asserting the refusal index).
#[test]
fn refusal_labels_table_matches_the_canonical_kinds() {
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey_no_escape(state, move |harness| {
        let labels = harness
            .handle
            .window()
            .global::<taskboard_ui_slint::ExportRefusalLabels>()
            .get_kinds();
        assert_eq!(
            labels.row_count(),
            taskboard_ui_slint::requests::REFUSAL_KINDS.len(),
            "the labels table drifts from the canonical refusal kinds"
        );
        for i in 0..labels.row_count() {
            assert!(
                !labels.row_data(i).expect("row").is_empty(),
                "refusal kind {i} carries no label"
            );
        }
    });
}

// SPDX-License-Identifier: AGPL-3.0-only

//! The status-surface integration leg (OP3, the accepted journey plan
//! `2026-11-21-parity-restore-journeys-status-surface.md` §6's W-4/W-5
//! rows): the #1111 CLI draining line's committed-artifact pins (US-3)
//! and the `StatusQuery` reply's 1:1 `StatusView` mirror (W-4). The
//! GUI-leg journeys live in
//! `crates/taskboard-ui-slint/tests/journeys/status_surface_journeys.rs`.

use chrono::{TimeZone, Utc};
use taskboard_app::commands::status::{StatusView, render_text_at};

/// A bare no-daemon view with only the named diagnostics set — the
/// struct-literal base for the text-rendering pins (the in-file
/// `bare` precedent, `commands/status.rs`).
fn no_daemon_view(holder_pid: Option<u32>, draining: Option<bool>) -> StatusView {
    StatusView {
        daemon: false,
        version: None,
        backend: None,
        pid: None,
        sync: None,
        holder_pid,
        connector_attached: None,
        connectors: None,
        boards: None,
        engine: None,
        files_sync: None,
        draining,
    }
}

/// The frozen render clock shared by the committed-artifact pins (D-4):
/// one arbitrary instant — the artifact text embeds no clock-derived
/// text on these views (no files-sync block), and the pin freezes the
/// instant regardless so a future clock-reading clause cannot silently
/// unpin the line.
fn frozen_now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 21, 12, 0, 0)
        .single()
        .expect("frozen instant")
}

/// US-3/S1 (OP3 W-5, #1111, journey plan US-3/AC-1 with the `us3_s1`
/// step token) — the draining line, pinned byte-for-byte with the
/// render clock frozen (D-4): `daemon == false ∧ draining == Some(true)`
/// renders "daemon draining" with the artifact-and-lock fact plus the
/// lock holder pid, instead of the plain "no daemon" line.
#[test]
fn us3_s1_draining_view_renders_the_draining_line_committed_artifact() {
    let view = no_daemon_view(Some(4242), Some(true));
    assert_eq!(
        render_text_at(&view, frozen_now()),
        "daemon draining: the sync artifact sits beside a held lock (lock holder pid 4242)"
    );

    // The holder pid is the diagnostic sidecar's best-effort fact: a
    // draining view without one still renders the draining posture.
    let pidless = no_daemon_view(None, Some(true));
    assert_eq!(
        render_text_at(&pidless, frozen_now()),
        "daemon draining: the sync artifact sits beside a held lock"
    );
}

/// US-3/S2 (OP3 W-5, journey plan US-3/AC-2) — the additive law: a
/// `draining: None` no-daemon view renders byte-identically to the
/// pre-#1111 rendering (the plain no-daemon line, with and without the
/// diagnostic holder pid). `Some(false)` never mints (B6's law), and
/// the renderer treats it exactly as absent — the same typed rule.
#[test]
fn us3_s2_draining_none_no_daemon_rendering_is_byte_identical() {
    let frozen = frozen_now();
    assert_eq!(
        render_text_at(&no_daemon_view(None, None), frozen),
        "no daemon serving this database"
    );
    assert_eq!(
        render_text_at(&no_daemon_view(Some(7), None), frozen),
        "no daemon serving this database (lock holder pid 7)"
    );
    assert_eq!(
        render_text_at(&no_daemon_view(Some(7), Some(false)), frozen),
        "no daemon serving this database (lock holder pid 7)",
        "Some(false) is the absent fact (B6: never minted), not a posture"
    );
}

/// US-3/S3 (OP3, journey plan AC-3's payload guard) — the draining
/// line is a text-render fact only: the JSON payload of the draining
/// view carries the byte-additive B6 shape (`daemon`, `draining`,
/// `holder_pid` and nothing else), unchanged by the renderer work.
#[test]
fn us3_s3_the_json_payload_stays_the_byte_additive_b6_shape() {
    let view = no_daemon_view(Some(4242), Some(true));
    let json = serde_json::to_value(&view).expect("json");
    let keys = json
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(keys, ["daemon", "draining", "holder_pid"]);
}

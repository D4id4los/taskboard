// SPDX-License-Identifier: AGPL-3.0-only

//! The `export --what pdf` journeys (#624 stage B): real binary, real
//! sqlite tempdir, no network (a standalone core performs none).
//! Routing only — the layout geometry and the renderer have their own
//! tiers. Compiled only with the `pdf-export` feature; the default
//! build's contract (a clap usage error for `--what pdf`) is the CI
//! tree-hygiene step's, not a runtime path.

#![cfg(feature = "pdf-export")]

mod common;

use common::{binary_command, tempdir};

fn standalone_config_file(dir: &std::path::Path) -> std::path::PathBuf {
    let config = dir.join("taskboard.toml");
    let db = dir.join("taskboard.db");
    let toml = format!(
        "[nextcloud]\ncredential_store = 'env'\n[storage]\ndb_path = '{}'\n\
         [app]\nshutdown_timeout_secs = 5\n",
        db.display()
    );
    std::fs::write(&config, toml).expect("config written");
    config
}

fn run(config: &std::path::Path, args: &[&str]) -> std::process::Output {
    binary_command()
        .arg("--config")
        .arg(config)
        .args(args)
        .output()
        .expect("the binary runs")
}

/// The happy path: `--out` writes the file (typed, never a stdout
/// dump — stdout stays empty in text mode), `--no-open` skips the
/// viewer, exit 0, and the file really is a pdf.
#[test]
fn pdf_export_writes_the_file_with_no_open() {
    let dir = tempdir();
    let config = standalone_config_file(dir.path());
    let out = dir.path().join("sheet.pdf");
    let output = run(
        &config,
        &[
            "export",
            "--what",
            "pdf",
            "--out",
            out.to_str().unwrap(),
            "--no-open",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "export exits 0");
    // stdout stays the payload lane: the receipt's text render is
    // exactly the path — never pdf bytes, never a silent dump.
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert_eq!(stdout.trim(), out.to_str().unwrap());
    let bytes = std::fs::read(&out).expect("the file exists");
    assert!(bytes.starts_with(b"%PDF-"));
}

/// `--json` carries the receipt on the normal envelope: path, pages ≥
/// 1, `opened: skipped` (the flag was passed).
#[test]
fn pdf_export_json_receipt_is_structural() {
    let dir = tempdir();
    let config = standalone_config_file(dir.path());
    let out = dir.path().join("receipt.pdf");
    let output = run(
        &config,
        &[
            "--json",
            "export",
            "--what",
            "pdf",
            "--out",
            out.to_str().unwrap(),
            "--no-open",
        ],
    );
    assert_eq!(output.status.code(), Some(0));
    let receipt: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON receipt on stdout");
    assert!(receipt["path"].is_string());
    assert!(
        receipt["pages"].as_u64().is_some_and(|pages| pages >= 1),
        "at least one page renders: {receipt}"
    );
    assert_eq!(receipt["opened"], "skipped");
}

/// The data-family filters (`--since` etc.) are a typed refusal under
/// the pdf leg: exit 1, nothing on stdout.
#[test]
fn pdf_export_refuses_data_family_filters() {
    let dir = tempdir();
    let config = standalone_config_file(dir.path());
    let out = dir.path().join("no.pdf");
    let output = run(
        &config,
        &[
            "export",
            "--what",
            "pdf",
            "--out",
            out.to_str().unwrap(),
            "--no-open",
            "--since",
            "2026-01-01",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout.len(), 0, "nothing on stdout");
    assert!(!out.exists());
}

/// The pdf-only options are a typed refusal under the data families:
/// exit 1, nothing on stdout.
#[test]
fn pdf_options_are_refused_outside_the_pdf_leg() {
    let dir = tempdir();
    let config = standalone_config_file(dir.path());
    let out = dir.path().join("no.pdf");
    let output = run(
        &config,
        &[
            "export",
            "--what",
            "tracking",
            "--out",
            out.to_str().unwrap(),
            "--layout",
            "a4",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout.len(), 0, "nothing on stdout");
    assert!(!out.exists());
}

/// An unknown `[export] pdf_layout` key is a typed refusal (never
/// silently replaced by the default): exit 1, no file.
#[test]
fn an_unknown_config_layout_is_refused() {
    let dir = tempdir();
    let config = standalone_config_file(dir.path());
    let mut toml = std::fs::read_to_string(&config).expect("readable");
    toml.push_str("[export]\npdf_layout = 'letter'\n");
    std::fs::write(&config, toml).expect("writable");
    let out = dir.path().join("no.pdf");
    let output = run(
        &config,
        &[
            "export",
            "--what",
            "pdf",
            "--out",
            out.to_str().unwrap(),
            "--no-open",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(!out.exists());
}

/// The config default destination: no `--out`, `[export] pdf_dir`
/// names the directory (created on demand), the file name carries the
/// date and the layout key.
#[test]
fn the_config_default_destination_is_used_without_out() {
    let dir = tempdir();
    let config = standalone_config_file(dir.path());
    let pdf_dir = dir.path().join("prints/nested");
    let mut toml = std::fs::read_to_string(&config).expect("readable");
    let export_toml = format!(
        "[export]\npdf_dir = '{}'\npdf_layout = 'a5'\n",
        pdf_dir.display()
    );
    toml.push_str(&export_toml);
    std::fs::write(&config, toml).expect("writable");
    let output = run(&config, &["--json", "export", "--what", "pdf", "--no-open"]);
    assert_eq!(output.status.code(), Some(0));
    let receipt: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON receipt");
    let path = std::path::PathBuf::from(receipt["path"].as_str().expect("a path"));
    assert_eq!(path.parent(), Some(pdf_dir.as_path()));
    assert!(path.exists());
    assert!(
        path.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("taskboard-") && n.ends_with("-a5.pdf"))
    );
}

/// A relative `--out` in the session lane is the typed
/// not-absolute refusal (the wire carries no cwd); here the daemon
/// does not run, so the one-shot lane resolves against its own cwd —
/// the session-lane refusal is covered by the deliver tier's unit
/// tests. What this pins: the one-shot happy path with a *relative*
/// destination lands in the cwd.
#[test]
fn pdf_export_resolves_a_relative_out_against_the_cwd() {
    let dir = tempdir();
    let config = standalone_config_file(dir.path());
    let out = dir.path().join("relative.pdf");
    let expected = std::env::current_dir().expect("cwd").join(&out);
    let output = run(
        &config,
        &[
            "export",
            "--what",
            "pdf",
            "--out",
            out.to_str().unwrap(),
            "--no-open",
        ],
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(
        expected.exists(),
        "the file landed at the cwd-resolved path"
    );
    let _ = std::fs::remove_file(expected);
}

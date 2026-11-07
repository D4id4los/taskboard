// SPDX-License-Identifier: MIT OR Apache-2.0

//! The settings editor's per-intent config-overlay writers (the GUI
//! settings editor's typed write port, plan §2): one writer per
//! editable field-group commit, each running *apply → prove → write*
//! over the shared [`crate::config::overlay_document`] /
//! [`crate::config::write_config_atomic`] skeleton (comments,
//! formatting, and unrelated tables preserved by construction; one
//! intent = one atomic rename).
//!
//! The prove gate (plan D3/D14) is this module's generalization over
//! the older per-intent writers: every editor write decodes the
//! resulting document through the file-layer figment chain and runs
//! [`AppConfig::validate`] *before* the atomic write, so the editor
//! never turns a bootable file into an unbootable one. Attribution:
//! an edit that introduces a failure on a previously-valid file is
//! refused typed (file untouched); against an already-invalid base
//! the refusal fires only when the failing field lives inside the
//! edited section — any other pre-existing failure warns and writes
//! (the editor must not be bricked by garbage it did not create).

use std::path::Path;

use toml_edit::DocumentMut;

use chrono::Timelike as _;
use taskboard_domain::ReminderOffset;

use crate::config::{ConfigError, SinkChoice};

/// A settings-overlay write's failure classes: shape/IO failures of
/// the shared skeleton, and the prove gate's typed refusal carrying
/// the config rejection it refused on (the handler maps the blamed
/// field onto the reply's typed lattice).
#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    /// The file could not be read, parsed, or written.
    #[error("config overlay io failed")]
    Io(#[source] std::io::Error),
    /// The prove gate refused: the resulting document fails
    /// [`AppConfig::validate`], and per the attribution rule this
    /// write is the blamed party (or edits into an already-failing
    /// section). The file is untouched. Boxed: the config error
    /// carries the figment source and would dominate the `Result`
    /// layout on every per-intent call.
    #[error("config overlay rejected")]
    Rejected(#[source] Box<ConfigError>),
}

/// The prove gate + atomic write, shared by every writer in this
/// module. `section`/`key` address the one cell this intent edits
/// (`toml_edit`-mutated, so the rest of the document survives
/// verbatim); the gate proves the *file as a whole* stays bootable.
fn prove_and_write(
    path: &Path,
    section: &str,
    key: &str,
    value: toml_edit::Item,
) -> Result<(), OverlayError> {
    let mut doc = crate::config::overlay_document(path).map_err(OverlayError::Io)?;
    // The base verdict, taken before the mutation: validity of the
    // file the editor found. A base decode failure is recorded, not
    // fatal — D4 warns and writes for unrelated pre-existing
    // garbage.
    let base_verdict = prove_verdict(&doc.to_string());
    {
        let table = doc
            .entry(section)
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_mut()
            .ok_or_else(|| {
                OverlayError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("[{section}] is not a table"),
                ))
            })?;
        table.insert(key, value);
    }
    let result = prove_verdict(&doc.to_string());
    match result {
        // The file as a whole still boots: write.
        Ok(()) => write(path, &doc),
        Err(result_err) => match base_verdict {
            // A previously-valid file: the edit introduced the
            // failure — refuse typed, file untouched.
            Ok(()) => Err(OverlayError::Rejected(result_err)),
            Err(base_err) => {
                let blamed_inside = result_err
                    .field()
                    .is_some_and(|field| field.starts_with(&format!("{section}.")));
                if blamed_inside {
                    // The edited section already fails and still
                    // fails: refuse rather than pretend the write
                    // fixed anything.
                    Err(OverlayError::Rejected(result_err))
                } else {
                    tracing::warn!(
                        base = %base_err.kind(),
                        result = %result_err.kind(),
                        section,
                        key,
                        "settings write over an already-invalid config: \
                         unrelated pre-existing failure, writing anyway"
                    );
                    write(path, &doc)
                }
            }
        },
    }
}

/// The prove verdict over one document's text: the file-layer decode
/// AND the semantic validation — a file that merely decodes but fails
/// [`AppConfig::validate`] does not boot, so the gate refuses on it.
fn prove_verdict(text: &str) -> Result<(), Box<ConfigError>> {
    crate::config::AppConfig::decode_file_only_str(text)
        .map_err(Box::new)
        .and_then(|config| config.validate().map_err(Box::new))
}

fn write(path: &Path, doc: &DocumentMut) -> Result<(), OverlayError> {
    crate::config::write_config_atomic(path, doc.to_string().as_str()).map_err(OverlayError::Io)
}

fn set_str(path: &Path, section: &str, key: &str, value: &str) -> Result<(), OverlayError> {
    prove_and_write(path, section, key, toml_edit::value(value))
}

fn set_secs(path: &Path, section: &str, key: &str, secs: u64) -> Result<(), OverlayError> {
    prove_and_write(
        path,
        section,
        key,
        toml_edit::value(i64::try_from(secs).unwrap_or(i64::MAX)),
    )
}

/// Writes `[preferences] day_start_time`. The rendered form is
/// `%H:%M` for whole minutes (the user-echo convention: the editor
/// never widens the spelling it was handed) and `%H:%M:%S` otherwise.
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_day_start(path: &Path, time: chrono::NaiveTime) -> Result<(), OverlayError> {
    let rendered = if time.second() == 0 && time.nanosecond() == 0 {
        time.format("%H:%M").to_string()
    } else {
        time.format("%H:%M:%S").to_string()
    };
    set_str(path, "preferences", "day_start_time", &rendered)
}

/// Writes `[reminders] enabled`.
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_reminders_enabled(path: &Path, enabled: bool) -> Result<(), OverlayError> {
    prove_and_write(path, "reminders", "enabled", toml_edit::value(enabled))
}

/// Writes `[reminders] tick_secs`.
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_reminder_tick(path: &Path, secs: u64) -> Result<(), OverlayError> {
    set_secs(path, "reminders", "tick_secs", secs)
}

/// Writes `[reminders] default_before` in the offset's canonical text
/// form (the domain's own serializer — no UI-side spelling fork).
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_reminder_before(path: &Path, before: ReminderOffset) -> Result<(), OverlayError> {
    set_str(path, "reminders", "default_before", &before.text_form())
}

/// Writes `[reminders] sink` (the config word: `auto`/`log`/`desktop`).
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_reminder_sink(path: &Path, sink: SinkChoice) -> Result<(), OverlayError> {
    let word = match sink {
        SinkChoice::Auto => "auto",
        SinkChoice::Log => "log",
        SinkChoice::Desktop => "desktop",
    };
    set_str(path, "reminders", "sink", word)
}

/// Writes `[sync] poll_interval_secs`.
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_poll_interval(path: &Path, secs: u64) -> Result<(), OverlayError> {
    set_secs(path, "sync", "poll_interval_secs", secs)
}

/// Writes `[sync] backoff_initial_secs`. The `initial ≤ max` ordering
/// is NOT enforced here — it is the prove gate's whole-document
/// verdict that refuses a crossing (plan D8: the error lands at the
/// gate, attributed by the handler).
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_backoff_initial(path: &Path, secs: u64) -> Result<(), OverlayError> {
    set_secs(path, "sync", "backoff_initial_secs", secs)
}

/// Writes `[sync] backoff_max_secs`. The ordering refuses at the gate
/// symmetric to [`set_backoff_initial`].
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_backoff_max(path: &Path, secs: u64) -> Result<(), OverlayError> {
    set_secs(path, "sync", "backoff_max_secs", secs)
}

/// Writes `[ui] due_soon_days` (0 is legal — an empty soon set; the
/// ceiling refuses at the gate).
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_due_soon_days(path: &Path, days: u32) -> Result<(), OverlayError> {
    prove_and_write(
        path,
        "ui",
        "due_soon_days",
        toml_edit::value(i64::from(days)),
    )
}

/// Writes `[focus] enabled`.
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_focus_enabled(path: &Path, enabled: bool) -> Result<(), OverlayError> {
    prove_and_write(path, "focus", "enabled", toml_edit::value(enabled))
}

/// Writes `[focus] work_mins`. The `1..=240` bound refuses at the
/// prove gate (typed `invalid-focus-timing` blaming this key).
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_focus_work_mins(path: &Path, mins: u64) -> Result<(), OverlayError> {
    set_secs(path, "focus", "work_mins", mins)
}

/// Writes `[focus] break_mins`. The bound refuses at the gate,
/// symmetric to [`set_focus_work_mins`].
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_focus_break_mins(path: &Path, mins: u64) -> Result<(), OverlayError> {
    set_secs(path, "focus", "break_mins", mins)
}

/// Writes `[focus] chain` (the auto-chain knob, #600).
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_focus_chain(path: &Path, enabled: bool) -> Result<(), OverlayError> {
    prove_and_write(path, "focus", "chain", toml_edit::value(enabled))
}

/// Writes `[focus] long_break_mins`. The bound refuses at the gate,
/// symmetric to [`set_focus_work_mins`].
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_focus_long_break_mins(path: &Path, mins: u64) -> Result<(), OverlayError> {
    set_secs(path, "focus", "long_break_mins", mins)
}

/// Writes `[focus] long_break_every`. The `1..=24` bound refuses at
/// the prove gate, blaming this key.
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_focus_long_break_every(path: &Path, every: u64) -> Result<(), OverlayError> {
    set_secs(path, "focus", "long_break_every", every)
}

/// Writes `[focus] notify` (the boundary-notification knob, #600).
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_focus_notify(path: &Path, enabled: bool) -> Result<(), OverlayError> {
    prove_and_write(path, "focus", "notify", toml_edit::value(enabled))
}

/// Writes `[files_sync] enabled` (round-5 chunk 8, D-8.6) — the only
/// key of the table this UI writes; `poll_secs` is never touched.
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_files_sync_enabled(path: &Path, enabled: bool) -> Result<(), OverlayError> {
    prove_and_write(path, "files_sync", "enabled", toml_edit::value(enabled))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(dir: &tempfile::TempDir, contents: &str) -> std::path::PathBuf {
        let path = dir.path().join("taskboard.toml");
        std::fs::write(&path, contents).expect("seed config");
        path
    }

    const COMMENTED: &str = "# operator comment\n\
        [ui]\nscale = 1.5\n\
        [reminders]\ntick_secs = 30\ndefault_before = \"5m\"\n\
        [sync]\npoll_interval_secs = 30\nbackoff_initial_secs = 5\nbackoff_max_secs = 300\n\
        [nextcloud]\n# keep me\ncredential_store = \"file\"\n";

    /// The nine writers land their typed value, preserve the
    /// comments and every unrelated table, and the written file
    /// decodes carrying a value that differs from the default (W-1..W-7,
    /// table-driven: one preservation proof, one presence proof per
    /// writer).
    #[test]
    fn every_writer_lands_its_value_and_preserves_the_document() {
        type Writer = Box<dyn Fn(&std::path::Path) -> Result<(), OverlayError>>;
        let cases: Vec<(&str, Writer)> = vec![
            (
                "preferences.day_start_time",
                Box::new(|p| {
                    set_day_start(p, chrono::NaiveTime::from_hms_opt(5, 0, 0).expect("t"))
                }),
            ),
            (
                "reminders.enabled",
                Box::new(|p| set_reminders_enabled(p, false)),
            ),
            (
                "reminders.tick_secs",
                Box::new(|p| set_reminder_tick(p, 45)),
            ),
            (
                "reminders.default_before",
                Box::new(|p| {
                    set_reminder_before(
                        p,
                        ReminderOffset::parse_text_form("1h30m").expect("parses"),
                    )
                }),
            ),
            (
                "reminders.sink",
                Box::new(|p| set_reminder_sink(p, SinkChoice::Desktop)),
            ),
            (
                "sync.poll_interval_secs",
                Box::new(|p| set_poll_interval(p, 60)),
            ),
            (
                "sync.backoff_initial_secs",
                Box::new(|p| set_backoff_initial(p, 10)),
            ),
            (
                "sync.backoff_max_secs",
                Box::new(|p| set_backoff_max(p, 600)),
            ),
            ("ui.due_soon_days", Box::new(|p| set_due_soon_days(p, 14))),
        ];
        let defaults = serde_json::to_value(crate::config::AppConfig::default()).expect("serial");
        for (dotted, write) in cases {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = seed(&dir, COMMENTED);
            write(&path).unwrap_or_else(|e| panic!("{dotted}: write refused: {e}"));

            let written = std::fs::read_to_string(&path).expect("read back");
            assert!(
                written.contains("# operator comment") && written.contains("# keep me"),
                "{dotted}: comments survived"
            );
            assert!(written.contains("scale = 1.5"), "{dotted}: [ui] survives");
            assert!(
                written.contains("credential_store = \"file\""),
                "{dotted}: [nextcloud] survives"
            );
            let decoded =
                crate::config::AppConfig::decode_file_only_str(&written).expect("decodes");
            let landed = serde_json::to_value(&decoded).expect("serializes");
            let (section, key) = dotted.split_once('.').expect("dotted");
            assert_ne!(
                &landed[section][key], &defaults[section][key],
                "{dotted}: the written leaf differs from the default"
            );
        }
    }

    /// The written values decode to the exact typed values the
    /// writers were handed (the typed round-trip; the preservation
    /// test above only proves presence-by-difference).
    #[test]
    fn the_written_values_round_trip_to_the_handed_typed_values() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, COMMENTED);
        set_day_start(&path, chrono::NaiveTime::from_hms_opt(5, 30, 0).expect("t")).expect("lands");
        set_reminders_enabled(&path, false).expect("lands");
        set_reminder_tick(&path, 45).expect("lands");
        set_reminder_before(
            &path,
            ReminderOffset::parse_text_form("1h30m").expect("parses"),
        )
        .expect("lands");
        set_reminder_sink(&path, SinkChoice::Desktop).expect("lands");
        set_poll_interval(&path, 60).expect("lands");
        set_backoff_initial(&path, 10).expect("lands");
        set_backoff_max(&path, 600).expect("lands");
        set_due_soon_days(&path, 14).expect("lands");

        let written = std::fs::read_to_string(&path).expect("read back");
        let decoded = crate::config::AppConfig::decode_file_only_str(&written).expect("decodes");
        decoded.validate().expect("the written config stays valid");
        assert_eq!(
            decoded
                .preferences
                .day_start_time
                .parse::<chrono::NaiveTime>()
                .expect("parses"),
            chrono::NaiveTime::from_hms_opt(5, 30, 0).expect("t")
        );
        assert!(!decoded.reminders.enabled);
        assert_eq!(decoded.reminders.tick_secs, 45);
        assert_eq!(
            ReminderOffset::parse_text_form(&decoded.reminders.default_before)
                .expect("parses")
                .as_duration(),
            ReminderOffset::parse_text_form("1h30m")
                .expect("parses")
                .as_duration()
        );
        assert_eq!(decoded.reminders.sink, SinkChoice::Desktop);
        assert_eq!(decoded.sync.poll_interval_secs, 60);
        assert_eq!(decoded.sync.backoff_initial_secs, 10);
        assert_eq!(decoded.sync.backoff_max_secs, 600);
        assert_eq!(decoded.ui.due_soon_days, 14);
        // The whole-document writes preserved the untouched keys.
        assert!((decoded.ui.scale - 1.5).abs() < f64::EPSILON);
        assert_eq!(
            decoded.nextcloud.credential_store,
            crate::config::CredentialStoreKind::File
        );
    }

    /// A write against an absent file creates it with the section
    /// and the key — no husk tables beyond the one written subtree
    /// (W-1's absent-file pin; the file decodes after).
    #[test]
    fn a_write_on_an_absent_file_creates_it_and_decodes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        set_day_start(
            &path,
            chrono::NaiveTime::from_hms_opt(23, 15, 0).expect("t"),
        )
        .expect("creates");
        let written = std::fs::read_to_string(&path).expect("created");
        let decoded = crate::config::AppConfig::decode_file_only_str(&written).expect("decodes");
        decoded.validate().expect("valid");
        assert_eq!(
            decoded
                .preferences
                .day_start_time
                .parse::<chrono::NaiveTime>()
                .expect("parses"),
            chrono::NaiveTime::from_hms_opt(23, 15, 0).expect("t")
        );
    }

    /// The gate's arm (a) + the cross-field red rule (W-9): raising
    /// `backoff_initial` past `backoff_max` is typed-refused on an
    /// otherwise-valid file — typed scalar inputs alone cannot
    /// express the ordering, only the whole-document prove can — and
    /// the file is byte-untouched.
    #[test]
    fn a_crossing_edit_refuses_typed_and_touches_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, COMMENTED);
        let before = std::fs::read_to_string(&path).expect("read");

        let err = set_backoff_initial(&path, 400).expect_err("the gate refuses the crossing");
        assert!(
            matches!(err, OverlayError::Rejected(_)),
            "the refusal is typed Rejected, got {err:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            before,
            "the file is byte-untouched"
        );

        // Symmetric arm: lowering max below the standing initial.
        let err = set_backoff_max(&path, 3).expect_err("the gate refuses the crossing");
        assert!(matches!(err, OverlayError::Rejected(_)));
        assert_eq!(std::fs::read_to_string(&path).expect("read"), before);
    }

    /// The gate's arm (a), single-field shape: an edit that
    /// introduces a failure in its own section refuses typed and the
    /// refusal names the blamed field.
    #[test]
    fn an_introduced_failure_refuses_with_the_blamed_field() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, COMMENTED);

        let err = set_reminder_tick(&path, 0).expect_err("zero tick refuses");
        let OverlayError::Rejected(config_err) = &err else {
            panic!("typed Rejected, got {err:?}");
        };
        assert_eq!(config_err.field(), Some("reminders.tick_secs"));
        // A minted `NaiveTime` cannot produce an unparseable day
        // start — the typed-input rule makes that edit
        // unrepresentable (the gate's real catch is the cross-field
        // and interaction shapes pinned above).
    }

    /// The unknown-key disposition: the config's sections deny
    /// unknown fields, so a file carrying one is already a
    /// boot-blocker — the editor neither drops the key nor corrupts
    /// the file. The gate's warn-and-write arm lands the edit with
    /// the unknown key preserved verbatim (`toml_edit` round-trips
    /// what it did not parse), and the file is no less bootable than
    /// the editor found it.
    #[test]
    fn an_unknown_key_survives_an_unrelated_edit_verbatim() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(
            &dir,
            "# operator note\n[ui]\nmystery_key = true\nscale = 1.0\n[sync]\npoll_interval_secs = 30\n",
        );
        // The seed genuinely fails the file-layer decode (unknown
        // fields are denied), so both gate sides run the
        // pre-existing-garbage arm.
        assert!(matches!(
            crate::config::AppConfig::decode_file_only_str(
                &std::fs::read_to_string(&path).expect("read")
            ),
            Err(crate::config::ConfigError::Decode(_))
        ));

        set_poll_interval(&path, 60).expect("the unrelated edit lands");
        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(
            written.contains("mystery_key = true") && written.contains("# operator note"),
            "the unknown key and the comment survive verbatim: {written}"
        );
        assert!(
            matches!(
                crate::config::AppConfig::decode_file_only_str(&written),
                Err(crate::config::ConfigError::Decode(_))
            ),
            "the file is no more bootable than the editor found it"
        );
    }

    /// The gate's arm (b) (W-9, D4): a pre-existing invalid section
    /// does not brick unrelated edits — the write lands warningly.
    /// The seeded file fails validation at `reminders.tick_secs = 0`;
    /// the edit targets `[ui]`.
    #[test]
    fn an_unrelated_preexisting_failure_warns_and_writes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(
            &dir,
            "[reminders]\ntick_secs = 0\n[ui]\nscale = 1.0\n[nextcloud]\ncredential_store = \"file\"\n",
        );
        // The seed genuinely fails validation, at the reminders leaf.
        let seeded = crate::config::AppConfig::decode_file_only_str(
            &std::fs::read_to_string(&path).expect("read"),
        )
        .expect("decodes");
        assert_eq!(
            seeded.validate().expect_err("the seed is invalid").field(),
            Some("reminders.tick_secs")
        );

        set_due_soon_days(&path, 7).expect("the unrelated edit lands");
        let decoded = crate::config::AppConfig::decode_file_only_str(
            &std::fs::read_to_string(&path).expect("read"),
        )
        .expect("decodes");
        assert_eq!(decoded.ui.due_soon_days, 7, "the edit landed");
        assert_eq!(decoded.reminders.tick_secs, 0, "the garbage is untouched");
    }

    /// The gate's arm (c): a pre-existing invalid *edited section*
    /// refuses — editing into a section that already fails is a
    /// refusal, not a silent write-over.
    #[test]
    fn an_edit_into_an_already_failing_section_refuses() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(
            &dir,
            "[reminders]\ntick_secs = 0\n[ui]\nscale = 1.0\n[nextcloud]\ncredential_store = \"file\"\n",
        );

        let err = set_reminders_enabled(&path, false).expect_err("the same-section edit refuses");
        assert!(matches!(err, OverlayError::Rejected(_)));
        let decoded = crate::config::AppConfig::decode_file_only_str(
            &std::fs::read_to_string(&path).expect("read"),
        )
        .expect("decodes");
        assert!(decoded.reminders.enabled, "the file is untouched");
    }

    /// A non-table section (a bare value where a table belongs) is a
    /// typed shape refusal, never a panic.
    #[test]
    fn a_non_table_section_is_a_typed_shape_refusal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, "reminders = 3\n");
        let err = set_reminder_tick(&path, 45).expect_err("shape refusal");
        assert!(
            matches!(err, OverlayError::Io(ref e) if e.kind() == std::io::ErrorKind::InvalidData),
            "got {err:?}"
        );
    }

    // The env-override truth tables (W-10/W-11) live beside the diff
    // core they pin in `config.rs`'s test module; the writers here
    // consume the set only through the handler layer.

    // -- the [focus] writers (the focus settings editor, #625) ---------

    /// A minute write lands its typed value; garbage (0) is refused
    /// typed at the gate and the file stays byte-identical — never a
    /// silent clamp at any layer.
    #[test]
    fn focus_minute_writes_and_the_gate_refuses_garbage() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, COMMENTED);
        set_focus_work_mins(&path, 50).expect("the minute write lands");
        let decoded = crate::config::AppConfig::decode_file_only_str(
            &std::fs::read_to_string(&path).expect("read back"),
        )
        .expect("decodes");
        assert_eq!(decoded.focus.work_mins, 50);

        let before = std::fs::read_to_string(&path).expect("read");
        let err = set_focus_break_mins(&path, 0).expect_err("zero minutes refuse");
        assert!(
            matches!(err, OverlayError::Rejected(_)),
            "the refusal is typed Rejected, got {err:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            before,
            "the file is byte-untouched"
        );
    }

    /// The `long_break_every` bound refuses at the gate with the typed
    /// config rejection blaming the edited key.
    #[test]
    fn focus_every_bound_refuses_at_the_gate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, COMMENTED);
        let err = set_focus_long_break_every(&path, 25).expect_err("above the every bound refuses");
        let OverlayError::Rejected(config_err) = &err else {
            panic!("typed Rejected, got {err:?}");
        };
        assert_eq!(config_err.field(), Some("focus.long_break_every"));
    }

    /// The three bool knobs round-trip both ways.
    #[test]
    fn focus_bool_writes_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, COMMENTED);
        set_focus_enabled(&path, true).expect("lands");
        set_focus_chain(&path, true).expect("lands");
        set_focus_notify(&path, false).expect("lands");
        let decoded = crate::config::AppConfig::decode_file_only_str(
            &std::fs::read_to_string(&path).expect("read back"),
        )
        .expect("decodes");
        decoded.validate().expect("the written config stays valid");
        assert!(decoded.focus.enabled);
        assert!(decoded.focus.chain);
        assert!(!decoded.focus.notify);
    }
}

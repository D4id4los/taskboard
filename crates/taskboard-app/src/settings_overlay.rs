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
    prove_and_write_with(path, section, |table| {
        table.insert(key, value);
        Ok(())
    })
}

/// The prove gate + atomic write over a multi-key edit: one document
/// load, all mutations, one verdict, one write — a composite user
/// action lands as one user-visible write.
fn prove_and_write_with(
    path: &Path,
    section: &str,
    edit: impl FnOnce(&mut toml_edit::Table) -> Result<(), OverlayError>,
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
        edit(table)?;
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

/// One batch edit's address: the `(section, key)` cell and the typed
/// value already minted as a `toml_edit` item (the batch surface's
/// parse step owns the typing — the writer only places what it is
/// handed).
pub struct BatchEdit {
    /// The target section (created when absent, `files_sync`-writer
    /// precedent).
    pub section: String,
    /// The target key inside the section.
    pub key: String,
    /// The typed value as a `toml_edit` item.
    pub item: toml_edit::Item,
}

/// The doc-level multi-section batch writer (plan D-BATCH-SHAPE, the
/// #910 atomic transaction): one document load, every `(section, key)`
/// mutation, **one** prove verdict, **one** atomic write. A failing
/// mid-batch key rolls back by construction — the write never
/// happened. The base-verdict/attribution rule is the per-intent
/// writers', generalized to the edited-section *set*: against a
/// previously-valid file any result failure is refused typed; against
/// an already-invalid base the refusal fires only when the blamed
/// field lives inside an edited section (the batch must not be bricked
/// by garbage it did not create), any other pre-existing failure
/// warns and writes.
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn prove_and_write_doc(path: &Path, edits: &[BatchEdit]) -> Result<(), OverlayError> {
    let mut doc = crate::config::overlay_document(path).map_err(OverlayError::Io)?;
    let base_verdict = prove_verdict(&doc.to_string());
    for edit in edits {
        let table = doc
            .entry(&edit.section)
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_mut()
            .ok_or_else(|| {
                OverlayError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("[{}] is not a table", edit.section),
                ))
            })?;
        table.insert(&edit.key, edit.item.clone());
    }
    let result = prove_verdict(&doc.to_string());
    match result {
        // The file as a whole still boots: write.
        Ok(()) => write(path, &doc),
        Err(result_err) => match base_verdict {
            // A previously-valid file: the batch introduced the
            // failure — refuse typed, file untouched.
            Ok(()) => Err(OverlayError::Rejected(result_err)),
            Err(base_err) => {
                let blamed_inside = result_err.field().is_some_and(|field| {
                    edits
                        .iter()
                        .any(|edit| field.starts_with(&format!("{}.", edit.section)))
                });
                if blamed_inside {
                    Err(OverlayError::Rejected(result_err))
                } else {
                    tracing::warn!(
                        base = %base_err.kind(),
                        result = %result_err.kind(),
                        sections = ?edits.iter().map(|edit| edit.section.as_str()).collect::<Vec<_>>(),
                        "batch write over an already-invalid config: \
                         unrelated pre-existing failure, writing anyway"
                    );
                    write(path, &doc)
                }
            }
        },
    }
}

/// The doc-level multi-removal writer (the batch-unset twin of
/// [`prove_and_write_doc`], plan D-WRITER): one document load, one
/// removal per `(section, key)` pair reusing [`remove_key`]'s
/// in-document semantics — an absent key (or absent section) is an
/// idempotent no-op for that pair, and no empty table is minted —
/// **one** prove verdict, **one** atomic write. Removals are a
/// `(section, key)` tuple slice: a removal carries no value item, so
/// a `BatchEdit`-style payload struct would carry a permanently-`None`
/// field. A failing mid-batch key rolls back by construction — the
/// write never happened. The base-verdict/attribution rule is
/// [`prove_and_write_doc`]'s, generalized to the removed-section set.
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn prove_and_remove_doc(
    path: &Path,
    removals: &[(String, String)],
) -> Result<(), OverlayError> {
    let mut doc = crate::config::overlay_document(path).map_err(OverlayError::Io)?;
    let base_verdict = prove_verdict(&doc.to_string());
    for (section, key) in removals {
        if let Some(table) = doc.get_mut(section).and_then(|item| item.as_table_mut()) {
            table.remove(key);
        }
    }
    let result = prove_verdict(&doc.to_string());
    match result {
        // The file as a whole still boots: write.
        Ok(()) => write(path, &doc),
        Err(result_err) => match base_verdict {
            // A previously-valid file: the batch introduced the
            // failure — refuse typed, file untouched.
            Ok(()) => Err(OverlayError::Rejected(result_err)),
            Err(base_err) => {
                let blamed_inside = result_err.field().is_some_and(|field| {
                    removals
                        .iter()
                        .any(|(section, _)| field.starts_with(&format!("{section}.")))
                });
                if blamed_inside {
                    Err(OverlayError::Rejected(result_err))
                } else {
                    tracing::warn!(
                        base = %base_err.kind(),
                        result = %result_err.kind(),
                        sections = ?removals.iter().map(|(section, _)| section.as_str()).collect::<Vec<_>>(),
                        "batch removal over an already-invalid config: \
                         unrelated pre-existing failure, writing anyway"
                    );
                    write(path, &doc)
                }
            }
        },
    }
}

/// The rendered `day_start_time` form: `%H:%M` for whole minutes (the
/// user-echo convention: a write never widens the spelling it was
/// handed) and `%H:%M:%S` otherwise. One definition — the per-intent
/// writer and the batch writer both render through it.
pub(crate) fn render_day_start(time: chrono::NaiveTime) -> String {
    use chrono::Timelike as _;
    if time.second() == 0 && time.nanosecond() == 0 {
        time.format("%H:%M").to_string()
    } else {
        time.format("%H:%M:%S").to_string()
    }
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
    let rendered = render_day_start(time);
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

/// Writes `[files_sync] enabled` and, when re-anchoring (round-6
/// D-6.6), `[files_sync] anchor` — one editor transaction, one
/// user-visible write (the commit-on-pick toggle: enabled and anchor
/// change as one user action, never a torn intermediate file state).
/// `poll_secs` is never touched.
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_files_sync_enabled(
    path: &Path,
    enabled: bool,
    anchor: Option<&str>,
) -> Result<(), OverlayError> {
    prove_and_write_with(path, "files_sync", |table| {
        table.insert("enabled", toml_edit::value(enabled));
        if let Some(name) = anchor {
            table.insert("anchor", toml_edit::value(name));
        }
        Ok(())
    })
}

/// Removes one key from its section (the `config unset` twin of the
/// setters, journey W-4): the prove gate runs on the *post-removal*
/// document — a removal that would break a previously-bootable file is
/// refused typed and the file is untouched, byte-identical to the
/// setters' contract. An absent key (or absent section) is an
/// idempotent no-op: nothing writes, no empty table is minted.
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn remove_key(path: &Path, section: &str, key: &str) -> Result<(), OverlayError> {
    let doc = crate::config::overlay_document(path).map_err(OverlayError::Io)?;
    let present = doc
        .get(section)
        .and_then(|item| item.as_table())
        .is_some_and(|table| table.contains_key(key));
    drop(doc);
    if !present {
        return Ok(());
    }
    prove_and_write_with(path, section, |table| {
        table.remove(key);
        Ok(())
    })
}

/// Writes `[files_sync] anchor` (round-6 D-6.6): the connector whose
/// Nextcloud account the lane mirrors. Creates the table if absent.
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_files_sync_anchor(path: &Path, name: &str) -> Result<(), OverlayError> {
    prove_and_write(path, "files_sync", "anchor", toml_edit::value(name))
}

/// Writes `[files_sync] poll_secs` (the #1051 writer twin of the #673
/// reader-side re-arm): the files-sync actor's per-cycle cadence
/// override. Creates the table when absent. Any ordering/upper-bound
/// rule is NOT enforced here — it is the prove gate's whole-document
/// verdict, symmetric to [`set_backoff_initial`].
///
/// # Errors
///
/// [`OverlayError`] — see the module docs.
pub fn set_files_sync_poll_secs(path: &Path, secs: u64) -> Result<(), OverlayError> {
    set_secs(path, "files_sync", "poll_secs", secs)
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

    const FILES_SEED: &str = "# operator comment\n\
        [[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
        server_url = \"https://c.example\"\nusername = \"alice\"\n\
        [[sync.connectors]]\nname = \"home\"\nbackend = \"nextcloud\"\n\
        server_url = \"https://d.example\"\nusername = \"bob\"\n\
        [files_sync]\nenabled = false\n# keep me\npoll_secs = 7\n";

    /// The anchor writer lands the key, preserves the comments and
    /// `poll_secs`, and the written file validates (round-6 D-6.6).
    #[test]
    fn the_anchor_writer_lands_its_key_and_preserves_the_document() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, FILES_SEED);
        set_files_sync_anchor(&path, "home").expect("lands");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("# operator comment"));
        assert!(text.contains("# keep me"));
        let decoded = crate::config::AppConfig::decode_file_only_str(&text).expect("decodes");
        decoded.validate().expect("the written config stays valid");
        assert_eq!(decoded.files_sync.anchor.as_deref(), Some("home"));
        assert_eq!(decoded.files_sync.poll_secs, Some(7), "poll_secs survives");
        assert!(!decoded.files_sync.enabled);
    }

    /// The composite toggle write lands both keys in one transaction;
    /// the `None` arm leaves a pre-existing anchor untouched.
    #[test]
    fn the_enabled_writer_lands_the_anchor_only_when_reanchoring() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, FILES_SEED);
        set_files_sync_enabled(&path, true, Some("home")).expect("lands");
        let decoded = crate::config::AppConfig::decode_file_only_str(
            &std::fs::read_to_string(&path).expect("read"),
        )
        .expect("decodes");
        decoded.validate().expect("the written config stays valid");
        assert!(decoded.files_sync.enabled);
        assert_eq!(decoded.files_sync.anchor.as_deref(), Some("home"));

        set_files_sync_enabled(&path, false, None).expect("lands");
        let decoded = crate::config::AppConfig::decode_file_only_str(
            &std::fs::read_to_string(&path).expect("read"),
        )
        .expect("decodes");
        assert!(!decoded.files_sync.enabled);
        assert_eq!(
            decoded.files_sync.anchor.as_deref(),
            Some("home"),
            "None leaves the anchor key untouched"
        );
    }

    /// The anchor writer creates the `[files_sync]` subtree on a
    /// file without the table (the #721 writer semantics this chunk
    /// inherits); the prove gate still binds the anchor to a real
    /// connector.
    #[test]
    fn the_anchor_writer_creates_the_table_when_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(
            &dir,
            "[[sync.connectors]]\nname = \"work\"\nbackend = \"nextcloud\"\n\
             server_url = \"https://c.example\"\nusername = \"alice\"\n",
        );
        set_files_sync_anchor(&path, "work").expect("lands");
        let decoded = crate::config::AppConfig::decode_file_only_str(
            &std::fs::read_to_string(&path).expect("read"),
        )
        .expect("decodes");
        assert_eq!(decoded.files_sync.anchor.as_deref(), Some("work"));
    }

    /// #1051: the `poll_secs` writer lands its typed value, creates the
    /// `[files_sync]` table when absent, and preserves the siblings
    /// and comments (mirroring the `poll_secs survives` test in
    /// `set_files_sync_enabled`'s suite).
    #[test]
    fn set_files_sync_poll_secs_lands_the_value_and_preserves_siblings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, COMMENTED);
        set_files_sync_poll_secs(&path, 90).expect("lands");
        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(
            written.contains("# operator comment") && written.contains("# keep me"),
            "comments survived: {written}"
        );
        assert!(written.contains("scale = 1.5"), "[ui] survives");
        assert!(
            written.contains("backoff_initial_secs = 5"),
            "[sync] siblings survive"
        );
        let decoded = crate::config::AppConfig::decode_file_only_str(&written).expect("decodes");
        decoded.validate().expect("the written config stays valid");
        assert_eq!(decoded.files_sync.poll_secs, Some(90));
    }

    /// #1051's invalid-value rule at the prove gate: a `0` that
    /// arrived by any path is refused typed, the file untouched (the
    /// whole-document verdict, symmetric to the backoff pair).
    #[test]
    fn set_files_sync_poll_secs_zero_refuses_at_the_prove_gate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, COMMENTED);
        let before = std::fs::read_to_string(&path).expect("read");
        let err = set_files_sync_poll_secs(&path, 0).expect_err("the gate refuses");
        assert!(matches!(err, OverlayError::Rejected(_)), "{err:?}");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            before,
            "the file is untouched"
        );
    }

    // -- the removal writer (journey W-4, US-3/S2) -----------------------

    /// `us3_s2_remove_key_leaves_the_key_and_proves_bootable`: the
    /// removal writer drops the override, keeps every unrelated table
    /// and comment, and the post-removal document still proves
    /// bootable (the figment re-read sees the default again).
    #[test]
    fn us3_s2_remove_key_leaves_the_key_and_proves_bootable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, COMMENTED);
        remove_key(&path, "reminders", "tick_secs").expect("the removal lands");

        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(
            !written.contains("tick_secs"),
            "the override is gone: {written}"
        );
        assert!(
            written.contains("# operator comment") && written.contains("# keep me"),
            "comments survived: {written}"
        );
        assert!(written.contains("scale = 1.5"), "[ui] survives");
        assert!(
            written.contains("default_before"),
            "the sibling key survives: {written}"
        );
        let decoded =
            crate::config::AppConfig::decode_file_only_str(&written).expect("still decodes");
        decoded.validate().expect("the post-removal file boots");
    }

    /// The prove gate binds removals too: dropping the max backoff
    /// below the standing initial value crosses the ordering, so the
    /// removal refuses typed (`Rejected`) and the file is untouched.
    #[test]
    fn a_validity_crossing_removal_refuses_and_touches_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(
            &dir,
            "[sync]\npoll_interval_secs = 30\nbackoff_initial_secs = 400\nbackoff_max_secs = 400\n",
        );
        let err = remove_key(&path, "sync", "backoff_max_secs")
            .expect_err("the crossing removal refuses");
        assert!(
            matches!(err, OverlayError::Rejected(_)),
            "typed rejection: {err:?}"
        );
        let text = std::fs::read_to_string(&path).expect("the file survives");
        assert!(
            text.contains("backoff_max_secs = 400"),
            "byte-identical file: {text}"
        );
    }

    /// Removing an already-absent key is a typed success — the
    /// idempotent unset.
    #[test]
    fn removing_an_absent_key_is_an_idempotent_success() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, "[reminders]\ntick_secs = 30\n");
        remove_key(&path, "focus", "work_mins").expect("absent removal succeeds");
        let text = std::fs::read_to_string(&path).expect("read back");
        assert!(text.contains("tick_secs = 30"), "document intact: {text}");
        assert!(!text.contains("focus"), "no empty table created: {text}");
    }

    // -- the doc-level batch writer (journey W-4, US-2/S4) ----------------

    /// `us2_s4_batch_lands_one_atomic_document` (US-2 AC-1): a
    /// cross-section batch lands every pair through **one** document —
    /// one atomic rename — with the comments and every unrelated table
    /// preserved, and the written file decodes carrying all pairs.
    #[test]
    fn us2_s4_batch_lands_one_atomic_document() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, COMMENTED);
        let edits = vec![
            BatchEdit {
                section: "reminders".to_owned(),
                key: "tick_secs".to_owned(),
                item: toml_edit::value(45_i64),
            },
            BatchEdit {
                section: "focus".to_owned(),
                key: "work_mins".to_owned(),
                item: toml_edit::value(50_i64),
            },
            BatchEdit {
                section: "sync".to_owned(),
                key: "poll_interval_secs".to_owned(),
                item: toml_edit::value(60_i64),
            },
        ];
        prove_and_write_doc(&path, &edits).expect("the batch lands");

        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(
            written.contains("# operator comment") && written.contains("# keep me"),
            "comments survived: {written}"
        );
        assert!(written.contains("scale = 1.5"), "[ui] survives");
        assert!(
            written.contains("credential_store = \"file\""),
            "[nextcloud] survives"
        );
        let decoded = crate::config::AppConfig::decode_file_only_str(&written).expect("decodes");
        decoded.validate().expect("the written batch stays valid");
        assert_eq!(decoded.reminders.tick_secs, 45);
        assert_eq!(decoded.focus.work_mins, 50);
        assert_eq!(decoded.sync.poll_interval_secs, 60);
    }

    /// US-2 AC-4, the atomic rollback: one batch whose *later* pair
    /// crosses the whole-document validity (the backoff ordering)
    /// refuses typed and the file is byte-identical — the earlier,
    /// individually-valid pair is NOT landed (the write never
    /// happened).
    #[test]
    fn a_failing_mid_batch_key_rolls_back_the_whole_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, COMMENTED);
        let before = std::fs::read_to_string(&path).expect("read");
        let edits = vec![
            BatchEdit {
                section: "reminders".to_owned(),
                key: "tick_secs".to_owned(),
                item: toml_edit::value(45_i64),
            },
            BatchEdit {
                section: "sync".to_owned(),
                key: "backoff_initial_secs".to_owned(),
                item: toml_edit::value(400_i64),
            },
        ];
        let err =
            prove_and_write_doc(&path, &edits).expect_err("the crossing pair refuses the batch");
        let OverlayError::Rejected(config_err) = &err else {
            panic!("typed Rejected, got {err:?}");
        };
        assert_eq!(
            config_err.field(),
            Some("sync.backoff_initial_secs"),
            "the blame names the offending key's field"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            before,
            "the file is byte-identical — no half-provisioned batch"
        );
    }

    /// `us1_s4_batch_unset_lands_one_atomic_document` (US-1 AC-1/AC-4/
    /// AC-7): a cross-section removal batch lands every removal through
    /// **one** document — one atomic rename — with the comments and
    /// every unrelated table preserved; an absent key in the same
    /// batch is idempotent success; and a batch whose removal would
    /// cross the whole-document validity refuses typed with the file
    /// byte-identical (the earlier, individually-valid removal is NOT
    /// landed — the write never happened).
    #[test]
    fn us1_s4_batch_unset_lands_one_atomic_document() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(&dir, COMMENTED);
        let removals = vec![
            ("reminders".to_owned(), "tick_secs".to_owned()),
            ("sync".to_owned(), "poll_interval_secs".to_owned()),
            // Absent: idempotent success inside the same batch (AC-7).
            ("focus".to_owned(), "work_mins".to_owned()),
        ];
        prove_and_remove_doc(&path, &removals).expect("the batch lands");

        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(
            written.contains("# operator comment") && written.contains("# keep me"),
            "comments survived: {written}"
        );
        assert!(written.contains("scale = 1.5"), "[ui] survives");
        assert!(
            written.contains("credential_store = \"file\""),
            "[nextcloud] survives"
        );
        assert!(
            !written.contains("tick_secs") && !written.contains("poll_interval_secs"),
            "both overrides left the file: {written}"
        );
        let decoded = crate::config::AppConfig::decode_file_only_str(&written).expect("decodes");
        decoded.validate().expect("the written batch stays valid");

        // AC-4: a later removal crossing the ordering refuses typed and
        // the file is byte-identical — the earlier removal did not land.
        // The proven crossing shape: max at its default falls below the
        // standing initial (the `remove_key` refusal seed's shape).
        let dir2 = tempfile::tempdir().expect("tempdir");
        let path2 = seed(
            &dir2,
            "[reminders]\ntick_secs = 30\n\
             [sync]\npoll_interval_secs = 30\nbackoff_initial_secs = 400\nbackoff_max_secs = 400\n",
        );
        let before = std::fs::read_to_string(&path2).expect("read");
        let doomed = vec![
            ("reminders".to_owned(), "tick_secs".to_owned()),
            ("sync".to_owned(), "backoff_max_secs".to_owned()),
        ];
        let err = prove_and_remove_doc(&path2, &doomed).expect_err("the crossing removal refuses");
        assert!(matches!(err, OverlayError::Rejected(_)), "{err:?}");
        assert_eq!(
            std::fs::read_to_string(&path2).expect("read"),
            before,
            "the file is byte-identical — no half-deprovisioned batch"
        );
    }

    /// The batch attribution rule against an already-invalid base: a
    /// pre-existing failure in an *unedited* section warns and writes
    /// (the batch must not be bricked by garbage it did not create);
    /// a failure blamed inside an *edited* section refuses. Both
    /// seeded through `tick_secs = 0` (the arm-c seed's shape).
    #[test]
    fn the_batch_attribution_rule_follows_the_edited_section_set() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = seed(
            &dir,
            "[reminders]\ntick_secs = 0\n[ui]\nscale = 1.0\n[nextcloud]\ncredential_store = \"file\"\n",
        );
        // Arm 1: the failure is outside the edited sections — writes.
        let edits = vec![BatchEdit {
            section: "sync".to_owned(),
            key: "poll_interval_secs".to_owned(),
            item: toml_edit::value(60_i64),
        }];
        prove_and_write_doc(&path, &edits).expect("the unrelated edit lands");
        let decoded = crate::config::AppConfig::decode_file_only_str(
            &std::fs::read_to_string(&path).expect("read"),
        )
        .expect("decodes");
        assert_eq!(decoded.sync.poll_interval_secs, 60, "the edit landed");

        // Arm 2: the failure is blamed inside the edited section —
        // refuses typed, the file keeps the pre-batch leaf values.
        let before = std::fs::read_to_string(&path).expect("read");
        let edits = vec![BatchEdit {
            section: "reminders".to_owned(),
            key: "enabled".to_owned(),
            item: toml_edit::value(false),
        }];
        let err = prove_and_write_doc(&path, &edits).expect_err("the edited-section blame refuses");
        assert!(matches!(err, OverlayError::Rejected(_)), "{err:?}");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), before);
    }
}

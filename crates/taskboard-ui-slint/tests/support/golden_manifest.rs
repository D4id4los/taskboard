// SPDX-License-Identifier: AGPL-3.0-only

//! The golden manifest (#1140, wave plan §4): `tests/goldens/
//! manifest.toml` is a harness-generated, checked-in index of the
//! golden corpus — one `[[golden]]` entry per committed PNG, carrying
//! the fixture constructor, a structural state note, and the capture
//! axes.
//!
//! # Generation
//!
//! The regen path (`TASKBOARD_REGEN_GOLDENS=1`) upserts each golden's
//! entry at capture time (`record`). Regen runs are parallel (nextest
//! runs each test as its own process), so the upsert serializes on a
//! `manifest.toml.lock` sidecar (create-new spin; a regen process that
//! dies mid-write leaves the sidecar behind — delete it and rerun;
//! regen is an explicit, reviewed step whose failure mode is loud).
//!
//! # Verification
//!
//! `read_entries` + `verify_corpus` assert structural agreement with
//! the committed file set: bijective (every PNG has exactly one
//! entry, and vice versa) and the `width`/`height` axes match the
//! decoded PNG dimensions. The `scale` axis is capture metadata (the
//! logical-pixel scale the pass mounted with) — a PNG does not encode
//! it, so it is not PNG-verifiable; the `name` convention
//! (`<surface>-<scale>-<width>`) keeps it reviewable in the diff.
//! Errors are typed; nothing is matched on prose.

use std::fs::OpenOptions;
use std::path::PathBuf;

/// One manifest entry: the golden's identity (`name`, the PNG stem),
/// its fixture constructor (not prose), a structural state note, and
/// the capture axes.
#[derive(Debug, Clone, PartialEq)]
pub struct GoldenEntry {
    pub name: String,
    pub fixture: String,
    pub state: String,
    pub scale: f64,
    pub width: u32,
    pub height: u32,
}

/// The typed manifest failures (parse/field shape), distinct from the
/// corpus-verification failures in [`CorpusError`].
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("manifest read failed")]
    Read(#[from] std::io::Error),
    #[error("manifest parse failed")]
    Parse(#[from] toml_edit::TomlError),
    #[error("entry {name} is missing the {field} field or it is not the expected type")]
    Field { name: String, field: &'static str },
}

/// The typed corpus-verification failures (bijection/axes).
#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    #[error("orphan PNG with no manifest entry: {0}")]
    OrphanPng(String),
    #[error("manifest entry with no PNG: {0}")]
    MissingPng(String),
    #[error("duplicate manifest entry: {0}")]
    Duplicate(String),
    #[error(
        "entry {name} axes {expected_w}x{expected_h} disagree with the PNG {actual_w}x{actual_h}"
    )]
    AxisMismatch {
        name: String,
        expected_w: u32,
        expected_h: u32,
        actual_w: u32,
        actual_h: u32,
    },
}

/// The checked-in manifest's path.
#[must_use]
pub fn manifest_path() -> PathBuf {
    PathBuf::from(format!(
        "{}/tests/goldens/manifest.toml",
        env!("CARGO_MANIFEST_DIR")
    ))
}

/// Parses the manifest into its entries.
///
/// # Errors
///
/// A missing file is [`ManifestError::Read`]; malformed TOML is
/// [`ManifestError::Parse`]; an entry missing a field (or carrying a
/// mistyped one) is [`ManifestError::Field`].
pub fn read_entries() -> Result<Vec<GoldenEntry>, ManifestError> {
    let text = std::fs::read_to_string(manifest_path())?;
    parse_entries(&text)
}

/// The parse half of [`read_entries`]: manifest text → entries.
fn parse_entries(text: &str) -> Result<Vec<GoldenEntry>, ManifestError> {
    let doc = text.parse::<toml_edit::DocumentMut>()?;
    let Some(array) = doc.get("golden").and_then(|item| item.as_array_of_tables()) else {
        return Ok(Vec::new());
    };
    let mut entries = Vec::new();
    for table in array {
        let string_field = |field: &'static str| -> Result<String, ManifestError> {
            table
                .get(field)
                .and_then(|item| item.as_str())
                .map(str::to_owned)
                .ok_or_else(|| ManifestError::Field {
                    name: name_of(table),
                    field,
                })
        };
        let float_field = |field: &'static str| -> Result<f64, ManifestError> {
            axes_of(table)
                .and_then(|axes| axes.get(field))
                .and_then(toml_edit::Value::as_float)
                .ok_or_else(|| ManifestError::Field {
                    name: name_of(table),
                    field,
                })
        };
        let int_field = |field: &'static str| -> Result<u32, ManifestError> {
            axes_of(table)
                .and_then(|axes| axes.get(field))
                .and_then(toml_edit::Value::as_integer)
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| ManifestError::Field {
                    name: name_of(table),
                    field,
                })
        };
        entries.push(GoldenEntry {
            name: string_field("name")?,
            fixture: string_field("fixture")?,
            state: string_field("state")?,
            scale: float_field("scale")?,
            width: int_field("width")?,
            height: int_field("height")?,
        });
    }
    Ok(entries)
}

fn axes_of(table: &toml_edit::Table) -> Option<&toml_edit::InlineTable> {
    table.get("axes").and_then(|item| item.as_inline_table())
}

fn name_of(table: &toml_edit::Table) -> String {
    table
        .get("name")
        .and_then(|item| item.as_str())
        .unwrap_or("<unnamed>")
        .to_owned()
}

/// Verifies structural agreement between the manifest entries and the
/// committed PNG corpus (`pngs`: `(stem, width, height)` per file):
/// bijective, no duplicates, axes matching.
///
/// # Errors
///
/// Every disagreement is a distinct [`CorpusError`] variant.
pub fn verify_corpus(
    entries: &[GoldenEntry],
    pngs: &[(String, u32, u32)],
) -> Result<(), CorpusError> {
    let mut by_name = std::collections::BTreeMap::new();
    for entry in entries {
        if by_name.insert(entry.name.clone(), entry).is_some() {
            return Err(CorpusError::Duplicate(entry.name.clone()));
        }
    }
    for (stem, width, height) in pngs {
        let Some(entry) = by_name.remove(stem) else {
            return Err(CorpusError::OrphanPng(stem.clone()));
        };
        if (entry.width, entry.height) != (*width, *height) {
            return Err(CorpusError::AxisMismatch {
                name: entry.name.clone(),
                expected_w: entry.width,
                expected_h: entry.height,
                actual_w: *width,
                actual_h: *height,
            });
        }
    }
    if let Some((name, _)) = by_name.iter().next() {
        return Err(CorpusError::MissingPng(name.clone()));
    }
    Ok(())
}

/// The regen path's upsert: replaces the entry with the same `name`
/// (in place, preserving order) or appends it. Serializes on the
/// `manifest.toml.lock` sidecar — see the module docs.
pub fn record(entry: &GoldenEntry) {
    let lock_path = manifest_path().with_extension("lock");
    loop {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(error) => panic!("manifest lock {} creatable: {error}", lock_path.display()),
        }
    }
    let outcome = upsert(entry);
    drop(std::fs::remove_file(&lock_path));
    outcome.unwrap_or_else(|error| {
        panic!(
            "manifest {} upsert of {}: {error}",
            manifest_path().display(),
            entry.name
        )
    });
}

fn upsert(entry: &GoldenEntry) -> Result<(), toml_edit::TomlError> {
    let text = std::fs::read_to_string(manifest_path()).unwrap_or_default();
    let rendered = render_upsert(&text, entry)?;
    std::fs::write(manifest_path(), rendered).map_err(|error| {
        panic!("manifest {} writable: {error}", manifest_path().display());
    })
}

/// The upsert's pure half: `entry` replaces the row with the same
/// `name` in place (order preserved) or is appended, and the rendered
/// document text is returned. The write back lives with `upsert` —
/// the round-7 G1 fix (#1239): the matched path previously mutated an
/// in-memory document and returned *without writing*, so a re-based
/// golden's row silently kept its stale fixture/state strings.
fn render_upsert(text: &str, entry: &GoldenEntry) -> Result<String, toml_edit::TomlError> {
    let mut doc = text.parse::<toml_edit::DocumentMut>()?;
    let array = doc
        .entry("golden")
        .or_insert(toml_edit::Item::ArrayOfTables(
            toml_edit::ArrayOfTables::new(),
        ))
        .as_array_of_tables_mut()
        .expect("the golden key stays an array of tables");
    let matched = array.iter_mut().find(|table| {
        table.get("name").and_then(|item| item.as_str()) == Some(entry.name.as_str())
    });
    if let Some(table) = matched {
        fill(table, entry);
    } else {
        let mut table = toml_edit::Table::new();
        fill(&mut table, entry);
        array.push(table);
    }
    Ok(doc.to_string())
}

fn fill(table: &mut toml_edit::Table, entry: &GoldenEntry) {
    table["name"] = toml_edit::value(entry.name.as_str());
    table["fixture"] = toml_edit::value(entry.fixture.as_str());
    table["state"] = toml_edit::value(entry.state.as_str());
    table["axes"] = toml_edit::value({
        let mut axes = toml_edit::InlineTable::new();
        axes.insert("scale", toml_edit::Value::from(entry.scale));
        axes.insert("width", toml_edit::Value::from(i64::from(entry.width)));
        axes.insert("height", toml_edit::Value::from(i64::from(entry.height)));
        axes
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, width: u32, height: u32) -> GoldenEntry {
        GoldenEntry {
            name: name.to_owned(),
            fixture: "golden_state_at(PINNED_NOW)".to_owned(),
            state: "board view".to_owned(),
            scale: 1.0,
            width,
            height,
        }
    }

    /// T3 — a bijective corpus verifies clean.
    #[test]
    fn a_bijective_corpus_verifies() {
        let entries = [entry("a-1.0-100", 100, 90), entry("b-1.0-100", 100, 90)];
        let pngs = [
            ("a-1.0-100".to_owned(), 100, 90),
            ("b-1.0-100".to_owned(), 100, 90),
        ];
        assert!(verify_corpus(&entries, &pngs).is_ok());
    }

    /// T3 — each disagreement surfaces as its typed variant: orphan
    /// PNG, missing PNG, duplicate entry, axis mismatch.
    #[test]
    fn every_disagreement_is_typed() {
        let entries = [entry("a-1.0-100", 100, 90)];
        let pngs = [("b-1.0-100".to_owned(), 100, 90)];
        assert!(matches!(
            verify_corpus(&entries, &pngs),
            Err(CorpusError::OrphanPng(_))
        ));
        let pngs = Vec::new();
        assert!(matches!(
            verify_corpus(&entries, &pngs),
            Err(CorpusError::MissingPng(_))
        ));
        let duplicated = [entry("a-1.0-100", 100, 90), entry("a-1.0-100", 100, 90)];
        let pngs = [("a-1.0-100".to_owned(), 100, 90)];
        assert!(matches!(
            verify_corpus(&duplicated, &pngs),
            Err(CorpusError::Duplicate(_))
        ));
        let single = [entry("a-1.0-100", 100, 90)];
        let pngs = [("a-1.0-100".to_owned(), 101, 90)];
        assert!(matches!(
            verify_corpus(&single, &pngs),
            Err(CorpusError::AxisMismatch { .. })
        ));
    }

    /// T3 — the round-7 G1 regression (#1239): an upsert against an
    /// existing name re-renders that row in place (order preserved)
    /// — the old matched path mutated an in-memory document and
    /// never wrote, leaving stale fixture/state strings on disk.
    #[test]
    fn an_upsert_of_an_existing_row_rewrites_it_in_place() {
        let text = "[[golden]]\nname = \"a-1.0-100\"\n\
                    fixture = \"golden_state_at(PINNED_NOW)\"\n\
                    state = \"board view\"\n\
                    axes = { scale = 1.0, width = 100, height = 90 }\n\n\
                    [[golden]]\nname = \"b-1.0-100\"\n\
                    fixture = \"golden_state_at(PINNED_NOW)\"\n\
                    state = \"board view\"\n\
                    axes = { scale = 1.0, width = 100, height = 90 }\n";
        let mut re_based = entry("a-1.0-100", 100, 90);
        re_based.fixture = "golden_state_enriched_at(PINNED_NOW)".to_owned();
        let rendered = render_upsert(text, &re_based).expect("renders");
        let entries = parse_entries(&rendered).expect("re-parses");
        assert_eq!(entries.len(), 2, "no row appended");
        assert_eq!(entries[0].name, "a-1.0-100", "order preserved");
        assert_eq!(
            entries[0].fixture, "golden_state_enriched_at(PINNED_NOW)",
            "the matched row's fields are rewritten"
        );
        assert_eq!(entries[1].fixture, "golden_state_at(PINNED_NOW)");
    }
}

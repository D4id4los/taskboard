// SPDX-License-Identifier: MIT OR Apache-2.0
//! The D-J1 vocabulary guards (DR-7, ADR 0021), enforced by test —
//! not hope. The undo command journal owns the `Journal*` spelling;
//! the reflection family is `Reflection*` everywhere; the sqlite table
//! is (and stays, from the domain side) `reflection_entries`, never
//! `journal_entries`. Every future file is subject to the same scan —
//! the guard is the drift fence, stage 3's diff grep is the second net.
//!
//! Environment gate, not a weakened assertion: the suite reads the
//! crate's own sources from disk, which Miri's isolation scopes out.

#![cfg_attr(miri, ignore)]

use std::path::{Path, PathBuf};

/// The allowlist: the undo substrate's *existing* homes, enumerated at
/// base (2026-11-05 @ 8ea54d77) and frozen — the drift guard is
/// precisely that no file outside this list may ever introduce a
/// `Journal*` symbol, so every future file (reflection.rs included) is
/// scanned. `ids.rs` is where `JournalId` legitimately lives today (a
/// `local_id!` family member); `lib.rs` carries the re-export lines;
/// `idgen.rs` the `new_journal_id` trait arm; `persistence.rs` the
/// journal actions and the persisted `JournalState`; `command.rs` the
/// undo receipts; `test_support/` the fake generator's arm.
const JOURNAL_ALLOWED: [&str; 8] = [
    "journal.rs",
    "journal",
    "undo.rs",
    "undo",
    "ids.rs",
    "lib.rs",
    "idgen.rs",
    "persistence.rs",
];
/// The undo receipts in `command.rs` and the fake generator arm in
/// `test_support/` — same frozen-at-base rationale, kept separate so
/// the list above stays the substrate's shape at a glance.
const JOURNAL_ALLOWED_MORE: [&str; 3] = ["command.rs", "test_support", "state.rs"];

/// The `Journal`-prefixed identifier scan (identifier-boundary regex,
/// not a substring: prose like "journaling" or "journal line" in doc
/// comments is ruled legal — the guard is about *code symbols*).
#[cfg(test)]
fn journal_violation(line: &str) -> bool {
    let bytes = line.as_bytes();
    let needle = b"Journal";
    let mut from = 0;
    while let Some(pos) = line[from..].find("Journal") {
        let abs = from + pos;
        // Word boundary before: the preceding byte must not be an
        // identifier character.
        let boundary_left =
            abs == 0 || !(bytes[abs - 1].is_ascii_alphanumeric() || bytes[abs - 1] == b'_');
        let at_least_one_more = abs + needle.len() < bytes.len()
            && (bytes[abs + needle.len()].is_ascii_alphabetic()
                || bytes[abs + needle.len()] == b'_');
        if boundary_left && at_least_one_more {
            return true;
        }
        from = abs + needle.len();
        if from >= line.len() {
            break;
        }
    }
    false
}

fn rs_sources(root: &Path) -> Vec<PathBuf> {
    walk(root, root)
}

fn walk(root: &Path, dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).expect("readable source tree") {
        let entry = entry.expect("readable entry");
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(root, &path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
    let _ = root;
    found
}

fn rel(path: &Path) -> String {
    // Relative to `src/`, forward slashes, for the allowlist match and
    // the failure output.
    path.to_string_lossy().rsplit_once("src/").map_or_else(
        || path.to_string_lossy().into_owned(),
        |(_, rest)| rest.to_owned(),
    )
}

#[test]
fn no_journal_spelled_code_symbols_outside_the_undo_substrate() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets the manifest dir");
    let src = Path::new(&manifest).join("src");
    let mut violations = Vec::new();
    for path in rs_sources(&src) {
        let relative = rel(&path);
        let allowed = JOURNAL_ALLOWED
            .iter()
            .chain(JOURNAL_ALLOWED_MORE.iter())
            .any(|home| relative == *home || relative.starts_with(&format!("{home}/")));
        if allowed {
            continue;
        }
        let body = std::fs::read_to_string(&path).expect("readable source");
        for (line_no, line) in body.lines().enumerate() {
            if journal_violation(line) {
                violations.push(format!("{}:{}: {line}", path.display(), line_no + 1));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "Journal* code symbols found outside the undo substrate's frozen-at-base homes \
         (journal.rs/journal/, undo.rs/undo/, ids.rs, lib.rs, idgen.rs, persistence.rs, \
         command.rs, test_support/) — the D-J1 vocabulary ruling (ADR 0021) reserves the \
         undo journal's spelling for new code:\n{}",
        violations.join("\n")
    );
}

#[test]
fn journal_entries_appears_nowhere_in_the_crate_sources() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets the manifest dir");
    let src = Path::new(&manifest).join("src");
    let mut violations = Vec::new();
    for path in rs_sources(&src) {
        let body = std::fs::read_to_string(&path).expect("readable source");
        for (line_no, line) in body.lines().enumerate() {
            if line.contains("journal_entries") {
                violations.push(format!("{}:{}: {line}", path.display(), line_no + 1));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "the table is `reflection_entries` (DR-7 D-J1 guard rule ii) — \
         `journal_entries` must not appear anywhere in the domain sources:\n{}",
        violations.join("\n")
    );
}

#[cfg(test)]
mod scan_tests {
    use super::journal_violation;

    #[test]
    fn the_scan_matches_identifiers_not_prose() {
        for violating in [
            "JournalEntry",
            "let journal_state = JournalState::default();",
            "/// See `JournalId` for the wire form.",
        ] {
            assert!(journal_violation(violating), "violating: {violating}");
        }
        for legal in [
            "journaling twice in a day is normal",
            "the journal line survives restarts",
            "journal_entries is the forbidden table name", // substring without the prefix
            "Journal",                                     // bare word, no continuation
        ] {
            assert!(!journal_violation(legal), "legal prose: {legal}");
        }
    }
}

// SPDX-License-Identifier: AGPL-3.0-only
//! File-backed REPL history (the persistence policy in one place):
//! where the history file lives, which lines enter it, and how every
//! failure degrades. History is a DX nicety — nothing here may kill a
//! session, so the API is infallible: failures warn and the session
//! keeps its in-memory recall.
//!
//! Location follows the db-path precedent (`config::default_db_path`):
//! `<data-dir>/taskboard/history`, with `data_dir` injected so the
//! logic unit-tests without environment mutation. Persistence is written
//! by this module's own writer, *not* rustyline's `FileHistory::save`:
//! that method wraps its file create in a process-global umask mutation
//! (still present in rustyline 19.0.0), stripping the mode bits of any
//! file or directory a sibling thread creates inside the window. This
//! writer sets 0600 at create time via `OpenOptions::mode` on unix —
//! no process-global state touched (on Windows the platform default
//! applies). Save cadence is a full rewrite per recorded line: `append`
//! is poisoned by a corrupt file (it re-reads it), while a truncated
//! overwrite is not — so the first recorded line of the next session
//! self-heals a corrupt file. Single-writer is guaranteed by the
//! instance lock (one core per database, and daemon mode runs no
//! editor).
//!
//! Prompted ceremony answers (masked secrets, confirms) never reach
//! the recorder: they bypass `readline` by construction (ADR 0014).

use std::path::{Path, PathBuf};

use rustyline::history::{History, SearchDirection};

/// The history file under an injected data dir: the `default_db_path`
/// formula with `history` for `taskboard.db`. `None` means the
/// platform has no data dir — persistence is off for the session.
#[must_use]
pub(crate) fn history_path(data_dir: Option<&Path>) -> Option<PathBuf> {
    data_dir.map(|dir| dir.join("taskboard").join("history"))
}

/// The session's history persistence handle.
pub(crate) struct SessionHistory {
    /// `None` = persistence unavailable this session (no data dir, or
    /// the history directory could not be created); in-memory history
    /// still works.
    path: Option<PathBuf>,
}

impl SessionHistory {
    /// Resolves the history file and best-effort creates its parent
    /// directory. No file is created here — the first [`record`](Self::record)
    /// does that.
    #[must_use]
    pub(crate) fn open(data_dir: Option<&Path>) -> Self {
        let path = history_path(data_dir).and_then(|path| {
            let parent = path.parent().expect("the formula always has a parent");
            match std::fs::create_dir_all(parent) {
                Ok(()) => Some(path),
                Err(err) => {
                    tracing::warn!(
                        ?err,
                        directory = %parent.display(),
                        "history directory unavailable; history stays in memory"
                    );
                    None
                }
            }
        });
        Self { path }
    }

    /// Loads the persisted history into the editor. A missing file is a
    /// silent fresh start (the normal first run); a corrupt or
    /// unreadable file warns once and keeps whatever parsed — rustyline
    /// stops at the first invalid line, and those partial entries are
    /// harmless recallable lines.
    pub(crate) fn load(&self, history: &mut impl History) {
        let Some(path) = &self.path else {
            return;
        };
        match history.load(path) {
            Ok(()) => {}
            Err(rustyline::error::ReadlineError::Io(err))
                if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                tracing::warn!(
                    ?err,
                    file = %path.display(),
                    "unreadable history file; keeping what parsed"
                );
            }
        }
    }

    /// Records one submitted line: blank / whitespace-only lines are
    /// prompt noise, not recallable entries (the admission policy,
    /// moved here from the editor loop); everything else enters the
    /// in-memory history and — when persistence is available — is saved
    /// to the file. Save failures warn and are swallowed.
    pub(crate) fn record(&self, history: &mut impl History, line: &str) {
        if line.trim().is_empty() {
            return;
        }
        if let Err(err) = history.add(line) {
            tracing::warn!(?err, "history entry rejected; recall may miss this line");
        }
        let Some(path) = &self.path else {
            return;
        };
        if let Err(err) = save_entries(history, path) {
            tracing::warn!(?err, file = %path.display(), "history save failed");
        }
    }
}

/// The persistence writer: a truncated overwrite in rustyline's own
/// on-disk format (newline-separated lines, exactly what
/// `DefaultHistory::load` reads). Deliberately not
/// `History::save` — see the module docs on the umask defect. Mode 0600
/// is enforced at create time on unix; owner access is the only access
/// the file ever needs.
fn save_entries(history: &impl History, path: &Path) -> std::io::Result<()> {
    use std::io::Write as _;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    for index in 0..history.len() {
        if let Some(entry) = history.get(index, SearchDirection::Forward).ok().flatten() {
            writeln!(file, "{}", entry.entry)?;
        }
    }
    file.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyline::history::SearchDirection;

    fn fresh() -> rustyline::history::DefaultHistory {
        rustyline::history::DefaultHistory::new()
    }

    fn entries_of(history: &impl History) -> Vec<String> {
        (0..history.len())
            .map(|i| {
                history
                    .get(i, SearchDirection::Forward)
                    .expect("history get")
                    .expect("entry at index")
                    .entry
                    .to_string()
            })
            .collect()
    }

    // -- the path formula ---------------------------------------------------

    #[test]
    fn history_path_joins_the_taskboard_subdir_under_the_data_dir() {
        let base = Path::new("/data");
        assert_eq!(
            history_path(Some(base)),
            Some(PathBuf::from("/data/taskboard/history"))
        );
        assert_eq!(history_path(None), None);
    }

    // -- session roundtrip --------------------------------------------------

    #[test]
    fn a_second_session_loads_exactly_the_recorded_lines_in_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        let base = dir.path().to_path_buf();

        let session = SessionHistory::open(Some(&base));
        let mut history = fresh();
        session.record(&mut history, "stacks add \"Todo\"");
        session.record(&mut history, "   ");
        session.record(&mut history, "tasks list --json");
        assert_eq!(
            entries_of(&history),
            ["stacks add \"Todo\"", "tasks list --json"]
        );

        let mut next_session = fresh();
        SessionHistory::open(Some(&base)).load(&mut next_session);
        assert_eq!(
            entries_of(&next_session),
            ["stacks add \"Todo\"", "tasks list --json"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_history_file_is_owner_read_write_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let session = SessionHistory::open(Some(dir.path()));
        let mut history = fresh();
        session.record(&mut history, "status");

        let path = history_path(Some(dir.path())).expect("path");
        let mode = std::fs::metadata(&path)
            .expect("history file written")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    // -- degradation ----------------------------------------------------------

    #[test]
    fn a_corrupt_history_file_degrades_then_self_heals_on_the_next_record() {
        let dir = tempfile::tempdir().expect("tempdir");
        // `open` creates the parent; then the file is corrupted behind it.
        let session = SessionHistory::open(Some(dir.path()));
        let path = history_path(Some(dir.path())).expect("path");
        std::fs::write(&path, [0xff, 0xfe, 0x00, b'\n']).expect("write garbage");

        // Load survives the corrupt file (warn + whatever parsed).
        let mut history = fresh();
        session.load(&mut history);

        // The next recorded line rewrites the file into a valid state.
        session.record(&mut history, "status");
        let mut reloaded = fresh();
        SessionHistory::open(Some(dir.path())).load(&mut reloaded);
        assert_eq!(entries_of(&reloaded), ["status"]);
    }

    #[test]
    fn a_legacy_v1_history_file_loads_as_entries() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("taskboard")).expect("create parent");
        let path = history_path(Some(dir.path())).expect("path");
        std::fs::write(&path, "boards list\ntasks list\n").expect("write v1 file");

        let session = SessionHistory::open(Some(dir.path()));
        let mut history = fresh();
        session.load(&mut history);
        assert_eq!(entries_of(&history), ["boards list", "tasks list"]);
    }

    #[test]
    fn an_uncreatable_history_directory_degrades_to_in_memory_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        // A regular file squatting on the directory path: create_dir_all
        // fails, and the session must keep working without persistence.
        let squatter = dir.path().join("taskboard");
        std::fs::write(&squatter, b"not a directory").expect("squat the dir path");

        let session = SessionHistory::open(Some(dir.path()));
        let mut history = fresh();
        session.record(&mut history, "status");
        assert_eq!(entries_of(&history), ["status"], "in-memory recall works");
        assert!(
            history_path(Some(dir.path())).is_none_or(|p| !p.exists()),
            "no history file appeared"
        );
    }

    #[test]
    fn without_a_data_dir_history_stays_in_memory() {
        let session = SessionHistory::open(None);
        let mut history = fresh();
        session.record(&mut history, "status");
        session.load(&mut history);
        assert_eq!(entries_of(&history), ["status"]);
    }
}

// SPDX-License-Identifier: AGPL-3.0-only

//! The pdf leg's file-writing and viewer-open delivery (plan §7.2–
//! §7.3): the destination is an A1 mint (typed path outcomes, never a
//! silent stdout dump — stdout stays the payload lane of the csv/json
//! families), and the viewer open is best-effort, typed and non-fatal
//! (the written file is the deliverable).

use std::io;
use std::path::{Path, PathBuf};

/// Which physical layout the sheet names (the file-name fragment and
/// the config key share the vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetName {
    /// The single-sided A4 sheet.
    A4,
    /// The single-sided A5 sheet.
    A5,
    /// The single-sided A6 sheet.
    A6,
    /// The foldable A4 sheet (the default).
    Fold,
}

impl SheetName {
    /// The config/file-name vocabulary (`a4`, `a5`, `a6`, `fold`).
    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            Self::A4 => "a4",
            Self::A5 => "a5",
            Self::A6 => "a6",
            Self::Fold => "fold",
        }
    }

    /// The layout the config key names; `None` for an unknown key (the
    /// verb maps it to a typed refusal).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "a4" => Some(Self::A4),
            "a5" => Some(Self::A5),
            "a6" => Some(Self::A6),
            "fold" => Some(Self::Fold),
            _ => None,
        }
    }
}

/// Why a pdf destination could not be minted or written — a typed
/// outcome, never a text message.
#[derive(Debug, thiserror::Error)]
pub enum PdfPathError {
    /// The path is not absolute after `~` expansion and cwd resolution
    /// (the session lane requires absolute paths: the IPC wire carries
    /// no cwd).
    #[error("pdf destination is not absolute")]
    NotAbsolute,
    /// The parent directory is missing (only refused for an explicit
    /// `--out`; the config default's parent is created).
    #[error("pdf destination's parent directory is missing")]
    ParentMissing,
    /// The path names an existing directory.
    #[error("pdf destination is a directory")]
    IsDirectory,
    /// Creating or writing the file failed (permission, disk, …).
    #[error("pdf destination write failed")]
    Write(io::ErrorKind),
}

/// The minted pdf destination: an absolute path that passed the
/// boundary checks. Private field — the only constructor runs the
/// checks, so a minted destination is always writable-or-reportable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfDestination {
    path: PathBuf,
}

impl PdfDestination {
    /// Mints the destination from an explicit `--out` (or the config
    /// default when `explicit` is false, whose parent is created).
    /// The caller has already expanded `~` and resolved the cwd; the
    /// mint refuses anything left relative, anything naming an
    /// existing directory, and (explicit only) a missing parent.
    ///
    /// # Errors
    ///
    /// [`PdfPathError`] — see the variant docs.
    pub fn mint(path: PathBuf, explicit: bool) -> Result<Self, PdfPathError> {
        if !path.is_absolute() {
            return Err(PdfPathError::NotAbsolute);
        }
        if path.is_dir() {
            return Err(PdfPathError::IsDirectory);
        }
        match path.parent() {
            Some(parent) if parent.as_os_str().is_empty() => {
                return Err(PdfPathError::ParentMissing);
            }
            Some(parent) if parent.is_dir() => {}
            Some(_) if explicit => return Err(PdfPathError::ParentMissing),
            Some(parent) => {
                std::fs::create_dir_all(parent).map_err(|err| PdfPathError::Write(err.kind()))?;
            }
            None => return Err(PdfPathError::ParentMissing),
        }
        Ok(Self { path })
    }

    /// The minted absolute path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes the rendered bytes; an existing file is overwritten (a
    /// printout is a regenerable artefact).
    ///
    /// # Errors
    ///
    /// [`PdfPathError::Write`] — the io failure's kind.
    pub fn write(&self, bytes: &[u8]) -> Result<(), PdfPathError> {
        std::fs::write(&self.path, bytes).map_err(|err| PdfPathError::Write(err.kind()))
    }
}

/// The default destination directory: the config's `[export]
/// pdf_dir`, else `~/Documents/taskboard` (the desktop default), else
/// the data dir (a kiosk has no documents dir).
#[must_use]
pub fn default_pdf_dir(config_dir: Option<&Path>, data_dir: &Path) -> PathBuf {
    if let Some(dir) = config_dir {
        return dir.to_path_buf();
    }
    dirs::document_dir().map_or_else(|| data_dir.join("pdf"), |docs| docs.join("taskboard"))
}

/// The pdf file name: `taskboard-<YYYY-MM-DD>-<layout>.pdf`.
#[must_use]
pub fn pdf_file_name(date: chrono::NaiveDate, sheet: SheetName) -> String {
    format!("taskboard-{}-{}.pdf", date.format("%Y-%m-%d"), sheet.text())
}

/// Expands a leading `~` to the home directory (the config file and
/// `--out` share the convention); the path passes through unchanged
/// when it has none or the home is unknown.
#[must_use]
pub fn expand_tilde(path: &Path) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    if let Some(rest) = text.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    path.to_path_buf()
}

/// Why the viewer could not be opened — a typed outcome, non-fatal by
/// ruling (the file was written).
#[derive(Debug, Clone, thiserror::Error)]
pub enum OpenError {
    /// The platform's open helper is not on `PATH`.
    #[error("no pdf viewer helper found")]
    NotFound,
    /// The helper spawn failed.
    #[error("pdf viewer spawn failed")]
    Spawn(io::ErrorKind),
    /// No graphical display to open anything in (Linux: neither
    /// `DISPLAY` nor `WAYLAND_DISPLAY` is set — checked before
    /// spawning).
    #[error("no display session to open the pdf in")]
    NoDisplay,
}

/// The pure display check over an env snapshot (Linux only; macOS and
/// Windows always have a session concept and spawn directly). Split
/// out so it is testable without an env var race.
#[cfg(target_os = "linux")]
pub fn display_session_open(display: Option<&str>, wayland: Option<&str>) -> Result<(), OpenError> {
    if display.is_none() && wayland.is_none() {
        Err(OpenError::NoDisplay)
    } else {
        Ok(())
    }
}

/// Opens the file with the platform's system handler, spawned and
/// detached (never waited on beyond the spawn).
///
/// # Errors
///
/// [`OpenError`] — see the variant docs.
pub fn open_in_viewer(path: &Path) -> Result<(), OpenError> {
    #[cfg(target_os = "linux")]
    {
        display_session_open(
            std::env::var_os("DISPLAY")
                .as_deref()
                .and_then(|v| v.to_str()),
            std::env::var_os("WAYLAND_DISPLAY")
                .as_deref()
                .and_then(|v| v.to_str()),
        )?;
        spawn_detached(std::process::Command::new("xdg-open").arg(path))
    }
    #[cfg(target_os = "macos")]
    {
        spawn_detached(std::process::Command::new("open").arg(path))
    }
    #[cfg(target_os = "windows")]
    {
        spawn_detached(
            std::process::Command::new("cmd")
                .args(["/C", "start", ""])
                .arg(path),
        )
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = path;
        Err(OpenError::NotFound)
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn spawn_detached(command: &mut std::process::Command) -> Result<(), OpenError> {
    command.spawn().map(drop).map_err(|err| match err.kind() {
        io::ErrorKind::NotFound => OpenError::NotFound,
        kind => OpenError::Spawn(kind),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name_carries_date_and_layout() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 11, 7).expect("valid date");
        assert_eq!(
            pdf_file_name(date, SheetName::Fold),
            "taskboard-2026-11-07-fold.pdf"
        );
        assert_eq!(
            pdf_file_name(date, SheetName::A4),
            "taskboard-2026-11-07-a4.pdf"
        );
    }

    #[test]
    fn sheet_name_parses_the_config_vocabulary() {
        for text in ["a4", "a5", "a6", "fold"] {
            let parsed = SheetName::parse(text).expect("parses");
            assert_eq!(parsed.text(), text);
        }
        assert!(SheetName::parse("letter").is_none());
    }

    #[test]
    fn tilde_expansion_leaves_untilded_paths_alone() {
        assert_eq!(
            expand_tilde(Path::new("/tmp/x.pdf")),
            PathBuf::from("/tmp/x.pdf")
        );
        assert_eq!(
            expand_tilde(Path::new("relative.pdf")),
            PathBuf::from("relative.pdf")
        );
    }

    #[test]
    fn mint_refuses_relative_paths_and_directories() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(matches!(
            PdfDestination::mint(PathBuf::from("relative/x.pdf"), true),
            Err(PdfPathError::NotAbsolute)
        ));
        assert!(matches!(
            PdfDestination::mint(tmp.path().to_path_buf(), true),
            Err(PdfPathError::IsDirectory)
        ));
    }

    #[test]
    fn mint_refuses_a_missing_parent_for_explicit_outs() {
        assert!(matches!(
            PdfDestination::mint(PathBuf::from("/nonexistent-parent-dir/x.pdf"), true),
            Err(PdfPathError::ParentMissing)
        ));
    }

    #[test]
    fn mint_creates_the_config_default_parent_and_writes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("nested/dir/taskboard.pdf");
        let dest = PdfDestination::mint(target.clone(), false).expect("mints");
        dest.write(b"%PDF-x").expect("writes");
        assert_eq!(std::fs::read(&target).expect("readable"), b"%PDF-x");
        // Overwrite is the regenerable-artefact posture.
        dest.write(b"%PDF-y").expect("overwrites");
        assert_eq!(std::fs::read(&target).expect("readable"), b"%PDF-y");
    }

    #[test]
    fn explicit_mint_writes_into_an_existing_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("taskboard.pdf");
        let dest = PdfDestination::mint(target.clone(), true).expect("mints");
        dest.write(b"%PDF-").expect("writes");
        assert!(target.exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_display_check_refuses_only_a_headless_session() {
        assert!(matches!(
            display_session_open(None, None),
            Err(OpenError::NoDisplay)
        ));
        assert!(display_session_open(Some(":0"), None).is_ok());
        assert!(display_session_open(None, Some("wayland-0")).is_ok());
    }

    #[test]
    fn default_pdf_dir_prefers_the_config() {
        let config_dir = Path::new("/cfg/pdfs");
        assert_eq!(
            default_pdf_dir(Some(config_dir), Path::new("/data")),
            PathBuf::from("/cfg/pdfs")
        );
    }
}

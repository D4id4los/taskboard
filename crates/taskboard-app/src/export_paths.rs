// SPDX-License-Identifier: MIT OR Apache-2.0

//! The export family's destination-path helpers (the J2 export plan's
//! W-6 seam lifted out of the feature-gated `pdf` module): the
//! csv/json leg's GUI destination default and the `~` expansion
//! convention compile in EVERY build — a default (kiosk) tree carries
//! no `pdf` module, but its config still carries the `[export]`
//! section's home. The pdf leg consumes the same two helpers (the
//! one-resolution rule — a GUI default and the CLI default cannot
//! diverge).

use std::path::{Path, PathBuf};

/// The default destination directory: the config's `[export]
/// pdf_dir`, else `~/Documents/taskboard` (the desktop default), else
/// the data dir (a kiosk has no documents dir).
#[must_use]
pub fn default_pdf_dir(config_dir: Option<&Path>, data_dir: Option<&Path>) -> PathBuf {
    if let Some(dir) = config_dir {
        return dir.to_path_buf();
    }
    dirs::document_dir().map_or_else(
        || {
            data_dir
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
                .join("pdf")
        },
        |docs| docs.join("taskboard"),
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde_passes_absolute_paths_through() {
        let absolute = Path::new("/tmp/x");
        assert_eq!(expand_tilde(absolute), absolute.to_path_buf());
    }

    #[test]
    fn the_default_dir_prefers_the_config_key_verbatim() {
        let configured = default_pdf_dir(Some(Path::new("/data/pdfs")), None);
        assert_eq!(configured, PathBuf::from("/data/pdfs"));
    }
}

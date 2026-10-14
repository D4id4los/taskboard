// SPDX-License-Identifier: AGPL-3.0-only
//! Bundled font registration (chunk decision D11).
//!
//! The UI ships its type faces inside the binary: Inter static
//! instances (Regular 400, `SemiBold` 600, Bold 700) and `JetBrains` Mono
//! Regular 400 — all SIL OFL 1.1 (see `assets/fonts/LICENSES.md` for
//! the per-font licence record). Registration happens at startup, via
//! `include_bytes!` + the runtime font registration API, so the app
//! stays offline-capable, single-binary, and typographically
//! deterministic (static instances pin the role table's weights).
//!
//! Implementation note (the D11 fallback as executed): slint 1.18.1 no
//! longer exposes `register_font_from_memory` on the public crate root;
//! the documented runtime-registration path is the
//! `unstable-fontique-011` feature's `shared_collection()` (shared
//! storage — registrations apply process-wide). The feature is additive
//! and unifies with i-slint-core's default `shared-parley`, so it adds
//! no new crates and changes no renderer/backend selection (Q10).

use std::sync::{Arc, Once};

/// Inter Regular 400 (SIL OFL 1.1).
const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
/// Inter `SemiBold` 600 (SIL OFL 1.1).
const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
/// Inter Bold 700 (SIL OFL 1.1).
const INTER_BOLD: &[u8] = include_bytes!("../assets/fonts/Inter-Bold.ttf");
/// `JetBrains` Mono Regular 400 (SIL OFL 1.1).
const JETBRAINS_MONO_REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");

/// The bundled faces, in registration order (the theme's
/// `Typography.family-sans` / `family-mono` names resolve against the
/// fonts' own metadata: "Inter", "`JetBrains` Mono").
const BUNDLED_FONTS: &[&[u8]] = &[
    INTER_REGULAR,
    INTER_SEMIBOLD,
    INTER_BOLD,
    JETBRAINS_MONO_REGULAR,
];

static REGISTER_ONCE: Once = Once::new();

/// Registers every bundled font with Slint's shared font collection.
/// Idempotent per process (guarded), so re-mounts (tests) are safe.
///
/// # Panics
///
/// Panics if Slint's font context cannot be initialised — the same
/// failure the subsequent window creation would hit, surfaced earlier.
pub fn register_bundled_fonts() {
    REGISTER_ONCE.call_once(|| {
        use slint::fontique_011::fontique;
        let mut collection = slint::fontique_011::shared_collection();
        for font in BUNDLED_FONTS {
            let blob = fontique::Blob::new(Arc::new(font.to_vec()));
            let registered = collection.register_fonts(blob, None);
            tracing::debug!(families = ?registered.iter().map(|(family, _)| *family).collect::<Vec<_>>(), "registered bundled font");
        }
        tracing::info!(count = BUNDLED_FONTS.len(), "bundled fonts registered");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_font_blobs_look_like_truetype_fonts() {
        // Structural fact, not text: every embedded blob starts with a
        // sfnt header (0x00010000 or 'true'/'OTTO').
        for font in BUNDLED_FONTS {
            assert!(font.len() > 12, "font blob has content");
            let magic = u32::from_be_bytes([font[0], font[1], font[2], font[3]]);
            assert!(
                matches!(magic, 0x0001_0000 | 0x7472_7565 | 0x4F54_544F),
                "sfnt magic expected, got {magic:#010x}"
            );
        }
    }
}

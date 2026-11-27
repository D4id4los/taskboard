// SPDX-License-Identifier: AGPL-3.0-only

// Hex colour words read as colours (0xRRGGBB, the CSS convention) and
// the byte-splitting shifts are exact — the two pedantic allows are
// deliberate, module-wide.
#![allow(clippy::unreadable_literal)]
#![allow(clippy::cast_possible_truncation)]

//! The palette data (design §2/D1–D2, #1437): the 14 colour tokens —
//! the 9 `Theme` palette colours plus the 5 `Semantic` state literals —
//! as Rust data, pushed into the generated `Theme`/`Semantic` globals.
//!
//! The two palettes:
//!
//! - [`Palette::DARK`] is the *verbatim* current theme.slint values
//!   (design §2.1): the `.slint` literals are the boot-time pre-push
//!   fallback defaults, and the mirror-parity test
//!   (`token_tests::the_slint_defaults_mirror_the_dark_palette`)
//!   pins defaults ≡ `DARK` field-by-field (the house mirror-parity
//!   pattern — `settings_form.rs` mirrors config validation; drift
//!   breaks a test).
//! - [`Palette::LIGHT`] derives from emacs leuven's chroma (white
//!   page, warm grey surfaces, muted accents), tuned to pass the
//!   WCAG contrast floors (the floors are the contract; the seeds
//!   were the design doc's inputs, not the shipped values — design
//!   §2.2).
//!
//! The two derived `Semantic` entries (`offline` → `text-secondary`,
//! `focus-ring` → `Theme.focus-ring`) stay slint-side conditionals:
//! they are references, not data, and keep working under any palette
//! (design D1).
//!
//! The theme-invariant layers (typography, spacing, radii, widths,
//! window dimensions) are deliberately **not** fields here — geometry,
//! not colour (design D1's recorded boundary).
//!
//! # The file-driven future (D6, deferred)
//!
//! The future theme-file format is this struct serialized (one field
//! per token, the same 14 keys); a loader = read → deserialize →
//! contrast-check → push. None of this module's shape is wasted on
//! the deferred subsystem — see backlog issue #1441.

/// The 14-colour palette table (design D1). RGB cells are `0xRRGGBB`;
/// hex literals stay unseparated (colour-reading convention — the
/// lint's allow is deliberate, palette-wide).
/// the scrim is `0xRRGGBBAA` (translucent wash — never a text
/// carrier, invisible to the contrast extractor by construction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// The window shell / board strip.
    pub background: u32,
    /// Containers (column cards, toolbar, tracks, chips).
    pub surface: u32,
    /// Content on a container (task cards, fields, panels).
    pub surface_raised: u32,
    /// The hover step (the token tests' distinctness guard).
    pub surface_hover: u32,
    pub text_primary: u32,
    pub text_secondary: u32,
    pub border: u32,
    pub focus_ring: u32,
    /// `0xRRGGBBAA`.
    pub scrim: u32,
    pub success: u32,
    pub warning: u32,
    pub danger: u32,
    pub overdue: u32,
    pub sync_active: u32,
}

impl Palette {
    /// The current dark palette, verbatim (design §2.1) — byte-equal
    /// to the theme.slint literals (mirror-parity test).
    pub const DARK: Self = Self {
        background: 0x14161c,
        surface: 0x1d2028,
        surface_raised: 0x262a35,
        surface_hover: 0x303545,
        text_primary: 0xe8eaf0,
        text_secondary: 0xa6abbd,
        border: 0x646c85,
        focus_ring: 0x7aa2f7,
        scrim: 0x0b0d1280,
        success: 0x6fd98b,
        warning: 0xe8c268,
        danger: 0xe8756f,
        overdue: 0xff9e64,
        sync_active: 0x7aa2f7,
    };

    /// The leuven-derived light palette (design §2.2), tuned so every
    /// declared contrast pair meets its role-class floor on the
    /// *worst* surface (the hover step — the floors are the contract;
    /// measured outcomes recorded in the C1-impl report):
    /// `text-primary` ≥ 10.0:1, `text-secondary` ≥ 4.8:1, every state
    /// colour ≥ 4.5:1 (body), `border`/`focus-ring` ≥ 3.7:1
    /// (non-text) — all against `surface-hover`, the darkest light
    /// surface the states can render on.
    pub const LIGHT: Self = Self {
        background: 0xffffff,
        surface: 0xf7f8f8,
        surface_raised: 0xeef0f2,
        surface_hover: 0xe2e6ea,
        text_primary: 0x333333,
        text_secondary: 0x5f6368,
        border: 0x6b7280,
        focus_ring: 0x3a63a8,
        scrim: 0x1a1c2280,
        success: 0x1d6f3e,
        warning: 0x7a5f00,
        danger: 0xa03a32,
        overdue: 0x8a4416,
        sync_active: 0x3a63a8,
    };

    /// The scrim as the slint `Color` form (ARGB-encoded).
    #[must_use]
    pub fn scrim_color(self) -> slint::Color {
        // The cell is stored `0xRRGGBBAA` (the slint 8-digit literal's
        // byte order — the parity test reads theme.slint's literal
        // verbatim); the `Color` constructor is ARGB-encoded, so the
        // halves swap here.
        let rgba = self.scrim;
        let argb = (rgba & 0xff) << 24 | rgba >> 8;
        slint::Color::from_argb_encoded(argb)
    }
}

/// A validated theme choice (A1 pattern — the house hand-rolled
/// private-field newtype, design §1.4): minted fallibly at the trust
/// boundary (config decode / env parse / request pick). A two-value
/// closed set — no leniency; the file-driven future (D6/#1441)
/// widens it deliberately when it lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThemeName(ThemeKind);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ThemeKind {
    #[default]
    Dark,
    Light,
}

/// The typed mint failure: an unknown theme word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeNameError;

impl ThemeName {
    /// The dark default (the zero-config look; the serde default's
    /// value, design D5).
    pub const DARK: Self = Self(ThemeKind::Dark);
    pub const LIGHT: Self = Self(ThemeKind::Light);

    /// Mints from the config word (`"dark"` / `"light"`); anything
    /// else is the typed [`ThemeNameError`] — never a silent fallback
    /// (the `UiScale` mint's fail-fast posture).
    ///
    /// # Errors
    ///
    /// [`ThemeNameError`] for any word outside the closed set.
    pub fn new(name: &str) -> Result<Self, ThemeNameError> {
        match name {
            "dark" => Ok(Self::DARK),
            "light" => Ok(Self::LIGHT),
            _ => Err(ThemeNameError),
        }
    }

    /// The config word (the `toml_edit` writeback's value).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self.0 {
            ThemeKind::Dark => "dark",
            ThemeKind::Light => "light",
        }
    }

    /// The palette the choice selects.
    #[must_use]
    pub fn palette(self) -> &'static Palette {
        match self.0 {
            ThemeKind::Dark => &Palette::DARK,
            ThemeKind::Light => &Palette::LIGHT,
        }
    }

    /// The segmented control's index (the keymap-preset grammar:
    /// index order is the request seam's contract).
    #[must_use]
    pub fn index(self) -> i32 {
        match self.0 {
            ThemeKind::Dark => 0,
            ThemeKind::Light => 1,
        }
    }

    /// Resolves the segmented control's pick back to a choice (the
    /// same order [`Self::index`] emits — one source, the
    /// `preset_index` precedent).
    #[must_use]
    pub fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::DARK),
            1 => Some(Self::LIGHT),
            _ => None,
        }
    }
}

/// Pushes a palette into the generated globals (design D2): the 9
/// `Theme` palette properties plus the 5 `Semantic` state literals.
/// ~14 setter calls — the live-push cost (D4: no window rebuild, no
/// restart; the `SetScale` precedent).
pub(crate) fn apply(window: &crate::AppWindow, palette: &Palette) {
    use crate::Semantic as SemanticGlobal;
    use crate::Theme as ThemeGlobal;
    use slint::ComponentHandle as _;

    let theme = window.global::<ThemeGlobal>();
    theme.set_background(slint::Color::from_rgb_u8(
        (palette.background >> 16) as u8,
        (palette.background >> 8) as u8,
        palette.background as u8,
    ));
    theme.set_surface(color(palette.surface));
    theme.set_surface_raised(color(palette.surface_raised));
    theme.set_surface_hover(color(palette.surface_hover));
    theme.set_text_primary(color(palette.text_primary));
    theme.set_text_secondary(color(palette.text_secondary));
    theme.set_border(color(palette.border));
    theme.set_focus_ring(color(palette.focus_ring));
    theme.set_scrim(palette.scrim_color());

    let semantic = window.global::<SemanticGlobal>();
    semantic.set_success(color(palette.success));
    semantic.set_warning(color(palette.warning));
    semantic.set_danger(color(palette.danger));
    semantic.set_overdue(color(palette.overdue));
    semantic.set_sync_active(color(palette.sync_active));
}

/// `0xRRGGBB` → the slint `Color`.
fn color(rgb: u32) -> slint::Color {
    slint::Color::from_rgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The closed set rejects garbage and accepts exactly its words —
    /// the A1 mint's fail-fast contract (design §1.4).
    #[test]
    fn the_theme_name_mint_is_the_closed_two_value_set() {
        assert_eq!(ThemeName::new("dark"), Ok(ThemeName::DARK));
        assert_eq!(ThemeName::new("light"), Ok(ThemeName::LIGHT));
        assert_eq!(ThemeName::new("Dark"), Err(ThemeNameError));
        assert_eq!(ThemeName::new("system"), Err(ThemeNameError));
        assert_eq!(ThemeName::new(""), Err(ThemeNameError));
    }

    /// Index ↔ name ↔ palette agreement: the segmented control's
    /// index order is the request seam's contract, and each choice
    /// selects its own palette.
    #[test]
    fn index_name_and_palette_agree_per_choice() {
        for index in 0..2 {
            let theme = ThemeName::from_index(index).expect("closed set round-trips");
            assert_eq!(theme.index(), i32::try_from(index).expect("small"));
            assert_eq!(ThemeName::new(theme.as_str()), Ok(theme));
        }
        assert!(ThemeName::from_index(2).is_none());
        assert_eq!(ThemeName::DARK.palette(), &Palette::DARK);
        assert_eq!(ThemeName::LIGHT.palette(), &Palette::LIGHT);
        assert_ne!(Palette::DARK, Palette::LIGHT, "the pair is distinct");
    }

    /// The surface ladder's per-palette invariant (the r2 D35 law,
    /// parameterized): each step paints over darker steps only —
    /// ascending luminance dark, descending light — so the ladder's
    /// structural order holds under either palette. The contrast-side
    /// guard (the hover step ≥ the surface→raised step) lives in the
    /// token tests, parameterized there over both palettes.
    #[test]
    fn the_surface_ladder_orders_per_palette() {
        for palette in [Palette::DARK, Palette::LIGHT] {
            let lum = |rgb: u32| {
                let linear = |byte: u8| {
                    let c = f64::from(byte) / 255.0;
                    if c <= 0.04045 {
                        c / 12.92
                    } else {
                        ((c + 0.055) / 1.055).powf(2.4)
                    }
                };
                0.2126 * linear((rgb >> 16) as u8)
                    + 0.7152 * linear((rgb >> 8) as u8)
                    + 0.0722 * linear(rgb as u8)
            };
            let steps = [
                lum(palette.background),
                lum(palette.surface),
                lum(palette.surface_raised),
                lum(palette.surface_hover),
            ];
            let ascending = steps.windows(2).all(|w| w[0] < w[1]);
            let descending = steps.windows(2).all(|w| w[0] > w[1]);
            assert!(
                ascending || descending,
                "the four surface steps are strictly monotonic per palette"
            );
        }
    }

    /// The 8-digit scrim convention holds per palette: a translucent
    /// wash (alpha byte < 0xFF), never an opaque text carrier.
    #[test]
    fn both_scrims_are_translucent() {
        for palette in [Palette::DARK, Palette::LIGHT] {
            assert_eq!(palette.scrim & 0xff, 0x80, "the wash alpha stays ~50%");
        }
    }

    /// The scrim's byte-order pin: the cell is `0xRRGGBBAA` (the slint
    /// literal's order), the `Color` constructor is ARGB — the push
    /// must swap the halves, or the wash lands nearly transparent (the
    /// capture-overlay dark goldens caught exactly that drift).
    #[test]
    fn the_scrim_color_swaps_the_rgba_halves_into_argb() {
        assert_eq!(
            Palette::DARK.scrim_color(),
            slint::Color::from_argb_encoded(0x800b_0d12),
        );
        assert_eq!(
            Palette::LIGHT.scrim_color(),
            slint::Color::from_argb_encoded(0x801a_1c22),
        );
    }
}

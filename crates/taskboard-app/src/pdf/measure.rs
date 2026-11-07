// SPDX-License-Identifier: AGPL-3.0-only

//! The print layout's measure and wrap helpers (plan §5): the
//! [`Measure`] seam that keeps the layout engine pure, the production
//! [`FontMetrics`] over the embedded Noto Sans (via skrifa — krilla
//! exposes no glyph advances), and the greedy [`wrap`] with char-boundary
//! hard breaks.
//!
//! Known approximation (recorded at the seam): summed advances ignore
//! kerning, while krilla shapes with rustybuzz — the rendered width can
//! differ slightly, almost always narrower in Latin text. The layout's
//! per-line safety slack absorbs this; width checks are floors, never
//! ceilings. Single-script only: krilla's `draw_text` does no BIDI —
//! the content is German and English, accepted by the analysis report.

/// Widths in points for a given font size — the seam between the pure
/// layout engine and the concrete font.
pub trait Measure {
    /// The width of `text` when set at `size_pt`.
    fn advance_pt(&self, text: &str, size_pt: f32) -> f32;
}

/// Production measure over a static font file (the embedded Noto
/// Sans). Sums per-glyph advances scaled by `size / units_per_em` (a
/// missing glyph measures as `.notdef`; a glyph with no recorded
/// advance measures as zero width — both lenient, deliberate: the
/// byte-level coverage is pinned by tests instead of a runtime
/// refusal).
pub struct FontMetrics {
    font: skrifa::raw::FontRef<'static>,
}

impl FontMetrics {
    /// Builds the measure over a static font file (the
    /// `include_bytes!`-embedded TTFs live for `'static`).
    ///
    /// # Errors
    ///
    /// [`FontMetricsError::NotParseable`] when the bytes are not a
    /// readable font — an embedding bug, surfaced typed at boot.
    pub fn from_static(data: &'static [u8]) -> Result<Self, FontMetricsError> {
        skrifa::raw::FontRef::new(data)
            .map(|font| Self { font })
            .map_err(|_err| FontMetricsError)
    }

    /// The embedded Noto Sans Regular (the print body weight).
    ///
    /// # Panics
    ///
    /// Never in a correct build: the vendored TTF is tested parseable
    /// (`font_metrics_measures_the_embedded_notosans`).
    pub fn embedded_regular() -> Self {
        const REGULAR: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Regular.ttf");
        Self::from_static(REGULAR).expect("the vendored Noto Sans Regular parses")
    }
}

/// The static font file was not parseable — an embedding bug, not a
/// runtime contingency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("embedded font file is not parseable")]
pub struct FontMetricsError;

impl Measure for FontMetrics {
    fn advance_pt(&self, text: &str, size_pt: f32) -> f32 {
        use skrifa::GlyphId;
        use skrifa::prelude::MetadataProvider as _;
        let charmap = self.font.charmap();
        let metrics = self.font.glyph_metrics(
            skrifa::instance::Size::new(size_pt),
            skrifa::instance::LocationRef::default(),
        );
        text.chars()
            .map(|ch| {
                let glyph = charmap.map(ch).unwrap_or(GlyphId::NOTDEF);
                metrics.advance_width(glyph).unwrap_or(0.0)
            })
            .sum()
    }
}

/// Greedy whitespace wrap: breaks between words, hard-breaks a single
/// word wider than the line at a char boundary (never a byte boundary
/// — umlauts survive), trims lines, drops nothing. Empty input wraps
/// to no lines. A non-positive `max_w_pt` degenerates to one char per
/// line (every break is forced) — progress is guaranteed.
#[must_use]
pub fn wrap(text: &str, max_w_pt: f32, size_pt: f32, m: &impl Measure) -> Vec<String> {
    let fits = |s: &str| m.advance_pt(s, size_pt) <= max_w_pt;
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if fits(&candidate) {
            current = candidate;
            continue;
        }
        if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        // The word alone overflows even a fresh line: hard-break at
        // char boundaries. The empty-piece case always accepts (the
        // forced break), so a single overflowing char still makes
        // progress on its own line.
        let mut piece = String::new();
        for ch in word.chars() {
            let mut extended = piece.clone();
            extended.push(ch);
            if piece.is_empty() || fits(&extended) {
                piece = extended;
            } else {
                out.push(std::mem::take(&mut piece));
                piece = ch.to_string();
            }
        }
        current = piece;
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
pub(crate) struct FixedWidth(pub(crate) f32);

#[cfg(test)]
impl Measure for FixedWidth {
    fn advance_pt(&self, text: &str, size_pt: f32) -> f32 {
        self.0 * size_pt * f32::from(u16::try_from(text.chars().count()).unwrap_or(u16::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// The umlaut/quote coverage pin (dispatch acceptance): the
    /// embedded faces map every glyph of the German fixture (no
    /// `.notdef` fallback) and measure it positively.
    #[test]
    fn font_metrics_measures_the_embedded_notosans() {
        let m = FontMetrics::embedded_regular();
        let bold = FontMetrics::from_static(include_bytes!("../../assets/fonts/NotoSans-Bold.ttf"))
            .expect("the vendored Noto Sans Bold parses");
        for metrics in [&m, &bold] {
            let charmap = skrifa::prelude::MetadataProvider::charmap(&metrics.font);
            for ch in ['M', 'ü', 'ß', 'g', '„', '“', '–', 'a', ' ', '0', '.', ':'] {
                assert!(charmap.map(ch).is_some(), "glyph {ch:?} is not covered");
                let w = metrics.advance_pt(&ch.to_string(), 10.0);
                assert!(w > 0.0, "glyph {ch:?} measures {w}");
            }
            let word = "Müßiggang „Prüfung“ – Größe";
            let w = metrics.advance_pt(word, 8.0);
            assert!(w > 0.0);
        }
    }

    /// Regular vs bold differ (the heading weight is really a weight).
    #[test]
    fn bold_measures_wider_than_regular_on_a_heading() {
        let regular = FontMetrics::embedded_regular();
        let bold = FontMetrics::from_static(include_bytes!("../../assets/fonts/NotoSans-Bold.ttf"))
            .expect("the vendored Noto Sans Bold parses");
        assert!(bold.advance_pt("Overdue", 10.0) > regular.advance_pt("Overdue", 10.0));
    }

    #[test]
    fn wrap_breaks_between_words_and_trims() {
        let m = FixedWidth(1.0);
        // Every char is 1·size wide: a 10 pt line fits 10 chars.
        let lines = wrap("aa bb cccccc dd", 10.0, 1.0, &m);
        assert_eq!(lines, vec!["aa bb", "cccccc dd"]);
    }

    #[test]
    fn wrap_hard_breaks_an_overflowing_word_at_char_boundaries() {
        let m = FixedWidth(1.0);
        let lines = wrap("abcdefghijklmnop", 5.0, 1.0, &m);
        assert_eq!(lines, vec!["abcde", "fghij", "klmno", "p"]);
    }

    #[test]
    fn wrap_hard_break_keeps_umlauts_intact() {
        let m = FixedWidth(1.0);
        // 'ü' is two bytes: a byte-boundary break would split it.
        let lines = wrap("üüüüüüüüüü", 5.0, 1.0, &m);
        assert_eq!(lines, vec!["üüüüü", "üüüüü"]);
    }

    #[test]
    fn wrap_of_empty_input_is_no_lines() {
        let m = FixedWidth(1.0);
        assert_eq!(wrap("", 10.0, 1.0, &m), Vec::<String>::new());
        assert_eq!(wrap("   ", 10.0, 1.0, &m), Vec::<String>::new());
    }

    #[test]
    fn wrap_of_a_non_positive_width_makes_progress_per_char() {
        let m = FixedWidth(1.0);
        assert_eq!(wrap("abc", 0.0, 1.0, &m), vec!["a", "b", "c"]);
    }

    proptest! {
        /// Nothing is lost or reordered: concatenating the lines and
        /// ignoring whitespace reproduces the input's non-whitespace
        /// characters, in order.
        #[test]
        fn wrap_preserves_every_non_whitespace_char(
            text in proptest::string::string_regex("[a-zA-Z0-9 äöüß „“–]{0,200}").unwrap(),
            width in 1.0..40.0f32,
        ) {
            let m = FixedWidth(1.0);
            let lines = wrap(&text, width, 1.0, &m);
            let joined: String = lines.concat().chars().filter(|c| !c.is_whitespace()).collect();
            let expected: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            prop_assert_eq!(joined, expected);
        }

        /// Every line measures within the width, except a forced
        /// single-character break (a char wider than the whole line
        /// cannot fit at any split point). With FixedWidth(1) at size
        /// 1, lines are ≤ ceil(width) chars, and the only allowed
        /// longer line is length 1 when width < 1 — excluded by the
        /// strategy (width ≥ 1).
        #[test]
        fn every_wrapped_line_fits_the_width(
            text in proptest::string::string_regex("[a-zA-Z0-9 äöüß]{0,200}").unwrap(),
            width in 1.0..40.0f32,
        ) {
            let m = FixedWidth(1.0);
            for line in wrap(&text, width, 1.0, &m) {
                prop_assert!(m.advance_pt(&line, 1.0) <= width, "line {line:?} overflows {width}");
            }
        }
    }
}

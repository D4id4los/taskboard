// SPDX-License-Identifier: AGPL-3.0-only

//! The krilla renderer (plan §6): a display list of sheets in —
//! deterministic PDF bytes out. Determinism comes from the inputs
//! alone: document metadata dates are never set, so krilla emits none
//! (the byte smoke test stays stable).

use krilla::Document;
use krilla::geom::{PathBuilder, Point, Transform};
use krilla::page::PageSettings;
use krilla::paint::Stroke;
use krilla::text::{Font, TextDirection};

use crate::pdf::layout::{DrawOp, Sheet, Weight};

/// The embedded faces (Noto Sans Regular + Bold), built once per run.
pub(crate) struct FontAssets {
    /// The body weight.
    pub regular: Font,
    /// The heading weight.
    pub bold: Font,
}

impl FontAssets {
    /// The two vendored Noto Sans faces (see `assets/fonts/README.md`
    /// for provenance and the OFL-1.1 record).
    ///
    /// # Panics
    ///
    /// Never in a correct build: the vendored TTFs are tested
    /// parseable (`FontMetrics` coverage tests ride the same files).
    #[must_use]
    pub fn embedded() -> Self {
        const REGULAR: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Regular.ttf");
        const BOLD: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Bold.ttf");
        Self {
            regular: Font::new(REGULAR.into(), 0).expect("the vendored Noto Sans Regular parses"),
            bold: Font::new(BOLD.into(), 0).expect("the vendored Noto Sans Bold parses"),
        }
    }

    /// The face for a weight.
    #[must_use]
    pub fn face(&self, weight: Weight) -> &Font {
        match weight {
            Weight::Regular => &self.regular,
            Weight::Bold => &self.bold,
        }
    }
}

/// The rendering failed.
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    /// A page size was not mintable — a layout bug (a non-positive or
    /// non-finite page), never a runtime contingency.
    #[error("page size rejected")]
    PageSize,
    /// krilla refused to serialize the document.
    #[error("pdf serialization failed")]
    Serialize(#[from] krilla::error::KrillaError),
}

/// Renders the display list into deterministic PDF bytes. No document
/// metadata dates are set, so krilla emits none — the same display
/// list always renders byte-identically (the smoke test pins it).
///
/// # Errors
///
/// [`PdfError`] — see the variant docs.
pub fn render(sheets: &[Sheet], fonts: &FontAssets) -> Result<Vec<u8>, PdfError> {
    let mut document = Document::new();
    for sheet in sheets {
        let settings = PageSettings::from_wh(sheet.size.w_pt(), sheet.size.h_pt())
            .ok_or(PdfError::PageSize)?;
        let mut page = document.start_page_with(settings);
        let mut surface = page.surface();
        for op in &sheet.ops {
            let transform = panel_transform(op.panel());
            let pushed = transform.is_some();
            if let Some(t) = &transform {
                surface.push_transform(t);
            }
            match op {
                DrawOp::Text {
                    at,
                    size_pt,
                    weight,
                    text,
                    ..
                } => surface.draw_text(
                    Point::from_xy(at.x, at.y),
                    fonts.face(*weight).clone(),
                    *size_pt,
                    text,
                    false,
                    TextDirection::LeftToRight,
                ),
                DrawOp::Rule {
                    from, to, width_pt, ..
                } => {
                    let mut builder = PathBuilder::new();
                    builder.move_to(from.x, from.y);
                    builder.line_to(to.x, to.y);
                    if let Some(path) = builder.finish() {
                        let stroke = Stroke {
                            width: *width_pt,
                            ..Stroke::default()
                        };
                        surface.set_stroke(Some(stroke));
                        surface.draw_path(&path);
                        surface.set_stroke(None);
                    }
                }
                DrawOp::Checkbox {
                    at,
                    side_pt,
                    ticked,
                    ..
                } => {
                    draw_checkbox(&mut surface, at.x, at.y, *side_pt, *ticked);
                }
            }
            if pushed {
                surface.pop();
            }
        }
        surface.finish();
        page.finish();
    }
    Ok(document.finish()?)
}

trait OpPanel {
    fn panel(&self) -> &crate::pdf::layout::PanelXf;
}

impl OpPanel for DrawOp {
    fn panel(&self) -> &crate::pdf::layout::PanelXf {
        match self {
            DrawOp::Text { panel, .. }
            | DrawOp::Rule { panel, .. }
            | DrawOp::Checkbox { panel, .. } => panel,
        }
    }
}

/// The krilla transform of a panel placement; `None` for the identity
/// (no graphics-state push needed).
fn panel_transform(panel: &crate::pdf::layout::PanelXf) -> Option<Transform> {
    match panel {
        crate::pdf::layout::PanelXf::Identity => None,
        crate::pdf::layout::PanelXf::Translated { dx, dy } => {
            Some(Transform::from_row(1.0, 0.0, 0.0, 1.0, *dx, *dy))
        }
        crate::pdf::layout::PanelXf::Rot180 { tx, ty } => {
            Some(Transform::from_row(-1.0, 0.0, 0.0, -1.0, *tx, *ty))
        }
    }
}

/// The checkbox frame plus, when ticked, the tick stroke — paths, not
/// font glyphs, so they never depend on glyph coverage.
fn draw_checkbox(
    surface: &mut krilla::surface::Surface<'_>,
    x: f32,
    y: f32,
    side: f32,
    ticked: bool,
) {
    let mut builder = PathBuilder::new();
    if let Some(rect) = krilla::geom::Rect::from_xywh(x, y, side, side) {
        builder.push_rect(rect);
    }
    if let Some(frame) = builder.finish() {
        surface.set_stroke(Some(Stroke {
            width: 0.7,
            ..Stroke::default()
        }));
        surface.draw_path(&frame);
        surface.set_stroke(None);
    }
    if ticked {
        let mut builder = PathBuilder::new();
        builder.move_to(x + 0.2 * side, y + 0.55 * side);
        builder.line_to(x + 0.42 * side, y + 0.78 * side);
        builder.line_to(x + 0.85 * side, y + 0.2 * side);
        if let Some(tick) = builder.finish() {
            surface.set_stroke(Some(Stroke {
                width: 0.8,
                ..Stroke::default()
            }));
            surface.draw_path(&tick);
            surface.set_stroke(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FontAssets, render};
    use crate::pdf::layout::layout_day;
    use krilla::geom::Point;
    use krilla::page::PageSettings;
    use krilla::text::{Font, TextDirection};

    /// The plan §9 step-1 spike: render one glyph at (10, 10) on an A4
    /// page and read the page content stream to pin krilla's coordinate
    /// convention before any geometry is built on it.
    #[test]
    fn krilla_origin_spike_pins_the_coordinate_convention() {
        use flate2::read::ZlibDecoder;
        use std::io::Read as _;

        // Byte-level substring find (PDF streams carry binary font
        // data, so no UTF-8 view — its offsets would drift).
        fn find_sub(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
            if haystack.len() < needle.len() || from > haystack.len() - needle.len() {
                return None;
            }
            (from..=haystack.len() - needle.len())
                .find(|&i| &haystack[i..i + needle.len()] == needle)
        }

        let regular: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Regular.ttf");
        let font = Font::new(regular.into(), 0).expect("the embedded Noto Sans parses");

        let mut document = krilla::Document::new();
        let mut page = document
            .start_page_with(PageSettings::from_wh(595.276, 841.890).expect("A4 is positive"));
        let mut surface = page.surface();
        surface.draw_text(
            Point::from_xy(10.0, 10.0),
            font,
            20.0,
            "x",
            false,
            TextDirection::LeftToRight,
        );
        surface.finish();
        page.finish();
        let bytes = document.finish().expect("a one-page document serializes");
        assert!(bytes.starts_with(b"%PDF-"));

        // Inflate every zlib stream; the page content stream is the one
        // with text ops.
        let mut found: Option<Vec<u8>> = None;
        let mut pos = 0;
        while let Some(rel) = find_sub(&bytes, b"\nstream\n", pos) {
            let start = rel + "\nstream\n".len();
            let end = find_sub(&bytes, b"endstream", start).expect("well-formed streams");
            let mut decoder = ZlibDecoder::new(&bytes[start..end]);
            let mut text = Vec::new();
            if decoder.read_to_end(&mut text).is_ok() && find_sub(&text, b"BT", 0).is_some() {
                found = Some(text);
            }
            pos = end;
        }
        let found = String::from_utf8(found.expect("a page content stream with text ops"))
            .expect("content streams are ASCII");
        println!("---- page content stream ----\n{found}");
        // The pinned convention (the layout model builds on it): a
        // root `cm` flips y — top-left origin, y down, exactly the
        // display list's convention — and the draw's start point lands
        // verbatim in the text matrix. The renderer therefore passes
        // coordinates through without flipping.
        assert!(
            found.contains("1 0 0 -1 10 10 Tm"),
            "expected the (10, 10) placement under a y-flip matrix in:\n{found}"
        );
    }

    // ---- Byte smoke tests (plan §8) ----------------------------------------

    use crate::pdf::layout::{PrintDay, PrintItem, PrintLayout, PrintSection, SectionKind, Sheet};

    fn fixture_sheets() -> Vec<Sheet> {
        let day = PrintDay {
            date: chrono::NaiveDate::from_ymd_opt(2026, 11, 7).expect("valid date"),
            sections: vec![PrintSection {
                kind: SectionKind::Scheduled,
                items: vec![
                    PrintItem {
                        title: "Müßiggang „Prüfung“ – Größe".to_owned(),
                        done: false,
                        time: Some("09:30".to_owned()),
                        effort: None,
                    },
                    PrintItem {
                        title: "Board review".to_owned(),
                        done: true,
                        time: None,
                        effort: None,
                    },
                ],
            }],
        };
        layout_day(
            &day,
            PrintLayout::FoldA6x4,
            &crate::pdf::measure::FixedWidth(0.6),
        )
    }

    fn find_sub(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
        if haystack.len() < needle.len() || from > haystack.len() - needle.len() {
            return None;
        }
        (from..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
    }

    fn count_sub(haystack: &[u8], needle: &[u8]) -> usize {
        let mut count = 0;
        let mut pos = 0;
        while let Some(rel) = find_sub(haystack, needle, pos) {
            count += 1;
            pos = rel + needle.len();
        }
        count
    }

    /// The structural byte checks (plan §8): header, page count, the
    /// embedded font, the A4 media box, and byte-determinism.
    #[test]
    fn render_smoke_structural_checks_hold() {
        let sheets = fixture_sheets();
        let bytes = render(&sheets, &FontAssets::embedded()).expect("renders");

        assert!(bytes.starts_with(b"%PDF-"));
        // One page dict per sheet: the media box token is per page
        // (the /Type token's spelling varies with the object layout).
        assert_eq!(count_sub(&bytes, b"/MediaBox"), sheets.len());
        // The embedded subsetter output is really in there.
        assert!(find_sub(&bytes, b"/FontFile2", 0).is_some());
        // The fold sheet's A4 media box: the token count above is per
        // page; here the A4 width is pinned (the height's spelling is
        // krilla's float formatting, so only the prefix is asserted).
        let mediabox = find_sub(&bytes, b"/MediaBox", 0)
            .map(|i| &bytes[i..i + 24])
            .expect("a media box");
        assert!(std::str::from_utf8(mediabox).is_ok_and(|s| s.contains("595.27")),);
    }

    #[test]
    fn render_is_byte_deterministic() {
        let sheets = fixture_sheets();
        let fonts = FontAssets::embedded();
        let first = render(&sheets, &fonts).expect("renders");
        let second = render(&sheets, &fonts).expect("renders");
        assert_eq!(first, second);
    }

    /// The rendered page carries a text object (the umlaut/quote
    /// fixture really draws), and the rotated panels' ops are wrapped
    /// in a graphics-state transform (`q ... cm` with the pinned
    /// [-1 0 0 -1] row for the bottom row).
    #[test]
    fn render_draws_text_and_the_rot180_transform() {
        use flate2::read::ZlibDecoder;
        use std::io::Read as _;

        let day = PrintDay {
            date: chrono::NaiveDate::from_ymd_opt(2026, 11, 7).expect("valid date"),
            sections: vec![PrintSection {
                kind: SectionKind::Scheduled,
                items: (0..120)
                    .map(|n| PrintItem {
                        title: format!("Aufgabe {n} Müller „Strüße“"),
                        done: false,
                        time: None,
                        effort: None,
                    })
                    .collect(),
            }],
        };
        let sheets = layout_day(
            &day,
            PrintLayout::FoldA6x4,
            &crate::pdf::measure::FixedWidth(0.6),
        );
        assert_eq!(sheets.len(), 1);
        let bytes = render(&sheets, &FontAssets::embedded()).expect("renders");

        let mut contents: Vec<String> = Vec::new();
        let mut pos = 0;
        while let Some(rel) = find_sub(&bytes, b"\nstream\n", pos) {
            let start = rel + "\nstream\n".len();
            let end = find_sub(&bytes, b"endstream", start).expect("well-formed streams");
            let mut decoder = ZlibDecoder::new(&bytes[start..end]);
            let mut text = Vec::new();
            if decoder.read_to_end(&mut text).is_ok() && find_sub(&text, b"BT", 0).is_some() {
                contents.push(String::from_utf8(text).expect("content streams are ASCII"));
            }
            pos = end;
        }
        assert_eq!(contents.len(), 1, "one content stream with text ops");
        let stream = &contents[0];
        assert!(stream.contains("BT"));
        // The panels' pushed transforms, as krilla concatenates them
        // with the root y-flip: the top-right panel's pure translation
        // and the bottom row's 180° rows.
        assert!(
            find_sub(stream.as_bytes(), b"q -1 0 0 1", 0).is_some()
                && find_sub(stream.as_bytes(), b"q 1 0 0 -1 297.638", 0).is_some(),
            "expected the pushed panel transforms in:\n{stream}"
        );
        // The identity panel draws no push (the cover ops sit at the
        // stream's top, untransformed apart from the page root).
        assert!(stream.contains("1 0 0 -1 0 841.89 cm"));
    }
}

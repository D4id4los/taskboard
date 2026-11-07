// SPDX-License-Identifier: AGPL-3.0-only

//! The krilla renderer (plan §6): a display list of sheets in —
//! deterministic PDF bytes out. Determinism comes from the inputs
//! alone: document metadata dates are never set, so krilla emits none
//! (the byte smoke test stays stable).

#[cfg(test)]
mod tests {
    use krilla::Document;
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

        let mut document = Document::new();
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
}

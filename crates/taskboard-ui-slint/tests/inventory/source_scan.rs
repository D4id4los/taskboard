// SPDX-License-Identifier: AGPL-3.0-only

//! The structural Slint source scanner shared by the inventory
//! members (`view_mount_inventory`, `severing_census`): a comment- and
//! string-blanked view of a source with brace-depth tracking, so
//! `//`-in-strings and interpolated labels cannot desync structural
//! matching, plus the component-declaration helpers both inventories
//! read. Promoted verbatim from `view_mount_inventory` (chunk E1,
//! #1252) — the census reuses the identical technique.

use std::ops::Range;

/// A source blanked for structural scanning: line comments and every
/// character inside string literals (including `\{…}` interpolations
/// and escaped quotes) become spaces, so `//`-in-string and braces in
/// labels cannot desync the scanner. `depth` is byte-indexed (one
/// entry per byte of `text`, multi-byte characters included):
/// `depth[i]` is the number of open braces strictly before byte
/// offset `i`.
pub(crate) struct Blanked {
    pub(crate) text: String,
    pub(crate) depth: Vec<usize>,
}

impl Blanked {
    pub(crate) fn blank(src: &str) -> Self {
        let mut text = String::with_capacity(src.len());
        // One depth entry per byte, so consumers can index `depth`
        // with byte offsets from `text` (multi-byte characters in
        // comments and labels would otherwise shift every later
        // offset).
        let mut depth = Vec::with_capacity(src.len());
        let mut d = 0usize;
        let mut chars = src.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '/' && chars.peek() == Some(&'/') {
                // Line comment: blank to end of line (newline kept).
                text.push(' ');
                depth.push(d);
                for n in chars.by_ref() {
                    if n == '\n' {
                        text.push('\n');
                        depth.push(d);
                        break;
                    }
                    text.push(' ');
                    depth.push(d);
                }
                continue;
            }
            if c == '"' {
                // String literal: quote chars kept, contents blanked.
                text.push(c);
                depth.push(d);
                let mut escaped = false;
                for taken in chars.by_ref() {
                    // Escaped characters are kept verbatim (they may
                    // be multi-byte); everything else is blanked to a
                    // one-byte space. Depth follows the *text's* bytes.
                    let kept = escaped || taken == '"';
                    text.push(if kept { taken } else { ' ' });
                    for _ in 0..(if kept { taken.len_utf8() } else { 1 }) {
                        depth.push(d);
                    }
                    if escaped {
                        escaped = false;
                    } else if taken == '\\' {
                        escaped = true;
                    } else if taken == '"' {
                        break;
                    }
                }
                continue;
            }
            text.push(c);
            for _ in 0..c.len_utf8() {
                depth.push(d);
            }
            if c == '{' {
                d += 1;
            } else if c == '}' {
                d = d.saturating_sub(1);
            }
        }
        Self { text, depth }
    }

    /// The body range of the balanced block opened at byte offset
    /// `open` (a `{`): strictly between the braces. At the matching
    /// `}`, the recorded depth is still one past `open`'s (the
    /// decrement happens after the byte).
    pub(crate) fn body(&self, open: usize) -> Option<Range<usize>> {
        let outer = self.depth.get(open)?.checked_add(1)?;
        let mut i = open + 1;
        while i < self.text.len() {
            if self.text[i..].starts_with('}') && self.depth[i] == outer {
                return Some(open + 1..i);
            }
            i += 1;
        }
        None
    }

    /// Word runs (`[A-Za-z0-9_-]+`, identifier-style) plus the
    /// operator tokens the forwarding check reads (`=>`, `<=>`,
    /// parentheses), as (start, &str), in order. Other punctuation is
    /// skipped.
    pub(crate) fn tokens(&self, range: Range<usize>) -> Vec<(usize, &str)> {
        let bytes = self.text.as_bytes();
        let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
        let mut out = Vec::new();
        let mut i = range.start;
        while i < range.end {
            if is_word(bytes[i]) {
                let start = i;
                while i < range.end && is_word(bytes[i]) {
                    i += 1;
                }
                out.push((start, &self.text[start..i]));
            } else if self.text[i..].starts_with("<=>") {
                out.push((i, "<=>"));
                i += 3;
            } else if self.text[i..].starts_with("=>") {
                out.push((i, "=>"));
                i += 2;
            } else if bytes[i] == b'(' || bytes[i] == b')' {
                out.push((i, &self.text[i..=i]));
                i += 1;
            } else {
                i += 1;
            }
        }
        out
    }
}

/// A component declaration: name and its body's byte range.
pub(crate) struct ComponentDecl {
    pub(crate) name: String,
    pub(crate) name_start: usize,
    pub(crate) body_open: usize,
    pub(crate) body_end: usize,
}

/// Every component declared in a blanked source (any component body
/// may host instantiation sites, exported or not).
pub(crate) fn declared_components(src: &Blanked) -> Vec<ComponentDecl> {
    let mut out = Vec::new();
    for (start, token) in src.tokens(0..src.text.len()) {
        if token != "component" || src.depth[start] != 0 {
            continue;
        }
        let Some((name_start, name)) = src
            .tokens(start + token.len()..src.text.len())
            .into_iter()
            .next()
        else {
            continue;
        };
        // The body's opening brace: the first `{` after the name
        // (the `inherits` clause holds no braces).
        let Some(open) =
            (name_start + name.len()..src.text.len()).find(|&i| src.text[i..].starts_with('{'))
        else {
            continue;
        };
        if src.depth[open] != 0 {
            continue;
        }
        let Some(body) = src.body(open) else { continue };
        out.push(ComponentDecl {
            name: name.to_owned(),
            name_start,
            body_open: open,
            body_end: body.end,
        });
    }
    out
}

/// Whether a component declaration is exported: `export` appears in
/// the tokens just before the component keyword (the gap also holds
/// nothing else — `export component` is the only legal prefix).
pub(crate) fn is_exported(src: &Blanked, comp: &ComponentDecl) -> bool {
    let gap_start = comp.name_start.saturating_sub("export component ".len());
    src.tokens(gap_start..comp.name_start)
        .iter()
        .any(|(_, t)| *t == "export")
}

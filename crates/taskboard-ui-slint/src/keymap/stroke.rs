// SPDX-License-Identifier: AGPL-3.0-only

//! The stroke grammar (chunk decision D6, design §5.3): one non-
//! modifier key plus modifier state, parsed from the config spelling
//! (`"Ctrl+Alt+Delete"`, `"g"`, `"F1"`).
//!
//! Normalization: for *printable* keys shift is expressed by the
//! character itself (`"Shift+A"` ≡ `"A"` — Slint delivers printables
//! case-carrying); for *named* keys `Shift` is a real modifier. The
//! canonical `Display` form (mods in Ctrl, Alt, Shift, Meta order) is
//! the kbd-hint text source.

use std::fmt;

/// The named keys (D6): Slint 1.18.1 delivers them as private-use /
/// control chars (`Key` is char-based). One row per named spelling;
/// a new named key is one row here and nothing else.
const NAMED_KEYS: &[(&str, char)] = &[
    ("Space", '\u{20}'),
    ("Return", '\u{a}'),
    ("Escape", '\u{1b}'),
    ("Tab", '\u{9}'),
    ("Backspace", '\u{8}'),
    ("Delete", '\u{7f}'),
    ("Insert", '\u{F727}'),
    ("Home", '\u{F729}'),
    ("End", '\u{F72B}'),
    ("PageUp", '\u{F72C}'),
    ("PageDown", '\u{F72D}'),
    ("Up", '\u{F700}'),
    ("Down", '\u{F701}'),
    ("Left", '\u{F702}'),
    ("Right", '\u{F703}'),
    ("F1", '\u{F704}'),
    ("F2", '\u{F705}'),
    ("F3", '\u{F706}'),
    ("F4", '\u{F707}'),
    ("F5", '\u{F708}'),
    ("F6", '\u{F709}'),
    ("F7", '\u{F70A}'),
    ("F8", '\u{F70B}'),
    ("F9", '\u{F70C}'),
    ("F10", '\u{F70D}'),
    ("F11", '\u{F70E}'),
    ("F12", '\u{F70F}'),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mod {
    Ctrl,
    Alt,
    Shift,
    Meta,
}

impl Mod {
    const fn spelling(self) -> &'static str {
        match self {
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Meta => "Meta",
        }
    }

    fn from_spelling(token: &str) -> Option<Self> {
        match token {
            "Ctrl" => Some(Self::Ctrl),
            "Alt" => Some(Self::Alt),
            "Shift" => Some(Self::Shift),
            "Meta" => Some(Self::Meta),
            _ => None,
        }
    }
}

/// Why a stroke spelling could not be parsed. Typed outcomes only —
/// callers match variants, never text (AGENTS §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StrokeParseError {
    /// The empty string.
    #[error("empty stroke")]
    Empty,
    /// Only modifiers, no key (`"Ctrl"`).
    #[error("modifier-only stroke")]
    ModifierOnly,
    /// The same modifier spelled twice (`"Ctrl+Ctrl+a"`).
    #[error("duplicate modifier")]
    DuplicateModifier,
    /// A token that is neither a modifier nor a known key spelling.
    #[error("unknown key token")]
    UnknownKey,
    /// A control character used as a printable key.
    #[error("control character is not a bindable key")]
    ControlChar,
}

/// One stroke: a key plus modifier state, post-normalization. `key`
/// is either a printable scalar or a named-key char (the [`NAMED_KEYS`]
/// table); the bools mirror Slint's `KeyboardModifiers`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
// The four bools *are* the domain: they mirror Slint's
// `KeyboardModifiers` verbatim (D6); a bitfield would obscure both
// the parse rules and the event-side conversion.
#[allow(clippy::struct_excessive_bools)]
pub struct Stroke {
    pub key: char,
    pub ctrl: bool,
    pub alt: bool,
    /// Meaningful only for named keys (shift-folded away for
    /// printables — D6).
    pub shift: bool,
    pub meta: bool,
}

impl Stroke {
    /// The canonical modifier list in display order.
    fn mods(self) -> Vec<Mod> {
        let mut mods = Vec::with_capacity(4);
        if self.ctrl {
            mods.push(Mod::Ctrl);
        }
        if self.alt {
            mods.push(Mod::Alt);
        }
        if self.shift {
            mods.push(Mod::Shift);
        }
        if self.meta {
            mods.push(Mod::Meta);
        }
        mods
    }

    #[must_use]
    pub const fn named_name(self) -> Option<&'static str> {
        let mut i = 0;
        while i < NAMED_KEYS.len() {
            if NAMED_KEYS[i].1 == self.key {
                return Some(NAMED_KEYS[i].0);
            }
            i += 1;
        }
        None
    }
}

impl fmt::Display for Stroke {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for m in self.mods() {
            write!(f, "{}+", m.spelling())?;
        }
        match self.named_name() {
            Some(name) => f.write_str(name),
            None => write!(f, "{}", self.key),
        }
    }
}

/// Whether `c` may appear as a printable key: any scalar that is not
/// a control character, a named-key char, or a modifier-space char.
fn is_printable(c: char) -> bool {
    !c.is_control() && NAMED_KEYS.iter().all(|&(_, ch)| ch != c)
}

/// Parses one stroke spelling (D6 grammar). Pure and total: every
/// malformed input maps to a typed [`StrokeParseError`], nothing
/// panics on user input.
///
/// # Panics
///
/// Never: the empty-segment access is guarded (checked above), so
/// no indexing or `unwrap` can fire.
///
/// # Errors
///
/// [`StrokeParseError`] — typed, see the variant docs.
pub fn parse(spelling: &str) -> Result<Stroke, StrokeParseError> {
    let mut parts = spelling.split('+');
    let last = match parts.next_back() {
        Some(last) if !last.is_empty() || spelling.contains('+') => last,
        _ => return Err(StrokeParseError::Empty),
    };
    // `+`-split degenerate forms (leading/trailing `+`, empty middles)
    // leave empty non-key segments.
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut meta = false;
    for token in parts {
        let m = Mod::from_spelling(token).ok_or(StrokeParseError::UnknownKey)?;
        let seen = match m {
            Mod::Ctrl => &mut ctrl,
            Mod::Alt => &mut alt,
            Mod::Shift => &mut shift,
            Mod::Meta => &mut meta,
        };
        if *seen {
            return Err(StrokeParseError::DuplicateModifier);
        }
        *seen = true;
    }
    if last.is_empty() || Mod::from_spelling(last).is_some() {
        return Err(StrokeParseError::ModifierOnly);
    }
    if let Some((name, ch)) = NAMED_KEYS.iter().find(|(name, _)| *name == last) {
        let _ = name;
        return Ok(Stroke {
            key: *ch,
            ctrl,
            alt,
            shift,
            meta,
        });
    }
    let mut chars = last.chars();
    let (c, second) = (chars.next().expect("non-empty checked above"), chars.next());
    if second.is_some() {
        return Err(StrokeParseError::UnknownKey);
    }
    if c.is_control() {
        return Err(StrokeParseError::ControlChar);
    }
    if !is_printable(c) {
        return Err(StrokeParseError::UnknownKey);
    }
    // Shift-folding (D6): a *bare* printable carries shift in the
    // character (`"Shift+a"` ≡ `"A"` — Slint delivers printables
    // case-carrying). Inside a chord (`Ctrl`/`Alt`/`Meta`), shift is a
    // real modifier — `Ctrl+Shift+Z` must stay distinct from
    // `Ctrl+Z`, exactly as the backend delivers the two events.
    let chorded = ctrl || alt || meta;
    let (key, shift) = if chorded {
        (c, shift)
    } else {
        (if shift { c.to_ascii_uppercase() } else { c }, false)
    };
    Ok(Stroke {
        key,
        ctrl,
        alt,
        shift,
        meta,
    })
}

/// Parses a whole sequence (the config array form, D7). An empty
/// sequence is malformed at the binding level, not here.
///
/// # Errors
///
/// The first [`StrokeParseError`] in the sequence.
pub fn parse_sequence<'a>(
    spellings: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<Stroke>, StrokeParseError> {
    spellings.into_iter().map(parse).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn ok(spelling: &str) -> Stroke {
        parse(spelling).expect("valid spelling")
    }

    #[test]
    fn parses_plain_printables_case_sensitively() {
        assert_eq!(ok("j").key, 'j');
        assert_eq!(ok("J").key, 'J');
        assert_eq!(ok("/").key, '/');
        assert_eq!(ok(":").key, ':');
        assert_ne!(ok("j"), ok("J"));
    }

    #[test]
    fn parses_named_keys() {
        assert_eq!(ok("Escape").key, '\u{1b}');
        assert_eq!(ok("Down").key, '\u{F701}');
        assert_eq!(ok("F12").key, '\u{F70F}');
        assert_eq!(ok("Space").key, ' ');
    }

    #[test]
    fn modifier_order_is_irrelevant_and_normalized_away_in_display() {
        let any_order = ok("Alt+Ctrl+a");
        assert!(any_order.ctrl && any_order.alt);
        assert_eq!(any_order.to_string(), "Ctrl+Alt+a");
    }

    /// Shift-folding (D6): *bare* printables carry shift in the
    /// character; named keys keep a real shift modifier; chorded
    /// printables keep shift so `Ctrl+Shift+Z` ≠ `Ctrl+Z` (the
    /// undo/redo pair in the standard preset depends on it).
    #[test]
    fn shift_folds_for_bare_printables_but_not_named_or_chorded_keys() {
        let folded = ok("Shift+a");
        assert_eq!(folded.key, 'A');
        assert!(!folded.shift);
        assert_eq!(folded, ok("A"));

        let named = ok("Shift+Up");
        assert!(named.shift);
        assert_ne!(named, ok("Up"));
        assert_eq!(named.to_string(), "Shift+Up");

        let undo = ok("Ctrl+Z");
        let redo = ok("Ctrl+Shift+Z");
        assert_ne!(undo, redo);
        assert!(!undo.shift);
        assert!(redo.shift);
        assert_eq!(redo.to_string(), "Ctrl+Shift+Z");
        assert_eq!(undo.to_string(), "Ctrl+Z");
    }

    #[test]
    fn duplicate_modifier_is_typed_malformed() {
        assert_eq!(
            parse("Ctrl+Ctrl+a"),
            Err(StrokeParseError::DuplicateModifier)
        );
    }

    /// The malformed classes (T4): each maps to its own variant, so
    /// validation findings stay typed.
    #[test]
    fn malformed_classes_are_typed() {
        assert_eq!(parse(""), Err(StrokeParseError::Empty));
        assert_eq!(parse("Ctrl"), Err(StrokeParseError::ModifierOnly));
        assert_eq!(parse("Ctrl+Shift"), Err(StrokeParseError::ModifierOnly));
        assert_eq!(parse("Ctrl+Bogus"), Err(StrokeParseError::UnknownKey));
        assert_eq!(parse("Ctrl+ab"), Err(StrokeParseError::UnknownKey));
        assert_eq!(parse("\u{1}"), Err(StrokeParseError::ControlChar));
        // `Ctrl+<unknown>` must not be misread as modifier-only.
        assert_eq!(parse("Ctrl+"), Err(StrokeParseError::ModifierOnly));
    }

    // T5: parse ∘ Display ∘ parse is the identity over every
    // generatable stroke spelling (canonical forms included).
    proptest! {
        #[test]
        fn parse_display_parse_is_idempotent(
            ctrl in proptest::bool::ANY,
            alt in proptest::bool::ANY,
            meta in proptest::bool::ANY,
            named in 0..NAMED_KEYS.len(),
            printable in "[a-zA-Z0-9]",
        ) {
            let named_stroke = Stroke { key: NAMED_KEYS[named].1, ctrl, alt, shift: true, meta };
            let printable_stroke = Stroke { key: printable.chars().next().expect("one char"), ctrl, alt, shift: false, meta };
            for stroke in [named_stroke, printable_stroke] {
                let displayed = stroke.to_string();
                prop_assert_eq!(parse(&displayed).expect("canonical display reparses"), stroke);
                prop_assert_eq!(parse(&displayed).expect("again").to_string(), displayed);
            }
        }
    }
}

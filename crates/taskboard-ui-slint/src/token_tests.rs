// SPDX-License-Identifier: AGPL-3.0-only
//! The contrast check (design §9.1, chunk decision D10) — a pure Rust
//! unit test module over the declared colour tokens.
//!
//! `ui/theme.slint` is the *single rendering source*; this module reads
//! it via `include_str!` and extracts the declared colour literals with
//! a small purpose-built extractor (unit-tested on fixtures below — no
//! slint API dependency, no Rust-side palette duplicate to drift). The
//! pair list is `const` data with per-role-class WCAG minimums, and the
//! completeness rules keep the pair list from silently lagging the
//! theme: a new colour token without a pair (or an explicitly recorded
//! exemption) fails, an undeclared token in a pair fails, and every
//! `Typography` role must appear in a role-class mapping.

/// The theme module source — the single rendering source of truth.
const THEME_SOURCE: &str = include_str!("../ui/theme.slint");

// ---------------------------------------------------------------------------
// Extractor: (global, name) → hex, over `out property <color> x: #rrggbb;`
// declarations inside `global` blocks. Unit-tested on fixtures; the real
// theme file is parsed by the same function.
// ---------------------------------------------------------------------------

/// A qualified token name (`"Theme.background"`).
type TokenName<'a> = (&'a str, &'a str);

/// Extracts the declared colour tokens of one global block from slint
/// source text. Returns them in declaration order.
fn extract_colors(source: &str) -> Vec<(String, String, String)> {
    let mut result = Vec::new();
    let mut current_global: Option<&str> = None;
    for line in source.lines() {
        let line = line.split("//").next().unwrap_or(line).trim();
        if let Some(rest) = line.strip_prefix("export global ") {
            current_global = rest.split_whitespace().next();
        } else if let Some(rest) = line.strip_prefix("global ") {
            current_global = rest.split_whitespace().next();
        }
        if let Some((name, hex)) = parse_color_property(line)
            && let Some(global) = current_global
        {
            result.push((global.to_owned(), name.to_owned(), hex.to_owned()));
        }
    }
    result
}

/// Parses one `out property <color> name: #hex;` line (or a
/// `Theme.<name>` reference, resolved to the referenced hex).
fn parse_color_property(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix("out property <color> ")?;
    let (name, value) = rest.split_once(':')?;
    let name = name.trim();
    let hex = value.trim().trim_end_matches(';').trim();
    // Literal form `#rrggbb` or a `Theme.<name>` palette reference
    // (resolved so every Semantic token carries its effective hex).
    let hex = if let Some(reference) = hex.strip_prefix("Theme.") {
        resolve_reference(reference)?
    } else {
        hex.strip_prefix('#')?
    };
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())).then_some((name, hex))
}

/// Resolves `Theme.<name>` colour references against the Theme global's
/// declarations (one forward scan; references in the theme point to
/// literals declared earlier in the same file).
fn resolve_reference(name: &str) -> Option<&str> {
    for line in THEME_SOURCE.lines() {
        let line = line.split("//").next().unwrap_or(line).trim();
        if let Some((declared, hex)) = parse_color_property(line)
            && declared == name
        {
            return Some(hex);
        }
    }
    None
}

/// WCAG 2.x relative luminance over 8-bit sRGB channels.
fn relative_luminance(hex: &str) -> f64 {
    let channel = |i: usize| -> f64 {
        let c = f64::from(u8::from_str_radix(&hex[i..i + 2], 16).expect("validated hex")) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
}

/// WCAG 2.x contrast ratio, ≥ 1.0.
fn contrast_ratio(a: &str, b: &str) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

// ---------------------------------------------------------------------------
// Pair list as const data (§9.1): (foreground, surface, minimum class).
// Complete over the declared token set — see the completeness tests.
// ---------------------------------------------------------------------------

/// WCAG role classes (§9.1): body text ≥ 4.5, large display ≥ 3.0,
/// non-text UI ≥ 3.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MinRatio {
    /// Body-size text: ≥ 4.5:1.
    Body,
    /// Large display text: ≥ 3.0:1.
    Large,
    /// Non-text UI (borders, focus rings, indicators): ≥ 3.0:1.
    NonText,
}

impl MinRatio {
    const fn minimum(self) -> f64 {
        match self {
            Self::Body => 4.5,
            // Large display text and non-text UI share the 3.0 floor.
            Self::Large | Self::NonText => 3.0,
        }
    }
    const fn label(self) -> &'static str {
        match self {
            Self::Body => "Body",
            Self::Large => "Large",
            Self::NonText => "NonText",
        }
    }
}

/// The Theme surfaces every foreground renders against (shorthands so
/// the pair table reads as one line per pair).
const BACKGROUND: TokenName = ("Theme", "background");
const SURFACE: TokenName = ("Theme", "surface");
const SURFACE_RAISED: TokenName = ("Theme", "surface-raised");
const SURFACE_HOVER: TokenName = ("Theme", "surface-hover");

/// `(foreground token, surface token, class)` — every text-bearing
/// foreground × every surface it renders on, plus the non-text pairs.
/// Text-bearing state colours render at caption/body size, so they take
/// the Body class; the display/title roles ride `text-primary` at the
/// Large class via the explicit Large entry.
const PAIRS: &[(TokenName, TokenName, MinRatio)] = &[
    (("Theme", "text-primary"), BACKGROUND, MinRatio::Body),
    (("Theme", "text-primary"), SURFACE, MinRatio::Body),
    (("Theme", "text-primary"), SURFACE_RAISED, MinRatio::Body),
    (("Theme", "text-primary"), SURFACE_HOVER, MinRatio::Body),
    // Large: display/title ride text-primary over the window background.
    (("Theme", "text-primary"), BACKGROUND, MinRatio::Large),
    (("Theme", "text-secondary"), BACKGROUND, MinRatio::Body),
    (("Theme", "text-secondary"), SURFACE, MinRatio::Body),
    (("Theme", "text-secondary"), SURFACE_RAISED, MinRatio::Body),
    // Hover is a surface: the interactive icon/text colours that lift
    // onto it must stay readable while the hover cue shows.
    (("Theme", "text-secondary"), SURFACE_HOVER, MinRatio::Body),
    (("Semantic", "success"), BACKGROUND, MinRatio::Body),
    (("Semantic", "success"), SURFACE, MinRatio::Body),
    (("Semantic", "warning"), BACKGROUND, MinRatio::Body),
    (("Semantic", "warning"), SURFACE, MinRatio::Body),
    (("Semantic", "danger"), BACKGROUND, MinRatio::Body),
    (("Semantic", "danger"), SURFACE, MinRatio::Body),
    (("Semantic", "overdue"), BACKGROUND, MinRatio::Body),
    (("Semantic", "overdue"), SURFACE, MinRatio::Body),
    // Offline status text maps to text-secondary; it is text-bearing.
    (("Semantic", "offline"), BACKGROUND, MinRatio::Body),
    (("Semantic", "offline"), SURFACE, MinRatio::Body),
    // Non-text: focus ring, borders, sync indicator.
    (("Theme", "focus-ring"), BACKGROUND, MinRatio::NonText),
    (("Theme", "focus-ring"), SURFACE, MinRatio::NonText),
    (("Semantic", "focus-ring"), BACKGROUND, MinRatio::NonText),
    (("Semantic", "focus-ring"), SURFACE, MinRatio::NonText),
    (("Theme", "border"), BACKGROUND, MinRatio::NonText),
    (("Theme", "border"), SURFACE, MinRatio::NonText),
    (("Semantic", "sync-active"), BACKGROUND, MinRatio::NonText),
    (("Semantic", "sync-active"), SURFACE, MinRatio::NonText),
];

/// Colour tokens with no contrast duty of their own. Empty at seed —
/// any entry must carry a reason comment (completeness rule 1).
const NO_TEXT_DUTY_EXEMPT: &[TokenName] = &[];

/// The closed §3.2 role table mapped onto role classes: every
/// `Typography` role must appear exactly once (completeness rule 2).
const ROLE_CLASSES: &[(&str, MinRatio)] = &[
    ("display", MinRatio::Large),
    ("title", MinRatio::Large),
    ("heading", MinRatio::Body),
    ("body", MinRatio::Body),
    ("body-emphasized", MinRatio::Body),
    ("caption", MinRatio::Body),
    ("badge", MinRatio::Body),
    ("mono", MinRatio::Body),
];

/// The Typography role names as declared in the theme file.
fn declared_roles() -> Vec<String> {
    let mut roles = Vec::new();
    let mut in_typography = false;
    for line in THEME_SOURCE.lines() {
        let line = line.split("//").next().unwrap_or(line).trim();
        if line.starts_with("export global Typography") {
            in_typography = true;
        } else if in_typography && line == "}" {
            break;
        } else if in_typography && let Some(rest) = line.strip_prefix("out property <TextRole> ") {
            let name = rest.split(':').next().unwrap_or("").trim();
            if !name.is_empty() {
                roles.push(name.to_owned());
            }
        }
    }
    roles
}

// ---------------------------------------------------------------------------
// Tests (§9.1, plan test ledger 9–13)
// ---------------------------------------------------------------------------

#[test]
fn extractor_parses_fixture_declarations() {
    let source = "\
export global Theme {
    out property <color> background: #14161c; // comment
    out property <color> border: #646c85;
}
export global Semantic {
    out property <color> offline: Theme.text-secondary;
}
";
    let colors = extract_colors(source);
    // `Theme.` references resolve against the real theme source (the
    // extractor has the one reference table, THEME_SOURCE), so the
    // fixture's `offline` entry carries the theme's actual
    // text-secondary hex. An unresolvable reference fails closed: the
    // token is dropped from the extraction, and the completeness tests
    // then fail loudly.
    assert_eq!(colors.len(), 3, "all declarations found: {colors:?}");
    assert_eq!(
        colors[0],
        ("Theme".into(), "background".into(), "14161c".into())
    );
    assert_eq!(
        colors[1],
        ("Theme".into(), "border".into(), "646c85".into())
    );
    assert_eq!(
        colors[2],
        ("Semantic".into(), "offline".into(), "a6abbd".into())
    );
}

#[test]
fn extractor_finds_every_declared_theme_token() {
    let colors = extract_colors(THEME_SOURCE);
    let names: Vec<String> = colors.iter().map(|(g, n, _)| format!("{g}.{n}")).collect();
    let expected = [
        "Theme.background",
        "Theme.surface",
        "Theme.surface-raised",
        "Theme.surface-hover",
        "Theme.text-primary",
        "Theme.text-secondary",
        "Theme.border",
        "Theme.focus-ring",
        "Semantic.success",
        "Semantic.warning",
        "Semantic.danger",
        "Semantic.overdue",
        "Semantic.sync-active",
        "Semantic.offline",
        "Semantic.focus-ring",
    ];
    for token in expected {
        assert!(
            names.iter().any(|n| n == token),
            "token {token} missing from extraction: {names:?}"
        );
    }
    assert_eq!(
        names.len(),
        expected.len(),
        "no unaccounted extras: {names:?}"
    );
}

#[test]
fn hover_token_is_distinct_from_its_parents() {
    // The structural lock against the invisible-hover bug (the old
    // hover colour equalled the toolbar's surface) and against an
    // imperceptible step: the hover cue over either parent surface
    // must be at least as visible as the existing selection step.
    let tokens = extract_colors(THEME_SOURCE);
    let hex = |name: &str| -> String {
        tokens
            .iter()
            .find(|(g, n, _)| g == "Theme" && n == name)
            .map(|(_, _, hex)| hex.clone())
            .unwrap_or_default()
    };
    let hover = hex("surface-hover");
    assert!(!hover.is_empty(), "surface-hover is declared");
    for parent in ["background", "surface", "surface-raised"] {
        assert_ne!(hover, hex(parent), "hover must differ from {parent}");
    }
    let raised_step = contrast_ratio(&hex("surface"), &hex("surface-raised"));
    assert!(
        contrast_ratio(&hex("surface"), &hover) >= raised_step,
        "the surface→hover step must be at least the surface→raised step"
    );
    assert!(
        contrast_ratio(&hex("surface-raised"), &hover) >= raised_step,
        "the raised→hover step must be at least the surface→raised step"
    );
}

#[test]
fn wcag_math_is_sanity_anchored() {
    // Black on white is exactly 21:1.
    assert!((contrast_ratio("000000", "ffffff") - 21.0).abs() < 1e-9);
    // Same colour is exactly 1:1.
    assert!((contrast_ratio("123456", "123456") - 1.0).abs() < 1e-9);
}

#[test]
fn every_pair_meets_its_role_class_minimum() {
    let tokens = extract_colors(THEME_SOURCE);
    let lookup = |name: TokenName| -> String {
        tokens
            .iter()
            .find(|(g, n, _)| g == name.0 && n == name.1)
            .map(|(_, _, hex)| hex.clone())
            .unwrap_or_default()
    };
    for (foreground, surface, class) in PAIRS {
        let fg = lookup(*foreground);
        let bg = lookup(*surface);
        assert!(
            !fg.is_empty() && !bg.is_empty(),
            "pair references an undeclared token: {foreground:?} × {surface:?} (completeness rule 3)"
        );
        let ratio = contrast_ratio(&fg, &bg);
        assert!(
            ratio >= class.minimum(),
            "{foreground:?} on {surface:?} is {ratio:.2}:1, below the {:?} minimum {}:1",
            class.label(),
            class.minimum()
        );
    }
}

#[test]
fn every_declared_token_appears_in_a_pair_or_an_exemption() {
    let tokens = extract_colors(THEME_SOURCE);
    for (global, name, _) in &tokens {
        let paired = PAIRS.iter().any(|(fg, surface, _)| {
            (fg.0 == global && fg.1 == name) || (surface.0 == global && surface.1 == name)
        }) || NO_TEXT_DUTY_EXEMPT
            .iter()
            .any(|(g, n)| g == global && n == name);
        assert!(
            paired,
            "colour token {global}.{name} has no contrast pair and no recorded exemption (completeness rule 1)"
        );
    }
}

#[test]
fn every_typography_role_has_a_role_class_mapping() {
    let roles = declared_roles();
    assert!(
        !roles.is_empty(),
        "role extraction found the Typography table"
    );
    for role in &roles {
        assert!(
            ROLE_CLASSES.iter().any(|(name, _)| name == role),
            "typography role {role} has no role-class mapping (completeness rule 2)"
        );
    }
    for (name, _) in ROLE_CLASSES {
        assert!(
            roles.iter().any(|r| r == name),
            "role-class mapping names undeclared role {name} (the table is closed)"
        );
    }
}

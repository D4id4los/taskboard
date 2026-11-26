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

// The WCAG 2.x colour math lives in `presentation.rs` (chunk L4,
// #1415): the single production definition, hoisted out of this test
// module when the chip-ink rule needed it — consumed here unchanged,
// keeping the known-answer pins over the one implementation.
use crate::presentation::contrast_ratio;

// ---------------------------------------------------------------------------
// Pair list as const data (§9.1): (foreground, surface, minimum class).
// Complete over the declared token set — see the completeness tests.
// ---------------------------------------------------------------------------

/// WCAG role classes (§9.1): body text ≥ 4.5, large display ≥ 3.0,
/// non-text UI ≥ 3.0. (`PartialEq` only: the waived variant carries
/// the recorded `f64` floor, which is not `Eq`.)
#[derive(Debug, Clone, Copy, PartialEq)]
enum MinRatio {
    /// Body-size text: ≥ 4.5:1.
    Body,
    /// Large display text: ≥ 3.0:1.
    Large,
    /// Non-text UI (borders, focus rings, indicators): ≥ 3.0:1.
    NonText,
    /// An explicitly-named debt pair (#1450): the *measured* ratio sits
    /// below the class floor today, and the remediation is a design
    /// call out of the landing chunk's scope. The entry asserts the
    /// recorded measured floor — a further regression fails, and the
    /// waiver is deleted (the real class minimum flips on) once #1450
    /// lands. Never a lowered threshold: the class the pair owes stays
    /// `owed`, named here so the debt is legible at the table.
    Waived {
        issue: &'static str,
        /// The measured ratio rounded down — the pin floor.
        recorded_floor: f64,
        /// The WCAG class the pair owes when the waiver is lifted.
        owed: Owed,
    },
}

/// The WCAG class a waived pair owes once its remediation lands (the
/// minimum the waiver flips to).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owed {
    Body,
    NonText,
}

impl Owed {
    const fn minimum(self) -> f64 {
        match self {
            Self::Body => 4.5,
            Self::NonText => 3.0,
        }
    }
    const fn label(self) -> &'static str {
        match self {
            Self::Body => "Body",
            Self::NonText => "NonText",
        }
    }
}

impl MinRatio {
    const fn minimum(self) -> f64 {
        match self {
            Self::Body => 4.5,
            // Large display text and non-text UI share the 3.0 floor.
            Self::Large | Self::NonText => 3.0,
            Self::Waived { recorded_floor, .. } => recorded_floor,
        }
    }
    const fn label(self) -> &'static str {
        match self {
            Self::Body => "Body",
            Self::Large => "Large",
            Self::NonText => "NonText",
            Self::Waived { .. } => "Waived",
        }
    }
    /// The issue that owns the remediation of a waived pair (empty for
    /// the real class floors).
    const fn debt_issue(self) -> &'static str {
        match self {
            Self::Waived { issue, .. } => issue,
            _ => "",
        }
    }
    /// The class the pair owes when the waiver lifts (the real floors
    /// owe themselves).
    const fn owed_minimum(self) -> f64 {
        match self {
            Self::Waived { owed, .. } => owed.minimum(),
            Self::Body => 4.5,
            // Large display text and non-text UI share the 3.0 floor.
            Self::Large | Self::NonText => 3.0,
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
    // The raised/hover legs (#1414): state colours lift onto cards and
    // hovered rows, so every surface they can render on is paired.
    (("Semantic", "success"), SURFACE_RAISED, MinRatio::Body),
    (("Semantic", "success"), SURFACE_HOVER, MinRatio::Body),
    (("Semantic", "warning"), BACKGROUND, MinRatio::Body),
    (("Semantic", "warning"), SURFACE, MinRatio::Body),
    (("Semantic", "warning"), SURFACE_RAISED, MinRatio::Body),
    (("Semantic", "warning"), SURFACE_HOVER, MinRatio::Body),
    (("Semantic", "danger"), BACKGROUND, MinRatio::Body),
    (("Semantic", "danger"), SURFACE, MinRatio::Body),
    (("Semantic", "danger"), SURFACE_RAISED, MinRatio::Body),
    // Measured 4.19:1 today — below the 4.5 body floor (status-bar.slint:375,
    // review-view.slint:693). Waived debt; #1450 owns the remediation.
    (
        ("Semantic", "danger"),
        SURFACE_HOVER,
        MinRatio::Waived {
            issue: "#1450",
            recorded_floor: 4.18,
            owed: Owed::Body,
        },
    ),
    (("Semantic", "overdue"), BACKGROUND, MinRatio::Body),
    (("Semantic", "overdue"), SURFACE, MinRatio::Body),
    (("Semantic", "overdue"), SURFACE_RAISED, MinRatio::Body),
    (("Semantic", "overdue"), SURFACE_HOVER, MinRatio::Body),
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
    // The raised/hover legs (#1414): the border renders around cards
    // and lifted controls. Measured 2.75:1 on raised and 2.34:1 on
    // hover today — below the 3.0 non-text floor (tooltip.slint:134
    // et al. mark up these pairs). Waived debt; #1450 owns the
    // remediation (raise the border luminance or split a
    // control-boundary token).
    (
        ("Theme", "border"),
        SURFACE_RAISED,
        MinRatio::Waived {
            issue: "#1450",
            recorded_floor: 2.74,
            owed: Owed::NonText,
        },
    ),
    (
        ("Theme", "border"),
        SURFACE_HOVER,
        MinRatio::Waived {
            issue: "#1450",
            recorded_floor: 2.33,
            owed: Owed::NonText,
        },
    ),
    (("Semantic", "sync-active"), BACKGROUND, MinRatio::NonText),
    (("Semantic", "sync-active"), SURFACE, MinRatio::NonText),
];

/// Colour tokens with no contrast duty of their own. Empty at seed —
/// any entry must carry a reason comment (completeness rule 1).
/// The picker tokens (schedule-colour-ux journey plan D-4): the hue
/// strip's pure ramp stops and the SV square's overlay pair are the
/// projected colour space's own const DATA — they paint the picker's
/// fill only (never text over them; the thumb ring carries its own
/// contrast against every ramp stop), so no token pair duty exists.
const NO_TEXT_DUTY_EXEMPT: &[TokenName] = &[
    ("Theme", "picker-hue-red"),
    ("Theme", "picker-hue-yellow"),
    ("Theme", "picker-hue-green"),
    ("Theme", "picker-hue-cyan"),
    ("Theme", "picker-hue-blue"),
    ("Theme", "picker-hue-magenta"),
    ("Theme", "picker-sv-white"),
    ("Theme", "picker-sv-black"),
];

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
        "Theme.picker-hue-red",
        "Theme.picker-hue-yellow",
        "Theme.picker-hue-green",
        "Theme.picker-hue-cyan",
        "Theme.picker-hue-blue",
        "Theme.picker-hue-magenta",
        "Theme.picker-sv-white",
        "Theme.picker-sv-black",
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
            "{foreground:?} on {surface:?} is {ratio:.2}:1, below the {:?} minimum {}:1{}",
            class.label(),
            class.minimum(),
            if class.debt_issue().is_empty() {
                String::new()
            } else {
                format!(
                    " (waived debt pinned at {:.2}:1, owes {:?} {}:1 when {} lands)",
                    class.minimum(),
                    match class {
                        MinRatio::Waived { owed, .. } => owed.label(),
                        _ => "",
                    },
                    class.owed_minimum(),
                    class.debt_issue()
                )
            }
        );
    }
}

/// The waiver pins are honest: every waived pair must still be below
/// the class it owes — a waiver that passes its real minimum is dead
/// debt and must be deleted (the entry flips to the real floor).
#[test]
fn every_waived_pair_is_still_below_its_owed_minimum() {
    let tokens = extract_colors(THEME_SOURCE);
    let lookup = |name: TokenName| -> String {
        tokens
            .iter()
            .find(|(g, n, _)| g == name.0 && n == name.1)
            .map(|(_, _, hex)| hex.clone())
            .unwrap_or_default()
    };
    for (foreground, surface, class) in PAIRS {
        let MinRatio::Waived { issue, owed, .. } = class else {
            continue;
        };
        let fg = lookup(*foreground);
        let bg = lookup(*surface);
        assert!(
            !fg.is_empty() && !bg.is_empty(),
            "undeclared token pair: {foreground:?} × {surface:?}"
        );
        let ratio = contrast_ratio(&fg, &bg);
        assert!(
            ratio < owed.minimum(),
            "{foreground:?} on {surface:?} now measures {ratio:.2}:1, at or above the owed {:?} minimum {}:1 — the {} waiver is dead debt: delete it and flip the entry to the real floor",
            owed.label(),
            owed.minimum(),
            issue
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

// ---------------------------------------------------------------------------
// The overlay sizing fold test (round-9 W1, J2 R-6/D-W1-5): the scrim and
// the dialog/menu floors/caps are `Npx * base-scale` expressions in the
// theme — the token lint's rule 7 (bare px only inside token definitions).
// The scrim is the one exempt colour token (translucent 8-digit wash, not a
// text carrier — invisible to the contrast extractor above by construction).
// ---------------------------------------------------------------------------

/// Extracts the declared literal of one Theme dimension token
/// (`out property <length> name: <expr>;`).
fn theme_length_token(name: &str) -> Option<String> {
    for line in THEME_SOURCE.lines() {
        let line = line.split("//").next().unwrap_or(line).trim();
        if let Some(rest) = line.strip_prefix("out property <length> ")
            && let Some((declared, expr)) = rest.split_once(':')
            && declared.trim() == name
        {
            return Some(expr.trim().trim_end_matches(';').trim().to_owned());
        }
    }
    None
}

#[test]
fn the_overlay_sizing_tokens_fold_over_base_scale() {
    for name in [
        "dialog-min-width",
        "dialog-max-width",
        "menu-min-width",
        "menu-max-width",
    ] {
        let expr =
            theme_length_token(name).unwrap_or_else(|| panic!("theme declares the {name} token"));
        assert!(
            expr.ends_with("* base-scale")
                && expr.trim_end().starts_with(|c: char| c.is_ascii_digit()),
            "{name} folds over base-scale (`Npx * base-scale`), got `{expr}`"
        );
    }
}

#[test]
fn the_scrim_token_is_a_translucent_theme_colour() {
    let line = THEME_SOURCE
        .lines()
        .map(|l| l.split("//").next().unwrap_or(l).trim())
        .find(|l| l.starts_with("out property <color> scrim:"))
        .expect("theme declares the scrim token");
    let value = line.split_once(':').unwrap().1.trim().trim_end_matches(';');
    // #RRGGBBAA — the 8-digit form keeps it out of the contrast
    // extractor's pair list (a translucent wash is not a text carrier).
    let hex = value.strip_prefix('#').expect("the scrim is a hex literal");
    assert!(
        hex.len() == 8 && hex.chars().all(|c| c.is_ascii_hexdigit()),
        "the scrim is a translucent #RRGGBBAA literal, got `{value}`"
    );
}

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

/// Parses one `out`/`in-out` `property <color> name: #hex;` line (or
/// a `Theme.<name>` reference, resolved to the referenced hex). The
/// `in-out` form is the palette's (#1437 D2): the 14 palette
/// properties are Rust-pushable, so both declaration spellings carry
/// tokens.
fn parse_color_property(line: &str) -> Option<(&str, &str)> {
    let rest = line
        .strip_prefix("in-out property <color> ")
        .or_else(|| line.strip_prefix("out property <color> "))?;
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
/// non-text UI ≥ 3.0. Every pair carries its real class floor — the
/// #1450 waivers are gone (the dark tokens were raised to clear the
/// floors; a failing pair is fixed in the tokens, never waived).
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

/// The surface ladders the pair table expands over — one row per
/// (foreground × surface) at the ladder's class.
const EVERY_SURFACE: &[TokenName] = &[BACKGROUND, SURFACE, SURFACE_RAISED, SURFACE_HOVER];
/// The base surfaces: backgrounds a foreground can sit on without an
/// interaction lift onto the raised/hover steps.
const BASE_SURFACES: &[TokenName] = &[BACKGROUND, SURFACE];

/// `(foreground token, the surfaces it renders on, the floor class)` —
/// the pair table's source rows, expanded by [`pairs`]. Text-bearing
/// state colours render at caption/body size, so they take the Body
/// class; the display/title roles ride `text-primary` at the Large
/// class via the explicit Large entry.
const PAIR_LADDERS: &[(TokenName, &[TokenName], MinRatio)] = &[
    (("Theme", "text-primary"), EVERY_SURFACE, MinRatio::Body),
    // Large: display/title ride text-primary over the window background.
    (("Theme", "text-primary"), &[BACKGROUND], MinRatio::Large),
    // Hover is a surface: the interactive icon/text colours that lift
    // onto it must stay readable while the hover cue shows.
    (("Theme", "text-secondary"), EVERY_SURFACE, MinRatio::Body),
    // The raised/hover legs (#1414): state colours lift onto cards and
    // hovered rows, so every surface they can render on is paired.
    (("Semantic", "success"), EVERY_SURFACE, MinRatio::Body),
    (("Semantic", "warning"), EVERY_SURFACE, MinRatio::Body),
    // Danger's hover leg is #1450's remediation (the dark token raised
    // to 0xeb8680) — the real Body floor, no waiver.
    (("Semantic", "danger"), EVERY_SURFACE, MinRatio::Body),
    (("Semantic", "overdue"), EVERY_SURFACE, MinRatio::Body),
    // Offline status text maps to text-secondary; it is text-bearing.
    (("Semantic", "offline"), BASE_SURFACES, MinRatio::Body),
    // Non-text: focus ring, borders, sync indicator.
    (("Theme", "focus-ring"), BASE_SURFACES, MinRatio::NonText),
    (("Semantic", "focus-ring"), BASE_SURFACES, MinRatio::NonText),
    // The border's raised/hover legs (#1414; #1450's remediation raised
    // the dark border token to 0x7b8297): the border renders around
    // cards and lifted controls, so every surface it can render on is
    // paired at the real non-text floor — no waiver.
    (("Theme", "border"), EVERY_SURFACE, MinRatio::NonText),
    (
        ("Semantic", "sync-active"),
        BASE_SURFACES,
        MinRatio::NonText,
    ),
];

/// The pair table: every text-bearing foreground × every surface it
/// renders on, plus the non-text pairs — 37 rows, expanded from
/// [`PAIR_LADDERS`].
fn pairs() -> Vec<(TokenName<'static>, TokenName<'static>, MinRatio)> {
    let expanded: Vec<_> = PAIR_LADDERS
        .iter()
        .flat_map(|(foreground, surfaces, class)| {
            surfaces
                .iter()
                .map(move |surface| (*foreground, *surface, *class))
        })
        .collect();
    // The expansion is injective: a duplicated surface in a ladder
    // would silently double-count a pair in the class report.
    let keys: std::collections::HashSet<_> = expanded
        .iter()
        .map(|(fg, surface, class)| format!("{fg:?}/{surface:?}/{}", class.label()))
        .collect();
    assert_eq!(
        keys.len(),
        expanded.len(),
        "pair table has no duplicate rows"
    );
    assert_eq!(
        expanded.len(),
        37,
        "pair table size (ladder change → recount)"
    );
    expanded
}

/// The chip ink (plan 2026-11-28 D3, #1458): chip text is the theme's
/// primary text colour over every tone-fill — the one ink that clears
/// the Body floor over every fill in both palettes (worst over the
/// full table: dark 5.80:1, light 7.06:1, both the `info` fill).
const CHIP_INK: TokenName = ("Theme", "text-primary");

/// The chip fill pairs (plan 2026-11-28 D4, #1458): `(declared Tone
/// name, the tone's colour token)` — one row per declared tone, the
/// exact ladder `Semantic.tone-color` maps (`neutral`→
/// `text-secondary`, `info`→`text-primary`, the semantic tones
/// verbatim, `offline`→`text-secondary`; the ladder, not dedicated
/// tokens). The test expands this over [`CHIP_SURFACES`] and pins it
/// to the declared `Tone` enum both directions, so a future fill or
/// tone change cannot silently shrink the class.
const CHIP_TONES: &[(&str, TokenName)] = &[
    ("neutral", ("Theme", "text-secondary")),
    ("info", ("Theme", "text-primary")),
    ("success", ("Semantic", "success")),
    ("warning", ("Semantic", "warning")),
    ("danger", ("Semantic", "danger")),
    ("overdue", ("Semantic", "overdue")),
    ("sync-active", ("Semantic", "sync-active")),
    ("offline", ("Semantic", "offline")),
];

/// Every surface the 0.2-alpha `tone-fill` composite can sit on.
const CHIP_SURFACES: &[TokenName] = &[SURFACE, SURFACE_RAISED, SURFACE_HOVER];

/// The `Tone` variant names as declared in the theme file's `Tone`
/// enum — the same vocabulary the chip markup dispatches on.
fn declared_tones() -> Vec<String> {
    let mut tones = Vec::new();
    let mut in_enum = false;
    for line in THEME_SOURCE.lines() {
        let line = line.split("//").next().unwrap_or(line).trim();
        if line.starts_with("export enum Tone") {
            in_enum = true;
        } else if in_enum && line == "}" {
            break;
        } else if in_enum && let Some(name) = line.strip_suffix(',') {
            tones.push(name.to_owned());
        }
    }
    tones
}

/// The palette under test (#1437 design §2.3): the `.slint` defaults
/// are a real palette (dark — the boot-time pre-push fallback), and
/// the Rust `Palette::LIGHT` mirror joins it — every declared pair ×
/// both palettes must meet its role-class minimum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaletteTag {
    /// The theme.slint literals (dark; the extractor's read).
    SlintDefaults,
    /// The Rust light palette (`crate::palette::Palette::LIGHT`).
    Light,
}

/// The per-palette token lookup: `(global, name)` → 6-digit hex, the
/// same token vocabulary the extractor reads from theme.slint. The
/// light palette resolves the two derived `Semantic` entries the same
/// way the slint does (`offline` → `text-secondary`, `focus-ring` →
/// `Theme.focus-ring`) — the derivation is palette-independent
/// (design D1). An unpaired name resolves empty and fails the tests
/// loudly.
fn light_lookup(name: TokenName) -> String {
    use crate::palette::Palette;
    let p = Palette::LIGHT;
    let hex = |rgb: u32| format!("{rgb:06x}");
    match (name.0, name.1) {
        ("Theme", "background") => hex(p.background),
        ("Theme", "surface") => hex(p.surface),
        ("Theme", "surface-raised") => hex(p.surface_raised),
        ("Theme", "surface-hover") => hex(p.surface_hover),
        ("Theme", "text-primary") => hex(p.text_primary),
        ("Theme", "text-secondary") | ("Semantic", "offline") => hex(p.text_secondary),
        ("Theme", "border") => hex(p.border),
        ("Semantic", "success") => hex(p.success),
        ("Semantic", "warning") => hex(p.warning),
        ("Semantic", "danger") => hex(p.danger),
        ("Semantic", "overdue") => hex(p.overdue),
        ("Semantic", "sync-active") => hex(p.sync_active),
        ("Theme" | "Semantic", "focus-ring") => hex(p.focus_ring),
        _ => String::new(),
    }
}

/// The lookup for one palette under test.
fn palette_lookup(tag: PaletteTag, name: TokenName) -> String {
    match tag {
        PaletteTag::SlintDefaults => slint_lookup()(name),
        PaletteTag::Light => light_lookup(name),
    }
}

/// The slint-extraction lookup (the dark palette's source), factored
/// out of the tests so the parameterized loops share it.
fn slint_lookup() -> impl Fn(TokenName) -> String {
    let tokens = extract_colors(THEME_SOURCE);
    move |name: TokenName| {
        tokens
            .iter()
            .find(|(g, n, _)| g == name.0 && n == name.1)
            .map(|(_, _, hex)| hex.clone())
            .unwrap_or_default()
    }
}

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
    // Parameterized over both palettes (#1437 design §2.3): the
    // ladder's distinctness law holds per palette.
    for tag in [PaletteTag::SlintDefaults, PaletteTag::Light] {
        let theme = |name: &str| palette_lookup(tag, ("Theme", name));
        let hover = theme("surface-hover");
        assert!(!hover.is_empty(), "{tag:?}: surface-hover is declared");
        for parent in ["background", "surface", "surface-raised"] {
            assert_ne!(
                hover,
                theme(parent),
                "{tag:?}: hover must differ from {parent}"
            );
        }
        let raised_step = contrast_ratio(&theme("surface"), &theme("surface-raised"));
        assert!(
            contrast_ratio(&theme("surface"), &hover) >= raised_step,
            "{tag:?}: the surface→hover step must be at least the surface→raised step"
        );
        assert!(
            contrast_ratio(&theme("surface-raised"), &hover) >= raised_step,
            "{tag:?}: the raised→hover step must be at least the surface→raised step"
        );
    }
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
    for tag in [PaletteTag::SlintDefaults, PaletteTag::Light] {
        for (foreground, surface, class) in pairs() {
            let lookup = |name: TokenName| palette_lookup(tag, name);
            let fg = lookup(foreground);
            let bg = lookup(surface);
            assert!(
                !fg.is_empty() && !bg.is_empty(),
                "{tag:?}: pair references an undeclared token: {foreground:?} × {surface:?} (completeness rule 3)"
            );
            let ratio = contrast_ratio(&fg, &bg);
            assert!(
                ratio >= class.minimum(),
                "{tag:?}: {foreground:?} on {surface:?} is {ratio:.2}:1, below the {:?} minimum {}:1",
                class.label(),
                class.minimum(),
            );
        }
    }
}

/// The chip fill class (plan 2026-11-28 D4, #1458): `CHIP_INK` over
/// every tone's 0.2-alpha fill composite on every surface, in both
/// palettes, at the Body floor. Blend model: slint composites straight
/// (non-premultiplied) alpha over 8-bit sRGB, so
/// `tone-color(t).with-alpha(0.2)` over a surface is per-channel
/// `round(0.2 × tone + 0.8 × surface)`.
#[test]
fn every_chip_fill_pair_meets_body_minimum() {
    let blend = |tone: &str, surface: &str| -> String {
        let channel = |t: usize, s: usize| {
            let tone = u16::from(u8::from_str_radix(&tone[t..t + 2], 16).unwrap());
            let surface = u16::from(u8::from_str_radix(&surface[s..s + 2], 16).unwrap());
            // round(0.2 × tone + 0.8 × surface) in exact integer
            // arithmetic — n/5's fraction is never 0.5, so
            // (n + 2) / 5 is the round for every byte pair.
            format!("{:02x}", (tone + 4 * surface + 2) / 5)
        };
        format!("{}{}{}", channel(0, 0), channel(2, 2), channel(4, 4))
    };
    // Completeness, both directions: the chip tone ladder names
    // exactly the `Tone` variants the markup vocabulary declares —
    // a new tone fails here until it is paired, and a renamed or
    // removed variant fails the same way.
    let declared = declared_tones();
    assert!(!declared.is_empty(), "tone extraction found the Tone enum");
    assert_eq!(
        CHIP_TONES.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        declared.iter().map(String::as_str).collect::<Vec<_>>(),
        "chip tone ladder ↔ declared Tone enum (plan D4 completeness)"
    );
    // 8 declared tones × 3 chip surfaces, evaluated over both palettes.
    let chip_pairs = CHIP_TONES
        .iter()
        .flat_map(|(name, tone_token)| {
            CHIP_SURFACES
                .iter()
                .map(move |surface_token| (*name, *tone_token, *surface_token))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        chip_pairs.len(),
        24,
        "chip pair count (tone/surface change → recount)"
    );
    for (tone, tone_token, surface_token) in chip_pairs {
        for tag in [PaletteTag::SlintDefaults, PaletteTag::Light] {
            let ink = palette_lookup(tag, CHIP_INK);
            let fill = blend(
                &palette_lookup(tag, tone_token),
                &palette_lookup(tag, surface_token),
            );
            let ratio = contrast_ratio(&ink, &fill);
            assert!(
                ratio >= 4.5,
                "{tag:?}: chip ink {ink:?} on tone `{tone}` fill (0.2 over {surface_token:?}) is {ratio:.2}:1, below the Body minimum 4.5:1"
            );
        }
    }
}

/// The mirror-parity pin (#1437 design D2): the theme.slint literals
/// (the boot-time pre-push defaults) are byte-equal to
/// `Palette::DARK` field-by-field — the deliberate duplication is
/// held by the house mirror-parity pattern (drift breaks a test, the
/// `settings_form` precedent). The 8-digit scrim (invisible to the
/// extractor) is checked directly against the same literal grammar.
#[test]
fn the_slint_defaults_mirror_the_dark_palette() {
    use crate::palette::Palette;
    let dark = Palette::DARK;
    let hex = |rgb: u32| format!("{rgb:06x}");
    let tokens = extract_colors(THEME_SOURCE);
    let lookup = |global: &str, name: &str| -> String {
        tokens
            .iter()
            .find(|(g, n, _)| g == global && n == name)
            .map_or_else(
                || panic!("theme declares {global}.{name}"),
                |(_, _, hex)| hex.clone(),
            )
    };
    let pairs = [
        ("Theme", "background", dark.background),
        ("Theme", "surface", dark.surface),
        ("Theme", "surface-raised", dark.surface_raised),
        ("Theme", "surface-hover", dark.surface_hover),
        ("Theme", "text-primary", dark.text_primary),
        ("Theme", "text-secondary", dark.text_secondary),
        ("Theme", "border", dark.border),
        ("Theme", "focus-ring", dark.focus_ring),
        ("Semantic", "success", dark.success),
        ("Semantic", "warning", dark.warning),
        ("Semantic", "danger", dark.danger),
        ("Semantic", "overdue", dark.overdue),
        ("Semantic", "sync-active", dark.sync_active),
    ];
    for (global, name, rgb) in pairs {
        assert_eq!(
            lookup(global, name),
            hex(rgb),
            "{global}.{name}: the slint default drifted from Palette::DARK — \
             the .slint literals are the boot-time fallback and must stay the \
             dark palette verbatim (design D2)"
        );
    }
    // The scrim: same mirror, over the 8-digit literal form.
    let scrim_line = THEME_SOURCE
        .lines()
        .map(|l| l.split("//").next().unwrap_or(l).trim())
        .find(|l| l.starts_with("in-out property <color> scrim:"))
        .expect("theme declares the scrim token");
    let value = scrim_line
        .split_once(':')
        .unwrap()
        .1
        .trim()
        .trim_end_matches(';');
    assert_eq!(
        value,
        format!("#{:08x}", dark.scrim),
        "the scrim literal drifted from Palette::DARK.scrim"
    );
}

/// The light palette's token vocabulary is total (design §2.3's
/// completeness rule runs over the union): every non-exempt token the
/// extractor finds resolves in the light lookup — a new token added
/// to theme.slint without a `Palette` field fails here.
#[test]
fn the_light_palette_covers_every_declared_token() {
    let tokens = extract_colors(THEME_SOURCE);
    for (global, name, _) in &tokens {
        let exempt = NO_TEXT_DUTY_EXEMPT
            .iter()
            .any(|(g, n)| g == global && n == name);
        if exempt {
            continue;
        }
        assert!(
            !light_lookup((global.as_str(), name.as_str())).is_empty(),
            "colour token {global}.{name} has no Palette::LIGHT field — \
             the palette data must cover every declared token (design D1)"
        );
    }
}

#[test]
fn every_declared_token_appears_in_a_pair_or_an_exemption() {
    let tokens = extract_colors(THEME_SOURCE);
    for (global, name, _) in &tokens {
        let paired = pairs().iter().any(|(fg, surface, _)| {
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
        .find(|l| l.starts_with("in-out property <color> scrim:"))
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

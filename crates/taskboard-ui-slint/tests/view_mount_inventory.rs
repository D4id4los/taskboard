// SPDX-License-Identifier: AGPL-3.0-only

//! The view-mount callback inventory (round-5 D8): the Slint-side
//! complement of `tests/seam_inventory.rs`, which covers only the
//! Rust install side. Source-scan, not compiler introspection — the
//! failure mode is a missing line of markup (F1's unforwarded
//! callback), which a source assertion pins exactly.
//!
//! Covered boundary: every `export component` in `ui/views/` and
//! `ui/compositions/` (the crate's mounted surface), instantiated
//! anywhere in `ui/app-window.slint`, `ui/views/`, or
//! `ui/compositions/`. Widgets (opt-in callback protocols, in-file
//! mounts) and private sub-components (inline closures, not
//! forwarding seams) are out of boundary — the chunk plan §2.1
//! records the triage.
//!
//! Two-sided assertion, `seam_inventory`'s honesty rule:
//! 1. totality — every covered site forwards every callback its
//!    component declares, or a predicate-witnessed exemption covers
//!    the pair at that site;
//! 2. no stale exemptions — each exemption names a live (component,
//!    callback) pair and a predicate that still witnesses a site.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::PathBuf;

/// A source blanked for structural scanning: line comments and every
/// character inside string literals (including `\{…}` interpolations
/// and escaped quotes) become spaces, so `//`-in-string and braces in
/// labels cannot desync the scanner. `depth` is byte-indexed (one
/// entry per byte of `text`, multi-byte characters included):
/// `depth[i]` is the number of open braces strictly before byte
/// offset `i`.
struct Blanked {
    text: String,
    depth: Vec<usize>,
}

impl Blanked {
    fn blank(src: &str) -> Self {
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
    fn body(&self, open: usize) -> Option<Range<usize>> {
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
    fn tokens(&self, range: Range<usize>) -> Vec<(usize, &str)> {
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
struct ComponentDecl {
    name: String,
    name_start: usize,
    body_open: usize,
    body_end: usize,
}

/// Every component declared in a blanked source (any component body
/// may host instantiation sites, exported or not).
fn declared_components(src: &Blanked) -> Vec<ComponentDecl> {
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
fn is_exported(src: &Blanked, comp: &ComponentDecl) -> bool {
    let gap_start = comp.name_start.saturating_sub("export component ".len());
    src.tokens(gap_start..comp.name_start)
        .iter()
        .any(|(_, t)| *t == "export")
}

/// Callbacks declared at body depth 0 (`callback name(args) -> ret;`,
/// whitespace/newline tolerant).
fn declared_callbacks(src: &Blanked, comp: &ComponentDecl) -> Vec<String> {
    let body = comp.body_open + 1..comp.body_end;
    let inner = src.depth[comp.body_open]; // depth of the body's braces
    let mut out = Vec::new();
    for (start, token) in src.tokens(body) {
        if token != "callback" || src.depth[start] != inner + 1 {
            continue;
        }
        let Some(semi) = (start..comp.body_end).find(|&i| src.text[i..].starts_with(';')) else {
            continue;
        };
        let Some((_, name)) = src.tokens(start + token.len()..semi).into_iter().next() else {
            continue;
        };
        out.push(name.to_owned());
    }
    out
}

/// An instantiation site of some component: the parent, the
/// instantiated name, and the site body's byte range.
struct Site {
    parent: String,
    name: String,
    head_start: usize,
    body_open: usize,
    body_end: usize,
}

/// Instantiations of registry members within one component's body:
/// a site head is `Name {`, optionally `ident := Name {` or
/// `if (cond): Name {`. Declaration heads (`component`/`inherits`)
/// and `global` blocks are not sites.
fn instantiation_sites(src: &Blanked, comp: &ComponentDecl, registry: &Registry) -> Vec<Site> {
    let body = comp.body_open + 1..comp.body_end;
    let mut out = Vec::new();
    for open in body.start..body.end {
        if !src.text[open..].starts_with('{') {
            continue;
        }
        // The name candidate: whitespace, then an identifier run
        // ending just before `{` (covered component names are
        // CamelCase — no hyphens).
        let mut end = open;
        while end > body.start
            && src.text[end - 1..end]
                .chars()
                .all(|c| c.is_ascii_whitespace())
        {
            end -= 1;
        }
        let run_end = end;
        while end > body.start
            && src.text[end - 1..end]
                .chars()
                .all(|c| c.is_ascii_alphanumeric())
        {
            end -= 1;
        }
        let start = end;
        if start == run_end {
            continue;
        }
        let name = &src.text[start..run_end];
        if !registry.contains(name) {
            continue;
        }
        // The token before the name: reject declaration heads.
        if src
            .tokens(body.start..start)
            .last()
            .is_some_and(|(_, t)| *t == "component" || *t == "inherits" || *t == "global")
        {
            continue;
        }
        let Some(site_body) = src.body(open) else {
            continue;
        };
        out.push(Site {
            parent: comp.name.clone(),
            name: name.to_owned(),
            head_start: start,
            body_open: open,
            body_end: site_body.end,
        });
    }
    out
}

/// Whether callback `cb` has a handler at site-body depth 0:
/// `cb =>`, `cb (…) =>`, or `cb <=>`. A `changed` prefix makes it a
/// property change handler, not a callback handler.
fn has_handler(src: &Blanked, site: &Site, cb: &str) -> bool {
    let body = site.body_open + 1..site.body_end;
    let inner = src.depth[site.body_open]; // depth of the body's braces
    let tokens = src.tokens(body);
    for (idx, (start, token)) in tokens.iter().enumerate() {
        if *token != cb || src.depth[*start] != inner + 1 {
            continue;
        }
        if idx > 0 && tokens[idx - 1].1 == "changed" {
            continue;
        }
        let after = &tokens[idx + 1..];
        match after.first().map(|(_, t)| *t) {
            Some("=>" | "<=>") => return true,
            Some("(") => {
                let Some(close) = after
                    .iter()
                    .position(|(q, t)| *t == ")" && src.depth[*q] == src.depth[*start])
                else {
                    continue;
                };
                if after.get(close + 1).map(|(_, t)| *t) == Some("=>")
                    || after.get(close + 1).map(|(_, t)| *t) == Some("<=>")
                {
                    return true;
                }
            }
            _ => (),
        }
    }
    false
}

/// A deliberate non-forwarding, witnessed by a site-shape predicate:
/// the predicate double-checks reachability — a rewired site loses
/// the witness, Side 2 fails, and the callback re-enters enforcement.
struct Exemption {
    component: &'static str,
    callback: &'static str,
    site_predicate: &'static str,
    reason: &'static str,
}

impl Exemption {
    /// A `DatePicker` callback that the date-only track-form mount
    /// (`show-time: false`) cannot fire or deliberately does not wire.
    const fn date_only(callback: &'static str, reason: &'static str) -> Self {
        Self {
            component: "DatePicker",
            callback,
            site_predicate: "show-time: false",
            reason,
        }
    }
}

const EXEMPTIONS: &[Exemption] = &[
    Exemption {
        component: "ActionMenu",
        callback: "invoke-submenu-entry",
        site_predicate: "is-flyout: true",
        reason: "fires only in compact replace mode; the flyout's mount condition excludes Breakpoint.compact — structurally unreachable",
    },
    Exemption {
        component: "ActionMenu",
        callback: "level-back",
        site_predicate: "is-flyout: true",
        reason: "the back row renders only under compact && active-group >= 0; same mount-condition exclusion — structurally unreachable",
    },
    Exemption::date_only(
        "quick-toggled",
        "the quick-times chevron lives in the time section under if (root.show-time); the track-form grid mount is date-only — structurally unreachable while the predicate holds",
    ),
    Exemption::date_only(
        "time-edited",
        "the time section renders under if (root.show-time); the track-form grid mount is date-only — structurally unreachable while the predicate holds",
    ),
    Exemption::date_only(
        "time-picked",
        "as time-edited: date-only mount, the time callbacks cannot fire",
    ),
    Exemption::date_only(
        "time-accepted",
        "as time-edited: date-only mount, the time callbacks cannot fire",
    ),
    Exemption::date_only(
        "grid-focus-changed",
        "reachable but deliberately unwired: the track-form's keymap context is flow-armed adapter-side; the reverse focus seam is the task-detail mount's mechanism only",
    ),
];

/// The covered registry: exported components of `ui/views/` and
/// `ui/compositions/`, with their declared callbacks.
struct Registry(BTreeMap<String, Vec<String>>);

impl Registry {
    fn contains(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }
}

/// `ui/app-window.slint` plus the views/ and compositions/ walks,
/// sorted for a deterministic scan. The walk (not a literal list)
/// keeps the file set self-maintaining: a new view or composition
/// file is covered the day it lands.
fn scanned_files() -> Vec<PathBuf> {
    let ui = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut files = vec![ui.join("app-window.slint")];
    for dir in ["views", "compositions"] {
        let mut found: Vec<_> = std::fs::read_dir(ui.join(dir))
            .expect("the ui subdirectories exist")
            .map(|e| e.expect("readable directory entry").path())
            .filter(|p| p.extension().is_some_and(|e| e == "slint"))
            .collect();
        found.sort();
        files.extend(found);
    }
    files
}

/// (path, blanked) for every scanned file.
fn load_sources() -> Vec<(PathBuf, Blanked)> {
    scanned_files()
        .into_iter()
        .map(|p| {
            let src = Blanked::blank(
                &std::fs::read_to_string(&p).expect("slint sources are readable UTF-8"),
            );
            (p, src)
        })
        .collect()
}

fn char_safe_snippet(text: &str, start: usize, budget: usize) -> String {
    text[start..]
        .chars()
        .take(budget)
        .collect::<String>()
        .replace('\n', " ")
}

#[test]
fn every_mounted_view_composition_callback_has_a_handler_at_its_mount() {
    let sources = load_sources();
    let mut registry = Registry(BTreeMap::new());
    for (path, src) in &sources {
        let under_mounted_dir = path
            .parent()
            .and_then(|d| d.file_name())
            .is_some_and(|d| d == "views" || d == "compositions");
        if !under_mounted_dir {
            continue;
        }
        for comp in declared_components(src) {
            if is_exported(src, &comp) {
                registry
                    .0
                    .insert(comp.name.clone(), declared_callbacks(src, &comp));
            }
        }
    }

    let mut all_sites = Vec::new();
    for (path, src) in &sources {
        for comp in declared_components(src) {
            for site in instantiation_sites(src, &comp, &registry) {
                all_sites.push((path.display().to_string(), src, site));
            }
        }
    }

    // Side 1 — totality.
    let mut failures = Vec::new();
    let mut covered_exemptions = std::collections::BTreeSet::new();
    for (path, src, site) in &all_sites {
        let site_body = &src.text[site.body_open + 1..site.body_end];
        for cb in &registry.0[&site.name] {
            if has_handler(src, site, cb) {
                continue;
            }
            let exemption = EXEMPTIONS.iter().find(|e| {
                e.component == site.name
                    && e.callback == cb.as_str()
                    && site_body.contains(e.site_predicate)
            });
            match exemption {
                Some(e) => {
                    covered_exemptions.insert((e.component, e.callback, e.site_predicate));
                }
                None => failures.push(format!(
                    "{}: {} mounts {} without forwarding callback {:?} (site head: {:?})",
                    path,
                    site.parent,
                    site.name,
                    cb,
                    char_safe_snippet(&src.text, site.head_start, 40),
                )),
            }
        }
    }

    // Side 2 — no stale exemptions: each names a live (component,
    // callback) pair, witnesses a real site body, and stays needed.
    for e in EXEMPTIONS {
        let Some(cbs) = registry.0.get(e.component) else {
            failures.push(format!(
                "stale exemption: component {} no longer exists",
                e.component
            ));
            continue;
        };
        if !cbs.iter().any(|c| c == e.callback) {
            failures.push(format!(
                "stale exemption: component {} no longer declares callback {:?}",
                e.component, e.callback
            ));
        }
        if !all_sites.iter().any(|(_, src, site)| {
            site.name == e.component
                && src.text[site.body_open + 1..site.body_end].contains(e.site_predicate)
        }) {
            failures.push(format!(
                "stale exemption: no {} site body witnesses the predicate {:?} ({} — re-triage the exemption)",
                e.component, e.site_predicate, e.reason,
            ));
        }
        if !covered_exemptions.contains(&(e.component, e.callback, e.site_predicate)) {
            failures.push(format!(
                "stale exemption: {}::{:?} is enforced by Side 1 without the exemption (drop it or fix the predicate)",
                e.component, e.callback
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "view-mount callback inventory violations:\n{}",
        failures.join("\n")
    );
}

fn mini_registry(entries: &[(&str, &[&str])]) -> Registry {
    Registry(
        entries
            .iter()
            .map(|(n, cbs)| {
                (
                    (*n).to_owned(),
                    cbs.iter().map(|c| (*c).to_owned()).collect(),
                )
            })
            .collect(),
    )
}

fn sole_site(src: &Blanked, parent: &str, registry: &Registry) -> Site {
    let comps = declared_components(src);
    let comp = comps
        .iter()
        .find(|c| c.name == parent)
        .expect("parent declared");
    let sites = instantiation_sites(src, comp, registry);
    assert_eq!(sites.len(), 1, "exactly one instantiation site");
    sites.into_iter().next().expect("checked above")
}

#[test]
fn a_handler_with_arguments_answers_the_forwarding_check() {
    let src = Blanked::blank(
        "component A { callback edited(string); } \
         component B { A { edited(t) => root.edited(t); } }",
    );
    let registry = mini_registry(&[("A", &["edited"])]);
    let site = sole_site(&src, "B", &registry);
    assert!(has_handler(&src, &site, "edited"));
}

#[test]
fn a_zero_arg_handler_answers_the_forwarding_check() {
    let src = Blanked::blank(
        "component A { callback clicked(); } \
         component B { A { clicked => {} } }",
    );
    let registry = mini_registry(&[("A", &["clicked"])]);
    let site = sole_site(&src, "B", &registry);
    assert!(has_handler(&src, &site, "clicked"));
}

#[test]
fn a_missing_handler_is_not_answered_by_a_declaration_or_property() {
    let src = Blanked::blank(
        "component A { callback edited(string); } \
         component B { A { property <string> edited: \"x\"; } }",
    );
    let registry = mini_registry(&[("A", &["edited"])]);
    let site = sole_site(&src, "B", &registry);
    assert!(!has_handler(&src, &site, "edited"));
}

#[test]
fn a_changed_prefix_is_not_a_callback_handler() {
    let src = Blanked::blank(
        "component A { callback edited(string); property <string> edited-text; } \
         component B { A { changed edited-text => {} } }",
    );
    let registry = mini_registry(&[("A", &["edited"])]);
    let site = sole_site(&src, "B", &registry);
    assert!(!has_handler(&src, &site, "edited"));
}

#[test]
fn a_missing_handler_is_detected_at_its_own_site_only() {
    // A nested sibling site's handler must not answer for this site.
    let src = Blanked::blank(
        "component A { callback clicked(); } \
         component B { A { HorizontalBox { A { clicked => {} } } } }",
    );
    let registry = mini_registry(&[("A", &["clicked"])]);
    let comps = declared_components(&src);
    let b = comps.iter().find(|c| c.name == "B").expect("B declared");
    let sites = instantiation_sites(&src, b, &registry);
    assert_eq!(sites.len(), 2, "both nested instantiations are sites");
    assert!(!has_handler(&src, &sites[0], "clicked"));
    assert!(has_handler(&src, &sites[1], "clicked"));
}

#[test]
fn declaration_heads_and_globals_are_not_instantiation_sites() {
    let src = Blanked::blank(
        "export component A { callback clicked(); } \
         global G { property <int> n; } \
         component B { } \
         component C inherits A { }",
    );
    let registry = mini_registry(&[("A", &["clicked"]), ("B", &[]), ("C", &[])]);
    let comps = declared_components(&src);
    assert!(
        !comps.iter().any(|c| c.name == "G"),
        "globals are not component declarations"
    );
    for name in ["B", "C"] {
        let comp = comps.iter().find(|c| c.name == name).expect("declared");
        for site in instantiation_sites(&src, comp, &registry) {
            assert_ne!(site.name, name, "the declaration head is not a site");
        }
    }
}

#[test]
fn blanking_survives_comments_and_interpolations_in_strings() {
    let src = Blanked::blank(
        "component A {\n\
         property <string> u: \"https://cloud.example.com\";\n\
         property <string> t: \"\\{root.task.title}\";\n\
         callback clicked();\n\
         // callback commented-out();\n\
         }",
    );
    let comps = declared_components(&src);
    assert_eq!(comps.len(), 1, "the string braces do not split the body");
    assert_eq!(
        declared_callbacks(&src, &comps[0]),
        vec!["clicked".to_owned()],
        "commented declarations are not declarations"
    );
    assert!(
        !src.text.contains("https://"),
        "string contents are blanked: a `//` inside a string cannot start a comment"
    );
    assert!(
        src.text.matches('"').count() == 4,
        "the quote chars survive (string extents stay addressable)"
    );
}

#[test]
fn multi_byte_characters_keep_byte_offsets_aligned() {
    let src = Blanked::blank(
        "component A {\n\
         // Kommentare: §12, Grade – 5\n\
         property <string> t: \"Auftrag § 12 – offen\";\n\
         callback clicked();\n\
         }",
    );
    assert_eq!(
        src.text.len(),
        src.depth.len(),
        "depth is byte-indexed: one entry per byte of text"
    );
    let comps = declared_components(&src);
    assert_eq!(comps.len(), 1);
    assert_eq!(
        declared_callbacks(&src, &comps[0]),
        vec!["clicked".to_owned()],
        "declarations after multi-byte comments and labels still parse"
    );
}

#[test]
fn a_multi_line_declaration_parses_to_its_name() {
    let src = Blanked::blank(
        "component A {\n    callback\n        create-accepted(string, string, bool)\n            -> bool;\n}",
    );
    let comps = declared_components(&src);
    assert_eq!(
        declared_callbacks(&src, &comps[0]),
        vec!["create-accepted".to_owned()],
    );
}

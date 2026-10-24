// SPDX-License-Identifier: AGPL-3.0-only
//! REPL tab-completion (WI-G): a rustyline [`Completer`] over the static
//! clap CLI shape and the live `ArcSwap<AppState>` snapshot.
//!
//! Layering (the WI-G acceptance core): everything below the glue is a
//! pure function of its arguments — no I/O, no channels, no runtime, no
//! rustyline types — so the whole engine unit-tests in CI without a TTY:
//!
//! 1. [`tokenize_prefix`] — cursor-aware word split of a line being
//!    typed (the same grammar as `shell_words`: whitespace, `'…'`,
//!    `"…"`, `\` escapes; a token ends at the cursor).
//! 2. [`plan`] — which [`Kind`] answers the cursor position, over a
//!    one-time clap-shape walk ([`CompletionSpec::from_cli`]).
//! 3. [`candidates`] — prefix-matched candidates from the spec (verbs,
//!    subverbs, flags, literals) and from a state snapshot (live
//!    stack/task/label titles).
//! 4. [`complete`] — the composition; the caller does the one impure
//!    thing (`load_full()`).
//!
//! Invariants the engine maintains (the plan's [CONFIRM] decisions):
//!
//! - **Parity with the selectors**: a title is offered iff the
//!   corresponding `resolve_*` could select it — stacks `!deleted`,
//!   tasks `Task::is_live` (done and archived stay selectable), labels
//!   `!deleted`; canonical ordering per resolver. A *hard* title
//!   collision (two live entities sharing the title) replaces the title
//!   row with one uuid-disambiguated row per entity — display
//!   `"{title} - {uuid}"`, insert the uuid — because selecting a
//!   colliding title manufactures a guaranteed `Ambiguous*` error; the
//!   honest menu offers the escape instead. Titles are the identifier
//!   humans parse: the uuid is the distinguisher column, never the
//!   leading text. Unambiguous titles keep exactly the plain row.
//! - **Session-refused surface is not offered**: `daemon`, `config`,
//!   `gui`
//!   and `--config` never appear as candidates — completing them would
//!   manufacture a guaranteed in-session refusal
//!   (`repl::LineRefusal`). The ceremony verbs are offered instead
//!   (ADR 0014): they are session ceremonies now, and their flags walk
//!   in from the clap spec like every other verb's. The same honesty
//!   rule removes root-local flags (`--version`) from off-root menus:
//!   clap rejects them there,
//!   so completing them would only manufacture a usage error.
//! - **Conservative routing**: an unclassifiable token (unknown verb,
//!   unknown flag mid-line, unattributable value position) yields *no*
//!   candidates — wrong candidates are worse than none.
//! - **Re-quoting**: replacements go through `shell_words::quote`
//!   (the documented inverse of the line grammar) plus one trailing
//!   space, so a completed line re-splits and re-parses.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use clap::CommandFactory as _;

use taskboard_domain::{AppState, ids};

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// What one completion offers (the rustyline-free core type).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    /// Menu text (the bare candidate, unquoted).
    pub display: String,
    /// What is inserted at `start` (quoted via `shell_words::quote`,
    /// one trailing space).
    pub replacement: String,
}

/// Which source answers a completion position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Word 0: the offered verbs (clap groups + `exit`/`quit`, refusals
    /// excluded).
    Verbs,
    /// Subverbs of a group verb (boards/stacks/tasks/labels).
    SubVerbs,
    /// Long flags valid at the position (root globals ∪ node flags,
    /// exclusions applied).
    Flags,
    /// Live stack titles (parity with `resolve_stack`).
    Stacks,
    /// Live task titles (parity with `resolve_task`).
    Tasks,
    /// Live label titles (parity with `resolve_label`).
    Labels,
    /// Live board uuids/titles (parity with `resolve_board` — live,
    /// `!deleted`, title-matched, uuid-disambiguated collisions).
    Boards,
    /// The configured connector names (boot-fixed for a session; config
    /// is not engine state).
    Connectors,
    /// Static enumerable literals for one flag (`--due` → `none`).
    Literals(&'static [&'static str]),
    /// Free text or unclassifiable: offer nothing.
    None,
}

/// One long flag of a command path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlagSpec {
    /// The long name, without `--`.
    long: String,
    /// Whether the flag consumes a following word (`--stack`: yes;
    /// `--json`: no) — needed to count positionals while routing.
    takes_value: bool,
    /// The completion kind of the flag's value (`--stack` → Stacks,
    /// `--due` → Literals, everything else → free text).
    value_kind: Kind,
    /// A root-local flag (`--version`): clap does not propagate it to
    /// subcommand paths, so it is unioned in at word 0 only — offering
    /// it mid-line would manufacture a usage error.
    root_only: bool,
}

impl FlagSpec {
    /// The offered form: the flag with its `--` prefix.
    fn offered_form(&self) -> String {
        format!("--{}", self.long)
    }
}

/// The cursor context of a completion request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Prefix {
    /// Fully-typed words before the cursor word (quotes resolved).
    pub(crate) words: Vec<String>,
    /// The word under/after the cursor, decoded so far; `""` when the
    /// cursor sits at a fresh word.
    pub(crate) stem: String,
    /// Byte offset in `line` where a replacement for the cursor word
    /// begins (a leading quote is part of what gets replaced — the
    /// candidate is re-quoted wholesale).
    pub(crate) stem_start: usize,
    /// The cursor word exactly as typed (quotes and escapes intact) —
    /// the attached-value detection (`--stack=To`) reads this; for
    /// plain flag text it is byte-identical to `stem`.
    pub(crate) raw: String,
}

// ---------------------------------------------------------------------------
// Tokenization (pure)
// ---------------------------------------------------------------------------

/// Splits `line[..pos]` into the completed [`Prefix`]: the words before
/// the cursor word, the decoded cursor stem, and the byte offset where
/// its replacement starts. Deliberately matches the `shell_words`
/// grammar (whitespace, single/double quotes, backslash escapes) but
/// tolerates the unfinished shapes a line being typed has (unbalanced
/// quotes, trailing separator, cursor mid-word).
#[must_use]
pub(crate) fn tokenize_prefix(line: &str, pos: usize) -> Prefix {
    let pos = pos.min(line.len());
    let mut words: Vec<String> = Vec::new();
    let mut token = String::new();
    let mut token_start: Option<usize> = None;
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for (i, ch) in line.char_indices() {
        if i >= pos {
            // The cursor ends the scan: whatever is pending is the
            // cursor word (or a fresh one if no token is open).
            break;
        }
        if escaped {
            token.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => {
                escaped = true;
                token_start.get_or_insert(i);
            }
            '\'' if !in_double => {
                in_single = !in_single;
                token_start.get_or_insert(i);
            }
            '"' if !in_single => {
                in_double = !in_double;
                token_start.get_or_insert(i);
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if token_start.take().is_some() {
                    words.push(std::mem::take(&mut token));
                }
            }
            c => {
                token.push(c);
                token_start.get_or_insert(i);
            }
        }
    }

    let (stem, stem_start) = match token_start {
        Some(start) => (token, start),
        // Fresh word: the replacement inserts at the cursor itself.
        None => (String::new(), pos),
    };
    let raw = line
        .get(stem_start..pos)
        .map_or_else(|| stem.clone(), std::string::ToString::to_string);
    Prefix {
        words,
        stem,
        stem_start,
        raw,
    }
}

// ---------------------------------------------------------------------------
// The clap-shape walk (pure, once per session)
// ---------------------------------------------------------------------------

/// The completion surface, walked once from [`crate::cli::Cli`] at
/// editor-thread construction (clap's tree is static; the walk is
/// cheap). Command paths are keyed by their verb words
/// (`["tasks", "add"]`).
#[derive(Debug)]
pub(crate) struct CompletionSpec {
    /// Group verbs → their subcommand names (+ aliases), sorted.
    /// Refused groups (`daemon`, `config`) are absent entirely.
    groups: BTreeMap<String, Vec<String>>,
    /// The verbs offered at word 0: the walked groups and flat verbs
    /// minus the session-refused set, plus the REPL-only `exit`/`quit`.
    offered_verbs: Vec<String>,
    /// Root global flags (`--json`, `--help`, `--version`, and
    /// `--config` — kept only for value-word counting), unioned into
    /// every path's flag set at query time. Root-local flags
    /// (`--version`: clap does not propagate it) union in at word 0
    /// only — see [`FlagSpec::root_only`].
    globals: Vec<FlagSpec>,
    /// Long flags per path.
    flags: BTreeMap<Vec<String>, Vec<FlagSpec>>,
    /// Positional kinds per path, in declaration order.
    positionals: BTreeMap<Vec<String>, Vec<Kind>>,
}

/// Verbs whose lines `repl::resolve_words` refuses in-session; nothing
/// about them is offered, and any line starting with one completes
/// nothing. The ceremony verbs (connect/login/logout, ADR 0014) are
/// offered like any other verb since chunk 3. `gui` is a lifecycle
/// arm (a process entry): completing it would manufacture a
/// guaranteed in-session refusal.
const REFUSED_VERBS: [&str; 3] = ["daemon", "config", "gui"];

/// Flags never offered as candidates: a per-line `--config` is refused
/// in-session (`LineRefusal::ConfigFlag`). The spec keeps the flag in
/// `globals` for value-word counting while routing; the menu filter
/// ([`CompletionSpec::offered_flags_at`]) drops it.
const EXCLUDED_FLAGS: [&str; 1] = ["config"];

/// Value-flag → completion kind, by long name. New flags following the
/// positional-id conventions get completion for free; only a new *kind*
/// of source needs a map entry here.
fn flag_value_kind(long: &str) -> Kind {
    match long {
        "stack" => Kind::Stacks,
        "due" | "scheduled" => Kind::Literals(&["none"]),
        "importance" | "urgency" => {
            Kind::Literals(&["very-low", "low", "normal", "high", "very-high", "none"])
        }
        "board" | "local-board" => Kind::Boards,
        "connector" => Kind::Connectors,
        "type" => Kind::Literals(&["task", "stack", "board"]),
        _ => Kind::None,
    }
}

/// Positional id → completion kind. `task`/`stack`/`label` are the
/// selector positions (`resolve_task`/`resolve_stack`/`resolve_label`
/// parity), `board` is `boards use`'s positional (the `--board` flag's
/// kind); `remote_board` (`boards select`'s positional — a remote u64,
/// server state) completes nothing locally, `boards list` is its
/// source.
fn positional_kind(id: &str) -> Kind {
    match id {
        "task" => Kind::Tasks,
        "stack" => Kind::Stacks,
        "label" => Kind::Labels,
        "board" => Kind::Boards,
        _ => Kind::None,
    }
}

fn walk_flags(cmd: &clap::builder::Command, at_root: bool) -> Vec<FlagSpec> {
    cmd.get_arguments()
        .filter(|arg| !arg.is_hide_set())
        .filter_map(|arg| {
            arg.get_long().map(|long| FlagSpec {
                long: long.to_owned(),
                takes_value: arg.get_num_args().is_some_and(|range| range.takes_values()),
                value_kind: flag_value_kind(long),
                root_only: at_root && !arg.is_global_set(),
            })
        })
        .collect()
}

fn walk_positionals(cmd: &clap::builder::Command) -> Vec<Kind> {
    cmd.get_arguments()
        .filter(|arg| !arg.is_hide_set() && arg.is_positional())
        .map(|arg| positional_kind(arg.get_id().as_str()))
        .collect()
}

fn subcommand_names(cmd: &clap::builder::Command) -> Vec<String> {
    let mut names: Vec<String> = cmd
        .get_subcommands()
        .flat_map(|sub| {
            std::iter::once(sub.get_name().to_owned())
                .chain(sub.get_all_aliases().map(String::from))
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Sorts a walked flag list by long name and dedups it (node-first
/// order is preserved by the stable sort, so a node-local flag wins a
/// same-named root global in [`CompletionSpec::flags_at`]'s union).
fn normalize(mut flags: Vec<FlagSpec>) -> Vec<FlagSpec> {
    flags.sort_by(|a, b| a.long.cmp(&b.long));
    flags.dedup_by(|a, b| a.long == b.long);
    flags
}

impl CompletionSpec {
    /// Walks the real CLI shape. Unit tests pin the walked spec against
    /// the `cli.rs` catalogue, so a clap upgrade or verb change that
    /// alters what the walk sees fails loudly instead of silently
    /// serving stale candidates.
    #[must_use]
    pub(crate) fn from_cli() -> Self {
        let mut root = crate::cli::Cli::command();
        root.build();
        let globals = normalize(walk_flags(&root, true));
        let mut groups = BTreeMap::new();
        let mut flags = BTreeMap::new();
        let mut positionals = BTreeMap::new();
        let mut offered_verbs = vec!["exit".to_owned(), "quit".to_owned()];
        for sub in root.get_subcommands() {
            let name = sub.get_name().to_owned();
            if REFUSED_VERBS.contains(&name.as_str()) {
                continue;
            }
            let path = vec![name.clone()];
            if sub.has_subcommands() {
                let subs = subcommand_names(sub);
                for sub_name in &subs {
                    if let Some(sub_cmd) = sub.get_subcommands().find(|s| {
                        s.get_name() == *sub_name || s.get_all_aliases().any(|a| a == sub_name)
                    }) {
                        let nested = vec![name.clone(), sub_name.clone()];
                        flags.insert(nested.clone(), normalize(walk_flags(sub_cmd, false)));
                        positionals.insert(nested, walk_positionals(sub_cmd));
                    }
                }
                groups.insert(name.clone(), subs);
            }
            flags.insert(path.clone(), normalize(walk_flags(sub, false)));
            positionals.insert(path, walk_positionals(sub));
            offered_verbs.push(name);
        }
        Self {
            groups,
            offered_verbs,
            globals,
            flags,
            positionals,
        }
    }

    fn is_refused(verb: &str) -> bool {
        REFUSED_VERBS.contains(&verb)
    }

    /// The resolved flag set at a path: node flags ∪ the root globals
    /// clap actually propagates (globals are unioned at word 0 in full;
    /// off the root, root-local flags like `--version` drop out — clap
    /// would reject them there). Used for value-word counting and
    /// flag lookup; the *menus* go through [`Self::offered_flags_at`].
    fn flags_at(&self, path: &[String]) -> Vec<FlagSpec> {
        let mut flags = self.flags.get(path).cloned().unwrap_or_default();
        flags.extend(
            self.globals
                .iter()
                .filter(|f| path.is_empty() || !f.root_only)
                .cloned(),
        );
        normalize(flags)
    }

    /// The flag menu at a path: the resolved flag set minus the
    /// session-refused `--config` (counted, never offered).
    fn offered_flags_at(&self, path: &[String]) -> Vec<FlagSpec> {
        self.flags_at(path)
            .into_iter()
            .filter(|f| !EXCLUDED_FLAGS.contains(&f.long.as_str()))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Planning (pure)
// ---------------------------------------------------------------------------

/// The routing decision for one completion request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompletionPlan {
    /// Byte offset where the replacement is inserted (the cursor word's
    /// start, or just after `=` for attached flag values).
    pub start: usize,
    /// Which source answers.
    pub kind: Kind,
    /// The stem to prefix-match against.
    pub stem: String,
    /// The offered subverbs (the `SubVerbs` kind).
    pub subverbs: Vec<String>,
    /// The offered flags at this position (Flags kind; exclusions
    /// applied, deduped by long name).
    pub offered_flags: Vec<FlagSpec>,
}

impl CompletionPlan {
    fn none(prefix: &Prefix) -> Self {
        Self {
            start: prefix.stem_start,
            kind: Kind::None,
            stem: String::new(),
            subverbs: Vec::new(),
            offered_flags: Vec::new(),
        }
    }
}

/// The word-0 plan: a bare global-flag stem completes the root
/// globals; anything else is the verb slot.
fn plan_verb_slot(spec: &CompletionSpec, prefix: &Prefix) -> CompletionPlan {
    if prefix.raw.starts_with("--") {
        return CompletionPlan {
            start: prefix.stem_start,
            kind: Kind::Flags,
            stem: prefix.stem.clone(),
            subverbs: Vec::new(),
            offered_flags: spec.offered_flags_at(&[]),
        };
    }
    CompletionPlan {
        start: prefix.stem_start,
        kind: Kind::Verbs,
        stem: prefix.stem.clone(),
        subverbs: Vec::new(),
        offered_flags: Vec::new(),
    }
}

/// Routes one prefix to its completion plan. Unclassifiable shapes
/// yield `Kind::None` — no candidates, never a guess.
#[must_use]
pub(crate) fn plan(spec: &CompletionSpec, prefix: &Prefix) -> CompletionPlan {
    // Word 0: the verb slot (the stem may be non-empty: `ta|`).
    if prefix.words.is_empty() {
        return plan_verb_slot(spec, prefix);
    }

    let verb = &prefix.words[0];
    if CompletionSpec::is_refused(verb) {
        return CompletionPlan::none(prefix);
    }
    let plain = prefix.raw == prefix.stem;
    let attached = plain && prefix.raw.starts_with("--") && prefix.raw.contains('=');
    if let Some(eq) = attached.then(|| prefix.raw.find('=')).flatten() {
        // `--flag=value|`: the value completes from the flag's kind;
        // the replacement starts just past the `=`.
        let flag_long = &prefix.raw[2..eq];
        let path = prefix.words.clone();
        let kind = spec
            .flags_at(&path)
            .iter()
            .find(|f| f.long == flag_long)
            .map_or(Kind::None, |f| f.value_kind.clone());
        return CompletionPlan {
            start: prefix.stem_start + eq + 1,
            kind,
            stem: prefix.stem[eq + 1..].to_owned(),
            subverbs: Vec::new(),
            offered_flags: Vec::new(),
        };
    }

    // A group verb with its subverb slot still open: `tasks |`.
    if prefix.words.len() == 1
        && let Some(out) = spec.plan_subverb_slot(prefix)
    {
        return out;
    }

    let Some(path_end) = spec.resolve_path(&prefix.words) else {
        return CompletionPlan::none(prefix);
    };
    let path = &prefix.words[..path_end];

    // Flag-stem completion: `tasks list --st|`.
    if prefix.raw.starts_with("--") {
        return CompletionPlan {
            start: prefix.stem_start,
            kind: Kind::Flags,
            stem: prefix.stem.clone(),
            subverbs: Vec::new(),
            offered_flags: spec.offered_flags_at(path),
        };
    }

    // Count the positionals consumed so far on the path, skipping flag
    // words and their values; a value-taking flag with no attached
    // value claims the next word — the cursor word, if nothing follows.
    let path_flags = spec.flags_at(path);
    let mut consumed = 0usize;
    let mut pending_value: Option<Kind> = None;
    for word in &prefix.words[path_end..] {
        if pending_value.take().is_some() {
            continue; // this word is the previous flag's value
        }
        if word == "--" {
            return CompletionPlan::none(prefix);
        }
        if word.starts_with("--") {
            let long = word.trim_start_matches('-');
            let long = long.split('=').next().unwrap_or(long);
            let Some(flag) = path_flags.iter().find(|f| f.long == long) else {
                // Unknown flag mid-line: unclassifiable (§2.7).
                return CompletionPlan::none(prefix);
            };
            if flag.takes_value && !word.contains('=') {
                pending_value = Some(flag.value_kind.clone());
            }
            continue;
        }
        consumed += 1;
    }

    let kind =
        pending_value.unwrap_or_else(|| spec.spec_positional(&prefix.words[..path_end], consumed));
    CompletionPlan {
        start: prefix.stem_start,
        kind,
        stem: prefix.stem.clone(),
        subverbs: Vec::new(),
        offered_flags: Vec::new(),
    }
}

impl CompletionSpec {
    /// The plan for a bare group verb with its subverb slot still open
    /// (`tasks |`, `tasks ad|`). After a group verb word 1 is always
    /// the subverb slot (the CLI shape guarantees every group verb
    /// takes its nested subcommand). `None` keeps the positional
    /// routing (the group slot is filled).
    fn plan_subverb_slot(&self, prefix: &Prefix) -> Option<CompletionPlan> {
        let subs = self.groups.get(&prefix.words[0])?;
        if prefix.raw.starts_with("--") {
            // `tasks --js|`: a global flag stem before the subverb.
            return Some(CompletionPlan {
                start: prefix.stem_start,
                kind: Kind::Flags,
                stem: prefix.stem.clone(),
                subverbs: Vec::new(),
                offered_flags: self.offered_flags_at(&prefix.words),
            });
        }
        Some(CompletionPlan {
            start: prefix.stem_start,
            kind: Kind::SubVerbs,
            stem: prefix.stem.clone(),
            subverbs: subs.clone(),
            offered_flags: Vec::new(),
        })
    }
}

/// The walked node for a line's verb words: the word index just past
/// the path (`["tasks", "add"]` → 2, `["sync"]` → 1). `None` for
/// unknown verbs or a malformed path (conservative: no candidates).
impl CompletionSpec {
    fn resolve_path(&self, words: &[String]) -> Option<usize> {
        let verb = &words[0];
        if let Some(subs) = self.groups.get(verb) {
            let sub = words.get(1)?;
            if !subs.contains(sub) {
                return None;
            }
            return Some(2);
        }
        // Flat verbs (sync/status and the REPL exits; refused verbs are
        // checked before this runs). `exit`/`quit` have no walked path:
        // an empty node, only the globals union in.
        self.offered_verbs.contains(verb).then_some(1)
    }

    fn spec_positional(&self, path: &[String], index: usize) -> Kind {
        self.positionals
            .get(path)
            .and_then(|kinds| kinds.get(index))
            .cloned()
            .unwrap_or(Kind::None)
    }
}

// ---------------------------------------------------------------------------
// Candidate generation (pure)
// ---------------------------------------------------------------------------

/// The replacement form of one candidate: `quote(word) + " "` — the
/// trailing space advances the cursor past the token uniformly for
/// verbs, flags, and titles.
fn quoted(word: &str) -> Candidate {
    Candidate {
        display: word.to_owned(),
        replacement: format!("{} ", shell_words::quote(word)),
    }
}

/// Builds the candidates for one plan: static ones from the spec, live
/// titles from a state snapshot (filters/ordering per the parity rule,
/// duplicate titles deduped).
#[must_use]
pub(crate) fn candidates(
    spec: &CompletionSpec,
    state: &AppState,
    connector_names: &[String],
    plan: &CompletionPlan,
) -> Vec<Candidate> {
    let stem = plan.stem.as_str();
    match &plan.kind {
        Kind::Verbs => spec
            .offered_verbs
            .iter()
            .filter(|v| v.starts_with(stem))
            .map(|v| quoted(v))
            .collect(),
        Kind::SubVerbs => plan
            .subverbs
            .iter()
            .filter(|s| s.starts_with(stem))
            .map(|s| quoted(s))
            .collect(),
        Kind::Flags => plan
            .offered_flags
            .iter()
            .map(FlagSpec::offered_form)
            .filter(|flag| flag.starts_with(stem))
            .map(|flag| quoted(&flag))
            .collect(),
        Kind::Literals(literals) => literals
            .iter()
            .filter(|l| l.starts_with(stem))
            .map(|l| quoted(l))
            .collect(),
        Kind::Stacks => {
            let mut stacks: Vec<(i64, ids::StackId, &str)> = state
                .stacks
                .values()
                .filter(|s| !s.deleted)
                .map(|s| (s.order, s.id, s.title.as_str()))
                .collect();
            stacks.sort();
            live_entries(
                stacks.iter().map(|(_, id, title)| (id.as_uuid(), *title)),
                stem,
            )
        }
        Kind::Tasks => {
            let mut tasks: Vec<(ids::StackId, i64, ids::TaskId, &str)> = state
                .tasks
                .values()
                .filter(|t| t.is_live())
                .map(|t| (t.stack, t.order, t.id, t.title.as_str()))
                .collect();
            tasks.sort();
            live_entries(
                tasks.iter().map(|(_, _, id, title)| (id.as_uuid(), *title)),
                stem,
            )
        }
        Kind::Labels => live_entries(
            state
                .labels
                .values()
                .filter(|l| !l.deleted)
                .map(|l| (l.id.as_uuid(), l.title.as_str())),
            stem,
        ),
        Kind::Boards => live_entries(
            state
                .boards
                .values()
                .filter(|b| !b.deleted)
                .map(|b| (b.id.as_uuid(), b.title.as_str())),
            stem,
        ),
        Kind::Connectors => connector_names
            .iter()
            .filter(|n| n.starts_with(stem))
            .map(|n| quoted(n))
            .collect(),
        Kind::None => Vec::new(),
    }
}

/// Builds the menu rows for one live kind: prefix-matches (case-
/// sensitive — selectors are exact-match) the `(uuid, title)` pairs in
/// their canonical order, then groups by title. A title with one
/// entity keeps today's plain row; a hard collision emits one
/// uuid-disambiguated row per entity — display `"{title} - {uuid}"`
/// (title first: the identifier humans parse; the uuid is only the
/// distinguisher), replacement the quoted uuid — and no plain title
/// row (selecting a colliding title manufactures a guaranteed
/// `Ambiguous*` error). Group order = first encounter in canonical
/// order; within a group, canonical entity order.
fn live_entries<'a>(
    entries: impl Iterator<Item = (uuid::Uuid, &'a str)>,
    stem: &str,
) -> Vec<Candidate> {
    let matched: Vec<(uuid::Uuid, &str)> = entries
        .filter(|(_, title)| title.starts_with(stem))
        .collect();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, title) in &matched {
        *counts.entry(title).or_default() += 1;
    }
    let mut emitted_groups: BTreeSet<&str> = BTreeSet::new();
    let mut out = Vec::new();
    for (_, title) in &matched {
        if !emitted_groups.insert(title) {
            continue;
        }
        if counts[title] == 1 {
            out.push(quoted(title));
        } else {
            for (member, member_title) in matched.iter().filter(|(_, t)| *t == *title) {
                out.push(Candidate {
                    display: format!("{member_title} - {member}"),
                    replacement: format!("{} ", shell_words::quote(&member.to_string())),
                });
            }
        }
    }
    out
}

/// The composition the Helper calls (after its one `load_full()`):
/// `None` when the position completes nothing ([`Kind::None`]); an
/// empty list when the kind matches but no candidate carries the stem.
#[must_use]
pub(crate) fn complete(
    spec: &CompletionSpec,
    state: &AppState,
    connector_names: &[String],
    line: &str,
    pos: usize,
) -> Option<(usize, Vec<Candidate>)> {
    let prefix = tokenize_prefix(line, pos);
    let plan = plan(spec, &prefix);
    if plan.kind == Kind::None {
        return None;
    }
    let out = candidates(spec, state, connector_names, &plan);
    Some((plan.start, out))
}

// ---------------------------------------------------------------------------
// rustyline glue — the module's only rustyline surface
// ---------------------------------------------------------------------------

/// The rustyline `Helper` the editor thread installs: a [`CompletionSpec`]
/// plus the shared-state handle. Completion happens *inside*
/// `readline` on the editor thread; the only impure act is one
/// lock-free `load_full()` per Tab — no channels, no awaits, no
/// mutation (the UI-layer read rule, AGENTS §1).
pub(crate) struct ReplHelper {
    spec: CompletionSpec,
    state: Arc<arc_swap::ArcSwap<AppState>>,
    /// The configured connector names, boot-fixed for the session (the
    /// connect ceremony rewrites the default entry's identity, never
    /// the name set). Config is not engine state, so it rides beside
    /// the state handle, not through it.
    connector_names: Vec<String>,
}

impl ReplHelper {
    #[must_use]
    pub(crate) fn new(
        state: Arc<arc_swap::ArcSwap<AppState>>,
        connector_names: Vec<String>,
    ) -> Self {
        Self {
            spec: CompletionSpec::from_cli(),
            state,
            connector_names,
        }
    }
}

impl rustyline::completion::Completer for ReplHelper {
    type Candidate = rustyline::completion::Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        // None / Kind::None / an empty match all map to *no candidates*,
        // never an error — a wrong-shaped line must not fail the Tab.
        let state = self.state.load_full();
        let out = complete(&self.spec, &state, &self.connector_names, line, pos)
            .unwrap_or((pos, Vec::new()));
        Ok((
            out.0,
            out.1
                .into_iter()
                .map(|c| rustyline::completion::Pair {
                    display: c.display,
                    replacement: c.replacement,
                })
                .collect(),
        ))
    }
}

impl rustyline::hint::Hinter for ReplHelper {
    type Hint = String;
}
impl rustyline::highlight::Highlighter for ReplHelper {}
impl rustyline::validate::Validator for ReplHelper {}
impl rustyline::Helper for ReplHelper {}

// ---------------------------------------------------------------------------
// Tests — all structural, no TTY, no text assertions on UI labels
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        Candidate, CompletionPlan, CompletionSpec, Kind, Prefix, candidates, complete, plan,
        quoted, tokenize_prefix,
    };
    use taskboard_domain::ids::{BoardId, LabelId, StackId, TaskId};
    use taskboard_domain::{
        AppState, Board, Label, LabelClocks, Stack, StackClocks, Task, TaskClocks,
    };
    use uuid::Uuid;

    // -- fixtures -----------------------------------------------------------

    fn seeded_id(seed: u128) -> Uuid {
        Uuid::from_u128(seed)
    }

    /// The domain's epoch clock: every fixture clock field shares it
    /// (clock values are irrelevant to completion).
    fn epoch() -> chrono::DateTime<chrono::Utc> {
        taskboard_domain::BoardClocks::epoch()
    }

    fn board(state: &mut AppState) -> BoardId {
        let board_id = BoardId::from_uuid(seeded_id(0x100));
        state.boards.insert(
            board_id,
            Board {
                id: board_id,
                remote: None,
                title: "Personal".to_owned(),
                color: taskboard_domain::Color::new("0082c9"),
                archived: false,
                deleted: false,
                clocks: taskboard_domain::BoardClocks {
                    title: epoch(),
                    color: taskboard_domain::BoardClocks::epoch(),
                    archived: taskboard_domain::BoardClocks::epoch(),
                    deleted: taskboard_domain::BoardClocks::epoch(),
                },
                remote_seen: None,
            },
        );
        board_id
    }

    fn stack(
        state: &mut AppState,
        board: BoardId,
        order: i64,
        title: &str,
        deleted: bool,
    ) -> StackId {
        let stack_id = StackId::from_uuid(Uuid::new_v4());
        state.stacks.insert(
            stack_id,
            Stack {
                id: stack_id,
                remote: None,
                board,
                title: title.to_owned(),
                order,
                archived: false,
                deleted,
                clocks: StackClocks {
                    title: epoch(),
                    order: epoch(),
                    deleted: epoch(),
                },
                remote_seen: None,
            },
        );
        stack_id
    }

    #[allow(clippy::too_many_arguments)]
    fn task(
        state: &mut AppState,
        stack_id: StackId,
        order: i64,
        title: &str,
        done: bool,
        deleted: bool,
        archived: bool,
    ) -> TaskId {
        let task_id = TaskId::from_uuid(Uuid::new_v4());
        state.tasks.insert(
            task_id,
            Task {
                id: task_id,
                remote: None,
                title: title.to_owned(),
                description: String::new(),
                duedate: None,
                scheduled: None,
                done: done.then(chrono::Utc::now),
                stack: stack_id,
                order,
                labels: std::collections::BTreeSet::default(),
                effort: None,
                importance: None,
                urgency: None,
                archived,
                deleted,
                clocks: TaskClocks {
                    title: epoch(),
                    description: epoch(),
                    duedate: epoch(),
                    scheduled: epoch(),
                    done: epoch(),
                    position: epoch(),
                    labels: epoch(),
                    effort: epoch(),
                    importance: epoch(),
                    urgency: epoch(),
                    archived: epoch(),
                    deleted: epoch(),
                },
                remote_seen: None,
            },
        );
        task_id
    }

    fn label(state: &mut AppState, board: BoardId, title: &str, deleted: bool) -> LabelId {
        let label_id = LabelId::from_uuid(Uuid::new_v4());
        state.labels.insert(
            label_id,
            Label {
                id: label_id,
                remote: None,
                board,
                title: title.to_owned(),
                color: taskboard_domain::Color::new("31cc7c"),
                deleted,
                clocks: LabelClocks {
                    title: epoch(),
                    color: epoch(),
                    deleted: epoch(),
                },
                remote_seen: None,
            },
        );
        label_id
    }

    fn fresh_state() -> (AppState, BoardId) {
        let mut state = AppState::default();
        let board_id = board(&mut state);
        (state, board_id)
    }

    fn replacements(out: &[Candidate]) -> Vec<&str> {
        out.iter().map(|c| c.replacement.as_str()).collect()
    }

    fn titles_of(out: &[Candidate]) -> Vec<String> {
        out.iter().map(|c| c.display.clone()).collect()
    }

    fn kind_plan(kind: Kind, stem: &str) -> CompletionPlan {
        CompletionPlan {
            start: 0,
            kind,
            stem: stem.to_owned(),
            subverbs: Vec::new(),
            offered_flags: Vec::new(),
        }
    }

    // -- tokenizer table (§4.2) ----------------------------------------------

    #[test]
    fn the_tokenizer_table_splits_stems_offsets_and_attached_values() {
        let tok = |line: &str, pos: usize| -> Prefix { tokenize_prefix(line, pos) };
        let w = |ws: &[&str]| -> Vec<String> { ws.iter().map(|s| (*s).to_owned()).collect() };

        // `ta|`
        let p = tok("ta", 2);
        assert!(p.words.is_empty(), "no words before the stem");
        assert_eq!(p.stem, "ta");
        assert_eq!(p.stem_start, 0);

        // `tasks ad|`
        let p = tok("tasks ad", 8);
        assert_eq!(p.words, w(&["tasks"]));
        assert_eq!(p.stem, "ad");
        assert_eq!(p.stem_start, 6);

        // `tasks add |` — fresh word past the separator.
        let p = tok("tasks add ", 10);
        assert_eq!(p.words, w(&["tasks", "add"]));
        assert_eq!(p.stem, "");
        assert_eq!(p.stem_start, 10);

        // `tasks add "Buy m|` — inside an open quote; the leading quote
        // is part of what gets replaced.
        let p = tok("tasks add \"Buy m", 16);
        assert_eq!(p.words, w(&["tasks", "add"]));
        assert_eq!(p.stem, "Buy m");
        assert_eq!(p.stem_start, 10);
        assert_eq!(p.raw, "\"Buy m");

        // `tasks list --stack=To|` — attached value.
        let p = tok("tasks list --stack=To", 21);
        assert_eq!(p.words, w(&["tasks", "list"]));
        assert_eq!(p.raw, "--stack=To");
        assert_eq!(p.stem_start, 11);

        // `tasks list --stack To|` — standalone value after the flag.
        let p = tok("tasks list --stack To", 21);
        assert_eq!(p.words, w(&["tasks", "list", "--stack"]));
        assert_eq!(p.stem, "To");
        assert_eq!(p.stem_start, 19);

        // Cursor mid-word.
        let p = tok("tasks list --json", 15);
        assert_eq!(p.words, w(&["tasks", "list"]));
        assert_eq!(p.stem, "--js");
        assert_eq!(p.stem_start, 11);

        // Escaped separator decodes into the stem.
        let p = tok("tasks add Buy\\ m", 16);
        assert_eq!(p.words, w(&["tasks", "add"]));
        assert_eq!(p.stem, "Buy m");
        assert_eq!(p.stem_start, 10);
    }

    #[test]
    fn the_tokenizer_offsets_are_byte_exact_on_multibyte_titles() {
        // "Ü" is two bytes: the stem start must still land on the
        // opening quote, and the decoded stem must carry full chars.
        let line = "stacks rename \"Üb";
        let pos = line.len();
        let p = tokenize_prefix(line, pos);
        assert_eq!(p.words, vec!["stacks".to_owned(), "rename".to_owned()]);
        assert_eq!(p.stem, "Üb");
        assert_eq!(p.stem_start, "stacks rename ".len());
        assert_eq!(p.raw, "\"Üb", "the raw slice covers the opening quote");
    }

    // -- spec walk -------------------------------------------------------------

    #[test]
    #[allow(clippy::too_many_lines)] // one pin per CLI family, kept flat
    fn the_spec_walk_matches_the_cli_catalogue() {
        let spec = CompletionSpec::from_cli();

        // Word 0: groups + flat verbs + REPL exits; refusals excluded.
        for verb in [
            "boards", "stacks", "tasks", "labels", "timers", "sync", "status", "exit", "quit",
        ] {
            assert!(
                spec.offered_verbs.iter().any(|v| v == verb),
                "{verb} must be offered"
            );
        }
        for refused in ["daemon", "config", "gui"] {
            assert!(
                !spec.offered_verbs.iter().any(|v| v == refused),
                "{refused} must not be offered"
            );
            assert!(CompletionSpec::is_refused(refused));
        }
        // The ceremony verbs are offered; their flag menus walk in.
        for offered in ["connect", "login", "logout"] {
            assert!(
                spec.offered_verbs.iter().any(|v| v == offered),
                "{offered} must be offered"
            );
            assert!(!CompletionSpec::is_refused(offered));
        }
        // `connect`'s flag menu (the flat verb has no subcommands —
        // the routing yields `Kind::None` for a filled flat verb).
        let connect_flags = spec
            .flags
            .get(&vec!["connect".to_owned()])
            .expect("connect walked");
        for long in ["server", "user", "switch-backend", "force"] {
            assert!(
                connect_flags.iter().any(|f| f.long == long),
                "--{long} must be offered for connect"
            );
        }
        let login_flags = spec
            .flags
            .get(&vec!["login".to_owned()])
            .expect("login walked");
        for long in ["server", "user"] {
            assert!(login_flags.iter().any(|f| f.long == long));
        }
        assert_eq!(
            routed(&spec, "connect ", 8),
            Kind::None,
            "a flat verb's filled slot completes nothing"
        );

        // Group subverbs.
        for (group, subs) in [
            (
                "boards",
                vec![
                    "archive",
                    "color",
                    "create",
                    "delete",
                    "list",
                    "publish",
                    "reconcile",
                    "rename",
                    "reset",
                    "select",
                    "unbind",
                    "unarchive",
                    "use",
                ],
            ),
            ("stacks", vec!["add", "delete", "move", "rename"]),
            (
                "tasks",
                vec![
                    "add", "delete", "done", "edit", "label", "list", "move", "remind", "unlabel",
                ],
            ),
            ("labels", vec!["add", "delete", "edit", "list"]),
            ("timers", vec!["add", "list", "cancel"]),
        ] {
            let walked = spec.groups.get(group).expect("group walked");
            for sub in subs {
                assert!(walked.iter().any(|s| s == sub), "{group} {sub} missing");
            }
        }

        // Positional kinds, declaration order.
        let kinds = |path: [&str; 2]| {
            spec.positionals
                .get(&path.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
                .expect("path walked")
        };
        assert_eq!(
            kinds(["stacks", "rename"]),
            &[Kind::Stacks, Kind::None],
            "stack selector, then free-text title"
        );
        assert_eq!(kinds(["tasks", "label"]), &[Kind::Tasks, Kind::Labels]);
        assert_eq!(
            kinds(["tasks", "remind"]),
            &[Kind::Tasks],
            "the remind selector, then flags only"
        );
        assert_eq!(
            kinds(["boards", "use"]),
            &[Kind::Boards],
            "boards use's positional completes live boards"
        );
        assert_eq!(
            kinds(["boards", "select"]),
            &[Kind::None],
            "select's remote positional is server state (completes nothing locally)"
        );
        assert_eq!(kinds(["tasks", "list"]), &[] as &[Kind]);
        assert_eq!(
            kinds(["tasks", "add"]),
            &[Kind::None],
            "a new task title is free text"
        );
        assert_eq!(
            kinds(["timers", "add"]),
            &[Kind::None],
            "a timer label is free text"
        );
        assert_eq!(
            kinds(["timers", "cancel"]),
            &[Kind::None],
            "no dynamic alarm completion (the D17 static-tokens ruling)"
        );

        // Value flags.
        let flags = |path: [&str; 2]| {
            spec.flags
                .get(&path.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
                .expect("path walked")
        };
        for path in [["tasks", "list"], ["tasks", "add"], ["tasks", "move"]] {
            let stack_flag = flags(path)
                .iter()
                .find(|f| f.long == "stack")
                .expect("--stack walked");
            assert!(stack_flag.takes_value);
            assert_eq!(stack_flag.value_kind, Kind::Stacks);
        }
        let due = flags(["tasks", "edit"])
            .iter()
            .find(|f| f.long == "due")
            .expect("--due walked");
        assert_eq!(due.value_kind, Kind::Literals(&["none"]));
        for path in [
            ["boards", "rename"],
            ["boards", "delete"],
            ["labels", "add"],
        ] {
            let board_flag = flags(path)
                .iter()
                .find(|f| f.long == "board")
                .expect("--board walked");
            assert!(board_flag.takes_value);
            assert_eq!(board_flag.value_kind, Kind::Boards, "{path:?}");
        }
        let local = flags(["boards", "select"])
            .iter()
            .find(|f| f.long == "local-board")
            .expect("--local-board walked");
        assert_eq!(local.value_kind, Kind::Boards);
        let sync_flags = spec
            .flags
            .get(&vec!["sync".to_owned()])
            .expect("sync walked");
        let sync_connector = sync_flags
            .iter()
            .find(|f| f.long == "connector")
            .expect("--connector walked");
        assert_eq!(sync_connector.value_kind, Kind::Connectors);
        for path in [
            ["boards", "list"],
            ["boards", "select"],
            ["boards", "publish"],
        ] {
            let connector_flag = flags(path)
                .iter()
                .find(|f| f.long == "connector")
                .expect("--connector walked");
            assert!(connector_flag.takes_value);
            assert_eq!(connector_flag.value_kind, Kind::Connectors, "{path:?}");
        }

        // Globals: `--config` kept for counting but never offered;
        // `--version` is root-local (`root_only`), `--json` propagates.
        let globals = |long: &str| spec.globals.iter().find(|f| f.long == long).expect(long);
        for long in ["json", "help", "version", "config"] {
            assert!(spec.globals.iter().any(|f| f.long == long), "{long} walked");
        }
        assert!(globals("version").root_only, "clap keeps it at the root");
        assert!(!globals("json").root_only, "global args propagate");

        // The resolved set at `tasks list`: globals minus root-local,
        // plus the node's own flags. The offered menu there drops
        // `--config` on top.
        let resolved = spec.flags_at(&["tasks".to_owned(), "list".to_owned()]);
        let resolved_longs: Vec<&str> = resolved.iter().map(|f| f.long.as_str()).collect();
        assert!(resolved_longs.contains(&"json"));
        assert!(resolved_longs.contains(&"stack"));
        assert!(
            !resolved_longs.contains(&"version"),
            "root-local off the root"
        );
        let menu = spec.offered_flags_at(&["tasks".to_owned(), "list".to_owned()]);
        let menu_longs: Vec<&str> = menu.iter().map(|f| f.long.as_str()).collect();
        assert_eq!(
            menu_longs,
            ["help", "json", "stack"],
            "--config counted, never offered"
        );
    }

    // -- plan routing table ------------------------------------------------------

    fn routed(spec: &CompletionSpec, line: &str, pos: usize) -> Kind {
        plan(spec, &tokenize_prefix(line, pos)).kind
    }

    #[test]
    fn the_plan_routing_table_routes_verbs_subverbs_flags_and_positionals() {
        let spec = CompletionSpec::from_cli();

        assert_eq!(routed(&spec, "", 0), Kind::Verbs);
        assert_eq!(routed(&spec, "ta", 2), Kind::Verbs);
        assert_eq!(
            routed(&spec, "--js", 4),
            Kind::Flags,
            "bare global flag stem"
        );
        assert_eq!(routed(&spec, "tasks ", 6), Kind::SubVerbs);
        assert_eq!(routed(&spec, "tasks ad", 8), Kind::SubVerbs);
        assert_eq!(
            routed(&spec, "tasks --js", 10),
            Kind::Flags,
            "global stem before the subverb"
        );
        assert_eq!(routed(&spec, "tasks list --st", 15), Kind::Flags);
        assert_eq!(routed(&spec, "tasks list --stack=To", 21), Kind::Stacks);
        assert_eq!(routed(&spec, "tasks list --stack To", 21), Kind::Stacks);
        assert_eq!(routed(&spec, "tasks list --stack ", 19), Kind::Stacks);
        assert_eq!(
            routed(&spec, "tasks add ", 10),
            Kind::None,
            "new title is free text"
        );
        assert_eq!(routed(&spec, "stacks rename ", 14), Kind::Stacks);
        assert_eq!(routed(&spec, "stacks rename \"Old\" ", 20), Kind::None);
        assert_eq!(routed(&spec, "tasks label ", 12), Kind::Tasks);
        assert_eq!(routed(&spec, "tasks label \"t1\" ", 17), Kind::Labels);
        assert_eq!(
            routed(&spec, "tasks edit t2 --due ", 20),
            Kind::Literals(&["none"])
        );
        assert_eq!(routed(&spec, "frobnicate ", 11), Kind::None, "unknown verb");
        assert_eq!(
            routed(&spec, "tasks list --bogus x", 20),
            Kind::None,
            "unknown flag mid-line"
        );
        assert_eq!(
            routed(&spec, "daemon ", 7),
            Kind::None,
            "refused verb completes nothing"
        );
        assert_eq!(routed(&spec, "config pr", 9), Kind::None);
        assert_eq!(
            routed(&spec, "gui ", 4),
            Kind::None,
            "the lifecycle verb completes nothing in-session"
        );
        assert_eq!(
            routed(&spec, "exit ", 5),
            Kind::None,
            "the REPL exit has no arguments"
        );
        assert_eq!(
            routed(&spec, "tasks list ", 11),
            Kind::None,
            "no positionals on tasks list"
        );
    }

    #[test]
    fn attached_values_route_to_the_flag_kind_and_offset_past_the_equals() {
        let spec = CompletionSpec::from_cli();
        let p = tokenize_prefix("tasks list --stack=To", 21);
        let out = plan(&spec, &p);
        assert_eq!(out.kind, Kind::Stacks);
        assert_eq!(out.stem, "To");
        assert_eq!(out.start, "tasks list --stack=".len());
    }

    // -- candidates over a hand-built AppState ------------------------------------

    #[test]
    fn live_candidates_parity_the_selector_filters_and_orderings() {
        let spec = CompletionSpec::from_cli();
        let (mut state, board_id) = fresh_state();
        let todo = stack(&mut state, board_id, 1, "Todo", false);
        stack(&mut state, board_id, 0, "Archived", false);
        stack(&mut state, board_id, 2, "Tombstoned", true);
        // Tasks: a done and an archived task stay selectable (is_live);
        // a tombstoned one does not.
        task(&mut state, todo, 1, "Buy milk", false, false, false);
        task(&mut state, todo, 0, "Archived chore", false, false, true);
        task(&mut state, todo, 2, "Done report", true, false, false);
        task(&mut state, todo, 3, "Deleted thing", false, true, false);
        label(&mut state, board_id, "urgent", false);
        label(&mut state, board_id, "gone", true);

        // Stacks: tombstoned absent, canonical (order, id) order.
        let titles = titles_of(&candidates(
            &spec,
            &state,
            &[],
            &kind_plan(Kind::Stacks, ""),
        ));
        assert_eq!(titles, ["Archived", "Todo"]);

        // Tasks: done and archived in, tombstoned out.
        let titles = titles_of(&candidates(&spec, &state, &[], &kind_plan(Kind::Tasks, "")));
        assert_eq!(titles, ["Archived chore", "Buy milk", "Done report"]);

        // Labels: tombstoned out.
        let titles = titles_of(&candidates(
            &spec,
            &state,
            &[],
            &kind_plan(Kind::Labels, ""),
        ));
        assert_eq!(titles, ["urgent"]);
    }

    #[test]
    fn candidate_prefixes_are_case_sensitive_and_collisions_offer_uuid_rows() {
        let spec = CompletionSpec::from_cli();
        let (mut state, board_id) = fresh_state();
        let todo = stack(&mut state, board_id, 0, "Todo", false);
        let other = stack(&mut state, board_id, 1, "Other", false);
        let first = task(&mut state, todo, 0, "Buy milk", false, false, false);
        let second = task(&mut state, other, 0, "Buy milk", false, false, false);
        task(&mut state, other, 1, "buy milk later", false, false, false);

        // Case-sensitive: `buy` matches only the lowercase title, never
        // `Buy milk` (selectors are exact-match).
        let out = candidates(&spec, &state, &[], &kind_plan(Kind::Tasks, "buy"));
        assert_eq!(titles_of(&out), ["buy milk later"]);

        // A hard collision replaces the deduped title row with one
        // uuid row per entity: title first, the uuid the distinguisher.
        let out = candidates(&spec, &state, &[], &kind_plan(Kind::Tasks, "Buy"));
        assert_eq!(out.len(), 2, "one uuid row per colliding entity");
        let offered: std::collections::BTreeMap<String, String> = out
            .iter()
            .map(|c| (c.display.clone(), c.replacement.trim_end().to_owned()))
            .collect();
        for id in [first, second] {
            let uuid = id.as_uuid().to_string();
            let display = format!("Buy milk - {uuid}");
            let selector = offered
                .get(&display)
                .unwrap_or_else(|| panic!("no row for {display}: {offered:?}"));
            // The insertion resolves exactly — the full uuid is the
            // always-exact tier of the cascading selector.
            assert_eq!(
                crate::commands::tasks::resolve_task(&state, selector).expect("resolves"),
                id,
                "row {display} inserts a selector that resolves exactly"
            );
            // The insertion is the uuid, re-parsing through the line
            // grammar (quoting roundtrip).
            assert_eq!(
                shell_words::split(selector),
                Ok(vec![uuid.clone()]),
                "row {display} inserts the uuid alone"
            );
        }
    }

    #[test]
    fn colliding_stack_and_label_titles_offer_uuid_rows_with_the_same_parity() {
        let spec = CompletionSpec::from_cli();
        let (mut state, board_id) = fresh_state();
        let first = stack(&mut state, board_id, 0, "Sprint", false);
        let second = stack(&mut state, board_id, 1, "Sprint", false);
        let label_a = label(&mut state, board_id, "urgent", false);
        let label_b = label(&mut state, board_id, "urgent", false);

        // Stacks: two uuid rows, each resolving exactly; display is
        // title-first. Canonical order is (order, id) — the fixture's
        // orders (0, 1) decide, so the rows are deterministic.
        let out = candidates(&spec, &state, &[], &kind_plan(Kind::Stacks, "Spr"));
        assert_eq!(out.len(), 2);
        for (cand, id) in out.iter().zip([first, second]) {
            let uuid = id.as_uuid().to_string();
            assert_eq!(cand.display, format!("Sprint - {uuid}"));
            assert_eq!(cand.replacement, format!("{} ", shell_words::quote(&uuid)));
            assert_eq!(
                crate::commands::tasks::resolve_stack(&state, Some(cand.replacement.trim_end()))
                    .expect("resolves"),
                id
            );
        }

        // Labels: same rule (id canonical order — order-agnostic set
        // assertion, the fixture ids are generated).
        let out = candidates(&spec, &state, &[], &kind_plan(Kind::Labels, "urg"));
        assert_eq!(out.len(), 2);
        let offered: Vec<(String, LabelId)> = out
            .iter()
            .map(|c| {
                let selector = c.replacement.trim_end();
                (
                    c.display.clone(),
                    crate::commands::labels::resolve_label(&state, selector)
                        .expect("the insertion resolves exactly"),
                )
            })
            .collect();
        assert_eq!(
            offered
                .iter()
                .map(|(_, id)| *id)
                .collect::<std::collections::BTreeSet<_>>(),
            [label_a, label_b].into_iter().collect()
        );
        for (display, id) in offered {
            assert_eq!(display, format!("urgent - {}", id.as_uuid()));
        }
    }

    #[test]
    fn a_colliding_group_replaces_only_its_title_row_and_keeps_canonical_order() {
        let spec = CompletionSpec::from_cli();
        let (mut state, board_id) = fresh_state();
        let todo = stack(&mut state, board_id, 0, "Todo", false);
        // One stack, orders decide the canonical order: Dup, Solo, Dup.
        let dup_low = task(&mut state, todo, 0, "Dup", false, false, false);
        task(&mut state, todo, 1, "Solo", false, false, false);
        let dup_high = task(&mut state, todo, 2, "Dup", false, false, false);

        let out = candidates(&spec, &state, &[], &kind_plan(Kind::Tasks, ""));
        // Group order = first encounter: the Dup group (order 0) leads,
        // the unique title follows; no plain `Dup` row anywhere.
        assert_eq!(out.len(), 3);
        assert_eq!(
            out[0].display,
            format!("Dup - {}", dup_low.as_uuid()),
            "the colliding group leads at its first canonical position"
        );
        assert_eq!(
            out[1].display,
            format!("Dup - {}", dup_high.as_uuid()),
            "within the group, canonical entity order"
        );
        assert_eq!(out[2].display, "Solo", "the unique title stays plain");
        assert_eq!(out[2].replacement, "Solo ");
    }

    #[test]
    fn verb_subverb_flag_and_literal_candidates_prefix_match_from_the_spec() {
        let spec = CompletionSpec::from_cli();
        let state = AppState::default();

        // Verbs: `ta` → only tasks; the full menu carries the exits and
        // none of the session-refused verbs.
        let prefix = tokenize_prefix("ta", 2);
        assert_eq!(
            replacements(&candidates(&spec, &state, &[], &plan(&spec, &prefix))),
            ["tasks "]
        );
        let prefix = tokenize_prefix("", 0);
        let all_verbs = candidates(&spec, &state, &[], &plan(&spec, &prefix));
        let all = replacements(&all_verbs);
        assert!(all.contains(&"exit "));
        assert!(all.contains(&"boards "));
        // The refused verbs are absent; the ceremony verbs are offered
        // like any other (ADR 0014).
        assert!(all.iter().all(|r| !r.starts_with("daemon")
            && !r.starts_with("config")
            && !r.starts_with("gui")));
        assert!(all.contains(&"connect "));
        assert!(all.contains(&"login "));
        assert!(all.contains(&"logout "));

        // Subverbs: `tasks l` → label, list.
        let prefix = tokenize_prefix("tasks l", 7);
        let out = candidates(&spec, &state, &[], &plan(&spec, &prefix));
        assert_eq!(replacements(&out), ["label ", "list "]);

        // Flags: `--st` on `tasks list` → only `--stack`.
        let prefix = tokenize_prefix("tasks list --st", 15);
        assert_eq!(
            replacements(&candidates(&spec, &state, &[], &plan(&spec, &prefix))),
            ["--stack "]
        );

        // Bare `--` before any verb: the root globals, `--config` excluded.
        let prefix = tokenize_prefix("--", 2);
        let out = candidates(&spec, &state, &[], &plan(&spec, &prefix));
        let flags: Vec<&str> = out.iter().map(|c| c.display.as_str()).collect();
        assert_eq!(flags, ["--help", "--json", "--version"]);

        // Root-local `--version`: offered at word 0, never mid-line —
        // clap rejects it there, so completing it would manufacture a
        // usage error.
        let prefix = tokenize_prefix("--ver", 5);
        assert_eq!(
            replacements(&candidates(&spec, &state, &[], &plan(&spec, &prefix))),
            ["--version "]
        );
        let prefix = tokenize_prefix("tasks list --ver", 16);
        let out = candidates(&spec, &state, &[], &plan(&spec, &prefix));
        assert!(
            out.is_empty(),
            "--version is root-local: {:?}",
            replacements(&out)
        );

        // `--config` is counted while routing (its value word is never
        // mistaken for a positional) though never offered.
        assert_eq!(
            routed(&spec, "stacks rename --config x ", 25),
            Kind::Stacks,
            "--config's value consumes itself; the stem targets the stack slot"
        );

        // Literals: `--due` offers exactly `none`.
        let prefix = tokenize_prefix("tasks edit t2 --due ", 20);
        assert_eq!(
            replacements(&candidates(&spec, &state, &[], &plan(&spec, &prefix))),
            ["none "]
        );

        // The field flags: `--scheduled` mirrors `--due`; the levels
        // offer the hyphenated vocabulary plus the clear sentinel;
        // `--effort` stays free text.
        let prefix = tokenize_prefix("tasks edit t2 --scheduled ", 26);
        assert_eq!(
            replacements(&candidates(&spec, &state, &[], &plan(&spec, &prefix))),
            ["none "]
        );
        let prefix = tokenize_prefix("tasks edit t2 --importance h", 28);
        assert_eq!(
            replacements(&candidates(&spec, &state, &[], &plan(&spec, &prefix))),
            ["high "],
            "the h-prefix matches only `high`"
        );
        let prefix = tokenize_prefix("tasks edit t2 --urgency ", 24);
        assert_eq!(
            replacements(&candidates(&spec, &state, &[], &plan(&spec, &prefix))),
            [
                "very-low ",
                "low ",
                "normal ",
                "high ",
                "very-high ",
                "none "
            ]
        );
        let prefix = tokenize_prefix("tasks edit t2 --effort ", 23);
        assert!(
            candidates(&spec, &state, &[], &plan(&spec, &prefix)).is_empty(),
            "--effort is free text"
        );
    }

    // -- board + connector candidates (D6-8) ---------------------------------------

    fn board_state() -> AppState {
        let mut state = AppState::default();
        let clocks = taskboard_domain::BoardClocks {
            title: epoch(),
            color: epoch(),
            archived: epoch(),
            deleted: epoch(),
        };
        let mk = |raw: u128, title: &str, deleted: bool| {
            let id = BoardId::from_uuid(seeded_id(raw));
            (
                id,
                Board {
                    id,
                    remote: None,
                    title: title.to_owned(),
                    color: taskboard_domain::Color::new("0082c9"),
                    archived: false,
                    deleted,
                    clocks,
                    remote_seen: None,
                },
            )
        };
        let (a, board) = mk(0x10, "Personal", false);
        state.boards.insert(a, board);
        let (b, board) = mk(0x11, "Work", false);
        state.boards.insert(b, board);
        let (c, board) = mk(0x12, "Gone", true);
        state.boards.insert(c, board);
        state
    }

    /// `Kind::Boards` parity: live (`!deleted`) boards only, title rows,
    /// collision disambiguation mirrors the Stacks rule.
    #[test]
    fn board_candidates_list_live_boards_with_stacks_parity() {
        let spec = CompletionSpec::from_cli();
        let state = board_state();
        let out = candidates(&spec, &state, &[], &kind_plan(Kind::Boards, ""));
        assert_eq!(titles_of(&out), ["Personal", "Work"], "tombstoned is out");

        // A hard title collision: one uuid row per board, replacement
        // is the uuid.
        let mut state = board_state();
        let (dup, board) = {
            let id = BoardId::from_uuid(seeded_id(0x13));
            (
                id,
                Board {
                    id,
                    remote: None,
                    title: "Work".to_owned(),
                    color: taskboard_domain::Color::new("0082c9"),
                    archived: false,
                    deleted: false,
                    clocks: taskboard_domain::BoardClocks {
                        title: epoch(),
                        color: epoch(),
                        archived: epoch(),
                        deleted: epoch(),
                    },
                    remote_seen: None,
                },
            )
        };
        state.boards.insert(dup, board);
        let out = candidates(&spec, &state, &[], &kind_plan(Kind::Boards, "Wor"));
        assert_eq!(out.len(), 2, "one uuid row per colliding board");
        for cand in &out {
            let uuid = cand.display.split(" - ").nth(1).expect("uuid suffix");
            assert_eq!(
                cand.replacement,
                format!("{} ", shell_words::quote(uuid)),
                "the insertion is the uuid"
            );
        }
    }

    /// `Kind::Connectors` prefix-filters the configured names (plain
    /// quoted literals).
    #[test]
    fn connector_candidates_prefix_filter_the_configured_names() {
        let spec = CompletionSpec::from_cli();
        let state = AppState::default();
        let names = vec!["alpha".to_owned(), "beta".to_owned()];
        let out = candidates(&spec, &state, &names, &kind_plan(Kind::Connectors, ""));
        assert_eq!(titles_of(&out), ["alpha", "beta"]);
        let out = candidates(&spec, &state, &names, &kind_plan(Kind::Connectors, "b"));
        assert_eq!(titles_of(&out), ["beta"]);
    }

    /// Context correctness: connector names offered only at
    /// `--connector` positions; board ids only at `--board`/
    /// `--local-board`/`boards use` positions; nothing new at task/
    /// stack/label selector positions.
    #[test]
    fn the_new_kinds_route_only_at_their_positions() {
        let spec = CompletionSpec::from_cli();
        assert_eq!(
            routed(&spec, "boards list --connector ", 24),
            Kind::Connectors
        );
        assert_eq!(routed(&spec, "sync --connector ", 17), Kind::Connectors);
        assert_eq!(
            routed(&spec, "boards select 3 --connector ", 28),
            Kind::Connectors
        );
        assert_eq!(routed(&spec, "boards rename --board ", 22), Kind::Boards);
        assert_eq!(
            routed(&spec, "boards select --local-board ", 28),
            Kind::Boards
        );
        assert_eq!(routed(&spec, "boards use ", 11), Kind::Boards);
        // The remote positional completes nothing (server state).
        assert_eq!(routed(&spec, "boards select ", 14), Kind::None);
        // Unchanged selector positions.
        assert_eq!(routed(&spec, "tasks label ", 12), Kind::Tasks);
        assert_eq!(routed(&spec, "stacks rename ", 14), Kind::Stacks);
        assert_eq!(routed(&spec, "boards list ", 12), Kind::None);
    }

    // -- quoting roundtrips ---------------------------------------------------------

    #[test]
    fn every_replacement_roundtrips_through_the_line_grammar() {
        let spec = CompletionSpec::from_cli();
        let (mut state, board_id) = fresh_state();
        let todo = stack(&mut state, board_id, 0, "Todo", false);
        stack(&mut state, board_id, 1, "Two words", false);
        stack(&mut state, board_id, 2, "quote'inside", false);
        task(&mut state, todo, 0, "Buy milk", false, false, false);
        task(&mut state, todo, 1, "plain", false, false, false);

        // Collect every replacement the engine can offer, from real
        // prefixes so each kind's menu is actually populated.
        let mut subjects: Vec<String> = Vec::new();
        for line in ["", "tasks ", "tasks list --"] {
            let prefix = tokenize_prefix(line, line.len());
            for c in candidates(&spec, &state, &[], &plan(&spec, &prefix)) {
                subjects.push(c.replacement);
            }
        }
        for kind in [Kind::Stacks, Kind::Tasks, Kind::Labels] {
            for c in candidates(&spec, &state, &[], &kind_plan(kind, "")) {
                subjects.push(c.replacement);
            }
        }
        assert!(subjects.len() > 10, "the menus must be populated");

        for replacement in &subjects {
            assert!(replacement.ends_with(' '), "one trailing space");
            let word = replacement.trim_end();
            let split = shell_words::split(word).expect("quoted form re-splits");
            assert_eq!(split.len(), 1, "replacement {replacement:?} is one word");
        }
    }

    #[test]
    fn quoting_roundtrips_each_display_word_back_exactly() {
        // The load-bearing property: `split(replacement.trim_end())` is
        // exactly the display word — for bare, spaced, and
        // quote-bearing titles alike. The quoting *style* is never
        // asserted.
        for word in ["Todo", "Buy milk", "quote'inside", "Übung"] {
            let c = quoted(word);
            let split = shell_words::split(c.replacement.trim_end()).expect("re-splits");
            assert_eq!(split, vec![word], "roundtrip for {word:?}");
            assert_eq!(c.display, word);
        }
    }

    // -- composed complete() + glue ---------------------------------------------------

    #[test]
    fn complete_routes_live_titles_end_to_end() {
        let spec = CompletionSpec::from_cli();
        let (mut state, board_id) = fresh_state();
        stack(&mut state, board_id, 0, "Todo", false);

        // `stacks rename To|` → the live title, inserted at the stem.
        let (start, out) =
            complete(&spec, &state, &[], "stacks rename To", 16).expect("stack kind");
        assert_eq!(start, "stacks rename ".len());
        assert_eq!(replacements(&out), ["Todo "]);

        // No match → an empty list, still Some (never an error).
        let (_, out) = complete(&spec, &state, &[], "stacks rename Zzz", 17).expect("stack kind");
        assert_eq!(out, Vec::new());

        // Unclassifiable → None (the glue maps that to no candidates).
        assert!(complete(&spec, &state, &[], "frobnicate ", 11).is_none());
    }

    #[test]
    fn the_glue_maps_the_engine_to_rustyline_pairs_without_a_terminal() {
        use rustyline::completion::Completer as _;
        use rustyline::history::DefaultHistory;

        let (mut state, board_id) = fresh_state();
        stack(&mut state, board_id, 0, "Todo", false);
        let helper = super::ReplHelper::new(
            std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(state)),
            Vec::new(),
        );
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // Verb completion: mapped field-for-field into a Pair.
        let (start, out) = helper.complete("ta", 2, &ctx).expect("completes");
        assert_eq!(start, 0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].display, "tasks");
        assert_eq!(out[0].replacement, "tasks ");

        // Unclassifiable → no candidates, no error.
        let (_, out) = helper.complete("frobnicate ", 11, &ctx).expect("no error");
        assert_eq!(out.len(), 0);

        // Live titles read through the ArcSwap snapshot.
        let (start, out) = helper
            .complete("tasks list --stack=To", 21, &ctx)
            .expect("completes");
        assert_eq!(start, "tasks list --stack=".len());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].display, "Todo");
    }
}

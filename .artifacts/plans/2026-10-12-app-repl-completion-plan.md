# [Plan] WI-G — REPL Tab-Completion

- **Date**: 2026-10-12
- **Target**: `taskboard-app` only — new `src/completion.rs` (the
  pure candidate engine + the rustyline `Helper` glue), `src/repl.rs`
  (editor construction + one signature change), `src/lib.rs` (module
  registration), `tests/repl.rs` (mechanical factory update),
  `tests/cli_tty.rs` (one pty completion journey), docs (README.org,
  CHANGELOG.md, ADR 0011 amendment, backlog bookkeeping). **No
  changes** to `taskboard-domain`, `taskboard-state`,
  `taskboard-storage-sqlite`, `taskboard-sync-nextcloud`,
  `taskboard-ui-slint`; no new workspace dependencies.
- **Goal**: dispatch plan §4 WI-G — a rustyline `Helper` for the REPL
  session: verb/subverb/flag completion derived from the static clap
  CLI shape, and live stack/task/label-title completion from the
  session's `ArcSwap<AppState>` (lock-free `load_full` snapshot on the
  editor thread). Completion is strictly read-only (UI-layer rule,
  AGENTS §1); all candidate computation lives in pure functions
  testable without a TTY (WI-G acceptance).
- **Status**: DRAFT — awaiting user review. Product decisions in §2
  are **proposed** (marked [CONFIRM]); everything else is engineering
  shape. (No gated semantics in this item — WI-G has no §4 open
  questions beyond the design choices recorded in §2.)
- **Origin**: backlog "[2026-10-11] REPL tab-completion / hints"
  (`.artifacts/taskboard-potential-improvements-backlog.md`), entered
  from WI-4's plan §7; dispatched by
  `.artifacts/plans/2026-10-12-backlog-batch-dispatch-plan.md` §4 WI-G.
- **Predecessors**: WI-3 (verb surface) and WI-4 (the REPL itself)
  merged; WI-D (app config DX follow-ups) merged — `main` @ `3ac505b`
  carries all three. The WI-4 plan explicitly deferred this feature to
  "once the verb surface settles post-WI-3"; that trigger is met.
- **Branch / worktree**: `feat/app-repl-completion`, worktree
  `../taskboard-WT-feat-app-repl-completion` (created from `main` @
  `3ac505b`). One PR for plan + implementation (house precedent).

---

## 1. Context & Known Facts (do not re-derive; verified on `main` @ `3ac505b`)

- **The REPL owns exactly one blocking read, on one dedicated thread**
  (`repl.rs:99-145`): `spawn_line_reader()` builds a
  `rustyline::DefaultEditor` inside the `repl-line-reader` thread,
  loops `readline(PROMPT)`, and streams `LineEvent`s through a
  bounded(1) `mpsc`. The `Editor` is created and confined to that
  thread on purpose (its `Send` story is sidestepped, WI-4 §4.3).
  Completion happens *inside* `readline` — it never touches the
  channel, the session loop, or the async runtime.
- **rustyline 17.0.2 API surface (verified against the vendored
  source)**: `DefaultEditor` is `Editor<(), DefaultHistory>`
  (lib.rs:599); `Editor::<H, DefaultHistory>::new()` exists for any
  `H: Helper` (lib.rs:601-609); `Editor::set_helper(Option<H>)`
  registers it (lib.rs:839). `Helper` is
  `Self: Completer + Hinter + Highlighter + Validator` (lib.rs:549-553)
  — the latter three have no-op defaults, so a completion-only helper
  implements `Completer` and the marker. `Completer::complete(&self,
  line, pos, ctx) -> Result<(usize, Vec<Self::Candidate>)>` returns
  the byte offset where insertion starts plus candidates
  (completion.rs:50-69); insertion replaces `line[start..cursor]`
  (`Completer::update`, completion.rs:71-74). `Candidate` needs
  `display()` + `replacement()`; `Pair { display, replacement }` is
  the ready-made struct (completion.rs:29-34). `Context::new(&dyn
  History)` is public "visible for testing" (lib.rs:563-571) — the
  glue layer is unit-testable without a terminal.
- **The editor thread starts before the core boots today**:
  `run_session` calls `spawn_line_reader()` (repl.rs:463) and then
  `run_session_with_lines` boots via `bootstrap`, waits the one-time
  boot-pull, prints the banner, and loops (repl.rs:353-395). So no
  `ArcSwap<AppState>` exists where the editor is constructed — the
  wiring order must change (§4.5).
- **The live-state read side is already the designed UI seam**:
  `EngineHandle::shared_state() -> Arc<ArcSwap<AppState>>`
  (state/actor.rs:144-146), `EngineHandle: Clone` (actor.rs:72).
  `AppState` is plain data: `BTreeMap` boards/stacks/tasks/labels +
  sync status (domain/state.rs:24-39); every handler already reads it
  with `load_full()` (e.g. stacks.rs:39, tasks.rs). `Arc<ArcSwap<_>>`
  is `Send + Sync` — it can move into the editor thread.
- **Completion candidates must parity the selector semantics, or a
  completed line fails to parse/resolve**:
  - `resolve_stack` (commands/tasks.rs:54-88): live = `!deleted`
    (archived stacks **are** selectable); exact uuid → exact title;
    canonical order `(order, id)`.
  - `resolve_task` (commands/tasks.rs:480-522): live = `Task::is_live`
    = `!deleted` (domain/entities.rs:186-191 — done and archived tasks
    are still selectable); canonical order `(stack, order, id)`.
  - `resolve_label` (commands/labels.rs:20-66): live = `!deleted`,
    `BTreeMap` (id) order.
  A completed title is by construction a valid exact-title selector
  iff the same live filter + case-sensitive equality hold. Titles are
  completed, not uuids (WI-G's own wording: "board/stack/task-title
  completion").
- **The CLI shape is static and walkable**: `cli.rs` derives `Cli`
  with subcommand groups `boards|stacks|tasks|labels` (nested
  subcommands), flat verbs `login|logout|sync|config|status|daemon`
  (`daemon` nests `stop`, `config` nests `print`); all flags are
  long-only (`#[arg(long)]` throughout — there are no short flags to
  complete); the only value-taking flags are `--config`, `--server`,
  `--user`, `--color` (×3), `--due`, `--order` (×2), `--timeout-secs`
  (×3), `--stack` (×3, on `tasks list/add/move`), `--title` (×2),
  `--description`; positionals are named by their field ids — `task`,
  `stack`, `label` (selectors) vs `title`/`board` (free text / remote
  id). `Cli::command()` (clap `CommandFactory`) exposes the tree:
  `get_subcommands()`, `get_arguments()`, `Arg::get_id/get_long/
  is_positional/is_hide_set/get_num_args`. Global args (`--config`,
  `--json`) live on the root and are unioned in at query time.
- **In-session refusals already define "what cannot be usefully
  completed"** (`resolve_words`, repl.rs:264-281 + `LineRefusal`
  repl.rs:174-191): verbs `daemon`, `login`, `logout`, `config` and
  the flag `--config` are session-refused; `exit`/`quit` are REPL-only
  builtins (repl.rs:243-249) that are *not* clap subcommands.
- **`shell_words` is already the line grammar**: lines are parsed by
  `shell_words::split` (repl.rs:240); `shell_words::quote` (1.1.1,
  lib.rs:282) is its documented inverse — minimal quoting (`Todo`
  bare, `Buy milk` → `'Buy milk'`, embedded quote → mixed `'…'\''…'`).
  Completions must re-quote with it or the inserted title breaks the
  re-split.
- **Test harness precedent**: `tests/repl.rs` drives full sessions
  against injected `LineEvent` channels + buffers (harness at
  repl.rs:60-78); the editor thread never appears there and **must
  not** start to (no TTY in CI). `tests/cli_tty.rs` drives the real
  binary under a `script(1)` pty with paced input and
  skip-when-`script`-missing guards (cli_tty.rs:39-42, 80-121,
  179-216) — the WI-4 REPL pty journey lives there; completion's
  end-to-end proof rides the same pattern.
- **The IPC twin is unaffected**: the daemon IPC server feeds argv
  words straight into `resolve_words` (repl.rs:252-258, ADR 0012) —
  no TTY, no completion. This work is editor-thread-only.
- **No new dependencies**: rustyline 17's default feature set carries
  the completion machinery (`custom-bindings` is *not* needed);
  `shell-words` and `clap` are already workspace deps of
  `taskboard-app`. `cargo deny` therefore sees no tree change.

## 2. Proposed Decisions ([CONFIRM] at review)

1. **[CONFIRM] Scope: verbs, subverbs, long flags, live titles — no
   hints, no uuids, no remote ids.** Completed positions: word 0 verb
   names (+ REPL builtins `exit`/`quit`), group-verb subverbs, long
   flags valid at the position, live selector titles for `task` /
   `stack` / `label` positionals and the `--stack` value flag.
   Excluded and why: **hints** (ghost text on every keystroke would
   re-read state per keypress and widen churn — the `Helper` seam
   makes them a pure follow-up; the backlog title says "/ hints" but
   the dispatch task line defines completion only), **uuid
   candidates** (titles are the human affordance; uuid flows come from
   receipts/scripts — WI-G's own wording is "title completion"),
   **remote `boards select` ids** (no local source; they come from
   `boards list` output), **short flags** (none exist).
2. **[CONFIRM] Session-refused surface is not offered**: the verbs
   `daemon`, `login`, `logout`, `config` and the flag `--config` are
   excluded from REPL candidates because completing them manufactures
   a guaranteed `LineRefusal` — the honest menu shows what a line can
   actually do in-session. The exclusion list is REPL presentation
   policy layered *after* the walked shape (one small, unit-pinned
   set, cross-referenced to `LineRefusal`); everything else the walk
   finds is offered, including `--help`/`--version` at word 0 (a
   help/version line is a legitimate, working line); root-local flags
   (`--version` — clap does not propagate it) are not offered
   mid-line, where they would only manufacture a usage error.
   Rejected alternative: complete the full
   clap shape verbatim (purer, but knowingly steering users into
   refusals is worse UX for one test-pinned list).
3. **[CONFIRM] Candidate kinds are derived from walked argument ids**:
   the spec walk maps positional ids `task`/`stack`/`label` to live
   kinds; every other positional (`title`, `board`, …) is free text
   (no candidates); value-flags map by long name (`--stack` → Stacks;
   `--due` → the single static literal `none`, its only enumerable
   value; all other value-flags → free text). New verbs that follow
   the same id conventions get completion for free; only a *new* kind
   of source (e.g. remote boards) would need a map entry.
4. **[CONFIRM] Parity rule (the load-bearing invariant)**: a title is
   offered iff the corresponding `resolve_*` could select it — same
   live filter (stacks `!deleted`; tasks `is_live()`; labels
   `!deleted`), same canonical ordering (`(order, id)`,
   `(stack, order, id)`, id-ascending). Case-sensitive prefix
   matching (selectors are exact-match; bash/psql precedent).
   Ambiguous duplicate titles are deduped to one menu entry —
   completing either yields the same ambiguous selector, and the
   command's existing `Ambiguous*` error honestly lists the
   candidates; duplicates in the menu are pure noise.
5. **[CONFIRM] Replacements re-quote via `shell_words::quote` and
   always end with one trailing space**: a completed token is inserted
   at the cursor word's start offset; standalone words get
   `quote(candidate) + " "` (the space advances the cursor past the
   token — uniform for verbs, flags, and titles), the attached form
   `--stack=To|` completes the value stem after `=` with
   `quote(candidate) + " "` (yielding `--stack='Two words' `, which
   `shell_words::split` parses as one word and clap as flag=value).
   Quoting is pinned by roundtrip tests (`split(quote(x)) == [x]`),
   never by asserting the quoting style's text.
6. **[CONFIRM] The editor thread spawns after bootstrap, via a
   lines-factory parameter.** `run_session_with_lines` gains
   `lines: impl FnOnce(Arc<ArcSwap<AppState>>) ->
   mpsc::Receiver<LineEvent>`; production passes `spawn_line_reader`
   (whose signature becomes
   `fn(Arc<ArcSwap<AppState>>) -> Receiver<LineEvent>`); tests pass
   `|_| rx`. The factory runs immediately after `bootstrap` succeeds
   and before the boot-pull wait, preserving today's "prompt is up
   during the boot-pull wait" behavior. Rejected alternative
   (recorded): `OnceLock`-shared state handle filled by the boot —
   keeps the current spawn order but adds a temporal "no state yet"
   mode to every completion; the factory keeps the helper's state
   handle always-valid by construction.
7. **[CONFIRM] Conservative routing: unclassifiable tokens complete
   nothing.** If a word cannot be classified (unknown flag mid-line,
   garbage verb, a value-position we cannot attribute), the plan
   yields no candidates rather than guessing. Wrong candidates are
   worse than none; an empty menu never breaks the line.

## 3. Scope

**In scope**:

| Area | Deliverable |
|---|---|
| Pure engine | New private module `src/completion.rs`: cursor-aware prefix tokenizer, one-time clap-shape walk (`CompletionSpec`), completion planning (`plan`), candidate generation over `&AppState`, the composed entry `complete` — all free of rustyline types, all unit-tested |
| Helper glue | `ReplHelper` (`CompletionSpec` + `Arc<ArcSwap<AppState>>`) implementing `Completer` (+ default `Hinter`/`Highlighter`/`Validator`, marker `Helper`); glue unit-tested via `Context::new` |
| Wiring | `spawn_line_reader(state)` constructs `Editor<ReplHelper, DefaultHistory>` + `set_helper`; `run_session_with_lines` factory parameter; `lib.rs` module registration |
| Tests | Unit suites (tokenizer, spec walk, plan routing, candidates, quoting — structural assertions, no TTY); glue test; `tests/repl.rs` mechanical factory update; one `cli_tty.rs` pty completion journey |
| Docs | README.org REPL section bullet; CHANGELOG `[Unreleased]` Added; ADR 0011 short amendment (completion reads only the UI-side `ArcSwap`); backlog sub-entry ✅ DONE (final stage-2 commit, dispatch §2) |

**Out of scope** (§7): hints/ghost text; uuid candidates; remote
board-id completion; history persistence (separate backlog entry);
`daemon stop`-style completion over the IPC channel (no TTY there);
any `commands/` handler change; anything outside `taskboard-app`.

## 4. Design

### 4.1 The pure seam (the WI-G acceptance core)

One new module, `src/completion.rs`, private to the crate. Everything
below the glue line is a pure function of its arguments — no I/O, no
channels, no runtime, no rustyline types — so the entire engine is
unit-testable in CI without a TTY:

```rust
/// What one completion offers (the rustyline-free core type).
pub(crate) struct Candidate {
    /// Menu text (e.g. the bare title).
    pub display: String,
    /// What is inserted at `start` (already quoted, trailing space).
    pub replacement: String,
}

/// Which source answers a completion position.
pub(crate) enum Kind {
    /// Word 0: the offered verbs (clap groups + `exit`/`quit`).
    Verbs,
    /// Subverbs of a group verb (boards/stacks/tasks/labels/…).
    SubVerbs,
    /// Long flags valid here (root globals ∪ node flags, exclusions applied).
    Flags,
    /// Live title kinds — parity with resolve_* (§2.4).
    Stacks,
    Tasks,
    Labels,
    /// Static enumerable literals for one flag (`--due` → `none`).
    Literals(&'static [&'static str]),
    /// Free text or unclassifiable: offer nothing (§2.7).
    None,
}
```

Layering, top to bottom:

1. `tokenize_prefix(line: &str, pos: usize) -> Prefix` — cursor-aware
   word split (§4.2).
2. `plan(spec: &CompletionSpec, prefix: &Prefix) -> CompletionPlan`
   — which `Kind` answers, plus the byte offset `start` where the
   replacement is inserted (§4.4).
3. `candidates(spec, state: &AppState, plan) -> Vec<Candidate>` —
   static candidates from the spec, live titles from a state snapshot
   (§4.4/§4.6).
4. `complete(spec, state, line, pos) -> Option<(usize, Vec<Candidate>)>`
   — the composition the Helper calls (after doing the one impure
   thing: `load_full()`).

The glue (`ReplHelper`) is the only code that touches rustyline; it
maps `Candidate` → `rustyline::completion::Pair` field-for-field.
The REPL session loop, `resolve_words`, `dispatch_line`, and the IPC
server are untouched.

### 4.2 Cursor-aware tokenization (pure)

`shell_words::split` cannot tokenize a line that is still being
typed (unbalanced quotes, trailing space, cursor mid-word), so the
engine has its own prefix tokenizer with deliberately matched
grammar: whitespace separators; `'…'` and `"…"` quote runs; `\`
escapes (as in `shell_words`); a token ends at the cursor. Output:

```rust
pub(crate) struct Prefix {
    /// Fully-typed words before the cursor word (quotes resolved).
    words: Vec<String>,
    /// The word under/after the cursor as typed so far; "" when the
    /// cursor sits past a separator (fresh word).
    stem: String,
    /// Byte offset in `line` where a replacement for the stem begins.
    stem_start: usize,
    /// Whether the stem is a value attached by `=` (`--stack=To`).
    attached_value: bool,
}
```

`stem_start` points at the start of the cursor *word* (a leading quote
is part of what gets replaced — we re-quote the whole candidate, so
completing inside `"Buy m` replaces from the `"` with `'Buy milk' `).
Position examples (… | = cursor): `ta|` → stem `ta` @0;
`tasks ad|` → words `[tasks]`, stem `ad` @6; `tasks add |` → stem
`""` @10 (all titles offered); `tasks add "Buy m|` → words
`[tasks, add]`, stem `Buy m` @10 (inside the quote, re-quoted on
completion); `tasks list --stack=To|` → attached_value, stem `To`
@19; `tasks list --stack To|` → words `[…, --stack]`, stem `To` @19.
A cursor left of earlier text simply completes the left prefix (the
replacement covers `start..pos` only — rustyline's own semantics).

### 4.3 Walking the clap shape (pure, once per session)

`CompletionSpec::from_cli()` walks `Cli::command()` at editor-thread
construction (clap's tree is static; the walk is cheap):

```rust
pub(crate) struct CompletionSpec {
    /// Group verbs with nested subverbs: boards/stacks/tasks/labels/
    /// daemon/config → their subcommand names + aliases.
    groups: BTreeMap<String, Vec<String>>,
    /// Flat verbs offered at word 0 (sync, status) — the walked
    /// groups minus §2.2 exclusions, plus exit/quit.
    offered_verbs: Vec<String>,
    /// Long flags per node: root globals (--json; --config carried for
    /// word-counting but never offered) ∪ each path's own flags.
    flags: BTreeMap<Vec<String>, Vec<FlagSpec>>,   // path = verb words
    /// Positional kinds per path, in declaration order.
    positionals: BTreeMap<Vec<String>, Vec<Kind>>, // e.g. stacks/rename → [Stacks, None]
}

pub(crate) struct FlagSpec {
    long: String,
    /// Whether the flag consumes a following word (--stack: true;
    /// --json: false) — from Arg::get_num_args; needed for §4.4 counting.
    takes_value: bool,
    /// Completion kind for the value (Stacks for --stack; Literals for
    /// --due; None otherwise).
    value_kind: Kind,
}
```

Walk rules: `Command::get_subcommands()` → names +
`get_all_aliases()`; `get_arguments()` → `get_long()` for flags,
`is_positional()` for positionals; skip `is_hide_set()` args;
positional id → `Kind` via the §2.3 map (`task`→Tasks, `stack`→
Stacks, `label`→Labels, else None); flag long-name → `value_kind`
via the same names (`--stack`→Stacks, `--due`→Literals(["none"]));
root globals are unioned into every path's flag set at query time
(the unbuilt `Command` tree does not propagate globals down, so the
spec carries them once); root-local globals (non-`global` args like
`--version`) union in at word 0 only — mid-line clap rejects them. The §2.2 exclusions (`daemon`, `login`,
`logout`, `config` verbs; `--config`) are applied when building
`offered_verbs`/candidate lists — `--config`'s FlagSpec stays in the
spec for value-word counting (§4.4). Unit tests pin the walked spec
against the CLI's real shape (groups, subverb lists, positional
orders, value-flags) — they fail loudly if a clap upgrade or verb
change alters what the walk sees.

### 4.4 Planning a completion (pure)

`plan(spec, prefix) -> CompletionPlan { start: usize, kind: Kind }`:

1. **No words yet** (`words` empty) → `Verbs`, `start = stem_start`.
   (The stem may be non-empty: `ta|` → Verbs.)
2. **Stem is flag-shaped** (`--…`, not `attached_value`) → `Flags`
   for the current path; flag candidates match the stem's prefix.
3. **Attached value** (`--flag=stem`) → look the flag up in the
   path's flag set → its `value_kind`; `start` = offset after `=`.
4. **Stem is a standalone value or fresh word**:
   - If `words[0]` is a group verb **and** the word being completed
     is its *first* subcommand slot → `SubVerbs` (prefix-matched).
   - Else count *positionals consumed so far* on the path: scan
     `words[1..]`, skipping flag words and their `takes_value`
     consumed values (per `FlagSpec`, globals included); the n-th
     remaining word occupies the n-th positional of the path's
     `positionals` list; the stem then targets the next positional →
     its `Kind`. Path unknown, positional index past the list, or any
     unclassifiable word (§2.7) → `None`.
5. `SubVerbs` beats positional logic: after a group verb, word 1 is
   always the subverb slot (the CLI shape guarantees it — every group
   verb requires its nested subcommand).

### 4.5 Threading & wiring (the only structural change to repl.rs)

```rust
// repl.rs — production editor construction, replacing DefaultEditor:
pub fn spawn_line_reader(
    state: Arc<arc_swap::ArcSwap<taskboard_domain::AppState>>,
) -> mpsc::Receiver<LineEvent> {
    // …thread::spawn (unchanged)…
    let mut editor =
        match rustyline::Editor::<ReplHelper, _>::new() { /* same degrade-to-Eof arm */ };
    editor.set_helper(Some(ReplHelper::new(state)));
    // …readline loop unchanged…
}

// run_session_with_lines — new factory parameter (§2.6):
pub async fn run_session_with_lines(
    config: AppConfig,
    json_default: bool,
    lines: impl FnOnce(Arc<ArcSwap<AppState>>) -> mpsc::Receiver<LineEvent>,
    io: &mut CommandIo<'_>,
) -> u8 {
    // bootstrap → lines(app.engine.shared_state()) → boot-pull wait
    // → banner → repl_loop → shutdown   (order per §2.6)
}
```

On each Tab, `ReplHelper::complete` does exactly one impure thing —
`state.load_full()` (lock-free Arc-clone snapshot; trivially cheap on
a kiosk) — then calls the pure `complete`. The editor thread never
sends on channels, never awaits engine work, never mutates: the
review focus ("only ever reads the ArcSwap") holds by construction.
`Editor` confinement to the thread is unchanged; `ReplHelper`'s
fields are plain data + `Arc`, so the whole value moves into the
thread. On bootstrap failure the factory is simply never called
(today the reader thread dies when its channel drops — same end
state, one thread less).

### 4.6 Candidate generation (pure)

- **Verbs/SubVerbs/Flags**: prefix-match (case-sensitive) over the
  spec lists; replacements `quote(word) + " "`. Flags are offered
  with their `--` prefix included.
- **Literals**: the static list, same shape.
- **Live titles** (Stacks/Tasks/Labels): snapshot filters per the
  parity rule (§2.4) — `state.stacks.values().filter(|s| !s.deleted)`
  sorted `(order, id)`; `state.tasks.values().filter(Task::is_live)`
  sorted `(stack, order, id)`; `state.labels.values().filter(|l|
  !l.deleted)` in id order — prefix-match on the raw stem, dedupe
  identical titles (§2.4), replacement `shell_words::quote(title) +
  " "`.
- Empty stem = all candidates of the kind (the standard
  "show me everything" Tab).

## 5. Normative Shapes (signatures binding; bodies indicative)

```rust
// src/completion.rs (private module; SPDX AGPL header per AGENTS §6)
pub(crate) struct Prefix { /* §4.2 */ }
pub(crate) fn tokenize_prefix(line: &str, pos: usize) -> Prefix;

pub(crate) struct CompletionSpec { /* §4.3 */ }
impl CompletionSpec {
    pub(crate) fn from_cli() -> Self;   // walks Cli::command()
}

pub(crate) struct CompletionPlan { pub start: usize, pub kind: Kind }
pub(crate) fn plan(spec: &CompletionSpec, prefix: &Prefix) -> CompletionPlan;

pub(crate) fn candidates(
    spec: &CompletionSpec,
    state: &AppState,
    plan: &CompletionPlan,
) -> Vec<Candidate>;

pub(crate) fn complete(
    spec: &CompletionSpec,
    state: &AppState,
    line: &str,
    pos: usize,
) -> Option<(usize, Vec<Candidate>)>;

// rustyline glue — the module's only rustyline imports:
pub(crate) struct ReplHelper {
    spec: CompletionSpec,
    state: Arc<ArcSwap<AppState>>,
}
impl ReplHelper {
    pub(crate) fn new(state: Arc<ArcSwap<AppState>>) -> Self;
}
impl rustyline::completion::Completer for ReplHelper {
    type Candidate = rustyline::completion::Pair;
    fn complete(&self, line, pos, _ctx) -> rustyline::Result<(usize, Vec<Pair>)> {
        let state = self.state.load_full();
        complete(&self.spec, &state, line, pos)
            .map(|(start, cs)| (start, cs.into_iter()
                .map(|c| Pair { display: c.display, replacement: c.replacement })
                .collect()))
            .ok_or_else(rustyline::error::ReadlineError::new) // or an Ok-empty convention — stage 2 picks the honest mapping
    }
}
impl rustyline::hint::Hinter for ReplHelper {}
impl rustyline::highlight::Highlighter for ReplHelper {}
impl rustyline::validate::Validator for ReplHelper {}
impl rustyline::Helper for ReplHelper {}
```

(`Kind::None` and `Option::None` from `complete` must map to a
no-candidates answer, not a Tab error beep for wrong-shaped input —
stage 2 pins the exact mapping in the glue test.)

## 6. TDD Implementation Order (branch `feat/app-repl-completion`)

Every commit standalone and gate-green (rebase-merge hygiene);
tests first within each step.

1. **Commit 1 — `feat(app): REPL completion engine (pure) + helper`**:
   the whole `completion.rs` (engine + glue + unit suites) together
   with its wiring (`spawn_line_reader` state parameter + editor type
   switch, `run_session_with_lines` factory, `lib.rs` registration,
   `tests/repl.rs` mechanical `|_| rx` updates) — one commit because
   the engine alone would be dead code under `-D warnings`.
   Unit suites (all structural, no TTY, no text assertions):
   - *Tokenizer table*: every §4.2 example row (stems, offsets,
     attached-value, in-quote, fresh-word, cursor-mid-word), plus one
     multibyte title row pinning byte-offset correctness.
   - *Spec walk*: groups + subverbs match `cli.rs`'s catalogue;
     positional orders (`stacks rename` → `[Stacks, None]`;
     `tasks label` → `[Tasks, Labels]`); value-flags (`--stack` on
     `tasks list/add/move` → Stacks; `--due` → Literals(["none"]));
     exclusions applied (`daemon`/`login`/`logout`/`config` and
     `--config` absent from offered candidates, `--config` still
     counted); `exit`/`quit` present at word 0.
   - *Plan routing table*: (line, pos) → kind for verbs, subverbs,
     flag stems, `--stack=` attached form, positional kinds,
     value-after-flag, free-text → None, garbage verb → None,
     unknown flag mid-line → None.
   - *Candidate rows over a hand-built `AppState`*: live-filter
     parity (tombstoned stack/task/label absent; a done task and an
     archived task present — they are selectable); canonical
     ordering; case-sensitive prefix (`buy` does not match
     `Buy milk`); empty stem offers all; duplicate titles deduped.
   - *Quoting roundtrips*: for the offered replacements,
     `shell_words::split(replacement.trim_end())` yields exactly the
     intended single word (`Buy milk` → `'Buy milk'`; `Todo` →
     `Todo`; embedded-quote title → mixed form) — roundtrip property,
     never asserting the quoting style's text.
   - *Glue test*: `ReplHelper` over a hand-built `ArcSwap<AppState>`,
     driven via `Completer::complete(&helper, line, pos,
     &Context::new(&DefaultHistory::new()))` — asserts start offsets
     and Pair mappings without an `Editor` or terminal.
2. **Commit 2 — `test(app): pty completion journey`**: extend the
   existing `cli_tty.rs` REPL pty journey (paced input, skip-guard on
   `script(1)`) with completion-proof lines, asserted structurally:
   - verb proof: feed `tasks\t list --json\n` (note: Tab expands
     `tasks` + trailing space, so the line runs as `tasks list`): the
     session's stdout line parses as the tasks-list JSON payload —
     without expansion the line is a clap usage error and no such
     payload exists;
   - live-title proof: after `stacks add` of a two-word stack, feed
     `tasks list --stack <prefix>\t --json\n` with a prefix of the
     stack title: the parsed JSON is the expected task list (an
     unexpanded prefix would be an `UnknownStack` JSON error —
     different shape, distinguishable structurally).
3. **Commit 3 — `docs(app): REPL tab-completion notes + backlog
   done`**: README.org REPL section bullet; CHANGELOG
   `[Unreleased]` → Added; ADR 0011 amendment (completion is
   read-only over the UI-side `ArcSwap` on the editor thread; the
   refusal-exclusion policy); backlog sub-entry "[2026-10-11] REPL
   tab-completion / hints" ✅ DONE with a one-line note (hints
   explicitly deferred, §7).
4. **Full gates** (AGENTS §10) after every commit; §8/§9 close out.

## 7. Out of Scope → Backlog (notes appended with commit 3)

- **Hints / ghost text** (title preview at the prompt, Hinter): the
  `Helper` seam makes this additive; deferred to keep this branch
  completion-only (§2.1).
- **Uuid candidates** for task/stack/label selectors (display
  `title (uuid)`, replace uuid when titles collide hard): follow-up
  on the same engine if wanted in practice.
- **`--server`/`--board` remote-value completion** (from `boards
  list` caches): needs a cache design; not now.
- Existing sibling entries (history persistence, Ctrl-C
  cancellation) are untouched.

## 8. Docs & Bookkeeping

- **README.org**: one bullet in the interactive-shell section — Tab
  completes verbs, subverbs, flags, and live stack/task/label titles.
- **CHANGELOG.md**: `[Unreleased]` → Added: REPL tab-completion.
- **ADR 0011 amendment** (short paragraph): completion runs entirely
  on the editor thread, reads only `ArcSwap<AppState>` snapshots
  (lock-free, UI-side rule), candidates parity the `resolve_*`
  selector semantics, and session-refused verbs/flags are excluded
  from the menu.
- **Backlog**: mark the sub-entry ✅ DONE (commit 3).

## 9. Verification & Acceptance

- **Gates** (AGENTS §10, after every commit): `cargo fmt --all --
  --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo nextest run --workspace`; `cargo test --doc --workspace`;
  `cargo deny check`; `cargo llvm-cov --workspace` — app-crate line
  coverage must not regress (the new module arrives fully unit-tested;
  the only uncovered lines should be the thin editor-construction
  arm the pty journey exercises).
- **No TTY anywhere in CI unit/integration layers**: completion is
  proven by the unit + glue suites; the pty journey is the one
  real-terminal proof and carries the skip guard (house precedent).
- **Manual smoke walkthrough (user-run, documented here per WI-G
  acceptance)** — fresh standalone profile, no keyring, no network:
  1. `taskboard` in a terminal → banner + prompt.
  2. bare Tab → the verb menu (boards/stacks/tasks/labels/sync/
     status/exit/quit — and *no* daemon/login/logout/config).
  3. `ta` Tab → `tasks ` completed with a trailing space; Tab again
     → the subverb menu (list add done move edit delete label
     unlabel).
   4. `tasks list --` Tab → the path's flag menu (`--help --json
      --stack`); `--st` Tab → `--stack `; `--ver` Tab on a fresh line →
      `--version ` (root-local flags are word-0 only — mid-line clap
      rejects them, so they are not offered there).
  5. `boards create "Personal"`, `stacks add "Todo"`,
     `tasks add "Buy milk" --stack Todo`.
  6. `tasks list --stack ` Tab → `'Todo' ` inserted (quoted, trailing
     space); Enter → the list.
  7. `tasks done ` Tab → `'Buy milk' ` (done tasks stay completable —
     `done --undo` reopens them).
  8. `tasks edit <task> --due ` Tab → `none ` offered.
   9. `tasks done "Buy m` Tab (inside the open quote) → the line
      becomes `tasks done 'Buy milk' ` and parses (`tasks add` titles
      are free text by design, §2.3, and complete nothing).
  10. duplicate titles: create a second "Buy milk" → Tab offers one
      menu entry; Enter → the existing `AmbiguousTask` error listing
      candidates (honest, pre-existing behavior).
  11. `lab` Tab → labels…; `labels add "urgent"`,
      `tasks label <task> ` Tab → `'urgent' `.
  12. Ctrl-D exits cleanly; the pty session's terminal is restored
      (TtyStateGuard, unchanged).
- **Acceptance (WI-G)**: pure candidate functions unit-tested with
  structural assertions (§6.1); the smoke walkthrough above
  documented in the plan; no business logic in the REPL layer (§4.5 —
  read-only by construction); gates green; backlog sub-entry ✅ DONE.

## 10. Risks & Mitigations

- **rustyline Tab behavior under `script(1)` ptys in CI**: the WI-4
  pty suite already proves rustyline editing works there; the
  completion journey rides the same harness and skip-guards on
  `script` availability; the completion *engine* never depends on it
  (unit + glue layers are terminal-free).
- **Replacement/trailing-space behavior varies by keymap**: pinned
  empirically by the pty journey (the structural payload only appears
  if the expansion produced a parseable line); pure-layer tests pin
  the replacement strings themselves, independent of rustyline's
  insert mechanics.
- **clap walk drift** (clap upgrade or verb change silently alters
  `get_arguments` results): the spec-walk suite pins the walked shape
  to the CLI catalogue — drift fails tests loudly instead of serving
  stale candidates.
- **Word-counting misroutes** (typo'd flags, repeated verbs,
  interleaved globals): the conservative §2.7 rule — any
  unclassifiable token yields no candidates; wrong is worse than
  none. Pinned by routing-table rows.
- **Parity decay** (a future `resolve_*` filter change, e.g.
  excluding archived entities, forgets completion): the parity rule
  is documented in §2.4 + ADR amendment and pinned by the
  live-filter rows; reviewer checks filter identity (stage-3 focus
  list).
- **Byte-offset bugs on multibyte titles** (rustyline positions are
  byte-based): tokenizer table includes a multibyte row; candidate
  generation is offset-free.
- **Editor-thread confinement regression** (someone moves `Editor`
  construction out of the thread): unchanged construction site; the
  glue test constructs the helper directly, so no incentive exists to
  hoist it.
- **Scope creep** (hints, uuids, remote ids): triaged in §2.1/§7; the
  engine's `Kind` enum makes each a contained follow-up.

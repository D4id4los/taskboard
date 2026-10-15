# [Plan] P6 — REPL History Persistence + Uuid Completion Candidates

- **Date**: 2026-10-15
- **Kind**: TDD implementation plan (dispatch chunk P6, stage 1 of the
  three-stage agent protocol) — two standalone feature commits plus one
  bookkeeping commit on one branch.
- **Target**: `taskboard-app` only — new `src/repl_history.rs`, edits to
  `src/repl.rs` (editor-thread wiring), `src/completion.rs`
  (collision-aware live-title generator), `src/lib.rs` (+1 module line),
  `tests/cli_tty.rs` (data-dir isolation + one recall journey), docs
  (README.org, CHANGELOG.md, backlog). **No changes** to the root
  `Cargo.toml` (D1 — the dispatch's rustyline-feature premise is
  superseded by verification), to `src/config.rs` (GUI no-go zone,
  read-only reuse of its *precedent* only), to any other crate, and no
  ADR edits.
- **Goal**: dispatch §7 P6 — (a) file-backed REPL history at the XDG
  data dir following the db-path precedent, best-effort, never killing
  the session; (b) uuid-disambiguated completion candidates for exactly
  the hard title-collision case, quiet menu otherwise.
- **Status**: READY for stage 2. Every open question the dispatch named
  is settled in §2. D8 and D9 were reviewed with the user on
  2026-10-15 (D9's title-first principle is quoted in it verbatim);
  all other decisions were agreed as recorded.
- **Origin**: backlog "[2026-10-11] REPL history persistence
  (file-backed)" and "[2026-10-12] REPL completion: uuid candidates for
  selectors" (`.artifacts/taskboard-potential-improvements-backlog.md`);
  dispatched by `.artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md`
  §7 P6; the completion engine's seams and deferred items come from
  `.artifacts/plans/2026-10-12-app-repl-completion-plan.md` (hints /
  ghost text explicitly deferred there — still out of scope here).
- **Predecessors**: WI-G completion merged (engine + helper + pty
  journey patterns). Base: `main` @ `c9e6762` (== `origin/main`).
- **Branch / worktree**: `feat/app-repl-history-uuid-completion`,
  `../taskboard-WT-feat-app-repl-history-uuid-completion` (created from
  `main` @ `c9e6762`).

---

## 1. Context & Known Facts (verified on `main` @ `c9e6762`; rustyline
facts verified against the vendored 18.0.1 source in the cargo registry)

- **The rustyline pin is major 18, and file history is already compiled
  in.** Root `Cargo.toml:66` pins `rustyline = "18"` (lockfile: 18.0.1)
  with default features; the app crate takes `rustyline.workspace = true`
  (`crates/taskboard-app/Cargo.toml:42`). Rustyline 18's default feature
  set is `["custom-bindings", "with-dirs", "with-file-history"]`, and
  with that feature `pub type DefaultHistory = FileHistory`
  (vendored `history.rs:644-646`). **The dispatch's premise "the
  workspace pin carries no features — the file-history feature must be
  added" is outdated** (it also guessed major 17); no dependency change
  of any kind is needed, and `cargo deny` sees no tree change.
- **The editor already uses `FileHistory` — it just never loads or
  saves.** `spawn_line_reader` (`repl.rs:169-253`) constructs
  `rustyline::Editor::<completion::ReplHelper, rustyline::history::DefaultHistory>::new()`
  inside the `repl-line-reader` thread (`repl.rs:175-188`, degrade-to-Eof
  arm on construction failure), registers the helper at `:189`, and adds
  non-blank submitted lines at `:197-199` inside the `Ok(line)` arm
  (blank lines are "prompt noise, not recallable entries"; prompted
  ceremony answers ride the oneshot path and never reach that arm).
- **`FileHistory` file behavior (vendored 18.0.1 `history.rs`)**:
  - `save(path)` short-circuits when nothing is new; otherwise
    `File::create` (full truncate — **does not create parent
    directories**), file lock, full rewrite from the `#V2` header.
  - `save_to` calls `fix_perm(file)` on every save; on Unix that is
    `fchmod(S_IRUSR | S_IWUSR)` (`history.rs:827-829`) — **rustyline
    itself forces 0600**; on Windows it is a no-op.
  - `append(path)` fast-appends only when the file is unchanged since
    load; otherwise it **re-reads the whole file** — a corrupt file
    makes `append` error (poisoned) while `save` overwrites it blindly.
  - `load(path)` = `File::open` (missing file → `io::NotFound` error);
    parsing adds entries until the first invalid line — a corrupt file
    yields a partial in-memory history plus an `Err`. A non-`#V2` first
    line is treated as a v1 entry (legacy tolerance).
  - Convenience API: `Editor::{load_history, save_history,
    append_history}` (`lib.rs:816-826`) and `Editor::history_mut`
    (`lib.rs:841`). The `History` trait's `load`/`save`/`add` are
    usable directly on a bare `DefaultHistory` **without an Editor or a
    TTY** — the persistence logic is unit-testable hermetically.
- **The data-dir precedent (dispatch directive: follow it)**:
  `default_db_path()` (`config.rs:707-710`) resolves
  `dirs::data_dir().map(|d| d.join("taskboard").join("taskboard.db"))`;
  the config crate's tested seam pattern is a pure function over an
  injected base dir (`xdg_candidate(config_dir: Option<&Path>)`,
  `config.rs:364-376`) with a thin `dirs::*()` wrapper. `src/config.rs`
  is a GUI no-go zone — this plan **reuses the pattern, not the file**.
  `dirs` is already an app dependency (`Cargo.toml:40`).
- **Single writer is guaranteed**: one core per database — a second
  core boot (second daemon or REPL) fails `instance-locked`
  (`README.org` "One core per database"; `lockfile::InstanceLock`,
  `bootstrap.rs:71`). Daemon mode runs no editor thread. No history
  file is ever written by two processes.
- **Completion engine seams** (`src/completion.rs`): `Kind` enum at
  `:67-86` (Verbs/SubVerbs/Flags/Stacks/Tasks/Labels/Literals/None —
  which *source* answers a position); `candidates()` at `:608-669`
  filters + canonically sorts live entities (stacks `(order, id)`,
  tasks `(stack, order, id)` with `is_live()`, labels `!deleted` in id
  order) and hands titles to `live_titles()`; `live_titles()`
  (`:673-682`) prefix-matches case-sensitively and dedupes titles via a
  `BTreeSet`; `quoted()` (`:597-602`) builds `display` = bare word,
  `replacement` = `shell_words::quote(word) + " "`. The module-doc
  invariant at `:22-27` documents the dedupe rule and must be updated.
- **Selector semantics (the parity target)**: `resolve_task`
  (`commands/tasks.rs:487-533`) is a cascading selector — exact uuid →
  unique ≥8-char uuid prefix → exact live title; its `AmbiguousTask`
  payload lists **titles only**, so on a hard collision the error
  cannot disambiguate (exactly the gap uuid candidates fill).
  `resolve_stack` (`tasks.rs:61-96`): exact uuid → exact title (no
  prefix tier); `resolve_label` (`labels.rs:20-66`): same shape, id
  order. All three `*Id` types are `Uuid` newtypes with const
  `from_uuid`/`as_uuid` (`domain/src/ids.rs:17-28`); `uuid` is already
  an app dependency.
- **Menu text and inserted text are decoupled by the crate** (the load-
  bearing fact behind D9/D10, verified in the vendored
  `completion.rs:10-15`): `trait Candidate { fn display(&self) -> &str;
  fn replacement(&self) -> &str; }` — `display` is what the menu prints
  as its own rows below the prompt (the unix-path-completion listing);
  `replacement` is the only text that ever lands in the line. Menu
  content is never sent with the command. What rustyline *cannot* do:
  annotate the executed line with non-parsing text — the line must
  parse as a selector, and a title is by definition ambiguous, so the
  only always-exact insertable selector is the uuid (ghost-text
  `Hinter` annotations are prompt-preview-only and deferred by the
  WI-G plan). Default `CompletionType` is `Circular` — repeated Tab
  cycles the rows; `List` (type-to-narrow listing) also exists; both
  display the same `display()` rows.
- **Test seams**: the completion unit suite lives inline with
  `fresh_state`/`stack`/`task`/`label`/`kind_plan`/`titles_of` helpers
  (`completion.rs:1221-1268`); the assertion at `:1264-1267` pins the
  old "duplicate titles dedupe to one menu entry" behavior and is
  *rewritten* (deliberate semantic change, §2 D9 — not a weakening).
  Hermetic REPL tests inject `EditorIo` channels — the editor thread
  never starts there, so history file I/O is invisible to them (good:
  it must stay that way; no test touches a real data dir).
  `tests/cli_tty.rs` drives the real binary under a `script(1)` pty
  (`timeout script <binary> --config <tempdir>`, e.g. `:241-253`) with
  skip-when-`script`-missing guards; the temp config pins `db_path`,
  but **nothing isolates the data dir today** — see §2 D13.
- **Docs surfaces**: README.org's REPL section documents completion at
  `:216-219`; `CHANGELOG.md` carries an `[Unreleased] → Added` section.

## 2. Decisions (settled; D9 records the user's reviewed principle)

1. **D1 — No rustyline feature addition; no root `Cargo.toml` change.**
   `with-file-history` ships in v18's default set and the pinned
   `DefaultHistory` *is* `FileHistory` (§1). The dispatch's expected
   shared-file adjacency with the GUI slint pin therefore evaporates.
   Rejected: pinning `features = ["with-file-history"]` explicitly —
   redundant self-documentation at the cost of a pointless workspace
   diff (and the predicted rebase hot spot).
2. **D2 — The history backend type does not change.** The editor keeps
   constructing `DefaultHistory`; persistence is *added around* it via
   the `History` trait's `load`/`save` (`add` already happens at
   `:198`). Rejected: swapping in a custom `History` impl — the crate's
   own `FileHistory` already owns the format, locking, and permissions.
3. **D3 — History file location: `<data-dir>/taskboard/history`.**
   Follows the db-path precedent formula exactly
   (`dirs::data_dir()/taskboard/<file>`, §1), resolved through a pure
   `history_path(data_dir: Option<&Path>) -> Option<PathBuf>` plus a
   thin `dirs::data_dir()` wrapper at the call site — the
   `xdg_candidate` seam pattern, testable without env mutation.
   Rejected: deriving from the resolved `db_path`'s parent (couples the
   REPL's terminal affordance to `[storage]` config, breaks on bare
   relative db paths, and a db override is about *data*, not muscle
   memory); rejected: a config knob (zero new config surface; the chunk
   must not edit `src/config.rs` anyway). If the platform has no data
   dir (`None`): persistence off for the session, in-memory recall
   still works, one warning.
4. **D4 — 0600 permissions come from rustyline itself.** `save_to`
   `fchmod`s the file to owner-read-write on every save (§1); this plan
   adds no permission code and simply *asserts* the mode in a
   `#[cfg(unix)]` test. (Windows: crate no-op; nothing to assert.)
5. **D5 — Failure behavior: history degrades, never kills.** Load: a
   missing file is a silent fresh start (first run is the normal case);
   a corrupt or unreadable file logs one `tracing::warn` and keeps
   whatever parsed (partial entries are harmless recallable lines).
   Parent-dir creation failure (e.g. a regular file squatting on the
   directory path) downgrades `SessionHistory` to `path: None` —
   persistence off, in-memory history still on. Per-line save failures
   warn and are swallowed (`let _ =` semantics at the call boundary);
   the editor thread never panics and never exits early on a history
   error. History is a DX nicety, never correctness-bearing.
6. **D6 — Save cadence: full-rewrite `save` after each recorded line**
   (not `append`). Rationale: `append` is *poisoned* by a corrupt file
   (it re-reads the file and errors, §1) while `save` overwrites it —
   so `save` makes the first recorded line of the next session
   self-heal a corrupt file, which D5's degrade then survives. The
   single-writer guarantee (§1) makes rewriting safe; the cost is
   bounded by `max_history_size` (rustyline default 100 entries — a few
   KiB per line, negligible at human typing speed). Recorded trade-off:
   a SIGKILL mid-`save` can truncate the file; the corrupt-load degrade
   plus self-heal covers the aftermath. Rejected: exit-time-only save
   (a crash loses the whole session) and append-per-line (fails
   forever on the exact corrupt-file case D5 must survive).
7. **D7 — Admission policy unchanged, moved, not widened**: exactly
   today's rule — non-blank submitted lines are recorded; blank /
   whitespace-only lines are not; prompted ceremony answers (masked
   secret, confirm) never enter history (they bypass `readline` by
   construction, `repl.rs:193-196`). The blank-check moves from the
   call site into `SessionHistory::record` so the policy lives with the
   persistence concern. No new knobs (ignore-space/dedup keep rustyline
   defaults — unchanged behavior).
8. **D8 — No new `Kind` variant; the extension lives in the
   candidate generator.** The backlog's Proposed Approach says "a new
   `Kind` variant", but `Kind` maps *positions to sources* — uuid rows
   answer the same position from the same live snapshot, so a new
   variant would need a second-Tab or position convention that does not
   exist and would be dead routing. The substance of the backlog's own
   context line — "extend the live-title generator to emit
   uuid-disambiguated candidates" — is what this plan implements.
   Supersedes the backlog's Proposed Approach wording; agreed with the
   user in review (2026-10-15).
9. **D9 — A hard collision *replaces* the deduped title row with one
   title-first uuid row per entity** — display `"{title} - {uuid}"`
   (ASCII separator, per the review sketch), insert the uuid; unique
   titles keep today's single plain row, and the menu stays quiet for
   them. Settled in review around one principle (user, 2026-10-15):
   **titles are the identifier humans parse — the uuid is meaningless
   to users and may only ever be the distinguisher column, never the
   thing the human is asked to read first.** Every menu row therefore
   leads with the title; the uuid rides along as the disambiguator,
   and the menu rows themselves are display-only (never sent with the
   command, §1 Candidate fact). Why *replace* the plain title row
   rather than keep it: selecting a colliding title manufactures a
   *guaranteed* `Ambiguous*` error — the exact shape of the WI-G
   honesty rule that refuses to offer `daemon`/`--config` (completion
   plan §2.2: "the honest menu shows what a line can actually do").
   The `Ambiguous*` error remains reachable by typing the title by
   hand, which is what the error text honestly serves. Alternatives
   weighed in review and rejected: (i) *keep the title row alongside
   the uuid rows* — plants a trap row whose only outcome is the error
   the feature exists to escape; (ii) *disambiguate via an enriched
   `Ambiguous*` error instead of the menu* (error lists `title -
   uuid` rows, user re-runs with the uuid) — keeps the menu title-only
   but costs a failed round-trip, and the error payload lives in
   `commands/`, outside P6's dispatched file scope; it rides the §7
   backlog idea and may yet happen *in addition*; (iii) *display-only
   annotation of the executed line* — impossible in rustyline (§1).
   What cannot change without breaking the user principle or the
   resolver contract: the executed line carries only the uuid (a
   title on the line is by definition ambiguous). If a different menu
   display format is later preferred, it is a one-line change in the
   generator plus its display assertions.
10. **D10 — Insert the full uuid (36 chars), never a shortened prefix;
    display it in full too.** The exact-uuid tier of every resolver is
    stable even if new entities appear between Tab and Enter; the
    ≥8-char prefix tier can *become* ambiguous later (UUIDv7 timestamp
    prefixes collide within about a minute — documented in
    `resolve_task`, `tasks.rs:494-497`), which would turn a completed
    line into a future `Ambiguous*`. The display carries the full uuid
    as well: WYSIWYG — the executed line matches the chosen menu row
    verbatim, so nothing surprising appears on the line after
    selection (an abbreviated display label was considered and
    rejected for exactly that mismatch; wide rows are accepted).
11. **D11 — Stem matching stays title-prefix only, case-sensitive.**
    Uuid rows appear iff their *title* prefix-matches the stem, exactly
    like today's title rows. Uuid-text stem completion (paste a uuid
    fragment, Tab expands it) is rejected for this chunk: the backlog
    scopes uuid rows to colliding titles, and uuid-fragment matching
    would immediately beg the question for *unambiguous* entities too —
    a different feature; appended to the backlog (§7).
12. **D12 — The collision rule applies uniformly to Stacks, Tasks, and
    Labels.** All three kinds have the exact-uuid selector tier and the
    `Ambiguous*` outcome; parity is one rule, three generators.
13. **D13 — Test isolation is injection-first, and the pty harness must
    gain data-dir isolation.** Unit tests inject `Option<&Path>`
    tempdirs (the `xdg_candidate` pattern — never process-global
    `XDG_DATA_HOME` mutation, which races parallel tests). But once
    lines persist, every existing `cli_tty.rs` pty journey would append
    to the *developer's real* `~/.local/share/taskboard/history` (the
    temp config pins only `db_path`; history resolves independently,
    D3) — test pollution and a nondeterminism source (a dev's real
    history could surface inside a test session). Commit (a) therefore
    also adds `.env("XDG_DATA_HOME", <per-test tempdir>)` to every pty
    `Command` builder in `cli_tty.rs` (mechanical, ~4 sites; the pty
    suite is Linux/dev-CI where `dirs::data_dir()` honors it). This is
    a necessary test-hygiene consequence of feature (a), not scope
    creep — `tests/` files are not in any no-go zone.
14. **D14 — New private module `src/repl_history.rs`** (SPDX
    `AGPL-3.0-only` per AGENTS §6; crate-level `#![forbid(unsafe_code)]`
    inherited). One type with one responsibility (the persistence
    policy: where, what, and how degradeable); `repl.rs` keeps the
    loop/handshake only. No ADR edit (README + this plan record the
    decision; ADR 0011's scope is the threading model, which is
    untouched). CHANGELOG + README bullets ride the feature commits.

## 3. Scope

| Area | Deliverable |
|---|---|
| `src/repl_history.rs` (new) | Pure `history_path` + `SessionHistory` (`open`/`load`/`record`) over `&mut impl History`; full inline unit suite (tempdir-injected, no TTY, no env mutation): path formula, roundtrip across two sessions, corrupt-file degrade + self-heal, uncreatable-parent degrade, 0600 assertion (`#[cfg(unix)]`) |
| `src/repl.rs` | Editor-thread wiring only: `SessionHistory::open(dirs::data_dir()…)` before the loop, `load` after editor construction, `record` replacing the raw `add_history_entry` block at `:197-199`; doc-comment touch-ups |
| `src/completion.rs` | `live_titles` → collision-aware generator over `(uuid, title)` entries (§4.4); callers in `candidates()` map ids via `as_uuid()`; module-doc invariant `:22-27` rewritten; test suite rewritten/extended (D9 semantics, parity, ordering, quiet menu, quoting roundtrip) |
| `src/lib.rs` | `mod repl_history;` |
| `tests/cli_tty.rs` | `.env("XDG_DATA_HOME", …)` on every pty builder (D13); one two-session history-recall journey (skip-guarded, structural payload assertion) |
| Docs | README.org REPL section: history persistence sentence + collision-completion sentence update; CHANGELOG `[Unreleased] → Added` ×2 (one per feature commit); backlog: both entries ✅ DONE + two new out-of-scope ideas (commit 3) |

**Out of scope** (§7): hints/ghost text (WI-G deferral stands); uuid
stem matching; remote board-id completion; the `Ambiguous*` payloads
listing uuids (`commands/` surface — not this chunk's files); history
size/config knobs; any change to `commands/*`, `src/config.rs`,
`bootstrap.rs`, other crates, workflows, ADRs.

## 4. Design

### 4.1 `SessionHistory` (commit a)

The persistence policy in one type; every method is best-effort and
infallible *as an API* (degrade is internal, §2 D5):

```rust
pub(crate) struct SessionHistory {
    /// None = persistence unavailable this session (no data dir, or the
    /// parent dir could not be created); in-memory history still works.
    path: Option<PathBuf>,
}
```

- `history_path(data_dir: Option<&Path>) -> Option<PathBuf>` — pure:
  `data_dir.map(|d| d.join("taskboard").join("history"))` — the
  `default_db_path` formula with `history` for `taskboard.db`.
- `open(data_dir: Option<&Path>) -> Self` — resolve, then best-effort
  `create_dir_all(parent)`; failure (or `None` data dir) → `path: None`
  + one `tracing::warn`. No file is created here — `load` must not
  manufacture files; the first `record` creates the file.
- `load(&self, history: &mut impl History)` — `path` `None` → no-op;
  `Err(NotFound)` → silent fresh start; other `Err` → one
  `tracing::warn`, keep whatever parsed (rustyline's partial state).
  Never returns a `Result` — nothing at the call site may branch into
  session death.
- `record(&self, history: &mut impl History, line: &str)` — blank /
  whitespace-only → return (the admission policy, D7); `history.add`
  always (session recall even when degraded); if `path` is `Some`,
  `history.save(path)` — failure warns (D5/D6). The existing
  call-site comment about prompt noise moves here with the check.

### 4.2 Editor-thread wiring (commit a)

Inside `spawn_line_reader`'s thread closure only — the thread owns the
`Editor`, so it owns its history lifecycle; no signature changes
anywhere (`spawn_line_reader(state) -> EditorIo` is untouched, tests
inject `EditorIo` exactly as today):

```rust
// after the editor is constructed and the helper set (repl.rs:189):
let history = repl_history::SessionHistory::open(dirs::data_dir().as_deref());
history.load(editor.history_mut());
// the Ok(line) arm's block at :197-199 becomes:
history.record(editor.history_mut(), &line);
```

`spawn_line_reader`'s doc comment gains the persistence note (data-dir
file, best-effort, secrets never recorded — the oneshot answer path
bypasses this by construction).

### 4.3 pty isolation + recall journey (commit a)

- Every pty `Command` builder in `tests/cli_tty.rs` gains
  `.env("XDG_DATA_HOME", dir)` (D13) — precondition for the feature,
  not a nice-to-have: without it the *existing* journeys start writing
  real history. The journeys' configs already isolate db and config.
- One new journey (same harness, same skip guard): session 1 under a
  tempdir `XDG_DATA_HOME` runs `stacks list --json` and exits; session
  2 (fresh process, same tempdir) sends only Up-arrow + Enter — the
  recalled line must produce the same structural JSON payload shape
  session 1 produced (payload-parse assertion, never output-text
  matching). Proves the load→recall→execute wiring that unit tests
  cannot reach. Drop valve: if rustyline's Up-arrow under `script(1)`
  proves unreliable in this harness, stage 2 records a deviation and
  drops *this journey only* (never the isolation change) — the
  wiring is four lines and unit-reviewed; nothing else depends on it.

### 4.4 Collision-aware candidate generation (commit b)

`live_titles` becomes a grouping generator over `(uuid, title)` pairs;
the callers in `candidates()` already hold the ids (they sort by them)
and map with `as_uuid()`:

1. Collect prefix-matched `(title, uuid)` in the kind's canonical
   order (unchanged filters/sorts — parity rule untouched).
2. Count titles (`BTreeMap<&str, usize>`).
3. Walk the canonical list again: a title with count 1 → one plain
   `quoted(title)` row (today's exact shape); a title with count ≥ 2 →
   one uuid row per occurrence, `display = "{title} - {uuid}"` (title
   first — the identifier the human parses; the uuid is only the
   distinguisher, D9), `replacement = quote(uuid) + " "`, and **no**
   plain title row (D9). Group order = first encounter in canonical
   order (identical to today's dedupe emission order); within a group,
   canonical entity order (not uuid-lexical).

`Kind`, planning, routing, and every static kind are untouched (D8).
The module-doc invariant block (`completion.rs:22-27`) is rewritten to
state the new rule in place of the dedupe sentence.

## 5. Normative Shapes (signatures binding; bodies indicative)

```rust
// src/repl_history.rs (private module; SPDX AGPL-3.0-only)
pub(crate) fn history_path(data_dir: Option<&Path>) -> Option<PathBuf>;

pub(crate) struct SessionHistory { /* §4.1 */ }
impl SessionHistory {
    pub(crate) fn open(data_dir: Option<&Path>) -> Self;
    pub(crate) fn load(&self, history: &mut impl rustyline::history::History);
    pub(crate) fn record(
        &self,
        history: &mut impl rustyline::history::History,
        line: &str,
    );
}

// src/completion.rs — the generator seam (name indicative, shape binding):
// takes (uuid, title) pairs in canonical order, returns menu rows.
fn live_entries<'a>(
    entries: impl Iterator<Item = (uuid::Uuid, &'a str)>,
    stem: &str,
) -> Vec<Candidate>;
```

Glue note: `SessionHistory` is generic over `impl History` so unit
tests drive a bare `DefaultHistory` — no `Editor`, no terminal
(anything else re-imports the WI-G "no TTY in CI" guarantee).

## 6. TDD Implementation Order (branch `feat/app-repl-history-uuid-completion`)

Every commit standalone and gate-green; tests red before each
implementation slice.

1. **Commit 1 — `feat(app): file-backed REPL history at the data dir`**
   - Red: `history_path` unit rows (Some(base) → `base/taskboard/history`;
     `None` → `None`).
   - Red: session roundtrip — `open` on a tempdir, `record` two lines +
     one blank (skipped), then a *second* `DefaultHistory` +
     `SessionHistory::load` sees exactly the two lines in order
     (assert via `History::len` and `get(_, SearchDirection::Forward)`
     — never file-text assertions; rustyline's format is not our
     contract). Unix row: file mode after first save is exactly 0600.
   - Red: corrupt-file matrix — invalid-UTF-8 file: `load` returns
     normally (warn + partial), then `record` + reload roundtrips and
     the file is valid again (the D6 self-heal); v1-format file (no
     `#V2` header) loads as entries.
   - Red: degrade — a regular file squatting on `<base>/taskboard`
     makes `open` return the `path: None` session (observable:
     `record` still populates in-memory history, no file appears); no
     panic anywhere.
   - Green: implement `repl_history.rs`; wire `repl.rs` + `lib.rs`.
   - `tests/cli_tty.rs`: XDG isolation on all pty builders + the
     two-session recall journey (skip-guarded).
   - Docs: README history sentence; CHANGELOG Added line.
2. **Commit 2 — `feat(app): uuid completion candidates for colliding titles`**
   - Red (rewrite of `candidate_prefixes_are_case_sensitive_and_duplicates_dedupe`,
     `completion.rs:1249-1268`): case-sensitivity row unchanged; the
     duplicate-title row now expects **two uuid rows** — displays
     `format!("{title} - {uuid}")` (title first, D9) built from the ids
     the fixture helpers return, replacements `quote(uuid) + " "`, and
     the parity assertion `resolve_task(&state, uuid_str) == Ok(id)`
     for every offered row (the replacement is a selector that
     resolves exactly).
   - Red: new rows — stacks and labels collisions (labels assert the
     row *set* + parity, order-agnostic: label canonical order is id
     order and fixture ids are generated); mixed menu (one colliding +
     one unique title under one stem → plain row + uuid rows, group
     order = first encounter); quiet-menu row (unique titles → today's
     plain rows, no parenthesized displays — the existing
     `live_candidates_parity…` test already pins distinct-title output
     and stays green unchanged — no uuid text in any display); canonical-order row for tasks across
     stacks; quoting roundtrip
     (`shell_words::split(replacement.trim_end()) == [uuid_str]`).
   - Green: the grouping generator (§4.4), caller id-mapping,
     module-doc rewrite.
   - Docs: CHANGELOG Added line; README completion sentence update.
3. **Commit 3 — `docs(backlog): repl history + uuid completion done`** —
   both origin entries ✅ DONE with one-line notes; the two §7 ideas
   appended using the backlog's entry template.
4. **Full gates** (AGENTS §10) after every commit.

## 7. Out of Scope → Backlog (appended in commit 3, per AGENTS §7)

- **Uuid-fragment stem completion** (type/paste a uuid prefix, Tab
  completes the full uuid — for *any* live entity, not just
  collisions): rejected here (D11) because it changes the menu for
  unambiguous titles too, which the backlog scoped out.
- **`Ambiguous*` payloads listing uuids**: today the errors list titles
  only, which cannot disambiguate a hard collision; completion now
  offers the escape, but the typed error could too. It lives in
  `commands/` (not this chunk's files) — backlog idea for a later app
  round.

## 8. Docs & Bookkeeping

- **README.org** REPL section: one sentence — submitted lines persist
  across sessions in `<data-dir>/taskboard/history` (0600, best-effort;
  a corrupt file is warned about and rebuilt); the completion sentence
  gains: when live titles collide, the menu offers uuid-disambiguated
  rows that insert the uuid.
- **CHANGELOG.md**: `[Unreleased] → Added` — one line per feature
  commit (house precedent).
- **Backlog** (commit 3): both entries ✅ DONE + the two §7 ideas.
- **No ADR change** (D14); no workflow, version, or workspace-manifest
  change (D1).

## 9. Verification & Acceptance (dispatch §7)

- **Pure completion-engine tests** — structural candidate assertions,
  no TTY, no text assertions on error/status text (display-shape
  assertions are the product contract, as in the existing suite).
- **Hermetic history tests** — tempdir-injected data dir (never env
  mutation in unit tests, D13), permissions asserted (`#[cfg(unix)]`),
  corrupt-file survival asserted, zero real-home touches; the pty
  journey isolates `XDG_DATA_HOME` per process.
- **The quiet-menu invariant**: uuid rows appear only for hard
  collisions; unambiguous titles keep exactly today's menu (the
  untouched parity test pins it).
- **Gates** (AGENTS §10) green after each commit: fmt, clippy
  `--workspace --all-targets -D warnings`, nextest, doc-tests, deny
  (no tree change — trivially stable), llvm-cov (the new module is
  covered including both degrade arms — the squatting-file and
  corrupt-file tests reach them).
- **Both backlog entries ✅ DONE** in commit 3; dispatch acceptance
  met.

## 10. Risks & Mitigations

- **Up-arrow recall under `script(1)` may be flaky** (keymap/pty
  interaction): the journey is skip-guarded like its siblings and
  carries an explicit drop valve (§4.3); every behavior except the
  four-line wiring is unit-pinned without a terminal.
- **Existing pty journeys polluting the real data dir** once lines
  persist: the isolation change is *part of commit 1* and reviewed as
  a hard requirement, not an afterthought (D13).
- **Full-rewrite save truncates on SIGKILL mid-save** (D6 trade-off):
  the corrupt-load degrade plus first-record self-heal bounds the
  damage to "history rebuilt from the next session"; accepted for a DX
  nicety.
- **rustyline format/behavior drift across minors**: tests assert
  roundtrip + permissions through the public API only — never file
  text, never `#V2` details; a rustyline upgrade cannot break the
  suite on format grounds.
- **D9 changes a pinned assertion** (`completion.rs:1264-1267`): the
  test is rewritten to the new semantics — this is the backlog-driven
  behavior change itself, not a weakening; the review principle it
  implements is quoted in D9.
- **D8 supersedes the backlog's "new `Kind` variant" wording**:
  engineering reasoning recorded (§2); agreed with the user in review.
- **Rebase adjacency with the GUI lanes**: none — the predicted
  root-`Cargo.toml` hot spot dissolves under D1; remaining shared files
  (README/CHANGELOG/backlog) are trivial-adjacent per dispatch §1.4.
- **Windows**: permission assertion is `#[cfg(unix)]`; `fix_perm` is a
  crate no-op there and everything else is platform-neutral
  (`dirs::data_dir()` → `%APPDATA%`).

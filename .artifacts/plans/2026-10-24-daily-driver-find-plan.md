# [Plan] Daily-Driver Chunk 9 — Fuzzy Search: `find.rs` + Top-Level `find` + Palette

Stage-1 plan for chunk 9 of the daily-driver implementation round
(dispatch §10): the domain ranking core `find.rs` with its in-module
proptest suite behind the crate-swap firewall, the top-level
`taskboard find` CLI/REPL verb with the `FindRow` projection, and the
GUI jump-to palette (`Flow::Palette`, `ContextId::Palette`,
`search-palette.slint`, `IdTables.palette`, jump-to, the anchor
filter, the journey leg). Closes #348 (stage 3 posts the D-V9
amendment note: top-level verb, departing the issue's `tasks find`
wording).

Normative input, in MD-7 rank: implementation plan §2 DDF-14, §3
chunk 9, §4–§6; design doc `2026-10-19-views-finding-design-plan.md`
Part C (§3.8 D-V8 + the firewall paragraph, §3.9 D-V9 as amended,
§3.10 D-V10) + §5 decision/amendment record + §6 chunk mapping + §7
item 5 (the anchor-const home); dispatch plan §1 (execution model,
MD rules) + §10 (covers/acceptance/traps). No ACCEPTED ruling is
deviated from; every recorded decision below is either a plan-day
resolution of a detail the designs left open or an engineering
consequence of D-V8–D-V10, with the one prose-vs-prose tension
(archived tasks) settled by MD-7 rank in D1.

Precondition (MD-9) verified 2026-10-24: chunk 3 is contained in
`origin/main` — `ANCHOR_CARD_TITLE` lives at
`crates/taskboard-domain/src/appdata.rs:36` on `origin/main`
(cb02a38), re-exported at `lib.rs:59`, consumed presentation-side by
chunk 5's `is_anchor_task` (`ui-slint/src/presentation.rs:176`) and
CLI-side by the anchor-title guard (`app/src/commands/tasks.rs:429`).
Chunks 4–8 are also observed landed (fold, surfaces, rule verbs,
alarms, toast sink) — this chunk depends on none of them beyond the
shared base. The stage-1 worktree was created after
`scripts/worktree-doctor.sh` exited 0 (101G free, floor 25G); branch
`feat/domain-find` cut off `origin/main` cb02a38.

---

## 1. Verified baseline (symbols re-located at this plan's start; lines are plan-day references)

All re-read on this branch @ cb02a38.

Domain (`crates/taskboard-domain`):

- `state.rs` — `AppState { boards, stacks, tasks, labels:
  BTreeMap<Id, Entity>, reminders, sync, last_updated }` :25–46
  (BTreeMap = deterministic iteration; the determinism property's
  insertion-order clause is canonicality by construction).
  Canonical-ordering precedent: `(order, id)` in `sorted_tasks` :424
  (and `reminders.rs:674`) — `find`'s key follows the same shape
  discipline but is its own key (D3).
- `entities.rs` — `Board { id, remote, title, color, archived,
  deleted, clocks, remote_seen }` :351; `Stack { … board, title,
  order, archived, deleted … }` :373; `Task { … title, description,
  duedate, scheduled, done, stack, order, labels, effort,
  importance, urgency, archived, deleted … }` :396;
  `Task::is_live() = !deleted` :439 (does **not** test `archived`);
  `Task::is_open() = done.is_none()` :445.
- `ids.rs` — id newtypes over `Uuid` with derived `Ord` (byte
  order, deterministic) :16–17.
- `lib.rs` — module list :31–53; re-export block :58+ (`pub use
  days::day_window` is the re-export precedent for a single-function
  module); `pub mod test_support` gated
  `#[cfg(any(test, feature = "test-support"))]` :52.
- `test_support/` — `proptest_config(cases)` (house default 256 in
  `days.rs`), `timestamp_strategy`, `entities::task_strategy()`
  (arbitrary titles/flags incl. `archived`/`deleted`) — the proptest
  kit; no board/stack strategies exist (find's suite builds its own
  state strategies in-module, D5).
- `days.rs` — the in-module proptest precedent: `#[cfg(test)] mod
  tests` + `use crate::test_support::{proptest_config,
  timestamp_strategy}` :61–66.

App (`crates/taskboard-app`):

- `cli.rs` — top-level `Command` enum :29–132 (Connect, Login,
  Boards, Stacks, Tasks, Labels, Timers, Logout, Sync, Config,
  Status, Daemon, Gui); value-parser precedents `parse_color`,
  `parse_board_id`, `parse_due` :342+; no clap `ValueEnum` exists yet
  (D6 introduces the first).
- `commands/mod.rs` — `dispatch` (one arm per subcommand, flat) :608;
  `dispatch_line` (REPL/session; Connect/Login/Logout/Config/
  Daemon/Gui unreachable-document arm) :1348; `route_for_ipc`
  routing table :1779 (`Boards/Stacks/Labels/Tasks/Timers/Sync` ⇒
  `IpcOrLocal`) + its unit pin :1897.
- `commands/output.rs` — the row-projection doctrine (module doc
  :1–14): typed serde projections, never domain serde shapes;
  `TaskRow` :89; `LabelRow` lives in `labels.rs` instead (either home
  is precedented — `FindRow` goes in `commands/find.rs` per the
  impl-plan file list).
- `commands/labels.rs` — the read-verb precedent: `list` (boots core
  via `with_core`) + `list_on` (session seam reading
  `app.engine.shared_state().load_full()`) + `render_text_list`
  ("no labels" empty render) :53–263.
- `repl.rs` — `resolve_words` re-parses lines through
  `Cli::try_parse_from` :493 — the REPL inherits the verb for free
  once `dispatch_line` grows the arm; `the_line_table_parses_routes_
  and_refuses` :1113 pins examples.
- `completion.rs` — `CompletionSpec::from_cli()` walks the clap
  catalogue :337 (verbs/flags/positionals come for free);
  `flag_value_kind` :259 (value flags → `Kind`); `positional_kind`
  :278; `Kind::Literals(&[…])` for enumerable flag values :95;
  `the_spec_walk_matches_the_cli_catalogue` :1092 pins the walk.

UI (`crates/taskboard-ui-slint`; depends on `taskboard-domain`
already, `Cargo.toml:12`):

- `actions.rs` — `ContextId { Global, Board, StackDetail, TaskDetail,
  Settings, Menu, ViNormal, ViInsert }` :26 (+`name()` :36);
  `ActionId::PaletteSearch` :158 ("palette.search" :199); the
  reserved registry row (enabled `false`, GLOBAL, menu_rank 50,
  MenuGroup::ViewNav, "Not yet available — reserved") :376; the
  menu-cursor rows `MenuNext/MenuPrevious/MenuInvoke/MenuDismiss`
  with `contexts: &[ContextId::Menu]` :595–625; the pins
  `reserved_globals_are_present_and_disabled` :751 and
  `only_reserved_actions_stay_disabled` :773.
- `router.rs` — `UiEffect` :29 (OpenMenu…BoardSelect; no palette
  effect yet); `DispatchOutcome { Command, UiEffect, Disabled,
  NoTarget }` :95; the reserved arm
  `ActionId::AppUndo | AppRedo | AppHelp | CaptureQuick |
  PaletteSearch => Disabled` :227–232; `ActionRouter::dispatch` is
  flow-blind (pure id + `DispatchCtx`) :137.
- `view_state.rs` — `Flow { None, Create{stack}, CreateStack,
  EditTitle{task}, Move{task, target} }` :40; the buffer mirror
  (`buffer`, `set_buffer`) :86–174; `begin_create`'s opening :661
  (flow set + buffer cleared + `SetMode(Insert)` + `RefreshNow` —
  the palette opening copies this shape); `discard_edit` :738 (the
  only non-committing exit); `select_board(&state, board)` :382
  (board + selected_stack + focused_task = first task);
  `enter(StackDetail)` snapshots `subject_stack` :317;
  `reconcile` ends subject-bound flows :817 (a subject-free
  `Flow::Palette` needs no arm — falls to `_ => {}`).
- `shell.rs` — `chain()` :54: menu open ⇒ `[Menu]` (total mask);
  Insert ⇒ `[ViInsert, Global]`; else `[view, ViNormal, Global]`.
  The chain-flip precedent the palette copies.
- `keymap/mod.rs` — `visible_chain` stops at the first context in
  `masks` :69–81; `presets_mask_only_the_menu` pins
  `masks == [Menu]` :600. `keymap/presets.rs` — the three presets
  bind `palette.search`: Standard `Ctrl+F` (Global) :71, Emacs
  `Ctrl+s` :123, Vi `/` (ViNormal) :204; the menu-cursor rows per
  preset :98–101/:157–161/:209–212 (vi binds j/k in **Menu**).
- `adapter.rs` — `Adapter { view_state, state: Arc<ArcSwap<AppState>>,
  tables, models, … }` :66+; keystroke-mirror callbacks
  (`on_board_create_edited` etc. → `set_buffer`) :1610+; `refresh`
  (full `push`) :345; `apply_effect` handles the five menu effects
  menu-specifically (OpenMenu/InvokeSelected/CloseMenu/MenuBack/
  MenuNext|MenuPrevious) and routes everything else through
  `interpret` :418–475, whose `(Flow, UiEffect)` table is the
  interpretation seam :460–520; `sync` mirrors shell state onto
  `KeyboardState` :127.
- `models.rs` — `push(window, state, vs, keymap, chain, now,
  models) -> IdTables` :213 (one refresh pass; r2 D9: rows diffed
  onto persistent handles, stable addressing); `IdTables` :39
  (indices-back tables; `board_task`/`stack_task` accessors);
  `BANNER_ROW_CAP = 3` :34 + the cap/overflow pattern :649–674.
- `ui/app-window.slint` — `KeyboardState` global :53; the overlay
  mount pattern: intercepting `TouchArea` + `if (KeyboardState.
  menu-open): ActionMenu { … }` :235–245; view mounts keyed on
  `current-view` :170+; window-root `in-out property` + `callback`
  plumbing per view :170–199.
- `ui/compositions/action-menu.slint` — L2 composition sibling;
  `export struct MenuEntryModel` declared in the composition,
  imported into app-window :14 (the `PaletteRowModel` precedent).
- `ui/widgets/text-field.slint` — the L1 single-line field:
  `edited(string)`/`accepted(string)`, self-focusing on init :50+;
  "the insert-mode chain binds nothing printable" — printables reach
  the focused `TextInput` only because the capture layer rejects
  them (`Resolution::Unhandled` ⇒ intercept returns false) :19–21.
- `ui/widgets/list-row.slint` — `ListRow { text, meta, tone,
  emphasis }` — the result-row primitive (title + meta, emphasis =
  the selection ring) :9.
- `tests/journeys.rs` — the harness: `press(ActionId)` (resolves the
  chain-visible binding), `stroke`, `type_text` (through
  `press_stroke`, so the capture layer sees every char),
  `assert_commands`, `board_shape`, `emphasized_card` :110–200;
  the anchor-hiding journey `the_anchor_card_is_hidden_from_the_
  board` :1847 (plants `ANCHOR_CARD_TITLE` + a trailing-space
  near-miss — the palette leg's filter seed copies this);
  `fixture::golden_state` ("task-N" titles, one board) and
  `TwoBoardFixture` ("board one"/"board two").
- Snapshot corpora: `tests/snapshots/keymap_snapshots__registry.snap`
  and `…__resolved_preset_keymaps.snap` — this chunk's one
  deliberate insta pass (DF-9).

---

## 2. Shape of the chunk

Four production surfaces, one journey:

1. **Domain `find.rs`** — the pure, total, deterministic ranking
   core: the four contract types, the private classifier/rank
   internals, the in-module proptest suite, lib re-exports.
2. **App `find` verb** — top-level `CliCommand`-family variant
   `Find`, `commands/find.rs` (`FindRow` + render), `dispatch` +
   `dispatch_line` arms, `route_for_ipc` row, completion map entry.
3. **UI palette wiring** — registry flip + `ContextId::Palette` +
   cursor-action context widening, `Flow::Palette` + view-state
   methods, `UiEffect::OpenPalette` + router arm, shell chain head +
   preset rows + masks growth (the snapshot regen).
4. **UI palette overlay** — `search-palette.slint`, the mount, the
   per-keystroke call path, `push_palette` + `IdTables.palette` +
   the anchor filter + the 8-row cap, jump-to.
5. **The journey leg** (+ the CHANGELOG line).

Commit ladder (each standalone, rebase-merge hygiene):

| # | Subject | Carries |
|---|---|---|
| 1 | `docs(plan): fuzzy search core, find verb, and palette` | this plan |
| 2 | `feat(domain): the find ranking core` | `find.rs` (types + internals + in-module suite), `lib.rs` module + re-exports |
| 3 | `feat(app): the top-level find verb` | `cli.rs` variant + `FindType`, `commands/find.rs` (+ tests), `commands/mod.rs` three arms, completion entry (+ spec-walk test growth), repl line-table row |
| 4 | `feat(ui-slint): the palette context and flow` | `actions.rs` (flip + `ContextId::Palette` + widening + tests), `router.rs` (`OpenPalette` + arm), `view_state.rs` (`Flow::Palette` + methods + tests), `shell.rs` (chain head), `keymap` (preset rows + masks + contract-test growth), snapshot regen part 1 |
| 5 | `feat(ui-slint): the search palette overlay` | `search-palette.slint`, `app-window.slint` mount + plumbing, `models.rs` (`push_palette` + `IdTables.palette` + cap + anchor filter + tests), `adapter.rs` (interpret arms + keystroke path + sync), snapshot regen part 2 (resolved presets) |
| 6 | `test(ui-slint): the palette journeys` | `tests/journeys.rs` leg; CHANGELOG Unreleased line |

The insta pass spans commits 4–5 because each changes snapshot
inputs (4: registry/contexts; 5: preset rows); DF-9's "one
deliberate pass" is per chunk, not per commit — no `.pending` ships
either way.

---

## 3. Decisions (every open question settled here)

### D1 — scope predicates (and the archived-tasks reading)

Per-kind predicates, fixed:

| Kind | Predicate | Rationale |
|---|---|---|
| boards | `!deleted && !archived` | DDF-14 "live boards/stacks (unarchived)"; the `live_boards` precedent (`presentation.rs:163`) |
| stacks | `!deleted && !archived` | same wording, same shape |
| tasks | `is_live()` = `!deleted` | DDF-14 "tasks `is_live()` (done included)" — `is_live` is the named symbol and it tests only the tombstone |

The one prose tension, settled by MD-7 rank: design §3.8's
illustrative doc-comment says "tombstones and archived entities are
never hits" while the same section's scope bullet and DDF-14 (top
rank) assign tasks exactly `is_live()`. Reading: the doc-comment's
"archived" clause is the boards/stacks qualifier (where the design
puts "live-and-unarchived"); **archived and done tasks ARE hits**
(finding yesterday's finished item is the stated point of including
done; an archived task is equally findable, and the jump simply
lands on it). A task whose *stack* is archived/deleted still hits —
the task itself is live; the row's context line resolves what it
can. Labels are not searched (title-only over the three entity
kinds; label search is backlog with description search).

**The anchor task is NOT excluded in the domain** (D-V8: "the
anchor task is not excluded here — hiding is presentation-side");
the palette row mapping filters it (D11). The CLI does **not**
filter it either — recorded decision: the normative anchor-filter
scope is the palette's result rows (design D-V10 + the impl-plan
file list both place it there only), and the CLI verb is the
unfiltered scripting bridge whose ids are the point; a user who
searches a subsequence of `[taskboard] app-data` gets the anchor's
id, harmlessly and honestly. If the wave wants CLI hiding too, that
is a one-line follow-up on the same presentation helper, not this
chunk's scope.

### D2 — the classifier: exact class rules

All matching is over `str::to_lowercase` (std, Unicode-aware,
deterministic — the design's own folding choice) of both title and
query, computed once per title per call. **The query is trimmed
first**: `query.trim()`; empty-after-trim ⇒ the empty-query floor
(no hits). Leading/trailing whitespace in a search box is
accidental; interior whitespace is meaningful.

Classes, best-first, decided in this order (the classifier returns
the best attainable class — class monotonicity):

1. **Exact** — `folded_title == folded_query`.
2. **Prefix** — `folded_title.starts_with(folded_query)` (a prefix
   is the word-start at index 0, promoted).
3. **WordStart** — there exists an index `i > 0` with
   `folded_title[i..].starts_with(folded_query)` where the char
   immediately before `i` satisfies `!c.is_alphanumeric()`. The
   design's "after whitespace/punctuation" generalized to its
   char-class essence: whitespace, punctuation, symbols, underscore
   all qualify as boundaries (`foo_bar` ⇒ `bar` is a WordStart).
4. **Substring** — `folded_title.contains(folded_query)`.
5. **Subsequence** — the folded query's chars occur in the folded
   title in order (greedy two-pointer), spread allowed.

Otherwise: no hit. One entity ⇒ at most one hit (one title, one
classification). Empty titles can never match a nonempty query
(all five classes require the query to occur). No whitespace
normalization beyond the fold and the query trim — Exact is folded
byte-equality, nothing more (the anchor near-miss jurisprudence:
`"[taskboard] app-data "` ≠ the anchor title).

### D3 — the ranking key and the totality tiebreak

```rust
(MatchQuality, usize /* title_len */, SearchTarget)
```

sorted ascending; **no numeric score exists anywhere** (P7, the
dispatch trap). Details:

- `MatchQuality` derives `PartialOrd, Ord` — declaration order is
  rank order (the `Importance`/`Urgency` D-F1 enum-ordering
  precedent); ascending sort puts `Exact` first.
- `title_len` = `title.chars().count()` (human length; byte length
  would rank a short accented title below a longer ASCII one —
  recorded choice, both are deterministic), ascending
  (shorter titles first).
- **`SearchTarget` derives `PartialOrd, Ord`** — declaration order
  `Task < Stack < Board`, each variant comparing its inner id
  newtype (Uuid byte order). This *is* the "canonical id" of the
  mixed-kind search space: the kind tag makes the id tiebreak
  structurally total — within one kind, ids are unique (BTreeMap
  keys); across kinds, the variant tag discriminates. The totality
  property (output strictly ordered, no duplicate targets) holds by
  construction and is pinned by the proptest (D4).

### D4 — the property catalogue: contract-shaped vs algorithm-shaped

All eight named properties live in `find.rs`'s own `#[cfg(test)]`
module (the firewall, D5), each tagged `// contract-shaped` or `//
algorithm-shaped` so a future crate swap knows which must survive.
House config `proptest_config(256)`; states built by in-module
strategies over `test_support::task_strategy` + small board/stack
builders (none exist yet; they are test-module-local, part of the
suite that moves with a swap).

**Contract-shaped (7) — survive any implementation:**

1. **Determinism** — `find(s, q) == find(s', q)` where `s'` inserts
   the same entities in a different order (BTreeMap canonicality),
   and `find(s, q) == find(s, q)` on a second call (pure function).
2. **Case-fold invariance** — for a query `q` and `q'` = `q` with
   ASCII alpha casings flipped, `find(s, q) == find(s, q')`.
   Recorded subtlety: full Unicode case-folding is not an
   involution (`ß` ⇄ `SS`), so the property flips **ASCII** case
   only — an honest floor, not a ceiling on the folding itself.
3. **Empty-query floor** — `find(s, "") == []` and
   `find(s, " \t ") == []` for every state (floors on garbage
   input; no ceilings).
4. **Exact-title invariant** — plant one entity titled
   `q.to_lowercase()` into an arbitrary state: it hits with quality
   `Exact`; it precedes every non-`Exact` hit; and the first hit's
   quality is `Exact`.
5. **Growth monotonicity** — for an added entity `e`,
   `find(s ∪ e, q)` restricted to `s`'s entities equals
   `find(s, q)` (adding an entity never removes or reorders
   another's hit).
6. **Type-filter partition** — for each kind, filtering
   `find(s, q)` by `SearchTarget` kind yields an order-preserving
   subsequence of the full output (the caller-side `--type` lens;
   the property pins ranking stability under restriction).
7. **Totality** — the output's `(quality, title_len, target)` keys
   are strictly increasing and no target appears twice (the kind
   tag + id uniqueness make it strict; the proptest pins the sort).

**Algorithm-shaped (1 + units) — internal, may move/die with a
swap:**

8. **Class monotonicity** — when the folded title contains the
   folded query, a hit exists with quality `≤ Substring` (classes
   are refinements; the classifier returns the best class). The
   firewall paragraph names exactly this class-refinement invariant
   as the algorithm-shaped exemplar. Beside it, small hand-built
   classification unit tests pin one example per class (and one
   per non-class: no-hit) — algorithm-shaped by the same tag.

### D5 — the firewall's module layout

One file, `crates/taskboard-domain/src/find.rs`; `lib.rs` gains
`pub mod find;` and `pub use find::{MatchQuality, SearchTarget,
SearchHit, find};` (the `days::day_window` re-export precedent).

- **Public = the contract, exactly four items**: `find`, `SearchHit
  { target: SearchTarget, quality: MatchQuality }`, `SearchTarget
  {Task(TaskId), Stack(StackId), Board(BoardId)}`, `MatchQuality
  {Exact, Prefix, WordStart, Substring, Subsequence}`. No fifth
  item — no `kind()` accessor, no score type, no config struct: the
  CLI's `--type` matches on the enum directly (app-side), and a
  future fuzzy-matcher crate maps its numeric scores into
  `MatchQuality` inside this module (or the enum grows a variant —
  an in-module change).
- **Private**: the fold helper, `classify(folded_title,
  folded_query) -> Option<MatchQuality>`, the subsequence check,
  the boundary check, the per-kind entity walks, the sort. Nothing
  leaks; consumers read the ranked order and hit kinds only.
- **The property suite lives in this file's own test module** so it
  travels with a swap (D4's tags carry the contract/algorithm
  split).
- SPDX `MIT OR Apache-2.0` (domain crate); `#![forbid(unsafe_code)]`
  is crate-level already.

### D6 — the CLI verb: shape, validation, `FindRow`, exit codes

```
taskboard find <QUERY> [--type task|stack|board] [--limit N]
```

- `cli.rs` gains the `Command::Find` variant, placed after `Labels`
  and before `Timers` (the content-read verb family:
  boards/stacks/tasks/labels/find; help order is the only effect).
  Fields: `query: String` (positional, exactly one), `kind:
  Option<FindType>` with `#[arg(long = "type")]` (field name `kind`
  avoids the keyword; the long flag is `--type` exactly as
  designed), `limit: usize` with
  `value_parser = clap::value_parser!(usize).range(1..)`.
- **`--type` is a clap `ValueEnum`** (`FindType { Task, Stack,
  Board }`, kebab-case values) — the trap "validated loosely" is
  met by construction: an unknown word is a clap usage error (exit
  2), never a runtime string compare. This is the workspace's
  first `ValueEnum`; it lives in `cli.rs` with the other parse
  types.
- **`--limit N ≥ 1`** via the ranged value parser (0 is a usage
  error — "print nothing" is not a meaningful invocation);
  truncation applies **after** ranking (the design's scripting
  convenience).
- `commands/find.rs` (new): `FindRow { kind: String ("task"|
  "stack"|"board"), id: String (uuid), title: String, context:
  Option<String> }` — serde `Serialize` + `PartialEq` for tests;
  context = `"<stack> › <board>"` for task hits, `Some("<board>")`
  for stack hits, `None` for board hits (stack/board names resolved
  through the snapshot; a dangling parent renders `None` — data
  degradation, not an error). The row fields are the contract
  (D-F5 `TaskRow` stance); **no quality column** — D-V9 names
  exactly kind/id/title/context and quality stays inside ranking.
- `find(config, store, …)` boots via `with_core` (the `labels::
  list` precedent); `find_on(app, query, kind, limit) -> Vec<
  FindRow>` is the session seam: `shared_state().load_full()`,
  domain `find`, kind filter (match on `SearchTarget`), truncate,
  project. Infallible after boot — **exit 0 on every well-formed
  query**; no hits is the empty table, not an error (the `List`
  precedent). `render_text_list` renders tab columns
  `kind\tid\ttitle\tcontext` (context column empty for boards);
  the empty render is a "no hits" line (the "no labels"
  precedent). Row text is presentation glue — untested (AGENTS §5).
- `dispatch`: one arm → `find::find(...)` + emit (json or text),
  `0`. `dispatch_line`: `Command::Find { .. } => { find_on +
  emit }` — a pure read, no ceremony interception. `route_for_ipc`:
  **`IpcOrLocal`** (the Boards/Stacks/Tasks read family; its unit
  pin grows the row).
- REPL: nothing new to parse (clap reuse); `resolve_words` unit row
  `find needle --type board --limit 3` parses to the variant.
- Completion: `flag_value_kind` grows `"type" => Kind::Literals(
  &["task", "stack", "board"])`; the `query` positional is
  `Kind::None` (free text — it is not a selector); `--limit`
  completes nothing (numeric). `the_spec_walk_matches_the_cli_
  catalogue` grows the find node (positional + two flags + the
  literal set).

### D7 — where the find call happens (the anti-model respected)

`resolve_task` errors on ambiguity; `find` ranks — they compose but
never merge: `find_on` never calls `resolve_task`, and no resolve
path starts ranking. The palette (D10) and the CLI both call the
one domain `find` — **one ranking, many lenses**; no consumer-side
matching exists anywhere (the dispatch trap).

### D8 — the palette's flow/context/cursor wiring

- **Registry**: `PaletteSearch` flips `enabled: true`; description
  becomes a real one ("Opens the jump-to search palette over all
  boards."); `only_reserved_actions_stay_disabled` shrinks its
  expected set to `AppUndo/AppRedo/AppHelp/CaptureQuick`.
- **`ContextId::Palette`** joins the enum, `name() = "palette"`.
- **`Flow::Palette { selected: usize }`** joins `Flow` (the
  design's own shape — the cursor lives in the flow variant, not
  the shell). View-state methods:
  - `open_palette(&mut self) -> Vec<FlowOutcome>` — the
    `begin_create` opening: `flow = Palette { selected: 0 }`,
    buffer cleared, `[SetMode(Insert), RefreshNow]`.
  - `palette_cursor(&mut self, direction: i32)` — wrap over the
    visible row count (the `wrapped_index` ring-walk primitive,
    `menu.*` semantics; inert at 0 rows); `RefreshNow`.
  - `close_palette(&mut self)` — the `discard_edit` shape: flow →
    None, buffer cleared, `SetMode(Normal)` + `RefreshNow`. Esc /
    `MenuDismiss` / stale-target jumps all end here.
  - `palette_jump(&mut self, state: &AppState, target:
    SearchTarget) -> Vec<FlowOutcome>` — the D12 matrix; ends the
    flow like `close_palette`.
  - Query-change cursor reset (D10) is a field write on the flow
    variant (`selected = 0`) performed by the keystroke path — not
    a fourth method.
- **`reconcile` needs no Palette arm** (no subject entity; falls to
  `_ => {}`) — background state changes re-rank on the next
  refresh/keystroke instead of closing the palette.
- **Router**: `UiEffect::OpenPalette` joins the enum;
  `ActionId::PaletteSearch => UiEffect(OpenPalette)` replaces its
  membership in the reserved `Disabled` arm. The router stays
  flow-blind (D16's architecture unchanged).
- **Shell chain + masks** (the menu-open chain-flip precedent, one
  more context): `ShellState` grows `palette: bool` (mirrored by
  the adapter's `sync_view_context` from
  `matches!(vs.flow(), Flow::Palette { .. })`); `chain()` gains,
  after the menu branch and before the insert branch:
  `if self.palette { return vec![Palette, ViInsert, Global]; }`.
  `Preset::masks()` grows to `&[ContextId::Menu, ContextId::Palette]`
  — `visible_chain` stops at `Palette`, so the palette is a total
  mask exactly like the menu: cursor/Return/Escape resolve in
  `Palette`; `app.menu`/`palette.search` itself are masked while
  open (no re-trigger, no menu-over-palette); **printables are
  unbound everywhere and reach the focused `TextInput`** through
  the ordinary `Unhandled` fall-through (the `text-field.slint`
  discipline — same mechanism as insert mode). The contract test
  `presets_mask_only_the_menu` is renamed/rewritten to pin the two
  modal contexts.
- **Cursor actions widened**: the four `menu.*` rows' contexts
  become `&[ContextId::Menu, ContextId::Palette]` (registry
  widening, four rows — the design's "modal-list cursors are
  generic"; no relabeling, P7).

### D9 — `search-palette.slint` (L2) and the mount

A new composition, `ActionMenu`'s sibling, `ListRow`'s consumer:

```slint
export struct PaletteRowModel { text: string, meta: string, emphasis: bool }
export component SearchPalette inherits Rectangle { … }
```

- Content: one `TextField` (placeholder "Search…", self-focusing on
  init — the widget already does this), a column of `ListRow`s over
  the model (`text` = title, `meta` = the kind + context line
  assembled Rust-side, `emphasis` = `index == selected`), and an
  overflow caption row (`+N more`) shown when the count says so.
  No slint-side logic (the dumb-client rule): no filtering, no
  ranking, no cap arithmetic — only `index == root.selected-index`
  for the highlight.
- Mount in `app-window.slint` after the menu host, same pattern:
  intercepting `TouchArea` (`clicked => close`) + `if
  (root.palette-open): SearchPalette { … }`, centered, width
  bounded like the menu panel (min 320·scale, cap 560·scale), height
  by content. Plumbing: window-root `in-out property <bool>
  palette-open`, `<int> palette-selected-index`, `<[PaletteRowModel]>
  palette-model`, `<int> palette-overflow`, and `callback
  palette-query-edited(string)` (window properties, the board-model
  precedent — `KeyboardState` stays keyboard-echo state). The field
  binds `edited(text) => root.palette-query-edited(text)`.
- No kiosk surface, no flow-hint row (the overlay is its own
  affordance; `FlowHintModel` stays Move-only), no tooltips.

### D10 — the per-keystroke call path (no debounce)

1. Slint `TextInput.edited` → composition → window callback
   `palette-query-edited(text)`.
2. Adapter callback: `view_state.set_buffer(text)` (the D9-rule-1
   mirror); if the flow is `Palette`, reset `selected = 0` (every
   keystroke re-ranks — a stale mid-list cursor on a fresh ranking
   is a lie).
3. The adapter runs the palette pass **targeted**, not a full
   `refresh()`: `state.load()` (lock-free), domain `find(state,
   vs.buffer())`, anchor filter (D11), cap 8, clamp `selected`,
   `push_palette(window, …)` onto the persistent row handle (the
   r2 D9 row-diff discipline — no fresh `VecModel` per keystroke),
   then `finish_mutation`-style sync of the open/selected/overflow
   properties.
4. The refresh path stays authoritative for passive freshness:
   `push()` calls the same `push_palette` when the flow holds, so a
   background sync re-ranks the open palette on the next repaint
   without closing it (D8's no-reconcile-arm corollary). One
   function, two callers — the ranking never exists twice.

Board scale needs no debounce (D-V10's explicit call; no
speculative machinery lands).

### D11 — `IdTables.palette`, the anchor filter, the cap

- `IdTables` gains `pub palette: Vec<SearchTarget>` — indices-back,
  index-aligned with the visible row model (D2 discipline: ids
  never cross the seam; `InvokeSelected` resolves
  `tables.palette[selected]`).
- **Row pipeline, in order**: domain `find` (unfiltered — the
  domain stays convention-blind) → **anchor filter** (drop task
  hits whose task resolves to `title == ANCHOR_CARD_TITLE` —
  `presentation::is_anchor_task` reused verbatim; boards/stacks
  never carry the anchor title, so the filter is task-kind-only by
  construction) → **cap 8** (`PALETTE_ROW_CAP: usize = 8` in
  `models.rs` beside `BANNER_ROW_CAP`) → rows + `tables.palette` +
  `overflow = filtered_len.saturating_sub(8)`. Filter before cap so
  the anchor never occupies one of the 8 visible rows and the
  overflow counts real candidates. Cursor clamps into range after
  the cap (a refresh that shrinks rows keeps the nearest position;
  only a query change resets to 0 — D10).
- `push_palette` is pure-deterministic like the rest of `push`
  (same inputs ⇒ same rows), unit-tested at the `models.rs` layer
  with hand-built states (anchor planted, >8 matches planted).

### D12 — the jump-to matrix (Activate = `MenuInvoke` in palette)

`palette_jump(state, target)`, resolved against the live snapshot:

| Hit | Transition |
|---|---|
| Task | `select_board(state, task's stack's board)`; `focused_task = task`; view `Board` (the card's `emphasis` renders) |
| Stack | `select_board(state, stack's board)`; `selected_stack = subject_stack = stack`; `focused_task = first_task_of(stack)`; view `StackDetail` |
| Board | `select_board(state, board)`; view `Board` |

Every path ends the flow (`close_palette`'s ending: flow None,
buffer cleared, `SetMode(Normal)`, `RefreshNow`). The flow always
ends — Esc closes without effect, Activate closes with the jump;
there is no state in which the palette survives its own
activation. **Stale target** (the entity died between push and
invoke, or the row count shrank past `selected`): the jump is
inert — close only, no retarget (the `view.inbox`
quietly-no-op posture; a dead id must not crash or partially
retarget). Selection-follows-focus retargets `ViewState.board`
through `select_board` exactly as Q8/D-V3 §7-6 ruled for palette
jumps.

### D13 — preset rows per preset (existence and resolvability)

The design fixes existence + resolvability; the strokes are the
tunable. Rows added (binding context `ContextId::Palette`):

| Preset | Bindings |
|---|---|
| standard | `menu.next` Down, `menu.previous` Up, `menu.invoke` Return, `menu.dismiss` Escape |
| emacs | `menu.next` Ctrl+n, `menu.previous` Ctrl+p, `menu.invoke` Return, `menu.dismiss` Ctrl+g **and** Escape |
| vi | `menu.next` Down, `menu.previous` Up, `menu.invoke` Return, `menu.dismiss` Escape — **no j/k** |

Recorded reason for vi's divergence from its Menu rows: the palette
has a text field; j/k are printables that must reach it (binding
them would make typing `j` move the cursor instead). Arrows are the
vi preset's palette vocabulary. Return is bound in `Palette`
(head context) so the capture layer consumes it before the field's
`accepted` — Activate is jump-to, never commit-field. The
`resolved_preset_keymaps` snapshot regenerates (the deliberate
pass). The registry contexts widening (D8) rides the same corpus.

### D14 — the adapter's effect interpretation (where the palette arms live)

`apply_effect`'s five menu-effect arms gain flow-first branches
(the menu cannot be open while the palette holds — `Palette` masks
`Global`, so `app.menu` is unresolvable; ordering is still
defensive):

- `OpenPalette` (new arm) → `interpret(window, effect)` whose table
  gains `(Flow::None, OpenPalette) => view_state.open_palette()`;
  any other flow: inert `Vec::new()` (a palette over a move/edit
  flow is not a thing v1 allows — the modal discipline).
- `MenuNext | MenuPrevious` → palette branch:
  `view_state.palette_cursor(±1)` bounded by the visible row count,
  then the palette properties sync; else the menu path (unchanged).
- `InvokeSelected` → palette branch: resolve `tables.palette[
  selected]`, `view_state.palette_jump(&state, target)`, apply
  outcomes, close; else the menu path (unchanged).
- `MenuBack | CloseMenu` → palette branch: `close_palette()`
  (Esc/dismiss/mouse-out all mean close-without-effect — the
  `discard_edit` shape); else the menu paths (unchanged).
- `sync` grows the palette mirror: `set_palette_open(shell.palette)`
  + selected index; `sync_view_context` mirrors the flow bit into
  `shell.palette`.

### D15 — the journey leg (+ the CLI/REPL test matrix)

One journeys test, `the_palette_searches_and_jumps_across_boards`
(working name), on `TwoBoardFixture` + planted extras, structural
assertions only:

1. Plant: a uniquely-titled needle task ("zebra needle") in board
   two's stack; an anchor-titled live task; a decoy sharing the
   needle's board-one prefix.
2. `press(ActionId::PaletteSearch)` (enabled; resolves Ctrl+F in
   the default preset) → assert `palette-open` true, row model
   empty (empty-query floor, end to end).
3. `type_text` a needle prefix that matches needle + decoy (+ the
   anchor title's shared substring) → assert row count = matches
   **minus the anchor** (the filter, structurally: the anchor task
   is live and matching, so its absence from the count is the
   filter's proof); assert `palette-overflow` counts planted
   beyond-8 rows when >8 non-anchor matches exist (a second leg
   seeds 9+ matching tasks).
4. `menu.next` stroke (Down) → selected index reads 1.
5. `menu.invoke` (Return) → assert `current-view` = Board, the
   board-bar selected segment = board two's index, and
   `emphasized_card()` = the needle's (column, row) — the jump's
   full retarget, no text matched anywhere.
6. Re-open, `stroke(Escape)` → palette closed, view/focus
   unchanged, `assert_commands(&[])` throughout (pure view play —
   the palette dispatches nothing to the engine).

CLI/REPL tests (app crate): `cli.rs` parse units (`find` with
each flag combination parses; `--type bogus` and `--limit 0` are
usage errors — clap-level, typed); `commands/find.rs` units over
`find_on` with hand-built states (ranking order, kind filter,
limit-after-rank, context strings, empty-query empty rows, the
exact-title-first invariant at the projection layer);
`dispatch_routing.rs` grows the `IpcOrLocal` row; `cli_verbs.rs`
(or a sibling) gains one end-to-end `find` through `dispatch` with
a standalone core (exit 0, rows emitted — json shape pinned by
`#[derive(PartialEq)]` row equality, not text). The domain layer
owns matching semantics; these own routing/projection only (one
behavior, one test, lowest layer).

### D16 — bookkeeping

- CHANGELOG `Unreleased › Added`: one bullet — fuzzy search
  (#348, chunk 9): the pure domain ranking core
  (`find.rs`, class-ordered subsequence matching, no scores), the
  top-level `taskboard find` verb + REPL line, and the GUI
  jump-to palette (Ctrl+F / Ctrl+s / `/`), the anchor card hidden
  from palette rows.
- No tracker edits (MD-4). **Stage 3** posts the D-V9 amendment
  note on #348 (top-level `find`, departing the issue's
  `tasks find` wording) and the PR body carries `Closes #348`.

---

## 4. Implementation inventory (file-by-file)

| File | Change | ~Size |
|---|---|---|
| `domain/src/find.rs` (new) | contract types + private classifier/rank + in-module suite (8 properties + class units) | +~380 |
| `domain/src/lib.rs` | `pub mod find;` + re-exports | +6 |
| `app/src/cli.rs` | `Command::Find` + `FindType` ValueEnum + flags | +30 |
| `app/src/commands/find.rs` (new) | `FindRow`, `find`/`find_on`, render, units | +~170 |
| `app/src/commands/mod.rs` | dispatch + dispatch_line + route rows | +30 |
| `app/src/completion.rs` | `--type` literals + spec-walk test growth | +14 |
| `app/src/repl.rs` | line-table test row | +6 |
| `app/tests/dispatch_routing.rs`, `cli_verbs.rs` | routing row + e2e | +~60 |
| `ui/src/actions.rs` | `ContextId::Palette`, flip, widening, test updates | +18/−8 |
| `ui/src/router.rs` | `OpenPalette` + arm | +8 |
| `ui/src/view_state.rs` | `Flow::Palette` + 4 methods + units | +~120 |
| `ui/src/shell.rs` | `palette` bit + chain head + test growth | +16 |
| `ui/src/keymap/presets.rs` | 10 preset rows + masks | +22 |
| `ui/src/keymap/mod.rs` | masks contract-test rewrite | +4/−3 |
| `ui/src/adapter.rs` | interpret arms + keystroke callback + sync | +~85 |
| `ui/src/models.rs` | `push_palette`, `PALETTE_ROW_CAP`, `IdTables.palette`, anchor filter, units | +~110 |
| `ui/compositions/search-palette.slint` (new) | the overlay | +~85 |
| `ui/app-window.slint` | mount + properties + callback | +28 |
| `ui/tests/journeys.rs` | the palette leg | +~110 |
| snapshots ×2 | deliberate regen | churn |
| `CHANGELOG.md` | the D16 bullet | +8 |

**Estimated diff: ≈ +1300/−20 source lines over ~20 files — a
comfortable M**, matching the dispatch's size call. New `.rs`
files: `find.rs` (MIT OR Apache-2.0), `commands/find.rs` (AGPL) —
headers per AGENTS §6.

---

## 5. Test matrix (TDD order — each test written first, watched fail, then the code)

| Layer | Test | Owns |
|---|---|---|
| domain proptest | determinism (insertion order + double call) | purity/canonicality |
| domain proptest | case-fold invariance (ASCII flips) | the folding contract |
| domain proptest | empty/whitespace query floor | the floor |
| domain proptest | exact-title invariant (planted Exact ranks first) | Exact semantics |
| domain proptest | growth monotonicity | add-never-removes |
| domain proptest | type-filter partition | restriction stability |
| domain proptest | totality (strict keys, no dup targets) | the tiebreak |
| domain proptest | class monotonicity *(algorithm-shaped)* | refinement |
| domain unit | one example per class + one no-hit *(algorithm-shaped)* | classifier edges |
| app unit (cli) | parse rows: flags valid; `--type`/`--limit 0` usage errors | typed validation |
| app unit (find.rs) | ranking/filter/limit/context projection over hand-built states | the projection |
| app unit (routing) | `route_for_ipc(Find)` = IpcOrLocal | the lane |
| app integration | one `find` through dispatch (exit 0, rows) | the verb wiring |
| ui unit (view_state) | open/cursor-wrap/jump-matrix/stale-target/Esc | the flow machine |
| ui unit (shell/keymap) | chain `[Palette, ViInsert, Global]`; masks = {Menu, Palette}; preset resolvability | the chain flip |
| ui unit (models) | `push_palette`: anchor filtered before cap, overflow count, tables alignment | the row pipeline |
| ui journey | the D15 leg | the end-to-end verb |

No sleeps anywhere (nothing waits); no text assertions (row counts,
indices, focus targets, typed models only); `assert_commands(&[])`
pins the palette's engine silence.

---

## 6. Non-goals (dispatch §10 restated + this plan's)

- No description/label search, no typed query grammar, no debounce
  machinery, no recent-jumps, no kiosk surface.
- No numeric score anywhere; no fifth public item on `find`'s
  surface; algorithm-shaped properties never outside the module.
- No UI-side matching (the palette calls the domain `find`; the
  overlay renders rows only).
- No anchor filtering inside the domain, and none on the CLI rows
  (D1's recorded scope).
- No `relabeling` of the `menu.*` vocabulary ("list entry" polish
  is out); no flow-hint growth; no tooltips on palette rows.
- No chunk-10 surface (today view, capture, move ring) — the shared
  files (`view_state.rs`, `actions.rs`, `models.rs`, `cli.rs`,
  `repl.rs`) grow find's concerns only.
- No tracker edits; the #348 amendment note is stage 3's (MD-4).

---

## 7. Risks, residual windows, interpretation flags

- **Printable fall-through under the masked chain** — the palette
  field depends on `Unhandled` strokes reaching the focused
  `TextInput` with `Palette` first in the chain. Mechanically
  identical to insert mode (the resolver only resolves *bindings*;
  unbound keys reject), but the journey types real text through
  it, so any regression fails loudly at step 3 of D15. Risk: LOW.
- **The r2 D9 row-diff discipline on a keystroke-driven surface** —
  the palette is the first model that mutates per keystroke; a
  fresh `VecModel` per push would regress the stable-identity
  invariant the r2 round just established. Mitigated: `push_palette`
  diffs onto a persistent handle exactly like the banner rows;
  stage 3 reviews this specifically. Risk: LOW (pattern exists).
- **Chunk-10 file overlap** — 9 and 10 share `view_state.rs`,
  `actions.rs`, `models.rs`, `cli.rs`, `repl.rs`; the dispatch plan
  orders them serially. If 10 is dispatched concurrently anyway,
  MD-2's rebase dance applies; this plan's slices are cut to
  minimize collision (no shared hunks with today-view concerns).
  Risk: procedural, LOW.
- **Unicode folding corner cases** — `to_lowercase` is deterministic
  but not injective; D4's property set is scoped to ASCII flips
  and the classifier treats the fold as opaque. No known false
  ranking; if a pathological title matters later it is a swap-time
  question (the firewall exists for exactly this). Risk: LOW.
- **`SearchTarget` Ord as public surface** — deriving `Ord` on the
  contract enum makes the kind-tag tiebreak part of the contract.
  That is deliberate (D3: totality is contract-shaped), but a
  future swap cannot reorder the variants without a semver-visible
  change. Recorded, accepted. Risk: NONE operationally.
- **Interpretation flags (recorded-reason readings of prose, not
  ACCEPTED rulings)**: (a) archived tasks hit (D1, DDF-14 wording
  over the doc-comment's loose phrase); (b) the CLI does not hide
  the anchor task (D1, normative scope placement); (c) the palette
  chain is `[Palette, ViInsert, Global]` behind a `Palette` mask
  rather than a bare `[Palette]` (D8 — same resolution, richer
  chain for hints; the mask carries the semantics); (d) cursor
  wraps (menu parity) and resets on query change (D10); (e)
  `--limit 0` is a usage error, not "print nothing" (D6). If stage
  3 reads any ruling differently, the divergence is recorded, not
  silently applied.
- **Estimate risk** — the overlay is the chunk's widest new
  surface (slint + adapter + models); if the leg uncovers focus or
  mount friction (the menu's r2 D12/D13 lessons), the fix stays
  inside `search-palette.slint` + the adapter arms — bounded by the
  design's fixed scope. Risk: LOW-MED on calendar, none on shape.

---

## 8. Acceptance checklist (stage 2/3 verify against this)

- [ ] Domain: exactly four public items in/through `find` + lib
      re-exports; internals private; the suite in-module with the
      contract/algorithm tags; all eight properties green; no
      numeric score; tasks `is_live()`, boards/stacks
      live-and-unarchived; anchor not excluded domain-side.
- [ ] CLI: top-level `find` with `--type` ValueEnum + `--limit ≥ 1`
      ranged parser; exit 0 on well-formed queries; empty table on
      no hits; `FindRow` = kind/id/title/context; limit applies
      after ranking; `IpcOrLocal`; REPL line parses through clap;
      completion offers the three literals.
- [ ] Registry: `PaletteSearch` enabled with a real description;
      `ContextId::Palette` named "palette"; the four `menu.*` rows
      widened; the reserved-set pins updated.
- [ ] Keymap: chain `[Palette, ViInsert, Global]` while the flow
      holds; masks = {Menu, Palette}; three presets' palette rows
      (no vi j/k); snapshots regenerated (no `.pending`).
- [ ] Palette: `Flow::Palette { selected }` opens with
      `SetMode(Insert)` + cleared buffer; per-keystroke
      `set_buffer` + cursor reset + targeted `push_palette`;
      refresh re-ranks without closing; 8-row cap + overflow;
      anchor task filtered before the cap; `IdTables.palette`
      indices-back; jump-to per the D12 matrix; stale target inert;
      Esc/dismiss close without effect; the flow always ends.
- [ ] Journey: the D15 leg green — structural assertions only,
      `assert_commands(&[])` throughout, the anchor's absence
      proven by row counts.
- [ ] Gates (AGENTS §10) green; CHANGELOG bullet; no tracker edits;
      stage 3 posts the #348 amendment note + `Closes #348`.

---

## 9. References

- Dispatch plan: `.artifacts/plans/2026-10-19-daily-driver-implementation-dispatch-plan.md`
  §1 (execution model, MD-1…MD-10), §10 (chunk 9's
  covers/acceptance/traps/prompts).
- Implementation plan: `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  §2 DDF-14, §3 chunk 9, §4 gates, §5 sequencing/exit row, §6
  bookkeeping row.
- Design round: `.artifacts/plans/2026-10-19-views-finding-design-plan.md`
  Part C §3.8–§3.10 (D-V8 + the crate-swap firewall paragraph,
  D-V9 as amended, D-V10), §4 coherence map, §5 decision record +
  amendment session, §6 chunk mapping, §7 item 5.
- Sibling plan (house style + the chunk-8 seam record):
  `.artifacts/plans/2026-10-23-daily-driver-desktop-notifications-plan.md`.
- Anchor-const provenance: chunk 3's transport plan
  (`.artifacts/plans/2026-10-22-daily-driver-appdata-transport-plan.md`)
  and chunk 5's surfaces plan
  (`.artifacts/plans/2026-10-22-daily-driver-task-field-surfaces-plan.md`)
  — the presentation-side filter precedent this chunk's palette
  reuses (`is_anchor_task`).

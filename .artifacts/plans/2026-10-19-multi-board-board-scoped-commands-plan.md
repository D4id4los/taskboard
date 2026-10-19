# [Plan] Multi-board Chunk 2 — Domain Vocabulary Motion: Board-Scoped Commands

- **Date**: 2026-10-19
- **Kind**: stage-1 TDD-first implementation plan (dispatch plan §3,
  prompt 2.1) for chunk 2 of the multi-board implementation round.
- **Branch / worktree**: `feat/domain-board-scoped-commands` at
  `../taskboard-WT-feat-domain-board-scoped-commands` (created off
  `origin/main` = `1eef7ec`, chunk 1's PR #358 verified merged).
- **Normative sources** (MD-7 rank): implementation plan §2 DF-1,
  DF-2, DF-9, DF-10 / §3 chunk 2 / §4 / §5 / §6; design §3.1 (four
  vocabularies, nine-arm table, CommandError list, caller-side
  burden, snapshot-churn note) and §3.9 (snapshot rule); dispatch
  plan §1/§3. Deviations from prose are recorded in §8 with reasons;
  nothing here deviates from a design §5 ACCEPTED decision.
- **Line numbers below are plan-day references** (MD-8); symbols and
  finding IDs are the stable keys. Every site was re-located on
  2026-10-19 against `1eef7ec`.

## 1. Scope

**In** (DF-1 whole, applied to the command vocabulary): the nine
board-scoped `StateCommand` arms gain `board: BoardId`;
`CreateBoard`'s single-live-board refusal lifts (always mints); the
resolver becomes a typed lookup (`live_board_of`); the
`CommandError` set moves (−`NoBoard` −`BoardExists`,
+`UnknownBoard(BoardId)` +`BoardDeleted(BoardId)`, `BoardAlreadyBound`
widened); the `Execute` wire (the `StateCommand` enum itself,
`messages.rs`) mirrors; app mirrors widen (`commands/mod.rs`) plus
the CLI surface (`cli.rs`, `boards.rs`, `stacks.rs`, `labels.rs`):
`--board <uuid>` on board-scoped verbs, single-live-board default
resolved at invocation, typed ambiguity refusal; marked interim
fills at the remaining construction sites (session `select_on` path,
UI `CreateStack`); outbox **zero diff, verified**; snapshot pass 2.

**Out** (non-goals, implementation plan §3 chunk 2): no ceremony
redesign (chunk 3 — `select_on`'s single-board pre-check keeps
compiling against the widened variant; `destruct_on`/`read_scope`
get only the mechanical board parameter, not chunk 3's payload
work); no real selection homes (no `ReplSession.current_board`,
no `ViewState.board` — interim fills only; chunks 6/7 own them);
`SetBoard` kept verbatim (retires in chunk 4); no `UnbindBoard` /
`ReconcileByTitle` / `RemoteBoardInUse` (chunk 3); no sync-actor
changes (the interim — actor syncs its first bound board only —
closes at chunk 4); no new deps (DF-10: engine-internal vocabulary
on existing channels, topology untouched).

**Interim state, stated loudly** (implementation plan §3 chunk 2,
§5): after this chunk multiple boards and bindings are
engine-possible while the sync actor still syncs its first bound
board only. The gap stays *visible* — per-board `pending_ops`
climbs on the unsynced boards through chunk 1's map — and nothing
in this chunk may silence it. Multi-board is not user-advertised
until chunk 5. Stage 2's report restates this; stage 3 treats a
change that hides the climb as a finding.

## 2. Re-verified seam table (2026-10-19, base 1eef7ec)

| Seam | Site (plan-day line) | Role in this chunk |
|---|---|---|
| `CommandError` enum | domain `command.rs:56–92` | −`NoBoard`(:77) −`BoardExists`(:80); widen `BoardAlreadyBound`(:84); +2 variants. Stays `Copy` (all payloads are `Copy` id newtypes). |
| `live_board` resolver | domain `command.rs:134–140` | Replaced by `live_board_of(app, board)` + `existing_board(app, board)`. |
| Nine handler sites | domain `command.rs` — `CreateStack`:363, `CreateLabel`:464, `CreateBoard`:573, `RenameBoard`:602, `SetBoardColor`:619, `SetBoardArchived`:636, `DeleteBoard`:653, `ResetBoard`:753, `BindBoard`:823 | Arms re-shaped; `live_board(app).ok_or(NoBoard)?` → `live_board_of(app, *board)?`; `CreateBoard`'s guard (:574–577) deletes. |
| `StateCommand` enum | domain `messages.rs:20–204` | The `Execute` wire (travels verbatim in `EngineEnvelope::Execute`, state `actor.rs:50`); eight arms gain `board`; import `BoardId`. |
| `LocalOp` | domain `outbox.rs:22–54` | **Zero diff.** `UpdateBoard(BoardId)`/`DeleteBoard(BoardId)` already board-addressed. |
| App mirror enum | app `commands/mod.rs:84–234`, `error_kind`:264–305, `error_detail`:310–325, `domain_error_kind`:331–343 | Mirror motions per §4.3. |
| One-shot dispatch | app `commands/mod.rs:372` (arms at :423–:588) | Threads the flag into the verb wrappers. |
| Session dispatch | app `commands/mod.rs:998` `dispatch_line` → `boards_line`:1047 / `stacks_line`:1135 / `labels_line`:~1160 | Same clap surface — flag flows; resolution at invocation (interim vs chunk 6). |
| Verb handlers | app `commands/boards.rs` (`create_on`:117, `rename_on`:143, `read_scope`:170, `destruct_on`:235, `color_on`:265, `archive_on`:276, `delete_on`:287, `reset_on`:295, `board_receipt`:303, `publish_on`:428, `select_on`:566), `stacks.rs` (`add_on`:34), `labels.rs` (`list_on`:57, `add_on`:105) | Gain the resolved `BoardId`; implicit first-live-board picks scoped. |
| `resolve_target` | app `commands/tasks.rs:101–110` | Compile-forced: synthesizes `Rejected(NoBoard)` today; remapped (D2-7). |
| CLI definitions | app `cli.rs` (`BoardsCmd`:126, `StacksCmd`:203, `LabelsCmd`:316) | `--board` flag + `parse_board_id` value parser. |
| UI constructions | ui `view_state.rs:479` (`commit_edit`, `Flow::CreateStack` arm) + `append_stack_order`:90; `presentation::live_board`:250 | One interim fill; chunk 7 replaces. |
| Snapshot corpus | state `tests/snapshots.rs` (S1–S10) + `snapshots/` (20 snaps) | Pass 2: only S7's pair changes (§4.8). |
| Domain unit corpus | domain `command.rs` tests :1628–:3200 | ~14 tests rewritten/added (§5.1). |
| State test corpus | state `tests/{engine,sqlite_smoke,proptest,snapshots}.rs` | Mechanical board-field fills. |
| App test corpus | app `tests/{cli_verbs,cli_editing,cli_attach,cli_output,repl,it_cli,it_docker}.rs` | Refusal-kind updates (§5.3). |

## 3. Architecture of the motion

One vocabulary moves in one chunk (the design's half-run trap):
domain arms + wire + error set + app mirrors + CLI surface + every
construction site. The compiler is the exhaustiveness check — after
the enum arms change, no construction site compiles until it names
a board, and no `NoBoard`/`BoardExists` reference compiles at all.

Selection stays caller-side (DF-2). The domain gains only pure
vocabulary: `board: BoardId` fields and typed lookup refusals.
"Which board" is answered once per invocation by the app layer
(`resolve_board`, §4.4) or by the interim fills (§4.6); nothing of
selection enters the domain crate.

## 4. Settled decisions

### 4.1 D2-1 — `live_board_of` signature and typed refusal mapping

```rust
/// The named board, refusing typed when the id names nothing or a
/// tombstone (the parameterised replacement of the implicit
/// first-live-board pick; edit-style intents target live boards).
fn live_board_of(app: &AppState, board: BoardId) -> Result<BoardId, CommandError> {
    match app.boards.get(&board) {
        None => Err(CommandError::UnknownBoard(board)),
        Some(b) if b.deleted => Err(CommandError::BoardDeleted(board)),
        Some(_) => Ok(board),
    }
}
```

- **`UnknownBoard(BoardId)`** — the id names nothing (`boards.get`
  misses). Mirrors `UnknownTask(id)` (`command.rs:143`).
- **`BoardDeleted(BoardId)`** — the id names a tombstone. Today's
  `live_board` filters `!deleted` silently; the parameterised lookup
  makes the refusal typed. Mirrors `TaskDeleted(id)`. Note:
  *archived ≠ deleted* — archived boards stay commandable
  (unchanged semantics; the resolver checks `deleted` only).
- Returns `BoardId`, not `&Board` — handlers then index
  `app.boards[&board_id]` exactly as today (minimal diff).
- Terminal-intent companion for `DeleteBoard`, mirroring
  `existing_task`/`existing_stack` (`command.rs:151–155`):

```rust
/// An existing board for a *terminal* intent: tombstoned is not an
/// error here (the repeat is an accepted no-op), only absence is.
fn existing_board(app: &AppState, board: BoardId) -> Result<&crate::entities::Board, CommandError>
```

  This dissolves today's `boards.is_empty()` special case
  (`command.rs:656–661`): unknown id ⇒ `UnknownBoard(board)` typed;
  tombstoned id ⇒ accepted no-op. That special case existed only to
  distinguish "nothing at all" from "already tombstoned" under the
  implicit pick; the explicit id makes the distinction structural.

### 4.2 D2-2 — the nine arm shapes (field order: board first)

Verbatim from the design §3.1 table (board first everywhere — the
named target leads, matching the design's normative ordering):

| Arm | Post-change shape (in `messages.rs`, mirrored by the match in `command.rs`) |
|---|---|
| `CreateStack` | `CreateStack { board: BoardId, title: String, order: i64 }` |
| `CreateLabel` | `CreateLabel { board: BoardId, title: String, color: Color }` |
| `CreateBoard` | **unchanged** `{ title: String, color: Color }` — refusal **lifted** (D2-6) |
| `RenameBoard` | `RenameBoard { board: BoardId, title: String }` |
| `SetBoardColor` | `SetBoardColor { board: BoardId, color: Color }` |
| `SetBoardArchived` | `SetBoardArchived { board: BoardId, archived: bool }` |
| `DeleteBoard` | `DeleteBoard { board: BoardId }` |
| `ResetBoard` | `ResetBoard { board: BoardId }` |
| `BindBoard` | `BindBoard { board: BoardId, remote: RemoteBoardId }` |

- `messages.rs` adds `BoardId` to the `crate::ids` import; arm doc
  comments re-phrased from "the single live board"/"the bound
  board" to "the named board" (doc-only; no doc tests exist on
  these arms).
- Task/stack/label-scoped arms stay id-addressed (their entities
  carry board ownership already) — untouched.
- Handler bodies: each `live_board(app).ok_or(CommandError::NoBoard)?`
  becomes `live_board_of(app, *board)?`; `DeleteBoard` uses
  `existing_board`; `BindBoard` keeps its same-remote ⇒ accepted
  no-op rule, refuses `BoardAlreadyBound { board, bound }` on a
  differing remote.
- `CreateStack`/`CreateLabel` stamp the *named* board on the new
  entity (`stack.board = board` — same expression, now explicitly
  sourced). Doc comment on the enum's `CreateBoard` arm drops the
  "refuses when a live board exists" clause.

### 4.3 D2-3 — error-set motion + app mirror mapping table

**Domain `CommandError` (`command.rs:56–92`):**

| Motion | Variant | App twin (`commands/mod.rs`) |
|---|---|---|
| removed | `NoBoard` | app `CommandError::NoBoard` (:149) **removed too** — every use is replaced by `AmbiguousBoard` (zero/≥2 at resolution) or `Engine(Rejected(UnknownBoard/BoardDeleted))` (named id that does not resolve); see D2-7. `domain_error_kind` arm `D::NoBoard` (:334) deleted. |
| removed | `BoardExists` | no app twin existed (only `LocalBoardExists`, which stays — select-ceremony guard, re-derived chunk 3); `domain_error_kind` arm (:338) deleted; the `board-exists` kind disappears from the JSON shape (sanctioned: design §3.9 breaking-change candidates — "in-process vocabularies mirrored at the app edge with typed replacements"). |
| added | `UnknownBoard(BoardId)` | `domain_error_kind` arm `D::UnknownBoard(_) \| D::BoardDeleted(_) => "unknown-board"` (one arm, mirroring `UnknownTask \| TaskDeleted => "unknown-task"`). The app-level `CommandError::UnknownBoard(u64)` (:105) is a **different, remote-scoped variant** (Deck-side listing lookup, `select_prelude`) — untouched. |
| added | `BoardDeleted(BoardId)` | same arm as above (`"unknown-board"`). |
| widened | `BoardAlreadyBound(RemoteBoardId)` → `BoardAlreadyBound { board: BoardId, bound: RemoteBoardId }` | app `CommandError::BoardAlreadyBound { current: u64 }` (:108) widens to `BoardAlreadyBound { board: String, current: u64 }` — `board` is the local board's uuid string (receipt-side rendering format; `current` keeps its name and remote-id meaning; `error_detail` (:313) prints both). Kind stays `"board-already-bound"`. The F8-06 "seed of the multi-binding refusal vocabulary"; chunk 3's `RemoteBoardInUse` is the mirror image, not landed here. |
| added (app-only) | — | app `CommandError::AmbiguousBoard { candidates: Vec<String> }` — the CLI-layer typed ambiguity refusal (D2-4); kind `"ambiguous-board"`, detail `candidates.join(", ")`; `error_kind`/`error_detail` exhaustive matches force the rows. |

`CommandError` (domain) keeps `Copy` — `BoardId` and
`RemoteBoardId` are `Copy` id newtypes (`ids.rs` macro derives).

### 4.4 D2-4 — CLI mechanics: flag, default, ambiguity refusal

- **Flag spelling**: `--board <uuid>` (long flag, never positional —
  scripts stay explicit; design §3.1 caller-side burden). Lands on
  `BoardsCmd::{Rename, Publish, Color, Archive, Unarchive, Delete,
  Reset}`, `StacksCmd::Add`, `LabelsCmd::{List, Add}` — exactly the
  verbs whose handlers construct board-scoped commands (or read
  board-scoped content: `labels list`). `BoardsCmd::{List, Select,
  Create}` do not get it (`list` is remote-side; `create` mints
  without a target; `select` is chunk 3's ceremony, D2-8).
- **Value parser**: `fn parse_board_id(raw: &str) -> Result<
  taskboard_domain::BoardId, String>` in `cli.rs` —
  `uuid::Uuid::parse_str` → `BoardId::from_uuid`; malformed input
  is a clap usage error (exit 2), the `parse_color` precedent
  (:243). Type: `Option<taskboard_domain::BoardId>`.
- **Default resolution at invocation** (DF-2), one helper in
  `commands/mod.rs`:

```rust
/// Resolves the board a board-scoped verb targets: the flag when
/// given (typed lookup refusals when the id names nothing / a
/// tombstone), else the single-live-board default (typed refusal
/// when zero or ≥2 live boards make it ambiguous).
pub(crate) fn resolve_board(
    app: &crate::App,
    flag: Option<taskboard_domain::BoardId>,
) -> Result<taskboard_domain::BoardId, CommandError>
```

  - `Some(id)`: `shared_state()` lookup — absent ⇒
    `Engine(Rejected(domain UnknownBoard(id)))`; tombstoned ⇒
    `Engine(Rejected(domain BoardDeleted(id)))` (same kinds the
    engine would refuse with, surfaced before construction — one
    behavior, one home; `destruct_on`'s `read_scope` and the append
    orders need the resolved id pre-execute anyway).
  - `None`: live boards (`!deleted`) — exactly 1 ⇒ that id; 0 or
    ≥2 ⇒ `CommandError::AmbiguousBoard { candidates }`, candidates
    = the live boards' uuid strings in `BTreeMap` id order (the
    tokens `--board` accepts; **empty when zero** — the design
    unifies "zero" and "≥2" as the same ambiguity refusal).
  - Called inside the `_on` handlers (post-boot — shared state only
    exists then; both the one-shot `with_core` path and the session
    `dispatch_line` path funnel through `_on`, so one call site per
    verb).
- **Honest replacement**: flag omitted + exactly one live board ⇒
  that board — today's implicit pick, now a typed, visible rule.
  Every single-board v0.1.x script keeps working unflagged.
- REPL and daemon-IPC lines inherit the flag automatically: both
  parse through the same clap enums (`resolve_words` →
  `dispatch_line`, `repl.rs:473`/`mod.rs:998`).

### 4.5 D2-5 — outbox zero diff: verification method

`LocalOp` is already board-addressed where board verbs write it
(`UpdateBoard(BoardId)`, `DeleteBoard(BoardId)`, `outbox.rs:51/:53`)
and entity-scoped everywhere else (push resolves the owning board
through the entity graph — DF-1). This chunk therefore **changes
nothing** in `outbox.rs`. Proof, run by stage 2 and re-run by
stage 3:

1. `git diff origin/main...HEAD --stat --
   crates/taskboard-domain/src/outbox.rs` → **empty** (the
   controlling assertion; stage 3 treats any hunk as a finding
   unless the recorded reason is overwhelming).
2. `grep -n "enum LocalOp" -A 33 crates/taskboard-domain/src/outbox.rs`
   before/after → byte-identical (guards against accidental
   neighbor edits in the same file).
3. No handler body changes the `enqueue(LocalOp::…)` expressions:
   the nine handlers keep enqueueing the same ops with the same
   payloads (`UpdateBoard(board_id)` etc. — the id now arrives via
   `live_board_of` instead of the implicit pick, same value in the
   single-board world).

### 4.6 D2-6a — interim-fill inventory, marking convention, minimality rule

**Inventory** (every remaining construction site after the flag
surface; complete by compiler exhaustion + the §2 table):

| # | Site | Fill | Replaced by |
|---|---|---|---|
| 1 | `boards.rs select_on` (:566) — migration-path `BindBoard` construction (:604) and the pre-check's first-live-board find (:573) | resolve the first live board inline (BTreeMap id order), refuse typed (app `AmbiguousBoard { candidates: vec![] }`) when none exists | chunk 3 (ceremony redesign folds the pre-check into engine-refusal pass-through) |
| 2 | ui `view_state.rs commit_edit` `Flow::CreateStack` arm (:476–484) + `append_stack_order` (:90) | resolve the board inline via `presentation::live_board(state)` (:250 — first live **non-archived**, deterministic; the UI's existing notion); no board ⇒ no command (the flow ends empty — the UI's existing empty-board behavior) | chunk 7 (`ViewState.board` + seed rule) |
| 3 | session dispatch (`mod.rs` `boards_line`/`stacks_line`/`labels_line`) | **not a code fill**: the session lines resolve at invocation through the same `resolve_board` as one-shot (no session-sticky board exists yet — that absence *is* the interim) | chunk 6 (`ReplSession.current_board` pre-fills the flag) |

**Marking convention**: every fill carries one greppable comment
token at the resolution site —

```rust
// INTERIM(multi-board-2 → chunk-N): <what replaces it and when>
```

(e.g. `// INTERIM(multi-board-2 → chunk-7): inline first-live-board
resolve until ViewState.board + the seed rule land.`). Stage 2
concludes with `grep -rn "INTERIM(multi-board-2" crates/` and the
report lists every hit; stage 3 checks the inventory matches §4.6
exactly.

**Minimality rule**: an interim fill may only (a) read shared
state, (b) pick the deterministic first live board, (c) refuse
typed or dispatch nothing. It may **not** add session fields,
config keys, flags, or persistence; a fill that grows is scope
creep into chunks 6/7 (dispatch-plan trap list). The `--board`
flag itself is *not* interim — it is DF-2's real CLI home.

### 4.7 D2-6b — `CreateBoard` refusal lift

The single-board guard (`command.rs:574–577`) deletes; `CreateBoard`
always mints (titles are not uniqueness keys anywhere; `boards
list` disambiguates — design §3.1). The domain unit test
`create_board_refuses_when_a_live_board_exists` (`command.rs:2252`)
is **inverted, not deleted**: it becomes
`create_board_always_mints` — with a live board present a second
`CreateBoard` returns `CreatedBoard` with a *distinct* id; a
companion case mints two same-titled boards (the titles-not-unique
pin). `create_on`'s defensive `let … else`
(`boards.rs:130–134`) loses its `BoardExists` payload — the arm
becomes the same defensive-shape mapping its siblings use (see
D2-9). REPL test `repl.rs:425–443` ("session continues after a
typed engine refusal", currently vehicle: second `boards create` →
`board-exists`) re-vehicles on `tasks done <stranger-uuid>` →
engine `unknown-task` — same intent (typed engine rejection, line
failure, session continues), refusal that still exists.

### 4.8 D2-8 — snapshot pass 2 catalogue (DF-9)

- The corpus: `crates/taskboard-state/tests/snapshots.rs` (S1–S10)
  × 2 views = 20 `.snap` files.
- **Sharper expectation than "regenerate everything"**: the command
  *parameters* do not appear in `AppState`/`PersistedState` — the
  state records *effects* (entity rows, outbox ops), and in the
  single-board scenarios the named board is the only board, so the
  effects are byte-identical. The one behavior change that reaches
  the state is the refusal lift: **S7** (`s7_standalone_first_run_
  and_bind`) currently asserts the second `CreateBoard` rejection
  and snapshots one board; post-lift it mints a second board, so
  exactly `s7_published_app_state.snap` and
  `s7_persisted_state.snap` change (a second board row; S7's
  in-test assertion flips to the mint, D2-6b).
- Procedure: update the scenario code (board ids flow from the
  seeds — `common::board()` mints deterministic `CountingIds`
  ids; scenarios read the id from `shared_state()` via a tiny
  `Scenario::board_id()` helper or capture the seed before
  `start_with`), run the suite, then **review before accepting**:
  the pending diff must contain exactly the S7 pair and nothing
  else. Any other snap diff = "something else" (DF-9's review
  bar) — investigate, never blind-accept. `cargo insta accept`
  only after that review; no `.pending` snapshots ship.
- The scenario-code board fields are test-code changes, not
  snapshot changes — they ride the same commit.

### 4.9 D2-9 — defensive `let … else` arms on create receipts

`create_on` (boards.rs:130), `add_on` (stacks.rs:57), `add_on`
(labels.rs:118) map a non-create outcome to a typed error "guarding
an engine contract break". With the named-board refusals moving
into `resolve_board`/the engine, these arms map to
`Engine(Rejected(UnknownBoard(<the resolved id>)))` in boards.rs /
stacks.rs (the id is in hand) and in labels.rs to the same shape
(the resolved board id) — honest (the only surviving way the
outcome mismatches is the board dying between resolve and execute),
no new variants, and the `NoBoard`/`BoardExists` references die.
A `debug_assert!(matches!(…))` prefix may substitute where the
codebase prefers it; stage 2 picks the form matching sibling
handlers, stage 3 checks no panic path is introduced on user
input.

### 4.10 D2-10 — `resolve_target` remap (tasks.rs:101–110)

Today: board-less db ⇒ `NoStacks` remapped to
`Engine(Rejected(NoBoard))` (a friendlier kind for `tasks add` on a
fresh install). Compile-forced change. **Decision**: remap to
`CommandError::AmbiguousBoard { candidates: vec![] }` — the
sanctioned "zero live boards" refusal — preserving today's
user-facing distinction (create a board first) inside the new
vocabulary. Alternative (drop the mapping, surface `NoStacks`)
rejected: silent UX degradation for fresh installs, zero payoff.
(Task verbs themselves stay stack-id-scoped — no `--board` flag
there; not in the chunk's file list.)

### 4.11 D2-11 — UI fill mechanics

`commit_edit`'s `Flow::CreateStack` arm constructs with the board
from `presentation::live_board(state)`; `append_stack_order`
gains the resolved board id as a parameter (its first-live-board
find at :91 moves up to the caller — the interim fill site).
`stack_ids` (move flows) and every task-scoped construction stay
untouched (chunk 7 owns the UI board dimension; the UI remains a
single-board view until then). Router (`router.rs`) constructs no
board-scoped commands — verified by grep, nothing to do.

## 5. TDD plan (red → green per phase; AGENTS §5 rules)

Typed outcomes only (`matches!(err, E::Variant)`), no text
assertions, no sleeps (poll-with-deadline/join), fakes over mocks
(the existing `InMemoryRepository`/`CountingIds`/`FixedClock`
harness; CLI tests keep their direct-handler-call shape),
proptest for the pure resolver. Every new `.rs` file: none (all
edits in existing files — no SPDX additions needed; headers
already in place).

### 5.1 Domain (`command.rs` unit corpus — the lowest layer, written first)

1. **Resolver mapping** (new): `live_board_of` — absent id ⇒
   `UnknownBoard(id)`; tombstoned id ⇒ `BoardDeleted(id)`; live ⇒
   `Ok(id)`. `existing_board` — absent ⇒ `UnknownBoard(id)`;
   tombstoned ⇒ `Ok(&board)`.
2. **Refusal mapping per arm** (rewrites of the `…_without_a_
   live_board_*` tests at :1672/:1772/:2288/:2382/:2611/:2689):
   `CreateStack`, `CreateLabel`, `RenameBoard`, `SetBoardColor`,
   `SetBoardArchived`, `ResetBoard`, `BindBoard` each refuse
   `UnknownBoard(board)` / `BoardDeleted(board)` with the *named*
   id in the payload.
3. **Multi-board semantics** (the F16-06 pin, new): two live
   boards seeded; `CreateStack { board: B, … }` lands the stack on
   B (A untouched); `RenameBoard`/`SetBoardColor` touch only the
   named board.
4. **`DeleteBoard` terminal intent** (rework of :2508–:2625):
   unknown id ⇒ `UnknownBoard`; tombstoned id ⇒ accepted `NoOp`;
   live id ⇒ cascade unchanged (existing assertions keep holding
   with the named id).
5. **`BindBoard` widened refusal** (rework of :2308–:2329):
   differing remote ⇒ `BoardAlreadyBound { board, bound }` with
   both payloads asserted; same-remote ⇒ `NoOp` (existing).
6. **Refusal lift** (D2-6b): `create_board_always_mints` +
   same-title coexistence; `BoardExists`/`NoBoard` no longer
   compile anywhere in the corpus (the deletion is the test).
7. **Idempotence kept**: same-title `RenameBoard` ⇒ `NoOp`;
   same-color `SetBoardColor` ⇒ `NoOp`; archived-repeat ⇒ `NoOp`
   (existing tests, board field added mechanically).
8. **Proptest** (pure resolver): for generated `AppState` board
   maps and any `BoardId`, `live_board_of` is `Ok` ⟺ the id
   exists ∧ `!deleted`, and the error variant matches the
   condition — extends the existing `proptest` module in
   `command.rs`'s tests (or the crate's proptest corpus if that is
   the house pattern; stage 2 follows the neighbor convention).

### 5.2 State crate (engine thread — routing only, not re-tested domain behavior)

- `snapshots.rs`: S-code board fields (D2-8); S7 assertion flip;
   the S-series stays the behavioral contract (one snapshot pass,
   S7-only diff).
- `engine.rs` / `sqlite_smoke.rs` / `proptest.rs`: mechanical
  board-field fills at construction sites; refusal assertions
  (`NoBoard`/`BoardExists` kinds) remapped per §4.3. No new engine
  tests: the engine forwards `StateCommand` verbatim (routing, not
  semantics — "one behavior, one test, lowest layer").

### 5.3 App crate

1. **`resolve_board` unit tests** (new, direct-call, fake-core
   harness per the existing `with_core` test pattern): flag given +
   live ⇒ that board; flag + absent ⇒ engine-kind `unknown-board`
   (`Engine(Rejected(UnknownBoard))` asserted structurally);
   flag + tombstoned ⇒ `BoardDeleted`; omitted + exactly one ⇒ it;
   omitted + zero ⇒ `AmbiguousBoard { candidates: [] }`; omitted +
   two ⇒ `AmbiguousBoard` with both uuids in id order.
2. **Kind table** (`cli_output.rs:188–268`): rows for
   `AmbiguousBoard → "ambiguous-board"`, widened
   `BoardAlreadyBound → "board-already-bound"` (detail carries
   board + current), `D::UnknownBoard | D::BoardDeleted →
   "unknown-board"`; `D::NoBoard`, `D::BoardExists`,
   `CommandError::NoBoard` rows removed. (`SyncErrorKind::NoBoard
   → "no-board"` at :188/:525/:560/:580 is a **different enum** —
   untouched.)
3. **Verb-level tests** (per affected handler, direct `_on` calls
   in the existing harness): `boards rename/color/archive/delete/
   reset/publish` target the flagged board (two-board core, assert
   the named board changed, the other untouched); `stacks add`
   appends on the named board's order (two boards with different
   stack maxima — the scoping pin); `labels add`/`labels list`
   scope by the named board; unflagged single-board cores keep
   today's outcomes (the default pin).
4. **CLI-flag tests** (existing corpus extension): `--board`
   accepted on the ten verbs; malformed uuid ⇒ clap usage error
   (exit 2 class); the ambiguity refusal is typed on zero- and
   two-board cores (`cli_verbs.rs` V1 :77–87 and `cli_editing.rs`
   :291–300 flip to `ambiguous-board`; `cli_attach.rs` publish
   :252–265 flips likewise).
5. **REPL/session tests** (`repl.rs` corpus): session line
   `boards rename --board <uuid> "x"` works against the session
   core; unflagged session line on a one-board core works; the
   re-vehicled typed-refusal test (D2-6b); `dispatch_routing.rs`
   mechanical updates.
6. **`resolve_target` remap test**: board-less db `tasks add` ⇒
   `AmbiguousBoard` with empty candidates (replaces the V1
   expectation).

### 5.4 UI crate

- `view_state.rs` corpus (:912/:934/:960 CreateStack constructions):
  mechanical board fields from the seeded board; a two-board case
  pinning that the interim fill picks the deterministic first live
  board (`presentation::live_board` order); `append_stack_order`
  tests parameterised by board.

### 5.5 Test matrix summary

| Layer | New | Rewritten | Mechanical |
|---|---|---|---|
| domain `command.rs` | resolver mapping, multi-board lands, refusal lift + same-title, proptest | 8 refusal tests, BindBoard payload, DeleteBoard terminal | idempotence corpus |
| state | — | S7 assertion | S-series fields, engine/smoke/proptest corpora |
| app | `resolve_board` 6 cases, kind rows, per-verb two-board pins, flag/ambiguity CLI cases, session lines | V1/cli_editing/cli_attach kind flips, repl :425 re-vehicle | dispatch routing |
| ui | two-board interim pin | — | CreateStack corpus |

## 6. Implementation phases (stage 2)

Each phase ends green on `cargo nextest run -p <touched crates>`
+ `cargo clippy -p … -- -D warnings`; the full §10 gate set runs
once at the end (it is a workspace-wide sequence). Commit subjects
follow `<type>(<module>):` with standalone bodies (rebase-merge
hygiene):

1. `test(domain)+feat(domain): board-scoped StateCommand arms,
   typed board refusals, CreateBoard refusal lift` — messages.rs
   arms, error set, `live_board_of`/`existing_board`, handler
   bodies, domain corpus (§5.1), proptest. (TDD: corpus committed
   red alongside the motion in one standalone commit — the tests
   and the vocabulary are one semantic unit; a red-then-green
   two-commit split would leave a non-compiling midpoint, violating
   standalone-commit hygiene.)
2. `feat(app): --board flag, invocation default, typed ambiguity
   refusal, widened mirrors` — cli.rs, resolve_board, mod.rs
   mirrors + dispatch + session arms, boards/stacks/labels/tasks
   handlers, app corpus (§5.3).
3. `feat(ui): interim board fill at the CreateStack construction
   site` — view_state.rs + corpus (§5.4); marked per D2-6a.
4. `test(state): snapshot pass 2 — S7 refusal-lift diff only` —
   scenario board fields, S7 flip, the reviewed insta accept
   (§4.8).
5. `chore(changelog): Unreleased line for the board-scoped command
   vocabulary` (+ any doc-comment polish riding the motion). Stage
   2's report states the interim loudly; no tracker edits (MD-4).

## 7. Quality gates (AGENTS §10, verbatim sequence)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo test --doc --workspace
cargo deny check
cargo llvm-cov nextest --workspace --all-features --no-tests=warn --no-report
scripts/coverage-quarantine-profraw.sh target/llvm-cov-target
cargo llvm-cov report --lcov --output-path lcov.info
rm -rf target/llvm-cov-target
```

Plus this chunk's own gates: outbox zero-diff check (D2-5), the
INTERIM grep inventory (D2-6a), the S7-only snapshot diff review
(D2-8), and `grep -rn "NoBoard\|BoardExists" crates/` returning
only the sanctioned survivors (app `LocalBoardExists`,
`SyncErrorKind::NoBoard`, `domain_error_kind`'s new arms — none
matching the removed domain variants).

## 8. Risks & recorded deviations

- **R1 — kind-string churn is wire-visible**: `no-board` and
  `board-exists` leave the CLI JSON error shape (replaced by
  `ambiguous-board` / `unknown-board`). Sanctioned by design §3.9
  (typed replacements at the app edge); the CHANGELOG line names
  it. Residual risk: external scripts pinning the old kinds —
  accepted by the design's breaking-change analysis.
- **R2 — deviation (sharpened), recorded**: DF-9/§3 chunk 2 phrase
  snapshot pass 2 as "the command-corpus regeneration"; this plan
  expects an S7-only diff (§4.8) because chunk 1 already took the
  `AppState`-widening churn and command parameters do not appear
  in the snapshotted state. Same rule, sharper review bar — a
  blind accept-all would *weaken* the "nothing else" discipline.
- **R3 — deviation (naming), recorded**: the design names the
  refusal semantics ("typed ambiguity refusal") but not the
  variant; `AmbiguousBoard { candidates: Vec<String> }` follows
  the `AmbiguousStack`/`AmbiguousLabel` house shape. Prose-level
  choice, no ACCEPTED decision touched.
- **R4 — `select` stays flag-less this chunk** (D2-8/inventory #1):
  `select`'s ceremony is chunk 3's redesign target; wiring a flag
  now would create surface chunk 3 immediately reshapes. Its
  `BindBoard` construction uses the marked interim fill.
- **R5 — hidden construction sites**: the compiler enumerates them
  (struct variants force every literal); risk is only in
  non-compiled artifacts (docs/examples). Sweep:
  `grep -rn "StateCommand::" docs/ examples/ 2>/dev/null` (expected
  empty) + `grep -rn "DeleteBoard\b\|ResetBoard\b" docs/` for prose
  drift (doc updates are chunk 8; nothing changes now).
- **R6 — two-session hazard**: none — no PR is stacked (MD-2);
  chunk 3 starts only after this chunk merges.
- **R7 — interim visibility**: nothing in this chunk may damp
  per-board `pending_ops` on unsynced boards; stage 3's trap list
  carries the check (a silencing change is a finding).

## 9. Estimated diff

~1,100–1,400 changed lines across ~18 files: domain command.rs
(~450 incl. corpus), messages.rs (~60), outbox.rs (**0**), app
cli.rs (~40), commands/mod.rs (~130), boards.rs (~160), stacks.rs
(~35), labels.rs (~50), tasks.rs (~10), app tests (~180 across
~7 files), state tests (~120 across 4 files), ui view_state.rs
(~30), CHANGELOG (1). House size **L** — matches the dispatch
plan's grading.

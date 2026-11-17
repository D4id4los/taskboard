# Chunk C9 Plan — Per-Board Undo Line Scoping (#355)

Round: `2026-11-17-backlog-reduction-round-plan.md` §3-C9 (normative) ·
Dispatch: `2026-11-17-backlog-reduction-dispatch-plan.md` §1/§9 ·
Branch: `feat/state-per-board-undo` · Stage: 2 implements, 3 reviews.
Family ordering: #356 (C8, landed via PR #916 on 2026-11-17 — this
chunk's precondition, verified) → #355 (this) → #354 → #353; #357
exploratory. Engine-side only — journey-plan exempt.

## 0. Re-anchor: what C8 actually landed (main `29c01d1f`)

- The journal line (`JournalState`) is the one restore surface; C8
  reclassified `DeleteStack` to undoable (`PreImages`) and added
  `StateCommand::RestoreStack { id }` with the combined-inverse
  synthesis (`InverseClass::DeletedStack` → `RestoreStack` + N ×
  `RestoreTask` through the ordinary catalogue, D-3 of the C8 plan).
- **The C9 seam, as C8's D-9 promised**: `JournalEntry.board:
  Option<BoardId>` — tagged at record time by `touched_board`
  (`undo.rs`), best-effort, `None` = not board-scoped or the lookup
  chain is gone. C8 added the `stack_delete_and_restore_entries_carry_
  the_board_tag` pin. **No `JournalEntry` shape change is needed here.**
- `plan_undo`/`plan_redo` (`undo.rs`) are scope-free today: undo takes
  the global head, redo takes the greatest `(created_at, id)` child.
  `Undo`/`Redo` are engine-routed unit arms of `StateCommand`
  (externally-tagged serde; never persisted — `record_entry` routes
  them away before journaling, and outbox ops carry `LocalOp`, never
  `StateCommand`).
- `UndoView::derive(journal)` (`journal.rs`) is the memory-only
  projection published into `AppState.undo`; `UNDO_RECENT_WINDOW = 10`.
- The app-layer undo caller is the CLI
  (`taskboard-app/src/commands/tasks.rs:803`,
  `execute(StateCommand::Undo)`) — global context, no board scope.

## 1. Recorded decisions

### D-1 — Scoping model: the strict frontier rule, not selective undo
The journal stays **one global line/tree** (D-U4). Per-board scoping is
a *gate on the frontier*, never a walk through the middle of the line:

- **Scoped undo** (`UndoOnBoard { board }`): operates on the global
  head, and only if the head's `board` tag is in scope. If the head is
  out of scope → typed refusal (`NothingToUndoForBoard`, D-5). No
  skipping: a board-A undo never undoes (or walks past) a board-B edit
  — which is exactly #355's stated goal ("undo in one board never walks
  through another board's edits"), achieved by refusal rather than by
  selective undo. Selective skip-ahead undo breaks the line's
  invertibility story (intermediate entries stay applied while an
  ancestor's inverse fires; the skipped entries' redo paths die) and is
  deliberately **not** this chunk — refinement recorded as a backlog
  issue pending #355's own "real usage evidence" ruling.
- **Scoped redo** (`RedoOnBoard { board }`): the greatest
  `(created_at, id)` child **among in-scope children**. Siblings are
  alternative continuations (the fork law), so selecting among them is
  the existing D5 freedom narrowed, not a new semantics; an
  out-of-scope *newer* sibling does not block an in-scope child. No
  in-scope child → typed refusal (`NothingToRedoForBoard`, D-5).
- Composition with C8's D-5 is inherited verbatim: a scoped undo whose
  in-scope head is a `DeleteStack` entry fires the combined
  stack+cascade restore; everything downstream (watermark atomicity,
  occupancy guard, pushed-edit rule, head move) is `plan_undo`'s
  existing machinery, untouched.

### D-2 — The command surface: appended arms, `Undo`/`Redo` stay global
Two new `StateCommand` arms, **appended** (C8's D-8/R-3 serde
precedent — never inserted):

```rust
UndoOnBoard { board: BoardId },
RedoOnBoard { board: BoardId },
```

- `Undo`/`Redo` keep their exact semantics (global scope) — the CLI
  caller and the entire existing corpus stay untouched. The UI legs
  (#353/#354) will dispatch the new arms from a board context; **no UI
  crate changes in this chunk** (out of scope per the round plan §1).
- Both new arms are engine-routed like `Undo`/`Redo`:
  `plan_journaled_command` routes them to `plan_undo`/`plan_redo` with
  a scope; a direct dispatch to the ordinary catalogue is the existing
  typed `CommandError::EngineRouted` (the two new arms join that arm).
- Serde compatibility: `Undo`/`Redo` never serialize into persisted
  data (journal entries and outbox ops never carry them — verified in
  §0), so appending arms is wire-safe; a round-trip test pins the new
  arms' representation anyway (cheap insurance).

### D-3 — The scope predicate and the `None`-tag rule
`UndoScope { Global, Board(BoardId) }` (new enum, `undo.rs`) with one
predicate: an entry is **in scope** iff `entry.board == Some(b)`.
Entries tagged `None` (reminder/tracking ephemera, reflections, and the
honest-ambiguity case where `touched_board`'s lookup chain is gone)
belong to **no board scope**: a scoped undo/redo refuses on them
(`NothingToUndoForBoard`/`NothingToRedoForBoard`); the global
`Undo`/`Redo` still reach them. Honest ambiguity stays honest — a
scoped line never guesses.

### D-4 — Tag semantics for cross-board moves (recorded, pinned)
`touched_board` reads the **pre-state**, so a cross-board `MoveTask`
(source stack → destination stack on another board) is tagged with the
**source board** — the board whose state the entry's inverse restores.
Consequence (desired, recorded): the move's undo belongs to the source
board's scope; its redo target entry (the forward re-plan) gets the
destination board's tag. The scoped surfaces are "restore what this
board looked like", not "undo everything that happened while viewing
this board". One pin test; no `touched_board` code change.

### D-5 — Typed outcomes (the refusals)
Two new `CommandError` variants, field-named per the round laws:

```rust
NothingToUndoForBoard { board: BoardId },
NothingToRedoForBoard { board: BoardId },
```

Distinct from `NothingToUndo`/`NothingToRedo` so a UI surface can
distinguish "the line is empty/exhausted" from "the frontier belongs to
another board" (and the empty-global-line case scoped to a board
reports `NothingToUndoForBoard` too — the board-scoped question was
"anything on *this* board?", and the typed answer is no). All other
rejections pass through verbatim: `TargetNotUndoable` (in-scope
`NotUndoable` head — R-U6 unchanged), `TargetNoLongerInvertible`,
`UndoTargetHasDependents`, anything the inverse's planning rejects.

### D-6 — The projection: `derive_for_board`, `AppState` unchanged
`UndoView::derive(journal)` stays byte-identical. New:

```rust
pub fn derive_for_board(journal: &JournalState, board: BoardId) -> UndoView
```

- `can_undo`: head exists **and** is in scope (D-3).
- `can_redo`: at least one in-scope child of the head (D-1's sibling
  rule).
- `head_label`: only when `can_undo`.
- `recent`: the newest-first in-scope entries, still bounded by
  `UNDO_RECENT_WINDOW` (the window applies per board).
- The same `UndoView` shape is reused (the fields are scope-agnostic);
  `AppState` and the engine publish path are untouched — **wiring the
  projection into a board's UI surface is #353/#354's job**. The
  derivation is exposed pure so both future UI consumers and the
  engine-side tests share one function (the `derive` precedent).

### D-7 — Ceremony fences carried over: zero classification churn
C9 changes **no** `record_entry` classification row and migrates **no**
arm between `NotUndoableReason`s — the #842 erratum ruling (board
ceremony verbs stay `NotUndoable(BoardCeremony)`) holds verbatim. The
lockstep matches that *do* gain arms (the compilation contract):

- `command_label`: "Undo"/"Redo" reuse for the new arms
  (presentation-only).
- `touched_board`: `None` for the new arms (engine-routed, never
  journaled — honest).
- `record_entry` / `command_class`: the new arms join the
  "engine-routed, never journaled / unreachable" arms exactly as
  `Undo`/`Redo` do (the `every_arm_journals_entry_or_nothing` table
  extends with two "nothing" rows — extension, not rewrite).
- `plan_command`: the new arms hit the `EngineRouted` rejection arm
  alongside `Undo`/`Redo` (`command.rs:3094`).

### D-8 — No persistence change
`JournalState`, `JournalEntry`, the sqlite mirror, and the eviction are
untouched (D-9 of C8 already guaranteed the tag). Scope lives only in
the command payload and the planner's gate — nothing new to persist,
nothing to migrate. The persisted journal stays board-tagged and
board-agnostic at once; scoping is a read-side derivation.

## 2. Implementation map (stage 2)

| File | Change |
|---|---|
| `crates/taskboard-domain/src/messages.rs` | `UndoOnBoard`/`RedoOnBoard` arms (appended; serde doc comment per arm). |
| `crates/taskboard-domain/src/command.rs` | The two arms join the `EngineRouted` rejection arm; two new `CommandError` variants (D-5). |
| `crates/taskboard-domain/src/undo.rs` | `UndoScope` enum; `plan_undo`/`plan_redo` gain the `scope` parameter (the gate at the top: scoped undo checks the head's tag before the existing check order; scoped redo filters `children_of`); `plan_journaled_command` routes the new arms. No synthesis change. |
| `crates/taskboard-domain/src/journal.rs` | `UndoView::derive_for_board` (D-6); `command_label` arms (D-7). |
| `crates/taskboard-state/src/*` | No expected change (the engine plans through `plan_journaled_command`; the new arms ride the same path). |
| `crates/taskboard-app/src/commands/tasks.rs` | **No change** — the CLI's global `Undo` stays global (D-2). |
| tests | §3 catalogue: `undo/tests.rs`, `journal/tests.rs`, `command.rs` unit tests (EngineRouted arms), `crates/taskboard-state/tests/undo.rs`, `tests/proptest.rs` (domain proptest module). |

## 3. Test catalogue

Snapshots / domain tests (extend the corpus, never rewrite — law [L]c):

1. `scoped_undo_refuses_typed_when_the_head_is_another_boards_edit` —
   `NothingToUndoForBoard { board }`, state and journal byte-identical,
   head unmoved (the strict frontier rule, D-1).
2. `scoped_undo_restores_when_the_head_is_in_scope` — same plan shape
   as the global undo (incl. a `DeleteStack` head firing the combined
   C8 restore — D-1 composition).
3. `scoped_undo_on_a_none_tagged_head_refuses` — D-3's no-scope rule
   (`StopTracking`-adjacent None-tag setup via the honest-ambiguity
   fixture used by `touched_board` tests).
4. `scoped_undo_on_a_not_undoable_head_still_names_the_reason` —
   in-scope `NotUndoable` head → `TargetNotUndoable` passthrough (the
   D-5 passthrough row).
5. `scoped_redo_picks_the_greatest_in_scope_child` — fork: head with an
   in-scope older sibling and an out-of-scope newer sibling → the
   in-scope one is redone; the global `Redo` still picks the greatest
   overall (both rules pinned side by side).
6. `scoped_redo_refuses_when_no_child_is_in_scope` —
   `NothingToRedoForBoard { board }`.
7. `derive_for_board_filters_can_undo_can_redo_and_the_recent_window` —
   projection test over a two-board journal (D-6; window bound applies
   per board).
8. `scoped_undo_moves_the_head_and_journals_nothing` — batch shape:
   `MoveJournalHead` last, no `RecordJournal` (the engine-routed
   contract, asserted structurally).
9. `undo_on_board_and_redo_on_board_commands_round_trip` — serde
   round-trip of the two new arms (D-2's cheap insurance).
10. `every_arm_journals_entry_or_nothing` — the lockstep table gains
    the two "never journaled" rows (existing test extended, law [L]c).
11. `cross_board_move_entry_is_tagged_with_the_source_board` — D-4's
    pin (`touched_board` pre-state rule over a `MoveTask` across
    stacks/boards).
12. Engine-level (`tests/undo.rs`):
    `engine_scoped_undo_refuses_when_the_head_is_out_of_scope_and_the_published_state_is_unchanged`
    and
    `engine_scoped_undo_flips_the_published_state_and_the_persisted_journal`
    (mirror of the existing engine undo test over `UndoOnBoard`).

Proptests (pure domain):

- P1 `scoped_undo_never_touches_out_of_scope_entities`: over generated
  two-board journals, a scoped undo either refuses typed or leaves
  every entity of every other board byte-identical (the issue's core
  property, mechanical via the frontier rule).
- P2 `derive_for_board_agrees_with_the_scoped_planners`: `can_undo` is
  true iff `plan_undo` scoped would not return
  `NothingToUndoForBoard`, and `can_redo` iff `plan_redo` scoped would
  not return `NothingToRedoForBoard` — the projection and the planners
  share one predicate by construction, proven.
- No sleeps anywhere: planner-level, injected clock seams per the
  existing harness (law [L]b).

## 4. Acceptance (stage 3 gate)

- Typed outcomes only; no text assertions; no `RwLock`/`Mutex`.
- Zero classification churn (D-7): the diff touches no
  `NotUndoableReason` assignment; stage 3 greps the diff for
  `record_entry`/`command_class` row moves.
- `Undo`/`Redo` global semantics unchanged: the existing corpus is
  green untouched (extension-only).
- No UI crate change; the CLI caller untouched.
- Scope gate + selection rules exactly as D-1 records; an undocumented
  interaction path between scope and the fork law is a finding.
- Gates: pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest
  run --workspace` (defer per the #879 rule if the lock is held; the
  dispatcher owns the full §10 suite).

## 5. Risks

- **R-1 (expectation mismatch)**: users may expect skip-ahead undo
  ("Ctrl+Z in board A walks back through A's edits only") — the strict
  frontier rule refuses instead. Deliberate; #355's own ruling deferred
  to usage evidence. The refinement is a filed backlog issue, and the
  typed `NothingToUndoForBoard` refusal makes the state legible.
- **R-2 (tag honesty)**: scoping inherits `touched_board`'s
  best-effort nature — a `None` tag (lookup chain gone) silently
  narrows the scoped line. Recorded as correct-by-design (D-3), but
  stage 3 checks the pin test exists so the behaviour is contract, not
  accident.
- **R-3 (cross-board move asymmetry)**: D-4's source-board tag means a
  destination-board scope cannot undo a move *into* it (the source
  board's scope can). Coherent but potentially surprising; pinned by
  test 11 and revisitable with usage evidence (same vehicle as R-1).
- **R-4 (redo sibling selection)**: "greatest in-scope child" vs
  "greatest child" divergence only manifests in forked lines; P1/P2 +
  test 5 pin both readings so the rules cannot drift apart silently.
- **R-5 (rebase)**: C10/C11 land around this chunk; stage 2 opens with
  `git fetch --prune; git rebase origin/main` and re-runs the gate
  after the rebase.

## 6. Estimated diff

~600–750 lines: domain ~200–250 (two arms, scope enum, planner gates,
projection, error variants), tests ~400–500 (12 snapshot/structural +
2 proptests + engine mirror), zero storage/persistence/UI change.

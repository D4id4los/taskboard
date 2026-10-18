# [Plan] Board/Stack Structure Editing — chunk 15, remediation wave 1

- **Date**: 2026-10-18
- **Kind**: feature design plan (stage-1 contract for the three-stage
  chunk; the decision list in §4 is binding on stages 2/3 — deviations
  need a recorded reason).
- **Covers**: #132 (local board-edit commands + `Board` clocks — the
  color/archive/deleted remainder; the title half landed as WI-3),
  #171 (board reset/delete ceremony), #177 (board color/archive edit
  verbs and their clocks), #304 (GUI: create new stacks on a board).
- **Branch/worktree**: `feat/board-stack-structure-editing` in
  `../taskboard-WT-feat-board-stack-structure-editing`, off current
  `main` (8232a27, post chunks 1–10 + 13).
- **Origin**: dispatch plan §16
  (`.artifacts/plans/2026-10-17-remediation-wave1-dispatch-plan.md`),
  issues #132/#171/#177/#304 (normative), findings report §fold
  multi-board inputs (F3-04, F7-05 — see §3.4).

---

## 1. Problem & scope

`Board` is adoption-governed for everything except its title: no local
verbs exist to set a board's color or archived flag, no way to delete a
board (with cascade) or reset local content, and the GUI cannot create
a stack — board structure is read-only everywhere except `boards
rename`. The WI-3 title work left the seams ready:
`BoardClocks { title }` (with the doc comment reserving the
color/deleted clocks), `LocalOp::RenameBoard` whose push already
full-sends title **+ color + archived** (Deck PUT semantics force the
full changeset), and `merge_board`'s field-wise title defense as the
per-field LWW template.

In scope: four new `StateCommand` verbs (color, archive, delete,
reset), the `BoardClocks` completion, one `LocalOp` rename + one new
`LocalOp`, one new `PersistenceAction` (chunk 8's totality contract
applies), one storage migration, the sync planner/executor/pipeline
arms, CLI verbs with a confirmation ceremony, and the GUI stack-create
affordance.

Out of scope (non-goals, each with a home):

- board *un*-delete / Deck `undoDelete` surfacing (`client
  .restore_board` exists; a restore verb is a later round — new issue
  if wanted, not here);
- multi-board anything: board ids on command payloads, board
  switchers, per-board sync status (D2 epic; §3.4);
- app-only Deck field round-trip strategy (#207 — our two fields
  *are* Deck fields; no gap here; cited per dispatch §16);
- `stacks`/`boards` mutations beyond the four issues (e.g. stack
  archive — Deck carries `archived` on stacks but no issue asks for
  the verb);
- architecture.org rewrite (chunk 12 owns the file; this chunk's doc
  footprint is an ADR 0004 amendment + doc comments only — recorded
  deliberately to avoid colliding with the open chunk-12 effort).

## 2. Verified starting shape (seam survey, base 8232a27)

Everything below was read on the base commit; stage 2 re-verifies
anchors if `main` moved.

- **Entities** (`taskboard-domain/src/entities.rs`): `Board` already
  carries `color: Color` (lenient), `archived`, `deleted`,
  `remote_seen`; `BoardClocks { title }` only. `Stack`/`Label` carry
  the full per-field clock pattern (`deleted` clock included) — the
  template.
- **Commands** (`command.rs`): `live_board()` = smallest non-deleted
  board id (archived boards stay commandable); `CreateBoard` stamps
  `clocks.title = now`, queues **no** op (publish ceremony owns remote
  creation); `RenameBoard` queues `LocalOp::RenameBoard`; `DeleteStack`
  is the cascade template (terminal-intent no-op on repeat; bound
  children enqueue deletes, unbound children `FailOp` their queued
  ops; D-c reminder-rule cascade per task).
- **Outbox** (`outbox.rs`): ops are id-scoped, no field sets — every
  Deck write is full-send. `RenameBoard(BoardId)` exists.
- **Push planner** (`push.rs`): `plan_board_groups` materializes
  `MaterializedOp::RenameBoard` for a **bound, live** board and
  subsumes *all* queued board ops into that one group — i.e. the wire
  call is already "PUT the whole board changeset"; `RenameStack` /
  `MoveStack` share one full-send arm the same way. Stacks/labels have
  delete-dominance in their planners; boards do not (no delete op
  existed).
- **Executor** (`sync-nextcloud/src/push_exec/execute.rs:102`): the
  board arm PUTs `mapping::board_changes_from(row)` = title + color +
  archived, with the lenient-color→`DEFAULT_BOARD_COLOR` fallback;
  echo `RemoteEcho::Board`.
- **Merge** (`merge.rs`): `adopt_board_if_newer` (whole-entity
  freshness gate, stamps every clock with `remote.last_modified`),
  `merge_board` (field-wise **title** defense — the extension point),
  `adopt_board_after_push` (title clock advances to
  `max(local, echo.lm)`; sets `deleted: false` unconditionally),
  `adopt_remote_board` (R1), `tombstone_board` (clears binding — used
  for the observed-absence path).
- **Pipeline** (`pipeline.rs`): a pulled `deleted_at` on the bound
  board flows `merge_board` → `deleted: true` → `cascade_board()`
  (tombstones stacks/tasks/labels, **fails** their pending ops — the
  remote-death model cancels children ops rather than pushing them).
  The board push-echo arm adopts `RemoteEcho::Board` for
  `LocalOp::RenameBoard`. `keep_board_title` + `pending_board_intent`
  protect a just-queued rename from the same cycle's pull.
- **Actor** (`actor.rs`): target = commanded `SetBoard` else first
  bound board (F6-17); pull is conditional on **persisted** validators
  read fresh each cycle; 304s only happen when a validator is sent.
  Dead-board synthesis exists for unreadable boards.
- **Storage** (`taskboard-storage-sqlite`): `boards` has `ck_title`
  (migration 0002, epoch default = "never locally written"); outbox
  rows are snake-case `op_kind` text tags, `rename_board` rides
  `board_id` (migration 0003 backfilled exactly this way — the
  precedent for a tag rename). `execute_batch` is **compiler
  -exhaustive** (chunk 8, #243): adding a `PersistenceAction` variant
  breaks this crate's build until persisted — the totality proof is
  the compiler, and `apply_actions` in domain is likewise exhaustive.
- **Engine** (`taskboard-state`): `EngineEnvelope::Execute` →
  `plan_command` → batch; new verbs need zero engine-machinery change.
  Insta S-series snapshots capture published `AppState` + persisted
  `PersistedState` (outbox included → the `LocalOp` rename churns
  snapshots containing `rename_board` rows; accepted, F4-03 churn).
- **UI** (`taskboard-ui-slint`): actions registry → router →
  `UiEffect` → adapter → `ViewState` flows; `Flow::Create` +
  `commit_edit` dispatching `StateCommand::CreateTask` is the
  dispatch template; `append_order` is the order-default template;
  `presentation::live_board` filters `!deleted && !archived` (so a
  locally archived board blanks the GUI — intended, §4.9);
  keymap presets bind per-context (vi "o" = TaskCreate).
- **CLI/IPC** (`taskboard-app`): IPC `Request::Exec` carries raw argv
  — the daemon is the single parse authority, so new CLI verbs need
  **no** protocol change. `BoardsCmd` has list/select/create/rename/
  publish; `parse_color` (six lowercase hex) exists, doc'd
  labels-only; `StacksCmd` delete is documented no-confirmation.

## 3. Design inputs & constraints

### 3.1 Issue requirements (normative digest)

- **#132**: `BoardClocks { title, color, archived, deleted }`;
  color/archive verbs with their clocks. (The issue's exact field list
  says `{title, color, archived, deleted}` — adopted verbatim.)
- **#171**: `boards delete` with cascade + confirmation; reset is
  more than "new db path"; tombstone semantics and remote cascade
  decided here.
- **#177**: color/archive verbs, field-wise title merge is the
  template, storage migration called out.
- **#304**: GUI stack creation — name entry, default order appended
  after existing stacks, dispatches a `StateCommand`, dumb-client
  rule.

### 3.2 Deck wire facts (client.rs)

- `update_board(id, BoardChanges { title, color, archived })` — full
  send; omitted fields would take server defaults (documented on
  `BoardChanges`).
- `delete_board(id)` — soft delete, returns the board (echo carries
  `deletedAt`); `restore_board` exists (out of scope, §1).
- Board `GET` returns title/color/archived/deletedAt/lastModified —
  all our fields round-trip; **no #207 gap** for this chunk.

### 3.3 ADR 0004 (sync conflict policy) as amended 2026-10-11

Per-field LWW: remote wins ⟺ `remote.last_modified > field_clock`
(strictly). Local edits stamp only touched fields; adoption stamps
every field with the entity stamp; pushes advance clocks to
`max(local, echo.lm)`. This chunk extends the 2026-10-11 board-title
amendment to the three remaining board fields (see §4.2 and the ADR
amendment in §7).

### 3.4 Multi-board wave guardrails (findings §fold, F3-04/F7-05)

- **F3-04**: the sync-status surface (`SyncStatus`/`last_updated`) is
  a global single slot until the D2 wave; **this chunk adds nothing
  to it** and scopes every new op/action by board id (the storage
  `board_id` seam), not by "the" board.
- **F7-05**: the UI has no board dimension in `ViewState`/`IdTables`;
  the wave will add `board: BoardId`. **This chunk adds no board
  field to the UI** — stack creation addresses the single live board
  via the existing `StateCommand::CreateStack` (whose board
  resolution already lives in the domain, `live_board()`), so the
  wave's dimension lands additively later.
- Board verbs take **no board-id payload** (the `RenameBoard`/
  `CreateBoard`/`BindBoard` convention: they address the live board).
  When the wave parameterizes commands by board (F16-23), this whole
  family — existing and new verbs alike — is updated together; that
  design round owns the shape. Recorded so it reads as a scoped
  convention, not an accident.

## 4. Decision list (the contract)

Each decision records its rationale. **D-numbers are referenced by the
test catalogue (§6) and milestones (§8).**

### D1 — `StateCommand` surface

Keep `RenameBoard { title }` unchanged. Add exactly four variants,
none carrying a board id (§3.4):

```rust
SetBoardColor    { color: Color }        // lenient domain value; CLI validates (D10)
SetBoardArchived { archived: bool }      // one verb, both directions (SetTaskDone pattern)
DeleteBoard      {}                      // destructive ceremony (D5)
ResetBoard       {}                      // local repair ceremony (D6)
```

Rationale: three small verbs beat one sparse `UpdateBoard{changes}`
here because (a) `RenameBoard` already exists as the specific-verb
shape and renaming it would churn CLI/REPL/IPC-adjacent surfaces for
nothing, (b) one-behavior-one-test maps 1:1 onto small verbs, and (c)
all three edit verbs converge to the *same* outbox op (D4), so the
domain catalogue stays the only place verb granularity exists.

`CommandOutcome::Applied` for all four (delete/reset receipts render
from the returned plan counts at the CLI layer via a small receipt
struct — `CommandOutcome` stays as-is; no new variants). `CommandError`
gains nothing: verbs on a tombstoned board surface `NoBoard` exactly
like `RenameBoard` does today (the "edit against tombstone" class is
already covered by the live-board guard; archiving does **not** block
commands — `live_board()` filters `deleted` only, matching today).

### D2 — clock semantics (`BoardClocks` completion)

```rust
pub struct BoardClocks { pub title: DateTime<Utc>,
                         pub color: DateTime<Utc>,
                         pub archived: DateTime<Utc>,
                         pub deleted: DateTime<Utc> }
```

(#132's field list, verbatim.) Semantics, mirroring `LabelClocks`/
`StackClocks`:

- **Local verb** stamps only its own field clock with the injected
  `now`; a local color edit must not defend (or clobber) the title.
- **Adoption** (`adopt_board_if_newer`, `adopt_remote_board`) stamps
  *all four* clocks with `remote.last_modified` — the existing
  whole-entity behavior, unchanged.
- **Field-wise merge** (`merge_board`): the title defense generalizes
  to a per-field verdict table — title, color, archived each keep the
  local value while `local_clock ≥ remote.last_modified`… strictly:
  remote wins ⟺ `remote.last_modified > local_clock` (ADR 0004 rule,
  per field now). **`deleted` is special**: the tombstone defense uses
  the deleted clock the same way, and additionally the tombstone
  *never clears* without an explicit restore verb (none exists) —
  a pulled live board does not resurrect a locally-deleted board
  whose `clocks.deleted > remote.last_modified` (D7 edge case).
- **Echo** (`adopt_board_after_push`): each clock advances to
  `max(local_clock, echo.last_modified)` (title already does; extend
  to all four).
- Pre-existing rows keep behavior bit-for-bit: the migration defaults
  the three new clocks to `BoardClocks::epoch()` (0002's mechanism),
  so remote stamps strictly newer than `remote_seen` keep winning.

LWW meaning for bumping: a bumped field clock is a *veto* against
same-stamp-or-older remote content for that field only; the full-send
PUT still carries all fields (Deck forces full-send), so concurrent
remote edits to *other* fields can be overwritten by our push if they
land between our last pull and our push — the entity-granularity
hazard ADR 0004 already documents for every entity. Not new; recorded
as a risk (§9 R2), not fixed here.

### D3 — storage actions & totality (chunk 8 contract)

Exactly **one new `PersistenceAction`**:

```rust
/// Clear the local content of one board: stacks, tasks, labels,
/// outbox entries, sync validators, and reminder facts (rules,
/// deliveries, snooze links, mutes) of its tasks. The board row
/// itself (identity, binding, clocks, remote_seen) survives.
ClearBoardContent(BoardId)
```

Used by `ResetBoard` (D6). Rationale: a populated board would
otherwise emit hundreds of tombstone upserts + op removals; the bulk
action is one statement per table, atomic, and expressible in SQL
exactly (subqueries by board). Chunk 8's contract: the variant lands
**in the same commit** as its `apply_actions` (domain) and
`execute_batch` (sqlite) arms — both matches are compiler-exhaustive,
so totality is proven by construction (a build that persists the
action everywhere or not at all); the domain contract-test enumerator
(`test_support/contract.rs`) gains the arm in the same commit too.

Everything else reuses existing actions: board upserts
(`UpsertBoard`) for color/archive/delete; `EnqueueOp`/`FailOp` for the
outbox; `UpsertValidators` untouched.

SQLite shape for `ClearBoardContent` (order matters within the batch's
transaction): delete `outbox` rows whose `board_id = ?` **or** whose
task/stack/label id columns reference the board's entities
(subqueries), then `tasks` (via stack join), `stacks`, `labels`
(by board), reminder rules/deliveries/snoozes/mutes of the board's
tasks, and `sync_metadata` rows for the board's validator keys. Board
row and `sync_status` survive. All by board id / subquery — no
"delete everything" shortcuts (§3.4).

### D4 — sync-ability per verb (recorded reasons)

| Verb | Op queued | Wire call | Reason |
|---|---|---|---|
| `RenameBoard` (existing) | `UpdateBoard` (renamed, D5) | PUT full changeset | unchanged behavior |
| `SetBoardColor` | `UpdateBoard` | same PUT | Deck PUT carries color; full-send makes the op field-agnostic |
| `SetBoardArchived` | `UpdateBoard` | same PUT | same — archived rides the PUT |
| `DeleteBoard` | `DeleteBoard` (new) when a live board exists | `DELETE boards/{id}` (bound only) | mirrors `DeleteStack`: op queued unconditionally, executor synthesizes completion when unbound (uniform model; drains on next cycle under a connector, like standalone stack deletes today) |
| `ResetBoard` | none | none — **local-only** | remote is the source of truth for a bound board's content; pushing tombstones would *delete* remote content, which is `DeleteBoard`'s job, not a repair's (D6) |
| `CreateBoard`/`BindBoard` (existing) | none | n/a | unchanged (publish ceremony owns remote creation) |

No verb touches `SyncStatus`/`last_updated` (§3.4/F3-04).

### D5 — `LocalOp` rename + new op; planner; executor

- `LocalOp::RenameBoard` → **`LocalOp::UpdateBoard`** (same payload).
  The push has full-sent title+color+archived since its introduction;
  with three verbs converging on it the name must tell the truth.
  `MaterializedOp::RenameBoard` → `MaterializedOp::UpdateBoard`
  likewise. One mechanical-rename commit series first (chunk-1
  style), so the feature commits diff against honest names.
- New `LocalOp::DeleteBoard(BoardId)`; storage tag `delete_board`
  (rides the `board_id` column).
- Storage tag for the rename: write `update_board`; a migration
  backfills `rename_board` rows (D8). Decode accepts **only**
  `update_board` after the migration — one representation, no alias
  drift (the 0003 precedent backfilled rather than aliased).
- `plan_board_groups` gains delete-dominance, mirroring stacks: a
  queued `DeleteBoard` for a bound board (binding is *kept* on the
  tombstone until the push confirms — R9c) materializes
  `MaterializedOp::DeleteBoard` and subsumes the board's other ops;
  a `DeleteBoard` for an unbound board (or any board-op group on an
  unbound board) is a `Noop` group (synthetic completion). The
  existing bound+live gate stays for `UpdateBoard` groups.
- Executor: `MaterializedOp::UpdateBoard` arm is today's arm renamed;
  new `MaterializedOp::DeleteBoard` arm mirrors `DeleteStack`
  (`client.delete_board(target, id)`; echo `RemoteEcho::Board`;
  `PushCtx { existing: true, delete: true }` classification). The
  unbound case (`overlay` has no board → it always has one when
  bound; guard as `all_applied` for safety) mirrors the
  `DeleteStack` `None => all_applied` shape.

### D6 — `ResetBoard` semantics (the #171 "reset" half)

**What reset clears** (all scoped to the live board, via
`ClearBoardContent`): stacks, tasks, labels (rows — a reset is a
local repair, not a tombstone ceremony: nothing is pushed, so
tombstones would be dead weight), the outbox (pending ops address
entities that cease to exist), **the sync validators** (mandatory:
with warm validators the next conditional pull could 304 and serve
nothing/stale cache content — the reset would silently not
re-materialize; verified: the actor sends conditional headers only
when a persisted validator exists, so clearing them forces an
unconditional refetch), and the board's reminder facts (rules,
deliveries, snooze links, mutes — orphaned otherwise, since re-adoption
mints fresh local ids).

**What reset keeps**: the board row (id, title, color, archived,
binding, clocks, `remote_seen`) and `sync_status`. The connect session
survives; a bound board re-materializes from the next pull under
fresh local ids (R1 adoption path); an unbound board is simply empty.

Idempotence/repeats: on a board whose content, outbox, validators and
reminder facts are all already empty, the plan is zero actions →
`CommandOutcome::NoOp` (house rule: nothing changed, nothing
persisted); otherwise `Applied`. A board that doesn't exist → `NoBoard`.

### D7 — `DeleteBoard` semantics (the #171 "delete" half)

Local, mirroring `DeleteStack` + the pipeline's `cascade_board`
semantics applied at command time:

1. Board tombstone: `deleted = true`, `clocks.deleted = now`;
   **binding kept** until the push confirms (R9c; `tombstone_board`'s
   binding-clear stays on the observed-absence path only).
2. Cascade: every live stack of the board, every live task of those
   stacks, and every live label of the board is tombstoned with
   `clocks.deleted = now`; the D-c reminder-rule cascade fires per
   task (`task_rule_deletes`). This mirrors the remote R6 cascade so
   the local view matches what the server will do.
3. **Pending ops are cancelled, not pushed**: every queued op
   addressing the board or any of its children is `FailOp`-ed.
   Rationale: Deck's board DELETE cascades server-side; pushing
   per-child deletes would chase 404s through the board's corpse
   (pipeline's `cascade_board` cancels children ops for exactly this
   reason — R6 parity).
4. Queue exactly one op: `LocalOp::DeleteBoard(board)` (always —
   bound or unbound; D4's uniformity note).
5. Terminal intent: repeating `DeleteBoard` is an accepted no-op.
   `NoBoard` when no board exists at all.

Push finalize: the DELETE echo carries `deletedAt`; the pipeline's
board-echo arm grows a `LocalOp::DeleteBoard` branch that adopts the
echo **preserving the tombstone** (new small merge helper, e.g.
`adopt_board_after_delete_push`: `deleted: true`,
`clocks.deleted = max(local, echo.lm)`, binding **cleared**, `remote_seen = echo.lm`) — the existing
`adopt_board_after_push` hard-codes `deleted: false` and must not be
reused for deletes.

Resurrection guard (D2's deleted-clock defense): a delete whose push
fails (or lands, then a racing pull shows a live board) must not be
undone by `merge_board` — the pulled board is filtered through the
`deleted` clock exactly like the other fields.

### D8 — storage migration (one file)

`migrations/0005_board_edit_verbs.sql`:

1. `ALTER TABLE boards ADD COLUMN ck_color TEXT NOT NULL DEFAULT
   '<epoch>';` — same for `ck_archived`, `ck_deleted` (epoch default
   = `BoardClocks::epoch()`'s RFC-3339 text, 0002's mechanism).
2. `UPDATE outbox SET op_kind = 'update_board' WHERE op_kind =
   'rename_board';` (0003 backfill precedent).

One migration, not two: both statements ship with one feature, are
independently idempotent, and a split buys no rollback story sqlite
migrations don't have anyway. Ripple: `repo.rs` board row/upsert/
decode gain three columns; `codec.rs` writes/parses `update_board` +
`delete_board`; `connect.rs` fixtures + `tests/roundtrip.rs` update.

### D9 — UI: stack creation (#304)

Dumb-presentation-client throughout — the UI layer dispatches, never
mutates:

- **Action**: `ActionId::StackCreate` (`"stack.create"`, enabled,
  contexts `Board` + `StackDetail`, Structure-ish category placement
  next to TaskCreate in the registry order).
- **Router**: `StackCreate` → `UiEffect::BeginCreateStack`. With no
  live board (`presentation::live_board()`) the router refuses (same
  refusal class as TaskCreate on an empty board) — a never-booted
  session offers nothing to create a stack *on*.
- **Flow**: `Flow::CreateStack` + `begin_create_stack()` (insert mode,
  buffer cleared); `commit_edit` grows an arm dispatching
  `StateCommand::CreateStack { title, order }` where `order` comes
  from a new pure helper `append_stack_order(state)` = live stacks'
  max canonical order + 1 (mirrors `append_order`; #304's "sensible
  default order appended after existing stacks"). Empty buffer
  commits nothing (D9 rule 1 territory — flow just ends).
- **Empty-board edge** (the point of the chunk): unlike task-create,
  `BeginCreateStack` needs **no** target stack — it works on a live
  board with zero stacks. The board view's empty state must present
  the affordance (menu always does; the empty-board visual hint joins
  the existing empty-state composition).
- **Keymap**: vi preset `O` (Board context — the vim "open" family);
  default preset `S` (Board); emacs preset left unbound-and-menu
  -invokable unless a free natural stroke exists — the preset
  collision tests arbitrate; no binding is stolen from an existing
  action.
- **Rendering**: reuse the existing create-row composition (same
  inline entry line as task create); no new widget classes.

No board field enters `ViewState`/`IdTables` (§3.4/F7-05). GUI
affordances for color/archive/delete/reset are **out of scope** (no
issue asks; CLI owns them this chunk).

### D10 — CLI verbs + confirmation ceremony

`BoardsCmd` grows (argv rides `Request::Exec`; zero IPC change):

- `boards color <COLOR>` — six-lowercase-hex, validated by the
  existing `parse_color` (its "labels only" doc comment generalizes:
  same grammar, same reason — a malformed color is a permanent push
  rejection; adopted garbage keeps the lenient storage + push
  fallback, new input gets boundary validation).
- `boards archive` / `boards unarchive` — two subcommands, one
  `SetBoardArchived` each (reads better than `--undo`).
- `boards delete [--confirm]` and `boards reset [--confirm]` — the
  ceremony (#171): without `--confirm` the command refuses with a
  typed CLI-layer error (`ConfirmationRequired`) whose rendering
  states what would be destroyed/repaired (stack/task/label counts,
  pending-op count, and for delete a remote-destruction warning when
  the board is bound); with it, the engine command runs and the
  receipt reports the landed counts. Confirmation lives in the CLI
  layer only — the domain command stays pure and total (a library
  caller makes its own policy).
- Output renderers (text + `--json`) and REPL completion entries join
  the existing patterns.

### D11 — docs footprint

- **ADR 0004 amendment** (2026-10-18): boards complete the per-field
  merge — color/archived join the title defense; `deleted` gets the
  tombstone defense (D2/D7); `LocalOp::UpdateBoard` renamed to tell
  the full-send truth; reset is local-only with the validator-clear
  requirement recorded (the 304 hazard is a real trap worth a
  sentence in the ADR).
- Doc comments at every renamed/extended seam (entities, outbox,
  push, executor arm, codec tags).
- architecture.org: **untouched** (chunk 12 owns the rewrite; this
  chunk's claims land in the ADR, which chunk 12's pass will pick
  up). Recorded deliberately.

## 5. Per-crate change map (stage-2 execution order = §8 milestones)

| Crate | Changes |
|---|---|
| `taskboard-domain` | `BoardClocks` +3 fields; 4 `StateCommand` arms + no-op/rejection semantics; `LocalOp` rename + `DeleteBoard`; `op_targets_task`/`op_to_columns`-adjacent match arms; `plan_board_groups` delete-dominance + `MaterializedOp` rename/new; `merge.rs` per-field board merge + delete-echo helper + resurrection defense; `pipeline.rs` echo arm + `pending_board_intent` update; `PersistenceAction::ClearBoardContent` + `apply_actions` arm; `test_support` (builders, strategies, contract) |
| `taskboard-state` | nothing mechanical — engine machinery is generic; new insta scenarios + routing tests; snapshot fixtures churn (BoardClocks fields, `update_board` tags) |
| `taskboard-storage-sqlite` | migration 0005 (schema + backfill); `repo.rs` board columns + `ClearBoardContent` arm + `load()`; `codec.rs` tags; `connect.rs` fixtures; roundtrip/contract/actor tests |
| `taskboard-sync-nextcloud` | executor arm rename + `DeleteBoard` arm; planner consumers unchanged (domain-owned); wiremock tests for the delete push + existing suites' renames |
| `taskboard-app` | `BoardsCmd` verbs + handlers + receipts + renderers + completion; confirmation layer; CLI tests |
| `taskboard-ui-slint` | `ActionId::StackCreate` + registry row; router arm; `Flow::CreateStack` + `begin_create_stack` + `append_stack_order` + `commit_edit` arm; empty-board affordance; keymap presets; view_state/router/adapter tests |

## 6. Test catalogue (one behavior, one test, lowest layer)

### 6.1 Domain unit (command.rs / merge.rs / push.rs / pipeline.rs / persistence.rs)

- `set_board_color` stamps `clocks.color` only; title/archived/deleted
  clocks untouched; enqueues `UpdateBoard` (typed
  `PersistenceAction` assertions).
- `set_board_color` same color → `NoOp`, zero actions.
- `set_board_color` without a live board → `NoBoard` (typed
  `matches!`).
- `set_board_color` on a tombstoned board → `NoBoard`.
- `set_board_archived` both directions: same three-guard shape +
  clock isolation (mirror of the color trio — one test each).
- `set_board_color`/`archived` on an *archived* board succeeds
  (archived boards stay commandable).
- `delete_board` on a populated board: typed plan assertions — board
  tombstone + `clocks.deleted` stamped; children tombstoned (stacks,
  their tasks, labels); per-task reminder-rule deletes; **all**
  pending ops of board+children `FailOp`-ed; exactly one
  `EnqueueOp(DeleteBoard)`.
- `delete_board` repeat → `NoOp`; absent board → `NoBoard`.
- `delete_board` on unbound board: same tombstone semantics, op still
  queued (D4 uniformity; planner noop-completes it).
- `reset_board`: content/outbox/validator/reminder rows cleared via
  exactly one `ClearBoardContent`; board row fields (title, color,
  binding, clocks, `remote_seen`) present in the upsert-free plan —
  i.e. **no** `UpsertBoard` action, the row is untouched.
- `reset_board` on an already-empty board → `NoOp` (zero actions).
- `reset_board` absent board → `NoBoard`.
- merge: `merge_board` per-field verdicts — remote newer on one field
  adopts only that field (four fields × both directions, property-
  style where cheap); epoch clocks ⇒ remote wins (0002 parity).
- merge: deleted-tombstone defense — local `clocks.deleted > lm`
  keeps the tombstone against a pulled live board (the D7
  resurrection guard).
- merge: `adopt_board_after_push` advances all four clocks to
  `max(local, lm)`; `adopt_board_after_delete_push` keeps `deleted`,
  stamps the deleted clock, clears the binding.
- push planner: `DeleteBoard` dominance (subsumes a queued
  `UpdateBoard`); bound gate materializes delete; unbound board →
  `Noop` group for both op kinds; planner partition totality holds
  (every op id in exactly one group).
- pipeline: delete-echo finalize (binding cleared, tombstone kept);
  reset-then-pull re-adoption (fresh ids; validators absent ⇒ no
  conditional skip — expressed as: a snapshot with unchanged stamps
  still adopts when local content is gone… scoped to what
  `apply_sync_report` can see; the 304 mechanics are the actor's,
  covered in 6.4).
- persistence: `ClearBoardContent` in `apply_actions` (memory) —
  clears exactly the D6 sets, keeps the board row + sync status;
  contract-test enumerator updated.

### 6.2 Engine (`taskboard-state`)

- Routing only (behaviors live in 6.1): one engine-suite test per new
  verb asserting the receipt surfaces (`Applied`/`NoOp`) and the
  storage batch landed (repo snapshot). 
- Insta S-series additions (published `AppState` + persisted
  `PersistedState` pairs):
  - `s_boards_color_archive`: create board → color → archive →
    snapshots (clock fields visible, one `update_board` op row);
  - `s_boards_delete_cascade`: populated (2 stacks, 3 tasks, label,
    queued ops) → delete → snapshot (tombstones, cancelled ops gone,
    one `delete_board` op);
  - `s_boards_reset`: populated + validators → reset → snapshot
    (empty content/outbox/validators/reminders, board row intact).
- Existing snapshots updated for the `BoardClocks` shape and
  `update_board` tag churn (reviewed; F4-03 accepted).

### 6.3 Storage (`taskboard-storage-sqlite`)

- Migration: a db seeded with a `rename_board` outbox row + legacy
  board row migrates to 0005 and loads back as `UpdateBoard` with
  epoch new-clocks (roundtrip test pattern).
- `execute_batch` `ClearBoardContent`: scoped clears per D3, board
  row + sync status survive; validator rows for the board's keys
  removed.
- Codec: `update_board`/`delete_board` round-trips; `rename_board`
  tag rejected post-migration (decode-rejection suite gains the
  legacy-tag case — it *is* unknown now, by design).
- Contract suite (`tests/contract.rs`): the new action joins the
  memory↔sqlite parity walk.

### 6.4 Sync (`taskboard-sync-nextcloud`)

- Wiremock: `DeleteBoard` push issues exactly one
  `DELETE /boards/{id}`, adopts the echo (tombstone finalize at the
  engine-report level or executor-outcome level per the existing
  suite's layer), completes + subsumes the board's queued ops.
- Wiremock: delete 404 → `RemoteMissing` classification (delete ctx);
  delete 403 → tombstone-vs-failed per the dead-board rules
  (chunk 2's pinned semantics — new test only if the board arm
  diverges from the stack arm's classification, else assert via the
  shared `classify` behavior).
- Suite renames: `RenameBoard` → `UpdateBoard` across
  `push_exec/tests.rs` fixtures (mechanical).
- Actor: no new actor tests beyond the planner/executor coverage
  (routing unchanged); the existing hermetic suites stay green
  unmodified apart from renames.

### 6.5 App/CLI (`taskboard-app`)

- `boards color`: sets + rejects malformed (typed CLI error, usage
  exit); snapshot of receipt rendering (text) where the suite already
  does so.
- `boards archive`/`unarchive`: verb → `SetBoardArchived` dispatch,
  receipt; unarchive on live board is a no-op receipt.
- `boards delete` without `--confirm` → typed refusal, state
  untouched (repo-snapshot equality); with `--confirm` → cascade
  landed (typed counts in the receipt struct — JSON shape asserted
  structurally).
- `boards reset` same two-sided confirmation test + content-cleared
  assertion.
- REPL/completion: new verbs appear (existing completion test
  pattern).

### 6.6 UI (`taskboard-ui-slint`)

- view_state: `begin_create_stack` → insert-mode outcomes; commit
  with a title dispatches `CreateStack` with `append_stack_order`
  (typed `StateCommand` equality); commit with empty/whitespace title
  dispatches nothing; flow ends either way.
- `append_stack_order`: empty board → 0; populated → max+1 (unit).
- router: `StackCreate` → `BeginCreateStack`; refused with no live
  board (typed `DispatchOutcome`).
- registry: `StackCreate` enabled, non-reserved (the structural
  reserved-set tests carry it automatically).
- keymap: preset additions don't collide (existing collision tests);
  vi `O`/default `S` bind (or the recorded fallback if emacs stays
  unbound).
- adapter (fake-engine): the flow's command reaches the commands
  channel (dispatch seam, one test).
- No test asserts slint-internal state (dumb-client rule; house
  harness pattern).

## 7. Documentation & ADR

- ADR 0004 amendment per D11 (fields, deleted defense, UpdateBoard
  rename, reset's validator-clear hazard).
- `CHANGELOG.md` unreleased section: the four verbs + GUI stack
  creation (house voice) — if the file's structure has an Unreleased
  anchor at stage-2 time; else follow the existing pattern.
- Doc comments per §5; no architecture.org edits (D11).

## 8. Milestone order (stage 2 executes these; gates green after each)

1. **M1 — domain, mechanical first** (chunk-1 style): the
   `RenameBoard`→`UpdateBoard` rename commit series renames **Rust
   symbols only** across domain/sync/storage-codec/tests — the codec
   keeps writing and parsing the `rename_board` wire tag, so every
   intermediate commit builds *and* loads existing databases. The
   wire-tag flip (`update_board`) happens in M3, together with the
   migration backfill, in one commit. Then M1's feature commits:
   `BoardClocks` fields, the four verbs, merge/planner/pipeline
   arms, `ClearBoardContent` (totality in-commit).
2. **M2 — engine**: routing tests + insta scenarios + snapshot churn.
3. **M3 — storage**: migration 0005 (clocks + tag backfill), repo/
   codec/connect updates, storage tests.
4. **M4 — sync**: executor delete arm + wiremock suite; chunk-2's
   pinned tests green unmodified.
5. **M5 — CLI**: verbs, ceremony, completion, tests.
6. **M6 — UI**: action/flow/router/presets/empty-board affordance +
   tests.

Dependency note: M1's per-commit buildability forces the codec-tag
discipline above (Rust-name rename ≠ wire-tag flip; they meet in M3).
Stage 3 re-verifies each milestone's gates.

## 9. Risks & open edges

- **R1 merge-model edge — delete vs pull race**: a failed delete push
  leaves a locally-tombstoned board that the next pull sees as live.
  The D2 deleted-clock defense is the guard; it is pinned by an
  explicit domain test (6.1). Residual: a remote edit *newer* than
  our tombstone legitimately un-deletes nothing — the tombstone stays
  until an explicit restore verb exists (none does; recorded in the
  ADR as intended behavior).
- **R2 LWW entity-granularity**: full-send PUT can overwrite a
  concurrent remote edit of a *different* field that lands in our
  pull→push window. Inherent to Deck + ADR 0004 (documented
  "Negative" consequence); not new, not fixed here.
- **R3 Deck mapping gap**: none for our fields (title/color/archived
  round-trip; malformed stored color falls back at push as today).
  `restore_board` exists un-wired — out of scope with a trail (§1);
  if wanted, a new issue, not a drive-by.
- **R4 snapshot churn**: every board-bearing snapshot changes
  (BoardClocks shape, op tag). Accepted (F4-03 narrowing is
  next-wave); stage 3 reviews new snapshots for transient-state
  pinning (timestamps: the suite's fixed-clock harness keeps them
  deterministic).
- **R5 worktree/base drift**: chunk 11 (CI) and chunk 12 (architecture
  .org) PRs are open while this chunk runs. This chunk touches
  neither surface (D11 keeps us out of architecture.org; no workflow
  edits). Chunk 13 merged — no interaction. Rebase per dispatch §1
  mechanics if `main` moves underneath.
- **R6 keymap collisions**: preset strokes are a guess until the
  collision tests run; the fallback (menu-only in a preset) is
  recorded in D9 so M6 cannot stall on it.
- **R7 confirmation bypass via REPL/IPC**: `Request::Exec` carries
  raw argv — the daemon parses argv, so the `--confirm` flag is
  enforced identically over IPC; a library caller bypassing the CLI
  is by-design policy-free (D10). No security claim — local
  single-user trust model (ADR 0012).

## 10. Acceptance (from dispatch §16, made concrete)

- The four verbs exist with per-field clock semantics per D1/D2;
  outbox/local-op entries per D4 (or the recorded local-only reason).
- Deck round-trip for color/archived decided (no gap — §3.2; #207
  consciously not needed, cited).
- GUI stack creation dispatches through the dumb-presentation path
  (D9), works on an empty board.
- Insta catalogue per 6.2; sync actor tests prove pushability per
  6.4; CLI verbs per D10.
- Full AGENTS §10 gate suite green after every milestone.
- PR body: summary + `Closes #132` + `Closes #171` + `Closes #177` +
  `Closes #304` (stage 3 ships it).

# [Plan] Multi-board Chunk 3 — Engine Ceremonies at N: unbind, reconcile, parameterised destruct

- **Date**: 2026-10-19
- **Kind**: stage-1 TDD-first implementation plan (dispatch plan §4,
  prompt 3.1). No code in this commit.
- **Branch / worktree**: `feat/state-board-ceremonies` at
  `../taskboard-WT-feat-state-board-ceremonies`, cut from `origin/main`
  `9bd4d10` ("chore(changelog): Unreleased lines for the board-scoped
  command vocabulary") — chunk 2 (PR #362) merged 2026-10-19T02:16Z.
- **Normative sources** (MD-7 rank): implementation plan
  `.artifacts/plans/2026-10-18-multi-board-wave-implementation-plan.md`
  §2 (DF-1, DF-5, DF-10 normative; DF-2/DF-4/DF-9 context), §3 chunk 3,
  §4, §5, §6; design
  `.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`
  §3.5 (all five sub-questions), §3.1 (the `CommandError` ceremony list,
  the caller-side burden), §5 ACCEPTED record; dispatch plan
  `.artifacts/plans/2026-10-18-multi-board-impl-dispatch-plan.md` §1
  (execution model, MD-2…MD-9), §4 (chunk 3 covers/acceptance/traps).
- **Line numbers below are plan-day references** (MD-8); symbols and
  finding IDs are the stable keys. Every symbol was re-located on this
  base at plan time; the seam table in §2 is the 2026-10-19 truth.

## 1. Scope

**In** (impl plan §3 chunk 3, normative):

- Domain `command.rs`/`messages.rs`: new `StateCommand::UnbindBoard
  { board }`, `StateCommand::ReconcileByTitle { board }`; new
  `CommandError::RemoteBoardInUse { remote, board }`; the bind-wipe
  rule inside `BindBoard`'s planner arm with a counted outcome; the
  push-side dead-letter rule for update-class ops on wiped refs
  (`apply_push_report`); a pure reconcile module (matcher + local-local
  clock merges); `PersistenceAction::DeleteSyncStatus(BoardId)`.
- State: routing only — the two new arms ride `execute_command`'s
  plan→apply→interpret path; `interpret` handles the new action via the
  domain's shared batch semantics.
- Storage: `DeleteSyncStatus` SQL + repo arm + contract/roundtrip tests.
- Sync actor: the **target-liveness guard** (D3-2 — the one actor
  touch, a recorded deviation): a sticky `LoopState.target` that no
  longer matches any persisted binding is evicted (with a cache clear),
  so unbind is not resurrected by the next poll.
- App `commands/boards.rs`: `select_on` folded to engine-refusal
  pass-through + the mint-and-bind fresh path + the broadcast-nudge
  replacement of `send_set_board`; `wait_for_adoption` parameterised by
  board watching that board's `SyncStates` row; `publish_on` switched
  to the same nudge + board-row wait; new `unbind_on` / `reconcile_on`
  ceremonies with loud flags; `read_scope` extended with
  `descendant_refs`; `commands/mod.rs` mirrors + `boards_line` arms;
  `cli.rs` verb shapes; `output.rs` receipts/renderers; `repl.rs` line
  grammar for the two new verbs.
- Bookkeeping: this chunk's `CHANGELOG.md` Unreleased line.

**Out** (non-goals, impl plan §3 chunk 3 + round §1):

- No actor/routing redesign beyond the D3-2 guard (chunk 4's derived
  board set + `SetBoard` retirement); no plural connectors or config
  shape (chunk 5); no CLI/REPL connector verbs or `boards use`
  (chunk 6 — the REPL keeps chunk 2's interim first-live-board resolve
  for the new verbs' defaults); no UI changes (chunk 7); no tracker
  edits (stage 3's PR body closes #170/#179 per MD-4).
- `SetBoard` **stays compile-clean** — variant, both sync-actor handler
  arms, and `App::sync_commands()` — but nothing sends it anymore
  (§4.10 inventories the removal for chunk 4).
- No fuzzy/normalized-key matching growth, no dry-run flags (the
  design's out-list; stage-3 mandate).
- No `insta` regeneration pass is *planned* (D3-13): the new arms are
  additive and existing ceremony snapshots bind ref-less-descendant
  boards. Any snapshot diff that appears is a finding to investigate,
  never blindly accepted.

## 2. Re-verified seam table (2026-10-19, base 9bd4d10)

| Symbol | Home (plan-day line) | Chunk-3 role |
|---|---|---|
| `select_on` | app `commands/boards.rs:633` | rewrite: resolve-or-mint → `BindBoard` pass-through → nudge → board-row wait; INTERIM comment at :640 dies |
| `select_prelude` | boards.rs:607 | now returns the validated listing entry (title feeds the mint) |
| `publish_on` / `send_set_board` | boards.rs:486 / :563 | nudge swap; `send_set_board` deleted |
| `wait_for_adoption` | boards.rs:710 | parameterised by `BoardId`, watches `state.sync.get(board)` |
| `read_scope` / `BoardScope` / `destruct_on` | boards.rs:176 / :165 / :250 | already board-parameterised (chunk 2); gains `descendant_refs` |
| `resolve_board` | app `commands/mod.rs:386` | refuses zero-or-≥2 (`AmbiguousBoard`, :152); select needs the zero-live case → new sibling helper (D3-7) |
| `CommandError` (domain) | domain `command.rs:56` | `UnknownBoard`/`BoardDeleted`/`BoardAlreadyBound{board,bound}` present (chunk 2); +`RemoteBoardInUse{remote,board}` |
| `live_board_of` / `existing_board` | command.rs:139 / :152 | reused by the new arms |
| `BindBoard` planner arm | command.rs:839 | gains wipe + `RemoteBoardInUse`; same-remote no-op already lands (keep) |
| `CommandOutcome` | command.rs:101 | +`Bound{wiped_refs}`, +`Unbound{stranded_ops}`, +`Reconciled{…}` (D3-6); not serde, no IPC surface (verified: zero `CommandOutcome` refs under `app/src/ipc/`) |
| `RemoteStackRef`/`RemoteCardRef`/`RemoteLabelRef` | domain `ids.rs:141/121/150` | self-identifying refs; `.board` is the wipe discriminator |
| `SyncStates` (`get`/`aggregate`) | domain `state.rs:100` | the per-board watch target (chunk 1) |
| `SyncCommand` | domain `messages.rs:290` | `SetBoard` stays; `SyncNow` is the nudge payload |
| `StateCommand::RequestSync` | messages.rs:221 | the ceremony's nudge entry (`forward_sync_now` fans out; state `actor.rs:525`) |
| `forward_sync_now` / `mark_bound_syncing` | state `actor.rs:600` / :729 | unchanged; the nudge marks every bound board (interim, chunk 4 scopes it) |
| `apply_push_report` | domain `pipeline.rs:376` (Rejected at :1187) | dead-letter rule lands here (D3-3) |
| `op_board_id` | pipeline.rs:75 | the op→board attribution (unbind's stranded count) |
| `PersistenceAction` | domain `persistence.rs:177` | +`DeleteSyncStatus(BoardId)` (:198 neighbourhood) |
| `apply` (batch semantics) | persistence.rs:239 | handles the new action once; fakes + sqlite inherit |
| merge helpers | domain `merge.rs` (`merge_task`:170, `merge_stack`:351, `merge_label`:521) | local-vs-remote merges; reconcile adds local-local clock merges (D3-4) |
| sync actor target block | sync `actor.rs:328–354`; `LoopState` :155; `PullCache` :142 (single-board, cleared on retarget :204) | D3-2 guard + cache clear |
| sync actor dead-board carve-outs | actor.rs:703–766 (design anchors) | untouched; tombstone-drain pin (D3-15) |
| `push_exec` outcomes | sync `push_exec/outcomes.rs` (`reject_primary`:103) | `Rejected{LocalData}` producer — unchanged; the engine decides dead-letter |
| `App::sync_commands` | app `bootstrap.rs:178` | survives unused (lib surface, D3-12) |
| `boards_line` | app `commands/mod.rs:1104` | +2 verb arms |
| `BoardsCmd` | app `cli.rs:126` (`Select`:129, `--board` flag pattern :160+) | +`Unbind`/`Reconcile`; `Select` gains `--board` |
| receipts | app `commands/output.rs` (`SelectReceipt`:470, `BoardDestructReceipt`:496) | +`UnbindReceipt`/`ReconcileReceipt`; `SelectReceipt` unchanged shape |
| REPL boards grammar | app `repl.rs:350` (`BoardsCmd::Select` construction) | +2 line verbs, interim default resolve |
| ceremony tests | app `tests/repl.rs` (r11:510), `tests/cli_connect.rs`; state `tests/engine.rs` (a15:1249), `tests/snapshots.rs` | must-not-break + extension homes |

## 3. Behavioural architecture (the ceremonies at N)

### 3.1 The bind ceremony (`boards select`, `boards publish`) after this chunk

```
select <remote> [--board <uuid>] [--bind-local-board]:
  prelude: fresh listing pre-validation (returns the entry)   [unchanged, +entry]
  resolve local board:
    --board given           → that board (UnknownBoard passes through on dispatch)
    exactly one live board  → it
    zero live boards        → FRESH PATH: CreateBoard(entry.title, default color)
    ≥2 live, no --board     → AmbiguousBoard (typed, chunk 2's variant)
  if resolved board is live and unbound and !bind_local_board
                            → LocalBoardExists (ceremony gate — D3-7)
  dispatch BindBoard{board, remote}                          [ONE rule, ONE home]
    NoOp   (same remote)    → immediate receipt from current state (no wait — D3-7)
    Bound{wiped_refs}       → nudge: engine.execute(RequestSync)     [no SetBoard]
                             → wait_for_adoption(board, startup_row) [that board's row]
                             → ReconcileByTitle{board} (automatic, ungated — D3-8)
                             → receipt (+ wipe count surfaced)
    Err(rejection)          → pass-through: UnknownBoard / BoardDeleted /
                              BoardAlreadyBound{board,bound} / RemoteBoardInUse{remote,board}
```

`publish` keeps its own prelude (remote create) then the same tail:
`BindBoard` (a bound board refuses `BoardAlreadyBound` — the app-side
pre-check at boards.rs:503 folds into that pass-through), nudge,
board-row wait.

### 3.2 The unbind ceremony (`boards unbind`)

```
unbind [--board <uuid>] [--confirm]:
  resolve board (chunk 2 rules: default single-live; zero/≥2 → AmbiguousBoard)
  scope = read_scope(board) + descendant_refs        [one pre-count, one truth]
  !confirm → typed refusal stating: bound remote, pending ops that will strand,
             descendant-ref count (the refs a future different-remote rebind wipes)
  confirm  → dispatch UnbindBoard{board}
             → receipt: board id, ex-bound remote, stranded ops, kept refs
  engine: live_board_of (UnknownBoard/BoardDeleted refusals — a tombstoned
          board keeps its binding until the tombstone push finalizes (R9c);
          unbinding it would strand the remote delete: refused);
          already-unbound → NoOp (idempotent); else UpsertBoard(remote=None)
          + DeleteSyncStatus(board); outbox untouched (ops strand honestly);
          descendant refs untouched (same-remote rebind re-adopts by them).
```

### 3.3 The reconcile pass (`ReconcileByTitle`)

Runs (a) automatically after the bind ceremony's adoption-wait and
(b) manually via `boards reconcile` (idempotent). Pure domain planning
over `(app, outbox)`; see §4.6 for the algorithm.

### 3.4 Honest stranding — the three op fates

| Op class after unbind / bind-wipe | Fate | Visibility |
|---|---|---|
| ops of an unbound board (any class) | wait; drain on rebind | `pending_ops` per board climbs (outbox attribution, load-time derived) |
| create-class ops on wiped-ref entities | wait; re-create on the new remote once the owning stack re-binds (pull adopt + reconcile, or a fresh stack create) | stays queued |
| update-class ops on wiped-ref entities whose board **is** bound | dead-letter at the next push-report ingest (`Rejected{LocalData}` ⇒ `FailOp`, D3-3) | forecast counted in the bind receipt (D3-6/D3-9); exact at push time |
| update-class ops on ref-less entities of a **never-bound** board | wait (the ADR 0010 standalone precedent) | stays queued |
| ops on entities bound to a different remote | wait (chunk 4's ownership filter owns that world) | stays queued |

## 4. Settled decisions

Every open question the dispatch prompt named, settled. Numbered
D3-*; chunk-2's D2-* numbering continues.

### 4.1 D3-1 — the fresh-install select path mints and binds (no `SetBoard`-dependent adoption)

**Problem**: today's board-less `select` sends `SetBoard(remote)` so the
actor pulls that remote and RULE_R1 adopts it as a local board
(`pipeline.rs:438–450`). With the nudge replacing `SetBoard`, the actor
has no target source for an unbound remote — its fallback
(`actor.rs:332–338`) reads persisted bindings only — and a SyncNow on a
board-less db reports `NoBoard` and waits forever. Breaking fresh
installs is not an option; keeping a targeted send is forbidden.

**Decision**: the fresh path becomes
`CreateBoard(entry.title, DEFAULT_BOARD_COLOR)` → `BindBoard{fresh,
remote}` → nudge → board-row wait. The pull then merges **by binding**
(RULE_R6, not R1) and the epoch-clock local shell adopts the remote's
title/color/archived (the merge's field-wise LWW with default clocks).
`select_prelude` returns the validated listing entry so the mint uses
the remote's real title. This is exactly chunk 4's end-state shape
("a newly bound board is picked up by the owning connector's next read
with no message at all"), landed early because the ceremony needs it to
survive its own `SetBoard` exit. RULE_R1 board adoption stays in the
merge machinery (unchanged snapshots) but becomes unreachable from
ceremonies; the D3-2 guard removes the only other path there.

**Consequence pinned by test**: repeated `select` of the same remote on
a fresh install is the same-remote no-op (`NoOp` ⇒ immediate receipt).

### 4.2 D3-2 — sync-actor target-liveness guard (recorded deviation from the non-goal)

**Problem**: `LoopState.target` is sticky (`actor.rs:354`). After
`UnbindBoard` clears the binding, the next poll still targets the old
remote and RULE_R1 **resurrects the board as a new local entity** —
unbind would be visibly broken (#179 unverifiable).

**Decision**: in the cycle's target resolution, a sticky
`st.target` is kept only while some persisted board still holds it —
**including tombstoned boards** (R9c: a delete's binding survives until
the tombstone push finalizes, which is precisely the drain path D3-15
pins). Otherwise the target is evicted and `st.cache.clear()` runs (the
cache is single-board, `PullCache` at :142 — the same clear `SetBoard`'s
retarget performs; chunk 4 replaces both with per-board keys and the
derived set).

**Deviation record**: impl plan §3 chunk 3 says "no actor/routing
changes (chunk 4)". Reason for deviating: the #179 exit criterion
("unbind clears the binding and strands ops honestly") is false
without it — the poll resurrects the board; the guard is the minimal
slice of chunk 4's derived-set rule (validate targets against persisted
bindings each cycle) and is deleted by chunk 4's motion. It adds no
channel, no message, no topology change (DF-10 holds).

### 4.3 D3-3 — the dead-letter rule for update-ops on wiped refs

**Design text**: "the push's existing unknown-binding path dead-letters
them". **Verified truth**: the unknown-binding path produces
`Rejected { LocalData }` (`reject_primary`, outcomes.rs:103) and
`apply_push_report` dead-letters **only** `BadRequest`
(pipeline.rs:1187–1190) — everything else stays queued. So the rule must
be added, scoped so the other stranding classes survive.

**Decision**: in `apply_push_report`, a `Rejected { LocalData }`
outcome for an op whose entity row exists, is **ref-less**
(`remote == None`), is **update-class** (`UpdateTask | MoveTask |
AssignLabel | UnassignLabel | RenameStack | MoveStack | UpdateLabel`),
and whose **owning board is bound** ⇒ `FailOp` (dead-letter). Rationale
per class:

- *Ref-less + bound board* is the post-wipe signature. A never-bound
  board's entities are ref-less but their board is unbound ⇒ they wait
  (standalone/publish contract: "the first cycle drains the outbox").
- Entities bound to another remote have refs ⇒ they wait (interim
  multi-board honesty; chunk 4's ownership filter takes over).
- *Create-class* ops on ref-less entities wait: they re-create once the
  owning stack is re-bound (pull-adopt + reconcile, or the stack's own
  pending create landing first). Cancelling them would destroy data the
  design explicitly promises to re-create.
- *Delete-class* ops need nothing: unbound-entity deletes already
  complete synthetically (`all_applied`, outcomes.rs:83; planner Noop
  groups for missing rows).

**Why push-side and not bind-side**: bind-time cancellation would also
kill update-ops on entities that the immediately-following reconcile
re-matches (they become pushable the moment the survivor adopts the
ref, §4.6). Push-side, only the ops that are *still* unpushable at the
next cycle die — the design's exact claim. The bind receipt's op count
is therefore a **forecast** (an upper bound that reconciles downward);
the wipe-ref count is exact at bind time (D3-6). Pinned by tests:
forecast == eventual when nothing reconciles; forecast ≥ eventual.

### 4.4 D3-4 — reconcile survivorship: the local row adopts the remote identity

For each matched pair (local-only entity ⇄ remote-bound sibling that
the post-bind pull adopted as a fresh row):

- **Survivor = the local-only row.** It keeps the user's local id —
  reminder rules, snooze links, and any pending update ops address that
  id and stay live (update-ops become pushable once the ref lands).
- The survivor's new `remote` = the bound row's ref; `remote_seen` and
  clocks merge field-wise (D3-10); fields resolve by the newer clock
  (tie ⇒ the bound row's value — remote continuity, deterministic).
- **The pull-adopted duplicate row is neutralized**, not deleted by
  tombstone-with-ref (that would push a real remote delete): upsert it
  as `deleted = true, remote = None` — a ref-less tombstone whose
  delete group completes synthetically. It has no ops and no reminder
  facts (its id is fresh from the pull), so no cascade is needed.
- **Stack merges re-parent children**: every task under the absorbed
  stack is re-parented to the survivor stack (`UpsertTask` with the new
  stack id) — this is what makes "tasks match only within matched
  stacks" well-defined post-merge: the task pass groups tasks by
  *stack row*, so tasks only ever pair inside a merged (or natively
  shared) stack; same-title tasks in different stacks never pair.
- **Label merges re-point edges**: tasks whose label set references the
  absorbed label id are rewritten to the survivor label id.
- **Duplicate create-ops cancelled**: create-class ops addressing a
  survivor that just adopted a ref are `FailOp`s (otherwise the next
  push POSTs a duplicate card — the planner materializes creates for
  rows regardless of binding). Counted in the outcome. Update-class ops
  on the survivor stay and push the merged state.
- Ops addressing neutralized rows cannot exist (fresh ids) — no
  transfer machinery, no op rewriting (scope-creep guard).

### 4.5 D3-5 — `UnbindBoard` engine semantics

Arm: `UnbindBoard { board }`. Planner (`command.rs`, after
`BindBoard`'s arm):

1. `existing_board` lookup: `UnknownBoard(board)` if absent.
2. Tombstoned ⇒ `BoardDeleted(board)` (protects the R9c tombstone push
   — unbinding a deleting board would strand the remote delete and let
   the planner's unbound-delete Noop finalize it locally while the
   remote copy survives).
3. Unbound live board ⇒ `no_op()` (idempotent repeat).
4. Bound live board ⇒ actions `[UpsertBoard(remote: None),
   DeleteSyncStatus(board)]`, outcome
   `Unbound { stranded_ops }` where `stranded_ops` = the count of
   outbox ops attributed to the board via the entity graph
   (`op_board_id`, pipeline.rs:75 — the same attribution `read_scope`
   and the load-time `pending_ops` derivation use; one truth).
   Descendant refs, the outbox, and validators are untouched.

`DeleteSyncStatus(BoardId)` is a new `PersistenceAction` (persistence.rs
enum + the shared `apply` batch semantics: remove the key from
`PersistedState.sync`/`SyncStates`; sqlite: `DELETE FROM sync_status
WHERE board = ?`; fakes inherit the shared semantics; contract +
roundtrip tests extended). Design source: §3.2 — "unbind simply deletes
the board's status rows" (a stale `Synced` row for an unbound board
would be a lie the badge could show).

### 4.6 D3-6/4.6 — the `ReconcileByTitle` algorithm (pure domain module `domain/src/reconcile.rs`)

New module, pure, no new deps (`cargo deny` trivially green). Public
surface used by the planner (and the app's pre-count):

```rust
pub fn normalized_title(raw: &str) -> String;          // trim + casefold (D3-10)
pub struct ReconcileCounts { pub stacks: usize, pub labels: usize,
                             pub tasks: usize, pub cancelled_ops: usize }
pub fn plan_reconcile(app: &AppState, outbox: &[PendingOp], board: BoardId)
    -> (Vec<PersistenceAction>, ReconcileCounts);      // pure; empty ⇒ nothing to do
```

Algorithm (deterministic, three passes, one board):

1. **Scope**: the board's live stacks/labels/tasks, split into
   *local-only* (`remote == None`) and *remote-bound*. Unbound board or
   no remote-bound siblings ⇒ empty plan (`NoOp`).
2. **Stacks**: pair each remote-bound stack (iterated in id order) with
   the unmatched local-only stack of equal `normalized_title`, if any —
   one-to-one, deterministic (see D3-10). Each pair: merge (D3-4),
   re-parent the absorbed stack's tasks, cancel the survivor's
   create-class ops (`CreateStack`).
3. **Labels**: same pairing over the board's labels; merged survivors
   re-point task label edges; cancel `CreateLabel` ops.
4. **Tasks**: grouped by stack row (post-re-parenting): within each
   stack, pair remote-bound tasks with unmatched local-only tasks by
   `normalized_title`; merge; cancel `CreateTask` ops (and the
   task-addressing `AssignLabel`/`UnassignLabel`? no — those ride the
   survivor's update path; only *create-class* ops cancel, per design).
5. **Merges** (`merge_local_*` helpers in the module): field-wise
   newer-clock-wins over the two local rows; resulting clocks are the
   field-wise max (proptest-pinned); `remote_seen` = the bound row's.

Actions emitted: upserts for survivors, re-parents, edge rewrites, and
neutralizing upserts for absorbed rows; `FailOp` per cancelled create.
Planner arm `ReconcileByTitle { board }`: `live_board_of` (typed
refusals), then `plan_reconcile` — empty ⇒ `NoOp`, else
`Reconciled { ..counts }`.

**Idempotence argument** (pinned by test): after a pass, every matched
pair is a single bound row — the local-only side of each pair no longer
exists, so re-running finds strictly fewer candidates; unmatched
entities deterministically re-fail to match (same normalization, same
scope) ⇒ second run is `NoOp` with zero counts.

**Clock-merge reuse note**: `merge_task`/`merge_stack`/`merge_label`
(merge.rs) are local-vs-remote-view merges; reconcile needs
local-vs-local. The helpers implement the same per-field clock
discipline (ADR 0004's amendment) over two clock-carrying local rows —
new pure code, property-tested against the clocks (field-wise max),
not a re-use of the remote-view signatures.

### 4.7 D3-10 — matcher details

- `normalized_title` = `trim()` + simple case folding
  (`chars().flat_map(char::to_lowercase)`; deterministic and total).
  Exactly trim+casefold — no Unicode normalization, no collation
  (fuzzy growth is the design's out-list).
- Pairing is one-to-one and deterministic: remote-bound side iterated
  in id order; each absorbs the smallest-id unmatched local-only
  same-title entity. A second same-title local-only entity stays (the
  union behavior — "no match ⇒ both stay" applied to the leftover).
- Tasks pair only within a shared stack row (§4.4's re-parenting makes
  "within matched stacks" ≡ "within one stack row"); cross-stack
  same-title tasks never pair (conservative by design).

### 4.8 D3-7 — `select`'s fold-to-pass-through + resolution rules

- The app-side `BoardAlreadyBound { current }` construction
  (boards.rs:667) and the single-bound-board INTERIM block (:640–672)
  are deleted; the engine's typed refusals are the only board-state
  rule (`select`'s `Err` pass-through plus `publish`'s fold of its
  boards.rs:503 pre-check).
- `LocalBoardExists` **survives as the ceremony's `--bind-local-board`
  gate**: the engine cannot know the user's intent flag; the design
  folds only the `BoardAlreadyBound` construction into the engine
  ("one rule, one home" — the flag gate is UX policy, caller-owned per
  DF-2).
- Board resolution for `select` uses a new sibling of `resolve_board`
  (mod.rs:386 refuses zero-or-≥2): `resolve_board_or_none_for_bind` —
  zero live boards is the **fresh path** (D3-1), not ambiguity; ≥2
  without `--board` still refuses `AmbiguousBoard`. `--board <uuid>`
  rides the chunk-2 flag pattern (`parse_board_id`, cli.rs:283).
- **NoOp ⇒ immediate receipt**: a same-remote re-select must not wait
  for a fresh cycle (the row is already settled); the receipt renders
  current state. `Bound ⇒ nudge + wait + auto-reconcile`.
- The nudge is `app.engine.execute(StateCommand::RequestSync).await?`
  — the engine fans `SyncNow` to every registered connector
  (`forward_sync_now`) and marks bound boards `Syncing`
  (`mark_bound_syncing`). `send_set_board` is deleted. The startup
  snapshot for the wait is **that board's row**
  (`state.sync.get(board).cloned()`), taken before the nudge.

### 4.9 D3-8 — the new verbs' clap shapes + loud flags

```text
taskboard boards unbind    [--board <uuid>] [--confirm]
taskboard boards reconcile [--board <uuid>] [--confirm]
```

- `--board` follows the chunk-2 house pattern (long flag, uuid parser,
  single-live default, `AmbiguousBoard` on zero/≥2) — the design's
  `boards unbind <board>` placeholder reads as "which board"; DF-2's
  "flag, not positional — scripts stay explicit" rules.
- **`--confirm` on both** (uniform loud-flag shape, impl plan §3):
  - `unbind` without `--confirm` ⇒ typed refusal stating the scope
    (bound remote, pending ops that will strand, descendant-ref count)
    — destruct_on's shape; new app-mirror variant
    `CommandError::UnbindConfirmationRequired { board, bound_remote,
    pending_ops, descendant_refs }` (destruct's `ConfirmationRequired`
    fields don't fit; do not churn it).
  - `reconcile` without `--confirm` ⇒ typed refusal stating the match
    counts (computed by running the pure `plan_reconcile` and
    discarding actions — a count, **not** a dry-run flag: no output
    mode, no `--dry-run` exists).
  - The **automatic** post-bind reconcile run is ungated: the design
    runs it "by the bind ceremony automatically after adoption-wait";
    the operator's consent was `--bind-local-board` (the design's
    recorded mitigation for the mis-merge trade-off).

### 4.10 D3-11/4.12 — receipts, outcomes, mirrors, and the `SetBoard` inventory

- `CommandOutcome` (domain): `BindBoard`'s success widens
  `Applied` → `Bound { wiped_refs: usize }` (exact count of refs the
  plan cleared); `Unbound { stranded_ops: usize }` (D3-5);
  `Reconciled { stacks, labels, tasks, cancelled_ops }` (D3-6). Not
  serde, not IPC (verified). Existing engine/command tests asserting
  `Applied` for binds are updated in-chunk (behaviour grew; not
  weakened — they additionally assert counts).
- App receipts (output.rs): `UnbindReceipt { board, bound_remote,
  stranded_ops, kept_refs }`; `ReconcileReceipt { board, …counts }`;
  `SelectReceipt` shape unchanged (the wipe count rides the human
  renderer and a new optional field `wiped_refs: Option<usize>`
  serialized `skip_serializing_if = "Option::is_none"` — additive JSON,
  single-board selects render exactly today's bytes … minus nothing:
  `Bound{0}` sets `Some(0)`; decision: emit the field only when
  `wiped_refs > 0`, keeping the no-wipe receipt byte-identical).
  Renderers + `exit_code`/kind-string mappings in mod.rs mirror the
  new variants (`RemoteBoardInUse` → kind `remote-board-in-use`,
  exit 1 class — mirroring `BoardAlreadyBound`'s treatment).
- **`SetBoard` chunk-4 removal inventory** (compile-clean here, removed
  there in the tagging motion): (1) `SyncCommand::SetBoard` variant
  (messages.rs:292); (2) the sync-actor command arm (actor.rs:199);
  (3) the absorber arm (actor.rs:258); (4) the `LoopState.target`
  stickiness + `PullCache::clear`-on-retarget those arms exist to serve
  (subsumed by the derived per-board set + per-board cache keys);
  (5) `App::sync_commands()` (bootstrap.rs:178) — after
  `send_set_board`'s deletion it has zero callers but stays lint-clean:
  `taskboard-app` is lib+bin (Cargo.toml `[lib]` + `[[bin]]`) and the
  lib's public surface is the integration-test API, so `dead_code`
  does not fire. Stage 2 verifies this with the gate suite; if a lint
  fires anyway, the fix is `#[allow(dead_code)]` + an
  `INTERIM(multi-board-4)` comment — never a fake caller.
- IPC protocol: zero diff (ceremonies boot a core in-process; chunk 6
  owns daemon verb growth). UI: zero diff (chunk 7).

### 4.11 D3-9 — `wait_for_adoption(board)`: the board-row decision table

Signature: `wait_for_adoption(app, board: BoardId, remote:
RemoteBoardId, startup_row: Option<SyncStatus>, timeout)`. Loop
(unchanged predicate-poll shape: engine signals + 200 ms fallback,
deadline, no sleeps-in-tests):

| Observation (that board's row + binding) | Verdict |
|---|---|
| row fresh-settled (`phase != Syncing` ∧ (row ≠ `startup_row` ∨ seen signal)) ∧ `phase == Idle` ∧ binding present | success — `SelectReceipt` (adoption = the Completed report for *this* board landed and merged) |
| fresh-settled ∧ `Failed { last_error }` | `SyncFailed(kind)` |
| `Syncing` / not-fresh / `Offline` | keep waiting |
| deadline | `SyncTimedOut` |

Binding-present stays a conjunct of success (defensive: the row and the
binding are updated in one batch, but the receipt promises both).
`publish_on` switches from the aggregate `wait_for_sync_result` to this
fn (its receipt's `sync` view becomes the board's row view). The
global `sync` verb keeps `wait_for_sync_result` byte-for-byte
(aggregate; chunk 6 owns its growth) — the two waits share the
predicate-poll idiom, not a forked decision table.

### 4.12 D3-13 — snapshots: no regeneration pass

Chunk 3 owns no insta pass (DF-9's passes were chunks 1–2). The new
arms are additive; `BindBoard`'s wipe only fires on boards whose
descendants carry foreign refs — the existing corpus binds
ref-less-descendant boards, so accepted diffs must be empty. **Rule for
stage 2/3**: any snapshot diff ⇒ stop and diagnose (a semantics change
hiding behind "regenerate" is the failure mode); fix forward, never
`--accept` blind. No `.pending` files ship.

### 4.13 D3-14 — destruct/read_scope at N, and the tombstone drain (D3-15)

- Chunk 2 already parameterised `read_scope(app, board_id)` /
  `destruct_on(app, board_id, verb, confirm, command)` and the receipts
  name the board id — the design's "mechanical parameterisation" landed
  early with the `--board` flags. Chunk 3's remainder: (a) extend
  `BoardScope` with `descendant_refs: usize` (live stacks+tasks+labels
  of the board with `remote.is_some()` — feeds unbind's refusal/receipt
  and the bind pre-count); (b) pin per-board behaviour at N ≥ 2 with
  typed tests (two boards with differing scopes: the refusal states
  *that* board's counts; the receipt names *that* board's id; the
  pre-count ⇄ refusal ⇄ receipt round-trip holds).
- **Tombstone drain**: the `DeleteBoard` op keeps the board's binding
  until the push confirms (R9c); the D3-2 guard deliberately treats a
  tombstoned-but-bound board as a live target, so the op drains through
  exactly the connector whose target is that binding — at N=1
  connector that is "the owning connector" by construction; chunk 5's
  N=2 tier-2 test exercises the plural form. Pinned here at two
  layers: pipeline (the finalize rule clears the binding — existing
  behaviour re-pinned per-board) and the actor guard test (a
  tombstoned board's target survives; a finalized one's target is
  evicted next cycle).

## 5. TDD plan — full test catalogue (typed outcomes; AGENTS §5)

Layer discipline: domain owns the semantics; state tests routing only;
app tests ceremony orchestration; one behaviour, one test, lowest
layer. No text assertions — `matches!` on variants / structural facts.
No sleeps — paused time / predicate-deadline / join handles. Fakes over
mocks (existing `SyncStateReader`/repo fakes; wiremock only where a
wire is the subject).

### 5.1 Domain — `command.rs` unit corpus (planner arms)

Bind ladder + wipe (extends the :2427–2481 corpus):

1. `bind_remote_held_by_another_board_refuses_remote_board_in_use` —
   two boards, second binds the first's remote ⇒ `Err(RemoteBoardInUse
   { remote, board: first })` (the holding board is named).
2. `bind_wipe_clears_only_foreign_refs` — board with descendants
   carrying refs to R1 and R2; bind onto R2 ⇒ upserts clear only the
   R1 refs; R2 refs (pathological-but-possible) untouched; board row
   upserted with the new binding.
3. `bind_outcome_counts_wiped_refs` — `Ok(Bound { wiped_refs })` ==
   number of cleared refs; zero wipes ⇒ `Bound { wiped_refs: 0 }`.
4. `bind_wipe_forecast_counts_stranded_update_ops` — the exported pure
   pre-count helper returns the update-class ops on to-be-wiped
   entities (structural count over the plan's predicate).
5. Existing `bind_same_remote_is_noop` / `bind_other_remote_refuses`
   stay green unchanged (regression guards of the ladder).

Unbind:

6. `unbind_unknown_board_refuses` / `unbind_tombstoned_board_refuses`
   (`UnknownBoard` / `BoardDeleted`).
7. `unbind_unbound_board_is_accepted_noop` — `NoOp`, empty actions.
8. `unbind_clears_binding_and_drops_status_row` — actions are exactly
   `[UpsertBoard(remote: None), DeleteSyncStatus(board)]`;
   `Unbound { stranded_ops }` == the op-attribution count.
9. `unbind_leaves_outbox_and_descendant_refs_untouched` — no
   Enqueue/Complete/Fail actions; no stack/task/label upserts.

Reconcile (arm level; algorithm-level in §5.2):

10. `reconcile_unknown_board_refuses` / `reconcile_deleted_board_refuses`.
11. `reconcile_unbound_board_is_noop` / `reconcile_no_candidates_is_noop`
    — `NoOp`, empty actions, zero counts.
12. `reconcile_outcome_carries_counts` — `Reconciled` fields ==
    actions emitted (merged stacks/labels/tasks, FailOps).

### 5.2 Domain — `reconcile.rs` (+ proptests)

Unit (deterministic scenarios, structural asserts):

- `normalized_title_trims_and_casefolds` (`"  To-Do "` ≡ `"to-do"`).
- `stacks_match_by_normalized_title_and_merge` — survivor keeps the
  local id, gains the ref; absorbed row upserted `deleted ∧ remote:
  None`; clocks field-wise max.
- `stack_merge_reparents_absorbed_children`.
- `labels_match_and_repoint_task_edges`.
- `tasks_match_only_within_shared_stack_row` — same title, two stacks ⇒
  no pair.
- `task_merge_newer_clock_wins_per_field` — mixed winners.
- `duplicate_create_ops_cancel_counted` — `CreateTask`/`CreateStack`/
  `CreateLabel` on survivors ⇒ one `FailOp` each, counted; a
  `CreateTask` on an *unmatched* local-only task survives.
- `second_run_is_empty` — idempotence (plan over the post-pass state).
- `two_same_title_locals_one_bound_absorbs_deterministic_min_id`.

Proptest (`proptest!`, 256 cases, strategies over small alphabets):

- `norm_is_idempotent_and_order_insensitive`.
- `pairing_iff_normalized_equality_within_scope` — matcher ⟺ norm
  equality ∧ same stack row (tasks) ∧ same board; never cross-board.
- `pairing_is_one_to_one_and_deterministic` — re-planning the same
  input yields identical pairs; no entity pairs twice.
- `merged_clocks_are_fieldwise_max` (each entity kind).
- `wipe_scope_equals_foreign_refs` — the bind pre-count predicate ==
  descendants with `ref.board != target` (random ref assignments).

### 5.3 Domain — `pipeline.rs` dead-letter rule

- `localdata_rejected_update_on_refless_entity_of_bound_board_failops`
  — the op is removed; counted.
- `localdata_rejected_update_on_never_bound_board_stays_queued`.
- `localdata_rejected_op_on_entity_bound_elsewhere_stays_queued`.
- `localdata_rejected_create_class_stays_queued` (the re-create
  promise).
- `badrequest_rejection_still_dead_letters` (existing rule, re-pinned
  beside the new one).

### 5.4 Domain — persistence contract

- `delete_sync_status_removes_only_that_row` (shared `apply`
  semantics); fake-repo contract test extension (contract.rs
  neighbourhood of the FailOp case :153); absent row = no-op.

### 5.5 State — routing + interpret (engine thread)

- `unbind_and_reconcile_arms_execute_and_publish` — dispatch via the
  engine handle, assert the published `AppState` (binding cleared,
  status row gone; merged survivors visible) — routing only, no
  re-tested domain semantics.
- `delete_sync_status_interpret_drops_the_map_key`.
- Existing `a15_bind_board_sets_the_remote_binding` (engine.rs:1249)
  updated: outcome assertion `Bound { wiped_refs: 0 }`.
- Snapshot corpus: green with **zero** accepted diffs (D3-13).

### 5.6 Storage — sqlite

- `delete_sync_status_roundtrip` (roundtrip.rs, beside the
  CompleteOp/FailOp cases :178) — upsert two boards' rows, delete one,
  load shows the survivor.

### 5.7 Sync actor — the guard (tier-1, existing fake seams)

- `stale_target_evicted_when_no_binding_remains` — target R1, bindings
  cleared ⇒ cycle takes the fallback path (or skips); cache cleared.
- `tombstoned_board_keeps_the_target` — R9c retention ⇒ the cycle still
  targets the deleting board (drain path alive).
- `finalized_tombstone_evicts_target_next_cycle` (pipeline finalize +
  guard composition; pipeline-level where the actor seam is not the
  subject).

### 5.8 App — ceremonies (hermetic: with_core + fakes; wiremock only where a wire is the subject)

select/publish:

- `fresh_install_select_mints_binds_and_nudges` — board-less db;
  after the ceremony: one live board, binding set, receipt rendered;
  engine fed a fake `Completed` report (existing fake-report seams)
  settles the board's row ⇒ success without timeout. Asserts **no
  targeted send occurred** structurally: the ceremony's only sync-side
  interaction is `RequestSync` (compile-level: `send_set_board` is
  gone; review re-greps `SetBoard(` construction sites = 0).
- `reselect_same_remote_is_immediate_noop_receipt` — no wait (paused
  clock never advances; the receipt returns).
- `select_remote_held_by_other_board_passes_through` —
  `RemoteBoardInUse` surfaces typed (kind string + exit code mapped).
- `select_two_live_boards_without_flag_is_ambiguous` /
  `select_zero_live_boards_takes_fresh_path` (the D3-7 split).
- `select_local_board_without_bind_flag_refuses_local_board_exists`.
- `select_bind_wipe_receipt_counts` — stale-ref board rebound onto a
  new remote: receipt `wiped_refs` == pre-count; forecast note pinned.
- `publish_nudge_is_request_sync_and_wait_is_per_board` — publish on a
  bound board refuses `BoardAlreadyBound` pass-through (fold).

unbind/reconcile:

- `unbind_without_confirm_refuses_stating_scope` — typed refusal;
  counts round-trip `read_scope` (stranded ops, kept refs).
- `unbind_with_confirm_receipt_names_board` — id, ex-bound remote,
  stranded ops, kept refs; second run ⇒ no-op receipt.
- `unbind_passes_engine_refusals` (unknown/deleted boards).
- `reconcile_without_confirm_states_match_counts` /
  `with_confirm_lands_counts`.
- `reconcile_manual_rerun_is_idempotent`.
- `bind_ceremony_runs_reconcile_automatically` — post-adoption
  `Reconciled` outcome observed (fake report carrying a same-title
  sibling ⇒ merged counts in the ceremony's wake).

destruct at N:

- `destruct_refusal_states_that_boards_counts` / `destruct_receipt_
  names_board_id` — two boards, differing scopes; pre-count ⇄ refusal
  ⇄ receipt round-trip.
- `tombstone_drains_through_owning_target` — §4.13's two-layer pin.

wait:

- `wait_for_adoption_success_failed_timeout_typed` — the decision
  table (fake reports + paused clock; stale-row non-short-circuit).

Existing must-not-break: `r11_select_on_a_standalone_core_surfaces_
the_typed_gate` (repl.rs:510 — standalone + local board +
`bind_local_board: false` ⇒ `LocalBoardExists`, unchanged), the repl
boards-grammar suite, `cli_connect.rs` (untouched surface), the
`sync` verb's aggregate wait (byte-shape unchanged).

### 5.9 REPL — grammar only

- `boards unbind/reconcile` line verbs parse and dispatch (interim
  first-live-board default, marked INTERIM(multi-board-6), chunk 2's
  convention); ambiguity refusal typed.

## 6. Implementation phases (stage 2)

Every commit standalone (rebase-merge hygiene); TDD red→green per
phase; gates at each phase boundary, full suite at the end.

1. **Domain — vocabulary + persistence**: `CommandError::
   RemoteBoardInUse`, the two arms, `CommandOutcome` widenings,
   `DeleteSyncStatus` (+ `apply` semantics + contract tests). Unit
   corpus §5.1 bind-ladder/unbind rows.
2. **Domain — bind-wipe + dead-letter**: wipe in `BindBoard`'s arm +
   the exported pre-count helper; `apply_push_report` rule. §5.1/§5.3.
3. **Domain — reconcile module**: matcher, merges, `plan_reconcile`,
   planner arm. §5.1 reconcile rows + §5.2 (incl. proptests).
4. **Storage**: `DeleteSyncStatus` SQL + roundtrip. §5.6.
5. **State**: routing/interpret tests; `a15` update; snapshot
   zero-diff check (D3-13 rule). §5.5.
6. **Sync actor**: the D3-2 guard + cache clear. §5.7.
7. **App**: cli verbs → mirrors/kind strings → receipts/renderers →
   `select_on`/`publish_on` rewrite + `wait_for_adoption(board)` +
   `unbind_on`/`reconcile_on` + `read_scope` extension → `boards_line`
   arms → repl grammar. §5.8/§5.9. Delete `send_set_board`.
8. **Bookkeeping**: CHANGELOG Unreleased line; full AGENTS §10 suite.

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

`cargo deny` green with zero dependency changes (design §3.4's
boundary bar). Tier note: chunk 3 carries **no tier-2 tests** (no
docker requirement; the two-connector drain is chunk 5's tier-2).

## 8. Risks, recorded deviations, interim statements

- **R1 — bind-wipe × reconcile ordering (the round's hardest seam)**.
  The forecast/exact split (D3-3/D3-6) is the mitigation; the pinned
  pair of tests (forecast == eventual without matches; ≥ with) makes
  any drift visible. Residual risk: an update-op on a *reconciled*
  entity pushes the merged state — correct, but the merge's tie-break
  (bound row wins on equal clocks) must stay deterministic (proptest).
- **R2 — D3-2 guard deviation** (recorded in §4.2): minimal, subsumed
  by chunk 4; the traps list calls actor changes chunk 4's — stage 3
  reviews the guard against exactly the recorded reason, and chunk 4's
  prompt re-cites it for removal.
- **R3 — design-prose deviation on the dead-letter's home** (recorded
  in §4.3): the design's "existing unknown-binding path" does not
  dead-letter today; the rule is added push-side with the scoping that
  preserves every other stranding class. Implementing Q5's ACCEPTED
  rule (dead-letter + counted) faithfully; only the incidental layer
  phrasing differs.
- **R4 — `CommandOutcome::Bound` widening** touches existing engine
  tests (a15 and kin): in-chunk updates, assertions strengthened with
  counts — never weakened.
- **R5 — snapshot discipline**: no pass owned; any diff is a finding
  (D3-13). The wipe and the new arms must be invisible to the corpus.
- **R6 — interim state persists**: the actor still syncs its first
  bound board only (chunk 4 fixes); a second bound board's ops strand
  visibly (per-board `pending_ops` climbs; update-ops on *its wiped
  refs* can dead-letter at the first board's cycles — the §3.4 table's
  interim row). Multi-board stays un-advertised until chunk 5. The new
  verbs are honest in the single-board world they ship into.
- **R7 — fresh-path mint changes first-run bytes**: the minted shell's
  title comes from the prelude listing; color converges via the merge.
  A regression here would surface as `wait_for_adoption` timeouts —
  the §5.8 fresh-install test pins the whole path hermetically.

## 9. Bookkeeping & handoff

- CHANGELOG `Unreleased`: one user-visible line for the verbs
  (`boards unbind`, `boards reconcile`, board-scoped select/publish
  receipts; stage 2 lands it, chunk 8 promotes).
- No tracker edits (MD-4): stage 3's PR body carries `Closes #170` and
  `Closes #179`; the annotation comment on #125 rides the PR open.
- Estimated diff: **~2,800–3,600 lines** across six crates including
  tests (domain ≈ 1,300 incl. proptests; app ≈ 900; state/storage/
  sync/repl ≈ 400; CHANGELOG/plan-adjacent ≈ 100). The impl plan
  sizes the chunk M; the reconcile module's re-parenting/edge-rewrite
  breadth is what pushes the estimate toward M/L — flagged, not
  scoped down (the design names all of it).
- Handoff: stage 2 implements in this worktree; stage 3 rebases,
  reviews against §4's decisions + the dispatch traps list, ships
  PR-green + `CLEAN`, removes the worktree.

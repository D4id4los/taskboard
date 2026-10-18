# [Plan] Multi-board Wave — Detailed Design (Stage-0 round)

- **Date**: 2026-10-18
- **Kind**: detailed design of record (draft, status PROPOSED) for the
  `wave-multi-board` milestone. Resolves framing plan §5 Q1–Q9 as one
  coherent design: per question — re-verified anchors, option space,
  trade-offs, exactly one recommendation, consequences for later work,
  and a topology-invariant conformance note. Includes the draft ADR
  0010 amendment text (Q4, §6) and the F16-22 migration design against
  0001/0005 conventions (Q9). The decision record (§5) is explicitly
  left for the user's session; D-W9's SyncStatus keying is **posed with
  a recommendation, never self-ruled** (MD-8).
- **Origin**: framing plan
  `.artifacts/plans/2026-10-18-multi-board-wave-design-plan.md` §5
  (normative question list); dispatch plan
  `.artifacts/plans/2026-10-18-multi-board-design-round-dispatch-plan.md`
  §1/§2 (binding constraints, acceptance); stage-1 work plan
  `.artifacts/plans/2026-10-18-multi-board-design-round-workplan-plan.md`
  (method decisions MD-1…MD-10).
- **Status**: PROPOSED — awaiting the user's decision session. Zero
  production code, zero tracker edits (this file and the work plan are
  the chunk's entire diff).

---

## 1. Mission & binding constraints

Lift the two standing structural invariants — one live board, one
active connector — so N boards sync across N simultaneously active
connectors with per-board routing, **without re-opening the WI-J
channel topology**.

**The acceptance test, stated once (framing §1, connect-flow §4.5,
ADR 0014 decision 3)**: *the engine and `App` are connector-count
agnostic; "at most one active connector" is an app/config-layer
invariant only; every change in this design lands additively on the
§4.2 fan-out/fan-in seams. A proposal that needs to change §4.2's
topology has misdesigned something else.* Every section below carries
a conformance note against this invariant (MD-3).

Binding constraints (dispatch §2): D-W1…D-W8 may not be contradicted;
D-W9 is posed for the user; AGENTS §2 domain purity for the trait
port; the audit's "no speculative prep" stance — this round produces
a design, zero production code, zero tracker edits.

**The one design in one paragraph** (expanded per question in §3): a
`board: BoardId` parameter becomes explicit on every board-scoped
`StateCommand` (selection is a caller-owned concept — CLI/REPL session
state, UI `ViewState.board` — never engine state); sync status becomes
a per-board map keyed by `BoardId` (connector attribution derived from
the board's connector-tagged binding); connector identity becomes a
domain newtype `ConnectorId` minted from config names, routing happens
in the app-layer registry (the engine fan-out stays broadcast-`SyncNow`
only; `SyncCommand::SetBoard` retires in favour of binding-derived
targets); the connector port is split — the domain vocabulary
(`SyncCommand`/`SyncReport`/`SyncStateReader` + `ConnectorId`) plus an
app-layer `ConnectorModule` factory trait; ceremonies generalise to
named boards (`destruct_on` keeps its typed-refusal shape) with new
`UnbindBoard` and `ReconcileByTitle` commands; config grows
`[[sync.connectors]]` array-of-tables with the legacy scalar +
`[nextcloud]` synthesising the one "nextcloud" connector bit-for-bit;
network authority becomes per-connector events + an app-layer
all-down authority emitting the existing global events; the UI gains
`ViewState.board` and `board.next`/`board.previous` actions mirroring
the stack switcher; one table-rebuild migration (0006) lands first.

## 2. Verified baseline (MD-1/MD-2)

Worktree base SHA at stage-2 execution: branch
`docs/multi-board-detailed-design` @ `1004120`, merge-base with
`origin/main` = `62dd839` — **identical to the plan-day snapshot**, so
every drift value below is drift versus the *framing* plan's `878bb12`
anchors / the audit's `v0.1.0` anchors, re-confirmed at execution time.
The remediation-wave-1 tail did not land on `main` between plan day and
execution (verified at stage 2: `git merge-base HEAD origin/main` =
`62dd839` = `origin/main` tip). **Stage-3 review update**: the branch
was rebased onto the then-`origin/main` tip `361ab0b` before handoff;
the single commit `62dd839..361ab0b` is docs-only (the
remediation-followups dispatch plan), so the code anchors below were
re-spot-checked at the rebased base and all hold — no drift values
changed.

Locating-command classes (MD-2, reproducible): `grep -n` over the
named file for the named symbol; `sed -n '<range>p'` for bodies;
`ls` for module inventories; `gh issue view <n>` for tracker text
(read-only). The figment probe (Q6) is reproduced verbatim in §3.6.

| Finding / source | Seam (crate paths abbreviated as in the work plan) | This round's lines | Drift vs prior snapshots |
|---|---|---|---|
| F16-25 | domain/state.rs entity maps `boards/stacks/tasks/labels/reminders` | 27–37 | none vs plan day |
| F3-04/F16-22 | domain/state.rs `sync: SyncStatus` / `last_updated` / struct / `SyncPhase` enum | 39 / 42 / 47–53 / 58–71 | exact |
| F16-22 | state/core.rs `sync_status()` / `mark_syncing` | 72 / 113 | exact |
| F16-22 | state/actor.rs `ingest_sync_report` + `append_status_if_changed` + `go_offline` | 611 / 668–681 / 728–743 | new pins (cluster re-check per work plan) |
| F16-22 | storage migrations/0001_init.sql `sync_status` `CHECK (id = 1)` + seed | 102–109 | exact |
| F16-22 | storage repo.rs status load / upsert loop | 505 / 990–996 | +9/+37 vs audit's 496/953 |
| F16-22 | domain/persistence.rs `PersistedState.sync` / `From<PersistedState>` | 97 / 104–115 | new pin |
| F3-04/F16-23 | domain/outbox.rs `LocalOp` enum / `UpdateBoard(BoardId)` / `DeleteBoard` | 22–62 / 51 / 53 | exact |
| F16-06 | domain/command.rs `live_board()` / nine call sites | 134–141; 364 / 465 / 576 / 603 / 620 / 637 / 657 / 755 / 825 | +1 on the last three vs plan day (575/656/754/824); +21…+28 vs framing on F8-06 rows below |
| F8-06 | domain/command.rs `CommandError` variants `NoBoard`/`BoardExists`/`BoardAlreadyBound(RemoteBoardId)` | 77 / 80 / 84 | exact vs plan day (framing's `:56` drift already recorded there) |
| F8-06 | app/commands/mod.rs mirrors `BoardAlreadyBound`/`NoBoard`/`LocalBoardExists` | 108 / 149 / 179 | exact vs plan day |
| #334 | domain/messages.rs `SetBoardColor`/`SetBoardArchived`/`DeleteBoard`/`ResetBoard`; `SyncCommand`/`SyncReport` enums; `SetBoard` | 142 / 149 / 158 / 164; 272 / 292; 274 | exact |
| #334 | domain/entities.rs `BoardClocks` / `Board.remote` / `remote_seen` | 95 / 124 / 136 | exact |
| F16-25 | domain/persistence.rs `trait SyncStateReader` / `BoxFuture` alias | 68 / 25 | exact |
| F16-05 | app/bootstrap.rs `connectors: Mutex<Vec<SyncActor>>` / `.first()` pick / `ensure_vacant` call / `AttachError::AlreadyAttached` / `ensure_vacant` fn | 141 / 181 / 266 / 431 / 443–447 | exact vs plan day (framing's 443 = the fn; the variant sits at 431) |
| F16-05 | sync/actor.rs resume pull-target block / bound-board find | 327–334 / 361–366 | exact |
| F16-05 | state/actor.rs fan-out registry `sync_outs` / `AttachConnector` push / `forward_sync_now` | 328 / 396 / 449+585–600 | new pins |
| F16-05 | sync/actor.rs `spawn_sync_actor` signature | 120–129 | new pin |
| F16-24 | app/commands/boards.rs `destruct_on` / receipts / `read_scope` / `select_on` / `send_set_board` | 235–259; 152–156/272/283; 163–225; 566–628; 496–501 | exact, expanded from plan day's 233–299 |
| remediation chunk 5 (#258) | sync/push_exec stale-binding re-resolve | mod.rs:10–11, execute.rs:471–479, tests.rs:952+ | new pin |
| F7-05 | ui/presentation.rs `live_board()` | 248–252 | +1 vs framing's 250–254 tail |
| F7-05 | ui/view_state.rs `live_board` consumer / `ViewState` struct / `select_stack` | 198 / 58–72 / 383–390 | exact |
| F7-05 | ui/models.rs `IdTables` struct / `push` builder | 35–45 / 101 | exact |
| Q8 seams | ui/actions.rs `ActionId` / `ActionEntry` / `StackCreate` / `REGISTRY` | 92 / 177 / 107 / 206+ | exact; presets.rs 80/165 |
| Q8 seams | ui/keymap/{mod,presets,stroke}.rs; ui/compositions/board-layout.slint | keymap mod.rs:23–131; board-layout.slint:27–56 | new anchors |
| Q3/Q4 seams | domain/ids.rs `RemoteBoardId` / `RemoteStackRef.board` / `RemoteCardRef.board` | 80 / 109–114 / 98–104 | new anchors |
| Q6 seams | app/config.rs `SyncBackend` / `SyncSection` (`deny_unknown_fields`) / `sync_backend()` / `AppMode::{Daemon,Desktop,Kiosk}` | 159–177 / 172 / 526–540 / 87–110 | pinned (audit's 159–177/526–535 holds) |
| Q6 seams | app/commands/connect.rs `persist` + toml_edit overlay write | 390–413 / 509 | new pin |
| Q7 seams | sync/actor.rs offline FSM (`offline` field, `fail_cycle` NetworkLost emit, restored emit) | 160/187; 492–520 (emit 499); 451–470 (emit 463–468) | new pins |
| Q7 seams | state/actor.rs `NetworkLost`→`go_offline` / `NetworkRestored`→forward arms | 443–450 | new pin |
| Q7 seams | domain/messages.rs `SystemEvent` enum | 249–261 | new pin |
| Misc | app/commands/mod.rs `require_connector`; commands/output.rs `SyncStatusView`; commands/status.rs `StatusView.sync`; commands/sync.rs wait decision table; bootstrap.rs `wait_for_boot_pull` 705+; `SYSTEM_BROADCAST_CAPACITY` 77 | — | new pins |

Also re-verified read-only: issues #179/#170 texts (`gh issue view`),
GUI plan R1 (lines 32–45), R4 (88–103, keyboard-journey acceptance at
103: "performs every core verb: navigate boards/stacks/tasks, edit…"),
R9 (203–222); ADR 0010's vocabulary-stance headline at .org line 140;
ADR 0014 decision 3 (the invariant wording); migrations 0001/0003/
0005 in full; codec.rs op tag table (56/157) and the legacy-tag
rejection pin (401–409); the insta snapshot corpus
(state/tests/snapshots: 20 `.snap` files, none naming `sync` — the
widening rewrites the `persisted_state`/`published_app_state` pairs
wholesale because `AppState` serialises whole).

## 3. The nine questions

### 3.1 Q1 — Board parameter shape (F16-06 + F16-23, one motion)

**Anchors.** The nine `live_board` sites (command.rs:134–141 resolver;
call sites 364 `CreateStack`, 465 `CreateLabel`, 576 `CreateBoard`
refusal, 603 `RenameBoard`, 620 `SetBoardColor`, 637 `SetBoardArchived`,
657 `DeleteBoard`, 755 `ResetBoard`, 825 `BindBoard`); the resolver's
own doc admits the dodge ("the smallest live board id,
deterministically … the MVP holds 0..1"); the `CommandError` variants
(77/80/84); outbox ops already board-addressed (`UpdateBoard(BoardId)`
outbox.rs:51, `DeleteBoard` :53); `PersistedState.sync` +
`SyncReport` (status payloads, Q2's seam); the non-engine consumers
(sync resume actor.rs:327–334/361–366; UI presentation.rs:248–252 →
view_state.rs:198); CLI/REPL call paths (commands/boards.rs
`*_on` fns dispatch parameter-less `StateCommand`s).

**Option space (framing Q1's fork, elaborated).**

- **Option 1 — engine-side selected-board state + explicit override.**
  `AppState` (or an engine-side session slot) grows a "current board";
  commands omitting the parameter resolve against it.
- **Option 2 — fully explicit scoping, no selection concept.** Every
  board-scoped `StateCommand` carries `board: BoardId`; each caller
  names the board on every invocation; no selection anywhere.
- **Option 3 — hybrid: explicit command parameter + caller-owned
  selection.** The command surface is exactly option 2's; the
  interactive frontends (CLI/REPL session, UI) keep their own
  "selected board" as *caller* state that pre-fills the parameter.

**Evaluation.**

*Option 1* fails on three independent grounds. (a) Purity/placement:
a selected board is view/session state, not application state —
persisting it drags a presentation concern through `AppState`,
`PersistedState`, the repo codecs, and the snapshot corpus for zero
engine-semantic gain (AGENTS §1: the UI is a dumb client; §2: domain
carries data, not session policy). (b) Totality: `live_board` is
`Option` — the engine would need a typed refusal for "no selection
established" *in addition to* "board unknown", i.e. the
implicit-resolution bug (F16-06) survives as "stale selection" bugs.
(c) Fan-out: two frontends (REPL + daemon UI) share one engine; a
single engine-side selection is a write-contention seam the
event-driven architecture has no channel for — inventing one is a
topology change (rejected by the invariant).

*Option 2* is the honest engine-side core; its weakness is purely
ergonomic: `boards rename --board <uuid> "New"` on a REPL where the
user has one obvious board in mind breeds exactly the "convenient
helper" wrappers that re-introduce implicit resolution outside the
compiler's view.

*Option 3* keeps the engine contract of option 2 (the compiler forces
every construction site to name a board — the F16-06 fix is
structural, not conventional) while giving interactive callers a
session default, living where its lifecycle already lives: the REPL's
`ReplSession` (mutable between lines, ADR 0011's model), the CLI's
per-invocation flag default, and the UI's `ViewState` (Q8). Nothing
of it enters the domain crate.

**Recommendation — option 3.** Fully explicit `board: BoardId` on the
command surface; caller-owned selection in the app/UI layers.

**The chosen shape applied to all four vocabularies** (dispatch
acceptance (b) — the half-run trap):

1. *Commands* (`StateCommand`, command.rs / messages.rs — the
   `Execute` wire carries the same shapes):

   | Arm (today's line) | Post-change shape |
   |---|---|
   | `CreateStack { title, order }` (:363) | `CreateStack { board: BoardId, title, order }` |
   | `CreateLabel { title, color }` (:464) | `CreateLabel { board: BoardId, title, color }` |
   | `CreateBoard { title, color }` (:573) | unchanged (mints a board; no target to name) — **refusal lifted** |
   | `RenameBoard { title }` (:602) | `RenameBoard { board: BoardId, title }` |
   | `SetBoardColor { color }` (:619) | `SetBoardColor { board: BoardId, color }` |
   | `SetBoardArchived { archived }` (:636) | `SetBoardArchived { board: BoardId, archived }` |
   | `DeleteBoard` (:653) | `DeleteBoard { board: BoardId }` |
   | `ResetBoard` (:753) | `ResetBoard { board: BoardId }` |
   | `BindBoard { remote }` (:823) | `BindBoard { board: BoardId, remote }` |

   Each site's `live_board(app).ok_or(NoBoard)?` becomes
   `live_board_of(app, board)?` — a lookup plus liveness check with the
   typed refusals below. Task/stack/label-scoped commands stay
   id-addressed (their entities carry board ownership already).

2. *Outbox ops* — **zero diff, verified**: board verbs are already
   board-addressed (`UpdateBoard(BoardId)`, `DeleteBoard`); stack/
   label/task ops are entity-id-scoped and the push resolves the owning
   board through the entity graph (`EntityTables` + `RemoteIndex`,
   actor.rs:361–366) — the same resolution the pull uses. No op gains
   a board field; Q3's push-filter change (boards owned by *this*
   connector) reads the same entity graph.

3. *`CommandError` variants* (command.rs:77–84, mirrored in
   app/commands/mod.rs:108/149/179):
   - `NoBoard` **removed** (its meaning — "no live board exists" —
     dissolves; per-board lookups refuse specifically).
   - `UnknownBoard(BoardId)` **new** — mirrors `UnknownTask(id)`
     (command.rs:143): the id names nothing.
   - `BoardDeleted(BoardId)` **new** — mirrors `TaskDeleted(id)`: the
     id names a tombstone (today's `live_board` filters `!deleted`
     silently; the parameterised lookup makes the refusal typed).
   - `BoardExists` **removed** — the single-live-board refusal lifts
     (framing §2: "single-live-board refusal lifted" is in-scope);
     `CreateBoard` always mints (titles are not uniqueness keys
     anywhere; `boards list` disambiguates).
   - `BoardAlreadyBound(RemoteBoardId)` **widens** to
     `BoardAlreadyBound { board: BoardId, bound: RemoteBoardId }` —
     the F8-06 "seed of the multi-binding refusal vocabulary";
     Q5 adds the mirror-image variant `RemoteBoardInUse` (§3.5).
   - App mirrors widen mechanically (mod.rs:108 `BoardAlreadyBound
     { current }` gains `board`; `NoBoard`/`LocalBoardExists`
     survivors are re-derived per ceremony, Q5).

4. *Status payloads* — the singleton `AppState.sync: SyncStatus`
   becomes the per-board map of Q2 (chunk 1 lands it; the command
   surface and the status surface land in the same vocabulary motion —
   framing §6 chunk 2 — so no intermediate shape exists where commands
   are board-scoped but status is not).

**Sub-decision: what replaces first-live-board implicit selection at
the non-engine consumers.**
- *Sync resume path* (actor.rs:327–334): the target stops being "the
  first bound board" and becomes "the boards bound to **this**
  connector" — the Q3/Q4 connector-tagged binding filter. No
  selection concept; ownership is data.
- *UI* (presentation.rs:248–252 → view_state.rs:198): the consumer
  becomes `ViewState.board` (Q8); `presentation::live_board` remains
  only as the *seeding rule* for a fresh `ViewState` (first live
  non-archived board, deterministic).

**Caller-side burden (consequences).**
- CLI: board-scoped `boards` verbs gain `--board <uuid>` (flag, not
  positional — scripts stay explicit); when exactly one live board
  exists the flag may be omitted (the CLI resolves it at invocation
  with a typed refusal when zero or ≥2 live boards make the default
  ambiguous — the ambiguity refusal is the honest replacement of
  today's implicit pick).
- REPL: `ReplSession` grows `current_board: Option<BoardId>`; the
  `boards use <uuid>` line sets it; verbs pre-fill from it. Affects
  completion (board ids join the completer's tables) — chunk 6 of the
  implementation sketch.
- UI: `ViewState.board` (Q8); every dispatch constructs commands with
  it (the router's exhaustive match forces each site).

**Snapshot churn**: command-shape changes rewrite the engine snapshot
corpus (20 files) — one deliberate `insta` regeneration pass inside
the vocabulary chunk (chunk 2), per Q9's standing rule.

**Topology note**: commands are engine-internal vocabulary (mpsc
payload shapes on existing channels); no channel is added, removed,
or re-pointed. The fan-out/fan-in seams are untouched. **Engine
additive; topology untouched.**

### 3.2 Q2 — SyncStatus keying + the F16-22 three-layer widening (D-W9: the user rules)

**Anchors.** `AppState.sync: SyncStatus` (state.rs:39), the struct
(state.rs:47–53: `phase`, `last_success`, `pending_ops`),
`SyncPhase` (58–71: `Idle`/`Syncing`/`Offline`/`Failed{last_error}`),
global `last_updated` (state.rs:42); engine mutators (core.rs:72
getter, :113 `mark_syncing`; actor.rs:611 `ingest_sync_report`,
668–681 `append_status_if_changed`, 728–743 `go_offline`);
storage `sync_status` table `CHECK (id = 1)` + idle seed
(0001_init.sql:102–109); repo persistence (505 load, 990–996 upsert;
`PersistenceAction::UpsertSyncStatus`); the three consumer surfaces
(UI badge presentation.rs:152–171 `SyncBadge` + status-bar.slint:14–52;
CLI status verb status.rs:46–53 `StatusView.sync` +
output.rs:136–145 `SyncStatusView`; sync verb's wait decision table
sync.rs:96–140; `wait_for_boot_pull` bootstrap.rs:705+).

**Option space (framing Q2's fork).** (a) per-board status with
connector attribution; (b) per-connector status; (c) `(connector,
board)` pair keying. Each states where the global `last_updated`
lands (F3-04 names both).

**Preliminary fact that collapses option (c).** A board's binding is
singular (`Board.remote: Option<RemoteBoardId>`, entities.rs:124) and
Q5's multi-binding refusals keep it singular — a board syncs through
at most one connector at any time (the "board moved between
connectors" exit criterion is a rebind, not a dual-bind). Therefore
board → connector is a *function*, and a `(connector, board)` pair
key stores the connector twice (once in the key, once derivable from
the board). Option (c) is option (a) with a redundant key component:
it has no state a reader can reach that (a) does not also reach, at
the cost of a wider table, a two-column key, and a new consistency
rule (pair entries whose connector disagrees with the board's
binding). It is priced below for completeness and dismissed on this
ground.

**Evaluation.**

*Option (a) — per-board, attribution derived.* The domain shape:
`AppState.sync` becomes a map keyed by `BoardId` (sketch: a newtype
`SyncStates(BTreeMap<BoardId, SyncStatus>)` with `get(board)` and an
`aggregate()` fold for the surfaces that want one badge-shaped
answer). Connector attribution is **not a stored field** — it is
derived from the board's connector-tagged binding (Q3: `Board.remote`
widens to a tagged binding). Single source of truth; no
denormalisation to keep consistent; unbind (Q5) simply deletes the
board's status rows.
- Engine mutator diff is the minimum of the three: reports are
  per-board cycles (`SyncReport::Completed` carries one board's
  snapshot — messages.rs:292–336), so `append_status_if_changed`
  re-keys by the report's board with no aggregation step;
  `mark_syncing(board)` marks the board the triggering command named
  (the engine knows it post-Q1; the board-less `RequestSync` nudge
  marks every bound board — the nudge fans to every connector);
  `go_offline` scopes to the down connector's boards (Q7 derives the
  set from the same bindings).
- Storage: one row per bound board, `board` primary key (Q9's 0006
  rebuild). Pre-migration rows: the legacy singleton backfills the
  ≤1 bound board a v0.1.x database can have.
- Surfaces: the UI badge reads the *selected* board's status (Q8) —
  board-level freshness is exactly what a board-scoped surface wants;
  the CLI `status` verb lists per-connector health by grouping the
  per-board rows on derived attribution (a connector with zero bound
  boards lists as configured-and-idle — honest). `pending_ops` per
  board derives from the outbox by the entity graph (the same filter
  `read_scope` already implements, boards.rs:200–224).
- D-W7 projection (Q7): all-down is computed by the app-layer
  authority from per-connector events, not from this map — the map
  never claims global network truth; per-board `Offline` phase is set
  by the engine when a connector's boards go dark. Clean separation.
- Cost: N rows instead of 1; the aggregate fold exists for
  back-compat surfaces (the daemon `status` answer keeps an
  aggregate-shaped `sync` field, see below).

*Option (b) — per-connector.* `AppState.sync` keyed by `ConnectorId`.
- Engine mutator diff: reports arrive per board; attributing a
  per-connector row requires mapping board→connector at every ingest
  (available, one hop) and *aggregating* multiple board-cycles into
  one phase — the aggregation rule ("two boards, one succeeded one
  failed — what phase?") is a new policy the design would have to
  invent and pin, for a grain no surface asked for.
- Information loss: two boards on one connector are
  indistinguishable in status (which board is stale?). The UI badge
  for the selected board cannot answer "when did *this* board last
  sync" without re-deriving per-board data the map threw away.
- Storage: fewer rows; identical rebuild cost (the singleton table
  dies either way).
- D-W7 projection: connector-level Offline is natural here — but Q7
  shows the authority lives in the app layer over events, not in this
  map, so (b) gains nothing there.

*Option (c) — pair keying.* As argued: strictly (a) plus a redundant
component; wider migration, wider key, new consistency rule, no new
reachable state.

**Where the global `last_updated` lands (F3-04's second half).**
Recommendation: **unchanged in place and semantics** — a global
"last local write anywhere" (`AppState.last_updated`, state.rs:42).
It is cheap change-detection metadata, already maintained by the
engine's interpret loop, board-agnostic by design; per-board write
recency already exists as `BoardClocks`/entity clocks (entities.rs:95+)
for every consumer that needs board grain. Widening it to a map would
rewrite every mutation site for no named reader.

**Recommendation — option (a).** Per-board status keyed by `BoardId`,
connector attribution derived from the binding (never stored), global
`last_updated` unchanged.

**Wire-compat consequence (D-W6).** The JSON contract grows
additively: the daemon `status` answer keeps its `sync` field
(populated from the aggregate fold over the default connector's
boards — byte-shape unchanged for the single-board v0.1.x install)
and adds a `connectors: […]` array (name, derived health, boards
with their statuses) plus a `boards: […]` per-board view. The CLI
human output renders the arrays; no field is removed.

**MD-8: this is a recommendation only.** The decision record row
(§5) for D-W9 is marked **open — user rules**. The user choosing (b)
instead changes: the domain map key (`ConnectorId`), the ingest
aggregation rule (a small policy to pin), the 0006 table shape
(connector PK), and the badge read (selected board → owning
connector's row). Q9 prices both shapes so the ruling is informed;
the rest of the design (Q1/Q3/Q4/Q5/Q7) is keying-agnostic.

**Topology note**: status is engine state + a storage table; reports
already arrive on the permanent fan-in inbox per board-cycle. No
channel changes. **Engine-internal widening; topology untouched.**

### 3.3 Q3 — Connector addressing home (F16-05)

**Anchors.** `App.connectors: Mutex<Vec<SyncActor>>` (bootstrap.rs:141)
with the `.first()` anonymous pick inside `sync_commands()`
(178–188, its own comment deferring to "the multi-connector round");
`ensure_vacant`/`try_register`/`AlreadyAttached` (266/307/431/443–460);
the engine's fan-out registry + lazy pruning (state/actor.rs:328,
449, 585–600) and `EngineEnvelope::AttachConnector` push (43/75/396);
`SyncCommand::{SetBoard, SyncNow, Detach}` (messages.rs:272–288) with
`SetBoard`'s untargeted shape (274); the resume pick (sync/actor.rs:
327–334) and the absorber's target/cache handling (~256–266);
`Board.remote` (entities.rs:124) + `RemoteBoardId` (ids.rs:80);
`spawn_sync_actor` (sync/actor.rs:120–129; bootstrap call 293–300).

**Option space.** (a) routing tag on every `SyncCommand`; (b)
app-layer registry map + per-connector command channels (routing
table board→connector, engine fan-out unchanged, app routes); (c)
enveloped per-connector commands (connector id on an envelope the app
unwraps). Plus: where connector ids are minted (config key vs minter)
and typed (domain newtype vs app-local String).

**Key structural fact.** Targeted `SyncCommand`s do not travel the
engine's fan-out — they travel the app-held per-actor sender
(`send_set_board` → `App.sync_commands()`, boards.rs:496–501). The
engine fan-out carries only the broadcast nudge (`SyncNow`) and the
engine never originates a targeted command today. So "where does
routing live" is really "who picks the channel": the app picks it in
all three options; the question is whether the *message also carries*
the decision.

**Evaluation.**

*Option (a) — tag on every command.* `SyncCommand::SetBoard {
connector, board }`. The tag is redundant with the channel the app
already chose (the sender had to know the connector to pick the
channel); `SyncNow` is a broadcast — a tag on it is meaningless or
wrong (which connector? all of them); `Detach` is app→one-actor by
construction (WI-H). Cost: every future targeted command grows a
field whose only reader is the actor that already received it via the
right channel. Benefit: the message is self-describing in logs. That
benefit is better served by the actor tracing its own connector id
(Q4's module carries it).

*Option (b) — app-layer routing.* `SyncActor` grows `id:
ConnectorId`; `App` gains `connector_commands(&self, id) ->
Option<Sender<SyncCommand>>` (a named lookup for any targeted send).
The routing table board→connector **is** the set of `Board.remote`
tagged bindings — no separate table to maintain; reading state *is*
the lookup. The engine fan-out stays broadcast-only: unchanged code,
unchanged topology. (The anonymous `sync_commands()` `.first()` pick
has exactly one caller today — `send_set_board`, boards.rs:497 — so
`SetBoard`'s retirement below removes the method outright rather
than re-pointing it.)

*Option (c) — envelope.* `enum ConnectorCommand { SetBoard(…),
Detach }` wrapped in `Targeted { connector, inner }`, app unwraps.
Same routing knowledge as (b) with an extra vocabulary layer and an
unwrap step; the envelope's connector field has exactly option (a)'s
redundancy problem. Connect-flow §4.5 named (a)-or-(c) as the
anticipated shapes; both were sketched before the ownership-tag
design made the app-side lookup complete — with bindings tagged, the
tag-on-message variants carry no information the channel choice
didn't already encode.

**Minting and typing.** Connector ids are **config-minted**: each
`[[sync.connectors]]` entry's `name` key (Q6) — human-meaningful,
stable across restarts, validated unique + kebab-case at load (typed
`ConfigError`). Typing: a **domain newtype** `ConnectorId` (ids.rs
house pattern; wraps a `String` — unlike entity ids it is a *name*,
not a UUID). It must be domain-typed because the *binding* carries it
(`Board.remote` widens to a tagged binding — connect-flow §4.5's
"source/connector tag on the binding", an additive domain field), and
Q2's attribution derives from it. An app-local `String` would force
the binding to store an app type — a boundary violation.

**`SyncCommand::SetBoard`'s fate.** Today it exists because the actor
owns *one* implicit target that must be told the binding (the
ceremony's `send_set_board`). With Q3's ownership tags, the actor's
board set is **derived from persisted bindings every cycle** (boards
whose `binding.connector` equals the actor's own id — the resume-path
generalisation of actor.rs:327–334): a newly bound board is picked up
by the owning connector's next read with no message at all. The
ceremony's need is only "cycle soon" — the broadcast `SyncNow`
satisfies it (attach is a rare, human-scale event; the cost is other
connectors' cycles, each cheap for unchanged boards via per-board
validators — `ValidatorKey::Stacks(RemoteBoardId)`, persistence.rs:
123–131 — the #157 guardrail, §3.9/§7). Recommendation: **retire
`SetBoard` from the vocabulary** in the same motion that tags the
bindings; `LoopState.target: Option<RemoteBoardId>` becomes the
derived per-board target set with per-board cache keys (no
cache-clear-on-retarget). Consequence: `SyncCommand` reduces to
`{SyncNow, Detach}` — simpler port surface for Q4, and the
connect-flow §4.5 "untargeted SetBoard" item is resolved by removal
rather than by tagging. (`SyncCommand` is in-process mpsc only —
never persisted, never on the IPC wire — so reshaping it is not a
compat surface; verified: no repo/codec/protocol reference encodes
it.)

**Recommendation — option (b).** App-layer named routing over the
tagged-binding table; `ConnectorId` domain newtype minted from config
names; `SetBoard` retired.

**Consequences.** `App::attach_connector` records the id
(`try_register` gains the key; `AlreadyAttached` becomes
`ConnectorNameInUse(ConnectorId)` — same guard, named); the resume
pick and `wait_for_adoption` parameterise by board; `spawn_sync_actor`
gains the connector id (the actor learns its name — board-set filter
+ tracing); `connector_attached()` stays; the daemon-side attach verb
(ADR 0014 completion note) grows a name parameter; `App::
sync_commands()` itself is removed with `SetBoard` (its sole caller
was the ceremony's send). CLI verb growth rides Q6 (chunk 6).

**#157 amplification pricing (framing §8, work-plan risk).** The
per-connector cycle iterates the connector's boards sequentially; per
board: three listing pulls (boards/stacks/archived-stacks) served
`304` by per-board validators when unchanged, plus per-card detail
refresh only for boards whose listings changed. Cycle cost is
therefore O(boards + changed cards), not O(total cards). Guardrail
recorded; the perf work itself stays #157, unscheduled.

**Topology note**: no new channel; the app routes over senders it
already holds; the engine fan-out is untouched (it never carried
targeted commands). **App-layer diff only; topology untouched.**

### 3.4 Q4 — Trait promotion shape (D-W4, F16-08)

**Anchors.** `SyncStateReader` (persistence.rs:68) on the hand-rolled
`BoxFuture` alias (persistence.rs:25) — module doc: "the domain may
not depend on `futures`, `tokio`, or `async-trait`" (verified against
domain's Cargo.toml: serde/chrono/uuid/thiserror/tracing only, the
tracing dep being macro-level observability per its own comment); the
full
vocabulary surface (messages.rs:272–336, persistence.rs:59–100);
`spawn_sync_actor`'s free-function signature (sync/actor.rs:120–129)
and its single call site (bootstrap.rs:293–300); ADR 0010's stance
headline (.org:140) + consequences mention; AGENTS §2 crate rules;
deny.toml (license/ban gates — the boundary exit criterion is
`cargo deny` staying green, which a domain type + app trait cannot
affect: no new dependencies anywhere).

**Option space.** Trait surface: (a) one port trait owning
commands+reports+state-reader+spawn vs (b) split ports (vocabulary
contract vs spawn/factory). Home: (c) domain crate via BoxFuture vs
(d) a new tiny `taskboard-connector-port` crate. Dispatch: object-safe
`dyn` vs generic.

**Evaluation.**

*What can live in the domain.* The data half is already there and
already a trait where a trait fits: `SyncCommand`/`SyncReport` are
enums (a message vocabulary — they cannot "become" a trait without
losing their exhaustive-match discipline, AGENTS §5's compiler-forced
arm argument), and `SyncStateReader` is a domain trait on the
BoxFuture pattern. The wave adds `ConnectorId` and the tagged binding
to the same home. AGENTS §2 compliance is inherited, not re-argued:
zero network/UI/async-runtime deps (verified above).

*What cannot.* A spawn contract returns a `tokio::task::JoinHandle`
(sync/actor.rs:127) — a tokio type, forever outside the domain. A
factory consuming `AppConfig` slices + `CredentialStore` consumes app
types. Forcing either into the domain via BoxFuture-style erasure
(`BoxFuture<'static, JoinHandle>`— impossible; the handle *is*
runtime-affine) or by duplicating config types in the domain would be
purity theatre: a seam that *looks* central but re-states app types.

*Option (a) — one trait owning everything.* Requires the spawn half
to live where the tokio types live → the trait itself leaves the
domain → the "port in the domain" reading fails; or the trait stays
in the domain *without* spawn and owns only data — which is option
(b)'s first half wearing option (a)'s name.

*Option (b) — split ports.* Piece 1 (domain, exists): the vocabulary
contract — `SyncCommand`/`SyncReport`/`SyncStateReader` +
`ConnectorId`/tagged binding. Piece 2 (app, new):

```rust
// taskboard-app — illustration, not a diff (MD-7)
/// One configured connector implementation: builds its client from
/// config + secrets and spawns its actor over the shared vocabulary.
pub trait ConnectorModule: Send + Sync {
    fn id(&self) -> &ConnectorId;
    fn build_and_spawn(
        &self,
        engine_reader: Arc<dyn SyncStateReader>,
        reports: mpsc::Sender<SyncReport>,
        system: broadcast::Sender<SystemEvent>,
        cfg: SyncActorConfig,
    ) -> Result<SyncActor, AttachError>;
}
```

with one impl (`NextcloudModule`: `DeckClient` construction from the
connector's config entry + the credential store, wrapping today's
`spawn_sync_actor`). Bootstrap iterates configured modules — the loop
is the N=2 exit criterion made real: a second connector crate ships a
`ConnectorModule` impl + its config entry decode; bootstrap,
attach, and shutdown code is unchanged.

*Home (c) vs (d).* (c) domain: holds for piece 1 (already does). For
piece 2 the domain cannot host it (above) — so "home: domain" is true
exactly of the data contract and false of the factory, which is the
split. (d) a new `taskboard-connector-port` crate for the factory:
one consumer (the app), zero reuse horizon (a second connector
*implements* the vocabulary; it does not consume the factory), an
extra workspace member + licensing header surface — rejected as
speculative structure (the framing §8 scope-creep guardrail; F16-07's
"no speculative prep").

*Dispatch.* `dyn ConnectorModule` (object-safe by construction — the
sketch's methods take/return concrete types): bootstrap holds
`Vec<Arc<dyn ConnectorModule>>` built from config. Generic dispatch
would parameterise bootstrap for one element count of variety — noise.

*Narrow-seam check (trap (f)).* The trait has no discovery, no
dynamic loading, no plugin registry, no versioning handshake: config
names a module, the app constructs it. The seam is exactly ADR
0010's anticipated promotion, one connector later than anticipated
and decided now (D-W4).

*Test-fake story (AGENTS §5).* A handwritten `FakeModule` in app
tests spawns a scripted actor over the same channels (answers
`SyncNow` with configurable `SyncReport`s) — bootstrap/ceremony
integration tests gain connector control without wiremock; the
engine side keeps its existing `InMemoryRepository`-style fakes over
`SyncStateReader`.

**Recommendation — option (b)/(c).** Split ports: the domain
vocabulary (already home) + the app-layer `ConnectorModule` factory
(`dyn`); no new crate.

**Consequences.** The exit criterion "`cargo deny` boundary gates
stay green" is met trivially (no dependency changes in any crate).
ADR 0010's amendment (draft in §6) rewrites the "vocabulary, not a
trait" stance to record the two-piece port. The sync crate's public
surface is unchanged (`spawn_sync_actor` stays public; the module
adapter calls it) — chunk 4 of the sketch implements the trait; the
amendment text lands with that chunk (dispatch §4, MD-9).

**Topology note.** A factory constructing the *same* channels the
app already constructs. **No channel changes; topology untouched.**

### 3.5 Q5 — Ceremony redesign (F16-24)

**Anchors.** `select_on` (boards.rs:566–628: single-bound-board
check → `LocalBoardExists` refusal → `BindBoard` → `SetBoard` →
`wait_for_adoption`), `publish_on` (:428+), `read_scope`
(:163–225 — the pre-count filter over entity graph + outbox),
`destruct_on` (:235–259 — typed `ConfirmationRequired { verb, stacks,
tasks, labels, pending_ops, bound }` refusal, else dispatch +
`BoardDestructReceipt`), `send_set_board` (:496–501); the engine
seeds (`CommandError::BoardAlreadyBound` command.rs:84; app mirror
mod.rs:108); the stale-binding pre-write-fetch re-resolve landed by
remediation chunk 5 (push_exec mod.rs:10–11, execute.rs:471–479 —
"#258"); outbox `DeleteBoard` tombstone-finalizing push (outbox.rs:53
+ actor's dead-board 404/403 carve-outs, actor.rs:703–766); issue
texts #179 (unbind: "what happens to remote-bound content") and #170
(title-match reconciliation, proposed bind-time pass).

**Sub-question 1 — multi-binding refusal vocabulary.** Options:
widen `BoardAlreadyBound` in place vs a dedicated variants list.
Recommendation: **the variants list** (typed-outcome discipline —
each refusal names *which* rule fired): engine-side, `BindBoard {
board, remote }` refuses with `UnknownBoard(board)` /
`BoardDeleted(board)` / `BoardAlreadyBound { board, bound }` (this
board already bound elsewhere; same-remote rebind stays an accepted
no-op) / **new** `RemoteBoardInUse { remote, board }` (another local
board already holds that remote binding — the multi-board mirror of
today's single-board guard, the F8-06 seed grown to its pair). App
mirrors widen mechanically; the select ceremony's pre-check folds
into the engine's refusals (its own `BoardAlreadyBound { current }`
construction at boards.rs:589–590 becomes a pass-through of the
engine's typed outcome — one rule, one home).

**Sub-question 2 — unbind (#179).** New command
`StateCommand::UnbindBoard { board }`: clears the board's binding;
all content stays local (the issue's open question — answer: **keep
local copies**; remote-bound content is already local content by the
merge policy). Three design points:
- *Child refs.* Stacks/tasks/labels carry their own remote refs, and
  those refs self-identify their remote board (`RemoteStackRef.board`,
  `RemoteCardRef.board`, ids.rs:98–114) — stale refs can never be
  confused with another remote's namespace. Unbind keeps them (they
  make same-remote rebind a re-adopt: the pull re-matches children by
  their refs exactly as it matches boards). The dangerous case is
  *rebinding to a different remote while children still carry the old
  remote's refs*: the push would address the old remote. Rule:
  **`BindBoard` on a board whose descendants carry refs from a
  different remote wipes those refs** (fresh binding ⇒ descendants
  become local-only; pending creates re-create them on the new
  remote, the pull adopts the new remote's content), with the wipe
  counted in the bind's outcome receipt.
- *Outbox interplay.* Ops for an unbound board strand honestly: the
  push filters by boards owned by the cycling connector (Q3), so the
  ops wait (`pending_ops` "keeps climbing honestly" — ADR 0010's
  standalone precedent) and drain on rebind. No silent cancellation,
  no data loss. For update-class ops on entities whose refs a
  bind-wipe cleared: the push's existing unknown-binding path
  dead-letters them, and the ceremony receipt counts them so the
  operator sees the consequence before confirming.
- *Ceremony.* `boards unbind <board>`: `destruct_on`'s shape —
  typed refusal without `--confirm` stating the scope (bound remote,
  pending ops that will strand, descendant-ref count), else dispatch
  with a pre-count receipt — the loud-flag pattern
  `connect --switch-backend`/`--force` set (framing §3).

**Sub-question 3 — title-match attach (#170).** Options: implement
the reconciliation pass vs keep identity-based attach with the union
caveat documented. D-W2 pulled #170 into the wave, so "document the
caveat" is not on the table; the options are *where* the pass lives.
Recommendation: an **engine command** `StateCommand::ReconcileByTitle
{ board }`, run by the bind ceremony automatically after
adoption-wait and available as `boards reconcile <board>` for manual
re-runs (idempotent). Semantics: after the first pull has unioned the
sets, match local-only entities against remote-bound siblings by
normalized title (trim + casefold — deterministic, no fuzzy matching
in-wave); for each matched pair, adopt the remote identity and merge
fields through the existing per-field clock merge (the same machinery
ADR 0004's amendment extended to boards); duplicate create-ops in the
outbox for the absorbed local entity are cancelled (counted
`FailOp`s). Typed outcome with counts (stacks/labels/tasks merged,
ops cancelled) — the receipt is the ceremony's honesty. Tasks match
only within matched stacks (title-in-context), keeping the pass
O(same-title pairs) and conservative: **no match ⇒ both stay** (the
documented union behavior for genuinely distinct names). Trade-off
recorded: title heuristics can mis-merge same-named-but-distinct
entities — mitigated by running *after* the operator explicitly chose
`--bind-local-board` onto that remote (the intent is "these are the
same board"), and by the receipt showing what merged.

**Sub-question 4 — reset/delete of one board among N.** A pure
consequence of Q1: `destruct_on(app, verb, confirm, board, command)`
parameterises `read_scope(board_id)` (today it takes only `app` and
finds the first live board itself — boards.rs:170–174; the
parameterisation is mechanical) and
dispatches `DeleteBoard { board }` / `ResetBoard { board }`. The
typed refusal states *that board's* counts; the receipt names the
board id. Tombstone-finalizing push at N: the `DeleteBoard` op drains
through the board's owning connector (Q3's filter) — exactly one
connector finalizes it because exactly one owns it.

**Sub-question 5 — adopt/resume at N boards.** Which actor resumes
which board: the ownership filter (Q3) — each connector's actor
adopts/resumes precisely its tagged boards; `wait_for_adoption`
parameterises by board id and watches that board's status row (Q2).
The stale-binding re-resolve composes unchanged: it operates
per-entity within a push of one board's ops (#258's machinery is
board-agnostic).

**Topology note.** All ceremony steps are engine commands +
app-layer orchestration over existing handles; `SetBoard`'s
retirement (Q3) *removes* a message rather than adding one.
**Additive/contractive on vocabulary; topology untouched.**

### 3.6 Q6 — Config shape

**Anchors.** `[sync]` scalar resolution (`SyncBackend` enum
config.rs:159–177; `SyncSection` with `deny_unknown_fields` :172;
`sync_backend()` :526–540; validation loudness :542–570 — the
half-configured-file-stays-loud rule); the ceremony's persist path
(`persist` connect.rs:390–413; the comment-preserving `toml_edit`
overlay setting `doc["sync"]["backend"] = "nextcloud"` :509);
`require_connector` (mod.rs:46–51); figment `0.10` (workspace
	Cargo.toml:62, features `toml`+`env`; lock-resolved `0.10.19`); the load chain
`Serialized::defaults → Toml::file → Env::prefixed("TASKBOARD_").split("__")`
(config.rs:714–718).

**Figment probe (MD-2 evidence; run against figment `=0.10.19` in a
throwaway /tmp crate — no repo code; the probe source is the exact
chain of the app's load with a two-field `[sync]` probe struct).**
Observed:
1. `[[sync.connectors]]` array-of-tables (two entries) decodes into
   `Vec<ConnectorCfg>` — supported, ordered.
2. Scalar + array **both present decode silently** — figment/serde
   does *not* reject the mix; loudness must come from
   `AppConfig::validate` as a typed rule.
3. Scalar alone decodes (today's world).
4. An unknown key inside a connector entry is **silently ignored**
   unless the entry struct is `#[serde(deny_unknown_fields)]`.
5. `connectors = []` decodes as an empty vec.
6. `TASKBOARD_SYNC__BACKEND=standalone` over a file containing an
   array: the env overrides the scalar, the array survives — the env
   override path composes with the new shape.
7. `TASKBOARD_SYNC__CONNECTORS=junk`: typed figment error
   ("invalid type: found string, expected a sequence") — **connectors
   cannot be configured via env**; the scalar env override (case 6)
   remains the scripting seam.
8. A strict (`deny_unknown_fields`) `[sync]` struct accepts the
   known scalar+array mix (case 2 under strictness); unknown `[sync]`
   keys are rejected with a typed error naming the key — the
   precedent for entry-level `deny_unknown_fields`.

**Option space.** `[[sync.connectors]]` named entries (fixed); the
legacy-scalar resolution rule variants: (i) scalar aliases the
default/first connector (mix tolerated), (ii) scalar synthesizes a
named entry at load (mix rejected), (iii) scalar rejected once any
array exists (no synthesis); validation loudness: which mixes are
typed errors vs tolerated.

**Evaluation.** Variant (i) tolerates scalar+array coexistence: with
two nextcloud connectors, `backend = "nextcloud"` names *which one*?
Ambiguous — framing §8's "never silently pick" says no. Variant (iii)
rejects the mix but gives the v0.1.x file no array path until the
user rewrites it — `connect --switch-backend`'s scalar write
(connect.rs:509) into an array-carrying file would error, breaking
the write path the framing promised to keep valid. Variant (ii):
scalar (with `[nextcloud]`) *is* the zero-migration representation of
one connector named `nextcloud`; arrays are the N ≥ 1
representation; **any mix of the two representations is a typed
validation error** (`ConfigError::ConnectorShapeMixed` — naming both
keys). The write path stays valid because the ceremony's overlay
writer becomes shape-aware: no array in the file ⇒ today's scalar
write verbatim; array present ⇒ the overlay rewrites the *default
(first) entry's* `server_url`/`username` keys and never touches
`backend`. Round-trip holds in both worlds.

**Recommendation — variant (ii) with loud mix rejection.**
- New section shape (illustration, MD-7):

  ```toml
  [[sync.connectors]]
  name = "work"            # kebab-case, unique — the ConnectorId mint (Q3)
  backend = "nextcloud"    # the connector kind (module selector, Q4)
  server_url = "https://nc.work.example"
  username = "viktor"

  [[sync.connectors]]
  name = "personal"
  backend = "nextcloud"
  server_url = "https://nc.home.example"
  username = "vp"
  ```

- Entry struct `#[serde(deny_unknown_fields)]` (probe case 4 — the
  typo guard; `SyncSection`'s own precedent at config.rs:172).
- Resolution: array present ⇒ the connector set is exactly the array
  (first entry = the default connector); array absent ⇒ the legacy
  world verbatim — `sync_backend()`'s auto rule stands, and a
  resolved `nextcloud` synthesizes one connector named `nextcloud`
  from `[nextcloud] server_url`/`username`; resolved `standalone` ⇒
  zero connectors.
- Validation (typed, loud): scalar + array both present ⇒ error;
  `[nextcloud]` table + array both present ⇒ error (the array owns
  server/user keys); `backend = "standalone"` + array ⇒ error;
  duplicate or non-kebab `name` ⇒ error; `backend = "nextcloud"`
  resolved with an empty array ⇒ error (backend names a connector
  that does not exist). `[sync]`'s poll/backoff knobs stay global
  (per-connector tuning is a backlog candidate, §7).
- Env: the scalar override keeps working (probe 6); connector
  configuration via env is impossible (probe 7) — documented
  limitation, matched by figment's own model.
- `require_connector` widens to "at least one connector configured"
  (same typed `SyncDisabled` refusal, pluralised semantics); verbs
  grow `--connector <name>` where targeting matters (chunk 6).
- Credential store: already keyed per-(server, username) — N
  identities out of the box (framing §3).

**D-W6 conformance.** A v0.1.x single-board config (scalar +
`[nextcloud]`) loads, validates, and behaves identically — the
synthesized connector *is* today's bootstrap input; the exit
criterion's config half is met with zero file migration.

**Topology note.** Config is load-time data; the modules it names are
constructed over the same channels (Q4). **No channel changes;
topology untouched.**

### 3.7 Q7 — Network authority mechanics (D-W7 decided; mechanics open)

**Anchors.** The offline FSM: `LoopState.offline` (sync/actor.rs:160,
init :187); `fail_cycle` emits `NetworkLost` exactly once per episode
on transport-classified failures (`matches!(kind,
SyncErrorKind::Network) && !st.offline`, :498–502; classification
:396); the success path reports first, then `NetworkRestored`, then
clears the flag (:451–470); the actor's own select loop ignores the
events ("this actor *is* the network authority anyway", :236–239);
`SystemEvent` (messages.rs:249–261: `NetworkLost`/`NetworkRestored`/
`Shutdown`/`StorageFailed`); the engine's arms (state/actor.rs:443–
450: `NetworkLost` → `go_offline` (idempotent, sets the global phase
`Offline`, :728–743); `NetworkRestored` → `forward_sync_now`, "the
next report is the evidence"); broadcast wiring (bootstrap.rs:77
capacity, per-actor `system` clones); consumers of the phase: UI
badge (presentation.rs:160–171 → status-bar.slint) and CLI.

**The D-W7 bar**: all connectors down ⇒ `NetworkLost`; any responding
⇒ healthy. An option that emits `NetworkLost` while one connector
responds is wrong (work plan's evidence list).

**Option space.** (a) each actor emits its own per-connector state
and an app-layer aggregator computes all-down and emits the system
event; (b) actors keep emitting `NetworkLost`/`Restored` and a single
authority reconciles/suppresses (subscription-side aggregation);
(c) the engine aggregates from report attribution (rides Q2's
keying).

**Evaluation.** The blocking fact for (b) and (c): a
`broadcast::Receiver` cannot tell *which sender* a unit-variant event
came from. (b)'s "reconcile/suppress" would have the authority
guess provenance — or the variants must carry the connector id
anyway, at which point (b) has become (a) with extra steps. (c)
aggregates from *reports*: a down connector's evidence is the
*absence* of reports — "no report lately" conflates a slow cycle, a
backoff wait, and a dead transport (today's code deliberately never
infers from silence: `NetworkRestored`'s arm comment "the next report
is the evidence"). (c) also couples the engine to connector
counting it cannot see (the registry is app-layer). So the mechanics
land in (a)'s shape by elimination; the design freedom left is the
event vocabulary and the authority's home — both fixed by minimal
diff:

- *Actors* emit **new per-connector variants** carrying the id —
  `SystemEvent::ConnectorLost(ConnectorId)` /
  `SystemEvent::ConnectorRestored(ConnectorId)` (additive variants on
  the existing broadcast; the emit sites keep their exactly-once-per-
  episode edge semantics verbatim). The global names
  `NetworkLost`/`NetworkRestored` are **retired from the actors** and
  become the authority's vocabulary — same names, same meaning
  (all-down / any-back), now emitted with the counting done.
- *The authority* is an app-layer task spawned in bootstrap: holds
  the per-connector up/down set (seeded from the configured module
  set; updated by `attach_connector`/`detach_connector` registry
  hooks — app-internal lifecycle bookkeeping, not inter-actor
  traffic), subscribes the system broadcast, and emits the existing
  `NetworkLost` when the last up connector flips down /
  `NetworkRestored` when the first flips up. The engine's existing
  arms (:443–450) keep their exact behavior — now driven by the
  counted truth. Single-connector installs are byte-for-byte today's
  semantics (one connector: per-connector down *is* all-down; the
  authority is a pass-through with the same edge trigger).
- *Per-board scoping (rides Q2)*: on `ConnectorLost(c)` the engine
  sets the phase of *c's boards* to `Offline` (the same
  `go_offline` write, scoped by the binding tags); on
  `ConnectorRestored(c)` it forwards the nudge. The engine learns the
  connector's board set from its own state (bindings) — no new
  channel, no counting.

**UI banner semantics.** The all-down banner is the existing global
phase path — unchanged rendering, now honest under partial
connectivity (D-W7: one down server is normal operation, not an
outage signal; the global phase never flips for it). Per-connector
detail: the CLI `status` verb's `connectors` array (Q2 consequence)
and the desktop status surface's per-board badge (the selected
board's status goes `Offline` when *its* connector is down — the
distinction the user can actually act on). Kiosk (R9): the badge path
is already the minimal surface; no kiosk-specific banner work.

**Flapping/hysteresis.** A flapping connector toggles its boards'
phases and its own per-connector events; the global event flips only
when the flap crosses the all-down boundary. Backoff
(`failure_streak`, `retry_after_floor`, `advertised_floor`) already
paces the actor's retries — no hysteresis is added in-wave (the
episode-edge rule *is* the debounce for the global signal: one
`NetworkLost` per crossing, not per failure). Recorded as a backlog
candidate if real usage flaps annoyingly (§7).

**Topology note.** New *variants* on the existing broadcast; a new
app-internal task (a subscriber + emitter on existing channels). The
engine's arms are additive. **No new channel; topology untouched.**

### 3.8 Q8 — UI board dimension (F7-05; R4/R1/R9)

**Anchors.** `presentation::live_board` (248–252, "the single live,
non-archived board (MVP 0..1)") consumed at view_state.rs:198
(`ViewState::stack_ids` derives from it); `ViewState` (58–72: `view`,
subjects, `focused_task`, `selected_stack`, `flow`, `buffer` — no
board field); the stack-switcher seam to mirror (`select_stack`,
view_state.rs:383–390: sets selection + resets focus to the new
stack's first task, returns `FlowOutcome::RefreshNow`; adapter.rs:571
wires segment clicks); the actions registry (`ActionId` enum
actions.rs:92–119 with dotted ids 122–166; `ActionEntry` 177–204;
`REGISTRY` 206+; `StackCreate` = `"stack.create"` at 107/141);
keymap (keymap/mod.rs:23–131 `Keymap` over
`BTreeMap<ContextId, BTreeMap<Vec<Stroke>, ActionId>>` with
chain resolution; presets.rs:80 `"S"` in Board, :165 `"O"` in
ViNormal for `StackCreate`); `IdTables` (models.rs:35–45) and the
`push` builder (:101); board-layout.slint (27–56: the compact
switcher over `selected-stack`); status-bar.slint (14–52: the sync
badge); R1 (compositions layer, primitives → compositions → views),
R4 (keyboard-only first-class; the scripted journey at GUI-plan:103
names "navigate boards" a core verb), R9 (breakpoint/container
behaviour; the board view's small-screen degeneration).

**Option space.** (i) `ViewState.board: BoardId` + board switcher as
action-layer actions mirroring the stack switcher; (ii) a view-tree
navigation level (boards as a navigable view). Sub-decisions:
selection home (pure UI state vs engine-visible — fixed by Q1:
caller-owned); `IdTables` re-keying per board vs per-view-instance;
board-bar composition under R1; kiosk-surface degradation under R9.

**Evaluation.** (ii) makes "which board am I on" a *view* (a node in
`ViewId`'s tree) rather than *state*: every board-scoped interaction
would then re-derive its target from the view position, the focus
walk (D7/D8's ring model) gains a whole level to traverse, and the
kanban centrepiece becomes N keystrokes deep. The stack switcher
settled this exact shape one level down: selection state + one
mechanism (keys and clicks both move it — "one mechanism, D8",
view_state.rs:381-382's doc). (i) mirrors it at the board level and
keeps the view tree flat.

**Recommendation — option (i), concretely (dispatch acceptance (e):
named integration points).**
- `ViewState.board: Option<BoardId>` (None = fresh install, zero
  boards); seeded by `presentation::live_board` when None (the
  deterministic first-live rule survives as the seed only); every
  board-scoped read that today flows through view_state.rs:198 flows
  through `self.board` instead.
- **Actions** (`actions.rs`): three new `ActionId`s — `BoardNext`
  (`"board.next"`), `BoardPrevious` (`"board.previous"`),
  `BoardSelect` (`"board.select"` — the switcher strip's click
  target, mirroring how segment clicks route through the action
  layer's `select_stack` seam). Registry rows: `contexts` =
  `VIEW_NAV`-style `[Board, StackDetail, TaskDetail]` + `Global`
  (switching boards is a global-capable verb, like the nav.* family),
  `category` = navigation, `enabled = true`.
- **Keymap** (`keymap/presets.rs`): `"b"`/`"B"` in `Board` and
  `ViNormal` contexts for next/previous (single-stroke, mirroring
  `"S"`/`"O"`'s precedent); `"]b"`/`"[b"` as bracket-style aliases if
  the presets table wants them — the preset table is the tunable;
  the design fixes only that the entries exist and resolve in the
  `Board` + `ViNormal` contexts.
- **Dispatch** (`router.rs` / `view_state.rs`): a new
  `ViewState::select_board(&mut self, state: &AppState, board:
  BoardId) -> Vec<FlowOutcome>` mirroring `select_stack`: sets
  `board`, resets `selected_stack` to the new board's first stack and
  `focused_task` to that stack's first task, returns
  `[FlowOutcome::RefreshNow]`. `BoardNext`/`BoardPrevious` walk the
  canonical board order (the presentation module's ordering helper
  — same order as `boards list`) with the no-wrap rule D7 applies at
  the ends. **Dumb-client check (AGENTS §1)**: switching performs no
  engine command — `FlowOutcome::Command` is never produced; board
  switching is pure view state, exactly like stack switching. This is
  Q1's caller-owned selection made concrete — one selection home
  (ViewState), not two.
- **`IdTables`**: **no re-keying** — the tables are per-refresh of
  the viewed board by construction ("same thread, rebuilt together
  with the models", models.rs:31-33); the `push` builder derives its
  board argument from `ViewState.board` instead of
  `presentation::live_board`. Per-view-instance tables would
  double-refresh N−1 invisible boards per cycle — rejected.
- **Board bar (R1)**: a new composition `board-bar.slint` (L2 layer,
  sibling of toolbar/status-bar) rendering the switcher strip:
  regular/expansive show one segment per live board (title, archived
  marker, selected state); compact hides the strip (R9's degeneration
  — the board-layout compact path already drops chrome; keyboard
  switching still works). Primitives are reused, none restyled
  view-locally (R1's rule).
- **Keyboard journey (R4)**: the scripted journey gains its board leg
  — with two boards seeded, the journey performs `board.next`, asserts
  the view re-rendered the second board's stacks, `board.previous`
  back. Board switching operable by keyboard alone is the wave's
  exit-criterion wording, and the journey test is its pin.
- **Kiosk (R9)**: the kiosk surface inherits the compact
  degeneration (no strip); switching remains keyboard-available to
  the operator. A config-pinned fixed-board kiosk (one board, no
  switching) is a **backlog candidate** (§7) — not needed for the
  exit criteria and not pre-built (the no-speculative-prep stance).
- **Badge interplay (Q2)**: the status badge shows the selected
  board's status; switching boards re-reads the per-board map in the
  same refresh pass (no extra wiring — the badge derives from the
  state the refresh already loads).

**Consequences.** Snapshot/harness implications: the UI crate's
view-state tests grow board-dimension cases (select/seed/no-wrap);
no `insta` churn in the UI crate (its tests are plain assertions);
the engine corpus churn belongs to Q1/Q2's chunks. Chunk 7 of the
implementation sketch owns all of the above.

**Topology note.** Pure presentation layer: reads `ArcSwap` state,
emits view-local outcomes. **Engine untouched; topology untouched.**

### 3.9 Q9 — Migration & compat plan (D-W6)

**Anchors.** 0001's conventions block (migrations/0001_init.sql:3–11:
UUID TEXT ids, chrono TEXT timestamps never in ORDER BY, `ck_*`
clocks, remote bindings as nullable columns with codec-enforced
all-or-none, no triggers/views/generated columns — logic in Rust) and
its `sync_status` seed (:109); 0003's backfill precedent (add column
→ `UPDATE … WHERE` move → the codec's column-per-tag rule);
0005's two precedents in full (epoch-default `ck_*` columns keeping
pre-migration rows' remote-wins merge behavior bit-for-bit; the
in-migration `rename_board` → `update_board` re-tag with decode
rejecting the legacy tag — codec.rs:56/157/401–409 + the round-trip
pin connect.rs:297); the snapshot corpus (20 `.snap` files, §2); the
config additive path (Q6); sqlx's per-file transactional migrations.

**Design — migration 0006 (`0006_multi_board_sync_status.sql`,
sketch against the conventions).**

```sql
-- 0006_multi_board_sync_status: the SyncStatus singleton becomes a
-- per-board row (F16-22 three-layer widening, wave chunk 1); board
-- bindings gain their connector tag (connect-flow §4.5).
-- Convention notes: timestamps stay chrono TEXT (never ORDER BY);
-- connector attribution is NOT stored (derived from the board's
-- binding at read — one source of truth); no triggers — the
-- backfill is plain SQL inside the file's transaction.

-- 1. Binding connector tag: NULL = the legacy default connector
--    (resolved at decode time; see below). Pre-migration rows keep
--    their behavior bit-for-bit: the one binding belonged to the
--    one configured connector, which is the default.
ALTER TABLE boards ADD COLUMN connector TEXT NULL;

-- 2. Status rebuild: one row per bound board. A v0.1.x database has
--    at most one bound board (the single-board invariant), so the
--    singleton's phase/last_success backfills that board's row; a
--    board-less singleton (standalone installs) is dropped — its
--    content is vacuous (the hydrated default is Idle; a stale
--    persisted Failed for a connector that no longer exists is not
--    behavior worth preserving — noted deviation from 0005's
--    bit-for-bit bar, which governs merge behavior, not status
--    cosmetics).
CREATE TABLE sync_status_new (
    board       TEXT PRIMARY KEY NOT NULL REFERENCES boards (id),
    phase       TEXT NOT NULL,
    last_error  TEXT NULL,
    last_success TEXT NULL
);
INSERT INTO sync_status_new (board, phase, last_error, last_success)
SELECT b.id, s.phase, s.last_error, s.last_success
  FROM boards b
  JOIN sync_status s ON s.id = 1
 WHERE b.remote_board_id IS NOT NULL AND b.deleted = 0;
DROP TABLE sync_status;
ALTER TABLE sync_status_new RENAME TO sync_status;
```

- *Column/key style*: `board` TEXT PK (UUID text — 0001's rule),
  `phase/last_error/last_success` copied verbatim (names unchanged —
  the row codec's diff is the key only). No seed: rows exist per
  bound board; a fresh install has none (the map hydrates empty —
  today's `SyncStatus::default()` world for standalone).
- *The NULL connector's resolution home*: `spawn_storage_actor`
  gains a `default_connector: ConnectorId` parameter (app-owned,
  config-derived at bootstrap); the row codec decodes `connector
  IS NULL` as that id and **writes non-NULL on every new upsert**
  (legacy NULLs are read-only relics that vanish with their boards'
  first rebind). Single resolution home, no bootstrap rewrite pass —
  the alternative (a boot-time UPDATE claiming sentinel rows) is
  documented and rejected: it spreads the rule across two layers and
  needs its own crash-window story.
- *Decode-rejection reuse (0005's second precedent)*: the old
  `CHECK (id = 1)` table is dropped, not re-tagged — there is no
  op-kind-style shared key to alias, so the precedent applies as
  "the old representation cannot survive the migration", pinned by a
  round-trip test mirroring connect.rs:223–297's shape (build a
  pre-0006 db → migrate → the bound board's row decodes; the
  singleton is gone).
- *Per-Q2-option pricing (MD-6/coherence item 6)*: if the user rules
  D-W9 as (b) per-connector, the table becomes
  `sync_status(connector TEXT PRIMARY KEY, phase, last_error,
  last_success)` and the backfill maps the singleton to the default
  connector's row (`INSERT … SELECT '…default…', phase, …`) — same
  file skeleton, same conventions, one INSERT simpler (no boards
  join). Option (c) pair-keying prices as the two-column PK with the
  boards join — dismissed with Q2's redundancy argument but priced.
- *The additive v0.1.x upgrade test (exit criterion)*: build a
  v0.1.x-shaped db (one bound board, a queued op, a non-idle status)
  + a v0.1.x config; boot the new build against both; assert: the
  config synthesizes the `nextcloud` connector (Q6), the binding
  decodes tagged-default, the board's status row hydrates, a cycle
  runs and merges per the unchanged policy (0005's epoch-default
  clocks keep the merge bit-for-bit), the queued op drains. Tier-2
  against a dockerized Nextcloud per the framing's exit criteria.
- *Rollback/abort story*: single-transaction file (sqlx migrate) —
  a failed 0006 leaves the pre-migration db untouched; no
  forward-only trap is introduced. (House stance: no down-migrations
  exist anywhere; 0006 does not add one.)

**Config additive path**: Q6 — zero file migration; the synthesis
rule *is* the compatibility layer.

**Snapshot regeneration strategy (framing §8: deliberate, never
bitrot).** The `AppState` widening (Q2's map) lands in wave chunk 1 —
the standing P1-1 decision — and its `insta` regeneration is one
deliberate `cargo insta accept` pass *inside that chunk*, with the
diff reviewed as "the map shape, nothing else". The command-vocabulary
changes (Q1) land in chunk 2 and rewrite the command-test corpus in
that chunk's own single pass. Rule recorded for the implementation
plan: a chunk that widens a snapshotted shape owns its regeneration;
no chunk ships with `.pending` snapshots or skips reviewing the
accepted diff.

**Breaking-change candidates (D-W6's only door: concrete benefit).**
Examined and all declined: the JSON `status` answer keeps its `sync`
field (aggregate) and grows arrays (additive — Q2); `SyncCommand`
reshaping is in-process only (Q3 — verified no persistence/wire
reference); `CommandError::BoardExists`/`NoBoard` removals are
in-process vocabularies mirrored at the app edge with typed
replacements (Q1); the `CHECK(id=1)` table's replacement is the
migration itself, priced above. **Conclusion: the wave needs zero
D-W6 breaking steps; the one non-additive schema change (the rebuild)
is the pre-sanctioned standing decision.**

**Topology note.** Storage-layer migration + codec diffs; no channel
changes anywhere in Q9. **Topology untouched.**

## 4. One-design coherence map (MD-5's pass, recorded)

**The four vocabularies × the recommended shapes (coherence item 1).**

| Vocabulary | Shape after the wave | Landing chunk (framing §6, advisory) |
|---|---|---|
| Commands (`StateCommand`) | `board: BoardId` on all nine board-scoped arms; `CreateBoard` refusal lifted; new `UnbindBoard`, `ReconcileByTitle` | 2 (vocabulary), 3 (ceremonies' commands) |
| Outbox ops (`LocalOp`) | **unchanged** — already board-addressed / entity-scoped; push filters by connector-owned boards | 4 (filter only) |
| `CommandError` | −`NoBoard` −`BoardExists`; +`UnknownBoard(BoardId)` +`BoardDeleted(BoardId)`; `BoardAlreadyBound { board, bound }`; +`RemoteBoardInUse { remote, board }` | 2 |
| Status payloads | `AppState.sync` → per-board map (Q2a); `SyncCommand` → `{SyncNow, Detach}` (SetBoard retired); `SyncReport` unchanged (already per-board snapshots) | 1 (status), 2 (SyncCommand) |

**Selection/identity story (items 2–3).** One selection home per
frontend, none in the engine: CLI flag default (resolved at
invocation, typed ambiguity refusal), REPL `ReplSession.current_board`,
UI `ViewState.board` (Q8's switcher is its only mutator — one
mechanism, mirroring the stack switcher). One connector identity
end-to-end: config `name` → `ConnectorId` (domain newtype, Q3) →
binding tag (entities) → registry key (`SyncActor.id`) → module id
(`ConnectorModule::id`, Q4) → CLI `--connector` values. No second
name minted anywhere.

**Status flow (item 4).** Actor cycle (per board, connector-owned) →
`SyncReport` (fan-in inbox, unchanged) → engine ingest re-keys the
per-board map (Q2) → surfaces: UI badge = selected board's row;
CLI `status` = per-connector grouping + aggregate `sync` field;
global `Offline` phase = the authority's counted events (Q7), never
the map's invention. "Per-board map with derived attribution" is the
one phrasing used in every section.

**Ceremonies (item 5).** `destruct_on` generalises over Q1's board
parameter; its typed-refusal vocabulary is Q1's error list; unbind's
receipt counts reuse `read_scope`; reconcile runs post-adoption
through the clock merge; the bind ceremony's nudge is the broadcast
`SyncNow` through the engine (`RequestSync`), with no targeted send
left anywhere (Q3). Loud flags throughout
(`--confirm` / `--switch-backend` / `--force` precedents).

**Migration pricing (item 6).** Q9's 0006 sketch prices all three
D-W9 options' table shapes; the user's ruling selects one INSERT.

**Invariant audit (item 7).** Every section's conformance note reads
"additive on the seams / engine-internal / app-layer only"; the only
*removed* message (`SetBoard`) never traversed the engine fan-out;
the only new events are broadcast variants; the only new task (the
authority) subscribes and emits on existing channels. Nothing in
§3.1–3.9 requires touching connect-flow §4.2's topology.

**D-W conformance (item 8).** D-W1 (scope: all nine answers stay
inside D2 stages 1–2; reconcile is the one pulled issue's proposed
approach, no fuzzy extras); D-W2 (#179 → `UnbindBoard` ceremony,
#170 → `ReconcileByTitle`); D-W3 (two connectors live: Q3/Q4/Q6 make
the exit criterion constructible); D-W4 (trait promoted, ADR draft
§6); D-W5 (switcher + ViewState board in-wave); D-W6 (zero breaking
steps, §3.9); D-W7 (counted authority, §3.7); D-W8 (this round is
docs-only); D-W9 **posed** (§3.2/§5 — open for the user).

## 5. Decision record (PROPOSED — the user's session rules)

| Q | Subject | Recommendation | Status |
|---|---|---|---|
| Q1 | Board parameter shape | Option 3: explicit `board: BoardId` on all board-scoped commands; caller-owned selection (CLI flag default / REPL session / `ViewState.board`) | PROPOSED |
| Q2 | SyncStatus keying (D-W9) | Option (a): per-board map, attribution derived from bindings; global `last_updated` unchanged | **OPEN — user rules** |
| Q3 | Connector addressing home | Option (b): app-layer named routing over tagged bindings; domain `ConnectorId` from config names; `SetBoard` retired | PROPOSED |
| Q4 | Trait promotion shape | Split ports: domain vocabulary + app `ConnectorModule` (`dyn`); no new crate | PROPOSED |
| Q5 | Ceremony redesign | Typed multi-binding variants; `UnbindBoard` (keep content, strand ops honestly, bind-wipe rule); `ReconcileByTitle` engine command; `destruct_on` parameterised | PROPOSED |
| Q6 | Config shape | `[[sync.connectors]]` named entries; legacy scalar + `[nextcloud]` synthesize the default connector; mixes are typed errors; shape-aware ceremony writer | PROPOSED |
| Q7 | Network authority mechanics | Option (a): per-connector `ConnectorLost/Restored(id)` events + app-layer all-down authority emitting the existing global events | PROPOSED |
| Q8 | UI board dimension | `ViewState.board` + `board.next/previous/select` actions mirroring the stack switcher; IdTables unchanged; compact hides the strip | PROPOSED |
| Q9 | Migration & compat | 0006 rebuild per §3.9; config additive; one deliberate insta pass per widening chunk; zero breaking steps | PROPOSED |

## 6. Draft ADR 0010 amendment (MD-9 — *draft; lands with the
trait-promotion implementation chunk, not this PR*)

Full replacement text for ADR 0010's decision section "** The
connector port stays vocabulary, not a trait" (0010 .org line 140)
and the two consequences lines that restate the stance ("a second
connector, board reset/delete, and title-match reconciliation are
backlog" — the reset/delete and title-match halves of that sentence
are already stale post-#334/D-W2; this amendment rewrites the
sentence wholesale):

```org
** The connector port: shared vocabulary + an app-layer module trait

=SyncCommand= / =SyncReport= / =SyncStateReader= remain the connector
contract (ADR 0007) and stay in =taskboard-domain= — the data half of
the port, where the multi-board wave added =ConnectorId= and the
connector-tagged board binding. The spawn half is =taskboard-app='s
=ConnectorModule= trait: one implementation per connector (today:
nextcloud), =dyn=-dispatched in bootstrap's registry loop over the
configured =[[sync.connectors]]= entries. A second connector
implements the module trait against the shared vocabulary — no
discovery, no dynamic loading, no plugin registry; the app still
selects what to construct, now by config name instead of a backend
word. Decided in the multi-board design round (D-W4); recorded here
because this ADR previously kept the vocabulary traitless pending a
second concrete connector — the wave needs the seam (two connectors
live simultaneously) and the vocabulary had already stopped moving.
```

Consequences-lines replacement (the stale backlog sentence):

```org
- Board reset/delete landed with PR #334's verbs; title-match
  reconciliation (#170) lands with the multi-board wave; =logout='s
  interaction with =backend= is WI-3's question.
```

Adjacent ADR 0010 drift items **not** amended by this round (MD-9):
F10-03/F10-04/F15-18 — backlog candidates (§7).

## 7. Backlog candidates (out-of-scope ideas; the user promotes)

- Per-connector `poll_interval`/backoff tuning (Q6 keeps them global).
- Hysteresis/debounce beyond the episode-edge rule for flapping
  connectors (Q7).
- Config-pinned fixed-board kiosk surface (Q8 — kiosk switching stays
  keyboard-available in-wave).
- UI board creation/archive actions (structure-editing growth beyond
  the switcher; `stack.create` is the precedent).
- Fuzzy/normalized-key growth for title reconciliation (Q5 ships
  trim+casefold only).
- `boards reconcile --dry-run` (the receipt already reports counts;
  a dry-run flag is ceremony sugar).
- Daemon-side per-connector attach/detach verbs over IPC (ADR 0014's
  completion note anticipates the pattern; N-naming rides a later
  round).
- Connector configuration via env (figment cannot express the array —
  probe case 7; revisit only with a concrete scripting need).
- ADR 0010 drift amendments F10-03/F10-04/F15-18 (MD-9's adjacents).
- #157 per-card refresh perf work (guardrail recorded in §3.3; the
  issue stays unscheduled).

## 8. References

- Framing plan (normative except §6):
  `.artifacts/plans/2026-10-18-multi-board-wave-design-plan.md`.
- Dispatch plan §1/§2:
  `.artifacts/plans/2026-10-18-multi-board-design-round-dispatch-plan.md`.
- Stage-1 work plan (MD-1…MD-10):
  `.artifacts/plans/2026-10-18-multi-board-design-round-workplan-plan.md`.
- Connect-flow design plan §4.2/§4.5 (the topology invariant):
  `.artifacts/plans/2026-10-12-connect-flow-design-plan.md`; ADR 0014
  (`docs/adr/0014-connector-attach-session-login.org`); ADR 0010
  (`docs/adr/0010-standalone-first-optional-sync-connector.org`).
- Audit findings F16-05/06/07/08/22/23/24/25, F3-04, F7-05, F8-06:
  `.artifacts/reports/2026-10-17-v0.1.0-audit-findings-report.md`.
- GUI requirements R1/R4/R9:
  `.artifacts/plans/2026-10-14-gui-design-requirements-plan.md`.
- Issues #179, #170 (read-only `gh issue view`).
- Code anchors @ stage-2 base `1004120` (merge-base `62dd839`; re-verified
  post-rebase at merge-base `361ab0b`, docs-only delta): the full table in §2.
- Figment probe: figment `0.10.19`, results in §3.6 (probe chain =
  the app's load chain; run in a throwaway crate outside the repo).

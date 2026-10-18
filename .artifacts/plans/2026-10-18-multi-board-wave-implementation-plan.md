# [Plan] Multi-board Wave — Implementation (N boards × N connectors)

- **Date**: 2026-10-18
- **Kind**: implementation plan for the **accepted** multi-board
  design (origin:
  `.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`,
  decision record §5 — ACCEPTED 2026-10-18, all nine recommendations,
  no deviations). Chunked for one branch + PR per chunk; chunks are
  written so they can be implemented directly by an agent or promoted
  into a dispatch plan without restructuring. **Design** below means
  that document; each chunk names its normative origin sections there.
- **Status**: READY — gated. Implementation dispatch is its **own**
  dispatch round (template `.artifacts/templates/dispatch-plan.md`),
  written after this plan's PR merges **and** remediation-wave-1
  closes (design-round dispatch plan §4). This plan does not dispatch
  itself.
- **Decisions baked in** (design §5, all ACCEPTED 2026-10-18):
  Q1 explicit `board: BoardId` on all board-scoped commands,
  caller-owned selection (§3.1); Q2/D-W9 **ruled (a)** per-board
  status keying, attribution derived from bindings, global
  `last_updated` unchanged (§3.2); Q3 app-layer named routing, domain
  `ConnectorId` from config names, `SetBoard` retired (§3.3); Q4
  split ports — domain vocabulary + app `ConnectorModule` (§3.4); Q5
  typed multi-binding variants, `UnbindBoard`, `ReconcileByTitle`,
  parameterised `destruct_on` (§3.5); Q6 `[[sync.connectors]]` with
  legacy-scalar synthesis and loud mix rejection (§3.6); Q7
  per-connector events + app-layer all-down authority (§3.7); Q8
  `ViewState.board` + `board.next/previous/select` (§3.8); Q9
  migration 0006 as sketched, zero breaking steps (§3.9).

## 1. Scope

**In** (framing §2, user-confirmed 2026-10-18): multi-board engine
semantics (board parameter, single-live-board refusal lifted,
ceremonies at N); connector registry + per-board routing; **fully
implemented multi-connector** (two connectors live simultaneously is
an exit criterion); connector trait promotion + ADR 0010 amendment;
UI board dimension incl. the desktop board switcher; config
`[[sync.connectors]]` + CLI/REPL verb growth; the F16-22 SyncStatus
three-layer widening — **first chunk, alone** (standing decision
P1-1, 2026-10-17).

**Out** (later waves / backlog — restated from design §7 for
visibility, **not promoted**: the user owns tracker edits per AGENTS
§7): per-connector `poll_interval`/backoff tuning; hysteresis beyond
the episode-edge rule; config-pinned fixed-board kiosk; UI board
creation/archive actions; fuzzy/normalized-key reconciliation growth
(`boards reconcile --dry-run` included); daemon-side per-connector
attach/detach verbs over IPC; connector configuration via env
(figment cannot express the array); ADR 0010 drift amendments
F10-03/F10-04/F15-18; #157 per-card refresh perf work (guardrail in
DF-5); Projects hierarchy, multi-user decks, org-file connector, the
D5 app-data substrate *implementation* (its one shared decision
point, F16-08, is decided — D-W4), LAN sync server, CRDT
collaboration (framing §2's later waves).

## 2. Design facts every chunk relies on

Each fact cites its design section; per-chunk sections reference
"DF-N (← design §3.x)". File paths + symbols are the stable keys —
line numbers appearing anywhere are plan-day references (design §2's
re-verified table); every implementing agent re-locates symbols at
its own start (finding IDs and symbols are the stable keys, lines
are not).

1. **DF-1 — the four vocabularies after the wave** (← design
   §3.1/§4). *Commands* (`StateCommand`, domain `command.rs` +
   `messages.rs` Execute wire): the nine board-scoped arms gain
   `board: BoardId` — `CreateStack`, `CreateLabel`, `RenameBoard`,
   `SetBoardColor`, `SetBoardArchived`, `DeleteBoard`, `ResetBoard`,
   `BindBoard`; `CreateBoard` is unchanged and its single-live-board
   refusal is **lifted** (always mints). Each `live_board(app)
   .ok_or(NoBoard)?` becomes `live_board_of(app, board)?` (lookup +
   liveness). Task/stack/label-scoped commands stay id-addressed.
   New arms land with their chunks: `UnbindBoard { board }`,
   `ReconcileByTitle { board }` (chunk 3). *Outbox ops*: **zero
   diff** — `LocalOp::UpdateBoard(BoardId)`/`DeleteBoard` are
   already board-addressed; stack/label/task ops resolve the owning
   board through the entity graph (`EntityTables` + `RemoteIndex`).
   *`CommandError`* (domain `command.rs`, mirrored in app
   `commands/mod.rs`): −`NoBoard` −`BoardExists`;
   +`UnknownBoard(BoardId)` +`BoardDeleted(BoardId)`; widen
   `BoardAlreadyBound(RemoteBoardId)` → `BoardAlreadyBound { board,
   bound }`; +`RemoteBoardInUse { remote, board }` (chunk 3).
   *Status payloads*: `AppState.sync` → per-board map (DF-9);
   `SyncCommand` → `{SyncNow, Detach}` (`SetBoard` retired, chunk
   4); `SyncReport` unchanged (already per-board snapshots).
2. **DF-2 — selection is caller-owned, never engine state** (←
   design §3.1 caller-side burden, §3.8). Three homes, one per
   frontend: CLI `--board <uuid>` flag with a single-live-board
   default resolved at invocation and a **typed ambiguity refusal**
   when zero or ≥2 live boards make the default ambiguous; REPL
   `ReplSession.current_board: Option<BoardId>` set by
   `boards use` (`crates/taskboard-app/src/repl.rs`,
   `ReplSession`); UI `ViewState.board` (chunk 7). Nothing of
   selection enters the domain crate.
3. **DF-3 — the `ConnectorId` mint chain, one name end-to-end** (←
   design §4 selection/identity story, §3.3). config `name` (the
   `[[sync.connectors]]` entry key; legacy scalar synthesizes
   `"nextcloud"`) → domain newtype `ConnectorId` (`ids.rs`, wraps
   `String` — a *name*, not a UUID) → binding tag (`Board.remote`
   widens to a connector-tagged binding, chunk 4) → registry key
   (`SyncActor.id`) → module id (`ConnectorModule::id`) → CLI
   `--connector` values. No second name minted anywhere; kebab-case
   + uniqueness validated at config load (typed `ConfigError`).
4. **DF-4 — the binding-tag decode rule** (← design §3.9). Storage
   `boards.connector TEXT NULL` (migration 0006, chunk 1): **NULL
   decodes as the default connector** — the resolution home is
   `spawn_storage_actor(default_connector: ConnectorId)`
   (storage `actor.rs`, app-derived at bootstrap); **non-NULL is
   written on every upsert** (chunk 1's codec writes the default id —
   the only connector that exists there; chunk 4's domain field
   becomes the source). Legacy NULLs are read-only relics that vanish
   with their boards' first rebind; no boot-time UPDATE pass
   (rejected in the design: spreads the rule across layers).
5. **DF-5 — ceremony rules** (← design §3.5, §3.3 guardrail).
   `BindBoard { board, remote }` refuses typed:
   `UnknownBoard`/`BoardDeleted`/`BoardAlreadyBound { board, bound }`
   (same-remote rebind = accepted no-op)/**new**
   `RemoteBoardInUse { remote, board }`. **Bind-wipe rule**:
   `BindBoard` on a board whose descendants carry refs from a
   *different* remote wipes those refs (fresh binding ⇒ descendants
   local-only; creates re-create on the new remote), wipe counted in
   the bind receipt. **Honest stranding**: ops for an unbound board
   wait (`pending_ops` climbs honestly); no silent cancellation;
   update-class ops on wiped refs dead-letter and are counted in the
   pre-count receipt. **Reconcile**: trim + casefold title match
   (deterministic; no fuzzy matching in-wave), tasks match only
   within matched stacks, merge through the per-field clock merge,
   duplicate create-ops cancelled as counted `FailOp`s, idempotent,
   typed outcome with counts; **no match ⇒ both stay**. `destruct_on`
   keeps its typed `ConfirmationRequired` refusal shape, gains the
   board parameter (`read_scope(board)`). Loud flags throughout
   (`--confirm` pattern). **#157 guardrail**: per-connector cycle
   cost is O(boards + changed cards) — per-board validators serve
   `304` for unchanged listings; the perf issue itself stays
   unscheduled.
6. **DF-6 — config resolution + the typed-rejection list + the
   shape-aware writer** (← design §3.6, figment probe facts).
   `[[sync.connectors]]` array-of-tables; entry struct
   `#[serde(deny_unknown_fields)]`; first entry = the default
   connector. Array absent ⇒ legacy world verbatim: `sync_backend()`
   auto rule stands; resolved `nextcloud` + `[nextcloud]` synthesize
   one connector named `nextcloud`; resolved `standalone` ⇒ zero
   connectors. Typed rejections (all loud, naming the offending
   keys): scalar + array both present; `[nextcloud]` table + array
   both present; `backend = "standalone"` + array; duplicate or
   non-kebab `name`; `backend = "nextcloud"` resolved with an empty
   array. `poll`/`backoff` knobs stay global. Env override works for
   the scalar only (figment cannot express the array — probe case
   7); credential store already keyed per-(server, username).
   Ceremony writer (`connect.rs` persist path) is **shape-aware**:
   no array in file ⇒ today's scalar write verbatim; array present ⇒
   rewrite the default (first) entry's `server_url`/`username` and
   never touch `backend`. Round-trip holds in both worlds.
7. **DF-7 — network authority mechanics** (← design §3.7). Actors
   emit **new** `SystemEvent::ConnectorLost(ConnectorId)` /
   `ConnectorRestored(ConnectorId)` (episode-edge semantics verbatim:
   exactly-once per offline episode; `fail_cycle` on
   transport-classified failures; report-then-restore order); the
   global names `NetworkLost`/`NetworkRestored` are **retired from
   the actors** and become the authority's vocabulary. The
   **authority** is an app-layer bootstrap task: holds the
   per-connector up/down set (seeded from configured modules;
   updated by attach/detach registry hooks — app-internal lifecycle
   bookkeeping, not inter-actor traffic), subscribes the system
   broadcast, emits `NetworkLost` when the last up connector flips
   down / `NetworkRestored` when the first flips up. Engine arms:
   `ConnectorLost(c)` → `go_offline` scoped to *c's boards* (the
   engine derives the set from its own bindings); `ConnectorRestored
   (c)` → forward the nudge. Single-connector installs are
   byte-for-byte today's semantics. No hysteresis in-wave.
8. **DF-8 — UI integration points** (← design §3.8, §3.2 badge).
   `ViewState.board: Option<BoardId>` (None = fresh install);
   **seed rule**: seeded by `presentation::live_board` when None
   (deterministic first-live-non-archived; survives as the seed
   only). `ViewState::select_board(&mut self, state, board) ->
   Vec<FlowOutcome>` mirrors `select_stack` (sets board, resets
   `selected_stack` to the new board's first stack, `focused_task`
   to that stack's first task, `RefreshNow`). Three `ActionId`s —
   `BoardNext` (`board.next`), `BoardPrevious` (`board.previous`),
   `BoardSelect` (`board.select`) — contexts `Board`/`StackDetail`/
   `TaskDetail` + `Global`, category navigation; keymap entries in
   the `Board` + `ViNormal` contexts (preset table is the tunable).
   **Dumb-client rule**: switching performs no engine command —
   `FlowOutcome::Command` never produced. `IdTables`: **no
   re-keying** — the `push` builder derives its board argument from
   `ViewState.board` (tables are per-refresh of the viewed board by
   construction). `board-bar.slint` composition (L2 sibling of
   toolbar/status-bar): one segment per live board; **compact hides
   the strip** (keyboard switching still works). Badge: selected
   board's status; re-read in the same refresh pass.
9. **DF-9 — migration 0006 + the snapshot rule** (← design §3.9).
   One file, single transaction:
   `0006_multi_board_sync_status.sql` — (a) `boards.connector TEXT
   NULL` added; (b) `sync_status` rebuilt as `sync_status_new(board
   TEXT PRIMARY KEY → boards.id, phase, last_error, last_success)`,
   backfilling the legacy singleton (row id = 1) into the ≤1 bound
   live board a v0.1.x database can have, then drop + rename; no
   seed; a board-less singleton (standalone installs) is dropped
   (vacuous content — the design's recorded, priced deviation from
   0005's bit-for-bit bar, which governs merge behavior, not status
   cosmetics). Conventions: chrono TEXT timestamps (never ORDER BY),
   no triggers, plain SQL in-transaction, no down-migration. SQL
   sketch: design §3.9 verbatim. Domain shape: `AppState.sync`
   becomes a newtype map `SyncStates(BTreeMap<BoardId, SyncStatus>)`
   with `get(board)` + an `aggregate()` fold for back-compat
   surfaces (daemon `status` keeps an aggregate-shaped `sync` field;
   UI/CLI surfaces migrate per their chunks). **Snapshot rule**: a
   chunk that widens a snapshotted shape owns its **one deliberate**
   `insta` regeneration pass inside the chunk, the accepted diff
   reviewed as "the shape change, nothing else" — chunk 1 owns the
   `AppState`-widening pass (the `persisted_state`/
   `published_app_state` pairs), chunk 2 owns the command-corpus
   pass. No chunk ships `.pending` snapshots.
10. **DF-10 — the topology invariant (standing acceptance test)** (←
    design §1, connect-flow §4.5, ADR 0014 decision 3). The engine
    and `App` are connector-count agnostic; "at most one active
    connector" is an app/config-layer invariant only; every change
    lands additively on the §4.2 fan-out/fan-in seams. The only
    removed message (`SetBoard`) never traversed the engine fan-out;
    the only new events are broadcast variants; the only new task
    (the authority) subscribes and emits on existing channels. A
    chunk needing a topology change has misdesigned something else.

## 3. Chunks

Sizes on the house scale. Re-chunking record vs the framing §6
sketch (the sketch was advisory; the design's own cross-references
endorse this spine — §3.9 "wave chunk 1", §3.4 "chunk 4 … implements
the trait", §3.1/§3.8 "chunk 6"/"chunk 7"): the sketch's chunk 2
(vocabulary + trait port) splits into chunks 2 and 4 here —
green-`main` sequencing lands the trait with the connector identity
it needs (design §4's landing column is advisory by its own header);
`SetBoard`'s retirement moves from "with the ceremonies" to chunk 4,
the same motion that tags the bindings (design §3.3); the sketch's
chunk-1 scope grows to carry the whole of 0006 (binding-tag column +
`ConnectorId` + decode home — design §3.9's one-file sketch is the
contract). Everything else maps 1:1 (sketch 3→3, 5→5, 6→6, 7→7,
8→8).

### Chunk 1 — SyncStatus per-board map + migration 0006, whole (M; branch `feat/state-sync-status-map`; closes #298, annotates #125)

- **Origin**: design §3.2 option (a) — ruled by the user (D-W9);
  §3.9 (0006 sketch, decode home, rollback story); §3.3
  (`ConnectorId` mint).
- **Files** (paths + symbols): new
  `crates/taskboard-storage-sqlite/migrations/0006_multi_board_sync_status.sql`;
  storage `repo.rs` (status load + upsert loop re-keyed by board),
  `codec.rs` (boards row codec: `connector` column per DF-4),
  `actor.rs` (`spawn_storage_actor` gains `default_connector`);
  domain `state.rs` (`AppState.sync` → `SyncStates(BTreeMap<BoardId,
  SyncStatus>>` + `get`/`aggregate`; global `last_updated`
  **unchanged**), `ids.rs` (`ConnectorId` newtype), `persistence.rs`
  (`PersistedState.sync` shape); state `core.rs` (`sync_status()`
  getter, `mark_syncing(board)`), `actor.rs` (`ingest_sync_report` /
  `append_status_if_changed` re-keyed by the report's board;
  `go_offline` marks every *bound* board's row — chunk 1's world has
  one connector, so this is today's semantics bit-for-bit with ≤1
  board); app surfaces reading the aggregate fold:
  `commands/status.rs` (`StatusView.sync`), `commands/output.rs`
  (`SyncStatusView`), `commands/sync.rs` (wait decision table),
  `bootstrap.rs` (`wait_for_boot_pull`; passes `default_connector`
  into the storage spawn); ui `presentation.rs` (`SyncBadge` reads
  the fold at this chunk; the selected-board read is chunk 7).
- **Behaviours**: migration lands whole (status rebuild **and** the
  `boards.connector` column — DF-9); codec decode NULL = default
  connector, upserts write non-NULL (DF-4); existing surfaces keep
  their shapes via the `aggregate()` fold; **snapshot pass 1** — the
  one deliberate `insta` regeneration for the `AppState` widening
  (20-file corpus; DF-9); migration round-trip test mirroring
  `connect.rs:223–297`'s shape (build a pre-0006 db → migrate → the
  bound board's row decodes; the singleton is gone).
- **Non-goals**: no plural connectors (chunk 5); no domain
  binding-type widening — `Board.remote` stays `Option<
  RemoteBoardId>` until chunk 4 (the codec writes the default id
  from the spawn parameter, not a domain field); no `StateCommand`
  shape changes (chunk 2); no `SyncCommand` changes (chunk 4).

### Chunk 2 — Domain vocabulary motion: board-scoped commands (L; branch `feat/domain-board-scoped-commands`; annotates #125)

- **Origin**: design §3.1 (all four vocabularies, caller-side
  burden, the nine-arm table); §3.9 (snapshot rule).
- **Files**: domain `command.rs` (nine arms per DF-1;
  `live_board()` → `live_board_of(app, board)`; error set per
  DF-1: −`NoBoard` −`BoardExists`, +`UnknownBoard` +
  `BoardDeleted`, `BoardAlreadyBound` widened), `messages.rs`
  (Execute wire mirrors); app `commands/mod.rs` (mirror widening),
  `cli.rs` + `commands/boards.rs`/`stacks.rs`/`labels.rs`
  (`--board <uuid>` flag on board-scoped verbs + single-live default
  with the typed ambiguity refusal, DF-2); mechanical **interim
  fills** at the remaining call sites — `repl.rs` dispatch sites and
  the UI's command-construction sites resolve the first live board
  inline, each marked interim (chunk 6 / chunk 7 replace them).
- **Behaviours**: `CreateBoard`'s single-live-board refusal lifted
  (always mints; titles are not uniqueness keys); **snapshot pass
  2** — the command-corpus regeneration (DF-9); the CLI default is
  the honest replacement of today's implicit pick (flag omitted +
  exactly one live board ⇒ that board; zero or ≥2 ⇒ typed refusal).
- **Interim state (stated loudly, DF-10's honest-visibility
  cousin)**: after this chunk, multiple boards and bindings are
  **engine-possible** while the sync actor still syncs its first
  bound board only — until chunk 4's derived board set. The interim
  is visible, never silent: per-board `pending_ops` climbs on the
  unsynced boards (chunk 1's map). Multi-board is **not
  user-advertised** until chunk 5 (config) lands; the implementation
  round must not "fix" the interim out of order.
- **Non-goals**: no ceremony redesign (chunk 3 — `select_on`'s
  app-side `BoardAlreadyBound` pre-check keeps compiling against the
  widened variant); no real selection homes (interim fills only);
  `SetBoard` kept (retires in chunk 4).

### Chunk 3 — Engine ceremonies at N (M; branch `feat/state-board-ceremonies`; closes #170 and #179, annotates #125)

- **Origin**: design §3.5 (all five sub-questions); §3.1's
  sub-decision (the ceremony error list).
- **Files**: domain `command.rs`/`messages.rs` (new
  `UnbindBoard { board }`, `ReconcileByTitle { board }`; new
  `CommandError::RemoteBoardInUse { remote, board }`); state
  `actor.rs`/`core.rs` (handlers: bind refusals per DF-5, bind-wipe
  rule with counted receipt, unbind clears the binding and strands
  ops honestly, reconcile semantics per DF-5, same-remote rebind =
  accepted no-op → re-adopt); app `commands/boards.rs`
  (`destruct_on`/`read_scope` parameterised by board — typed
  refusal states that board's counts, receipt names the board id;
  new `boards unbind <board>` and `boards reconcile <board>`
  ceremony verbs in the loud-flag shape; `select_on`'s single-board
  pre-check folds into pass-through of the engine's typed refusals;
  the bind ceremony stops *sending* `SetBoard` — the nudge is the
  broadcast `SyncNow` through `RequestSync`; `wait_for_adoption`
  parameterises by board, watching that board's status row);
  `commands/mod.rs` (app mirrors).
- **Behaviours**: `UnbindBoard` keeps all content local (remote-
  bound content is already local by the merge policy); descendant
  refs self-identify their remote board (`RemoteStackRef.board`,
  `RemoteCardRef.board`), so stale refs are never confused across
  remotes, and same-remote rebind re-adopts by refs;
  `ReconcileByTitle` runs automatically after the bind ceremony's
  adoption-wait and is manually re-runnable, idempotent; the
  `DeleteBoard` tombstone-finalizing push drains through the board's
  owning connector (exactly one connector finalizes it).
- **Non-goals**: no actor/routing changes (chunk 4); no config
  shape (chunk 5); `SetBoard` is no longer *sent* here but the
  variant + handler + `App::sync_commands()` remain (compile-clean
  pub surface; removed in chunk 4's motion).

### Chunk 4 — Connector identity end-to-end on the wire (M; branch `feat/sync-connector-modules`; annotates #187 and #125)

- **Origin**: design §3.4 (trait + ADR amendment); §3.3 (derived
  board sets, `SetBoard` retirement, #157 guardrail); §3.6
  (legacy-scalar synthesis-of-one — the subset needed to mint the
  id); §3.9 (the v0.1.x additive-upgrade boot test).
- **Files**: new `crates/taskboard-app/src/connectors.rs`
  (`ConnectorModule` trait per design §3.4's sketch — `id()`,
  `build_and_spawn(...)` over `Arc<dyn SyncStateReader>`, the
  report/system channels, `SyncActorConfig`; `NextcloudModule`
  adapter wrapping `spawn_sync_actor`; `dyn` dispatch in a
  bootstrap registry loop, N=1 at this chunk); domain `entities.rs`
  (the binding-type widening: `Board.remote` becomes a
  connector-tagged binding carrying `RemoteBoardId` +
  `ConnectorId`), `ids.rs`/`messages.rs` as needed by the widening;
  sync `actor.rs` (`spawn_sync_actor` gains the connector id; the
  resume/absorber path derives the actor's board set from tagged
  bindings each cycle — the generalisation of today's first-bound-
  board pick; `LoopState.target` becomes the derived per-board
  target set with per-board cache keys, **no
  cache-clear-on-retarget**; the stale-binding re-resolve (#258)
  composes unchanged — board-agnostic per-entity), `push_exec/`
  (push ownership filter: boards owned by *this* connector); state
  `actor.rs` + domain `messages.rs` (`SetBoard` retired — variant,
  engine handler, and `App::sync_commands()` removed in the same
  motion that tags the bindings); app `bootstrap.rs` (registry loop,
  legacy-scalar synthesis-of-one: resolved `nextcloud` + `[nextcloud]`
  ⇒ one connector named `nextcloud`; resolved `standalone` ⇒ none);
  storage `codec.rs` (upserts source the tag from the domain field
  per DF-4). **ADR 0010 amendment text lands here** — design §6's
  draft verbatim into
  `docs/adr/0010-standalone-first-optional-sync-connector.org`.
- **Behaviours**: the newly bound board is picked up by the owning
  connector's next read with no message at all (the broadcast
  `SyncNow` satisfies the ceremony's "cycle soon"; unchanged boards
  are cheap via per-board validators — DF-5's guardrail); **the
  v0.1.x additive-upgrade boot test**: build a v0.1.x-shaped db (one
  bound board, a queued op, a non-idle status) + v0.1.x config; boot
  the new build; assert the synthesized connector, tagged-default
  binding decode, status-row hydration, a cycle merging per the
  unchanged policy (0005's epoch-default clocks), the queued op
  draining (tier-2 dockerized per §4).
- **Non-goals**: no `[[sync.connectors]]` plural (chunk 5 — config
  decode stays the legacy scalar + synthesis-of-one); no authority
  mechanics (chunk 5's events wait); no CLI verb growth
  (chunk 6); no second connector *kind* (nextcloud only).

### Chunk 5 — App plural world: config, registry, authority (L; branch `feat/app-connector-registry`; closes #187, annotates #125)

- **Origin**: design §3.6 (full); §3.7 (full); §3.3 consequences
  (registry/naming).
- **Files**: app `config.rs` (`[[sync.connectors]]` decode, entry
  `deny_unknown_fields`, name validation, the full typed-rejection
  list per DF-6); `commands/connect.rs` (shape-aware ceremony
  writer per DF-6); `bootstrap.rs` (`ensure_vacant` →
  `ConnectorNameInUse(ConnectorId)` — same guard, named; N-actor
  spawn over the registry; `connector_commands(id)` named lookup;
  attach-verb name parameter (daemon-side attach grows the name);
  the **authority task** per DF-7); domain `messages.rs`
  (`SystemEvent::ConnectorLost(ConnectorId)` /
  `ConnectorRestored(ConnectorId)` additive variants; global
  `NetworkLost`/`NetworkRestored` retired from the actors); sync
  `actor.rs` (offline FSM emits the per-connector variants,
  episode-edge semantics verbatim); state `actor.rs` (arms:
  `ConnectorLost(c)` → `go_offline` scoped to c's boards via the
  binding tags; `ConnectorRestored(c)` → forward the nudge); app
  `commands/mod.rs` (`require_connector` pluralised — "at least one
  connector configured", same typed `SyncDisabled` refusal).
- **Behaviours**: two connectors configured and spawned is the
  normal path (the N=2 exit criterion made real); single-connector
  installs are byte-for-byte today's semantics (the authority is a
  pass-through with the same edge trigger); **tier-2
  two-connector test**: two dockerized Nextcloud instances, boards
  bound across both, per-connector health visible in status, one
  connector down ⇒ its boards' phases `Offline`, the global banner
  silent while the other responds.
- **Non-goals**: no UI (chunk 7); no new connector *kind*
  (nextcloud only — the trait's second implementation is the
  post-wave horizon, not an in-wave exit criterion); no per-board
  CLI surface growth beyond what DF-6's verbs require (chunk 6).

### Chunk 6 — CLI/REPL verb growth (M; branch `feat/app-cli-connector-verbs`; annotates #125)

- **Origin**: design §3.2 (wire consequence — the additive JSON
  answer); §3.3 (CLI consequences); §3.6 (`require_connector`
  pluralised, `--connector` flags).
- **Files**: app `commands/status.rs` + `commands/output.rs`
  (per-connector `status` rendering: the daemon JSON answer keeps
  its `sync` field — aggregate fold, byte-shape unchanged for the
  single-board v0.1.x install — and adds `connectors: […]` (name,
  derived health, boards with their statuses) + `boards: […]`
  arrays; human output renders the arrays; a connector with zero
  bound boards lists as configured-and-idle); `ipc/protocol.rs`
  (the additive answer shape); `cli.rs` + the targeting verbs
  (`--connector <name>` flags where targeting matters);
  `repl.rs` (`ReplSession.current_board` + the `boards use <uuid>`
  line replacing chunk 2's interim fill; board ids join the
  completer's tables); `completion.rs` (completions growth).
- **Behaviours**: the `--board`/`--connector` flag pair composes:
  board-scoped verbs name their board (DF-2 default rules), sync/
  status verbs may name their connector; the exit criterion "one
  board moved between connectors" is exercised here (unbind +
  rebind onto the second connector's remote, verified by per-board
  status attribution moving).
- **Non-goals**: no UI (chunk 7); no engine/domain changes beyond
  the wire shape (this chunk is app-side rendering + verb plumbing).

### Chunk 7 — UI board dimension (M; branch `feat/ui-board-switcher`; annotates #125)

- **Origin**: design §3.8 (all integration points); §3.2 (badge
  interplay).
- **Files**: ui `view_state.rs` (`ViewState.board` + seed rule +
  `select_board` per DF-8 — replaces chunk 2's UI interim fills),
  `presentation.rs` (`live_board` survives as the seeding rule
  only; the canonical board-order helper — same order as `boards
  list`; badge reads the selected board's status row), `actions.rs`
  (`BoardNext`/`BoardPrevious`/`BoardSelect` registry rows per
  DF-8), `keymap/presets.rs` (entries present and resolving in
  `Board` + `ViNormal`; preset table is the tunable), `router.rs`
  (dispatch — the exhaustive match forces every construction site),
  `models.rs` (`push` builder derives its board argument from
  `ViewState.board`; **no `IdTables` re-keying**), new
  `ui/compositions/board-bar.slint` (one segment per live board;
  compact hides the strip per DF-8), `ui/compositions/
  board-layout.slint` (strip mount), `adapter.rs` (segment clicks
  route through the action layer, mirroring the `select_stack`
  seam).
- **Behaviours**: next/previous walk the canonical board order with
  the no-wrap rule at the ends; switching performs **no engine
  command** (dumb-client rule — pure view state, exactly like stack
  switching); **the scripted keyboard-journey leg**: with two
  boards seeded, the journey performs `board.next`, asserts the
  view re-rendered the second board's stacks, `board.previous`
  back — the exit criterion's pin; kiosk inherits the compact
  degeneration (no strip; keyboard switching remains
  operator-available).
- **Non-goals**: no engine commands from switching (DF-8's
  dumb-client rule); no config-pinned fixed-board kiosk (backlog);
  no UI board create/archive actions (backlog — `stack.create` is
  the precedent, growth is post-wave).

### Chunk 8 — Docs + wave exit verification (S; branch `docs/multi-board-wave`; closes #125 — the wave exit)

- **Origin**: design §4 (coherence map — the four-vocabulary table
  as the documentation spine), §3.9 (compat summary).
- **Files**: `docs/architecture.org` (connector registry +
  `ConnectorModule`, per-board status map + derived attribution,
  the counted all-down authority, `SetBoard`'s retirement, the
  caller-owned selection story); `README.org` (the
  `[[sync.connectors]]` config shape incl. the legacy-scalar
  synthesis rule and the typed mix rejections, multi-board usage:
  `--board`/`boards use`/the switcher keys); `CHANGELOG.md` (the
  wave's user-visible entries promoted from the per-chunk
  Unreleased lines); the wave exit-criteria verification record at
  `.artifacts/reports/<date>-multi-board-wave-exit-report.md`
  (evidence pointers into the verifying chunks' tests, per §5's
  map).
- **Non-goals**: no ADR 0010 text (landed in chunk 4); no new
  drift amendments (F10-03/F10-04/F15-18 stay backlog); no code.

## 4. Quality gates & conventions

Every chunk: the full AGENTS §10 gate set (`fmt --check`, `clippy
-D warnings`, `nextest`, doc tests, `cargo deny`, the
CI-identical three-command llvm-cov sequence). TDD per
`docs/testing_strategy.org` with the house rules restated: typed
outcomes, never text assertions; no sleeps — `tokio::time::pause()`
/
`advance()`, poll-a-predicate-with-deadline, join handles; fakes
over mocks (the `FakeModule` story for app-side connector control
without wiremock; the engine side keeps its existing
`SyncStateReader` fakes); proptest for pure domain logic (the
reconcile matcher, the aggregate fold); one behaviour, one test,
at the lowest layer that owns it. New `.rs` files carry the
per-crate SPDX header (AGENTS §6); `#![forbid(unsafe_code)]`
everywhere except `taskboard-ui-slint`. **Snapshot rule** (DF-9):
a widening chunk owns its one deliberate `insta` pass; no `.pending`
snapshots ship. **Tier notes**: chunks 4–5 carry tier-2 tests
against dockerized Nextcloud (single instance for 4's boot test;
two instances for 5's two-connector test + 6's move-board
exercise) — same harness conventions as the existing tier-2 corpus;
if the runner lacks docker in a given environment the chunk
records that honestly rather than skipping silently. **Branch/PR
conventions**: one branch + PR per chunk (AGENTS §8); stacked
descendants do the rebase dance proactively after any merge.

## 5. Sequencing & exit criteria

**Spine (default)**: `1 → 2 → 3 → 4 → 5 → {6 ∥ 7} → 8`. Chunk 1
lands **alone first** (standing decision P1-1 — nothing stacks on
the singleton it removes). **Parallel option**: chunk 7's true
dependencies are chunks 1 + 2 (the per-board map and the
board-scoped command surface), so it *may* be dispatched in
parallel with 3–5 when the round wants it; the serial spine is the
default presentation. Either way, stacked branches expect the
post-merge rebase dance (AGENTS §8: rebase onto `origin/main`
dropping already-merged duplicates, `--force-with-lease`, never
`--force`). Chunks 6 ∥ 7 after 5; 8 last.

**Interim state (the wave's one deliberate interim — sequencing
fact, not a design deviation)**: between chunks 2 and 4, multiple
boards and bindings are engine-possible while the actor still
syncs its first bound board only. The gap is *visible* (per-board
`pending_ops` climbs on unsynced boards via chunk 1's map), never
silent, and multi-board is not user-advertised until chunk 5's
config lands. The implementation round does not "fix" this out of
order (it closes at chunk 4's derived board set).

**Wave exit criteria → verifying chunks** (framing §6 draft,
refined by design §3.9's test specs):

| Exit criterion | Verified by |
|---|---|
| Two connectors live simultaneously (tier-2) | chunk 5 |
| Multiple boards syncing across them | chunk 5, exercised by 6 |
| One board moved between connectors | chunk 6 |
| v0.1.x config + DB upgrade additively keeps working | chunk 1 (round-trip) + chunk 4 (boot test) |
| Board switching by scripted keyboard journey alone | chunk 7 |
| `cargo deny` green with the promoted port | every chunk (trivially — no new deps, design §3.4) |

Chunk 8 records the verification with evidence pointers.

## 6. Completion bookkeeping

Closing happens **in the implementation round** — issues close as
their chunks' PRs merge; this plan's own chunk edits no tracker
(AGENTS §7). Per-chunk map:

| Chunk | Issue action |
|---|---|
| 1 | **closes #298** (SyncStatus three-layer singleton, F16-22); annotates #125 (progress note: map + 0006 landed) |
| 2 | annotates #125 (board-scoped vocabulary landed) |
| 3 | **closes #179** (`--forget-board`/unbind), **closes #170** (title-match reconciliation); annotates #125 |
| 4 | annotates #187 (connector identity end-to-end, trait promoted, ADR 0010 amended), #125 |
| 5 | **closes #187** (multiple active connectors — the N=2 exit criterion verified); annotates #125 |
| 6 | annotates #125 |
| 7 | annotates #125 |
| 8 | **closes #125** (multi-board support — wave exit, verification record linked) |

Also rides the chunks: ADR 0010 amendment text → chunk 4 (design
§6's draft, verbatim); `docs/architecture.org` + `README.org` +
`CHANGELOG.md` → chunk 8; per-chunk CHANGELOG Unreleased lines
land with each chunk and are promoted by 8. Design §7's backlog
candidates are restated in §1 for the user to promote — this wave
does not create, label, or close anything beyond the five
milestone issues above.

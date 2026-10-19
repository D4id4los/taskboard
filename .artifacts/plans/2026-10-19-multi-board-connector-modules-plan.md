# [Plan] Multi-board Chunk 4 — Connector identity end-to-end: module trait, tagged bindings, `SetBoard` retirement

- **Date**: 2026-10-19
- **Kind**: stage-1 TDD-first implementation plan (dispatch plan §5,
  prompt 4.1). No code in this commit.
- **Branch / worktree**: `feat/sync-connector-modules` at
  `../taskboard-WT-feat-sync-connector-modules`, cut from `origin/main`
  `6014a53` ("feat(app): board ceremonies at N — unbind/reconcile verbs,
  nudge-driven bind, per-board wait") — chunk 3 (PR #364) merged
  2026-10-19T11:32Z. `scripts/worktree-doctor.sh` passed (139G free,
  floor 25G).
- **Normative sources** (MD-7 rank): implementation plan
  `.artifacts/plans/2026-10-18-multi-board-wave-implementation-plan.md`
  §2 (DF-1, DF-3, DF-4, DF-5, DF-10 normative; DF-2/DF-6/DF-9 context),
  §3 chunk 4, §4, §5, §6; design
  `.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`
  §3.4 (trait sketch, narrow-seam check, test-fake story), §3.3
  (derived board sets, `SetBoard`'s fate, #157 amplification pricing),
  §3.6 (the synthesis-of-one subset), §3.9 (the additive-upgrade test
  spec), §5 ACCEPTED record, §6 (ADR 0010 text, landed verbatim);
  dispatch plan `.artifacts/plans/2026-10-18-multi-board-impl-dispatch-plan.md`
  §1 (execution model, MD-2…MD-9), §5 (chunk 4 covers/acceptance/traps).
- **Line numbers below are plan-day references** (MD-8); symbols and
  finding IDs are the stable keys. Every symbol was re-located on this
  base at plan time; the seam table in §2 is the 2026-10-19 truth.

## 1. Scope

**In** (impl plan §3 chunk 4, normative):

- New `crates/taskboard-app/src/connectors.rs`: the `ConnectorModule`
  trait per design §3.4's sketch, the `NextcloudModule` adapter wrapping
  `spawn_sync_actor`, the legacy-scalar synthesis-of-one
  (`configured_connectors`), and the bootstrap registry loop at N=1.
- Domain: `Board.remote` widens from `Option<RemoteBoardId>` to a
  connector-tagged binding (`RemoteBoardBinding` in `ids.rs`); the
  merge/pipeline/command fallout of the widening; `SyncCommand::SetBoard`
  retired from `messages.rs` (with its test construction).
- Sync: `spawn_sync_actor` gains the connector id; the resume/absorber
  path derives the actor's board set from tagged bindings **each
  cycle** (replacing `LoopState.target`/`resolve_target`); per-board
  pull-cache keys with **no cache-clear-on-retarget**; the push
  ownership filter (boards owned by *this* connector), including the
  per-board overlay seeding.
- State: `spawn_state_engine` gains `default_connector` (the engine's
  binding-tag mint at `BindBoard`); `append_validators_if_changed`
  keys by the report's board (the last first-bound-board pick on the
  ingest path); `SetBoard`-era doc comments reworded.
- Storage: the boards row decode gains the `connector` column (NULL ⇒
  the spawn-time default, non-NULL ⇒ the row's tag); upserts source the
  tag from the domain binding per DF-4.
- App bootstrap: the registry loop; `App::sync_commands()` removed;
  `SyncActor` replaced by the public `SpawnedConnector`; `App` holds
  the config-minted default id for the attach path.
- **ADR 0010 amendment**: design §6's two blocks verbatim (anchors in
  §3.7 below).
- **The v0.1.x additive-upgrade boot test** (tier-2, MD-5) + the
  FakeModule story for app-side connector control.
- **Snapshot pass 3**: the one deliberate `insta` regeneration for the
  binding-shape widening (DF-9's rule applied — see D-16).

**Out** (chunk 4 non-goals, restated): no `[[sync.connectors]]` plural
decode — config stays the legacy scalar + synthesis-of-one (chunk 5);
no per-connector events / authority (chunk 5); no CLI verb growth
(chunk 6); no UI (chunk 7); no second connector *kind* (nextcloud
only); no N>1 registry plumbing beyond what the loop's iteration
already is (speculative generality is a stage-3 trap).

## 2. Re-verified seam table (2026-10-19, base 6014a53)

| Symbol (stable key) | This round's line(s) | Notes vs design §2 table |
|---|---|---|
| app `bootstrap.rs` `SyncActor { commands, join }` | 121–124 | becomes `SpawnedConnector` + `id` |
| app `App.connectors: Mutex<Vec<SyncActor>>` | 141 | type swaps; guard semantics unchanged |
| app `App::sync_commands()` (`.first()` pick) | 178–188 | **removed** — zero callers outside the struct's own docs (chunk 3 removed `send_set_board`) |
| app `App::connector_attached` / `detach_connector` / `attach_connector` | 200 / 219 / 263 | attach keeps `client: DeckClient`; spawn call gains the id |
| app `ensure_vacant` / `try_register` / `stop_actor` | 443 / 456 / 472 | unchanged semantics (chunk 5 renames the error) |
| app `bootstrap()` client-construction match | 548–585 | moves into `NextcloudModule` |
| app `default_connector` mint (chunk 1's constant) | 587–591 | becomes the synthesis fn's output |
| app `spawn_sync_actor` boot call | 636–645 | becomes the registry loop's `build_and_spawn` |
| app `SYNC_COMMAND_CAPACITY` doc (SetBoard mention) | 78–80 | reword |
| sync `spawn_sync_actor(client, state, commands, reports, system, cfg)` | 120–129 | gains `id: ConnectorId` |
| sync `SyncActorConfig` | 41–53 | unchanged |
| sync `PullCache` (cleared on `SetBoard`) | 138–152 | per-board keys; clear-on-SetBoard dies |
| sync `LoopState { target, … }` | 154–165 | `target` removed |
| sync run-loop `SetBoard` arm | 199–207 | removed (with `SyncNow`/`Detach` staying) |
| sync nudge absorber `SetBoard` arms | 255–271 | absorber reduces to `SyncNow`/`Detach` |
| sync `resolve_target` + guard tests (D3-2) | 283–294, 839–910 | replaced by `owned_boards` + derived-set tests |
| sync `cycle()` single-target body | 315–493 | per-board loop (§3.3) |
| sync `pull_snapshot(target, …)` | 556–767 | unchanged per-board shape; called per board |
| sync `BindingOverlay::from_index(board, index)` | overlay.rs:22–41 | filters by the refs' own `board` field |
| sync push `#258` re-resolve (`fetch_card_for_write`, `relocate_card`, `delete_after_re_resolve`) | execute.rs:486+/543+/848+ | composes unchanged (board-agnostic per-entity) |
| domain `Board.remote: Option<RemoteBoardId>` | entities.rs:124 | the widening |
| domain `ConnectorId` | ids.rs:95–126 | chunk 1's newtype; unchanged |
| domain `SyncCommand::SetBoard` | messages.rs:309–320 | variant retired; `copy_enums_travel_by_value` (:391–402) constructs it |
| domain `SyncReport` | messages.rs:328–364 | **unchanged** (DF-1) |
| domain `plan_command(app, outbox, command, ids, now)` | command.rs:290–296 | gains `default_connector` (D-5) |
| domain `BindBoard` planner arm (binding mint at `updated.remote = Some(*remote)`) | command.rs:875–956 | mints the tagged binding |
| domain merge board fns (`adopt_board_if_newer`, `merge_board`, `adopt_board_after_push`, `adopt_board_after_delete_push`, `adopt_remote_board`) | merge.rs:641–810 | preserve `local.remote`; `adopt_remote_board` gains a connector param (D-8) |
| domain pipeline board section (`board_ids_by_remote`, adopt-new) | pipeline.rs:431–452 | match key `.board`; adopt-new mint via threaded id |
| domain persistence binding-change check | persistence.rs:429 | `.map(\|r\| r.board)` mechanical |
| domain `SyncPhase::NoBoard` doc (SetBoard mention) | state.rs:91 | reword |
| state `spawn_state_engine(...)` (9 params) | actor.rs:243–252 | gains `default_connector` (D-5) |
| state `forward_sync_now` (engine fan-out, `SyncNow` only) | actor.rs:600–610 | unchanged — confirms the engine never carried targeted commands |
| state `append_validators_if_changed` (first-bound pick) | actor.rs:744–761 | keyed by the report's snapshot board (D-11) |
| state `mark_bound_syncing` / `ingest_sync_report` Failed-with-no-pushes arm | actor.rs:729–733 / 647–667 | stay (N=1-connector exact; code comment already prices chunk 5) |
| storage `SqliteTaskRepository.default_connector` (writes the tag today) | repo.rs:37, 798 | write source flips to the domain binding (D-4) |
| storage boards upsert INSERT | repo.rs:777–805 | tag column source changes |
| storage `BoardRow` + boards SELECT (no `connector` column read) | repo.rs:42–57, 431–446 | column joins the SELECT + decode |
| storage `ClearBoardContent` validator cascade | repo.rs:1254–1255 | untouched (remote-id-keyed already) |
| ADR 0010 stance section / consequences bullet | 0010.org:140–145 / 163–165 | the two §6 anchors |
| snapshot corpus (YAML; `remote: <n>` lines) | state tests snapshots, 10 of 20 files | snapshot pass 3 |
| tier-2 harness (`SyncPair.set_board` uses `SetBoard`) | sync tests common/sync_pair.rs:86–95 | arms targets via the engine's `BindBoard` instead |
| app tier-2 (`it_docker.rs` `daemon_journey` seeds binding via storage) | app tests it_docker.rs:60+ | the boot test's home convention |

Verified negative facts (grep on this base): the state engine has **no**
`SetBoard` arm (the impl plan's "engine handler" is the sync actor's
run-loop + absorber arms — recorded clarification, not a deviation);
nothing outside `bootstrap.rs`/doc comments references
`App::sync_commands` or sends `SyncCommand::SetBoard` since chunk 3;
the UI crate has zero `Board.remote` readers; `SyncCommand` appears in
no repo/codec/protocol reference (in-process only — the retirement is
not a compat surface).

## 3. Behavioural architecture

### 3.1 The binding type and its mint chain (DF-3)

`ids.rs` gains, next to `ConnectorId` and the ref types:

```rust
/// A board's remote binding: the remote board *and* the connector that
/// owns it. The connector tag is config-minted (DF-3): one name from
/// config through the registry, the module, and the storage row.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RemoteBoardBinding {
    /// The connector that owns this binding (its actor syncs this board).
    pub connector: ConnectorId,
    /// The remote board id within that connector's namespace.
    pub board: RemoteBoardId,
}
```

`Board.remote` becomes `Option<RemoteBoardBinding>`. The mint chain is
exactly DF-3: config name → `ConnectorId` → this binding tag →
`SpawnedConnector.id` (the registry key) → `spawn_sync_actor`'s id →
`ConnectorModule::id`. No second mint anywhere: the one config-derived
`ConnectorId` value threads from `bootstrap` into the storage spawn
(chunk 1's pattern), the engine spawn (new, D-5), and the module.

Where bindings are minted or read (complete inventory, re-verified):

- **Mint 1 — `BindBoard`'s planner arm** (command.rs:954
  `updated.remote = Some(*remote)`): mints
  `Some(RemoteBoardBinding { connector: <engine default>, board: *remote })`.
  The same-remote no-op check and `RemoteBoardInUse` compare
  `.map(|r| r.board) == Some(*remote)` (board-id equality; at N=1 the
  tag is constant, so this is exact).
- **Mint 2 — `adopt_remote_board`** (merge.rs:799, the pipeline's
  adopt-new path at pipeline.rs:448): gains a `connector: ConnectorId`
  parameter (D-8).
- **Preserve, never re-mint — `merge_board` / `adopt_board_if_newer` /
  `adopt_board_after_push` / `adopt_board_after_delete_push`**: all four
  reconstruct the `Board`; each carries `local.remote.clone()` forward
  instead of `Some(remote.id)` — the merge never changes *which* remote
  board a local board is bound to (the binding is identity, not
  content; the snapshot came from that very board).
- **Clear — `UnbindBoard`** (command.rs:974 `updated.remote = None`):
  unchanged semantics.

### 3.2 The `ConnectorModule` trait + registry loop (design §3.4)

`connectors.rs` (new, app crate, AGPL header):

```rust
/// One configured connector implementation: builds its client from
/// config + secrets and spawns its actor over the shared vocabulary
/// (design §3.4). The narrow seam: no discovery, no dynamic loading,
/// no plugin registry, no versioning — config names a module, the app
/// constructs it.
pub trait ConnectorModule: Send + Sync {
    fn id(&self) -> &ConnectorId;
    fn build_and_spawn(
        &self,
        commands: mpsc::Receiver<SyncCommand>,
        engine_reader: Arc<dyn SyncStateReader>,
        reports: mpsc::Sender<SyncReport>,
        system: broadcast::Sender<SystemEvent>,
        cfg: SyncActorConfig,
    ) -> Result<SpawnedConnector, AttachError>;
}

/// A running connector's halves plus its registry key.
pub struct SpawnedConnector {
    pub id: ConnectorId,
    pub commands: mpsc::Sender<SyncCommand>,
    pub join: JoinHandle<()>,
}
```

`NextcloudModule::new(name, server_url, username, credentials)` holds
the config slice + credential store; `build_and_spawn` performs
credential load (`credentials.load(server, username)`), cleartext warn
(F12-03, moves with client construction), `DeckClient::new`, and
`spawn_sync_actor(self.id.clone(), client, …)` — wrapping today's
public free function, whose surface is unchanged (design §3.4
consequence).

The registry loop in `bootstrap`: iterate
`configured_connectors(&config)` (§3.6); for each spec construct the
module by `backend` (`Nextcloud ⇒ NextcloudModule`, `Standalone ⇒` the
empty vec never yields one) and call `build_and_spawn` with the
bootstrap-owned command receiver. N∈{0,1} this chunk; the loop's shape
*is* the seam, not speculative plumbing — a second module impl (chunk 5+
world) adds a match arm, nothing else.

`bootstrap` keeps its public signature (module doc example untouched);
internally it derives the module list and the loop runs it. For the
FakeModule story (§3.9) the loop's body is reachable through a
`pub(crate)`-visibility test seam: `bootstrap_with_modules(config,
credentials, notifier, modules)` — production `bootstrap` synthesizes
the Nextcloud module and delegates; app tests pass a `FakeModule`.

### 3.3 The derived per-cycle board set (design §3.3)

`LoopState.target: Option<RemoteBoardId>` is **deleted**;
`resolve_target` and its D3-2 guard are replaced by a pure function:

```rust
/// The boards this connector's actor owns: persisted bindings tagged
/// with this actor's id, live and tombstoned alike (R9c: a deleting
/// board's finalizing push is a sync fact), in board-id order.
fn owned_boards(
    id: &ConnectorId,
    boards: &BTreeMap<BoardId, Board>,
) -> Vec<(BoardId, RemoteBoardBinding)>;
```

The cycle (one `read_state`, one `read_at` stamp, then a per-board
loop):

1. **Read** once (unchanged stamping discipline).
2. **Derive** `owned_boards(actor_id, &persisted.boards)`. Empty +
   `commanded` ⇒ one `SyncReport::Failed { kind: NoBoard, … }` (today's
   decision-10 semantics, once per cycle — not per board); empty +
   poll tick ⇒ silent skip.
3. **Per board** (board-id order): push (that board's ops only,
   §3.4) → pull (`pull_snapshot` unchanged, called with the board's
   remote id and its own cache slots) → report (one
   `Completed`/`Failed` per board; `SyncReport` unchanged per DF-1 —
   the engine keys status and validators off the snapshot's board).
4. **Failure policy** (D-12): a board's failure — aborted push group or
   failed pull — reports that board's `Failed` (with its landed pushes
   as evidence) and the loop **continues** to the next board; a
   `SyncErrorKind::Network` failure additionally **ends the cycle
   early** (remaining boards unattempted, no reports for them): the
   transport is shared and down, the episode edge fires once, and the
   engine's `go_offline` (on `NetworkLost`) stamps every bound board's
   row `Offline` anyway — no board is left stuck `Syncing` by the skip.
   Backoff (`failure_streak`, `retry_after_floor`) stays actor-global.

The `#157` guardrail pricing holds by construction: per board, three
conditional listing pulls (the boards listing is shared — the first
board's 200 warms `cache.boards`, later boards 304-fill from it; the
design prices the boards listing per board) + per-card detail refresh
only for boards whose listings changed. Cycle cost O(boards + changed
cards).

**Cache**: `PullCache` becomes
`{ boards: Option<Vec<Board>>, stacks_active: BTreeMap<RemoteBoardId,
Vec<Stack>>, stacks_archived: BTreeMap<RemoteBoardId, Vec<Stack>> }`.
Per-board keys make "retarget" a non-event: a newly bound (or rebound)
board simply has (or lacks) its own slot; **no cache-clear-on-retarget
survives** (the `SetBoard` arm's and the eviction path's `clear()`s all
die with their callers). Soundness of a warm slot across an
unbind/rebind round trip: the cache content and the persisted
validators were written by the same cycle, so a later 304 (validator
match) fills from content that is still the server's truth. At cycle
start, stack slots whose remote id is absent from the derived set are
pruned (D-10 — memory proportional to the live set; not a
retarget-clear: boards *in* the set never lose their slot to set
changes).

**The absorber** reduces to `SyncNow ⇒ rerun_requested` and `Detach ⇒
return`. The absorber/retarget interaction risk (chunk 3's interim
`SetBoard` arms) dissolves by removal: mid-cycle bind/unbind decisions
land in the *next* cycle's derivation; the ceremony's broadcast
`SyncNow` nudge (chunk 3) already guarantees that cycle is prompt, and
the absorber coalesces any nudges that queued during the in-flight
cycle into one rerun.

### 3.4 The push ownership filter

A pure domain helper next to `LocalOp` (outbox.rs):

```rust
impl LocalOp {
    /// The board that owns this op's entity (board ops: themselves;
    /// stack ops: the stack's board; label ops: the label's board;
    /// task ops: the task's stack's board). `None` when the owning
    /// entity row no longer exists (clear-board content wipes).
    pub fn owning_board(&self, entities: &EntityTables<'_>) -> Option<BoardId>;
}
```

(Rationale for the home: app `read_scope` resolves the same graph
app-side — design §3.2 calls it "the same filter"; one rule, one home,
and the sync crate cannot reach into the app. `read_scope` adoption is
optional at stage 2's judgment; the sync filter must use the domain
helper.)

In the cycle, per board: filter `persisted.outbox` to ops whose
`owning_board` is this board; plan over the filtered slice
(`plan_pushes` unchanged — pure, partitioning); execute with an overlay
seeded **only from this board's refs** — `BindingOverlay::from_index`
additionally filters `stack_by_ref`/`task_by_ref`/`label_by_ref` by
`r.board == board` (today the single-board index made the filter a
no-op; at N≥2 an unfiltered overlay would stamp this board's id onto
another board's refs via `card_of`'s `self.board` override). Ops owned
by boards outside the set strand (never planned) — the honest
stranding chunk 3 pinned; `pending_ops` climbs per board via the map.
Ops whose owner is unresolvable (`None`) ride the **first** owned
board's planning pass, once per cycle (D-13): the planner
synthetically completes no-ops for missing entities — that bookkeeping
is cycle-level, and if the set is empty they strand exactly as today's
no-target cycle strands them.

`#258`'s stale-binding re-resolve composes unchanged: it operates
per-entity within `fetch_card_for_write`/`delete_card` (execute.rs),
board-agnostic by construction — the overlay feeding it is simply
board-scoped now.

### 3.5 The `SetBoard` retirement — one motion with the tags

The branch's diff shows both halves (the binding widening and the
removal) — either alone is the design's named sequencing trap. The
complete removal inventory (all re-verified to be the *only* sites):

1. domain `messages.rs`: the `SyncCommand::SetBoard` variant + its doc;
   the `copy_enums_travel_by_value` test construction (:396).
2. sync `actor.rs`: the run-loop arm (:199–207) and both absorber arms
   (:258–263).
3. app `bootstrap.rs`: `App::sync_commands()` (:178–188, zero callers),
   its doc, and the `SYNC_COMMAND_CAPACITY` doc's SetBoard mention.
4. Doc rewords only: domain `state.rs:91` (`SyncPhase`/`NoBoard` — "a
   cycle fired before `SetBoard`" → derived-set phrasing), app
   `commands/boards.rs:5` module doc.
5. The tier-2 `SyncPair::set_board` harness (sync tests common) —
   arms targets via the engine's `BindBoard` execute path instead
   (which is also the truer production path post-chunk-3).

Clarification recorded: the impl plan's "engine handler" is the sync
actor's run-loop arm — the state engine never had one (its fan-out
`forward_sync_now` only ever sends `SyncNow`; verified by grep).

### 3.6 Bootstrap synthesis-of-one (design §3.6 subset)

`connectors.rs`:

```rust
/// The configured connector set from the legacy scalar world (design
/// §3.6's subset; chunk 5 replaces the internals with `[[sync.
/// connectors]]` decode, the shape stays): resolved `nextcloud`
/// synthesizes exactly one connector named "nextcloud"; resolved
/// `standalone` synthesizes none.
fn configured_connectors(config: &AppConfig) -> Vec<ConnectorSpec>;
```

`ConnectorSpec { name: ConnectorId, backend: SyncBackend }`. Config
reading stays **entirely legacy**: `config.sync_backend()` (the auto
rule), `config.server_url()`/`config.username()` (`[nextcloud]`),
`config.sync_actor_config()`. `config.rs` itself has **zero diff** this
chunk — no array decode, no new validation (chunk 5's). The storage +
engine `default_connector` values derive from the synthesis: the first
spec's name, else the constant `"nextcloud"` (inert under standalone —
no bindings exist to decode; a legacy bound db flipped to standalone
decodes its NULLs against the stable name and nothing consumes the
tag).

`App` gains a private `default_connector: ConnectorId` (the single
config-derived mint, held once): `attach_connector` uses it for its
spawn (the attach path keeps its pre-built `DeckClient` and calls
`spawn_sync_actor` directly with the id — D-7; chunk 5 grows the
attach-verb name and unifies as needed).

### 3.7 The ADR 0010 amendment (design §6, verbatim)

Two anchored edits in `docs/adr/0010-standalone-first-optional-sync-
connector.org`, both character-for-character from design §6 (stage 3
diffs them against the design doc; paraphrase is a finding):

1. **Replace the stance section** — lines 140–145 today (the headline
   `** The connector port stays vocabulary, not a trait` plus its
   four-line body ending `promote this to a domain trait — not built
   speculatively.`; the section ends where `** Wire word` begins at
   :147) — with the §6 decision-section block (headline `** The
   connector port: shared vocabulary + an app-layer module trait` +
   its body).
2. **Replace the consequences bullet** — lines 163–165 today (the
   wrapped `- =logout='s interaction with =backend= is WI-3's
   question; a second connector, board reset/delete, and title-match
   reconciliation are backlog.`) — with the §6 consequences block
   (`- Board reset/delete landed with PR #334's verbs; title-match
   reconciliation (#170) lands with the multi-board wave; =logout='s
   interaction with =backend= is WI-3's question.`).

The design's §6 intro is explicit that the second block rewrites that
sentence wholesale. No other ADR 0010 line changes (F10-03/F10-04/
F15-18 stay backlog).

### 3.8 Storage decode + write (DF-4)

- **Decode** (the rule's single home): `BoardRow` gains
  `connector: Option<String>`; the boards SELECT gains the column;
  row→board maps `remote_board_id + connector` →
  `Some(RemoteBoardBinding { board, connector })` with **NULL ⇒
  `self.default_connector`** (the spawn-time parameter, app-derived).
  A non-NULL tag naming a connector that is not configured decodes
  as-is (D-3): the domain type accepts any name; validation of
  existence is nobody's codec job — the owning actor simply never
  finds the board (its set filter misses), ops strand honestly,
  `pending_ops` says so. No boot-time UPDATE pass (the design's
  rejected alternative — a surviving one is a stage-3 finding).
- **Write**: the boards upsert's tag source flips per DF-4 —
  `board.remote.as_ref().map(|r| r.connector.as_str()).unwrap_or(
  default_connector.as_str())`: bound rows carry their binding's tag
  (the domain field is the source), unbound rows keep the default
  (chunk 1's "non-NULL on every upsert" invariant holds; the tag on an
  unbound row is inert).

### 3.9 The v0.1.x additive-upgrade boot test + FakeModule

**Boot test** (design §3.9's assertion list, tier-2 per MD-5): a new
body fn in app tests `common` + an `#[ignore]`-gated wrapper
`it_nextcloud_docker_v0_1_x_additive_upgrade_boot` in
`tests/it_docker.rs` (the file's convention: body once, thin named
wrapper, `scripts/nextcloud_it_setup.sh up`, `with_deadline`).

- **Db fixture** (v0.1.x-shaped = schema at migrations ≤0005): build by
  applying the storage crate's 0001–0005 migration SQL files in order
  to a fresh sqlite db (raw SQL execution — mirroring chunk 1's
  round-trip test fixture approach; re-locate that builder and reuse
  its shape), then INSERT: one live board row (`remote_board_id = 77`,
  no `connector` column — it does not exist pre-0006), one stack + one
  bound task (epoch-default `ck_*` clocks — 0005's world), one queued
  outbox op for the task (e.g. `UpdateTask`), and a non-idle
  `sync_status` singleton (id = 1, `phase = 'syncing'`).
- **Config fixture**: the v0.1.x scalar world verbatim —
  `[nextcloud] server_url/username` (the dockerized instance), no
  `sync.backend` (auto-resolves nextcloud), env-provided app password
  (the existing `env_password` pattern).
- **Assertions** (typed/structural, never text): boot succeeds; after
  the first settled cycle the published state shows (1) the board's
  binding is `Some(RemoteBoardBinding { connector: "nextcloud",
  board: 77 })` — the synthesized connector + tagged-default decode;
  (2) the board's status row exists and settled (the singleton's
  backfill hydrated at migration; the cycle then settles it —
  `last_success` set, phase `Idle`); (3) the cycle merged per the
  unchanged policy (epoch-default clocks: the remote board's
  title/color win — assert the merged fields against the remote
  fixture); (4) the queued op drained (`pending_ops == 0` on the
  board's status row; the op's effect visible remotely — one typed
  remote read through `DeckClient`).
- **Hermetic subset** (runs everywhere): the db + config fixture
  through `bootstrap` against a refused/unroutable server asserts the
  boot-time half only — migration ran, decode + hydration + synthesis
  correct, `Offline`/`Failed` settles per the offline-first path; the
  merge + drain assertions are the tier-2 half. If the runner lacks
  docker, the stage-2 report + PR body record exactly that (MD-5);
  never a silent skip.

**FakeModule** (design §3.4's test-fake story — handwritten, app tests
`common`): implements `ConnectorModule`; `build_and_spawn` spawns a
scripted task over the real channels that answers `SyncNow` with a
configurable queue of `SyncReport`s (and stops on `Detach`/close).
Uses: registry-loop wiring at N=1 (`bootstrap_with_modules` +
`FakeModule` → engine nudge → scripted `Completed` lands → the board's
status row settles) and the detach path — connector control without
wiremock. The engine side keeps its existing `InMemoryRepository`/
`SyncStateReader` fakes; no mock library.

## 4. Settled decisions

Each settles an open question the dispatch plan left to stage 1;
deviations from prose carry the reason, none contradict an ACCEPTED
decision (design §5).

- **D-1 — binding type + home**: `RemoteBoardBinding { connector,
  board }` in `ids.rs` (the ref-type home; sibling of `RemoteCardRef`).
  Full derive set matching siblings (no `Copy` — `ConnectorId` is
  String-backed). Serde: plain map form
  (`remote: {connector: nextcloud, board: 77}`) — self-describing in
  YAML snapshots and the storage decode. Rejected: tuple form
  (unreadable in snapshots), widening every descendant ref type
  (out of the design's named scope — only the board binding carries
  the tag; descendant refs self-identify their remote *board*, which
  is board-scoped, not connector-scoped).
- **D-2 — engine binding-tag mint home**: `spawn_state_engine` gains
  `default_connector: ConnectorId` (10th param — the exact pattern
  `spawn_storage_actor` set in chunk 1), threaded to `plan_command`.
  Rejected: widening `BindBoard { board, remote, connector }` — the
  ACCEPTED §3.1 nine-arm table pins `BindBoard { board, remote }`;
  putting config names in `AppState` — config echo through the
  snapshot corpus; post-stamping actions in the engine — splits the
  mint from the plan. At N=1 the engine default *is* the only
  connector; chunk 5 revisits when the plural world needs
  per-ceremony naming (its file list already owns the attach-name
  growth).
- **D-3 — unknown non-NULL tag decode**: decode as-is; no codec-level
  existence validation; unowned boards strand honestly. (Chunk 4's
  world cannot mint such a row anyway — one name exists.)
- **D-4 — upsert tag source**: bound rows ⇒ the binding's tag (domain
  field is the source per DF-4); unbound rows ⇒ the default (keeps
  chunk 1's non-NULL-on-every-upsert invariant; the unbound tag is
  inert).
- **D-5 — `plan_command` signature**: gains
  `default_connector: &ConnectorId`. Fan-out is mechanical (engine
  call sites + domain test corpus); stage 2 keeps the edits purely
  mechanical (no semantic drift inside test bodies).
- **D-6 — trait final shape vs the sketch**: exactly the sketch's
  `id()`/`build_and_spawn(engine_reader, reports, system, cfg)` plus
  (a) the `commands: mpsc::Receiver<SyncCommand>` parameter and (b)
  the return type named `SpawnedConnector` instead of the sketch's
  `SyncActor` — the sketch is MD-7 illustration: the bootstrap owns
  the command pair (created before the engine spawn; handing creation
  to the module would force the engine rewire through the attach
  envelope for no gain), and `SyncActor` is a private struct the trait
  cannot name. `AttachError` reuse is the sketch's own. Object-safe,
  `dyn`-dispatched, `Vec<Arc<dyn ConnectorModule>>` never needed at
  N=1 (the loop takes the specs; the vec-of-modules world is chunk
  5's). No discovery/plugin surface (the narrow-seam check).
- **D-7 — the attach path**: `attach_connector(client: DeckClient)`
  keeps its signature and calls `spawn_sync_actor` directly with
  `App.default_connector`; the module factory is the boot path's
  constructor (client from config + secrets — attach already holds a
  ceremony-built client). Chunk 5's attach-name parameter owns the
  unification question. Single mint preserved via the `App` field.
- **D-8 — adopt-new mint**: `adopt_remote_board` gains a
  `connector: ConnectorId` param; the engine's `apply_sync_report`
  call threads the engine default. Exact at N=1 (the only actor is
  the default connector). The adopt-new path itself is rare (a raced
  mid-cycle unbind). Chunk-5 handoff note: at N≥2 the fan-in report
  carries no provenance — adopt-new attribution and cross-server
  `RemoteBoardId` collisions in `board_ids_by_remote` become chunk 5's
  vocabulary question (risk R-5; not solved speculatively here).
- **D-9 — empty-set semantics**: commanded cycle + empty derived set ⇒
  one `Failed { NoBoard }` report (decision 10 verbatim); poll tick ⇒
  silent skip. The days of per-`SetBoard` cycle kicks are gone —
  `SyncNow` is the only nudge.
- **D-10 — cache pruning**: per-board stack slots for remote ids not
  in the current derived set are pruned at cycle start; boards in the
  set never lose slots to set changes (that is the
  no-cache-clear-on-retarget rule). Memory stays proportional to the
  live set; a rebound board pays one unconditional refetch —
  correctness unchanged (validators still gate every 304 fill).
- **D-11 — `append_validators_if_changed` re-keys by the report's
  board** (the snapshot's `RemoteBoardId`, matched against
  `merged.boards` bindings): the last first-bound-board pick on the
  ingest path — at ≥2 boards per connector the old pick would file
  board B's validators under board A. Forced-correctness consequence
  of the derived set (design §3.2's "append_status_if_changed re-keys
  by the report's board" is chunk 1's status-side twin; this is the
  validators side, same story). The Failed-with-no-pushes arm (all
  bound boards marked) stays — the code comment already prices it
  ("per-connector attribution scopes this when reports carry a
  connector"); exact at N=1 board and for Network kinds generally.
- **D-12 — per-board failure policy**: continue per board on any
  failure; a Network-classified failure additionally ends the cycle
  early (episode edge once; `go_offline` stamps the unattempted
  boards' rows anyway). Rejected: abort-all-on-any-failure (starves
  healthy boards behind one broken board — contradicts per-board
  attribution); never-abort (N retries per cycle against a dead
  transport).
- **D-13 — orphan ops**: owner-unresolvable ops ride the first owned
  board's planning pass once per cycle (the planner's synthetic
  no-op-completion is cycle bookkeeping); empty set ⇒ they strand, as
  today's no-target cycle strands them.
- **D-14 — `NextcloudModule` ownership of client construction**: the
  bootstrap's client-construction match (incl. the F12-03 cleartext
  warn and credential load) moves into the module — "builds its client
  from config + secrets" is the design's own sentence. The
  `BootstrapError::Client`/`Credential` variants map from
  `build_and_spawn`'s `AttachError`-shaped result at the loop
  (wrap, not duplicate).
- **D-15 — `SyncCommand` serde**: stays derived (the enum keeps
  Serialize/Deserialize); only the variant goes. No persistence or
  wire reference exists (verified).
- **D-16 — snapshot pass 3**: DF-9's rule ("a chunk that widens a
  snapshotted shape owns its one deliberate insta regeneration pass")
  applies to this chunk's `Board.remote` widening — 10 of the 20
  state-test snapshots carry a `remote: <n>` line. One deliberate
  `cargo insta accept` pass inside the chunk; the accepted diff
  reviewed as "the binding shape, nothing else"; no `.pending` ships.
  The impl plan's chunk-4 file list does not name the pass — this is
  the normative DF-9 applied, recorded here so stage 3 treats the
  regeneration as expected, not as drift.
- **D-17 — `wait_for_boot_pull` / aggregate surfaces**: unchanged —
  the aggregate fold over the per-board map is the chunk-1 shape and
  stays byte-stable for the single-board install.

## 5. TDD plan — full test catalogue (typed outcomes; AGENTS §5)

Layered lowest-first; one behaviour, one test. House rules: typed
outcomes (never text), no sleeps (`pause()`/`advance()`,
poll-with-deadline, join handles), fakes over mocks, no quality
ceilings.

**Domain (pure, unit)**

1. `RemoteBoardBinding` serde round trip (map form) — property: a
   generated (name, u64) pair round-trips through YAML/JSON.
2. `BindBoard` mints the tagged binding with the engine default id;
   same-remote rebind (board-id equality) = accepted no-op;
   `BoardAlreadyBound`/`RemoteBoardInUse` unchanged shapes.
3. `UnbindBoard` clears the binding (existing chunk-3 tests port
   mechanically to the new type).
4. Merge preservation: `merge_board`/`adopt_board_if_newer`/
   `adopt_board_after_push`/`adopt_board_after_delete_push` carry
   `local.remote` verbatim (a tagged binding survives every merge
   path — construct locals with a non-default tag to prove
   preservation, not re-minting).
5. `adopt_remote_board(connector, …)` mints the binding with the
   passed id.
6. `LocalOp::owning_board` truth table: each of the four op families ×
   (owner present / tombstoned / row-missing) — tombstoned rows still
   resolve (rows survive deletion); missing rows → `None`. Proptest:
   a generated entity graph + op never resolves to a board the entity
   does not belong to.
7. `SyncCommand` post-retirement: the copy/wire tests hold for
   `{SyncNow, Detach}` (the `SetBoard` construction is gone); exhaust
   the enum (no dead arms).
8. Persistence-plan check (:429 port): the validator-cascade retain
   still keeps other boards' keys under the binding type (existing
   test, mechanical port).

**State engine**

9. Engine mints the binding through `spawn_state_engine`'s
   `default_connector` — `BindBoard` on an unbound board publishes
   `remote: Some({connector: default, board})` (in-memory +
   persisted-action assertion).
10. `append_validators_if_changed` keys by the report's board: two
    bound boards, a `Completed` report for the second — the
    `UpsertValidators` actions use the second board's `RemoteBoardId`
    (typed action inspection, not snapshot).
11. Per-board report ingestion at 2 boards on one connector: one
    `Completed` per board settles each board's status row (existing
    chunk-1 ingest tests extended; snapshot pass 3 covers the shape).
12. Engine-only `SetBoard`-era tests: no engine test referenced
    `SetBoard` (verified) — no engine-side removals beyond the spawn
    signature.

**Sync actor (hermetic, wiremock where a server is needed)**

13. `owned_boards` unit tests — the ported D3-2 semantics: tombstoned
    board stays in the set (R9c drain); finalized tombstone (binding
    cleared) drops out; fresh actor derives from persisted bindings
    (decision-13 resume); boards tagged with *another* connector are
    excluded (the new N=2 predicate, testable at N=1 with a
    hand-tagged fixture); board-id ordering.
14. Derived set recomputes per cycle: seed a binding mid-test (fake
    reader state), nudge, the next cycle targets it with **no**
    targeting message — the "newly bound board picked up by the next
    read" behaviour (wiremock: the new board's endpoints get hit).
15. Per-board push isolation: two owned boards, ops on both — board
    A's cycle pushes only A's ops; B's ops strand until B's turn
    (assert outcomes per report + stranded count unchanged).
16. Ownership exclusion: an op owned by a foreign-tagged board is
    never planned (no wiremock hit for it; it strands).
17. Overlay board-scoping: `from_index` with refs of two boards
    returns only the named board's entries (unit, overlay.rs).
18. No cache-clear-on-retarget: warm slots for board A survive a
    cycle that adds board B (assert A's next pull 304-fills from its
    slot — wiremock conditional sequence); pruning: a slot for a
    board that left the set is gone (unit on the cache type).
19. Empty derived set: commanded ⇒ exactly one `Failed { NoBoard }`
    report; poll tick ⇒ none (report-count assertions).
20. Failure policy: board A pull fails (non-Network) ⇒ A's `Failed`
    report + B still cycles (B's endpoints hit); A fails Network ⇒
    cycle ends, no B report, one `NetworkLost` broadcast, both
    boards' rows `Offline` in the engine.
21. The `#258` re-resolve suite (push_exec tests) passes unchanged —
    board-agnostic composition proof (existing corpus; no edits
    beyond fixture porting to the binding type).
22. Absorber: `SyncNow` bursts during a cycle collapse to one rerun
    (existing absorber test ported — the `SetBoard` arms' cases
    become derived-set cases in 14).

**Storage**

23. Decode rule: NULL tag ⇒ default id; non-NULL tag ⇒ the row's name
    (round-trip through upsert+load); unknown name decodes as-is
    (D-3). Existing chunk-1 codec tests ported + the two new cases.
24. Write source: bound board upsert writes the binding's tag; unbound
    writes the default (typed row inspection).
25. Migration round-trip (chunk 1's) still green — no migration diff
    this chunk.

**App (bootstrap / connectors.rs / FakeModule)**

26. `configured_connectors`: resolved nextcloud ⇒ one spec named
    `nextcloud`; standalone ⇒ empty (typed vec assertions).
27. Registry loop wiring: `bootstrap_with_modules` + `FakeModule` —
    nudge → scripted `Completed` → the board's status row settles;
    `SpawnedConnector.id` is the config-minted id.
28. `attach_connector` spawns with the default id: attach after boot
    (detached first) re-adopts — actor derives the persisted binding
    (existing attach tests ported).
29. `App::sync_commands` absence: compile-level (its callers are
    gone); detach/shutdown paths unchanged (existing b-suite green
    after porting fixtures to the binding type).
30. Boot failure mapping: `build_and_spawn` credential/client errors
    surface as the existing `BootstrapError` variants (typed).

**Tier-2 (dockerized, MD-5)**

31. The v0.1.x additive-upgrade boot test (§3.9): full assertion list;
    the hermetic subset (§3.9) runs everywhere.
32. `SyncPair` harness port: `set_board` → engine `BindBoard`; the
    existing docker suite green through the derived-set world.

**Snapshots**

33. Pass 3 (D-16): the 10-file regeneration; review rule — the
    binding shape and nothing else.

## 6. Implementation phases (stage 2)

TDD inside each phase; every commit standalone (rebase-merge hygiene).

1. **Domain widening**: `RemoteBoardBinding`, `Board.remote`, mint
   sites (BindBoard via a temporary default passed in tests),
   preserve sites in merge, `adopt_remote_board` param,
   `LocalOp::owning_board`, `SyncCommand::SetBoard` removal, doc
   rewords. Domain compiles + tests green.
2. **State**: `spawn_state_engine(default_connector)`,
   `plan_command` param, `append_validators_if_changed` re-key;
   engine tests + snapshot pass 3 (the state suite's harness fixture
   `seed_board_state` ports here).
3. **Storage**: decode + write source; codec tests.
4. **Sync actor**: spawn id, `owned_boards`, `LoopState`/cache rework,
   per-board cycle, ownership filter, overlay scoping, absorber
   reduction; hermetic + wiremock suites ported/extended.
5. **App**: `connectors.rs` (trait, `NextcloudModule`, synthesis,
   `SpawnedConnector`), `bootstrap_with_modules` split, registry loop,
   `App.default_connector`, `sync_commands` removal, attach port;
   FakeModule + app tests.
6. **Tier-2 + docs**: boot test (+ hermetic subset), `SyncPair` port,
   ADR 0010's two blocks, CHANGELOG Unreleased line, full gate run.

## 7. Quality gates (AGENTS §10, verbatim sequence)

`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets
-- -D warnings`; `cargo nextest run --workspace`; `cargo test --doc
--workspace`; `cargo deny check`; the CI-identical three-command
llvm-cov sequence + quarantine + report + `rm -rf
target/llvm-cov-target`. SPDX headers on new `.rs` files (app:
AGPL-3.0-only; domain/sync/state/storage: MIT OR Apache-2.0);
`#![forbid(unsafe_code)]` holds outside `taskboard-ui-slint` (which
this chunk never touches). `cargo deny` green with **zero dependency
changes** — the trait's boundary exit criterion.

## 8. Risks, recorded deviations, interim statements

- **R-1 — the absorber/retarget interaction** (the named risk):
  correctness now rests entirely on the derived set being the only
  target source. Mitigations pinned by tests 13/14/18/22: per-cycle
  re-derivation, prompt cycles via the broadcast nudge + absorber
  coalescing, per-board cache keys (a stale slot is unreachable
  garbage, never wrong data — validators gate every 304 fill), and
  the D3-2 semantics ported into `owned_boards` (tombstoned boards
  drain). Residual: a board rebound to a *different* remote id
  mid-cycle keeps its old slot until the next cycle's prune — inert
  by the same validator argument.
- **R-2 — `plan_command` signature fan-out**: wide mechanical test
  edits risk semantic drift; stage-2 discipline: mechanical only,
  stage-3 greps the diff for non-mechanical hunks inside test bodies.
- **R-3 — snapshot pass 3 discipline**: the accepted diff must be the
  binding shape only (10 files, ~10 lines + YAML structure); anything
  else is a finding.
- **R-4 — tier-2 availability**: the boot test's merge/drain half
  needs docker; MD-5 honest recording if absent (hermetic half always
  runs).
- **R-5 — N≥2 provenance handoff** (recorded, not solved — D-8):
  fan-in reports carry no connector identity; adopt-new attribution
  and cross-server `RemoteBoardId` collisions in
  `board_ids_by_remote` are chunk 5's vocabulary question. Chunk 4 is
  N=1-exact everywhere the question can arise.
- **R-6 — faceless-failure blur at ≥2 boards on one connector**
  (D-11's accepted residue): a per-board non-Network pull failure with
  no landed pushes marks every bound board `Failed` (the report
  carries no board identity). Visible, self-healing (the next cycle
  re-settles healthy boards), exact at N=1 board and for Network
  kinds. **Backlog candidate for the user** (not an autonomous
  issue): per-board failure attribution needs report provenance —
  a `SyncReport` vocabulary question that DF-1 pins as unchanged for
  this wave, so it cannot be fixed here.
- **Deviations**: none from ACCEPTED decisions. Recorded
  clarifications/applications: the "engine handler" in the impl
  plan's SetBoard inventory is the sync actor's arm (verified
  negative fact); snapshot pass 3 is DF-9's rule applied to this
  chunk's widening (D-16); the trait's two shape adjustments vs the
  illustrative sketch (D-6).
- **Interim statement**: with chunk 4 landed, one connector syncs
  *all* boards bound to it (the chunk-2/3 interim closes);
  multi-board remains not-user-advertised until chunk 5's config —
  `[[sync.connectors]]` does not exist yet, every install is
  synthesis-of-one, and the engine default tag is the only tag any
  ceremony can mint.

## 9. Bookkeeping & handoff

- Stage 2: CHANGELOG.md Unreleased line (user-visible: bindings carry
  their connector; a connector syncs every board it owns; `SetBoard`
  retired internally). No tracker edits (MD-4).
- Stage 3: PR body annotates #187 and #125 (chunk 4 does **not**
  close #187 — chunk 5's N=2 exit criterion does); rebase-first,
  checks green + `CLEAN`, worktree removed per the handoff contract.
- Estimated diff: ~1,300–1,700 lines across 6 crates + ADR + tests
  (M, the top of M — the sync-actor cycle rework and the app
  connectors module are the two masses; the domain/state/storage
  slices are mechanical ports).

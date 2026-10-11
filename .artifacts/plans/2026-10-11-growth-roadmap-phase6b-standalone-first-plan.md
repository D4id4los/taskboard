# [Plan] Growth Roadmap Phase 6b — Stand-alone-first: the Optional Sync Connector

- **Date**: 2026-10-11
- **Target**: `taskboard-domain` (`messages.rs`, `command.rs`, `state.rs`
  outcome/error enums, proptest strategies), `taskboard-app`
  (`config.rs`, `bootstrap.rs`, `cli.rs`, `commands/{boards,stacks→new,
  mod,output}.rs`, `main.rs` daemon arm), test suites in domain/state/app,
  docs (ADR 0010, architecture.org, roadmap amendment, README,
  CHANGELOG, backlog appends). **No changes** to
  `taskboard-storage-sqlite` (schema is already board-scoped and
  `remote` is nullable), `taskboard-sync-nextcloud` (it becomes the
  first connector unchanged), `taskboard-ui-slint`.
- **Goal**: implement the user's 2026-10-10 decision (backlog entry
  "[2026-10-10] Stand-alone-first product shape"): the app works fully
  stand-alone — local task management with no server configured — and
  treats Nextcloud sync as an optional connector selected by config.
  Fresh installs default to stand-alone; existing `[nextcloud]`
  configs keep today's behavior. Local board creation becomes possible
  (boards so far only appear via sync adoption), and two attach
  ceremonies let stand-alone content migrate onto a remote board:
  `boards publish` (create a fresh Deck board from the local one) and
  `boards select --bind-local-board` (bind onto an existing board).
- **Status**: ACCEPTED — implemented on `feat/standalone-sync-connector`
  (steps 1–9 of §6; tier-2 J4 green).
- **Parent roadmap**: `.artifacts/plans/2026-10-08-growth-roadmap-plan.md`
  §3 (this plan inserts as Phase 6b between Phase 6 and Phase 7;
  §8 of this plan amends the roadmap file), §4 backlog entry above.
- **Predecessors**: Phase 6 merged. Authored against `main` @ `cc34e3f`.
  Analysis backing this plan:
  `.artifacts/reports/2026-10-11-m1-exit-pre-gui-analysis-report.md`.
- **Branch / worktree**: `feat/standalone-sync-connector`, worktree
  `../taskboard-WT-feat-standalone-sync-connector` (created from
  `main` at implementation start). One PR for the plan (phase 4/5/6
  precedent: plan revisions and implementation commits land on the
  same branch).

---

## 1. Context & Known Facts (do not re-derive; verified on `main` @ `cc34e3f`)

- **The Nextcloud coupling is app-layer, not engine-layer.**
  `bootstrap()` unconditionally loads credentials and builds the
  `DeckClient` before spawning anything (bootstrap.rs:265-266) and
  always spawns the sync actor (bootstrap.rs:301-308). The state
  engine, in contrast, already tolerates a missing connector:
  - The actor loop's exit condition treats a closed sync-reports
    channel as one closed input among three (`actor.rs:268`); with no
    sender ever created, `reports_open` is false from the first poll
    and the engine serves commands indefinitely.
  - `RequestSync` with a dead connector replies `NoOp` and skips the
    memory-only `Syncing` transient (`actor.rs:394-403`); mutations
    likewise only mark `Syncing` when the nudge forwards
    (`actor.rs:449`). `forward_sync_now` classifies a closed channel
    as a `debug!` "no sync actor" event (`actor.rs:472-474`) —
    engine-only tests already run this shape.
- **Config validation is the gate that blocks stand-alone boots**:
  `AppConfig::validate` requires a valid `nextcloud.server_url` and a
  non-empty `nextcloud.username` unconditionally (config.rs:267-285).
  Durations and `db_path` checks are connector-blind already.
- **Boards only exist via sync adoption today.** Every board-scoped
  command resolves `live_board(app)` and rejects with `NoBoard`
  (command.rs:334 for `CreateStack`, :417 for `CreateLabel`); there is
  no `StateCommand` that creates a board. `Board` has **no per-field
  clocks by design** ("no local board-edit commands exist, so board
  content only arrives via remote adoption", entities.rs:90-108) —
  board fields carry `remote: Option<RemoteBoardId>` and
  `remote_seen` only.
- **The outbox is already connector-blind.** `CreateStack` /
  `CreateLabel` / `CreateTask` enqueue outbox ops unconditionally
  (command.rs:351-357 etc.); nothing requires a remote binding to
  enqueue. Under a missing connector the ops simply accumulate in
  sqlite — exactly the "local write-ahead queue a connector drains
  when configured" the backlog entry wants. `DeleteTask` already
  splits bound (enqueue delete op) vs unbound (fail the queued ops —
  "the server never knew the entity", command.rs:317-327).
- **Offline-authored chains drain coherently in one cycle.** Push
  groups are ordered stacks → labels → tasks (`plan_pushes`, push.rs
  `kind_rank`; pinned by `groups_are_ordered_stacks_labels_tasks`);
  a `CreateStack` echo extends the in-cycle `BindingOverlay`, after
  which a `CreateTask` whose stack was unbound resolves its remote
  stack id from the overlay. `create_task` *rejects* only when the
  stack binding is still missing at execution time (push_exec.rs:345).
  The same dependency chain covers stand-alone-authored content once a
  connector is attached.
- **The pull matches boards by remote binding, so bind-before-pull
  merges instead of duplicating**: the pipeline builds
  `board_ids_by_remote` from existing `board.remote` refs and only
  mints a fresh local board when the snapshot's remote id is unmatched
  (pipeline.rs:289-305). Setting `local_board.remote = Some(R)` before
  the first cycle therefore makes the first pull merge into the
  existing board (`adopt_board_if_newer`) — remote identity wins for
  board fields (boards have no clocks; adoption semantics).
- **Identity, not titles, is the merge key** — a local-only stack and
  a remote stack with the same title are two entities. Consequence
  (accepted, documented): attaching a stand-alone board onto a remote
  board that already holds similarly-named content *unions* both sets
  (the outbox re-creates the local entities remotely, the pull adopts
  the remote ones). `boards publish` onto a freshly created remote
  board avoids this entirely; title-match reconciliation is a backlog
  entry (§7).
- **Storage needs nothing**: `stacks.board` and `labels.board` are
  `REFERENCES boards (id)` (migrations/0001_init.sql:27, 68);
  `boards.remote_board_id` is nullable; `PersistenceAction::
  UpsertBoard(Board)` exists (persistence.rs:172) and is exercised by
  adoption. Local boards are `remote = NULL` rows in the same table.
- **The single-board invariant is `live_board()` = min(live BoardId)**
  (command.rs:99). This plan preserves it by refusing a second live
  board at both entrances: `CreateBoard` rejects `BoardExists`;
  `boards select` keeps its `BoardAlreadyBound` refusal and gains a
  typed `LocalBoardExists` guidance error instead of silently
  adopting alongside a local board.
- **The Deck client already has the publish primitive**:
  `DeckClient::create_board(title, color)` (client.rs:360) — plus
  `boards()` for pre-validation and `DeckColor::from_hex` for color
  mapping.
- **Engine surface for new verbs** (phase 6 plan §1, unchanged):
  `execute(StateCommand) -> CommandOutcome`; `CommandOutcome` =
  `CreatedTask/CreatedStack/CreatedLabel/Applied/NoOp/SyncRequested`
  (command.rs:81-94); domain `CommandError` =
  `UnknownTask/TaskDeleted/UnknownStack/StackDeleted/UnknownLabel/
  LabelDeleted/NoBoard` (command.rs:55+). CLI `domain_error_kind`
  maps these to kebab-case JSON kinds (commands/mod.rs:186-194).
- **Handler pattern to extend** (ADR 0009): core-booting verbs wrap an
  `AsyncFnOnce(&App)` body in `with_core` (commands/tasks.rs:32-43);
  pre-core commands (`login`, `boards list`) build only config + a
  `DeckClient` via `client_for`. `CommandError::error_kind()` is an
  exhaustive match the compiler extends (commands/mod.rs:140-166).
- **No new workspace dependencies.** Everything needed is in the
  approved stack. (The kiosk/no-`Secret Service` story even improves:
  stand-alone mode never touches a credential store.)

## 2. Confirmed Product Decisions (user, 2026-10-10 + this plan)

1. **Stand-alone is the default product shape at M2 (v0.1.0)**
   (backlog entry, user decision): local-first task management with no
   server; Nextcloud sync is the first optional connector; future
   connectors (`taskboard-sync-<name>` crates) follow the same shape.
2. **Config value naming**: `[sync] backend = "standalone" |
   "nextcloud"`, optional — absence means **auto**: `nextcloud` iff
   `[nextcloud] server_url` is present, else `standalone`. (Wire word
   "standalone" instead of the backlog sketch's "none": self-describing
   in an operator's config file; recorded in ADR 0010.)
3. **Auto is loud, not silent**: any explicit `server_url` resolves
   the backend to `nextcloud`, so a half-configured file (server but
   no username) fails validation with today's typed `EmptyUsername`
   rather than silently degrading to stand-alone. Explicit
   `backend = "standalone"` is the only way to keep a `[nextcloud]`
   table inert.
4. **Attach is opt-in and identity-based** (this plan): `boards
   publish` is the primary ceremony (fresh remote board, no union
   ambiguity); `boards select --bind-local-board` binds onto an
   existing remote board with documented union semantics. Plain
   `boards select` refuses with typed guidance while a live
   local-only board exists.
5. **The connector port stays vocabulary, not a trait** (this plan):
   `SyncCommand`/`SyncReport`/`SyncStateReader` already are the
   connector contract (ADR 0007); the app selects which actor to spawn
   (today: nextcloud or none) with a plain match. A second connector
   may promote this to a domain trait — not built speculatively
   (AGENTS §5 spirit; the query-envelope precedent).

## 3. Scope

**In scope**:

| Area | Deliverable |
|---|---|
| Domain commands | `StateCommand::CreateBoard { title, color }` and `StateCommand::BindBoard { remote }`; `CommandOutcome::CreatedBoard(BoardId)`; `CommandError::BoardExists` + `CommandError::BoardAlreadyBound(RemoteBoardId)`; proptest strategies extended (§4.1) |
| Config | `SyncBackend` enum (`standalone`/`nextcloud`, kebab-case); `SyncSection.backend: Option<SyncBackend>`; resolved accessor `AppConfig::sync_backend()`; `validate()` gating per decision §2.2/2.3 (§4.2) |
| Bootstrap | Backend-conditional wiring: `standalone` skips credential load, `DeckClient`, and `spawn_sync_actor`; `App` holds the sync join optionally; channels still created (rx dropped) so the engine nudges fail closed exactly as designed (§4.3) |
| CLI | `boards create <title> [--color <hex>]`; `stacks add <title> [--order <n>]`; `boards publish [--timeout-secs]`; `boards select … --bind-local-board`; typed `SyncDisabled` refusals for `sync`, `daemon`, `boards list/select/publish` under `standalone`, and `login` under *explicit* standalone (§3.1) |
| Output contract | `BoardReceipt` / `StackReceipt` projections; new error kinds `board-exists`, `board-already-bound`, `local-board-exists`, `sync-disabled` (§4.5) |
| Tests | Domain unit + proptest; state engine S/A-series extensions; app C/B-series config/bootstrap extensions; new ST-series (standalone CLI journeys, hermetic sqlite, **no wiremock** — the test-cost win of this phase); V-series attach ceremonies over wiremock; J4 docker journey (§6) |
| Docs | ADR 0010; `architecture.org` product-shape + config amendment; roadmap master-plan Phase 6b insertion; README stand-alone-first quickstart; CHANGELOG; backlog appends (§7, §8) |

**Out of scope** (backlog or later phases): board-edit commands
(`RenameBoard`, board clocks — existing entry stays); title-match
reconciliation for attach (new §7 entry); deleting/resetting a board
(no `DeleteBoard` command — a fresh db path remains the reset);
`logout`'s interaction with `backend` (WI-3 decides); a second
connector; multi-board; REPL/daemon-IPC (WI-4/5); any UI work.

### 3.1 CLI surface after this phase

```
taskboard boards create <title> [--color <hex>]        # local board; both backends
taskboard stacks add <title> [--order <n>]             # needs a live board; both backends
taskboard boards publish [--timeout-secs <60>]         # nextcloud only; local board → new remote board
taskboard boards select <id> [--timeout-secs <60>] [--bind-local-board]
                                                        # nextcloud only; bind flag migrates a local board
taskboard sync | daemon | boards list                  # nextcloud only (typed sync-disabled under standalone)
taskboard login                                         # works under auto; refuses under explicit standalone
tasks list/add/done/move                                # unchanged, backend-agnostic
```

## 4. Design Decisions

1. **Backend resolution is explicit-or-auto, resolved once.**
   `SyncSection.backend: Option<SyncBackend>` (serde kebab-case
   `standalone`|`nextcloud`, `#[serde(default)]` = auto).
   `AppConfig::sync_backend()` returns the effective backend:
   explicit value, else `nextcloud` iff `nextcloud.server_url`
   `.is_some()`, else `standalone`. Every consumer (validate,
   bootstrap, CLI refusals, `login`) calls this one accessor — no
   scattered re-derivation. Auto-keying on `server_url` (not
   `username`, not "table present") keeps a half-configured file loud
   (§2.3). Env override works through the existing figment chain
   (`TASKBOARD_SYNC__BACKEND`).

2. **Validation gates on the resolved backend.** `validate()`:
   resolved `nextcloud` → today's checks verbatim (URL shape,
   username). Resolved `standalone` → the same checks apply only to
   *present* values (a `server_url` that is present but malformed is
   still `InvalidServerUrl` — typo guard; neither field is
   *required*). Duration and db-path checks unchanged. `login`
   continues to bypass `load()`+validate via `extract_raw` (ADR 0009)
   and additionally refuses with a typed `SyncDisabled` when the
   *explicit* backend is `standalone` (guidance: set
   `[sync] backend = "nextcloud"` or remove the key); under auto it
   behaves exactly as today (and after it writes `server_url`, auto
   resolves `nextcloud` — no file key needs writing).

3. **Bootstrap: one shape, conditionally-spawned connector.** The
   failure-prone pure prefix keeps its order (mode check → validate →
   lock → db open → engine hydration). Credential resolution +
   `DeckClient::new` move under `if sync_backend == Nextcloud`. The
   channel graph is created unconditionally; under `standalone` the
   `sync_rx` receiver and `reports` sender are dropped immediately —
   the engine then sees exactly the closed-channel world its loop
   already handles (§1), and `App::sync_commands()` senders fail
   closed (`try_send` → `Closed` → debug!). `App` grows a private
   `sync: Option<(mpsc::Sender<SyncCommand>, JoinHandle<()>)>`-shaped
   field (sender still exposed via `sync_commands()` for API
   stability); `shutdown()` joins the sync actor only when `Some`.
   The instance lock, storage actor, engine spawn, and shutdown
   sequence are backend-blind.

4. **`CreateBoard` is a first-class domain command with no outbox
   op.** Plan: refuse when a live board exists (`CommandError::
   BoardExists` — the single-board invariant's create-side guard);
   mint `Board { id: new_board_id(), remote: None, title, color,
   archived: false, deleted: false, remote_seen: None }`; persist
   `UpsertBoard`; **no `EnqueueOp`** — a local board creation is not a
   remote intent (server-side board creation is the `publish`
   ceremony's job, which binds to an existing remote board). Outcome:
   `CommandOutcome::CreatedBoard(BoardId)`. The engine's interpret
   path needs nothing new (`UpsertBoard` already lands adoptions).
   Boards keep their no-clocks design (§1); `publish`/bind never
   merge board fields locally except via adoption.

5. **`BindBoard { remote }` is the attach primitive.** Plan: require
   exactly one live board and require it local-only (`NoBoard` /
   `BoardAlreadyBound(remote)` otherwise); set `board.remote =
   Some(remote)`, persist `UpsertBoard`, no outbox op. Correctness
   rests on §1's pipeline fact: the next pull matches the board by
   `remote` and merges (`adopt_board_if_newer`) instead of adopting a
   duplicate. The ceremony pre-validates the remote id against a
   fresh `boards()` listing (bogus ids must be `UnknownBoard` before
   any mutation, mirroring ADR 0009's select rule). Local-only stacks
  /tasks/labels then drain via their queued ops (§1 offline-chain
   fact); entities the remote board already holds are adopted
   alongside — union semantics, documented (§2.4).

6. **`boards publish` composes existing primitives; no new sync-crate
   surface.** Requires `nextcloud` backend (typed refusal under
   standalone) and exactly one live local-only board (else
   `NoBoard`/`BoardAlreadyBound`). Flow: `client_for(config, store)`
   → `create_board(local.title, color)` with the color mapped via
   `DeckColor::from_hex(local.color)` falling back to a
   `DEFAULT_BOARD_COLOR` const when the lenient stored value is not
   valid hex (CLI-validated input makes this defensive) → execute
   `BindBoard { remote: created.id }` → send `SetBoard(created.id)` →
   wait via the existing `wait_for_sync_result` (the first cycle
   drains the outbox onto the fresh board; `Succeeded` requires
   `last_success` to advance). A transport failure mid-ceremony is
   safe: the board may exist server-side unbound — retrying publishes
   a second board; the runbook documents deleting the orphan in the
   Nextcloud UI (the duplicate-create crash-window backlog entry owns
   the general problem).

7. **`boards select` grows `--bind-local-board`; refusal is the
   default.** With a live local-only board present: without the flag
   → typed `LocalBoardExists` (CLI-level `CommandError` variant; text
   names the flag and `publish`); with the flag → pre-validate id →
   `BindBoard` → `SetBoard` → adoption wait (today's helper). Without
   a local board the ceremony is exactly today's (pre-validate →
   `SetBoard` → adoption wait). Rebind refusal (`BoardAlreadyBound`)
   unchanged.

8. **`stacks add` is the minimum verb that makes stand-alone
   usable** (WI-3 owns the rest of the stack/label verbs). Clap group
   `Stacks` with `Add { title, --order }`; order appends by default
   (`max(order) + 1` across the board's live stacks, canonical
   order); executes `CreateStack` (which requires the live board —
   board-less dbs surface the engine's `NoBoard`, mapped to the
   existing `no-board` kind). Receipt: `StackReceipt { id, title,
   order }`.

9. **Typed `SyncDisabled` is the uniform refusal**, exit 1, kind
   `sync-disabled`, for: `sync`, `daemon`, `boards list`,
   `boards select`, `boards publish` under resolved `standalone`, and
   `login` under *explicit* standalone (§4.2). The checks run at
   handler entry (after config load, before any core boot or client
   construction). Rationale: these verbs' entire purpose is the
   connector; failing loudly beats hanging or silently doing local
   work. (`tasks *`, `boards create`, `stacks add` are
   backend-agnostic and never refuse.) Note the status-backlog
   implication resolves itself: under standalone a core boot performs
   no network I/O by construction.

10. **`pending_ops` keeps climbing under standalone — honest by
    design.** Mutations enqueue ops; nothing drains them until a
    connector attaches. The JSON `SyncStatusView` stays untouched
    (phase idle, `pending_ops` N). Human `tasks list` under
    standalone appends one informational line ("stand-alone: N
    changes waiting for a sync connector") sourced from config +
    state — presentation-only, no domain change, no log-text
    assertions (structural JSON assertions unchanged).

11. **No `SyncPhase`/`SystemEvent` changes.** Idle is the correct
    phase for a connector-less core; no broadcast events fire. The
    engine's closed-channel behaviors are already pinned by its
    engine-only tests (§1).

## 5. Normative Shapes (signatures binding; bodies indicative)

### 5.1 Domain (`messages.rs`, `command.rs`, `state.rs`)

```rust
// messages.rs — StateCommand grows:
/// Create the single local board (refuses when a live board exists).
CreateBoard {
    /// Board title.
    title: String,
    /// Board color (lenient; validated at the CLI boundary).
    color: Color,
},
/// Attach the local board to an existing remote board (pull merges by
/// this binding; the outbox then drains onto it).
BindBoard {
    /// The remote board to bind to.
    remote: RemoteBoardId,
},

// command.rs — outcomes/errors grow:
pub enum CommandOutcome {
    /* … */
    /// The command created the board with this id.
    CreatedBoard(BoardId),
}
pub enum CommandError {
    /* … */
    #[error("a live board already exists")]
    BoardExists,
    #[error("the board is already bound to a remote board")]
    BoardAlreadyBound(RemoteBoardId),
}
```

Proptest: `test_support::messages` strategy generates the two new
variants (random titles/colors/remote ids); the
`plan_command_is_total_and_ok_plans_are_self_consistent` property
then covers them without new harness code.

### 5.2 Config (`config.rs`)

```rust
/// `[sync] backend`: which connector drives sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncBackend {
    /// No connector: fully local operation (fresh-install default).
    Standalone,
    /// The Nextcloud Deck connector (first implementation).
    Nextcloud,
}

impl SyncSection {
    /// Raw optional `[sync] backend`; `None` = auto.
    pub backend: Option<SyncBackend>, // field, #[serde(default)]
}

impl AppConfig {
    /// The effective backend: explicit value, else auto — `nextcloud`
    /// iff `nextcloud.server_url` is present, else `standalone`.
    #[must_use]
    pub fn sync_backend(&self) -> SyncBackend { /* … */ }
}
```

### 5.3 Bootstrap (`bootstrap.rs`)

```rust
// App's sync half becomes optional; everything else is unchanged.
struct SyncActor {
    commands: mpsc::Sender<SyncCommand>,
    join: JoinHandle<()>,
}

pub struct App {
    /* … */
    sync: Option<SyncActor>,   // None under backend = standalone
}

impl App {
    /// The sync-command seam. Under standalone the receiver is
    /// dropped: sends fail closed (`Closed`) exactly as a dead
    /// connector — callers must not rely on delivery.
    #[must_use]
    pub fn sync_commands(&self) -> mpsc::Sender<SyncCommand> { /* … */ }
}
// shutdown(): `if let Some(sync) = self.sync { join_bounded(sync.join, …) }`
```

### 5.4 CLI (`cli.rs`, `commands/`)

```rust
pub enum BoardsCmd {
    List,
    Select { board: u64, #[arg(long, default_value_t = 60)] timeout_secs: u64,
             /// Bind the existing local board onto the remote board
             /// (migration path for stand-alone data).
             #[arg(long)]
             bind_local_board: bool },
    /// Create the local board (stand-alone first-run command).
    Create { title: String, #[arg(long)] color: Option<String> },
    /// Publish the local board as a new Nextcloud board and bind it.
    Publish { #[arg(long, default_value_t = 60)] timeout_secs: u64 },
}

pub enum StacksCmd {
    /// Create a stack on the board (appended by default).
    Add { title: String, #[arg(long)] order: Option<i64> },
}

// commands/mod.rs — CommandError grows:
#[error("no sync connector configured")]
SyncDisabled,
#[error("a local board exists — pass --bind-local-board or use boards publish")]
LocalBoardExists,
// error_kind(): "sync-disabled", "local-board-exists",
//                "board-exists", "board-already-bound" (domain table)
```

### 5.5 Output (`commands/output.rs`)

```rust
pub struct BoardReceipt { pub id: String, pub title: String, pub color: String }
pub struct StackReceipt { pub id: String, pub title: String, pub order: i64 }
// SelectReceipt/PublishReceipt reuse BoardsListEntry + SyncStatusView
// (publish reports the created remote board + the cycle outcome).
```

## 6. Worktree, Branch & TDD Implementation Order

Branch `feat/standalone-sync-connector`, worktree
`../taskboard-WT-feat-standalone-sync-connector` (from `main`).
One PR. TDD order — commit per step, every commit passing the full
gate (§9):

1. **Domain commands** (`test/domain-create-board-bind` content, on
   the branch): `CreateBoard`/`BindBoard` arms + outcome/error
   variants + strategies; unit tests (create-happy, `BoardExists`,
   bind-happy, `NoBoard`, `BoardAlreadyBound`, no-op repeat semantics:
   `BindBoard` twice → second is a rejection? — no: repeat bind to the
   *same* remote is an accepted no-op; bind to a different remote is
   `BoardAlreadyBound`) and the totality proptest green.
2. **State engine**: A/S-series extensions — `CreateBoard` →
   `CreatedBoard`, persisted (`UpsertBoard` in the fake repo's
   recorded batch), published (`EngineSignal::StateUpdated`);
   `BindBoard` mutates the board row; snapshot test: create → stack →
   task → bind sequence pinning the resulting `AppState` tree.
3. **Config**: `SyncBackend` + `backend` field + `sync_backend()`
   accessor + validate gating (C-series table: auto-nextcloud via
   file and via `TASKBOARD_NEXTCLOUD__SERVER_URL`; explicit standalone
   ignores missing credentials but still rejects a malformed present
   URL; `deny_unknown_fields` rejects a typo'd backend value with
   `Decode`).
4. **Bootstrap**: standalone shape — no credential store consulted,
   engine+storage spawn and serve `CreateBoard`; shutdown clean with
   `sync: None`; existing B-series nextcloud tests untouched-green
   (they carry `server_url` → auto-nextcloud).
5. **CLI standalone verbs (ST-series, hermetic sqlite, no wiremock)**:
   `boards create` (receipt; second create → `board-exists` kind);
   `stacks add` (append order; board-less db → `no-board`);
   `tasks add/list` over a stand-alone db end-to-end; `sync`/`daemon`/
   `boards list` refusals (`sync-disabled`); human `tasks list`
   stand-alone note line (text presence checked as *structure*: the
   note is printed to stdout — assert the line count/shape, not
   wording? No: assert the fact via a dedicated output function's
   boolean, keeping the no-text-assertion rule).
6. **`login` gating**: explicit-standalone refusal (typed); auto path
   unchanged (L-series stays green).
7. **Attach ceremonies (V-series, wiremock)**: `select
   --bind-local-board` (pre-validation, `BindBoard`, `SetBoard`,
   adoption wait, union outcome visible in local state: remote stack
   adopted alongside the pushed local one); refusal without the flag
   (`local-board-exists`); `publish` (create_board mock → bind →
   settle; color fallback arm; `NoBoard`/`BoardAlreadyBound` arms);
   both under explicit `nextcloud` config.
8. **J4 docker journey** (`it_nextcloud_docker_cli_standalone_publish`
   in `tests/it_cli.rs`): fresh db, backend standalone → `boards
   create` + `stacks add` + `tasks add` offline; `login`; `boards
   publish --json` → `Succeeded`; server-side truth via raw
   `DeckClient` (board, stack, card exist; card title matches);
   `tasks list --json` still shows the local ids (binding preserved).
9. **Docs & bookkeeping** (§8): ADR 0010, architecture.org, roadmap
   amendment, README, CHANGELOG, backlog appends; full gates + tier-2
   run + the §9 stand-alone runbook.

## 7. Out of Scope → Backlog (entries appended with this plan)

- **Title-match reconciliation for attach** — binding unions by
  identity; attaching onto a remote board with overlapping names
  duplicates those entities. Revisit only if real usage hits it
  (mitigation: `publish` onto a fresh board).
- **Board reset/delete ceremony** — deleting a local board (and its
  content) is currently "new db path"; a `boards delete` with
  cascade + confirmation is its own destructive-design question.
- (Existing entries stay: local board-edit commands + `Board` clocks;
  multi-board; encrypted-file credential store; duplicate-create
  crash-window reconciliation — publish's mid-ceremony orphan is an
  instance of it.)

## 8. Docs & Bookkeeping (part of this phase's PR)

- **ADR 0010 — "Stand-alone-first: the optional sync connector"**:
  the product-shape decision (user 2026-10-10) and its M2 framing;
  backend resolution (explicit-or-auto, `server_url`-keyed loudness);
  why the engine needed nothing (closed-channel contracts); `CreateBoard`
  without an outbox op and `BindBoard` as the attach primitive
  (bind-before-pull merge proof, pipeline.rs board matching); union
  semantics of attach + the publish mitigation; the wire word
  "standalone" vs the sketch's "none"; connector-selection as a match,
  not a trait (decision 5). Status: Accepted.
- **`docs/architecture.org`**: replace the implicit
  sync-is-mandatory framing in the overview with
  "stand-alone by default, sync when configured"; config section gains
  `[sync] backend`; the bootstrap description notes the conditional
  connector spawn.
- **Roadmap master plan** (`.artifacts/plans/2026-10-08-growth-roadmap-plan.md`):
  insert "Phase 6b — Stand-alone-first (optional sync connector)"
  between Phase 6 and Phase 7 with a one-paragraph summary and a link
  to this plan; note the offline-first reframing in §2 decisions.
- **README.org**: quickstart becomes stand-alone-first:
  `boards create` → `stacks add` → `tasks add` → `tasks list` works
  with zero network/credentials; the Nextcloud section follows
  (`login` → `boards list` → `publish` or `select`).
- **CHANGELOG** `[Unreleased]` → Added: stand-alone mode, `boards
  create/publish`, `stacks add`, `--bind-local-board`; Changed:
  fresh installs default to stand-alone (configs with
  `[nextcloud] server_url` unchanged).
- **Backlog**: append §7 entries.

## 9. Verification & Acceptance

- Full quality gate (AGENTS §10): fmt, clippy `-D warnings`, nextest,
  doc tests, `cargo deny check`, `cargo llvm-cov --workspace`
  (`SQLX_OFFLINE=true`; the new ST/V-series must keep the app crate's
  floor at ≥ its current 80.3% lines — new code arrives tested).
- Tier 2: `eval "$(scripts/nextcloud_it_setup.sh up)" && cargo nextest
  run -p taskboard-app --run-ignored only -E 'test(it_nextcloud_docker)'`
  green including J4; teardown after.
- **Stand-alone runbook (phase exit, user-run)**: on a clean
  machine/profile with **no keyring available and no network**:
  (1) `taskboard boards create "Personal" --json` → receipt;
  (2) `taskboard stacks add "Todo" --json`; (3) `taskboard tasks add
  "first task" --json` → exit 0 (durable locally);
  (4) `taskboard tasks list` shows board/stack/task + the stand-alone
  note; (5) `taskboard sync` → exit 1, kind `sync-disabled`;
  (6) with network + keyring: `taskboard login`, `taskboard boards
  publish --json` → `Succeeded`; the board/stack/task appear in the
  Nextcloud web UI; (7) a card created in the web UI arrives via the
  daemon/`sync` cycle. Acceptance: user confirms.

## 10. Consumer Handoff Map

| Consumer | Takes from Phase 6b |
|---|---|
| WI-3 (CLI completeness) | `stacks add` pattern + `Stacks` clap group to grow; `resolve_task` selectors land next to `resolve_stack`; the standalone test harness (no wiremock) for every new verb |
| WI-4 (REPL) | The `with_core` bodies it extracts now include board/stack verbs; standalone REPL needs no connector ceremony |
| WI-5 (daemon IPC) | `SyncDisabled` as the model for typed connector-absent replies over IPC |
| Phase 7 UI | First-run without any server: the UI mounts a stand-alone core; `publish`/`select --bind-local-board` are the settings-screen sync-enable actions; `pending_ops` + backend flag feed the sync-status widget |
| Future connectors | The vocabulary-only port + the app-side match are the seam to extend (`taskboard-sync-<name>` + one match arm + config value) |

## 11. Risks & Mitigations

- **Union duplication on `--bind-local-board`** (identity vs titles):
  documented semantics, `publish` as the ambiguity-free path, backlog
  entry for reconciliation. J4 proves the publish path against a real
  server.
- **Publish mid-ceremony orphan** (crash between `create_board` and
  `BindBoard`): the retry publishes a second board; runbook documents
  manual cleanup; the general duplicate-create window is an existing
  backlog entry — not widened by this plan (the binding is local and
  idempotent to re-run).
- **Auto-resolution surprises** (a stray `server_url` flipping a
  standalone box to nextcloud): loud by design — validation then
  demands the username and credentials, so the failure is typed and
  immediate, never a silent network attempt; ADR documents the rule.
- **Stale `pending_ops` growth on long-lived standalone installs**:
  honest (they *are* pending); the UI note explains it; a tombstone/
  op-prune story stays out of scope.
- **`sync_commands()` misuse under standalone** (sending `SetBoard`
  into a closed channel): the ceremonies refuse at handler entry
  (decision 9); the seam's fail-closed contract is doc-pinned in 5.3.
- **Scope creep** (board rename/delete, logout flip, second
  connector, title matching): each triaged to §7/backlog; the domain
  additions are exactly two commands.

# [Plan] GUI Review Round 2 — Chunk 6: Connection management rework (D32–D34)

Stage-1 plan for `feat/ui-r2-connection-management` (dispatch plan §7, prompt
6.1). Normative input: the design round
`.artifacts/plans/2026-10-23-gui-review-round2-plan.md` §7 (D32–D34), facts
F-C7/F-C8/F-C9/F-D1–F-D4, risk R4, test inventory §1.7; the dispatch plan
§1 (shared execution model + code-quality mandate) and §7 (this chunk).
Base: `origin/main` @ `ff7e8f4` (chunk 5 merged — PR #432 — precondition
verified 2026-10-25). All line numbers below are this base's.

**Scope guard**: zero diffs over `taskboard-domain`, `taskboard-state`,
`taskboard-storage-sqlite`, `taskboard-sync-nextcloud` (design §1.6 — every
dispatched command exists); `cli_connect.rs` and the secret/redaction pins
pass untouched; no schedule/pull-semantics changes; the secret path (F-C9)
untouched. `ui_settings.rs`'s A-family *scenarios* all survive; the
list-carrying assertions widen to the reply pair (§3.3 — the design itself
changes that payload), recorded so nobody calls the widening a weakening.

## 1. Verified facts (anchored, post-chunk-5 `main`; worktree line numbers)

- **F1 The engine-command route exists and is already handed to the lane.**
  `ConnectionWorkspace { config_write_path, store, control, engine }`
  carries `engine: EngineHandle`
  (`crates/taskboard-app/src/ui_requests.rs:56-66`), constructed by both
  UI lifecycles (desktop `crates/taskboard-app/src/ui.rs:330-338`, kiosk
  `crates/taskboard-app/src/main.rs:509`). `EngineHandle::execute`
  (`crates/taskboard-state/src/actor.rs:130-143`) awaits a typed
  `Result<CommandOutcome, ExecuteError>` — the receipt implies durability.
  This is the *same* method the REPL bind path uses
  (`crates/taskboard-app/src/commands/boards.rs:705-712`:
  `app.engine.execute(StateCommand::BindBoard { .. }).await?`). The
  forwarder task already runs under tokio, so awaiting `execute` there is
  lane-legal.
- **F2 BindBoard / UnbindBoard shapes** (`crates/taskboard-domain/src/messages.rs:139-148,
  242-250`): `BindBoard { board: BoardId, remote: RemoteBoardId, connector:
  ConnectorId }`; `UnbindBoard { board: BoardId }` — already-unbound is an
  accepted no-op, tombstoned boards refuse. Engine refusal classes
  (`crates/taskboard-domain/src/command.rs:57-107`): `UnknownBoard`,
  `BoardDeleted`, `BoardAlreadyBound { board, bound }`, `RemoteBoardInUse
  { remote, board }`. Outcomes: `Bound { wiped_refs }`, `Unbound {
  stranded_ops }`, `NoOp` (`command.rs:169-193`).
- **F3 The connector lifecycle primitives** (`crates/taskboard-app/src/bootstrap.rs:141-336,
  646-667`): `ConnectorControl::{attach_connector(name, client),
  detach_connector_named(id), connector_commands(id), connector_attached()}`
  with `AttachError { ConnectorNameInUse, Engine, Draining, Credential,
  Client }` and idempotent `DetachOutcome`. `connector_commands` is the
  "is this name attached right now" lookup.
- **F4 The save ceremony's shape** (`crates/taskboard-app/src/ui_requests.rs:291-354`):
  read-raw → legacy refusal → authoritative re-validation →
  `verify_credentials` → overlay upsert → `store_and_rotate_secret` →
  detach-first → `DeckClient::new(draft credentials)` → `attach_connector`
  → `wait_for_boot_pull(&engine, SAVE_SETTLE)` → `Ok(current_connectors)`.
  Mappings: `map_config_error`/`map_ceremony_error`/`map_attach_error`/
  `map_credential_error`/`map_io_error` (`ui_requests.rs:485-566`).
  `RemoveConnection` = legacy refusal → overlay remove → best-effort named
  detach (`let _ =`) → best-effort secret delete → fresh list
  (`ui_requests.rs:360-390`); bindings survive Remove by recorded
  semantics.
- **F5 Boards carry their binding**: `Board.remote: Option<RemoteBoardBinding>
  { connector: ConnectorId, board: RemoteBoardId }`
  (`crates/taskboard-domain/src/entities.rs:351-369`, `ids.rs:131-137`).
  Live-board rule: `!deleted && !archived` (`presentation.rs:372-381`,
  `live_boards`). The settings board list must use the same rule so the
  surface and the selector agree.
- **F6 The remote-id problem**: D33's `AttachConnector { board, connector }`
  payload carries no remote id, but `BindBoard` needs one. The REPL's
  resolve-or-mint ceremony is *remote-id-first* (`select_prelude` lists
  boards, the operator names one; `commands/boards.rs:787-1010`). The
  settings flow is *local-board-first*, so the handler derives the remote
  id itself (decision D6-2 below).
- **F7 The settings UI state machine (post-chunk-5)**: `SettingsVm { draft,
  rows, liveness, pending: Option<PendingKind>, replies, outcome, detail,
  preset }` (`crates/taskboard-ui-slint/src/adapter.rs:1030-1047`); the
  single `pending` gate disables every settings op while one is in flight
  (`PendingKind { Test, Save, Remove, Preset, Scale }`,
  `adapter.rs:1000-1007`); the reply drain polls `try_recv` under the
  repaint timer (`adapter.rs:1163-1254`); the outcome choke point
  `set_settings_outcome` drives the auto-clear timer
  (`adapter.rs:1261-1298`); `settings_status::status_text(outcome, detail)`
  is the totality-pinned presentation mapping
  (`crates/taskboard-ui-slint/src/settings_status.rs:30-122`); the mock
  clock drives the timers in journeys via `driver.advance_time`
  (`tests/journeys.rs:2485-2535`).
- **F8 The redaction pins** (`src/requests.rs:168-211` proptest,
  `crates/taskboard-app/src/ui_requests.rs:1141-1155` request-Debug pin,
  `tests/journeys.rs` the save-ceremony journey's Debug pin): all pass on
  this base and must pass **untouched**.
- **F9 The secret-free row type** `ConnectorInfo { name, server_url,
  username, read_only }` (`src/requests.rs:36-44`) rides `UiSettings.
  connectors` (mount seed, `lib.rs:83`, filled at `ui.rs:341` via
  `resolve_connector_infos`, `ui.rs:230-245`) and every List reply.
- **F10 `SyncBackend { Standalone, Nextcloud }`** (`crates/taskboard-app/src/config.rs:184-189`);
  connector entries are Nextcloud-only in practice — the Save ceremony
  hardcodes `backend: SyncBackend::Nextcloud`
  (`ui_requests.rs:468`), and the UI form carries no backend field at all.
- **F11 The test harness surface**: journeys' `Harness::mount_with_settings`
  (`tests/journeys.rs:59-76`) mounts through the public `mount()` with a
  `UiSettings` value and drains the request lane (`drain_requests`,
  `journeys.rs:87-91`); replies are synthesized by the test and settled
  via `UiHandle::drain_setting_replies` (`lib.rs:422`). App-side,
  `ui_settings.rs` boots a real standalone core per test and drives the
  real forwarder over wiremock (`tests/ui_settings.rs:70-111`).

## 2. D32 — the board-scoped connection surface

### 2.1 Lane types (recorded)

`crates/taskboard-ui-slint/src/requests.rs`:

```rust
/// One local board for the settings board selector — secret-free,
/// id in the uuid hand-over form (the stack-widths precedent: the
/// domain newtypes carry no serde, and the adapter converts at the seam).
pub struct BoardInfo {
    pub id: uuid::Uuid,
    pub name: String,
    /// The connector name this board is bound to, `None` when unbound.
    pub bound_connector: Option<String>,
}

/// The refreshed boards+connectors pair every state-changing settings
/// reply carries (D32): rows and the selector re-render from the same
/// reply, so neither surface goes stale (F18).
pub struct ConnectionsSnapshot {
    pub boards: Vec<BoardInfo>,
    pub connectors: Vec<ConnectorInfo>,
}

pub enum UiRequest {
    // …existing variants…
    /// Bind the named local board to the named connector, attaching the
    /// connector if needed (D33). The remote id is derived app-side.
    AttachConnector {
        board: BoardId,
        connector: String,
        reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
    },
    /// Unbind the board from its connector; the connector's actor stops
    /// only when no other live board remains bound to it (§4.2).
    DetachConnector {
        board: BoardId,
        connector: String,
        reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
    },
}
```

Decisions recorded:

- **D2-1 BoardId on the lane.** The lane rule is "domain newtypes are fine,
  no serde" (`requests.rs:8`); `StackWidthsChanged` already carries
  `BoardId`. `BoardInfo.id` stays `uuid::Uuid` because it crosses the
  `UiSettings` hand-over too, where the uuid-keyed shape is the house
  precedent (F9/F1 of round-1 D16). The adapter converts once at the seam
  (`BoardId::from_uuid` — the same conversion `install` already runs for
  widths, `adapter.rs:1668-1679`).
- **D2-2 The pair replaces the bare list on the state-changing replies.**
  `SaveConnection` and `RemoveConnection` replies become
  `Result<ConnectionsSnapshot, SettingsError>`. `TestConnection` keeps
  `Result<(), SettingsError>` — it mutates nothing, so there is nothing to
  refresh; "every settings reply" (D32) is read as every reply that can
  change the surface. Keyed/preset/scale replies unchanged.
- **D2-3 New `SettingsError` variants, totality-pinned.**

  ```rust
  pub enum SettingsError {
      // …existing 13 variants…
      /// No live, unclaimed remote board on this connector matches the
      /// local board's title — there is nothing to adopt.
      NoRemoteMatch,
      /// The engine refused the bind/unbind (board/remote already bound,
      /// stale board id).
      BindRefused,
  }
  ```

  `settings_status.rs` is the compile-pin: `error_text` gains two arms,
  the proptest generator + `variant_name` gain two entries, the
  `all.len()` pin moves 13 → 15 (`settings_status.rs:47-73, 169-197,
  243-303`). Engine-gone maps to the existing `Gone`; storage failure to
  the existing `AttachFailed` (mapping table §4.5).

### 2.2 `UiSettings` extension and refresh points (recorded)

- `UiSettings` gains `pub boards: Vec<requests::BoardInfo>` (`lib.rs:59-87`,
  `Default` → empty). Desktop lifecycle fills it in the same statement that
  fills `connectors` (`ui.rs:341`): `ui_settings.boards = resolve_board_infos(&app.engine)`.
  Kiosk likewise (`main.rs:509` region). New pure helper beside
  `resolve_connector_infos` (`ui.rs:230-245`):

  ```rust
  pub fn resolve_board_infos(engine: &EngineHandle) -> Vec<BoardInfo>
  ```

  — `engine.shared_state().load()`, `presentation::live_boards`-equivalent
  filter (`!deleted && !archived`, F5), `remote.as_ref().map(|b| b.connector.to_string())`.
  App-side unit-tested over a hand-built `AppState` (no engine needed —
  take `&AppState` internally, `&EngineHandle` wrapper only).
- Mount refresh point: `install` seeds `SettingsVm { rows, boards, .. }`
  (`adapter.rs:1707-1714`); the first `push_settings` lands both.
- Reply refresh point: the drain's list arm swaps **both** VM lists from a
  `ConnectionsSnapshot` (`adapter.rs:1185-1200` — the `PendingReply::List`
  payload type changes to `Result<ConnectionsSnapshot, SettingsError>`).
  No speculative polling; rows/selector move only at mount and replies
  (review trap (e)).

### 2.3 Board selector — widget choice (recorded: **row list, not BoardBar**)

D32 offers BoardBar language vs a compact list; stage-1 picks the **list**:

- The settings column already owns a row-card language (`ConnectionRow`
  cards with `SettingsButton`s, `settings-view.slint:240-283`); the board
  selector reuses it — visual consistency inside one view.
- `BoardBar` is the board *switching* idiom (tab role, `segment-activated`
  routes view-state board selection); a second tab-strip inside settings
  would teach two meanings for the same widget. Its segments also carry no
  badge slot for bound/unbound.
- Anatomy: one row per live board (`SettingsBoardRow` slint struct: `id:
  string`(uuid), `name: string`, `bound-connector: string`, `bound: bool`,
  `selected: bool`, `busy: bool`). Click selects (radio semantics, adapter
  VM field `selected_board: Option<uuid::Uuid>`); the binding badge is a
  `RowDot`-family status element + the connector name text (bound) or
  nothing (unbound); a bound row carries the **Detach** button (§5.3).
  No board-create/board-delete here — the board surface owns lifecycle
  (chunk 3); the selector is a *binding* surface.

### 2.4 Connector-row anatomy (recorded)

`ConnectionRow` gains `busy: bool` (spinner marker, §5). Per-row controls:

- **Test** — unchanged semantics, `enabled: !row.read-only`.
- **Bind-to-this-board** — new button. `enabled: !row.read-only &&
  SettingsModel.selected-board != "" && no-op-pending` (the pending gate
  renders through `save-enabled`-style scalars, §5.2). Click routes
  `attach-clicked(row-index)` → adapter resolves row name + selected board
  id → `AttachConnector`. *Not* disabled when the selected board is bound
  to this same connector: re-issue is defined, idempotent behavior
  (re-attach after Remove, engine `NoOp` — D4-3). Bound to a *different*
  connector: the request goes out and the engine refuses
  (`BoardAlreadyBound` → `BindRefused`); explicit Detach is the only road
  (no destructive auto-rebind).
- **Remove** — unchanged semantics (D33: "Remove stays config-level
  (overlay remove + detach), unchanged semantics"). It already detaches
  the attached connector first (`remove_connection`, F4) — the prompt's
  "Remove on an attached connector detaches first" is the *existing*
  behavior, re-pinned here, test unchanged. Bindings survive Remove; the
  board badge keeps showing the stale connector until Detach (recorded;
  the badge is the honest state, `remove_connection`'s doc says so).
- Badges: backend badge renders from the row's configured backend
  (all Nextcloud today, F10) as a static `RoleText` next to the name —
  structural, never a text-assert target.

### 2.5 Backend picker (recorded)

The add-connection form gains a `SegmentedControl` (chunk 5's widget,
`ui/widgets/segmented-control.slint`) with `options: ["Nextcloud",
"Standalone"]` and a new `in property <[bool]> segment-enabled` defaulting
all-true (backward compatible — the keymap picker passes nothing). The
settings instance passes `[true, false]`: **Nextcloud enabled+selected,
Standalone a disabled "coming soon" segment** (D32). `picked` from a
disabled segment is swallowed by the widget (the `SettingsButton`
`enabled` gate precedent). No lane payload carries a backend (F10: the
Save ceremony hardcodes Nextcloud) — the picker is presentational truth.
Structure-asserted only: segment count, enabled flags, selected index;
never the option strings (R5). Widget extension gets its own
`usability_components.rs` rows (§7).

## 3. D33 — the attach/detach app-side choreography

### 3.1 Attach handler (recorded order)

`crates/taskboard-app/src/ui_requests.rs`, new `async fn
attach_connection(workspace, board_id, connector_name) ->
Result<ConnectionsSnapshot, SettingsError>`:

1. **Resolve the connector config** — `read_raw_config(workspace)` (F4's
   re-read rule), legacy refusal, find the `ConnectorCfg` by name; miss →
   `Gone` (a stale form naming a removed connector).
2. **CredentialStore load** — `workspace.store.load(server_url, username)`
   → `map_credential_error` (`NoSecretStored` the common refusal). The
   stored secret never leaves the handler scope.
3. **Derive the remote id (D6-2).** Read the board row from
   `workspace.engine.shared_state()`:
   - Board unknown/deleted → `Gone` (stale selector).
   - `board.remote` is `Some(b)` **and** `b.connector == connector_name`
     → idempotent path: `remote = b.board`, **skip the listing** (nothing
     to discover; no network call — wiremock-proven, §7-A5).
   - `board.remote` is `Some(_)` for a different connector → refuse
     `BindRefused` *before any IO* (the engine would refuse; failing
     early keeps the story total).
   - Unbound → **fresh listing verify-and-resolve**: build
     `DeckClient::new(server_url, username, stored)` and call `boards()`
     — the same call `verify_credentials` makes (F-D3's "verify if
     needed": the listing *is* the verification), mapped through
     `map_ceremony_error`. Match rule (recorded): **exact title equality
     with the live local board's title, over live remote boards
     (`!archived && is_live()` — `select_prelude`'s filter), lowest
     remote id wins on duplicates** (deterministic, stable; same-title
     Deck boards are an accepted world — `ReconcileByTitle` exists for
     them). No match → `NoRemoteMatch`.
4. **Ensure attached** — `workspace.control.connector_commands(&id)`
   (F3): `Some` → already attached, skip (recorded: unconditional
   `attach_connector` would refuse `ConnectorNameInUse` on a *healthy*
   attached connector and surface the misleading AttachBusy text; the
   lookup primitive exists precisely for targeted decisions). `None` →
   `attach_connector(id, client).await` → `map_attach_error`.
5. **Bind through the engine command path** — `workspace.engine.execute(
   StateCommand::BindBoard { board, remote, connector: ConnectorId::from(name) })`.
   `EngineHandle::execute` at `taskboard-state/src/actor.rs:130`, handle
   at `ui_requests.rs:65`, constructed at `ui.rs:337`/`main.rs` — the
   sender exists at the call site (dispatch prompt's proof demand); the
   same engine-command route the REPL bind uses (F1). Map `Err`:
   `Rejected(BoardAlreadyBound | RemoteBoardInUse)` → `BindRefused`;
   `Rejected(UnknownBoard | BoardDeleted)` → `Gone`;
   `Storage(_)`/`EngineGone` → `AttachFailed`/`Gone` (full table §4.5).
   `Ok(Bound{..}) | Ok(NoOp)` both proceed.
6. **Settle** — `wait_for_boot_pull(&workspace.engine, SAVE_SETTLE)`
   (`bootstrap.rs:1074`, already `pub(crate)` and used by this file at
   `ui_requests.rs:352`): the first pull matches by the fresh binding and
   merges (F-D3); bounded, offline-first.
7. **Reply** — `Ok(current_connections(workspace))` (the pair builder,
   §3.3). `tracing::info!` at start (before step 3) and success (before
   the reply), fields per D6-5.

**Failure/rollback story (recorded — the prompt's question answered):**
*attach ok but bind refused ⇒ no detach rollback; the connector stays
attached.* Reason: the connector is in the config, and boot's steady state
for an in-config connector is *attached* (F-D3: boot attaches the
configured set) — rolling back would fight the config the request did not
change, and the actor is live-but-boardless only until the next
bind/save. The bind refusal is typed, final for this request, and visible
(D28 machinery). This mirrors the REPL session's opposite choice (there
the core is ephemeral and shutdown owns teardown). Detach-side failure
after UnbindBoard (actor stop times out) is best-effort `let _ =` — the
binding is already gone; the reply is `Ok` (remove_connection precedent,
F4).

### 3.2 Detach handler (recorded order + one recorded refinement)

`detach_connection(workspace, board_id, connector_name)`:

1. Resolve config + legacy refusal (as above); board unknown/deleted →
   `Gone`.
2. **`workspace.engine.execute(StateCommand::UnbindBoard { board })`**
   first — typed and authoritative: refusal (`BoardDeleted` tombstone
   rule) → nothing has been touched; `Unbound{..} | NoOp` (already
   unbound is accepted, messages.rs:242-250) → proceed.
3. **Conditional actor stop (recorded refinement of D33's literal
   order):** re-read state; if **no other live board remains bound to
   this connector**, `let _ =
   workspace.control.detach_connector_named(&id).await` (idempotent,
   best-effort — the F4 precedent); else leave the actor running.
   *Reason recorded:* D33 pins "`detach_connector_named` +
   `StateCommand::UnbindBoard`", but a board-scoped detach that stops a
   shared actor would silently kill sync for every *other* board bound to
   the same connector — contradicting the chunk's own board-scoped thesis
   (D32) — and doing the untyped step *first* risks a stopped actor plus
   a surviving binding when the engine refuses. In the single-board case
   (the design's mental model) both orders land identically. The
   multi-board guard is wiremock-free unit territory (§7-A7).
4. **No `wait_for_boot_pull`** — the UnbindBoard receipt already implies
   durability (F1) and no pull is awaited by the design for detach (D33
   names the settle only for attach).
5. Reply the pair; start/success info logs (D6-5).

### 3.3 The pair builder

`fn current_connections(workspace) -> ConnectionsSnapshot` beside
`current_connectors` (`ui_requests.rs:448-456`): boards from
`workspace.engine.shared_state().load()` (live filter, binding name),
connectors from the existing `current_connectors` body. `SaveConnection`
/`RemoveConnection`/`AttachConnector`/`DetachConnector` all reply it.

### 3.4 `handle_settings` dispatch

Two new arms (`ui_requests.rs:244-267`), same fire-and-forget-honest
reply discipline (`let _ = reply.send(..)`). The forwarder's
never-debounce rule already covers them (settings requests execute on
the spot, `ui_requests.rs:101-103`).

## 4. D34 — the visible ceremony

### 4.1 Busy state machine (recorded)

- **One op at a time — the existing gate, extended, not replaced**
  (dispatch acceptance (d): "the `pending` gate reused").
  `PendingKind` gains `Attach`, `Detach` (`adapter.rs:1000-1007`).
  `SettingsVm` gains `busy_target: String` — the connector/board name the
  op addresses — set by the send seams, cleared with `pending`.
- **Push shape**: `SettingsModel` gains `busy: bool` (any op — drives the
  global button disablement together with `save-enabled`) and
  `busy-target: string`; `ConnectionRow`/`SettingsBoardRow` gain `busy:
  bool` (row == target && kind matches → spinner). The adapter resolves
  the flag in `push_settings` (F7's one write path).
- **Spinner rendering**: the row's status dot slot renders the busy state
  as `Semantic.sync-active` (the status line's busy token,
  `settings-view.slint:89-98`) with a purely decorative `animate`
  rotation for feel — *structure is the model flag; the animation is
  never asserted* (mock-clock tests assert the flag timeline, not pixels).
- **Mock-clock discipline**: pending → reply transitions are
  reply-driven (drain), so tests need no clock for the gate itself; only
  the auto-clear (existing `STATUS_VISIBLE_MS` machinery) touches the
  timer, already journey-proven (`journeys.rs:2485-2535`).
- **No concurrent second op**: every seam's `pending.is_some()` early
  return stays; the journeys pin it (§7-J4).

### 4.2 Save click-through (recorded mechanism — the stage-1 pick)

Every Save click produces a visible reaction (F21/F-C7):

- The Save `SettingsButton` gains `clicked` firing **regardless of
  enablement**: the markup always routes `save-clicked(..)` (the
  `enabled` gate moves out of the widget's `TouchArea` for this one
  button — a `visible-reaction` variant of `SettingsButton`, one new
  property `click-through: bool`, default false so every other consumer
  is untouched).
- Adapter `settings_save` reorders honestly: commit fields → if
  `pending.is_some()` → `set_settings_outcome(Rejected, None)` (visible
  "refused" + generic sentence); else if `!draft_valid` →
  `set_settings_outcome(Rejected, Some(Invalid { field, kind }))` from
  `settings_form::validate(&draft)` — the **chunk-5 mapping renders the
  exact reason sentence with zero new presentation code** (F7); else send
  (existing path). The silent-absorb paths die; no request is routed on
  a refused click (the lane stays clean — journey-pinned, §7-J5).
- The same click-through treatment goes to **Bind** (no board selected →
  `Rejected` + a dedicated mapped sentence? No — no new variant:
  `Invalid { field: Field::Name, kind: Empty }` lies. Decision: Bind with
  no selection is *disabled*, not click-through — only Save carries
  click-through, because only Save has the F21 finding. Recorded.)

### 4.3 Info logs (recorded fields)

App-side, in the four ceremony handlers (`save_connection`,
`attach_connection`, `detach_connection`, `remove_connection`):

```rust
tracing::info!(connector = %name, host = %host, "settings save started");
tracing::info!(connector = %name, host = %host, "settings save succeeded");
```

`host` is the authority extracted from `server_url` (scheme-stripped,
userinfo-stripped; a small pure `fn url_host(&str) -> &str` beside the
mappings, unit-tested). **Fields are name and host only — never the full
URL query, never username, never credentials** (D34; the redaction rule
extends to logs). Failure paths keep their existing warn/debug logs.

### 4.4 Status text for the new outcomes

`settings_status.rs` gains: `NoRemoteMatch` → adoption-miss sentence,
`BindRefused` → conflict sentence. Totality pins updated (the compile
does the chasing). The UI-side outcome fold (`SettingsVm::outcome_of`)
maps the two new variants → `Rejected` (user-input conflicts), reviewed
in §4.5.

### 4.5 The full mapping table (recorded)

| Source | SettingsError |
|---|---|
| config: connector name miss / board stale | `Gone` |
| store: `NotFound` / `Unsupported` / other | `NoSecretStored` / `StoreUnsupported` / `StoreFailed` |
| listing (`boards()`): 401/403 / unreachable / deck / cfg-write | `VerifyRejected` / `VerifyUnreachable` / `VerifyFailed` / `PersistFailed` (via `map_ceremony_error`) |
| listing: no title match | `NoRemoteMatch` |
| `attach_connector`: `ConnectorNameInUse`/`Draining` / `Engine`/`Client` / `Credential` | `AttachBusy` / `AttachFailed` / credential table (existing `map_attach_error`) |
| `execute` bind: `Rejected(BoardAlreadyBound, RemoteBoardInUse)` | `BindRefused` |
| `execute` bind/unbind: `Rejected(UnknownBoard, BoardDeleted)` | `Gone` |
| `execute`: `Storage(_)` / `EngineGone` | `AttachFailed` / `Gone` |

The A10 mapping-table test (`ui_requests.rs:1057-1136` pattern) extends
over every new arm.

## 5. UI seams (adapter + view)

- `settings_view` slint: board-selector section (rows + badges + Detach)
  between the "Connections" header and the connector list; Bind button on
  connector rows; backend picker above the form fields; busy dots.
  Callbacks: `board-selected(string)` (uuid), `attach-clicked(int)`,
  `detach-clicked(int)` (board-row index), `bind-clicked(int)`
  (connector-row index).
- Adapter seams: `settings_board_selected`, `settings_bind_row`,
  `settings_detach_row` — index → id-table-free resolution (board rows
  carry their uuid in the model; connector rows resolve by index against
  `SettingsVm.rows`, the existing pattern). Each ends in
  `set_settings_outcome(Busy/Failed)` + `push_settings`.
- `PendingReply::List` payload → `Result<ConnectionsSnapshot, _>`; the
  drain arm sets both VM lists (`adapter.rs:1185-1200`).
- `push_settings` extends with board rows + busy scalars
  (`adapter.rs:1317-1363`); the board model rides a persistent handle
  like `settings_rows` (`models.rs:477-484` slot pattern).
- `install`: seed `boards` + `selected_board` from `UiSettings`
  (`adapter.rs:1707`).

## 6. Non-goals (re-pinned)

No domain/state/sync/storage diffs; no multi-backend connectors; no
schedule/pull changes; secret path untouched (F-C9); no board
create/delete/archive in settings; no remote-board *picker* UI (the
title-adopt rule is the binding UX this chunk; a picker is
`backlog:` material if review wants one); no REPL/IPC changes.

## 7. Test catalogue (TDD-first — stage 2 lands tests with each step)

App crate — `crates/taskboard-app/tests/ui_settings.rs` (wiremock + real
boot + real forwarder; extends A6/A7/A9 patterns; scenarios preserved,
list assertions widened to the pair):

- **A1** mount/board resolution: `resolve_board_infos` over a seeded
  `AppState` — liveness filter, bound/unbound names, uuid round-trip.
- **A2** attach happy path: seeded unbound board "home" + config
  connector "work" whose wiremock listing contains live "home"; assert
  Ok pair (boards show binding), `engine` state carries the binding
  (`Board.remote.connector == "work"`), actor attached
  (`connector_commands().is_some()`), file untouched by attach.
- **A3** attach no-match: listing lacks the title → `NoRemoteMatch`,
  **nothing attached** (verify-before-attach order proof), no binding.
- **A4** attach bind-refused: board already bound to connector *a*;
  attach targets *b* with a matching listing → early `BindRefused`, no
  IO at all (the pre-IO refusal arm).
- **A5** attach idempotent re-bind: board already bound to the same
  connector, actor already attached, **no listing route mounted** (an
  unmatched wiremock request would surface as unreachable → the Ok
  outcome proves the skip) → Ok, engine `NoOp` path.
- **A6** attach with stored-secret missing → `NoSecretStored`; attach
  with 401 listing → `VerifyRejected`; persisted-nothing in both.
- **A7** detach: single bound board → unbound in state, actor stopped
  (`connector_commands().is_none()`), Ok pair. Multi-board guard: two
  boards on one connector → detach one ⇒ actor **still attached**, other
  binding intact (the recorded refinement, pinned as desired semantics).
- **A8** detach idempotent: unbound board → Ok (`NoOp`), no panic.
- **A9** dropped reply receiver on attach → forwarder survives, work
  landed (A7-pattern).
- **A10** Save/Remove replies now carry the pair (existing A6/A9
  scenarios widened); mapping-table test extended (§4.5); the
  request-Debug redaction pin **untouched** and passing.

UI crate — `crates/taskboard-ui-slint/tests/journeys.rs` (+ 
`usability_components.rs`):

- **J1** board selector: mount with `UiSettings.boards` → rows render
  (count/badges as model facts); click selects (`selected-board`); a
  reply snapshot re-renders both surfaces (F18's no-stale pin).
- **J2** bind routing: selected board + connector row Bind → exactly one
  `AttachConnector { board, connector }` on the lane with the right ids;
  synthesized Ok snapshot lands boards+rows; synthesized
  `Err(BindRefused)` lands `Rejected` + non-empty status text.
- **J3** detach routing from a bound board row; Ok snapshot clears the
  badge.
- **J4** busy gate: with an attach pending (no reply yet), Save/Test/
  second Bind route nothing; the initiating row's `busy` flag is set;
  the reply clears it (no clock needed); auto-clear interplay rides the
  existing mock-clock journeys.
- **J5** save click-through: invalid draft + click → `Rejected` outcome,
  non-empty status text, lane empty; valid-but-pending → `Rejected`,
  lane empty; valid+idle → request (existing J5 scenario keeps passing).
- **J6** backend picker: segments = 2, enabled flags `[true, false]`,
  selected 0; clicking the disabled segment emits nothing (widget test +
  settings-instance test in `usability_components.rs`).
- **Redaction pins** (`journeys.rs` Debug pin, `requests.rs` proptest):
  untouched.

## 8. Code inventory (this chunk)

| File | Change |
|---|---|
| `taskboard-ui-slint/src/requests.rs` | `BoardInfo`, `ConnectionsSnapshot`, 2 requests, 2 error variants; reply types of Save/Remove |
| `taskboard-ui-slint/src/adapter.rs` | VM fields (`boards`, `selected_board`, `busy_target`), `PendingKind`+2, 3 seams, Save click-through, drain/push/install extensions |
| `taskboard-ui-slint/src/models.rs` | `SettingsBoardRow` push + busy flags + persistent handle |
| `taskboard-ui-slint/src/settings_status.rs` | 2 texts + generator/variant pins |
| `taskboard-ui-slint/src/settings_form.rs` | none |
| `taskboard-ui-slint/src/lib.rs` | `UiSettings.boards` |
| `taskboard-ui-slint/ui/views/settings-view.slint` | selector, Bind/Detach buttons, picker, busy dots, Save click-through |
| `taskboard-ui-slint/ui/widgets/segmented-control.slint` | `segment-enabled` |
| `taskboard-app/src/ui.rs` | `resolve_board_infos`, mount fill |
| `taskboard-app/src/main.rs` | kiosk mount fill (one line) |
| `taskboard-app/src/ui_requests.rs` | `attach_connection`/`detach_connection`/`current_connections`/`url_host`, dispatch arms, mappings, info logs |
| tests | §7 catalogue |

`CHANGELOG.md` Unreleased updated in the feature commits (stage 2).

## 9. Risks

- **R-a (R4 carried) the engine-command lane from settings is new
  surface**: mitigated — the handle provably exists (F1); the REPL E2E
  suites are the unchanged-behaviour gate; the choreography is
  wiremock-hermetic. The `execute` await sits in the forwarder task, so
  a slow settle delays *later requests*, never the UI thread; the
  `SAVE_SETTLE` bound (5 s) matches the save path's existing budget.
- **R-b the title-adopt rule can bind the "wrong" same-title board** on
  duplicate remote titles: deterministic lowest-id pick is recorded; the
  engine's `RemoteBoardInUse` still guards a contested bind; a
  remote-picker UI is the recorded backlog escape hatch.
- **R-c detach-order refinement vs D33's literal text**: recorded with
  reasons (§3.2); stage-3 reviews the multi-board pin (A7) as desired
  semantics, not drift.
- **R-d `SegmentedControl` grows a property** — small blast radius
  (single widget, default preserves consumers), covered by chunk 5's
  existing widget tests plus J6.
- **R-e the widened `ui_settings` assertions invite "you changed a
  test" findings**: the design itself changes the reply payload (D32);
  the redaction/untouched set is enumerated in the scope guard.

## 10. Implementation order (TDD-first, stage 2)

1. Lane types + `settings_status` arms + mapping tests (compile-pin
   green), `resolve_board_infos` + A1.
2. App-side attach/detach handlers + A2–A8 (wiremock first), pair replies
   on Save/Remove (A10), info logs.
3. UI model/push/drain extensions + J1 (selector renders + reply
   refresh).
4. Bind/Detach seams + J2/J3; busy machine + J4; Save click-through +
   J5.
5. Backend picker (widget + J6); `usability_components` rows.
6. Full gate suite (AGENTS §10); `cli_connect.rs` + redaction pins
   verified untouched-and-green; Tier-4 manual checklist (spinner feel,
   selector feel, save-reaction feel) recorded for the stage-2 report.

**Estimated diff**: ~1,200–1,400 lines across ~12 files (≈55% tests),
no boundary-crate diffs.

# [Plan] GUI Review Round 5, Chunk 8 — Account-First Connections

- **Date**: 2026-11-07
- **Status**: PROPOSED — stage-1 plan for chunk 8 of the round-5
  dispatch. Branch `feat/app-account-connections`, cut from the
  fetched `origin/main` @ `4d762074` (the local `main` ref is behind;
  the branch is seeded off `origin/main`). `scripts/worktree-doctor.sh`
  passed pre-creation (200 G free against the 25 G floor; three kept
  worktrees, none disposable).
- **Precondition verdict (recorded)**: **DR-8 chunk 2 (#630) has
  LANDED in `origin/main`** — the files transport wiring is merged
  (changelog commit `e7fe4513`; `files_sync_spawn_decision` /
  `files_sync_spawn` / `FilesSyncStatus` live in
  `crates/taskboard-app/src/bootstrap.rs:107-200`, gated on
  `[files_sync] enabled` ∧ the default connector being Nextcloud).
  **The config-detection fallback is NOT taken** — the Files toggle is
  real (its runtime effect is restart-scoped, §7 below). The D7 human
  ruling landed 2026-11-05; no gate remains. DR-9 (boards-over-files,
  accepted 2026-11-07) explicitly lists "UI beyond the existing
  status/connection surfaces' growth" as a non-goal — it does not gate
  or reshape this chunk.
- **Origin** (normative): round plan
  `2026-11-05-gui-review-round5-plan.md` §2 row F14, §3 D7 (the
  three-layer concept model + the rejected alternatives), §4.8;
  dispatch plan `2026-11-05-gui-review-round5-implementation-dispatch-plan.md`
  §1/§9. Context: r4 plan
  `2026-11-01-ui-r4-settings-connections-plan.md` (what D39 built,
  §10 here lists what this chunk supersedes); filesync round D-SF10
  (`2026-11-05-filesync-substrate-design-round-plan.md` §Part D —
  the `[files_sync] { enabled, poll_secs? }` shape and the
  "URL/credential from the same account the Deck connector uses"
  contract this chunk must not disturb).
- **Line references** are from this branch @ `4d762074`. The dispatch
  prompt's line numbers have drifted; anchors below are verified.

## 0. What the code confirms (anchors)

- **The connections section** (`crates/taskboard-ui-slint/ui/views/settings-view.slint`):
  the board selector `BoardPicker` (`:438-507`, shared `Dropdown` +
  Detach, the r4 D39 shape — selection drives Bind-only), the flat
  connector rows (`:802-861`, dot/name/user@server/Test/Bind/Remove),
  the add/edit form (`:869-945`: backend SegmentedControl, four
  `LabeledField`s incl. Name, Save with click-through). View
  callbacks: `:569-610` (`board-selected`, `bind-clicked(int)`,
  `detach-selected`, `test-clicked(int)`, `remove-clicked(int)`,
  `save-clicked(name, server, username, password)`).
- **The adapter** (`src/adapter.rs`): `board_selector_model`
  (`:4528-4589`), `push_settings` (`:4591-4694`; selector re-derived
  per pass — the mount-refresh), the seams `settings_board_selected`
  (`:5123`), `settings_bind_row` (`:5134`, index → `AttachConnector`),
  `settings_detach_selected` (`:5173`), `settings_test_row`
  (`:5056`), `settings_remove_row` (`:5090`), `settings_save`
  (`:4992`); `PendingKind` (`:3788-3800`: Test/Save/Remove/Attach/
  Detach/Preset/Scale/Editor{…}) and the busy/status-line ceremony
  (`SettingsOutcome` + `settings_status::status_text`, the pinned
  status row at `settings-view.slint:671-686`).
- **The lane** (`src/requests.rs`): `ConnectionsSnapshot { boards,
  connectors }` (`:59-62`), `BoardInfo { id: uuid, name,
  bound_connector }` (`:48-53`), `SettingsError` (`:85-121`, incl.
  `NoRemoteMatch` — retiring) and the `UiRequest` variants
  `SaveConnection` / `RemoveConnection` / `AttachConnector { board,
  connector }` / `DetachConnector` / `TestConnection`.
- **The app lane** (`crates/taskboard-app/src/ui_requests.rs`):
  `save_connection` (`:722-791`: verify-first → overlay upsert →
  secret rotate → detach-first → attach → `wait_for_boot_pull` →
  snapshot), `attach_connection` (`:847-952`: the title-match
  adoption arm `:899-907` — `filter(title ==)` + `min_by_key(id)` →
  `NoRemoteMatch` — being retired), `detach_connection` (`:963-995`,
  the multi-board actor-stop guard), `remove_connection`
  (`:797-832`), `current_connections` (`:1125-1139`), and the typed
  mappings (`map_credential_error`, `map_ceremony_error`,
  `map_execute_error` `:1000-1018`).
- **Config + identity**: `configured_connectors`
  (`crates/taskboard-app/src/connectors.rs:170-200` — the array
  world, kebab names minted via `ConnectorId::from_kebab`; the legacy
  synthesis names one connector `nextcloud`); the overlay writers
  `overlay_upsert_connector` / `overlay_remove_connector`
  (`connections.rs:192`, `:255`); the credential key is
  `(server_url, username)` under service `taskboard`
  (`secrets.rs:26`, `KEYRING_SERVICE`) — **already account-shaped**,
  D7's premise confirmed.
- **The engine surface**: `StateCommand::BindBoard { board, remote,
  connector }` and `UnbindBoard { board }`
  (`crates/taskboard-domain/src/command.rs`, the bind guards:
  binding-scoped equality, `BoardAlreadyBound`, `RemoteBoardInUse`
  scoped per connector, the bind-wipe of foreign refs);
  `StateCommand::CreateBoard { title, color }` replies
  `CommandOutcome::CreatedBoard(id)` — **the receipt carries the
  minted id**. `DeckClient::boards()` → `Vec<Board>` with `id: u64`
  (`sync-nextcloud/src/model.rs:19`); `RemoteBoardId` wraps it.
- **Files sync**: `[files_sync]` decodes to `config.files_sync`
  (`enabled: bool`, `poll_secs`); the spawn decision binds to the
  **default connector's** identity (D-SF10). The lane has no
  files-sync variant today.

## 1. D-8.1 — the account-card information architecture

The Connections section becomes, in tree order (the walk order, §6):

1. **Sign in** — the form shrinks to three fields: **Server**,
   **Username**, **App password** + a **Sign in** button. The
   **Name field retires** (D-8.3 auto-derives the connector name; the
   user never names connectors in v1 — renaming is fenced with the
   `[[accounts]]` reshape). The backend SegmentedControl stays
   (Nextcloud enabled / Standalone disabled, unchanged). The Save
   ceremony is reused verbatim — sign-in *is* Save with a derived
   name (`settings_save` → `save_connection`); only the name argument
   changes source.
2. **Account cards** — one card per distinct `(server_url, username)`
   across the configured connectors (the grouping rule, D-8.2). A
   card renders:
   - the identity line (`username @ host`), the folded status dot
     (the `connector_row_status` fold, worst-of over the card's
     connectors), and **Remove** (the account-removal guard:
     refuses typed while any board is bound to any of the card's
     connectors — `SettingsError::BindRefused` pre-IO; the cascade is
     the fenced backlog item);
   - the **Deck** service row: present iff the card has ≥ 1
     connector; per-connector binding rows (D-8.2 collision rule);
   - the **Files** toggle row (D-8.4) — rendered only on the card
     matching the default connector's identity;
   - the **Boards on server** mapping list (D-8.5): a **Refresh**
     affordance + remote-board rows.
3. **Remote-board rows** (inside a card's Deck area): `remote title`,
   the bound local board's name (or the empty placeholder), and per
   state: bound rows carry **Disconnect**; unbound rows carry the
   **local-board Dropdown** (the chunk-3 overlay `Dropdown`, the
   `BoardPicker` grammar: picking an unbound local board = explicit
   bind) and a **Create local board** button (D-8.6).

Busy rendering stays centralized: `SettingsModel.busy` +
`busy_target` (now also naming accounts/remote titles) drive the
per-row `BusyDot` exactly as today; the pinned status line carries
every outcome. **No new status machinery.**

## 2. D-8.2 — the grouping rule

- **Key**: exact `(server_url, username)` string equality, no URL
  normalization (no trailing-slash folding, no case folding) — v1;
  the config file is the identity authority and the credential store
  keys on the exact pair (`store.load(&server_url, &username)`).
  Normalization would silently split or merge stored credentials.
  Recorded leniency gap: `https://x/` and `https://x` land as two
  cards with the same keyring entry — visible, typed, and the user
  can Remove one; a config-file fix, not a UI heuristic.
- **Order**: first appearance in config file order (stable across
  replies; no sorting churn).
- **Name collisions (two connectors, same identity)**: legal in the
  config world (`configured_connectors` does not dedupe). The card
  shows **one Deck row per connector**, labelled with the connector
  name only when the card carries more than one. Mapping rows are
  keyed by `(connector name, remote id)` — the engine's
  binding-scoped model (D6-2: the same numeric id through a
  different connector is a genuinely different remote) is *not*
  silently merged; duplicate remote titles across a card's
  connectors render as separate rows. `ListRemoteBoards` always
  addresses one named connector.
- **Auto-derived connector name (new sign-ins)**:
  `kebab(host) + "-" + kebab(username)` (e.g. `cloud-example-com-alice`),
  minted app-side in `save_connection` when the profile name is
  empty (the lane's `ConnectionProfile.name` becomes derived, not
  user-typed; the UI sends `""`). Uniqueness: on collision with an
  existing connector name, append `-2`, `-3`, … up to `-9`; exhausted
  → typed `SettingsError::Invalid { field: Field::Name,
  kind: InvalidKind::Duplicate }` (the field lattice arm already
  exists; the status sentence is the adapter's copy, unasserted).
  Kebab validity is guaranteed by construction (host chars pass the
  same filter `ConnectorId::from_kebab` accepts; the mint runs
  through `from_kebab` and refuses typed otherwise).

## 3. D-8.3 — the `ListRemoteBoards` request/reply/failure taxonomy

```rust
// src/requests.rs
pub struct RemoteBoardRow {
    pub remote_id: u64,          // the Deck numeric id (u64, model.rs:19)
    pub title: String,
    pub bound_local: Option<uuid::Uuid>,  // set iff bound through THIS connector
    pub connectable: bool,       // live remote && not bound anywhere via this connector
}
pub enum UiRequest {
    // ...
    ListRemoteBoards {
        connector: String,
        reply: oneshot::Sender<Result<Vec<RemoteBoardRow>, SettingsError>>,
    },
}
```

- App-side handler `list_remote_boards`: config lookup (`Gone` when
  the connector vanished) → `store.load` (the existing
  `map_credential_error`: `NoSecretStored` / `StoreUnsupported` /
  `StoreFailed`) → `DeckClient::new` (`AttachFailed` on URL refusal)
  → `client.boards()` (the existing `map_ceremony_error`:
  `VerifyUnreachable` / `VerifyRejected` / `VerifyFailed`) → project
  secret-free rows against the engine state's bindings (live boards
  only; `bound_local` from `remote.binding.connector == connector`).
  Deleted/archived remotes are filtered (the same live filter the
  retired title-match arm used) — the list is the connectable
  universe.
- **No state changes, no persistence, no attach** — a pure query +
  network read. The listing *is* the credential verification (the
  same property the retired arm relied on), so the row Test button
  becomes redundant for Deck accounts and **retires** (D-8.9);
  `TestConnection` stays on the lane for the transition window only
  if a consumer remains — decision: **the lane variant retires with
  the button** (no other consumer exists; the REPL has its own
  verify path).
- **Test seam (fakes-over-mocks, AGENTS §5)**: `ConnectionWorkspace`
  gains `remotes: Arc<dyn RemoteBoards>` — a one-method port
  (`async fn boards(&self, server_url, username, password:
  &AppPassword) -> Result<Vec<RemoteBoardRow>, SettingsError>` is
  the *handled* shape; the port returns the raw client result and
  the projection stays app-side: `async fn boards(&self, server:
  &str, user: &str, password: &AppPassword) ->
  Result<Vec<taskboard_sync_nextcloud::Board>, DeckError>`), with
  the production impl constructing the `DeckClient`. Unit tests use
  a `HashMap`-backed fake; the UI journeys' fake lane answers from
  a fixture. `#![forbid(unsafe_code)]` unaffected; the port lives in
  `connections.rs` (app crate, AGPL header unchanged).
- **Busy taxonomy**: new `PendingKind::ListRemote` — busy markers on
  the owning card's Deck area (`busy_target` = the connector name),
  the global busy gate disables sibling buttons; the reply clears it.
  The dropped-reply wedge guard is the existing `PendingReply`
  consumption rule — a dropped oneshot resolves `Err` through the
  same arm every other kind uses (no new guard mechanism).

## 4. D-8.4 — the explicit bind: `AttachConnector` grows `remote`

**Decision: extend, not a new variant.** `AttachConnector { board:
BoardId, connector: String, remote: u64, reply }` — the round plan's
suggestion adopted; the UI always knows the remote id (it clicked the
row that carries it).

- `attach_connection` reshapes: config lookup → secret load →
  **pre-IO binding guard** (board bound to another connector →
  `BindRefused`; board bound to this connector with a *different*
  remote → `BindRefused`; same `(connector, remote)` → idempotent
  no-op fast path that still ensures attach + settle, preserving the
  re-issue semantics) → ensure-attached (the named-lookup guard,
  unchanged) → `StateCommand::BindBoard { board, remote,
  connector }` → `wait_for_boot_pull` → snapshot.
- **Retired with it**: the title-match adoption arm, the
  `NoRemoteMatch` variant (dead after this chunk — removed from
  `SettingsError`, the review greps for it), and the
  "derive remote id from the board title" comment chain.
- `map_execute_error` stays byte-identical (the engine guards are
  unchanged: `BoardAlreadyBound` / `RemoteBoardInUse` /
  `UnknownBoard`).

## 5. D-8.5 — create-local-from-remote

One user intent, two engine writes, one lane request — **a new
`CreateLocalBoard { connector: String, remote: u64, reply:
oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>> }`**
(the reply is the snapshot both surfaces re-render from, the r2 D32
rule):

1. Resolve connector + secret (the shared preamble).
2. List remotes through the D-8.3 port → find `remote` → take its
   title (`Gone` if absent/live-filtered).
3. `engine.execute(StateCommand::CreateBoard { title, color:
   Color::new(DEFAULT_BOARD_COLOR) })` — the receipt
   `CreatedBoard(id)` names the minted board.
4. The explicit-bind sequence of D-8.4 with that id (ensure-attach →
   `BindBoard` → settle).
5. Reply the fresh snapshot.

**Failure story (recorded)**: a refused bind after a successful
`CreateBoard` performs *no* delete rollback — the local board is a
valid standalone outcome (binding is the failure, visible typed);
the same no-rollback honesty the attach path already records. Busy
kind: reuses `PendingKind::Attach` semantics — decision: new
`PendingKind::CreateBind` for honest attribution; same ceremony.

## 6. D-8.6 — the Files toggle (detection + gate)

- **Rendered on**: the account card whose `(server_url, username)`
  equals the **default connector's** identity (the first array entry,
  or the legacy synthesis) — the D-SF10 contract: the files substrate
  reads its URL + credential from that account. Other cards render
  no Files row (their identity could not drive the substrate without
  filesync-crate changes — the non-goal fence).
- **Value**: `config.files_sync.enabled` (the raw read the lane
  already does per request). Standalone-default world → no Files row.
- **Toggle**: new lane variant `SetFilesSyncEnabled { enabled: bool,
  reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>> }`
  — the editor-write path (`run_editor_write`) with
  `Field::FilesSyncEnabled` added to the `Field` enum (the totality
  lists in `editor_field_keys()`/`editor_field_key` force the new
  arm — the compiler enrolls it). Overlay writer
  `overlay_set_files_sync_enabled` beside the other scalar setters.
- **Apply semantics**: `OnRestart` with the registry justification
  *"the files-sync actor spawns at boot; live spawn needs substrate
  wiring fenced to the filesync round"* — registered in the apply
  registry, so the UI's restart badge renders honestly (the chunk-5
  mechanism; no pretend-live). No filesync crate changes, no
  `bootstrap.rs` changes.
- Toggle UX: the chunk-4 stop grammar (a two-segment
  `SegmentedControl` On/Off — the `focus-enabled-picked` precedent),
  disabled while busy, commit-on-pick.

## 7. D-8.7 — busy/error surfacing (the existing ceremony, reused)

Everything routes through the pinned status line + `SettingsOutcome`
mapping + `PendingKind`/`busy_target` busy markers; the new kinds
(`ListRemote`, `CreateBind`) join the `connector_busy` match in
`push_settings`. The error copy lives in `settings_status` (adapter
constants, unasserted by tests). The busy gate disables every
connections-section button while any ceremony is in flight — the
current rule, unchanged. No new global, no new channel, no new flow
slot (the r4 shape is adapter fields; this chunk adds none beyond the
remote-board model push).

## 8. D-8.8 — the keyboard walk (the chunk-4 stop grammar, per card)

Tree order, every element a stop unless listed exempt:

1. Sign-in fields (Server, Username, App password — `LabeledField`s,
   already stops), **Sign in** button (was Save).
2. Per account card: **Remove**; **Files On/Off** (when rendered);
   **Refresh** (Boards on server).
3. Per remote row: the local-board **Dropdown** (closed field stop +
   flyout ladder — the chunk-3 overlay grammar verbatim,
   `flyout-site` discriminator gets the next free site), **Create
   local board** (unbound rows), **Disconnect** (bound rows).

Exempt (not stops): the identity/status line, the status dot, the
BusyDot, InfoTips' hover targets (the recorded class). The chunk-4
`focus_coverage` invariant already covers the settings view — the new
stops must be stops or the build fails; the walk journey pins the
card-scoped reading order. **No modal, no flow slot**: the section
stays a plain scrolled form surface (Escape does nothing new; the
dropdown's Tab-rejects rule walks on).

## 9. Superseded r4 D39 decisions (for the review)

| r4 decision | Fate in this chunk |
|---|---|
| Board-first selector ("Select Board to Connect/Disconnect") as the primary binding IA (`BoardPicker` driven by `selected_board`) | **Superseded** — D7 ruled account-first; the card rows bind explicitly. The shared `Dropdown` widget itself survives (reused per remote row). |
| `settings_board_selected` + selection-driven Bind-only semantics (the zero-requests-on-select pin) | **Superseded** — the selection VM state and its prune pass retire with the selector; binding is per-row explicit. |
| Per-connector **Bind** button armed by the global selection (`bind-clicked(index)`) | **Superseded** — replaced by per-remote-row actions. `bind-clicked(int)` and its adapter seam die. |
| **Detach-selected** beside the selector (`detach-selected()`) | **Superseded** — Disconnect moves onto the bound remote row (same `DetachConnector` request, same multi-board guard, same ceremony; the seam re-targets from `selected_board` to the row's `(board, connector)`). |
| `SettingsBoardCopy` ("…does not switch the board view") | **Replaced** by the account-card copy set (presentation constants, same never-asserted rule). |
| Row **Test** button (+ lane `TestConnection`) | **Retired** — the listing verifies credentials; no other consumer remains. |
| The `Name` field on the sign-in form | **Retired** — D-8.2 auto-derivation. |
| Title-match adoption (`attach_connection` `:899-907`) + `SettingsError::NoRemoteMatch` | **Retired** (the round's headline). |

**Not superseded** (stay verbatim): the `Dropdown` widget and its
overlay flyout, the busy ceremony and status line, the Save
verify-first ordering, the Remove choreography, `connector_row_status`
folding, the detach multi-board actor-stop guard, the env-lock and
apply-registry editor machinery.

## 10. Test catalogue (TDD; red-first where new)

**Tier 1 — app-lane units** (`ui_requests.rs` / `connections.rs` /
`presentation`-equivalent fakes):
- `list_remote_boards` over the fake port: projects live rows, sets
  `bound_local` only for this connector's bindings, filters
  deleted/archived remotes; typed failures (`NoSecretStored`,
  `Gone`) leave no state change (assert via a state probe before/after).
- The name mint: host+username kebab, collision suffixes, kebab-refusal.
- `attach_connection` explicit-remote: bind lands, idempotent
  re-issue with the same remote, `BindRefused` pre-IO on
  bound-elsewhere, no `boards()` call on the idempotent path.
- `create_local_board`: receipt id bound to the remote; bind-refusal
  after create leaves the board (typed error, no rollback).
- `set_files_sync_enabled`: env-locked refusal, overlay write, the
  registry carries the OnRestart justification (the chunk-5
  registry test pattern).

**Tier 2 — adapter/presentation units**: the account grouping fold
(connectors → cards, order, collision rendering), the
`RemoteBoardRow` → `DropdownRow` projection (indicator/tone/busy),
the busy-kind attribution table.

**Tier 3 — journeys** (`tests/journeys.rs`, the fake lane/engine):
- The D7 session: sign-in (derived name visible in the reply rows) →
  Deck present → Refresh → rows render → bind an unbound local board
  explicitly → binding visible in state and in the reply snapshot.
- Create-local-from-remote lands a bound board.
- Typed failure sessions: no-secret listing and an unreachable
  listing surface through the status line with zero state change and
  busy cleared.
- The keyboard-only session: the §8 walk reaches every action
  (stops), a bind completes via Enter on the dropdown flyout.
- Rewrites of the retired journeys: the r4 selector/Bind/Detach
  journeys re-target the new seams; the title-match adoption journey
  **dies** (its acceptance is subsumed by the explicit-bind journey);
  none weakened otherwise.
- The focus-coverage invariant stays green over the new section
  (stops joined by construction; any miss is a build failure).

**Regenerated**: any `keymap_snapshots`/insta snapshots the model
push touches (verified at implementation; none expected — the
settings model is not snapshotted).

## 11. Commit slices (stage 2; standalone, rebase-clean)

1. `feat(app): the remote-board listing lane` — the port seam +
   `ListRemoteBoards` + `RemoteBoardRow` + fakes + Tier-1 tests.
2. `feat(app): the explicit remote bind` — `AttachConnector.remote`,
   the title-match/`NoRemoteMatch` retirement, `CreateLocalBoard`,
   Tier-1 tests.
3. `feat(app): the files-sync toggle lane` — `SetFilesSyncEnabled`,
   `Field::FilesSyncEnabled`, the overlay writer, registry entry.
4. `feat(ui-slint): the account-card connections section` — the
   grouping fold, model pushes, the section markup, the retired
   selector/Test seams' removal, Tier-2 tests.
5. `test(ui-slint): the account-connection journeys` — Tier-3
   journeys + the retired-journey rewrites; CHANGELOG.
6. Stage 3: rebase, review (incl. the §9 supersession checklist and
   the grep-for-`NoRemoteMatch` negative), ship, open the two fenced
   backlog issues (`backlog: Deck push-to-remote board creation`,
   `backlog: account-removal cascade`), teardown.

## 12. Risks

- **R1 — journey churn**: the r4 connection journeys are seam-driven
  and numerous; the retirements touch ~5 of them. Mitigation: the
  rewrites ride slice 5 with one journey per retired seam; none may
  be deleted without a named successor (the review checklist).
- **R2 — filesync rebase friction**: `bootstrap.rs`/`config.rs` are
  DR-9's busiest files; this chunk touches `config.rs` (Field enum
  is ui-crate-side, but the overlay writer is app-side) and *not*
  bootstrap. Bounded; the standard dance applies.
- **R3 — grouping ambiguity** (same identity, two connectors; URL
  variants): rendered honestly per D-8.2, but the IA has not been
  user-reviewed at collision time — tier-4 manual check item.
- **R4 — `CreateBoard` color**: the default color constant is a
  presentation-adjacent choice made app-side; pinned by one unit
  test, trivially changeable.
- **R5 — the port seam's async-trait surface**: `async fn` in a
  trait on the pinned toolchain (native async-in-traits, no
  `async_trait` crate — not in the approved stack); if the pinned
  compiler rejects the object-safe form, the fallback is an
  `Arc<dyn Fn(...) -> BoxFuture>` seam — recorded deviation, never a
  concrete-type test hole.
- **R6 — remote-list staleness**: rows render from the last
  Refresh's reply plus live binding state; a stale remote id bind
  fails typed through the engine (`RemoteBoardInUse`/engine guards)
  — acceptable; auto-refresh on card mount is fenced (it makes
  every settings open a network call).

## 13. Diff size estimate

`requests.rs` +55/−10; `ui_requests.rs` +180/−90; `connections.rs`
+45; `connectors.rs` +15; new `remotes.rs` (the port) ~+60;
`config.rs` (field key) +8; adapter.rs +260/−140; `models.rs` +40;
`settings-view.slint` +280/−170; `presentation.rs` +70;
`app-window.slint` ±15; journeys +~380/−180; unit tests +~200.
**Net ≈ +900 lines** — M+ (the dispatch's M, honest at the top of
the band; the retirement deletions keep it from L).

## 14. Stage-2 deviations (recorded 2026-11-07)

- **No `RemoteBoards` port (§3 test seam).** The app-lane suite
  (`tests/ui_settings.rs`) already drives the real `DeckClient`
  against wiremock — a hermetic fake Deck server — and the attach/bind
  tail needs a real client for the connector actor anyway. A second
  seam would duplicate that fake, so `ConnectionWorkspace` is
  unchanged; the listing tests run over wiremock + `FakeCredentialStore`.
- **Re-sign-in reuses the connector (§2).** A sign-in whose exact
  `(server_url, username)` is already configured reuses that
  connector's name (the upsert rotates its secret) instead of minting
  a `-2` sibling; the suffix ladder applies to new identities only.
- **Account removal is identity-wide (§1).** `RemoveConnection` removes
  every connector sharing the named connector's identity (the card is
  the account) and refuses `BindRefused` pre-IO while any live board
  is bound through one of them.
- **Commit slicing (§11).** The explicit-bind lane change and the UI
  section land in one commit: the `AttachConnector` shape change and
  the retired selector/Test seams cannot build apart. Tier-3 journeys
  for the session, create, failures, and Files toggle ride that
  commit; the keyboard-only session and the CHANGELOG ride the last.
- **`SettingsModel.rows` kept** as the per-connector data model (the
  status-episode journeys read it); the cards render from the new
  `connection-items` projection.
- **Files toggle copy**: `SettingsBoardCopy` keeps its global name
  (re-purposed for the account-card copy) to avoid churning the
  tooltip/flyout wiring.

## Stage-3 review amendments (2026-11-07)

- `RemoteBoardRow` drops `bound_local` (and the sketched
  `connectable`): no production consumer read it — the adapter
  re-derives each row's binding from the engine state on every push
  pass (r4 D39 §2.3), so the app-side projection was dead,
  duplicated logic. The row is `{ remote_id, title }`.
- Remote ids cross the UI lane typed: `RemoteBoardRow.remote_id`,
  `AttachConnector.remote` and `CreateLocalBoard.remote` are
  `RemoteBoardId`, not raw `u64` (the §2.1 type-driven policy;
  `board` was already a `BoardId`).
- `remove_connection`'s account-identity fan-out and its
  bound-board guard became named helpers
  (`account_connector_names`, `any_live_board_bound_through`) — one
  level of abstraction in the choreography.

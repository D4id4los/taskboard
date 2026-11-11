# [Plan] Round-6 Chunk 4 — Publish a local board to the server (D-6.9, #699)

- **Date**: 2026-11-12
- **Status**: STAGE-1 PLAN (TDD-first implementation spec; no code). Branch
  `feat/ui-r6-publish-board`, worktree
  `../taskboard-WT-feat-ui-r6-publish-board`, branched off `origin/main` @
  `59f150ef` — **chunk 3 (`feat/ui-files-sync-anchor`) has already landed**,
  so `ConnectorInfo.backend` and the backend-word labels exist in the base;
  all anchors below were re-verified at that SHA.
- **Origin (normative)**: `.artifacts/plans/2026-11-11-gui-review-round6-plan.md`
  §3 D-6.9 and §2 row R6-9; dispatch plan
  `.artifacts/plans/2026-11-11-ui-r6-dispatch-plan.md` §1/§5; issue #699.
- **Non-goals** (dispatch §5, binding): no new Deck API surface
  (`create_board` exists, `crates/taskboard-sync-nextcloud/src/client.rs:525`);
  no files-target publish (chunk 5's Move-to-Files flow); no change to the
  CLI verb's behaviour or output.

---

## 0. The finding, restated

The publish ceremony fully exists at CLI/REPL level (`boards publish` →
`create_board` REST → `BindBoard` → W2 orphan sweep → nudge → adoption
wait → reconcile, `crates/taskboard-app/src/commands/boards.rs:637-1007`).
What is missing is only the GUI entry point: a settings-Deck-card row that
publishes a chosen unbound local board onto that connector's server. The
chunk is: one request arm, one extraction, one UI row + picker wiring.

## 1. Decisions (every open question settled)

### D-1 — The shared core already exists; the seam is the parameter, not a new helper

`publish_prelude` (boards.rs:661) + `publish_on` (boards.rs:676) are
already the extracted shared ceremony, shared verbatim by the CLI one-shot
wrapper (`publish`, boards.rs:637) and the session dispatcher
(`crates/taskboard-app/src/commands/mod.rs:1970-1990`). No new helper is
extracted — **the extraction this chunk makes is one of parameterisation**:
`publish_on`, `sweep_publish_orphans` (boards.rs:798),
`bind_nudge_wait_reconcile` (boards.rs:970), and `wait_for_adoption`
(boards.rs:1127) currently take `app: &crate::App` but touch **only
`app.engine`** (verified by reading every body: `shared_state`, `execute`,
`subscribe`). Their signatures become `engine: &taskboard_state::EngineHandle`
(`EngineHandle` lives in `taskboard-state::actor`; `taskboard-app` already
depends on it — `App.engine` and `ConnectionWorkspace.engine` are both
`EngineHandle`). Call sites:

- CLI one-shot (`publish`, boards.rs:637): passes `&app.engine`.
- REPL session dispatcher (`commands/mod.rs:1977`): passes `&app.engine`
  (the session case also has a live `App` — the REPL boots one per line).
- **New UI arm** (`ui_requests.rs`): passes `&workspace.engine` — this is
  the point of the change: the UI request forwarder holds a
  `ConnectionWorkspace` (engine + connector control + credential store,
  `ui_requests.rs:69-79`), never a bootable `App`.

This is a pure refactor for CLI/REPL: `boards::publish`'s public signature
is untouched, so the CLI/REPL tests (`tests/cli_attach.rs`, the publish
cases; `tests/repl.rs`) pin it unchanged.

### D-2 — Request payload and reply shape

```rust
// crates/taskboard-ui-slint/src/requests.rs, beside AttachConnector (:342):
/// Publish a local board onto the named connector's server: create the
/// Deck board from the local one's title/color and bind to it (the
/// `boards publish` ceremony, D-6.9). Refuses typed when the board is
/// already bound.
PublishBoard {
    board: BoardId,
    connector: String,
    reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
},
```

Reply is `ConnectionsSnapshot` — identical to `AttachConnector` /
`CreateLocalBoard` / `DetachConnector`, so the standard refresh path
(`drain_setting_replies`' `PendingReply::List` arm) re-pushes the settings
page unchanged. **Decision**: no `PublishReceipt` over the lane — the UI
needs "bound + settled", and the snapshot already carries the binding, the
sync phase, and the rendered row model; a second receipt shape would
duplicate the refresh machinery for no UI need.

Typed refusals ride the existing `SettingsError` lattice — **no new
variants**. Mapping from the ceremony's `CommandError` classes
(`map_ceremony_error`, ui_requests.rs — the same mapper
`live_remote_boards` uses):

- engine `BoardAlreadyBound` → `SettingsError::BindRefused` (the
  pre-IO guard makes this the common refusal; see D-3);
- engine `UnknownBoard` / `BoardDeleted` → `SettingsError::Gone`;
- sync/transport classes while settling → `VerifyUnreachable` /
  `VerifyFailed` per the mapper's existing split;
- `SyncTimedOut` → a new **`SettingsError::SyncTimedOut`** — decision:
  add exactly one variant. The lattice has no "the ceremony outlived its
  budget" arm and `AttachFailed` would lie (nothing failed; it is still
  moving). Verify against `map_ceremony_error`'s current arms at
  implementation time; if it already degrades timeouts to a usable
  variant, keep the lattice untouched and record that instead.

### D-3 — The UI arm: guard, attach-ensure, ceremony

New `async fn publish_board(workspace: &ConnectionWorkspace, board:
BoardId, connector_name: &str) -> Result<ConnectionsSnapshot,
SettingsError>` in `ui_requests.rs`, shaped after `attach_connection`
(ui_requests.rs:1003):

1. `ensure_migrated` → `connector_entry` (the connector named on the card;
   `Gone` when it vanished).
2. Load the stored secret (`NoSecretStored`/`StoreFailed` classes, the
   attach path's mapper).
3. **Pre-IO unbound guard** (the publish ceremony's own rule, moved to the
   front so the refusal never touches the network): read
   `workspace.engine.shared_state()`, refuse `BindRefused` when the board
   is deleted/archived or `remote.is_some()`. `publish_on` re-checks after
   the create (the racy second enforcer, boards.rs:692-700) — unchanged.
4. **Ensure the connector's actor is attached** — decision: extract the
   attach half of `bind_through` (ui_requests.rs:1048: the
   `ConnectorControl.connector_commands(..).is_none()` → build `DeckClient`
   → `attach_connector` sequence) into a small private helper
   `ensure_attached(workspace, entry, password) -> Result<ConnectorId,
   SettingsError>` and call it from both `bind_through` and `publish_board`.
   The publish nudge (`RequestSync` inside `bind_nudge_wait_reconcile`)
   only reaches the connector when its actor is registered; without this
   the ceremony answers the honest-but-useless `SyncUnavailable`.
5. Build the ceremony client (`DeckClient::new`, the
   `live_remote_boards` pattern, ui_requests.rs:888) and run
   `publish_on(&workspace.engine, &client, Some(board), PUBLISH_SETTLE,
   &connector_id)`.
6. Reply `Ok(current_connections(workspace))`; failures map per D-2.

**Recorded failure story** (house pattern, mirrors `create_local_board`'s):
a transport failure after the `create_board` POST leaves the board
server-side unbound; the W2 orphan sweep cleans same-title/same-color
crash-window orphans on the *next* publish through that connector, and the
typed error is final for this request — no delete-rollback
(the duplicate-create crash window is #150's owned problem).

### D-4 — Wait budget

`const PUBLISH_SETTLE: Duration = Duration::from_secs(60)` beside
`SAVE_SETTLE` (ui_requests.rs:329) — **the CLI default** (`timeout_secs`
default 60, cli.rs `boards publish`), deliberately *not* `SAVE_SETTLE`'s
5 s: the budget buys the full first outbox drain onto a freshly created
board (potentially hundreds of cards), the same semantics the CLI
ceremony promises. The busy state covers it (D-6); `SyncTimedOut` is the
typed outcome when it expires.

### D-5 — UI row placement and picker wiring

On the Deck connection card (`settings-view.slint`, the
`ConnectionItemKind.deck` block at :915): the existing row gains a second
button, **"Publish a local board…"**, beside `Refresh` —
`enabled: !SettingsModel.busy`, `clicked => root.publish-clicked(index)`.
The picker is the **existing Dropdown grammar**: a `DeckPublishPicker`
component cloned from `RemotePicker` (:462) — same `rows:
SettingsModel.board-options`, same `SettingsBoardCopy` copy, same
`SettingsBoardFacts` flyout observation seams — with one difference:
**`flyout-site: 1`** (the remote rows' pickers own site 0; the flyout
grammar's site indices exist exactly to let two picker classes coexist —
verify the site count on `Dropdown` at implementation time and add site 1
if it is not declared yet). Options source: `SettingsModel.board-options`,
re-derived every push pass from the engine state as the unbound live
boards (`push_settings`, adapter.rs:4699-4704 — unchanged code; the
unbound-only filter *is* the publish precondition, so the picker needs no
new filtering). Empty case: the Dropdown's existing `empty-hint`
("No unbound local boards") renders; the button stays enabled so the hint
is discoverable. The backend-aware label (landed chunk 3) is untouched.

Adapter side (taskboard-ui-slint/src/adapter.rs):

- `ItemAddr::Deck { connector }` already addresses the card.
- New seam `settings_publish_picked(window, item: usize, option: usize)`:
  resolve `ItemAddr::Deck` + `local_choices[option]` → `BoardId`, refuse
  while `pending.is_some()`, then `send_connections_op(..,
  UiRequest::PublishBoard { .. }, reply_rx, PendingKind::Publish,
  &connector, None)` — the exact `settings_remote_bind_picked` shape
  (adapter.rs:5238) minus the remote.
- `PendingKind::Publish` (the enum at adapter.rs:3857): the Deck row's
  busy marker joins the deck-row `matches!` list (:4712 region) so the
  card's `BusyDot` spins while the ceremony runs;
  `busy_target = connector`, `busy_remote = None`.

The UI stays a dumb client: it picks a board id and a connector name; the
app owns every rule.

### D-6 — Pending/busy states

`PendingKind::Publish` follows the existing machine exactly: set on send
(`send_connections_op` → `SettingsOutcome::Busy`), the reply clears it via
the standard `PendingReply::List` drain; during the window every settings
button renders disabled (`SettingsModel.busy`), the addressed Deck card
renders its `BusyDot`, and a second publish pick is absorbed by the
`pending.is_some()` guard. No new outcome text beyond what
`SettingsError::SyncTimedOut` (if added) needs in `settings_status.rs`'s
sentence table — one entry, matching the existing error-sentence grammar.

## 2. TDD ordering & test catalogue

Order: (a) the parameterisation refactor lands first with the existing
tests green (commit 1); (b) app-arm tests red → implementation (commit 2+);
(c) UI seam/wiring (commit 3); (d) gates.

1. **Refactor pin (existing, must stay green unchanged)**:
   `tests/cli_attach.rs` publish ceremony cases (:239 happy path,
   :271/:287 refusals, :412/:463/:518/:572 sweep cases,
   :534 `v21_a_sweep_failure_never_fails_the_publish`);
   `tests/cli_standalone.rs:177`; `tests/repl.rs` publish lines;
   the settings seam tests (`tests/ui_settings.rs`,
   `tests/ui_live_settings.rs`) pin `bind_through`'s refactor into
   `ensure_attached`.
2. **New — app arm happy path** (`tests/ui_settings.rs`, wiremock +
   `booted_workspace`, the attach tests' harness): an unbound local board;
   `UiRequest::PublishBoard` through the forwarder; wiremock asserts the
   `POST /boards` carried the local board's title/color; the reply is
   `Ok(snapshot)`; the snapshot shows the binding to the minted remote id;
   the engine settles to `Idle` with the binding present (terminal state,
   no wall-clock assertions). Structural facts only.
3. **New — bound-board refusal**: same harness with a bound board; the
   reply is `Err(BindRefused)`; wiremock records **no** `POST /boards`
   (the pre-IO guard), typed-outcome assert via `matches!` — no text
   assertions.
4. **New — connector gone**: publish against an unknown connector name →
   `Err(Gone)`, no network.
5. **New — picker options**: the unbound-only list is existing
   `push_settings` behaviour already exercised by the settings seam tests;
   this chunk adds the Deck-row picker over the *same* model, so the
   UI-side catalogue item is the wiring test only: a slint-side journey /
   adapter test in the house pattern asserting the picked
   (deck-item, option) pair dispatches `PublishBoard` with the right
   `(connector, board)` and sets `PendingKind::Publish` (dispatch-level
   routing, per AGENTS §5 "one behavior, one test, lowest layer" — the
   filter itself is not re-tested).
6. **No quality ceilings**: the sweep/timeout tests stay floors
   (`v21`'s "sweep failure never fails the publish" shape); no new
   ceiling assertions.

## 3. Risks

- **The 60 s busy window**: the settings page is fully disabled while the
  ceremony settles. Accepted for this chunk (the CLI is equally blocking);
  if review finds it hostile, the backlog remedy is a per-row progress
  phase, not a shorter budget (a shorter budget publishes half-drained
  boards).
- **`SettingsError` lattice growth** (one variant, D-2): sentence-table
  and any exhaustive matches must follow; bounded, mechanical.
- **Flyout site collision**: if `Dropdown`'s site grammar only declares
  site 0, adding site 1 touches the shared Dropdown — contained, but the
  review must verify no site-0 picker regresses.
- **Anchor drift**: origin-plan line numbers predate chunk 3's landing;
  this plan's anchors are the re-verified ones (SHA noted above).

## 4. Estimated diff

~250–400 lines: boards.rs signature change + call sites (~30),
ui_requests.rs arm + `ensure_attached` + const (~90),
requests.rs enum arm + error variant (~15), adapter.rs seam +
PendingKind + busy lists (~60), settings-view.slint button +
picker component (~50), tests (~120), status sentence (~5).

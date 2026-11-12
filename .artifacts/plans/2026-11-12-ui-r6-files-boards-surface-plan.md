# [Plan] Round-6 Chunk 5 — Files-boards UI surface (D-6.8 phase 2, #722)

- **Date**: 2026-11-12
- **Status**: STAGE-1 PLAN (TDD-first implementation spec; no code). Branch
  `feat/ui-files-boards-surface`, worktree
  `../taskboard-WT-feat-ui-files-boards-surface`, branched off `main`
  (fast-forwarded to `origin/main`) @ `71786d12` — **chunk 3
  (`feat/ui-files-sync-anchor`) is MERGED** (gate verified: `ConnectorInfo`
  carries `backend`, connector labels are backend-word-aware, the Files
  row rides every Nextcloud card) **and chunk 4
  (`feat/ui-r6-publish-board`) has also landed**, so the PublishBoard
  request arm, `PendingKind::Publish`, and the `DeckPublishPicker` exist
  in the base as the exact pattern this chunk mirrors. All anchors below
  were re-verified at that SHA.
- **Origin (normative)**:
  `.artifacts/plans/2026-11-11-gui-review-round6-plan.md` §3 D-6.8 phase 2
  and §2 row R6-8; dispatch plan
  `.artifacts/plans/2026-11-11-ui-r6-dispatch-plan.md` §1/§6; issue #722
  (the reshaped-scope comment: visibility + Move-to-Files, no connect
  picker — adoption is automatic).
- **Non-goals** (dispatch §6, binding): no engine work (landed via DR-9);
  no connect/bind picker for files boards (adoption is automatic — the
  accepted design); no Deck→files bulk operations; no mixed-fleet exit
  ceremony UI beyond the move verb. Added by this plan: no `--undo` UI
  (backlog issue), no files-binding disconnect UI (backlog issue), no
  per-board adopting/adopted status surfacing (backlog issue).

---

## 0. The finding, restated

The boards-over-Files engine is fully landed: `RemoteBoardId::Files(Uuid)`
(`crates/taskboard-domain/src/ids.rs:133`, payload-is-the-tag D-BF4), the
resumable four-step move ceremony
(`crates/taskboard-app/src/board_move_ceremony.rs`, user contract
`docs/boards-move-runbook.org`), and automatic cross-device adoption
(`plan_board_adoption`, `crates/taskboard-state/src/actor.rs:1008-1026`).
The UI shows none of it: the settings Files item is only the On/Off
toggle, files-bound boards render **nowhere** (their binding's connector
is the synthesized `files-sync` identity, which is no config connector,
so the per-card binding loops in `connection_items_model` skip them), and
no request variant reaches the move ceremony. This chunk adds the
visibility rows and one Move-to-Files request arm.

## 1. Decisions (every open question settled)

### D-1 — The eligible set for Move-to-Files is the ceremony's own guard, read structurally

The ceremony's position is *derived from the world*
(`derive_move_stage`, board_move_ceremony.rs:153): a move starts from a
**Deck-typed binding** (`BindingKind::Deck`); an unbound board with no
floor refuses `NotDeckBound`; a files-bound board is already `Committed`
(a rerun is a no-op). The picker's option set mirrors that guard
**structurally**:

```rust
// adapter.rs push_settings, beside local_choices:
// the Deck-bound live boards — the move ceremony's eligible set
// (derive_move_stage: BindingKind::Deck starts a move; unbound
// refuses NotDeckBound; files-bound is already committed).
let move_choices: Vec<requests::BoardInfo> =
    crate::presentation::boards_of(&state)
        .into_iter()
        .filter(|board| /* Deck-typed binding; see D-6 for the carrier */)
        .collect();
```

Concretely in the model layer: a new presentation helper
(`crates/taskboard-ui-slint/src/presentation.rs`, beside `boards_of`)
`deck_bound_boards(&AppState) -> Vec<BoardInfo>` — live
(`!deleted`, `!archived`) boards whose `remote` exists with
`remote.board.as_deck().is_some()`. Decision: **unbound boards are NOT
eligible** (the ceremony refuses them — the prompt's "unbound + Deck-
bound?" question settles as Deck-bound only; the ceremony's own guards
are normative). Boards mid-move (unbound with a published floor) are
invisible to the picker — honest: a partially-moved board is not a
fresh move target; the caption copy (D-8) points at resumption.

Files-bound detection is by **id type, not connector name**: the UI
never string-matches `files-sync` (D-BF4: the payload is the tag —
`binding.board.as_files().is_some()` IS the files lane). This keeps the
adapter free of `FILES_SYNC_CONNECTOR_NAME` literals.

### D-2 — Request payload and reply shape

```rust
// crates/taskboard-ui-slint/src/requests.rs, beside PublishBoard:
/// Move a Deck-bound local board onto the files tier (the resumable
/// `boards move --to-files` ceremony, D-6.8 phase 2 / #722). The
/// files lane is the resolved anchor — no connector parameter (the
/// single-lane posture, #723 stands). Refuses typed when the board
/// is not Deck-bound or the files lane is off.
MoveBoardToFiles {
    board: BoardId,
    reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
},
```

Decision: **no connector field** — the ceremony resolves the files spec
from the config (`files_sync_spawn_decision`) and the Deck spec from the
board's own binding; a connector parameter would only invite a lane the
config does not have. Reply is `ConnectionsSnapshot`, identical to
`PublishBoard`/`DetachConnector`, so the standard
`drain_setting_replies` refresh path re-pushes the settings page
unchanged. No `MoveReceipt` over the lane: the UI needs "files-bound +
settled", which the snapshot's `bound_connector` plus the engine state
already carry; the receipt's step list is CLI output decoration, not UI
state (the busy gate is the only in-flight truth — D-9).

### D-3 — The parameterisation refactor: `AppWorld` runs on the engine handle, not a booted `App`

The CLI verb bootstraps a whole `App` per invocation
(`commands/mod.rs:1050-1075`); the UI process cannot — its engine is
already running and `App` holds the process-lifetime `InstanceLock`
(a second in-process bootstrap would refuse). But `AppWorld`'s reads are
narrower than `&App` (verified body-by-body):

- `app.engine` — `shared_state()`, `execute`, `subscribe`,
  `SyncStateReader::read_state` (board_move_ceremony.rs:318-537);
- `app.config` — `configured_connectors` (retired_binding, live_moved_from)
  and the verb dispatch's `files_sync_spawn_decision`;
- the `store`.

**Decision**: re-key the ceremony's world to those seams —
`AppWorld { engine: EngineHandle, config: AppConfig, board, ... }`
(`EngineHandle: Clone`, `AppConfig: Clone`; the two fields swap from
references to owned clones — `run_on`'s signature becomes
`run_on(engine: &EngineHandle, config: &AppConfig, store: &Arc<dyn
CredentialStore>, board, direction, timeout)`; `run_until` likewise).
Call sites:

- CLI verb (`commands/mod.rs`): passes `&app.engine, &config` — the
  `config` is already in scope there; the verb's behaviour and output
  are untouched, pinned by the `cli_verbs.rs` move tests.
- **New UI arm** (`ui_requests.rs` `move_board_to_files`): passes
  `&workspace.engine` and the `AppConfig` from `ensure_migrated(workspace)`.
- The ceremony's own tests (board_move_ceremony.rs:676+, the fake
  `MoveWorld` machines) survive **unchanged** — the `MoveWorld` trait is
  not touched; only the production impl's construction changes
  (`app_world` → `engine_world`, taking engine + config).

The one deep dependency is `pull_deck` →
`commands::sync::sync_on(app, …)`, which uses `app.engine` (execute
`RequestSync`, `shared_state`, `subscribe`) plus `app.system` **only for
the CLI's resurrect printing** (`report_resurrections` — output
decoration, not ceremony effect). **Decision**: extract
`sync_on_engine(engine: &EngineHandle, timeout, scope) -> Result<SyncOutcome, CommandError>`
(the nudge + `wait_for_sync_result` core, re-keyed onto
`engine.subscribe()`); `sync_on(app, …)` keeps its public signature for
the other CLI verbs and delegates to it, keeping the resurrection
reporting. `wait_for_sync_result` and `watched_status` are likewise
re-keyed (they read `app.engine` only). `pending_ops_of` /
`read_scope` (boards.rs:196-215) read `app.engine.shared_state()`
exclusively — re-key to the engine handle. This mirrors chunk 4's
D-1 ("the seam is the parameter, not a new helper") one level up: the
ceremony machine (`advance`, `undo`, the `MoveWorld` trait) is **not**
reimplemented anywhere.

### D-4 — The UI arm: guards, then the shared ceremony

```rust
// crates/taskboard-app/src/ui_requests.rs, beside publish_board:
async fn move_board_to_files(
    workspace: &ConnectionWorkspace,
    board: BoardId,
) -> Result<ConnectionsSnapshot, SettingsError> {
    let raw = ensure_migrated(workspace)?;
    // Pre-IO guards, mirroring publish_board's posture: the world's
    // own derived guards remain the racy second enforcers.
    let spec = crate::bootstrap::files_sync_spawn_decision(&raw)
        .ok_or(SettingsError::FilesSyncDisabled)?;
    let state = workspace.engine.shared_state().load();
    let row = state.boards.get(&board)
        .filter(|b| !b.deleted && !b.archived)
        .ok_or(SettingsError::Gone)?;
    let binding = row.remote.as_ref().ok_or(SettingsError::BindRefused)?;
    if binding.board.as_deck().is_none() {
        return Err(SettingsError::BindRefused); // files-bound or foreign shape
    }
    drop(state);
    crate::board_move_ceremony::run_on(
        &workspace.engine, &raw, &workspace.store, board,
        MoveDirection::ToFiles, DEFAULT_MOVE_TIMEOUT,
    ).await.map_err(map_move_error)?;
    tracing::info!(board = %board.as_uuid(), "settings move to files committed");
    Ok(current_connections(workspace))
}
```

`map_move_error` (new, beside `map_ceremony_error`) folds `MoveError`
into the typed `SettingsError` classes (D-5), logging the concrete
`MoveError` at `warn` for the diagnostics the coarse class drops.

### D-5 — Typed error mapping (new `SettingsError` variants, no strings)

`SettingsError` gains three field-less variants (the enum's existing
shape — every variant is a typed class; AGENTS §5 bans text assertions,
so the classes are the UI's only contract):

- `FilesSyncDisabled` — `MoveError::FilesSyncDisabled` (`[files_sync]`
  off or unspawnable: no lane to move onto);
- `OutboxNotQuiescent` — `MoveError::OutboxNotQuiescent { pending }`
  (the pre-IO quiescence guard; edits still queued);
- `MoveFailed` — the step/transport classes:
  `PublishFailed`, `PublishUnconfirmed`, `FilesSubtreeForeign`,
  `CommitFailed`, `DeckMarkerFailed`, `StateReadFailed`,
  `StrandedOps`, `UndoCoordinatesLost`, `AssignRefused`,
  `UnbindRefused`, `NotDeckBound`-via-race.

Existing variants absorb the rest: `DeckPullFailed`/`DeckPullIncomplete`
→ `SyncTimedOut` (exists; "the wait budget expired" is exactly its
meaning), `Gone` for a dead board id, `BindRefused` for the
not-Deck-bound guard. `ReverseFenced` is unreachable from this request
(the UI never asks for the fenced direction — the arm passes
`MoveDirection::ToFiles` literally). Each new variant's doc comment
names the `MoveError` classes it folds. The settings status line renders
the class via the existing `settings_status` fold (D-10's copy rows).

### D-6 — The Files card's visibility rows: derived from bindings only

`connection_items_model` (adapter.rs:~4790) extends the Files-item arm:
after the `ItemAddr::Files` push, the **files-bound live boards** render
as their own rows. New `ConnectionItemKind::FilesBoard` + a matching
`ItemAddr::FilesBoard { board: BoardId }` (index-aligned addrs; no seam
consumes the address yet — the rows carry no actions — but the address
table stays total and a future detach seam resolves without reshaping):

```rust
// inside the `if let Some(files_connector) = card.connectors…` block,
// after the Files item push:
for board in state.boards.values()
    .filter(|b| !b.deleted && !b.archived)
    .filter(|b| b.remote.as_ref().is_some_and(|binding|
        binding.board.as_files().is_some()))
{
    items.push(crate::ConnectionItem {
        kind: crate::ConnectionItemKind::FilesBoard,
        title: board.title.clone().into(),
        ..blank.clone()
    });
    addrs.push(ItemAddr::FilesBoard { board: board.id });
}
```

Decisions recorded:

- **Ordering**: `state.boards` iteration order, filtered — the same
  convention as the Deck card's `live_bindings` walk; no re-sort (the
  boards map's order is the engine's; determinism comes from the map).
- **Placement**: on the card that carries the Files row (the existing
  `card.connectors.iter().find(is_nextcloud)` selection). When files
  sync is toggled Off while files-bound boards exist, the rows **still
  render** under a toggle showing Off — binding truth is independent of
  the lane switch, and hiding them would fake the world.
- **Labels**: row title = the local board's title. No detail column and
  no action buttons (non-goals: no disconnect/bind UI). The kind itself
  is the label: a `FilesBoard` row renders with a fixed lead word
  ("Files", the row grammar the Files toggle row already established)
  followed by the title — no per-row backend word needed.
- **Scope**: only live boards. Deleted/archived files-bound boards are
  not the Files card's concern (the boards view owns those states).

### D-7 — The move picker: `FilesMovePicker` on the Files row

A `component FilesMovePicker inherits Dropdown` in
`ui/views/settings-view.slint`, cloned from `DeckPublishPicker`
(settings-view.slint:526) with the move grammar:

```slint
export global SettingsMoveCopy {
    in-out property <string> label: "Move a board to Files";
    in-out property <string> description:
        "Moves this Deck board onto this account's Files. Other devices
         adopt it automatically. If the move is interrupted, running it
         again resumes it.";
    in-out property <string> placeholder: "Move to Files…";
    in-out property <string> empty-hint: /* the no-eligible-board line */;
    in-out property <string> row-description: /* the per-option line */;
}
```

Placement: inside the `ConnectionItemKind.files` row's
`HorizontalLayout`, after the `SegmentedControl` — one affordance whose
closed field is the button, the publish precedent (one grammar, the
flyout renders window-level). The row wires
`callback move-picked(int, int)` → `root.move-picked(index, i)` →
`app-window.slint`'s `on_settings_move_picked` → the adapter seam.

The caption is **honest about the ceremony's contract** (runbook §1–2):
it names automatic adoption (eventual — no timing promised, dispatch §6
review focus) and resumability. It does not promise undo; the undo
window is CLI-only this chunk (backlog).

### D-8 — The adapter seam and options routing

```rust
// adapter.rs, beside settings_publish_picked (adapter.rs:5497):
/// The Files row's move seam (round-6 D-6.8 phase 2, #722): picking a
/// Deck-bound local board moves it onto the files tier. A stale
/// item/option index is absorbed silently.
pub(crate) fn settings_move_picked(
    &mut self, window: &AppWindow, item: usize, option: usize,
) {
    let Some(ItemAddr::Files { .. }) = self.settings.items.get(item).cloned()
        else { return };
    let Some(board) = self.settings.move_choices.get(option).copied()
        else { return };
    if self.settings.pending.is_some() { return; }
    let (reply, reply_rx) = oneshot::channel();
    let request = UiRequest::MoveBoardToFiles { board: BoardId::from_uuid(board), reply };
    self.send_connections_op(window, request, reply_rx,
        PendingKind::Move, &/* the item's files connector */, None);
}
```

`SettingsVm` gains `move_choices: Vec<uuid::Uuid>` beside
`local_choices` (index-aligned with the picker's options), set in
`push_settings` every pass from D-1's presentation helper — the
re-derive-every-pass rule (r4 D39 §2.3) applies verbatim. The busy
gate (`pending.is_some()`) and the silent stale-index absorption are
the publish seam's, verbatim.

### D-9 — Pending/busy states: one truth, no fake progress

- `PendingKind::Move` (beside `PendingKind::Publish`, adapter.rs:3870).
- `busy_target` = the item's files-connector name (the marker string
  convention, r2 D34 — it routes the row highlight, nothing else).
- The **Files row** shows `BusyDot` while `pending == Some(PendingKind::Move)`
  (the Files item's `busy` field extends its current lock-driven
  rendering); the global `SettingsModel.busy` gate disables every other
  control for the ceremony's duration — the existing machine, no new
  state.
- There is **no step progress**: the ceremony's `MoveStep` list is
  terminal output, not UI state. The in-flight truth is one dot; the
  reply (Ok or typed Err) is the next truth. An interruption (app
  close, engine drop) drops the pending lane exactly like the other
  ceremonies; the world-derived stage makes the next attempt resume,
  and the D-7 copy says so. This is the dispatch's "the UI reflects
  in-flight state, not a fake progress" verbatim.
- Budget: the ceremony's single wait (the final Deck pull) uses the
  CLI's default 60 s — lifted into one shared constant
  (`pub const DEFAULT_MOVE_TIMEOUT: Duration` in
  board_move_ceremony.rs; the CLI's `default_value_t = 60` cites it),
  so the UI cannot drift from the runbook's documented budget.

### D-10 — Status copy rows (settings_status)

`settings_status.rs` gains the outcome sentences for the three new
`SettingsError` variants and `MoveFailed`/`SyncTimedOut`-on-move
attribution. Copy pattern: name the recovery, never the internals —
e.g. FilesSyncDisabled → the enable-Files sentence; OutboxNotQuiescent →
the stop-editing-then-rerun sentence (the runbook §6 recovery, in
user words); MoveFailed → the rerun-resumes sentence. The text lives in
the existing `settings_status` fold — no new status machinery.

## 2. TDD ordering & test catalogue

Order (red-first per step; the refactor steps are pinned by the
existing suites, so their "red" is the new UI-arm tests):

1. **Presentation** (pure, first): `deck_bound_boards` helper tests in
   `taskboard-ui-slint` — Deck-bound live boards listed; unbound,
   files-bound, deleted, archived excluded; order = engine order.
   (`presentation.rs` test module, hand-built `AppState` fixtures —
   the `state_board`/`state_files_board` style; a files-bound fixture
   helper is added beside the existing one.)
2. **Refactor** (compile-green, suite-green): D-3's re-keying —
   `AppWorld`/`run_on`/`run_until`/`sync_on_engine`/`read_scope` onto
   the engine handle. Pinned unchanged by: the ceremony module's fake
   machines (board_move_ceremony.rs:676+), `tests/cli_verbs.rs` move
   cases, `tests/it_docker.rs` (skipped locally, CI runs it), the sync
   verb tests. No test is edited in this step — that is the pin.
3. **App arm** (`tests/ui_settings.rs`, the Publish section's harness
   verbatim: `booted_workspace` + `spawn_ui_request_forwarder`):
   - `move_refuses_a_files_bound_board_before_any_io` — files-bound
     fixture → `BindRefused`, binding intact, unreachable server
     proves no IO (the P2 pattern).
   - `move_refuses_an_unbound_board_before_any_io` — `BindRefused`.
   - `move_refuses_when_files_sync_is_disabled` — config without
     `[files_sync]` → `FilesSyncDisabled`, unreachable server.
   - `move_reports_a_queued_outbox_typed` — Deck-bound board with a
     queued op → `OutboxNotQuiescent` (pre-network: the guard fires
     before the floor publish; mount only the Deck-pull mock and assert
     the typed class + the binding intact).
   - **Happy path** `move_commits_a_deck_board_onto_files`: wiremock
     Deck server (`mount_standard_pull`) + WebDAV mounts for
     probe/put/confirm (the `tests/files_sync_convergence.rs` mock
     helpers) → reply `Ok`, snapshot's row `bound_connector ==
     "files-sync"`, engine terminal state
     `binding.board.as_files().is_some()`, phase Idle. Terminal-state
     assertions only (no sleeps; the ceremony's reply implies
     settlement). *Fallback recorded*: if the convergence helpers
     cannot serve the four WebDAV steps without re-mounting their
     whole grammar, this test drops to the ceremony machine's
     `run_until` coverage + the refusal tests, and the deviation is
     reported — the refusals already prove request → arm → guard →
     typed reply.
4. **Adapter journeys** (`tests/journeys.rs`, the JP pattern at
   :9738): `a_files_move_pick_routes_move_board_to_files` — exactly one
   `MoveBoardToFiles` carrying the picked board, busy gate set, the
   Files row's `busy` marker on; Ok reply clears gate + `SettingsOutcome::Ok`;
   stale item/option indexes absorbed; Deck rows' publish picks
   unaffected. Plus `the_files_card_lists_files_bound_boards` —
   mutated `AppState` with a files binding renders a `FilesBoard` row
   titled by the local board, and a Deck-bound board does NOT render
   one (binding-scoped equality).
5. **Seam inventory** (`tests/seam_inventory.rs`): the
   `on_settings_move_picked` entry.
6. **Status copy** (`settings_status` test module): the new variants
   map to their sentences (structural: variant → non-empty mapping
   exists; no text assertions beyond the module's existing convention).

Suites that must stay green untouched: the move ceremony tests,
`tests/cli_verbs.rs` (move/publish), the settings tests
(`tests/ui_settings.rs` existing), the chunk-3 label/Files-row tests,
the journeys publish tests.

## 3. Risks

- **The sync-path re-key** (D-3) is the chunk's widest edit — it touches
  `commands/sync.rs` used by several verbs. Mitigation: `sync_on`'s
  signature is unchanged (delegation), so only the ceremony's call
  sites move; the sync verb tests pin the delegation.
- **The happy-path WebDAV mount** may cost more test scaffolding than
  the arm itself; the recorded fallback (step 3) keeps the chunk honest
  without a fake-coverage ceiling (AGENTS §5: floors on garbage input
  are fine; this is a coverage-shape decision, not an assertion cap).
- **A 60 s busy window** on a slow Deck server blocks the settings
  page's controls — the same exposure PublishBoard accepted
  (`PUBLISH_SETTLE`); the gate is cancellable by design (reply or
  process exit), and the runbook documents the wait. Accepted; not a
  new risk class.
- **Files-bound rows render with files sync Off** (D-6) could surprise a
  user who toggled Files off expecting the boards to vanish — they
  don't (the binding persists; the runbook §2b documents the frozen
  sync). The honest rendering is the decision; the alternative (hiding)
  would fake world state.

## 4. Estimated diff

~550–750 lines including tests: ceremony re-key ~120, sync re-key ~60,
requests/models/settings_status ~120, adapter + presentation ~120,
slint view + copy ~80, tests ~250–350 (journeys dominate).

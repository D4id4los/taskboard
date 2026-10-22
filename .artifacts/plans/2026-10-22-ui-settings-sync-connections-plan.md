# [Plan] Settings view with sync connection management (GUI review round 1, chunk 3)

- **Date**: 2026-10-22
- **Status**: PROPOSED — stage-1 chunk plan for dispatch chunk 3
  (`feat/ui-settings-sync-connections`), written against
  `origin/main` @ `f8db641` (chunk 2 merged — precondition gate
  verified). Design round:
  `.artifacts/plans/2026-10-22-gui-review-round1-plan.md` §8
  (D24b–D28b, normative), §10 (testing), §14 (R4/R5); dispatch plan
  §1/§4 (chunk table, acceptance, code-quality mandate).
- **Scope guard**: touches only `taskboard-ui-slint` +
  `taskboard-app`. The four other crates must show an **empty diff**
  (`git diff main...HEAD -- crates/taskboard-domain
  crates/taskboard-state crates/taskboard-storage-sqlite
  crates/taskboard-sync-nextcloud` asserted at review); the S-series
  snapshots must not move. No sync-actor or sync-crate changes:
  attach/detach already exist (F-C). No new credential-store backends,
  no free-form `taskboard.toml` editor (#351 stays open), no
  restart-requiring settings (D27b).

## 1. Verified facts (anchored; do not re-derive)

Line references verified on `origin/main` @ `f8db641`.

### 1.1 The request lane (chunk 2's deliverable — this chunk extends it)

- **F-A1** `mount(state, signals, system, commands, requests,
  settings)` — the lane sender reaches the UI
  (`crates/taskboard-ui-slint/src/lib.rs:153-158`). The adapter holds
  `requests: mpsc::Sender<UiRequest>` and sends with `try_send`,
  warn-and-drop (`src/adapter.rs:66-68,265-278,374-381`).
- **F-A2** `UiRequest` (`src/requests.rs`) is one variant today
  (`StackWidthsChanged { board, widths }`), derives
  `Debug, Clone, PartialEq, Eq`, "no serde".
- **F-A3** App side: `ui_request_channel()` (capacity 16) +
  `spawn_ui_request_forwarder(rx, config_write_path, shared_state)`
  (`crates/taskboard-app/src/ui_requests.rs:26-77`) — debounce
  (`UI_REQUEST_DEBOUNCE` = 500 ms, `tokio::time::timeout` under the
  paused clock), write-time prune, flush via
  `overlay_stack_widths` → `crate::config::overlay_document` +
  `write_owner_only_atomic` (`src/config.rs:1168,1186`). Single-owner
  rule (R6 of chunk 2): this task owns every write of the
  `[ui.stack_widths]` subtree.
- **F-A4** Two spawn sites: `run_desktop`
  (`src/ui.rs:299-306`, `config_write_path` parameter since chunk 2)
  and the kiosk appliance (`src/main.rs:509-514`). Both own the booted
  `App`.
- **F-A5** The Tier-2 harness drains the lane directly
  (`tests/journeys.rs:34-37,63-80`: `requests_rx` as harness surface).

### 1.2 The ceremonies (the extraction target)

- **F-B1** One-shot `connect` (`src/commands/connect.rs`):
  `run()` = D2 backend guard → `gather_identity` → D3 re-point guard →
  `verify` (`:394-404`: `DeckClient::new` + `boards()`, the
  credential-verification call) → `persist` (`:409-444`: in-memory
  merge + `validate()` + `write_config_overlay`) → `store_secret`
  (`:451-474`: store new, then delete replaced identity tolerating
  `NotFound`/`Unsupported`). Nothing persists unless verification
  passed.
- **F-B2** `write_config_overlay` (`connect.rs:523-588`) writes the
  **scalar world** (`[nextcloud]` keys) or **entry 0 only** (array
  world) — by contract and by tests. It cannot address a named entry.
- **F-B3** Session layer (`src/session_ceremonies.rs`, ADR 0014):
  `connect_session` = ceremony → config refresh → detach-first the
  default → `attach_connector` → `wait_for_boot_pull` settle;
  `logout_session` = `logout::run` → `detach_connector` → refresh;
  `attach_from_persisted` (`:235-340`) is the daemon-side attach with
  the same swap-first rule; `deck_client_from_spec` (`:215-225`) is
  the shared client builder.
- **F-B4** One-shot `logout` (`src/commands/logout.rs`):
  `run()` = require identity → `store.delete` (`NotFound` idempotent,
  `Unsupported` typed refusal) → strip `[nextcloud]` identity keys iff
  backend auto (`strip_config_keys`, `:182-211`, same
  read-modify-write + owner-only-atomic mechanism).

### 1.3 The attach/detach primitives

- **F-C1** `App::attach_connector(name, client)`
  (`src/bootstrap.rs:314-380`: name guard → engine rewire →
  `spawn_sync_actor` over parked-reports clone → register → authority
  hook), `detach_connector` (`:205`), `detach_connector_named`
  (`:265`), `connector_attached` (`:186`). The registry is
  `connectors: std::sync::Mutex<Vec<SyncActor>>` (`:145`); the lock is
  never held across `.await`; `ensure_name_free`/`try_register`/
  `stop_actor` are already free functions over the mutex.
- **F-C2** `wait_for_boot_pull(app, timeout)`
  (`src/bootstrap.rs:903`, `pub(crate)`) — the bounded,
  offline-first settle both ceremonies share.
- **F-C3** Typed outcomes: `AttachError`
  {`ConnectorNameInUse`, `Engine`, `Draining`, `Credential`, `Client`},
  `DetachOutcome` {`Detached`, `NoConnector`, `TimedOut`}.
- **F-C4** `crate::connectors::configured_connectors(&AppConfig)`
  (`src/connectors.rs:169`) is the single source of "which connectors
  exist" — it synthesizes `nextcloud` from the legacy scalar table
  (`DEFAULT_CONNECTOR_NAME`, `:40`) and passes the array through in
  file order. `ConnectorSpec { name: ConnectorId, backend,
  server_url, username }` (`:148-157`).

### 1.4 Config facts

- **F-D1** `ConnectorCfg { name, backend, server_url, username }`
  (`src/config.rs:242-253`), `deny_unknown_fields`, secret-free by
  construction (`config.rs:4-9`).
- **F-D2** The validation ladder for entries
  (`AppConfig::validate`, `config.rs:839-909`): shape rejections first
  (`ConnectorShapeMixed` `:636`, `NextcloudTableWithConnectors`
  `:642` — **the legacy-singleton exclusion**), then per entry
  `is_kebab_name` (`:1104-1112`), unique names
  (`DuplicateConnectorName`), `backend != Standalone`,
  `reqwest_host_ok(server_url)` (the `DeckClient::new` rule),
  non-empty username.
- **F-D3** `UiSection` (`config.rs:342-357`): `scale` (validated
  `0.5–3.0` finite, `:958-961`), `keymap { preset, overrides }`,
  `stack_widths` (chunk 2). `KeymapPreset` {Standard, Emacs, Vi}
  (`:371-378`). `write_path` (`:770`) resolves the write target.
- **F-D4** `CredentialStoreKind` {Keyring, Env, File}
  (`config.rs:141-155`); secrets resolve through the `CredentialStore`
  port (`AppPassword` newtype). `url_is_cleartext` (`:1120`) is the
  informational cleartext flag's classifier.

### 1.5 UI facts

- **F-E1** `ViewId` {Board, StackDetail, TaskDetail}
  (`src/view_state.rs:26-31`); the cycle is `step_view`'s literal
  `order` array (`:258-271`); `context()` maps view → keymap context
  (`:210-216`); `reconcile` (`:764`) seeds/repairs the board
  selection, refresh-time, the one normalization site.
- **F-E2** `AppSettings` registry row (`src/actions.rs:339-348`):
  `enabled: false`, `menu_rank: 10`, `menu_group: None` (top-level
  first row), contexts GLOBAL, description states "not yet available".
  The menu projection (`src/menu.rs:81-110`) needs no change to
  surface the row live.
- **F-E3** `Ctrl+,` is **already bound** to `AppSettings` in all
  three presets (`src/keymap/presets.rs:68` standard, `:118` emacs
  `Ctrl+x c`, `:196` vi `Z s`) — the "un-reservation" is purely the
  `enabled` flip plus dispatch.
- **F-E4** The router's disabled arm
  (`src/router.rs:191-197`) absorbs `AppSettings` today. `UiEffect`
  (`:30-82`) is the exhaustive effect vocabulary; `shell.apply`
  (`src/shell.rs:86-146`) records nothing for view-intent effects.
- **F-E5** The context chain (`shell.rs:54-68`): menu open → `[Menu]`
  (pure modal); insert → `[ViInsert, Global]` (printables reach the
  focused `TextInput` untouched — the D18 text-input contract,
  `ui/widgets/text-field.slint:1-6`); otherwise
  `[view?, ViNormal, Global]`. Mask declarations: Menu only
  (`keymap/mod.rs` contract test `presets_mask_only_the_menu`).
  Resolution walks the chain, first match wins.
- **F-E6** The repaint driver (`lib.rs:305-350`): a repeating slint
  Timer draining `signals` and `system` with `try_recv`, setting
  `dirty` → `refresh()`. `SystemEvent`s are refresh triggers only
  today.
- **F-E7** Models push (`src/models.rs:106-154`): reconcile → one-way
  push → id tables. The status bar reads the *selected board's* sync
  row (`models.rs:357-372`) through
  `presentation::sync_badge_for_board`. The View slint enum
  (`ui/app-window.slint:36-40`) mounts the three views conditionally
  (`:145-180`).
- **F-E8** Domain truth for status: per-board `SyncStatus { phase:
  SyncPhase, .. }` (`state.rs:49-51`, phases Idle/Offline/Failed/
  Syncing at `:144`); `AppState::bound_boards_of(connector)`
  (`state.rs:391`) attributes boards to a connector from the bindings
  (`RemoteBoardBinding.connector`, `ids.rs:134`);
  `SystemEvent::ConnectorLost/Restored(ConnectorId)`
  (`messages.rs:332-334`).
- **F-E9** `UiSettings` (`lib.rs:52-77`) carries scale, keymap
  (built), startup findings, kiosk, stack_widths. Resolved app-side in
  `resolve_ui_settings` (`src/ui.rs:186-215`).

### 1.6 The gates that must stay green

- **F-F1** Connect/logout E2E: `tests/cli_connect.rs` (c13–c17 happy
  path, 401/403 no-persist, forced repoint, cleartext flag, env store,
  comment-preserving overlays — wiremock-backed), `tests/cli_login.rs`,
  the REPL session suites (`tests/repl.rs`), `tests/cli_attach.rs`
  (daemon attach). These run **unchanged** as the extraction gate (R4).
- **F-F2** Config suites (`tests/config.rs` + `config.rs` mod tests:
  shape rejections, round-trips, owner-only modes), bootstrap
  attach/detach tests, `ui_requests.rs`'s debounce/overlay tests —
  all extend, none weaken.

## 2. Decisions

### D1 — Ceremony extraction boundary (R4)

New module `crates/taskboard-app/src/connections.rs` (SPDX
MIT OR Apache-2.0) holding the **non-interactive ceremony core**,
extracted by *moving bodies verbatim* (extract, don't rewrite):

- `verify_credentials(server: &str, user: &str, password: &str)
  -> Result<(), CommandError>` — the body of `connect.rs::verify`
  (`F-B1`), parameterized by fields instead of `Identity`. The
  one-shot's `verify(&identity)` becomes a one-line delegate; the
  `deck_variant` log helper moves with it.
- `store_and_rotate_secret(store, server, user, password,
  old: Option<(&str, &str)>) -> Result<String, CommandError>` — the
  body of `connect.rs::store_secret` (`F-B1`), same parameterization;
  the one-shot delegates.
- **Named-entry overlay ops** (new code, but built on the shared
  chunk-2 helpers `overlay_document` + `write_owner_only_atomic` —
  the `toml_edit` read-modify-write pattern is not duplicated):
  - `overlay_upsert_connector(path, &ConnectorCfg) -> io::Result<()>`
    — array world only: replace the entry with the same `name` (in
    place, preserving document order) or append; the scalar world is
    refused typed (io `InvalidData`) as the writer-level backstop.
  - `overlay_remove_connector(path, name) -> io::Result<bool>` —
    removes the named entry; removing the last entry removes the
    `connectors` key entirely (no empty husk — the
    `[ui.stack_widths]` removal precedent; `Some(vec![])` would be
    equally valid, the key-less standalone world is cleaner).
    Unknown name → `Ok(false)`.
  - `overlay_set_ui_scale(path, f64)` /
    `overlay_set_keymap_preset(path, preset-word)` — bare `[ui]`
    subtree writes preserving every other key including
    `[ui.keymap] overrides`.
- **REPL call sites**: `connect.rs` keeps its public surface (`run`,
  `ConnectArgs`, prompts) and delegates to the extracted core.
  `logout.rs` is untouched. `session_ceremonies.rs` is untouched (it
  already sits at the right layer).

**Byte-stability argument (R4)**: every moved body is copied
verbatim; only the homes and parameter shapes change. The unchanged
`tests/cli_connect.rs` / `cli_login.rs` / `repl.rs` / `cli_attach.rs`
suites and the in-file unit tests are the gate (F-F1). The
`write_config_overlay` entry-0/scalar writer is **not** touched at
all (see DEV-2).

### D2 — Request/reply protocol (D25b, AGENTS §3 query pattern)

`UiRequest` (UI crate, still no serde) gains:

```rust
TestConnection  { target: ConnectionTarget, reply: oneshot::Sender<Result<(), SettingsError>> }
SaveConnection  { profile: ConnectionProfile, reply: oneshot::Sender<Result<Vec<ConnectorInfo>, SettingsError>> }
RemoveConnection{ name: String, reply: oneshot::Sender<Result<Vec<ConnectorInfo>, SettingsError>> }
SetKeymapPreset { preset: Preset, reply: oneshot::Sender<Result<KeymapUpdate, SettingsError>> }
SetScale        { scale: f64, reply: oneshot::Sender<Result<(), SettingsError>> }
```

- `ConnectionProfile { name: String, server_url: String,
  username: String, password: Secret }`; `ConnectionTarget` is the
  same shape without `name` (a row test addresses an existing
  identity). **`Secret(String)` newtype with a redacted `Debug`
  (`"**redacted**"`) and no `Display`** — the structural
  secret-never-in-logs test asserts
  `format!("{request:?}")` contains no secret substring (T6).
- `ConnectorInfo { name, server_url, username }` — the secret-free
  list row; also added to `UiSettings` as `connectors:
  Vec<ConnectorInfo>` (resolved at mount via
  `configured_connectors`, F-C4) so the list is populated before the
  first open.
- `KeymapUpdate { keymap: keymap::Keymap, findings: u32 }` — the
  app re-resolves the config's overrides under the chosen preset
  (pure `keymap::build`) and hands the rebuilt map back; the adapter
  swaps `self.keymap`.
- `SettingsError` (UI-crate enum, `Clone + PartialEq`, typed
  variants only — R5): `LegacyConfig`, `Invalid { field: Field,
  kind: InvalidKind }` (`Field` {Name, ServerUrl, Username, Password,
  Scale}; `InvalidKind` {Empty, NotKebab, BadUrl, NotFinite,
  OutOfRange, Duplicate}), `NoSecretStored`, `StoreUnsupported`,
  `StoreFailed`, `VerifyRejected` (401/403), `VerifyUnreachable`,
  `VerifyFailed`, `PersistRejected`, `PersistFailed`, `AttachBusy`,
  `AttachFailed`, `Gone` (channel/config lost mid-flight). The
  app-side mapping from `CommandError`/`AttachError`/`CredentialError`
  is one exhaustive function (unit-pinned over every arm).
- **Replies are fire-and-forget-honest**: `let _ = reply.send(..)` —
  a dropped receiver (stale form, UI gone) is logged and never a
  panic; persistence is never undone by a failed reply.
- **Derives shrink**: `UiRequest` loses `Clone, PartialEq, Eq` (a
  oneshot sender is neither). Reconcile the chunk-2 tests/journeys
  that matched lane payloads: switch to `matches!`/field reads in the
  same commit (mechanical; no assertion weakened — T-10–T-12 keep
  their semantics).

### D3 — The app-side handler & the ConnectorControl seam

- `spawn_ui_request_forwarder` gains a fourth parameter:
  `connections: ConnectionWorkspace` where
  `ConnectionWorkspace { config_write_path: PathBuf,
  store: Arc<dyn CredentialStore>, control: ConnectorControl }`.
  Both spawn sites build it (`run_desktop`, kiosk `run_kiosk`) —
  kiosk parity is in scope for plumbing (D12).
- **Loop shape**: the outer `recv()` dispatches on the variant.
  `StackWidthsChanged` keeps the exact debounce/merge/prune/flush
  path. Settings requests are **never debounced** — handled
  immediately (they are one-shot actions with replies). A settings
  request arriving *inside* a width quiet window is executed on the
  spot by the drain loop (match arm inside the `timeout` loop), not
  merged. Recorded: this preserves chunk-2's debounce semantics for
  widths byte-for-byte (T8).
- **Save choreography** (the ceremony, in order, nothing persisted
  before verify): authoritative re-validation (the config rules,
  F-D2, via `AppConfig` shaped in memory + `validate`) →
  `verify_credentials` → `overlay_upsert_connector` →
  `store_and_rotate_secret` (old = the replaced entry's identity when
  an entry with that name existed and its server/user differ) →
  **detach-first, then attach**: if a connector under that name is
  live, `control.detach_connector_named(name)`; then build the client
  **from the draft password** (`DeckClient::new(server, user,
  password)`) — not `store.load` — because those are the exact
  credentials just verified (recorded; sidesteps the env-store
  load-back mismatch) → `control.attach_connector(name, client)` →
  `wait_for_boot_pull` settle (extracted to take
  `(&EngineHandle, Duration)`, `F-C2`) → re-read the config
  (`extract_raw` + `validate` + `configured_connectors`) and reply
  with the fresh `Vec<ConnectorInfo>`.
- **Remove choreography**: typed legacy refusal first (D4) →
  `overlay_remove_connector` → `control.detach_connector_named(name)`
  (idempotent `NoConnector`) → best-effort
  `store.delete(server, username)` of the removed identity
  (`NotFound`-tolerant, the logout semantics) → reply fresh list.
  Board bindings to that connector stay (local operation continues —
  the detach semantic).
- **Test choreography (per-row)**: `store.load(server, user)` →
  `verify_credentials` → reply `Ok(())` or the typed error
  (`NoSecretStored` on `CredentialError::NotFound`,
  `StoreUnsupported` on `Unsupported`).
- **DEV-1 (recorded deviation, reason below)**:
  `crates/taskboard-app/src/bootstrap.rs` extracts a
  **`ConnectorControl`** handle — `pub struct ConnectorControl {
  connectors: Arc<std::sync::Mutex<Vec<SyncActor>>, engine:
  EngineHandle, reports: mpsc::Sender<SyncReport>, system:
  broadcast::Sender<SystemEvent>, actor_config: SyncActorConfig,
  shutdown_timeout: u64, authority: AuthorityHandle }` — with
  `attach_connector` / `detach_connector_named` /
  `connector_attached` as its methods (bodies moved verbatim from
  `App`; `App::connector_control()` hands out a cheap clone; `App`'s
  own methods become one-line delegates so every REPL/CLI/daemon call
  site is untouched). The `connectors` field type becomes
  `Arc<Mutex<..>>`. **Reason**: the request forwarder is a spawned
  task and cannot borrow `&App`; `App::shutdown(self)` by value makes
  an `Arc<App>` + `try_unwrap` fragile (a lingering clone would make
  graceful shutdown *impossible*), while the control-handle split is
  the SRP-conformant seam (process shell vs connector lifecycle) and
  keeps the existing bootstrap tests meaningful unchanged. This
  extends D25b's "callable app-layer functions" with the cloneable
  capability those functions need.

### D4 — The legacy-singleton typed rejection (acceptance item)

The settings Save/Remove handlers refuse when the **resolved config
is the scalar world** (`config.sync.connectors.is_none()`), replying
`SettingsError::LegacyConfig` — before any file/store access. The
list still *displays* the synthesized `nextcloud` entry
(`configured_connectors`, F-C4) but mutating it is refused with the
typed error; the row renders as read-only (a model flag derived from
the same fact, asserted structurally). No silent migration (the
acceptance criterion); `taskboard connect` remains the documented
migration path. Unit test T11 pins it.

### D5 — Settings surface placement (D24b: a `ViewId` excluded from cycling)

- `ViewId::Settings` joins the enum; `step_view`'s `order` array is
  **unchanged** (Board → StackDetail → TaskDetail). `next_view`/
  `prev_view` while `view == Settings` return `Vec::new()` (inert —
  recorded; the cycle never passes through Settings in either
  direction).
- `ViewState` gains `settings_return: Option<ViewId>` (only the three
  cycle views are ever stored). `open_settings()`: if already in
  Settings, no-op; otherwise record the current view and enter
  Settings (`RefreshNow`). `close_settings()`: restore
  `settings_return.take().unwrap_or(Board)` + `RefreshNow` — "returns
  to the **exact previous view**" is this snapshot, not a cycle walk.
- `context()` returns the new `ContextId::Settings` while open.
- `reconcile` keeps running board normalization while Settings is
  open and never leaves Settings (subject validation arms don't fire
  for the Settings view) — unit-pinned (T13).
- The slint `View` enum gains `settings`; `models::push` maps it;
  `app-window.slint` mounts
  `if (root.current-view == View.settings): SettingsView {}` inside
  the view-stack area (the same conditional-mount pattern, F-E7).

### D6 — Entry/exit choreography & the enablement set (D28b)

The exact enablement set, all in the same commit as the registry
snapshot regeneration:

1. `actions.rs`: `AppSettings.enabled = true`; description rewritten
   to state what it does (no "not yet available"); row shape
   otherwise unchanged (rank 10, top-level). Snapshot regenerated.
2. `router.rs`: `AppSettings → UiEffect::OpenSettings` (out of the
   disabled arm; the arm's comment updated); new `UiEffect::
   OpenSettings` variant; `shell.apply` records nothing for it (the
   view-state intent arm, `F-E4`).
3. `adapter.rs` interpret arm: `OpenSettings → view_state.
   open_settings() + RefreshNow` — **toggle semantics**: dispatching
   AppSettings while Settings is open closes it (the recorded
   idempotent-answer for the menu row, `Ctrl+,`, and a second press;
   the menu-invoke path already closed the menu first, D3).
4. `actions.rs`: `ContextId::Settings` (+ `name() == "settings"`);
   presets gain `(AppSettings, ContextId::Settings, ["Escape"])` in
   **all three presets** — Escape closes (dialog semantics; toggle
   route). Global `Ctrl+,`/emacs/vi opener rows unchanged (F-E3).
   No mask declaration is added for Settings (masks stay Menu-only —
   the contract test keeps passing); the chain in Settings is
   `[Settings, ViNormal, Global]`, so vi's `ViNormal`-bound verbs
   stay live (recorded as acceptable: the board beneath remains
   navigable; standard/emacs Board-scoped verbs don't resolve there).
   Preset snapshots regenerate.
5. `keymap` contract tests: the `Settings` context appears in the
   chain-walk tests; `every_preset_covers_the_core_verbs` extended
   with `AppSettings` (it now has a Settings-context binding in every
   preset).
6. The settings view carries a back affordance (chevron
   `IconButton`, task-view precedent) routing the same
   close path via a dedicated `settings-back-clicked` seam.
7. Menu row: live with zero projection changes (F-E2); the Tier-2
   journey pins invoke-closes-menu-then-opens-settings (J2).

### D7 — The connections form model

- UI-crate pure module `src/settings_form.rs`:
  `ConnectionDraft { name, server_url, username, password }` +
  `validate(&draft, existing: &[ConnectorInfo]) -> Result<,
  FormErrors>` mirroring the config rules **structurally**:
  `is_kebab` (the `F-D2` char-class rules re-expressed), non-empty
  username/password, URL host rule (the `DeckClient::new` shape:
  scheme http/https + non-empty host + no userinfo), and
  name-collision only against *other* rows (upsert semantics make
  same-name save an edit — recorded). **Parity vectors**: the same
  fixtures `config.rs`'s validation tests use are asserted against
  the UI mirror in both crates' tests, so drift breaks a test
  (the app-side re-validation stays authoritative; the mirror exists
  so the form can flag fields before a network round-trip).
- Widgets: `text-field.slint` gains `in property <bool> secret` —
  sets `input.input-type: InputType.password` (echo off; the a11y
  label stays the field's address, never the value). The settings
  view (`ui/views/settings-view.slint`, new) composes
  `TextField` rows (name, server, username, password), the row list
  (`ConnectionRowModel { name, server, username, status, read_only }`
  + `SettingsModel { rows, editing, preset, scale }` — names/enums
  finalized at implementation, structure as here), status dots via
  the existing `StatusDot` tone widget, and the Test/Save/Remove
  buttons. Text entry rides the D18 insert-mask contract: a focused
  settings field engages `Mode::Insert` (the adapter's
  focus/edited seams, the create-row precedent); commit/cancel
  returns to Normal. Field text mirrors into `ViewState`'s settings
  draft on `edited` (the buffer precedent) — the draft never leaves
  the process except as the `SaveConnection` payload.
- Save is verify-first (the ceremony invariant, F-B1): the UI's Save
  button is enabled only when `validate` passes; the app still
  re-validates and verifies before persisting anything.
- **Save / Remove / Test replies** drive the form state machine:
  pending flag set on send, cleared on `try_recv` outcome **or**
  closed/lagged receiver (a dead lane must never wedge the form —
  R-reply below). Errors surface as a typed status enum on the model
  (rendered structurally; never asserted by text — R5).

### D8 — Live status projection (F17 + D25b "existing drains")

- **UI-local liveness map**: the adapter gains
  `connector_down: BTreeMap<ConnectorId, bool>` (absent = no
  override). The repaint driver's `system` drain (F-E6) records
  `ConnectorLost → true` / `ConnectorRestored → false` before its
  refresh trigger. No new state plumbing, no new timer (D25b).
- **Row status** (pure, `presentation.rs`): for connector `c` — if
  the map says down, `Down`; else fold
  `state.bound_boards_of(c)`'s phases with the existing badge
  ordering (`Syncing > Failed > Offline > Idle`, the
  `sync_badge_for_board` precedent) into
  `{Syncing, Failed, Offline, Idle}`; no bound boards → `Idle`
  ("attached, nothing to do"). Unit-pinned (T15); the Tier-2 journey
  flips it via injected `SystemEvent`s (J6).

### D9 — Keymap preset & scale flows (D27b live-apply + persist)

- **Preset**: the picker shows `UiSettings.keymap_preset` (new field,
  filled from `ui.keymap.preset`). On pick: optimistic UI marker,
  send `SetKeymapPreset`; the app writes
  `[ui.keymap] preset = "<word>"` via `overlay_set_keymap_preset`
  (overrides subtree untouched, T5) and replies `KeymapUpdate`; the
  adapter swaps `self.keymap` + updates
  `UiStatus.startup-keymap-findings` and refreshes — resolution,
  menu hints, and tooltips re-project from the new map immediately
  (live-apply). Tier-1: `resolve()` flips (vi colon in normal
  context) after the swap (T14).
- **Scale**: the field commits (Return / spinner step) values in
  `0.5–3.0` (UI-side parity constants + finite check); on commit the
  adapter sets `Theme.base-scale` immediately (the mount precedent,
  `lib.rs:152-155`) and sends `SetScale`; the app validates typed
  (`InvalidScale` → `SettingsError::Invalid`) and persists via
  `overlay_set_ui_scale`; an `Err` reply reverts the theme value to
  the previous setting. Deterministic toml float rendering via
  `toml_edit::value(f64)`.
- **No restart for anything in v1** (D27b); nothing here changes
  that.

### D10 — Kiosk parity (D12)

`run_kiosk` builds the same `ConnectionWorkspace` from the appliance
`App` + its resolved store. The kiosk credential store is `file` in
the target deployment (ADR 0008) — the store/attach path is
store-agnostic. Visual QA on the appliance is a Tier-4 checklist row,
not a separate design.

## 3. Testing catalogue (TDD-first; AGENTS §5 / design §10)

Naming: T = Tier-1 unit (pure, no Slint), A = app-layer hermetic
(no UI), J = Tier-2 journey (`tests/journeys.rs` harness), M =
manual/Tier-4. **No text assertions anywhere** (R5): labels,
descriptions, tooltips, and error *messages* are never matched —
typed outcomes, structural facts, a11y labels as addresses only.
No sleeps: the debounce keeps its `start_paused` tests; journeys
advance the mock clock.

App crate (`connections.rs` mod tests + `ui_requests.rs` tests):

- **A1** `overlay_upsert_connector` adds an entry to a hand-written
  `[[sync.connectors]]` file preserving a leading comment and
  unrelated tables; `AppConfig::load` round-trips it (the c17
  pattern).
- **A2** Upsert replaces the named entry in place (order of the
  other entries preserved; no `backend`/`[nextcloud]` keys written).
- **A3** Upsert against a scalar-world file is refused typed
  (io `InvalidData`) — the writer-level backstop.
- **A4** Remove deletes the named entry; removing the last entry
  removes the `connectors` key (no empty husk); unknown name →
  `Ok(false)`, file untouched.
- **A5** `overlay_set_ui_scale`/`overlay_set_keymap_preset` write
  only their key; `[ui.keymap] overrides` and comments survive;
  round-trip through figment.
- **A6** The full Save ceremony over a wiremock mock server (the
  `cli_connect.rs` fixture pattern): verify ok → file gains the
  entry → fake store holds the secret → reply carries the fresh
  list; **the written file contains no substring of the secret**;
  `format!("{request:?}")` contains no substring of the secret (the
  redacted-Debug proof); a failing verify (401) persists nothing and
  replies typed.
- **A7** Dropped-reply: reply receiver dropped before the handler
  answers — no panic, the file write still landed.
- **A8** Interleaving: a `SetScale` request inside a width debounce
  window is executed immediately; the widths flush still lands once,
  merged, at window end (`start_paused`).
- **A9** Legacy rejection: Save/Remove against a legacy
  `[nextcloud]`-only config → `LegacyConfig`, file and store
  byte-untouched.
- **A10** The `SettingsError` mapping table: every
  `CommandError`/`AttachError`/`CredentialError` arm → its typed
  variant (exhaustive match pinned).
- **A11** Extraction equivalence gate: the moved `verify`/
  `store_secret` bodies' existing unit tests move with them;
  `tests/cli_connect.rs`, `tests/cli_login.rs`, `tests/repl.rs`,
  `tests/cli_attach.rs` run **unchanged** and green.
- **A12** `ConnectorControl`: the existing bootstrap attach/detach
  tests pass unchanged through the delegating `App` methods (API
  unchanged); a direct control-handle test covers the same flow
  without an `App`.

UI crate:

- **T1** `settings_form::validate` truth tables (kebab, empty
  fields, URL shapes incl. userinfo rejection, collision-against-
  other-rows) — fixtures shared with `config.rs`'s validation tests
  (parity).
- **T2** `ViewState`: `open_settings` records the return view;
  `close_settings` restores it exactly (Board default);
  `next_view`/`prev_view` are inert while open; `context()` is
  `Settings`; `reconcile` while open keeps it open and still
  normalizes the board.
- **T3** Preset swap: applying a `KeymapUpdate` changes `resolve()`
  (vi `:` in normal context resolves; standard's doesn't) and the
  findings counter.
- **T4** The live-status fold: bound-board phases → row status per
  the ordering; the down-override wins; empty bindings → Idle.
- **T5** The secret redaction: `Secret`'s `Debug` output never
  contains the payload (structural, property-fed with adversarial
  secrets — `proptest` over printable strings).
- **J1** `Ctrl+,` from Board opens Settings; Escape closes onto
  Board; the back affordance closes too.
- **J2** Menu row "Open settings" invoke: menu closes first, then
  Settings opens (D3 × D28b).
- **J3** From StackDetail: open → close returns to **StackDetail**
  (exact previous view).
- **J4** Cycle exclusion: with Settings open, `nav.next-view`/
  `nav.prev-view` change nothing; from the return view, cycling
  Board → Stack → Task → Board never lands on Settings.
- **J5** Form journey: keystrokes into the fields (insert-mask
  contract), Save routes `SaveConnection` on the drained lane —
  typed payload asserted by field reads; `format!("{req:?}")` shows
  no secret; a second Save with the same name is an edit (upsert
  payload).
- **J6** Inject `ConnectorLost` → the row's status enum flips;
  `ConnectorRestored` restores it.
- **J7** Preset picker → `SetKeymapPreset` on the lane; the lib-level
  test applies a `KeymapUpdate` reply through the drain path and
  observes resolution change (the reply lane's UI half).
- **Snapshot**: `keymap_snapshots__registry.snap` +
  `resolved_preset_keymaps.snap` regenerate **in the same commits**
  as the registry/preset changes (the chunk-2 precedent).

Manual / Tier-4 (recorded in the PR body):

- **M1** Against a real Nextcloud (or the docker profile): add,
  test, save (appears in-app without restart), restart → still
  configured and attached at boot; remove → detached without
  restart.
- **M2** REPL coexistence: `taskboard connect` from a second process
  while the GUI runs — GUI keeps working; file stays valid (the
  documented cross-process last-writer-wins exposure, R6).
- **M3** Kiosk appliance: settings reachable, file-store save works.
- **M4** Window stability with the settings view mounted at the min
  floors (the D15 invariant: no view switch resizes the window).

## 4. Implementation slicing (commit order; every commit standalone)

1. `refactor(app)`: extract `verify_credentials` +
   `store_and_rotate_secret` into `connections.rs`; `connect.rs`
   delegates (A11 green, suites unchanged).
2. `feat(app)`: the named-entry + `[ui]` overlay ops (A1–A5).
3. `refactor(app)`: the `ConnectorControl` extraction (A12).
4. `feat(app)`: `ConnectionWorkspace` + the settings request handlers
   in the forwarder; both spawn sites wired (A6–A10).
5. `feat(ui-slint)`: `ContextId::Settings`, enablement set, `ViewId::
   Settings` placement + view-cycle exclusion, stub settings view
   mounts (T2, J1–J4, snapshots in-commit).
6. `feat(ui-slint)`: the settings view (form, list, status, reply
   drain, secret redaction) (T1, T3–T5, J5–J7).
7. `feat(app)` + `feat(ui-slint)` remainder: preset/scale
   persist+live-apply completion (A5 consumer, T3 consumer) — folded
   into 4/6 where the commit stays standalone.

## 5. Risks

- **R4 (extraction)** — mitigated by verbatim moves + the unchanged
  E2E gate (A11); the only semantic delta allowed is none: any drift
  found while moving is a bug to fix, not a rewrite opportunity.
- **R5 (text assertions)** — every new assertion in §3 is typed or
  structural; the status/row *labels* are markup-owned.
- **R6 (overlay writer contention)** — connector identity keys now
  have two writers (REPL ceremony, settings forwarder). Recorded
  position: each write stays atomic (temp + rename + owner-only), the
  settings path re-reads the file per request, and no cross-process
  lock is added — the same last-writer-wins exposure two concurrent
  CLIs already have (M2 documents it). The single-owner rule of
  chunk 2 is *extended*, not weakened: the `[ui.stack_widths]`
  subtree still has exactly one writer.
- **R-reply (new)** — reply-lane liveness: the form clears its
  pending state on closed/lagged receivers too (D7); a dead
  forwarder degrades to "changes stay UI-local", never a wedged UI.
- **R-cycle (new)** — the vi `ViNormal` bindings staying live in the
  Settings context (D6.4) is a recorded acceptability, not an
  oversight; revisit only if a review finding shows a harmful
  interaction with open fields (insert mode masks them anyway).

## 6. Deviations from the design round (each with a recorded reason)

- **DEV-1 (extends D25b)**: the `ConnectorControl` handle extraction
  in `bootstrap.rs`. Reason in D3 — the forwarder task needs a
  cloneable connector-lifecycle capability; `Arc<App>` +
  `try_unwrap` would put graceful shutdown at the mercy of task-drop
  timing, and the split is the SRP line the dispatch mandate draws
  (process shell vs connector lifecycle). REPL/CLI/daemon call sites
  unchanged.
- **DEV-2 (refines D25b's "write_config_overlay reuse")**: settings
  writes use **new** named-entry/`[ui]` overlay ops in
  `connections.rs` built on the shared `overlay_document`/
  `write_owner_only_atomic` helpers, because the existing writer is
  entry-0/scalar-only *by contract and tests* (F-B2). The REPL's own
  writer stays byte-untouched — that is what makes R4's
  byte-stability argument honest.
- **DEV-3 (fills D24b's "a test button")**: two test surfaces —
  the per-row Test (stored secret via `store.load`) and Save's
  verify-first step (draft credentials). Reason: Save must honor the
  ceremony's nothing-persists-before-verification invariant; a row
  test must not require retyping a password.
- **DEV-4 (fills D24b/D25b's legacy gap)**: the legacy scalar world
  is displayed read-only and refused typed on mutation
  (`LegacyConfig`) — exactly the acceptance wording, made concrete.

## 7. Estimated diff size

- `taskboard-ui-slint` ≈ 1.4–1.7k lines: `settings-view.slint` ~350
  new, `adapter.rs` +250 (seams, reply drain, liveness map),
  `view_state.rs` +130, `settings_form.rs` ~180 new, `models.rs` +90,
  `requests.rs` +70, `router.rs`/`actions.rs`/`presets.rs` +60,
  `text-field.slint` +8, journeys/tests +450.
- `taskboard-app` ≈ 0.9–1.1k lines: `connections.rs` ~320 new,
  `ui_requests.rs` +250, `bootstrap.rs` ±90 (moved), `ui.rs`/
  `main.rs` +40, tests +400.
- Total ≈ **2.4–2.8k lines** including tests — an L chunk, as
  dispatched.

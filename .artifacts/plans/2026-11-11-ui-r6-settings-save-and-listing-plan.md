# [Plan] UI R6 Chunk 2 — Settings Save + Listing (`#721`, D-6.5 + D-6.7)

- **Date**: 2026-11-11
- **Kind**: stage-1 chunk plan (TDD-first; no code in this commit).
- **Origin (normative)**: `.artifacts/plans/2026-11-11-gui-review-round6-plan.md`
  §3 D-6.5 + D-6.7, §2 rows R6-5/R6-7; dispatch plan
  `.artifacts/plans/2026-11-11-ui-r6-dispatch-plan.md` §1/§3; GitHub issue
  #721 (root cause verified).
- **Branch**: `fix/app-settings-save-and-listing` (worktree
  `../taskboard-WT-fix-app-settings-save-and-listing`, created off `main`
  @ `67a5b03d`; worktree-doctor green, 183G free).
- **Anchor drift note**: the origin plan's file:line anchors were read at
  `53ffa1fc`/`ca961b3d`; they had drifted by planning time. All anchors
  below were re-verified against `main` @ `67a5b03d`.

## 1. Verified seams (re-anchored @ 67a5b03d)

| Seam | Where |
|---|---|
| `ensure_migrated` | `crates/taskboard-app/src/ui_requests.rs:1236-1259` — `Ok(false)` from `migrate_config_file` unconditionally → `PersistRejected` (the #721 bug) |
| `read_raw_config` | `ui_requests.rs:1223-1225` — `AppConfig::extract_raw(Some(write_path))`, **env layer included** |
| `save_connection` | `ui_requests.rs:727-~805` — ceremony order untouched (non-goal) |
| `list_remote_boards` | `ui_requests.rs:835-853` — pure query, unchanged |
| `connectors_item` | `crates/taskboard-app/src/connections.rs:99-127` — doc-level identity-husk predicate at `:109-112`; missing-array → `InvalidData` refusal |
| `overlay_upsert_connector` | `connections.rs:192-243` — navigates both array renderings; `Upsert::NotAnArray` refusal |
| `migrate_document_inner` | `crates/taskboard-app/src/migration.rs:109-141` — the subtree-creation precedent (`or_insert` `[sync]`, fresh `ArrayOfTables`, push) |
| `migrate_config_file` | `migration.rs:193-214` — absent file → `Ok(false)`; no legacy keys → `Ok(false)` |
| `settings_view_mounted` | `crates/taskboard-ui-slint/src/adapter.rs:5959-5975` — clears drafts, `push_settings`; no listing today |
| `settings_refresh_item` (manual Refresh) | `adapter.rs:5274-~5300` — guard `pending.is_some()`, `send(...)` with `PendingKind::ListRemote` + `PendingReply::Remotes` |
| `PendingReply::Remotes` / `PendingKind` | `adapter.rs:3834-3876`; the drain arm `adapter.rs:4334-4349`; the settle block `:4443-4451` clears `pending`/`busy_target`/`busy_remote` + routes the outcome **for every settled reply** |
| `SettingsVm` | `adapter.rs:3920-3977` — `remotes: BTreeMap<String, Vec<RemoteBoardRow>>` (`:3928`), single `pending: Option<PendingKind>` (`:3939`), `busy_target` (`:3936`) |
| `connection_items_model` | `adapter.rs:4643-4756` — per connector: Deck row (`listed: listing.is_some()`) then the listing's Remote rows; bindings resolved from live state |
| `ItemAddr::Remote` | `adapter.rs:3891-3895` — `{ connector, remote: RemoteBoardId, bound: Option<BoardId> }` |
| Env identity | `TASKBOARD_NEXTCLOUD__SERVER_URL` / `TASKBOARD_NEXTCLOUD__USERNAME` (figment `split("__")`, `config.rs:33, 1890`) — visible through `read_raw_config` |
| Must-not-break tests | `crates/taskboard-app/tests/ui_settings.rs` (1604 lines; notably `a_legacy_file_migrates_ahead_of_the_save_ceremony`, `a_legacy_file_on_unwritable_media_refuses_persist_failed`, `remove_on_a_legacy_file_migrates_then_removes_the_sole_entry`, `the_listing_projects_live_rows_and_changes_nothing`, `a_rejected_listing_is_a_typed_verify_refusal`) |

## 2. Decisions (every open question settled)

### D1 — the identity-husk predicate: typed helper in `ui_requests.rs`, doc-level predicate stays in `connections.rs`; cross-referenced, not unified

`ensure_migrated`'s new refusal condition is evaluated on the **typed**
`AppConfig` it already holds:

```rust
/// The scalar-world corner: an identity beside no connectors array —
/// env-injected identity or partial legacy keys. The same predicate
/// `connections::connectors_item` keys on at the document level.
fn has_legacy_identity(raw: &AppConfig) -> bool {
    raw.nextcloud.server_url.is_some() || raw.nextcloud.username.is_some()
}
```

**Why not one shared helper**: the writer's predicate
(`connections.rs:109-112`) operates on the raw `toml_edit` document
*pre-decode* — it cannot construct an `AppConfig` (a mixed
identity-beside-array file decode-rejects, which is exactly why the
writer checks at doc level). Unifying would mean re-parsing the file as
`toml_edit` inside `ensure_migrated` (a second parse per save) or moving
typed logic into the writer (wrong layer). The two predicates provably
agree on every input that matters: a present key with a value lands as
`Some` in the typed config; a `credential_store`-only husk lands as
`None`/`None` at the typed level and passes both. The named typed
helper carries the cross-reference comment; `connectors_item`'s comment
gains the mirror reference. **Decision recorded: two predicates, two
levels, one shared name (`has_legacy_identity`) documented as the typed
half.**

### D2 — `ensure_migrated`'s new rule

```
connectors present        → Ok(raw)                      (unchanged)
migrate → Ok(true)        → re-read, log, Ok             (unchanged)
migrate → Ok(false):      → has_legacy_identity(raw)?
                              yes → PersistRejected      (the corner stands)
                              no  → Ok(raw)              (THE FIX: fall
                                                          through to the
                                                          writer, which
                                                          creates the
                                                          subtree; the
                                                          writer already
                                                          creates file +
                                                          directory)
migrate → Err             → PersistFailed                (unchanged)
```

`save_connection` needs **no change**: it already handles the
`raw.sync.connectors == None` case gracefully (`old` at
`ui_requests.rs:765-774` iterates `.iter().flatten()`), and
`overlay_upsert_connector` is the next step of the ceremony. Doc comment
of `ensure_migrated` rewritten to the new rule (the old text pins the
bug: "non-migratable scalar world … → typed `PersistRejected`" now
covers only the identity-husk corner).

### D3 — subtree creation in `overlay_upsert_connector`: new `ensure_connectors_item` helper; `connectors_item` stays for the remover

**`connectors_item` is not the only navigation of that region** — the
remove path (`overlay_remove_connector`, `connections.rs:249+`) must
keep refusing (you cannot "remove into existence"). **Decision:**

- New private helper `ensure_connectors_item(doc: &mut DocumentMut) ->
  io::Result<&mut toml_edit::Item>`:
  1. identity-husk check — verbatim move of `connectors_item`'s
     predicate + refusal (the comment travels with it);
  2. `sync.connectors` present → return it (both renderings, unchanged
     navigation);
  3. absent → **create**, mirroring `migrate_document_inner`
     (`migration.rs:118-128`):
     - `doc.entry("sync").or_insert(Item::Table(Table::new()))` —
       `or_insert` preserves the existing `[sync]` table's decor when
       `[sync]` exists but `connectors` doesn't; a fresh table when
       neither exists;
     - `sync.insert("connectors", Item::ArrayOfTables(ArrayOfTables::new()))`.
     Creation always uses the **array-of-tables rendering** (the
     migration precedent; the plain-inline-array rendering is only
     navigated, never minted).
- `overlay_upsert_connector` swaps its `connectors_item(&mut doc)?` call
  for `ensure_connectors_item(&mut doc)?`. The `Upsert::NotAnArray`
  refusal stays (present-but-scalar `connectors` = a non-array value).
- `connectors_item` remains exactly as-is for the remove path; its doc
  comment updated ("the creator's counterpart is
  `ensure_connectors_item`").

Byte-preservation: `toml_edit` round-trip guarantees every other table,
comment, and the mode ride the existing `overlay_document` +
`write_config_atomic` path unchanged — pinned by a round-trip test (T4).

### D4 — the mount auto-list: dedicated `auto_listings` lane, NOT the shared `pending` machine

The origin plan offered "reuse `PendingKind::ListRemote` +
`vm.busy_target` or add per-connector pending". **Decision: neither
reuse of the single-pending machine nor `PendingKind` — a dedicated
per-connector lane:**

- `SettingsVm` gains
  `auto_listings: BTreeMap<String, oneshot::Receiver<Result<Vec<RemoteBoardRow>, SettingsError>>>`
  (connector → parked reply).
- **Why not the shared `replies` vec**: the drain's settle block
  (`adapter.rs:4443-4451`) clears `pending`/`busy_target`/`busy_remote`
  and routes `set_settings_outcome` **for every settled reply**. An
  auto-list reply landing while a Save/Remove/Attach is in flight would
  (a) wipe the in-flight op's busy marker mid-ceremony and (b) stamp a
  global `Ok`/`Unavailable` outcome the user never asked for. The
  existing `PendingReply::Remotes` arm and its settle semantics are
  correct for the *manual* refresh and stay byte-identical.
- **Why not a per-connector busy flag on `PendingKind`**: `pending` is
  deliberately single-slot (the buttons disable while pending; r2 D34).
  Multi-connector mount listing through it would serialize or
  wrongly disable the form.
- **Fire rule** (`settings_view_mounted`, after `push_settings`): for
  every connector name in `vm.rows`, fire `UiRequest::ListRemoteBoards`
  iff ALL of: no `remotes` entry, not a key in `auto_listings`, and not
  the target of the manual lane (`!(pending == Some(ListRemote) &&
  busy_target == connector)`). The send is a bare
  `self.requests.try_send(...)` + `auto_listings.insert(...)` (a full
  lane degrades to a log line, the `send` choke-point precedent); a
  failed try_send inserts nothing. Rapid remounts cannot double-fire:
  the in-flight receiver occupies the map slot until its reply lands.
  **The auto-list never fires at boot** — only `settings_view_mounted`
  calls it (dispatch hard rule).
- **Drain**: new `drain_auto_listings(&mut self, window)` called beside
  `drain_setting_replies` in the same repaint pass. Per settled reply
  (`try_recv`, never blocking): `Ok(Ok(rows))` → `remotes.insert`,
  `Ok(Err(_))` → nothing (the row's `listed` stays false → the refresh
  hint stays), `Closed` → nothing; in all three cases remove the map
  entry. **Never touches `pending`, `busy_target`, or
  `set_settings_outcome`** — a background listing is silent by
  construction; the outcome banner belongs to user-initiated ops only.
  A settled or failed auto-list marks the pass dirty exactly like the
  settings drain (`refresh_forced`).
- A *successful* manual refresh and an auto-list can coexist; the last
  landed listing wins (`remotes.insert` is idempotent per connector).
  The manual Refresh button's own guard and busy rendering are
  untouched.

### D5 — bindings-without-listing Remote item shape

In `connection_items_model`, after the listing loop for a connector:

- Collect the connector's live bindings from the state:
  boards where `!deleted && !archived && binding.connector == connector`.
- Emit a synthesized `ConnectionItem { kind: Remote, bound: true }` for
  each binding whose `binding.board` (a `RemoteBoardId`) is **not**
  among the landed listing's `remote_id`s (structural equality —
  `RemoteBoardId` is `PartialEq`; the listing enriches, never
  duplicates).
- **Title**: the bound local board's `title` (the remote title is
  unknown until a listing lands; when one lands, the binding moves into
  the listing loop where `detail` carries the local title and `title`
  the remote title — the D-6.7 acceptance's "enrichment").
- **`detail`**: empty (nothing to show pre-listing).
- **Address**: `ItemAddr::Remote { connector, remote: binding.board,
  bound: Some(board.id) }` — every existing per-remote action (detach,
  bind-pick) resolves against it unchanged, which is the point: the row
  is fully actionable before any listing exists.
- Ordering: synthesized rows render after the listing's rows, per
  connector, walk order preserved. The Deck row's `listed` flag is
  untouched (it stays the refresh-hint driver).
- A connector with no listing and no bindings renders just the Deck row
  with the refresh hint — today's behaviour, unchanged.

## 3. Test catalogue (TDD-first order)

App crate (`crates/taskboard-app/tests/ui_settings.rs` + writer unit
tests in `connections.rs`'s test module):

- **T1** `a_save_on_a_config_without_a_sync_section_creates_the_subtree_and_attaches`
  — the exact #721 repro: config file with no `[sync]`, no
  `[nextcloud]`; handler-level save → `Ok(snapshot)`, file now carries
  `[[sync.connectors]]` with the draft entry; actor attached.
- **T2** `a_save_with_no_config_file_at_all_creates_file_directory_and_entry`
  — absent file variant; the writer's create-file/create-directory
  contract pinned end to end.
- **T3** `an_env_injected_identity_still_refuses_persist_rejected` —
  `TASKBOARD_NEXTCLOUD__SERVER_URL` (+username) set against a sync-less
  file → the save replies `SettingsError::PersistRejected` and the file
  is untouched. (The corner-survival pin the stage-3 review watches.)
- **T4** writer unit tests (`connections.rs` tests):
  `overlay_upsert_connector` from an absent file (file + dirs + subtree
  created, owner-only mode) and from a sync-less file with comments and
  unrelated tables — a **byte round-trip assertion** that every other
  table and comment survives verbatim; the present-but-scalar
  `connectors` refusal stays; the `[nextcloud]`-with-`credential_store`-
  only husk remains a valid write target (the M11 pin).
- **T5** `ensure_migrated` table via the handler (it is private):
  legacy full identity → migrates then proceeds (already pinned by
  `a_legacy_file_migrates_ahead_of_the_save_ceremony` — stays green,
  untouched); the two new `Ok(false)` fall-through cases are T1/T2; the
  husk corner is T3.

UI crate (`taskboard-ui-slint`, adapter test module):

- **T6** mount auto-list: `settings_view_mounted` with two connectors
  and no listings → exactly two `ListRemoteBoards` requests on the
  request lane; `remotes` gains entries when the replies land
  (`drain_auto_listings`).
- **T7** pending guard: mount → drain-nothing → mount again (rapid
  remount) → still exactly two requests total; after a listing lands,
  the next mount does not re-fire for that connector; the third
  connector-level guard: `pending == ListRemote && busy_target ==
  connector` (manual refresh in flight) suppresses the auto-fire.
- **T8** auto-list failure is silent: an `Err` reply clears the
  `auto_listings` slot, leaves `remotes` empty (refresh hint rides
  `listed == false`), leaves `outcome == None`, and never touches a
  concurrently in-flight manual op's `pending`/`busy_target` (the
  interleaving that motivates the dedicated lane — assert `pending`
  survives a settled auto reply mid-`Save`).
- **T9** bindings render without a listing: state with a board bound to
  a never-listed connector → `connection_items_model` yields the
  synthesized Remote row (`bound: true`, local title, resolvable
  `ItemAddr::Remote`); when the listing then lands containing that
  remote id, the row is **not duplicated** (listing row wins, detail
  enriched); a second binding whose remote is absent from the listing
  keeps its synthesized row.

## 4. Non-goals (inherited + confirmed)

- No ceremony-order change in `save_connection`; no config-format
  change beyond the created subtree; no persistent listing cache; no
  boot-time listing; no backoff on repeatedly failing auto-lists
  (filed as **#741**); no changes to the manual Refresh seam or the
  `PendingReply::Remotes` drain arm's semantics; no remove-path changes
  (its `connectors_item` refusal stands — T4 pins the counterpart).

## 5. Risks

- **R1 — the drain interleaving** (the reason D4 exists): any tempting
  shortcut that parks auto replies in the shared `replies` vec corrupts
  in-flight ops' busy/outcome state. Stage 3's functional focus already
  names the guard; T8 pins it.
- **R2 — behavioural ripple of D2 on `remove_connection`**: it also
  calls `ensure_migrated`; on a sync-less config it now proceeds past
  migration (previously `PersistRejected`). Downstream it still
  resolves no identity and the remover's own refusal handles the
  missing entry — `remove_on_a_legacy_file_migrates_then_removes_the_sole_entry`
  and the existing remove tests must stay green unchanged (they will
  run in the gate suite; any red is a finding, not a pin to weaken).
- **R3 — env-injected identity test isolation**: T3 must lock
  `TASKBOARD_NEXTCLOUD__*` via the workspace's env-guard convention
  (the `xdg_lock`-style serialization the recent config-path tests use)
  — env races with parallel nextest are the flake class to avoid.
- **R4 — anchor drift**: line anchors above are @ `67a5b03d`; stage 2
  re-verifies before editing.

## 6. Estimated diff

App: `ui_requests.rs` ~+25/−8, `connections.rs` ~+45/−6,
`migration.rs` untouched. UI: `adapter.rs` ~+85/−4 (VM field, fire
helper, drain method, model emission), `.slint` untouched (the refresh
hint already rides `listed`). Tests ~+280. Total ≈ **440 lines**.

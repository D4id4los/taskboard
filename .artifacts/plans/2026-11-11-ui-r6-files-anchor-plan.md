# [Plan] UI R6 Chunk 3 — Files-sync anchor choice + backend-aware labels

- **Date**: 2026-11-11
- **Status**: STAGE-1 PLAN (TDD-first; no implementation). Chunk 3 of the
  round-6 dispatch (`2026-11-11-ui-r6-dispatch-plan.md` §1 row 3): origin
  plan `.artifacts/plans/2026-11-11-gui-review-round6-plan.md` §3 **D-6.6
  and D-6.8 phase 1 (normative)**, findings §2 rows R6-6/R6-8. Boundary:
  #723 (still exactly one files-sync lane; the anchor is a *choice*, not
  per-account state).
- **Branch**: `feat/ui-files-sync-anchor` (worktree
  `../taskboard-WT-feat-ui-files-sync-anchor`, created off `origin/main`
  @ `8507f3cb` after `scripts/worktree-doctor.sh` passed: 183G free).
- **Verified against**: the worktree tree @ `8507f3cb`; all anchors below
  were read at that SHA. Line numbers will drift once chunk 2
  (`fix/app-settings-save-and-listing`) lands first — re-verify by
  symbol, the findings are stable.

---

## 1. Scope

One user-visible contract change with four layers:

1. **Config**: `[files_sync]` gains an `anchor` key naming the connector
   whose account the files-sync lane mirrors. Absent = today's behaviour
   (first Nextcloud connector).
2. **Bootstrap**: the files-sync actor spawns under the resolved anchor's
   identity, not unconditionally the first connector's.
3. **Settings UI**: every Nextcloud account card gets the Files row
   (today only the first card); the toggle re-anchors on pick; the row's
   caption names the account semantics.
4. **Backend labels**: `ConnectorInfo` carries the backend; connector
   rows stop reading "Deck" for standalone connectors (D-6.8 phase 1).

Non-goals (binding): no multi-lane sync (#723), no restart-semantics
change (the actor still spawns at boot; a toggle says "next restart"),
no Files-boards surface (chunk 5, #722), no `[files_sync] poll_secs`
UI, no engine work.

## 2. Decisions (every open question settled)

### D1 — Config key + serde shape

`FilesSyncSection` (`crates/taskboard-app/src/config.rs:274-283`) gains:

```rust
/// The connector whose Nextcloud account the lane mirrors (kebab
/// name). Absent = the first Nextcloud connector — the pre-anchor
/// behaviour, so existing configs serialize and decode unchanged.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub anchor: Option<String>,
```

`deny_unknown_fields` stays; `is_default()` needs no change
(`Option::is_none` skip keeps the absent-table placeholder serializing
to nothing). The key name is `anchor` — it names the *account the lane
is anchored to*, matching the origin plan's vocabulary; `connector`
would collide with the lane's own `files-sync` identity
(`FILES_SYNC_CONNECTOR_NAME`, `bootstrap.rs:103`).

**D1a (serde shape)**: plain `Option<String>`, not a newtype. The
kebab-ness is enforced by *matching against configured connectors*
(validation), not by the type — the config file is a trust boundary
where the connector set is only known after decode, so a fallible mint
at the type would need the whole config in scope. Documented leniency
at the field (A1 pattern's documented-leniency clause); the refusal is
typed at validation (D2).

### D2 — Anchor resolution rule + typed validation refusals

**Resolution** — one pure `AppConfig` method beside
`resolved_default_backend` (`config.rs:1365-1372`):

```rust
#[must_use]
pub fn resolved_files_sync_anchor(&self) -> Option<&crate::connectors::ConnectorSpec>
```

Rule, in order:
1. `files_sync.anchor` set **and** naming a configured Nextcloud
   connector → that spec.
2. Unset → the **first Nextcloud connector** in `configured_connectors`
   walk order (not `specs.first()` — the fallback is
   first-*Nextcloud*, which is also what makes `enabled` decodable on a
   config whose first connector is standalone; see D2b).
3. Otherwise (unset and no Nextcloud anywhere; set-and-invalid never
   reaches here — validation refuses first) → `None`.

**Validation** (`validate`, the block at `config.rs:1556-1558`),
deterministic what-is-written checks, first failure typed:

- `anchor` set and naming no configured connector → **new variant**
  `ConfigError::FilesSyncAnchorUnknown { field: &'static str,
  name: String }`. Refused **even when `enabled == false`** (the
  `poll_secs == Some(0)` precedent: a dangling reference is config
  garbage regardless of the mode being off). Wire the variant into the
  `field()` ladder (`config.rs:1154` region →
  `Some("files_sync.anchor")`) and the `code()` ladder
  (`config.rs:1190` region → `"files-sync-anchor-unknown"`).
- `anchor` set and naming a **non-Nextcloud** connector → reuse
  `FilesSyncWithoutNextcloud { field: "files_sync.anchor" }` (same
  cause class — "this key needs a Nextcloud account" — one new variant
  would duplicate it; the `field` discriminates the key). The variant's
  fields stay `&'static str`, so no shape change.
- `enabled == true` → the existing `FilesSyncWithoutNextcloud {
  field: "files_sync.enabled" }` refusal, but the **condition changes**
  from `resolved_default_backend() != Nextcloud` to
  `resolved_files_sync_anchor().is_none()`.

**D2b (deliberate semantic widening, recorded)**: today,
`enabled` + first-connector-standalone + a later Nextcloud connector is
*rejected* (`resolved_default_backend` looks only at `specs.first()`).
After this chunk it **resolves and spawns** under the later Nextcloud
connector. That is the chunk's point (the anchor is a choice), not
drift; the old rejection was an artifact of the scalar key. The
origin-plan sentence "`enabled` requires a resolvable anchor" is
implemented exactly this way.

### D3 — Bootstrap: spawn under the anchor's identity

`files_sync_spawn_decision` (`bootstrap.rs:118-136`): replace
`specs.first()` + backend check with
`config.resolved_files_sync_anchor()` (the re-derive-never-revalidate
contract stands — an unvalidated config yields `None` honestly). The
spec's `server_url`/`username` come from the resolved anchor; the
actor identity stays the fixed `files-sync` kebab
(`FILES_SYNC_CONNECTOR_NAME`) — the lane identity is per-device
substrate, not per-account (D-SF10/#723 boundary).

The existing test `files_sync_spawn_decision_derives_from_the_default_connector`
(`bootstrap.rs:2491`) stays green unchanged: no anchor set + first
connector Nextcloud resolves to the same spec.

### D4 — Snapshot + applies/env metadata wiring

- `SettingsSnapshot` (`crates/taskboard-ui-slint/src/requests.rs:204-206`)
  gains `pub files_sync_anchor: Option<String>` — the **effective**
  (resolved) anchor name, not the raw key: the per-card row needs
  "is this card the anchor", which is a resolved fact; a raw-echo would
  force every consumer to re-resolve. Filled at both minting sites
  (`crates/taskboard-app/src/ui.rs:339` and `:391`) from
  `config.resolved_files_sync_anchor().map(|spec| spec.name.to_string())`.
- **Applies registry**: `"files_sync.anchor"` →
  `ApplySemantics { apply: SectionApply::OnRestart, .. }` (the same
  entry shape as `"files_sync.enabled"`; the registry drives the
  row's `files-restart` caption). Located beside the existing
  registration (grep `files_sync.enabled` in the applies/registry
  builder, `ui_requests.rs` region `:634`).
- **Env honesty**: the dotted-key ladder
  (`ui_requests.rs:634`, `Field::FilesSyncEnabled => "files_sync.enabled"`)
  is per-`Field`, and the anchor has **no** `Field` variant (D5) — so
  the env-override set for the anchor rides the generic
  `AppConfig::env_overridden_keys` scan: `TASKBOARD_FILES_SYNC__ANCHOR`
  must land in `snapshot.env_overridden` as `"files_sync.anchor"`
  (verify the scan is key-generic over the figment env layer; if it
  enumerates a fixed key list, add the key there — record which shape
  was found in the implementation report).
- **`files_locked` semantics** (adapter, D6): the Files row locks when
  *either* key is env-overridden — `env_overridden` contains
  `files_sync.enabled` **or** `files_sync.anchor`. A per-key lock
  display (anchor locked but toggle free) is finer than the row's one
  caption can express; filed as backlog (see §5) — one lock caption
  for the row is this chunk's contract.

### D5 — Request shape: the payload carries the intended anchor

`UiRequest::SetFilesSyncEnabled` (`requests.rs:471-477`) gains the
anchor:

```rust
SetFilesSyncEnabled {
    enabled: bool,
    /// `Some(name)` only when the pick re-anchors (turning On a
    /// non-anchor card). `None` = leave the key untouched.
    anchor: Option<String>,
    reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
},
```

**No new `UiRequest::SetFilesSyncConnector`** (deviation from the
origin plan's sketch, recorded reason): the toggle is commit-on-pick —
enabled and anchor change as **one user action**, and a separate
request would either force two round-trips (a torn intermediate file
state between them) or arrive with ordering hazards through the
single-slot settings pending machine. One request carrying both is the
atomic shape; the origin's `set_files_sync_anchor` overlay writer
**is** delivered (D7), so nothing normative is lost.

No new `Field` variant: the row's field identity remains
`Field::FilesSyncEnabled` (the pending/status/invalid lattice keys off
the row, not the key). The app arm (`ui_requests.rs:515-521`) passes
both values to the overlay writer.

### D6 — Per-card Files row semantics

`connection_items_model` (`crates/taskboard-ui-slint/src/adapter.rs:4643-4762`):

- **Emission**: the Files item is emitted for **every card that
  contains at least one Nextcloud connector** (replacing the
  `default_identity == (server_url, username)` first-card-only gate at
  `adapter.rs:4685`). Backend knowledge comes from `vm.rows` (the
  `ConnectorInfo` hand-over, now backend-carrying, D8) — the adapter
  builds a `name → backend` map once per pass.
- **`ItemAddr::Files`** (`adapter.rs:3881` region) gains the payload:
  `ItemAddr::Files { connector: String }` — the card's **first
  Nextcloud connector's name** (the re-anchor target; walk order is
  deterministic).
- **`files_on`** = `snapshot.files_sync_enabled && snapshot.files_sync_anchor ∈ card.connectors`
  — the one global flag shown as per-card truth: exactly one card (the
  anchor's) renders On. Non-anchor Nextcloud cards render Off.
- **`files_locked` / `files_restart`** as D4 (either-key env lock;
  restart caption from the `applies` registry for
  `files_sync.enabled` — the caption is row-level, unchanged shape).
- **Pick seam** (`settings_files_picked`, `adapter.rs:5410-5441`):
  resolve `ItemAddr::Files { connector }`; keep the pending and
  env-lock absorption as-is (the env check extends to
  `files_sync.anchor`, D4); send
  `SetFilesSyncEnabled { enabled, anchor: if enabled && !files_on {
  Some(card's first Nextcloud connector) } else { None }, reply }` —
  **On on a non-anchor card re-anchors to it and enables; On on the
  anchor card, or any Off, leaves the anchor key untouched.** The
  adapter reads `files_on` from the item model it already built (pass
  it through or recompute from the snapshot — decision: recompute
  `files_on` at the seam from the snapshot, the model is a projection
  and the seam must not depend on render-pass freshness).
- The `blank` item gains `backend_word: SharedString::new()` (D8) so
  every `..blank.clone()` arm keeps compiling.

### D7 — Overlay writer

`crates/taskboard-app/src/settings_overlay.rs:309-316`:

- New public writer, per the origin plan:
  `pub fn set_files_sync_anchor(path: &Path, name: &str) ->
  Result<(), OverlayError>` — `prove_and_write(path, "files_sync",
  "anchor", toml_edit::value(name))`. Creates the table if absent
  (the round-6 chunk-2 writer semantics; `prove_and_write`'s current
  create-the-subtree behaviour is verified during implementation — if
  it still refuses a missing table, that refusal is fixed in chunk 2's
  branch and this writer inherits it; note the dependency in the
  report if chunk 2 has not landed yet).
- `set_files_sync_enabled` gains a parameter:
  `pub fn set_files_sync_enabled(path: &Path, enabled: bool,
  anchor: Option<&str>)` — writes `enabled`, then calls
  `set_files_sync_anchor` when `Some`. Two `prove_and_write` calls
  inside one editor transaction (`run_editor_write` holds the lock and
  the env guard across the closure) = one user-visible write. Existing
  overlay tests update to the new signature (mechanical).

### D8 — `ConnectorInfo.backend`: enum, not bool (recorded decision)

`crates/taskboard-ui-slint/src/requests.rs:37-41`:

```rust
/// The connector's sync mechanism, secret-free — the Files-row gate
/// and the backend-aware row label (round 6, D-6.6/D-6.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorBackend { Standalone, Nextcloud }
```

`ConnectorInfo` gains `pub backend: ConnectorBackend`; the app fill
(`crates/taskboard-app/src/ui.rs:612-623`, `resolve_connector_infos`)
maps `spec.backend`. **Why an enum over `bool`**: the UI crate cannot
see the app's `SyncBackend` (crate boundary, AGENTS §2), a bool is
opaque at every use site, and chunk 5 (files-boards surface, #722)
branches on the same fact — a named two-variant enum is the A1-pattern
shape (readable, data-less variants, no derive crates). App-side
mapping is a total `match`, so the enum can never silently drift from
`SyncBackend` (exhaustive match on the app enum; adding a
`SyncBackend` variant breaks the build here — the drift alarm).

### D9 — Backend word on `ConnectionItem` + label rendering

- `ConnectionItem` (`requests.rs` struct near `:37` region used by
  adapter `:4666`) gains `pub backend_word: SharedString` (Slint-side
  string — the UI stays dumb; the adapter owns the mapping). The
  adapter fills it on the Deck row from the connector's backend:
  Nextcloud → `"Deck"`, Standalone → `""`.
- **Label rendering** (`ui/views/settings-view.slint:917`): replace
  `text: item.show_title ? "Deck · " + item.title : "Deck"` with:

  ```slint
  text: item.backend-word == "" ? item.title
      : (item.show_title ? item.backend-word + " · " + item.title
                         : item.backend-word);
  ```

  Recorded reading of "standalone connectors drop the Deck prefix": a
  standalone row shows the **connector's own name** (both single and
  multi-connector cards — a bare "Deck" was the mislabelling the
  review flagged; a bare empty string is not an option). A Nextcloud
  single row still reads "Deck"; a multi Nextcloud card reads
  "Deck · name". Composition stays in Slint (presentation), only the
  *word* is app-supplied.

### D10 — Caption copy

- The Files row's restart caption
  (`settings-view.slint`, the `files-restart` arm ~`:903`): **"Files
  sync mirrors this account's Files — takes effect at the next
  restart."** (origin D-6.6's copy; replaces the generic restart
  sentence on the Files row only). The env-lock caption stays.
- `settings_status.rs:261` (`InvalidKind::OutOfRange` for
  `Field::FilesSyncEnabled`): the sentence drops "as the default
  connection" → the anchor-world truth ("Files sync needs a Nextcloud
  account."). Copy is never asserted by tests (AGENTS §5 no-text rule);
  the unit loop pins non-emptiness only.

## 3. Test catalogue (TDD order: red first per layer)

Existing tests that **pin today's contract and must be rewritten**
(the "no pins of today's behaviour" rule — these assert the
first-card-only / default-backend limitation, not a goal):

- `ui_requests.rs:2450-2490` (`SetFilesSyncEnabled` round-trip):
  extend for the anchor payload + snapshot echo.
- any `connection_items_model` tests asserting the Files item appears
  exactly once / on the first card (grep `ConnectionItemKind::Files` in
  the adapter tests) → rewrite as the per-card truth table below.
- `bootstrap.rs:2491` stays green (D3).
- config validation tests around `config.rs:2809` (`FilesSyncWithoutNextcloud`)
  gain anchor-shaped siblings; the xdg-hermeticity config tests (the
  recent `xdg_lock` fixtures) are untouched except where a fixture
  builds a `FilesSyncSection` (additive field, `Default` keeps them
  compiling).

New tests, lowest layer per behaviour (AGENTS §5; typed outcomes, no
text assertions, no sleep):

1. **config resolution** (`config.rs` unit): absent anchor → first
   Nextcloud; set anchor → that connector; first-standalone +
   second-Nextcloud + enabled → resolves (D2b's widening pinned as the
   *desired* semantics); unset + no Nextcloud → `None`.
2. **config validation** (typed `matches!`): unknown anchor name →
   `FilesSyncAnchorUnknown` (also with `enabled == false`);
   anchor → standalone connector → `FilesSyncWithoutNextcloud { field:
   "files_sync.anchor" }`; enabled without resolvable anchor →
   `FilesSyncWithoutNextcloud { field: "files_sync.enabled" }`;
   `field()`/`code()` ladders return the new values.
3. **config serde round-trip**: a config without `anchor` serializes
   byte-identical (the pin-less-configs-unchanged contract); with
   anchor → key present; unknown fields still denied.
4. **bootstrap decision** (`bootstrap.rs` unit, tempdir/xdg fixture
   per the existing spawn-decision tests): anchor names the second
   connector → spec carries *its* url/username; identity stays
   `files-sync`.
5. **overlay** (`settings_overlay.rs`): `set_files_sync_anchor` writes
   the key preserving comments; `set_files_sync_enabled(.., Some(..))`
   writes both keys; `None` leaves a pre-existing anchor untouched.
6. **ui_requests app arm**: `SetFilesSyncEnabled { anchor: Some }` →
   file holds both, reply snapshot echoes the resolved anchor;
   `env_overridden` contains `"files_sync.anchor"` when
   `TASKBOARD_FILES_SYNC__ANCHOR` is set (xdg-hermetic fixture, the
   `xdg_lock` pattern from the recent config-path commits).
7. **snapshot minting** (`ui.rs`): both minting sites fill
   `files_sync_anchor` from the resolved anchor.
8. **adapter model** (`connection_items_model`): two Nextcloud cards →
   two Files items; standalone card → none; `files_on` truth table
   (enabled+anchor-card / enabled+other-card / disabled);
   `ItemAddr::Files.connector` is the card's first Nextcloud connector;
   `backend_word` "Deck" vs "" per backend.
9. **adapter seam** (`settings_files_picked`): On on a non-anchor card
   sends `anchor: Some(..)`; On on the anchor card and any Off send
   `anchor: None`; env-locked pick absorbed (either key).
10. **presentation** (if the Files-row caption/help facts ride a
    presentation fn): structural only — non-empty, presence.

## 4. Risks

- **Chunk-2 dependency flip**: `set_files_sync_anchor` creating the
  `[files_sync]`/`[sync]` subtree on a sync-less file is chunk 2's
  writer fix (#721). If chunk 2 lands first, this chunk inherits it;
  if not, the anchor writer must tolerate the already-fixed-by-then
  upstream or re-derive the insertion (the origin D-6.5 recipe).
  Recorded for stage 2 to verify at implementation time.
- **`ConnectorBackend` ↔ `SyncBackend` duplication**: two enums, one
  mapping. The exhaustive `match` makes drift a compile error, but a
  third backend variant will need a UI word decision — noted in the
  enum's doc comment.
- **ItemAddr shape change ripples**: `ItemAddr::Files` gains a field —
  every `ItemAddr::Files` pattern (the pick seam, tests) updates;
  mechanical, grep-covered.
- **Env-key scan shape unknown** (D4): if `env_overridden_keys` is a
  fixed key list rather than a generic scan, the anchor key needs
  explicit registration — stage 2 records which shape was found.
- **Slint property default**: `ConnectionItem`'s Slint-side struct and
  the adapter `blank` must both gain `backend_word` or every
  construction site breaks — grep-covered, low risk.

## 5. Backlog

Filed autonomously per AGENTS §7 (numbers listed in the stage-1
report; reference this plan's permalink):

- Per-key env-lock display on the Files row (anchor locked, toggle
  free — the row cannot express which key is pinned).
- (none other at planning time; any stage-2 discovery follows the
  standing authorization.)

## 6. Estimated diff

~450–650 lines including tests, across: `config.rs` (+~80),
`bootstrap.rs` (+~30), `settings_overlay.rs` (+~50),
`ui.rs` (+~15), `ui_requests.rs` (+~60), `requests.rs` (ui-slint,
+~30), `adapter.rs` (+~80), `settings-view.slint` (+~10),
`settings_status.rs` (+~2), plus test additions interleaved (the
majority of the total).

# [Plan] GUI Review Round 3, Chunk 5 — Legacy Config Auto-Migration

- **Date**: 2026-10-26 (planned 2026-10-28)
- **Status**: PROPOSED — stage-1 plan for chunk 5 of the round-3
  dispatch. Branch `feat/ui-r3-legacy-config-migration`, cut from
  fetched `origin/main` (c6cae7a). Independent chunk — no predecessor
  gate (dispatch §1: "5 and 7 independent"). `scripts/worktree-doctor.sh`
  run from the main checkout before worktree creation: PASS (222 G
  free, floor 25 G; three sibling worktrees live and kept).
- **Origin** (normative): design round
  `2026-10-26-gui-review-round3-plan.md` §3.5 (D21), §1 finding
  R3-F22, §4 testing (migration bullets), §7-R5, §8 (the #351
  annotation); dispatch plan
  `2026-10-26-gui-review-round3-implementation-dispatch-plan.md`
  §1/§6 (acceptance a–g, prompts 5.1–5.3).
- **Scope guard**: `taskboard-app` (config/connect/connections/
  ui_requests/ui/migration) + `taskboard-ui-slint` (requests/adapter/
  settings_status/settings-view/journeys — removals only).
  `taskboard-domain`, `taskboard-state`, storage, sync: **empty
  diff**. Fenced territories (DR-2/3/4/5/6, undo v1): untouched.
- **All `file:line` anchors are this worktree at c6cae7a** (post
  chunk-1..4-independent merges). The dispatch prompt's older anchors
  (e.g. "figment 1129-1134", "load 751") drifted with upstream
  merges; every anchor below was re-derived and read, not carried
  over.

## 0. The seam map (facts this plan is built on)

- **The legacy scalar world**: `[nextcloud]` carries
  `server_url`/`username` (+ process-global `credential_store`,
  `config.rs:167-175`), no `[[sync.connectors]]`.
  `AppConfig::validate` (`config.rs:969-1014`) rejects two *mixed*
  shapes: `ConnectorShapeMixed` (`[sync] backend` + array, 973-975)
  and `NextcloudTableWithConnectors` (nextcloud *identity keys* +
  array, 976-980 — "credential_store alone stays valid", 736-741).
- **The settings gates** (the typed rejection D21 removes):
  `save_connection` (`ui_requests.rs:317-320`),
  `remove_connection` (`ui_requests.rs:390-393`),
  `attach_connection` (`ui_requests.rs:444-447`) — all
  `read_raw_config` (`ui_requests.rs:670`, `extract_raw` at the
  write path) → `connectors.is_none()` → `SettingsError::LegacyConfig`.
- **`SettingsError::LegacyConfig`** (`requests.rs:88-91`) is mapped
  at `adapter.rs:1595-1614` (`outcome_of` → `Rejected`),
  `settings_status.rs:49-51` (the user-facing sentence), and pinned
  in `settings_status.rs` tests (178, 253, 299 — the totality list,
  the proptest generator, `variant_name`).
- **The read-only row** (D4's surface): produced by
  `resolve_connector_infos` (`ui.rs:276-291`, `legacy =
  config.sync.connectors.is_none()`), carried on
  `ConnectorInfo.read_only` (`requests.rs:41-43`), pushed at
  `adapter.rs:2037`, rendered by `ConnectionRow.read-only`
  (`settings-view.slint:47-49`) gating Test/Bind/Remove
  (`settings-view.slint:527,539,547`), pinned in
  `journeys.rs:2529-2571`.
- **The writer backstop**: `connectors_item`
  (`connections.rs:99-120`) refuses any file with a `[nextcloud]`
  *table* or no `sync.connectors` array;
  `overlay_upsert_connector` (`connections.rs:182`) upserts into both
  array renderings and writes via `write_config_atomic`
  (`config.rs:1372` — tmp sidecar, mode-preserving, rename);
  `overlay_document` (`config.rs:1323`) is the parse skeleton.
  Pinned by connections.rs A3 (`upsert_refuses_the_scalar_world_typed`,
  `connections.rs:432-449`).
- **The REPL ceremony**: `connect::run`
  (`commands/connect.rs:229-279`) takes the pre-read `raw` +
  `config_path`; its `write_config_overlay`
  (`commands/connect.rs:512-570`) has **Path A (scalar world — still
  writes `[nextcloud]` verbatim)** and Path B (array world — rewrites
  entry 0). Two callers feed it: one-shot `connect_command`
  (`commands/mod.rs:1205-1253`, raw at 1213) and the session's
  `connect_line` (`connector_ceremonies.rs:78-92`, raw at 92); the
  `login` alias delegates (`commands/mod.rs:1169-1199`). Session
  refreshes re-`load` (`connector_ceremonies.rs:178`).
- **Boot**: `main.rs` loads at 113 (`gui` arm), 207 (bare invocation
  → DesktopUi/Kiosk/daemon/REPL routes), 240 (explicit `daemon`) —
  the *serving* entries; 151 (IPC client probe), 319 (`status`),
  362 (`daemon stop`) are *observations*. `AppConfig::load`
  (`config.rs:862-872`) = `extract_raw` + `validate`; the figment
  chain is `config.rs:1280-1285` (defaults → file → env).
- **Logout** (`commands/logout.rs:27-72`) requires
  `nextcloud.server_url/username` — on an array-world file it
  already refuses `NotConfigured` today. Unchanged by this chunk.
- **Tests today**: `ui_settings.rs` A9
  (`the_legacy_singleton_world_is_refused_typed_on_save_and_remove`,
  289-363) pins the rejection; `journeys.rs:2529-2571` pins the
  read-only row; `tests/config.rs` + `config.rs` unit tests pin the
  mixed-shape rejections (these **stay**).

## 1. D21 settlement — function home + signature (pure core, IO shell)

**Decision — a new `crates/taskboard-app/src/migration.rs`** (AGPL
header; registered in `lib.rs`). The transform lives on the
`toml_edit` **document** (file bytes), not on the figment-merged
config — the trigger must be answerable from the file alone (§3
explains why the merged view is not safe to trigger on).

```rust
// pure — no IO, unit + property tested
pub(crate) fn migrate_document(doc: &mut toml_edit::DocumentMut) -> bool;
pub(crate) fn migrate_config_in_memory(config: &mut AppConfig) -> bool;

// IO shell
pub(crate) fn migrate_config_file(path: &Path) -> std::io::Result<bool>;
pub(crate) fn load_migrated(explicit: Option<PathBuf>) -> Result<AppConfig, ConfigError>;
```

- `migrate_document` returns whether it mutated (the trigger and the
  transform are one total function — an early `false` for every
  non-legacy shape, mutation only after all preconditions hold).
- `migrate_config_file`: parse via `overlay_document` (absent file →
  `Ok(false)`; unparseable → the existing `InvalidData` io-error,
  unchanged from today's decode failure) → `migrate_document` → on
  `true`: backup (§4) then `write_config_atomic`.
- `load_migrated` is the serving-boot wrapper (§2); connect's entry
  seam (§5) reuses `migrate_config_file` directly.
- Shared rendering: `connections.rs`'s `connector_entry_table` +
  `backend_word` (`connections.rs:131-163`) become `pub(crate)` and
  are reused by `migrate_document` — one entry-rendering rule, no
  duplicate. Same for `ui_requests.rs`'s `url_host`
  (`ui_requests.rs:619-623`) — relocation to a `pub(crate)` home so
  the migration log can honour the log-safe-host rule (prompt 5.2:
  names/hosts only, never full URLs, never secrets).

## 2. When it runs (the "load?" question)

**Decision — three seams, never `AppConfig::load` itself.**

1. **Serving boot**: `main.rs` 113 (gui), 207 (bare invocation —
   desktop/kiosk/daemon/REPL routes), 240 (explicit `daemon`) switch
   to `migration::load_migrated(cli.config.clone())`:
   `AppConfig::load` → `migrate_config_file(config_path)`:
   - `Ok(false)` → return the loaded config untouched.
   - `Ok(true)` → re-`load` (the file is now valid array-world) →
     info log → return. A re-load failure here is **not** a coding
     bug — it is the both-file-and-env-identity corner (§3) — and is
     handled by the §4 failure ladder, not an expect.
   - `Err(io)` (the persist failed: read-only fs, full disk) →
     **in-memory fallback**: `migrate_config_in_memory(&mut config)`;
     if the mirrored config `validate()`s → `tracing::warn!` +
     proceed (D21: "app still works"); if it does not (the env
     corner again) → keep the original scalar config + warn (the app
     runs the legacy world; settings writes retry via seam 3).
2. **Connect ceremony entry** (acceptance f): step 0 inside
   `connect::run` (it already owns `config_path`,
   `commands/connect.rs:229`): if `raw.sync.connectors.is_none()` →
   `migrate_config_file(config_path)`: `Ok(true)` → re-`extract_raw`
   and continue with the array-world raw (the D2 standalone guard
   then no-ops, `persist` takes Path B); `Ok(false)` → continue with
   the raw as-is (today's behavior verbatim); `Err(io)` → warn +
   continue with the raw as-is (the ceremony's own persist hits the
   same fs and fails honestly if the failure is hard; a transient
   one completes on a later boot). One seam, both callers and the
   `login` alias inherit.
3. **Settings write ceremonies** (the D21 completion path): the
   three `LegacyConfig` gates become one helper in `ui_requests.rs`:

   ```rust
   fn ensure_migrated(workspace: &ConnectionWorkspace) -> Result<AppConfig, SettingsError>
   ```

   `read_raw_config` → array present → return it; absent →
   `migrate_config_file(&workspace.config_write_path)`: `Ok(true)` →
   `read_raw_config` again (the migrated file); `Ok(false)` →
   `Err(PersistRejected)` (the env-identity / hand-made scalar
   corner — the writer backstop would refuse anyway; refusing early
   with the same outcome class); `Err(_)` → `Err(PersistFailed)`
   (D21's typed refusal — never silent). This is "the next
   successful overlay write completes it" made concrete: a boot
   whose persist failed is completed by the first Save/Remove/Attach.

**Never migrates**: `status_command` (`main.rs:319`),
`daemon_stop_command` (`main.rs:362`), the IPC client probe
(`main.rs:151`), `config print` (`commands/config.rs:22`) — read-only
observations and one-shots; migrating there would mutate files from
read commands and race a running daemon. `AppConfig::load` stays
pure: it is shared by all of the above plus dozens of file-fixture
tests; D21's "on load" is interpreted as *when a serving process or a
write ceremony loads its config to run*, and this plan records that
reading.

## 3. The exact file transformation

**Trigger** (all must hold; checked before any mutation):
- no `sync.connectors` key of any rendering;
- `[nextcloud]` present with **non-empty** `server_url` **and**
  non-empty `username` strings (a half-configured file stays loud —
  today's `EmptyUsername`/`InvalidServerUrl` boot refusal is
  preserved);
- `[sync] backend` absent **or** `"nextcloud"` (an explicit
  `backend = "standalone"` install is *not* the legacy single-server
  config — migrating it would flip standalone semantics; it keeps
  today's behavior, D2 guard included).

**Transform** (single pass, then one atomic write):
1. capture the identity strings;
2. remove the `sync.backend` key (drop a keyless `[sync]` table) —
   mandatory: keeping it beside the new array is
   `ConnectorShapeMixed`, i.e. the upgrade would brick boot on every
   explicit-`nextcloud` install. The loud scalar word is folded into
   the entry's own `backend` field — the ADR 0010 choice stays
   structurally encoded;
3. remove `nextcloud.server_url` and `nextcloud.username`; drop the
   `[nextcloud]` table only when it is now keyless — a
   `credential_store` value **survives** (it is process-global,
   `config.rs:736-741`'s own rule; stripping the whole table would
   silently flip the credential-store kind);
4. append one `[[sync.connectors]]` entry via the shared
   `connector_entry_table`: `name = "nextcloud"` (exactly
   `DEFAULT_CONNECTOR_NAME`, `connectors.rs:32` — the id the legacy
   synthesis and every existing DB binding already use; the mint is
   kebab-proven by `connectors.rs:283-287`), `backend = "nextcloud"`,
   the captured `server_url`/`username`.

Byte-preservation is `toml_edit`-by-construction for comments,
formatting, and unrelated tables (the house overlay discipline);
comments *attached to the removed keys* vanish with them — inherent
to the semantics, pinned as such in M1. The `connectors` array is
created at the (created-or-existing) `[sync]` table's end; document
order otherwise untouched.

**Secrets**: never read, never written — the migration is
config-file-only, and the store stays keyed server+username
(acceptance d; `store_and_rotate_secret`/`CredentialStore` untouched).

### Why the trigger is doc-based, not merged-config-based (the recorded corner)

`extract_raw`/`load` merge `TASKBOARD_*` env over the file. An
env-injected `TASKBOARD_NEXTCLOUD__SERVER_URL/username` makes the
*merged* config legacy-shaped while the file is not the source. A
merged-view trigger would write the array into the file while env
keeps injecting the identity keys — the next load is
`NextcloudTableWithConnectors` and the boot dies. The doc-based
trigger migrates only when the identity *lives in the file*, so:
- env-identity-only installs never migrate (file untouched); their
  settings writes refuse typed (`PersistRejected`, seam 3) exactly as
  they were refused before (`LegacyConfig`) — same UX class, honest;
- file+env-both-identity installs: the file migrates, the re-load
  mixes → the §4 ladder restores the backup and proceeds scalar-world
  (today's semantics byte-for-byte). `connect` in this corner fails
  typed at its re-`extract_raw` instead of succeeding — a recorded
  typed-refusal consequence for a misconfigured corner (env
  *silently won* before; now it refuses loudly).

## 4. Backup + failure policy

- **Name**: `<config-file-name>.bak-legacy` in the same directory
  (`taskboard.toml.bak-legacy` for the default path; a custom
  `--config` path derives its own). Written **only when the document
  mutates** — idempotence means it is never clobbered by a later
  migration; a user *restoring* the backup (legacy file again)
  re-triggers migration and the copy overwrites with the same
  pre-image content — lossless by construction.
- **Best-effort** (D21): `std::fs::copy` failure → `tracing::warn!`
  and continue (the transform is comment-preserving and the payload
  is two keys; a failed backup must not block a working upgrade).
- **Persist-failure ladder** (boot, `load_migrated`): re-load fails
  after a successful write → restore the backup (best-effort) and
  proceed with the original scalar config + warn; restore also fails
  → propagate the load error (typed boot failure — the only honest
  terminal when neither shape can be served). The write itself
  failing → the in-memory fallback of §2.1.
- Every boot on a read-only-media kiosk repeats the in-memory
  fallback: one warn per boot, no loop, app fully functional.

## 5. REPL-path verification (acceptance f — the traced matrix)

| Path | Config source | Migration? | Post-chunk behavior on a legacy file |
|---|---|---|---|
| one-shot `connect`/`login` (`mod.rs:1213` → `connect::run`) | `extract_raw(write_path)` | yes — inside `run` (§2.2) | file migrated; persist takes Path B; guards (D2/D3) evaluate the array world |
| session `connect_line` (`connector_ceremonies.rs:92` → same `run`) | same | yes — same seam | identical; session refresh re-`load`s the migrated file |
| `logout` (`mod.rs:1257`, `logout.rs:27-72`) | `extract_raw` | **no** | on an already-migrated file: typed `NotConfigured` — the array world's *existing* contract (recorded consequence, #351); on a not-yet-migrated file (boot-less one-shot logout): today's scalar strip verbatim |
| session `attach`/daemon attach (`connector_ceremonies.rs:178,255`) | `AppConfig::load` | no (re-read only) | reads the migrated array world; entry-based targeting unchanged |
| IPC `detach-connector` (`ipc/server.rs:445`) | `app.config` (boot-migrated) | n/a | `configured_connectors` returns the entry — the same id `nextcloud`, identity matching byte-identical |
| settings Save/Remove/Attach/Test | `read_raw_config` | yes — seam 3 | migrated then served; refusal only for genuinely unwritable files |

## 6. Removal inventory (R5 grep-gate)

Removed:
1. `SettingsError::LegacyConfig` — `requests.rs:88-91` (variant + doc).
2. `adapter.rs` — the `outcome_of` arm (1597) and the "legacy
   config" wording in the `Rejected` doc (1522).
3. `settings_status.rs` — the `error_text` arm (49-51), the totality
   list entry (178; count assert 15 → 14), the proptest generator
   arm (253), the `variant_name` arm (299). The sentence is
   unreachable; no replacement copy.
4. `ConnectorInfo.read_only` — `requests.rs:41-43`; the producer
   flag in `ui.rs:279-289`; the push at `adapter.rs:2037`.
5. `ConnectionRow.read-only` — `settings-view.slint:47-49`; Test/
   Bind/Remove gates (527, 539, 547) drop the `!row.read-only &&`
   conjunct (`selected-board != ""` on Bind stays).
6. `journeys.rs:2529-2571` — the legacy-row fixture and the two
   `read_only` assertions; the second row becomes a plain row (the
   `Down`-episode pin stays).
7. `ui_settings.rs` A9 (289-363) — **replaced**, not weakened: it
   pinned the bug R3-F22 reports. The recorded-decision comment names
   round-3 D21 + this plan §6.

Stays (explicit anti-overreach):
- `ConfigError::ConnectorShapeMixed` / `NextcloudTableWithConnectors`
  + every pin (`config.rs:2142,2277,2482`, `tests/config.rs`) — the
  mixed shapes remain constructible by hand/env (prompt: "mixed-shape
  refusal stays for hand-made files").
- `connectors_item` (`connections.rs:99-120`) — but its first guard
  relaxes from *any* `[nextcloud]` table to *identity keys*
  (`server_url`/`username` present), mirroring validate's predicate
  exactly — otherwise a migrated file that kept `credential_store`
  would refuse every later Save. The doc comments cross-reference;
  paired tests pin both sides (M10/A3).
- A3 (`connections.rs:432-449`) — its comment updates: it now pins
  the hand-made-scalar corner, not the removed UI gate.
- `configured_connectors`' legacy synthesis arm + `DEFAULT_CONNECTOR_NAME`
  + `connectors.rs` tests — the in-memory scalar world stays
  reachable (env identity, unmigratable files, `config print`).
- `connect.rs` `write_config_overlay` Path A + its tests — the
  unmigrated scalar corner still connects exactly as today.
- `logout` — untouched.

**Grep-gate** (stage 2 runs it as the completeness proof; stage 3
re-runs it — a hit is a finding):
```sh
grep -rn "LegacyConfig" crates/           # must be empty
grep -rn "read_only\|read-only" crates/taskboard-ui-slint crates/taskboard-app  # must be empty
```

## 7. Test catalogue (TDD-first; M-numbers are the plan's pins)

`migration.rs` (new):
- **M1** doc round-trip: a hand-tuned legacy file (leading comment,
  `[storage]`, `[ui]`+`[ui.keymap]`, `[preferences]`, `[nextcloud]`
  *with* `credential_store`) → migrate → reload via
  `AppConfig::load`: exactly one entry (name `nextcloud`, backend,
  server, username), identity keys gone, `credential_store` kept,
  comments + unrelated tables preserved (structural `contains`
  asserts — the connections.rs A1 precedent), idempotent
  (second `migrate_document` → `false`, bytes equal).
- **M2** proptest — *migrating twice is identity*: over generated
  legacy-shaped toml inputs, `migrate(migrate(x)) == migrate(x)`
  (bytes) and the in-memory mirror's double application is the
  single application.
- **M3** trigger matrix: no identity → false; empty/missing username
  → false; explicit `backend = "standalone"` → false; explicit
  `backend = "nextcloud"` → true; array already present → false
  (hand-made mixes are the validator's job, never migrated).
- **M4** in-memory mirror: `validate()` passes post-mirror; the
  env-identity corner (mirror + injected identity) fails `validate`
  — pinning the §2.1 keep-original arm's premise.
- **M5** file shell: backup carries the exact pre-image bytes;
  absent file → `Ok(false)`; write failure (read-only dir,
  `cfg(unix)`) → the io-error surfaces, in-memory fallback decided by
  the caller.
- **M6** `load_migrated` ladder: happy path (file transformed,
  reload returned); persist-failure → mirrored config + the file
  untouched; re-load-mix corner → backup restored, scalar config
  returned. Pure tempdir unit tests over the lib fn (no bin harness
  needed).

`commands/connect.rs` / `tests/cli_connect.rs`:
- **M7** ceremony on a legacy file (the `cli_connect.rs:64` harness):
  run succeeds; the file is array-world post-ceremony (Path B, entry
  0 rewritten); a second connect does not duplicate entries.
- **M8** explicit-standalone file: no migration; the D2 guard's
  typed refusal/prompt path unchanged.

`tests/ui_settings.rs`:
- **M9** (A9's replacement): legacy file + `SaveConnection` over
  wiremock → typed Ok; file transformed (array present, identity
  gone, `.bak-legacy` present with the pre-image); store holds the
  new secret. Persist-failure twin (`cfg(unix)` read-only dir) →
  `PersistFailed` typed, file byte-untouched.
- **M10** `RemoveConnection` on a legacy file migrates first, then
  removes the sole entry (the no-empty-husk rule leaves a clean
  no-array file).

`connections.rs`:
- **M11** relaxed backstop: `[nextcloud]` holding only
  `credential_store` beside a valid array → upsert succeeds;
  A3 stays green (hand-made scalar still refused, file untouched).

Removals with their tests: settings_status (list/gen/name shrink),
journeys read_only pins, A9. Existing `tests/config.rs`,
`config.rs` unit pins: expected green, untouched (they call
`AppConfig::load` directly — no migration inside load).

## 8. Bookkeeping & delivery

- CHANGELOG `Unreleased` → **Changed**: legacy single-server configs
  migrate automatically on boot/connect/settings-write (backup
  `taskboard.toml.bak-legacy`, best-effort); the typed legacy
  rejection and the read-only settings row are gone.
- Round plan §8: issue **#351 gets an annotation** when the PR opens
  (stage 3): its migration half landed here; its logout-on-array-world
  half (`NotConfigured` for migrated installs) remains open there.
- Stage-3 ship per dispatch §1 (PR + green checks + `mergeStateStatus:
  CLEAN`, then worktree teardown anchored at the project root).

## 9. Risks

- **R5 (the round's own risk)** — the typed error path has callers
  beyond the three gates: the grep-gate above is the proof; the full
  caller map (§0/§6) was read, and every hit is in the inventory.
- **The env-identity corners** (§3): doc-based triggering keeps every
  mixed state unreachable by construction; the two recorded
  consequences (settings `PersistRejected` for env-only installs;
  connect's typed refusal in the both-identity corner) are
  behavior *changes* for those corners and are pinned by M3/M4/M6.
- **Backstop/validator drift** (§6): two predicates
  (typed `validate` vs doc `connectors_item`) must agree; paired
  tests (M11 + A3 + the validate pins) hold the mirror.
- **The connector name choice** is load-bearing for existing DB
  bindings (`bound_connector` rows decode through
  `ConnectorId::from_kebab`): the name is pinned to
  `DEFAULT_CONNECTOR_NAME` by M1's assert, not by convention.
- **Diff-size estimate**: ~550–650 lines including tests
  (~60% tests) — new `migration.rs` ≈ 300, rewrites in
  ui_settings/journeys ≈ 120, removals ≈ 80, seams ≈ 60,
  CHANGELOG/plan excluded. M-sized, matching dispatch §1.

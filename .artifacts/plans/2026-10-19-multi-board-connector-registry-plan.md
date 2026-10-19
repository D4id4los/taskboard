# [Plan] Multi-board Chunk 5 — App Plural World: Config, Registry, Authority

- **Date**: 2026-10-19
- **Kind**: stage-1 TDD-first work plan for chunk 5 of the multi-board
  implementation round (dispatch plan §6, prompt 5.1). No code in this
  commit; this plan is stage 2's contract.
- **Origin** (normative rank, dispatch MD-7): implementation plan
  `.artifacts/plans/2026-10-18-multi-board-wave-implementation-plan.md`
  §2 DF-3/DF-6/DF-7/DF-10, §3 chunk 5, §4, §5, §6; design doc
  `.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`
  §3.6 (full, incl. the figment probe's eight cases), §3.7 (full, incl.
  the broadcast-provenance blocking fact and the flapping note), §3.3
  (registry/naming consequences), §5 ACCEPTED record; dispatch plan
  `.artifacts/plans/2026-10-18-multi-board-impl-dispatch-plan.md`
  §1/§6.
- **Status**: READY for stage 2. Precondition verified 2026-10-19:
  chunk 4 (PR #368, `feat/sync-connector-modules`) is merged; base is
  `origin/main` @ `35aa926`. Worktree
  `../taskboard-WT-feat-app-connector-registry`, branch
  `feat/app-connector-registry`. `scripts/worktree-doctor.sh` green
  (138G free, floor 25G).
- **Exit criterion owned here**: two connectors live simultaneously
  (impl plan §5) — the hermetic half in bootstrap tests, the tier-2
  half in `it_docker.rs`. PR body carries `Closes #187` (stage 3,
  MD-4); annotation comment on #125.

## 1. Scope

**In** (impl plan §3 chunk 5, normative): the `[[sync.connectors]]`
array decode + the typed-rejection list (DF-6); the shape-aware
ceremony writer (DF-6); the N-actor registry with named routing,
`ConnectorNameInUse`, the attach verb's name parameter (design §3.3
consequences); the per-connector network events + the all-down
authority (DF-7); the engine's tag-scoped `go_offline`;
`require_connector` pluralised; the two-connector tier-2 test; the
single-connector byte-for-byte guarantee.

**Out** (non-goals, restated): no UI work (chunk 7 — the UI crate has
zero diff this chunk: its repaint driver matches `Ok(_)` on system
events, so the vocabulary growth compiles clean through it); no second
connector *kind* (nextcloud only); no per-connector poll/backoff knobs;
no `--connector` CLI flags, no per-connector `status` rendering, no
`boards use` (all chunk 6); no hysteresis (design §3.7's flapping
note); no ceremony verbs for cross-connector *binding* (chunk 6's
move-board exercise — see D5-19 for how the tier-2 test crosses
connectors without them); no connector configuration via env (figment
probe case 7 — documented limitation).

## 2. Re-verified seam table (2026-10-19, base 35aa926)

Symbols are the stable keys (MD-8); line numbers are this plan's day
references.

| Seam | Where | Shape today |
|---|---|---|
| `SyncBackend` / `SyncSection` | `crates/taskboard-app/src/config.rs:159–198` | `backend: Option<SyncBackend>` + three duration knobs, `deny_unknown_fields` |
| `sync_backend()` | `config.rs:532` | explicit else auto (`nextcloud` iff `nextcloud.server_url` present) |
| `AppConfig::validate` | `config.rs:549` | the loudness block: URL/username checks, nextcloud-required block, durations, reminders, db, ui scale |
| `ConfigError` + `kind()` | `config.rs:394/436` | seven variants; stable discriminant strings |
| figment chain | `config.rs:713` | `Serialized::defaults → Toml::file → Env::prefixed("TASKBOARD_").split("__")` |
| ceremony persist + overlay | `crates/taskboard-app/src/commands/connect.rs:393/492` | in-memory merge of `[nextcloud]` keys + `toml_edit` overlay (`doc["nextcloud"][…]`, `backend` iff switched) |
| connect guards | `connect.rs:284/346` | `decide_backend_switch` (D2), `decide_repoint` (D3) over `raw.nextcloud` |
| registry loop / synthesis | `crates/taskboard-app/src/connectors.rs:158` + `bootstrap.rs:504–527/637` | `configured_connectors(config) → Vec<ConnectorSpec{name, backend}>`; `bootstrap` builds `NextcloudModule`s reading `config.server_url()/username()`; `bootstrap_with_modules` loops modules, first takes the boot pair |
| `SyncActor` / guards | `bootstrap.rs:121/435/448` | `{commands, join}` — no id; `ensure_vacant`/`try_register` count-≤-1; `AttachError::AlreadyAttached` |
| `attach_connector` / `detach_connector` | `bootstrap.rs:243/199` | attach takes a built `DeckClient`, spawns tagged `default_connector`; detach drains all |
| `App` fields | `bootstrap.rs:128` | `connectors: Mutex<Vec<SyncActor>>`, `default_connector`, parked `reports` twin; shutdown destructures |
| `FakeModule` | `bootstrap.rs:883` | scripted `SyncReport`s per `SyncNow`; `_system` unused |
| broadcast wiring | `bootstrap.rs:588` | `broadcast::channel::<SystemEvent>(64)`; engine, actors, logger subscribed |
| daemon attach/detach | `crates/taskboard-app/src/ipc/server.rs:385–439` + `session_ceremonies.rs:223` | wire carries `(server_url, username)` identity; detach identity-matches `config.nextcloud`; attach re-reads config, matches, swap-first |
| session ceremonies | `session_ceremonies.rs:77/130` | connect: ceremony → refresh → detach-first(all) → attach → settle; logout: drain-all |
| `require_connector` | `crates/taskboard-app/src/commands/mod.rs:46` | `sync_backend() == Standalone ⇒ SyncDisabled` |
| `SystemEvent` | `crates/taskboard-domain/src/messages.rs:285` | `NetworkLost/NetworkRestored/Shutdown/StorageFailed`, `Copy` |
| offline FSM | `crates/taskboard-sync-nextcloud/src/actor.rs:162/553/526` | `LoopState.offline`; `fail_cycle` emits `NetworkLost` iff `Network && !offline`; success path reports then `NetworkRestored` iff `was_offline`; select loop ignores the network events (:232) |
| engine arms | `crates/taskboard-state/src/actor.rs:475–488` | `NetworkLost → go_offline` (all bound boards, idempotent); `NetworkRestored → forward_sync_now` |
| `go_offline` | `state/actor.rs:808` | per-board idempotent Offline writes over `core.app().bound_boards()` |
| `bound_boards()` | `crates/taskboard-domain/src/state.rs:267` | all boards with `remote.is_some()`, tombstones included |
| sync-actor doc claim | `sync/actor.rs:13–16` | "this actor is the network authority" — rewritten here |
| tier-2 corpus | `crates/taskboard-app/tests/it_docker.rs:767/834`, `tests/common/mod.rs:311/496`, `scripts/nextcloud_it_setup.sh`, `.github/workflows/nextcloud-integration.yml` | one dockerized instance (`TASKBOARD_IT_DOCKER_*`), hermetic + docker halves, `write_v01x_db` raw-sqlx fixture, `eventually` + `with_deadline` helpers |
| `EnvCredentialStore` | `crates/taskboard-app/src/secrets.rs:22` | single `TASKBOARD_APP_PASSWORD` for any `(server, username)` |

## 3. Settled decisions

Every open question the dispatch prompt named is settled here with a
recorded decision (D5-N). Deviations from the normative documents are
marked with their reason; none touches an ACCEPTED design decision.

### 3.1 D5-1 — the `ConnectorCfg` entry struct and the `SyncSection` field

New in `config.rs` (app crate — config decode, not domain vocabulary):

```rust
/// One `[[sync.connectors]]` entry. Every field is required: a missing
/// field is a decode failure (structure), not a semantic check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorCfg {
    pub name: String,        // kebab-case, unique — validated (D5-4)
    pub backend: SyncBackend, // the connector kind (module selector)
    pub server_url: String,
    pub username: String,
}
```

`SyncSection` grows `#[serde(default, skip_serializing_if =
"Option::is_none")] pub connectors: Option<Vec<ConnectorCfg>>`. **The
`Option` is load-bearing**: `None` (array absent) vs `Some(vec!)`
(array present, including `Some(vec![])`) is exactly the presence fact
the resolution rule and every mix rejection key on — a bare `Vec`
cannot distinguish them. `deny_unknown_fields` on `SyncSection` keeps
rejecting unknown `[sync]` keys (probe case 8); the entry struct's own
`deny_unknown_fields` is the typo guard (probe case 4). `poll`/
`backoff` knobs stay global (design; per-connector tuning is §7
backlog).

### 3.2 D5-2 — the resolution order; `ConnectorSpec` carries identity

`configured_connectors(config) -> Vec<ConnectorSpec>` in `connectors.rs`
stays the single resolution home and the single source of truth for
"how many connectors exist":

- `sync.connectors = Some(entries)` ⇒ **exactly the array, in file
  order; the first entry is the default connector**. `Some(vec![])` is
  a valid zero-connector world (the array door to standalone).
- `sync.connectors = None` ⇒ **the legacy world verbatim**:
  `sync_backend()`'s auto rule stands; resolved `nextcloud` synthesizes
  exactly one connector named `"nextcloud"` (identity from
  `[nextcloud] server_url`/`username`); resolved `standalone` ⇒ zero.

`ConnectorSpec` grows `server_url: String` and `username: String`, so
module construction stops reading the single-connector accessors
`config.server_url()/username()` (bootstrap.rs:516–521 rewires to the
per-spec fields; the accessors survive for the legacy display paths).
`DEFAULT_CONNECTOR_NAME` stays the synthesized name and the
storage-decode default. `App.default_connector` remains the first
module's id (the `BindBoard`/adopt-new mint — unchanged mechanics).

### 3.3 D5-3 — the six typed rejections → five `ConfigError` kinds

All loud, all in `AppConfig::validate`, all before the legacy
`[nextcloud]` semantic checks (**validation order is part of this
decision**: with `[nextcloud] server_url` garbage *and* an array
present, the mix error must fire, not `InvalidServerUrl`). Mapping:

| # | Design's rejection (DF-6) | Fires when | Kind + payload | `kind()` |
|---|---|---|---|---|
| 1 | scalar + array both present | `sync.backend.is_some()` (file **or** env) ∧ `connectors.is_some()` | `ConnectorShapeMixed` | `connector-shape-mixed` |
| 2 | `[nextcloud]` table + array | `nextcloud.server_url.is_some() ∨ nextcloud.username.is_some()` ∧ `connectors.is_some()` | `NextcloudTableWithConnectors` | `nextcloud-table-with-connectors` |
| 3 | `backend = "standalone"` + array | — subsumed by #1 | `ConnectorShapeMixed` | — |
| 4 | duplicate `name` | two entries share a name | `DuplicateConnectorName { name: String }` | `duplicate-connector-name` |
| 5 | non-kebab `name` | D5-4's predicate fails | `InvalidConnectorName { name: String }` | `invalid-connector-name` |
| 6 | `backend = "nextcloud"` resolved with an empty array | `Some(Nextcloud)` ∧ `Some(vec![])` | `ConnectorShapeMixed` (subsumed) | — |

Decisions embedded in the mapping:

- **#3/#6 are subsumed by #1** because #1's scope is *presence of the
  scalar key* (any value, any array length, any provenance). This is
  the design's own framing — variant (ii): "any mix of the two
  representations is a typed validation error (`ConfigError::
  ConnectorShapeMixed` — naming both keys)". Each design-listed
  instance still gets its own pinning test (below), so nothing on the
  list goes untested; two of them share a kind. Recorded deviation
  from a literal six-kinds reading, reason: six kinds would encode the
  same rule three times and force error-matching on which half of a
  mix the user wrote.
- **The env mix is loud** (probe case 6's decode-level composition +
  validation): `TASKBOARD_SYNC__BACKEND=…` over an array file errors
  with `ConnectorShapeMixed`. Recorded behavior note: this is new
  loudness for a combination that is meaningless anyway; the env
  scalar keeps working exactly as before on array-absent files (the
  scripting seam the design preserves).
- **#2 scopes to the identity keys**, not the table's existence:
  `[nextcloud] credential_store = "env"` + an array stays **valid**
  (the store-kind selection is process-global, not per-connector
  identity). The rejection fires only when the table carries
  `server_url`/`username` — the keys the array owns.
- **`connectors = []` alone** (no scalar, no `[nextcloud]` identity)
  is valid: zero connectors, `require_connector` refuses, standalone
  behavior. Both doors to "no connectors" stay open.
- **Offending-key naming**: the fixed-pair variants name their keys in
  the `Display` text (`sync.backend` + `sync.connectors`;
  `nextcloud.server_url`/`username` + `sync.connectors`); the
  name-carrying variants name the offender structurally in their
  payload. Tests assert variants and payloads (AGENTS §5 no-text
  rule); `Display` is for humans.
- One **added** kind (not on the design's list — recorded addition,
  reason: the per-entry analogue of the existing
  `InvalidServerUrl`/`EmptyUsername` loudness; without it a garbage
  entry URL surfaces only at boot as `BootstrapError::Client`, losing
  which entry offended): `InvalidConnectorEntry { name: String, field:
  &'static str }` (`invalid-connector-entry`) fires when an entry's
  `server_url` fails `reqwest_host_ok` (field `"server_url"`), its
  `username` is empty/whitespace (field `"username"`), or its
  `backend` is `Standalone` (field `"backend"` — a standalone *entry*
  is a config error: the array lists connectors; `standalone` is the
  absence of one, and today's `unreachable!` arm in bootstrap's module
  construction must never be reachable from config).
- `ConfigError::kind()` extends with the five strings; the
  `config_error_kinds_are_stable_discriminants` test extends.

### 3.4 D5-4 — the kebab predicate

`is_kebab_name(&str) -> bool`: non-empty; every char ∈ `[a-z0-9-]`; no
leading or trailing `-`; no `--`. (`"work"`, `"nc-2"`, `"personal"`
pass; `"Work"`, `"a_b"`, `"-a"`, `"a-"`, `"a--b"`, `""`, `"a b"` fail.)
Pure function in `config.rs`, unit-pinned over both lists. This is the
`ConnectorId` mint's input filter (DF-3); the domain newtype itself
stays unfiltered (chunk 1's shape — config validates, domain trusts).

### 3.5 D5-5 — the shape-aware ceremony writer

`write_config_overlay` (connect.rs:492) grows two paths over the
parsed `toml_edit::DocumentMut`:

- **Path A — no `sync.connectors` key in the document**: today's
  writes verbatim (`doc["nextcloud"]["server_url"/"username"]`; iff
  `switch_backend`, `doc["sync"]["backend"] = "nextcloud"`; file
  created when absent).
- **Path B — the array is present**: rewrite the **first** entry's
  keys only — `doc["sync"]["connectors"][0]["server_url"/"username"] =
  value(...)` — and touch nothing else: never `backend` (neither
  `[sync].backend` nor any entry's), never `[nextcloud]`, never other
  entries. `switch_backend` is structurally false in this world
  (D5-6).

`persist` (connect.rs:393) mirrors the shape in-memory so the
pre-write `validate()` models the file about to be written: array
world ⇒ mutate `merged.sync.connectors[0]`'s `server_url`/`username`;
scalar world ⇒ today's `merged.nextcloud` writes. The shape fact comes
from `raw.sync.connectors.is_some()` (env cannot set the array — probe
case 7 — so the decoded shape **is** the file's shape).

Round-trip tests in both file shapes: overlay → `AppConfig::
extract_raw(path)` decodes and validates; the identity lands in the
right home (`[nextcloud]` vs entry 0); entry 1 is untouched; a
pre-existing comment elsewhere in the file survives (the
comment-preservation contract); `switch_backend` writes only in path
A.

### 3.6 D5-6 — the ceremony guards go shape-aware

- `decide_backend_switch`: if `raw.sync.connectors.is_some()` ⇒
  `Ok(false)` immediately — nothing to flip (the array carries its own
  per-entry backend words) and no prompt. Legacy world byte-for-byte.
- `decide_repoint`: compares the gathered identity against **the
  configured default identity** = entry 0's `(server_url, username)`
  (array world) or the `[nextcloud]` pair (scalar world; half-configured
  stays "unconfigured", D8). Different ⇒ the same `--force`/confirm/
  `ConnectionExists` ladder. Receipt fields unchanged.

### 3.7 D5-7 — the registry: named entries, named lookup, named refusals

In `bootstrap.rs`:

- `SyncActor` gains `id: ConnectorId` (the registry key).
- `ensure_vacant` → `ensure_name_free(&connectors, name)` refusing
  `AttachError::ConnectorNameInUse(ConnectorId)` — same guard, named
  (design §3.3). `AttachError::AlreadyAttached` is **removed** and its
  two maps updated: session ceremonies' match arm and the IPC wire
  (`AttachWireOutcome::AlreadyAttached` **survives** on the wire,
  produced by `ConnectorNameInUse` — the wire vocabulary is unchanged
  this chunk). `b11_double_attach_is_refused_typed` updates its
  assertion to the named variant — semantics identical.
- `try_register` registers iff the name is free (same lock-held
  re-check shape).
- `attach_connector(&self, name: ConnectorId, client: DeckClient)` —
  the spawned actor is tagged `name` (was `default_connector`);
  guard + register by name. Callers: session ceremonies pass the
  refreshed config's **default connector id** (first spec);
  `attach_from_persisted` passes the resolved target's id (D5-14).
- **New** `connector_commands(&self, id: &ConnectorId) ->
  Option<mpsc::Sender<SyncCommand>>` — the named lookup for any
  targeted send (design §3.3 option (b)'s accessor). Its first
  consumer is `detach_connector_named`.
- `detach_connector()` keeps its drain-all semantics (session logout,
  today's callers). **New** `detach_connector_named(&self, id:
  &ConnectorId) -> DetachOutcome` sends `Detach` on that connector's
  channel and joins it bounded (`NoConnector` when the name is not
  registered). `App::shutdown`'s registry drain is unchanged (it stops
  every actor directly).
- The N-actor spawn loop is already chunk 4's shape — this chunk
  makes N=2 *reachable* (config) and *safe* (guards + authority).

Recorded scope note: a per-name detach primitive is not on the chunk's
file list by name, but it is the mirror the named registry requires
(the design's own "`try_register` gains the key" consequence), and the
session swap needs it to avoid killing connector 2 while rewriting
entry 1 (D5-16).

### 3.8 D5-8 — `require_connector` pluralisation; the `sync_backend()` caller table

`require_connector` (commands/mod.rs:46) becomes:

```rust
if crate::connectors::configured_connectors(config).is_empty() {
    return Err(CommandError::SyncDisabled);
}
```

Same typed refusal (`SyncDisabled`, kind `sync-disabled`), pluralised
semantics ("at least one connector configured"). Legacy equivalence is
structural: nextcloud-resolved ⇒ one spec ⇒ pass (today: non-standalone
⇒ pass); standalone ⇒ zero ⇒ refuse.

Every `sync_backend()` caller re-verified and classified (gate ⇒
set-emptiness via `configured_connectors().is_empty()`; display ⇒
D5-9's default word; resolution ⇒ already the home):

| Caller | Class | Change |
|---|---|---|
| `commands/mod.rs:47` `require_connector` | gate | D5-8 |
| `commands/mod.rs:805`, `mod.rs:1412` (sync-verb standalone gates) | gate | set-emptiness |
| `commands/tasks.rs:49` | gate | set-emptiness |
| `repl.rs:668` (prelude) | gate/display per its body | set-emptiness (gate half); display half per D5-9 |
| `ipc/server.rs:396` (daemon detach) | gate/identity | D5-15 rework |
| `commands/config.rs:29`, `commands/status.rs:71` (backend word displays) | display | D5-9 |
| `config.rs:567` (validate's nextcloud-required block) | — | **no change**: under an array the identity keys are mix-rejected before this block and `sync_backend()` auto-resolves standalone, skipping it — correct by ordering (D5-3) |
| `connectors.rs:159` | resolution home | D5-2 |
| `bootstrap.rs` logging | cosmetic | first-spec word |

### 3.9 D5-9 — display backend word under an array

Where a surface shows the backend word (`config print`, `StatusView.
backend`), the value becomes: array present ⇒ the **first entry's**
`backend` (the default connector's kind); array absent ⇒ today's
`sync_backend()` resolution. Helper `resolved_default_backend()` in
`config.rs` (pure). Legacy files produce byte-identical output.
`connector_flag` (commands/status.rs:24) switches its first parameter
from `SyncBackend` to "connectors configured: bool"
(`Some(attached)` iff non-empty; `None` when empty) — legacy outputs
identical.

### 3.10 D5-10 — `SystemEvent`: additive per-connector variants, actors retire the global names

`domain/messages.rs`: `SystemEvent` gains
`ConnectorLost(ConnectorId)` / `ConnectorRestored(ConnectorId)`.
`NetworkLost`/`NetworkRestored` **remain in the enum** — "retired from
the actors" is emit-site retirement; the names become the authority's
vocabulary on the same broadcast (design §3.7). The enum **drops
`Copy`** (`ConnectorId` wraps `String`); it keeps `Clone`/`PartialEq`/
`Eq`/serde. Blast radius, re-verified: the engine's select arms bind
by value (fine); the sync actor's ignore arm widens (D5-12); the
system-event logger and the UI repaint driver match `Ok(_)`/`Ok(event)`
catch-alls (no UI diff); `messages.rs`'s `copy_enums_travel_by_value`
test adjusts to Clone semantics; the test ignore-lists
(engine.rs:610) widen. `broadcast` capacity 64 is unchanged (payloads
grow by a short string; event rate unchanged).

### 3.11 D5-11 — the engine arms: tag-scoped `go_offline`

`state/actor.rs` system-event match grows:

```rust
Ok(SystemEvent::ConnectorLost(c)) if !draining =>
    go_offline(&mut core, &shared, &signals, &repo, clock.as_ref(), Some(&c)).await,
Ok(SystemEvent::ConnectorRestored(_)) if !draining =>
    forward_sync_now(&mut sync_outs),
Ok(SystemEvent::ConnectorLost(_) | SystemEvent::ConnectorRestored(_)) =>
    tracing::debug!("connector event ignored: engine draining"),
```

The existing `NetworkLost`/`NetworkRestored` arms are **unchanged**
(they are now driven by the authority with the counted truth).
`go_offline` gains `scope: Option<&ConnectorId>`: `None` = today's
body (every bound board); `Some(c)` filters to boards whose
`remote.connector == c` — via a new pure domain helper
`AppState::bound_boards_of(&self, connector: &ConnectorId) ->
Vec<BoardId>` beside `bound_boards()` (tombstones included, id order —
mirrors the existing helper's contract; unit-tested in the domain
crate). A `ConnectorLost` for a connector with zero bound boards hits
the existing empty-targets early return. Per-board idempotence makes
the N=1 sequence (scoped write, then the global write from the
authority's `NetworkLost`) a no-op second pass — terminal state
identical to today.

### 3.12 D5-12 — the actors' emit rewrite (episode edges verbatim)

`sync/actor.rs`:

- `fail_cycle` gains the connector id (it already closes over
  `system`; the id rides from `cycle`, which holds it) and emits
  `SystemEvent::ConnectorLost(id.clone())` under **exactly today's
  edge** — `matches!(kind, SyncErrorKind::Network) && !st.offline`,
  then `st.offline = true`. Exactly-once per offline episode.
- The success path emits `SystemEvent::ConnectorRestored(id.clone())`
  under `was_offline`, **after** the per-board `Completed` reports are
  sent (report-then-restore order unchanged — the reports land inside
  the board loop; the event fires in the cycle's closing block).
- The select loop's ignore arm widens to
  `StorageFailed | NetworkLost | NetworkRestored | ConnectorLost(_) |
  ConnectorRestored(_)` — each actor ignores the whole network
  vocabulary, including other connectors' events (the authority owns
  counting; the design's provenance fact makes anything else
  unimplementable).
- The module doc's "this actor is the network authority" paragraph is
  rewritten: the actor reports **its own** connector's episodes; the
  app-layer authority counts and emits the global names.

### 3.13 D5-13 — the authority task (DF-7, design §3.7 option (a))

New file `crates/taskboard-app/src/authority.rs` (SPDX
AGPL-3.0-only; the counting authority is app-layer by design — the
registry is app-layer and the engine cannot see connector counts).

**State** — `AuthorityState` = an up/down membership map,
`BTreeMap<ConnectorId, bool>` (`true` = up, meaning *not observed
down*; a connector that has never cycled is presumptively up — its
first transport failure flips it, which is exactly today's first-
failure trigger). Guarded by `std::sync::Mutex`, shared as `Arc`
between the task and `App`'s lifecycle hooks. This is app-internal
bookkeeping, not inter-actor traffic (design's explicit framing;
AGENTS §3's lock rule governs cross-actor communication, which this
is not).

**Wiring** — `bootstrap_with_modules`: seed the state from the module
set (all up) **and subscribe the task's receiver before any module
spawns** (a first-cycle failure cannot race the subscription), spawn
the task, park `Arc<AuthorityHandle>` + join handle on `App`. On boot
failure the unwind sends `Shutdown` (already does) and bounded-joins
the authority with the engine.

**The task loop** — subscribe → update set → maybe-emit (one level of
abstraction):

- `ConnectorLost(c)`: if `c` is a member, mark down; if now *every*
  member is down (nonempty by construction), send `NetworkLost`.
- `ConnectorRestored(c)`: if `c` is a member, mark up; if it is the
  first up (previously zero up), send `NetworkRestored`.
- `Shutdown`: return. `Lagged`: log and return (a 64-event backlog
  miss means the counted truth is stale; the engine's per-connector
  arms keep working, the global edge degrades honestly — same
  convention as the system-event logger). `Closed`: return.
- Events for **non-members** (an id not in the set — a raced
  detach's stale in-flight event) are ignored: membership is the
  lifecycle truth, events are observations of members.
- Exactly-once is structural: emissions happen only on set
  **transitions**; a duplicate `ConnectorLost` for an already-down
  member changes nothing (the actor's episode edge already gives one
  per episode; the authority's transition logic dedups regardless).

**Hooks** (sync fns on the handle, called from `App::attach_connector`
/ `detach_connector*`):

- `attach(id)`: insert up. Emit `NetworkRestored` **iff the pre-insert
  set was nonempty and all-down** (a real hole closed). Coming from an
  empty set (standalone → first attach) emits **nothing** — today's
  attach fires no network event, and byte-for-byte holds.
- `detach(id)`: remove. Emit `NetworkLost` **iff the post-remove set
  is nonempty and all-down** (everything remaining is observed down —
  the counted truth flipped). Removal to the **empty** set (drain-all,
  logout, standalone) emits **nothing**: a connector-less core is
  today's standalone world, which never fires network events.

**The single-connector pass-through argument** (the recorded proof the
dispatch acceptance demands): N=1 seeded `{c: up}`. First transport
failure → actor emits `ConnectorLost(c)` → authority marks down →
all(1)-down → `NetworkLost` — same trigger (first failure of the
episode), same broadcast, strictly after the actor's event. First
success → `Completed` report lands → actor emits
`ConnectorRestored(c)` → first-up → `NetworkRestored` — same
report-then-restore order. The engine's global arms see identical
events for identical reasons; it additionally reacts to the
per-connector events with the scoped write, whose terminal state
equals the global write (idempotent second pass). The swap dance
(detach → attach) at N=1 passes through the **empty** set on both
steps — silent both times, exactly today. No hysteresis is added (the
episode-edge rule *is* the debounce; design's flapping note).

**Shutdown ordering**: actors joined → authority joined bounded (it
exited on the `Shutdown` broadcast long before) → reports twin
dropped → engine joined. `ShutdownOutcome` unchanged.

### 3.14 D5-14 — the daemon-side attach verb's name parameter

`ipc/protocol.rs`: `Request::AttachConnector` grows `#[serde(default)]
pub name: Option<String>` — `None` = the default (first) connector.
**Additive on the wire, no `PROTOCOL_VERSION` bump** (ADR 0012
amendment's additive precedent): `None`-carrying requests are
byte-identical to today's; the client ceremony always targets the
default (it rewrote entry 0), so it sends `None`; the `Some(name)`
path is pinned by IPC unit tests and consumed by chunk 6's
`--connector` flags.

`session_ceremonies::attach_from_persisted` grows `name:
Option<&str>`: resolve the connector set from the re-read config
(D5-2); target = the entry named `name`, else the first (`None`); no
such name ⇒ `NotConfigured`; the request's `(server_url, username)`
must equal the target entry's pair (`IdentityMismatch` otherwise — the
identity check survives as the guard that the client connected *that*
connector); build the client from the entry + the daemon's store;
swap = `detach_connector_named(target)` when live; attach under the
target's id; settle. `AlreadyAttached` on the wire is produced by
`AttachError::ConnectorNameInUse`. Legacy equivalence: the resolved
set is `[nextcloud(s, u)]`, identity matching is byte-for-byte
today's. `deck_client_from` (config, store) becomes
`deck_client_from_spec(spec, store)` taking the entry's identity —
the session's `build_client` uses the refreshed config's **default
spec**.

Recorded skew corner (accepted): an old daemon ignores the unknown
`name` key and attaches its default — a new client asking for a
non-default connector under a one-release daemon/client skew sees
`Attached` for the wrong connector. The nudge is best-effort by
construction (every other degradation already ends in "restart the
daemon"); same-binary upgrades (the normal case) are exact.

### 3.15 D5-15 — the daemon-side detach handler goes set-aware (recorded scope addition)

`ipc/server.rs:385`'s detach handler identity-matches
`config.nextcloud` — a source the same chunk's array shape orphans
(under an array file it resolves standalone and answers `NoConnector`
while two connectors run: a known-wrong shipped path). Rework in the
same motion (~15 lines + unit test): resolve the set; find the entry
whose `(server_url, username)` equals the request's; empty set ⇒
`NoConnector`; no match ⇒ `IdentityMismatch`; match ⇒
`detach_connector_named`. Legacy equivalence: the set is
`[nextcloud(s, u)]`, matching is byte-for-byte today's comparison.

### 3.16 D5-16 — session ceremonies at N ≥ 2 (recorded corner)

- `connect_session`: ceremony (rewrites the default entry, D5-5) →
  refresh → swap = `detach_connector_named(default_id)` when that name
  is live (**connector 2 survives the swap** — pinned by test) →
  `attach_connector(default_id, client)` → settle. The default id
  comes from the refreshed config's first spec.
- `logout_session`: keeps the drain-all `detach_connector()` — the
  scalar-world ceremony's honest generalization. **Recorded corner**:
  at N ≥ 2 a REPL `logout` tears down the session's whole connector
  set (the config array survives; a reboot restores both). `logout`
  under an array file refuses `NotConfigured` on the strip step
  (nothing strippable) — honest, untouched. The plural REPL surface
  is chunk 6's.

### 3.17 D5-17 — CHANGELOG

`CHANGELOG.md` Unreleased lines: plural `[[sync.connectors]]` config
with legacy-scalar compatibility and typed mix rejections; N-actor
connector registry with named attach/detach; per-connector network
events with the all-down authority (single-connector behavior
unchanged). Chunk 8 promotes.

### 3.18 D5-18 — the two-connector tier-2 test design (MD-5)

**Harness** — `scripts/nextcloud_it_setup.sh`'s `up` additionally
brings up instance **B** (container `taskboard-it-nextcloud-b`, port
default 8371 via `TASKBOARD_IT_PORT_B`, same compose file/project
suffix, same Deck install + user + app-password flow) and the export
block gains `TASKBOARD_IT_DOCKER_B_URL/_USER/_TOKEN`; `down` tears
both down. Existing consumers unchanged (A's names/vars untouched);
the CI workflow's single `up`/`down` steps now cover both tests — no
workflow diff. Same harness conventions (loopback, throwaway,
export-block), no new mechanics beyond the second instance.

**Test** — `it_docker.rs`, beside the v0.1.x tests, following their
two-half pattern:

- Hermetic half (runs everywhere): the N=2 registry + authority over
  two `FakeModule`s (D5-19's catalogue) — the exit criterion's
  plumbing without a server.
- Dockerized half `it_nextcloud_docker_two_connectors_live_and_
  partial_outage` (`#[ignore]`, filter prefix `it_nextcloud_docker` so
  CI's existing `--run-ignored only -E 'test(it_nextcloud_docker)'`
  runs it): read both `LiveCfg`s (A + B; absent B ⇒ the MD-5 honest
  skip note in the report); create one board on each server via each
  `DeckClient`; **fixture the cross-connector world in the db**
  (chunk 5 has no ceremony verb that mints a non-default tag —
  `BindBoard` tags the engine's `default_connector`, and that is
  chunk 6's move-board surface): a `write_two_connector_db` sibling of
  `write_v01x_db` (raw sqlx, current schema, no rollback) writing two
  boards with tagged bindings — `RemoteBoardBinding { connector:
  "alpha", board: <A's board> }` / `{ connector: "beta", board: <B's
  board> }`. Config: a real `[[sync.connectors]]` file (names
  `alpha`/`beta`, both `backend = "nextcloud"`, the two URLs/users);
  credentials via a handwritten keyed fixed store (two
  `(server, user) → password` entries — `EnvCredentialStore` is
  single-secret by construction); a short `poll_interval_secs` so
  cycles are observable. Boot `bootstrap(config, store, LogSink)`.
- Assertions: (1) **two connectors live** — both boards' status rows
  settle `Idle` with `last_success` set, each binding tagged with its
  own connector (the N=2 exit criterion); (2) **per-connector health
  visible** — per-board rows + binding tags distinguish the two
  connectors (chunk 6 renders the `connectors` array; chunk 5 pins the
  underlying attribution — recorded interpretation); (3) **one-down**:
  `docker stop` instance A's container (`std::process::Command` — a
  new tier-2-only orchestration step, recorded), then within a
  deadline: alpha's boards' rows flip `Offline`; beta's boards'
  `last_success` keeps advancing (its cycles continue); a subscribed
  broadcast receiver observes **no `NetworkLost`** while beta
  responds; (4) **all-down**: `docker stop` B ⇒ `NetworkLost` observed
  (the D-W7 bar's both halves). Cleanup `docker start`s both
  best-effort before the teardown; `with_deadline` bounds everything;
  container-down flakiness degrades through the MD-5 honest record.

### 3.19 D5-19 — the single-connector byte-for-byte pin list

The guarantee is behavioral, not textual: event *variants* change by
design (actors emit per-connector names); every observable journey
outcome is pinned unchanged:

- Config: the entire existing config unit corpus passes unmodified
  (legacy decode/validate/synthesis untouched) — the strongest pin.
- `connectors.rs` synthesis tests: updated only where `ConnectorSpec`
  grows identity fields; assertions on names/counts unchanged.
- Bootstrap B-series: `b1`–`b10`, `b12`, `b13` pass unmodified;
  `b11` swaps its refusal assertion to `ConnectorNameInUse` (the
  named variant of the same guard).
- v0.1.x additive-upgrade boot test, both halves: **unmodified**
  (scalar synthesis is verbatim).
- Sync actor A7 (wiremock): rewritten to expect `ConnectorLost(id)` /
  `ConnectorRestored(id)` with the actor's own id — same exactly-once
  episode edge, same backoff pacing, same report-then-restore order;
  the sibling authority test (below) pins that the *global*
  `NetworkLost`/`NetworkRestored` still fire exactly once per episode
  in the same order at N=1.
- Engine arm tests (`engine.rs` A7 + the Offline-persistence tests):
  **unmodified** — the global arms' behavior is unchanged, now driven
  by the authority.
- CLI journeys (`cli_connect`, `cli_standalone`, `cli_attach`,
  `it_docker` daemon journeys): unmodified; any output drift is a
  finding.
- Hermetic E2E (`e2e_hermetic.rs`) network pins: updated to the
  per-connector variants with ids asserted.

## 4. TDD plan (red → green per phase; AGENTS §5 rules)

Typed outcomes only — rejection kinds asserted by variant, offending
names/keys asserted structurally (payloads), never message text; no
sleeps (`tokio::time::pause()`/`advance()` for FSM edges,
poll-a-predicate-with-deadline, join handles); fakes over mocks (the
`FakeModule` story; the keyed fixed credential store); one behavior,
one test, at the lowest layer that owns it.

### 4.1 Phase 1 — domain (messages.rs, state.rs)

1. `SystemEvent` serde round-trips `ConnectorLost/Restored` carrying
   ids; the by-value travel test adjusts to Clone.
2. `bound_boards_of`: filters by tag; tombstones included; id order;
   empty for a foreign tag.

### 4.2 Phase 2 — sync actor (actor_wiremock.rs)

3. A7 rewrite: transport failure broadcasts `ConnectorLost(id)` with
   the actor's id exactly once per episode; backoff pacing unchanged;
   recovery reports `Completed` **then** broadcasts
   `ConnectorRestored(id)`.
4. Foreign network events (including another connector's
   `ConnectorLost`) leave the loop unaffected.

### 4.3 Phase 3 — state engine (engine.rs)

5. `ConnectorLost(c)` marks only c's boards `Offline` (two boards, two
   tags; rows scoped; the other board's row untouched).
6. `ConnectorLost(c)` with zero c-boards writes nothing.
7. `ConnectorRestored(c)` forwards `SyncNow` (the existing global-arm
   nudge assertion, per-connector variant).
8. Draining ignores both variants (the ignore-arm sibling).
9. Existing `NetworkLost`/`NetworkRestored` arm tests pass
   **unmodified** (the pin).

### 4.4 Phase 4 — config (config.rs)

10. Array decode: two entries, file order, first = default;
    `deny_unknown_fields` on entries (unknown key ⇒ `Decode`).
11. `connectors = []` alone is valid (zero connectors).
12. Mix rejections, one test per DF-6 instance: file
    `backend`+array (any backend value — the #1/#3/#6 instances
    parametrized, incl. the empty-array-with-nextcloud case);
    `[nextcloud]`-identity+array; duplicate name; each non-kebab
    shape from D5-4's reject list.
13. `[nextcloud] credential_store` + array is valid.
14. Entry semantics: bad URL / empty username / `standalone` entry ⇒
    `InvalidConnectorEntry` with the name + field.
15. Validation order: array + garbage `[nextcloud]` URL ⇒ the mix
    error, not `InvalidServerUrl`.
16. Env mix: `TASKBOARD_SYNC__BACKEND` + file array ⇒
    `ConnectorShapeMixed` (probe case 6 + loudness).
17. `kind()` strings for the five new kinds; the discriminant test
    extends.
18. The full legacy corpus passes unmodified.

### 4.5 Phase 5 — ceremony writer (connect.rs)

19. Overlay path A: today's writes verbatim (comment preservation,
    file creation, `backend` iff switched) — existing pins.
20. Overlay path B: entry 0's identity rewritten; entry 1 untouched;
    no `backend` write anywhere; no `[nextcloud]` keys created; a
    comment elsewhere survives.
21. Round-trip both shapes: overlay → `extract_raw` → decode+validate;
    identity in the right home.
22. Guards: array-present raw ⇒ no flip offered (no prompt, `false`);
    repoint compares entry 0's identity (different ⇒ the typed
    refusal ladder; same ⇒ `None`).

### 4.6 Phase 6 — registry + authority (connectors.rs, bootstrap.rs, authority.rs)

23. `configured_connectors`: array world specs (order, identity,
    default-first); legacy synthesis unchanged; empty array ⇒ `[]`.
24. Authority state unit tests (sync fns, no runtime): every D5-13
    edge — N=1 pass-through, N=2 last-down/first-up, duplicate-event
    dedup, attach/detach edges, empty-set silence, non-member
    events ignored.
25. Authority task over a real broadcast: seeded `{a, b}`;
    `Lost(a)` ⇒ no global; `Lost(b)` ⇒ `NetworkLost`; duplicate
    `Lost(b)` ⇒ nothing; `Restored(a)` ⇒ `NetworkRestored`;
    `Restored(b)` ⇒ nothing.
26. `FakeModule` grows an optional event script (emits
    `ConnectorLost/Restored` for its id on `SyncNow`); boot N=2 with
    scripted failures: the boot-integrated authority pin — a's lost ⇒
    silent, b's lost ⇒ `NetworkLost`, b's restored ⇒
    `NetworkRestored`.
27. Boot N=2 hermetic exit-criterion half: both modules registered;
    `RequestSync` fans to both (each script lands on its own adopted
    board, tagged per module); `connector_commands("a"/"b")` resolve,
    unknown ⇒ `None`; `attach_connector("a", …)` over the live `a` ⇒
    `ConnectorNameInUse`; `detach_connector_named("a")` stops only a
    (b still answers a nudge).
28. `b11` updated (D5-19); B-series + v0.1.x halves pass unmodified.

### 4.7 Phase 7 — surfaces (commands/mod.rs, status, ipc, session)

29. `require_connector`: legacy pass/refuse unchanged; array nonempty
    ⇒ pass; empty array ⇒ `SyncDisabled` (same kind).
30. Backend-word displays: array ⇒ first entry's word; legacy
    byte-identical; `connector_flag(configured, attached)`.
31. IPC: attach request `name` field serde (absent ⇒ `None` ⇒ today's
    bytes); `attach_from_persisted` name resolution (named hit,
    unknown name ⇒ `NotConfigured`, per-entry identity mismatch);
    daemon detach set-aware matching (right connector detached;
    legacy equivalence).
32. Session connect under an array config: the swap detaches only the
    default (connector 2 stays live), attaches the refreshed default
    id.

### 4.8 Phase 8 — tier-2 (it_docker.rs + harness)

33. The D5-18 dockerized test; the hermetic half already landed in
    phase 6. MD-5 honest recording when docker/B is absent.

### 4.9 Test matrix summary

| Layer | New | Updated | Unmodified pins |
|---|---|---|---|
| domain | 2 | 1 (Clone test) | ids/state corpora |
| sync actor | 2 | A7 + hermetic net pins | push/pull corpora |
| state engine | 4 | ignore-lists | global-arm tests, snapshots |
| app config | ~9 | kinds test | full legacy corpus |
| app connect | ~6 | overlay pins | receipt/serde tests |
| app registry/authority | ~6 | b11 | B-series, v0.1.x halves |
| app surfaces/ipc | ~6 | flag/display tests | CLI journeys |
| tier-2 | 1 (+harness) | — | existing docker tests |

No insta snapshot pass is owned here: no snapshotted `AppState` shape
changes (events are not state; the status rows are already per-board).
Any snapshot drift is a finding to investigate, never a regeneration.

## 5. Implementation phases (stage 2)

Ordered so every commit stands alone (rebase-merge hygiene; a fix
lands in the commit it fixes, never a duplicate-subject follow-up):

1. `feat(domain): per-connector system event variants` — messages.rs
   (+ Clone fallout), state.rs helper + tests. Phases 4.1.
2. `feat(sync): the offline FSM emits per-connector episode events` —
   actor rewrite + tests. Phase 4.2.
3. `feat(state): tag-scoped go_offline arms` — engine arms + tests.
   Phase 4.3.
4. `feat(app): [[sync.connectors]] decode and typed rejections` —
   config.rs (+ kinds, kebab, entry validation) + connectors.rs
   resolution + tests. Phases 4.4/4.6.23.
5. `feat(app): shape-aware connect ceremony writer` — connect.rs +
   tests. Phase 4.5.
6. `feat(app): the all-down network authority` — authority.rs + the
   authority's bootstrap wiring + tests. Phase 4.6.24–25.
7. `feat(app): named connector registry and attach verb` — bootstrap
   registry naming, attach/detach primitives, require_connector,
   status/flag displays, ipc attach name + detach set-awareness,
   session ceremonies + tests. Phases 4.6.26–28, 4.7.
8. `test(app): the two-connector tier-2 journey` — harness B instance
   + fixture writer + the dockerized test + the FakeModule boot pins
   if not already in 7. Phase 4.8.
9. `chore(changelog): Unreleased lines for plural connectors` —
   D5-17. (Or folds into 7 — stage 2's call, standalone is fine.)

Gates after each phase locally; the full AGENTS §10 suite before
handoff (below).

## 6. Quality gates (AGENTS §10, verbatim sequence)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo test --doc --workspace
cargo deny check
cargo llvm-cov nextest --workspace --all-features --no-tests=warn --no-report
scripts/coverage-quarantine-profraw.sh target/llvm-cov-target
cargo llvm-cov report --lcov --output-path lcov.info
rm -rf target/llvm-cov-target
```

Tier-2: run the dockerized test if the runner has docker (both
instances up via the extended `up`); otherwise record the absence
honestly (MD-5) — which assertions ran hermetically, which need the
tier-2 runner. New `.rs` files: `authority.rs` carries
`// SPDX-License-Identifier: AGPL-3.0-only` (app crate);
`#![forbid(unsafe_code)]` holds everywhere except `taskboard-ui-slint`
(no UI diff this chunk).

## 7. Risks & recorded deviations

- **The authority's edge semantics** (the dispatch's named risk) —
  settled in D5-13: emissions on set transitions only; hooks' edge
  rules (attach fills a real all-down hole ⇒ `NetworkRestored`;
  detach leaving all-down-nonempty ⇒ `NetworkLost`; empty set always
  silent). The N=1 swap passes through the empty set — both steps
  silent, byte-for-byte. Residual risk: the N ≥ 2 swap flicker
  (Lost/Restored around a session swap under partial outage) is
  honest-but-noisy; accepted, no hysteresis in-wave.
- **`Copy` removal from `SystemEvent`** — compiler-shepherded; every
  match site re-verified (engine, actor, logger, tests); the UI's
  catch-all means zero UI diff. Residual: a stray `.copied()` in a
  test helper — clippy/gates catch.
- **`toml_edit` array-of-tables indexing** — `[[sync.connectors]]`
  parses as `ArrayOfTables`; `doc["sync"]["connectors"][0][key]`
  navigation is supported but the entry may also appear as a plain
  array of inline tables if a user writes `connectors = [{…}]` (which
  also decodes through figment!). The writer tests cover both
  renderings; the implementation must navigate generically (an
  `Item::Value(Array)` path), not assume `ArrayOfTables`.
- **Env-mix loudness is new behavior** (D5-3):
  `TASKBOARD_SYNC__BACKEND` over an array file now errors where it
  was previously meaningless-but-decodable. Recorded; documented in
  the CHANGELOG line's body.
- **IPC one-release skew corner** (D5-14): an old daemon ignores
  `name`. Accepted within the best-effort nudge; no version bump.
- **Tier-2 orchestration** (`docker stop`/`start` mid-test) is new
  mechanics — deadline-bounded, best-effort restore, MD-5 honest
  degradation. CI brings both instances up in the existing single
  `up` step.
- **Aggregate fold under partial outage** (observation, no chunk-5
  action): `SyncStates::aggregate()` is worst-phase, so the aggregate
  surfaces (daemon `status.sync`, `wait_for_boot_pull`) read `Offline`
  when one connector's boards are offline even though the network is
  partially up. This is chunk 1's landed fold and chunk 7's
  selected-board read is the designed remedy for the badge; the tier-2
  test asserts the *broadcast* silence (the global event) plus
  per-board rows, not the aggregate. Recorded for chunk 7's plan.
- **Scope additions with reasons** (all compile-forced or the minimal
  honest completion of the normative list): `connectors.rs` identity
  fields (D5-2 — the file list's "config decode" home);
  `authority.rs` as a new file (D5-13 — SRP: counting is one
  responsibility); `detach_connector_named` (D5-7 — the named
  registry's mirror; the session swap needs it); daemon detach
  set-awareness (D5-15 — the same chunk's config shape orphans its
  identity source); display/backend-word callers beyond `mod.rs`
  (D5-9 — every `sync_backend()` caller re-classified, table in
  D5-8); domain `bound_boards_of` (D5-11 — the scoped write's pure
  core); harness B-instance (D5-18 — the two-instance exit criterion
  needs two instances). No deviation from an ACCEPTED design decision.

## 8. Estimated diff

~16 files, ~2,000–2,400 lines including tests:

| File | Est. |
|---|---|
| `app/src/config.rs` (+tests inline) | ~380 |
| `app/src/connectors.rs` | ~110 |
| `app/src/commands/connect.rs` (+tests) | ~230 |
| `app/src/authority.rs` (new, +tests) | ~300 |
| `app/src/bootstrap.rs` (+tests) | ~380 |
| `app/src/commands/mod.rs` + `status.rs` + `repl.rs`/`tasks.rs` gates | ~90 |
| `app/src/session_ceremonies.rs` | ~60 |
| `app/src/ipc/protocol.rs` + `server.rs` (+tests) | ~200 |
| `domain/src/messages.rs` + `state.rs` | ~80 |
| `sync-nextcloud/src/actor.rs` | ~60 |
| `state/src/actor.rs` | ~60 |
| `sync-nextcloud/tests/actor_wiremock.rs` + `e2e_hermetic.rs` | ~120 |
| `state/tests/engine.rs` | ~90 |
| `app/tests/it_docker.rs` + `tests/common/mod.rs` | ~320 |
| `scripts/nextcloud_it_setup.sh` | ~60 |
| `CHANGELOG.md` | ~6 |

House scale: **L**, matching the implementation plan's chunk-5 rating.

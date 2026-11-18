# The Ceremony Identity Injection — Chunk Plan (tier-2 P5, #751)

- **Date**: 2026-11-18
- **Kind**: chunk plan for tier-2 round chunk P5 (`feat/ceremony-identity-injection`),
  three-stage execution per `.artifacts/plans/2026-11-18-tier2-dispatch-plan.md` §7.
- **Origins**: the dispatch plan §1 (P5 row) and §7 (normative); the
  recommendation report §Tier-2 (#751 entry); issue #751 (body + the two
  comments); round-6 chunk-5 plan `.artifacts/plans/2026-11-12-ui-r6-files-boards-surface-plan.md`
  (D-3 re-key pattern, §2 step-3 recorded fallback tests).
- **Status**: READY. No UX journey plan in origin (`n/a — test seam +
  injection`, per the dispatch plan's coverage column); no surface delta.
- **Non-goals**: the boot gate's ambient identity read, the
  files-sync actor's spawn-gate data-dir re-derivation, the convergence
  harness (`run_mover`), and the `MoveWorld` fake machines are all
  out of scope (D-7 below).

## 0. Seam survey (evidence, read on the worktree at main `d0e80ec6`)

- `crates/taskboard-app/src/board_move_ceremony.rs`
  - `run_on` (:555) / `run_until` (:581) take
    `(engine, config, store, board, direction, [stop_after,] timeout)`;
    both delegate construction to `app_world` (:609).
  - `app_world` (:622–629) is the **only ambient identity read on the
    ceremony path**: `dirs::data_dir()/taskboard/device.json` →
    `taskboard_sync_files_nextcloud::device::load_or_create(&device_json, clock)`
    → `DeviceIdentity` into `SyncContext.identity`. `None` data dir maps
    to `MoveError::FilesSyncDisabled` (typed, preserved by D-1).
  - `AppWorld` (:296) holds a concrete `WebDavFilesTransport` (:301)
    built from `files_sync_spawn_decision(config)` + the credential
    store — the transport is NOT injectable and this chunk does not
    inject it (the hermetic tests point the config anchor at wiremock).
  - The ceremony's own tests (:676+, the fake `MoveWorld` machines) never
    construct `app_world` — they survive **unchanged** (the D-3 pin).
- Boot gate (same global path, **divergent mechanism — recorded**):
  `bootstrap.rs` `resolve_device_identity` (:145) →
  `resolve_device_identity_at(data_dir, clock)` (:152) — it *already*
  has the `_at` path seam for its own tests. The spawn gate
  `files_sync_spawn` (:213–220) re-derives `dirs::data_dir()` a second
  time before handing `device_json` to the actor (the actor itself takes
  the path as a parameter). Neither is on the ceremony call path.
- Call sites of `board_move_ceremony::run_on` (exactly four; the
  `habits::run_on` matches are a different function):
  1. CLI verb, `commands/mod.rs:1185` (`run_core` bootstrap per
     invocation; `app.engine`/`&config` in scope);
  2. REPL verb, `commands/mod.rs:2174` (`session.session_config()`);
  3. UI move arm, `ui_requests.rs:1693` (`move_board_to_files`);
  4. UI undo arm, `ui_requests.rs:1785` (`MoveDirection::Undo`).
  `ConnectionWorkspace` (`ui_requests.rs:79`) currently carries
  `config_write_path / store / control / engine` — no device path.
  Construction sites: `ui.rs:805`, `main.rs:854`,
  `tests/ui_settings.rs:84` (`booted_workspace`),
  `tests/ui_live_settings.rs:35`, `ui_requests.rs:2001` (unit-test fixture).
- The twin r8 files-sync-status surface (`files_sync_surface.rs`) does
  **not** share `app_world` (it reads the status watch lane) — recorded,
  no overlap.
- Convergence tests (`tests/files_sync_convergence.rs`) use
  `FakeFilesTransport` around a hand-built `SyncContext` (`run_mover`
  :836 calls `publish_move_floor` directly, never `run_on`) — unaffected;
  their WebDAV mock-helper grammar is the reuse candidate for the
  happy-path test.
- The two recorded fallback tests (r6 plan §2 step 3): 
  `move_reports_a_queued_outbox_typed` and
  `move_commits_a_deck_board_onto_files` — never landed; the ambient
  identity load is exactly what blocked them (issue #751). They belong in
  `tests/ui_settings.rs` on the app-arm harness (`booted_workspace` +
  `spawn_ui_request_forwarder`).
- **P3 overlap (recorded per dispatch brief)**: PR #1012 (open, unmerged)
  deletes `MoveDirection::ToDeck` / `MoveError::ReverseFenced` — the
  fence lives at `run_until`'s head (:590) and the CLI verb's
  early-return + `move_direction` helper (`commands/mod.rs:1172`,
  :2010) — i.e. inside this chunk's diff sites (the ceremony signature
  and two of the four call sites). The chunk is dispatch-independent;
  the stage-3 rebase resolves order. The plan below is written against
  the current main state (ToDeck still present); the P5 implementation
  must not *rely* on the ToDeck arms existing (write the edits so they
  apply cleanly with the ToDeck/ReverseFenced lines simply absent).

## 1. Decisions

### D-1 — Injection shape: the `device.json` **path** parameter, not a resolved `DeviceIdentity`

The criterion from the dispatch brief: resolved identity is preferred
when construction is cheap and pure; path when the ceremony owns loading
semantics. Here the ceremony **owns loading semantics**: `load_or_create`
is async, fallible, and carries the fork-guard contract (create-exactly-
once, atomic tmp-rename, corrupt-file-never-regenerated —
`device.rs` module docs). Resolving at the call sites would either (a)
duplicate that ceremony at three callers, or (b) make callers pre-load a
file whose create arm they then cannot own. The path is a cheap,
copyable, pure value; the single `load_or_create` call stays in
`app_world`.

Shape:

```rust
pub async fn run_on(
    engine: &EngineHandle,
    config: &AppConfig,
    store: &Arc<dyn CredentialStore>,
    board: BoardId,
    direction: MoveDirection,
    device_json: &std::path::Path,      // new, last before timeout
    timeout: Duration,
) -> Result<MoveReceipt, CommandError>
```

`run_until` gains the same parameter (same position, before
`stop_after`/`timeout` — after `direction`, mirroring `run_on`).
`app_world` takes it and drops its `dirs::data_dir()` call.

Default-path helper (the resolved default, one definition):

```rust
/// The platform default identity home `<data>/taskboard/device.json`;
/// `FilesSyncDisabled`-mapped when the platform has no data dir —
/// the same typed outcome `app_world` produced pre-injection.
pub fn default_device_json() -> Result<PathBuf, CommandError>
```

The `None`-data-dir → `MoveError::FilesSyncDisabled` mapping moves from
`app_world` into this helper; the typed outcome for default-path callers
is byte-identical (round law (c): a seam, not a semantic change).

### D-2 — Call-site updates (behavior-identical)

- CLI verb (`commands/mod.rs:1185`) and REPL verb (:2174): pass
  `&crate::board_move_ceremony::default_device_json()?`. The `?`'s
  `CommandError` is the closure's error type already (CLI); the REPL arm
  maps identically (`emit_result` of the same error class).
- **`ConnectionWorkspace` gains `device_json: PathBuf`** (documented as
  "the identity file this workspace's boot minted; the move ceremony's
  injection point"). The two arms (`move_board_to_files`, the Undo arm)
  pass `&workspace.device_json`. This is what makes the UI-arm tests
  hermetic *through the forwarder* — no `XDG_DATA_HOME` mutation, no
  cross-test race (the #751 acceptance).
- Construction sites: `ui.rs:805` and `main.rs:854` pass
  `crate::board_move_ceremony::default_device_json().expect(...)` —
  acceptable because both are post-bootstrap production boot paths where
  a missing data dir already failed (or degraded) identity resolution at
  boot; the `.expect` is on an invariant, not a test of behavior.
  *(Alternative rejected: storing the resolved path in `App` — wider
  API churn for zero behavioral gain; the boot gate's own `_at` seam
  already covers bootstrap's tests.)*
- Test construction sites: `ui_settings.rs::booted_workspace` and
  `ui_live_settings.rs:35` and `ui_requests.rs:2001` set a tempdir path
  (or an explicit path) — each suite owns its identity file.
- The ceremony module's fake `MoveWorld` tests: unchanged (pin).
- `tests/files_sync_convergence.rs`: unchanged (pin).

### D-3 — The two fallback tests, hermetic shape (TDD note: they cannot run pre-injection)

**Before-shape evidence (recorded instead of a red run)**: both tests
drive the ceremony past `app_world` through the UI request forwarder;
pre-injection the identity load is the ambient `dirs::data_dir()` read,
so the tests would touch the real platform data dir (or race under
`XDG_DATA_HOME` mutation) — exactly the #751 blocker. The first green
run post-injection is the evidence; the injection diff and these tests
land in the same commit-sequence.

Both live in `tests/ui_settings.rs`, app-arm harness (`booted_workspace`
+ `spawn_ui_request_forwarder` + `send`), the Publish section's pattern:

- `move_reports_a_queued_outbox_typed` — Deck-bound board with a queued
  op; wiremock mounts **only** the Deck-pull endpoint; the request
  replies the typed outbox class (`matches!` on the mapped
  `SettingsError` variant), the binding stays Deck (structural: the
  snapshot row's `remote` still carries the Deck shape). Pre-network by
  construction (the quiescence guard fires before the floor publish).
- `move_commits_a_deck_board_onto_files` — wiremock Deck server
  (`mount_standard_pull` if reusable, else the minimal pull grammar) +
  the WebDAV mounts for the ceremony's probe/put/confirm steps, reusing
  the convergence tests' mock-helper grammar where it serves without
  re-mounting their whole cycle. Assert terminal state only: reply `Ok`,
  the snapshot row's `bound_connector == "files-sync"`, the engine state
  `binding.board.as_files().is_some()`, files phase Idle. No sleeps; the
  reply implies settlement.
  - **Recorded fallback (r6 §2 step 3, still binding)**: if the WebDAV
    mount grammar cannot serve the four steps without re-mounting the
    whole convergence cycle, this test drops to `run_until` coverage
    (stop after `Published`) + the refusal tests, and the deviation is
    reported.
- Identity fixture: each test mints its identity via the injected
  tempdir path — `booted_workspace` puts `device_json` in its own
  `tempfile::tempdir()`; the ceremony's `load_or_create` create arm
  mints there (no shared dir, no env). **No `XDG_DATA_HOME` anywhere.**
- The existing pre-`app_world` refusal tests stay untouched (round law
  (c)); they prove request → arm → guard → typed reply.

### D-4 — Boot gate stays ambient (out of scope — recorded + filed)

`resolve_device_identity` / `files_sync_spawn` keep the ambient
`dirs::data_dir()` read. Parameterizing `bootstrap()` ripples its
signature across every test that boots an `App`; the gate already has
`resolve_device_identity_at` for its own tests. Consequence (existing
behavior, not introduced here): in-process suites that call
`bootstrap()` (`booted_workspace` among them) still mint the real
platform `device.json` — filed as a backlog issue, not fixed here.

### D-5 — Layering

All edits stay inside `taskboard-app` (the ceremony module, its call
sites, the workspace struct, tests). No crate-boundary change; no new
dependency; `dirs` usage simply moves from `app_world` into
`default_device_json` (same crate, same dependency).

### D-6 — Typed outcomes only (round law (a))

No new error variants; the only mapping moved is
`data_dir() == None → MoveError::FilesSyncDisabled` (into
`default_device_json`). The new tests assert variant membership and
structural state, never text.

## 2. Implementation order (stage 2)

1. **Signature + helper** (compile-green): `default_device_json`,
   `run_on`/`run_until`/`app_world` gain the parameter; the four call
   sites + the five workspace construction sites update in the same
   commit (the workspace field and the ceremony parameter move together
   — no intermediate broken state).
2. **Hermetic refusal surface**: `booted_workspace` + the other test
   constructions get tempdir identities; existing suites stay green.
3. **The two fallback tests** (D-3), happy-path last.
4. Gate: pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest
   run --workspace` (background + poll; commit before any wait; defer
   per the #879/#894 rule if contended).

## 3. Risks

- **P3 textual overlap** (PR #1012, unmerged): deletes lines inside this
  chunk's diff sites (`run_until`'s fence head, `commands/mod.rs` verb
  arm). Mitigation: chunk is dispatch-independent; edits written to
  apply with the ToDeck/ReverseFenced lines absent; stage-3 rebase
  resolves order.
- **Happy-path WebDAV mount cost**: the ceremony drives
  `WebDavFilesTransport` against wiremock; the convergence mock grammar
  may not transfer one-to-one. Mitigation: the recorded fallback (D-3)
  bounds the worst case to `run_until` mid-ceremony coverage.
- **`App`-less `.expect` on `default_device_json` in production
  constructors**: two call sites unwrap. Acceptable (post-boot
  invariant; the boot already resolved identity there); flagged here so
  stage-3 review judges it explicitly.
- **Ambient boot read persists in tests** (D-4): the new tests are
  hermetic for the *ceremony* path; the boot inside `booted_workspace`
  still mints the platform file. Pre-existing, filed as backlog.

## 4. Estimated diff

~280–380 lines including tests: ceremony signature + helper ~40,
commands/mod.rs ~10, ui_requests (field + two arms) ~25, ui.rs/main.rs
+ remaining constructions ~20, the two fallback tests ~180–280
(wiremock mounts dominate). No crate-boundary or dependency changes.

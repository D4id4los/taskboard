# App-Test Hygiene (#984 / #1013 / #982 / #981) — Chunk Plan

- **Date**: 2026-11-20
- **Kind**: chunk plan — stage-1 output for dispatch-plan chunk **B3**
  (round 3), branch `test/app-test-hygiene`, worktree
  `../taskboard-WT-test-app-test-hygiene`.
- **Origin (normative)**:
  `.artifacts/reports/2026-11-19-backlog-reduction-round3-scope-report.md`
  §B3; `.artifacts/plans/2026-11-19-backlog-reduction-round3-dispatch-plan.md`
  §1 (wave 4), §9; issues #984 #1013 #982 #981.
- **Landed anchor**: R4's test-binary consolidation has merged — the
  app suite is 8 binaries; grouped roots (`cli_sync.rs`, `cli_tasks.rs`,
  `cli_output.rs`, `app_core.rs`, `app_services.rs`) pull members from
  `tests/cli/`, `tests/core/`, `tests/services/`, each root consuming
  one shared `mod common` (`tests/common/mod.rs`) via `crate::common`.
  `it_cli.rs`, `it_docker.rs`, `mcp_smoke.rs` remain standalone roots.
- **No UX plan needed**: zero user-visible surface delta (test
  hygiene + one boot seam parameterization).

## 0. Sweep inventory (against the landed shape)

### 0.1 #984 — ambient `TASKBOARD_*` composition without the env lock

Method: per-file counts of composition sites (`AppConfig::load`,
`extract_raw`, `extract_file_only`, `config::path`) vs env-lock
acquisitions (`env_lock` / `taskboard_env_lock` / `xdg_lock`) across
`crates/taskboard-app/tests/`. Composing call sites exist in:

| File (binary root) | composing sites | env-lock sites | exposure |
|---|---|---|---|
| `core/config.rs` (`app_core`) | 32 | 31 | near-covered; the remaining gaps are explicit-path loads that still compose the figment **Env provider** (e.g. the `config::path(None)`/override test) — audit each |
| `core/repl.rs` (`app_core`) | 11 | 1 | **largest gap** — 10 `AppConfig::load(Some(path))` sites compose ambient env unlocked |
| `cli/cli_connect.rs` (`cli_tasks`) | 5 | 2 | the #897-named legs are locked; remainder (`:549` `AppConfig::load`) unlocked |
| `cli/cli_editing.rs` (`cli_tasks`) | 5 | 1 | #897-reroute legs locked (`:547`); 4 later `extract_raw` mirrors unlocked |
| `cli/cli_login.rs` (`cli_tasks`) | 2 | 4 | locks present; verify the two `AppConfig::load` sites are inside a held guard's window |
| `services/ipc.rs` (`app_services`) | 2 | 0 | **uncovered** |
| `core/dispatch_routing.rs` (`app_core`) | 1 | 4 | covered by existing guards |
| `it_docker.rs` (standalone) | 2 | 9 | partially covered; the `:351`/`:470` `extract_raw` sites need guard windows checked |

Binaries with zero composition sites (`cli_sync`, `cli_output`,
`mcp_smoke`, `it_cli`, the pure-output `cli/*` members, `daemon_signals`,
`lockfile` (composition-wise), `secrets`, `tty`, `habits`, `export_pdf`,
`ipc`-cli, `attach`, `verbs`, `standalone` composition-wise) are out of
#984's scope except where a production handler composes env inside
`bootstrap()`-driven fixtures — the hostile-env run (§5 P1) is the
oracle, this table is the starting map, not the gate.

### 0.2 #1013 — device-identity home stays ambient

`crates/taskboard-app/src/bootstrap.rs`:
- `resolve_device_identity` (~:147) reads `dirs::data_dir()?.join("taskboard")`
  then delegates to the already-injected `resolve_device_identity_at(data_dir, clock)`.
- `files_sync_spawn` (~:194) **re-derives** `dirs::data_dir()` (~:215)
  for `device.json` instead of receiving it.
- Every in-process `bootstrap()` caller therefore mints/reads the real
  platform `device.json`. Sweep of `bootstrap(` call sites (tests):
  `cli/cli_breaks.rs`, `cli/cli_journal.rs` (4), `cli/cli_reminders.rs` (3),
  `cli/cli_repeat.rs`, `cli/cli_timers.rs`, `cli/cli_undo.rs`,
  `cli/cli_track.rs` (4), `cli/cli_verbs.rs` (2), `cli/cli_standalone.rs` (2),
  `core/bootstrap.rs` (13), `core/lockfile.rs` (8), `core/repl.rs` (6),
  `services/ipc.rs` (5), `services/ui_live_settings.rs`,
  `services/ui_settings.rs` (`booted_workspace`, ~:68 — named in the
  issue), `services/web_dashboard.rs`, `it_docker.rs` (7) — plus the
  src-side test modules (`bootstrap.rs` tests ×7, `ui_requests.rs`
  `test_workspace_with_app`, `test_support::standalone_boot_fixture`).
  The real binary's `run()` call site stays ambient (the only intended
  consumer of the platform default).

### 0.3 #982 — `test_workspace_with_app` drops its tempdir under the live app

`crates/taskboard-app/src/ui_requests.rs` tests module (~:2007): the
local `tempfile::TempDir` backing `db_path` drops at function end
while the returned `(ConnectionWorkspace, App)` still reference paths
inside it. 23 `test_workspace(` call sites + the `port_workspace`
harness (`:3686`, which does return the dir) + the direct
`test_workspace_with_app` call site (`:3464`). Green today only
because the engine holds the open sqlite handle; the disk contract is
wrong by construction and diverges from
`test_support::StandaloneFixture` (which owns its guard).

### 0.4 #981 — duplicated `RefusingStore` doubles

- Shared definition: `src/test_support.rs` `RefusingStore` (derives
  `Debug`, `Err(NotFound)`/`Err(Unsupported)` semantics).
- Local double #1: `src/bootstrap.rs` tests module (~:2599), 6 `Arc::new(RefusingStore)`
  call sites.
- Local double #2: `src/ui_requests.rs` tests module (~:2041), manual
  `Debug` impl (needed because it lands in `ConnectionWorkspace.store`),
  2 call sites.
- `ConnectionWorkspace`'s `store` field requires `Debug` — the shared
  type already satisfies it via `#[derive(Debug)]`.

## 1. Decisions (every open question resolved)

**D-1 (#984) — env-lock application shape: shared helper import,
per-test acquisitions, no per-binary auto-fixture.**
`tests/common/mod.rs` already owns the landed C6/#865 machinery
(`env_lock`, `taskboard_env_lock` = save-all-`TASKBOARD_*`-then-clear
under one mutex, `xdg_lock` = sandboxed config home). The C6/#865
option-2 decision (snapshot-all / restore-all, `OsString`-exact,
guard-as-last-field drop order) is normative and is **not** re-litigated.
Every composing call site's enclosing test acquires
`let _env = common::taskboard_env_lock();` (or `xdg_lock()` where the
site resolves a default path — `AppConfig::load(None)`,
`config::path(None)`) at the top of the test body. Rationale:
- a binary-level fixture cannot work — grouped roots run members from
  many modules in one process; per-test acquisition is the only
  granularity that serializes correctly under nextest's per-process
  parallelism;
- the helper is already imported per root (`mod common`), so this is
  a one-line change per test, not new machinery;
- composition sites that pass an explicit file path still compose the
  figment Env provider, so explicit-path sites are **not** exempt —
  this is exactly the ui_settings-shaped miss (#984's finding).
No new file-layer oracle is added in this chunk: the #897
`extract_file_only` pattern already exists in `cli_connect.rs`; reuse
it where a post-write validity assertion currently reads through
`AppConfig::load` under hostile ambients, but do not propagate it as a
general rule.

**D-2 (#1013) — identity-home injection path: a `BootIdentity`
enum parameter on a new `bootstrap_in`; `bootstrap()` stays ambient.**
- New enum in `bootstrap.rs`:
  `pub enum BootIdentity { Platform, At(std::path::PathBuf) }` —
  `Platform` = today's `dirs::data_dir()?.join("taskboard")` (the real
  binary's only path); `At(dir)` = the suite's data dir (the
  `resolve_device_identity_at` seam already accepts it internally).
- `pub async fn bootstrap_in(config, store, sink, identity: BootIdentity)
  -> ...` carries the resolved data dir through
  `resolve_device_identity` → the spawned `files_sync_spawn`, which
  **stops re-deriving** `dirs::data_dir()` and takes the path
  (kill the ~:215 re-derivation; `BootError::NoDataDir` fires only on
  `Platform` + unavailable `data_dir()` — typed outcome preserved).
- `bootstrap(config, store, sink)` becomes a one-line delegate to
  `bootstrap_in(.., BootIdentity::Platform)` — the ~60 existing test
  call sites keep compiling; the binary call site is untouched.
- Test consumers flip per-fixture, not per-test: each suite's single
  boot fixture (e.g. `ui_settings::booted_workspace`, the `cli/*`
  helpers that build `AppConfig` then boot, `core/bootstrap.rs`'s
  harness, `test_support::standalone_boot_fixture`,
  `ui_requests::test_workspace_with_app`) passes
  `BootIdentity::At(dir.path().to_owned())` reusing the tempdir the
  fixture already owns (the db dir; one tempdir, one owner, per D-4).
  Default remains ambient only where no fixture tempdir exists
  (`it_docker.rs` boots subprocess-adjacent suites — flip them too;
  every boot gets a tempdir identity home; there is no legitimate
  in-process platform-home boot left in the suite).
- Acceptance grep: after the flip, `grep -rn 'dirs::data_dir'
  crates/taskboard-app/src/bootstrap.rs` hits exactly the `Platform`
  arm(s); `grep -rn 'bootstrap(' crates/taskboard-app/tests` hits only
  fixture-internal call sites that pass `BootIdentity::At` (directly
  or via the fixture).

**D-3 (#1013) — no derive crates, plain enum.** `BootIdentity` is a
hand-rolled enum-with-data (A1 policy); no newtype ceremony beyond it;
documented leniency: none needed — both arms are total.

**D-4 (#982) — the fixture owns the tempdir: return a
`WorkspaceFixture` struct; the guard outlives every borrower.**
- `test_workspace_with_app(path)` becomes
  `workspace_fixture(path) -> WorkspaceFixture` where
  `struct WorkspaceFixture { _dir: tempfile::TempDir, workspace:
  ConnectionWorkspace, app: crate::bootstrap::App }` — plain fields,
  the `StandaloneFixture` shape (no `Deref`, no cleverness). Drop
  order guarantee: fields drop in declaration order, `_dir` **first**
  would delete under live handles — so `_dir` is declared **last**:
  workspace and app drop (engine actors joined by `App`'s teardown)
  before the directory is deleted. The pinned contract: **the app's
  shutdown precedes the tempdir's drop**, testable by an ordering
  assertion (a `Drop`-observed flag or simply the struct-field-order
  doc + one test that drops `WorkspaceFixture` and asserts the db path
  is deleted only after `App`'s join — the cheap form: assert the
  struct layout via a test that moves out `app`, drops it, and only
  then observes the dir still present; final form per stage-2
  judgment, typed outcome, no text assertions).
- `test_workspace(path)` (the 23-site convenience) becomes a thin
  accessor that **keeps** the fixture alive for the test's duration —
  decision: delete `test_workspace` and update its call sites to
  `let fixture = workspace_fixture(path).await;` + `&fixture.workspace`,
  because "the core deliberately leaks for the test's lifetime" is the
  bug #982 names, not a feature to preserve. Mechanical, ~23 sites.
- `port_workspace` folds onto the same struct (its separately-returned
  seeded `dir` and the fixture's db dir unify: the seeded config and
  the db share one tempdir — matches `standalone_boot_fixture`).
- The injected `device_json` tempdir inside the current
  `test_workspace_with_app` (`tempdir().keep()`, the #751 ceremony
  shape) is replaced by D-2's `BootIdentity::At` on the *same* fixture
  tempdir — the `keep()`-and-leak disappears with the refactor.

**D-5 (#981) — dedup shape: one `test_support::RefusingStore`, plus a
convenience constructor.**
- Delete both local doubles; import
  `crate::test_support::RefusingStore`. The shared type already
  derives `Debug` (satisfies `ConnectionWorkspace`'s bound) and its
  semantics (`NotFound` on load, `Unsupported` on store/delete) are
  the superset both locals implement — verify the bootstrap.rs local's
  arm-for-arm equality during implementation; if the local differs in
  any arm, the shared definition wins and the differing test's
  expectation is a finding to raise, never a silent semantic merge.
- Add `pub(crate) fn refusing_store() -> std::sync::Arc<dyn
  crate::secrets::CredentialStore>` to `test_support.rs` to kill the
  six `Arc::new(RefusingStore)` repetitions at the call sites.
- Out of scope: `common::FakeCredentialStore` and
  `forbidden_store()` (tests/common) are *different* fakes (stateful /
  "would fail any touch" with different intent docs) — not folded.

## 2. Implementation phases (per-issue commits, each standalone)

1. **P0 — #981 first** (smallest, unblocks nothing but shrinks the
   diff surface P2/P3 touch): fold the doubles, `refusing_store()`.
2. **P1 — #1013**: `BootIdentity` + `bootstrap_in` +
   `files_sync_spawn` de-re-derivation; flip every suite boot fixture
   to `At`; `standalone_boot_fixture` gains the parameter.
3. **P2 — #982**: `WorkspaceFixture` struct + the 23-site mechanical
   update in `ui_requests.rs` (builds on P1's fixture change there).
4. **P3 — #984**: the env-lock acquisitions per §0.1's table, then
   the hostile-env audit run (§5) to catch anything the static sweep
   missed; add locks for those too.
5. **P4 — gate + proofs** (§5), commit hygiene pass.

## 3. Explicit non-goals

- No production behavior change beyond the `bootstrap_in` seam (the
  real binary's path is byte-for-byte identical: `Platform` arm).
- No new mocking/derive machinery; no `unsafe` beyond the landed
  edition-2024 env patterns already in `tests/common/mod.rs`
  (integration-test crate only; `test_support.rs` stays
  `#![forbid(unsafe_code)]`).
- No folding of `FakeCredentialStore`/`forbidden_store`, no journey
  harness work (B4a's lane), no `it_cli`/`it_docker`/`mcp_smoke`
  restructuring beyond the named seams.
- No text assertions introduced anywhere; typed outcomes only
  (`matches!` on errors, structural facts).

## 4. Risks & traps

- **R-1**: `bootstrap_in`'s signature ripple — contained by the
  delegate (D-2); if a call site cannot use a fixture tempdir (none
  known), it stays on `bootstrap()`/`Platform` and is recorded as a
  named gap, not silently left.
- **R-2**: the `WorkspaceFixture` field-order guarantee is silent —
  the ordering test (D-4) is the pin; a future field reorder must
  re-run it. Comment at the struct declaring the contract.
- **R-3**: flipping all boots to `At` may surface latent tests that
  *depend* on the platform device.json pre-existing (none expected;
  if one appears, it is seeded explicitly in the fixture, never via
  the platform home).
- **R-4**: env-lock acquisitions can deadlock if a test holds
  `taskboard_env_lock` and a callee also acquires — the helper is
  non-reentrant (`Mutex`): acquisition is test-body-top only, never
  inside helpers the test calls while holding it. Audit each added
  acquisition's call path (the existing `cli_editing.rs:547` /
  `core/config.rs:137` sites are the pattern to copy).
- **R-5**: hostile-env run can fail tests that legitimately assert
  ambient-env behavior (secrets env-store tests) — those already hold
  locks per §0.1; any newly failing test is triaged per AGENTS §5
  ("a new test failing can be a bug") before touching it.
- **R-6**: parallel-agent gate contention — run the gate via
  `scripts/run-quality-gates.sh cargo nextest run --workspace`
  (background + poll; commit before waiting; defer per the #879/#894
  rule if the lock is held).

## 5. Proof / acceptance

1. **Hostile-env pass** (#984's oracle): run the app test set with
   `TASKBOARD_CONFIG=/nonexistent/x.toml` and
   `TASKBOARD_SYNC__POLL_INTERVAL_SECS=abc` planted in the invoking
   shell — all app binaries green, no-fail-fast.
2. **No-real-home-write grep** (#1013): `device.json` writes land only
   under fixture tempdirs — grep-audit per D-2's acceptance grep.
3. **Lifetime ordering** (#982): the `WorkspaceFixture` ordering pin
   (D-4) green.
4. **One-definition** (#981): `grep -rn 'struct RefusingStore'
   crates/taskboard-app/src` → exactly one hit (`test_support.rs`).
5. **Gate**: pre-commit hooks +
   `scripts/run-quality-gates.sh cargo nextest run --workspace`.

## 6. Estimated diff size

~700–900 lines across ~30 files: `bootstrap.rs` (~+60/-20 seam),
`test_support.rs` (~+15), `ui_requests.rs` (~+40/-80 mechanical),
~20 test files with 1–5-line fixture/lock edits each. Mostly
mechanical; no new crates, no new dependencies.

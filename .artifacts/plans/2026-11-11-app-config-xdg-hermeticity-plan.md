# [Plan] App config XDG hermeticity — the #726 chunk

- **Date**: 2026-11-11
- **Kind**: TDD-first implementation plan (stage-1 output of the
  three-stage protocol; execution model per
  `.artifacts/plans/2026-11-11-ui-r6-dispatch-plan.md` §1).
- **Origin**: GitHub issue #726 (verified root cause + repro —
  normative), AGENTS §5/§8/§10.
- **Branch**: `fix/app-config-xdg-hermeticity` (worktree
  `../taskboard-WT-fix-app-config-xdg-hermeticity`, off `origin/main` @
  ca961b3d).
- **Scope**: make the `taskboard-app` config test module hermetic
  against a live `$XDG_CONFIG_HOME/taskboard/taskboard.toml`; plant a
  deterministic regression pin for the XDG read-path precedence. No
  production-code changes.

## 1. Root cause (from #726, confirmed against the tree)

`AppConfig::load(None)` resolves the config path as
explicit `--config` → `TASKBOARD_CONFIG` → `xdg_if_present(dirs::config_dir())`
(`src/config.rs:1219-1233`, the XDG candidate at
`src/config.rs:952-963`). When the resolution lands on the XDG
candidate, the *live* file joins the figment chain
(defaults → toml → `TASKBOARD_*` env), and `validate()` then refuses
the mixed shape: an env-injected legacy `[nextcloud]` identity beside
the live file's `[[sync.connectors]]` is
`ConfigError::NextcloudTableWithConnectors`
(`src/config.rs` `NextcloudTableWithConnectors` variant).

`c1_defaults_parse_with_no_file` (`tests/config.rs:35-63`) sandboxes
the CWD and injects `TASKBOARD_NEXTCLOUD__{SERVER_URL,USERNAME}` but
never pins `XDG_CONFIG_HOME`, so on a machine with a live connected
config it fails exactly this way. CI never sees it (no live config).
Exists on `main` @ ca961b3d.

**Merge-order verdict (decision c, recorded)**: the env provider wins
per key over the file, so the env-injected `[nextcloud]` identity is
overlaid onto a tree that already carries `[[sync.connectors]]` —
validation **refuses** with `NextcloudTableWithConnectors`. The
documented precedence ("the file wins" is *not* what happens when env
identity is present) is: mixed shape ⇒ typed refusal. The pin asserts
that.

## 2. Caller survey (decision b — the blast radius)

Every `AppConfig::load` / resolution call site in the crate's
integration suites, and whether it can touch the real XDG config home:

| Site | Call | Touches real XDG? |
|---|---|---|
| `tests/config.rs` `c1_defaults_parse_with_no_file` | `load(None)` | **YES — the #726 failure** |
| `tests/config.rs` `c15_ui_scale_env_override` | `load(None)` | **YES** (same hole, hasn't bitten yet: a live config only collides if it would poison `ui.scale`'s load — any live file decodes and its content merges) |
| `tests/config.rs` `c16_fresh_machine_diverges_read_none_write_xdg`, `c16_existing_cwd_install_is_the_write_target` | `config::path(None)` | No — these already pin `XDG_CONFIG_HOME` via `EnvOverride` (the in-file precedent for the fix) |
| `tests/config.rs` — all remaining C-tests (`c2`, `c4`–`c6`, `c10` env-provider half, `c13`, both `config print` tests, ipc/focus tests) | `load(Some(file))` / `print(Some(file))` | No — `config_path` returns the explicit path before consulting XDG |
| `tests/cli_login.rs:229` | `load(None)` | No — its fixture pins `XDG_CONFIG_HOME` to a tempdir (lines 100-102, 152-154, 207-209), the planted file is the resolution |
| `tests/cli_connect.rs` (10 sites), `tests/ipc.rs` (2), `tests/repl.rs` (13) | `load(Some(config_path))` | No — explicit paths |
| `tests/secrets.rs`, `ui_live_settings.rs`, `bootstrap.rs`, `dispatch_routing.rs`, `it_docker.rs` | `env_lock()` but no default-path resolution | No |

So the sandbox has exactly two in-module victims (`c1`, `c15`), and
every caller outside `tests/config.rs` is already hermetic.

## 3. Decisions

### a. The sandbox seam — a dedicated `xdg_lock()`, not a widened `env_lock()`

`env_lock()` (`tests/common/mod.rs:103`) is shared by seven test
binaries (config, secrets, ui_live_settings, cli_login, bootstrap,
repl, it_docker, dispatch_routing). Pinning `XDG_CONFIG_HOME` inside
`env_lock()` would silently change the environment of the
docker/live-gated suites (`it_docker`'s `live_config` fixtures) and of
`cli_login`, which deliberately layers its *own* XDG pin on top — a
wider behavioural change than the bug justifies, and invisible at the
call site.

**Decision**: add `pub fn xdg_lock() -> XdgLock` to
`tests/common/mod.rs`: acquires `env_lock()` (env + cwd remain the
process-global resources it already documents), then sets
`XDG_CONFIG_HOME` to a fresh `tempfile::TempDir` via `EnvOverride`,
with the guard owning the `TempDir` so the directory outlives the
critical section. Per-test attributes are rejected: they duplicate the
RAII the helper already owns and invite forgetting one test. Scope:
`tests/config.rs` only — `c1` and `c15` switch from `env_lock()` to
`xdg_lock()`; the two `c16` XDG-pinning tests may adopt the helper as a
cleanup *only* if their assertions are unchanged (they pin the same
variable; the helper's tempdir replaces their manual one — keep their
`chdir` sequencing intact). Other files stay untouched (already
hermetic per §2).

### b. Blast radius — §2's table is the record

Only `c1` and `c15` get the sandbox; everything else is already
hermetic (explicit paths short-circuit `config_path`; `cli_login`
self-pins).

### c. The regression pin — a planted connectors-carrying XDG file

New test in `tests/config.rs` (C-series style, after `c16`):

`the_xdg_read_path_refuses_env_identity_beside_connectors` — under
`xdg_lock()` (its tempdir is the `XDG_CONFIG_HOME`), plant
`<xdg>/taskboard/taskboard.toml` with a minimal valid
`[[sync.connectors]]` entry (kebab-case name + server_url + username —
copy the shape an existing connectors fixture in
`src/config.rs` unit tests uses), inject
`TASKBOARD_NEXTCLOUD__{SERVER_URL,USERNAME}` via `EnvOverride`, call
`AppConfig::load(None)`, and assert
`matches!(err, ConfigError::NextcloudTableWithConnectors)` — the
typed refusal per §1's merge-order verdict (env identity overlays the
connectors-carrying tree; validation rejects the mixed shape). This
also pins the resolution side deterministically: with the live config
present the *same* code path resolves the XDG candidate; the sandbox
makes the content ours.

**TDD order and the red/green story (recorded honestly)**:

1. *Red*: with `XDG_CONFIG_HOME` pointed at a fixture dir containing
   the crafted connectors config (§4), run the **unfixed**
   `c1_defaults_parse_with_no_file` — it fails with
   `NextcloudTableWithConnectors` (this is #726's repro; the failing
   test is c1 itself, per AGENTS §5 "a new test failing can be a
   bug" — here the bug is the test's environment, not the production
   code: the merge-order refusal is correct, documented behaviour).
2. *Green*: land `xdg_lock()` and switch `c1` (then `c15`) to it — c1
   passes under the same fixture XDG.
3. The pin test is written against the helper from the start (it is
   green immediately — it is a determinism pin of the documented
   refusal, not a red/green probe; its value is that on a machine
   with a live config it now exercises the *real* XDG resolution path
   with *controlled* content, which no existing test does).

No production code changes: the refusal itself is correct and
documented (`ConnectorShapeMixed`/`NextcloudTableWithConnectors`
variant docs); the defect was test hermeticity.

### d. Acceptance evidence for the reviewer

Recorded in this plan's §5 after the local runs (command lines +
outcomes, both unfixed and fixed). The stage-3 reviewer re-runs the
§5.1 repro command to confirm the fix.

## 4. Test plan (TDD-first)

- **Fixture**: a helper in the test (or the snippet below)
  writing the crafted config:

  ```toml
  [[sync.connectors]]
  name = 'fixture-cloud'
  server_url = 'https://fixture.example.com'
  username = 'alice'
  credential_store = 'env'
  ```

  (exact key set taken from the connectors decode struct in
  `src/config.rs`; stage 2 adjusts to whatever the minimal valid entry
  is — the shape, not the exact keys, is the contract).

- **Commands** (fixture XDG at `$TMP/fixture-xdg-config`, containing
  `taskboard/taskboard.toml` as above):

  ```sh
  # unfixed, red:
  XDG_CONFIG_HOME=$TMP/fixture-xdg-config \
    cargo nextest run -p taskboard-app -E 'test(c1_defaults_parse_with_no_file)'
  # → FAIL: defaults load: NextcloudTableWithConnectors  (= #726)

  # fixed, green:
  XDG_CONFIG_HOME=$TMP/fixture-xdg-config \
    cargo nextest run -p taskboard-app -E 'test(config)'   # whole module
  # → PASS (c1, c15 sandboxed; the pin green under any XDG)
  ```

- **Gates**: full AGENTS §10 suite via
  `scripts/run-quality-gates.sh` in the worktree.

## 5. Acceptance evidence

*(filled in by stage 2 during implementation; stage 1 records the
expected shape)*

- §4's red run: outcome recorded verbatim (expected:
  `NextcloudTableWithConnectors` on unfixed code with the fixture XDG).
- §4's green run: whole `config` module passing under the same
  fixture XDG, plus the full gate suite green.

## 6. Non-goals

- No production-code change to `AppConfig::load`/`config_path` — the
  XDG read path and the mixed-shape refusal are correct.
- No widening of `env_lock()`'s semantics (§3a).
- No hermeticity work in other test files (none needed, §2).
- Multi-lane files sync (#723), config-format changes (dispatch chunk
  2's territory), and any `TASKBOARD_CONFIG`-vs-XDG precedence change.

## 7. Estimated diff

`tests/common/mod.rs` +~35 lines (`XdgLock` guard + `xdg_lock()`);
`tests/config.rs`: 2 one-line seam swaps + the ~35-line pin test +
(possibly) the 2 `c16` tests adopting the helper. **~90 lines, test-only.**

## 8. Stage-2 checklist

1. Reproduce §4's red run; record it in §5.
2. `xdg_lock()` in `tests/common/mod.rs`; switch `c1`, `c15`;
   green run per §4; adopt in the `c16` tests if assertions unchanged.
3. Add the pin test (§3c).
4. Full gates via `scripts/run-quality-gates.sh`.
5. Commits: standalone, `test(app): …` subjects; hand off at
   gates-green (stage 3 pushes/PRs).

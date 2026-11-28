# Plan: the fail-fast real-home guard for sandboxed invocations (#1554)

## Problem

`AppConfig::load(None)` — no `--config`, no `TASKBOARD_CONFIG` — falls
through to the XDG candidate (the user's live config on a dev machine)
and then fills the real default db path
(`dirs::data_dir()/taskboard/taskboard.db`), so any bare invocation of
the `taskboard` binary targets the operator's real board store. The
test suite has no guard against this; it happened for real on
2026-11-27 (#1519 chunk 2 stage 2: a worktree test run created a board
in the user's database).

## Guard shape (decided)

**Sentinel + stronger refusal, armed by presence, enforced at the one
decode point.**

- Sentinel env: `TASKBOARD_TEST_SANDBOX` (pub const
  `config::SANDBOX_SENTINEL_ENV`, matching the `TASKBOARD_*` prefix
  convention). **Presence** (any value) arms the guard — no value
  grammar to get wrong. Operators, kiosk appliances, and deploys never
  set it; the guard only ever *refuses*, so it cannot unlock anything
  in production even if set maliciously.
- New typed variant `ConfigError::SandboxedRealHomePath { path:
  PathBuf }`, kind discriminant `"sandboxed-real-home-path"` (extends
  the stable `ConfigError::kind()` vocabulary, so `--json` envelopes
  carry it).
- **Stronger invariant** (the issue's preferred form, adopted because
  it is simple): when the sentinel is armed, ANY consulted path — the
  resolved config file and the resolved db path — that lies under
  `dirs::home_dir()` is refused, not only the bare-fall-through case.
  Match is `Path::starts_with` (component-wise literal prefix;
  canonicalization deliberately not attempted — documented leniency).
- **Enforcement site: `AppConfig::extract_raw`** — the single
  decode-and-fill point consumed by both `AppConfig::load` (every
  verb/daemon boot) and `login` (which reads before writing). A
  refusal there fires before any store is opened or any config is
  written. Pure core `sandbox_real_home_refusal(sandboxed, home,
  paths) -> Option<ConfigError>` mirrors the #1511 pattern (pure
  truth-table core + thin env-reading consumer); the consumer reads
  `SANDBOX_SENTINEL_ENV` and `dirs::home_dir()`.
- No behaviour change without the sentinel: production, kiosk, docker
  tier, and CLI journeys are untouched (the sentinel is never set
  outside test helpers).

## Coverage

1. Unit tests (config.rs test module, injected `sandboxed`/`home`/
   paths — never the real env or real home):
   - pure-core truth table: unarmed → `None`; armed + no home →
     `None`; armed + path under home → typed
     `SandboxedRealHomePath` (asserted with `matches!`); path outside
     → `None`; the component-wise prefix boundary
     (`<home>/taskboard-evil` under home `<home>` does NOT match —
     `starts_with` is component-wise).
   - `kind()` stability: `"sandboxed-real-home-path"` joins
     `config_error_kinds_are_stable_discriminants`.
2. Integration tests (new target `tests/cli_sandbox_guard.rs`, real
   binary, sandboxed `HOME`/`XDG_*` under a tempdir — never the real
   home):
   - **Refusal**: sentinel armed + scrubbed `TASKBOARD_*` (except the
     sentinel) + sandboxed XDG tree whose config names a db under the
     *sandbox* home's data dir + no `--config`/`TASKBOARD_CONFIG` →
     exit code non-zero, stderr parses as JSON with
     `error.kind == "sandboxed-real-home-path"` (stable typed
     discriminant, never message text), and the sandbox db file was
     **not created** (structural proof nothing opened a store).
   - **Positive**: sentinel armed + explicit isolated config in a
     tempdir → the verb exits 0 (the guard does not obstruct
     properly-sandboxed invocations).

## Helper audit (every binary-spawning site in the workspace)

| Site | Today | Change |
|---|---|---|
| `tests/common/mod.rs::binary_command` (:597) | scrubs `TASKBOARD_*`, plants nothing | plant sentinel after the scrub |
| `tests/common/mod.rs::spawn_daemon` (:665) | wraps `binary_command` | covered automatically |
| `tests/it_cli.rs::strip_taskboard_env` (:39) + direct `Command::new` wraps (:548, :597) | scrubs only | plant sentinel |
| `tests/cli/cli_tty.rs::strip_taskboard_env` (:336) + direct `Command::new` wraps (pty/`timeout` spawns) | scrubs only | plant sentinel |
| `tests/it_docker.rs` direct spawns (:697, :708, :751, :763, :780) | explicit `--config`, no sentinel | plant sentinel at each site |
| `tests/mcp_smoke.rs` (:48) | explicit `TASKBOARD_CONFIG`, no sentinel | plant sentinel |

A helper that spawns the binary without the sentinel is a finding;
none remain after this change.

## Docs

One sentence in `docs/testing_strategy.org`: binary-spawning helpers
plant `TASKBOARD_TEST_SANDBOX`, and the binary fail-fast refuses any
real-home path while it is armed (#1554). Helper doc comments state
the same constraint.

## Commits

1. `docs(plan): the fail-fast real-home guard for sandboxed invocations (#1554)` — this file.
2. `feat(app): refuse real-home config paths under the test-sandbox sentinel (#1554)` — const, error variant, pure core, extract_raw enforcement, unit tests, docs sentence.
3. `test(app): every binary-spawning helper plants the sandbox sentinel (#1554)` — helper changes + integration tests.

## Gate

Fast gate only (stage-2 discipline): pre-commit hooks +
`scripts/run-quality-gates.sh cargo nextest run --workspace`. The
coverage trio is the dispatcher's. Flock-deferral rule applies.

## Stage-3 REVIEW prompt (paste-ready, verbatim)

You are the STAGE 3 (REVIEW) agent for issue #1554
(`test/real-home-guard`, worktree
`/home/viktoria/src/taskboard-WT-test-real-home-guard`). Review the
landed commits (do not re-implement): the guard is the
`TASKBOARD_TEST_SANDBOX` sentinel + `ConfigError::SandboxedRealHomePath`
refusal in `AppConfig::extract_raw` (pure core
`sandbox_real_home_refusal`), kind `"sandboxed-real-home-path"`.

**Discipline (binding — docs/agent_dispatch.org)**: A subagent is
killed after ~600s without observable activity. **Long-command
discipline**: (1) *The coverage trio never runs inside a subagent*: a
stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(flock-safe; background + poll with useful work between polls if
needed); the dispatcher runs the full AGENTS §10 suite between stage
2's landing and stage 3's ship. *Flock-deferral rule: when the gate
lock is contended and no other useful work remains, never block —
commit all landed work, report "gate deferred: lock held by
<holder>", and end the stage; the dispatcher runs the gate.* (2)
*Stage 3's scope ends at PR-open*: push, open the PR, report — the
dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`,
teardown, and at-merge issue closures. (3) *Any command expected over
~5 minutes runs backgrounded* with short interleaved polls and useful
work between polls — commit all landed work BEFORE any wait. Large
scratch under /var/tmp, never /tmp. (4) *Every agent-launched cargo
invocation routes through `scripts/memory-gate.sh`* — nothing bare.
(5) *Stage prompts restate this discipline verbatim.* **Never commit
with `--no-verify` or any hook bypass** — if the memory gate's lock is
contended, wait in a backgrounded command or apply the flock-deferral
rule.

Review scope:
1. The guard cannot be bypassed: every resolution route that can reach
   a real-home path (bare `--config`-less boot, `TASKBOARD_CONFIG`,
   XDG candidate, `login` write path, db default fill) is refused when
   the sentinel is armed; verify `extract_raw` is the single point all
   of them cross (`AppConfig::load`, `login`).
2. Production is unaffected: without the sentinel nothing changes —
   confirm no production code path sets or depends on the sentinel and
   no test asserts on it outside the helper plant sites.
3. Every binary-spawning helper in the workspace plants the sentinel
   (audit against the plan's table plus a fresh
   `grep -rn "CARGO_BIN_EXE_taskboard\|Command::new" crates/taskboard-app/tests`).
4. AGENTS §5: typed outcomes only (the `--json` kind discriminant is a
   stable typed code, not message text), no sleeps beyond the
   existing polled-wait helpers, no real-home access anywhere in the
   new tests.

End state: fix findings on the branch, commit
`<type>(<module>): …`, run the fast gate (backgrounded, memory-gated),
push with `--force-with-lease` if needed, open the PR against `main`
titled per the plan, report the PR URL — do not watch CI, do not
teardown the worktree, do not run the coverage trio.

# Chunk D plan — config/test-harness hygiene

Round: `.artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md` §5
(normative) · Dispatch: `.artifacts/plans/2026-11-16-correctness-hygiene-dispatch-plan.md` §5
Branch: `test/config-harness-hygiene` · Covers: #728 #729 #730 #607 + #833
read-only rider · Scope guard: **no production-code changes; diff confined
to `crates/taskboard-app/tests/common/mod.rs`, `crates/taskboard-app/tests/config.rs`,
`crates/taskboard-app/tests/cli_login.rs` (cfg gates only), one new
`scripts/load-repro.sh`, and `.gitignore`-untouched tmp evidence.**

## 0. Decisions recorded (open questions resolved up front)

| Q | Decision |
|---|---|
| D1 — where does the cwd guard live? | `crates/taskboard-app/tests/common/mod.rs`, sibling of `EnvGuard`/`XdgLock`. No production cwd mutation exists (grep: `set_current_dir` appears only under `tests/`), so no backlog issue for the round-plan §5 contingency. |
| D2 — does the env lock replace `env_lock()` in config.rs? | Yes, everywhere in `tests/config.rs`: every `env_lock()` call site becomes `taskboard_env_lock()`. Uniformity beats per-test lane reasoning; cost is one snapshot per test. Other suites keep `env_lock()` this chunk. |
| D3 — does the env lock also fix the non-UTF-8 `EnvOverride::set` prior-value edge (#729 footnote)? | Only for the `TASKBOARD_*` lane (the new helper snapshots `OsString`s). `EnvOverride` itself is untouched — its keys are single, known-UTF-8-relevant pins; widening it is a separate concern. |
| D4 — #730 platform seam per `cfg!` | Linux: `XDG_CONFIG_HOME` (identical code path, byte-for-byte behavior preserved). macOS: additionally redirect `HOME` to the sandbox root — `dirs::config_dir()` on mac resolves `$HOME/Library/Application Support` and ignores `XDG_CONFIG_HOME`, so `HOME` is the only available env seam; both vars set while the env mutex is held. Windows: **no env seam exists** (`dirs` uses the Roaming AppData known-folder API); affected cases are `#[cfg(not(target_os = "windows"))]`-gated with a named-gap comment referencing #730. No registry/known-folder mutation in tests — out of proportion and process-global. |
| D5 — production config-dir seam (#730's alternative proposal)? | Rejected for this chunk: it is a production API change (violates the scope guard). The `HOME`-redirection seam covers macOS today; the production seam, if ever wanted, becomes a backlog issue only if stage-2 finds the test seam insufficient. |
| D6 — #607: the leaking scripts aren't checked in | Confirmed: `/tmp/rearm-*.sh` were ad-hoc recipes; nothing under `scripts/` carries `kill %N` or spinners. Decision: land the sanctioned pattern as a checked-in reusable runner `scripts/load-repro.sh` (bounded spinners, PID files, `trap cleanup EXIT INT TERM` killing recorded PIDs, no job-spec kills) so future repro recipes cannot repeat the leak; #607 closes citing this script plus its self-test evidence. |
| D7 — #833 finding posting | Stage-2 drafts the audit findings; stage-3 posts the comment and opens any `backlog:` issues (dispatch plan §6 assigns the #833 close to chunk D's ship step). Stage-2 reports findings in its handoff text either way. |
| D8 — panic-safety demonstration shape | Single deterministic test using `std::panic::catch_unwind` (with `AssertUnwindSafe` for the mutex-guarded fixture): chdir via the guard, panic inside the closure, assert the unwind was caught **and** `current_dir()` is back to the original. No cross-test ordering assumptions (nextest runs per-process; a `#[should_panic]` + sibling-asserts pair would be ordering-dependent under a merged harness). |

## 1. #728 — RAII cwd guard

**Files**: `crates/taskboard-app/tests/common/mod.rs`, `crates/taskboard-app/tests/config.rs`.

- New type in `common/mod.rs`:

  ```rust
  /// RAII cwd guard: chdirs now, restores the previous directory on
  /// drop — including on the unwind path, where the manual
  /// `set_current_dir(original)` in the tests this replaces was
  /// skipped (#728's leak: the process cwd stayed in a tempdir that
  /// `TempDir::drop` then deletes, contaminating every sibling test).
  pub fn cwd_lock(path: &Path) -> CwdGuard { ... }
  pub struct CwdGuard { original: PathBuf }
  impl Drop for CwdGuard { fn drop(&mut self) {
      // deletion-tolerant: `original` is the pre-test dir and always
      // exists; an error here is a panic, not a silent leak.
      std::env::set_current_dir(&self.original).expect("restore cwd");
  } }
  ```

  Constructor captures `current_dir()` first, then chdirs (fail loudly if
  either fails). `#[must_use]`. The guard does **not** take the env mutex
  itself — cwd is process-global like env, so callers already holding
  `taskboard_env_lock()`/`xdg_lock()` serialize naturally; a test that
  only chdirs holds nothing extra (documented on the type).
- Adopters in `tests/config.rs`: `c1_defaults_parse_with_no_file`,
  `c16_fresh_machine_diverges_read_none_write_xdg`,
  `c16_existing_cwd_install_is_the_write_target` — the three manual
  save/`set_current_dir`/restore sites (lines 28–38, 487–493, 506–513 on
  this base). The trailing manual restores are deleted; the guard binds
  at the top with the other fixtures.
- **Panic-safety demonstration (D8)**: new test
  `cwd_guard_restores_on_unwind` in `tests/config.rs` — capture original,
  inside `catch_unwind(AssertUnwindSafe(|| { let _g = cwd_lock(dir); panic!("deliberate"); }))`,
  assert the result is `Err`, then assert `current_dir() == original`.
  No `#[should_panic]`; no sleeps; typed structural assertion.

## 2. #729 — `TASKBOARD_*` env-lock helper

**Files**: `crates/taskboard-app/tests/common/mod.rs`, `tests/config.rs`.

- New helper in `common/mod.rs`:

  ```rust
  /// Saves every `TASKBOARD_*` variable (OsString-exact: non-UTF-8
  /// values round-trip), removes them all, and restores the full
  /// snapshot on drop. Correct under a merged harness: the whole
  /// mutation happens while the process env mutex is held (the saved
  /// `EnvGuard` field drops last).
  pub fn taskboard_env_lock() -> TaskboardEnvLock
  ```

  Snapshot uses `std::env::vars_os()` filtered by
  `key.as_encoded_bytes().starts_with(b"TASKBOARD_")` (same prefix check
  the existing child-spawn scrub at `common/mod.rs:508` uses). Drop
  restores each entry: `Some` ⇒ `set_var`, `None` ⇒ `remove_var`.
  **Save-then-clear-all, never clear-a-fixed-list** — the helper must
  restore variables it did not know about beforehand (dispatch review
  trap (b)).
- `tests/config.rs`: all `env_lock()` sites (≈20) become
  `taskboard_env_lock()` (D2); the now-redundant bare
  `remove_env("TASKBOARD_CONFIG")` lines (c15, c16, xdg-read-path) are
  deleted — the lock already cleared it.
- **In-test restore proof**: `taskboard_env_lock_restores_ambient_values` —
  plant hostile values (`TASKBOARD_CONFIG=/nonexistent/taskboard.toml`,
  `TASKBOARD_SYNC__POLL_INTERVAL_SECS=5`) at the top under the lock,
  take `taskboard_env_lock()`, assert both are gone (structural), drop
  the lock, assert both hostile values are back, then clear them via the
  existing `remove_env` and drop guard. Proves save-unknown-keys restore
  semantics deterministically, without relying on the operator's shell.
- **Hostile-env proof (acceptance, stage-2 + stage-3)**: run

  ```sh
  env TASKBOARD_CONFIG=/nonexistent/taskboard.toml \
      TASKBOARD_SYNC__POLL_INTERVAL_SECS=5 \
      TASKBOARD_NEXTCLOUD__SERVER_URL=https://hostile.invalid \
      scripts/run-quality-gates.sh cargo nextest run -p taskboard-app --config-suite
  ```

  (exact nextest filter at implementation time) — the whole config suite
  must pass with ambient hostile values; command + output recorded in
  the PR body. This is the round-plan acceptance: "the test proves the
  env lock".

## 3. #730 — per-platform config sandbox

**Files**: `crates/taskboard-app/tests/common/mod.rs`, `tests/config.rs`,
`tests/cli_login.rs` (cfg gates only), `tests/bootstrap.rs` if it shares
`xdg_lock` (verify at implementation; no behavior change either way).

- `xdg_lock()` gains a `cfg!`-armed body (Linux arm byte-identical in
  effect — D4):

  | Platform | Seam set by the lock | Cases affected |
  |---|---|---|
  | Linux | `XDG_CONFIG_HOME` = sandbox (as today) | none — identical code path |
  | macOS | `HOME` = sandbox root **and** `XDG_CONFIG_HOME` = sandbox (harmless, keeps one restoration path) | all `xdg_lock()` consumers |
  | Windows | seam not possible ⇒ compile-time gate | `c1`, c15, c16 both, xdg read-path test, `cli_login`'s three planted-file fixtures |

  Implementation shape: `XdgLock` stores a `Vec<EnvOverride>` so two
  overrides restore in reverse order (`HOME` first-set restores last);
  doc comment names the `dirs` behavior per platform and cites #730.
- Windows-gated cases get:

  ```rust
  #[cfg(not(target_os = "windows"))] // platform gap: dirs::config_dir()
  // on Windows reads the Roaming AppData known folder and honors no env
  // seam — no deterministic sandbox exists in-process (#730).
  ```

  **Named-gap comment, never deletion.** The gate is `cfg` on the test
  attribute (compiles out), not runtime skip, so `cargo nextest` on
  Windows simply doesn't list them.
- Structural check for review trap (c): grep the diff — no line inside
  the `cfg(target_os = "linux")` arm may differ from today's body except
  variable placement into the Vec.

## 4. #607 — PID-recording + trap cleanup script

**Files**: new `scripts/load-repro.sh` (D6).

- Shape:

  ```bash
  SPIN_DIR="$(mktemp -d /tmp/load-repro.XXXXXX)"
  cleanup() { for f in "$SPIN_DIR"/*.pid; do [ -f "$f" ] && kill "$(cat "$f")" 2>/dev/null; done; rm -rf "$SPIN_DIR"; }
  trap cleanup EXIT INT TERM
  spawn_spinner() { ( local n=0; while [ "$n" -lt "${1:-600}" ]; do :; n=$((n+1)); done ) & echo $! > "$SPIN_DIR/spinner-$.pid"; }
  ```

  Bounded loops (default deadline counter, per #607's "consider a bounded
  spinner"), PID recorded at spawn, killed only by recorded PID from the
  EXIT/INT/TERM trap — **no `kill %N` anywhere**, cleanup runs on every
  exit path (review trap (d)).
- **Self-test method**: `scripts/load-repro.sh --self-test` spawns N
  spinners, sleeps ~1s (script shell, not a Rust test — no test-law
  implications), triggers `exit`, then from the trap-before-rm context
  asserts every recorded PID fails `kill -0` (i.e. is dead), prints a
  typed PASS/FAIL line, and exits nonzero on any survivor. Stage-2 runs
  it and records the command + output on #607 as the closure evidence
  (round-plan acceptance: "self-test run recorded on the issue").
- The script is tool-only (no SPDX `.rs` implications); header comment
  cites #607 and states the pattern contract for future repro recipes.

## 5. #833 rider — read-only audit checklist

No code changes in this scope (dispatch trap (e): any helper edit in the
diff is a finding). Stage-2 executes:

1. Enumerate `adapter.refresh(` callers in
   `crates/taskboard-ui-slint/src/lib.rs` + `tests/` (non-`refresh_forced`).
2. For each, classify: does any input to the subsequent render move
   **outside the state `Arc`** (set_focused / activate_focused /
   select_board / view swap / clock-derived text) before the refresh?
   ⇒ guarded-refresh staleness candidate.
3. Spot-verify the chunk-1 fix held: `lib.rs:670` and `lib.rs:2973` use
   `refresh_forced`; the kiosk recur-row assertion reads a post-forced
   model.
4. Check the helper contract is discoverable: `adapter.rs` `refresh` /
   `refresh_forced` doc comments state the "moved outside the state Arc ⇒
   forced" rule; note whether `UiHandle::refresh` (public helper) repeats
   the trap (#833's rename/doc-warning suggestion).
5. Journey steps: sweep the `lib.rs` journey/test helpers for the
   pattern-regex "mutate-then-refresh".
6. Output: findings → draft comment on #833 (posted at stage-3, D7);
   fixes warranted ⇒ `backlog:` issues under the standing authorization;
   **no helper rewrites in this branch**.

## 6. Implementation order & commits (stage-2)

1. `test(app): the RAII cwd guard for the config tests` — §1 (+ demo test).
2. `test(app): the TASKBOARD_* env-lock helper` — §2 (+ restore-proof test).
3. `test(app): the per-platform config sandbox` — §3.
4. `chore(scripts): the PID-trap load-repro runner` — §4 (+ self-test evidence on #607).
5. #833 audit executed read-only; findings in handoff.

## 7. Gate

Pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest run
--workspace` (flock-safe, background + poll). Coverage trio does **not**
run here. Hostile-env run of §2 is an extra nextest invocation through
the same wrapper.

## 8. Risks

- **macOS `HOME` redirection breadth**: overriding `HOME` affects more
  than `dirs` (e.g. `tempfile` uses `TMPDIR`, unaffected; but any
  `~`-expanding code in the process changes). Confined to test processes
  holding the env mutex; accepted, documented on `XdgLock`.
- **`catch_unwind` + mutex poison**: the demo panics while holding the
  env mutex if fixtures share it — the existing
  `unwrap_or_else(PoisonError::into_inner)` already tolerates poison;
  the demo avoids taking the mutex at all (cwd-only).
- **Windows coverage loss** from the gates is pre-existing
  non-execution (no Windows test CI today — #730 notes this); the gate
  makes it explicit instead of silently wrong.
- **Non-UTF-8 env `set_var` restore** uses `std::os` setters accepting
  `OsString` — edition-2024 unsafe block discipline as the existing
  `set_env`/`remove_env` (SAFETY comment mirrors them).

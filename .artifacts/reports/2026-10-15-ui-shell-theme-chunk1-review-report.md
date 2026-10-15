# [Report] GUI Phase 7 Chunk 1 (`feat/ui-shell-theme`) — Stage-3 Review

- **Date**: 2026-10-15
- **Scope**: chunk 1 of the GUI Phase 7 implementation dispatch
  (`.artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md`
  §1/§2) against the accepted design plan
  `.artifacts/plans/2026-10-14-gui-phase7-design-plan.md`
  (§1–§3, §6.1–6.2, §9.1, §11/§16) and the chunk plan
  `2026-10-14-ui-shell-theme-plan.md`.
- **Deliverable**: PR #77 — `feat/ui-shell-theme` → `main`
  (https://github.com/D4id4los/taskboard/pull/77).
- **Verdict**: **APPROVED** — all required checks green, branch handed
  off for user merge.

## 1. Functional review findings

- Slint pinned to caret `1.18.1` with default features (design Q10);
  the CI consequence (system `libfontconfig1-dev` build dependency) is
  recorded and wired into every apt-install step of `ci.yml` and the
  linux matrix of `release.yml`.
- Token lint gate `scripts/check_slint_tokens.py` implements all three
  §3.4 rules (literal colours, literal font attributes, literal `px`
  spacing outside `ui/theme.slint`) with comments stripped before
  matching and **no suppression escape hatch** (no pragma, no per-file
  waivers); wired into CI and the repo pre-commit hook.
- Contrast check (`token_tests.rs`) enforces §9.1 minimums over a
  complete foreground × surface pair table; a new theme token pair
  without an assertion fails the completeness rule (pair table is
  checked against the theme's token set, not hand-maintained only).
- Out-of-range `[ui] scale` is a typed `ConfigError` (figment layer),
  unit-tested in `crates/taskboard-app/tests/config.rs`.
- `SLINT_SCALE_FACTOR` is read before Slint init
  (`crates/taskboard-app/src/ui.rs`); `set_const_scale_factor`
  deliberately unused per §6.2 (D13) — the winit backend applies the
  env at window creation.
- The UI→engine forwarder is a bootstrap-owned spawn adapting the UI
  mpsc onto `EngineHandle::dispatch`, logs dropped commands after
  shutdown, and is covered by a hermetic fake-handle test.
- The theme module `ui/theme.slint` is the only place colours, font
  attributes, and base dimension arithmetic appear; text roles are
  exactly the closed §3.2 set.
- Fonts (Inter Regular/SemiBold/Bold, JetBrains Mono Regular) are
  bundled and registered at startup (`src/fonts.rs`) with license
  manifest `assets/fonts/LICENSES.md`.
- `taskboard-ui-slint` gained no IO dependency and no business logic;
  it stays a presentation-only consumer of `ArcSwap<AppState>`.

## 2. Code-quality mandate (§1.3)

- No dead code or speculative generality found: no runtime
  theme-switching machinery, no second-window plumbing (POST-1.0).
- SRP holds: UI crate presents only; `taskboard-app` resolves config
  and wires actors; the forwarder only forwards.
- Review fixes were folded into the affected commits by rebase
  (single-timestamp rewrite, 2026-10-15 01:45) — no duplicate-subject
  fixup commits on the branch.

## 3. Issues found & fixed during review

1. **Missing `libfontconfig1-dev` in CI/release apt deps** — the slint
   build failed in every GitHub Actions job (this was the red CI state
   on the first push). Fixed in all `ci.yml` jobs and the linux
   `release.yml` job.
2. **`token_tests.rs` pair-table readability** — the contrast pair
   table was verbose and repetitive; compressed with shorthand
   constants (`BACKGROUND`, `SURFACE`, `SURFACE_RAISED`) at one line
   per pair (−123/+43 lines, behaviour unchanged).

## 4. Gate verification

Local re-run on the final amended history (2026-10-15):

- `cargo fmt --all -- --check` — clean
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo nextest run --workspace` — 783 passed, 26 skipped, 0 failed
- `cargo test --doc --workspace` — ok
- `cargo deny check` — advisories/bans/licenses/sources ok
- `cargo llvm-cov` not run locally (coverage-instrumented artefacts
  were the cause of the prior agent's disk exhaustion); delegated to
  the CI Coverage job, which **passes**.

CI on the pushed head (run 38657580695): all 8 checks **pass** —
Code Quality & Linting, MSRV Check, Performance Benchmarking, Security
& License Audit, SonarCloud Analysis, SonarCloud Code Analysis, Test
Execution, Code Coverage.

## 5. Hand-off

- Branch: `feat/ui-shell-theme` (head `2b1cb79`), force-pushed with
  `--force-with-lease`.
- PR: https://github.com/D4id4los/taskboard/pull/77 — green; awaiting
  **user merge** on GitHub (never merged by the agent, `main` untouched).

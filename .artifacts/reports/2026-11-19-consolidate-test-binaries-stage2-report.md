// SPDX-License-Identifier: MIT OR Apache-2.0

# R4 stage-2 report — consolidate the test binaries (round 3)

Branch: `refactor/consolidate-test-binaries` (7 commits over
`origin/main` @ `fb447e4b`). Plan:
`.artifacts/plans/2026-11-19-consolidate-test-binaries-plan.md`.
Stage-2 executed across two agent sessions (the first was quota-cut
mid-task; this report covers the resumed completion).

## 1. Landed shape

- ui-slint: 31 probe/journey binaries → 6 targets (commit `3f0a7f45`).
- app: 34 test binaries → 8 targets (commit `9b162f38`).
- Workspace: **98 → 47 test binaries**, matching plan §3.
- journeys.rs, it_cli.rs, it_docker.rs, mcp_smoke.rs, pin_guard.rs:
  untouched (plan §3's keep-separate list).

## 2. The shared-module mechanics flaw and its fix

### The flaw (found by the first stage-2 session)

The first grouping pass moved member files verbatim, keeping each
member's standalone preamble — `#[path = "../support/mod.rs"]
mod support;` (ui-slint) / `#[path = "../common/mod.rs"] mod common;`
(app). Under nextest's one-process-per-test model this is harmless;
under plain `cargo test` a merged binary then instantiates **several
copies** of the harness module in one process, and with them the
process-global machinery:

- ui-slint `driver.rs` runs the whole slint platform off two
  `OnceLock`s (`SENDER`, `OPTIONS`) whose guard asserts one
  `DriverOptions` per process — per-member copies multiply the
  platform init and the agreement assertion fires wholesale
  (#1016's finding class).
- app: `clippy::duplicate_mod` (deny under `--all-targets -D
  warnings`) rejects every merged app target at the pre-commit gate.

### The fix (commits on this branch)

One shared module resolution per merged root:

1. Each merged root declares the module exactly once
   (`#[path = "support/mod.rs"] mod support;` /
   `#[path = "common/mod.rs"] mod common;`); members consume it via
   `use crate::support;` / `use crate::common;`, and their bare
   `use support::…` paths resolve through edition-2024 uniform paths
   against that single root module.
2. `driver.rs`'s harness self-tests now read a root-declared
   `pub(crate) const HEADLESS_RENDERER: bool` instead of hardcoding
   `true`, so they agree with whichever binary hosts the shared
   process (journeys.rs keeps its own copy, all-`true`).
3. `structural.rs` resolves the bench fixture module
   (`../benches/support/mod.rs`) once at its root; the member
   consumes it via `crate::support`.

### The strict mode partition (plan §6 mitigation enforced)

The plan §3.1 map placed `virtualisation.rs` in `probe_composition`
on the premise "already uses the headless:false option". That premise
was wrong: the module has 14 option sites, 13 `false` **and one
`true`** — the deep-board golden capture at the i16 crash scale
(`a_board_beyond_the_i16_pixel_ceiling_renders`) needs the software
renderer for `take_snapshot`. No grouping containing it can pass
plain `cargo test` wholesale. Per the plan's own mitigation ("map
partitions strictly on `headless_renderer`"), the module moved to the
renderer-on group: `virtualisation.rs` now lives in `probe_pixel.rs`
with its option sites standardized on `headless_renderer: true`
(test function names unchanged, only the harness mode). Documented
in both root files' doc comments.

### Why the extracted-test alternative was rejected

Moving only the one `true` test out of `virtualisation.rs` would have
changed its nextest string from
`virtualisation::a_board_beyond…` to
`<new module>::a_board_beyond…` — a test-case rename in the law-(c)
sense. Moving the whole file keeps every function name verbatim.

## 3. Name-set identity (law c)

Method: `cargo nextest list --workspace | sort` on a detached
`origin/main` worktree (baseline; note: the main checkout sits on a
stale branch 29 commits behind — an earlier stale capture was
discarded) vs the branch.

- The six untouched crates (`domain`, `state`, `storage-sqlite`,
  `sync-files`, `sync-files-nextcloud`, `sync-nextcloud`): **byte
  identical** multisets (1326 + 206 + 150 + 146 + 121 + 332 tests).
- `taskboard-ui-slint` (1634 → 1361): exact function-level bijection
  modulo exactly the intended deduplication — the 13 harness
  self-test names (`support::driver::tests::*`,
  `support::golden_compare::tests::*`, `support::guard::tests::*`)
  were compiled into 25 standalone binaries before and 4 merged
  binaries after (25 → 4 copies each; 13 × 21 = 273 duplicate
  entries removed; 1634 − 273 = 1361). Verified programmatically:
  canonical before == after, residual 0.
- `taskboard-app` (1088 → 1088): exact bijection; the only string
  delta is the inserted member-module segment
  (`cli_editing::<fn>` etc.), which is inherent to grouping and
  touches no function name. Verified programmatically: canonical
  before == after, residual 0.

Verdict: **zero test-case renames, zero losses, zero additions.**
The plan §5 "diff must be empty" criterion as literally written
(byte-identical `nextest list` output) is unsatisfiable under any
grouping that nests members as modules — the discrepancy decomposes
exactly into (a) the member-module prefix and (b) the harness-dedup
this chunk exists to perform. Recorded here as the plan-criterion
erratum; the function-level identity above is the substantive
guarantee.

## 4. Plain `cargo test` evidence (the chunk's core criterion)

Merged targets, wholesale, on this branch:

| Target | Result |
|---|---|
| `journey_suites` | ok — 102 passed, 8 ignored |
| `probe_composition` | ok — 68 passed |
| `probe_pixel` | ok — 45 passed |
| `structural` | ok — 22 passed |
| `app_core` | ok — 138 passed, 1 ignored |
| `cli_output` | ok — 45 passed |
| `cli_sync` | ok — 88 passed |
| `cli_tasks` | ok — 112 passed |
| `app_services` | 71 passed; 2 failures, both the pre-existing plain-cargo class (see below) |

The OnceLock/platform-init multiply is **gone**: the four ui-slint
merged targets pass in one process each, and `probe_composition`'s
former 3 failures (2 harness self-tests + the virtualisation capture)
are resolved by the shared module + the strict partition.

The two `app_services` residuals:

- `ipc::j1_daemon_stop_answers_while_a_verb_is_in_flight` — fails
  **on `origin/main` standalone too** (`Credential(NotFound)`,
  ipc.rs:68; reproduced in the baseline worktree). Pre-existing.
- `ui_settings::a_sign_in_derives_the_connector_name_and_a_re_sign_in_reuses_it`
  (+ nondeterministically its `a_save_with_no_config_file_at_all…`
  sibling) — passes standalone on `origin/main`; fails only under
  plain `cargo test` in the merged process (cross-member env/xdg
  interference). Same per-test-isolation dependence as #1016;
  nextest-green. Commented on #1016 with the app-side evidence
  (comment 7028116041).

## 5. Per-gate results

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
  (via pre-commit memory-gate; the app half initially failed with
  the `duplicate_mod` class until the shared-common fix).
- `scripts/run-quality-gates.sh cargo nextest run --workspace`
  (flock-wrapped): **4730 passed, 0 failed, 64 skipped**
  (Summary 152.687s; lock briefly contended with the parallel
  `feat/ui-row-cursor-walk` lane, waited cleanly).
- Slint token lint (pre-commit): clean.
- Nextest filtering smoke: `cargo nextest list -E 'test(us1_s1)'`
  and `-E 'test(probe)'` resolve the same function names, now also
  inside the merged binaries (e.g.
  `taskboard-ui-slint::journey_suites focus_law_journeys::us1_s1_…`)
  — `test()` substring resolution is unaffected by the member-module
  prefix.

## 6. Measurements, before vs after

Method: `/usr/bin/time`-wrapped, suites pre-built once (numbers are
run-dominated, not build-dominated), executed **sequentially** on an
idle box (a first parallel pair was discarded as contention-polluted;
first attempts without `--no-fail-fast` aborted early at the known
plain-cargo failures and are also discarded).

| Metric | before (`origin/main`, 98 binaries) | after (branch, 47 binaries) |
|---|---|---|
| `cargo test --all-targets --no-fail-fast` wall | 358.56 s | 662.40 s |
| same, CPU (user+sys) | 272.5 + 48.9 s | 268.6 + 37.9 s |
| `nextest run --workspace` wall | 86.9 s | 152.7 s |
| same, passed/failed | 5002/1 (flaky, #697) | 4730/0 |

Reading: CPU is flat (the merge neither adds nor removes work);
nextest wall roughly doubles with the halved binary count — cargo's
across-binary parallelism has 47 instead of 98 scheduling units, so
the long serial members (cli daemons, app_core's 188 s bootstrap
block under plain cargo) fill the tail. This is the known trade-off
the plan accepted for build locality ("Larger single binaries slow
targeted rebuilds" was judged acceptable; the run-side cost is the
mirror image and was not in the plan's §2 baseline derivation —
recorded here as a finding for stage 3 / the follow-up consolidation
issue). The plain-cargo wall additionally includes the #1016-class
process-serialization artifacts that nextest does not pay.

## 7. Salvaged vs redone (session continuity)

- Salvaged: the 5 landed commits (plan, journeys.rs label correction,
  both grouping commits, tooling-path updates) and the in-flight
  uncommitted conversion of 24 member files to `use crate::support;`
  — the direction was sound and is completed verbatim.
- Completed from the recorded direction: the root-side `mod support`
  declarations (the uncommitted work had only done the member side),
  the `structural.rs` bench-fixture variant, the mode partition
  (virtualisation), the app-crate twin fix (the first session had
  not touched it — caught by the pre-commit `duplicate_mod` gate),
  all verification, and this report.

## 8. Deviations from the plan

1. `virtualisation.rs` → `probe_pixel` (not `probe_composition`);
   option sites standardized `false` → `true`. Plan §3.1/§7 premise
   was factually wrong; the §6 mitigation dictated the move. No test
   names changed.
2. `driver.rs`/roots gained the `HEADLESS_RENDERER` const seam (not
   in the plan's diff inventory; required so the shared harness
   self-tests agree with any host binary).
3. Name-set identity criterion satisfied at function level (§3) —
   the literal byte-identity reading is an erratum.
4. journeys.rs carries one added const (7 lines) though the plan
   marked it "unchanged" — required because it compiles the shared
   `driver.rs`.

## 9. Issue activity

- Commented on #1016 (app-side plain-cargo evidence + the landed
  ui-slint mechanics fix): comment 7028116041. The two `app_services`
  plain-cargo residuals stay on #1016's plate (nextest is the gate;
  plain `cargo test` wholesale for those members remains B4a-class
  scope).
- Opened #1090 (backlog: the run-side wall regression — nextest 86.9 s ->
  152.7 s with CPU flat; the plan's §2 baseline only derived build-side
  costs). Commented on #697 with a second flaky observation of
  `cli_tty tty_ctrl_c_during_a_stalled_line_cancels_and_the_next_line_executes`
  (failed in the origin/main baseline run, passed on the branch).
- Every other out-of-scope finding maps onto #1016.

## 10. Handoff

Stage 3 owns review + PR open. Worktree
`taskboard-WT-refactor-consolidate-test-binaries` retained for that
stage; the baseline worktree `taskboard-WT-r4-main-baseline`
(detached `origin/main`) was measurement-only and is removed at
handoff. No push performed.

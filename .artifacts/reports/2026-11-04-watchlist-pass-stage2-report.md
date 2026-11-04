# [Report] The November Watchlist Pass — stage-2 implementation record

Chunk 1.6 of the backlog round 3 dispatch
(`.artifacts/plans/2026-11-03-backlog-round3-dispatch-plan.md` §2); plan:
`.artifacts/plans/2026-11-03-watchlist-pass-plan.md`. Branch
`test/watchlist-pass-2026-11`, worktree
`../taskboard-WT-test-watchlist-pass-2026-11`. Stage-2 run date:
2026-11-04 (work started 01:26 UTC). No production code changed on this
branch; the only diff artefacts are this report and the plan (D3, D10).

## 1. #560 — ruling executed; the flip-proof transcript

**Ruling (plan §1.5/D1, restated):** the natural daily window is the
**device-local calendar's today** (D-F6: midnight rollover on the
device's fixed UTC offset, resolved once at boot by
`AppConfig::resolved_day_calendar`). Production is correct; the
pre-`396bd6e3` test oracle (UTC `date_naive()`) was the wrong-calendar
side. Grounds are semantic (§1.2–1.4: D-F6's written contract, the
night-owl knob, the house's local-time direction in `c228159f`/`3aa7cb96`),
not test-convenience.

**State of the world:** the test-only fix had already landed on
`origin/main` as `396bd6e3` (PR #609, via duplicate observation #606)
before this branch opened; this chunk verified rather than re-fixed.
Branch verified to contain `origin/main` tip (`396bd6e3`) —
`git merge-base --is-ancestor origin/main HEAD` → true.

**Flip-proof (plan §1.8, binding protocol, executed 2026-11-04
01:26–01:33 UTC):**

1. Oracle temporarily flipped to the wrong calendar —
   `offset: *chrono::Local::now().offset()` →
   `offset: chrono::FixedOffset::east_opt(0).unwrap()` in
   `crates/taskboard-app/tests/cli_habits.rs` (h02); production
   untouched; nothing committed.
2. Hostile zone picked by the hour rule (UTC hour `01` < 12 →
   `TZ=UTC+11`, POSIX sign inversion: device-local = UTC−11, local date
   lags).
3. Straddle verified **before** the run:
   `date -u +%F` → `2026-11-04`; `TZ=UTC+11 date +%F` → `2026-11-03`.
   Dates differ — the run is evidence.
4. `TZ=UTC+11 cargo nextest run -p taskboard-app --test cli_habits
   h02_log_receipts_name_window_and_count_and_adjust_corrects` →
   **FAIL**, verbatim signature:

   ```
   FAIL [   0.052s] taskboard-app::cli_habits
       h02_log_receipts_name_window_and_count_and_adjust_corrects
   thread 'h02_log_receipts_name_window_and_count_and_adjust_corrects'
       panicked at crates/taskboard-app/tests/cli_habits.rs:135:5:
   the natural daily window is the local-calendar today
   Summary: 1 test run: 0 passed, 1 failed, 4 skipped
   ```

   Exactly the #560 mechanism: receipt window = the local calendar's
   day, oracle = the UTC calendar's day, and the bracket did **not**
   mask the wrong calendar (it only absorbs time-passage). The pin is
   load-bearing.
5. Oracle restored (`git checkout --`), h02 re-run plain →
   **PASS** (1 passed, 4 skipped, 0.088 s).

No weakening anywhere: the landed bracket tolerates two adjacent
*todays* on one fixed calendar, never two calendars (plan §1.9 ledger).

**1.2-serialization gate:** not engaged (D2) — the local ruling needs
no production change; the UTC counterfactual's serialization record is
plan §1.7.

## 2. #570 — bounded loaded repro: bounded negative

Protocol (plan §2.2–2.3, binding): #572 load shape, one-at-a-time
iterations, bound 30 iterations or 90 min wall, capture-before-rerun on
any failure.

**What ran** (2026-11-04 01:30:42 → 01:41:58 UTC, commit under test
`8933b5d2` — production identical to `origin/main`/`396bd6e3`):

- Load: 32 spinners (`yes >/dev/null`, 2 × nproc=16) held for the whole
  loop; one parallel `cargo build --workspace` in the scratch checkout
  (`/home/viktoria/src/taskboard`) for the first ~10 iterations.
  Sampled 1-min load: 22.27 at loop start, 57.45 mid-loop. (Script
  defect, cosmetic only: the per-iteration `uptime` line failed to
  render due to a bad `sed` quoting; the start/mid samples above are
  the recorded load evidence.)
- 30 iterations of
  `cargo nextest run -p taskboard-app --test cli_ipc
  t16_track_and_export_over_the_relay` (filter runs t16 alone; no
  sibling pty contamination). Iteration wall times 0.8–4.3 s after the
  first (iter 1 absorbed cargo lock waits behind the parallel build).

**Outcome: 30/30 green, failures = 0.** Stopping rule fired: the
30-iteration bound (90-min wall bound not reached).

**Disposition (plan §2.4):** this is a **bounded negative**, not
recurrence-free evidence — #570 keeps its single 2026-11-01 observation
and stays **open**, now carrying a standing capture policy (mirroring
#336's annotation shape):

> On any recurrence, record before re-running: (1) the full nextest
> failure section verbatim — the exit-code assert line and both
> `{out}`/`{err}` payloads (the preceding stderr lines distinguish a
> `try_connect` transport trace from silence); (2) the install scratch
> dir path (socket sidecar, daemon log) until the issue comment is
> posted; (3) lane, load shape, commit.

No speculative fix (dispatch hard rule; the #384 precedent). Close only
if root-caused and fixed — not the case here.

## 3. Standing recurrence scans — #287 / #336 / #384

Protocol (plan §3.1): thread-first, then
`gh run list --created ">=2026-11-01" --limit 300`.

1. **Thread reads**: all three threads' latest records are the
   2026-11-01 re-triage comments — no new observations to triage.
2. **Run scan**: 150 runs since 2026-11-01 (133 success, 10 cancelled,
   7 failure). Every failing run inspected via
   `gh run view <id> --log-failed`; **zero hits** for the family
   signatures (`actor_wiremock` a10/a12/`create_stack_transport_failure`,
   `delete_404_re_resolves_and_deletes_at_the_fresh_binding`,
   `l13_alias_happy_path_writes_stderr_connect_keeps_it_empty`).
   The 7 failures, all outside the scanned families:
   - 41645798201 — `taskboard-ui-slint::golden_visuals` (kiosk_today,
     notification goldens) — UI golden family, chunk-7 PR of round 4.
   - 41640838462 — log not retrievable via `--log-failed` (expired or
     redacted); conclusion `failure` on a docs-only PR, no test output
     available; not attributable to any watchlist family from available
     evidence.
   - 41624892750 — compile error (rustc exit 1 in coverage/clippy/test
     lanes) on the in-flight `feat(domain,app)` tracker-kind PR — code
     failure of the PR under review, not a flake.
   - 41513404840 — `cli_tty::tty_ctrl_c_during_the_masked_read…`
     30 s timeout — **the #524/#572 pty/masked-read family**. Out of
     this chunk's scan mandate (#287/#336/#384), and predates the
     chunk-1.1 re-arm fix soak; flagged here for the #524 watchlist
     triage rather than commented ad hoc (AGENTS §7: no opportunistic
     fixes; record via the owning issue's pass).
   - 41509332107 — `mcp_smoke::the_mcp_lane_boots_screenshots_and_exits_clean`
     panic (mcp_smoke.rs:217) — not on any watchlist; no issue exists;
     noted here as the observation record.
   - 41472239432 — `taskboard-ui-slint flow_table_tests::
     first_frame_hints_match_the_keymap_projection` — UI unit family.
   - 41430933177 — Miri workflow: `clock_gettime` REALTIME unsupported
     under isolation — toolchain/infra, not a test flake.
3. **Comment drafts** per the plan's format, one dated comment per
   issue (stage 3 posts): no recurrence over 2026-11-01 → 2026-11-04;
   capture policies unchanged, referenced by date; ruling: no code
   change, all three stay open.

## 4. Gates

Two full-suite runs via `scripts/run-quality-gates.sh` (flock-serialized
in the main checkout's git dir):

1. **First run (2026-11-04 ~03:05–03:10 UTC) — failed, new watchlist
   evidence, not a branch defect.** The nextest phase aborted fail-fast
   at 595/3131 on `taskboard-app::cli_track
   t_kind_status_tracked_today_excludes_respite`
   (`cli_track.rs:1288`): `left: 621, right: 2000` ("the 2_000s of work
   count; the respite block does not"). Mechanism: the test's
   `day_calendar()` helper is a fixed non-midnight calendar (day start
   05:00 at +02:00 = window start 03:00 UTC) while the work span
   anchors at `Utc::now()` unbracketed — run inside 2000 s after
   03:00 UTC, the span straddles the window start and the correctly
   clipping production yields `now − 03:00:00Z` (621 s at 03:10:21 UTC).
   The branch's production code is byte-identical to `origin/main`;
   per the plan's residual rule and AGENTS §7 the observation was
   recorded on the watchlist as issue **#610** (capture record + fix
   shape, no in-chunk fix, no weakening) instead of being tuned away.
   Isolation re-run outside the window (12:56 UTC): PASS.
2. **Second run (12:58 UTC, outside the 03:00–03:33 UTC failure
   window) — full suite passed, exit 0**: `cargo fmt --all -- --check`
   ✓; `cargo clippy --workspace --all-targets -- -D warnings` ✓;
   `cargo nextest run --workspace` ✓ (3131 run, 3131 passed, 41
   skipped, 109 s); `cargo test --doc --workspace` ✓; `cargo deny
   check` ✓ (informational duplicate-crate warnings only);
   `cargo llvm-cov nextest` phase ✓ (3131/3131 again, 124.7 s);
   coverage quarantine ✓ (all 3483 profraw files valid); lcov report
   written; `target/llvm-cov-target` cleaned by the wrapper per §10.

## 5. Deviations

- §2.2's scratch build: the plan said "a scratch checkout (not this
  worktree)"; used the main checkout `/home/viktoria/src/taskboard` as
  the scratch (an existing independent target dir; creating a fresh
  checkout would have spent ~minutes and disk for identical effect).
- Per-iteration uptime logging had a `sed` quoting bug (cosmetic);
  start/mid-loop load samples recorded manually instead.
- The run-scan used `--limit 300` (plan said 100): the first 100-run
  query returned exactly 100 rows, so the window could have been
  truncated; 300 covered the full window (150 runs total).
- The first gate run surfaced the #610 deterministic window-clip
  failure (§4.1); the suite was re-run in full after the failure
  window passed — the second run is the gate of record. No branch
  change was involved between the runs.

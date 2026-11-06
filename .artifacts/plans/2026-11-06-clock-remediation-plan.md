# [Plan] Clock-seam remediation — the flaky-test dedicated pass (implementation plan)

- **Date**: 2026-11-06
- **Kind**: implementation plan for the accepted remediation batches of
  the clock-seam audit. Dispatched via
  `.artifacts/plans/2026-11-06-clock-remediation-dispatch-plan.md`
  (4 chunks); this document is the normative origin for their stage-1
  plans.
- **Origin**: the audit report
  `.artifacts/reports/2026-11-06-clock-seam-audit-report.md` (§4 the
  inventory, §6 the batches, §7 the rulings) — accepted by the user on
  2026-11-06 with all four rulings: **R1 accepted** (thread `now`
  through `status_on`/`track::status`), **R2 accepted** (seam scope
  grows need-driven only — no prophylactic extension to the bypass
  passes), **R3 accepted** (single-read capture for the editor-path
  races — no new seam), **R4 accepted** (replace `StepClock`).
  The rule itself is now normative in
  `docs/testing_strategy.org` §6.1–6.2 and
  `docs/coding_guidelines.org` §2.2.
- **Normative base**: `origin/main` @ `81c6f585` + this branch's docs
  commits. File:line anchors are the audit's (that tree).

## 0. Scope and non-goals

**In scope**: every A row (2), every B row (6 clusters), batch 3
(fixture hygiene, the C6 cluster), and the issue bookkeeping
(#627, #610 close with evidence; #658/#659/#660 already fixed on main
get closing comments referencing the audit).

**Non-goals** (deliberately not dispatched — recorded so nobody
re-derives the boundary):

- No prophylactic extension of `UiSettings::clock` to the bypass
  passes (`run_alarm_pass`, notice expiry, editor validity — audit
  §2.4). C1's minute-margin seeds stay as they are until an
  exact-artifact test renders that content (R2).
- No threading of `now` through the other CLI handlers
  (`repeat_on`, `timers list_on`, `habits list_on`, `tasks`, web) —
  their tests are already-fine via flanking brackets; the D-F6
  completion is done where a test needs it (`track::status`).
- No changes to the live/docker sync tiers (deliberate wall reads) or
  the state/domain suites (already fully seamed).
- C2–C5 latent shapes: untouched; one umbrella backlog issue records
  them (see §6 bookkeeping).

## 1. Decisions (binding for the chunks; deviations need a recorded reason)

- **D-1 — the pinned instant.** One constant per test binary where a
  pin is used: `Utc.with_ymd_and_hms(2026, 11, 6, 12, 0, 0)` — a
  mid-day UTC instant, identical to the review-ledger template
  (`golden_visuals.rs:844-847`), so the whole golden family shares one
  capture class. Name it `PINNED_NOW` (or `fixed_now` where a const is
  impractical) with a doc comment citing the rule
  (`docs/testing_strategy.org` §6.2) and #660.
- **D-2 — anchor shape** (where anchoring is used): the #658 clamp —
  `start = (now − Δ).max(window_start)` with `window_start` derived
  from the *same* calendar the production code consumes
  (`day_window(now, day_start.0, day_start.1)`), never a hand-computed
  midnight.
- **D-3 — single-read shape** (R3): one `let now = …` (or
  `Local::now().date_naive()`) captured at the top of the test, passed
  into every seed builder and every expectation derivation in that
  test. The production read that remains (the adapter's own
  `Local::today` at `adapter.rs:1570/1618/1302`) is the accepted
  millisecond-at-local-midnight residual — R2/R3 explicitly decline to
  seam it; a comment at the test names the residual.
- **D-4 — golden regen**: exactly one regen run per chunk that touches
  goldens (`TASKBOARD_REGEN_GOLDENS=1 cargo nextest run -p
  taskboard-ui-slint --test golden_visuals`), then a compare run green
  without the variable; the PNG diff is the PR review artefact
  (`golden_visuals.rs:33-43`).
- **D-5 — verification matrix for pinned/anchored UI tests**: run the
  affected test binaries under `TZ=UTC`, `TZ=Pacific/Kiritimati`
  (UTC+14) and `TZ=Etc/GMT+12` — the #627 evidence method. Structural
  determinism across zones is the acceptance signal; wall-clock-hour
  independence follows by construction (all inputs are the constant).
- **D-6 — no assertion weakening.** Every fix keeps the assertion's
  semantics (AGENTS §5); where an expectation becomes derived instead
  of hardcoded (e.g. #610), the derivation uses the *same* pure
  function family the production code uses, or the same calendar —
  never a copy of production logic re-implemented in the test.

## 2. Chunk 1 detail — the CLI now-thread + app-crate races (R1, A2, B4, B5, B6)

### 2.1 R1: thread `now` through the track status path

Production (all in `crates/taskboard-app/src/commands/track.rs`):

- `status_on(app: &App, day: DayCalendar)` → `status_on(app: &App, day:
  DayCalendar, now: DateTime<Utc>)`; body drops its `Utc::now()`
  (`track.rs:69-72`). Update the D-F6 doc comment: `day` *and* `now`
  travel as data; the caller resolves both once per invocation.
- `pub async fn status(config, store, day)` → `status(config, store,
  day, now: DateTime<Utc>)` (`track.rs:76-82`); passes `now` through.
- The dispatch site `track.rs:1001` captures `Utc::now()` adjacent to
  its existing calendar resolution — one capture per invocation.
- The REPL leg `track.rs:1078` (`emit_result(io, Ok(status_on(app,
  day)), …)`) captures its own `Utc::now()` once per dispatch line.
- **Non-goal**: no other handler signatures change.

Tests:

- `cli_track.rs` `run_status` helper (`:213-220`) gains a `now` param
  threaded to `track::status`.
- **A2 / #610** (`t_kind_status_tracked_today_excludes_respite`,
  `:1320-1348`): capture `now` once; stamp work `[now − 2000 s, now]`
  and respite `[now − 4000 s, now − 3000 s]` (unchanged shapes);
  replace the hardcoded `2_000` expectation with
  `min(now, span_end) − max(span_start, window_start)` computed via
  the same `day_window` + clip the production view uses — or, simpler
  and equally deterministic, pass `now` to `track::status` and keep
  `2_000` (the span is verbatim-stamped by `log`, so the counted total
  is exactly 2000 s at the test's `now`). **Preferred: the simple
  form** — the span lies inside the window *at the test's now* because
  the test's `day_calendar()` window start is `now`-relative data
  computed by `day_window(now, …)`; assert `2_000`.
- **B4 / t10** (`t10_status_over_a_constructed_day_window`,
  `:590-631`): pass the same captured `now` to every `run_status`
  call; the anchors and expectations already derive from it. The
  03:00:00.000 24-hour-flip race disappears (no second read).
- Tests whose fixtures are *engine-stamped* (open entries via `start`
  — t1 `:251-270` etc.) keep their before/after brackets: they are
  already-fine and their stamp source is the engine clock, which R1
  does not thread. Do not touch them.
- **B5** (`cli_timers.rs:94-97`, `--at 09:30` leg): capture
  `let before = Utc::now()` before the parse; assert the typed shape
  `fire_at.with_timezone(&Local).time() ==
  NaiveTime::from_hms_opt(9, 30, 0)` and the day ∈
  {`before`'s local date, +1 day} (the cli_repeat neighbor-set
  pattern, `cli_repeat.rs:521-539`) — closes the 09:30:00.000 race
  without pinning a date.
- **B6** (`common/mod.rs:286` `card_json`): stamp `lastModified` at
  `(Utc::now() + TimeDelta::seconds(1)).timestamp()` — strictly
  future, so the echo can never read older than the local edit (the
  #130 carve-out's input); update the comment. Consumers
  (`cli_verbs.rs` v5 leg `:288`, `cli_attach.rs:44`) unchanged.

**Acceptance**: `t_kind_status…` and `t10…` pass with no wall-clock
input (every time-dependent value is the test's own `now`); the
09:30 leg and the v5 `pending_ops == 0` leg are race-free by
construction; full §10 gates green.

## 3. Chunk 2 detail — the clock-freeze family in ui-slint (A1, B1, B3)

One mechanism throughout: pin `UiSettings::clock` at `PINNED_NOW`
(D-1), derive every seed from the pinned instant, and (goldens only)
regenerate once (D-4).

### 3.1 A1 / #627 — the kiosk tracked-section test

`crates/taskboard-ui-slint/src/flow_table_tests.rs:3463`
(`the_kiosk_tracked_section_filters_to_the_viewed_board`):

- Add a fixed-instant sibling of `closed_entry_on` (`:2806-2827`):
  `closed_entry_at(state, task, start: DateTime<Utc>)` — same entry
  shape, absolute instants, no `Local::now()`.
- Seed, at `PINNED_NOW = 2026-11-06 12:00 UTC`: task_one
  `09:00–10:00 UTC` (fully past — contributes 1 h but is filtered out
  by the board), task_two `11:00–12:00 UTC` (ends exactly at the
  frozen `now` — `min(end, now) − start` = 1 h). Both entries lie
  strictly inside the frozen UTC day window, so the rows assertion
  `== vec![task_two]` and the `"1 h"` day-total phrase
  (`part_total(TimeDelta::hours(1))`) are exact at any wall time.
- Mount with `UiSettings { kiosk: true, clock: Some(PINNED_NOW),
  ..Default::default() }` (the test already builds a `UiSettings`
  at `:3570-3573`).
- The other `closed_entry_on` call sites (`:2939`, `:3149`, `:3420`)
  are **not** converted here — they are chunk 3's single-read family
  or already-fine; keep `closed_entry_on` for them.

### 3.2 B1 — the four UTC-midnight straddle tests

- `journeys.rs:13761` (`the_reorder_routes_the_two_swap_writes`):
  derive the two seed boxes from `next_day_window(PINNED_NOW, …)` and
  mount the journey with `clock: Some(PINNED_NOW)` (journeys mount
  through the real `mount` — pin via the settings they already build).
- `journeys.rs:13677` (`the_placement_routes_one_mask_and_undo_returns_it`):
  the expected instant at `:13916-13917` becomes
  `next_day_window(PINNED_NOW, …)`; the mount is pinned; the
  self-recorded race comment at `:13912-13915` is replaced by the pin
  rationale.
- `flow_table_tests.rs:3610` (`the_placed_box_reopen_seeds_the_form_from_the_instant`):
  seed instant from `next_day_window(PINNED_NOW, …) + 2 h`; the rig's
  mount pinned.
- `models.rs:3894-3916` (the `tracked()` helper residual): inspect the
  rig — if the models test rigs construct their `DayCtx` through the
  adapter (mount), pin it and stamp `tracked()` relative to
  `PINNED_NOW`; if the rigs build `DayCtx` directly, collapse to
  single-read (one `now` threaded into both `tracked()` and the rig's
  `DayCtx`). Either is acceptable — record which and why
  (§6.2's exact-instant row). The positivity assertion (`:4110-4113`)
  is untouched.

### 3.3 B3 — the nine clock-exposed golden journeys

Fixture plumbing (`tests/support/fixture.rs`):

- `golden_state()` (`:348-353`) gains `golden_state_at(now:
  DateTime<Utc>)` — the open entry stamps `now − 12 m` against the
  *parameter*; `golden_state()` delegates with `Utc::now()` (the
  structural-only journey consumers keep their shapes — audit C3).
- `today_overdue_state()` (`:466-493`) gains the same `_at` variant:
  the overdue anchor stays midnight-aligned (`(now − 1 d).date_naive()`
  at 00:00 of the pinned instant's yesterday — bucket-stable), the
  chip entry `now − 12 m`.

Journeys (`tests/golden_visuals.rs`) — each gets
`let now = PINNED_NOW;` seeds via the `_at` builders and
`UiSettings { clock: Some(now), ..scale_settings(…) }` (the
review-ledger template at `:833-885` verbatim in shape):

| Journey | line | seeds to re-relativize |
|---|---|---|
| the_board_goldens_hold_across_the_scale_breakpoint_matrix | :148 | chip (`golden_state_at`) |
| the_focused_cell_golden_pins_the_ring | :190 | chip |
| the_today_goldens_hold_across_the_scale_breakpoint_matrix | :272 | chip + overdue anchor (`today_overdue_state_at`) |
| the_kiosk_board_golden_holds | :380 | chip |
| the_kiosk_today_golden_holds | :400 | chip + overdue anchor |
| the_notification_golden_holds | :429 | chip + task due `(now−1d).date_naive()` + alarm fired `now − 30 m` (`:444-452`) |
| the_task_view_goldens_hold_across_the_scale_height_matrix | :503 | chip |
| the_focus_overlay_golden_holds | :587 | chip; also add `pin_utc_zone()` for parity (audit: it is the one golden without it) |
| the_track_entries_golden_holds | :667 | chip only (overlay rows are already fixed absolute instants) |

The rendered chip text becomes `· 12m` frozen — byte-stable at any
wall time and any run duration. Regen once (D-4); all nine PNGs'
diffs must be chip/row strips only (review-verifiable).

**The #663 correction (2026-11-06, after this plan was written)**:
the review-ledger golden, though outside the nine above, turned out
to carry the same class one level deeper — its inherited
`golden_state()` open entry (real `now − 12m`) is a boundary bomb
against its *frozen* oracle for the whole [12:00, 24:00) UTC band.
It was pinned and regenerated on the docs branch itself (PR #663;
audit report §9). Chunk 2's `golden_state_at(PINNED_NOW)` variant
below supersedes that in-journey re-stamp, and every pin-the-clock
conversion in this chunk must audit the seeds it *inherits*, not
just the ones it adds.

**Acceptance**: the nine journeys + the two B1 journeys + A1 pass
under the D-5 TZ matrix; `cargo nextest run -p taskboard-ui-slint`
green; goldens compare (not regen) green post-regen; the
`the_review_ledger_golden_holds` template untouched.

## 4. Chunk 3 detail — single-read capture on the editor paths (B2, R3)

`flow_table_tests.rs` (the exact-`LogTime` family) and one journey:

- `:2933` (`the_track_form_edit_commit_routes_the_exact_adjust_mask`),
  `:2852` (`the_track_form_add_commit_routes_logtime_with_the_exact_instants`),
  `:3260` (`the_add_commit_routes_over_the_form_subject_not_the_focus`):
  capture `let today = chrono::Local::now().date_naive()` **once** at
  the top of each test; derive both the entry seed (via
  `closed_entry_on`'s date — give the helper an optional `NaiveDate`
  or add `closed_entry_on_date(state, task, hour, today)`) and every
  expected instant from that one value (D-3). Each test carries the
  residual-race comment naming the accepted ms-at-local-midnight
  window (R2/R3).
- `journeys.rs:10936`
  (`the_setter_composes_the_custom_row_grammar`): the expectation at
  `:10998-11006` derives from the *same* single `Local::today` capture
  used to drive the setter session (the adapter's compose at
  `adapter.rs:1302` keeps its own read — accepted residual).
- **Non-goals**: no seam field for the editor paths (R2/R3); the
  already-fine call sites (`:3149` delete-affordance, `:3420` kiosk
  gate) are not touched beyond what the shared helper refactor needs.

**Acceptance**: each rewritten test has exactly one clock read
(grep-verifiable per test); expectations are all derived from that
capture; full gates green.

## 5. Chunk 4 detail — fixture hygiene below the UI (C6, R4)

All test-only; no production diffs; each site's assertion currently
ignores the value (audit C6), so the change is stamp-replacement only:

- `taskboard-state/tests/engine.rs:1504` — `Some(chrono::Utc::now())`
  → a fixed stamp from the suite's `common` `ts()` block (T0-based).
  This clears the §10 "fixture builders must not read the clock"
  violation in a Miri-scoped binary (the weekly lane's exposure).
- `taskboard-sync-nextcloud/src/push_exec/tests.rs:46` (`queued_at`)
  and `:103/:122/:156` (row clocks) — fixed increasing `ts()`-style
  stamps; if the file has no helper, add a tiny `fn ts(secs: i64)` in
  the test module mirroring the storage crate's blocks.
- `taskboard-sync-files/src/identity.rs:96` — the fixed-literal
  pattern its sibling tests already use (e.g. the RFC-3339 literal at
  `:57`).
- `taskboard-sync-files-nextcloud/src/device.rs:139/:156/:175/:191` —
  `FixedClock(<fixed literal>)` (the value is never consulted; the
  sibling at `:124` shows the asserting shape).
- **R4** `taskboard-sync-files-nextcloud/src/actor.rs:205-214` —
  replace `StepClock`'s `Utc::now() + 30 s` per-read wall read with an
  atomic stepping clock (fetch-add from a fixed base), mirroring
  `SteppingClock` (`taskboard-sync-nextcloud/tests/common/mod.rs:43-68`);
  fix the doc comment to tell the truth (a logical stepping clock).
  Keep the type name or rename to match the sibling — stage 1 decides,
  recorded.

**Acceptance**: `grep -rn "Utc::now()" crates/taskboard-state/tests
crates/taskboard-sync-nextcloud/src/push_exec/tests.rs
crates/taskboard-sync-files/src/identity.rs
crates/taskboard-sync-files-nextcloud/src/device.rs` returns zero
inline-test hits; the Miri-scope statement holds (the state test no
longer needs a realtime gate — do *not* add `cfg_attr(miri, ignore)`
to it; it should simply not read the clock); full gates green.

## 6. Bookkeeping (the dispatch plan's completion section owns execution)

- **#610** closes with chunk 1's PR (fix commit + the determinism
  argument in the close comment).
- **#627** closes with chunk 2's PR.
- **#658, #659, #660**: already fixed on main (`1072f064`,
  `81c6f585`) — each gets a closing comment referencing the audit
  report §5 (the framework explains the chosen tool) rather than new
  code. Any chunk may land these comments; default: chunk 2's stage 3.
- **Deferred backlog issue (one)**: `backlog: deferred latent
  clock-drift sites from the clock-seam audit` — C1 (minute-margin
  alarm/notice seeds, need-driven per R2), C2 (unpinned bar position),
  C3 (unrendered fixture stamps), C4 (virtualisation vacuous-pass
  risk), C5 (CLI latent shapes incl. the DST offset re-resolution).
  Labels `testing` + `p3`; body: summary + permalink to the audit
  report §4.3. Opened once, at completion bookkeeping.

## 7. Risks

- **Chunk 2's regen breadth**: nine PNGs regenerate; any accidental
  non-chip pixel change (e.g. a rendering-order side effect of
  pinning) would hide inside the diff — the review stage must diff
  the PNGs visually (GitHub renders them) and reject unexplained
  regions.
- **Chunk 1's signature ripple**: `track::status` is called from the
  dispatch table, the REPL leg, and two test helpers — small, but the
  stage-1 agent must enumerate callers before editing (grep
  `track::status|status_on`).
- **Chunk 3's helper refactor** touches `closed_entry_on`, shared by
  five call sites across two chunks' scopes — chunk 3 runs *after*
  chunk 2 merges (same file), and its stage 1 re-reads the merged
  helper.
- **The B6 `+1 s` future stamp**: `lastModified` in the future by
  construction — harmless for the merge path (echo is newer), but the
  stage-3 reviewer should confirm no consumer treats future stamps as
  skew (the sync skew logic uses the actor's injected `read_at`,
  not the echo — audit §2).

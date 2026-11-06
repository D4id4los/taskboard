# [Plan] Chunk 1 — the CLI now-thread + app-crate races (R1, A2, B4, B5, B6)

- **Date**: 2026-11-06
- **Kind**: stage-1 implementation plan for chunk 1 of the clock-seam
  remediation dispatch
  (`.artifacts/plans/2026-11-06-clock-remediation-dispatch-plan.md`).
- **Normative origins**: implementation plan
  `.artifacts/plans/2026-11-06-clock-remediation-plan.md` §0/§1/§2
  (decisions D-1..D-6 binding), audit report
  `.artifacts/reports/2026-11-06-clock-seam-audit-report.md` §4.1 row
  A2, §4.2 rows B4/B5/B6, §7 ruling R1 (accepted: the thread),
  `docs/testing_strategy.org` §6.1–6.2 (the seam-vs-anchor rule).
- **Branch**: `fix/app-track-status-now-thread` (worktree
  `../taskboard-WT-fix-app-track-status-now-thread`, created off
  `origin/main` @ `e42c1160` after a green `scripts/worktree-doctor.sh`
  run — 146 G free, floor 25 G).
- **Line anchors below** were re-verified on this exact tree
  (`origin/main` @ `e42c1160`).

## 0. Scope

Four production call-path edits in `crates/taskboard-app/src/commands/track.rs`
(the R1 now-thread), three test-family edits in
`crates/taskboard-app/tests/` (A2/#610, B4/t10, B5, B6). No other
handler signatures change (impl-plan §0 non-goal); no ui-slint files
touch (chunks 2–3's scope).

## 1. The caller inventory (grep `status_on|track::status`, this tree)

Complete; `tests/ipc.rs`'s `status_once` is the *different* module
`commands::status` — unrelated, untouched.

`status_on` (the booted-core read, `track.rs:69`):

1. `track.rs:82` — inside `track::status`'s `with_core` closure
   (pass-through).
2. `track.rs:1078` — the REPL/session leg in `track_line`
   (`emit_result(io, Ok(status_on(app, day)), render_text_status)`).

`track::status` (the one-shot boot lane, `track.rs:77`):

3. `track.rs:1001` — the `TrackInvocation::Status` arm of
   `track_command` (the dispatch closure after
   `let day = config.resolved_day_calendar()` at `:1000`).
4. `tests/cli_track.rs:215` — the `run_status` test helper
   (`:213-220`).
5. `tests/cli_track.rs:1342` — the #610 test
   (`t_kind_status_tracked_today_excludes_respite`, `:1314-1349`),
   direct call.

Indirect but unaffected: `t11` (`cli_track.rs:639+`) routes Status
through the real `dispatch_line` → `track_line`; `track_line` keeps
its signature (it captures `now` internally per §2.1 below), so t11
needs no edit. The other `run_status` call sites — t1 `:264`, t2
`:289`, t3 `:333` — are engine-stamped fixtures (already-fine per
impl-plan §2.1): they keep their before/after brackets and get only
the mechanical extra argument.

## 2. Decisions

### D-c1 — signatures: `now` after `day` (impl-plan §2.1 verbatim)

```rust
pub fn status_on(app: &crate::App, day: DayCalendar, now: DateTime<Utc>) -> TrackStatusView
pub async fn status(config: AppConfig, store: Arc<dyn CredentialStore>, day: DayCalendar, now: DateTime<Utc>) -> Result<TrackStatusView, CommandError>
```

`status_on`'s body becomes `status_view(&state.tracking, day, now)`
(the `Utc::now()` at `track.rs:71` is deleted). The D-F6 doc comment
at `:64-67` is updated: `day` *and* `now` travel as data; the caller
resolves both once per invocation. `#[must_use]` stays.

### D-c2 — capture points: one read per invocation, adjacent to the calendar resolution

- **Dispatch site** (`track.rs:998-1004`): the closure captures
  `let now = chrono::Utc::now();` immediately after the existing
  `let day = config.resolved_day_calendar();` and moves both into the
  async block: `async move { status(config, store, day, now).await }`.
- **REPL leg** (`track.rs:1077-1079`): `track_line` captures its own
  `let now = chrono::Utc::now();` inside the `TrackInvocation::Status`
  arm (one read per dispatch line — not hoisted to `track_line`'s
  top, so non-status verbs pay nothing).

### D-c3 — #610 keeps the hardcoded `2_000`, with a recorded deviation from impl-plan §2.1's *mechanism*

Impl-plan §2.1 prefers "keep `2_000`" over a derived expectation, and
justifies the now-relative span `[now − 2000 s, now]` by claiming the
window start is now-relative. **Re-verified against the tree: that
premise is false.** The test's `day_calendar()` (`cli_track.rs:31-36`)
is a *fixed-offset* calendar (05:00 @ +02:00 ⇒ the window opens at
**03:00 UTC today** regardless of `now`). A `[now − 2000 s, now]`
span still falls partly before the window start on every run inside
the daily band [03:00:00, 03:33:20) UTC — threading `now` fixes the
*dual read* (the handler no longer re-reads the clock) but not the
*fixture's window membership*, so the hardcoded `2_000` would still
fail in-band. This is exactly the §1 failure-window law.

**Decision (the deviation)**: keep the hardcoded `2_000` (the
impl-plan's preference) but stamp the spans into the window interior
relative to the *same calendar* the production view consumes — the
D-2 anchor shape, applied with the test's single captured `now`:

```rust
let now = chrono::Utc::now();                                   // the one read
let day = day_calendar();
let (window_start, _) =
    taskboard_domain::day_window(now, day.day_start_time, day.offset);
// Work strictly inside the window: exactly 2_000 s, verbatim by log.
let work = (window_start + TimeDelta::try_seconds(4_000).unwrap(),
           window_start + TimeDelta::try_seconds(6_000).unwrap());
// Respite inside the window too, disjoint from work.
let respite = (window_start + TimeDelta::try_seconds(1_000).unwrap(),
              window_start + TimeDelta::try_seconds(2_000).unwrap());
```

then `track::log(... LogMode::Span(work.0, work.1) ... Work)` /
`(... Respite)` (spans are stamped verbatim — `log` writes what it is
given), and finally
`track::status(config.clone(), forbidden_store(), day, now)` with the
**same** `now`. Determinism argument: both spans sit
≥ 1_000 s inside a 24 h window, so `work_in_window`'s clip at
`min(span_end, window_end)`/`max(span_start, window_start)` never
trims them; the handler reads no second clock; the counted total is
exactly 2_000 s (work-only — the respite leg keeps its exclusion
semantics). Asserted at every wall time, every TZ (the window start
moves with `now`'s UTC date, and the stamps move with it —
date-class-agnostic by construction, matching D-5's acceptance
signal). The assertion stays `assert_eq!(status.tracked_today_seconds,
2_000, …)` — no weakening, no text (AGENTS §5/D-6).

This honors D-6 (expectation not derived from a re-implementation;
the fixture simply never straddles the boundary) and the audit's A2
fix direction (a) ("anchor: start = max(…) and keep 2_000") in
spirit; the deviation from §2.1's literal "[now − 2000 s, now]"
mechanism is recorded here as §1 of this plan requires. The test's
existing comment ("the spans anchor at now — the day window is
calendar-relative") is replaced by the corrected rationale.

### D-c4 — t10 / B4: same captured `now` into every `run_status` call

`t10_status_over_a_constructed_day_window` (`:582-632`) already
derives `window_start/window_end` from its own `let now = Utc::now()`
(`:590`) and stamps all three closed entries plus the strict-bound
assert from it. The fix is exactly the now-thread: `run_status` gains
`now: DateTime<Utc>` (D-c5) and t10 passes its captured `now` at
`:622`. The handler no longer performs a second read, so the
03:00:00.000 UTC day-flip race (the handler's read stepping a day
past the test's, tripping the `:617-620` bound) disappears by
construction. Assertions unchanged (open-set membership + the
`>= 5_400` floor — a floor on garbage-free input, compliant with the
no-ceilings rule).

### D-c5 — `run_status` helper shape

```rust
async fn run_status(config: &AppConfig, day: taskboard_domain::DayCalendar, now: chrono::DateTime<chrono::Utc>) -> TrackStatusView
```

threading `now` into `track::status` at `:215`. Engine-stamped call
sites (t1 `:264`, t2 `:289`, t3 `:333`) pass
`chrono::Utc::now()` — one read per call, adjacent to the call; their
brackets and assertions stay byte-identical (their oracle is relative
— monotone elapsed, membership counts — so any read between boot and
assert satisfies them; §6.2 relative-oracle row). t10 passes its own
capture; the #610 test does not go through the helper (direct call,
D-c3).

### D-c6 — B5: the typed 09:30 assertion shape (no text, no pin)

`tests/cli_timers.rs:94-97` (t2 leg b) currently asserts a formatted
string (`local.format("%H:%M").to_string() == "09:30"`) — a text
assertion (AGENTS §5) — plus `fire_at > Utc::now()`, whose read races
`parse_at_flag`'s internal double `Local::now()` (`cli.rs:833-838`)
astride 09:30:00.000 local (audit B5). Replacement:

```rust
let before = chrono::Local::now();                      // before the parse
let receipt = run_add(&config, "time", &["--at", "09:30"]).await;
let local: chrono::DateTime<chrono::Local> = receipt.fire_at.into();
assert_eq!(
    local.time(),
    chrono::NaiveTime::from_hms_opt(9, 30, 0).expect("09:30"),
    "the local wall clock is the requested time"
);
let today = before.date_naive();
let tomorrow = today.succ_opt().expect("a date has a successor");
assert!(
    today == local.date_naive() || tomorrow == local.date_naive(),
    "the rolled day is today or tomorrow relative to the pre-parse capture"
);
```

Typed shape: `NaiveTime` equality + the two-date neighbor set (the
cli_repeat.rs:521-539 pattern, narrowed to {today, +1} because the
roll is strictly forward — the production read happens after
`before`, so its date can only be `before`'s date or its successor;
a mid-production midnight only picks the successor). The neighbor set
covers the residual production dual-read without pinning a date (the
audit's bracket fix, expressed structurally). Legs (a) and (c) are
untouched (fixed literals, no clock input).

### D-c7 — B6: `card_json` stamps strictly future, `+1 s`

`tests/common/mod.rs:281-292`: `lastModified` becomes
`(chrono::Utc::now() + chrono::TimeDelta::try_seconds(1).unwrap()).timestamp()`
— strictly future relative to every read the test's own process makes
afterwards, so the create echo can never floor back to an instant
older than the local edit (the whole-second B6 race at N.5 s vs N.7 s
is impossible: the echo is ≥ local-edit + 1 s − capture skew, and the
local edit stamps *before* the echo fixture is built). The comment
above the `let now` is updated to say why the stamp is deliberately
future (+1 s) and cite the audit's B6 row. Consumers — `cli_verbs.rs`
v5 leg (`:267`, asserting `pending_ops == 0` at `:278`) and
`cli_attach.rs:44` — are **unchanged**: `card_json`'s signature and
JSON shape are identical, only the timestamp value's phase moves.

## 3. Test catalogue — what changes, what must not

| Site | Disposition |
|---|---|
| `cli_track.rs` #610 (`:1314-1349`) | **Rewritten** per D-c3 (window-interior stamps, threaded `now`, `2_000` kept) |
| `cli_track.rs` t10 (`:582-632`) | **One-line-plus-comment change**: pass captured `now` at `:622` (D-c4) |
| `cli_track.rs` `run_status` (`:213-220`) | Gains the `now` param (D-c5) |
| `cli_track.rs` t1 `:264` / t2 `:289` / t3 `:333` | Mechanical arg only; brackets and assertions untouched (engine-stamped, already-fine) |
| `cli_track.rs` t11 (`:639+`) | **Untouched** — routes through `track_line`, which keeps its signature |
| `cli_track.rs` bracket-style track-kind tests (any test whose fixture is engine-stamped via `start`/`log` now-stamps) | **Untouched** — impl-plan §2.1: engine-stamped fixtures keep brackets; R1 does not thread the engine clock |
| `cli_timers.rs` t2 leg (b) (`:94-97`) | **Rewritten** per D-c6 (typed shape, neighbor set) |
| `cli_timers.rs` t1 (`:37-70`) and t2 legs (a)/(c) | Untouched (relative bracket / fixed literals) |
| `tests/common/mod.rs` `card_json` (`:281-292`) | `+1 s` stamp + comment (D-c7) |
| `cli_verbs.rs` v5 leg, `cli_attach.rs` | Untouched |

New tests: none required — every fix converts an existing test to a
deterministic-by-construction form (the dispatch's minimal-diff
policy). TDD ordering within the chunk: (1) rewrite #610's fixture
*first* and watch it fail to compile against the old signature —
then thread the signatures (the failing compile is the red step for
the production change); (2) t10; (3) B5; (4) B6. A deliberate
in-band demonstration is unnecessary: the D-c3 fixture is
date-class-agnostic by construction, which the D-5 TZ matrix
exercises.

## 4. Verification (the §10 gates + the D-5 matrix)

1. Full gates through the wrapper, from the worktree:
   `scripts/run-quality-gates.sh` wrapping the exact §10 sequence
   (`fmt --check`, `clippy -D warnings`, `nextest --workspace`,
   `cargo test --doc`, `cargo deny check`, the llvm-cov three-command
   sequence, the trailing `rm -rf target/llvm-cov-target`).
2. D-5 TZ matrix on the touched binaries (structural determinism is
   the acceptance signal):
   `TZ=UTC`, `TZ=Pacific/Kiritimati`, `TZ=Etc/GMT+12` over
   `cargo nextest run -p taskboard-app --test cli_track --test
   cli_timers --test cli_verbs --test cli_attach`
   (cli_verbs/cli_attach ride along because D-c7 moves their shared
   fixture's stamp phase).
3. Acceptance (impl-plan §2): `t_kind_status…` and `t10…` pass with
   no wall-clock input other than each test's own single capture;
   the 09:30 leg and the v5 `pending_ops == 0` leg are race-free by
   construction; gates green.

## 5. Risks

- **The D-c3 deviation** from impl-plan §2.1's literal mechanism —
  recorded above with the re-verification (the fixed-offset
  `day_calendar()` refutes the now-relative-span premise); the
  stage-3 reviewer should check the deviation rationale, not revert
  to the in-band-failing form.
- **`run_status` call sites**: three engine-stamped tests take a
  mechanical arg; a missed site is a compile error, not a silent
  regression (the signature change is self-guarding).
- **B5's neighbor set** tolerates a production midnight step but not
  a production *clock skew backwards* — `parse_at_flag` reads after
  the test's capture, so backwards skew is not a real shape on this
  hardware; if it ever were, the set widens to include
  `before.pred_opt()` (the cli_repeat three-date form).
- **B6's future stamp**: `lastModified` is now +1 s in the future by
  construction. The sync skew logic consumes the actor's injected
  `read_at`, not the echo (audit §2), so no consumer treats future
  card stamps as skew today — the stage-3 reviewer re-confirms (the
  impl-plan §7 risk, verbatim).
- **Estimated diff**: ~4 files, ≈ 70 lines changed
  (track.rs ≈ +10/−6; cli_track.rs ≈ +30/−18; cli_timers.rs
  ≈ +12/−7; common/mod.rs ≈ +5/−5). No new files.

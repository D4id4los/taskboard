# The Today Due Boundary Chunk Plan (Tier-2 P2, issue #893)

- **Date**: 2026-11-18
- **Kind**: chunk plan (stage-1 of the three-stage protocol; Tier-2
  dispatch plan §3 normative for scope, this plan resolves the open
  design questions)
- **Branch**: `fix/today-due-boundary` (worktree
  `../taskboard-WT-fix-today-due-boundary`, cut off `origin/main` @
  `c7be6e46`)
- **Origin**: issue #893; `.artifacts/plans/2026-11-18-tier2-dispatch-plan.md`
  §3; the N-4 landed law (`.artifacts/plans/2026-11-17-ui-r8-task-detail-editor-plan.md`,
  `days::day_window_end` / `DayCalendar::day_end`).

## 0. Disjointness verdict

**VERDICT: GO.** No in-flight r9 branches exist. `git branch -r`
(after `fetch origin --prune`) carries only
`origin/dependabot/*`, `origin/fix/ui-context-menu-bar`,
`origin/test/sync-nextcloud-docker-tier`, `origin/main`; the round-9
seeds named in the dispatch plan — K2 `feat/ui-directional-focus`, D2
`feat/ui-day-review-journey` — exist neither on origin nor locally,
and no local branch touches the planned diff sites (`router.rs`
`TaskDueToday`/`TaskScheduleToday` arms, `days.rs` membership/stamp
helpers, `due_edit.rs` bare-day mint). The worktrees the doctor
listed (`docs/clock-seam-audit`, `feat/ui-habit-row-rework`,
`fix-ui-r8-files-ops-dir`, `refactor-ui-overlay-migration`,
`test-golden-solo-render`) sit on disjoint surfaces (P1's subject is
`test-golden-solo-render`; the others are not in flight as branches
against these seams). Resource floors green (192G free; 18.9G
MemAvailable).

## 1. The bug, restated with the deeper finding

- `today_sections` membership (`crates/taskboard-domain/src/days.rs:274`)
  is `d >= now && d < end` — `end` is the today window's **exclusive**
  end (decision 2: unchanged).
- The `TaskDueToday` arm (`crates/taskboard-ui-slint/src/router.rs:1329`)
  writes `duedate = end` — the excluded instant. Same shape in
  `TaskScheduleToday` (`:1346`, `scheduled` mask; the same
  `s >= start && s < end` gate at `days.rs:270`) — **same-class
  defect, fix in scope** (stage-3 trap (c) resolved here).
- **Deeper finding (dispatch plan §3 anticipated it)**: the dispatch
  plan's premise "the write aligns with N-4 = the helper the editor
  uses" does not hold literally. The editor's bare-day mint
  (`due_edit.rs:194` `day.day_end(date)`, and the picker at
  `adapter.rs:12293`, and the CLI twins at
  `crates/taskboard-app/src/commands/mod.rs:1792,1797`) mints
  `day_window_end(date)` = the **window's exclusive end** of the
  named date. For a bare *today* entry that is the exact instant the
  today membership excludes — the editor and CLI bare-today paths
  carry the identical bug. So "router vs editor divergence" is not
  the issue; the issue is that **the N-4 mint itself, applied to the
  window's own end date, collides with the exclusive-end membership
  convention**. Unification therefore means: one *last-covered-instant*
  helper, adopted by every bare-day mint site (router verb arms,
  editor parse_stamp, picker, CLI).

## 2. Recorded decisions

- **D1 — the unified derivation and its owner (domain)**. New pure
  helpers in `taskboard-domain::days` (zero env reads, D-F6 data
  injection preserved):
  - `pub fn last_covered_instant(window_end: DateTime<Utc>) -> DateTime<Utc>`
    — the largest instant still inside the half-open window, i.e.
    `window_end − 1ms` (the stamping ms-resolution convention; the
    `day_window_end` doc already speaks of the literal
    `23:59:59.999`). Doc-comment records *why* it exists: the N-4
    stamp must survive the `[start, end)` membership comparisons at
    exact-instant precision, and `day_window_end` (the exclusive end)
    does not.
  - `impl DayCalendar { pub fn day_last_instant(&self, date: NaiveDate) -> DateTime<Utc> }`
    — `last_covered_instant(self.day_end(date))`, the stamping mint
    consumers call; sits beside `day_end` (which stays the pure
    window-end fact, its doc updated: the "identical classification
    at every whole-second comparison" sentence gains the caveat that
    exact-instant membership comparisons — `today_sections` — need
    `day_last_instant` instead).
  - No helper is duplicated in the adapter/UI layer (the #884 rule):
    the adapter computes `ctx` facts by *calling* the domain helper;
    it never re-derives.
- **D2 — membership stays `[now, end)`** (dispatch decision; no
  hunk may touch `today_sections`' predicates, `overdue_tasks`, or
  sorting).
- **D3 — every bare-day mint site switches to the unified mint**:
  1. `router.rs` `TaskDueToday` and `TaskScheduleToday` arms: the
     adapter supplies the stamp (below); the arms consume it — the
     router does no instant arithmetic.
  2. `DispatchContext` gains
     `pub today_last_instant: Option<DateTime<Utc>>` (doc: set by the
     adapter from `days::last_covered_instant(day_window.1)` — the
     window-derived fact, never `+/-24h` arithmetic; `NoTarget`
     without it, same backstop shape as `day_window`). The adapter
     populates it beside `ctx.day_window` (`adapter.rs:1443`) from
     `self.day_ctx().window` — same frozen-now seam the window
     already uses.
  3. `due_edit.rs::parse_stamp` bare-date branch: `day.day_end(date)`
     → `day.day_last_instant(date)`; the month-grid picker commit
     (`adapter.rs:12293`) likewise. Consequence, recorded and
     accepted: a bare-day entry's stamp now carries wall time
     `23:59` (local), so `at_time_suffix` renders `· 23:59` where the
     midnight stamp suppressed the suffix. That is honest output
     ("that whole day counts" until 23:59); the midnight suppression
     itself stays (a midnight stamp from older persisted rows keeps
     rendering without a time). Any snapshot touching this is
     re-blessed with a per-file diagnosis in the stage-2 report.
  4. CLI twins `commands/mod.rs:1792,1797` (`DueSpec::Date` →
     `day.day_last_instant(date)`): same-class, same unification
     clause; CLI receipt expectations and any pinned CLI tests update
     with the mint (typed facts, not text).
  5. Persistence: no migration. Old rows stamped at a midnight are
     still valid instants inside their named day; only *new* stamps
     use the last-covered convention.
- **D4 — `TaskScheduleTomorrow` is NOT a defect**: it writes
  `next_day_window(now).1` (tomorrow's exclusive end). No membership
  predicate gates on tomorrow's window (`today_sections` only gates
  today's; scheduled/due phrases are delta-bucketed, not
  window-tested). Unchanged; stage 3 re-verifies this analysis, and
  any discovered tomorrow-window membership consumer becomes a
  backlog issue.
- **D5 — us6_s2 re-anchor (strengthening, never weakening)**: keep
  the `today_overdue_state` two-row fixture (the second overdue row
  is the control). After the re-commit + settle:
  1. the drained command is pinned structurally:
     `UpdateTask { changes.duedate == Some(Some(stamp)) }` where
     `stamp = days::last_covered_instant(fixture_window.1)` computed
     from the fixture's own day facts (no wall clock — the fixture's
     injected `now`/window; clock law satisfied by construction);
  2. the context request is re-anchored to the **re-committed row's
     new position** — section 1 (due-today), row 0 — and still
     asserts the menu offers `task.due-today` (typed id list, not
     label text);
  3. new: the control row remains at section 0 row 0 (its own menu
     request still resolves) — the control now proves the matrix
     geometry instead of silently carrying the assertion.
- **D6 — the regression test from the instrumented repro (#893)**:
  new journey test `us6_s2_recommit_keeps_the_row_in_today` on
  `fixture::journey_state` (the single-overdue-row fixture the
  original diagnosis used): `ViewToday` → click overdue row (0,0) →
  `TaskDueToday` → settle → typed structural assertions only:
  the drained `UpdateTask` carries the D5 stamp, and the today table
  still holds the row (today-section row counts: overdue 0, due-today
  1 — the row moved *within* Today instead of leaving). Red under the
  current exclusive-end write (the table rebuilds `[0,0,0,0]`), green
  after D3.1. No due-phrase text assertions ([L]a).
- **D7 — unit matrix for the domain helper** (`days.rs` tests, beside
  `day_end`'s):
  - `last_covered_instant` of a default-config window is strictly
    inside `[start, end)` and classifies as due-today under the
    exact `today_sections` predicate (a property/proptest arm over
    generated `now` + day-start times: `start <= stamp < end`);
  - `day_last_instant(date) == last_covered_instant(day_end(date))`
    (the composition pin, mirroring `day_end_is_the_free_function_over_the_calendar`);
  - both N-4 law arms pinned: default `00:00` start → `23:59:59.999`
    local; a special `03:00` start → `02:59:59.999` local, still
    inside the named date's window (the wandering-stamp note in the
    existing doc is resolved by the mint staying inside the day).
- **D8 — census note rides the fix** (the due-today cell's write
  semantics change): a dated supersession line on the census rows for
  `task.due_today` (`docs/ux-surface-census.org` lines ~76 and ~288):
  "(2026-11-18 #893: the re-commit (and every bare-day due mint) now
  writes the day's **last covered instant** (`days::day_last_instant`),
  not the window's exclusive end — the `[now, end)` today membership
  is unchanged; a task re-committed today stays in Today and reads
  `· 23:59` on its stamp.)" The `task.schedule_today` row gets the
  same note.
- **D9 — `#893` comment**: stage 2 (or the dispatcher at merge)
  comments the deeper finding (§1) on issue #893 — the editor/CLI
  bare-today mint collided with the membership convention, fixed by
  the unified mint — so the closure record carries the full analysis.

## 3. TDD order (stage 2)

1. Domain helper + D7 matrix (red: helper absent; then green).
2. D6 regression journey (red under current behavior — the
   `[0,0,0,0]` rebuild from the issue's instrumented repro).
3. D3.1–3.2 (ctx field + adapter + router arms) → D6 green; D5
   re-anchor (us6_s2 must stay green *with* the stronger assertions —
   if it does not, that is a bug finding, not a test-loosening
   invitation).
4. D3.3–3.4 (editor, picker, CLI mints) + their test updates;
   snapshot re-blessing with per-file diagnosis.
5. D8 census note; `docs(plan)`-style standalone commits per commit
   hygiene (every commit stands alone).

## 4. Acceptance (mirrors dispatch §3, resolved)

- The instrumented repro passes: re-commit keeps the task in Today
  (typed section-membership facts, not phrases).
- The write and every bare-day mint use the one domain derivation
  (`day_last_instant` / `last_covered_instant`); no second derivation
  appears in the adapter, router, or CLI (grep-verifiable).
- us6_s2 strengthened per D5 (drained-command stamp pinned; menu
  asserted on the re-committed row; control row's role explicit).
- Membership predicates untouched (`git diff` shows no hunk in
  `today_sections` / `overdue_tasks`).
- Today corpus + whole-suite nextest gate green; every changed
  snapshot traced to D3.3's suffix consequence.
- Diffs confined to: `days.rs` (+helper/tests), `router.rs` (2 arms +
  ctx field + doc), `adapter.rs` (ctx population, picker mint),
  `due_edit.rs` (mint + doc + tests), `commands/mod.rs` (2 mints),
  `journeys.rs` (D5/D6), census doc. Estimated ~200–260 changed lines.

## 5. Risks

- **Suffix churn (D3.3)**: snapshots/tests embedding bare-day-derived
  phrases may show `· 23:59`. Bounded; re-bless with diagnosis only.
- **CLI stamp change** alters stored stamps for new CLI bare-day
  edits; sync consumers receive instants, no schema change — low.
- **`day_end` consumers beyond the five sites**: stage 2 re-greps
  `day_end(` before landing; any consumer that stamps (rather than
  windows) joins D3, any that windows stays on `day_end`.
- **Fixture window shape**: `journey_state`'s day facts must expose
  the window tuple for the D5 stamp computation; if the fixture hides
  it, compute from the fixture's `day_start_time`/offset via the same
  domain helpers (never the wall clock).

## 6. Stage boundaries

Stage 2 gate: pre-commit hooks + `scripts/run-quality-gates.sh cargo
nextest run --workspace` (flock deferral per the law; commit before
any wait). Full §10 suite, PR, CI watch, teardown, at-merge #893
closure: dispatcher/stage 3 per the dispatch plan.

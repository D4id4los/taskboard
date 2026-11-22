# Plan] The Today-Root Severing Sweep — Landed-State Verification & View-Root Census (Chunk A3, Round 5)

- **Date**: 2026-11-22
- **Kind**: chunk plan — verification-and-census spec for chunk A3 of
  backlog-reduction round 5 (branch `fix/today-severing-sweep`).
- **Origin**: issue #1197 (normative body); round-5 scope report
  `.artifacts/reports/2026-11-22-backlog-reduction-round5-scope-report.md`
  §A (#1197 row); dispatch plan
  `.artifacts/plans/2026-11-22-backlog-reduction-round5-dispatch-plan.md`
  §1 A3 row; the two prior recurrences: `8698fc0f` (task-view severing,
  report `.artifacts/reports/2026-11-14-task-view-window-max-width-collapse-report.md`,
  #778) and the r9 severing settle matrix chunk
  (`.artifacts/plans/2026-11-21-severing-settle-matrix-plan.md`, #1029);
  the per-variant kept-arm rung `0946b220` / `ee8cdd3d` (settle-matrix
  follow-through, unrelated to layout but the same sweep discipline).
- **Status**: RESOLVED-BY-LANDED-WORK. **The chunk's normative content
  is already in `origin/main`**: commit `1276151a`
  ("refactor(ui-slint): the view-root severing sweep + probes",
  2026-11-22 08:12, landed via the #1029 generalization) applies
  `Theme.layout-severing-max-width` at the Today root **and** at every
  previously-unsevered view root, with a `window_max_probe` twin per
  view. This plan therefore records the sweep inventory, verifies the
  landed state per root, resolves the diagnosis questions #1197 poses,
  and directs stage 2 to verification + closure rather than
  re-implementation. No source diff is expected on this branch beyond
  this plan document.

## 1. Diagnosis (resolved)

1. **Today miss mechanism** — not a fresh regression: the Today view
   root was created after the task-view precedent (`8698fc0f`,
   2026-11-14) but before any systemic rule existed; the idiom was
   applied ad-hoc per reported collapse (task view, then PlanNextDay in
   r9 chunk 7), and Today simply was never visited. #1197's own root
   statement ("Today has **no** severing max-width on its root") was
   accurate at report time and is stale as of `1276151a`, which adds
   the sever + the J4-US-3-style comment at
   `crates/taskboard-ui-slint/ui/views/today-view.slint:58-61`.
2. **Is it the #778 Slint bounded-max class again?** — Yes, in
   mechanism, though not via a new upstream surprise: the collapse
   class is exactly the bounded-max-row-collapses-root-layout-max
   behavior measured in #778 (winit applies the collapsed max as the
   OS window max-size hint). `1276151a`'s two-way TDD evidence
   reconfirms the class on main's current tree: with the sever lines
   reverted, the habit-history probe fails pre-fix with `h.max = 88`
   (bounded default-mount content). **Verdict: same #778 class;
   nothing new to report upstream.** The upstream-facing note rides as
   a comment on #778 (posted 2026-11-22) pointing at the now-complete
   per-view probe family as reproducer material; #778 stays open as
   the upstream-report tracker (p3, worked around).

## 2. View-root sweep inventory (the census)

Enumerated from `crates/taskboard-ui-slint/ui/` at `5b820f16`. The
sever is `max-width: Theme.layout-severing-max-width`
(`theme.slint:80`, sentinel value `99999px`); the probe assertion is
`h.max >= 99_999.0` after the materialization walk.

| Root (file) | Window-facing? | Sever present | Probe twin | Landed in |
|---|---|---|---|---|
| `views/today-view.slint` | yes (in `AppWindow`) | YES (:61) | `tests/today_window_max_probe*` | `1276151a` |
| `views/board-view.slint` | yes | YES (:39) | `tests/board_window_max_probe*` | `1276151a` |
| `views/stack-view.slint` | yes | YES (:23) | `tests/stack_window_max_probe*` | `1276151a` |
| `views/review-view.slint` | yes | YES (:286) | `tests/review_window_max_probe*` | `1276151a` |
| `views/settings-view.slint` | yes | YES (:651) | `tests/settings_window_max_probe*` | `1276151a` |
| `views/habit-history-view.slint` | yes | YES (:64) | `tests/habit_history_window_max_probe*` | `1276151a` |
| `views/task-view.slint` | yes | YES (:243) | `tests/window_max_probe*` (the original #778 probe) | `8698fc0f` |
| `views/plan-next-day-view.slint` | yes | YES (:79) | `tests/plan_window_max_probe*` | r9 chunk 7 |
| `views/review-walk-overlay.slint` | no — overlay `Rectangle` bound to the window's own geometry inside `AppWindow` (:1853), not a layout-merged window child | n/a — **verdict: no sever needed**; a full-stage overlay does not feed the window's max-size hint | none (by the same verdict) | — |
| `app-window.slint` (`AppWindow`) | the `Window` itself | n/a — **by design**: the Window root is the thing being protected; the sever lives on the view roots beneath it | n/a | — |
| `alarm-window.slint` (`AlarmWindow`) | its own `Window` | **no** — residual gap (see §4) | none | — |

**Stage-2 re-verification (2026-11-22, on `fix/today-severing-sweep`
rebased onto fresh `origin/main` @ `5b820f16`)**: census re-confirmed
8/8 against the live tree — all eight sever lines present at the exact
line numbers in the table above
(`grep -n layout-severing-max-width ui/views/*.slint`), the sentinel
`99999px` at `theme.slint:80`, and all eight probe twins wired as
`#[path = "probes/…_window_max_probe_tests.rs"]` modules in
`crates/taskboard-ui-slint/tests/probe_composition.rs`. Both exempt
verdicts re-checked and unchanged: `alarm-window.slint` still pins a
fixed width (`Theme.space-15 * 4`) with no sever — the residual is
tracked in #1251; `review-walk-overlay.slint` is still imported into
`AppWindow` as an overlay (`app-window.slint:25`), so the no-sever
verdict stands. No staleness race materialized: nothing moved between
`1276151a` and `5b820f16` that touches the census.

**Census summary**: 8 of 8 window-facing view roots severed, each with
a structural probe. 1 window root (`AlarmWindow`) unsevered but
fixed-size; 1 overlay root exempt by verdict; the app Window exempt by
design. **Systemic posture**: the rule of #1029 ("every view root
severs + a probe per view") is now the landed norm, not a per-bug
patch; the census note is this §2 table, to be cited by #1197's
closure comment.

## 3. Stage-2 direction (verification, not re-implementation)

1. No source change: touching the sever lines again would be
   re-implementing landed work. The branch carries only this plan.
2. Run the gate (`scripts/run-quality-gates.sh cargo nextest run
   --workspace` — includes the eight `window_max` probe tests) on the
   branch to confirm the landed state is green from this worktree too.
   Flock-deferral rule applies: if contended with nothing else
   pending, commit, report "gate deferred: lock held by <holder>",
   end the stage.
3. Open the PR (docs-only, carrying this plan). The PR body cites
   `1276151a` as the resolving commit; **at merge, #1197 closes with a
   comment citing this plan §2 and `1276151a`** (dispatcher owns
   at-merge closures). #1029 is already CLOSED and needs no touch.

## 4. Open questions → recorded decisions

- **Q: does A3 need to re-apply anything on top of `1276151a`?**
  Decision: **No.** The inventory shows full coverage of the
  window-facing view roots; duplicating the sever would be a no-op.
- **Q: is `AlarmWindow` in scope for the sweep?** Decision: **not in
  this chunk** (issue #1197 scopes view roots inside the main window),
  but it is a real residual of the same class — the window pins
  `width` to a fixed value, yet a bounded-max row in its
  `VerticalLayout` could still collapse the window's max-size hint the
  same way. Filed as `backlog:` issue (see §5) — sever + probe twin
  for the alarm window in a future small chunk.
- **Q: how is the systemic rule enforced going forward (third
  recurrence = prevention)?** Decision: the probe family is the
  enforcement for the existing roots; a new view root can still be
  created without a sever+probe. A CI-side structural guard (a grep
  census tying each `ui/views/*-view.slint` root to a sever line +
  probe file) is filed as a `backlog:` issue rather than built here
  (CI work is out of A3's S/M size class).
- **Q: goldens harness gap (#1197's coverage note)?** Decision: no
  work here — #1197's own body already rules the golden comparator
  out (fixed board-view mounts; luma-only per #1139); the probe family
  is the right host and now exists. The note rides in the closure
  comment; no separate issue (already recorded on #1197).

## 5. Backlog issues filed

- `backlog:` AlarmWindow lacks the layout-severing max-width + a
  window_max probe twin (same collapse class, fixed-width window). →
  **#1251** (filed 2026-11-22).
- `backlog:` CI structural guard asserting every `ui/views/*-view.slint`
  root carries the severing max-width and a window_max probe twin. →
  **#1252** (filed 2026-11-22).

## 6. Risks

- **Staleness race**: `origin/main` moved mid-round (`1276151a` landed
  ~the same day the dispatch plan was written); if further severing
  work lands before stage 2 runs, re-verify the census table against
  the then-current `origin/main` before opening the PR.
- **Probe oracle strength**: the probes assert the structural
  invariant only after the materialization walk on the default mount;
  a future row bounded only under a non-default state could still
  collapse a root its probe doesn't exercise. Accepted (#1029's
  landed posture); the CI guard backlog item is the systemic answer.
- **Empty-diff PR**: the branch's only content is this plan; if the
  project convention rejects docs-only PRs for code chunks, stage 2
  hands off to the dispatcher with the plan commit noted and the PR
  opened anyway (docs are PR-covered per §8).

## 7. Estimated diff size

Docs-only: this plan (~150 lines). Zero source diff. Gate cost: one
workspace nextest run (probe-inclusive).

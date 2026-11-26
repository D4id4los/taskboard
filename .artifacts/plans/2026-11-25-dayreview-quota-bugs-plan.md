# Day-Review Ratings + Quota-Kind Refresh — Chunk B2 Plan (round 8)

- **Date**: 2026-11-26
- **Chunk**: B2 of `.artifacts/plans/2026-11-25-backlog-reduction-round8-dispatch-plan.md` §7
  (scope report §B-B2, `.artifacts/reports/2026-11-25-backlog-reduction-round8-scope-report.md`).
- **Branch**: `fix/dayreview-quota-bugs` (off `origin/main` @ `526be42a`, which IS the #1475
  merge — the fetch note in the dispatch prompt resolved: #1475's merge commit is `526be42a` itself).
- **Covers**: #1431 (the day-review walk never collects mood/energy ratings — the record path
  hardcodes them), #1432 (the quota-kind switch doesn't refresh the pushed model).
- **Acceptance** (normative): both pinned red-first (a rating change persists; a kind switch
  re-renders); diffs confined to the named surfaces.
- **UX law**: the dispatch plan rules B2 "no UX plan" (bug fixes); the copy ruling the
  review-walk journey's D-W9 deferred ("a future walk extension needs its own copy ruling")
  is supplied by the dispatch plan itself ("restore the collection per the day-review
  journey's original grammar; the journey amendment cites this fix"). Stage 2 appends the
  journey amendment to `.artifacts/plans/2026-11-14-review-walk-reflection-ux-plan.md`.

## 1. Defect A — #1431, the walk's hardcoded ratings

### Diagnosis (current main, worktree line numbers)

- Domain is complete: `RecordReflection { cadence, day, window, text, mood: Option<u8>,
  energy: Option<u8>, flags }` (`crates/taskboard-domain/src/messages.rs:657–672`, the
  planner mints the `Rating`s); `ReflectionEntry`/`ReflectionAttributes` live
  (`crates/taskboard-domain/src/reflection.rs:291–330`).
- The record seam hardcodes the attributes: `WalkIntent::Record` dispatches
  `mood: None, energy: None, flags: ∅` verbatim
  (`crates/taskboard-ui-slint/src/adapter.rs:4375–4379`).
- The walk UI never asks: the Prompts stage (`stage-kind == 2`) renders one
  `MultiLineEditor` per template prompt and nothing else
  (`crates/taskboard-ui-slint/ui/views/review-walk-overlay.slint:224–236`); the `Walk`
  view-state level holds only `answers: Vec<String>`
  (`crates/taskboard-ui-slint/src/view_state.rs:1035`, mirrored by the
  `on_walk_answer_edited` seam at `adapter.rs:18631`).
- History: the original walk plan (`2026-11-06-review-walk-plan.md`) hardcoded the Nones
  in D-W4 and deferred attributes in **D-W9** ("the walk records text only; attributes stay
  the CLI's … a future walk extension needs its own copy ruling"). The round-8 dispatch plan
  supplies that ruling. The ratings remain editable elsewhere (the T-15 inline journal
  editor, `ui/views/review-view.slint:547–579`; its rating-draft parse grammar at
  `adapter.rs:18440–18463` — `parse::<u8>()` + `Rating::try_new`, blank = clear, an invalid
  draft never dispatches).

### Fix shape (diffs confined to the walk surfaces)

1. **View-state**: extend the `Walk` level with two rating drafts (`mood: String`,
   `energy: String`, empty = unrated), sharing the answers' lifecycle — cleared on walk
   close/record exactly where `answers` is cleared (the walk facts' reset sites).
2. **Overlay** (`review-walk-overlay.slint`, prompts stage only): after the prompt rows, a
   two-field rating row using the T-15 grammar verbatim — `TextField`s labelled
   "Mood (1-5, empty clears)" / "Energy (1-5, empty clears)", placeholder
   "1..=5 · empty = clear", validity mirrors. New callback
   `walk-rating-edited(int, string)` (kind 0 = mood, 1 = energy). No new widgets (the house
   widget policy; the editor's existing grammar is the precedent).
3. **Adapter seams**: `on_walk_mood_edited` / `on_walk_energy_edited` (`[bare_mut]`, the
   `on_walk_answer_edited` shape) storing the drafts. `WalkIntent::Record` parses each draft
   with the T-15 parse (blank → `None`, valid → `Some(byte)`) and feeds the payload instead
   of the hardcoded `None`s. **Refusal semantics**: an invalid rating renders `Invalid`
   validity and the record dispatches nothing (the T-15 law — an invalid draft never
   dispatches); the walk stays on the Prompts stage. The all-blank-text gentle end is
   unchanged and stays legal with or without ratings.
4. **Journey amendment**: append a dated note to
   `.artifacts/plans/2026-11-14-review-walk-reflection-ux-plan.md` — the D-W9 deferral is
   superseded by the round-8 dispatch ruling; ratings are collected in the Prompts stage,
   optional, blank = unrated; cites this plan and #1431.

### Red-first pins

- **The behavior pin (red pre-fix)**: extend the existing J14 test
  `recording_routes_one_entry_and_clears_the_card`
  (`crates/taskboard-ui-slint/tests/journeys.rs:17818`) — its trailing
  `assert_eq!(*mood, None)` block is a pin-of-today (D-W9's letter). Desired semantics: the
  test ALSO sets a rating via the new seam before `ReviewRecord`, and asserts
  `mood == Some(4)` (energy `None`). Pre-fix this is red (hardcoded `None`; the seam
  doesn't exist). Its mood/energy-`None` half moves to the new test below — the assertions
  are split by behavior, not weakened.
- **The optional pin**: new test — an unrated walk still records `mood: None, energy: None`
  (ratings are optional; the text-only record stays legal).
- **Persistence reading**: "a rating change persists" is asserted at the lowest layer that
  owns it — the drained command payload (the engine mint + storage round-trip is already
  covered by the domain/CLI tier; per the one-behavior-one-test law the UI journey pins the
  dispatch, not the engine).

## 2. Defect B — #1432, the quota-kind switch's dead model

### Diagnosis (current main, worktree line numbers)

- The seam `on_habit_quota_kind_picked`
  (`crates/taskboard-ui-slint/src/adapter.rs:14805–14828`) is a **pure draft mutation**: it
  maps the segment index to `PeriodKind::Day/Week/Month` and re-seats `draft.target` — then
  returns. No `finish_mutation`, no `refresh_forced`, no re-push.
- The pushed half lives in `push_task_detail` → `window.set_habit_model(...)`
  (`crates/taskboard-ui-slint/src/models.rs:3061`; `quota_kind`/`kind` both derive from
  `draft.quota_kind_index()` at `models.rs:3072,3078`), which only re-derives inside
  `models::push` (called from the adapter's sync pass, `adapter.rs:3421`). The
  SegmentedControl's `selected-index` and the kind-gated rows (weekday strip, month-day
  field) read the pushed model — so nothing repaints until an unrelated pass happens to run.
- The in-repo precedent for exactly this class: the reminder override's seams end with
  `a.refresh_forced(&window)` plus the comment "the draft moved outside the state Arc — the
  mirror pass re-derives" (`adapter.rs:~14932–14960`, #1035/W-13). The kind seam predates
  that discipline being applied to it (the r9-era models-pass wiring gap).

### Fix shape

End `on_habit_quota_kind_picked` with `a.refresh_forced(&window)` — the reminder-seam
precedent, one call inside the seam body. The pass re-pushes `habit_model` (segment index,
gated-row visibility, schedule slots) and the preview; no model code changes, no save-gate
changes. Diffs confined to the seam body (plus its test). The draft mutation itself (the
`PeriodKind` map, the day-carried-inert D5 decision) is already correct and untouched.

Out of scope (→ backlog **#1485**): the multi-second tail (the no-timeout Nextcloud HTTP
client, `client.rs:428`; persist-before-interpret, `crates/taskboard-state/src/actor.rs`).

### Red-first pin

New journey test beside the nudge-preview tests (`habit_surface_journeys.rs`, the
`us6_open_editor` + `invoke_habits_edit_asked(0)` harness shape):

1. Open the editor; `advance_time(0)`.
2. `invoke_habit_quota_kind_picked(1)`; `advance_time(0)`. **No explicit
   `harness.handle.refresh()`** — that is the trap: the existing preview tests call
   `refresh()` after their invokes (their seams already self-refresh, so the harness
   refresh masks nothing); here the harness refresh WOULD repaint the model and make the
   pin pass pre-fix. The pin must rely on the seam's own pass only.
3. Assert `window.get_habit_model().quota_kind == 1` (and, structurally, the PerWeek row's
   visibility bit flipped if exposed on the model).

Pre-fix: the pushed model still reads kind 0 → red. Post-fix: the seam's `refresh_forced`
re-pushes → green.

## 3. Stage-2 work list

1. Branch state: this plan + the two pins' test code land first (`test(ui):` commits), run
   to red via `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe;
   every cargo call through `scripts/memory-gate.sh`).
2. Defect B fix (one seam line + comment), pin 2 green.
3. Defect A fix: view-state drafts → overlay fields → seams → `WalkIntent::Record` parse;
   pins 1+3 green; the J14 extension replaces the today-pin per §1.
4. Journey amendment appended to the review-walk journey plan (`docs(plan):` commit).
5. Commit hygiene: `<type>(<module>): <desc>` subjects, each commit standalone.
6. Hand off at: pre-commit hooks + the flock-wrapped nextest run green; the coverage trio
   and the CI watch are the dispatcher's (docs/agent_dispatch.org) — gate deferral law
   applies ([D]).

## 4. Stage-3 traps

- **Do not weaken J14**: splitting its `None` assertions into the unrated-walk test is a
  behavior split, not a weakening; the rated path must assert `Some`.
- **No text assertions**: validity and refusal are asserted as typed/structural facts
  (`FieldValidity::Invalid` on the model, the drained-command slice), never field label or
  notice text.
- **No quality ceilings / no pins of today's behavior**: the unrated-`None` test asserts
  desired optionality semantics, not "current behavior".
- **Diff confinement**: no changes to `models.rs`, `push_task_detail`, the save gate, or
  any other editor seam; no new `ActionId`s (the walk seams are direct window callbacks,
  the `on_walk_answer_edited` shape); kiosk paths untouched (the walk is desktop-only and
  stays so).
- **Refusal semantics drift**: if stage 2 is tempted to make an invalid rating
  auto-clear-and-record, stop — the T-15 law (an invalid draft never dispatches) is the
  recorded default; deviating is a design question, escalate instead.
- **Red-first honesty for pin 2**: any test-side `handle.refresh()` between the pick and
  the assert voids the pin; review for it explicitly.
- **Gate discipline** ([D]): pre-commit hooks + flock-wrapped `scripts/run-quality-gates.sh
  cargo nextest run --workspace`; background + poll beyond ~5 minutes; commit landed work
  before any wait; on lock contention with no other work, commit and report
  "gate deferred: lock held by <holder>".

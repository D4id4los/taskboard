# [Plan] UX Remediation — Chunk 1: completability (the B-a first pass)

- **Date**: 2026-11-15
- **Kind**: stage-1 chunk plan for chunk 1 of the UX remediation phase
  (framework plan §6 phase 5; the framework's dress rehearsal, first
  code-bearing chunk). Planning only — no application code in this
  stage.
- **Origin**: the remediation dispatch plan
  `.artifacts/plans/2026-11-15-ux-remediation-dispatch-plan.md` §1/§2
  (the shared execution model and the dress-rehearsal mandate are
  binding); the accepted triage report
  `.artifacts/reports/2026-11-14-ux-retrofit-triage-report.md` §3 rows
  T-2 and T-7 and §4 (the B-a sublist); home issues #785 (T-2) and
  #790 (T-7); the owning UX plans `2026-11-14-board-stack-management-ux-plan.md`,
  `2026-11-14-habit-family-ux-plan.md`,
  `2026-11-14-capture-search-ux-plan.md`,
  `2026-11-14-recurrence-family-ux-plan.md`,
  `2026-11-14-timers-alarms-ux-plan.md`; the census
  `docs/ux-surface-census.org` (Board, Today, Task detail, Capture
  slices + the alarm-window block); `docs/ux_planning.org` §2.
- **Status**: READY — preconditions verified by the dispatcher (triage
  report merged at `main` @ `3a2edea0`, accepted by the user
  2026-11-15); worktree `../taskboard-WT-fix-ux-r1-completability` on
  branch `fix/ux-r1-completability` with the dispatch plan committed.
- **Binding inputs of stage 2**: every decision D-1..D-7 below and the
  commit sequencing §5. Deviations need a recorded reason in the
  stage-2 report.

## 1. Posture (inherited, restated for the chunk)

This is the first remediation chunk: unlike the phase-4 retrofit plans,
the amendments below prescribe **changed journeys**, not journeys-as-
implemented. The dispatch plan §2 acceptance is the definition of done:
both B-a journeys complete end-to-end (unarchive from the GUI; every
listed kiosk journey **visibly** gated), amended plans gate-green,
census updated, full AGENTS §10 gates green. The user's acceptance of
the triage report settles T-7's recorded doubt: **the silent part is
the defect** — kiosk gating becomes visible (disabled affordance or
hint), it is not removed, and no new kiosk capabilities appear.

## 2. Decisions

### D-1 — T-2 (#785): the unarchive affordance is a board-bar-mounted archived-boards roster

Two candidate shapes were before stage 1 (dispatch §2): a
filtered/archived section in the board bar, or a Settings-managed
archived-boards roster. **Decision: a roster mounted in the board bar**
— a trailing archive indicator that opens a dropdown of archived
boards. Reasons:

- The board bar is the board's home surface; board lifecycle verbs
  belong next to board navigation (consistency & standards: the bar
  already owns board selection, the segment context menu, and the
  compact switcher dropdown — one surface, one verb family).
- Settings is the connections/config surface; a Settings roster would
  scatter a board verb across surfaces (the G4 pattern the framework
  exists to kill) and force the user to know to look there — the same
  recognition failure as the CLI-only status quo.
- The exclusion of archived boards from the **segments** stands (the
  triage's letter: "the board bar's exclusion stands for normal
  navigation"); the roster is a *separate affordance*, not a filter on
  the live segments — no live-bar ambiguity about whether an archived
  segment is selectable.

Concrete shape (all existing seams unless named):

- **Visibility rule**: the indicator mounts only when the archived set
  is non-empty (`AppState.boards` filtered `b.archived && !b.deleted`).
  No dead affordance when nothing is archived; a non-empty archive is
  itself the honest signal. Archived boards stay excluded from
  segments, dropdown rows, and the board walk (`BoardNext`/
  `BoardPrevious` never land on them; `view_state.rs` normalization
  unchanged).
- **Roster rows**: title-only rows (the `BoardSegmentModel` posture —
  a display model, no archived field on segments; the roster's rows
  carry only the title). Activating a row dispatches
  `StateCommand::SetBoardArchived { board, archived: false }`
  (`messages.rs:179` — **exists**, no domain delta) + `RefreshNow`,
  then selects the restored board (the same select-after-mutate
  posture as the preset flow). `SetBoardArchived` is journal/undo-
  eligible (`undo.rs:324`), so `AppUndo` is the escape path.
- **No new `ActionId`**: the roster is a dropdown of rows; keyboard
  parity rides the shared dropdown's existing keyboard walk (the
  compact switcher's row pattern, `app-window.slint:914` precedent).
  A global `BoardUnarchive` action id would add a second entry point
  the journey does not need; recorded as a non-goal (chunk 4 owns the
  board-verb entry-point asymmetry family).
- **CLI stays**: `boards archive` / `unarchive` (`cli.rs:375-381`)
  unchanged; the GUI gains the missing half (US-10/S2's lockout
  resolves). Archive-from-GUI is **not** in this chunk (not a B-a
  finding; chunk 4 owns the board-verb asymmetry family) — recorded
  non-goal, no new backlog issue needed (the finding is homed by the
  dispatch plan's chunk 4).

### D-2 — T-7 (#790): the visible-gating grammar is "disabled affordance at the mount site", hint only where the surface itself does not exist

The acceptance settles the posture: gating becomes visible. Decision
per family — the census's own precedent ("kiosk greys in
`entry_states`", the `review.open` cell) is generalized:

| Family (finding ids) | Mount site today | Becomes | Why |
|---|---|---|---|
| Habit row actions (HB-6, D-T16) | `editing-visible` mount gate empties the buttons (`habits.slint:36,108,121,205,250`; `presentation.rs::habit_editing_visible`) | the flag becomes `editing-enabled` (same `!kiosk` derivation, renamed for honesty); the +1/reorder/repair/convert/create affordances **render disabled** (Theme disabled tone, non-interactive) instead of not mounting; the create editor flow stays unmountable (a disabled trigger, not a dead editor) | the rows already render; a greyed +1 reads as "exists, not for this device" — the display/control split becomes legible |
| Capture/focus toggles (CF-W-N4) | kiosk dispatch no-op (`adapter.rs:2846-2852`); menu rows exist but activate nothing | the menu rows for `FocusModeToggle` (and the kiosk-refused capture twins) render **disabled** on kiosk via the same `entry_states` greying the census records for `review.open`; the dispatch no-op stays as layer 3 | two existing layers (entry gate, dispatch gate) already agree; the fix is purely the *visual* layer — the menu row must look as dead as it is |
| Recur row actions (RC-W-N2) | the recur row's pause/edit affordances are model-side desktop-mounted (`task-view.slint:95-103,175-185`) | the recur row **renders** on kiosk (pattern + paused-chip display halves stay) with the pause/edit buttons disabled; the pause-confirm strip stays unmountable from a disabled trigger | same display/control split as habits; the badge already renders — the row completing its display half while its buttons grey is the honest posture |
| Alarm window (TA-W-N4, R3.5.3 fence) | never mounts on kiosk (`lib.rs:322-330`) | **a hint, not a disabled affordance**: the Today timers section (which renders on kiosk as the display half) gains a static muted caption signalling that alarms live on the interactive desktop surface; the fence stands, no window mounts, zero new commands | there is no widget to disable — the surface itself is fenced; an honest absence signal is the only shape that does not breach the R3.5.3 appliance posture |

General rule (the grammar the census will carry): on kiosk, an
existing widget at the mount site renders **disabled**; an absent
surface gets a **muted hint** on its display-half host section; both
are display-state only — no command variant, no `UiRequest`, no
UI-side logic (the flags derive Rust-side from `settings.kiosk`, the
existing `habit_editing_visible` pattern). This is decided, not open:
the dispatch plan's letter ("disabled affordance or hint") is resolved
as *disabled where a widget exists, hint where the surface is fenced*.

### D-3 — Platform-qualification format (census grammar amendment)

The census cells already qualify platforms inside cells (`E (desktop)`,
`E (global)`). The amendment adds one token, never a new column:
`E (desktop; kiosk: disabled)` for a control whose kiosk twin is a
disabled affordance, and `D (both; kiosk: hint)` for display halves
that carry the absence hint (alarm window's Today timers cell). The
`—` and `U` vocabulary is untouched. Every amended cell keeps its
evidence anchor and gains the amended-plan step id (`US-n/S-k` of the
owning plan), so `scripts/ux-coverage.sh` resolution is unchanged.

### D-4 — Amended journey steps per owning plan (the remediation design)

- **board-stack-management** (T-2): new story **US-11 — Unarchive a
  board from the GUI** (P1, desktop): S1 open the roster (indicator
  click; escape: close the dropdown, nothing dispatched); S2 activate
  the row → `SetBoardArchived { archived: false }` + `RefreshNow`;
  S3 the board rejoins the bar and becomes selected. Escape column:
  typed engine refusals through the outcome lane; undo via `AppUndo`
  (`undo.rs:324`). US-10/S2's system-response and escape cells are
  rewritten (the lockout sentence retires to history: the GUI path now
  exists, step US-11/S2). Capability-matrix `board.archive` Board cell:
  `U (BF-2)` → `E (US-11)`. Wiring-inventory rows: the indicator +
  roster (callback `archived-roster-activated(int)`, adapter seam,
  `SetBoardArchived`, journey steps US-11/S1–S3, test
  `us11_s2_roster_row_unarchives_and_selects`).
- **habit-family** (T-7 arm): US-1/US-3–US-5 platform notes change from
  "actions gated (do not mount)" to "actions disabled on kiosk"
  (`E (desktop; kiosk: disabled)` cells); each affected step's
  system-response column gains the disabled-tone sentence; wiring rows
  unchanged in contract (same callbacks) — the rows note the
  mount→disabled change as the reason the step's kiosk leg now renders.
- **capture-search** (T-7 arm): US-7 platform note and the
  `app.focus_toggle` cell gain `E (global; kiosk: disabled menu row)`;
  the step's kiosk leg records the greyed row + the unchanged no-op
  dispatch (layer 3 stands).
- **recurrence-family** (T-7 arm): US-1/US-2 platform notes and the
  recur cells gain the disabled posture; the recur row's kiosk leg
  renders pattern + chip + disabled buttons.
- **timers-alarms** (T-7 arm): the alarm-window stories gain the
  recorded absence posture: `D (both; kiosk: hint)` on the Today
  timers display cell; a wiring-inventory row with **no command
  variant** and the reason "pure display caption (framework §2.1.6
  reason law)".

Command-contract delta across all amendments: **zero new
`StateCommand`/`UiRequest`/`ActionId`** (D-1: `SetBoardArchived`
exists; D-2: gating is display-state). This is the delta section's
content in each amended plan.

### D-5 — Census updates

Ride the implementation commits (dispatch-plan mandate): the Board
slice's `board.archive` cell, the Today habit cells, the Capture
`app.focus_toggle` cell, the Task detail recur cells, and the
alarm-window block's Today timers cell all gain the D-3 tokens and
the amended-step citations; each slice's Kiosk note (e.g. the verbatim
seed posture at the Today slice, `habits.slint:7-10`) is superseded by
a dated pointer to the amended owning plan. No ruling is overturned —
the R3.5.3 fence and the R-1/R-2 rulings stand.

### D-6 — Test catalogue (TDD-first; typed outcomes, never text)

| # | Layer | Test | Asserts |
|---|---|---|---|
| T1 | presentation | `habit_editing_flag_disables_not_hides_on_kiosk` (renamed successor of `habit_editing_visible`, `presentation.rs` tests) | the typed flag: `!kiosk` → enabled, `kiosk` → disabled (bool, `matches!`-style) |
| T2 | seam (journeys.rs, driver) | `us11_s2_roster_row_unarchives_and_selects` | driver drives the roster callback → the dispatched command is `StateCommand::SetBoardArchived { archived: false }` + `RefreshNow`, and the view selects the board |
| T3 | seam | `us11_s1_roster_absent_while_no_board_is_archived` | structural: the bar model's roster set is empty ⇒ no indicator (model length, not text) |
| T4 | kiosk gate (the `the_kiosk_capture_never_samples` / `the_kiosk_mount_still_lands_on_the_board` shapes) | `the_kiosk_habit_actions_render_disabled_not_absent` | kiosk profile: the habit section model still carries the rows AND the editing flag is false (disabled posture), desktop profile: true |
| T5 | kiosk gate | `the_kiosk_focus_toggle_row_greys_and_still_no_ops` | kiosk: menu model's focus row carries the disabled flag; activating still yields zero effects (`adapter.rs:2846-2852` arm unchanged) |
| T6 | kiosk gate | `the_kiosk_recur_row_renders_with_disabled_actions` | kiosk: recur row display fields present, the action-enabled flags false |
| T7 | kiosk gate | `the_kiosk_alarm_fence_stands_with_the_absence_hint` | kiosk: alarm lanes `None` (`lib.rs:322-330` unchanged) AND the Today timers model carries the hint flag; desktop: lanes mounted, flag false |
| T8 | gate | `scripts/ux-coverage.sh <each amended plan> --gate` | every new/changed wiring row FOUND |

Presentation flags are the tests' oracle — no label text, no log text,
no screenshots-for-behavior (golden visuals may pin tone rendering
separately, as today).

### D-7 — Commit sequencing (stage 2; every commit standalone, rebase-clean)

1. `feat(ui): the archived-boards roster unarchives from the board bar`
   — D-1 implementation + T1–T3 + board-stack plan amendment + Board
   census cell (stage 2 amends plans in the same commit as the code
   that makes them true; the gate run closes the commit).
2. `fix(ui): kiosk gating renders disabled affordances and absence hints`
   — D-2 implementation + T1's rename rides commit 1 only if D-1
   touches `presentation.rs`; otherwise the flag rename + T4–T7 here,
   plus the four T-7 plan amendments + census cells.
3. `docs(census): the ux-r1 completability cell updates` — only if any
   census edit cannot ride 1–2 without tangling unrelated slices;
   default is ride-along (the dispatch mandate), this commit is the
   fallback, expected empty.
4. `chore(ui): gate-green pass` — not a commit; the gate + §10 suite
   runs per commit, the branch hands off at PR-green per the shared
   execution model.

Stage 3 re-runs the gate, executes the walkthrough protocol over
US-11 and each gated kiosk step, and holds the branch to the T-46
doctrine bar (#829) — in particular BF-W-N2 (destructive rows last,
greys disabled): the new disabled affordances must match the recorded
grey tone, not invent one.

## 3. Non-goals (recorded)

- No archive-from-GUI verb (chunk 4, T-8 family); no new kiosk
  capabilities; the R3.5.3 alarm-window fence stands (only the hint
  lands); no T-1 confirmations (chunk 2); no new `ActionId`s or
  command variants (D-1/D-4).

## 4. Risks

- **R-1**: the disabled-tone treatment must reuse the Theme's existing
  disabled tokens (BF-W-N2 bar) — a new primitive is a chunk-plan
  deviation; stage 2 stops and records if the tokens cannot express
  "disabled but visible" on kiosk's passive displays (contrast at
  glance distance).
- **R-2**: `BoardBarFacts`/golden visuals may pin the bar's segment
  set shape; the roster changes the bar's model — golden re-baselining
  is expected and stays inside commit 1.
- **R-3**: the `editing-visible` → `editing-enabled` rename touches
  the pilot habit-create plan's recorded D-T16 posture text; the
  amendment supersedes it with a dated pointer (D-5 mechanism), never
  edits the pilot plan in place.

## 12. Plan corrections

- **PC-1 (2026-11-15, chunk-6 stage 1)**: §2's D-1/D-4 claim
  "`SetBoardArchived` is journal/undo-eligible (`undo.rs:324`)" is
  **false** — the command falls in the BoardCeremony arm
  (`UndoFacts::NotUndoable(NotUndoableReason::BoardCeremony)` at
  `main` @ `d7a8173c`), so the unarchive's escape is the re-archive
  path, not `AppUndo`. The shipped behavior (this plan's unarchive
  seam, the r1 PR) is unaffected — the defect was in this plan's
  justification text only. Erratum homed on #842; the board-stack
  plan's carriage records the same correction as its PC-5.

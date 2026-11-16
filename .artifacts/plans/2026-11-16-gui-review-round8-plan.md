# [Plan] GUI Review Round 8 ("ux-r8") — Stage-1 Round Plan

- **Date**: 2026-11-16
- **Kind**: stage-1 round plan — the round's authority record, family
  map, ruling/collision ledger, intake coverage table, and dispatch
  chunking proposal. Companion artefacts: the eight UX journey plans
  listed in §2, each carrying the journeys, wiring contract, and
  command deltas for its family. No implementation decisions beyond
  the command contracts.
- **Origin**: the operator's round-8 review notes (recorded §1 below —
  they are the round's authority; anything in the intake list the
  notes already rule on is settled by the notes) plus the round-8
  intake: set A (the eight still-open `[ux-triage T-n]` issues
  #807/#809/#811/#812/#813/#819/#820/#823) and set B (the issues the
  2026-11-16 backlog cleanup deliberately skipped for this wave — the
  Settings-Files family, the accessibility set, the standalone-friction
  set, the boundary-machinery set, and the framework-debt riders).
- **Status**: ACCEPTED — the user accepted the round-8 plan set (this
  plan + the eight companion UX plans, all flipped to ACCEPTED) on
  2026-11-16; the framework is **mandatory** (phase 6 activated
  2026-11-16), so dispatch may chunk from these plans directly. The
  chunking proposal (§7) is the dispatch input.
- **Verified against**: `main` @ `1ffe9e64`. Anchors cited in the
  companion plans were read at this HEAD (symbol-level unless a line
  is stated); expect drift by implementation time — re-anchor by
  symbol, the findings are stable.
- **Sources read (normative order)**: `AGENTS.md`;
  `docs/ux_planning.org` (incl. §2.6 doctrine bar, §3 activation —
  phase 5 complete 2026-11-15, phase 6 complete 2026-11-16); the
  triage report
  `.artifacts/reports/2026-11-14-ux-retrofit-triage-report.md`; the six
  chunk plans `2026-11-15-ux-r{1..6}-*-chunk-plan.md` (already-
  retrofitted territory — not duplicated); the format exemplars
  (`2026-11-13-r7-*-ux-plan.md`) and the nine family plans
  (`2026-11-14-*-ux-plan.md`); the census `docs/ux-surface-census.org`;
  the day-boundary plan
  `.artifacts/plans/2026-10-21-daily-driver-day-boundary-plan.md`; the
  2026-11-16 cleanup triage report (set B provenance); the operator's
  design reference screenshot
  `/home/viktoria/Pictures/Screenshots/Screenshot_20261114_144156.png`
  (SuperProductivity context menu: icon quick-actions row, ▸ submenus,
  per-row key badges).

## 1. The round-8 notes (authority record)

The operator's notes, recorded as N-1..N-21 (light copy-edit only;
wording is the operator's). Each companion plan cites its notes by
these ids; this section is the citation target.

- **N-1**: In Task view and Settings view, navigating focus with
  C-f/b/n/p (emacs mode) does not work. It works in Board and Today.
- **N-2**: M-< / M-> do nothing either; in the *task* view even
  running the action through the main menu does nothing — contrast the
  board view, where the menu entries work and only the shortcuts fail.
- **N-3**: Closing the due/start date picker widgets resets focus to
  the top of the window. General rule for all pickers and pop-up
  windows: focus remains on the element active before the picker
  opened (here: the due/start field). Scan every widget that opens a
  picker or similar overlay and make sure focus is retained after
  closing.
- **N-4**: Picking a bare day (e.g. today) as due date without a time
  makes the system calculate overdue from 00:00 — wrong. It should use
  the configured day end for any due-date calculation, letting the due
  instant wander into the early morning of the next day when the day
  end is set specially, and 23:59:59.999 when no alternative end is
  configured.
- **N-5**: Clicking with the mouse into the HH:MM field of the
  date-time picker closes the entire picker instead of navigating to
  and activating the text fields.
- **N-6**: In the date-time picker quick options dropdown, the
  dropdown should auto-scroll to the current time — one rarely sets a
  time other than the current time.
- **N-7**: The task right-click menu in board view needs a quick
  actions bar at the top with icon-buttons for: schedule today,
  schedule tomorrow, unschedule, Start/Stop tracking (shown depends on
  context). These actions need shortcuts. Design reference:
  SuperProductivity (screenshot above).
- **N-8**: The task right-click context menu is too long — all actions
  including rare ones. (a) Do not show options that don't make sense
  in context (a "Stop Tracking" on a non-tracking task makes no
  sense). (b) Add submenus extending from the context menu like the
  main menu already has; group less-used functions there (e.g. the
  convert functions).
- **N-9**: When adding a new task in board view, the newly created
  task should receive focus.
- **N-10**: We need a way to reorder tasks within stacks in board
  view — mouse drag-n-drop AND a keyboard movement option.
- **N-11**: When a task context menu on the board is left open for
  multiple seconds, its contents eventually get replaced with the main
  menu contents, for an unknown reason.
- **N-12**: Task cards in board view should have a play/pause button
  that starts/stops tracking that task.
- **N-13**: In board view's narrowest layout, the add-task input field
  grows very large vertically (not in wider layouts). Fix.
- **N-14**: We need a menu somewhere to configure the colour of tags.
  The whole tag feature in the GUI needs a GUI design phase following
  the new standards.
- **N-15**: We need to reorder tasks within stacks (duplicates N-10's
  first half; recorded once) — and the checkbox marking a task done is
  missing a hover effect in board view.
- **N-16**: In the narrowest layout there is no mouse-only way to move
  tasks between stacks. Find an appropriate mechanism for mouse-only
  users.
- **N-17**: M-< / M-> (emacs mode) do nothing in board view while the
  bound actions work via the menu. Possibly the key parser fails to
  detect them on the operator's layout (Dvorak International: Alt +
  Shift + ,/< and Alt + Shift + ./>). No certainty — a thought.
- **N-18**: The "Files sync running !" message is formatted as a
  warning and never goes away. Either it is a warning (indicating a
  setup problem) or it is not, and should not be formatted as one —
  it feels like it should live with the sync status indicator. What is
  it meant to convey?
- **N-19**: For the two boards with Nextcloud Files sync enabled, the
  indicators show "Syncing"/"Idle" as if regularly syncing, but the
  pending-changes count only goes up (165 "Household", 8 "Tech
  Stuff"), and only an `identity.json` per board exists in the
  Nextcloud folder — no other sync files written. Investigate.
- **N-20**: In task view, Task Name/Title and Task description fields
  lack focus indicators — they look like gaps in the focus walk.
- **N-21**: In task view, actions — set due date, set deadline (the
  scheduled field), set custom reminders, edit description, clock in,
  clock out — should get keyboard shortcuts; for emacs, org-mode
  defaults such as C-c C-s (start/scheduled) and C-c C-d (due).
- **N-22**: The board view in narrow layout lacks a good mouse-driven
  board switcher; there once was a board-selection dropdown — replace
  the Board Title display with a dropdown of similarly formatted board
  titles that switches on click.
- **N-23**: Multi-select for bulk actions: Shift+Click selects the
  range between the focused element and the clicked one; Ctrl+Click
  toggles the clicked element; drag-n-drop moves all; commands
  (context menu, shortcuts) apply to all selected when that makes
  sense; the multi-selection context menu shows only multi-sensible
  actions, perhaps with a count note at the top. "Would that be an
  improvement to usability or not?"

## 2. Family map — the round's companion UX journey plans

| Plan (all `DRAFT`) | Notes | Intake carried |
|---|---|---|
| `2026-11-16-board-interaction-ux-plan.md` | N-10/N-15 (reorder), N-16 (compact cross-stack move), N-22 (compact board switcher), N-23 (multi-select) | #823 (T-40, folded), #834 |
| `2026-11-16-task-card-controls-ux-plan.md` | N-7 (quick actions), N-8 (menu restructure), N-9 (focus on create), N-11 (menu-swap bug), N-12 (card play/pause), N-13 (compact input growth), N-15's hover half | #811 (T-28 — note wins) |
| `2026-11-16-task-detail-editor-ux-plan.md` | N-4 (day-boundary due), N-5 (HH:MM click), N-6 (quick-options autoscroll) | #838, #759, #706, #840 |
| `2026-11-16-focus-restoration-ux-plan.md` | N-3 (the general focus-restore rule), N-20 (focus indicators) | #668, #669, #670, #724 |
| `2026-11-16-keymap-parity-ux-plan.md` | N-1 (C-f/b/n/p parity), N-2/N-17 (M-< M-> + Dvorak parsing), N-21 (task-action shortcuts) | — |
| `2026-11-16-files-sync-status-ux-plan.md` | N-18 (chip reclassification), N-19 (sync-pending investigation) | #741, #743, #745, #748, #749, #750, #746, #756 |
| `2026-11-16-tag-color-ux-plan.md` | N-14 (tag colour configuration + the tag GUI design phase) | #839 |
| `2026-11-16-cross-surface-friction-ux-plan.md` | — | #753, #832, #836, #772, #763, #712 |

Every note maps to exactly one owning plan; every intake issue maps to
exactly one disposition below (§4). No note or issue is dropped.

## 3. Ruling & collision ledger (the round's recorded decisions)

The triage rulings and the r1–r6 remediations are settled history;
nothing here re-litigates them. Where a note and a recorded ruling
collide, **the note wins** and the collision is recorded here with a
reason; the owning plan carries the dated supersession note and the
census diff. Where no note touches a standing deferral, the deferral
stands untouched.

| # | Collision / disposition | Ruling | Reason |
|---|---|---|---|
| C-1 | **T-28 / #811** (r6 D-6 deferral: card start/stop is feature work) vs **N-12** (card play/pause button) | **Note wins.** The deferral is superseded by the operator's explicit request; the affordance is planned (`task-card-controls` US-4). The census `task.track_start/stop` Board cells `U (TE-3)` → `E` on landing; #811 closes with the PR. | The operator's round-8 note is the newer authority; r6's own deferral text anticipated exactly this ("if ever scheduled, it owns a UX journey plan first" — this round's plan is that journey plan). |
| C-2 | **T-40 / #823** (r6 D-18 deferral: unlabeled move-aim badge) vs **N-10/N-16/N-23** (reorder, compact move, multi-drag — all redesign move feedback) | **Folded.** The move-interaction work owns the aim feedback design; the badge's dual purpose gets a designed, discoverable signal instead of a bolt-on caption (`board-interaction` US-5). #823 closes via that work. | A deferral whose subject is being redesigned wholesale should not be patched separately in the same round — one design, one owner. |
| C-3 | **r3 D-5 scope line** ("no free-form hex in the GUI; CLI covers it") vs **N-14** (configure tag colours) + **#839** | **Note wins.** The free-form colour field enters through the domain's fallible `Color` minting constructor (the A1 pattern — no UI-side re-derivation), retiring r3's scope line with a dated note (`tag-color` plan §2 R-1). | r3's reason was "no GUI validator duplication"; the A1-constructor route satisfies it while answering the request. |
| C-4 | **N-8a** (hide context-inapplicable rows) vs the **BF-W-N2 doctrine bar** ("grey disabled verbs", `ux_planning.org` §2.6) | **Complementary, not an overturn.** Grey = the verb applies to this subject but a precondition blocks it now (stays greyed). Hidden = the verb does not apply to this subject's state at all (row withheld). Both rules recorded together in the task-card-controls plan. | The doctrine bar governs enablement *within* the applicable set; N-8a narrows the applicable set itself. A regression against BF-W-N2 remains a review finding. |
| C-5 | **N-21's suggested C-c C-s** (scheduled edit) vs the **existing emacs `C-c C-s` = ReminderSnooze** (Global, `presets.rs:425-431`) | **Collision recorded, resolution rule fixed.** The candidates are org-mode-shaped but the chord space is occupied; stage 2 resolves via the keymap contract suite — either context precedence (TaskDetail chord wins while the detail holds; the Global row keeps its stroke elsewhere) or a recorded retune of ReminderSnooze to a free emacs chord. Never a silent drop of a preset row; any retune regenerates the keymap snapshots and is recorded. | The no-second-class contract: a collision is re-picked, never weakened away. |
| C-6 | **T-24/#807, T-26/#809, T-29/#812, T-30/#813, T-36/#819, T-37/#820** (r6 deferrals) vs the round-8 notes | **Standing.** No round-8 note touches any of them; the r6 deferral reasons stand re-affirmed, the issues stay open without milestones. (#807's specific question — "rule it or keep the register" — is answered: it stays a register; the r6 D-2 MINOR-arm reason is unchanged.) | Nothing new is on the table for these; re-opening without new information violates the intake discipline. |
| C-7 | **N-23's closing question** ("would multi-select be an improvement to usability or not?") | **Answered: yes** — with the guardrails the plan fixes (selection is adapter-owned state, never UI-side; multi-verbs dispatch per-target commands in anchor order; single-target-only verbs hide on multi-selection per C-4's hidden rule). | The operator asked; the plan answers in design terms and the walkthrough protocol verifies the answer at stage 3. |
| C-8 | **#749** (no disconnect/unbind on files-bound rows) vs the **r7 roster** (Stop Sync landed as a first-class row action, the round-6 overturn recorded in the census) | **Close-now candidate.** The r7 per-account roster (US-2) shipped Stop Sync on every files-bound row; the issue's premise (round-6 chunk-5 visibility-only rows) no longer holds. The files plan re-verifies against the landed roster and closes the issue with evidence, or fixes any genuine residue. | An intake item whose engine the wave already shipped is closed by evidence, not re-implemented. |

## 4. Intake coverage table (exactly-once)

| Item | Disposition |
|---|---|
| #807 (T-24) | C-6: register stands, re-affirmed in this plan; no plan work |
| #809 (T-26) | C-6: deferral stands (multi-select and focus-on-create are not inline editing — the distinction is recorded in the task-card-controls plan) |
| #811 (T-28) | C-1: planned — task-card-controls US-4 |
| #812 (T-29) | C-6: deferral stands |
| #813 (T-30) | C-6: deferral stands |
| #819 (T-36) | C-6: deferral stands |
| #820 (T-37) | C-6: deferral stands |
| #823 (T-40) | C-2: folded — board-interaction US-5 |
| #741 | files-sync plan (auto-list failure backoff) |
| #743 | files-sync plan (per-key env-lock display) |
| #745 | files-sync plan (per-row publish progress phase) |
| #748 | files-sync plan (move-ceremony undo window GUI) |
| #749 | C-8: files-sync plan verifies-and-closes (or fixes residue) |
| #750 | files-sync plan (per-board adoption state) |
| #668 | focus-restoration plan (Tab trap on modal surfaces) |
| #669 | focus-restoration plan (menu-open focus) |
| #670 | focus-restoration plan (Today walk refocus) |
| #724 | focus-restoration plan (notification/alarm row activation parity) |
| #753 | friction plan (zero-height flyout guard) |
| #759 | task-detail-editor plan (rule replacement preserves the pause) |
| #706 | task-detail-editor plan (dead-subject restoration) |
| #832 | friction plan (conversion-arm no-ops) |
| #834 | board-interaction plan (roster indicator recognition cue) |
| #836 | friction plan (armed strip disarm on view switch) |
| #839 | tag-color plan (free-form colour via the A1 constructor) |
| #840 | task-detail-editor plan (invisible armed flag / Escape rung) |
| #772 | friction plan (find/palette verb naming alignment) |
| #762 | machinery-enabler (§6): no round-8 journey needs a dropdown inside a total-mask modal; re-checked when the friction plan touches the Dropdown grammar |
| #746 | files-sync plan (publish picker flyout-site audit) |
| #712 | friction plan (Replace cell × walk child — chain-aware pre-pass) |
| #715 | machinery-enabler (§6): pure refactor, exempt (no surface delta); folded into whichever chunk first touches the child-effect-bubble sites |
| #716 | machinery-enabler (§6): pure refactor, exempt; the planner-stage-as-walk-child journey enters only if a round plans it as a feature |
| #756 | files-sync plan (Deck-to-Deck rebind ceremony — new capability, full journey treatment) |
| #763 | friction plan (one-shot target consumption — routing-correctness bug with user-visible wrong-target symptom) |
| #782 | framework debt, folded: every new seam in this round's plans carries the `// slint: <callback>` comment (house convention); a pre-framework-seam sweep rides the first implementation chunk of each plan (stage-2 gate rows must resolve) |
| #768 | framework debt, folded: this round's plans are framework-era — their step tokens are REQUIRED gate rows (the r3 D-9 posture); the retro-plan mapping debt stays homed on the issue, untouched |
| #760 | framework debt, folded: stage 3 of every chunk cross-checks the plan's wiring rows against `tests/seam_inventory.rs` (a review-protocol step, recorded in each plan's §9) |
| #775 | framework debt, folded: this round's census updates cite symbol anchors + the slice baseline SHA per the census evidence convention; the drift debt stays homed on the issue |

## 5. What round 8 does not do

- The 2026-11-16 cleanup report §D owns everything outside §2/§4:
  flaky-test watchlist, the audit registers (#274/#277/#284/#285),
  dated timers (#428/#429), performance/sync architecture
  (#131/#133/#137/#141/#146/#148/#152/#154/#155/#157), the CLI/dx
  surface (#167/#180/#191/#198/#218/#224/#225/#845).
- No re-opening of accepted triage classifications (C-6's letter).
- No new kiosk capabilities: the board/task surfaces are desktop
  (the census mount grammar); the focus/keymap families are desktop
  keymap rows; the files-sync surface is Settings (desktop). Kiosk
  posture per plan: recorded, no new gating instances expected.
- No journal redesign, no `UiRequest`-lane undo work (R-17 stands;
  #748's undo window is the *move ceremony's own* window exposed, not
  journal work).

## 6. The machinery lane (exempt enablers)

#715, #716, #762 are pure refactors/enablers with no surface delta —
the framework's exemption applies (`docs/ux_planning.org` trigger
rule). They are not chunked by this round's dispatch on their own;
each folds into the first chunk whose work touches its site, and the
fold is recorded in that chunk's plan. #763 and #712 are different:
both have user-visible failure modes (wrong-target dispatches; a
surviving walk under a replaced capture), so they are journey-planned
in the friction plan as bug fixes, with the fix living adapter-side.

## 7. Chunking proposal (dispatch input — written only after acceptance)

Per `AGENTS.md` §8: one branch/PR per plan is the default; stacked PRs
only where a dependency forces it. Proposed order (dependency-driven,
not priority-driven):

| Order | Chunk (branch seed) | Depends on | Why this order |
|---|---|---|---|
| 1 | `fix/ui-r8-keymap-parity` (P5) | — | Pure keymap/parser work; zero surface coupling; unblocks the quick-action shortcuts' preset rows (P2). |
| 2 | `fix/ui-r8-focus-restoration` (P4) | — | The focus-restore seams are infrastructure the editor/card plans' new overlays ride. |
| 3 | `fix/ui-r8-task-detail-editor` (P3) | 2 (focus restore) | The picker fixes reuse the restore seams; the day-boundary delta is domain-pure plus the editor funnel. |
| 4 | `feat/ui-r8-board-interaction` (P1) | 2 | Multi-select + reorder introduce the selection state and drop-target grammar the card plan's quick actions compose with. |
| 5 | `feat/ui-r8-task-card-controls` (P2) | 1, 4 | Quick actions need P5's preset rows and P1's selection grammar; the menu restructure lands after the swap-bug fix so the new grammar is built on a sane base. |
| 6 | `fix/ui-r8-files-sync-status` (P6) | — (parallelizable with 3–5; Settings files are disjoint) | The N-19 investigation runs first inside the chunk (diagnosis gates the fix design). |
| 7 | `feat/ui-r8-tag-color` (P7) | 4 (only if the roster entry point touches board menus) | Small; the A1 constructor work is domain-first. |
| 8 | `fix/ui-r8-cross-surface-friction` (P8) | last | #753's guard must see the final Dropdown grammar (P1's new dropdown sites); #763's fix re-bases cleanly once the other chunks' route additions landed. |

Every chunk runs the house three-stage protocol (`AGENTS.md` §7/§8 +
the remediation dispatch's shared execution model: doctor first, TDD
stage-1 chunk plans for code-bearing chunks, fast gate in-agent,
dispatcher-owned coverage trio, PR-green + `CLEAN` hand-off, worktree
teardown). Stage 2 of every chunk: `scripts/ux-coverage.sh <plan>
--gate` green on the owning plan (new-format step tokens are REQUIRED
rows), census cells updated per the owning plan's §-citations, new
seams carrying `// slint:` comments (#782 fold). Stage 3: walkthrough
protocol per P1 story, the six-heuristic pass, the doctrine bar
(`ux_planning.org` §2.6) as the regression bar, and the #760
seam-inventory cross-check.

## 8. Walkthrough bar for the round

Every companion plan's P1 stories are walked at stage 3 with the four
questions plus the six-heuristic rubric. The round's named regression
bar (beyond §2.6's five positives): keyboard/mouse parity on every new
affordance (N-7/N-15/N-21/N-23 are explicit parity requests), and the
focus contract of N-3 ("focus remains on the element active before
the picker opened") treated as a house law the round itself installs —
the focus-restoration plan's rule becomes the census's standing
posture note for every future overlay.

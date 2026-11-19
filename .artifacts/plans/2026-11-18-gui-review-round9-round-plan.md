# [Plan] GUI Review Round 9 — Implementation Round Plan

- **Date**: 2026-11-18
- **Kind**: round implementation plan — synthesis of the five round-9
  research reports into scope, decisions, chunk ledger, and sequencing.
  The per-chunk three-stage agent prompts land in the dispatch plan
  (`.artifacts/templates/dispatch-plan.md`) once the UX journey plans
  are ACCEPTED (AGENTS §7 ordering law).
- **Origin**: operator round-9 review pass (2026-11-18, ~30 findings);
  research reports:
  - `.artifacts/reports/2026-11-18-round9-habit-parity-research.md`
  - `.artifacts/reports/2026-11-18-round9-widget-framework-research.md`
  - `.artifacts/reports/2026-11-18-round9-keyboard-focus-research.md`
  - `.artifacts/reports/2026-11-18-round9-menus-controls-research.md`
  - `.artifacts/reports/2026-11-18-round9-dayreview-details-research.md`
  and the round-8 handoff
  `.artifacts/reports/2026-11-18-round8-handoff-to-round9-analysis.md`.
- **Status**: ACCEPTED — dispatch may chunk from it (operator acceptance 2026-11-18, full review pass).
  journey plans (§5); the dispatch plan is written only after.

## 1. Operator rulings locked 2026-11-18

| ID | Ruling |
|---|---|
| O-1 | ~~Habit per-week/per-month day semantics: full anchor-day windows~~ **SUPERSEDED 2026-11-18 (later operator pass, habit-report Q1 = option (b))**: the optional day is a **display/nudge day inside the canonical window** — windows stay ISO-Monday/calendar-1st; no D-H12 reopening, no streak-harness rework. Nudge law (Q7): with a day selected, nudging starts relative to that day; otherwise at the period's start, with reminder snooze support (user picks hours/days). |
| O-2 | Habit/task unification: **UI-level `RowSubject::Task\|Habit`** at the adapter/router layer. No domain `TaskKind` variant. Domain changes are additive only (description field, O-1). |
| O-3 | Day-review planner integration: **embed the planner as a guided step** inside the day-review dialog (rides issue #716's embedded-child integration); standalone PlanNextDay view remains for direct entry. |
| O-4 | Round-9 scope adds: keymap case-seam fix (K1), menu remainder (multi-selection bar, tooltip hints, #927 disarm), and a **GUI regression investigation of the task context-menu quick bar** — the operator's build from latest main shows NO quick-action buttons (screenshot 2026-11-18 14:19), despite PR #933 being merged; treat as a live regression until proven otherwise. |
| O-5 | **Standing CLI-parity ruling (2026-11-18, global, all future rounds)**: no feature may be CLI-only. CLI-only features were acceptable only before the GUI crate reached parity — that point is passed. Preferred state: complete GUI↔CLI parity; GUI-only allowed in special cases where a CLI implementation is too onerous. Every existing "the CLI covers it" deferral is void. Sweep executed same day: `.artifacts/reports/2026-11-18-round9-cli-parity-sweep-report.md` (15 CP findings; 5 round-9 joins, 6 backlog issues #944–#949, 3 covered, plus already-fixed and legitimately-out-of-scope ledgers). Census supersession note text: sweep report §6. |
| O-6 | **Standing widget-centralization mandate (2026-11-18, global)**: all widget definitions are systematically centralized — UI needs are met by general reusable base widgets with customization points, never special ad-hoc implementations. Whole-widget audit: `.artifacts/reports/2026-11-18-round9-widget-inventory-centralization-report.md` (36 classes inventoried; 24 already centralized; 14 ad-hoc; base taxonomy + sourced sizing defaults). Feeds chunks W3–W7; backlog #950–#954. Widget-framework Q1–Q6 answers recorded as J2 rulings R-3..R-7. |
| O-7 | **Habit-parity rulings (2026-11-18, operator pass over the habit research report)**: research §3 architecture recommendations all accepted. Open questions: Q1 → option (b), see O-1 supersession. Q2 → **no habits in boards**; habits live outside boards (future week/month/year calendar views may show them once those exist). Q3 → convert-to-recurring-task **keeps its armed-confirm step inside the context menu**. Q4 → **reuse the task detail editor surface** for habit editing, generalized as far as possible so visual adjustments transfer between task and habit variants (ties into O-6/W4's LabeledValueRow). Q5 → **#935 rides this round** (H-chunks). Q6 → **GUI habit removal in scope** (already joined H4, closes #944). Q7 → see O-1's nudge law. Additional features: approved → issues #955–#963 (skip days, undo, heatmap/history — the prime motivator, pause, reminder overrides, per-weekday schedules, inline quota edit, streak badge, statistics); declined → notes-per-completion (proposal 6, D-H5 law stands, no issue). |
| O-8 | **Keyboard rulings (2026-11-18, operator pass over the keyboard research §8; J3 rulings R-1..R-8)**: directional law is emacs-scoped — vi's motion equivalents and the Standard preset's **arrow keys** carry the functionality. No viewed-task changing from TaskDetail at all (subject walk removed, not re-homed). Today left/right = control-walk first cut (operator will trial). Planner gets directional grammar; **standing policy: strict keyboard–mouse parity app-wide**. Chorded-letter case law: Ctrl+T ≡ Ctrl+Shift+t, distinct from Ctrl+t; canonical spelling **Ctrl+Shift+t**. Selected-row cursors for every interactable-row widget app-wide (rule-list cursor this round → chunk K4). Tracking = **one smart toggle**. |
| O-9 | **Menu rulings (2026-11-18, operator pass over the menus research §6; J5 rulings R-2..R-6)**: the missing quick bar is a **confirmed live regression** (post-#933 build from ~1h before review — root-cause, not stale-build). Multi bar = whatever multi-sensible elements fit. Bar aesthetics judged after implementation. **Priority picker becomes a dropdown** (requires dropdown de-siting; time combobox and labels pickers stay bespoke — bad fit). Labels multi-toggle semantics deferred until a dedicated multi-label-usability design. |

## 2. What the research established (decision-grade facts)

1. **Habits** are a parallel, pointer-only implementation
   (`ui/compositions/habits.slint`, 314 lines): plain `ListRow`s,
   vertically stacked icon buttons, no right-click (menu.rs pins
   `is_habit` false), only `HabitCreate` bound (Ctrl+Shift+H, issue
   #934), no GUI editing (R-2 ruling now overturned by this operator
   pass), no description field anywhere in domain/changeset/CLI/sync.
   `anchor_time` + reminder engine exist (~80% of the auto-reminder
   ask); windows hard-coded ISO-Monday/1st (habit.rs:328-339).
2. **Widget framework**: ~13 overlay families mount as conditional
   `key-root` siblings with hand-copied grammar. Click-inside-closes =
   panels lacking a full-surface absorbing TouchArea (worst: day-review,
   recur editor discards drafts). Focus escape = containment is
   opt-in (`MaskWalkFacts` from PR #872) and day-review never armed it.
   Layout shifts come from conditional mounts inside layout trees
   (due-editor echo rows, task-view inline swaps), not the window-level
   panels. Slint 1.18.1 has no Dialog/anchor/trap primitives — the work
   is deduplicating the project's own inventions into a base layer.
3. **Keyboard**: no directional concept in the action vocabulary; board
   is law-conformant, TaskDetail violates by design (round-8 D-2 made
   C-n/p write `subject_task`). Board move bug root-caused:
   `StackNext/Prev` are `EffectClass::Nav` → settle matrix cancels the
   move (adapter.rs:4992). Ctrl+T dies at a chorded-letter **case seam**
   (`parse("Ctrl+T")` mints `'T'`, delivery `'t'`, no chord partners for
   letters) — every uppercase-spelled standard-preset Global chord
   shares it; tests feed `parse()` tokens so never catch it (#855
   comment 6979361380). Task-details has 22 verbs; gaps include done
   checkbox, tracker toggle, entries opener, effort edit/clear.
   Start/stop ARE bound (F9/F10 etc.) since #862 — only Board-context
   bindings and the toggle verb are missing.
4. **Menus**: #933's `task_quick_bar` merged 2026-11-18T01:02Z — yet the
   operator sees no bar on latest main (O-4): investigate as a
   regression (feature gate? MenuKind arm? build?). Genuine remainder:
   multi-selection menu opens with an empty bar (`adapter.rs` ~:10159),
   bar buttons lack `hint-keys`, #927 stale pinned rows, and the shared
   `ActionMenu` renderer never draws the submenu chevron (one-line fix
   at action-menu.slint:256-274, asset already vendored).
5. **Day-review/details**: all root-caused with file/line refs (see
   report). Day-review needs the 6-step guided redesign; the
   PlanNextDay width bug is the same class as the fixed task-view
   severing (`8698fc0f`); description editor shrink = missing
   `vertical-stretch:1` on the edit mount (task-view.slint:1301-1319);
   effort row = swap machinery to delete in favor of an always-mounted
   textbox; time-picker-on-top = two anchor-mirror races
   (task-view.slint:315-333, time-field.slint:92-126).

## 3. UX journey plans (DRAFT — need operator acceptance)

| Journey plan | Path | Clusters |
|---|---|---|
| J1 habit parity | `.artifacts/plans/2026-11-18-habit-parity-ux-plan.md` | A (+O-1, O-2) |
| J2 widget framework | `.artifacts/plans/2026-11-18-widget-framework-ux-plan.md` | B (base widgets + migration) |
| J3 keyboard focus | `.artifacts/plans/2026-11-18-keyboard-focus-ux-plan.md` | C (directional law, move fix, shortcut completion, case seam) |
| J4 day-review | `.artifacts/plans/2026-11-18-day-review-ux-plan.md` | E day-review half (+O-3) |
| J5 menus remainder | `.artifacts/plans/2026-11-18-menus-remainder-ux-plan.md` | D + O-4 (bar regression, multi bar, arrows, #927) |

Task-details polish (editor shrink, Effort Estimate, picker layout
shifts) rides J2's migration slice — no separate journey plan; its
wiring rows are recorded there.

## 4. Chunk ledger (the dispatch plan chunks these)

| Chunk | Journey slice | Branch | Covers | Size |
|---|---|---|---|---|
| W1 | J2 §foundations | `feat/ui-widget-framework-base` | `ModalDialog` + host (centered, scrim, content-sized, button model w/ accept/dismiss/custom roles, auto `MaskWalkFacts` arming, absorbing panel), `AnchoredOverlay` geometry helper (below/above flip + clamp, replaces ~10 copy-pasted expressions), dismissal law (R-4 role-based: edit dialogs outside-click = save, destructive = cancel), scrim (R-3), width floors/caps per R-6, geometry non-intersection assertions; `docs/widget_interactions.org` skeleton. No consumers migrated yet (two proof mounts max). Scope may grow per the whole-widget centralization audit (O-6) — see W0 below. | L |
| W2 | J2 §migration | `refactor/ui-overlay-migration` | Migrate time picker (due/start), recur editor, add-label, labels flyout, palette, date-picker, track-entries picker onto W1 bases; fix anchor-mirror races; task-details polish folds in: description editor stretch, Effort Estimate always-on textbox, in-layout editor swaps → overlays. | L |
| H1 | J1 §domain | `feat/domain-habit-fields` | Additive domain wave: habit description field (domain, changeset, CLI, sync wire) + the O-1(option b) optional-day field — a display/nudge day inside the canonical window (no window-boundary change; streak walk untouched). | S |
| H2 | J1 §row | `feat/ui-habit-row-rework` | `RowSubject::Task\|Habit` at adapter/router; habits render as `TaskCard`s in Today with horizontal action row; right-click context menu incl. convert-to-recurring-task demoted to a submenu; time chip display. | L |
| H3 | J1 §keyboard | `feat/ui-habit-keyboard-parity` | Habit action shortcuts, keyboard traversal + reorder (closes #934), drag-n-drop via the #918 gap-drop pattern. | M |
| H4 | J1 §editing | `feat/ui-habit-editing` | Habit editing on the **generalized task-detail editor surface** (O-7 Q4): description, optional time, optional day; reminder nudge GUI seam with the O-1 nudge law (day-relative, else period-start, snooze in hours/days); **GUI habit remove** (O-5/CP-1 join — staged-confirm grammar like task delete; #944 closes here). | M |
| K1 | J3 §case-seam | `fix/ui-keymap-chord-case` | The O-8 chorded-letter case law: binding identity (shift bit + case-insensitive letter) so Ctrl+T ≡ Ctrl+Shift+t ≠ Ctrl+t; `from_event` delivery contract tests over every preset's chorded-letter rows; all preset definitions re-spelled to the Ctrl+Shift+t convention (today view included). | S |
| K2 | J3 §directional | `feat/ui-directional-focus` | `nav.focus-*` ActionIds → `UiEffect::FocusDirectional` per-view table; preset strategy per J3 R-1 (emacs C-f/b/n/p, vi motion equivalents, Standard arrows; text-entry guard); TaskDetail C-n/p → walk ladder, subject walk removed entirely (J3 R-3; supersedes round-8 D-2 via census entry); Today control-walk first cut (R-4); planner directional grammar (R-5); board move kept-arms fix (adapter.rs:4992 settle matrix). | L |
| K3 | J3 §shortcuts | `feat/ui-taskdetail-shortcuts` | Fill the 22-verb table: done checkbox, entries opener, effort edit/clear, back/close; **one smart track-toggle** (J3 R-8) incl. Board context; tooltip hint coverage. | M |
| K4 | J3 §cursors | `feat/ui-row-cursor-walk` | Selected-row cursor for rule lists (new walk state, directional-walkable); records the app-wide interactable-row cursor heuristic (feeds W7's doc). | S |
| M1 | J5 | `fix/ui-context-menu-bar` | GUI regression investigation of the missing quick bar — **confirmed live regression (O-9 R-2)**, not a stale build: root-cause why `task_quick_bar` (merged in #933) does not render, with a structural GUI test proving presence; fix; multi-selection bar (multi-sensible elements per J5 R-3); `hint-keys` tooltips; #927 reconcile disarm; submenu chevron in `ActionMenu` (context + main menus). | M |
| M2 | J5 R-5 | `refactor/ui-priority-picker-dropdown` | Dropdown de-siting (self-registering mount replacing the hardcoded site enum + per-site `DropdownFlyout` arms), then the priority picker adopts the dropdown; time combobox and labels picker stay bespoke (J5 R-5). | M |
| D2 | J4 | `feat/ui-day-review-journey` | 6-step guided day-review on the W1 `ModalDialog`; planner embedded as a step (O-3, rides #716); empty-stage auto-skip incl. "Move to tomorrow" inert fix (`adapter.rs:3174`); PlanNextDay severing fix (generalized probe rule, `plan-next-day-view.slint:327`). | L |
| W3 | J2 §centralization (O-6) | `refactor/ui-button-primitives` | `TextButton` absorbing CtaButton/SettingsButton/VerbButton/ghost variants + the 8× button grammar (~500 lines, F-1); `CheckBox` primitive (F-8); closes #950, #953. | M |
| W4 | J2 §centralization (O-6) | `refactor/ui-labeled-value-row` | `LabeledValueRow` with display/cell/editor modes absorbing the 10 hand-rolled rows + 3 divergent edit grammars (F-2); closes #951. | M |
| W5 | J2 §centralization (O-6) | `refactor/ui-small-widget-primitives` | `ActionChip`, `SectionHeader`, `StatusNotice`, `ConfirmStrip`, `GhostOpener`; Theme `tone-color/tone-fill` functions deduplicating the 5× tone ladder (F-3); closes #952. | M |
| W6 | J2 §centralization (O-6) | `refactor/ui-menu-shell` | `MenuShell`+`MenuRow` unifying menu-row grammars; `AnchoredOverlayHost` absorbing the 7× anchor-facts mirroring incl. the twin pickers (F-5); closes #954. | M |
| W7 | J2 §doc (O-6) | `docs/ui-widget-interactions` | `docs/widget_interactions.org` extended to the full base-widget taxonomy (inventory report §3) with usage rules and sizing defaults (32px click target; dialogs 280/560; menus 200/360, ×base-scale). | S |

**Dependency graph & ordering**:

- Parallel-safe early wave: **K1, M1** (disjoint files; K1 touches
  presets.rs/keymap seam, M1 touches menu.slint/action-menu.slint).
- **W1 → W2 → D2** is the strict foundation chain (D2 consumes W2's
  migrated mounts).
- **H1 → H2 → H3 → H4** serialized (all touch adapter.rs / habits
  surfaces); H-chunks are file-disjoint from the W/D chain, so one H
  chunk may run beside one W/D chunk under the ≤3-agent resource cap,
  staggered ≥3 min (docs/agent_dispatch.org §2).
- **K2** after K1 lands (same seam); K3 anytime after K2's ActionId
  vocabulary exists.
- D2 also depends on H4 only loosely (habit rows appear in review) —
  not a hard edge; rebases resolve.

**Suggested dispatch order**: K1, M1 → W1 → (H1 ∥ W2) → (H2 ∥ D2) →
H3 → K2 → H4 → K3 → K4; the O-6 centralization wave W3 → W4 → W5 → W6 → W7
runs after W2 (they consume the base layer and the migrated mounts) and
is serialized within itself (all touch ui/widgets/ + shared surfaces);
individual W-chunks may interleave with the H/K tails under the ≤3-agent
cap where files are disjoint (W3/W4 touch task-view.slint — keep them
clear of K3 and D2).

## 5. Explicitly deferred (with homes)

- Habit proposals beyond the operator list (skip-day, streak badges,
  history view, pause, per-habit reminder overrides, per-weekday
  schedules, inline quota edit, undo for LogHabit): research report §4 →
  backlog issues at dispatch time (standing authorization).
- Dropdown de-siting + priority-picker adoption: menus report §5 —
  **not** pulled in (operator declined O-4 selection); backlog.
- #935 route() uniform typed-refusal pass, #938 Labels row on card
  popup, #920 coverage-gate grammar, #921 clock race, #873/#874: stay
  homed on their existing issues (handoff §3).
- N-19 re-test on real Nextcloud post-#942: operator-owned, outside
  this round.
- O-5 CLI-parity remainder (backlog issues, future rounds): #945 GUI
  board archive verb (candidate M1 pull-in if trivial), #946 board
  reset, #947 board reconcile + consent letter, #948 GUI export
  (pdf/csv/json — largest genuine gap), #949 GUI status surface.

## 6. Verification strategy (round-level)

- Every chunk: pre-commit hooks + flock-wrapped `cargo nextest`
  (subagents never run the coverage trio — AGENTS §7 dispatch
  discipline); dispatcher owns full §10 suites in free windows.
- Journey tests: step-ID tokens (`us<n>_s<k>`) embedded; window/rig
  tests wrap `on_slint_thread`; `scripts/ux-coverage.sh <plan> --gate`
  at stage 2 self-check and stage 3 + walkthrough at PR time.
- The M1 bar investigation must include an actual GUI test (operator's
  explicit ask): a slint-test rig (or the browser-GUI harness if a
  runnable build is available) that mounts the task context menu and
  asserts the quick-bar row presence structurally — never text.
- Census: every landing chunk writes its dated supersessions (D-H12,
  round-8 D-2, R-2) into `docs/ux-surface-census.org`.

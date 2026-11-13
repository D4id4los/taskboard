# [Plan] Round-7 UX Pilot — Comparison, Plan Corrections, and Re-work Disposition

- **Date**: 2026-11-13
- **Kind**: comparison plan — the phase-2 pilot's step 2–4 record (the
  framework plan §6.1 pilot discipline): the mechanical gate runs, the
  cognitive walkthrough per P1 story, the verdict tables, every
  deviation's resolution (plan correction vs re-work), the
  validation-assertion outcome, and the authority for the amendments
  this chunk makes to the three UX plans and the round-7 dispatch plan.
- **Origin**: the pilot dispatch plan
  `.artifacts/plans/2026-11-13-ux-journey-pilot-dispatch-plan.md` §3;
  the stage-1 chunk plan
  `.artifacts/plans/2026-11-13-ux-r7-pilot-comparison-chunk-plan.md`
  (binding; decisions D1–D7); the framework plan
  `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md`
  §4.3/§4.4/§6.1; `docs/ux_planning.org` §2.3–2.4.
- **Evidence baseline (D1)**: `origin/main` @ **`fb5863ed`** — both
  round-7 implementation chunks merged (habit create PR #758, recur
  editor PR #765), so the dispatch's open-PR-branch fallback does not
  apply. All `file:line` anchors below are read in that tree (the
  worktree checkout). The three UX plans sit at the three commits
  immediately below it.
- **Status**: COMPLETE — the comparison is done; no re-work items; the
  amendments it authorizes ride this chunk's branch (commit
  sequencing per the stage-1 plan §7).

## 1. Verdict summary (headline counts)

| Plan | Satisfied | Deviating | Missing | Missing-of-which pre-state |
|---|---|---|---|---|
| habit-create (`2026-11-13-r7-habit-create-ux-plan.md`) | 5 (US-1/S1–S5) | 0 | 0 (as verdicts; gate step-token rows are plan corrections, §3.1) | 0 |
| recur-editor (`2026-11-13-r7-recur-editor-ux-plan.md`) | 11 (US-1/S1–S5, US-2/S1–S4, US-3/S1–S2) | 0 | 0 (same note) | 0 |
| files-roster (`2026-11-13-r7-files-roster-ux-plan.md`) | 0 | 0 | 11 (US-1/S1–S3, US-2/S1–S3, US-3/S1–S2, US-4/S1–S3) | 11 — **the correct pre-state**: round-7 chunk 3 has not run; recorded as informational evidence, not failure (D1/D2) |

**Re-work list: empty.** Per the stage-1 plan §4.2 and the framework
§6.1 step 2, an empty re-work list is a valid outcome, and it is the
honest one: every plan claim for the two merged chunks is implemented
and behavior-tested; every difference found between plan text and
implementation is either an explicitly-unfixed assumption now bound
(D2's assumption-resolution rule) or a name-level delta within an
unchanged scope (D6) — none changes scope, affordance shape,
command usage, escape paths, or kiosk posture. Consequently:
- **No re-work chunk blocks are appended to the round-7 dispatch plan**
  (the re-work machinery exists, is documented in the amendment, and
  is unused).
- **No stage-1 implementation plan is superseded** (D6's trigger —
  scope/behavioral change — never fired); the stage-1 plans
  `2026-11-13-ui-r7-habit-create-plan.md` and
  `2026-11-13-ui-r7-recur-editor-plan.md` stand.
- The round-7 dispatch plan amendment is limited to the header status
  note plus the chunk-3 prompt re-authoring (D5.1).

## 2. Mechanical gate runs (D2 step 1)

Ran `scripts/ux-coverage.sh <plan> --gate` from the worktree at the
pinned baseline (exit 1 = MISSING rows, expected and explained below;
no parse errors anywhere — exit 2 never occurred; full per-row output
in the appendix §6).

| Plan | checks | FOUND | MISSING | Exit | Reading |
|---|---|---|---|---|---|
| habit-create | 15 | 6 | 9 | 1 | All 3 command tokens FOUND (`CreateHabit` at `messages.rs:461`); `ActionId::HabitCreate` FOUND (`actions.rs:297`); MISSING = 5 journey-step tokens + the pass-through/assumed callback names + the matrix-cell story check — all adjudicated below |
| recur-editor | 30 | 8 | 22 | 1 | `SeriesEdit` FOUND (`actions.rs:343`); `UpdateTask` FOUND ×4; MISSING = 11 journey-step tokens + assumed callbacks + in-context "—" action cells — adjudicated below |
| files-roster | 42 | 9 | 33 | 1 | Only the pre-existing command tokens FOUND (`MoveBoardToFiles` `requests.rs:425`, `UnbindBoard` `messages.rs:287`, `BindBoard` `messages.rs:152`); everything else MISSING — **the correct pre-state** for a plan whose chunk has not run |

**Gate finding recorded for phase 3 (backlog #768):** the gate's
journey-step check greps for `us<n>_s<k>` tokens in test names. The
two merged chunks landed comprehensive behavior tests under
descriptive names (`habit_create_sends_the_exact_command`,
`recur_editor_commits_the_exact_assembled_rule`, …) because the
implementation chunks pre-date the pilot plans they were compared to.
The step-token rows therefore stay MISSING even after the plan
corrections below bind the actual test names — the token law is
unsatisfiable retroactively without re-naming shipped tests. Phase-3
retrofit plans will all hit this; the gate needs an acceptance for a
bound-test-name column (a plan-side fix, not a code change).

## 3. Verdict tables (D2 step 2 — walkthrough) and resolutions (D4)

Verdict vocabulary per the stage-1 plan D3. Every verdict's evidence
is `file:line` at `fb5863ed` plus existing test names. The four
walkthrough questions are answered per step in the columns below
(Q1 see / Q2 recognize / Q3 feedback / Q4 recover). Authority order
(D4): UX-plan recorded ruling > round-7 plan §2 > blind-plan default >
implementation choice.

### 3.1 habit-create — US-1 (P1, the plan's only story)

| Step | Verdict | Q1 see | Q2 recognize | Q3 feedback | Q4 recover | Resolution |
|---|---|---|---|---|---|---|
| US-1/S1 arm | satisfied | `ui/compositions/habits.slint:110-116` — the plus `IconButton` mounts iff `editing-visible`; pass-through `create-asked` → `habits-create-asked` (`today-view.slint:106,201`, `app-window.slint:464,1607`); keyboard `ActionId::HabitCreate` in the `Today` context (`actions.rs:1307-1317`), bound in all three presets (`keymap/presets.rs:147,397,611`) | label "Add habit…" ≡ hover tip ≡ walk name (comment `habits.slint:108-109`); action label/description `actions.rs:1309-1316` — judged, not text-asserted | the inline form mounts below the header iff `editing-visible && create-open` (`habits.slint:118`), the flag is adapter-owned (`adapter.rs:2899-2902`) | Escape disarms through the keyscope arm (`adapter.rs:872-876`) and `disarm_habit_create` (`adapter.rs:912-916`); toggle discard-and-close + Escape-funnel journey test `lib.rs::habit_create_toggles_and_escape_disarms_without_shadowing` (lib.rs:3409); stale-arm silent disarm `lib.rs::habit_create_stale_arm_disarms_silently` (3326) | plan corrections PC-1/PC-3 (below) |
| US-1/S2 title | satisfied | title `TextField` with label+placeholder (`habits.slint:120-133`) | field label "Habit title" — judged | the draft echoes in the field itself; commit on Return (`accepted`) per the create-row precedent (comment `habits.slint:117-119`) | absorbed miss keeps the text (`create-confirmed → false` does not clear); Escape disarms the whole form (`adapter.rs:7514-7516`) | PC-3; the echo itself is form-local display with no dispatch — see MINOR M-2 |
| US-1/S3 target | satisfied | `SegmentedControl` Per day / Per week / Per month (`habits.slint:166-171`) | group label "Target" — judged | the picked variant shows via `selected-index`; quota semantics arm on commit (S5) | Escape/cancel disarms; a stale pick re-seats to index 0 on commit (`habits.slint:129-131` and the confirm arms) | PC-3; A-2's "UI-held until the seam mints it" default was adopted verbatim (`target-index` comment `habits.slint:65-68`) |
| US-1/S4 quota | satisfied | quota `TextField` with validity styling (`habits.slint:134-148`) | label "Quota (completions per period)" + placeholder "1" — judged | per-keystroke validity: `create-quota-edited` → `parse_habit_quota` → `habit_quota_validity` pushed (`adapter.rs:7509-7512`, `adapter.rs:1606-1608`; `presentation.rs:6637,6675`) | zero/junk/overflow refuse structurally through the mint — absorbed miss keeps the text (`adapter.rs:7531-7534`); tests `presentation.rs::parse_habit_quota_mints_through_the_structural_zero_refusal` (4569), `habit_quota_validity_is_total_and_hints_every_refusal` (4592); no-dispatch refusal journey `lib.rs::habit_create_refusals_absorb_without_dispatching` (3256) | none — the plan's fold semantics implemented exactly |
| US-1/S5 confirm | satisfied | confirm `IconButton` check/success, label "Create habit" (`habits.slint:172-183`) | label ≡ walk name — judged | the command dispatches and the form disarms + fields clear (`adapter.rs:7547-7562`) | invalid draft: confirm inert (the seam returns `false`, nothing dispatches); command failure keeps the form's typed-refusal posture; the board going away under the armed form disarms silently — the stale-arm law owns the seam's first word (`adapter.rs:7520-7528`) | PC-2 (the commit routes through `ActionId::HabitCreateCommit`, `actions.rs:311`, `router.rs:727-731`) |

**Exact-command evidence**: `lib.rs::habit_create_sends_the_exact_command`
(lib.rs:3201) asserts the dispatched `StateCommand::CreateHabit` carries
the section's board and the minted target (`lib.rs:3227-3231`).

**Plan corrections (habit-create)** — each recorded as a ruling-style
amendment in the plan (authority: this plan; date 2026-11-13):

- **PC-1 (binds A-1/A-2)**: the pass-through callback is renamed at
  the view boundary — `create-asked` in the composition,
  `habits-create-asked` in `today-view.slint` / `app-window.slint`;
  the adapter seams are `on_habits_create_asked`,
  `on_habits_create_quota_edited`, `on_habits_create_cancelled`,
  `on_habits_create_confirmed` (`adapter.rs:7505-7518`, inventory
  `tests/seam_inventory.rs:90-93`). A-1's "existing habit seam names
  unread blind" and A-2's "propagate unchanged" defaults resolved to
  the section-scoped rename; the section grammar wins.
- **PC-2**: the commit funnel routes through a second internal action,
  `ActionId::HabitCreateCommit` ("habit.create-commit",
  `actions.rs:306-311,1320-1330`; `router.rs:727-731`), following the
  habit family's internal-verb precedent (the `HabitLog` comment at
  `actions.rs:1324`); the plan's §7 delta named only `HabitCreate`.
  Scope, affordance shape, and command usage unchanged — name-level
  delta within unchanged scope (D6: no supersession).
- **PC-3**: the journey tests landed under descriptive names in the
  UI crate's driver suite (`crates/taskboard-ui-slint/src/lib.rs`,
  driven through `i-slint-backend-testing`), not
  `journeys.rs::us1_s*` tokens: `habit_create_sends_the_exact_command`
  (3201), `habit_create_refusals_absorb_without_dispatching` (3256),
  `habit_create_stale_arm_disarms_silently` (3326),
  `habit_create_kiosk_gates_the_affordance_not_the_rows` (3358),
  `habit_create_toggles_and_escape_disarms_without_shadowing` (3409).
  Per D3 this is the cheapest correction class; the plan's §8 and §6
  Test column are amended to the bound names. The gate's step-token
  rows stay MISSING (§2's finding, backlog #768).

### 3.2 recur-editor — US-1, US-2, US-3 (all P1)

| Step | Verdict | Q1 see | Q2 recognize | Q3 feedback | Q4 recover | Resolution |
|---|---|---|---|---|---|---|
| US-1/S1 mount (Add) | satisfied | "Add repeat" `IconButton` gated `!recur-visible && series-edit-visible` (`task-view.slint:653-659`); `ActionId::SeriesEdit` in `TaskDetail` (`actions.rs:1421-1430`), bound in all three presets (`presets.rs:195` Ctrl+R, `405-409` emacs C-c r, `703` vi r) | consequence-style label "Add repeat: the task joins a recurring series" (judgment: consistent with the stop-strip's consequence-first doctrine) | the overlay mounts window-level iff `recur-editor-open` (`app-window.slint:1199`); kiosk hides the affordance via `series_edit_visible(task, kiosk)` (`presentation.rs:1287`; test `presentation.rs` 8319-8326; journey `lib.rs::recur_editor_affordance_hides_on_kiosk` 2998) | Escape/cancel land on `on_recur_editor_cancel_clicked` (`adapter.rs:7411-7416`) — the draft dies with the level, nothing dispatches; Escape's global resolution through the masking chain (`keyscope.rs:719-723+`) | PC-5/PC-8/PC-9 |
| US-1/S2 pattern | satisfied | pattern `SegmentedControl` Daily/Weekly/Monthly + per-pattern fields (`recur-editor.slint`; seams `adapter.rs:7257-7327`) | per-pattern field labels/mounts — judged | the draft buffer (`level_state::RecurEditorState`) updates adapter-side per keystroke/pick, stale index no-ops (`adapter.rs:7262,7272`) | invalid pattern fields never block other fields (per-field validity in the model, `adapter.rs:2579-2663`); Escape discards | PC-5/PC-9 |
| US-1/S3 bound | satisfied | bound `SegmentedControl` Never/Until/Count (`on_recur_bound_picked`, `adapter.rs:7283-7296`) | as S2 | draft updates adapter-side | as S2 | PC-5/PC-9 |
| US-1/S4 echo | satisfied | live echo caption (`recur-editor.slint:54-57` `echo`/`echo-valid`) | echo readability — judged (plan §8 Manual) | valid draft renders the assembled `rule.text_form()`; invalid renders the typed `InvalidRecur` mapping (`assemble_recur_draft` + `recur_field_validity`, `presentation.rs`; tests module `presentation.rs:8060+`) | invalid keeps commit inert (the commit seam re-mints and refuses — `adapter.rs:7399-7401` "an invalid draft dispatches nothing"); editing re-validates; Escape discards | none beyond PC-9 |
| US-1/S5 commit | satisfied | commit control inside the overlay (`on_recur_commit_clicked`, `adapter.rs:7375-7407`) | commit/cancel labels — judged | `UpdateTask` with sparse `TaskChanges { recur: Some(Some(rule)) }` (`router.rs:765-776`); the overlay closes; the row + badge render via the ordinary refresh | typed failure: the seam re-checks the subject and re-mints before routing (the trust boundary never trusts the widget); the no-flow dispatch test `lib.rs::recur_editor_commit_without_the_flow_dispatches_nothing` (2983) | PC-7/PC-9 |
| US-2/S1 mount (Edit) | satisfied | "Edit repeat" affordance beside the pattern echo (`task-view.slint:776-782`); same `SeriesEdit` | judgment: as S1 | the editor mounts **pre-filled** from the current rule — `seed_recur_editor(t.recur, anchor)` (`adapter.rs:2558-2570`) | Escape discards the edits; the live rule untouched | PC-5/PC-6 |
| US-2/S2 adjust | satisfied | the same pattern/bound fields | as US-1/S2 | assembly laws via the domain mint (`recur::assemble_rule`, `taskboard-domain/src/recur.rs:791`; CLI refactored onto it — `commands/repeat.rs:291`) | as US-1 | PC-6/PC-9 |
| US-2/S3 echo | satisfied | as US-1/S4 | as US-1/S4 | validity re-arms on edit | as US-1/S4 | PC-9 |
| US-2/S4 commit | satisfied | as US-1/S5 | as US-1/S5 | the same sparse shape (`router.rs:765-776`); **D-S4 pinned**: `lib.rs::recur_editor_edit_preserves_series_identity` (2927) — series/continued_from/paused_until preserved | as US-1/S5 | PC-7/PC-9 |
| US-3/S1 arm stop | satisfied | the armed "Stop repeating" strip inside the overlay, consequence-first caption (`recur-editor.slint:274`); it does not render on a rule-less task (nothing to stop — the editor mounts Add-mode) | consequence caption "the series ends, the task stays as an ordinary task" — judged | the strip shows its confirm/cancel controls; the arm routes `SeriesEditDetachArm` (`adapter.rs:7414-7424`; `router.rs:787`) | cancel disarms back into drafting with the draft intact — `SeriesEditDetachCancel` (`adapter.rs:7426-7431`; `router.rs:788-790`; router tests `router.rs:1677-1685`) | PC-7 (the strip does warrant its own action ids — the plan's US-3 explicitly left this to implementation) |
| US-3/S2 confirm stop | satisfied | the armed strip's confirm control | judgment: as S1 | `TaskChanges { recur: Some(None) }` — the detach semantic (`router.rs:777-786`); badge/row unmount; the Add-repeat posture returns (the affordance's `!recur-visible` condition); **exactness pinned**: `lib.rs::recur_editor_detach_clears_the_rule_and_nothing_else` (2892) | cancel-before-commit is the recorded recovery (no undo claim — the plan's AC-3); a typed command failure keeps the editor open | none beyond PC-7 |

**Plan corrections (recur-editor)** — recorded as ruling-style
amendments in the plan (authority: this plan; date 2026-11-13):

- **PC-5 (binds A-3/A-4)**: the mount callbacks converge on
  `series-edit-asked` (`task-view.slint:171,658,781`,
  `app-window.slint:556,1634`) — not A-3's guessed
  `recur-edit-asked` shape; the editor's seams are the
  `on_recur_*` family (`adapter.rs:7249-7440`, inventory
  `tests/seam_inventory.rs:170-189`); `ContextId::RecurEditor` is the
  exact variant spelling A-4 assumed (`actions.rs:66`,
  mask registration `adapter.rs:10493`, keyscope chain
  `keyscope.rs:719+`). The draft buffer lives in the view-state
  level (`level_state::RecurEditorState` per `view_state.rs:247`'s
  EditDate identity rule), not the due-editor's flat facts globals —
  the same no-transient-UI-state law, newer home.
- **PC-6 (binds A-5)**: pre-fill mapping is
  `presentation::seed_recur_editor(t.recur, anchor)`
  (`adapter.rs:2566-2569`), pattern/bound/fields extracted
  from the live rule; the assembly laws stay design-fixed.
- **PC-7**: the overlay's commit/cancel/detach verbs are internal
  action ids in the `RecurEditor` context — `SeriesEditCommit`,
  `SeriesEditDetachArm`, `SeriesEditDetachCancel`, `SeriesEditDetach`
  (`actions.rs:1428-1464`, all pointer-grade per the picker-actions
  rule); US-3's entry-point note explicitly allowed this choice.
- **PC-8**: the mount affordances' labels are consequence-style
  sentences ("Add repeat: the task joins a recurring series",
  "Edit repeat: change the pattern or bound") rather than the plan's
  "Add repeat…"/"Edit repeat…" shorthand; label semantics were
  plan-designated manual/judged (§8), and the consequence-first
  wording matches the stop-strip doctrine. Affordance shape, mount
  sites, walk order, and the click ≡ key convergence are as planned.
- **PC-9**: journey tests landed under descriptive names in
  `crates/taskboard-ui-slint/src/lib.rs`:
  `recur_editor_commits_the_exact_assembled_rule` (2847),
  `recur_editor_detach_clears_the_rule_and_nothing_else` (2892),
  `recur_editor_edit_preserves_series_identity` (2927),
  `recur_editor_commit_without_the_flow_dispatches_nothing` (2983),
  `recur_editor_affordance_hides_on_kiosk` (2998); plus the router
  dispatch tests (`router.rs:1677-1741`) and the presentation
  validity/fold tests (`presentation.rs:8060+`). Bound into §8/§6 as
  PC-3; same gate-token note (backlog #768).

### 3.3 files-roster — US-1…US-4 (all P1)

**All 11 steps: `missing` — the recorded pre-state.** Round-7 chunk 3
(`feat/ui-r7-files-board-roster`) has not run: no
`StopBoardFilesSync`/`PublishBoardToFiles` variants exist in
`requests.rs`, the files-board loop still emits title-only rows
(`adapter.rs:4867-4883` equivalent), and the `FilesMovePicker` still
stands. The gate's 9 FOUND rows are exactly the pre-existing
machinery the plan reuses (`MoveBoardToFiles`, `UnbindBoard`,
`BindBoard`); the 33 MISSING rows are the plan's own claims. This is
the dispatch §3/§6.1 correct pre-state, recorded as informational
evidence — not a failure, and not a re-work item (chunk 3 implements
it). **No walkthrough is run against a non-existent implementation**;
the plan's assumptions (A-6) stay open for chunk 3's stage-1 plan to
bind, exactly the blind-plan discipline the pilot designed. The
files-roster plan is not amended (nothing to correct — the
comparison adjudicates implementations, and there is none yet).

## 4. Heuristic pass (D2 step 3) — six-heuristic rubric over the touched surfaces

Surfaces: Today/habits, Task detail/recur, Settings/files (the last
unimplemented — no pass possible, recorded as N/A until chunk 3).

- **Visibility of system status** — pass: armed/armed-empty states are
  adapter-pushed (`habits_create_open`, `recur-editor-open` +
  `recur-editor-model`); the live echo is per-keystroke.
- **User control & freedom** — pass: every step has an escape
  (toggle-discard, Escape funnels, cancel seams, armed-confirm cancel);
  the stale-arm laws own the seams' first word.
- **Error prevention & recovery** — pass: mints at the trust boundary
  (`parse_habit_quota`, `assemble_recur_draft`), absorbed misses keep
  text, invalid drafts dispatch nothing.
- **Consistency & standards** — pass: consequence-first labels, the
  convert-strip precedent, section-scoped grammars respected.
- **Recognition over recall** — pass: the recur editor mounts
  pre-filled from the live rule (US-2/S1); defaults are stated
  (quota placeholder "1").
- **Keyboard/mouse parity** — pass: `HabitCreate` and `SeriesEdit`
  bound in all three presets with the no-second-class totality gate
  extended (`keymap/mod.rs:478-482`).

**Findings: BLOCKER — none. MAJOR — none.** MINOR (each filed as a
`backlog:` issue per AGENTS §7 standing authorization):

- **M-1** (#769): the recur editor's cancel/Escape journey lacks an
  explicit pin that closing the overlay dispatches nothing (the
  cancel seam and `recur_editor_commit_without_the_flow_dispatches_nothing`
  imply it; the typed no-dispatch assertion for the cancel path
  itself is the nicety).
- **M-2** (#770): the habit-create form-local echo steps (S2 title
  echo, S3 target echo) have no direct tests — they are covered only
  indirectly through `habit_create_sends_the_exact_command` driving
  the `accepted()` commit path; no dispatch is involved, so this is
  polish, not a hole.

## 5. Validation-assertion outcome (the framework's phase-2 proof)

**The framework proved itself; the blind plans' claims re-verified.**

1. **Blind plans were blank/unruled at the baseline** — re-checked at
   `35e6b0a1` (the stage-1 plan §5 asked for the re-check, not trust):
   - `git show 35e6b0a1:crates/taskboard-ui-slint/src/actions.rs`
     lines 286-305: the habit verb family is HabitLog / HabitMove /
     HabitRepair / HabitConvertArm / HabitConvertToSeries only — no
     create verb, no `HabitCreate` anywhere in the registry.
   - `git show 35e6b0a1:crates/taskboard-ui-slint/ui/compositions/habits.slint`
     lines 63-76: the header is the static "Habits" + count caption,
     no affordance.
   - `git show 35e6b0a1:crates/taskboard-ui-slint/src/requests.rs`:
     no `bound_channel` / `BoardChannel` (grep empty);
     `PublishBoard` targets Deck by connector string only.
   ⇒ R7-1/2/3 read as unruled absences at planning time, exactly as
   the three plans' §1 validation-assertion sections claim. Applied
   at planning time, the framework would have flagged all three
   findings before any implementation existed — the phase-2 exit
   criterion's first half holds.
2. **The gate demonstrated real PASS and MISSING rows** — the two
   merged plans' FOUND rows are live anchors in the shipped tree
   (commands in `messages.rs`, actions in `actions.rs`), and the
   files-roster plan's all-MISSING run is the MISSING demonstration.
   The gate never produced a parse error (exit 2 never occurred) on
   any of the three plans or their post-amendment forms.

**One honest limitation recorded** (backlog #768, §2): the step-token
naming law is retroactively unsatisfiable for plans compared against
already-landed implementations; the comparison resolves it via bound
test names (PC-3/PC-9) and phase 3 must give the gate an acceptance
for that binding, or retrofit plans will permanently read exit 1.

## 6. Appendix — gate output (verbatim, pinned baseline `fb5863ed`)

```
=== habit-create (exit 1) ===
US-1/S1..S5 journey test → MISSING (5 rows)
R-1/R-2/R-3 reason → FOUND (plan §2 rulings)
habit.create cites US-1 → MISSING
W-1 callback `create-asked` → MISSING   (present in habits.slint:62; the adapter side is renamed habits_create_asked — PC-1)
W-1 action HabitCreate → FOUND (actions.rs:297)
W-1/W-2 command CreateHabit → FOUND (messages.rs:461) ×2
W-2 callback internal callbacks assumed (A-2) → MISSING
W-2 action — (form-local) → MISSING
checks: 15, FOUND: 6, MISSING: 9

=== recur-editor (exit 1) ===
US-1/S1..S5, US-2/S1..S4, US-3/S1..S2 journey test → MISSING (11 rows)
R-1/R-2 reason → FOUND (plan §2 rulings)
task.recur_set cites US-1/US-2, task.recur_clear cites US-3, task.recur_preview cites US-1/US-2 → MISSING (5 rows)
W-1/W-2 callback assumed (A-3) → MISSING   (actual: series-edit-asked — PC-5)
W-1/W-2 action SeriesEdit → FOUND (actions.rs:343) ×2
W-1..W-4 command UpdateTask → FOUND (messages.rs:44) ×4
W-3/W-4 callback assumed (A-3) → MISSING
W-3/W-4 action — (in-context verbs) → MISSING   (actual: the SeriesEdit* internal family — PC-7)
checks: 30, FOUND: 8, MISSING: 22

=== files-roster (exit 1) ===
US-1..US-4 journey test → MISSING (11 rows)
R-1..R-5 reason → FOUND (plan §2 rulings)
matrix story cells → MISSING (4 rows)
W-1/W-2 (no-callback/no-action/no-command rows) → MISSING (6 rows)
W-3 callback assumed (A-6) → MISSING; W-3 command MoveBoardToFiles → FOUND (requests.rs:425); StopBoardFilesSync/PublishBoardToFiles → MISSING
W-4 command UnbindBoard → FOUND (messages.rs:287); StopBoardFilesSync → MISSING
W-5 command MoveBoardToFiles → FOUND (requests.rs:425)
W-6 command BindBoard → FOUND (messages.rs:152); PublishBoardToFiles → MISSING
checks: 42, FOUND: 9, MISSING: 33
```

(The abridged form collapses the identical per-row repeats; the
row-by-row runs are reproduced in the chunk work log and were the
input to §3's verdicts. No PARSE-ERROR lines occurred in any run.)

## 7. Amendments authorized by this plan

1. **UX-plan ruling amendments** (this plan is the recorded authority;
   table headers stay token-exact; gate re-run post-amendment, §1
   evidence): habit-create (PC-1/PC-2/PC-3) and recur-editor
   (PC-5/PC-6/PC-7/PC-8/PC-9) — assumption-bindings and
   test-name/test-column bindings appended ruling-style; the
   files-roster plan is untouched.
2. **Round-7 dispatch plan** (D5): header status note (chunks 1–2
   merged, compared; pilot verdict) + chunk-3 prompts re-authored
   under the framework; **no re-work blocks** (the re-work list is
   empty, §1); untouched chunks verbatim.
3. **Census seed** `docs/ux-surface-census.org` (D7): the three pilot
   slices with evidence-anchored cells and dated, authorized rulings
   (including the round-6 → round-7 overturn on the files slice).

## 8. Deviations from the stage-1 chunk plan — none material

The prompt's "PR branch + fixed SHA + post-merge re-verify" was
settled not-applicable by D1 before stage 2 began. Every stage-1
decision (D1–D7) was executed as written; the comparison-plan
filename, verdict vocabulary, tie-break rule, amendment mechanics,
and census structure are all as recorded. The only judgment call this
stage added — recording the gate's retroactive-token limitation as a
backlog issue rather than silently leaving exit-1 unexplained — is an
addition within the standing issue authorization, not a deviation.

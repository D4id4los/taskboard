# Backlog-Reduction Round 5 — Scope Recommendation (2026-11-22)

Context: round 4 landed in full (the habit family HF1–HF4, the O-5
parity OP1–OP3, the restore CLI surface, the reaffirmed undo-v2 pair
UF1/UF2, the small gated bundles, and the entire support wave
including #327's mutants run, the golden wave, and the phase-2 binary
consolidation). 189 open issues; ~99 filed since 11-21 — a fresh
user-review batch (Today habits + task edit view), the S3 mutants
survivors, a five-issue flaky cluster on the retained-ETag wiremock
tests, and the round-4 follow-up tail.

## A. Wave 1 — the p1/p2 bug wave (first)

The new user-review batch carries two p1s and a systemic smell:

- **#1198 (p1)** — Today habit drag-n-drop never moves; context-menu
  Move up/down are no-ops (the survey names the seams: habits.slint
  drag/gap zones, `HabitMoveUp/Down` dispatch).
- **#1205 (p1)** — task edit view: Enter stolen by the Task Title
  (activates title-edit from every other widget) and overlay dismissal
  always restores focus to the title. Filed as one systemic
  investigation. **Related cluster** (same review, same view):
  #1203 (C-f/b/n/p dead in the edit view — the r9 K2 law's missing
  arm), #1204 (no focused state on the Description field),
  #1207 (edits became un-undoable mid-session). One chunk for the
  view, one investigation-first chunk for #1205's root cause.
- **#1200 (p2)** — Today habits not selectable by mouse click.
- **#1197 (p2)** — **third recurrence** of the layout-severing class
  (Today view has no root max-width sever). The prevention ask is
  binding: this time it's systemic — apply the severing idiom at the
  Today root **and** re-sweep the view roots (the #1029
  generalization's promise, re-checked; #778's upstream report stays).
- **#1206 (p2)** — date picker overlay clips the text field.
- **#1150 (bug)** — board ops whose target row was wiped strand in the
  outbox forever (`owning_board` returns `Some` unconditionally for
  board ops; neither the cycle nor the orphan lane picks them up).
  Domain/sync; the fix decides doc-truth vs orphan-lane catch.
- **#1215 — RULED 2026-11-22, option (b)**: the journey plan's R-5/
  US-2-S3 wording is amended to "the undo adds no delivery-addressing
  action; the action-layer prune is the standing delete contract" —
  the prune side effect is what manual delete already does; no
  storage change, `PersistenceAction::DeleteReminderRule`'s contract
  stays uniform across every removal path. The wording correction is
  a small chunk item (amend the plan + a dated supersession note; the
  `us2_s4b` pin already asserts the surviving semantics). Ruling
  recorded on the issue (comment 7079171410).

## B. Wave 2 — flaky dedicated pass (the new cluster + armed standbys)

**#1164–#1170** — five `retained_etag_wiremock` flakies (the S1 band
tests' shared wiremock timing seam — likely one root cause, five
closures). **#1214** (flow_table_tests masked-row flaky). The armed
standbys **#697/#650/#968** keep their standing rule: close only on a
recurrence with telemetry; the pass re-checks their capture state but
does not speculatively fix. Per AGENTS §7 this is the sanctioned
dedicated pass over the `flaky-test` label.

## C. Wave 3 — mutation-survivability (the S3 survivors)

The mutants run's surviving families: **#1231 (p2)** — the
commands-layer decision predicates are mutation-survivable wholesale
(one fake-engine unit layer for the predicates), **#1230** — the
`*_command` exit-code family in seven verbs (one CLI-tier table
test), **#1229** — the `render_text_*` prose family, **#1175**
(logout_command), **#1176** (the pdf-export dispatch branch untested
in its feature tier). One or two chunks; the kills are TDD-first;
the survivors' triage report is the S3 report's sequel.

## D. Wave 4 — feature follow-ups (journey-gated where user-visible)

The round-4 features' own follow-up tail — grouped:

- **Undo family**: #1146 (GUI restore surfaces for
  Label/TimeEntry/Habit), #1154 (ClearReminderRules first-class
  surface), #1155 (CLI `undo to`/`redo to` parity), #1156
  (deep-history browsing beyond the bounded windows), #1227
  (branch-picker keyboard selection), #1226 (sibling rows' payload
  labels). Two chunks + journeys.
- **Status/export polish**: #1147 (status panel command lane),
  #1148 (live-refresh lane), #1149 (export preview/clipboard).
- **Today-habits polish**: #1199 (button redesign), #1201
  (double-click opens edit), #1202 (per-action shortcuts),
  #1218 (tombstoned habit history GUI entry), #1144 (per-day heatmap
  cells), #1208 (pause editor Escape), #1212 — **RULED 2026-11-22,
  option (b)**: blank time field = anchor-time-only posture (no
  `time` in the override); the 09:00 mint is retired and the
  placeholder/doc change meaning. Stage-1 verifies the override's
  domain type can express absent-time; the small additive type
  change is authorized if needed (O-2 law). Ruling recorded on the
  issue (comment 7079171625). #1196 (quota seed typed path).
- **Habit editor follow-ups**: #1035 (nudge-schedule preview),
  #1048 (editor mirrors onto shared flows), #1049 (kiosk belt).
- **Misc gated**: #1225 (reload-lane row placement), #1234 (review
  walk's in-pane cursor), #1151 (capture accidental-commit recovery),
  #1152 (kiosk placement inventory for signal lanes), #1184 (p2: the
  quick-row budget is saturated — OP2/OP3 both added rows; a menu
  design decision), #1104 (p2: labels multi-toggle dropdown — design
  first per J5).

## E. Wave 5 — residue, docs, small

- **Gate/journey token residue**: #1181 (inline code spans defeat
  token parsing), #1186 (`--root` relative path bypasses the fixture
  prune), #1164 (the day-review plan's prose W-1 cell), #1217/#1236
  (the habit/small-gated plans' prose rows), #1237 (the seed echo at
  three sites), #1172 (three journeys members with private mount
  preambles), #1183 (the tombstone arm/confirm seam test).
- **Small fixes/engine**: #1211 (zero-duration snooze accepted at
  CLI, refused in GUI — unify), #1157 (finish the clean_ambient
  re-homing), #1158 (derive_sync_states materializes rows for
  unbound boards), #1137 (the diagnostics hop's Mutex cell → watch
  channel), #1095 (the LWW guard's whole-second strings), #1017
  (habit rows rebuild ModelRc every pass), #1185 (the doctor sweeps
  a fresh worktree off a stale local main), #1232 (connect prompts'
  stdin seam), #1216 (headless backend drops raw key events on modal
  grab), #1163 (insta filenames embed the binary name — constrains
  future consolidation), #1159 (ip16 pins the 60s suite floor).
- **Docs/compressed**: #1160 (the idle-box measurement rule),
  #1078 if still open (lld prereq), #892 (the #682 cross-reference —
  its condition likely lifted), #776 (the dead `boards use` verb —
  still open from round-2's recommendation!), #1103 (operator
  worktree cleanup note), #907 (the shellcheck gate — open since
  round 2).
- **Operator**: #867 (the hosted Deck curl matrix — still pending).

## F. Deferred (standing)

- **#1094** (cross-device same-second LogHabit merge) — the CRDT
  design round stays separate unless the operator promotes it.
- **Watches**: #988/#983/#987 (chrono posture), #975/#972/#977/#971
  (Slint upstream/menu decisions), #1021, #1052, #919, #980/#357
  (board-restore design), #1079 (lld memory footprint).
- **Flaky armed/watchlist**: #697/#650/#968 (recurrence-armed),
  #336/#287/#384/#883/#1020/#926 (no fresh recurrence).
- **Perf family**: #131/#137/#138/#139/#140/#141/#142/#146/#148/#152/
  #153/#154/#155/#157.
- **Feature-scale**: #122/#121/#127/#181/#184/#185/#191/#193/#195/
  #198/#213/#215/#216/#217/#218/#219/#224/#225/#444/#440/#494/#117/
  #639–#642/#680/#691/#700/#701/#715/#716/#723/#762/#769/#770/#775/#778/#782.
- **C-6 triage deferrals**: #807/#809/#812/#813/#819/#820.
- **#113/#114/#115** (operator intent gate, still pending).

## G. Round shape

~12–14 chunks in three waves: the bug wave (A, first — two p1s
justify priority), the flaky pass (B) and the mutation wave (C) as
the two test-focused middle waves, the feature follow-ups (D) as
journey-gated chunks, and the residue (E) as the light tail. The
round's two operator gates are **ruled** (2026-11-22): #1215 →
option (b) (wording amendment; no storage change), #1212 → option
(b) (blank = anchor-time-only; additive domain change authorized if
the type needs it) — both recorded on their issues and binding on
their chunks. Standard resource law: ≤3 concurrent, memory gate,
flock-deferral; #1159-adjacent re-measurements stay in dispatcher
hands.

Cut line if shrinking: keep A (the p1/p2 wave), B (the flaky
cluster), C (#1231 only), D's undo-family chunk (#1146/#1155 — the
round-4 features are incomplete without their surfaces), and E's
gate-residue chunk; defer the rest to round 6.

The dispatch plan follows on acceptance.

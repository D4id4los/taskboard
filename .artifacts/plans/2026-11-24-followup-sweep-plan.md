# Follow-up Sweep — Chunk E3, Round 6 — Inventory & Chunk Plan

Stage-1 deliverable for chunk E3 (dispatch plan
`.artifacts/plans/2026-11-24-backlog-reduction-round6-dispatch-plan.md`
§15/§16; scope report
`.artifacts/reports/2026-11-24-backlog-reduction-round6-scope-report.md`).
Branch: `feat/followup-sweep` (off fetched `origin/main`, df5ae1a0).
Every listed item was verified against the tracker **and** the code in
the worktree. Verdicts: **implement** (survivor, journey row cited) /
**dropped** (reason) / **scope-question** (dispatcher; not planned here).

## 1. Inventory

| Item | Tracker | Code check | Verdict |
|---|---|---|---|
| #1144-remainder (heatmap per-day cells) | CLOSED 2026-11-24, PR #1299 (D3b) | `HabitState.day_log` + `RecordHabitDayLog` + migration 0030 landed | **dropped** — closed in round 5. Two documented leniencies had no issue: filed as **#1340** (day-log device-local, no sync lane) and **#1341** (week/month undo divergence) |
| #1245 bulk restore rows | OPEN | No restore-all row anywhere (`restore_all`/`RestoreAll` absent); per-row restore only (labels overlay, habits context row) | **implement** — journey `.artifacts/plans/2026-11-22-undo-family-journeys-restore-clear-surfaces.md` §1 N2 / §2 R-6 (cited amendment) |
| #1246 ClearReminderRules CLI spelling | OPEN | No `rules clear` / `RulesClear` CLI verb; `StateCommand::ClearReminderRules` exists with editor-only surface | **implement** — same journey §2 R-4 (cited amendment; O-5 parity law) |
| #1248 kiosk supervisor remediation lane | OPEN | No supervisor-lane seam (desktop restart spawns daemon directly) | **scope-question** — its own row says it is "its own capability with its own kiosk journey delta"; that journey delta does not exist and a design round is not an amendment. Not planned |
| #1249 CLI remediation verbs (unlock / daemon restart) | OPEN | No `unlock`/`daemon restart` verbs; the typed semantics (stale-holder recheck, graceful-stop-then-detached-spawn, SelfPid refusal) exist GUI-side only | **implement** — journey `.artifacts/plans/2026-11-22-status-export-journeys-command-ux-plan.md` §1 non-goal #2 / R-2 (cited amendment: same accepted remediation executor, new terminal front) |
| #1152-remainder (kiosk signal lanes) | CLOSED 2026-11-24, PR #1224 + US-23 inventory pin | Status-bar display lanes not kiosk-gated; settings row enabled on kiosk chain | **dropped** — closed in round 5, inventory pinned (us23_s1/us23_s2) |
| #1227-remainder (branch-picker keyboard) | CLOSED, PR #1300 | `undo_picker_keyboard_cursor` wired (adapter.rs:1060); keyboard routing present | **dropped** — remainder resolved by round-5 PR #1300 |
| #1226-remainder (sibling payload labels) | CLOSED, PR #1300 | `undo_picker_branch_rows` carries payload context (adapter.rs:1074) | **dropped** — remainder resolved by round-5 PR #1300 |
| #1104-impl (labels picker multi-toggle) | OPEN | Picker still bespoke single-toggle; shared `Dropdown` still single-select | **scope-question** — J5 R-6 (`.artifacts/plans/2026-11-18-menus-remainder-ux-plan.md`) defers until a *dedicated multi-label-usability design*; §15 gates the impl on "if its design was accepted" — no such design exists. A design round is not an amendment. Not planned |

## 2. §16 verdict-only items

| Item | Tracker | Verdict |
|---|---|---|
| #1239 (re-base 27 goldens on enriched fixture) | OPEN | **filed forward to round 7** per §16 ("genuine survivors fold into E3 or are filed forward") — recommend round 7: the wholesale regen must be its own PR (single drift source, binary-heavy corpus diff) and must not ride a UI sweep; also sequenced *after* #1241 so the regen lands on a pinned encoder |
| #1241 (PNG encoding nondeterminism) | OPEN | **filed forward to round 7** — encoder-version/parameter investigation precedes any regen; grouping it with #1239 in one round-7 golden-hygiene lane keeps the single-drift-source law |
| #1290 (habits restore newest-tombstone-only) | OPEN | **implement** — defect verified in code (`roster.tombstoned_habits.first()` at `crates/taskboard-ui-slint/src/adapter.rs:5148`); folds into E3's restore commit (journey US-2, same undo-family plan). Menu real-estate counter-pressure (#1184 quick-row budget) applies — enumerate via the labels-overlay section shape |
| #1163 (insta snapshot names) | CLOSED 2026-11-23 | **dropped** — closed in round 5 |
| #1156-remainder (deep-history browsing) | OPEN | **no action** — the issue is itself the deliberate-bound audit record ("its own capability with its own motivation"); no defect in code, no journey row. Revisit only on demonstrated usage need |
| #1232-class leftovers (stdin test seam) | CLOSED 2026-11-23 | **dropped** — closed in round 5 |
| #1137 (diagnostics watch channel) | CLOSED 2026-11-21 | **verified closed** — tracker closed; no diagnostics Mutex cell remains. No new information; not reopened |
| #1133 (ConfigBatchError rename) | CLOSED 2026-11-21 | **verified closed** — `ConfigBatchError` no longer exists in `taskboard-domain` |
| #1097 (ci.yml bench_fixtures comment) | CLOSED 2026-11-21 | **verified closed** — `crates/taskboard-storage-sqlite/tests/suite/bench_fixtures.rs` exists and the ci.yml comment names it accurately |

## 3. Commit grouping (stage 2)

1. `feat(ui): bulk and enumerated restore rows for the tombstone rosters`
   — #1245 (restore-all rows, labels overlay + habits roster) and
   #1290 (habits restore enumerates the roster instead of
   `.first()`). Family journeys cited: undo-family
   `2026-11-22-undo-family-journeys-restore-clear-surfaces.md` §1 N2 /
   §2 R-6 / US-2 (amendments; no new plan). Armed-confirm ceremony per
   row is preserved; habits enumeration uses the labels-overlay
   section shape (quick-row budget respected).
2. `feat(app): CLI parity verbs — rules clear, unlock, daemon restart`
   — #1246 (tasks-rules-clear spelling of `ClearReminderRules`, O-5
   parity) and #1249 (`taskboard unlock` / `taskboard daemon restart`
   driving the existing GUI remediation executor verbatim — same
   stale-holder recheck, graceful-stop-then-detached-spawn, SelfPid
   refusal; semantics are reused, not forked). Journeys cited:
   undo-family §2 R-4; status/export
   `2026-11-22-status-export-journeys-command-ux-plan.md` §1 non-goal
   #2 / R-2 (amendments).

## 4. Scope questions for the dispatcher (not planned)

- **#1248** — kiosk supervisor remediation lane: needs its own kiosk
  journey delta (design round) before any implementation chunk.
- **#1104-impl** — labels picker multi-toggle dropdown: gated on a
  dedicated multi-label-usability design (J5 R-6); none accepted yet.

## 5. Risks

- Commit 1 touches the habits context row while #1184's quick-row
  budget work is a parallel concern — enumerated restore must not
  grow the quick-row count unboundedly (section/submenu shape).
- Commit 2's remediation verbs must reuse the GUI executor; any
  forked kill/rm semantics would reintroduce the folklore #1249
  exists to remove.
- No golden/PNG touchpoints in either commit (no drift risk).
- Estimated diff size: commit 1 ≈ 300–450 lines (UI adapter, slint
  surface, insta snapshots); commit 2 ≈ 250–400 lines (repl commands,
  executor wiring, PTY/CLI tests). Chunk E3 stays size M.

## 6. Backlog issues filed this stage

- **#1340** — habit day-log overlay is device-local, no sync lane
  (#1144 leniency 1).
- **#1341** — week/month undo divergence, day attribution survives
  undo (#1144 leniency 2).

# [Plan] UX Surface Census Stock-Take — Chunk 2: Remaining Surfaces + Consolidation + Report

- **Date**: 2026-11-14
- **Kind**: stage-1 planning chunk plan (dispatch-plan protocol, phase 3
  chunk 2). No application code; the deliverables are the four
  remaining census slices + consolidation + the diff-against-seed
  appendix in `docs/ux-surface-census.org`, and the stock-take report
  at `.artifacts/reports/2026-11-14-ux-surface-census-stocktake-report.md`.
- **Origin**: the stock-take dispatch plan
  `.artifacts/plans/2026-11-13-ux-census-stocktake-dispatch-plan.md`
  §1/§3 (binding; the census epistemic rule is restated in D-1); the
  framework plan
  `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md`
  §4.4/§6 phase 3; `docs/ux_planning.org` §2.4; chunk 1's plan
  `.artifacts/plans/2026-11-13-ux-census-stocktake-1-chunk-plan.md`
  (schema D-1…D-8 — applied **mechanically**, restated here only where
  chunk 2 extends them).
- **Precondition verified**: chunk 1's PR is merged — `origin/main`
  carries the census schema + the four task-flow slices at `6131bb4c`
  (branch `docs/ux-census-stocktake-1` deleted post-merge). Worktree
  `../taskboard-WT-docs-ux-census-stocktake-2` created with
  `git worktree add -b docs/ux-census-stocktake-2 … origin/main` after
  `scripts/worktree-doctor.sh` exited 0 (free: 174G, floor 25G).
- **Baseline SHA**: **`84c04a0b`** (= `origin/main` at plan time, the
  tree stage 2 checks out). Stated per-slice per chunk-1 D-7; seed
  cells keep `fb5863ed` / chunk-1 cells keep `dc290df2` — drift is
  expected and recorded, never normalized.
- **Status**: READY — stage 2 executes the method below; deviations
  need a recorded reason in its report.

## 1. Survey headline (read-only, at `84c04a0b`)

- **Review surface**: `review-view.slint` is deliberately read-only
  (rows carry no activation — `review-view.slint:1-5`); all
  affordances live in the walk overlay (15 callbacks,
  `review-walk-overlay.slint:115-129`) and the plan-next-day view
  (`plan-next-day-view.slint:44-52`; note — the dispatch prompt names
  `plan-next-day.slint`; the actual file is `plan-next-day-view.slint`,
  recorded deviation). The review/plan flow uses **no `UiRequest` at
  all**: it dispatches via `UiEffect` (`router.rs:30`,
  `WalkIntent` arms `adapter.rs:2144-2350`) into the engine verbs
  `UpdateTask` / `MoveTask` (via `PlannerHandoff`) / `RecordReflection`.
  `ContextId::Review` is declared read-only with zero bindings
  (`actions.rs:36`); 16 `Review*` ActionIds bind in
  `ContextId::ReviewWalk` (`actions.rs:390-425`); 5 `Plan*` ActionIds
  in `PlanNextDay` (`actions.rs:369-380`).
- **Settings surface**: talks **exclusively in `UiRequest`s** (28
  variants, `requests.rs:381-588`, every one carrying a
  `oneshot<Result<SettingsSnapshot, SettingsError>>` reply); zero
  `StateCommand`s. 20 callbacks in `settings-view.slint:643-696`
  → adapter seams `adapter.rs:10284-10480` → op fns `adapter.rs:5700-6201`.
  Facts inventory: `ConnectorInfo` (`requests.rs:51-62`),
  `BoardInfo` (`:80-95`), `SettingsSnapshot` (`:241-300` incl. the
  `SettingsFact` read-only inventory), `FilesBoardRow`/`FilesBoardAction`
  (`presentation.rs:5540-5650`). No `ContextId::Settings`-scoped verbs
  beyond `CONTROL_NAV` (`actions.rs:626-634`); `AppSettings` (open
  settings) is GLOBAL (`actions.rs:698-704`).
- **Alarm window** (`ui/alarm-window.slint`): exactly three affordances
  — `dismiss(int)`, `snooze(int)`, `cancel(int)`
  (`alarm-window.slint:32-34`) — sending `StateCommand::DismissReminder`
  / `SnoozeReminder{ALARM_WINDOW_SNOOZE=10min}` / `CancelAdhocAlarm`
  directly on an mpsc (`src/lib.rs:827-870`); **no ActionIds, no
  UiRequests**; mounted lazily iff not kiosk (`src/lib.rs:218-234`).
- **CLI**: full verb × twin classification surveyed (the stage-2
  working table lives in this plan's §5 annex); ~59 sub-family + 19
  top-level verbs classify as **GUI twins** (~36 verbs sharing a named
  engine `StateCommand`/`UiRequest` with a GUI affordance) or
  **CLI-only** (~28 verbs: pure reads, diagnostics, and the
  UI-unreachable engine verbs `CreateLabel/UpdateLabel/DeleteLabel/
  AssignLabel/UnassignLabel/RenameBoard/SetBoardColor/SetBoardArchived/
  ResetBoard/ReconcileByTitle/CreateBreakRule/DeleteBreakRule/
  UpdateHabit/DeleteHabit/UpdateReflection/DeleteReflection/
  ClearReminderRules`). One dead verb found: `boards use`
  (`cli.rs:358`) parses but its dispatch is unreachable
  (`commands/mod.rs:949,1949`) — filed as backlog **#776**; the census
  records it as CLI-only with the unreachable-dispatch evidence, not
  as a capability.

## 2. Files-roster state on the record (dispatch §3 item 4)

**State found: LANDED.** Round-7 chunk 3 merged into `origin/main`
**before** this chunk's baseline `84c04a0b`:

| commit | subject |
|---|---|
| `07b18e64` | feat(settings): the files-roster badge/sort/action fold (r7 D-7.3 W-2) |
| `6792c962` | feat(settings): the Stop-Sync request arm (r7 D-7.3 W-4) |
| `2f731a9e` | feat(settings): the Publish-to-Files request arm (r7 D-7.3 W-6) |
| `27b2e5f7` | feat(settings): the per-account files roster rows; the move picker retires (r7 D-7.3 W-3/W-5) |
| `e916609f` | docs(plan): the files-roster stage-2 bindings and the census slice |

Moreover the seeded *Settings (files sync)* slice was already updated
on that merge (`e916609f`): its cells read "implemented (r7 chunk 3)"
with real evidence and **the *planned-pending* markers have already
retired** — the code carries no `planned-pending` marker anywhere
(grepped: zero hits in `crates/` at `84c04a0b`).

**Resolution rule applied (dispatch §3):** landed → the Settings slice
merges the seeded files-sync slice **as it stands on `origin/main`**
(rulings `round-6`, `round-7 overturn`, `R-1`…`R-5`, `PC-10`, `PC-11`
preserved verbatim; the four `board.*` rows carry their implemented
evidence) and **extends** it to the full Settings surface at baseline
`84c04a0b`. One recorded provenance correction: the seeded slice's
stale provenance sentence ("This slice reflects the baseline reality at
fb5863ed: round-7 chunk 3 has not run") is superseded — the merged
slice states `Baseline: 84c04a0b` per D-7 with a note that the files
rows' evidence was photographed post-`e916609f`. This is a baseline/
provenance update, **not** a ruling alteration: the ruling table is
token-identical. The report's planned-pending section records:
*landed before baseline; markers retired; nothing carried*.

## 3. Recorded decisions

D-1 (epistemic rule), D-2's naming grammar, D-4's marker grammar,
D-6 (alarm window = its own slice), D-7 (baseline convention), and the
five-pass verb-inventory method apply **mechanically** from chunk 1's
plan — not restated. Chunk 2 settles only the surfaces' specifics:

### D-9 — Per-slice capability derivation (the four remaining surfaces)

Same five extraction passes (chunk-1 D-2), with the surface-specific
mapping evidence recorded per slice:

- **Review**: pass 2 (UiRequest) yields **zero rows** — the slice
  records that fact as a named finding ("the review flow is
  request-free; it dispatches `UiEffect`→`StateCommand`") rather than
  leaving pass 2 silent. Rows come from: the 16 `Review*` +
  5 `Plan*` ActionIds (pass 3, `ContextId::ReviewWalk`/`PlanNextDay`;
  `ContextId::Review` itself records `review.open` as its only row —
  the walk opener, `actions.rs:390`); the walk overlay's 15 +
  plan-next-day's 9 callbacks (pass 5, folded into the ActionId rows
  they route to — internal-commit folding per chunk-1 D-2 pass 3);
  the engine verbs the flow commits (`task.update_scheduled`,
  `task.move`, `reflection.record`) as `E`/`X` cells with the
  `WalkIntent` arm as evidence; and the read-only ledger
  (`review-view.slint:1-5`) as one `review.ledger_display` `D` row.
  Overlays (walk overlay, plan-next-day view) are recorded inside the
  Review slice per chunk-1 D-6 — the slice's view mounts them.
- **Settings**: pass 2 is the *primary* pass — every of the 28
  settings `UiRequest` variants gets its capability row (grouping:
  the 11 `SetFocus*`/knob `Set*` requests group under one
  `settings.knob_set` capability per the chunk-1 D-2 pass-2 polling
  precedent, with the variant list as cell evidence — one row per
  plumbing request would bury the matrix; the grouped row names every
  variant). Pass 3 yields `AppSettings` (open/close, GLOBAL) only.
  Pass 5 yields the display affordances: `connection.roster_display`
  (cards), `board.roster_display` (merged from the seed slice —
  same name, no re-derivation), `settings.facts_display`
  (`SettingsFact` read-only inventory), `settings.env_override_display`,
  and the files-roster badge/restart-caption rows the seed slice
  already carries. Pass 1: zero (no `StateCommand` from Settings —
  recorded as a named finding).
- **Alarm window** (own slice per chunk-1 D-6): three rows —
  `alarm.dismiss`, `alarm.snooze`, `alarm.cancel` — each `X` in the
  Alarm-slice matrix with `alarm-window.slint:32-34` +
  `lib.rs:827-870` evidence; `alarm.snooze` records the
  `ALARM_WINDOW_SNOOZE` 10-min constant; `alarm.cancel` notes its
  `StateCommand` twin. Display rows: `alarm.window_display` (row
  model/attention, `presentation.rs:2253-2290`). The slice records the
  kiosk mount gate (`src/lib.rs:218-234`) and the anchor-aware
  no-Done/Mute fact (D-P11 anchor, `alarm-window.slint:8-10`) as
  evidence notes, and cross-references `timer.create`/`timer.cancel`
  rows wherever they live (the timers-form ActionIds are another
  slice's rows — chunk 2 never re-derives another slice's rows,
  chunk-1 D-5).
- **CLI column completion**: the seven-column grammar already carries
  a CLI column in every slice; chunk 2 fills it for every row the
  slices own. The classification (twin vs CLI-only) is the survey's
  §5 annex table, re-verified by stage 2 against `cli.rs` at the
  baseline SHA (the annex is a planning aid, **not** census evidence —
  cells cite `cli.rs`/`commands/` `file:line` themselves).

### D-10 — The CLI-only rule (chunk-2 specific completion rule)

- **GUI twin**: the CLI verb and a GUI affordance share the same
  engine `StateCommand`/`UiRequest` → **one capability row**, the CLI
  cell citing the `cli.rs` verb line + the shared engine verb; the
  row lives in the slice owning the GUI surface (chunk-1 D-5), and
  the CLI cell is that row's seventh column. No duplicate row is
  coined for CLI.
- **CLI-only, entity-anchored**: the verb's engine verb has no GUI
  exposure (the UI crate never constructs it) or it is a pure read of
  an entity GUI surfaces own → a capability row in the slice owning
  that entity, CLI cell evidenced, GUI cells `—` (ruling) where an
  authority exists (e.g. the seed's habit rulings R-2/R-3) or `U` per
  D-4 otherwise. Chunk 1 already placed the habit ones; chunk 2 adds
  the remainder (`boards rename/color/archive/unarchive/reset/reconcile`,
  `labels add/edit/delete/list`, `tasks label/unlabel`,
  `tasks remind --clear/--list`, `tasks repeat` set-arm,
  `journal edit/delete`, `breaks add/remove/list`, `habits update/rm`
  — with per-verb evidence of UI-unreachability).
- **CLI-only, entity-less** (app lifecycle/diagnostics: `gui`,
  `status`, `daemon stop`, `config print/path`, `export`, `find`,
  `sync`): recorded in a short ***CLI-only register* sub-block**
  appended after the last slice — a table
  `| CLI verb | evidence (cli.rs:line) | note |` — not as matrix rows,
  because these capabilities' only column *is* the CLI (a matrix row
  of one cell defeats the surface-scoped grammar). The register is
  queryable like the unruled registers. `sync` gets a cross-reference
  note to its GUI twin ceremony (the `RequestSync` arm) even though it
  is registered.
- **Dead verb**: `boards use` is recorded in the CLI-only register
  with the unreachable-dispatch evidence (`commands/mod.rs:949,1949`)
  and the backlog pointer **#776** — it is *not* a capability row
  (nothing a user can do).

### D-11 — Consolidation checks (stage-2 self-check, recorded as evidence)

1. **No verb under two names**: extract every capability row name
   across all seven slices; `grep`/`sort`/`uniq -d` must return
   nothing. The `entity.` grammar (chunk-1 D-2) is the disambiguator;
   the merged seed rows (`board.roster_display` etc.) reuse the seed
   names verbatim.
2. **Ruling-ID uniqueness**: every ruling-table ID across the census
   (`R-*`, `PC-*`, `round-*`, `TD-*`…) is unique; `grep -o` over the
   ruling tables + `sort | uniq -d` empty. Seed and chunk-1 rulings
   are preserved token-for-token (stage 3 diffs them).
3. **Marker/register bijection**: every `U` marker has exactly one
   unruled-register row and vice versa, per slice (chunk-1 D-4),
   extended by the report-inventory reconciliation (D-13).
4. **Zero blank cells** in the four new slices' matrices.
5. **Cross-slice cell integrity**: a capability appearing in two
   slices carries the same name and non-contradictory cell values
   (e.g. `review.*` rows appear only in Review; `settings.*` knobs
   appear only in Settings; task verbs touched by Review's carry/file
   flow cite their Review evidence without duplicating the TaskDetail
   row).

### D-12 — The diff-against-seed appendix

Appended to the census after the last slice: a table
`| seed element | phase-3 fate | where it lives now | why |` covering —
the three feature-scoped seed slices dissolved into surface slices
(chunk-1 D-3, with their ruling blocks' new homes), the four chunk-1
slices authored fresh, the files-sync slice merged into Settings with
its planned-pending resolution, the new slices' capability additions
vs the seed's row set, and the status flip. Rationale column cites the
decision (chunk-1 D-2/D-3/D-6, this plan's D-9/D-10). No seed ruling
text re-quoted — IDs and homes only.

### D-13 — The stock-take report outline

`.artifacts/reports/2026-11-14-ux-surface-census-stocktake-report.md`:

1. **Method** — the verb-inventory five passes + mapping evidence,
   citing chunk-1 D-2 and this plan's D-9; the baseline SHAs and the
   files-roster state record (§2 above, with the commit table).
2. **Per-surface summaries** — one short section per surface (the
   seven columns + alarm window slice), with capability counts per
   surface (rows, `D`/`E`/`C`/`X` mix, `U` count, CLI-only count) and
   each slice's named findings (e.g. Review's request-free flow,
   Settings' request-only flow).
3. **The unruled-observation inventory** — phase 4's machine-readable
   input: **one flat markdown table, one row per `U` marker in the
   entire census** (all seven slices — chunk 1's four included;
   reconciled by check D-11.3):

   `| surface | capability | evidence-of-absence | suspected reason (if known) |`

   The first three columns are the chunk-1 D-4 register columns
   (verbatim from the registers). The fourth column is the dispatch's
   "suspected reason if known" — **reconciliation with D-4 recorded**:
   chunk-1 D-4 forbids a reason-as-judgment *in the census cell*; the
   report column is phase-4 triage input and carries only a
   *hypothesis sourced from a recorded artefact* (a code comment, a
   plan, an issue number — cited), or empty if none exists. No
   invented reasons.
4. **The planned-pending record** — the files-roster state (landed;
   SHAs; markers retired; nothing carried), per §2.
5. **Consolidation results** — the D-11 check commands and outcomes.
6. **The diff-against-seed appendix pointer** — the census appendix is
   the artefact; the report summarizes its counts.
7. **Honest limits** — what a census cannot see: label quality,
   layout, journey ergonomics (phase 4's walkthrough territory); the
   display-affordance granularity judgment call (chunk-1 D-2 pass 5's
   rule of thumb); anchor-rot risk between slice baselines.
8. **Backlog issues opened during phase 3** — #776 (+ any stage 2/3
   opens, appended).

### D-14 — Commit sequencing (every commit standalone, census parseable after each)

1. `docs(plan): the ux-census stocktake chunk-2 plan` — this plan.
2. `docs(ux): the review surface slice from the verb inventory` —
   Review slice (view + walk overlay + plan-next-day) appended.
3. `docs(ux): the settings surface slice from the verb inventory` —
   the seed files-sync slice merged in (rulings verbatim, provenance
   correction per §2) + the full Settings rows; the old seed slice
   heading dissolves in the same commit (no orphaned blocks).
4. `docs(ux): the alarm-window surface slice from the verb inventory` —
   per chunk-1 D-6.
5. `docs(ux): the cli column completion and the cli-only register` —
   CLI cells across all slices + the CLI-only register sub-block.
6. `docs(ux): the census consolidation and the diff-against-seed appendix` —
   D-11 checks applied, the appendix appended, the status line flips
   to *full census complete (phase 3)*.
7. `docs(report): the ux surface census stock-take report` — D-13.

Commits 2–5 each leave the census self-consistent; commit 6 is the
only one allowed to flip the status (it is the one that makes the
claim true). Amend/reword before the PR opens; no duplicate-subject
fixups.

## 4. Scope & acceptance (mirrors dispatch §3, operationalized)

- **In**: the four slices (Review, Settings, Alarm window, CLI
  completion), the CLI-only register, consolidation + the
  diff-against-seed appendix, the status flip, the report.
- **Out (non-goals)**: any code; any UX journey plans; any remediation;
  any new rulings; any triage judgment beyond recording; re-deriving
  chunk-1 slices' rows (their CLI *cells* are filled by commit 5, but
  rows are never re-derived — chunk-1 D-5).
- **Stage-2 self-check** (recorded as evidence): the D-11 checks with
  commands + outcomes; ≥10 new cell anchors spot-resolving at
  `84c04a0b`; seed + chunk-1 ruling text token-identical
  (`git show origin/main:docs/ux-surface-census.org` diff); the
  report's inventory reconciled 1:1 with the census markers.

## 5. Annex — CLI verb × twin classification (planning aid, not census evidence)

The stage-2 working table from this plan's survey (at `84c04a0b`;
stage 2 re-verifies each row against `cli.rs` and cites `file:line`
in the census cells):

- **GUI twins** (shared engine verb, one capability row in the owning
  slice): `connect`/`login`(deprecated alias)→`SaveConnection`,
  `logout`→`RemoveConnection`, `boards move`→`MoveBoardToFiles`,
  `boards list`→`ListRemoteBoards`, `boards select`→`AttachConnector`,
  `boards create`→`CreateLocalBoard`, `boards publish`→`PublishBoard`/
  `PublishBoardToFiles`, `boards delete`→`DeleteBoard` (GUI
  `BoardDelete`, `view_state.rs:2933`), `boards unbind`→`DetachConnector`,
  `stacks add/rename/delete/move`→`CreateStack`/`RenameStack`/
  `DeleteStack`/`MoveStack`, `tasks add/done/move/edit/delete`→
  `CreateTask`/`SetTaskDone`/`MoveTask`/`UpdateTask`/`DeleteTask`,
  `tasks remind`(+`--before/--insistence/--remove/--off/--on`)→
  `AddReminderRule`/`RemoveReminderRule`/`SetTaskRemindersMuted`,
  `tasks repeat` (pause arm)→`PauseSeries`, `tasks undo/redo`→`Undo`/
  `Redo`, `capture`→`CaptureTask`, `timers add/cancel`→
  `CreateAdhocAlarm`/`CancelAdhocAlarm`, `track start/stop/log/adjust/
  delete`→`StartTracking`/`StopTracking`/`LogTime`/`AdjustTimeEntry`/
  `DeleteTimeEntry`, `habits add/log/adjust`→`CreateHabit`/`LogHabit`/
  `AdjustHabitLog`, `journal add`→`RecordReflection`, `sync`→
  `RequestSync` (registered CLI-only, cross-referenced — D-10).
- **CLI-only, entity-anchored** (rows in the owning slice per D-10):
  `boards rename/color/archive/unarchive/reset` (engine verbs
  UI-unreachable), `boards reconcile`→`ReconcileByTitle`,
  `labels add/edit/delete/list` (`CreateLabel`/`UpdateLabel`/
  `DeleteLabel` UI-unreachable; list a pure read), `tasks label/
  unlabel` (`AssignLabel`/`UnassignLabel`), `tasks remind --clear/
  --list`, `tasks repeat` set-arm, `breaks add/remove/list`
  (`CreateBreakRule`/`DeleteBreakRule` UI-unreachable),
  `habits update/rm` (`UpdateHabit`/`DeleteHabit` UI-unreachable),
  `journal edit/delete` (`UpdateReflection`/`DeleteReflection`).
  `tasks list`, `timers list`, `track status/list`, `journal list/
  show`, `habits list`, `labels list` are pure-read rows of the same
  rule.
- **CLI-only, entity-less** (the D-10 register): `gui`, `status`,
  `daemon stop`, `config print/path`, `export`, `find` (shares the
  palette's ranking function, no engine verb — chunk-1 plan §5 noted
  the naming asymmetry; recorded as an evidence note, not a judgment),
  `sync`, and the dead `boards use` (#776).

## 6. Risks

- **The Settings matrix is request-shaped, not command-shaped**: 28
  `UiRequest` rows (11 grouped) is the census's largest slice; the
  grouping decision (D-9) is the load-bearing judgment call — stage 2
  must name every grouped variant in the cell evidence or completeness
  is unverifiable.
- **CLI classification drift**: the annex was derived at `84c04a0b`;
  if anything lands on `origin/main` mid-chunk (round-7 re-works are
  explicitly concurrent-safe per dispatch §1), the *re-verify at
  baseline* rule holds — stage 2 never cites a moving HEAD.
- **CLI-only GUI-cell semantics**: a CLI-only row's GUI cells are
  absences (the verb is unreachable from every GUI surface) — they
  are `U` markers needing register rows, *not* blanks. Volume risk:
  ~20 CLI-only rows × 7 GUI columns; the register will grow
  accordingly. Expected and fine (chunk-1 D-4: it is phase 4's input
  by design) — but stage 2 sweeps honestly per chunk-1's risk note.
- **Token-for-token ruling preservation through the Settings merge**:
  the seed slice moves wholesale; only the provenance sentence
  changes (§2). Stage 3 diffs the ruling tables.
- **Gate discipline**: docs-only diff; the full AGENTS §10 suite still
  runs via `scripts/run-quality-gates.sh`; a pre-existing unrelated
  failure means STOP and report.

## 7. Out-of-scope findings → backlog issues

- **#776** — dead CLI verb `boards use` (parsed, dispatch unreachable;
  opened during this survey, `dx` label).
- No other issues opened. Chunk-1 plan §5's candidates were judged
  here: `find` vs the search palette's naming asymmetry is an evidence
  note in the CLI register (naming, not a gap in UX coverage — no
  issue); `login`/`connect` duplication is already self-documented as
  deprecated in the parser (`cli.rs:56`) — no issue.

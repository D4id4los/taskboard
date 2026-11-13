# [Plan] UX Surface Census Stock-Take — Chunk 1: Census Schema + Task-Flow Surfaces

- **Date**: 2026-11-13
- **Kind**: stage-1 planning chunk plan (dispatch-plan protocol, phase 3
  chunk 1). No application code; the deliverable is the re-organized
  `docs/ux-surface-census.org` plus the two dispatch-plan records.
- **Origin**: the stock-take dispatch plan
  `.artifacts/plans/2026-11-13-ux-census-stocktake-dispatch-plan.md` §1/§2
  (binding; its census epistemic rule is restated in D-1); the framework
  plan `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md`
  §4.4/§6 phase 3; `docs/ux_planning.org` §2.4 (the census law); the
  seeded census `docs/ux-surface-census.org` (phase-2 output); the
  comparison plan
  `.artifacts/plans/2026-11-13-round7-ux-pilot-comparison-plan.md`
  (zero-deviation verdict; PC rulings are census authority).
- **Precondition verified**: both phase-2 artefacts are on
  `origin/main` — census seed at `dc290df2`, comparison plan at
  `032f5c90`. Worktree `../taskboard-WT-docs-ux-census-stocktake-1`
  created with `git worktree add -b docs/ux-census-stocktake-1` after
  `scripts/worktree-doctor.sh` exited 0 (free: 176G, floor 25G).
- **Status**: READY — stage 2 executes the method below; deviations
  need a recorded reason in its report.

## 1. Verb-inventory headline counts (the survey, read-only)

Derived by mechanical extraction at the working-tree baseline
`dc290df2` (= `origin/main` at plan time). Exact per-slice counts are
stage 2's job; these are the completion-scale anchors:

| Registry | Source | Count |
|---|---|---|
| `StateCommand` variants | `crates/taskboard-domain/src/messages.rs:21` | **56** |
| `UiRequest` variants | `crates/taskboard-ui-slint/src/requests.rs:344` | **29** |
| `ActionId` variants | `crates/taskboard-ui-slint/src/actions.rs:214` | **115** |
| `ContextId` variants | `crates/taskboard-ui-slint/src/actions.rs:26` | **18** |
| Adapter `install_seam!` sites | `crates/taskboard-ui-slint/src/adapter.rs` | **221** |
| CLI top-level verbs | `crates/taskboard-app/src/cli.rs:29` (`enum Command`) | **19** (incl. `Gui`, `Status`, `Logout`) |
| CLI sub-family verbs | `BoardsCmd` 14, `TasksCmd` 11, `TrackInvocation` 7, `HabitsInvocation` 6, `RemindInvocation` 4, `StacksCmd` 4, `LabelsCmd` 4, `ReflectionInvocation` 3, `ConfigCmd` 2, `BreaksInvocation` 2, `TimersInvocation` 2, `DaemonCmd` 1, `Sync`/`Export`/`Find`/`Capture` inline | **~59 + 4** |
| View/composition mount set | `ui/views/` (8 views), `ui/compositions/` (18 compositions) | 26 `.slint` files |

The count discrepancy *is* the finding the census exists to record:
115 action ids and 221 seams far outnumber the 56 engine commands —
most surface affordances are navigation/display/internal-commit
plumbing, not engine verbs. The schema must therefore record
display-only affordances as first-class capabilities, not noise.

## 2. Recorded decisions

Each decision below is binding for stage 2 (and chunk 2's stage 1,
which applies them mechanically). Deviations need a recorded reason.

### D-1 — The census epistemic rule (restated, binding)

Phase 3 **records reality; it never invents rulings**. Every
populated cell cites `file:line` at the slice's recorded baseline SHA
or an existing test name. Every ruling carries an existing authority.
An absence no authority ever ruled on is an *unruled observation*
(D-4), never dressed up as a ruling. (Dispatch §1, verbatim posture.)

### D-2 — The verb-inventory method

Capabilities are enumerated from the implementation in **five
extraction passes**, then mapped to surfaces:

1. **Engine verbs**: every `StateCommand` variant in
   `crates/taskboard-domain/src/messages.rs`. Each becomes a
   capability row, named `entity.verb` per the grammar below.
2. **Query/side verbs**: every `UiRequest` variant in
   `crates/taskboard-ui-slint/src/requests.rs` that a surface can
   trigger. `UiRequest`s that are pure polling (e.g. tick/watch
   requests) are grouped under one `app.<poll>` capability per
   family rather than one row per request — a row per request would
   bury the matrix in plumbing. The grouping is named in the slice.
3. **UI affordances**: every `ActionId` in
   `crates/taskboard-ui-slint/src/actions.rs`, bucketed by its
   `ContextId`. Internal-commit action ids (the
   `HabitCreateCommit`/`SeriesEdit*` family — comparison PC-2/PC-7)
   are *not* separate capability rows: they fold into the
   user-facing capability they commit (their identity is preserved
   in the cell evidence).
4. **CLI verbs**: every top-level `Command` variant plus each
   sub-family variant in `crates/taskboard-app/src/cli.rs` and
   `src/commands/`. Each is either a GUI-exposed capability's CLI
   twin (same row, CLI column) or a **CLI-only capability**
   (recorded as such; chunk 2 completes the CLI column per its
   plan, but chunk 1 already records CLI-only verbs it encounters
   for its four surfaces' entities).
5. **Display-only affordances**: badges, echoes, counts, and status
   text derived from `ui/views/`, `ui/compositions/`, and the facts
   globals the adapter pushes (`models.rs`/`presentation.rs`
   snapshot fields). Each display-only capability gets a `D` cell;
   the row name follows the same `entity.verb` grammar with a
   display verb (`display`, `badge`, `echo`, `preview`).

**Naming grammar**: `<entity>.<verb[_qualifier]>`, all lowercase,
snake_case, where *entity* is the domain object the verb acts on
(`task`, `habit`, `board`, `stack`, `label`, `connection`, `timer`,
`break_rule`, `track_entry`, `reflection`, `alarm`, `capture`,
`app`) and *verb* is the engine/clap word where one exists
(`task.move`, `board.move_to_files`), otherwise the affordance's
natural verb (`task.recur_display`). One capability = one row;
a capability shared across slices appears under the **same name**
in each slice that owns one of its surfaces (consolidation check,
dispatch §3 chunk 2). *Naming authority*: the engine/clap word wins
over invention; where neither exists, the affordance label's
comment in the `.slint` file wins.

**The mapping step**: for each derived verb, the evidence that a
surface exposes it is one of — (a) an `install_seam!` site or
`on_*` handler in `adapter.rs` whose section scope names the view
(221 sites, `tests/seam_inventory.rs` as the index); (b) the
`.slint` mount site (`ui/views/<view>.slint`,
`ui/compositions/<composition>.slint` with line range); (c) an
`ActionId` bound in the surface's `ContextId` (with keymap preset
binding if keyboard-reachable); (d) a CLI sub-command. A verb with
evidence in none of this chunk's four surfaces and none claimed by
chunk 2's surfaces is still listed (D-5) so no capability is lost.

### D-3 — Slice re-organization (feature-scoped → surface-scoped)

The seed's three feature-scoped slices dissolve into surface-scoped
slices: **habits → the Today slice**, **recurrence → the TaskDetail
slice**, **files sync → the Settings slice (chunk 2, untouched this
chunk)**. Mechanics:

- Each surface slice owns one capability×surface matrix with the
  full fixed column set (Board / TaskDetail / Today / Review /
  Settings / Capture / CLI) — the columns never change, only the
  slice's *ownership* of rows changes.
- **Every seed ruling is preserved verbatim** (token-for-token —
  stage 3 diffs them): R-1/R-2/R-3 (habits), R-1/R-2 (recurrence —
  re-homed as `TD-R-1`/`TD-R-2`? **No — IDs stay exactly as
  seeded**: `R-1`, `R-2` stay `R-1`, `R-2` but are disambiguated by
  their ruling block's slice heading, which the seed already
  provides (`Rulings (Today/habits)` → `Rulings (Today)` etc.).
  PC-1/PC-2/PC-5/PC-7/PC-8, the round-6 ruling, and the round-7
  overturn all keep their IDs and authority links untouched.
- **Provenance**: each merged row and each ruling block carries its
  seed `Origin:` line verbatim — the seed's Origin/authority text
  survives under the new slice heading, prefixed
  `(merged from seed slice: <seed heading>)`. The census stays
  auditable back to which pilot produced each ruling.
- The files-sync slice is **not** touched in chunk 1: its rows move
  to the Settings slice in chunk 2 (dispatch §3), where its
  *planned-pending* markers are resolved or carried. Chunk 1's
  four slices must not duplicate its rows.

### D-4 — Unruled-observation marker grammar

A cell that is *absent* (no affordance) and has **no authority that
ever ruled** the absence records:

```
U — unruled (phase-4 triage)
```

written in the cell as `U` with the row's evidence-of-absence in
the slice's *unruled register* sub-table: one row per marker —
`| capability | surface | evidence-of-absence (file swept, no seam/mount/verb) | date |`.
The `U` marker is deliberately distinct from `—` (deliberately
absent, needs a ruling — seed convention) and from blank (not yet
examined — a schema defect; blank cells must not survive stage 2's
self-check). Queryable: `grep -c 'U — unruled'` and the register
tables are the machine-readable inventory chunk 2's report
consumes. An `U` never carries a story ID, a ruling ID, or a
reason-as-judgment — only the sweep evidence.

### D-5 — Completeness for this chunk's four surfaces

**One matrix per surface slice** (not one global matrix): each of
Board / TaskDetail / Today / Capture gets a slice whose rows are
*all capabilities the verb-inventory derives as reachable from that
surface*, plus each capability's other-surface cells filled from
cross-surface evidence where it exists (e.g. `habit.create`'s Board
column shows `— (R-1)` from the merged ruling). Consequences:

- "Complete" = **every capability derived from any of the five
  extraction passes whose exposure lies (even partly) in one of the
  four surfaces has a row, and every cell in that row is either
  evidenced, `— (ruling)`, or `U`**. Blank cells are schema
  defects.
- Verbs exclusive to chunk-2 surfaces (Review, Settings, alarm
  window) do **not** appear as rows in chunk 1's slices — but if a
  chunk-1 verb has a cell in a chunk-2 surface's column (e.g.
  `task.update` from Settings), that cell is filled with evidence;
  chunk 2 never re-derives another slice's rows.
- CLI-only verbs for entities this chunk's surfaces touch (e.g.
  `habits update`, `habits rm`) appear as rows with the CLI cell
  evidenced and GUI cells per D-4 (the seed's R-2/R-3 rulings cover
  the habit ones; others become `U` or rulings where authority
  exists).

### D-6 — Alarm-window representation

The alarm window gets **its own slice block** in chunk 2, not a
matrix column. Reasons: it is a mount-time overlay outside the
fixed seven-column surface grammar (the columns are *where a user
acts*, and the alarm window is a surface a user is *moved to* by a
timer, with its own affordance set — the same shape as Capture's
overlay, which is a slice not a column); a column would force
alarm-window cells on every row of every slice (~25+ rows of
mostly-`—`). Convention settled now so chunk 2 executes
mechanically: `* Slice: Alarm window` with the same evidence
convention, its own rulings block, and cross-references from
`alarm.*`/`timer.*` capability rows wherever those verbs live in
other slices' matrices. Overlays generally (recur-editor,
due-editor, focus, capture-overlay, search-palette) follow the
same rule: **an overlay is recorded inside the surface slice whose
view mounts it**, with the overlay's file cited as the cell
evidence — only the alarm window warrants its own slice because
nothing else mounts it.

### D-7 — Baseline-SHA convention

One SHA per slice block, stated in the block's Origin/provenance
paragraph: `Baseline: <sha>` — the tree whose `file:line` anchors
the slice's cells. Chunk 1 states `dc290df2` for all four new
slices (the tree stage 2 checks out). The seed's single documented
baseline `fb5863ed` stays attached to the seed-derived cells it
anchors — **drift between slices is expected and recorded, not
normalized**: a cell never re-cited at a newer SHA just to unify
(anchors are verified at the SHA stated in their block; stage 3
spot-checks with `git show <sha>:<file>`). Where the same
capability carries cells anchored at two SHAs (a merged seed cell
at `fb5863ed` beside a new cell at `dc290df2`), each cell keeps
its own block's SHA and the block's provenance line notes the
seed-baseline inheritance. Chunk 2 states its own SHA at its
execution time; the *diff-against-seed appendix* explains any
reality drift the two SHAs expose.

### D-8 — Commit sequencing (every commit standalone)

1. `docs(plan): the ux-census stocktake chunk-1 plan` — this plan.
2. `docs(plan): record the phase-2 pilot dispatch plan and the phase-3 dispatch plan` — both
   dispatch plans committed for the record (dispatch §2 item: the
   untracked phase-2 pilot dispatch plan rides the PR).
3. `docs(ux): the census schema decisions and the verb-inventory method` —
   the census header rewrite: status line → *phase 3 in flight:
   task-flow surfaces complete*; the schema conventions (D-1,
   D-4 grammar, D-6 alarm scheme, D-7 baseline rule, the column
   set) added to the header; the seed slices' dissolution begins
   (ruling blocks moved wholesale, verbatim) — this commit leaves
   the census internally consistent (no slice references a
   dissolved block).
4. `docs(ux): the board surface slice from the verb inventory`
5. `docs(ux): the task-detail surface slice from the verb inventory`
6. `docs(ux): the today surface slice from the verb inventory`
7. `docs(ux): the capture surface slice from the verb inventory`

Commits 4–7 each leave the census parseable and self-consistent
(no capability row appears twice, no ruling block orphaned).
Amend/reword before the PR opens; no duplicate-subject fixups.

## 3. Scope & acceptance (mirrors dispatch §2, operationalized)

- **In**: the census schema header; the four surface slices (Board,
  TaskDetail, Today, Capture); all seed rulings verbatim; the two
  dispatch-plan records; the status line update.
- **Out (non-goals)**: Review/Settings/alarm/CLI slices (chunk 2);
  the stock-take report; any code; any new rulings; any UX journey
  plans; resolving *planned-pending* files-sync cells.
- **Stage-2 self-check** (recorded as evidence): (a) zero blank
  cells in the four slices; (b) every `U` marker has a register
  row and vice versa; (c) `git show origin/main:docs/ux-surface-census.org`
  diffed for ruling text — token-for-token preservation; (d) each
  new cell's anchor spot-resolves at its block's baseline SHA.

## 4. Risks

- **Anchor rot between SHAs**: seed cells cite `fb5863ed` lines;
  the working tree may have drifted since (nothing merged after
  `dc290df2` — verified, low risk). Mitigation: stage 2 verifies at
  the stated SHA, never at HEAD.
- **Verb-name collisions on consolidation**: `move` exists for
  task, stack, and board; the `entity.` prefix disambiguates, but
  stage 2 must not coin two names for one capability across slices
  (dispatch §3's consolidation check is the backstop).
- **Display-only capability granularity**: the fifth extraction
  pass (D-2) is the most judgment-prone; over-enumeration bloats
  the matrix. Rule of thumb recorded: a display affordance gets a
  row only if a user *can perceive it as a capability*
  (badge/echo/preview), not per rendered text fragment.
- **The `U` register could balloon**: expected and fine — it is
  phase 4's input by design; but stage 2 must sweep honestly
  (evidence-of-absence names the files swept), not mark `U` as a
  shortcut where a ruling authority actually exists.
- **Gate discipline**: docs-only diff, but the full AGENTS §10
  suite still runs via `scripts/run-quality-gates.sh`; a
  pre-existing unrelated failure means STOP and report (dispatch
  prompt 1.2 step 4).

## 5. Out-of-scope findings → backlog issues

None opened during this survey. Candidates noted for stage 2 (each
judged there against the standing authorization, not pre-filed):
the CLI's `find` verb has no GUI twin beyond the search palette
(naming asymmetry, `find` vs the palette's ranking — same engine
ranking, two names); `login`/`connect` duplication is already
self-documented as deprecated. If stage 2 finds more, it files
`backlog:` issues and lists the numbers in its report.

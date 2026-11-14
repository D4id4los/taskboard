# [Report] UX Surface Census Stock-Take (phase 3)

- **Date**: 2026-11-14
- **Kind**: phase-3 stock-take report (framework plan
  `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md`
  §6 phase 3; dispatch
  `.artifacts/plans/2026-11-13-ux-census-stocktake-dispatch-plan.md`
  §3). Backward-looking analysis of the census completion — the
  companion artefact is the completed census
  `docs/ux-surface-census.org` (branch `docs/ux-census-stocktake-2`).
- **Stage**: stage-2 implementation report (chunk 2: remaining
  surfaces + consolidation + report). Chunk 1 (schema + task-flow
  surfaces) landed via its own PR; its report-equivalent evidence
  lives in its chunk plan and the census header.
- **Epistemic posture**: the census records reality and never
  invents rulings (dispatch §1); this report inherits that posture —
  every claim cites the census or a `file:line` anchor at the
  recorded baseline SHAs.

## 1. Method

The verb-inventory method (chunk-1 plan D-2, applied mechanically per
chunk-2 plan D-9): five extraction passes over the implementation —

1. engine `StateCommand` variants (`crates/taskboard-domain/src/messages.rs`);
2. `UiRequest` variants (`crates/taskboard-ui-slint/src/requests.rs:363`)
   grouped per polling/knob family;
3. `ActionId` affordances bucketed by `ContextId`
   (`crates/taskboard-ui-slint/src/actions.rs`);
4. CLI verbs (`crates/taskboard-app/src/cli.rs` + `commands/`);
5. display-only affordances from `ui/views/`, `ui/compositions/`, and
   the facts globals the adapter pushes (`models.rs`/`presentation.rs`).

Each derived verb is mapped to its surfaces by adapter-seam /
mount-site / context-binding / CLI evidence, named `entity.verb`
snake_case with the engine/clap word as naming authority, and placed
in the surface slice whose view mounts it (overlays ride their
mounting view; the alarm window is its own slice, chunk-1 D-6).

**Baseline SHAs** (chunk-2 plan D-7; drift expected and recorded,
never normalized):

| slice | baseline | note |
|---|---|---|
| Board, Task detail, Today, Capture (chunk 1) | `dc290df2` | unchanged by chunk 2 (rows never re-derived) |
| Review, Settings, Alarm window, CLI completion (chunk 2) | `84c04a0b` | the tree stage 2 checked out; all `file:line` anchors verified at this SHA |
| seed-derived cells | `fb5863ed` (updated `e916609f` for the files rows) | inherited, kept |

**The files-roster state on the record** (dispatch §3 item 4;
chunk-2 plan §2): round-7 chunk 3 LANDED before the baseline —
commits `07b18e64` (roster fold), `6792c962` (Stop-Sync arm),
`2f731a9e` (Publish-to-Files arm), `27b2e5f7` (per-account roster
rows; move picker retired), `e916609f` (seed-slice evidence update).
Resolution rule applied: the Settings slice merged the seeded
files-sync slice *as it stands on `origin/main`*, the four `board.*`
rows carry their implemented evidence, the *planned-pending* markers
retired with that merge (zero `planned-pending` markers exist in
`crates/` or the census at `84c04a0b`), and the seed's stale
provenance sentence was superseded (a baseline/provenance update;
the ruling table is token-identical). **Nothing carried.**

## 2. Per-surface summaries

Matrix-only counts (register rows excluded); mix = populated
`D`/`E`/`C`/`X` cells; `U` = unruled-absence cells.

| surface slice | capability rows | D/E/C/X mix | U cells | CLI-only rows | register rows | named findings |
|---|---|---|---|---|---|---|
| Board | 24 | C 6, E 28, X 9, D 9 | 115 | 0 | 24 | chunk-1 slice, untouched |
| Today | 17 | C 2, E 17, X 2, D 6 | 75 | 3 (break_rule.*) | 14 | absorbed the CLI-only break-rule rows in chunk 2 |
| Capture | 11 | C 1, E 43, D 2 | 31 | 0 | 8 | chunk-1 slice, untouched |
| Task detail | 33 | C 1, E 53, X 6, D 11 | 147 | 8 (label.*, task.label_*, track status/list) | 31 | absorbed the CLI-only label/track rows in chunk 2 |
| Settings | 17 | C 2, E 20, X 5, D 4 | 72 | 0 | 14 | request-only surface (zero `StateCommand`s, pass-1 finding); 29 of 31 `UiRequest` variants are Settings-triggered; the 18 `Set*` knob requests group under one `settings.knob_set` row naming every variant (D-9 grouping) |
| Review | 24 | C 0, E 31, X 1, D 4 | 132 | 2 (reflection.edit/delete) | 24 | request-free flow (zero `UiRequest`s, pass-2 finding) — dispatches `UiEffect`→`StateCommand` (`MoveTask`/`UpdateTask`/`RecordReflection`); `ContextId::Review` itself binds nothing (read-only ledger, `actions.rs:33-36`) |
| Alarm window | 11 | C 0, E 65, X 0, D 6 | 6 | 0 | 6 | alarm window + notification surface; direct-mpsc `StateCommand` dispatch (no `UiRequest`s, no `ActionId`s on the window); D-P11 anchor-aware shape (no Done/Mute) recorded as a finding, not a ruling |
| **total** | **137** | — | **578** | **13** | **121** | — |

CLI-only *entity-less* verbs additionally live in the census's
**CLI-only register** (9 entries: `gui`, `status`, `daemon stop`,
`config print`, `config path`, `export`, `find`, `sync`, and the
dead `boards use`, backlog #776) — recorded there rather than as
matrix rows per chunk-2 D-10.

Named findings of note:

- **Review is request-free; Settings is command-free** — the two
  surfaces sit at opposite ends of the UI's two dispatch lanes; the
  census records both as findings, not as virtues.
- **The `settings.knob_set` grouping** is the load-bearing judgment
  call (chunk-2 plan §6): every grouped variant is named in the
  cell evidence so completeness stays verifiable.
- **Column semantics for the alarm window**: the always-on-top
  second window is operable over every surface when it shows, so its
  rows carry the same evidence cell in every GUI column (the
  notification-surface global-overlay precedent) — an absence
  register would have recorded six false absences per row.
- **Chunk-1 sweep correction (consolidation, D-11.5)**: chunk 1's
  registers recorded `task.move`/`task.scheduled_edit` as absent on
  Review; the walk's commit funnel (FileCommit → `MoveTask`
  `adapter.rs:2203-2227`, carry date commit → `UpdateTask`
  `adapter.rs:2252-2299`) is a task-move/scheduled-edit seam. The
  two cells were consolidated to cite the walk evidence and the two
  register entries note the retirement. Chunk-1 rows were otherwise
  never re-derived.

## 3. The unruled-observation inventory (phase 4's input)

One flat table, one row per census `U` register entry (121 entries
across the seven slices; reconciled 1:1 against the `U` markers by
consolidation check D-11.3 — every `U` cell's surface is covered by
exactly the register row below, and vice versa). The first three
columns are the chunk-1 D-4 register columns, verbatim from the
registers; the fourth column is the dispatch's *suspected reason if
known* — reconciliation with D-4 recorded: the census cells carry no
reason-as-judgment (D-4), and this column carries only a hypothesis
sourced from a recorded artefact (cited inline) or stays empty. No
invented reasons.

| surface | capability | evidence-of-absence surfaces | evidence-of-absence detail | suspected reason (if known, sourced) |
|---|---|---|---|---|
| Board | board.create | TaskDetail, Today, Review, Settings, Capture | Sweep-T/Y/R/S/C: no board-create seam or mount in any swept surface |  |
| Board | board.rename | TaskDetail, Today, Review, Settings, Capture | Sweep-T/Y/R/S/C: no board-rename seam or mount |  |
| Board | board.delete | TaskDetail, Today, Review, Settings, Capture | Sweep-T/Y/R/S/C: no board-delete seam or mount |  |
| Board | board.select | TaskDetail, Today, Review, Settings, Capture | Sweep-T/Y/R/S/C: no board-switch seam or mount outside the board bar |  |
| Board | board.color | Board, TaskDetail, Today, Review, Settings, Capture | Sweep-B/T/Y/R/S/C: no =SetBoardColor= seam; the GUI has no color affordance anywhere | no GUI affordance exists; the CLI is the only path (chunk-1 sweep-B evidence) |
| Board | board.archive | Board, TaskDetail, Today, Review, Settings, Capture | Sweep-B/T/Y/R/S/C: no =SetBoardArchived= seam; archived boards are excluded from the board bar (`board-bar.slint:21-34`), unreachable in the GUI | archived boards are excluded from the board bar (`board-bar.slint:21-34`), unreachable in the GUI (chunk-1 sweep-B evidence) |
| Board | board.reset | Board, TaskDetail, Today, Review, Settings, Capture | Sweep-B/T/Y/R/S/C: no =ResetBoard= seam | no GUI affordance exists; destructive reset is CLI-only (chunk-1 sweep-B evidence) |
| Board | board.reconcile | Board, TaskDetail, Today, Review, Settings, Capture | Sweep-B/T/Y/R/S/C: no =ReconcileByTitle= seam | no GUI affordance exists (chunk-1 sweep-B evidence) |
| Board | board.default_preset | TaskDetail, Today, Review, Settings, Capture, CLI | Sweep-T/Y/R/S/C: the empty-board preset CTA exists only on the board surface; Sweep-CLI: no preset verb |  |
| Board | stack.create | TaskDetail, Today, Review, Settings, Capture | Sweep-T/Y/R/S/C: no stack-create seam |  |
| Board | stack.rename | TaskDetail, Today, Review, Settings, Capture | Sweep-T/Y/R/S/C: no stack-rename seam |  |
| Board | stack.delete | TaskDetail, Today, Review, Settings, Capture | Sweep-T/Y/R/S/C: no stack-delete seam |  |
| Board | stack.move | TaskDetail, Today, Review, Settings, Capture | Sweep-T/Y/R/S/C: no stack-move seam |  |
| Board | stack.resize | TaskDetail, Today, Review, Settings, Capture, CLI | Sweep-T/Y/R/S/C: no column-width seam outside the board; Sweep-CLI: no stacks width verb |  |
| Board | stack.count_display | TaskDetail, Today, Review, Settings, Capture, CLI | Sweep-T/Y/R/S/C: no card-count badge outside the board columns; Sweep-CLI: counts are list output, not a badge capability |  |
| Board | task.create | TaskDetail, Today, Review, Settings, Capture | Sweep-T/Y/R/S/C: no inline create row; =TaskCreate= is Board/StackDetail-scoped (`actions.rs:1018-1027`) |  |
| Board | task.complete | Review, Settings, Capture | Sweep-R/S/C: no task row or done seam |  |
| Board | task.open | Review, Settings, CLI | Sweep-R/S: no task-open seam; Sweep-CLI: =find= (`cli.rs:89-101`) prints results, no open/jump verb |  |
| Board | task.move | Settings, Capture | Sweep-R/S/C: no task-move seam. (The Review entry retired 2026-11-14: the walk's file commit is a task-move seam — `adapter.rs:2203-2227`, chunk-2 D-11.5; the cell now cites it.) |  |
| Board | task.delete | Review, Settings, Capture | Sweep-R/S/C: no task-delete seam |  |
| Board | task.location_display | TaskDetail, Review, Settings, Capture | Sweep-T/R/S/C: no board›stack context line outside Today and the board |  |
| Board | task.badges_display | TaskDetail, Review, Settings, Capture, CLI | Sweep-T/R/S/C: no decoration chips outside board/today cards; Sweep-CLI: no badge output |  |
| Board | task.due_phrase_display | Review, Settings, Capture, CLI | Sweep-R/S/C: no due chip/echo; Sweep-CLI: no phrase output |  |
| Board | nav.item_walk | Review, Settings, Capture, CLI | =NavNextItem= contexts =VIEW_NAV= (`actions.rs:618-623`) bind only Today/Board/StackDetail/TaskDetail (Settings carries only the =CONTROL_NAV= text-field motions, `actions.rs:628-634`); Sweep-CLI: no nav verb |  |
| Today | habit.log | Board, TaskDetail, Review, Settings, Capture | Sweep-B/T/R/S/C: no habit-row seam outside the Today section (habit rows mount only there, `today-view.slint:181-205`) |  |
| Today | habit.reorder | Board, TaskDetail, Review, Settings, Capture | Sweep-B/T/R/S/C: no reorder seam |  |
| Today | habit.repair | Board, TaskDetail, Review, Settings, Capture | Sweep-B/T/R/S/C: no repair seam |  |
| Today | habit.convert_to_series | Board, TaskDetail, Review, Settings, Capture, CLI | Sweep-B/T/R/S/C: no convert seam; Sweep-CLI: the =habits= family (`cli.rs:1236-1362`) has no convert verb |  |
| Today | habit.state_display | Board, TaskDetail, Review, Settings, Capture | Sweep-B/T/R/S/C: no habit row outside Today |  |
| Today | task.due_today | Board, TaskDetail, Review, Settings, Capture, CLI | Sweep-B/T/R/S/C: no due-today affordance (=TaskDueToday= is Today-only, `actions.rs:1076-1078`); Sweep-CLI: no twin verb |  |
| Today | alarm.create_adhoc | Board, TaskDetail, Review, Settings, Capture | Sweep-B/T/R/S/C: no alarm-create seam (the alarm window is chunk 2's own slice, D-6) |  |
| Today | alarm.cancel_adhoc | Board, TaskDetail, Review, Settings, Capture | Sweep-B/T/R/S/C: no alarm-cancel seam |  |
| Today | alarm.display | Board, TaskDetail, Review, Settings, Capture | Sweep-B/T/R/S/C: no timers display outside Today and the status bar (chunk 2's alarm-window slice owns the rest, D-6) |  |
| Today | review.walk_preview | Board, TaskDetail, Review, Settings, Capture, CLI | Sweep-B/T/R/S/C: the surfacing card mounts only in the today view (`today-view.slint:206-238`); Sweep-CLI: none |  |
| Today | nav.item_walk | Review, Settings, Capture, CLI | =NavNextItem= contexts =VIEW_NAV= (`actions.rs:618-623`) bind only Today/Board/StackDetail/TaskDetail; Sweep-CLI: no nav verb |  |
| Today | break_rule.create | Board, TaskDetail, Today, Review, Settings, Capture | the break-rule engine verbs are UI-unreachable (no =on_break*= seam; the UI crate never constructs =CreateBreakRule= — adapter.rs and the UI crate swept); Today carries the row as the timer/alarm-adjacent daily surface |  |
| Today | break_rule.remove | Board, TaskDetail, Today, Review, Settings, Capture | the same UI-unreachable sweep |  |
| Today | break_rule.roster_display | Board, TaskDetail, Today, Review, Settings, Capture | no break-rule roster mounts anywhere in the UI (Sweep-B/T/R/S/C) |  |
| Capture | capture.quick | CLI | Sweep-CLI: no interactive capture verb — =capture= (`cli.rs:105-112`) commits its arguments directly and exits; that twin is recorded on =capture.commit= | the CLI verb commits its arguments directly and exits (`cli.rs:105-112`); the interactive overlay is GUI-only (Capture register) |
| Capture | capture.commit | TaskDetail, Today, Review, Settings | Sweep-T/Y/R/S: no capture-commit seam (the overlay commits into the inbox/backlog targets only, `view_state.rs:2024,3657`) |  |
| Capture | capture.dismiss | Board, TaskDetail, Today, Review, Settings, CLI | Sweep-B/T/Y/R/S: no overlay mount; Sweep-CLI: none |  |
| Capture | capture.hint_display | Board, TaskDetail, Today, Review, Settings, CLI | Sweep-B/T/Y/R/S: no overlay mount; Sweep-CLI: none |  |
| Capture | palette.jump | Board, TaskDetail, Today, Review, Settings, CLI | Sweep-B/T/Y/R/S: no palette mount; Sweep-CLI: =find= prints results, no jump verb |  |
| Capture | palette.results_display | Board, TaskDetail, Today, Review, Settings, CLI | Sweep-B/T/Y/R/S: no palette mount; Sweep-CLI: =find= output is the CLI slice's row (chunk 2) |  |
| Capture | app.menu | CLI | Sweep-CLI: no menu verb |  |
| Capture | app.focus_toggle | CLI | Sweep-CLI: no focus verb (`cli.rs` swept, no match) |  |
| Task detail | task.open | Review, Settings, CLI | Sweep-R/S: no task-open seam; Sweep-CLI: =find= (`cli.rs:89-101`) prints results, no open/jump verb |  |
| Task detail | task.back | Board, Today, Review, Settings, Capture, CLI | Sweep-B/Y/R/S/C: no back affordance outside the detail view; Sweep-CLI: none |  |
| Task detail | task.title_edit | Board, Today, Review, Settings, Capture | Sweep-B: board cards carry no title editor (click opens the detail, `adapter.rs:8349,8353`); Sweep-Y/R/S/C: no title-edit seam |  |
| Task detail | task.description_edit | Board, Today, Review, Settings, Capture | Sweep-B/Y/R/S/C: no description seam |  |
| Task detail | task.due_edit | Board, Today, Review, Settings, Capture | Sweep-B/Y/R/S/C: no due/date-picker seam |  |
| Task detail | task.scheduled_edit | Board, Today, Settings, Capture | Sweep-B/Y/R/S/C: no scheduled seam. (The Review entry retired 2026-11-14: the walk's carry date commit is a scheduled-edit seam — `adapter.rs:2252-2299`, chunk-2 D-11.5.) |  |
| Task detail | task.complete | Review, Settings, Capture | Sweep-R/S/C: no task row or done seam |  |
| Task detail | task.move | Settings, Capture | Sweep-R/S/C: no task-move seam. (The Review entry retired 2026-11-14: the walk's file commit is a task-move seam — `adapter.rs:2203-2227`, chunk-2 D-11.5; the cell now cites it.) |  |
| Task detail | task.delete | Review, Settings, Capture | Sweep-R/S/C: no task-delete seam |  |
| Task detail | task.reminder_rules_edit | Board, Today, Review, Settings, Capture | Sweep-B/Y/R/S/C: no reminder-rule seam (the global notification surface is chunk 2's, see the slice Origin note) |  |
| Task detail | task.reminders_mute | Board, Today, Review, Settings, Capture | Sweep-B/Y/R/S/C: no mute/unmute seam outside the detail banner |  |
| Task detail | task.reminder_snooze | Board, Today, Review, Settings, Capture, CLI | Sweep-B/Y/R/S/C: no snooze seam outside the detail banner; Sweep-CLI: no snooze verb |  |
| Task detail | task.track_start | Board, Review, Settings, Capture | Sweep-B/R/S/C: no track-start seam (board cards show the running badge, display only) |  |
| Task detail | task.track_stop | Board, Review, Settings, Capture | Sweep-B/R/S/C: no track-stop seam |  |
| Task detail | task.track_entry_log | Board, Today, Review, Settings, Capture | Sweep-B/Y/R/S/C: no track-entries overlay outside the detail |  |
| Task detail | task.track_entry_adjust | Board, Today, Review, Settings, Capture | Sweep-B/Y/R/S/C: no adjust seam |  |
| Task detail | task.track_entry_delete | Board, Today, Review, Settings, Capture | Sweep-B/Y/R/S/C: no entry-delete seam |  |
| Task detail | task.track_total_display | Review, Settings, Capture, CLI | Sweep-R/S/C: no tracked-total display; Sweep-CLI: =track status= (`cli.rs:1164`) is a status line, not this surface's total (CLI slice, chunk 2) |  |
| Task detail | task.recur_pause | Board, Review, Settings, Capture | Sweep-B/R/S/C: no pause/resume seam |  |
| Task detail | task.recur_convert_to_habit | Board, Review, Settings, Capture, CLI | Sweep-B/R/S/C: no convert seam; Sweep-CLI: =tasks repeat= has no convert verb (`cli.rs:1746-1755`) |  |
| Task detail | task.recur_preview | Review, Settings, Capture, CLI | Sweep-R/S/C: no editor mount; Sweep-CLI: no preview verb |  |
| Task detail | task.recur_display | Review, Settings, Capture, CLI | Sweep-R/S/C: no recur badge/echo; Sweep-CLI: none |  |
| Task detail | nav.item_walk | Review, Settings, Capture, CLI | =NavNextItem= contexts =VIEW_NAV= (`actions.rs:618-623`) bind only Today/Board/StackDetail/TaskDetail; Sweep-CLI: no nav verb |  |
| Task detail | label.create | Board, TaskDetail, Today, Review, Settings, Capture | the label engine verbs are UI-unreachable (no =on_label*= seam; the UI crate never constructs =CreateLabel=); the Task detail slice carries the label rows as the task entity's owning surface (labels bind to tasks; no GUI surface exposes them — the chunk-1 Board Origin deferred them to chunk 2) |  |
| Task detail | label.edit | Board, TaskDetail, Today, Review, Settings, Capture | the same UI-unreachable sweep |  |
| Task detail | label.delete | Board, TaskDetail, Today, Review, Settings, Capture | the same UI-unreachable sweep |  |
| Task detail | label.roster_display | Board, TaskDetail, Today, Review, Settings, Capture | labels render nowhere in the GUI (Sweep-B/T/Y/R/S/C: no label chip, badge, or roster — `models.rs:2413` records importance/urgency riding the card decorations slot) |  |
| Task detail | task.label_assign | Board, TaskDetail, Today, Review, Settings, Capture | =AssignLabel= is UI-unreachable (no assign seam anywhere) |  |
| Task detail | task.label_unassign | Board, TaskDetail, Today, Review, Settings, Capture | =UnassignLabel= is UI-unreachable |  |
| Task detail | track_entry.roster_display | Board, Today, Review, Settings, Capture | the track-entries table mounts only in the task detail view (the =task.track_entry_*= family's seams, `adapter.rs:9613-9921`) |  |
| Task detail | track.status_display | Board, TaskDetail, Today, Review, Settings, Capture | the status-bar tracking chip (the =task.track_stop= Today cell, `status-bar.slint:77-83`) shows the running state, not this status display |  |
| Settings | connection.save | Board, TaskDetail, Today, Review, Capture | Sweep-B/T/Y/R/C: no connection-form seam outside the settings view |  |
| Settings | connection.remove | Board, TaskDetail, Today, Review, Capture | Sweep-B/T/Y/R/C: no connection-remove seam |  |
| Settings | board.remote_list | Board, TaskDetail, Today, Review, Capture | Sweep-B/T/Y/R/C: no remote-boards refresh seam (the board bar reads the local snapshot; no =ListRemoteBoards= trigger) |  |
| Settings | board.attach | Board, TaskDetail, Today, Review, Capture | Sweep-B/T/Y/R/C: no remote-bind pick outside the settings view |  |
| Settings | board.create_local | Board, TaskDetail, Today, Review, Capture | Sweep-B/T/Y/R/C: no local-board-create seam outside the settings view (=CreateLocalBoard= has no =ActionId=) |  |
| Settings | board.detach | Board, TaskDetail, Today, Review, Capture | Sweep-B/T/Y/R/C: no detach seam outside the settings view |  |
| Settings | board.publish | Board, TaskDetail, Today, Review, Capture | Sweep-B/T/Y/R/C: no =PublishBoard= pick outside the settings view |  |
| Settings | board.roster_display | Board, TaskDetail, Today, Review, Capture, CLI | Sweep-B/T/Y/R/C: the per-account roster mounts only in the settings view (=settings-view.slint:1012=); Sweep-CLI: roster output is =boards list='s inventory (the =board.remote_list= CLI cell), not this display |  |
| Settings | files.sync_enabled_set | Board, TaskDetail, Today, Review, Capture, CLI | Sweep-B/T/Y/R/C: no files-sync pick outside the settings view; Sweep-CLI: no files-sync toggle verb (the config file is the CLI-side path, no set verb exists) |  |
| Settings | settings.knob_set | Board, TaskDetail, Today, Review, Capture, CLI | Sweep-B/T/Y/R/C: no knob seam outside the settings view; Sweep-CLI: no settings-mutation verb (the =config= family is read-only: =config print= `cli.rs:1518`/ =config path= `cli.rs:1522`) |  |
| Settings | app.settings | CLI | Sweep-CLI: no settings-open verb |  |
| Settings | connection.roster_display | Board, TaskDetail, Today, Review, Capture, CLI | Sweep-B/T/Y/R/C: no connections roster outside the settings view; Sweep-CLI: no connection roster output |  |
| Settings | settings.facts_display | Board, TaskDetail, Today, Review, Capture, CLI | Sweep-B/T/Y/R/C: no facts panel outside the settings view; Sweep-CLI: =config print= prints the resolved config, not the app-stated apply/env facts |  |
| Settings | settings.env_override_display | Board, TaskDetail, Today, Review, Capture, CLI | Sweep-B/T/Y/R/C: no env-locked badge outside the settings view; Sweep-CLI: no override output |  |
| Review | review.open | CLI | Sweep-CLI: no review/walk/planner verb (`cli.rs` swept, `commands/` — the =journal= family records, it does not walk) |  |
| Review | review.walk_cursor | Board, TaskDetail, Today, Settings, Capture, CLI | the walk verbs bind only =ContextId::ReviewWalk= (`actions.rs:1551-1569`); Sweep-B/T/Y/S/C: no walk seam outside the overlay; Sweep-CLI: no walk verb |  |
| Review | review.walk_back | Board, TaskDetail, Today, Settings, Capture, CLI | the same =ReviewWalk=-scoped binding (`actions.rs:1571-1579`, Escape preset `presets.rs:312`); Sweep-CLI: no walk verb |  |
| Review | review.walk_advance | Board, TaskDetail, Today, Settings, Capture, CLI | the same =ReviewWalk=-scoped binding (`actions.rs:1581-1589`); Sweep-CLI: no walk verb |  |
| Review | review.inbox_file | Board, TaskDetail, Today, Settings, Capture, CLI | the same =ReviewWalk=-scoped binding (`actions.rs:1591-1599`); Sweep-CLI: no file-to verb |  |
| Review | review.inbox_keep | Board, TaskDetail, Today, Settings, Capture, CLI | the same =ReviewWalk=-scoped binding (`actions.rs:1601-1609`); Sweep-CLI: no keep verb |  |
| Review | review.file_target | Board, TaskDetail, Today, Settings, Capture, CLI | the same =ReviewWalk=-scoped binding (`actions.rs:1611-1629`); Sweep-CLI: no target-nudge verb |  |
| Review | review.file_commit | Board, TaskDetail, Today, Settings, Capture, CLI | the same =ReviewWalk=-scoped binding (`actions.rs:1631-1639`); the CLI's move twin is recorded on the =task.move= row (Board slice) |  |
| Review | review.carry_tomorrow | Board, TaskDetail, Today, Settings, Capture, CLI | the same =ReviewWalk=-scoped binding (`actions.rs:1641-1649`); the CLI's scheduled-edit twin is recorded on the =task.scheduled_edit= row (Task detail slice) |  |
| Review | review.carry_pick | Board, TaskDetail, Today, Settings, Capture, CLI | the same =ReviewWalk=-scoped binding (`actions.rs:1651-1659 + 1671-1679`); Sweep-CLI: no pick-a-date verb |  |
| Review | review.carry_leave | Board, TaskDetail, Today, Settings, Capture, CLI | the same =ReviewWalk=-scoped binding (`actions.rs:1661-1669`); Sweep-CLI: no leave verb |  |
| Review | reflection.record | Board, TaskDetail, Today, Settings, Capture | Sweep-B/T/Y/S/C: no reflection-commit seam outside the walk (the prompts editor mounts only in the walk overlay) |  |
| Review | review.finish | Board, TaskDetail, Today, Settings, Capture, CLI | the same =ReviewWalk=-scoped binding (`actions.rs:1691-1699`); Sweep-CLI: no finish verb |  |
| Review | plan.open | CLI | Sweep-CLI: no plan verb (=ViewPlanNextDay= is GLOBAL menu-reachable from every surface, `actions.rs:1472-1480`) |  |
| Review | plan.begin_place | Board, TaskDetail, Today, Settings, Capture, CLI | the planner verbs bind only =ContextId::PlanNextDay= (`actions.rs:1485-1493`); Sweep-CLI: no place verb |  |
| Review | plan.commit_place | Board, TaskDetail, Today, Settings, Capture, CLI | the same =PlanNextDay=-scoped binding (`actions.rs:1495-1503`); Sweep-CLI: no commit-placement verb |  |
| Review | plan.box_earlier | Board, TaskDetail, Today, Settings, Capture, CLI | the same =PlanNextDay=-scoped binding (`actions.rs:1505-1513`); Sweep-CLI: no reorder verb |  |
| Review | plan.box_later | Board, TaskDetail, Today, Settings, Capture, CLI | the same =PlanNextDay=-scoped binding (`actions.rs:1515-1523`); Sweep-CLI: no reorder verb |  |
| Review | plan.box_remove | Board, TaskDetail, Today, Settings, Capture, CLI | the same =PlanNextDay=-scoped binding (`actions.rs:1525-1533`); Sweep-CLI: no box-remove verb |  |
| Review | review.ledger_display | Board, TaskDetail, Today, Settings, Capture, CLI | Sweep-B/T/Y/S/C: no ledger mount outside the review view |  |
| Review | review.stats_display | Board, TaskDetail, Today, Settings, Capture, CLI | Sweep-B/T/Y/S/C: no stat-section mount; Sweep-CLI: no aggregate output |  |
| Review | reflection.ledger_display | Board, TaskDetail, Today, Settings, Capture | Sweep-B/T/Y/S/C: no journal-ledger mount outside the review view |  |
| Review | reflection.edit | Board, TaskDetail, Today, Review, Settings, Capture | =UpdateReflection= is UI-unreachable (the review slice's ledger is read-only, `review-view.slint:347-351`; the walk only records) |  |
| Review | reflection.delete | Board, TaskDetail, Today, Review, Settings, Capture | =DeleteReflection= is UI-unreachable (no delete seam anywhere; the journal ledger carries no activation) |  |
| Alarm window | alarm.dismiss | CLI | Sweep-CLI: no reminder-dismiss verb (the =tasks remind= family mutates rules, it does not dismiss occurrences) |  |
| Alarm window | alarm.snooze | CLI | Sweep-CLI: no occurrence-snooze verb (the Task-detail banner's =ReminderSnooze= is GUI-only — its row's CLI cell is =U= there too) |  |
| Alarm window | alarm.window_display | CLI | Sweep-CLI: =timers list= `cli.rs:954` is the Today timers section's inventory (the =alarm.display= row's CLI cell), not the window's awaiting-occurrence rows |  |
| Alarm window | notification.dismiss | CLI | Sweep-CLI: no dismissal verb (same =DismissReminder= absence as =alarm.dismiss=) |  |
| Alarm window | notification.snooze | CLI | Sweep-CLI: no occurrence-snooze verb |  |
| Alarm window | notification.open | CLI | Sweep-CLI: =find= (`cli.rs:89-101`) prints results, no open/jump verb (the =task.open= register's CLI sweep, Task detail slice) |  |
Inventory headline: **121 unruled register entries over 578 `U`
cells**; the largest clusters are the task-detail edit family's
per-surface absences (Board/Today/Review/Settings/Capture) and the
global-overlay verbs' CLI absences. The four-column detail is
machine-readable above; the census's per-slice registers remain the
authoritative source.

## 4. The planned-pending record

The files roster LANDED before this chunk's baseline (`84c04a0b`) —
SHAs and commit table in §1; the *planned-pending* markers retired;
nothing carried; the ruling table moved token-identically into the
Settings slice. (Chunk-2 plan §2 is the operative record; this
section restates it per the dispatch's report outline.)

## 5. Consolidation results (the D-11 checks, commands + outcomes)

All checks run against the census at the consolidation commit:

1. **No verb under two names** — extracted every matrix row name
   across all slices (`python` scan of the `| Capability |` tables):
   137 rows, 131 distinct names. The 6 same-name repeats
   (`nav.item_walk` ×3, `task.open/complete/move/delete` ×2) are the
   shared-capability convention (same name = same capability, one
   row per owning slice, chunk-1 D-2/D-8) — not two names for one
   verb. `CHECK 5` additionally verified the repeated rows carry
   non-contradictory CLI cells. Outcome: **pass**.
2. **Ruling-ID uniqueness** — `awk` extraction of the ruling tables
   + `sort | uniq -d`: census-wide duplicates are `R-1`, `R-2`,
   `R-3` — the seed's per-slice IDs, deliberately stable and
   disambiguated by slice heading per chunk-1 D-3 (chunk-1 D-3 wins
   over D-11.2's census-wide reading; chunk 2 coins no new ruling
   IDs). Within each slice block every ID is unique. Outcome:
   **pass** (recorded reconciliation).
3. **Marker/register bijection** — `python` scan comparing every
   `U` cell against its slice's register surface lists: **pass**
   after two fixes the check itself caught (register rows for
   `label.*`/`reflection.*` initially landed in the neighbouring
   slice's register; `break_rule.*`/`review.file_commit`/`CLI`
   surface lists were incomplete). The check is the census's own
   quality gate and did its job.
4. **Zero blank cells** — same scan across all seven slices:
   **pass** (zero blanks census-wide, including the seed-merged
   Settings rows whose seed blanks were recorded as `U`).
5. **Cross-slice cell integrity** — the D-11.5 consolidation
   (task.move/task.scheduled_edit Review cells) applied and recorded
   in the diff-against-seed appendix; `review.*`/`settings.*` rows
   appear only in their own slices; CLI cells verified
   non-contradictory. Outcome: **pass**.
6. **Seed rulings verbatim** — `python` row-wise comparison of the
   seed's ruling-table rows (`dc290df2`) against the census: all 17
   seed ruling rows **token-identical** in their new homes (plus
   PC-10/PC-11, added post-seed at `e916609f`, preserved likewise).
7. **Anchor spot-checks** — 12 cells across the four new slices
   resolved at `84c04a0b` via `git show 84c04a0b:<file>` (11/12
   exact on first pass; the notification-snooze seam citation was
   corrected from `10186` to `10188` and its neighbours aligned —
   the fix the spot-check caught). **Stage-3 sweep**: the stage-3
   review's independent ≥10-cell spot-check surfaced a systematic
   drift in the walk/planner `ActionId` citations (each entry spans
   id..id+8 in `actions.rs`; the cited ranges were shifted ~9–11
   lines), three per-callback `lib.rs` ranges in the alarm-window
   slice, and the round-6 ruling's evidence anchor
   (`settings-view.slint:1013-1014`, the overturn comment — the
   cited `1065-1069` is layout code). All corrected in the census
   and mirrored in this report's inventory; ~25 further anchors
   (adapter seams, `requests.rs` variants, `cli.rs` verbs, mounts,
   test names) re-verified exact at `84c04a0b`.

## 6. The diff-against-seed appendix

The appendix lives in the census itself (`docs/ux-surface-census.org`,
`* Appendix: diff against the phase-2 seed`) — that file is the
artefact. Its counts: seed 3 feature-scoped slices / 12 rows →
census 8 surface slices + the CLI-only register / 137 rows; 14
distinct ruling IDs across 19 slice-scoped entries; all seed rulings
preserved; the files-sync planned-pending resolved (landed).

## 7. Honest limits — what a census cannot see

- **Label quality**: cell evidence cites affordances, not whether a
  label communicates (PC-8's consequence-style-label doctrine lives
  in the walkthrough, not the matrix). Phase 4's walkthrough
  territory.
- **Layout & composition**: the census knows the board bar mounts
  (`board-bar.slint`), not whether it is findable or sane at kiosk
  scale.
- **Journey ergonomics**: a capability × surface cell proves reachability,
  never that the journey completes comfortably — the four
  cognitive-walkthrough questions (`docs/ux_planning.org` §2.4) are
  phase 4's protocol.
- **Display-affordance granularity**: pass 5's rule of thumb (a row
  only if a user can perceive it as a capability, chunk-1 §4) is a
  judgment call; the line between a badge and a rendered text
  fragment is drawn per slice and could be drawn differently.
- **Anchor-rot between slice baselines**: chunk-1 cells anchor at
  `dc290df2`, chunk-2 cells at `84c04a0b`; code that moved between
  those trees is cited from the older SHA in the older slices. Drift
  is recorded, not normalized (D-7); stage-3 spot-checks resolve
  anchors at the SHA stated in each block.
- **Planned-pending as a class**: the marker grammar survives this
  census with zero live instances; its retirement here says nothing
  about future UX plans' pending cells.

## 8. Backlog issues opened during phase 3

- **#776** — dead CLI verb `boards use` (opened during the chunk-2
  stage-1 survey; the census's CLI-only register records it with the
  unreachable-dispatch evidence, not as a capability row).
- No further issues opened during stage 2: the chunk-1 sweep
  inaccuracy (`task.move`/`task.scheduled_edit` Review cells) was
  fixed in-scope via consolidation (D-11.5); the naming/drift
  observations (`find` vs the palette, `login`/`connect`
  duplication, the seed's `ui_requests.rs` path vs
  `requests.rs` — the former is `taskboard-app/src/ui_requests.rs`,
  which exists) are evidence notes in the census, not work items.

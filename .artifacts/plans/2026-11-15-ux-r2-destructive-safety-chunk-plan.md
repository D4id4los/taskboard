# [Plan] UX Remediation — Chunk 2: destructive-action safety

- **Date**: 2026-11-15
- **Kind**: stage-1 chunk plan for chunk 2 of the UX remediation phase
  (framework plan §6 phase 5; the dress rehearsal's second code-bearing
  chunk). Planning only — no application code in this stage.
- **Origin**: the remediation dispatch plan
  `.artifacts/plans/2026-11-15-ux-remediation-dispatch-plan.md` §1/§3
  (the shared execution model, the long-command discipline, and the
  dress-rehearsal mandate are binding); the accepted triage report
  `.artifacts/reports/2026-11-14-ux-retrofit-triage-report.md` §3 rows
  T-1, T-3, T-4; home issues #784 (T-1), #786 (T-3), #787 (T-4); the
  owning UX plans `2026-11-14-board-stack-management-ux-plan.md`
  (US-5), `2026-11-14-habit-family-ux-plan.md` (US-1),
  `2026-11-14-settings-connections-ux-plan.md` (US-2, US-5); the census
  `docs/ux-surface-census.org` (Board, Today, Settings slices);
  `docs/ux_planning.org` §2.
- **Status**: READY — preconditions verified by the dispatcher (chunk 1
  merged, PR #835; `main` carries the dispatch plan + the long-command
  discipline at `c79a4bbe`); worktree
  `../taskboard-WT-fix-ux-r2-destructive-safety` on branch
  `fix/ux-r2-destructive-safety` off post-chunk-1 `main`.
- **Binding inputs of stage 2**: every decision D-1..D-8 below and the
  commit sequencing §6. Deviations need a recorded reason in the
  stage-2 report.

## 1. Posture (inherited, restated for the chunk)

The user's acceptance of the triage report (2026-11-15) settles T-4's
recorded doubt to the rubric's floor: **confirmation stages are
required, not discretionary** — re-creatability of the destroyed
object is not an exception. T-1 and T-3's B-b letter matches directly
(NotUndoable ∧ unconfirmed). The chunk's definition of done (dispatch
§3 acceptance): every destructive step named below commits only after
its armed state, with escape paths; amended plans gate-green; census
updated; the walkthrough verifies the armed strips; full gates green.

House precedents this chunk reuses (consistency is the heuristic):
- the **recur editor's stop-strip** (`detach_armed`, `level_state.rs:125`,
  `recur-editor.slint:274-290`) — an arm click flips a typed flag, the
  consequence-naming strip mounts, Confirm routes the verb, Cancel
  disarms;
- the **habits convert strip** (`convert-armed-row`,
  `habits.slint:247-276`, `adapter.rs:7161-7197`) — the same grammar
  with a row index as the armed identity, one row armed at a time;
- the **armed track-delete row** (`track-armed-index`, D-TS5) — the
  armed identity is an index the adapter mirrors over the id table;
- the **stack-move flow's two-step invoke** (`view_state.rs:2957-3010`,
  board-stack US-6) — repeating the stroke cycles, the final stroke
  confirms; Escape cancels via the flow settle law
  (`view_state.rs:2830-2832`, keyscope's bare-Escape-ahead-of-resolver
  `keyscope.rs:237`).

## 2. Decisions

### D-1 — The confirmation pattern is the house armed-confirm strip, with two-step invoke as the keyboard twin

Of the candidate shapes (armed-confirm strip / two-step invoke / typed
confirm dialog), stage 1 decides: **an armed-confirm strip** that names
the consequence before anything dispatches, **plus two-step invoke on
the keyboard twin** (the same stroke that arms, repeated, confirms —
the stack-move grammar; the strip's Confirm/Cancel buttons are the
pointer twin of the second stroke / Escape). A modal typed-confirm
dialog is rejected: the house has no modal-dialog precedent on these
surfaces, the strips keep the destructive context visible in place
(Nielsen recognition-over-recall), and every existing armed flow
(strip or field) resolves through the flow settle law — a new dialog
layer would need its own focus/settle machinery for no UX gain.

Shared grammar (all confirmations in this chunk):

- **Arm** = first invocation; nothing dispatches; the strip mounts at
  the subject naming the consequence.
- **Confirm** = the strip's Confirm button, or the invoking
  `ActionId`'s stroke repeated; routes the exact existing verb.
- **Escape** = the strip's Cancel button, Escape, a foreign settle, or
  arming a different subject — all disarm; only a confirm dispatches.
- One armed subject at a time per surface family.
- The strip renders under the same enablement as the trigger (chunk 1's
  `editing-enabled`/disabled-tone grammar) — on kiosk the stage is
  **visible-gated, not silently absent**: the delete verbs were already
  reachable there (menu + keymap), so the confirmation rides the same
  reachability, no capability change either way (see D-7's gate).

### D-2 — T-1 (#784): board.delete and stack.delete arm a delete strip in the view state; the command pair and classes are untouched

- **Armed identity**: a new view-state flow `Flow::ConfirmDelete`
  carrying the resolved subject (`stack: StackId` /
  `board: BoardId`) — the same flow slot `MoveStack`/`RenameStack` use,
  so the whole existing machinery applies for free: foreign settle
  cancels (`view_state.rs:2830-2832`), reconcile ends the flow when the
  subject vanishes (the `begin_rename_*` vanish pattern,
  `view_state.rs:3082`), Escape resolves it first
  (`keyscope.rs:237`'s bare-Escape law), and the armed form law
  (`app-window.slint:646`) covers rendering.
- **`delete_stack` / `delete_board`** (`view_state.rs:2912-2937`)
  become two-phase: with no `ConfirmDelete` armed (for that subject)
  they arm the flow and return `RefreshNow` only; with the subject's
  flow armed they dispatch the existing
  `StateCommand::DeleteStack { id }` / `DeleteBoard { board }` +
  `RefreshNow` and clear the flow. **Zero command delta, zero undo-class
  change** — `DeleteStack` stays `NotUndoable(RestoreClass)`,
  `DeleteBoard` stays `NotUndoable(BoardCeremony)` (`undo.rs:126-138`);
  the confirmation *is* the escape path the finding demands. Arming a
  stack while a board delete is armed (or vice versa) re-arms to the new
  subject (one armed subject per the shared grammar).
- **Presentation**: the armed strip mounts inline at the subject — the
  stack header for `stack.delete`, the board-bar region for
  `board.delete` (board delete cascades; the strip's consequence text
  names the cascade, judged by the walkthrough not asserted by tests).
  A typed fact (`delete-armed: bool` + subject identity on the existing
  board/stack facts models — the `detach_armed` mirror pattern,
  `level_state.rs:2659/2688`) drives it; the strip carries Confirm
  (`Tone.danger`-adjacent per the theme's destructive vocabulary — no
  new Tone) and Cancel `IconButton`s, callbacks
  `delete-confirmed()` / `delete-cancelled()` wired through
  `app-window.slint` to adapter seams that route `StackDelete`-confirm /
  disarm. The context-menu "Delete stack/board" row and the keymap
  strokes (Ctrl+Delete / Ctrl+Shift+Delete, `presets.rs:152-164`) keep
  their `ActionId`s — they become the arm/confirm twin, click ≡ key at
  every step.
- **Registry descriptions** (`actions.rs:935-944` etc.) are updated to
  name the two-step ("first press arms, repeat or Confirm commits") —
  text changes are fine; tests never assert them.
- **Task delete is untouched**: it is undoable (`undo.rs:104-109`) —
  the dispatch plan's letter: consistency lands by the confirmations,
  not by weakening task delete. No arm on `DeleteTask` (its undo IS the
  recovery); recorded so stage 3's walkthrough does not flag the
  asymmetry as a finding.

### D-3 — T-3 (#786): the recovery is a targeted −1 affordance via the audited `AdjustHabitLog`; no confirmation on the +1, no journal-class change

- **Why not confirmation-on-commit**: the +1 is the habit family's
  highest-frequency, lowest-damage gesture (D-P14: surplus logs are
  first-class — an armed strip per click would tax every deliberate
  log to guard one mis-click class). The triage's own T-3 text frames
  the defect as the *mis-click* class, and the dispatch plan offers
  confirmation *or* an undo arm; error-prevention effort must be
  proportional to risk (the framework's rubric logic), so the recovery
  path is the answer, not the gate.
- **Why not a journal arm**: reclassifying `LogHabit` out of
  `NotUndoable(HabitSurface)` would break the recorded lossy-inversion
  rationale (`undo.rs:231-237`: the inverse of a pre-log absent window
  forges an explicit count-0 row — a different stored fact) and put a
  *generic* journal class change behind a UI mis-click. The domain
  decision on the table is instead: **the mis-click class recovers
  through the already-audited `AdjustHabitLog` valve** — for a window
  whose row exists (exactly the post-log state), a corrected count is
  an exact inverse with no forged facts (the same reasoning that makes
  `AdjustHabitLog` undo-eligible on pre-row-present windows,
  `undo.rs:277-295`). `LogHabit`'s class stays as-is; **zero undo-class
  change, zero `StateCommand` delta**.
- **The affordance**: a row "−1" `IconButton` beside the + (same
  `editing-enabled` gating, disabled on kiosk per chunk 1's grammar),
  callback `undo-log-clicked(int)`, adapter seam resolving the row →
  new `ActionId::HabitUndoLog` (section-scoped, `menu_group: None` —
  the `HabitLog` R-11 ruling's exact twin, `actions.rs:1296-1306`),
  router arm dispatching
  `StateCommand::AdjustHabitLog { habit, window: <row's window>,
  count: <count − 1> }` from the same row source the + uses
  (`router.rs:685-688` pattern). Guards: count 0 → the arm dispatches
  nothing (NoTarget semantics, no negative possible — `count: u32`).
  No arm/confirm on the −1: it is symmetric with the +1's risk profile
  (bounded damage, itself reversible by +), and an armed strip here
  would recreate the frequency problem one level down.
- **The surplus-logs design survives verbatim**: the −1 never disables
  at/after quota, never enforces a quota floor (D-P14's law applies to
  the decrement symmetrically — the row restyles, the button never
  greys by count); the *engine* is untouched; the valve remains "the
  explicit correction path the automatic path never uses".
- **Scope guard (recorded)**: the affordance addresses the row's own
  current window — the same resolution the + uses. A mis-click *make-up
  valve* (+1 to yesterday) recovers by decrementing that window with
  the same verb through the repair row's own window — recorded as a
  non-goal here (the repair row's recognition debt is HB-W-N2's, T-41
  #824), not silently dropped; the mechanism (`AdjustHabitLog` from a
  resolved window) generalizes if a later round wants a per-window
  editor.

### D-4 — T-4 (#787): all three removal verbs confirm — connection.remove, board.detach, Stop-Sync — via a settings-side armed-item strip

The acceptance settles the doubt to the floor, so **all three** get
confirmation stages (no re-creatability carve-out). Publish/move/attach
are non-destructive and stay one-step (their US-3/US-4/US-5/S2 escape
columns already hold: typed outcomes + re-bindability).

- **Armed identity**: a settings-VM armed-item flag —
  `settings_armed_item: Option<ItemAddr>` adapter-owned (the
  `convert-armed-row` mirror pattern generalized: identity by
  `ItemAddr`, not index, because the roster re-derives between renders
  — a stale armed index would point at a different row after a
  snapshot refresh; matching on `ItemAddr` makes a vanished subject's
  strip dead). One armed item at a time across the whole settings view
  (the shared grammar's rule, applied surface-wide).
- **Arm sites**: the connection card's Remove (`settings-view.slint:965`,
  seam `adapter.rs:10301-10309` → `settings_remove_item`
  `adapter.rs:5795`), the connection card's Disconnect
  (`settings_disconnect_item` `adapter.rs:6016`), and the Files roster
  row's Stop-Sync branch of `settings_files_board_action`
  (`adapter.rs:5892-5931`, `PendingKind::StopFilesSync` only — the
  Move and PublishFiles branches of the same seam stay one-step).
- **The armed strip**: mounts inline under the armed row's action
  button (a caption naming the consequence + Confirm/Cancel
  `SettingsButton`s — the settings surface's own widget vocabulary, no
  `IconButton` import from another composition's grammar). Callbacks
  `remove-confirmed(int)` / `remove-cancelled()` /
  `files-action-confirmed(int)` / `files-action-cancelled()` (the
  index form, resolved against `settings.items` at the seam like every
  settings seam; a stale index disarms — the silent-absorb law).
- **Request-lane interaction (the trap the dispatch plan names)**:
  arming touches **no lane state** — no `PendingKind`, no
  `send_connections_op`, the busy lane's posture is byte-for-byte
  untouched (T-38 #821 stays chunk 6's). Confirm alone dispatches: the
  existing `UiRequest::RemoveConnection` / `DetachConnector` /
  `StopBoardFilesSync` construction moves into the confirm seam
  verbatim; arm→confirm is one user action per request, **no
  double-dispatch** — the confirm seam clears the armed flag *before*
  the send, and once the reply lane pushes `pending`, the buttons
  disable (`enabled: !SettingsModel.busy`, unchanged), so a re-click
  cannot re-enter either path. Disarm also on: Cancel, arming another
  item, any outcome render for that lane, and the roster snapshot
  refresh replacing `settings.items` (the stale-`ItemAddr` dead-strip
  rule above).
- **Kiosk**: settings is desktop-only (US-2 platforms: desktop) — no
  kiosk leg; recorded so the test catalogue does not invent one.

### D-5 — Amended journey steps (escape columns become real)

- **board-stack-management** US-5: S2's system-response/escape cells
  rewritten — click/stroke one **arms** (strip mounts, nothing
  dispatches); S2 gains the confirm sub-step (repeat stroke or Confirm
  → the exact `DeleteStack`/`DeleteBoard` + `RefreshNow`); the escape
  column now names Escape / Cancel / foreign settle / subject-vanish
  reconcile, and the "escape-less" finding sentence retires to a dated
  pointer. S3's keyboard twin text updates to the two-step grammar.
  Wiring-inventory: two rows (stack strip, board strip) with
  callbacks `delete-confirmed`/`delete-cancelled`, the
  `StackDelete`/`BoardDelete` action ids (reason: unchanged ids, new
  two-phase route), journey steps US-5/S1–S3, tests per D-7.
  Capability-matrix cells `board.delete`/`stack.delete` Board column
  keep `X (US-5)` with the amended-step citation.
- **habit-family** US-1: S1's escape cell rewritten — the mis-click
  class recovers through the new US-6 sibling step (**US-6 — Undo a
  habit log**, new story: S1 click the row's −1 →
  `ActionId::HabitUndoLog` → `AdjustHabitLog { count: n−1 }`; escape:
  count-0 floor dispatches nothing, +1 re-raises, kiosk renders the −1
  disabled). US-1/S1's dispatch/system-response cells unchanged (the +
  stays one-click); wiring row for `undo-log-clicked`; the
  `habit.log` cell gains a sibling-citation. The conversion-composite
  no-undo note (HB-3/RC-5, `undo.rs:250-261`) is **not** resolved here
  — its recorded posture is the chain-readability rationale, and no
  journey asked for it; T-3's letter covers the log mutation. Recorded
  as out-of-scope (already homed by #786's body; no new issue).
- **settings-connections** US-2 and US-5: S1 (remove), S2 (Stop Sync)
  and US-5/S1 (disconnect) each split into arm + confirm sub-steps
  with real escape columns (Cancel, re-arm elsewhere, typed outcome on
  failure); the "deliberate end (no undo)" sentences retire to dated
  pointers; wiring rows for the four new callbacks; command delta zero
  (same three `UiRequest`s, dispatched from the confirm seams).
- **Command-contract delta across all amendments**: **one new
  `ActionId` (`HabitUndoLog`); zero new `StateCommand`; zero new
  `UiRequest`** — confirmations are UI-state (D-2/D-4), and the T-3
  recovery reuses the audited valve (D-3). This section's content in
  each amended plan.

### D-6 — Census updates

Ride the implementation commits: `board.delete`/`stack.delete` Board
cells gain the armed-strip evidence + amended-step citation;
`habit.log` Today cell gains the −1 sibling citation (new `habit.unlog`
row? — no: the census matrix's verb granularity is per capability;
the recovery is recorded *inside* the `habit.log` cell's evidence, not
as a new verb row — it has no command of its own);
`connection.remove`/`board.detach`/`board.unbind_files` Settings cells
gain the confirmation evidence (the T-4 doubt resolution recorded in
the census's rulings note: dated 2026-11-15, authority = the user's
acceptance + this plan). No ruling overturned; the R-1/R-3 scoping
rulings stand (the strips mount only where the verbs already mount).

### D-7 — Test catalogue (TDD-first; typed outcomes, never text)

| # | Layer | Test | Asserts |
|---|---|---|---|
| T1 | view_state unit | `delete_stack_arms_then_confirms_dispatching_exactly_once` | first call → flow armed, zero commands; second call → exactly `StateCommand::DeleteStack { id }` + `RefreshNow`, flow cleared |
| T2 | view_state unit | `delete_board_arms_per_subject_and_rearm_swaps` | board twin of T1; arming stack over an armed board re-arms (one subject) |
| T3 | view_state unit | `armed_delete_foreign_settle_and_vanish_disarm` | foreign settle cancels; subject vanish ends the flow, nothing dispatched (the rename-vanish pattern) |
| T4 | seam (journeys.rs, driver) | `us5_s2_menu_delete_row_arms_and_confirm_click_commits` | driver: context-menu delete row → armed fact true, zero commands; `delete-confirmed` → the exact command |
| T5 | seam | `us5_s3_repeat_stroke_confirms_and_escape_cancels` | keymap route: second Ctrl+Delete commits; Escape first → nothing dispatched ever |
| T6 | kiosk gate | `the_kiosk_delete_stage_is_visible_gated_not_absent` | kiosk profile: arm still yields the armed fact (the stage rides the verb's existing reachability), desktop parity |
| T7 | router unit | `habit_undo_log_routes_the_decremented_adjust` | `HabitUndoLog` with row context → `AdjustHabitLog { count: n−1 }`; count 0 → `NoTarget` |
| T8 | kiosk gate | `the_kiosk_habit_undo_affordance_renders_disabled` | the −1 follows the `editing-enabled` grammar (disabled, not absent) — chunk 1's `the_kiosk_habit_actions_render_disabled_not_absent` successor family |
| T9 | seam (settings) | `us2_s1_remove_arms_then_confirm_sends_exact_request_once` | arm → no `UiRequest`, lane idle; confirm → exactly one `RemoveConnection`, armed flag cleared, lane busy |
| T10 | seam | `us2_s2_stop_sync_and_us5_s1_disconnect_confirm_dispatch_once` | the same for `StopBoardFilesSync` / `DetachConnector`; re-click after confirm cannot double-dispatch (armed cleared pre-send) |
| T11 | seam | `cancel_rearm_and_snapshot_refresh_disarm_without_dispatch` | Cancel disarms; arming item B disarms A; a snapshot refresh killing the armed `ItemAddr` leaves a dead strip that dispatches nothing |
| T12 | presentation | `settings_armed_item_renders_strip_flag_only` | the VM's typed armed fact drives the strip (bool/Option), no label oracle |
| T13 | gate | `scripts/ux-coverage.sh <each amended plan> --gate` | every new/changed wiring row FOUND |

Presentation flags are the tests' oracle — no text assertions
(framework §2.1 law). The strips' consequence wording is judged by the
stage-3 walkthrough, never asserted.

### D-8 — Long-command discipline (restated as binding for stage 2)

The coverage trio never runs in the subagent; the gate is the
pre-commit hooks plus
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(flock-safe arbitrary-command mode). Any single command expected over
~5 minutes runs backgrounded with interleaved polls; all landed work
is committed before any wait.

## 3. Non-goals (recorded)

- No journal redesign and no `UiRequest`-lane undo (T-17 #800 is chunk
  5's); no CLI changes (all three removal verbs' CLI twins already
  carry their own guards — `boards delete --confirm` etc.); no
  task-delete confirmation (undoable; D-2); no conversion-composite
  undo (recorded posture, #786's body); no busy-lane outcome work
  (T-38 #821, chunk 6); no per-window habit log editor (D-3's scope
  guard); no `board.detach` on the Board surface (settings-only — the
  census's R-1 cells stand).
- Estimated diff size: **M** — view_state flow + facts + two slint
  strips (T-1, ~300 lines incl. tests), one affordance + router arm +
  registry row (T-3, ~120), settings armed-item VM + three arm-site
  edits + one strip composition + seams (T-4, ~200), plan/census
  amendments ~150. No domain-crate change at all.

## 4. Risks

- **R-1**: the two-phase `delete_stack`/`delete_board` changes a verb
  every existing test and menu row assumes commits — the audit sweep
  (`journeys.rs`, view_state.rs unit tests, `menu.rs` destructive-row
  ordering tests) may pin the one-shot dispatch; those tests update
  per the TDD catalogue (they assert *typed* outcomes, so the update
  is mechanical: arm, then confirm). A test that pinned one-shot
  semantics as "desired" rather than incidental is re-judged, not
  weakened (AGENTS §5).
- **R-2**: the `ItemAddr`-keyed armed flag must survive the roster
  re-derivation exactly; if the snapshot refresh mints new `ItemAddr`s
  for unchanged rows (not expected — the addresses are content-keyed),
  the dead-strip rule degrades to "disarm on refresh", which is
  recorded as the fallback, still correct.
- **R-3**: golden visuals may pin the stack header / board bar /
  settings rows; the strips change those models — re-baselining rides
  the implementation commits (the chunk-1 R-2 mechanism).
- **R-4**: `Flow::ConfirmDelete` entering the flow enum touches the
  walk/keyscope resolution order; the bare-Escape law
  (`keyscope.rs:237`) and the armed-form mask (`keyscope.rs:720`
  family) must resolve the delete arm ahead of the resolver — T3/T5
  pin this; stage 2 stops and records if the resolution order cannot
  express it without reordering unrelated arms.

## 5. Deviations from the origins

- Dispatch §3's T-3 letter offers confirmation *or* an undo arm; this
  plan chooses a third precise shape (targeted decrement via the
  audited valve) rather than either literal: not a confirmation (the
  frequency argument, D-3), and not a *journal* arm (the lossy-inversion
  class stays intact). Recorded as the D-3 decision with its domain
  reasoning; the acceptance's "where stage 1 decided, an undo arm"
  clause covers it.
- T-4's exact armed states (per-row strips keyed by `ItemAddr`, one
  armed surface-wide) are this plan's concretization of "their exact
  armed states" — recorded in D-4.

## 6. Commit sequencing (stage 2; every commit standalone, rebase-clean)

1. `feat(ui): board and stack delete stage an armed confirm strip`
   — D-2 implementation + T1–T6 + the board-stack plan amendment +
   the two Board census cells.
2. `feat(ui): the habit log gains a decrement recovery affordance`
   — D-3 implementation + T7–T8 + the habit-family plan amendment +
   the Today census cell.
3. `feat(ui): settings removal verbs stage armed confirm strips`
   — D-4 implementation + T9–T12 + the settings-connections plan
   amendment + the three Settings census cells + the T-4 doubt
   ruling note.
4. `chore(ux): gate-green pass` — not a commit; the gate runs per
   commit, `ux-coverage.sh --gate` closes each; the branch hands off
   at PR-green per the shared execution model.

Stage 3 re-runs the gate, executes the walkthrough protocol over
US-5's armed steps, US-1/US-6, and the three settings arm/confirm
pairs, and holds the branch to the T-46 doctrine bar (#829) — in
particular: the staged-clear positive (the walk's armed pickers
disengage-first grammar, `actions.rs:1577`) must not regress under the
new `Flow::ConfirmDelete` arm.

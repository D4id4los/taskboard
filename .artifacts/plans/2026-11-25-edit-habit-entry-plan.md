# Chunk B1 — the Edit Habit entry trio (#1303) — implementation plan

- **Chunk**: round 8 B1 (the round-6 A4 chunk that never dispatched);
  branch `fix/edit-habit-entry`; issue #1303 (p1, bug, ui). No UX
  journey plan required (p1 bug fix; dispatch plan §4).
- **Stage 1 diagnosis** at `origin/main` = `d7beae66` (the mount-site
  trap checked: the r9 H4 shared detail surface is the landed base;
  the round-4 D3b move did NOT relocate the mount — `ViewId::TaskDetail`
  root via `ViewState::open_habit_editor` remains the site).

## 0. The current surface (landed base, verified)

- `ViewState::open_habit_editor(habit)` mounts the shared editor by a
  **root swap**: `replace_root(ViewId::TaskDetail)`
  (`crates/taskboard-ui-slint/src/view_state.rs:1620-1622`). Unlike
  Settings/Status (a pushed `Surface` level restored by truncation),
  the origin view is destroyed by the mount.
- The adapter seeds `habit_edit_draft: Option<HabitEditDraft>`
  (`adapter.rs:667`) only inside the `(Flow::None, UiEffect::HabitEdit)`
  arm (`adapter.rs:5420-5433`), then calls `open_habit_editor`.
- The UI mount discriminator is `is_habit: habit.is_some()`
  (`models.rs:3044`, fed from `habit_edit_draft.as_ref()`
  `adapter.rs:3441`); the habit field mirrors push in the
  `if let Some(draft) = habit` block (`models.rs:3050+`).
- The issue's original defect-1 form ("even on resolution failure the
  editor still mounts", old `adapter.rs:5366-5374`) was repaired
  en passant by later rounds: the arm now ends in the typed refusal
  (`arm_no_target_notice(ActionId::HabitEdit)`). **The empty-shell
  repro as literally filed no longer fires through the context-menu
  path** — but the acceptance is still violated today through the
  residual subject-chain defect (§1) and defects 2/3 verbatim.

## 1. Defect 1 — the subject chain (entry-path-dependent editor)

**Diagnosis.** `Adapter::habit_edit_subject()`
(`crates/taskboard-ui-slint/src/adapter.rs:7780-7800`) resolves
`focused_habit` → `menu_row_subject` → `walk_subject`, filtered by
`is_live`. Two hygiene holes make the resolution entry-path-dependent:

- `adapter.focused_habit` (the adapter-side one-shot,
  `adapter.rs:524`) is initialised `None` once (`adapter.rs:12312`)
  and **never disarmed** — only overwritten. A habit the user focused
  (row click, context open, quota popover) hours earlier still answers
  the chain.
- `menu_row_subject` (`adapter.rs:556`) is **not reset by
  `apply_close_menu`** (`adapter.rs:7571-7597` closes anchor/kind/
  chain/subject-kind but leaves the row subject standing) — a stale
  value survives into later, unrelated menu opens.

Consequences per entry path:
- **Habit row context menu → Edit habit** (armed at
  `open_habit_context_menu`, `adapter.rs:8096-8105`): resolves
  correctly today.
- **Row double-click / walk Return** (`on_habits_edit_asked`
  `adapter.rs:12700-12716`; the walk fallback in
  `habit_edit_subject`): resolve correctly today.
- **Main action menu → Task group → Edit habit** (the row exists in
  the main-menu page, `menu.rs:1253`): **nothing arms a subject at
  this entry**. Resolution rides the stale one-shots — the editor
  opens for whatever habit was last focused (populated with the
  WRONG habit's values), or refuses when none is live. This is the
  residual form of the filed "empty/not the habit's values" defect.
- The draft-seeding `…map(HabitEditDraft::from_habit)` can in
  principle seed `None` while the mount proceeds (subject live but
  the row vanished between the filter and the seed — a race, not
  reachable single-threaded today). Structural hardening in §4 makes
  an unseeded mount unrepresentable.

**Fix shape (stage 2).**
1. One-shot hygiene: disarm `adapter.focused_habit` (and clear
   `menu_row_subject`) on the menu-close leg (`apply_close_menu`) and
   on the root-view landing that already clears the view-state twin
   — mirror the view_state `focused_habit` landing clears
   (`view_state.rs:2304/2325/2338`) at the adapter layer. Keep the
   arming sites as-is (the seams stay first in the chain).
2. Main-menu entry: the Task-group Edit habit row resolves its
   subject from the *current view context* — Today's focused/walk
   habit — never from a surviving one-shot minted by a different
   surface's menu. (The chain already consults `walk_subject`; the
   fix is the disarm law, not a new subject source.)
3. Draft-before-mount structural law: in the `HabitEdit` arm, mint
   the draft first and open the editor only `if let Some(draft)` —
   a seed failure lands in the same typed refusal as a subject
   failure. No `open_habit_editor` call exists without a `Some`
   draft afterwards.

## 2. Defect 2 — the board-name title

**Diagnosis.** `push_chrome`
(`crates/taskboard-ui-slint/src/models.rs:1846`, the title write at
`1870-1874`) special-cases only `ViewId::Today`; every other view —
including the `TaskDetail` root the habit editor mounts as — renders
the selected board's title. There is no habit-editor arm anywhere
(`grep toolbar_title` → the one write site).

**Fix shape (stage 2).** Extend the title match: while the habit
variant is mounted (`habit_edit_draft` present — pass it into
`push_chrome`, it already receives the detail push inputs via
`sync`; thread it through the existing call site `models.rs:1321`
region), the toolbar title is the habit's stored title (stored row's
`title`, falling back to the draft subject's row lookup — never the
board). The task-variant `TaskDetail` title is out of scope (the
task surface carries its own header; only the habit arm changes).

## 3. Defect 3 — cancel/save land on Board (the N-3 violation)

**Diagnosis.** Both editor-exit seams hard-route the board verb:
- cancel: `on_habit_editor_cancelled` →
  `a.route(&window, ActionId::NavFocusBoard)`
  (`adapter.rs:15046-15054`);
- save: the commit seam's tail, identical
  `route(ActionId::NavFocusBoard)` (`adapter.rs:15030-15040`).
`NavFocusBoard` → `view_state.focus_board()` →
`replace_root(ViewId::Board)`. Because the editor mounts by root swap
(§0), the originating view (Today) is destroyed at open and nothing
records it — cancel lands on Board regardless of where the editor
was opened from.

**Fix shape (stage 2).** Record the origin at the mount: extend
`open_habit_editor` (and the adapter draft seed) to snapshot the
current root view — the task detail's entry-snapshot precedent
(`view_state.rs:727` "the `TaskDetail` subject, snapshotted at
entry (D7)"). The cancel and save seams return to the recorded
origin (a typed return-to-origin verb or a direct
`view_state` return call — NOT a re-route through `NavFocusBoard`;
the hard-route dies). Today stays Today; Board stays Board; a
root swap to the same view is a no-op as today.

## 4. Acceptance pins (stage 2 writes these FIRST — all fail pre-fix)

Journey layer (`tests/journeys/habit_surface_journeys.rs` — the
existing pins dispatch `ActionId::HabitEdit` with a test-armed
`focused_habit` only, `:606-612`/`:975`/`:1314`; the menu paths are
unpinned — that is the gap):

1. **Populated-editor pin (all three entries)** — for each entry
   (context menu row invoke, main-menu Task-group row invoke, direct
   `dispatch_action`), open the editor over the fixture habit and
   assert the pushed `HabitDetailModel` mirrors the STORED values
   (title, description, quota fields) — populated fields, not
   non-crash. The context-menu and main-menu pins go through the
   real menu open → `invoke_entry` path (no test-armed focus for the
   main-menu pin — that is the regression this pin exists to catch).
2. **Right-subject pin** — with a second live habit in the fixture
   (so a stale one-shot has a wrong target to resolve to), the
   main-menu entry opens THE row/view's habit, not the previously
   focused one.
3. **Title pin (structural)** — with the editor mounted,
   `window.get_toolbar_title()` equals the habit's stored title
   (never the board title). No text-assertion violation: the fixture
   habit's title is the fixture's own input, the oracle is the
   stored row, not an error/UI-label phrase.
4. **Cancel-origin journey pin (N-3)** — open the editor from
   Today (context-menu path), cancel → `window.get_current_view()`
   is `View::Today`; same from the Board-rooted habit list → stays
   `View::Board`. Save variant mirrors it.
5. **Refusal pin** — a subjectless dispatch (no live habit anywhere
   in the chain) mounts nothing: the view does not change and the
   draft stays `None`; covers "empty-editor regression impossible"
   from the mount side.
6. Unit layer: `view_state` tests for the origin snapshot/return
   (`open_habit_editor` records the root; the return verb restores
   it; a root swap round-trip Today→TaskDetail→Today).

## 5. Stage-2 work list

1. Pins §4 first (TDD — each fails pre-fix; pin 1/2 fail via
   wrong-subject/refusal, pin 3 via board title, pin 4 via Board
   landing, pin 5 partially passes pre-fix — keep it as the
   regression guard, marked as such, not as the failing driver).
2. `view_state.rs`: origin snapshot in `open_habit_editor` + the
   return-to-origin verb (or restore call).
3. `adapter.rs`: the disarm law in `apply_close_menu` (+ the root
   landing twin); draft-before-mount ordering in the `HabitEdit` arm;
   cancel/save seams route the origin return, not `NavFocusBoard`.
4. `models.rs`: the `push_chrome` habit-title arm (thread the draft
   through).
5. Gates: pre-commit hooks + `scripts/run-quality-gates.sh cargo
   nextest run --workspace` (flock; background + poll); every cargo
   call through `scripts/memory-gate.sh`.
6. Commit discipline: `<type>(<module>):` subjects, each commit
   stands alone; hand off at PR-open readiness (stage 3 ships).

## 6. Stage-3 traps

- **The pinned-journey file is the fragile one**: `habit_surface_journeys.rs`
  existing tests arm focus via `invoke_habits_row_activated` — do not
  weaken them; add the menu-path pins beside them.
- **`NavFocusBoard` is load-bearing elsewhere**: only the two editor
  seams' routes change; the back-button hint projection
  (`models.rs:1896-1901`) and the general verb stay.
- **Kiosk posture**: the editor is unmountable on kiosk (the D7 gate,
  adapter arm `(_, UiEffect::HabitEdit) if self.kiosk`); the origin
  return must not disturb the kiosk refusal pins (`us11_s1_*`).
- **`habit_edit_subject` is shared**: the quota popover and history
  dialog openers resolve through the same chain — the disarm law
  changes their posture too; re-run their journeys (the quota
  popover arms `focused_habit` itself, `adapter.rs:5485`).
- **Draft field mirrors vs the shared flow buffers**: the title/
  description rows read the armed flow buffer (`models.rs:3056-3068`);
  the populated-field pin must assert the STORED seed (pre-editing
  state), not post-buffer values.
- **`menu_row_subject` clearing**: verify no consumer depends on the
  stale survival (the engine-sync menu re-derivation at
  `adapter.rs:3476-3495` reads it for the habit submenu page — the
  close leg clearing is safe because the menu is closed when it
  fires, but pin it).

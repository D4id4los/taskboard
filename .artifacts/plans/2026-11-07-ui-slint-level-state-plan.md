# [Plan] Chunk 2 — per-level surface state (`LevelState`)

- **Date**: 2026-11-07
- **Kind**: stage-1 implementation plan for chunk 2 of the W5b dispatch
  (`.artifacts/plans/2026-11-07-hostable-subsurfaces-implementation-dispatch-plan.md`
  §3). Branch `refactor/ui-slint-level-state` off `main` @ `bab41121`
  (chunk 1 merged), worktree
  `../taskboard-WT-refactor-ui-slint-level-state`.
- **Normative inputs**: the design round's §3 **D-H2** (per-level
  state ownership; `ViewState.buffer` per-level; one drop-with-level
  lifetime) and **D-H7 focus side** (the mount focus anchor stored in
  the parent's `LevelState`, restored on child exit); §4 fences. No
  consumer ports, no exit protocol, no new behaviour.
- **Acceptance (dispatch §3)**: `ViewState.buffer`, `TrackFormState`,
  `walk_answers`, `walk_date_text`, `plan_time_text`,
  `plan_effort_text`, and the due-edit buffers no longer exist as
  free-standing fields; every per-flow "wipe when the flow is not X"
  rule is deleted. Existing tests' assertions unchanged. New: the
  chunk-1 proptest extended (a level's state is unobservable after
  its level is truncated); per-slot clean-reopen unit tests. Gates
  green.

## 1. Verified anchors (`main` @ `bab41121`)

| Fact | Anchor |
|---|---|
| `Level { surface: Surface }` — private field, widen-ready; `Surfaces { chain: Vec<Level> }`; truncate the sole removal | `surfaces.rs` (chunk 1 D1) |
| `ViewState.buffer` — one shared `String` behind `buffer()`/`set_buffer()`; cleared at every text flow's open/commit/discard (~15 sites) | `view_state.rs:384-387, 513-519`; clears at :1191, :1361, :1409, :1421, :1497, :1531, :1548, :1600-1738, :2040, :2245; seeding at :1174/:1216 |
| `reconcile` clears the buffer alongside the dead-subject flow drop | `view_state.rs:2429, 2448` (and the editor-family arm :2455) |
| The settle text rule reads the shared buffer (commit non-empty / discard empty) | `view_state.rs:2175-2210` |
| `TrackFormState { seed, picker: Option<DuePickerState>, date_text, start_text, end_text }` — adapter-owned, wiped whenever the flow is not `TrackEntryForm` | `adapter.rs:122-142`; the wipe: `push_track_form` :1672-1682 (also `track_start/end_filter_armed`); seeding :1853-1860; test seam `track_form_mut_for_test` :1815 |
| The due editor's trio + companions: `due_buffer`, `due_echo`, `due_picker: Option<DuePickerState>`, `time_text`, `time_filter_armed`, `due_focus_steal` — wiped whenever the flow is not `EditDate` | `adapter.rs:334-364`; the wipe: `enforce_due_buffer_invariant` :1932-1959 (incl. the `date_picker_grid`/`KeyboardState` disarm and the `debug_assert`); the settle-side picker drop (`was_date`) :1978-1986; commit consumes :2405-2415 |
| `plan_time_text`/`plan_effort_text` — cleared at the commit funnel and every flow-end arm; seeded at re-open | `adapter.rs:196-202`; clears :1254-1255, :2810-2811, :2874-2875, :2897-2898; seed :2888 |
| `walk_answers`/`walk_date_text` — cleared at every walk close path; the date field's clear at its commit; `walk_resume` is the cross-surface handoff hint (stays adapter-side, chunk 1 D9) | `adapter.rs:203-210`; `clear_walk_buffers` :2033-2034 (callers :2270, :2281, :2035 region); date commit clear :2234 |
| The adapter's per-pass test bind seams write the fields directly | `adapter.rs:7012-7092` (plan), :7784-8220 (due), :8407-8459 (setter — **not** in scope), :9010 (walk) |
| `due_edit.rs` is slint-free ("no slint types, no IO") — `DuePickerState` can live inside `LevelState` | `due_edit.rs:1-5` |
| Focus today: `focus_handover: FocusHandover {None, Grid, Field}` (one-shot phase hand-over, adapter-side) and `invoke_refocus_view`/`reset_editing_facts` on walk close | `adapter.rs:115-120, 343-349`; :2273-2282 |
| Refresh never writes level text (rule 4): every pass only *reads* the buffers to push slint mirrors | `push_track_form`, `enforce_due_buffer_invariant`'s push half, the plan/walk mirror pushes |

## 2. Decision record

### D1 — `LevelState`: an enum keyed by state family (A1)

New module `crates/taskboard-ui-slint/src/level_state.rs` (SPDX
AGPL-3.0-only; slint-free like `due_edit.rs` — strings, domain ids and
`DuePickerState` only):

```rust
pub struct LevelState { family: Family, data: Data }  // see below — actually:

pub enum LevelState {
    /// chain[0] and every pushed view level: the focus anchor only
    /// (facts like board/subjects stay root-level ViewState fields,
    /// chunk 1 D6).
    View { anchor: FocusAnchor },
    /// Every text-buffer flow (Create, CreateStack, EditTitle,
    /// EditDescription, Capture, CreateBoard, RenameStack,
    /// RenameBoard): the flow's own buffer.
    Text { buffer: String, anchor: FocusAnchor },
    /// Flow::EditDate: the date editor's whole adapter trio.
    DateEditor { buffer: String, echo: String, picker: Option<DuePickerState>,
                 time_text: String, time_filter_armed: bool, anchor: FocusAnchor },
    /// Flow::TrackEntryForm: the whole TrackFormState.
    TrackForm(TrackFormState),          // TrackFormState gains `anchor`
    /// Flow::PlanPlace.
    PlanPlace { time_text: String, effort_text: String, anchor: FocusAnchor },
    /// Flow::ReviewWalk.
    Walk { answers: Vec<String>, date_text: String, anchor: FocusAnchor },
    /// Every non-text flow (Move, Palette, DragMove, MoveStack,
    /// FocusMode, TrackEntries, …): the anchor is all a level owns.
    Bare { anchor: FocusAnchor },
}
```

- **Keyed by state *family*, not by tag** (a 19-variant tag enum would
  be 19 identical payloads): the pairing tag ↔ family is enforced at
  the one minting site — `Level::new(surface, seed) -> LevelState` (a
  total match over `SurfaceKind`) is the only constructor; `push`
  mints through it. `TrackForm` state on an `EditDate` level is then
  unrepresentable where it is cheap (one constructor), and the family
  accessors stay small. Deliberate leniency, documented at the type:
  `Text` is shared by eight tags — their buffer *semantics* differ
  (trim vs. verbatim) but those live in the flows' commit paths, which
  are unchanged; the family only owns the lifetime.
- `anchor: FocusAnchor` rides every variant: D-H7's anchor is a fact
  about *every* level's exit, not just guests. New slint-free enum:

  ```rust
  pub enum FocusAnchor { None, StatusView, Field(FieldAnchor), Grid(GridAnchor) }
  ```

  whose arms name the restorable targets the adapter already knows
  (the exact arm set stage 2 fixes while wiring `FocusHandover`'s
  consumers — it is the vocabulary of adapter.rs:115-120 and the
  refocus seams, re-homed as data). The adapter maps an anchor to the
  actual window focus call; `FocusHandover` itself stays adapter-side
  (it is the *phase-step* one-shot, not the mount anchor — different
  lifetime, different writer).
- `Level { surface, state }` (both private; `surfaces.rs` widens).
  TrackFormState **moves** to `level_state.rs` (its `#[cfg(test)]`
  visibility dance goes with it — the test seam re-homes to a
  `pub(crate)` accessor on the chain, see D4). `due_edit.rs` is
  untouched.

### D2 — the split: *everything* moves into `LevelState`; the adapter holds none of it

The dispatch asks for the split between ViewState-held and
adapter-held halves and how both drop together. **Decision: no split
— one struct.** All the adapter-side slots (D-H2 names them) are
plain data (strings, `Vec<String>`, `DuePickerState`, bools, domain
ids): they move verbatim into `LevelState` inside `level_state.rs`.
The adapter keeps **zero** copies: its reads/writes go through
`ViewState` chain accessors (D4), its slint mirror pushes read the
level. Truncation drops the `Level` — view_state-held and
adapter-held state die in the same `Vec::truncate`, so a
half-dropped level is unrepresentable by construction (the proptest
invariant asserts the observable consequence, dispatch §3).

What deliberately stays adapter-side (not level state):

- `walk_resume: Option<WalkResume>` — the shape-B handoff hint
  (chunk 1 D9; it *crosses* levels, it does not belong to one).
- The one-shot targets/drafts (`create_target`, `track_entry_draft`,
  `reminder_target`, …) — "consumed by the next dispatch" one-shots,
  a different lifetime (armed by rows, not by a flow).
- `setter_time_text`/`setter_filter_armed` and the timer panel state
  — the timer setter is a panel, not a flow; nothing in the dispatch
  names it.
- `focus_handover` (see D1), `due_focus_steal` — **decision:
  `due_focus_steal` moves into `DateEditor`** (it is armed by the
  date editor's phase steps and consumed by its settle funnel — the
  level owns it); `focus_handover` stays (it hands focus *across*
  mounts, owned by no level).

### D3 — the drop seam: `Surfaces` removals return the dropped levels; `ViewState` drains them per pass

Deleting the per-pass wipes removes the place the slint side-effects
(grid disarm, keyboard global, mirror resets) used to ride. New
shape:

- `Surfaces::truncate_to(depth) -> Dropped` (and the settle fold /
  reconcile arms likewise) return the dropped `Level`s,
  innermost-first. **Truncate remains the sole removal API**; every
  removal now *hands you the corpses*.
- `ViewState` accumulates them into `retired: Vec<Level>` (a
  private drain-on-read queue, exactly the per-pass funnel shape the
  adapter already runs). Sites that already return outcomes fold the
  side-effectful postlude there; sites that drop without an outcome
  channel (`reconcile`, the settle fold) rely on the queue.
- The adapter's per-pass drain (`retire_levels(window)`, called where
  `enforce_due_buffer_invariant`/`push_track_form`'s wipe halves
  stand today) consumes the queue:
  - any retired `DateEditor` level ⇒ the grid disarm today's wipe
    performs (`shell.date_picker_grid = false` + the `KeyboardState`
    global),
  - the *parent's* `anchor` (the level below the deepest retirement —
    the queue is innermost-first, so the survivor is the last entry
    the drain inspects) ⇒ one focus restore,
  - the retired levels are then dropped. Empty queue ⇒ no writes —
    byte-identical to today's guard-when-armed wipes.
- The `debug_assert` half of `enforce_due_buffer_invariant` dies with
  the field; its invariant ("the buffers never outlive the flow") is
  now a construction fact asserted by the extended proptest (D6).

### D4 — the per-slot migration table

| Old (free-standing) | New home | Wipe sites deleted | Accessor surface (signature-kept) |
|---|---|---|---|
| `ViewState.buffer: String` | `LevelState::Text.buffer` (+ `Text` minted with the seed at open: `edit_description` seeds :1174, `edit_title` :1216, capture/creates mint empty) | all ~15 `buffer.clear()` in `view_state.rs` — every one is an open (mint), a commit/discard (the level ends or the field clears *inside* a surviving level — none survive today: every text commit ends the flow), or a reconcile drop (truncation) | `buffer() -> &str` (top text level, else `""`), `set_buffer(String)` (no-op without one — today's post-close writes are already clears; pinned by the existing unit tests, whose assertions are unchanged) |
| `due_buffer`/`due_echo`/`due_picker`/`time_text`/`time_filter_armed` | `LevelState::DateEditor` (minted with the seed from the task's current value — today's open seeds :2436 etc.) | `enforce_due_buffer_invariant`'s wipe branch :1935-1947 (grid disarm → D3's drain); the settle `was_date` picker drop :1978; the commit-consumes clears :2405-2415 (the commit ends the flow — drop covers; any commit that *keeps* the flow alive keeps an in-level clear, the one survivor) | `date_editor() -> Option<&DateEditorState>`, `date_editor_mut() -> Option<&mut _>`; the keystroke seams (:7784, :7924, :7995, :8053, :8149) re-key through it |
| `track_form: Option<TrackFormState>` + `track_start/end_filter_armed` | `LevelState::TrackForm(TrackFormState)` in `level_state.rs` (filters ride inside; `anchor` added) | `push_track_form`'s wipe branch :1677-1682; the seeding resets :1853-1854 (mint covers) | `track_form() -> Option<&TrackFormState>`, `track_form_mut()`; `track_form_mut_for_test` re-homes as `view_state.track_form_mut()`'s test alias — the seam's callers move, assertions unchanged |
| `plan_time_text`/`plan_effort_text` | `LevelState::PlanPlace` (the re-open seed :2888 becomes the mint's seed) | :2810-2811 (toggle-close), :2874-2875 (close-planner resume), :2897-2898 (remove-box) — all flow-ends; the commit funnel's clears :1254-1255 (the commit ends the flow) | `plan_place() -> Option<&PlanPlaceState>`, `plan_place_mut()`; the bind seams :7012-7092 re-key |
| `walk_answers`/`walk_date_text` | `LevelState::Walk` | `clear_walk_buffers` :2033-2034 and its three call sites (every one is a walk close — drop covers); the date-commit clear :2234 stays **inside** the level (the walk survives its date field's commit) | `walk() -> Option<&WalkState>`, `walk_mut()`; the bind seam :9010 re-keys |

- The slint push halves (`push_track_form`, the due editor's push,
  the plan/walk mirrors) keep their per-pass shape but read the
  level; absence renders the empty/default mirror exactly as today's
  wiped state rendered.
- **Rule 4 per level (D9 rule 4)**: no refresh pass writes a level's
  text — the push path is read-only over the chain, the only writers
  are the flow's own keystroke seams and mints, and a seam is armed
  only while its flow is the top level. Per level, "a refresh never
  touches text while a flow holds the buffers" holds by construction;
  the deleted wipes were its redundant belt.
- The stale `settings_return`/`planner_return` doc mentions chunk 1
  left at `view_state.rs:372`/`:848` are rewritten in the docs commit
  (the file is open anyway; comment hygiene, no behaviour).

### D5 — the focus anchor slot (D-H7, wired but flat)

- At every level mint (`push`), the adapter records the anchor the
  FocusHandover machinery would hand over — for chunk 2's flat flows
  this is the existing hand-over vocabulary (grid/field/status view),
  stored in the *parent's* `anchor` before the child mounts.
- On every retirement the drain (D3) restores the surviving parent's
  anchor (one `invoke_refocus`-shaped call per retirement batch —
  today's `window.invoke_refocus_view()` sites :2273/:2282 are this
  mechanism's walk instances; they re-key to the drain).
- No new user-visible behaviour: with depth ≤ 2 (root + one flow) the
  restore target is the root view level's anchor — identical to
  today's flowless refocus. Real nesting exercises it in chunk 3.

### D6 — tests (TDD, first commits)

1. **Proptest extension** (chunk 1's D11 strategy, one new generator
   arm: *write state into the top level, then close/settle/truncate*):
   after every command, **the retired level's state is unobservable**
   — no accessor returns it, a re-push of the same kind mints a
   family-default state (empty buffer/answers/texts, no picker), and
   the `retired` queue is empty at rest. Plus: the minting
   constructor's tag↔family pairing is total (the strategy pushes
   every tag).
2. **Per-slot clean-reopen unit tests** (dispatch §3): for each of
   the five families — open the flow, write state (buffer text, a
   picker open, typed answers), close it by every close path the
   family has (commit, discard/settle, reconcile subject-death,
   foreign settle), assert the re-opened level is family-default via
   the typed accessors (`matches!`/structural, never text).
3. **Anchor unit tests**: mount records the parent's anchor; exit
   drains and the anchor is consumed (read via the drain's return);
   `FocusAnchor`'s mapping to the adapter's focus calls pinned by one
   structural test per arm.
4. **Untouched**: every file chunk 1 named — `flow_table_tests.rs`,
   the keyscope tests, the journeys, `seam_inventory.rs`,
   `view_mount_inventory.rs`, and all existing `view_state.rs` unit
   tests. Their buffer assertions keep passing through the kept
   accessors; any assertion that seems to need changing is a
   behaviour change: stop and report (dispatch hard rule).

## 3. Commit sequence (each standalone green — one slot family per commit)

1. `feat(ui-slint): levels carry their state` — `level_state.rs`
   (`LevelState`, the family structs, `FocusAnchor`, the minting
   constructor), `Level` widened, removals return dropped levels +
   `ViewState::retired` + the adapter drain skeleton, accessors (D4's
   column) added, proptest extended, the D6.1/D6.2 tests. Pure
   addition + the drain; the adapter still compiles on its old fields
   (nothing migrated yet).
2. `refactor(ui-slint): the text buffer lives per level` — the
   `ViewState.buffer` family; the accessor re-keying of
   `settle_flow`/commit paths; ~15 clear sites retire.
3. `refactor(ui-slint): the date editor's buffers live per level` —
   the due family; `enforce_due_buffer_invariant`'s wipe half deletes,
   the grid disarm rides the drain (D3); the commit funnel re-keys.
4. `refactor(ui-slint): the entries form lives per level` —
   `TrackFormState` moves to `level_state.rs`; the push_track_form
   wipe branch and the test seam re-home.
5. `refactor(ui-slint): the plan form lives per level` — the
   PlanPlace family.
6. `refactor(ui-slint): the walk buffers live per level` — the Walk
   family (`clear_walk_buffers` retires; `walk_resume` untouched).
7. `feat(ui-slint): the focus anchor rides the level` — D5's
   record-at-mount / restore-at-retire wiring.
8. `docs(ui-slint): the level state documents itself` — the stale
   return-slot mentions (D4 tail), module docs; no CHANGELOG (no
   user-visible change).

Full gates via `scripts/run-quality-gates.sh` before handoff.

## 4. Risks

- **R1 — the due editor's D25 keep**: the field doc says "the clean
  close keeps the buffer/echo (D25)" (`adapter.rs:336-338`) — under
  drop-is-wipe the level dies at close. Read of the code: the wipe
  pass runs before the next push and nothing reads the trio between
  close and wipe, so the keep is unobservable; but stage 2 must prove
  it (grep every `due_buffer`/`due_echo` reader for post-close
  ordering) before deleting `enforce_due_buffer_invariant`. If a
  same-pass read exists, the close handler reads the level *before*
  truncation — behaviour byte-identical, and the plan is amended.
- **R2 — side-effect ordering in the drain**: today's wipes run
  inside specific funnels (settle, per-pass enforce) at exact points
  relative to the focus seams (the R-C exactness comment
  :1941-1946). The drain must sit where the enforce funnel stood
  (same pass, same order) so the grid cannot be armed over the next
  editor. Mitigation: the drain is called from exactly the sites the
  wipes stood at, not a new pass.
- **R3 — accessor fan-out**: the due family alone has ~15 direct
  field touches in `adapter.rs` plus the bind seams (:7784-8220);
  each becomes a `date_editor_mut()`-gated access. Mechanical but
  wide — the estimate below is dominated by it. The bind-seam bodies
  are test helpers: their moves are call-site moves, their
  assertions do not change.
- **R4 — `buffer()`/`set_buffer()` with no text level**: existing
  tests call them around `Flow::None` states; the accessors must
  return `""` / no-op, which is byte-equal to today's cleared-buffer
  reads. The existing suite (D6.4) is the guard — no assertion may
  move.

## 5. Estimated diff

`level_state.rs` new ≈ 400–450 (types + minting + tests),
`surfaces.rs` ≈ 80 (Level widens, removals return dropped levels),
`view_state.rs` ≈ 300 (buffer family, reconcile arms, accessors,
retired queue), `adapter.rs` ≈ 450–550 (five families re-keyed, wipe
branches deleted, drain + anchor wiring, bind seams moved).
Total ≈ **1.2–1.4k lines**, dominated by mechanical re-keying;
`flow_table_tests`, the keyscope tests, every journey and both
inventories: **zero assertion changes**.

## 6. Backlog issues

None opened at planning time: the D25-keep question (R1) is
in-scope verification for stage 2, the timer-setter and one-shot
slots are recorded out-of-scope decisions (D2), and the stale comment
cleanup is folded into commit 8. No new out-of-scope work discovered.

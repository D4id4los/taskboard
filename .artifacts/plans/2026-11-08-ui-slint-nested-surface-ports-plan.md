# [Plan] Chunk 3 — the nested-surface ports (exit protocol; N2 → N3 → N1)

- **Date**: 2026-11-08
- **Kind**: stage-1 implementation plan for chunk 3 of the W5b dispatch
  (`.artifacts/plans/2026-11-07-hostable-subsurfaces-implementation-dispatch-plan.md`
  §4). Branch `refactor/ui-slint-nested-surface-ports` off `main` @
  `53ffa1fc`, worktree
  `../taskboard-WT-refactor-ui-slint-nested-surface-ports`.
- **Normative inputs**: design round §0 **Q3** (foreign settle of a
  text level follows the text rule), **Q4** (subject death = the
  subtree just disappears, no exit), **Q6** (a kind at most once per
  chain) and §3 **D-H3** (the exit protocol), **D-H4** (settle
  composition; the Escape-ladder generalization), **D-H8 steps 3–4**;
  §2 N1–N3 binding (N4 stays the shape-B handoff); §4 fences (no
  engine change, no new user-visible surfaces, no embedded planner).
- **Acceptance (dispatch §4)**: user-observable behaviour of the
  entries overlay, its form and picker, and the walk's
  Inbox/Carry-forward row affordances unchanged — existing journeys
  pass with **zero assertion changes** (verified below: the journeys
  read slint mirrors and drained commands, never the flattened
  shapes); the flattened-representation assertions are enumerated
  with their exact re-expression (§4); the hand-rolled ladder rungs
  and `WalkStage`'s sub-state fields are deleted; the exit-protocol
  unit tests and the nested-chain proptest cases land first; gates
  green.

## 1. Verified anchors (`main` @ `53ffa1fc`)

| Fact | Anchor |
|---|---|
| `WalkStage::Inbox { filing: Option<StackId> }`, `CarryForward { picking_date: bool }`, `Prompts` — the sub-state fields to delete | `view_state.rs:184-194` |
| The walk's armed-affordance mutators: `review_walk_file` :2047, `review_walk_file_target` :2065, `review_walk_pick_date` :2089, `clamp_review_walk` :2108 (armed-candidate death → disarm; empty inbox → stage advance) | `view_state.rs` |
| The walk's Escape ladder `back_review_walk` :2008 = `cleared_substate` :2034 → stage-back → close; the ladder's rung order is plan D-W2 | `view_state.rs` |
| The commit paths that consume the sub-state: `FileCommit` (adapter :2100 — dispatches `MoveTask` then `review_walk_next_row`), `CarryDateCommit` (:2164 — parses the walk's `date_text`, dispatches `UpdateTask`, keeps the walk alive with the in-level clear :2226) | `adapter.rs` |
| `step_back_track_entries` :1868 — the entries ladder's flattened branch (`TrackEntryForm` → replace with `TrackEntries`) and close branch | `view_state.rs` |
| The entries form's picker: `TrackFormState.picker: Option<DuePickerState>` (now `LevelState::TrackForm`), toggled at `on_track_form_grid_toggled` :8665, keyed at `on_track_form_grid_key` :8674, day-clicked writes `date_text` + cursor :8708, mirrored by `push_track_form` :1711-1756 (`track_form_grid_open`) | `adapter.rs` |
| The track-form grid is **embedded** in the panel (r4 D-TS4): keys arrive through the panel's `date-grid-key` FocusScope seam, not the keymap; no `date_picker_grid` context arm (that bit arms only the window-level due picker) | `ui/compositions/track-entries.slint:197-202`, `keyscope.rs:125` |
| The walk overlay's armed mirrors: `ReviewWalkModel { filing, picking, candidate_index, date_text }` pushed from the stage match | `models.rs:2811-2916`; `ui/views/review-walk-overlay.slint:195-280` |
| `Flow::TrackEntryForm { task, seed }` opens via `write_flow` (sibling replace) at `open_track_entry_form` :1855; the list at `open_track_entries` :1847 | `view_state.rs` |
| Chunk-1 machinery: `Surfaces::{truncate_to, end_top_flow, replace_top_flow, offer, settle_fold}`; `FlowTag` (19 tags) with `scope_head()` declarations; `AlreadyInChain` at the one push site | `surfaces.rs:256-398, 407` |
| Chunk-2 machinery: `LevelState` families minted at `Level::new(SurfaceKind)` (one constructor, tag↔family pairing); removals return dropped levels → `ViewState.retired` → the adapter drain (grid disarm + the surviving parent's `FocusAnchor` restore); `LevelState::Walk { answers, date_text, anchor }`, `LevelState::TrackForm(TrackFormState)` | `level_state.rs:131 mint`, `view_state.rs:391-560` |
| The offer walk answers with the innermost flow level (`responding_flow` :2437); view levels bubble; the root's arms are total | `adapter.rs` |
| The scope head arming facts stay the `KeymapShell` booleans (chunk 1 D10: full concatenation deferred to this chunk) | `keyscope.rs:88-146` |
| Journeys read slint mirrors only: the entries ladder journey asserts `track_form_open`/`track_entries_open` (journeys.rs:12552); the walk journeys assert `walk_model.stage_kind` and drained commands (:15083-16066, J1–J20); **no journey matches `WalkStage`'s sub-state fields or `Flow::TrackEntryForm`** (grep verified) | `tests/journeys.rs` |
| Flattened-shape matches live in: `view_state.rs` unit tests :5499-5700 (`walk_flow` tuples), `flow_table_tests.rs` :97/:1017/:1029 (fixtures), `presentation.rs` tests :4671/:4688 (constructors), `adapter.rs` :2101/:2166/:2849/:9030 (arms/seam) | enumerated in §4 |
| `walk_resume: Option<WalkResume>` is adapter-side; the handoff closes the walk then re-opens via the planner route | `adapter.rs:175-2032`; chunk 1 D9 |

## 2. Decision record

### D1 — the exit types (A1; D-H3)

New types in `surfaces.rs` (the chain's own vocabulary; slint-free):

```rust
/// What a completed guest hands its parent (D-H3): a payload exists
/// only for parent-local continuation — the File-to picker's target
/// stack, the Pick-a-date field's parsed date. A guest whose
/// completion the parent does not consume completes without one.
pub enum LevelResult {
    Stack(StackId),
    Date(NaiveDate),
}

pub enum GuestExit {
    /// `None` = the parent consumes the completion, not a payload
    /// (the entries form: the list just stays).
    Completed(Option<LevelResult>),
    Cancelled,
}
```

- The `Option` inside `Completed` is the documented leniency (A1
  policy §2.1: deliberate leniency documented at the type): D-H3's
  shape is `Completed(LevelResult)`, but the entries form completes
  payload-less, and a third `Completed`-without-payload variant would
  give "no payload" two spellings at every match site. Payload
  variants are closed: no `GuestExit` grows an arm for a continuation
  no parent consumes.
- **`Cancelled` and `Completed(None)` are distinguishable on purpose**
  even where every current parent ignores both: the ignore is the
  parent's *default*, not the protocol's (a future parent may react
  to Cancelled; no one may ever react to a payload it did not ask
  for).

### D2 — delivery seam: `end_top_flow_with`, exits only through it

```rust
impl ViewState {
    /// Ends the top flow level, delivering `exit` to the level now
    /// beneath it (its `on_child_exit`). The chain truncates to the
    /// top flow level's index — the subtree goes with it, the
    /// parent's own ancestors stay.
    pub fn end_top_flow_with(&mut self, exit: GuestExit) -> Vec<FlowOutcome>
}
```

- `on_child_exit` is a `ViewState` method: `match` over the level
  beneath the ended subtree's flow, with the **default arm
  `Vec::new()`** (D-H3: default ignore). The handler is pure — it
  returns `Vec<FlowOutcome>` (commands for the File-to commit, buffer
  writes ride the parent's own level state, `RefreshNow`); the
  adapter applies outcomes through the existing funnel.
- **Exits ride only `end_top_flow_with`.** The retire drain
  (`truncate_to`/`end_top_flow`/`settle_fold`/`reconcile`) never
  delivers an exit — Q4 is then a construction fact: subject death
  truncates silently, exactly as chunk 1 left it. Pinned by a unit
  test that kills a subject under an open picker child and asserts
  the walk received no continuation (no `MoveTask`).
- Delivery reads the parent's flow *after* truncation (the parent is
  `top_flow()` of the shortened chain) — a child's payload can never
  be handed to a grandparent, because truncation removed the
  intermediate levels with the child only if the child's parent died
  first, which subject death handles through Q4, not through exits.

### D3 — the three ports' surfaces, kinds, and state homes

D-H2's split governs: pure interaction state stays in the `Flow`
variant; adapter-side text stays in `LevelState`. One new `Flow`
variant per child kind (the dispatch names "which Flow variants are
created or retire"):

| Port | New `Flow` variant (new `FlowTag`) | Child's state home | Parent payload consumption |
|---|---|---|---|
| N2 | `Flow::EntryDatePicker` (`FlowTag::EntryDatePicker`) | new `LevelState::Picker(DuePickerState)` family (the r4 D-TS4 reuse: the grid/cursor/month machinery verbatim; the time half unused) | `Completed(Some(Date))` → the form writes `date_text` (today's `on_track_form_day_clicked` body, minus the cursor poke) |
| N3a | `Flow::WalkFilePick { armed: Option<StackId> }` (`FlowTag::WalkFilePick`) | variant data (it was `WalkStage` sub-state — pure interaction state per D-H2; family `Bare` + anchor) | `Completed(Some(Stack(stack)))` → the walk dispatches `MoveTask` + `review_walk_next_row` (today's `FileCommit` body :2100) |
| N3b | `Flow::WalkDatePick` (`FlowTag::WalkDatePick`) | new `LevelState::WalkDate { date_text, anchor }` — **not** the `Text` family (see D5) | `Completed(Some(Date(instant)))` is minted **adapter-side**: the child's commit parses `date_text` (today's `CarryDateCommit` parse :2164); parse failure keeps the child live with the text (today's rule), so the payload is the *parsed instant* → the walk dispatches `UpdateTask` + next row; the walk's `date_text` field and its in-level clear (:2226) retire — the child's death is the clear |
| N1 | `Flow::TrackEntryForm` **stays** (no new variant) | unchanged (`LevelState::TrackForm`, minus the `picker` field — it moves to the N2 child) | `Completed(None)` → ignored (the default); `Cancelled` → ignored |

- **Retirements**: `WalkStage` loses both sub-state fields (field-less
  `Inbox | CarryForward | Prompts`); `TrackFormState.picker` is
  deleted; `LevelState::Walk.date_text` is deleted (the
  `WalkDatePick` child owns the text); `cleared_substate` (:2034) and
  `step_back_track_entries`'s flattened branch (:1869-1871) are
  deleted, not left dormant (dispatch hard rule).
- **The minting constructor** (`LevelState::mint`) gains the
  `Picker`/`WalkDate` arms keyed on the new tags — the tag↔family
  pairing stays total (the chunk-2 proptest's totality arm covers the
  new tags for free once the strategy pushes them; D7).
- **`WalkFilePick`'s armed candidate starts `Some(first)`** — today
  `review_walk_file` arms with the first candidate and refuses empty
  (:2055); the arm pushes only when candidates are non-empty, so the
  child is never minted unarmed; the `Option` remains for the
  candidate-death clamp (D6).

### D4 — open/close paths per port (who pushes, who ends)

- **N2**: `on_track_form_grid_toggled` (:8665) becomes: picker open →
  `end_top_flow_with(GuestExit::Cancelled)`; picker closed → push
  `Flow::EntryDatePicker` minting `DuePickerState::open(...)` with
  today's anchor arithmetic (:8676-8678). A refusal
  (`AlreadyInChain`) maps to a no-op — unreachable, pinned by unit
  test (D8). `on_track_form_grid_key` (:8674) re-keys to the child's
  `DuePickerState` (`entry_date_picker_mut()`-shaped accessor);
  `on_track_form_day_clicked` (:8708) becomes: the child ends
  `Completed(Some(Date))`; the form's `on_child_exit` writes
  `date_text`. The grid's `Commit` key (focus hand-over to the date
  field, :8694) **ends the child `Cancelled`** — today it is a no-op
  key with focus returning to the field while the grid stays open;
  observable difference: none rendered (`track_form_grid_open`
  follows the child's presence, so the grid closes where today it
  stayed open). **Checked against the journeys**: the quick-list and
  grid journeys (:14561, :8631-region) close the grid by toggle,
  click, or Escape — none pins "grid open, focus on field". Stage 2
  verifies with the journeys green untouched; if one pins it, STOP
  and report (dispatch hard rule). *Alternative rejected*: keeping the
  no-op would need the child to survive without holding the focus —
  a half-mounted level, the exact shape the protocol exists to
  forbid.
- **N3a**: `WalkIntent::File` (:2084) pushes the child with
  `armed: Some(first)`; `FileTargetPrev/Next` (:2091-2096) re-key to
  the child's `armed` (the wrapped ring unchanged); `FileCommit`
  (:2100) becomes: read the child's armed stack, end the child
  `Completed(Some(Stack))`, and let the walk's `on_child_exit` run
  today's body (dispatch `MoveTask` + `review_walk_next_row`, the
  handoff branch preserved verbatim). The candidate click seam
  (:9025) writes the child's `armed` instead of the stage's `filing`.
- **N3b**: `WalkIntent::CarryPickDate` (:2163) pushes
  `Flow::WalkDatePick` (mint `WalkDate` empty); `on_walk_date_edited`
  (:9051) re-keys to the child; `CarryDateCommit` (:2164) keeps the
  parse-keep-text rule *inside the child*, then ends it
  `Completed(Some(Date(instant)))`; the walk's `on_child_exit`
  dispatches `UpdateTask` + `finish_walk_row`. `WalkDatePick`'s own
  Escape is the generic Cancelled rung (D6) — today the armed date
  field's Escape is the ladder's sub-state rung (same stage, cleared).
- **N1**: `open_track_entry_form` (:1855) **pushes** the form above
  the list instead of `write_flow`. The push site is inside the
  list's own handler (row-activation / the add button) — the
  structural admission rule: no other path opens the form.
  `reconcile`'s dead-subject arms and the view-strand rule
  (:2687-2704) keep their predicates (they match both variants;
  truncation at the *list's* index — the child goes with the parent,
  Q4 by construction; stage 2 verifies the truncate depth is the
  list's, not the child's).

### D5 — Q3 ruling applied: the walk's date child is not a text-settle level

The `Text` family's foreign-settle rule (commit non-empty / discard
empty) **does not** apply to `WalkDatePick`: today a foreign settle
over the walk closes the whole flow — typed date text is *discarded*,
never committed (no journey or test pins otherwise; the walk's own
close paths all drop the buffers). Under the port, the fold settles
each level by its **own kind rule**: `WalkDatePick` maps to a settle
kind whose rule is *end `Cancelled`* (the modal family's terminator —
the same rule `ReviewWalk` itself settles by), so a navigation away
mid-typed-date discards exactly as today. `FlowKind` gains **no**
variant: the child's kind is `FlowKind::None` (the modals' row —
settle leaves kept-arm-owned surfaces; the child's close paths are
its exits, and its settle arm is the Cancelled end). This is the Q3
ruling *scoped correctly*: the text rule governs levels whose settle
kind is `Text`; the walk's date field never settled as text. The
entries form's typed fields likewise keep today's rule (their settle
kind stays `None` — the overlay cells — and refresh leaves them
alive; nothing changes). Pinned by a unit test: foreign settle with a
half-typed walk date dispatches nothing and the text is gone.

### D6 — the Escape ladder generalization (D-H4) and the rungs it deletes

The generic rule: a level's own rungs first (its kept arms), then one
**Cancelled rung** that ends exactly the top level — never two levels
in one stroke. Expressed as one shared helper the kept arms call:

```rust
impl ViewState {
    /// The Cancelled rung: ends the top flow level `Cancelled` when
    /// it sits above another flow level; `None` when the top flow
    /// level is at depth 1 (no parent — the caller's own close rung
    /// applies). Inert without a flow level.
    pub fn cancel_top_child(&mut self) -> Option<Vec<FlowOutcome>>
}
```

- The walk's kept arm (`WalkIntent::Back` :2071): child live →
  `cancel_top_child()`; else today's stage ladder (`back_review_walk`)
  minus `cleared_substate` — its rungs become: Prompts → Carry
  forward → Inbox → close, verbatim. The sub-state rung (sub-state →
  stage-back) is **replaced by the generic rule**, exactly D-H4's
  "a level's own rungs, then one `Cancelled` rung".
- The entries overlay's arm (`UiEffect::TrackEntryDismiss` :2644): the
  Insert-mode rung stays (a mode flip is not a level rung); the
  second rung becomes `cancel_top_child()` when the form child is
  live (ends the form, the list stays — today's `RefreshNow`-only
  outcome shape preserved by the helper returning the same outcome
  set); else the list's own close rung (`close_track_entries`,
  `SetMode`+`RefreshNow` — unchanged). `step_back_track_entries`
  retires whole.
- **Focus**: the Cancelled/Completed retirement rides the chunk-2
  drain — the surviving parent's `FocusAnchor` restores (one restore
  per retirement batch). The anchors each port records at mount:
  N2 → the form's date field (the grid's `focus-on-mount` mirror and
  the drain's grid disarm both key on the child level's retirement —
  the `Picker` family joins the drain's disarm rule beside
  `DateEditor`); N3 → the walk's cursor row (the armed candidate
  list / date field mount-focus facts); N1 → the activated list row.
  No new focus machinery: the ports are the first real tenants of
  chunk 2's D5 wiring; `focus_coverage` and the walk J-ladder
  journeys are the guard.

### D7 — Q6 verification: no kind repeats in any real chain

The reachable chains after all three ports:

- `[View(_) , TrackEntries, TrackEntryForm, EntryDatePicker]` (N1+N2)
- `[View(_), ReviewWalk, WalkFilePick]` / `[View(_), ReviewWalk, WalkDatePick]` (N3)
- everything pre-existing: root + ≤1 flow (XOR at depth 1), over-view
  + flow.

All four tags are new and appear at most once in their only possible
chains; `TrackEntries`/`TrackEntryForm` stay distinct tags. **No kind
repeats in any real chain** — the `AlreadyInChain` refusal is
unreachable from the UI. It stays a typed refusal mapped to a no-op
at each new push site (a defensive `Err` arm returning
`vec![FlowOutcome::RefreshNow]`, the chunk-1 D7 posture), pinned by
unit tests through the real open paths. The once-per-chain rule needs
no relaxation and no table.

### D8 — tests (TDD, first commits)

1. **Exit-protocol unit tests** (dispatch §4, land with commit 1):
   - `Completed(Some(Stack))` reaches the walk parent (the
     `MoveTask`+next-row outcomes observed); `Completed(None)` from
     the form reaches the list as the default ignore (no outcomes
     beyond the refresh the caller adds); `Cancelled` from the picker
     reaches the date field's parent — ignored, the form level
     intact.
   - Subject death (Q4): kill the subject under an open picker child
     → the subtree disappears, **no exit delivered** (no continuation
     command drained).
   - The default: a parent whose `on_child_exit` has no arm (every
     flow kind except the three parents) ignores — asserted by
     `matches!`/drained-commands, never text.
   - A grandchild payload never crosses an intermediate parent: push
     list → form → picker (synthetic through the chain API), complete
     the picker with `Date` — the form consumes it (`date_text`
     written), the list observes nothing.
2. **Nested-chain proptest cases** (dispatch §4): extend chunk 1's
   strategy with the three port families — a `form → picker` chain
   and a `walk → picker` chain occur; invariants re-asserted over
   them: child never outlives parent; a `Handled` effect never
   reaches an outer level; settle idempotent; state unobservable
   after truncation (chunk 2's extension, now with the `Picker`/
   `WalkDate` families); duplicate-kind push is `Err(AlreadyInChain)`.
   No ceiling assertions (the depth budget stays the command length).
3. **The re-expressed unit tests** (§4's table) move with their ports'
   commits — each listed assertion re-expresses the same fact over
   the chain (stage presence + child presence/armed value), never a
   weaker one (AGENTS §5: no weakening).

### D9 — the scope-stack concatenation (chunk 1 D10's remainder, landed here)

`scope_stack` concatenates each level's declared contexts
innermost-first, **stopping at the first masking context** (D-H6):
walk the chain topmost-first, append `FlowTag::scope_head()` (and the
root view's contexts), stop once a head that masks was appended. The
new children declare **empty** heads (`_ => &[]` already) — so for
every reachable state the concatenated result is byte-equal to
today's single-level read: the first mask (`TrackEntries` /
`ReviewWalk` / a modal head) is encountered at the same position, and
the silent children append nothing. The `KeymapShell` boolean arming
facts stay (the picker families' grid bit keeps its owner — the
window-level due picker; the embedded track-form grid has no context
bit today and gains none). Pinned by: the existing keyscope ordering
tests and T-S1 checklist untouched (their `ScopeInputs` literals are
unchanged), plus one new unit test walking a synthetic
`[root, TrackEntries, TrackEntryForm, EntryDatePicker]` chain
asserting the head is `[TrackEntries, ViInsert, Global]` — identical
to the flat read.

## 3. Commit sequence (each standalone green; one port per group, N2 → N3 → N1 per the dispatch)

1. `feat(ui-slint): the exit protocol` — `GuestExit`/`LevelResult`
   (D1), `end_top_flow_with` + `on_child_exit` default-ignore (D2),
   `cancel_top_child` (D6), the D8.1 unit tests, the proptest's
   nested-chain generator arms (D8.2). Pure addition — nothing wired,
   every journey green.
2. `refactor(ui-slint): the form's date picker is a child level` —
   **N2**: the `EntryDatePicker` variant + tag + `Picker` family,
   `TrackFormState.picker` deleted, the toggle/key/day-clicked seams
   re-keyed (D4), the drain's `Picker` disarm, the re-expressed
   track-form-picker unit assertions, the D4 grid-Commit journey
   verification.
3. `refactor(ui-slint): the walk's file-to picker is a child level` —
   **N3a**: `WalkFilePick`, `WalkStage::Inbox` field-less, the
   File/FileTarget/FileCommit arms re-keyed, the candidate-click seam
   re-keyed, `clamp_review_walk`'s disarm re-expressed as the child's
   Cancelled end, unit assertions re-expressed.
4. `refactor(ui-slint): the walk's pick-a-date is a child level` —
   **N3b**: `WalkDatePick` + the `WalkDate` family,
   `LevelState::Walk.date_text` deleted, the Carry arms re-keyed, the
   D5 foreign-settle discard pinned.
5. `refactor(ui-slint): the entries form is the list's child` —
   **N1**: `open_track_entry_form` pushes, `step_back_track_entries`
   retires, the TrackEntryDismiss arm becomes cancel-or-close
   (D6), reconcile depths verified (D4).
6. `refactor(ui-slint): the scope stack concatenates the chain` —
   D9.
7. `docs(ui-slint): the nested ports document themselves` — the
   design round's status line → **implemented (2026-11-08, chunks
   1–3)**; the `WalkStage`/overlay doc comments (:176-194,
   `Flow::TrackEntryForm`'s "layered in the Escape ladder") rewritten
   to the chain vocabulary; `docs/architecture.org`'s UI section
   sentence if it names the flattened shapes. No CHANGELOG entry (no
   user-visible change).

Full gates via `scripts/run-quality-gates.sh` before handoff.

## 4. Flattened-representation assertions: the exact re-expression table

No journey file changes (verified: `tests/journeys.rs` matches
`walk_model.stage_kind`, `track_form_open`, `track_entries_open` and
drained commands only — all of which keep their semantics). The
flattened-shape matches are:

| Site | Today | Re-expression |
|---|---|---|
| `view_state.rs` walk unit tests, `walk_flow` tuples :5517, :5529, :5550, :5572, :5579, :5587, :5612, :5617, :5629, :5638, :5662 | `WalkStage::Inbox { filing: None }` / `CarryForward { picking_date: false }` / `CarryForward { picking_date: true }` / `Inbox { filing: Some(_) }` | The stage arm drops its field (`WalkStage::Inbox`, `WalkStage::CarryForward`); the armed facts re-express as a child assertion beside it (`walk_child(&vs)`-shaped helper: `matches!(child, Flow::WalkFilePick { armed: None })` / `…Some(_)` / `matches!(child, Flow::WalkDatePick)` / no child). Each tuple pins the same observable stage + armed state; nothing weakened |
| `flow_table_tests.rs` :97 (`FK::TrackForm`), :1017/:1029 (fixtures building `Flow::TrackEntryForm` / `WalkStage::Inbox { filing: None }`) | fixtures | Call-site moves: the variant stays; the stage constructor loses its field. **One cell changes (deviation, recorded 2026-11-08 at review)**: `(FK::TrackForm, TrackEntryDismiss)` was `exp(Z, K(KTrackList))` ("the ladder steps form → list") and becomes `exp(Z, N)` — the flat fixture arms the form at depth 1 with no list beneath, so its dismissal now reads the close; the fact the old cell pinned (form dismiss steps back to the list over the real chain) is re-expressed by the view_state unit tests over the genuine `TrackEntries → TrackEntryForm` chain (`cancel_top_child` ends exactly the form, the list stays). Not a journey-level change; nothing weakened |
| `presentation.rs` tests :4671, :4688 | `WalkStage::Inbox { filing: None }` as a constructor argument | `WalkStage::Inbox` — call-site move |
| `adapter.rs` :2101 (`FileCommit`'s `Inbox { filing: Some(target) }` read), :2166 (`CarryDateCommit`'s `picking_date: true` gate), :9030 (candidate-click write) | production arms | Re-keyed to the child levels per D4 — behaviour-identical bodies |
| `adapter.rs` :2849 (`walk_resume`'s `open_review_walk_at(…, WalkStage::Prompts)`) | constructor | `WalkStage::Prompts` — unchanged (field-less arm) |
| `adapter.rs`/`models.rs` stage match (:2811) | destructures `filing`/`picking_date` for the mirrors | The mirrors read the child levels: `filing` = `WalkFilePick` child live, `candidate_index` = its armed position, `picking` = `WalkDatePick` child live. The slint property semantics are unchanged (the journeys' mirror reads hold) |
| `keyscope.rs` `FlowTag::scope_head()` :411 | `TrackEntries | TrackEntryForm` share a head | Unchanged (both tags keep their declarations; the new tags fall in the `_ => &[]` arm) |

The `WalkStage` doc comment (:176-182) and the `Flow::TrackEntryForm`
doc (:285-286, "layered in the Escape ladder") are rewritten in commit
7 — comment hygiene, no behaviour.

## 5. Risks

- **R1 — the N2 grid-Commit key** (D4): the one spot where the port
  *could* tighten behaviour (today the grid stays open under a
  field-focused Enter; the child model closes it). The journey sweep
  found no pin; stage 2 must run the track-form journeys before
  committing port 2 and STOP if one fails (dispatch hard rule). If a
  pin surfaces, the fallback is recorded here: the `Commit` key ends
  the child `Cancelled` **and** the arm re-opens nothing — the same
  fallback cannot preserve "grid open + field focus" without a
  half-mounted level, so that finding would escalate to the user as a
  design question.
- **R2 — `FileCommit`'s outcome ordering**: today the `MoveTask`
  command is *built before* `review_walk_next_row` mutates the stage,
  and the handoff branch applies outcomes then hands off (:2125-2135).
  The port must keep that order (payload consumed → command built →
  next row → possible handoff); a reordered variant would dispatch
  against a pre-advance cursor. The J-journeys drain commands per step
  and are the guard; the unit test D8.1 pins the order explicitly.
- **R3 — `clamp_review_walk`'s dual normalization** (armed candidate
  dies; inbox empties): the child-Cancelled end must happen *before*
  the empty-inbox stage advance in the same pass, or a one-pass
  window could show Carry forward with a stale armed child mirror.
  Today's order (disarm, then stage advance) is preserved by
  construction if the re-expression keeps the two ifs in sequence;
  pinned by a unit test.
- **R4 — the drain's anchor restore with three-deep chains**: chunk 2
  exercised depth ≤ 2. The first real depth-3 chain (list → form →
  picker) must restore the *form's* anchor (the immediate parent), not
  the list's — the drain's "the level below the deepest retirement"
  rule (chunk 2 D3) already names the survivor; the D8.1 grandchild
  test pins it.
- **R5 — hidden `WalkStage` matches**: any `matches!(stage,
  WalkStage::Inbox { filing: Some(_) })`-shaped site outside the
  enumerated ones would break compile (field-less enum), not behaviour
  — the compiler is the completeness check for the field deletion; the
  §4 table is checked against that error list at stage 2.

## 6. Estimated diff

`view_state.rs` ≈ 250 (exit seam + ladder rewrites + open paths +
unit re-expressions), `adapter.rs` ≈ 300 (seam re-keys, rung
deletions, grid disarm), `level_state.rs` ≈ 120 (two families, two
deletions, mint arms), `surfaces.rs` ≈ 60 (exit types +
`cancel_top_child` plumbing), `models.rs` ≈ 40, `keyscope.rs` ≈ 40,
docs ≈ 30, journeys/inventories/`flow_table_tests`: **zero assertion
changes** (fixture call-site moves only). Total ≈ **800–900 lines**.

## 7. Backlog issues

None opened at planning time: every borderline finding (the N2
grid-Commit semantics, the walk-date settle kind, the scope
concatenation remainder) is settled as an in-scope decision above
(D4, D5, D9), and N4/the embedded planner is already recorded as
deliberately undispatched (dispatch §5).

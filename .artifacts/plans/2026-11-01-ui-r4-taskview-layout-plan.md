# [Plan] GUI Review Round 4, Chunk 5 — Task-View Layout

- **Date**: 2026-11-01
- **Status**: PROPOSED — stage-1 plan for chunk 5 of the round-4
  dispatch. Branch `fix/ui-r4-taskview-layout`, cut from `main`
  (4d8be313; `scripts/worktree-doctor.sh` passed pre-creation, 230 G
  free against the 25 G floor).
- **Origin** (normative): design round
  `2026-11-01-gui-review-round4-plan.md` §3.5 (D16–D17), §2 decision 5
  (per-second UI ticking), §1 findings R4-F9/F10/F11/F15/F16, §7-R7;
  dispatch plan
  `2026-11-01-gui-review-round4-implementation-dispatch-plan.md`
  §1/§6 (acceptance a–e; blanket acceptance rule; code-quality
  mandate per §1).
- **Scope guard**: `taskboard-ui-slint` only (slint + its `src/`).
  `taskboard-domain`, `taskboard-state`, storage, sync, app: **empty
  diff**. `taskboard-domain/src/tracking.rs` was read (TimeEntry
  36-56, `duration` 117-124, `newest_open` 132-138,
  `open_entries` 128-130) but is **read-only** — the summary reads
  engine state through existing projections; no domain change.
  Non-goals per dispatch §6: no focus/navigation sweep (chunk 6 —
  only the blanket-rule walk stops land here), no entries list
  (chunk 10), no reminder rows (chunk 8), no timer rows (chunk 9).
  The card tracking badge stays fenced (#489);
  `open_elapsed_phrase` stays minute-bucketed for the status chip.
- **All `file:line` anchors are this worktree at 4d8be313**; the
  design round's anchors were re-derived (several drifted ~40 lines
  in `presentation.rs`/`models.rs`); every one below was read, not
  carried over.

## 0. Findings → mechanisms audit (what the code confirms)

- **F9 (back on its own row)**: the back `IconButton` sits in its own
  `HorizontalLayout` (`task-view.slint:155-165`) with the view-wide
  `space-3` row spacing — a full ~32 px row carrying one 32 px button.
- **F10 (phantom padding)**: no element in the view claims leftover
  height; the trailing `Rectangle { vertical-stretch: 1 }` spacer
  (`task-view.slint:398`) absorbs it invisibly — the "two empty
  lines" effect together with F9.
- **F11 (tracking in the wrong place)**: the Start/Stop `IconButton`s
  render inside the metadata row's State column
  (`task-view.slint:320-334`), far from the title a user is acting on.
- **F15 (no done checkbox)**: the view has none; the card precedent
  is complete — a stateless `done-box` rendering `model.done` only
  (`task-card.slint:123-157`) and the adapter's raw-send toggle
  `send_toggled_done` (`adapter.rs:4784-4795`), which reads the task's
  done flag and dispatches the exact inverted `SetTaskDone`.
- **F16 (no total)**: nothing sums a task's `TimeEntry`s. The engine
  already carries everything (`TrackingState.entries`, tracking.rs
  61-66); the detail verb projection
  (`detail_tracking_verb`, `presentation.rs:682-690`) is the pattern
  a total helper sits beside.
- **Tick seam (decision 5)**: the repaint driver is a 100 ms repeating
  slint `Timer` (`REPAINT_CADENCE_MS`, `adapter.rs:48`) that drains
  receivers, detects the wall-minute boundary into a `Cell`
  (`lib.rs:479, 534-540`), and refreshes — forced on the minute (the
  wall clock moved, the state pointer did not), guarded otherwise
  (`refresh` vs `refresh_forced`, `adapter.rs:634-646`; the #463
  invariant at `adapter.rs:620-633`: anything the derive reads
  outside the state Arc must arrive via `refresh_forced`). The alarm
  pass rides `(dirty || minute_boundary)` (`lib.rs:560-564`).
  Kiosk pins the view to Board (`adapter.rs:2992-2995`) — the task
  view never mounts there.

## 1. D16a — one header row (F9/F10 row shape)

**Composition** (replaces `task-view.slint:155-205` + the `:398`
spacer): one `HorizontalLayout`, `spacing: Theme.space-3`, content
height (no vertical-stretch):

```
[back IconButton][done checkbox (§2)][title element, horizontal-stretch: 1][tracking IconButton (§3)]
```

- **Element order**: back, checkbox, title, tracking — F9 (back
  inline), F15 (checkbox left of the title), F11 (tracking right).
- **Row height**: content-height. The tallest child is the back
  button's canonical `space-6` square (`icon-button.slint:57-59`); the
  title `RoleText` (Typography.title) and the edit-open `TextField`
  (single-line `natural-height`, `text-field.slint:90-91`) are both
  shorter. No stretch anywhere in the row → F9/F10's phantom height is
  gone by construction.
- **Title cell**: the closed `title-display` Rectangle keeps
  `horizontal-stretch: 1` (it already has it, `task-view.slint:183`);
  the edit-open `title-field` TextField — today a direct
  `VerticalLayout` child that fills width naturally — **gains
  `horizontal-stretch: 1`** for its new life inside the row. Both
  branches mount in the same cell; `edit-open` switches between them.
- **Checkbox and tracking button stay mounted while the title flow is
  open** — doneness and tracking are independent of the title buffer,
  and unmounting them mid-edit would bounce the walk. The row shows
  `[back][checkbox][field][tracking]` while editing.
- **Vertical centring**: the card row is the living proof of the
  pinned runtime's cross-axis behaviour for a fixed-height child in a
  `HorizontalLayout` (`task-card.slint:128-131` — the card checkbox
  renders centred in the goldens); the same construction is copied, so
  the same rendering follows. No bespoke `y` math in the row.
- **Mount conditions unchanged**: `edit-open` for the field,
  `can-track-start`/`can-track-stop` for the tracking button (exactly
  one while a subject exists, both false without — the model already
  carries this, `models.rs:1218-1219`).

**The chunk-6 seam contract (what the closed title element *is`).**
Chunk 6 (D18) makes the closed title a walk stop. To make that a pure
insertion, chunk 5 commits to this shape:

- `title-display` stays a plain named `Rectangle` with the exact child
  order `[hover Rectangle][RoleText][title-touch := TouchArea]` and
  the static `accessible-role/label/description/action` quartet on the
  Rectangle (today's shape, `task-view.slint:181-205`).
- Chunk 6 inserts a `FocusScope` overlay as the Rectangle's first
  child (100%×100%, `focus-on-tab-navigation: true` — the
  `icon-button.slint:63-84` focus-surface pattern) and *moves* the
  accessibility quartet onto it, adding the raw-key half.
- Chunk 5's obligations so nothing reworks: keep `title-touch` named,
  keep the accessible strings static (no geometry-dependent
  expressions), keep the hover surface keyed on `title-touch.has-hover`,
  add no key handling on the TouchArea. Recorded here so stage 2 of
  chunk 6 can cite this file.

## 2. D16b — the done checkbox (F15)

**Anatomy — the card checkbox's pattern, view grammar**: copy
`task-card.slint:128-157` verbatim for the visual core: `space-4`
square, `radius-small`, `focus-ring-width` border, border colour
`done ? Semantic.success : Theme.text-secondary`, the check `Path` in
`Semantic.success` under `if (done)`, `AccessibleRole.checkbox` with
the verb-pair label ("Mark task done" / "Mark task not done" — the
card's presentation literals, reused verbatim so the a11y tree and
`click_by_label` agree across surfaces).

Three view-grammar additions (the closed cells' interactive grammar +
the blanket rule):

1. **FocusScope overlay** (blanket rule b — the walk stop ships NOW,
   not in chunk 6): the `icon-button.slint:63-84` pattern — FocusScope
   child, `focus-on-tab-navigation: true`, `AccessibleRole.checkbox`
   + verb-pair label + `accessible-action-default => done-toggled()`,
   the Return/Space `key-pressed` half, and the focus ring carried by
   the box's `border-width`/`border-color` on `focus-surface.has-focus`.
   Child order `[focus-surface][check Path][done-touch]` — the
   TouchArea last so pointer delivery hits it first (the icon-button
   idiom).
2. **Tooltip arm** (blanket rule a): the TouchArea carries the
   `TooltipState.hover/unhover` `changed has-hover` pair
   (`icon-button.slint:317-323` idiom) with the verb-pair label, a
   description, and the pushed hint-keys (below).
3. **Hover surface**: `surface-hover` background while hovered (the
   icon-button's hover grammar; the card has none because cards sit on
   a busy surface — the view's closed cells all carry one).

**Dispatch — raw-send over the subject, NOT the registry route.**
`ActionId::TaskComplete`/`TaskReopen` act on `ctx.focused_task`
(`router.rs:376-379, 541-546`), but the view's subject is not
guaranteed focused: the cycle's fallback subjects
`first_task_of(subject_stack)` when focus is empty
(`view_state.rs:549-556`). The checkbox addresses the **subject** —
exactly the card-checkbox precedent's recorded reason ("the registry's
complete/reopen act on the focused task; the checkbox addresses an
arbitrary card", `adapter.rs:4059-4065`). So:

- new window callback `task-done-toggled()` (no args — there is
  exactly one subject);
- new adapter seam: resolve `view_state.subject_task()` (a stale/dead
  subject degrades to a no-op, the index-seam precedent) →
  `send_toggled_done(&a, task)` (`adapter.rs:4784-4795`, verbatim);
- **no local checked state anywhere**: the refresh push is the single
  writer of `done` (`TaskDetailModel.done` from
  `subject.is_some_and(|t| t.done.is_some())`, `models.rs:1217`) — the
  box renders the model only (the card's desync-proofing comment,
  `task-card.slint:123-127`).
- **No `settle_pointer_conflict`**: the board checkbox settles because
  the board owns the capture-settle lane; the task view's other clicks
  (title/due/description) don't settle and neither does this one.

No model change is needed for the checkbox itself (`done` is already
pushed).

**Hint keys**: `push_chrome`'s projection block
(`models.rs:782-851` — `first_binding_display(action, chain)` over the
live chain, the `nav_back` precedent at `models.rs:801-807`) gains a
`task-done-hint` cell → new `AppWindow` property
`task-done-hint: [string]`, pushed as `TaskComplete`'s binding while
the subject is open and `TaskReopen`'s while done (`vs` + `state` are
both in scope in `push_chrome`).

## 3. D16c — the tracking button moves (F11)

- The two `if (can-track-start/stop)` IconButtons leave the State
  column (`task-view.slint:320-334` deleted there); the State column
  keeps only the Completed/Open `RoleText`.
- They join the header row's right end with the **same** `IconKind`
  (`play`/`stop`), `Tone.info`, conditions, and callbacks
  (`task-track-start-clicked`/`task-track-stop-clicked` → the window's
  pure callbacks `app-window.slint:839-840` → `install_tracking_seams`,
  `adapter.rs:4905-4932`, untouched). The move is markup-only; the
  route seams follow the button because they were never attached to
  its position.
- **Blanket-rule additions while touching it**: `description` and
  `hint-keys` on both buttons (today label-only). New
  `PushModels`/`HintModels` cell + `AppWindow` property
  `task-track-hint: [string]`, pushed in `push_chrome`'s block as
  `TaskTrackStart`'s binding while `can-track-start`,
  `TaskTrackStop`'s while `can-track-stop`. (Hover is IconButton
  built-in.)
- **Comment updates that are part of the change**: the model doc
  comment ("the button beside the State cell",
  `task-view.slint:24-26`); the fence comment ("no elapsed text, no
  history (fenced #491)", `task-view.slint:320-322`) — reworded: the
  card badge stays fenced (#489) and the entries list stays chunk 10,
  but the task view now carries D17's summary row (below), so the
  blanket "no elapsed text" wording no longer describes this file.

## 4. D16d — the description absorbs leftover height (F10)

- **The spacer dies** (`task-view.slint:398`).
- The description section wrapper (`task-view.slint:357-396`) gains
  `vertical-stretch: 1` in the view's main column; **both** branches
  gain `vertical-stretch: 1` inside the section (the closed
  `description-display` Rectangle — which keeps its
  `horizontal-stretch: 1` — and the edit-open multiline TextField).
  One flexible region regardless of branch; the closed display's hover
  surface covers the whole region it grows into.
- **min/max lines**: the multiline floor stays exactly as-is — the
  three-line `min-height` (`text-field.slint:108-110`). No max is
  added: the growth source is the layout stretch, not the content.
- **Content-growth stays fenced**: the recorded layout-cycle trap
  stands (`text-field.slint:103-107` — binding `input.preferred-height`
  is a width-dependent cycle on the pinned 1.18.1, deprecated and
  panic-prone). This chunk changes **nothing** in
  `ui/widgets/text-field.slint`; typing beyond the visible window
  scrolls inside the TextInput (already the mechanism in today's
  three-line window). Tier-4 item: caret visibility with many lines.
- **Commit seams unchanged**: `accepted`/`edited`/`focus-lost` →
  `description-committed`/`description-edited`/
  `text-flow-focus-lost` — the round-3 D15/L1 rules ride along
  untouched; Return commits, Escape discards via the action layer,
  focus loss settles.
- **Every other row stays content-height**: header row (§1), metadata
  row, RuleList. The description is *the* flexible element — F10's
  "any element that eats leftover vertical space must be the editable
  description field".
- **Anchor side effect, noted not changed**: the date-picker overlay
  anchors read `due-column`/`scheduled-column` absolute positions
  through `changed` handlers (`task-view.slint:101-120`); the metadata
  row's absolute y moves up when the header shrinks. The handlers
  re-mirror after layout (their recorded mechanism) — no change, but
  the Tier-4 checklist re-verifies the anchored picker after the
  re-layout.

## 5. D17a — the tracked-time summary row (F16)

**Placement — its own row, directly under the metadata row, before
the RuleList.** Reasons: the metadata row already carries four columns
and crowds at min viewport; chunk 10's D35 mounts a *button beside*
this row (it needs horizontal room of its own); a full-width row under
the facts reads as the section total. Composition, content-height:

```
[Tracked (caption role, text-secondary)][total (mono role — the dates' role)]
```

**The pure helper** (`presentation.rs`, beside
`detail_tracking_verb`):

- `pub struct TrackedTotal { hours: u32, minutes: u32, seconds: u32 }`
  — the numeric reduction *is* the second-precision contract.
- `pub fn tracked_total(state: &AppState, subject: TaskId,
  now: DateTime<Utc>) -> TrackedTotal` — Σ over
  `state.tracking.entries.values()` where `entry.task == subject` of
  `entry.duration(now)` (`tracking.rs:117-124` — clamps inverted
  spans at zero, and an **open** entry's `end.unwrap_or(now)` carries
  the running elapsed, D17's "incl. a running entry's elapsed"). No
  UI types; pure; `now` is data.
- `pub fn tracked_total_phrase(&TrackedTotal) -> String` — the glue
  ("1h 23m 45s" shape). Segment rule: zero-leading segments drop
  (hours, then minutes); seconds always render; all-zero renders the
  canonical zero. Deterministic, diff-stable within a second.

**Model + push**: `TaskDetailModel` gains `tracked-total: string`,
pushed in `push_task_detail` (`models.rs:1204-1256`) — the phrase when
a subject exists (honest zero with no entries), empty string without.
Every construction site of the struct gains the field (codegen forces
it; the journeys' `get_task_model()` reads stay source-compatible).

**Tests — structural only** (the dispatch prompt's term; recorded
divergence: exact-string pins exist for the sibling
`countdown_phrase`, `presentation.rs:1598-1611`, but this helper's
precision contract is carried by the *numeric* `TrackedTotal` parts,
so the phrase needs no text pins): parts pinned exactly (§7 P1-P6);
the phrase pinned as non-empty, deterministic, and
zero-canonical-distinct.

## 6. D17b / §2 decision 5 — the per-second tick seam

**The flag.** `Adapter::seconds_surfaces_active: bool`
(crate-private), computed adapter-side — *not* inside the push passes
(`models::push` is a free function with no adapter access; one
computation site keeps the OR protocol honest):

```rust
// lib of the flag's comment: chunks 8/9 (countdown rows) extend THIS
// function's term — never introduce a second tick channel.
fn compute_seconds_surfaces_active(&mut self, state: &AppState) {
    let mut active = false;
    // D17 summary row (chunk 5): the mounted task view's subject has
    // any open entry — only then does the displayed total change per
    // second. Kiosk pins today's no-tick behaviour (the
    // timers-visible gate precedent, models.rs:482).
    if !self.kiosk
        && self.view_state.view() == ViewId::TaskDetail
        && let Some(subject) = self.view_state.subject_task()
        && state.tracking.open_entries().any(|e| e.task == subject)
    {
        active = true;
    }
    self.seconds_surfaces_active = active;
}
```

called in `refresh_forced` after `reconcile` (so the reconciled
view-state is what the term reads).

- **Which mounted surfaces count**: this term is the *display-changes-
  per-second* condition, deliberately tighter than the design text's
  "while an entry is open": a foreign task's open entry does not
  change this row, and the verb button may show Start during the
  transient double-open while the subject's older open entry still
  accrues (the D-T8 single-open *display* pick vs the arithmetic
  truth — the tick follows the arithmetic, the button follows D-T8;
  recorded).
- **Kiosk exclusion, three layers**: (a) structural — kiosk pins the
  view to Board (`adapter.rs:2992-2995`), the task view never mounts;
  (b) the explicit `!self.kiosk` term — freezes the decision even if
  kiosk view gating ever changes (the `models.rs:482` precedent of
  gating at the model); (c) the driver only consults the wall second
  while the flag holds. Layer (b) is the test-pinned one.

**The driver** (`lib.rs:470-567`):

- One clock read per tick (today's single `chrono::Utc::now()` at
  `lib.rs:535` is reused for both boundaries): `wall_minute` as today,
  plus `wall_second = now.timestamp()`.
- The second-boundary detector is an extracted, unit-testable cell —
  no new `slint::Timer`, no UI-held timer state (decision 5's fence):

  ```rust
  pub(crate) struct SecondTick(Cell<Option<i64>>);
  impl SecondTick {
      /// True exactly on the tick where the wall second changed.
      /// The cell tracks the wall second even while inactive, so the
      /// first active second after re-activation fires immediately
      /// (no stale-second skip).
      fn crossed(&self, active: bool, wall_second: i64) -> bool {
          let changed = self.0.get() != Some(wall_second);
          if changed { self.0.set(Some(wall_second)); }
          active && changed
      }
  }
  ```

- **Dirty classification — the exact restructure.** Today one `dirty`
  bool mixes receiver drains + delivery health + the minute term and
  feeds both the refresh branch and the alarm pass
  (`lib.rs:545-564`). Split it:

  - `signal_dirty` — receiver drains + delivery health (today's
    `dirty` *minus* the minute term);
  - `time_boundary = minute_boundary || second_boundary`;
  - refresh: `if signal_dirty || time_boundary { if time_boundary {
    refresh_forced } else { refresh } }` — preserves today's three
    cases exactly (minute alone → forced; signals alone → guarded;
    both → forced) and adds the fourth (second alone → forced);
  - alarm pass: `(signal_dirty || minute_boundary)` — **semantics
    byte-preserved**; the second boundary must never run the alarm
    pass at 1 Hz (its cadence is minute-level + signals). This
    exclusion is the restructure's reason for existing and is pinned
    by review + the SecondTick unit boundary.

- **Why `refresh_forced` and not a targeted push** (recorded
  alternative): the #463 invariant (`adapter.rs:620-633`) — the total
  derives from `(state, now)` and `now` is outside the state Arc, so
  the derive must be forced; and chunks 8/9's countdown phrases live
  in row models that only a derive re-pushes. One mechanism (the
  design's own words: "a refresh is also due"), not two write paths
  (the `push_notice` targeted-push shape was considered and rejected).

**Cost (R7)**: while the flag holds, the delta over today's idle
cadence is one wall-clock compare per 100 ms and one full derive per
second. The derive is the same pass every interaction already runs;
the Slint-side diff per tick is the model's row diff — for the summary
row that is 1-2 property mutations (`diff_rows` no-ops the rest). On
kiosk the flag is structurally + explicitly false: the per-tick delta
is one bool read — today's behaviour, pinned. The chunk report carries
this reasoning (and a measured number if the netbook-class reference
is available; the reasoning stands alone if not).

**Tick quality**: cadence stays 100 ms (`adapter.rs:48`) — a boundary
fires within ≤100 ms of the wall crossing, a visually smooth 1 Hz.
Flag staleness bounds: activation lands ≤ one cadence tick after the
opening refresh; deactivation lands with the closing effect's
synchronous refresh. No sleep-based anything; the tests inject the
wall second (§7 A6).

**Untouched by fence**: `open_elapsed_phrase` (status chip + card
badge) stays minute-bucketed; #489's card fence holds; the timer chip
is chunk 9's.

## 7. Test catalogue (TDD; red where the behaviour is new)

**Presentation unit** (`presentation.rs` tests mod; the fixture
idiom of `countdown_phrase_buckets_the_remaining_time_and_clamps`,
`presentation.rs:1598-1611`):

- **P1** `tracked_total` sums only the subject's entries (two tasks
  with entries; the other task's excluded).
- **P2** an open entry's elapsed-to-now is included (start T0,
  now T0+90 s → the 90 s lands in the parts).
- **P3** an inverted/degenerate span contributes zero — the total is
  never negative (`duration`'s clamp, pinned through the helper).
- **P4** no entries → the zero `TrackedTotal`.
- **P5** reduction + precision: 3661 s → (1,1,1); 59 s → (0,0,59);
  90 s → (0,1,30) — seconds survive minute rounding (the
  second-precision contract, numerically exact).
- **P6** phrase glue: deterministic; non-empty for every part triple;
  the zero rendering is distinct from the 1 s rendering; segment
  count grows as parts enter (structural, no text pins).

**Adapter/rig** (`flow_table_tests.rs` rig — the `derive_passes` seam,
`:1064+`):

- **A1** flag term: task view mounted + subject holds an open entry →
  `seconds_surfaces_active` true; open entry on another task → false;
  no open entry → false; view left (Back) → false.
- **A2** kiosk pin: `kiosk = true` with an open entry and a
  subject-candidate → the flag stays false (layer (b) of §6).
- **A3** checkbox dispatch: view checkbox click → exactly
  `SetTaskDone { id: subject, done: true }`; on a done subject →
  `done: false`; and a **subject-addressing pin** under the D-TU5
  fallback (subject = the stack's first task, focus elsewhere) the
  command still names the subject — the raw-send rationale made
  red-first.
- **A4** tracking move: header Start click → `StartTracking
  { task: subject }`; Stop → `StopTracking` (the old buttons' exact
  commands; the status-bar chip and notification-surface routes stay
  green via their existing journeys).
- **A5** summary push: with an open entry (the `add_open_entry`
  fixture, `tests/support/fixture.rs:145`), `get_task_model()`
  carries a non-empty `tracked_total`; after the entry closes
  (fixture mutation + refresh) the value is refresh-static (a later
  `now` changes nothing — the closed-entries sum).
- **A6** tick seam, paused-clock shape: the `SecondTick` unit driven
  with synthetic wall seconds — a 100 ms poll sequence fires exactly
  once per second, never while inactive, and immediately on the first
  active second after re-activation; plus the classification pins:
  the second boundary routes `refresh_forced` (derive_passes bumps
  through the rig) and the alarm-pass trigger excludes it (the
  restructure's pin). *Acceptance-mapping note*: dispatch acceptance
  (c)'s "ticks on the paused clock when open" is delivered by A6's
  injected-wall-second unit + A1's flag term + A5's value correctness;
  the live visual tick is the Tier-4 item (the UI driver reads the
  real wall clock; there is no pauseable clock below the engine).
- **A7** guard non-regression: a second-boundary forced pass closes
  the `last_derived` cache exactly like a minute pass (existing #463
  pins stay green; `refresh()` with an unchanged Arc still skips).

**Journeys** (`tests/journeys.rs`):

- **J1** the existing back-button journey (`journeys.rs:1216`) and
  the description-edit journey (`:6446`) stay green — the moved
  elements did not break their seams.
- **J2** checkbox click ≡ engine: covered by A3 at the harness level;
  the key path needs no new code (the registry verbs are untouched).
- **J3** hover facts: hovering the checkbox, the tracking button, and
  the summary value arms the tooltip with a populated description
  (the tooltip journey pattern, `journeys.rs:1485-1516`); label text
  never asserted.
- **J4** hint projections: with bound keys, `task-done-hint` and
  `task-track-hint` are non-empty; without, empty (the `nav-back`
  projection's test shape).

**Goldens** (`tests/golden_visuals.rs`):

- **G1** NEW task-view goldens: scale 0.5/1.5/3.0 × two window heights
  (short/tall) — pins one header row at content height, the
  description region absorbing the tall window's leftover, the
  summary row present. Capture follows `capture_today`
  (`golden_visuals.rs:190`) — mount, walk into the task view (the
  `NavFirstItem` + `TaskEdit` idiom of `:8225`). Regenerated in the
  same commit as the last visual change.
- **G2** board/today/kiosk goldens untouched — no card, column, or
  kiosk-surface change (their matrices must pass unchanged).

**Structural**:

- **S1** no spacer residue: the view's only `vertical-stretch: 1`
  elements after this chunk live in the description section (review
  item + golden evidence).
- **S2** the flag's extension fence: the `compute_seconds_surfaces_active`
  comment names itself the single tick channel for chunks 8/9
  (review item).

## 8. Commit slices (stage 2; standalone, rebase-clean)

1. `test(presentation): tracked total helper` — P1-P6 red→green
   (helper lands with its tests; nothing consumes it yet).
2. `feat(ui-slint): task-detail tracked-time summary row` — model
   field, push, row markup, hover tip (A1 false-arm, A5).
3. `feat(ui-slint): per-second tick seam` — flag + `refresh_forced`
   call, `SecondTick`, driver restructure, alarm-pass pin (A2, A6,
   A7).
4. `feat(ui-slint): one task-view header row` — back inline, checkbox
   (FocusScope + tooltip + raw-send seam), tracking move, hint
   projections, comment/fence updates (A3, A4, J3, J4).
5. `feat(ui-slint): description absorbs leftover height` — stretch
   terms, spacer removal, **G1 goldens regenerated in this commit**
   (the branch's full visual delta; the tip is what CI and review
   see), CHANGELOG Unreleased entry.

## 9. Risks & recorded probes

- **R-A (title field in the row)**: the field's mount-time focus grab
  (`autofocus`, `text-field.slint:134-143`) is unchanged — it mounts
  focused+selected inside the row exactly as it did below it. The row
  grows to the field's single-line height while editing (Tier-4
  confirms no jump).
- **R-B (1 Hz derive cost)**: reasoned in §6 (kiosk zero; desktop =
  one interaction-class pass per second with a ~1-mutation diff).
  Measure on the reference machine if available; the reasoning is the
  recorded fallback per R7.
- **R-C (struct re-set per tick)**: `refresh_forced` re-sets the whole
  `TaskDetailModel` every second while ticking. This already happens
  on every refresh today — including while the title/description
  fields are open — and the fields are unaffected (the D9 rule: the
  buffer is user-owned; `initial-text` is read at mount only). No new
  risk; equal-value re-evaluations don't repaint.
- **R-D (walk-order change)**: the checkbox's FocusScope is an
  additive tab stop; every existing walk journey must stay green — a
  regression there is a finding, never a weakened test.
- **R-E (rebase friction)**: chunks 1-4 may land first —
  `app-window.slint` (chunk 1 dismissal layers, chunk 4 tooltip
  dismissals) and `icon-button.slint` (chunk 3's Icon migration —
  this chunk's call sites keep the `IconKind` API, zero conflict by
  design) share files; `task-view.slint` itself is untouched by 1-4.
  Standard rebase dance at stage 3.
- **R-F (the flag's future contributors)**: chunks 8/9 extend
  `compute_seconds_surfaces_active`'s term; the fence comment (S2)
  and the single-site rule are the guard against a second tick
  channel appearing.
- **R-G (new golden capture)**: the task-view capture is the first of
  its kind in the matrix; if the headless renderer's text metrics
  make the title row brittle across environments, the golden pins
  geometry at the fixture's own font (the board goldens' precedent) —
  a flaky golden is triaged per AGENTS §7 (flaky-test issue), never
  deleted silently.

## 10. Tier-4 checklist (manual, on the reference machine)

- Header reads as **one row** at scale 0.5, 1.0 (implicit), 1.5, 3.0
  — no phantom lines, no two-row back+title stack at any scale.
- Two window heights: tall window → the description region visibly
  absorbs the leftover; short window → the region floors at its
  three-line editor / content display and the view scrolls per the
  page's own rules.
- Tick smoothness: start tracking with the summary mounted, watch 30 s
  — 1 Hz steps, no flicker, no relayout of neighbouring rows, the
  metadata row and rule list perfectly still.
- Title field: opens focused+selected inside the row; Return commits;
  Escape discards; focus loss settles (round-3 rules intact).
- Checkbox: hover surface + tooltip with hint keys where bound;
  Tab reaches it and the ring shows; Return/Space toggle; click
  toggles; done visual parity with the cards (success border + check).
- Tracking button: Start/Stop at the row's right; tooltip; the
  status-bar chip and notification Start button still work (routes
  untouched).
- Date-picker overlay: anchored correctly off the due/start cells
  after the header shrink moved the metadata row up (§4's anchor
  note).
- Summary row: "0s" on a never-tracked task; grows while tracking;
  freezes on stop; second precision visible on a sub-minute entry.

## 11. Bookkeeping

- **Closes nothing**; #489/#210 untouched (the card fence holds);
  #491's task-view history surface is chunk 10's (this chunk lays
  only the summary row it will mount beside).
- **CHANGELOG**: Unreleased → Fixed/Changed entries for the header
  row, checkbox, description stretch, summary row, and the ticking
  behaviour (user-visible).
- **Estimate**: ~500-600 changed lines incl. tests (task-view.slint
  ~±70, models.rs ~+40, presentation.rs ~+90 with tests, adapter.rs
  ~+30, lib.rs ~+30, app-window.slint ~+10, journeys/rig ~+180,
  golden_visuals ~+70) plus 6 new golden PNGs.

# [Plan] ListView row-anchored scroll anchoring (#592) — chunk 8 of the v0.2 dispatch (stage 1, measure–decide–implement gate)

- **Date**: 2026-11-05
- **Kind**: stage-1 plan for chunk 8 of the v0.2 dispatch. This chunk
  is a **measure-first gate**: the deliverable of stage 2 is the
  measurement and the criterion verdict — the optional row-anchored
  (visual) anchoring mode is implemented **only if** the measurement
  passes the criterion below, and a recorded **decline** (numbers +
  reasoning as the report, #592 annotated, no production diff) is a
  fully valid outcome. The criterion is stated here **before** the
  measurement runs (§3); the measurement design (§4) and the
  implementation sketch that applies on a pass (§5) follow it.
- **Origin**: dispatch plan §9
  (`.artifacts/plans/2026-11-04-v0.2-focus-reflection-dispatch-plan.md`,
  the §9 acceptance is binding: implemented ⇒ pins extended to
  pixel-still, no perf regression in the refresh/draw benches, goldens
  updated as data; declined ⇒ measurement report + issue annotation,
  gates green either way); impl-plan §5.8
  (`.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md`);
  GitHub issue #592 (its pins and the stage-3 accepted-contract
  decision); stage-3 report §3
  (`.artifacts/reports/2026-11-03-today-virtualisation-stage3-report.md`);
  design round §3c
  (`.artifacts/reports/2026-10-26-listview-virtualisation-design-round-report.md`).
- **Normative base**: `origin/main` = `947b9f76`; branch
  `perf/listview-row-anchor`, worktree
  `../taskboard-WT-perf-listview-row-anchor`, created with `-b` off
  the fetched `origin/main` (local `main` was behind it:
  `58ae9c55` vs `947b9f76`). Doctor: exit 0 — 219 GiB free against
  the 25 GiB floor. File:line anchors are that tree's; slint sources
  are the workspace's exact pin `i-slint-core-1.18.1`.

## 0. The mechanism correction (read this first, stage 2)

Issue #592 and the stage-3 report attribute the pinned
content-anchored reflow to "the slint 1.18.1 virtual repeater is
content-anchored, not row-anchored". The observable is right; the
attribution refines, and the refinement decides the whole measurement
design:

**The repeater's offset-shift machinery never fires for Today
membership changes.** Section rows reach the repeater through
`diff_rows` (`models.rs:368-399`), which diffs the desired rows
against a mirror and expresses *any* membership change as a
**`set_row_data` rebind cascade plus one tail mutation**:

- head removal (desired is shorter by one, shifted): `common =
  min(N, N−1)` ⇒ every compared row differs ⇒ **N−1 ×
  `set_row_data(i, …)`** then **one `remove(N−1)`** at the tail;
- head insertion: **N × `set_row_data`** then **one `push`** at the
  tail;
- any mid-list change: the same cascade from the change point down,
  tail add/remove.

`VecModel` notifies per mutation (`set_row_data` ⇒ `row_changed`,
`push`/`remove` ⇒ `row_added`/`row_removed` at that index), so every
structural notification this push discipline ever emits is
**tail-indexed**. The repeater's "entirely before the visible range:
shift the offset" branches (`repeater.rs:549-578` row_added,
`:580-610` row_removed) never run. What the pins actually observe:

1. every live slot re-binds in place to its successor's data
   (`row_changed` updates live instances immediately,
   `repeater.rs:533-547`; off-viewport rows are no-ops), and
2. `content-y` is untouched by the diff, so the next layout pass
   re-solves instance positions from the re-bound heights under a
   fixed content offset.

That is the content-anchored contract in full: the visible content
shifts by the changed row's extent because every live instance now
*draws different data* — nothing scrolled. It also explains the
insertion pin's odd doc detail, "it even prepends the inserted row
into the live window, clipped" (`virtualisation.rs:695`): slot 0's
new data *is* the inserted task.

**Consequences for this chunk:**

- **C-1**: The compensation does **not** fight the repeater's
  offset-shift branches (dormant here). It rides or fights the
  *scroll* machinery: the runtime rewrites `content-y` at the end of
  every ListView layout pass (`repeater.rs:428-439`,
  `content-y := −anchor_y + new_offset_y`, with
  `anchor_y = cached_item_height × offset`) and re-derives its window
  from content-y's direction against `previous_content_y`
  (`:335-352`). A compensation is a **synthetic scroll**: write
  content-y by the compensation delta and the next pass moves the
  window through the same scroll-up path a user flick takes (the
  prepend walk over actual heights, `:354-372`). Whether a markup-
  level write survives the runtime's own rewrite in and across passes
  is exactly measurement B — not decidable from the armchair, because
  slint's `changed`-callback dispatch order against the runtime's
  `content_height_set` (`:426`) → `content_y_set` (`:438`) sequence
  inside one pass is not pinned by any recorded evidence.
- **C-2**: The "extra extent measurement per off-viewport change"
  the issue worries about is not merely costly — it is
  **unobservable**. A row above the viewport is never instantiated;
  its extent exists nowhere in the UI. The sanctioned estimators are
  (a) the runtime's own average (`content-height / total-rows`, the
  same divisor the seek uses) and (b) a height cache of extents
  observed while a row was live (exact for ever-live rows, average
  fallback otherwise). Instantiating off-window rows to measure would
  re-arm the eager shape the engagement probes fail on purpose
  (`virtualisation.rs:79-105`) — excluded by D-2 below.
- **C-3**: There is an in-repo precedent for the write seam: the
  settings surface's scale-commit anchor (round-3 D20c) captures a
  viewport offset adapter-side and a `changed content-height` hook
  restores it with a **direct content-y write and a one-shot consume
  flag** (`settings-view.slint:628-655`, `adapter.rs:3337-3347`,
  `settings-facts.slint:7-18`) — "measured, not predicted … one
  commit, one consume — manual scrolling afterwards is never fought".
  That Flickable is eager, so the open question this chunk measures
  is precisely whether the D20c seam survives the *virtual* repeater's
  own content-y rewrite.
- **C-4**: The value being bought is bounded and real: only
  **above-viewport** changes reflow (background inserts/removals — a
  Nextcloud pull, a capture landing at the Overdue head, reminder
  drift re-sectioning) while the user is deep-scrolled. The common
  click-flip is within-viewport and already keeps the user's place
  exactly (pinned, `virtualisation.rs:893`). A decline costs only
  that polish; this is a p3 issue and the impl-plan's own bar is
  "drop without ceremony if the measurement says no" (§5.8).

## 1. Mission & binding constraints

Quantify, on the landed #373/#461 machinery, (i) what an off-viewport
membership change costs to compensate and what compensation can
achieve (extent observability, estimator accuracy), (ii) whether a
UI-side compensation write controls the viewport against the
repeater's own per-pass content-y rewrite, and (iii) what the
compensation would move in the refresh/draw benches — then apply the
pre-declared criterion (§3): implement the opt-in row-anchored mode on
the Today sections, or record the decline with the numbers.

Binding, verbatim force:

1. **Dispatch §9 non-goals**: the within-viewport semantics (already
   pinned correct, `virtualisation.rs:893`) are not changed; no other
   #373 consumer (board columns, compact leg, stack detail) changes
   behaviour — the mode is opt-in per call site and defaults off.
2. **Dispatch §9 acceptance**: if implemented — the three existing
   pins **extended** to pixel-still behaviour under the mode, never
   weakened with the mode off (the content-anchored pins stay
   byte-identical and green in the default shape); no perf regression
   in the refresh/draw benches; goldens updated as data. If declined
   — the measurement report with numbers, #592 annotation **drafted**
   (not silently closed; the close-or-keep decision is the user's),
   no production diff.
3. **AGENTS §5 across every measurement/impl test**: no sleeps
   (deterministic clock via `advance_probe_clock` /
   `driver.advance_time(0)`, the pins' idiom), no text assertions,
   no quality ceilings (the G-thresholds of §3 are decision criteria
   applied by the agent to recorded numbers, not test assertions —
   test assertions stay structural or exact), one behavior one test
   at the lowest layer, deterministic by construction.
4. **The sanctioned surface**: compensation code lives in
   `taskboard-ui-slint` (markup + adapter/presentation Rust). No
   crate-boundary change, no `forbid(unsafe_code)` status change, no
   private slint API (the D20c seam is public markup/property
   surface; models.rs gains only pure summary computation).
5. Stage 2 executes the measurement **before** touching anything
   production-shaped; the verdict is recorded in the stage-2 report
   before any implementation commit exists. The branch's diff on a
   decline outcome is: tests documenting the mechanism fact (§4 M-A)
   + docs (report, annotation draft, this plan's deviation record if
   any) — nothing else.

## 2. Verified baseline — numbered facts

All verified in this session on the branch base (`origin/main` =
`947b9f76`; markup/tests/bench files are byte-identical between it
and `58ae9c55` — only `models.rs`/`adapter.rs` grew, anchors below
re-verified on the branch base).

- **F1 — the in-house ListView**
  (`crates/taskboard-ui-slint/ui/widgets/listview.slint:7-67`):
  `ListView inherits Flickable` carrying the compiler's nominal
  five-property contract (the element name + `content-y/height/width`,
  `visible-height/width`; a rename silently renders eager — the
  engagement probes fail on purpose), the focus seek
  (`request-row/-nonce/-total-rows`, `scroll-row-into-view` top-anchors
  on `content-height / total-rows`, `:53-66`), and the paging seam
  (`page-request/-nonce`, `page`, `:38-51`). No `for` inside —
  consumers mount their own delegates.
- **F2 — the runtime's virtual layout**
  (`i-slint-core-1.18.1/model/repeater.rs:268-446`,
  `update_visible_instances`): cached average height (`:296-317`);
  1.5-screen random seek guard (`:323-334`); scroll-direction
  branches off `previous_content_y` (`:335-352`); gap fill/prepend
  walking **actual** instance heights (`:354-372`); layout walk
  placing instances (`:375-398`); per-pass recompute
  `content-height := cached_item_height × row_count` and
  `anchor_y := cached_item_height × offset` (`:424-426`); the
  **content-y rewrite** `content-y := −anchor_y + new_offset_y`,
  skipped only when already equal (`:428-439`). Layout state
  (`offset`, `cached_item_height`, `previous_content_y`, `anchor_y`):
  `:135-148`.
- **F3 — the dormant offset shifts** (`repeater.rs:549-610`):
  `row_added`/`row_removed` entirely before the live range shift
  `offset` and dirty every instance; partially-overlapping changes
  splice the live window. **Never exercised by the app's push
  discipline** (F5): every structural notification `diff_rows` emits
  is tail-indexed.
- **F4 — the pinned contract** (`tests/virtualisation.rs`): cross-
  section rest (`:589`), insertion above reflows **down** uniformly,
  page rests (`:700`), true removal above reflows **up** by *exactly*
  the removed row's extent, page rests (`:805`), within-viewport
  removal closes the rows below under the cursor, rows above and the
  page rest (`:893`). All pinned-actual, fixture `build_state(600)`
  (~200 Overdue rows), 1000×900 window, 30-stroke walk (position 29
  = order 99; `bench_label(87)` = position 26, the witness row).
- **F5 — the push discipline** (`models.rs`): `diff_rows`
  (`:368-399` — rebind cascade + tail mutation, §0), the per-section
  persistent handles `today_sections: [MirroredRows<TodayRowModel>;
  4]` (`:280`), `push_today` (`:2239-2351`, section diff at `:2351`)
  — a `ModelRc` is never swapped, so the repeater never `reset()`s.
- **F6 — the Today mount** (`ui/views/today-view.slint`): per
  section, `row-viewport-clip := Rectangle` (clip host,
  `:213`) over `row-viewport := ListView` (`:226`) whose
  `visible-height` is the window-derived bound (`:228`); the clip
  height is **assigned** by `changed content-height` (`:229-234`) —
  declarative min() is a statically-rejected binding loop, the
  assignment escape is the landed shape — re-applied by
  `changed watch-viewport-height` (`:299-305`); seek/page bindings
  `:235-250`; `total-rows: section.rows.length` (`:251`); the
  delegate carries an explicit height
  (`row-card.preferred-height + context-line block`, `:252-263`).
- **F7 — the D20c precedent** (the write seam, eager surface):
  `settings-view.slint:613-662` — two-way `content-y <=>
  SettingsScrollFacts.viewport-y` with a write-back mirror, and the
  one-shot anchor consumed inside `changed content-height` via a
  **direct content-y write** clamped to `[min(0, height −
  content-height), 0]`; the adapter half captures the pre-change
  offset (`adapter.rs:3337-3347`, `request_scale_anchor`); facts
  global `settings-facts.slint:7-18`. Proves: markup-level
  `changed content-height` hooks writing content-y land and hold on
  an eager Flickable, and the pan/wheel paths sever bindings by
  writing directly (probe finding recorded in that markup).
- **F8 — the hint-global precedent** (nonce-gated, edge-triggered
  adapter→markup signals): `push_focus_scroll_hint`
  (`adapter.rs:2345`, nonce increment `:2384`; sibling hint writers
  at `:2133`/`:2201`) consumed by nonce change in the ListView
  (`listview.slint:15-19`) — the shape an anchor-facts signal must
  follow to avoid double-fires.
- **F9 — the bench** (`benches/refresh_draw.rs`): view pins
  `[("today", false), ("board", true)]` (`:122`); `refresh/{n}`
  measures the #463 guarded no-op path (`lib.rs:869` `refresh()`);
  `draw-full`/`draw-partial` per view pin over
  `N_SIZES = [100, 1_000, 10_000]` (`benches/support/mod.rs:20`);
  the partial lane's flip is the **removal class** on Today (fixed
  2024 stamp outside the done-today window, `PARTIAL_STAMP_SECS`
  `:127`) rotating the rendered head rows
  (`today_flip_targets`, `benches/support/mod.rs:198`), with the
  visible-flip preamble guard and the max/last region carrier
  (`:318-423`). Reference numbers post-#511: today 1.42/1.43/1.44 ms
  full, 4.81/4.87/5.75 ms partial at the three N (stage-2 report §4
  of `2026-10-26-listview-virtualisation-stage2-report.md`) — the
  comparison base for G-3.
- **F10 — the probes** (`tests/virtualisation_probe.slint`):
  `EagerProbe` (`:16`) and `ListViewProbe` (`:31`, 300 items in a
  400 px viewport) — the harness a scratch compensation probe mounts;
  `find_by_label`/`absolute_position` are the sanctioned instrument
  class (the pins' idiom).

## 3. The decision criterion (stated before measuring)

Verdict rule: **implement** iff G-0 ∧ G-1 ∧ G-2 ∧ G-3 all pass;
**decline** otherwise, with the failed gate's numbers as the decline's
evidence. The gates are applied by the agent to numbers recorded in
the stage-2 report; no test asserts a threshold from this section
(AGENTS §5's ceiling ban governs *tests*, and these are decision
criteria, not assertions).

- **G-0 — seam controllability** (the make-or-break): a UI-side
  compensation write must demonstrably move the live window at a
  stable frame, surviving the runtime's own content-y rewrite (F2
  `:428-439`) without fighting it. Pass: some sanctioned write seam
  (the D20c `changed content-height` direct write is the primary
  candidate; a nonce/flag-gated variant is the fallback) achieves,
  on the probe harness, a commanded witness-row displacement that
  (a) is present at the first stable frame after the write (within
  one `advance_time` tick), and (b) **settles** — after it lands, two
  further consecutive stable passes move the witness < 0.5 px
  (sub-pixel = no oscillation). Fail: every candidate seam's write is
  overwritten by the runtime within the same pass, or re-derives away
  across passes, or oscillates (the compensation and the rewrite
  chasing each other).
- **G-1 — compensation accuracy**: with the best passing seam and
  only the sanctioned estimators (C-2: the runtime average; a
  height cache of ever-live extents), across the campaign of §4 M-C
  (≥ 12 above-viewport changes: head insertion, head removal, second-
  row removal, the real done-flip pair (Overdue removal + Done-today
  insertion), each at two walk depths — the pins' 30-stroke setup and
  a 15-stroke setup), measured as witness-row residual shift after
  compensation vs the uncompensated shift: **median residual ≤ 20 %
  and worst-case residual ≤ 50 %** of the uncompensated shift.
  Rationale (pre-declared): fixture card extents run ~48–70 px
  (design round §3c), so the average estimator's expected error is
  ≲ 19 % of the true extent — an average-based compensation that
  cannot meet 20 % median means the error is structural, not
  statistical; 50 % worst-case is the line below which compensation
  is motion noise without benefit.
- **G-2 — within-viewport and cross-section safety**: the
  discriminator (which changes compensate, which don't) must leave
  the pinned semantics exact: with the mode **on**, a within-viewport
  removal still closes under the cursor (successor takes the removed
  row's old y exactly, rows above rest, page rests — the case-3
  assertions), and a Done-section insertion still rests the Overdue
  viewport (the cross-section pin's property). The discriminator is
  Rust-side (only `diff_rows`/`push_today` know the mutation index;
  markup cannot tell where a change landed): compensate only when the
  first changed index maps strictly above the window head estimated
  the runtime's own way (`floor(−content-y / avg)`), with a
  don't-compensate guard band at the boundary — an ambiguity
  falls back to today's behaviour (never worse). Pass: zero
  measurable disturbance on the safety cases. Fail: any within-
  viewport case moves.
- **G-3 — no regression**: `refresh/{n}`, `draw-full/today/{n}`,
  `draw-partial/today/{n}` at all three N, one run before the
  production diff and one after (two runs each side on a quiet box if
  the first pair disagrees beyond its own spread), taken per the
  house rule as dev-machine references: the paired after-deltas must
  sit inside the run-to-run spread of the runs themselves, and the
  partial lane's region carrier must stay in its uncompensated class
  (the removal-reflow viewport extent — a synthetic scroll may
  repaint the shifted band, which is inside today's region class;
  a systematic region *growth class* is a decline datum). Times are
  never gates; the criterion consumes only the noise band (AGENTS:
  references, never gates).

Decline conditions, for the record (any one triggers decline):
- **D-1** = G-0 fails: no sanctioned seam controls the viewport
  against the runtime's rewrite (the compensation is unimplementable
  without private slint API or off-window instantiation — both
  excluded by constraint 4 and D-2).
- **D-2** = exact extents would require instantiating off-viewport
  rows (defeats virtualisation; the engagement probes fail on
  purpose) — recorded as a permanent exclusion, not a measurement
  outcome; the estimators of C-2 are the only sanctioned extent
  sources.
- **D-3** = G-1 fails: the achievable accuracy leaves the rows
  visibly moving anyway (median > 20 % or worst > 50 %).
- **D-4** = G-2 or G-3 fails: the compensation breaks pinned
  semantics or regresses the benches.

## 4. The measurement design (stage 2 executes in this order)

Instruments: the pin harness (`mount_today`-shaped mount of
`build_state(600)`, 1000×900, `driver.advance_time(0)` ticks;
`find_by_label` + `absolute_position`; `ListViewProbe` for the
scratch seam work). Permanent tests are committed; exploratory probes
are scratch (run, transcript into the stage-2 report, code deleted —
the stage-3 live-geometry precedent).

- **M-A — the notification shape (permanent, lands regardless of the
  verdict).** Extend the `diff_rows` unit tests
  (`models.rs:2712`-area): the head-removal shape asserts `mutations
  == N` with `row_count` N→N−1 (⇒ N−1 `set_row_data` + one tail
  `remove`); the head-insertion shape asserts `mutations == N+1` with
  row_count N→N+1 (⇒ N `set_row_data` + one tail `push`). The
  notification sequence follows deterministically from `VecModel`'s
  notify-per-mutation semantics and is cited, not re-tested. This
  pins the §0 mechanism correction in code: the contract's *cause*
  becomes a recorded fact for #592 and any future revisit, whichever
  way the gate rules. Invert-check: a scratch `diff_rows` variant
  that removes at index 0 fails the count assertion (discriminates
  rebind-cascade from true removal). Commit:
  `test(ui-slint): the diff push's rebind-cascade shape`.
- **M-B — seam controllability (scratch probe → G-0).** On
  `ListViewProbe`: engage, seek deep, then (i) write content-y by a
  commanded delta from a `changed content-height` handler (the D20c
  shape, direct write, one-shot flag); (ii) the same write deferred
  via a flag consumed on the next `init`-side tick; (iii) the same
  write from a plain property write outside any handler (control).
  Trigger a real head-model change through a `VecModel` to fire the
  pass. Record per variant: does the commanded displacement appear at
  the first stable frame; the per-pass witness movement for the two
  following passes (settle); any oscillation over 10 ticks. Also
  record the *ordering fact*: whether the handler's write survives
  the same pass's `content-y` rewrite or is superseded by it (read
  the witness position after one vs two ticks). Transcript → report.
- **M-C — accuracy campaign (scratch probe → G-1).** On the today
  mount (F4's fixture and walk), for each campaign change: record the
  uncompensated witness shift (the pins' method), then the shift with
  the candidate compensation applied — estimator (a) the runtime
  average (`content-height / total-rows` read before the change) and,
  if the sketch's height cache is prototyped cheaply, estimator (b)
  exact-for-ever-live extents (the changed rows in the campaign are
  ever-live by construction — they start on-screen before the walk).
  Report per-estimator: median and worst residual as % of the
  uncompensated shift, per change class and depth. Also record the
  changed rows' true extents vs the average (the distribution the
  threshold's rationale assumed). Transcript → report.
- **M-D — cost/regression evidence.** If G-0 ∧ G-1 pass: the
  production diff exists, and G-3's before/after lane runs happen on
  the branch (`refresh/{n}` + today draw lanes × 3 N, twice per side
  if the first pair disagrees; through the flock wrapper or standalone
  outside the gates window — the 4.1 plan's R5 discipline). If the
  gate declines at G-0/G-1: record the cost argument from code shape
  (the compensation is O(1) property writes + an O(diff) summary
  against a refresh pass that is already O(N) derive — the refresh
  lane's unchanged run on the branch evidences the floor) and skip
  the after-runs (nothing production-shaped exists to measure).
- **The verdict section** (stage-2 report): per-gate numbers, the
  verdict, and — on decline — the drafted #592 annotation (verdict,
  the load-bearing numbers, the M-A mechanism fact, the re-entry
  condition: what future change — e.g. an upstream repeater that
  exposes per-row extents or a saner notification path — would
  reopen). On implement: the deviation list, goldens status, and the
  CHANGELOG entry (user-visible: Today above-viewport changes stop
  reflowing deep-scrolled viewports).

## 5. Implementation sketch (applies only if §3's criterion passes)

Layering stays presentation-side; every piece follows a landed
precedent.

1. **Diff summary (models.rs, pure)**: `diff_rows` (or the
   `MirroredRows::diff`/`push` wrappers, `models.rs:404-425`) gains a
   summary return — first structural-change index, removed count,
   inserted count (a small plain-data struct; the mutation walk
   already knows all of it). `push_today` (`:2239-2351`) collects the
   four sections' summaries. Unit-tested at this layer (M-A's
   extension grows into it).
2. **Anchor facts (adapter + global, the D20c/F8 shape)**: a
   `TodayAnchorFacts`-style global — per section: first-changed
   index, signed row delta, nonce — pushed by the adapter's refresh
   pass (edge-triggered, nonce-gated: a nonce change is one commit;
   a stale nonce is dropped, never queued — "manual scrolling
   afterwards is never fought").
3. **Compensation (listview.slint, opt-in)**: `in property <bool>
   row-anchored: false;` plus private last-content-height state. On
   the facts' nonce (or the D20c `changed content-height` one-shot,
   per M-B's winner): if `row-anchored` and the facts' first-changed
   index is strictly above the estimated window head
   (`floor(−content-y / (content-height / total-rows))` minus a
   guard band of one average row), write
   `content-y += signed-extent` — the direction the pins fix
   (insertion above shifts rows down ⇒ compensate content-y by
   +extent; removal above shifts rows up ⇒ compensate by −extent;
   M-C confirms signs empirically before the diff lands). Extent =
   the estimator M-C validated (average first; the height cache only
   if it shipped in the prototype). No off-window instantiation, ever.
4. **Today opts in** (`today-view.slint` call site:
   `row-anchored: true`); board/compact/stack call sites unchanged
   (constraint 1).
5. **Pins (tests/virtualisation.rs, extensions — never weakenings)**:
   with the mode on, the insertion-above and true-removal-above pins
   gain pixel-still variants asserting the witness rests at exactly
   the pre-change y (exact-zero is assertable here: the campaign's
   changed rows are ever-live, so the validated estimator is exact
   for them; a red extension is a finding per AGENTS §5, not a
   threshold to tune). The three existing pins run unmodified with
   the mode off — byte-identical, still green. New within-viewport
   and cross-section safety assertions ride the same mount (G-2's
   cases).
6. **Benches + docs**: the refresh/draw lane doc block gains the
   mode's sentence (what the lanes measure with it on — the lanes run
   the default off mode; the G-3 after-runs are the recorded
   evidence); CHANGELOG entry (user-visible); goldens: the mode
   changes no static geometry — byte-identical expected, and if any
   golden diffs, that is a finding to diagnose, and only then
   "updated as data" per the dispatch.

Commit sequence (each standalone-green, rebase-merge hygiene):
the M-A test commit (§4); then on pass —
`feat(ui-slint): the section anchor facts` (sketch 1+2),
`feat(ui-slint): the ListView row-anchored compensation` (sketch 3+4
+ the extended pins, TDD: the pins' red is the missing property/mode,
compile-level), `perf(?)`/`docs(?)` the bench evidence + CHANGELOG
commit; on decline — only the M-A commit + the report/annotation
commit (`docs(report): the row-anchor decline`). Stage 3 reviews,
ships the PR (title per dispatch §9: `perf(ui-slint): the
row-anchored ListView mode` or `docs(report): the row-anchor
decline`), and ships the #592 annotation with it.

## 6. Milestones (plan structure, not commit boundaries)

- **M1 — the mechanism fact pinned** (M-A): the rebind-cascade shape
  is a permanent, invert-checked test; §0 is now code-backed.
- **M2 — the measurement** (M-B, M-C): the seam question and the
  accuracy question answered with recorded numbers; the criterion
  applied; the verdict written.
- **M3 — the outcome**: implement (sketch §5, G-3 lanes, goldens,
  CHANGELOG) or decline (report + annotation draft; no production
  diff).
- **M4 — the green sweep**: the full AGENTS §10 suite through
  `scripts/run-quality-gates.sh` (on a decline's docs+test diff: the
  gates that apply — fmt/clippy/deny/nextest — still run inside the
  wrapper); journeys and goldens green unmodified on the default
  path.

## 7. Acceptance checklist (maps 1:1 to dispatch §9)

- [ ] The measurement ran before any production diff; its numbers
      live in the stage-2 report (M-B/M-C transcripts, M-D lanes if
      reached).
- [ ] The criterion was applied as pre-declared (§3); the verdict is
      recorded with the per-gate numbers.
- [ ] M-A's mechanism test landed, invert-checked.
- [ ] Implemented ⇒ the three existing pins byte-identical and green
      (mode off), extended pixel-still pins green (mode on),
      within-viewport/cross-section safety asserted, benches
      within noise, goldens as data, CHANGELOG entry.
- [ ] Declined ⇒ measurement report with numbers, #592 annotation
      **drafted** (posted by the shipping stage, not silently
      closed), no production diff, gates green.
- [ ] Non-goals respected: within-viewport semantics untouched; no
      other #373 consumer changed.

## 8. Risks and resolved questions

- **R1 — the ordering unknown** (G-0): slint's `changed`-callback
  dispatch against the runtime's `content_height_set` →
  `content_y_set` sequence inside one pass is not pinned by recorded
  evidence; the write may be superseded in-pass, deferred-past it, or
  oscillate with the next pass's re-derivation. This is the chunk's
  primary unknown and the first thing M-B measures. If every
  sanctioned seam loses, D-1 declines — no private-API escape.
- **R2 — estimator accuracy is distribution-bound** (G-1): the
  average estimator's error scales with the row-height spread
  (~±19 % on the fixture, §3 rationale); a fixture/card change that
  widens the spread (e.g. context-line on/off mixing) degrades
  compensation quality. The height cache (exact for ever-live rows)
  is the mitigation; its coverage (rows never live — the typical
  early-session head row) falls back to the average. Measured, not
  assumed.
- **R3 — boundary mis-discrimination** (G-2): a change landing within
  the guard band of the estimated window head could compensate when
  it shouldn't. The pre-declared resolution: ambiguity never
  compensates (today's behaviour is the fallback); M-C includes a
  boundary case to record the guard band's real width.
- **R4 — synthetic scroll side-effects**: the compensation moves
  content-y through the same path as user scrolling — it may prepend
  instances above the window (the runtime's actual-height walk) and
  it updates `previous_content_y`; it must not trip the 1.5-screen
  seek (a one-row extent never approaches 1.5 screens) nor fight a
  concurrent seek (the nonce/drop-if-stale discipline; seeks and
  compensations are serialized by the refresh/tick cadence in
  practice — asserted nowhere, recorded in the report if M-B shows
  interleaving sensitivity).
- **R5 — bench noise discipline** (G-3): times are references, never
  gates; the criterion consumes only the measured noise band; runs
  go through the flock wrapper (or standalone outside a gates
  window) so parallel agents don't OOM the box (AGENTS §10).
- **R6 — partial-region class** (G-3): the compensation's synthetic
  scroll repaints the shifted band in the same pass as the flip's
  reflow; today's region is already the full content area on removal
  (design round §1.3), so no class growth is expected — measured
  anyway; a systematic growth is a decline datum.
- **R7 — honesty about value** (C-4): this is p3 polish on a
  background-change case; the impl-plan's bar is "drop without
  ceremony". A marginal pass (G-1 near its line) should weigh
  toward decline — the criterion's numbers decide, and the report
  states the margin.
- **Q — does the Done-section insertion need compensation?** No: it
  lands below the Overdue viewport in a separate ListView; the
  cross-section pin already holds it harmless to Overdue, and the
  page-level shift it causes (sections stack vertically) is out of
  scope — recorded as a known limit (the fixture's Overdue is ≫ the
  viewport bound, so its clip height is capped and the page does not
  move; short sections shrinking/growing would move the page —
  unchanged by this chunk either way).
- **Q — kiosk/board?** Kiosk mounts the board; the board call sites
  don't opt in (constraint 1); the mode's consumer is the desktop
  Today view only.
- **Q — why not compensate inside `diff_rows` (Rust-side content-y)?**
  content-y is UI-side scene state; models.rs never touches the
  scene. The summary/facts split (sketch 1→2→3) is the layering
  boundary: pure computation in models, scene writes in
  markup/globals, exactly the D20c/F8 precedents.

## 9. Self-check per the stage-1 prompt

- Measurement design covering the three mandated quantities: extent
  observability/cost (C-2, M-C), compensation correctness against
  the runtime's own behaviour (C-1, G-0, M-B), bench deltas (G-3,
  M-D) — §4.
- Decision criterion stated before measuring — §3, deliberately
  ahead of §4, with pre-declared thresholds and their rationale.
- Implementation sketch on pass — §5, every piece on a landed
  precedent (D20c write seam, F8 nonce signal, F5 persistent
  handles, the pins' instrument class).
- Doctor run (exit 0), worktree created with `-b` off the fetched
  `origin/main`, nothing implemented in this stage, nothing pushed.

# [Plan] GUI Review Round 5, Chunk 1 — Dead Seams

- **Date**: 2026-11-05
- **Status**: PROPOSED — stage-1 plan for chunk 1 of the round-5
  dispatch. Branch `fix/ui-slint-dead-seams`, cut from `main`
  (`5a663482`; `scripts/worktree-doctor.sh` passed pre-creation,
  184 G free against the 25 G floor; worktree
  `../taskboard-WT-fix-ui-slint-dead-seams` created with
  `git worktree add -b fix/ui-slint-dead-seams ../taskboard-WT-fix-ui-slint-dead-seams main`).
  No gate preconditions: chunk 1 is dispatch-order-first and
  order-independent (dispatch §1).
- **Origin** (normative): round plan
  `2026-11-05-gui-review-round5-plan.md` §2 row F1, §3 D8, §4.1;
  dispatch plan `2026-11-05-gui-review-round5-implementation-dispatch-plan.md`
  §1/§2 (acceptance: journey through the real callback; inventory
  fails on a deliberately-removed forwarding; all gates green; diff ≈
  one markup line + two test files; non-goals: no other callback
  changes, no Rust-side seam work, no panel changes). Code-quality
  mandate per dispatch §1 applies at stage 3.
- **Scope guard**: `taskboard-ui-slint` only — one markup line in
  `ui/app-window.slint`, one new test binary
  `tests/view_mount_inventory.rs`, journey additions in
  `tests/journeys.rs` (the house journey suite — journeys are not
  split out; "two test files" reads as *two test files touched*).
  Empty diff over every other crate (stage-3 review clause).

---

## 0. Findings → mechanisms audit (what the code confirms)

All anchors read on `main` @ `5a663482`.

| Fact | Anchor | Evidence |
|---|---|---|
| The affordance exists and fires the component callback | `ui/views/task-view.slint:686-702` | `entries-button := IconButton { label: "Time entries"; clicked => { TooltipState.unhover(); root.task-track-entries-clicked(); } }`, conditional on `root.task.track-entries-visible` |
| The callback is declared on the component root | `task-view.slint:131` | `callback task-track-entries-clicked();` (r4 D-TS8: routes `task.track-entries` over the subject) |
| The window declares the callback; the Rust seam is installed | `ui/app-window.slint:1214`; `src/adapter.rs:7269-7273` | `pure callback task-track-entries-clicked();`; `install_tracking_seams` binds `on_task_track_entries_clicked` → `track_target = subject_task()`, `route(ActionId::TaskTrackEntries)` — **the Rust side needs no change** (dispatch non-goal) |
| ⇒ The mount is the only dead link | `app-window.slint:438-475` | the `TaskView` mount forwards ~30 callbacks but **not** `task-track-entries-clicked` — an unbound Slint callback is a silent no-op (F1) |
| The seam inventory covers only the Rust install side | `tests/seam_inventory.rs` | two-sided source scan over `adapter.rs` (totality + no-unlisted); nothing pins the Slint mount forwarding — the gap D8 closes |
| Existing tests bypass the dead link | `tests/journeys.rs:12138-12143` | the entries-overlay walk journey invokes the **window-level** `invoke_task_track_entries_clicked()` — it exercises seam → route, never the mount forwarding, so F1 is invisible to it (its "through the affordance's seam" comment overstates the path) |
| The interpret arms' refusals are pinned at their layer | `src/adapter.rs:1949-1960`; `src/flow_table_tests.rs:3415-3456` | kiosk arm (`if self.kiosk => Vec::new()`) and the subjectless arm both refuse; `the_kiosk_gate_refuses_the_entries_open` pins kiosk at the interpret layer (affordance hidden, no flow, no command) |
| The click-equivalence journey grammar exists | `tests/support/driver.rs:88-99, 156-158, 280-317` | `click_by_label` = a11y default action on the labelled element — no coordinate arithmetic; `IconButton` maps `accessible-action-default => root.clicked()` (`ui/widgets/icon-button.slint:48-52`), so the a11y invocation lands in the exact handler a mouse click hits |
| The button is uniquely addressable at click time | `task-view.slint:689`; `ui/compositions/track-entries.slint:110` | the only other `"Time entries"` accessible label is the open overlay's own panel — absent while the button is clicked; `track_entries_visible: !kiosk && subject.is_some()` (`src/models.rs:1897`) keeps it mounted in the desktop fixture |
| The overlay renders the subject's entries | `src/models.rs:103` | the pushed `track-entries` rows are the subject's, index-aligned — row presence/label pins the open's subject |
| A `show-time` flag structurally disables the picker's time half | `ui/compositions/date-picker.slint:37,150`; `ui/compositions/track-entries.slint:193-199` | `if (root.show-time): TimeField {…}`; the track-form's grid mount passes `show-time: false` — its three time callbacks cannot fire there |
| The picker's grid-focus reverse seam is mount-specific by design | `date-picker.slint:107-117` | `changed grid-has-focus => root.grid-focus-changed(...)`; the task-detail mount consumes it (keymap-context arming, r3 D25); the track-form's context is flow-armed adapter-side (no `track_form_*_focus` seam exists in `tests/seam_inventory.rs`) |
| The flyout menu is the compact-mode twin | `app-window.slint:792-809`; `ui/compositions/action-menu.slint:51-56, 70-76, 120-132` | `invoke-submenu-entry`/`level-back` fire only under `compact && active-group >= 0`; the flyout's own mount condition excludes `compact` (`app-window.slint:792-793`) — both callbacks are structurally unreachable on that instance |

## 1. The fix (one markup line)

In the `TaskView` mount (`app-window.slint`), directly after the
`task-track-stop-clicked()` forwarding (line 473), mirroring the
component's declaration order (`task-view.slint:126-131`):

```slint
task-track-entries-clicked() => root.task-track-entries-clicked();
```

No other callback changes (dispatch non-goal). The line's target is
the window root's existing pure callback; the Rust seam, router,
flow-table and panel are untouched.

## 2. `tests/view_mount_inventory.rs` — the D8 inventory (new binary)

The Slint-side complement of `tests/seam_inventory.rs`: a source-scan
manifest assertion over the `.slint` tree. Source-scan (not compiler
introspection) is the round plan's deliberate choice (§4.1): the
failure mode is a missing line of markup, which a source assertion
pins exactly.

### 2.1 Covered boundary (decision, recorded)

- **Covered declarations**: every `export component` declared in
  `ui/views/*.slint` or `ui/compositions/*.slint` — the crate's
  public view/composition surface (25 components today: 7 views + 18
  compositions). These are the components other files mount; a
  declared-but-unforwarded callback on one is exactly F1's class.
- **Covered sites**: every instantiation of a covered component found
  in any component body in `ui/app-window.slint`, `ui/views/*.slint`,
  `ui/compositions/*.slint` (322 instantiation edges today, of which
  ~60 instantiate covered components). Site bodies are scanned for
  sites too — a composition mounting a composition is checked at that
  inner edge (the TrackEntriesPanel → DatePicker edge is the live
  example).
- **Excluded, with reasons**:
  - *Widget declarations* (`ui/widgets/*`: TextField, TippedField,
    TimeField, Dropdown, IconButton, TaskCard, Slider,
    SegmentedControl, ActivationStop, …): their callback surface is an
    opt-in protocol (e.g. `focus-lost` is deliberately handled only by
    mounts that settle flows), their mounts are in-file at authoring
    distance, and they are exercised by widget journeys + the chunk-4
    focus-coverage invariant. Including them today flags ~25 sites,
    all reviewed: none is F1's class (per-site triage table recorded
    in §6).
  - *Private sub-components* (non-exported: `LabeledField`,
    `SettingsButton`, `BoardPicker`, `GeneralField`, `PresetChip`,
    `StatCell`, …): not part of the mounted surface; wired in the same
    file by the same author, handlers are inline closures rather than
    forwarding seams.
  - *Widget-internal mounts* (sites inside `ui/widgets/*` files):
    widget-internal wiring, covered by widget journeys.

### 2.2 Parse strategy (brittle-regex avoidance)

A small string-aware Slint-subset tokenizer in the test file (~140
lines), not a pile of one-off regexes over raw text:

1. **One pass, string-aware**: scan each file producing (a) a
   *blanked* copy — line comments removed, every character inside a
   string literal replaced by a space (quote chars kept) — and (b) a
   per-character brace-depth array computed over the blanked copy.
   Blank strings *before* matching: the tree contains
   `"https://cloud.example.com"` placeholders (a naive `//` strip
   corrupts them) and `"\{root.task.title}"` interpolations (a naive
   brace counter desyncs). Escaped quotes `\"` handled in the string
   scanner.
2. **Declarations**: for every `(pure )?callback` token occurrence at
   depth 0 inside a component body, read forward to the terminating
   `;` (whitespace/newline tolerant) and parse
   `name (arg-types) (-> ret)?`. Depth-0 gating is what separates
   declarations from identically-spelled handlers inside nested
   element blocks. Component bodies come from balanced-brace
   extraction (via the depth array) over
   `(export )?component Name (inherits X)? {`.
3. **Sites**: regex over the blanked text for
   `(?:(\w[\w-]*)\s*:=\s*)?([A-Z]\w*)\s*\{`, optionally preceded by an
   `if (…):` mount condition (conditions match `[^{]*?` — they span
   lines and contain no braces once strings are blanked). A match is
   a site iff the name is a declared component (registry membership —
   builtins need no list) and the preceding token is neither
   `component` nor `inherits` nor `global`. The body is the balanced
   brace block.
4. **Forwarding check**: within the site body only, at body-relative
   depth 0, a *handler* for callback `cb` is the token `cb` followed
   by an optional `(…)` argument list and then `=>` (handler or
   forwarding — zero-arg handlers are spelled `clicked =>`, never
   `clicked() =>`) or `<=>` (two-way binding). A `changed` keyword
   immediately before the token disqualifies it (property change
   handler, not a callback handler). Depth-0 gating prevents a nested
   sibling instantiation's handler from answering for this site.
5. **File discovery**: runtime walk of
   `env!("CARGO_MANIFEST_DIR")/ui/{app-window.slint,views/,compositions/}`,
   sorted for determinism — *not* an `include_str!` literal list
   (18+ files would rot silently the day a new view/composition file
   lands; the walk keeps the file set self-maintaining).

Parser contracts get unit tests on inline mini-sources (the parser's
lowest layer): handler-with-args, zero-arg handler, `changed`-prefix
exclusion, string/comment blanking (`//` in a string, `\{…}` in a
label), multi-line declaration tolerance. One behavior, one test.

### 2.3 The inventory assertions (two-sided, seam_inventory's honesty rule)

- **Side 1 — totality**: for every covered site × every callback the
  instantiated component declares: a handler exists at the site, or
  an exemption entry covers the pair *at that site*. Failure message
  names file, parent component, site head-snippet, component,
  callback — actionable, not a count.
- **Side 2 — no stale exemptions**: every exemption entry must (a)
  name a (component, callback) that exists in the component's
  declaration list, and (b) carry a *site predicate* — a substring
  found in at least one instantiation body of that component. Both
  halves rot-check: a renamed/removed callback or a rewired site
  fails the test and forces re-triage.
- **F1 regression pin**: the `TaskView` mount is a covered site;
  removing the §1 forwarding line fails Side 1. This is the review
  spot-check (dispatch acceptance); no dedicated test for it.

### 2.4 The exemption list (complete today, predicates included)

| Component | Callback | Site predicate | Reason |
|---|---|---|---|
| `ActionMenu` | `invoke-submenu-entry` | body contains `is-flyout: true` | fires only in compact replace mode (`action-menu.slint:70-76`); the flyout's mount condition excludes `Breakpoint.compact` (`app-window.slint:792-793`) — structurally unreachable |
| `ActionMenu` | `level-back` | body contains `is-flyout: true` | the back row renders only under `compact && active-group >= 0` (`action-menu.slint:120-132`); same mount-condition exclusion — structurally unreachable |
| `DatePicker` | `time-edited` | body contains `show-time: false` | the time section renders under `if (root.show-time)` (`date-picker.slint:150`); the track-form grid mount is date-only (`track-entries.slint:194`) — structurally unreachable while the predicate holds |
| `DatePicker` | `time-picked` | body contains `show-time: false` | as above |
| `DatePicker` | `time-accepted` | body contains `show-time: false` | as above |
| `DatePicker` | `grid-focus-changed` | body contains `show-time: false` | reachable but deliberately unwired: the track-form's keymap context is flow-armed adapter-side (the reverse focus seam — `date-picker.slint:107-117` — is the task-detail mount's r3-D25 mechanism only); predicate identifies the same single site |

The predicates double as reachability witnesses: flipping
`show-time` to `true` or deleting the flyout's `is-flyout: true`
binding removes the predicate from the site body, Side 2 fails, and
the previously-exempt callbacks become enforceable. Recorded
limitation: a predicate is a site-shape witness, not a unique site id
— a *second* `show-time: false` DatePicker mount would inherit the
exemption; the reason text carries the contract, and stage-3 review
checks any new mount of these two components explicitly.

## 3. The journey (through the real callback)

One new journey in `tests/journeys.rs` — F1 is one behavior; the
refusal arms stay at their existing layer (§3.1).

```rust
#[test]
fn the_time_entries_affordance_opens_the_overlay_through_the_real_mount()
```

- **Fixture**: the entries-overlay journey's proven grammar
  (`journeys.rs:12102-12131`): `fixture::golden_state()` + one closed
  `TimeEntry` on `fx.task0` (start = now − 1 h, end = start + 30 min,
  `TimeEntryKind::Work`).
- **Path**: mount → `NavNextItem` → `TaskEdit` (arms the task view
  with a subject, the existing grammar) → `advance_time(0)` →
  **`harness.driver.click_by_label("Time entries")`** →
  `advance_time(0)` twice (the existing journey's deferred-nonce
  rhythm; `advance_time` is the mock-clock handler pass — no sleeps).
- **Assertions** (typed outcomes, labels only as addresses):
  1. *flow opens*: `window.get_task_model().track_entries_open` —
     fails today (the click dies at the mount), green after §1. This
     is the TDD red.
  2. *subject correct*: the panel's model carries exactly the
     subject's entry — `track_entries.row_count() == 1` and the row's
     accessible label is the inserted entry's local span
     (`start.with_timezone(&Local).format("%Y-%m-%d %H:%M")` — the
     existing journey's address grammar, `journeys.rs:12174-12184`).
     The fixture's other tasks carry no entries, so the row's
     presence pins the open went over `fx.task0`.
- **Explicitly NOT asserted here**: the walk/ladder behavior (pinned
  by the existing overlay journey, untouched), kiosk refusal and
  subjectless refusal (pinned at the interpret layer — §3.1), form
  seeding (pinned at the flow-table layer).

### 3.1 The kiosk refusal arm — unchanged, not duplicated

"Kiosk refusal arm unchanged" (dispatch prompt) is satisfied by
*non-interference*, not by a new test: `the_kiosk_gate_refuses_the_entries_open`
(`flow_table_tests.rs:3415`) already pins the kiosk arm at its lowest
natural layer — kiosk mount, `track_entries_visible == false`
(`models.rs:1897`'s `!kiosk` term — the button is not even mounted),
interpret refuses, zero commands. A UI-level kiosk leg would be
unisable anyway: with the button unmounted, the only entry is the
window callback, whose refusal on kiosk is indistinguishable from the
subjectless arm (`adapter.rs:1949` vs `:1954-1960`) — an ambiguous
assertion, and a duplicate of a pinned behavior (AGENTS §5:
one behavior, one test, lowest layer). Stage 2 must not add it; both
existing pins must stay green untouched.

### 3.2 One comment correction in the existing journey

The entries-overlay walk journey's comment at `journeys.rs:12138-12140`
("Open through the affordance's seam (click ≡ key)") describes the
window-level callback as the affordance's click path — exactly the
false confidence F1 exposed. Stage 2 rewords it to say the
window-level dispatch seam (the affordance's real click path is the
new journey's pin). Comment-only change, same commit as the new
journey.

## 4. TDD sequencing & commits

Stage 2 works in `../taskboard-WT-fix-ui-slint-dead-seams` (branch
`fix/ui-slint-dead-seams`), all paths crate-relative under
`crates/taskboard-ui-slint/`:

1. **Journey first, observe the red**: write §3's journey; run it and
   confirm it fails with the dead-callback failure mode (the overlay
   stays closed through the real click) — that red is F1 reproduced
   at the UI layer.
2. **Commit 1** — `fix(ui-slint): the task view forwards the entries
   affordance to the window` — the §1 forwarding line + §3's journey
   + §3.2's comment fix, together (every commit standalone: a lone
   red journey would fail the gates; AGENTS §8 hygiene).
3. **Commit 2** — `test(ui-slint): the view-mount callback inventory` —
   `tests/view_mount_inventory.rs` per §2. Green immediately on the
   fixed tree; its teeth are proven by the review spot-check
   (remove the forwarding line → Side 1 fails → restore) and by the
   parser unit tests.
4. **Gates**: the full AGENTS §10 suite through
   `scripts/run-quality-gates.sh` (from the main checkout) before
   handoff. No Cargo.toml changes — `tests/*.rs` binaries are
   auto-discovered; SPDX header `// SPDX-License-Identifier:
   AGPL-3.0-only` on the new file (AGENTS §6; ui crate = AGPL,
   `#![forbid(unsafe_code)]` already crate-wide via the lib).

**Estimated diff**: ~1 markup line + ~320–380 test lines
(`view_mount_inventory.rs` ≈ 300; `journeys.rs` ≈ 45 + one comment).
Two commits. S-sized, as dispatched.

## 5. Test-quality compliance (AGENTS §5 map)

- *No text assertions*: labels are element addresses (find-by-label)
  or span projections of seeded data; outcomes are typed model reads
  (`track_entries_open`, `row_count`). The inventory test asserts
  structure, never markup text content — its failure strings quote
  sources only for diagnosis.
- *Deterministic by construction*: `advance_time(0)` handler passes,
  no sleeps; the inventory scan is a pure function of the tree
  (sorted file walk).
- *No quality ceilings / no pins of today's accidents*: the inventory
  asserts totality (a floor: every declared callback forwarded), not
  a whitelist of today's lines; exemptions are predicate-witnessed so
  a rewired site re-enters enforcement.
- *Lowest layer*: open-behavior journey at the UI layer (new), kiosk
  and subjectless refusals at the interpret layer (existing, kept),
  parser contracts as parser unit tests (new).
- *A new test failing can be a bug*: the journey's initial red *is*
  the production bug; the inventory is written after the fix, so any
  red it ever shows is a real regression or a stale exemption.

## 6. Out-of-boundary findings (recorded, not fixed here)

The planning scan surfaced the items below; each is reviewed, outside
the D8 boundary (§2.1) and outside this chunk's non-goals. They are
recorded so stage 3 does not re-litigate them and so the backlog can
pick them up:

1. **Connection-form fields commit nothing on Enter** —
   `LabeledField` declares `committed()`/`focus-lost()`
   (`settings-view.slint:304,307`); its four connection-form mounts
   forward only `edited` (`settings-view.slint:780-829`), so Return
   in Name/Server/Username/App-password is a no-op (the form commits
   via the Save button; the General-editor wrapper *does* forward
   both — `settings-view.slint:451-470` — proving the omission is not
   a pattern). Proposed issue: `backlog: connection form fields
   commit/focus-lost unwired` (labels `ui`, `p3`), permalinking this
   plan §6.
2. **Widget-declaration flags reviewed, none F1's class** (all out of
   boundary per §2.1): `Dropdown.row-context` at the board-bar
   dropdown — already documented in prose as deliberate
   (`board-bar.slint:73-77`, D40's strip-only context menu) and the
   settings BoardPicker twin (same reasoning applies, uncommented);
   `SearchPalette` query-field `accepted` (Return reaches the capture
   layer first); the timer-setter/timers label/when fields'
   `accepted`/`edited`/`focus-lost` skips (commit rides the form's
   commit seam); the drag ghost's `TaskCard` is deliberately inert
   (`app-window.slint:910-923`); the plan-next-day cards render under
   view-level stops (`plan-next-day-view.slint`, #622's reviewed
   shape); the settings "Backend" `SegmentedControl` is display-only
   (`settings-view.slint:769-776`, disabled-pick swallow documented
   in prose); `FocusOverlay`'s note field skips `focus-lost` (the
   overlay closes via its own scrim seam). If stage 3 disagrees with
   any of these classifications, the finding goes to the report, not
   into this chunk's diff.

## 7. Acceptance checklist (dispatch §2 mapped)

- [ ] The journey passes through the real affordance click
      (`click_by_label` → component callback → mount → window seam),
      asserting flow-open + subject row (§3).
- [ ] The inventory test fails on a deliberately-removed forwarding
      (stage-3 spot check: delete §1's line, `cargo nextest run -p
      taskboard-ui-slint --test view_mount_inventory` fails, restore).
- [ ] The inventory test scans, not hardcodes: a newly declared
      unforwarded callback on any covered component fails it (Side 1
      has no per-callback whitelist; §2.3).
- [ ] A newly added view/composition file is auto-covered (runtime
      walk, §2.2 step 5).
- [ ] All AGENTS §10 gates green via `scripts/run-quality-gates.sh`;
      no clippy warnings; `cargo fmt --check` clean.
- [ ] Diff = one markup line + two test files (§4); every other crate
      empty.

## 8. Risks

- **The journey's a11y address** (`"Time entries"`): `find_by_label`
  returns the first match; the overlay panel shares the label but is
  unmounted at click time (§0). If a future relabel collides, the
  journey fails loudly (`click_by_label` panics on absence) or clicks
  the wrong element — the plan pins the uniqueness precondition here
  so stage 3 can check it.
- **Parser drift against future Slint syntax** (multi-line callback
  decls exist only as hypotheticals today; `states`/`property <if>`
  unused in this tree): the tokenizer is depth-gated and
  string-blanked, with unit tests for each contract; the failure mode
  is a loud inventory failure or Side-2 rot, never silent under-
  coverage. A construct the parser cannot read fails the *site*
  loudly (unbalanced braces surface as depth anomalies), not the
  check quietly.
- **Exemption predicates are shape witnesses, not unique ids** (§2.4):
  a second `is-flyout: true`/`show-time: false` mount inherits the
  exemption. Bounded by two components × four callbacks; the review
  step checks any new mount of `ActionMenu`/`DatePicker` explicitly.
- **`click_by_label` timing**: the a11y snapshot follows the mock
  clock's handler pass; the existing overlay journey proves the
  rhythm (`advance_time` after the view arm, after the invoke). If
  the button's label is not yet addressable after `TaskEdit`, one
  extra `advance_time(0)` is the sanctioned fix — never a sleep
  (AGENTS §5).

# [Plan] GUI Review Round 5, Chunk 2 — The MultiLineEditor (F2–F7, F12, D1)

- **Date**: 2026-11-05
- **Kind**: chunk plan (stage 1 of the three-stage dispatch) — TDD-first,
  no implementation in this commit.
- **Origin**: `.artifacts/plans/2026-11-05-gui-review-round5-plan.md`
  §2 rows F2–F7 + F12, §3 D1 (normative, including the licensing
  constraint), §4.2; dispatch plan
  `.artifacts/plans/2026-11-05-gui-review-round5-implementation-dispatch-plan.md`
  §1/§3 (normative).
- **Branch**: `feat/ui-slint-multi-line-editor` (this worktree).
- **Status**: READY — every open question settled in §3; the one
  empirical unknown (the viewport probe) has a designed decision gate
  (§5) with a pre-approved fallback (§5.3).

## 1. Current machinery (verified anchors, main @ `5a663482`)

- **The fence**: `ui/widgets/text-field.slint:132-139` — the multiline
  flavour of `TextField` is a fixed three-line window (`min-height:
  3 × placeholder-text.preferred-height + 2 × Theme.space-2`,
  `clip: true`) because binding the field's own `min-height` to
  `input.preferred-height` was a **width-dependent layout cycle on the
  pinned Slint 1.18.1** — a compile-time binding-loop deprecation
  warning ("may cause panic at runtime") that regressed the r3
  warning-free-build gate. Recorded at
  `.artifacts/plans/2026-10-26-ui-r3-taskview-dates-plan.md` §9
  ("Stage-3 deviation"). The single-line probes at
  `text-field.slint:64-69` (Return fires `accepted`, no newline) and
  the layout-citizenship pin (r2 D22, `:8-13`) stay untouched.
- **The description section**:
  `ui/views/task-view.slint:831-912` — display branch (a stretched
  `Rectangle` + `ActivationStop` id 11 + hover + tooltip + a `RoleText`
  with `wrap: word-wrap` but **no width** — not in a layout, so wrap
  never engages: F6; ink at `x: 0` under the 2 px focus ring: F2) and
  edit branch (a `TippedField` with `multiline: true`: F3/F4). The
  section is the view's flexible element (r4 D16d,
  `task-view.slint:838-844`).
- **The Rust funnels — zero-diff by design**:
  `src/view_state.rs` `begin_edit_description` (`:1105`),
  `commit_description` (`:1121`, buffer verbatim, newlines round-trip,
  empty commits `Some("")` = the clear), `settle_flow`'s
  `EditDescription` arm (`:1893-1899`, commit non-empty / discard
  empty); `src/adapter.rs` `install_task_seam` —
  `on_text_flow_focus_lost` (`:6428`, the `try`-borrow absorb for the
  teardown echo), `on_task_description_edit_opened` (`:6453`),
  `on_task_description_edited` (`:6463`), `on_task_description_committed`
  (`:6469`). The chunk changes **none of these**.
- **Key path**: `intercept` (`src/adapter.rs:590-686`) runs in the
  capture phase ahead of any widget handler (the r4 D33 capture-tunnel
  ruling); while a text field holds focus the chain collapses to the
  insert contexts (`EditingFacts.text-focus-count > 0`), where Return
  and S-Return are unbound (verified against
  `src/keymap/presets.rs`: every `Return` binding lives in
  non-insert contexts — Board/Today/Menu/Palette/FocusMode/DatePicker/
  TaskDetail-closed) — so both strokes reach the focused `TextInput`.
  Escape **is** bound in the insert chain (action layer →
  `discard_edit`) and therefore can never reach a widget handler.
- **The reference grammar (READ ONLY — licensing)**:
  `~/.cargo/registry/src/*/i-slint-compiler-1.18.1/widgets/common/
  textedit-base.slint` is GPL/royalty-free-licensed. It is consulted as
  the reference for *which builtin mechanisms exist*
  (`TextInput.single-line`, `TextInput.page-height`,
  `cursor-position-changed(position: Point)`, `TextInput`'s
  `key-pressed` callback, viewport `content-height:
  max(visible-height, preferred-height)`) and **never copied**: the
  scroll math below is re-implemented with house names, house tokens,
  a derived line height instead of the reference's hardcoded `20px`,
  and no horizontal arm (word-wrap makes it moot). The std-widgets
  `ScrollView`/`TextEdit` themselves are not imported (D1's
  zero-std-widgets posture); the viewport is the builtin `Flickable`
  — the house scroll mechanism (`ui/compositions/action-menu.slint:110`,
  `ui/views/settings-view.slint:594`).
- **Test infrastructure**: probe `.slint` files compile via `build.rs`
  entries (`compile_with_output_path`, debug info on) and instantiate
  on the testing backend (`tests/focus_seam_probe_tests.rs` is the
  shape); keys go through `Harness::stroke` /
  `press_stroke(Stroke)` (`tests/support/driver.rs:189`) — the stroke
  grammar keeps Shift on named keys, so `S-Return` is expressible
  (`src/keymap/stroke.rs:126-175`); goldens regenerate with
  `TASKBOARD_REGEN_GOLDENS=1` (`tests/golden_visuals.rs:80`); the
  task-view golden matrix + `task-view-focus-cell` exist under
  `tests/goldens/`.

## 2. What the findings demand (acceptance restated)

F2 display inset; F3 editor no longer a shrinking 3-line window;
F4 scrollbar + scroll-by-cursor in edit mode; F5 display scrolls; F6
display wraps at the column edge; F7 Enter = newline, Shift+Enter =
commit; F12 one reusable L1 widget (future journalling reuses it).
Dispatch acceptance adds: the probe gates the fence, the focus-seam
probe pins hold for the new widget, gates green.

## 3. Decisions (recorded; deviations from origin §4.2 marked — there are none substantive, the two clarifications are D-MLE-5/6)

1. **D-MLE-1 — the box/viewport split (the cycle-break)**. The
   widget's own geometry never binds to `input.preferred-height` (that
   is the r3 cycle). The box is layout-driven: `min-height` = a
   three-line floor measured from a hidden one-line `TextInput`
   (`line-metric`, the input's platform-default font — the same font
   the editor renders in, unlike today's `placeholder-text`
   Inter-body floor), plus `vertical-stretch` at the adoption site.
   The *content extent* rides the `Flickable`:
   `viewport-height: max(self.height, input.preferred-height)`. The
   dependency chain `preferred-height ← input.width ← flick.width ←
   editor.width ← ancestors` contains no edge back from
   `viewport-height` into any width — no cycle by construction. This
   is the reference grammar's cycle-break re-expressed, and it is
   what the probe (§5) verifies empirically before the fence lifts.
2. **D-MLE-2 — one widget, two inner mounts, host-owned activation**.
   `MultiLineEditor` (`ui/widgets/multi-line-editor.slint`, L1)
   carries view mode and edit mode as two `if` branches over one
   `editing: bool` — *two mounts at the host*, one component. The
   current two-branch shape of the description section is kept: the
   display mount and the edit mount are separate instantiations, so
   the edit mount's `init` re-seeds `initial-text` per open (the
   buffer lifecycle belongs to the view state; no re-seed handler
   needed). Click-to-open, hover, tooltip, and the `ActivationStop`
   (id 11) stay in the host markup around the display mount —
   activation is a call-site concern, not editor machinery (the walk
   inventory is unchanged by construction).
3. **D-MLE-3 — the scrollbar** (house tokens only; passes
   `scripts/check_slint_tokens.py`): a vertical strip over the
   viewport's right padding band — thumb width `Theme.space-1`,
   vertical inset `Theme.space-2`, `border-radius: radius-small`,
   colour `Theme.border` → `Theme.text-secondary` on hover/press, on
   a transparent track. **Visible iff the live viewport's
   `viewport-height > viewport inner height`** (strictly greater; both
   modes — display and edit share the strip, reading whichever
   `Flickable` is mounted). Thumb height
   `max(Theme.space-6, inner² / viewport-height)` (a grabbable floor);
   thumb position `(inner − thumb) × (−viewport-y) / (viewport-height
   − inner)`, clamped to `[0, inner − thumb]`. Interaction v1: thumb
   drag (a `TouchArea` on the thumb; `moved`-delta maps 1:1 through
   the `(viewport-height − inner)/inner` ratio, writing `viewport-y`)
   plus the `Flickable`'s native wheel and flick. Track-click paging
   is deferred (recorded here; not a finding — PageUp/PageDown and
   the wheel cover paging). The strip is a sibling *after* the
   `Flickable` in tree order so its `TouchArea` wins the pointer in
   its band and never flicks the content.
4. **D-MLE-4 — cursor-follow scroll, re-implemented**. On
   `cursor-position-changed(cpos)` (cpos in the input's coordinate
   space): if `cpos.y + viewport-y < 0` then `viewport-y = −cpos.y`;
   else if `cpos.y + viewport-y > inner − line-height` then
   `viewport-y = inner − line-height − cpos.y`; clamp `viewport-y`
   into `[min(0, inner − viewport-height), 0]`. `line-height` is
   `line-metric.preferred-height` (D-MLE-1's measurement element) —
   *not* the reference's hardcoded `20px`. No horizontal arm
   (`wrap: word-wrap` makes the column the width; the reference keeps
   one only because it supports `no-wrap`). `input.page-height` binds
   the viewport's inner height so native PageUp/PageDown page exactly
   one viewport.
5. **D-MLE-5 — callback surface (chunk 7's attachment contract)**.
   `edited(string)`, `accepted(string)`, `focus-lost()`; public
   function `grab-focus()`; inputs `editing`, `text` (display source,
   live), `initial-text` (edit seed, one-shot at init),
   `placeholder`, `label` (a11y), `autofocus: true`,
   `commit-affordance: false`. **No `discarded()` callback** —
   recorded: Escape stays the action layer's (the capture tunnel
   delivers bound strokes to the shell before any widget handler, so
   a widget-fired discard would be dead code; chunk 7's editor scope
   routes editor discard through the scope stack to the adapter, not
   through a widget callback). **No two-way `text`** in v1 — the
   description site seeds via `initial-text` only; chunk 7 adds the
   selection/clipboard surface (`set-selection-offsets`, cut/copy/
   paste) the editor command table will need, against the stable
   inner `input` id. The `EditingFacts` latch grammar
   (input-has-focus mirror, `focus-counted`/`counted-generation`
   re-baseline, init publish — `text-field.slint:86-113,171-187`) is
   **replicated with a cross-reference comment**, not extracted:
   chunk 7 generalizes focus publication anyway, and a shared
   component cannot reach into another component's `TextInput`;
   divergence risk is accepted for one chunk and pinned by §7's
   probe arms.
6. **D-MLE-6 — commit/Escape/focus-lost map onto the existing funnel,
   unchanged**. Shift+Enter → widget `accepted` → adapter
   `on_task_description_committed` → `commit_description` (newlines
   verbatim; empty = the `Some("")` clear). The commit affordance
   (checkmark) fires the same seam — click ≡ Shift+Enter. Plain Enter
   → native newline insertion (`single-line: false` reverses the
   recorded single-line probe `text-field.slint:64-69`); the widget's
   `key-pressed` returns `accept` only for the Return-class stroke
   with Shift (`event.text == "\n" || event.text == "\r"`), `reject`
   for everything else so native editing keeps every key. Escape →
   insert-chain binding → `discard_edit` (zero widget involvement).
   Focus loss → `focus-lost()` → `on_text_flow_focus_lost` →
   `settle_flow`'s `EditDescription` arm (commit non-empty, discard
   empty; the `try`-borrow seam absorbs the commit's own teardown
   echo). `flow_table_tests` rows — including the Activate-commits
   totality row — are **unchanged**: D1 redefines the *field-level*
   key grammar only. Consequently the r3 journey pin "type → Return ⇒
   `Some(_)`" (`tests/journeys.rs:6818-6849`, preset-parameterized)
   is a **designed redefinition** under normative D1: it becomes
   Return-inserts-newline + Shift+Return-commits. This is not a
   weakening; the new grammar is the contract (flagged for stage 3).
7. **D-MLE-7 — `TextField` retirement path, gated on the probe**.
   The description editor is the **only** `multiline: true` consumer
   (`task-view.slint:903`). On the probe-PASS path the flavour
   retires from `TextField` (and its `TippedField` forwarding) — dead
   code otherwise — and the fence comment is *superseded*: its
   constraint substance moves to the editor's own comment ("the box
   never binds to the input's preferred-height — the r3 recorded
   cycle; growth rides the viewport"), with the history permalink'd
   (r3 plan §9). This is the "lifted with a passing probe" arm — the
   dispatch's "not silently deleted" is satisfied by this plan + the
   editor's constraint comment. On the probe-FAIL path,
   `text-field.slint` is untouched verbatim (fence intact; the now
   unconsumed flavour stays as the fence's own documentation, its
   dead-ness recorded here for a later cleanup decision).
8. **D-MLE-8 — adoption keeps the InfoTip pair**. The edit mount
   composes `HorizontalLayout { MultiLineEditor …, InfoTip }`
   manually (the `TippedField` visual grammar, which stays for its
   four other consumers); the display mount keeps today's hover
   surface, `ActivationStop`, and tooltip chrome around the editor.

## 4. The widget (markup contract for `ui/widgets/multi-line-editor.slint`)

`export component MultiLineEditor inherits Rectangle`, SPDX
AGPL-3.0-only, tokens/`RoleText` only (the platform-default
`TextInput` font stays — the recorded `text-field.slint:206-209`
negative). Surface chrome = the closed-cell grammar:
`Theme.surface-raised` background, `radius-small`, focus ring
(`Theme.focus-ring-width` × `Semantic.focus-ring`) while the inner
input holds focus, `clip: true`, `min-width: Theme.space-15`,
`min-height: 3 × line-metric.preferred-height + 2 × Theme.space-2`.

Inside, the measurement element; then `if (!editing)` a
`view-flick := Flickable` (inset `Theme.space-2` on all sides — the
F2 fix: ink never starts under the ring) hosting
`view-text := RoleText { role: Typography.body; wrap: word-wrap;
width: parent.width; text: root.text; }` (width pinned to the
flick — wrap finally engages: F6) with
`viewport-height: max(self.height, view-text.preferred-height)`; and
`if (editing)` an `edit-flick := Flickable` (same inset) hosting
`input := TextInput` per D-MLE-1/4/5 (`single-line: false`,
`wrap: word-wrap`, `width: parent.width`, `page-height`,
`vertical-alignment: top`, the Shift+Return `key-pressed` arm, the
cursor-follow handler, `edited`/focus-latch wiring, `init` seeding
from `initial-text` + autofocus). The scrollbar strip (D-MLE-3)
follows the flicks; the commit-affordance checkmark (the
`text-field.slint:279-314` grammar verbatim, TouchArea-only) closes
the tree.

## 5. The probe (gate for everything)

### 5.1 Fixture and arms

New `tests/editor_viewport_probe.slint` + `tests/editor_viewport_probe_tests.rs`
+ the `build.rs` entry (the seven-entry precedent shape). A facts
global publishes: `input-preferred-height`, `viewport-height`,
`viewport-y`, `inner-height`, `line-height`, `buffer`, `accepted-count`,
`newline-count` (from the key handler's fall-through arm), `focus-lost-count`.
The fixture hosts the editor prototype at a fixed box size in a
window whose width the test changes.

- **P1 — viewport growth**: seed text with k embedded `\n` via the
  two-way buffer property, advance the probe clock, assert
  `viewport-height == max(inner-height, input-preferred-height)`
  (exact — it is our own binding) and `input-preferred-height >
  line-height` for k ≥ 1; append lines → both grow. Growth of the
  *content extent*, never the box (the box height is asserted
  constant — the no-bare-layout-constraint discipline).
- **P2 — width-change re-wrap**: set a long single-paragraph text;
  assert its `input-preferred-height` at width W; shrink the window
  to W/2; advance the clock; assert `input-preferred-height`
  strictly increased and `viewport-height` tracked. This is the
  width-dependence that the fence recorded as a *hazard* — pinned
  here as *behaviour* on the safe side of the split.
- **P3 — cursor-follow**: many lines; drive the cursor to the end
  with keyboard (type text, arrows/End through `stroke`); assert
  `viewport-y < 0` and `cursor-visibility` per the handler's own
  published math (the handler writes the cursor fact; the test
  asserts the invariant `0 ≤ cursor-y + viewport-y ≤ inner −
  line-height` after the final move).
- **P4 — grammar + focus arms**: Enter inserts `\n` into the buffer
  (newline-count / buffer fact); Shift+Enter fires `accepted` exactly
  once and inserts nothing; the focus-latch fires `focus-lost` once
  per real loss on a sibling `FocusScope` click and never on a plain
  `TouchArea` click (the `focus_seam_probe` scenario re-hosted —
  D-MLE-5's replication pin); the scrollbar visibility predicate
  flips exactly at overflow, thumb height within
  `[space-6, inner]` and proportional within rounding.

### 5.2 Verdict expectation

**Expected: PASS.** The safe binding graph has no cycle
(D-MLE-1); the reference grammar ships this exact shape in every
Slint `TextEdit`. The residual risks the probe actually guards
against: (a) the pinned compiler's loop analyzer being conservative
enough to flag even the acyclic `viewport-height ← preferred-height`
chain, and (b) the testing backend not re-evaluating
`preferred-height` after a width change (the P5 lazy-layout class).
Both are cheap to discover here and expensive to discover after
adoption.

### 5.3 Verdict mechanics and the pre-approved fallback

In-test capture of compiler diagnostics is unavailable (probe
`.slint` compile via `slint_build`, which fails on errors but only
*prints* warnings; the interpreter crate is not a dependency and is
not justified for one probe — recorded). The verdict therefore has
two halves, both stage-2 duties recorded in §8 slice 1: the **runtime
half** = P1–P4 green; the **build-log half** = a clean build of the
new sources with **zero binding-loop deprecation warnings** (the
gate the r3 deviation failed; re-checked by stage 3).

**FAIL → the paged-viewport fallback** (origin §4.2, pre-approved):
the editor keeps the fixed/stretched window with `clip: true`, binds
`input.page-height` (native PageUp/PageDown), drops the
`viewport-height` binding, the scrollbar, and cursor-follow; the
description editor still gains newlines, the Shift+Enter/Enter/
Escape grammar, the focus facts, and the display inset/wrap/scroll
fixes that don't depend on growth (display-mode scroll ALSO falls
back to flick-only, no thumb). The fence at `text-field.slint:132-139`
stays verbatim; the negative is recorded in this plan's §5.3, the
implementation report, and the widget's comment; D-MLE-7's retirement
arm does not run (TextField untouched). The journeys are identical
minus the scroll/thumb arms.

## 6. Adoption diff shape

- `ui/views/task-view.slint:845-912`: display branch → the host chrome
  (unchanged stop/hover/tooltip) wrapping `MultiLineEditor {
  editing: false; text: root.task.description; vertical-stretch: 1;
  horizontal-stretch: 1; }`; edit branch → `HorizontalLayout {
  MultiLineEditor { editing: true; initial-text:
  root.task.description-initial; … } InfoTip { about/help } }`; the
  four callback forwards (`description-committed/edited`,
  `text-flow-focus-lost`) and the section's `vertical-stretch` are
  unchanged. F2/F5/F6 fall out by construction.
- `ui/widgets/text-field.slint` + `ui/widgets/tipped-field.slint`:
  the D-MLE-7 retirement (probe-PASS path only).
- `src/adapter.rs`, `src/view_state.rs`, `src/models.rs`: **zero
  diff** (seams and funnels identical; verified against
  `tests/seam_inventory.rs`, which stays green untouched).
- `build.rs`: the probe entry. `CHANGELOG.md`: Unreleased entry.
- Goldens: regenerate the task-view matrix (6 PNGs) +
  `task-view-focus-cell-1.0-900.png` with `TASKBOARD_REGEN_GOLDENS=1`;
  stage 3 reviews the pixel diff for **locality** — only the
  description region may move; anything else is a finding.
- Cross-chunk notes for the review: the editor's `clip: true` site
  joins chunk 4's clip-audit list (its `TextInput` lives inside a
  `Flickable` → the Flickable carve-out keeps it a traversal
  candidate); chunk 7 consumes the D-MLE-5 surface.

## 7. Test catalogue (TDD-first; one behaviour, one test, lowest layer)

Ordered red→green; every item names its layer.

1. **Probe suite** (§5.1 P1–P4) — component layer, testing backend.
2. **Journeys** (`tests/journeys.rs`, fake engine, typed outcomes):
   - *Redefined* (D-MLE-6): the preset-parameterized description
     session at `:6818` — type `hello`, Return (newline), type
     `world`, Shift+Return ⇒ `UpdateTask { description:
     Some("hello\nworld") }` and the editor closed; per preset.
   - *New*: Enter-newline persistence — multi-line buffer commits
     verbatim through the funnel (newlines survive the round trip).
   - *New*: plain Return does **not** close the editor (the editor
     stays open, no command drained) — the F7 core.
   - *Re-pinned, unchanged*: Escape discards (no dispatch);
     focus-loss settle commits non-empty / discards empty; the
     display stop's Return opens the editor (activation reservation).
   - *New*: the display branch of a long description is scrollable
     (flick assertion via pointer drag on the testing backend or the
     widget-level P-arms cover it and the journey asserts only
     adoption wiring — implementer picks the lower layer per AGENTS
     §5 and records the choice).
3. **Goldens**: §6's regeneration + locality review.
4. **Untouched by design**: `flow_table_tests.rs`, `seam_inventory.rs`,
   the walk journeys, `focus_seam_probe_tests.rs` (TextField's own
   pin), kiosk goldens.

## 8. Commit slices (standalone, rebase-clean; tree green each)

1. `test(ui-slint): the multi-line viewport probe pins growth, re-wrap, and the key grammar` — probe fixture, tests, build.rs entry; records the verdict (+ build-log check) in the report-of-record (the commit body); on FAIL, the same commit records the fallback decision and ships the paged arms.
2. `feat(ui-slint): the MultiLineEditor widget` — the widget per §4
   (fallback shape if slice 1 failed), driven green by the probe
   suite (the widget mounts inside the probe fixture).
3. `feat(ui-slint): the task description adopts the MultiLineEditor` — §6 adoption, journeys updated/added, goldens regenerated, CHANGELOG.
4. `refactor(ui-slint): the single-line field retires its fenced multiline flavour` — D-MLE-7 (PASS path only; dropped on fallback).

Stage 2 runs the full AGENTS §10 suite through
`scripts/run-quality-gates.sh`; no push, no PR (stage 3 ships).

## 9. Risks

- **The probe fails** (§5.2's two residual mechanisms) — pre-approved
  fallback, bounded loss: no scrollbar/growth; grammar and display
  fixes survive. Probability judged low.
- **S-Return collides with a future/hidden binding** — verified
  unbound today in the insert chains; the preset-parameterized
  journey and `keymap_snapshots` catch any regression.
- **Testing-backend layout laziness** (P5 class) makes P2/P3 flaky —
  mitigated by explicit clock advances per probe house style; if the
  backend refuses width-change re-evaluation entirely, that IS the
  probe failing (fallback), not a test bug — stage 2 must not
  "fix" the test to pass (AGENTS §5).
- **Golden locality** — the inset shifts every description pixel;
  the locality review bounds it. The focus-cell golden may shift by
  the inset; acceptable, reviewed.
- **Flickable + TextInput interaction regressions** (selection drags
  vs flick) — the P4 focus arm + the type/edit journeys cover the
  edit path; the slider-grab probe precedent (r3 D20b) is the model
  if a gesture matrix becomes necessary (deferred; not a finding at
  v1).
- **Chunk-4 parallel lane** may touch the same section for stop
  audits — disjoint lines in practice (activation chrome vs editor
  internals); the standard rebase dance applies.

## 10. Estimated diff size

Widget ~260 lines; probe fixture ~120 + tests ~340; task-view
adoption ±50; text-field/tipped-field retirement −25; build.rs +11;
journeys ±80; CHANGELOG +8; 7 regenerated goldens. **≈ 900 lines of
source churn — M, as dispatched.**

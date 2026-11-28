# [Plan] The keybinding coverage chunk (#1394, #1393, #875, #1257)

- **Date**: 2026-11-28
- **Kind**: stage-1 (planning) chunk plan — recorded reality, per-issue
  binding decisions, UX-plan amendment text, invariants, test
  catalogue, and paste-ready stage-2/stage-3 prompts. No production
  code in this commit.
- **Origin**: backlog issues #1394, #1393, #875, #1257 (all p3 ui);
  the keybinding-coverage audit
  `.artifacts/reports/2026-11-25-keybinding-coverage-audit.md` (round-7
  B6, #1280); the O-8 keyboard rulings
  (`.artifacts/plans/2026-11-18-gui-review-round9-round-plan.md` §2);
  the keyboard-first law (`docs/ux_planning.org` §2.7, 2026-11-23).
- **Base**: `origin/main` @ `3fc4764b` (worktree
  `../taskboard-WT-feat-ui-keyboard-coverage`, branch
  `feat/ui-keyboard-coverage`). All file:line anchors below were read
  at this base.
- **Status**: READY — one chunk, size M (see §9). Chord choices carry
  operator rulings (§7) but the chunk proceeds on the recommended
  defaults; a ruling change is a table-row re-spell, not a redesign.

## 1. Recorded reality (file:line at `3fc4764b`)

### #1394 — the plan-window box family is unbound

The plan-next-day surface resolves over `ContextId::PlanNextDay`
(`crates/taskboard-ui-slint/src/actions.rs:47`, non-masking view
context). Its preset rows today are exactly: the `ViewPlanNextDay`
close rows (`presets.rs:400-401`) and the walk axes — Standard
`Left`/`Right` `NavFocusLeft`/`NavFocusRight` (`presets.rs:408-409`),
Emacs `Ctrl+f`/`Ctrl+b` (`presets.rs:1378-1379`). None of
`PlanBoxEarlier` (`actions.rs:659`), `PlanBoxLater` (`actions.rs:661`),
`PlanRemoveBox`, or `PlanCommitPlace` (`actions.rs:656`) appears in any
preset table — `grep 'PlanBoxEarlier\|PlanBoxLater\|PlanRemoveBox\|PlanCommitPlace'
crates/taskboard-ui-slint/src/keymap/presets.rs` returns nothing. The
verbs exist and route (`actions.rs:935-938`, registry rows around
`actions.rs:2727-2737`); a placed box is unreachable by chord for
reorder and removal.

Audit tension worth recording (new information — commented on the
issue): the audit's §2 lists `PlanCommitPlace` under
*served-by-modal-grammar* (Return ≡ confirm by construction) while its
§5 family table names the whole family a genuine gap. The §2
disposition is the correct one for `PlanCommitPlace`: it is the armed
confirm of the placement form, and the keyboard-first law's §2.7
satisfies it through the modal grammar. The genuine gaps in the family
are the reorder twins and removal.

### #1393 — the review-walk stage verbs are unbound

`ContextId::ReviewWalk` (`actions.rs:86`) is a **masking** context —
it is in every preset's `masks()` list (`presets.rs:106`), so Global
rows are unreachable while the walk holds; anything the walk needs
must live in the walk context itself. Its only rows are the shared
block `REVIEW_WALK_ROWS` (`presets.rs:257-266`):
`ReviewWalkCursorUp`=Up, `ReviewWalkCursorDown`=Down,
`ReviewWalkBack`=Escape. Unbound in every preset (grep evidence, same
method as the audit): `ReviewFileTargetPrev`/`ReviewFileTargetNext`
(`actions.rs:689-691`), `ReviewWalkPrev` (`actions.rs:714`),
`ReviewWalkPlanner` (`actions.rs:712`), `ReviewAbortArm`/`Accept`/
`Dismiss` (`actions.rs:718-722`). All four verbs route
(`router.rs:2409-2425` — `UiEffect::ReviewWalk(WalkIntent::Planner)`,
`WalkIntent::Prev`, `WalkIntent::AbortArm/Accept/Dismiss`), so each is
a silent no-op by key inside the walk.

Two structural facts pin the spelling space:

1. The walk has a text field in its Prompts stage (the reflection
   prompt rides the multi-line editor), so printable single letters
   are **not** safe walk grammar in every stage — a printable typed
   while the field holds focus edits text. Non-printable,
   non-caret-capable strokes (Alt-family, Ctrl-family without an
   editor-table entry) are stage-proof.
2. The same audit tension as #1394: `ReviewAbortArm`/`Accept`/
   `Dismiss` and `ReviewWalkPlanner`/`ReviewWalkPrev` appear in the
   audit's §2 *served-by-modal-grammar* list AND in its §5 gap table.
   Disposition here (commented on the issue): Arm is a pre-confirm
   verb with no Return/Escape address of its own — it gets a chord;
   Accept/Dismiss stay the armed grammar's Return/Escape. Planner and
   Prev are navigation verbs the modal grammar does not serve — they
   get chords.

### #875 — already landed on main (the issue is stale)

Round-9 chunk 9 (`feat/ui-directional-focus`, the fold recorded in the
issue comment, dispatch plan §10 amended 2026-11-18) shipped the
dedicated `ActionId`s **and** the explicit Shift+letter preset rows:

- `ActionId::NavExtendNext`/`NavExtendPrev` (`actions.rs:300-301`,
  documented as the #875 selection-extend twin at `actions.rs:294`,
  registry availability pinned at `actions.rs:1467`);
- preset rows in all three presets: Standard `Shift+Down`/`Shift+Up`
  (`presets.rs:447-448`), Emacs `Ctrl+Shift+n`/`Ctrl+Shift+p`
  (`presets.rs:939-940`), Vi `J`/`K` — the explicit uppercase-printable
  rows the issue asked for (`presets.rs:1497-1498`);
- the named-key shift-walk fold in the resolver
  (`resolver.rs:73-93`), with the printable-fold limitation documented
  in the comment exactly as the issue records it;
- routing `router.rs:2133-2134` → `UiEffect::ExtendSelectionNext/Prev`
  (`router.rs:232-233`) and typed tests (`router.rs:3224-3247`,
  `flow_table_tests.rs:207`).

Nothing is left to build for #875. This chunk's work on it is
verification (the tests above must not have been weakened) plus an
evidence comment; the issue closes at the PR merge like the others.

### #1257 — the directional walk dead-ends at a focused text field

The composition chain: the editor keyscope's per-preset editor table
(`crates/taskboard-ui-slint/src/keyscope.rs`, `editor_table` at
`keyscope.rs:453`) lawfully consumes the cursor commands while a text
field holds focus — Emacs binds `Ctrl+b/f/p/n/a/e/k/d` etc.
(`keyscope.rs:463-475`); Standard's table is deliberately empty
(`keyscope.rs:458-460`); Vi rides the mode grammar instead. In-field,
the caret keys (`Left`/`Right`/`Up`/`Down` — the Standard walk axis)
are consumed natively by the focused `TextInput`. The walk axes
therefore cannot depart any field stop: Emacs loses `Ctrl+f/b/n/p`
(the K2 guard, plan
`.artifacts/plans/2026-11-22-task-edit-view-focus-plan.md` §1.2),
Standard loses the arrows. The only departure today is native Tab
(unbound in the insert chain), which journeys use — a chords-only user
is stranded on a field stop. The issue asks for a design pass, not a
quick patch; the departure stroke must (a) be consumed by no editor
table and by no `TextInput` caret behaviour, and (b) keep every
caret key's text meaning intact.

## 2. Decisions — the exact bindings

Collision evidence: every spelling below was grepped against all of
`presets.rs` at base; none appears in the same context, and the
intra-context conflict-free guard
`presets_are_intra_context_conflict_free` (`keymap/mod.rs:1274`) plus
the Q15 override-conflict machinery (`keymap/mod.rs:1814-1838`) fail
the build on any miss. All rows are additive: **no existing row
changes or is removed**.

### D1 — #1394: the plan-window box family

The cross-preset grammar "Shift along the walk axis acts on the
focused thing" already exists twice (`HabitMoveUp/Down` = Shift+Up/
Shift+Down on Today, `presets.rs:137-138`; the #875 extend twins on
Board, §1 above). The box reorder joins it; removal joins the `Delete`
family (`TaskDelete` = Delete on Board/Today, `presets.rs:457,477`).

| Verb | Standard | Emacs | Vi | Context |
|---|---|---|---|---|
| `PlanBoxEarlier` | `Shift+Up` | `Ctrl+Shift+p` | `K` | `PlanNextDay` |
| `PlanBoxLater` | `Shift+Down` | `Ctrl+Shift+n` | `J` | `PlanNextDay` |
| `PlanRemoveBox` | `Delete` | `Delete` | `Delete` | `PlanNextDay` |
| `PlanCommitPlace` | Return only (unchanged) | Return only | Return only | — |

Spelling notes: the Emacs twins mirror the Board extend spellings in
their context (`presets.rs:939-940`) — `Ctrl+Shift+n` is `BoardCreate`'s
chord only in the `Board` context (`presets.rs:499`), so the
`PlanNextDay` rows are free; the Vi `J`/`K` uppercase printables are
the Board precedent verbatim (`presets.rs:1497-1498`) — the plan
window's placement form fields ride `ViInsert` masking
(`actions.rs:43-47`), so in `ViNormal` (where these rows resolve) the
uppercase glyphs never edit text. `Delete` is free in `PlanNextDay`
across all presets. `PlanCommitPlace` keeps the armed-Return grammar
(audit §2 disposition upheld; a second address for one semantics is
what the O-8 law forbids).

### D2 — #1393: the review-walk stage verbs

All rows land in the shared `REVIEW_WALK_ROWS` block
(`presets.rs:257`) — one block, all three presets, same as today's
cursor rows. Every spelling is non-printable and absent from both
editor tables, so each is stage-proof beside the reflection field
(§1 fact 1):

| Verb | Stroke (all presets) | Why this one |
|---|---|---|
| `ReviewFileTargetPrev` | `Left` | the File-to picker's horizontal axis; free in `ReviewWalk` (no `Left`/`Right` rows there); meaningful exactly while the picker is armed on the cursor row, where no field holds focus |
| `ReviewFileTargetNext` | `Right` | the twin |
| `ReviewWalkPrev` | `Alt+Left` | the app-wide Alt+Left = "back" convention (`NavFocusBoard`, `presets.rs:417/897/1574`) applied inside the masking walk context where the Global row is unreachable (no collision); Alt is never caret, never printable |
| `ReviewWalkPlanner` | `Ctrl+Shift+p` | the planner mnemonic — the same spelling as `ViewPlanNextDay`'s Global row (`presets.rs:392`), unreachable in the walk (masked), so no collision; not an editor command, field-safe |
| `ReviewAbortArm` | `Alt+a` | free app-wide (no `Alt+a` row anywhere at base); not in either editor table; the arm verb joins the Alt family rather than stealing a caret key (`Ctrl+a` is Emacs `LineStart` in-field) |
| `ReviewAbortAccept` / `ReviewAbortDismiss` | Return / Escape (unchanged) | the armed-confirm grammar — the §2.7 modal satisfaction; ruling recorded |

### D3 — #1257: the field-departing walk axis

**The departure chord is `Alt+<direction>`** — a parallel walk axis
that no text input ever consumes:

- Rows: `NavFocusLeft/Right/Up/Down` = `Alt+Left/Right/Up/Down` in the
  six view contexts that carry directional rows today — `Today`,
  `Board`, `StackDetail`, `TaskDetail`, `Settings`, `PlanNextDay` —
  in **Standard and Emacs** both (the contexts' exact row sets are
  `presets.rs:408-409,443-467,529-549,546-549` Standard;
  `presets.rs:1378-1400` Emacs).
- Why it cannot fire during text editing: `Alt`-modified strokes
  appear in **no** editor table (the Emacs table binds only
  `Ctrl+*`, `Alt+w`, `Ctrl+Space` — `keyscope.rs:463-479`; Standard's
  is empty), Slint's `TextInput` caret behaviour consumes only bare
  caret keys and printables, and the resolver's shift-fold touches
  only shift-bearing *named* walk rows (`resolver.rs:73-93`). The
  caret navigation keys keep their text meaning untouched: no
  existing in-field behaviour changes.
- Emacs additionally gets the chord-shaped departure quartet —
  `Alt+n`/`Alt+p` = `NavFocusDown`/`NavFocusUp`, `Alt+f`/`Alt+b` =
  `NavFocusRight`/`NavFocusLeft`, same six contexts, Emacs preset
  only. All four spellings are free app-wide at base. Rationale: the
  issue's stranded user is the *chords-only* one; arrows alone would
  still force a hand off the home row. Emacs gaining named-arrow rows
  is a deliberate D8 amendment (the source-editor-authentic preset
  gains the Alt axis it never had) — flagged as ruling R3.
- Vi: **no rows**. The vi contract already documents the departure —
  `Escape` → `ModeNormal` is the insert-chain's exit
  (`presets.rs:1453-1456`), and vi users are never stranded. Ruling
  R3 covers this too.

### D4 — #875: no new code

§1 records the landed state. Stage 2's only obligations: confirm the
listed tests are green and unweakened, and post the evidence comment
on the issue. No `ActionId`, no rows, no tests.

## 3. UX-plan decision (the activation rule, cited)

**Rule**: `docs/ux_planning.org` §3 — the UX-journey mandate has been
**mandatory since 2026-11-16 (phase 6 activation)**: "every new
user-visible capability (any new verb, badge, view, or setting a human
can perceive or act on …) includes — or cites — an accepted UX journey
plan … before a dispatch plan may chunk the work" (AGENTS §7 restates
it). Every chord added here is a user-perceivable new capability (a
key a human can press to act); #875's capability already landed inside
round-9's accepted UX coverage. `docs/ux_planning.org` §2.7 (the
keyboard-first law, 2026-11-23) independently makes a chordless common
verb a defect to be fixed like a review finding — these four issues
are exactly that fix.

**Vehicle**: the owning keymap journey plan is
`.artifacts/plans/2026-11-16-keymap-parity-ux-plan.md` (ACCEPTED
2026-11-16; the round-8 keymap family's user stories, capability
matrix, and wiring inventory). Per the settings-honesty precedent
(2026-11-27, ruling R3: "the law is about the user-visible change, not
the plan's age"), the chunk **amends** that plan on the branch — new
rows in its keymap tables, new user stories — rather than spawning a
fourth keymap plan. Stage 2 applies the amendment verbatim from §4
below and runs `scripts/ux-coverage.sh <plan> --root <worktree>
--gate` to PASS on the claimed rows.

### §4 The amendment text (stage 2 applies verbatim to `2026-11-16-keymap-parity-ux-plan.md`)

Append to the plan (dated 2026-11-28, citing this file and issues
#1394/#1393/#1257):

- **§2 matrix rows** (append after the existing `nav.*` rows):
  `plan.box_reorder` | Board — | TaskDetail — | Today — | Review — |
  Settings — | **PlanNextDay: E (US-4)** | Capture — | CLI U; and
  `review.stage_verbs` | **Review: E (US-5)**, all other columns U/—;
  and `nav.field_departure` | E (US-6) on Board, Today, TaskDetail,
  Settings (the field-stop surfaces), — elsewhere.
- **New US-4** — the plan-window box family: reorder the focused
  placed box earlier/later and remove it from the keyboard in every
  preset (Standard Shift+Up/Shift+Down/Delete; Emacs
  Ctrl+Shift+p/Ctrl+Shift+n/Delete; Vi K/J/Delete); placement commit
  stays Return. Steps: open plan-next-day, begin place, commit a box
  (Return), Shift+Down (moves later), Shift+Up (moves earlier),
  Delete (removes). S-steps carry the typed outcomes: box order
  changes, box removed.
- **New US-5** — the review walk's stage verbs: inside the walk,
  Left/Right walk the armed File-to target, Alt+Left returns to the
  previous stage, Ctrl+Shift+p jumps to the planner, Alt+a arms the
  abort (Return/Escape remain the armed confirm/dismiss). Steps: open
  the walk, arm the File-to picker, Left/Right change the target,
  Alt+Left steps back a stage, Ctrl+Shift+p lands on the planner,
  Alt+a arms abort.
- **New US-6** — the field departure: from any focused text field on
  a directional surface, Alt+Right (Emacs: Alt+f too) departs the
  field along the walk without altering the field's text; caret keys
  keep their in-field meaning. Steps: focus the effort field,
  Alt+Right moves focus off the field, the field's text is unchanged,
  Left/Right in-field still move the caret.
- **§6 wiring rows** (append): presets.rs row blocks for the three
  families; resolver unchanged; router unchanged (all verbs route
  today).
- **§8 verification** (append): the new tests of this plan's §5
  catalogue, keyed US-4/US-5/US-6.

## 5. Invariants (stage 2 must hold, stage 3 must verify)

1. **No existing binding changes or is removed.** The only preset
   edits are *added* row tuples (and the `REVIEW_WALK_ROWS` /
   `PlanNextDay` blocks grow). The preset snapshots and
   `presets_are_intra_context_conflict_free` (`keymap/mod.rs:1274`)
   must stay green — if a collision fires, the spelling was wrong:
   stop and re-read §2, never delete a competing row (the C-5 rule).
2. **The text editing keys keep their meaning.** No editor-table
   change (`keyscope.rs` untouched); no caret key gains a walk
   meaning that fires in-field. The #1257 axis is Alt-only, proven
   in-field by T4.
3. **The keymap coverage inventory grows stricter.** The newly bound
   verbs join the per-context coverage assertions (new tests T1/T2);
   `every_preset_covers_the_core_verbs` (`keymap/mod.rs:575`) keeps
   its core set unchanged.
4. **Typed primitives only (AGENTS §2/§3)**: zero new `ActionId`,
   `StateCommand`, or `UiRequest` variants — every verb routes today
   (§1); the work is rows + tests. No `RwLock`/`Mutex`, no new
   channels.
5. **UI purity**: the UI layer gains no logic — the adapter/router
   arms already dispatch these ids; stage 2 touches neither
   `router.rs` nor the adapter except where a journey test needs a
   seam that already exists. **AMENDED 2026-11-28, stage 3** (the
   #1394 half-delivery): invariant 1's rows without a target mint
   shipped as dead keys — the box verbs' one-shot `plan_target`
   existed only at the pointer seams — and a dead key is worse than
   a slightly larger PR. The deviation is bounded to presentation
   routing in the UI crate: `Adapter::route` gained a cursor
   fallback mint for `PlanBoxEarlier`/`PlanBoxLater`/`PlanRemoveBox`
   (the `effective_habit`/`SeriesResume` precedent; no new
   `ActionId`/effect/domain type), and the US-4 journey drives the
   real chords. #1574's proposed mint is this mint — the PR closes
   it. The invariant's original text stands for everything else.
6. **#875 is not re-implemented**: any diff touching the extend twins
   beyond reading them is a finding.
7. **Dvorak/layout leniency is not addressed here** (the Alt+letter
   rows ride the parser's existing normalization; the AltGr
   limitation stays recorded where it is).

## 6. Test catalogue (lowest layer, AGENTS §5 — typed outcomes, no text assertions, no sleeps)

- **T1 — the plan-window box family resolves in every preset**
  (`keymap/mod.rs`, next to `the_detail_verb_table_folds_to_a_binding_in_every_preset`
  at `mod.rs:1586`): for each preset,
  `first_binding_display` for `PlanBoxEarlier`/`PlanBoxLater`/
  `PlanRemoveBox` over the `[PlanNextDay, Global]` chain is `Some`,
  and the resolved stroke equals the §2 D1 table (structural assert
  on the binding's canonical spelling — typed, not text-of-UI).
- **T2 — the review walk's stage verbs resolve in every preset**
  (`keymap/mod.rs`, pattern of `picker_context_chains_and_never_masks`
  at `mod.rs:988`): over `[ReviewWalk, Global]`, `ReviewFileTargetPrev/
  Next`, `ReviewWalkPrev`, `ReviewWalkPlanner`, `ReviewAbortArm`
  resolve in all three presets; `ReviewAbortAccept`/`Dismiss` stay
  Return/Escape-addressed (assert the two have no Alt/Ctrl row — the
  armed grammar pin, named `..._currently_pins_...`? No — this is the
  desired semantics, not tolerated debt).
- **T3 — the departure axis is field-proof, structurally**
  (`keymap/mod.rs` or `keyscope.rs` tests): (a) the Alt+direction
  rows resolve `NavFocus*` over each of the six view chains in
  Standard and Emacs; (b) `editor_table(Preset::Emacs)` and
  `editor_table(Preset::Standard)` contain **no** Alt-modified
  binding (typed: `exact()` returns `None` for each Alt+direction
  sequence) — the structural proof the departure cannot be consumed
  in-field.
- **T4 — the field-departure journey** (`tests/journeys.rs`): focus a
  detail text field with text in it, press Alt+Right; typed outcome:
  focus left the field (the focus facts/`EditingFacts` count drops)
  and the field's value is unchanged; then press Left/Right **in** the
  field and assert the caret moved (value still present, focus
  stayed) — the caret-keys-keep-their-meaning half. Name embeds
  `US-6`.
- **T5 — the box-family journey** (`tests/journeys.rs`, name embeds
  `US-4`): open plan-next-day, place a box, Shift+Down / Shift+Up /
  Delete; assert the placed-box order/removal through the typed view
  state (not UI text).
- **T6 — the review-walk journey** (`tests/journeys.rs`, name embeds
  `US-5`): arm the File-to picker, Left/Right change the typed file
  target; Alt+Left steps the stage back (typed `WalkStage`);
  Ctrl+Shift+p lands the planner intent; Alt+a arms the abort (typed
  armed flag). No assertion on any rendered string.
- **Regression-oracle tests that must NOT be weakened**: T11/T12
  override tests (`mod.rs:1770-1860`),
  `presets_are_intra_context_conflict_free` (`mod.rs:1274`),
  `every_preset_covers_the_core_verbs` (`mod.rs:575`),
  `vi_preset_shape_matches_the_mode_model` (`mod.rs:1453`),
  `presets_mask_only_the_modal_contexts` (`mod.rs:1517`), the #875
  typed tests (`router.rs:3224-3247`, `flow_table_tests.rs:207`),
  the editor-table tests (`keyscope.rs:1363-1425`).

## 7. Verification commands (expected outcomes)

```sh
scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint keymap     # T1-T3 + the untouched collision/coverage pins
scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint --test journeys   # T4-T6 + the existing journeys
scripts/memory-gate.sh cargo clippy -p taskboard-ui-slint --all-targets -- -D warnings
scripts/ux-coverage.sh .artifacts/plans/2026-11-16-keymap-parity-ux-plan.md --root /home/viktoria/src/taskboard-WT-feat-ui-keyboard-coverage --gate   # PASS on US-4/US-5/US-6
scripts/run-quality-gates.sh cargo nextest run --workspace                # the stage gate (flock-safe)
```

The full AGENTS §10 trio is the **dispatcher's** between stage 2 and
stage 3, never inside a subagent (discipline item 1).

## 8. Diff-shape rules

- Allowed: `crates/taskboard-ui-slint/src/keymap/presets.rs` (added
  rows + comments stating the context-scoping constraints),
  `crates/taskboard-ui-slint/src/keymap/mod.rs` (T1-T3 tests only),
  `crates/taskboard-ui-slint/tests/journeys.rs` + `tests/support/`
  (T4-T6), the amended `.artifacts/plans/2026-11-16-keymap-parity-ux-plan.md`,
  this plan's companions.
- Not allowed: `keyscope.rs` editor tables, `resolver.rs`,
  `router.rs`, `actions.rs` (no new ids), `stroke.rs`, any
  non-UI-slint crate, `Cargo.toml`, slint markup, snapshot goldens
  (the preset snapshot tests must pass unchanged — added rows land in
  snapshots only if the snapshot is generated data; if a preset
  snapshot pins row counts, growing it is part of the change and is
  named in the commit), any weakening of §6's oracle list.
- Commit shape: standalone commits — `feat(ui): the plan-window box
  family gains chords (#1394)`, `feat(ui): the review-walk stage verbs
  gain chords (#1393)`, `feat(ui): an Alt walk axis departs focused
  text fields (#1257)`, `docs(plan): the keymap-parity journey
  amendment (US-4/US-5/US-6)`; no WIP noise (rebase-merge hygiene).

## 9. Chunk split

**One chunk** (branch `feat/ui-keyboard-coverage`, size **M**): all
four issues live in the same two-file family (preset tables + keymap
tests + journeys), share the collision-grammar review, and one PR
keeps the keymap invariant review single. Splitting #1257 out would
double the chord-space review for no shippable independence. Estimated
delta: ~25 added preset rows, ~6 new tests, one plan amendment —
roughly 300–450 LOC including tests.

## 10. Rulings requested (proceeding on the defaults)

- **R1 — the #1394 spellings (D1)**: default Shift+Up/Down ·
  Ctrl+Shift+n/p · J/K, Delete for removal, Return-only commit.
  Alternatives: Alt+Up/Down for the reorder twins (free app-wide,
  keeps Shift free for a future selection-extend on boxes); a dedicated
  Ctrl+Shift+Delete for removal. Operator chord preferences welcome;
  a change is a table re-spell.
- **R2 — the #1393 spellings (D2)**: default Left/Right · Alt+Left ·
  Ctrl+Shift+p · Alt+a. Alternatives: `p`/`b` printables for
  Planner/Prev (rejected by default: the walk's Prompts stage has a
  text field — printables are not stage-proof); Ctrl+Left/Right for
  the file-target twins (rejected by default: Ctrl+Left/Right is
  word-caret in fields, a meaning we must not teach the walk).
- **R3 — the #1257 departure (D3)**: default Alt+directions in
  Standard+Emacs **plus** the Emacs Alt+f/b/n/p quartet, vi excluded
  (Escape contract). Alternatives: (a) arrows-only axis, emacs
  arrows but no Alt quartet — smaller diff, leaves the chords-only
  emacs user hand-shifting; (b) the documented verbs-exit-only
  contract with a hint (the issue's option B) — no new rows at all,
  Tab remains the only departure; (c) the parallel-axis retune inside
  emacs only. Default chosen because it fixes both the arrow and the
  chord user with zero in-field behaviour change.
- **R4 — #875 disposition (D4)**: default already-landed → evidence
  comment, closes at merge. Alternative: re-open work if the operator
  wants extend parity on more surfaces (the rows exist Board-only) —
  that is a new issue, not this chunk.
- **R5 — the UX amendment vehicle (§3)**: default amend
  `2026-11-16-keymap-parity-ux-plan.md`. Alternative: a fresh tiny
  `2026-11-28-keybinding-coverage-ux-plan.md`. Default keeps one
  owning keymap journey plan; split only if the operator prefers
  plan-per-round hygiene.

## 11. Prompt 2 (stage 2 — IMPLEMENTATION) — paste-ready, verbatim

```text
You are the stage-2 IMPLEMENTATION agent for the chunk "keybinding coverage" (branch feat/ui-keyboard-coverage) in the taskboard repo, working in /home/viktoria/src/taskboard-WT-feat-ui-keyboard-coverage (work ONLY inside it, by absolute path; never touch /home/viktoria/src/taskboard or any other taskboard-WT-* directory — other agents own them). If the worktree is missing, recreate it: git -C /home/viktoria/src/taskboard worktree add -b feat/ui-keyboard-coverage ../taskboard-WT-feat-ui-keyboard-coverage origin/main — but if the branch's PR already landed, use feat/ui-keyboard-coverage-v2 off fetched origin/main instead (AGENTS §8; never git worktree add <path> main).

**Long-command discipline (binding — docs/agent_dispatch.org; restate it verbatim)**: a subagent is killed after ~600s without observable activity. (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate this discipline verbatim.* **Never commit with `--no-verify` or any hook bypass.** Never let a test or binary you run touch the user's real taskboard data (isolated config/data dir under /var/tmp; the test helpers plant `TASKBOARD_TEST_SANDBOX`). **The required SonarCloud quality gate fails a PR whose NEW code has more than 3 % duplicated lines** — drive the new preset rows and their tests from the shared row blocks and the existing test helpers, never copy-paste.

1. Read AGENTS.md, the stage-1 plan .artifacts/plans/2026-11-28-ui-keyboard-coverage-plan.md (normative: §2 decisions D1-D4, §3/§4 the UX amendment you must apply verbatim, §5 invariants, §6 test catalogue, §8 diff-shape rules), the origin issues (#1394, #1393, #875, #1257 with their comments), the audit .artifacts/reports/2026-11-25-keybinding-coverage-audit.md, and the journey plan you will amend: .artifacts/plans/2026-11-16-keymap-parity-ux-plan.md.
2. Implement exactly per the plan, TDD, every commit standalone per §8's commit shape. Concretely: (a) #1394 — append to crates/taskboard-ui-slint/src/keymap/presets.rs the PlanNextDay rows of plan table D1 (Standard block: Shift+Up/Shift+Down/Delete; Emacs block: Ctrl+Shift+p/Ctrl+Shift+n/Delete; Vi block: K/J/Delete) for PlanBoxEarlier/PlanBoxLater/PlanRemoveBox; PlanCommitPlace gets NO row (armed Return stands). (b) #1393 — extend the shared REVIEW_WALK_ROWS block (presets.rs:257) with Left/Right on ReviewFileTargetPrev/Next, Alt+Left on ReviewWalkPrev, Ctrl+Shift+p on ReviewWalkPlanner, Alt+a on ReviewAbortArm; ReviewAbortAccept/Dismiss get NO row. (c) #1257 — Alt+Left/Right/Up/Down NavFocus rows in the six view contexts (Today, Board, StackDetail, TaskDetail, Settings, PlanNextDay) in BOTH the Standard and Emacs preset blocks, plus the Emacs-only Alt+n/Alt+p (NavFocusDown/Up) and Alt+f/Alt+b (NavFocusRight/Left) rows in the same six contexts; Vi gets nothing. Every added row carries a one-line comment stating its constraint (masking context, field-proof spelling, or the shift-grammar family it joins). Touch NOTHING else in presets.rs — no existing row changes or moves (invariant §5.1).
3. Apply the UX amendment verbatim from plan §4 to .artifacts/plans/2026-11-16-keymap-parity-ux-plan.md (matrix rows, US-4/US-5/US-6 stories with steps, wiring rows, verification rows — all dated 2026-11-28 citing the stage-1 plan and the issues).
4. Tests per plan §6: T1/T2/T3 as keymap table tests (typed first_binding_display / editor_table assertions — no text assertions), T4/T5/T6 as journey tests in crates/taskboard-ui-slint/tests/journeys.rs named embedding US-6/US-4/US-5 respectively, asserting typed outcomes (focus facts, view-state box order, typed WalkStage, armed flag) — never rendered text, never sleeps. Do NOT weaken the oracle list in plan §6 (the override/conflict/coverage/vi-shape/masks tests, the #875 typed tests, the editor-table tests) — they are the regression proof that this chunk is purely additive.
5. #875 ships no code: verify the extend-twin tests are green (plan §1 lists them) and post ONE comment on issue #875 with the landed evidence (the preset rows and test paths, branch feat/ui-keyboard-coverage) — issues are always permitted (AGENTS §8); "no push" governs git refs only.
6. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-16-keymap-parity-ux-plan.md --root /home/viktoria/src/taskboard-WT-feat-ui-keyboard-coverage --gate and make it PASS on US-4/US-5/US-6 — MISSING on a claimed row is not done. Then the stage gate: scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; the flock-deferral rule above applies). Every cargo invocation goes through scripts/memory-gate.sh.
7. SPDX headers on any new .rs file (AGPL-3.0-only for taskboard-ui-slint); #![forbid(unsafe_code)] is crate-level already. Respect AGENTS §2 boundaries: UI crate only, presentation-client discipline, no new ActionId/StateCommand/UiRequest.
8. No push, no PR — shipping is stage 3's last task. Never push to or merge main.
9. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — list the issue numbers in your report. Search first (gh issue list --search) and comment on #1394/#1393/#1257 rather than duplicating when the finding is about their scope.

Report: per-gate results, the amended journey rows, the #875 comment link, any deviation from the plan and why.
```

## 12. Prompt 3 (stage 3 — REVIEW) — paste-ready, verbatim

```text
You are the stage-3 REVIEW agent for the chunk "keybinding coverage" (branch feat/ui-keyboard-coverage), working in /home/viktoria/src/taskboard-WT-feat-ui-keyboard-coverage (work ONLY inside it; never touch /home/viktoria/src/taskboard or any other taskboard-WT-* directory).

**Long-command discipline (binding — docs/agent_dispatch.org; restate it verbatim)**: a subagent is killed after ~600s without observable activity. (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate this discipline verbatim.* **Never commit with `--no-verify` or any hook bypass.** Never let a test or binary you run touch the user's real taskboard data (isolated config/data dir under /var/tmp; the test helpers plant `TASKBOARD_TEST_SANDBOX`). **The required SonarCloud quality gate fails a PR whose NEW code has more than 3 % duplicated lines** — flag any copy-pasted row blocks or test bodies for table-driven reuse.

1. Begin by rebasing onto main: git fetch origin --prune, rebase the branch onto origin/main (AGENTS §8's rebase dance: drop already-merged duplicate commits; resolve only real conflicts), push --force-with-lease if the rebase rewrote history. Then read AGENTS.md, the stage-1 plan .artifacts/plans/2026-11-28-ui-keyboard-coverage-plan.md (§5 invariants and §8 diff-shape rules are your acceptance criteria), the origin issues (#1394, #1393, #875, #1257), the audit .artifacts/reports/2026-11-25-keybinding-coverage-audit.md, and the amended .artifacts/plans/2026-11-16-keymap-parity-ux-plan.md; then review the full branch diff versus the post-rebase base (git diff origin/main...HEAD).
2. Functional focus (this chunk's specific traps): (a) purely additive — every presets.rs diff line is an ADDED row; any modified or removed existing row is a BLOCKER (invariant §5.1, the O-8 law). (b) collision discipline — presets_are_intra_context_conflict_free (keymap/mod.rs:1274) and the Q15 override tests (mod.rs:1814-1838) must be green without any preset row removed to make them so; if a collision was "resolved" by deleting or moving a row, that is a BLOCKER (the C-5 rule moves the spelling, never the competitor). (c) the armed grammars keep their monopoly — PlanCommitPlace and ReviewAbortAccept/Dismiss must have NO chord rows anywhere. (d) #1257's field-proof property — keyscope.rs is untouched, no editor table gained an Alt entry (structurally asserted by T3), and the T4 journey asserts BOTH the Alt departure AND that bare Left/Right keep caret meaning in-field; a journey asserting only the departure is incomplete. (e) vi exclusion — no #1257 rows in the vi blocks; the vi Escape contract stands. (f) the tests that must not have been weakened (stage-1 plan §6 oracle list): every_preset_covers_the_core_verbs, presets_are_intra_context_conflict_free, vi_preset_shape_matches_the_mode_model, presets_mask_only_the_modal_contexts, the T11/T12 override tests, the #875 typed tests (router.rs:3224-3247, flow_table_tests.rs:207), the editor-table tests (keyscope.rs:1363-1425) — verify none was relaxed, re-scoped, or asserted weaker. (g) no text assertions in the new tests (typed outcomes and structural facts only), no sleeps, no wall-clock dependence.
3. **Code-quality mandate** (stage 3): beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — the UI is a presentation client that dispatches typed events; (c) a single level of abstraction per function body. Findings are fixed on the branch.
4. Re-run the stage gate yourself: scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; the flock-deferral rule above applies), plus scripts/memory-gate.sh cargo clippy --workspace --all-targets -- -D warnings and scripts/memory-gate.sh cargo fmt --all -- --check; any failure is a finding. Re-run scripts/ux-coverage.sh .artifacts/plans/2026-11-16-keymap-parity-ux-plan.md --root /home/viktoria/src/taskboard-WT-feat-ui-keyboard-coverage --gate; MISSING on US-4/US-5/US-6 is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups; every commit stands alone for the rebase merge).
6. Ship — your scope ENDS at PR-open: push the branch, open the PR to main with gh (body: closes #1394, closes #1393, closes #875, closes #1257; summary of D1-D4 and the R rulings taken), and report the URL. Do NOT run gh pr checks --watch, do NOT verify mergeStateStatus, do NOT remove the worktree — the dispatcher owns the CI watch, the CLEAN verification, teardown, and at-merge issue closures. main is branch-protected: never merge; the user merges on GitHub.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report. Search first and comment rather than duplicating.

Report: verdict, findings (functional + mandate), what you fixed, PR URL.
```

## 13. Completion bookkeeping

- The PR body closes #1394, #1393, #875, #1257 at merge (the
  dispatcher owns the at-merge closures; #875 additionally carries the
  stage-2 evidence comment).
- The journey-plan amendment lands on this branch — the artefact is
  versioned with the code it describes.
- Post-merge: the audit's remaining gap families (#1395 staged commit
  confirms, #1396 panel-only verbs) stay open in the unscheduled queue
  (no milestone — AGENTS §7).

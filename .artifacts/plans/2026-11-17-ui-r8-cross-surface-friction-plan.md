# [Plan] ui-r8 chunk 8 — cross-surface friction & routing repairs

- **Date**: 2026-11-17
- **Kind**: stage-1 chunk plan (implementation spec) — chunk 8 of the
  ux-r8 dispatch, branch `fix/ui-r8-cross-surface-friction`.
- **Origin**: the accepted UX plan
  `.artifacts/plans/2026-11-16-cross-surface-friction-ux-plan.md`
  (all sections normative; delta-free §7) and the dispatch plan
  `.artifacts/plans/2026-11-16-gui-review-round8-dispatch-plan.md`
  §1/§9. Issues closed at merge: #753 #832 #836 #772 #763 #712.
- **Status**: READY for stage 2 (TDD implementation per §9's
  sequencing).
- **Seam baseline**: read on merged `main` at `6911b89f` (chunks 4/5
  landed: the final Dropdown grammar with the F14 empty-rows disabled
  shape, the quick bar, the `MenuKind` discriminator + withheld-filter
  machinery, the selection VM + board grammar, the `on_slint_thread`
  journey-test discipline).
- **Gate baseline** (stage-1 run of
  `scripts/ux-coverage.sh <origin plan> --gate`): 33 checks, 19
  FOUND, 14 MISSING. Every MISSING is a token this chunk must create
  (the §6 test names — the reproductions are the delta's
  proof-of-need — and the US-5/US-6 step tokens); none is a
  plan-vs-code defect in the origin's claims. Stage 2 embeds the
  tokens verbatim (test names + step IDs) and re-runs the gate to
  all-FOUND.

## 1. Decisions

### D-1 — The empty-model guard is landed; the chunk adds the proof (#753, US-1, W-1)

**Finding**: the guard already exists on merged main, per open path:
`dropdown.slint`'s adapter-driven `open-requested()` arm
(`if (root.rows.length > 0) { root.open = true; }`), the field
`TouchArea`'s `enabled: root.field-enabled && root.rows.length > 0`
(a disabled click target — the click path is unreachable by
construction at zero rows), and the a11y open arm's identical gate.
`DropdownFacts.open` only ever flips true through those arms, so the
window-level `DropdownFlyout` never mounts on a zero-row model. The
round-5 F14/D6 flyout conversion landed it; chunk 5's grammar kept it.

**Decision**: no new guard code. The chunk's W-1 delta is the journey
test `us1_ac1_zero_row_open_mounts_nothing` (§8) on a zero-board rig
wrapped in `on_slint_thread`, asserting the typed mount fact
(`DropdownFacts.open` stays false / the flyout's mount inventory stays
empty — never text), plus the census evidence extension (§10). AC-2
(the populated path unchanged) is the existing dropdown probe tests
standing; if stage 2 finds any open path that bypasses the guard
(none known), the guard's `rows.length > 0` branch is the one line to
add there — same shape, no empty-state row (the origin's AC-3
rejection stands).

### D-2 — The conversion arms route to the real seams, subject-resolved (#832, US-2, W-2)

**Finding**: `adapter.rs`'s effect-interpret arm
`(_, UiEffect::HabitConvertArm | UiEffect::SeriesConvertArm) =>
Vec::new()` is unconditional; `ActionId::HabitConvertArm` /
`SeriesConvertArm` dispatch to `UiEffect` (router.rs 910/952) and die.
The pointer seams work: `on_habits_convert_armed` (index →
`tables.habits` → `habit_convert_armed` + `habit_convert_target` +
`pattern_draft` default → `set_habits_convert_armed_row`) and
`on_task_recur_convert_asked` (`subject_task()` → rule check →
`series_target` + `convert_target` default + `recur_confirm_armed`).
`is_habit` is derivationally `false` in `task_menu_subject`
(presentation.rs, the recorded deviation), so the card menus never
offer the habit children; the GLOBAL menu rows (ranks 200..270) are
the menu route.

**Decision — subject resolution**:

- `SeriesConvertArm`: the effect arm resolves
  `view_state.subject_task()` (the focused/viewed task), verifies the
  live task carries a `recur` rule, then arms identically to
  `on_task_recur_convert_asked` (`series_target`, `convert_target` =
  `convert_target_default(&rule.pattern)`, `recur_confirm_armed =
  true`). No subject or no rule → the typed `NoTarget` surfacing
  (D-3). Kiosk already inert? No: these arms are not kiosk-gated —
  the arming flags are desktop strips; add the `if self.kiosk =>
  Vec::new()` guard to both arms, the entries-overlay precedent
  (the origin's R-2: no kiosk leg — the kiosk dispatch must stay
  structurally inert, and today a menu-routed arm would flip desktop
  flags invisible on kiosk; the guard makes it a typed no-op).
- `HabitConvertArm`: **a habit focus fact does not exist** — the
  habit section is pointer-only (row index via `IdTables.habits`;
  `view_state.rs` has zero habit awareness). The chunk adds the
  minimal focus fact, mirroring the `focused_task` precedent:
  - adapter field `focused_habit: Option<HabitId>`;
  - one new slint seam: the habit row's card gains
    `row-activated(index)` → `on_habits_row_activated`, which mints
    `focused_habit` from `tables.habits` (stale index absorbed, the
    notification-row guard shape). The seam carries the
    `// slint: habit-row-activated` comment (#782 law);
    `tests/seam_inventory.rs` gains the row (#760's stage-3
    cross-check).
  - the existing habit pointer seams (`plus_clicked`,
    `undo_log_clicked`, `convert_armed`, `repair_clicked`) also
    refresh `focused_habit` — click ≡ key composes both directions,
    and the row button's own arm stays byte-identical.
  - the effect arm resolves `focused_habit`, verifies the habit is
    live, reverse-resolves its row index through `tables.habits`
    (absent index → `NoTarget` surfacing; the strip flag needs the
    index for `habit_convert_armed`), then arms identically to the
    row seam.

**Decision — NoTarget surfacing (D-3)**: route()'s
`DispatchOutcome::NoTarget | Disabled => {}` arm is silent by design
for verbs; the menu-invoked convert arms surface the refusal on the
status-notice lane, the T-19 twin: a new
`arm_no_target_notice(&self, id: ActionId)` arming
`self.notice` from the action's registry description (the
disabled-row notice's shape, `MENU_NOTICE_HORIZON`). Tests assert the
structural fact (`adapter.notice.is_some()`), never text (AGENTS §5).
The global silent-`NoTarget` arm is NOT changed surface-wide
(backlog, §11).

**Decision — the arming identicality guarantee (AC-1)**: the effect
arm and the row seam converge on one private helper
`arm_habit_convert(&mut self, window, habit: HabitId)` /
`arm_series_convert(&mut self, window, task: TaskId)`; both callers
route through it. The strip flags, one-shots, and window echoes are
written in exactly one place each.

### D-4 — The disarm seam is the close path plus the mount boundary (#836, US-3, W-3)

**Finding**: `settings.armed_item` (adapter VM, not a `Flow`) disarms
on Cancel/displacement/busy/vanished-subject/outcome-render
(UX R-2 D-4's pass rules at adapter.rs ~6917) but survives
`close_settings` (view_state.rs 1303 — generic over-view truncate,
adapter-blind).

**Decision**: disarm at BOTH boundaries, each one line:
1. the close path — after `view_state.close_settings()` in both
   adapter callers (the route arm ~4143 and the seam ~10580),
   `self.settings_disarm_item()`; this is AC-1's letter (the seam
   test closes and asserts the adapter flag cleared);
2. the mount boundary — `settings_view_mounted` also clears
   `armed_item`, the `scale_draft` lifetime precedent (r3 D20a:
   surface-bound state dies at the mount). This catches every exit
   the two close sites miss (a nav root swap truncating a pushed
   Settings level, kiosk rotation) because the strip is only visible
   while mounted.

Interaction with the r2 grammar: none — the arm/confirm/displace
rules are untouched (AC-3 is the r2 tests standing); the disarm adds
two lifecycle cells to an existing flag's lifetime, no new state.

### D-5 — The alias is a clap alias; the cross-reference lives in the palette's empty state (#772, US-4, W-4)

**Decision**: `Commands::Find` gains `#[command(alias = "search")]`
(non-breaking; `find` stays primary; clap resolves the alias to the
same variant — AC-1 is a `Cli::try_parse_from` equality test in
`crates/taskboard-app/src/cli.rs`'s test set:
`us4_ac1_search_alias_matches_find` asserting the parsed variant is
`Find` with the same fields — a typed outcome, not help-text).
`find`'s own refusals ride along (the alias carries the variant's
arg validation verbatim).

**Decision — the palette cross-reference's home**: the palette's
zero-hit/empty-query state (the `empty-state` caption slot in the
palette composition) gains one presentation line naming the CLI twin
(`taskboard find`). It is judged at walkthrough (the origin's Manual
row), never asserted by tests; the census `palette.search` CLI-twin
cell records the admitted pair (`find` ⇄ `search`, one ranking) and
is AC-2's evidence. No registry/action change (§7 delta-free holds).

### D-6 — The per-action consumption set is a router-owned declaration (#763, US-5, W-5)

**Finding**: `route()` (adapter.rs 1334) moves ~20 one-shot fields
into `DispatchCtx` via `.take()` on every dispatch; the router arm
that matches the dispatched `ActionId` reads at most a few; the rest
are consumed into the ctx and dropped with it — any interleaved
dispatch between arm and confirm eats the target.

**Decision — representation**: a router-owned, hand-rolled (A1 — no
derive crates) closed set:

```rust
// router.rs
/// The one-shot ctx fields a dispatched action may consume
/// (the per-action consumption set, #763). Hand-rolled closed
/// set: a new ctx one-shot that forgets its bit here fails the
/// `one_shot_bits_are_total` test (below), not production.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct OneShotSet(u32); // one bit per field, private
impl OneShotSet { // const fns: with_create_target(...), ... }
pub(crate) fn route_consumes(id: ActionId) -> OneShotSet;
```

`route_consumes` is a pure `match id { ... }` naming exactly what
each arm's body reads (`series_target`+`convert_target` for
`SeriesConvertToHabit`; `habit_convert_target`+`pattern_draft` for
`HabitConvertToSeries`; `recur_edit_target`+`recur_draft_rule` for
the recur editor funnel; `habit_target` for `HabitLog`; ... the full
table is stage-2 work derived from the dispatch arms' field reads —
**the existing router tests and armed-journey tests are the oracle**:
every currently-passing pair must pass unchanged, AC-2). The
non-one-shot facts (`reminder_target`, `board_target`, `rule_target`,
`rule_add_when` — the take-by-assignment group at route 1348-1355)
join the set: same law, they are consumables too.

**route() becomes**: `let wants = ActionRouter::route_consumes(id);`
then each field moves into the ctx only when its bit is set
(`if wants.create_target() { ctx.create_target = self.create_target.take(); }`
— the undeclared field stays in the adapter untouched). A totalness
unit test walks every field key × a representative action set and
pins the bit/field correspondence (structural, typed).

**Decision — the reconciliation law (AC-3)**: consumption is not
ownership. A one-shot left in the adapter after its owner flow ended
is stale; the adapter clears flow-owned one-shots when their flow
level retires. Implementation: a small `fn reconcile_one_shots(&mut
self, retired: &[Level])` called by the adapter's `drain_retired`
consumer — a per-`Flow`-kind ownership table (`EditRecur{..}` →
`recur_edit_target`, `recur_draft_rule`; `TrackEntryForm{..}` →
`track_entry_draft`, `track_entry_target`; `EditDate{..}` →
`date_edit_target`-class fields as they exist; the walk/picker
children own none). Armed-strip one-shots (`habit_convert_target`,
`series_target`, `convert_target`, `pattern_draft`) are NOT
flow-owned — their law is the arm-seams' re-arm-before-every-route
(defense-in-depth, unchanged) — and the armed flags themselves
disarm via their existing Cancel/settle paths. The seam test
`us5_ac3_stale_one_shot_clears_at_flow_end` (§8) pins: arm the recur
editor, foreign-settle it, assert the adapter's `recur_edit_target`
is `None` (structural).

**The settle laws are untouched**: a foreign settle still disarms a
strip and ends its level (the origin's US-5/S1 escape column); the
fix governs target *consumption* only. Stage 3's trap check: no
over-broad set (recreates take-all), no under-broad set (breaks an
armed pair).

### D-7 — The Replace pre-pass is settle_foreign with a TEXT mask (#712, US-6, W-6)

**Finding**: `begin_capture` (view_state.rs 2497) is
`write_flow(Flow::Capture)` — the XOR replace kills only the TOP
level; with `ReviewWalk` + a `WalkFilePick`/`WalkDatePick` child, the
child dies and the walk survives beneath (the resolver port's
recorded deviation). `settle_foreign` (view_state.rs 3385) is the
D-H4 innermost-first fold, each level by `settle_by_rule`.

**Decision**: `begin_capture` gains the chain-aware pre-pass:

```text
while a flow level is live:
    if the top level's kind is TEXT -> discard_edit()   # the mask
    else                            -> settle_by_rule(&top, state)
then write_flow(Flow::Capture)  (capture at depth 1)
```

- **Fold order**: innermost-first (top level first), D-H4's letter;
  the loop is the `settle_foreign` shape with its idempotence guard
  (if a rule fails to end its level, `end_top_flow` + retire keeps
  the fold total — the `Surfaces::settle_fold` contract).
- **The TEXT mask's precise boundary**: the mask covers exactly the
  buffer-commit family in `settle_by_rule` — `Flow::Create`,
  `CreateStack`, `CreateBoard`, `EditTitle{..}`, `RenameStack{..}`,
  `RenameBoard{..}`, `EditDescription{..}`. For those kinds the
  pre-pass runs `discard_edit()` (never `commit_edit` /
  `commit_description`), regardless of buffer content: the capture
  fire-drill's replace-without-committing semantics, preserved
  verbatim for the top level (today's XOR) and extended to deeper
  TEXT levels. Every other kind takes its own `settle_by_rule` rule:
  the walk (`close_review_walk` — the commandless close), the pickers
  (`GuestExit::Cancelled` ends), move/drag (cancel), palette/focus/
  entries/labels (close shapes), armed strips (cancel without
  dispatch), date/recur editors (clean close, never commit).
- Non-flow levels (over-views, the settings surface) are untouched —
  the pre-pass folds flow levels only; capture over a pushed Settings
  view keeps the view level and today's behavior.

AC-2 (capture over a live TEXT title editor still discards) is the
existing capture-over-flow tests standing; `us6_ac1` and `us6_ac3`
(§8) pin the new fold.

## 2. Test catalogue (the §8 deliverables; typed outcomes only)

All journey tests that mount a rig or touch `window()` wrap in
`on_slint_thread(move || { .. })` (two chunk-5 CI failures' law).
Time-dependent fixtures use `tokio::time::pause()`/`advance()` — none
of these tests need sleeps; interleavings are scripted call
sequences. No text assertions anywhere; notices/flags/indices/depths
are the asserted facts.

| Test | Home | Design |
|---|---|---|
| `us1_ac1_zero_row_open_mounts_nothing` | `tests/journeys.rs` | zero-board rig on the slint thread; fire the switcher's open path (`open-requested` / the field click probe); assert `DropdownFacts.open == false` (the flyout mount fact). |
| `us1_ac2` (guard regression) | existing `tests/dropdown_probe_tests.rs` | stands unchanged. |
| `us2_ac1_menu_convert_arm_arms_the_row_strip` | `tests/journeys.rs` | **reproduction, lands first and fails first** (pre-fix the effect arm returns `Vec::new()`): seed a habit + focus fact via `on_habits_row_activated`; `route(HabitConvertArm)`; assert `habit_convert_armed == Some(idx)` and the window echo row index. |
| `us2_ac2` (series twin) | `tests/journeys.rs` | focused series task; `route(SeriesConvertArm)`; assert `recur_confirm_armed` + `convert_target` is the rule-derived default. |
| `us2_ac3_subjectless_convert_surfaces_no_target` | adapter seam tests | no focus fact; route both ids; assert `notice.is_some()` (the typed surfacing) and no flags set. |
| `us2_ac4` (confirm composes) | rides `us5_ac1`'s script | the convert confirm after the interleaving routes the conversion command. |
| `us3_ac1_leaving_settings_disarms_the_strip` | `tests/journeys.rs` | arm an item (`settings_arm_item`), run the settings close route/seam, assert `settings.armed_item.is_none()`; re-enter (`settings_view_mounted`) and assert no armed row echoes. |
| `us3_ac3` (r2 grammar stands) | existing r2 armed tests | stand unchanged. |
| `us4_ac1_search_alias_matches_find` | `crates/taskboard-app/src/cli.rs` tests | `Cli::try_parse_from(["taskboard","search","needle"])` yields the `Find` variant with identical fields (typed enum equality). |
| `us5_ac1_interleaved_dispatch_spares_the_confirm_target` | `tests/journeys.rs` | **reproduction, lands first and fails first** (pre-fix the take-all eats the target): arm the habit convert (row seam), then interleave an unrelated route (`TaskComplete` over a focused task), then confirm; pre-fix the confirm is `NoTarget`-silent, post-fix it dispatches `ConvertHabitToTask`. Deterministic call sequence, no timers. |
| `us5_ac2` (armed pairs unchanged) | existing router + armed-journey tests | the oracle for `route_consumes`' table. |
| `us5_ac3_stale_one_shot_clears_at_flow_end` | adapter seam tests | arm the recur editor (target set), foreign-settle the level, drain; assert the adapter's `recur_edit_target.is_none()`. |
| `us6_ac1_capture_over_a_walk_child_closes_the_walk` | `tests/journeys.rs` | `arm_flow(ReviewWalk)` + `arm_child_for_test(WalkFilePick)`; `begin_capture()`; assert chain depth == 1 and the top level is `Capture` (no walk/picker level survives). |
| `us6_ac2` (TEXT mask) | existing capture-over-flow tests + one new case | a non-empty TEXT title editor under `begin_capture` discards (no commit command in the outcomes). |
| `us6_ac3_capture_dismiss_restores_the_parent_view` | `tests/journeys.rs` | after `us6_ac1`'s fold, `close_capture()`; assert the root view is the walk's parent (unchanged root, zero flow levels, no phantom walk). |

## 3. Risks

- **`route_consumes` under-declaring** breaks an armed pair; the
  existing armed-journey/router tests are the gate (AC-2), and the
  totalness pin catches structural drift. Highest-risk verbs: the
  recur editor funnel (`recur_edit_target` + `recur_draft_rule`
  together) and the habit twins.
- **The `focused_habit` fact** is the chunk's only genuinely new
  state; if stage 2 finds the reverse index lookup
  (habit → row) fragile under live-state churn, the fallback is the
  `NoTarget` surfacing (the strip arms only from a resolvable row) —
  typed, never a panic.
- **The pre-pass fold** must not settle non-flow levels; the chain
  guards (`above_levels` flow filter) are already the
  `chain_has_flow` shape. The `us6` tests pin depth exactly.
- **Gate tokens**: the §2 names must land verbatim (test fn names +
  the `usN_acM_*` step tokens) or the coverage gate reports MISSING.

## 4. Commit sequencing (stage 2; every commit standalone; census cells ride their code commits per §9 of the origin)

1. `test(ui): the menu-routed convert arms are silent no-ops` — the
   `us2` reproductions failing (US-2/AC-1).
2. `fix(ui): the conversion-arm effects route the real arming seams` —
   D-2/D-3 (+ the habit focus seam, `// slint:`, seam inventory row);
   census cell #832.
3. `test(ui): an interleaved dispatch eats a confirm's target` — the
   `us5` reproduction failing (US-5/AC-1).
4. `fix(ui): route consumes only the dispatched action's one-shots` —
   D-6 (`OneShotSet`, `route_consumes`, reconcile); census #763.
5. `test(ui): the empty-model dropdown guard's mount fact` +
   `fix`-none — `us1_ac1` + census #753 (evidence extension; no code
   change expected — the commit is the test + census note, typed as
   `test(ui)`).
6. `fix(ui): leaving settings disarms the armed strip` — D-4;
   census #836.
7. `fix(ui): capture's Replace settles the chain innermost-first, TEXT-masked` —
   D-7; census #712.
8. `feat(cli): the find subcommand gains the search alias` — D-5 +
   the palette cross-reference + census #772.

Stage 3: rebase, gate all-FOUND, walkthrough protocol, PR
`fix(ui): routing and friction repairs — #753 #832 #836 #772 #763 #712`.

## 5. Non-goals (unchanged from the origin)

#762/#715/#716 stay on the machinery lane; no renames beyond the
alias; no empty-state dropdown row; no kiosk leg for the conversion
arms; no undo-grammar changes; the global `DispatchOutcome::NoTarget
=> {}` silence is out of scope (§11's backlog).

## 6. Estimated diff

~250 lines production (`adapter.rs` ~140, `router.rs` ~70,
`view_state.rs` ~35, `cli.rs` ~5, `habits.slint` + today-view glue
~20) plus ~500–650 test lines across `journeys.rs`, the adapter seam
tests, `cli.rs`, and the seam-inventory row. Eight commits.

## 7. Backlog (standing authorization; filed at planning time)

- NEW `backlog:` habit rows have no keyboard traversal — this chunk
  adds the focus *fact* (the menu verbs' subject), not key
  navigation over the habit section.
- NEW `backlog:` route()'s `DispatchOutcome::NoTarget | Disabled =>
  {}` is silent surface-wide; T-19's every-click-reacts covers only
  disabled menu rows — a uniform typed refusal surfacing is a
  separate doctrine pass.
- COMMENT on #762: the chunk's guard work is orthogonal (the
  flyout-site/modal-mount coupling stands; the empty-rows guard
  landed with round-5's F14/D6 conversion).

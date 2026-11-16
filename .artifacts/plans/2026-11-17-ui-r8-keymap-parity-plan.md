# [Plan] ui-r8 chunk 1 — keymap parity (stage-1 implementation plan)

- **Date**: 2026-11-17
- **Kind**: TDD-first chunk implementation plan (dispatch stage 1 —
  planning; no code in this commit).
- **Origin**: the accepted UX journey plan
  `.artifacts/plans/2026-11-16-keymap-parity-ux-plan.md` (§3 US-1..3,
  §4 steps, §6 W-1..W-3, §7 delta, §8 verification — all normative),
  chunked by `.artifacts/plans/2026-11-16-gui-review-round8-dispatch-plan.md`
  §1 (chunk 1) / §2 (shared execution model, coverage-gate steady
  state). Branch: `fix/ui-r8-keymap-parity`. Round-8 standing laws
  apply verbatim (typed outcomes; `tokio::time::pause/advance`; census
  rides the code commits; `// slint:` comments on every touched seam;
  the C-4 doctrine bar; the focus contract).
- **Worktree**: `../taskboard-WT-fix-ui-r8-keymap-parity`
  (doctor-passed 2026-11-17, 153G free).

## 1. Verified current-state delta (what is actually left to build)

Reading the seams moved several of the origin plan's premises; the
delta below is the honest remainder (the origin plan's wording
predates some landed rounds):

| Origin claim | Seam reality | Consequence |
|---|---|---|
| emacs NavNextItem/NavPrevItem are Board/Today-only | True — but emacs **NavNextControl/NavPrevControl already hold C-n/C-p at TaskDetail** (r4 D19, `presets.rs:568-569`), recorded there as "the item walk is semantically dead here" | US-1's TaskDetail rows are a recorded **move** (C-n/C-p → the item ids), not an add; the route must first be made live (§3 D-2) |
| Settings C-n/C-p/C-f/C-b missing; C-f/C-b extension for Today/StackDetail/TaskDetail are free candidates | **All already bound** (r3 D25/D26, r4 D19 — `presets.rs:603-618`; Settings rows `presets.rs:620-623`) | No preset edits; US-1's Settings/C-f/C-b acceptance is journey **evidence** over the existing rows (the §8 journey names still get written) |
| Standard preset's Down/Up route is the live oracle for the item walk | Standard **binds Down/Up = NavNextItem at TaskDetail** (`presets.rs:211-212`) but the route is equally dead there — `step_focus`/`jump_focus` have no TaskDetail arm; they resolve `focus_domain` (the board reading order) and write `view_state.focused_task`, while the detail's subject is the separately-snapshotted `subject_task` (`view_state.rs:457`) | The route fix (§3 D-2) is resolver-side, shared by every preset; Standard/vi row spellings do not change (the non-goal is preserved) |
| M-</M-> bindings exist (`presets.rs:570-571`) | True, in all four view contexts (emacs) + Ctrl+Home/End (Standard) + H/G (vi, ViNormal) | US-2 is two route fixes only — no preset edits (origin ruling R-2 stands) |
| N-17's parse failure | Confirmed mechanically: `Stroke::from_event` keys on the **delivered glyph** (`adapter.rs:4938 to_stroke` — Slint hands the adapter only `text` + modifiers, no base keysym); a token `Alt+<` (`parse` → `key:'<', alt:true, shift:false`) cannot match an event whose layout delivers `Alt+Shift+','` | §3 D-3's normalization is the fix; the base-keysym limitation is filed (§7) |
| `ReminderRuleAdd`/`TaskTrackStart`/`TaskTrackStop` lack preset rows | Confirmed — registry entries exist (contexts `GLOBAL`, menu-only, dispatch-time gating, `actions.rs:1298/1415/1425`; router arms `router.rs:726/962/969`) | US-3's "gain preset rows" is exactly bindings; no seam changes for these three |
| `TaskDueEdit`/`TaskScheduledEdit`/`TaskDescriptionEdit` | Do not exist; the pointer funnels do: `begin_date_edit` (`adapter.rs:10336`, the round-3 D11 one begin seam) and the description editor arm (`adapter.rs:3777`, r3 D15) | New ids + new `UiEffect`s + interpret arms calling the existing funnels (§3 D-4); the `// slint:` bind callbacks at `adapter.rs:10082/10091/10104` carry no `// slint:` comment today — stage 2 adds them where the gate rows resolve (#782) |

Gate inventory at plan time (`scripts/ux-coverage.sh
2026-11-16-keymap-parity-ux-plan.md --gate`): 16 FOUND / 13 MISSING —
all MISSING are the un-built W-1..W-3 action tokens, the `—`-command
rows, and the four US-3 step journeys plus US-3/S3–S4. Stage 2's
steady state: every claimed row FOUND after the commits below.

## 2. Round-8 standing laws (restated, binding)

Typed outcomes only (AGENTS §5 — no label/log/UI-text assertions);
`tokio::time::pause()/advance()` for any time-dependent test; census
cells (`docs/ux-surface-census.org`) updated in the same commits as
the code, citing the owning plan's US/W ids; every touched slint seam
carries `// slint: <callback>` (#782); the C-4 doctrine bar is the
regression bar (grey-within-applicable beside
withheld-when-inapplicable — nothing in this chunk hides rows, so the
bar is "no regression"); the focus contract (focus-restoration US-1):
the date/description editors are overlays — this chunk composes with
chunk 2's restore seams by *not touching* their exit paths and never
stealing focus outside the existing funnels' ladders. Long-command
discipline: the coverage trio never runs in this worktree's subagent
— the gate is the pre-commit hooks plus
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(flock-safe), backgrounded with committed work before any wait.

## 3. Recorded decisions

### D-1 — Final preset rows per preset (candidates → contract-suite resolutions)

The contract suite (conflict + totality + prefix-shadow passes,
`keymap/mod.rs`) is the oracle: a candidate that collides is re-picked
from the same context's free set and re-recorded here in the
implementation commit message — never dropped (no-second-class).

**emacs** (TaskDetail context unless noted):

| ActionId | Candidate | Occupancy check |
|---|---|---|
| NavNextItem / NavPrevItem | `C-n` / `C-p` (**move** from NavNextControl/NavPrevControl — r4 D19's own precedent) | frees the control walk to its C-f/C-b rows only; no other TaskDetail C-n/C-p contact |
| TaskDueEdit | `C-c C-d` | free (C-c Ctrl+d is BoardDelete in the Board context only) |
| TaskScheduledEdit | `C-c C-s` | **C-5 collision with Global ReminderSnooze — resolved by context precedence, see D-5** |
| TaskDescriptionEdit | `C-c '` | free (chorded printable; shift kept by the chord rule, `stroke.rs:306-311`) |
| ReminderRuleAdd | `C-c C-u` | free at TaskDetail (C-c C-u unbound anywhere in emacs) |
| TaskTrackStart / TaskTrackStop | `C-c C-i` / `C-c C-o` at TaskDetail; the same pair at Today | free (bare Ctrl+o is TaskCreate at Board/StackDetail — different spellings, different contexts) |

**standard** (CUA; no re-binds of existing rows — F2/F3/F4 stand):

| ActionId | Candidate | Occupancy check |
|---|---|---|
| TaskDueEdit / TaskScheduledEdit | `F5` / `F6` | free (F1–F4, F12 bound; F5–F11 unbound) |
| TaskDescriptionEdit | `F7` (the origin plan's candidate) | free |
| ReminderRuleAdd | `F8` | free |
| TaskTrackStart / TaskTrackStop | `F9` / `F10` at TaskDetail and Today | free |

**vi** (single-mnemonic detail verbs — the SeriesEdit `r` precedent,
`presets.rs:842`, binding at the TaskDetail context, not ViNormal):

| ActionId | Candidate | Occupancy check |
|---|---|---|
| TaskDueEdit | `d` (vim delete-into-register: date change as the detail's "delete") | free at TaskDetail (vi `d` rows live in Board contexts) |
| TaskScheduledEdit | `s` | free at TaskDetail |
| TaskDescriptionEdit | `e` | free at TaskDetail |
| ReminderRuleAdd | `a` (vim append — mnemonic add) | free at TaskDetail |
| TaskTrackStart / TaskTrackStop | `I` / `O` at TaskDetail and Today | free at TaskDetail; Today-context contact checked by the suite (vi Today rows are sparse) — any collision re-picks from `x`/`X` and is recorded |

Every new row gets a `presets.rs` comment citing the owning US id and
this plan's D number, in the house comment style.

### D-2 — The item-table route fix (US-2 AC-1's absorb + US-1's live walk)

**Diagnosis (verified)**: `step_focus` (`view_state.rs:1393`) and
`jump_focus` (`view_state.rs:1506`) resolve their domain as
`today_domain` (Today) or `focus_domain` (board reading order /
compact stack) and write `view_state.focused_task`. The open
TaskDetail's subject is `subject_task`, snapshotted at entry (D7,
`view_state.rs:457`) and **never updated by either walk** — so in the
TaskDetail view every NavNext/Prev/First/Last invocation mutates a
hidden board selection: the silent no-op absorb. It is route-level,
not front-level: key and menu absorb *identically* (the menu path is
`invoke_entry` → `route` → the same `UiEffect::FocusFirst/Last` → the
same `interpret` arm, `adapter.rs:3299`).

**Fix**: one resolver-side arm, shared by both fronts (click ≡ key by
construction): when the open view is `ViewId::TaskDetail`, the item
table is the subject's stack task list (the detail's own item table;
a subjectless/stackless detail is the typed empty domain — the
existing inert), and the walk's target is written to **both**
`subject_task` (the visible detail follows) and `focused_task`
(kept in sync, the board re-follow stands). Implementation home: a
`detail_item_domain` factored beside `focus_domain` and consumed by
`step_flat_domain`/`jump_focus` via a view-keyed domain selection —
`step_focus`/`jump_focus` gain the TaskDetail branch exactly where
Today has theirs. No wrap at the ends (the walk's bounds stand);
empty domain emits nothing (inert, as today).

**Consequence, recorded**: the Standard preset's Down/Up (and vi's
j/k) at TaskDetail become live — that is the bug being fixed, not a
preset change; row spellings are untouched (the non-goal holds). The
emacs C-n/C-p move (D-1) then lands the member rows on the now-live
route.

### D-3 — The parser normalization mechanism (US-2 AC-2/AC-3, N-17)

**Mechanism: shift-composition normalization over the delivered
glyph — a stroke-equivalence function on the event side, not a
preset edit, not a keysym map** (the adapter's seam delivers only the
composed text; a true base-keysym match is impossible at this seam —
filed, §7).

- New pure fn in `keymap/stroke.rs`:
  `Stroke::chord_shift_partner(self) -> Option<Stroke>` — for a
  **chorded** (`ctrl || alt || meta`) stroke whose key is an
  *unshifted* member of the US-ANSI shift-pair punctuation table
  `PUNCT_SHIFT_PAIRS` (the static pairs `(',') , '<'`, `('.') , '>'`,
  `('/') , '?'`, `(';') , ':'`, `('1') , '!'`, … the table carries
  exactly the pairs whose shifted glyph is a bound token shape), the
  partner is `key = shifted glyph`, `shift = false` (the shifted
  glyph carries shift in the character — the same fold rule as
  `parse`). Non-chorded strokes, named keys, and glyphs outside the
  table yield `None`.
- Match rule (additive): `Keymap::resolve` first tries the exact
  sequence (today's behaviour, byte-identical), then — only if the
  exact lookup missed and the event seq's last stroke has a
  partner — retries with the partner substituted. Unmatched events
  fall through unchanged (AC-3's additivity: every existing token's
  resolution is untouched; the exact-match-first ordering keeps
  `Alt+Shift+,` resolving `Alt+,`-style tokens ahead of the
  normalization, and no prefix/pending logic consults the partner —
  `has_prefix` stays exact, so the chord grammar cannot be altered).
- Why this shape and not alternatives: (a) a *keysym/base-key* match
  needs a seam Slint does not expose (the `KeyEvent` is text-only at
  `to_stroke`) — rejected as un-buildable here, filed; (b) a
  glyph→glyph *rewrite* of the event would change what text input
  and other consumers see — rejected (the normalization lives in the
  keymap matcher only, additive by construction); (c) per-layout
  tables — rejected (unbounded, and R-2 forbids preset/layout rows).
- Verification: parser unit tests per §5; the operator's manual
  Dvorak International confirmation is the AC-2 human half (the
  N-17 report's observed shape is "the base glyph arrives with
  Alt+Shift", exactly the shape this map fixes); a layout that
  composes a *third* glyph is out of reach at this seam (§7 issue).

### D-4 — The three new ActionIds: registry, contexts, routes

| ActionId | Registry entry (`actions.rs`) | Dispatch (`router.rs`) | Interpret (`adapter.rs`) |
|---|---|---|---|
| `TaskDueEdit` | label "Edit due date", contexts `&[ContextId::TaskDetail]`, Category::Task, menu_group Task, menu_rank beside TaskImportanceEdit | `UiEffect::BeginDateEdit { field: DateField::Due }` | a `(Flow::None, BeginDateEdit)` arm calling the **same funnel the row affordance's bind callback calls** — the round-3 D11 `begin_date_edit` body lifted into a shared `Adapter` method (settle → `begin_edit_date` → seed buffer → outcomes → picker); the slint callbacks (`adapter.rs:10082/10091/10104`) now call the method too — click ≡ key at the funnel, and those three seams gain their `// slint:` comments (#782) |
| `TaskScheduledEdit` | the scheduled twin | `BeginDateEdit { field: Scheduled }` | same funnel |
| `TaskDescriptionEdit` | label "Edit description", contexts TaskDetail | `UiEffect::BeginDescriptionEdit` | an interpret arm calling the description editor's arm (`adapter.rs:3777`, r3 D15) exactly as the field's pointer affordance does; the `Editor`-scope TEXT settle rule applies unchanged |

No new `StateCommand`, no new `UiRequest` (the origin §7 delta holds);
the `subject_task`-less detail is each arm's typed inert (the
funnels' existing refusals). The keymap `Display`/slug arms
(`actions.rs:531..`) get `task.due-edit` / `task.scheduled-edit` /
`task.description-edit`; hints and the registry snapshot regenerate.

### D-5 — The C-5 resolution: context precedence, no retune

The resolver is chain-order first-match (`keymap/mod.rs:86`, "the
highest visible context carrying exactly `seq` wins"), and the view
context precedes `Global` in every view chain. Therefore:

- emacs gains `C-c C-s = TaskScheduledEdit` in the TaskDetail context
  (D-1); `ReminderSnooze` **keeps** its Global `C-c C-s`
  (`presets.rs:425-431`) untouched.
- While the detail holds, `C-c C-s` resolves the scheduled editor
  (the TaskDetail row wins by chain order); in every other chain
  (Board, Today, Settings, …) it resolves ReminderSnooze exactly as
  today.
- Recorded cost: inside the TaskDetail chain the Global snooze chord
  is shadowed (menu-reachable everywhere; key-reachable in all other
  chains). A retune of ReminderSnooze was rejected — the C-5 rule
  allows either resolution, precedence preserves both rows verbatim
  (no snapshot churn, no relearn), and the shadow is exactly org's
  own buffer-local semantics.
- Contract-suite pin: a test asserting both rows exist (totality per
  context) and the chain-order resolution — `TaskDetail` chain →
  TaskScheduledEdit, `Board` chain → ReminderSnooze.

### D-6 — Journey wiring claims (the §6 rows stage 2 must land FOUND)

| Row | Stage-2 wiring |
|---|---|
| W-1 | the D-2 route fix + the D-1 emacs move; journey `us1_s1_emacs_motion_rows_step_the_walks` + `us1_ac2_settings_control_walk_rows_step` (evidence over the pre-existing Settings/C-f/C-b rows) |
| W-2 | the D-3 parser fn + the D-2 shared route; parser unit tests + `us2_s1_menu_first_last_matches_the_key_route` (menu invoke ≡ key invoke: both fronts assert the same resolved subject, the empty item set is the typed inert surfaced by the existing outcome path) |
| W-3 | the D-4 ids/effects/funnels + the D-1 row additions; the four US-3 journeys (§5) + the seam-level exact-command tests for Start/StopTracking and AddReminderRule (typed `StateCommand` assertions, router-level — the routing unit tests' home, `router.rs:1421+` precedent) |

## 4. Test catalogue (TDD order; every name is the §8 name)

Layer rule (one behavior, one test, lowest layer): parser cases in
`stroke.rs`; route/domain cases in `view_state.rs` unit tests +
`router.rs` dispatch tests; cross-front parity and arm parity in
`tests/journeys.rs`; the contract suite + `keymap_snapshots.rs`
(insti) for totality/snapshots. All outcome assertions typed
(`matches!` on actions/errors/commands); no sleeps (the journeys'
existing poll/deadline helpers stand).

1. **Parser unit tests** (`stroke.rs`, commit 1):
   - `alt_shifted_punct_tokens_match_base_glyph_events` —
     `Alt+<`/`Alt+>` tokens match `from_event(',', alt, shift, …)`
     shaped events (the Dvorak shape), via the resolve-level rule;
     named-key and non-table glyphs yield no partner.
   - `normalization_is_additive_and_exact_match_wins` — every
     pre-existing bound token resolves identically with the rule on
     (snapshot + a direct seq case), unmatched glyphs fall through,
     `Ctrl+Shift+Z ≠ Ctrl+Z` unchanged.
2. **Contract suite** (`keymap/mod.rs` tests, commit 3):
   - the D-5 C-5 pin (see above);
   - the totality/conflict/prefix passes over the new rows (existing
     tests, extended data — not weakened);
   - `keymap_snapshots.rs` regenerated (insta).
3. **Route unit tests** (`view_state.rs` + `router.rs`, commit 2/4):
   - `step_focus`/`jump_focus` TaskDetail arm: subject follows the
     walk, ends clamp, empty domain inert, `focused_task` synced;
   - the three new ids' dispatch: `UiEffect` typed outcomes, the
     tracking/rule ids' existing command routing pinned per preset
     row context (TaskDetail and Today for the tracking pair).
4. **Journeys** (`tests/journeys.rs`, commit 4):
   - `us1_s1_emacs_motion_rows_step_the_walks` — TaskDetail (C-n/C-p
     step the subject; C-f/C-b step controls) and Board/Today bounds;
   - `us1_ac2_settings_control_walk_rows_step` — the Settings field
     walk over C-n/C-p/C-f/C-b;
   - `us2_s1_menu_first_last_matches_the_key_route` — open the menu,
     invoke Focus first/last, assert the resolved subject equals the
     key path's (the parity law's letter);
   - `us3_s1_due_shortcut_arms_the_editor_like_the_row` — key fires
     `begin_date_edit`'s exact funnel state (armed window, seeded
     buffer, picker open — the §8 invariant) identical to the row
     affordance's; Escape settles per the existing ladder;
   - `us3_s2_description_shortcut_arms_the_editor` — the description
     arm's flow state (`Flow::EditDescription`) identical from both
     fronts;
   - `us3_s3_clock_shortcuts_route_the_exact_commands` —
     Start/StopTracking dispatch the same `StateCommand`s as the chip
     (typed command assertions; NoTarget/nothing-open inert);
   - `us3_s4_rule_add_shortcut_arms_the_funnel` — ReminderRuleAdd
     arms the rule-add funnel exactly as the preset buttons do; the
     custom-offset parse miss absorbs as today.

## 5. Commit sequencing (each commit green under fmt/clippy/nextest)

| # | Commit | Contents |
|---|---|---|
| 0 | `docs(plan): the ui-r8 keymap-parity chunk plan` | this file (landed before any wait) |
| 1 | `fix(ui): chorded punctuation tokens match base-glyph key events` | D-3 (`PUNCT_SHIFT_PAIRS`, `chord_shift_partner`, the additive resolve rule) + parser unit tests; census `nav.item_walk` Dvorak evidence cell rides here |
| 2 | `fix(ui): the detail's item table resolves the open TaskDetail` | D-2 (`detail_item_domain`, the step/jump arms, subject sync) + view_state unit tests; the N-2 route fix |
| 3 | `feat(ui): emacs motion member rows at task detail` | D-1 emacs move (C-n/C-p → item ids; control walk keeps C-f/C-b) + contract-suite extensions + D-5 pin + regenerated snapshots |
| 4 | `feat(ui): task-detail action shortcuts in every preset` | D-4 (three ids: enum, slug, registry entries, `UiEffect`s, router arms, interpret arms over the shared funnels, `// slint:` comments) + D-1's per-preset shortcut rows (all three presets, TaskDetail/Today) |
| 5 | `test(ui): the keymap-parity journeys + census updates` | the seven journeys; `docs/ux-surface-census.org` cells (nav.item_walk/nav.control_walk evidence, the detail verbs' key evidence, the TE-W-N2 retirement note) citing US/W ids; gate self-check `scripts/ux-coverage.sh <origin plan> --gate` all-FOUND recorded in the commit body |

Then the stage gate: `scripts/run-quality-gates.sh cargo nextest run
--workspace` (backgrounded, committed work first). Coverage trio stays
with the dispatcher.

## 6. Risks

- **R-1 (parser reach)**: a layout that composes a third glyph under
  Alt+Shift defeats the pair map at this seam (text-only event).
  Mitigation: the manual AC-2 half; the seam gap filed (§7).
- **R-2 (behavior change to Standard/vi at TaskDetail)**: Down/Up and
  j/k become live subject-walks — the intended fix, but it is a
  visible change to existing presets; the C-4 doctrine bar and the
  walkthrough judge it. If the walkthrough objects, the fallback
  (recorded, not silent) is scoping the D-2 arm to the *jump* pair
  only and filing the walk half — never a dropped row.
- **R-3 (C-c C-s shadowing)**: snooze is chord-shadowed inside the
  detail chain; menu reachability is the mitigation, the C-5 pin is
  the record.
- **R-4 (snapshot churn)**: registry/keymap snapshots regenerate in
  commits 3–4; no hand edits of snapshot files.
- **R-5 (cross-chunk)**: chunk 2 (focus restoration) lands after this
  PR and rides the same `actions.rs`/`presets.rs` — keep new enum
  variants appended, never reordered (serde/registry stability), so
  chunk 2's rebase is clean.

## 7. Backlog

- **base keysym seam** (D-3's limitation): "backlog: adapter key seam
  exposes only the composed glyph — layout-independent chord matching
  needs the physical/base keysym" — to be filed before hand-off
  (AGENTS §7 standing authorization), linked from the D-3 comment.
- (none found otherwise; the round-8 deferrals C-6 stand untouched.)

## 8. Definition of done (stage-2 hand-off)

- [ ] Gate: `scripts/ux-coverage.sh 2026-11-16-keymap-parity-ux-plan.md --gate` → every claimed row/step FOUND (0 MISSING)
- [ ] Pre-commit hooks green; `scripts/run-quality-gates.sh cargo nextest run --workspace` green
- [ ] The seven §8 journeys exist under their exact names
- [ ] Census cells updated in-code (commit 5), TE-W-N2 retired
- [ ] No vi redesign, no Standard re-binds, no three-stroke chords; the C-5 precedence recorded; every touched seam carries `// slint:`
- [ ] Backlog issue filed and referenced

# [Plan] UI R7 Chunk 1 — Create habits from the Today section

- **Date**: 2026-11-13
- **Kind**: stage-1 TDD implementation plan for dispatch-plan chunk 1
  (branch `feat/ui-r7-habit-create`, worktree
  `../taskboard-WT-feat-ui-r7-habit-create`). No code in this commit.
- **Origin** (normative): round-7 stage-1 plan §2 row R7-2, §3 D-7.2 and
  the §3 accessibility ledger
  (`.artifacts/plans/2026-11-13-gui-review-round7-plan.md`); dispatch
  plan §1/§2 (`.artifacts/plans/2026-11-13-gui-review-round7-dispatch-plan.md`).
- **Anchors re-verified** at `main` @ `35e6b0a1` (the dispatch plans'
  landing commit). Line drift vs the origin's `64efd33c` anchors is
  noted inline where it matters.
- **Domain truth (dispatch against, unchanged)**: `StateCommand::CreateHabit
  { board, title, target }` (`crates/taskboard-domain/src/messages.rs`,
  the habit verb family); `HabitTarget::{PerDay,PerWeek,PerMonth}` +
  `NonZeroCount::new` rejecting zero with `InvalidHabitTarget`
  (`crates/taskboard-domain/src/habit.rs:24-97`). No domain/state/storage
  change of any kind in this chunk.
- **Tests that must not break**: the engine habit snapshots (s45 family,
  `taskboard-state/tests/snapshots.rs` — untouched, no engine change);
  the existing habit seam tests (`install_habit_seams` verbs: +1,
  reorder, repair, convert arm/confirm/cancel — un-weakened); the
  keymap contract tests (`keymap/mod.rs` T6 totality — extended, not
  weakened); `tests/seam_inventory.rs` (extended with the new
  callbacks).

---

## 1. Findings that shape the design

1. **The section never mounts with zero rows** (`ui/views/today-view.slint:172`
   — `if (root.habits.length > 0)`): a board with no habits renders no
   section, so a header-only add affordance could never create the
   *first* habit. The mount gate must widen model-side (§4.3), not
   Slint-side.
2. **The habit verbs are uniformly unbound** (`actions.rs` habit family
   comment: "pointer-driven, no preset bindings"; no habit rows in any
   preset). `HabitCreate` deliberately breaks that posture — the origin's
   accessibility ledger demands a keyboard binding in every preset.
   Recorded as D-3 below with the comment update this forces.
3. **The inline-form precedents split**: `TimersSection`'s create form is
   *always mounted* (no armed flag; commit seam returns `bool`, an
   absorbed parse miss keeps the text, `adapter.rs:8781-8830`), while the
   convert confirm strip is *armed* (adapter-owned flag pushed as a
   property, `habits-convert-armed-row`). The dispatch plan mandates the
   armed shape ("mounted below the header while armed") — D-2 follows the
   convert-strip/stack-create precedent, borrowing the timers commit
   seam's `bool` return convention for parse misses.
4. **Escape for presentation state has a dedicated funnel**:
   `keyscope::timer_setter_dismisses` + the `special_scope_arms` block
   (`adapter.rs:810-822`) — the timer setter's Escape is an
   adapter-side decline, *not* a keymap binding, exactly because a
   `Today`-context Escape binding would statically shadow Global
   `TaskMoveCancel`'s Escape (exact-match-first resolution, chain
   `[Today, ViNormal, Global]`). This decides D-6.

## 2. Design decisions (each settles an open question)

### D-1 — Names and the armed-form flag's home

Adapter `Adapter` gains two private fields (source of truth, the
adapter-owned-state doctrine):

- `habit_create_open: bool` — the armed-form flag. Default `false`
  (`Adapter::new` initializer block, the `habit_convert_armed` spot).
- `habit_create_draft: Option<HabitCreateDraft>` — the one-shot commit
  payload, minted at the confirm seam (§D-4). `HabitCreateDraft` is a
  small adapter-internal struct `{ board: BoardId, title: String,
  target: HabitTarget }` living next to `router::TimerDraft`'s
  adapter-side kin (placement: `adapter.rs` private struct, like the
  other one-shot payload carriers; the router matches on the ctx copy).
- Quota text draft: `habit_create_quota: String` — mirrors
  `timer_when_draft` (the only field whose per-keystroke parse is
  adapter-side; the title text stays field-local like the timers
  label, validated once at commit).

Slint side: **a plain `in-out property <bool> habits-create-open` on
`AppWindow`** (declared beside `habits-convert-armed-row`,
`app-window.slint:1541`), pushed by the adapter — *not* a facts global.
**Deviation from the dispatch brief's "adapter field + facts global,
naming per the settings-facts precedent", recorded reason**: the facts
globals (`SettingsBoardFacts`, `EditingFacts`, scroll/anchor facts)
exist for *two-way* data the UI writes and Rust reads (geometry, focus
counts, dropdown open state read by the dropdown ladder). The armed
flag is one-way adapter→UI; the `timer-setter-open` /
`habits-convert-armed-row` properties are the exact precedent for that
shape. The keyscope dismissal arm (D-6) reads the *window property*
(`window.get_habits_create_open()`), not a global, matching
`timer_setter_dismisses(window.get_timer_setter_open(), stroke)`.

Callback grammar (names follow the section's existing
`habits-*-` forwarding convention):

| Layer | Name | Shape |
|---|---|---|
| `HabitsSection` | `create-asked()` | header add button; routes `HabitCreate` |
| `HabitsSection` | `create-confirmed(string title, string quota, int target-index) -> bool` | the timers commit convention: `false` = absorbed miss (text kept), `true` = dispatched (fields clear) |
| `HabitsSection` | `create-cancelled()` | adapter-direct disarm (D-6) |
| `HabitsSection` | `create-quota-edited(string)` | per-keystroke parse (the `timers-when-edited` twin) |
| `HabitsSection` | in `create-open: bool`, in `quota-validity: FieldValidity`, in `quota-error-hint: string` | adapter-pushed; the mount gate + the typed refusal display |
| `TodayView` | `habits-create-asked()`, `habits-create-confirmed(string, string, int) -> bool`, `habits-create-cancelled()`, `habits-create-quota-edited(string)`, in `habits-create-open`, in `habits-quota-validity`, in `habits-quota-error-hint` | verbatim forwarding |
| `AppWindow` | the same set as pure callbacks + the `habits-create-open` property | the `habits-plus-clicked` forwarding block (`app-window.slint:1532-1541`) |

`tests/seam_inventory.rs` gains every new callback name (the inventory
test enumerates; its list is extended, the existing entries untouched).

### D-2 — The form's control grammar (all existing widgets)

Mounted inside `HabitsSection` directly below the header row, gated
`if (root.editing-visible && root.create-open)` (the convert strip's
gate shape: a kiosk flip mid-flow unmounts the form, the adapter keeps
the flag, the form remounts if editing returns — the strip's own
documented law, `habits.slint:149-152`):

- **Title**: `TextField` (the timers form's widget; insert mode rides
  the standard text-focus machinery). Commits on Return via the
  field's accepted → `create-confirmed` with the current quota text
  and target index (the create-row precedent the origin names).
  Whitespace-empty title at commit → `false`, nothing dispatched
  (the timers label's D-P14 law verbatim).
- **Target**: `SegmentedControl` (the timers' widget import),
  three segments labelled `Per day` / `Per week` / `Per month`,
  indices 0/1/2 mapping to `HabitTarget::PerDay/PerWeek/PerMonth`.
  Selection is an `in-out property <int> target-index: 0` held by the
  composition — **UI-held selection state, deliberately** (the
  insistence picker's recorded precedent, `timers.slint:44-46`: the
  caller owns the selection; it is not business data until the seam
  mints it). Default `PerDay`.
- **Quota**: `TextField`, numeric. Per-keystroke `create-quota-edited`
  → adapter parses `trim().parse::<u32>()` then `NonZeroCount::new`;
  `Ok` pushes `FieldValidity.valid`, `Err(_)` (zero *or* junk *or*
  overflow) pushes `FieldValidity.invalid` + the hint string mapped
  presentation-side from `InvalidHabitTarget` (the
  `timer-when-error-hint` twin). **The zero refusal is structural**
  (the mint rejects it; the hint is presentation mapping) — tests
  assert the `FieldValidity` variant and the no-dispatch outcome,
  never hint text (AGENTS §5 no-text-assertions).
  **Empty text commits as 1** (the origin's default): an untouched
  field must not block commit; decision recorded here so the default
  is visible in the field's placeholder copy ("1").
- **Confirm / Cancel**: `IconButton`s (check / remove icons, the
  convert strip's pair) joining the walk after the fields, labels
  "Create habit" / "Cancel create" (the label ≡ hover tip ≡ walk
  name doctrine). Confirm invokes `create-confirmed`; Cancel
  invokes `create-cancelled`.

### D-3 — Action registry and free keys

- **`ActionId::HabitCreate`** (`"habit.create"`), `contexts:
  &[ContextId::Today]`, `category: Category::Task`,
  `menu_group: Some(MenuGroup::Task)`, `menu_rank` slotting after
  `HabitRepair` (215; later rows shift — the ranks are spacing-tolerant,
  the registry snapshot regenerates). Semantics: **open/toggle** —
  dispatch opens the armed form when closed; when armed it disarms
  (discard-and-close), the `ViewPlanNextDay` toggle precedent
  (`presets.rs:90-99`: the opener toggles, the planner-context Escape
  row closes). Opening requires an active board: no board → the flag
  stays down and the dispatch degrades to `NoTarget`-like silence
  (the stale-arm law; nothing to create onto).
- **`ActionId::HabitCreateCommit`** (`"habit.create-commit"`), same
  context/category, `menu_group: None` (internal dispatch verb,
  the `HabitLog` posture — it has no affordance of its own beyond the
  form's Confirm). Dispatch: `(ctx.habit_create_draft)` `Some` →
  `DispatchOutcome::Command(StateCommand::CreateHabit { .. })`;
  `None` → `NoTarget`.
- Router arms in the habit family block (`router.rs:636-652` region)
  for both ids; `Adapter::route`'s ctx wiring gains
  `ctx.habit_create_draft = self.habit_create_draft.take();` (the
  one-shot take list, `adapter.rs:897-901`); `RouteCtx` gains the
  field beside `habit_convert_target`.
- **Free keys**: `(HabitCreate, ContextId::Today, &["Ctrl+Shift+H"])`
  in **all three presets** — verified free everywhere (no
  `Ctrl+Shift+H` in any preset's table; `Ctrl+Shift` chords in Today
  are otherwise unused so no prefix shadow). One row per preset,
  the alternates shape unused. `HabitCreateCommit` ships unbound
  (like the other internal confirm verbs — `HabitConvertToSeries`
  has no preset row). The `actions.rs` habit-family comment ("no
  preset bindings") gains one sentence recording the deliberate
  exception with the origin ledger as authority.
- `keymap/mod.rs` T6: `HabitCreate` joins the core-verb list (the
  no-second-class gate now *enforces* the all-preset binding).
  Registry/resolved-preset keymap snapshots regenerate (insta
  review, expected).

### D-4 — The commit funnel (one-shot target-field law)

The confirm seam (`on_habits_create_confirmed`, the
`install_seam!` shape) is the trust boundary:

1. Resolve the board **fresh at commit time** from
   `a.view_state.board()` — never cached at arm time (what you see is
   what the habit lands on; the repair seam's "same row source"
   doctrine). `None` → silent no-op, and the armed form disarms
   (the board list changed under it — the stale-arm law).
2. Mint: `NonZeroCount::new(quota)` (empty text → 1; parse miss or
   `InvalidHabitTarget` → return `false`, text kept, nothing
   dispatched) and `HabitTarget::PerX(count)` from `target-index`
   (out-of-range index → `false`, structurally impossible from the
   SegmentedControl but the seam is total).
3. Empty/whitespace title → `false` (D-P14).
4. Arm the one-shot `habit_create_draft = Some(HabitCreateDraft {
   board, title: trimmed, target })`, clear the flag + the quota
   draft, route `HabitCreateCommit`; return `true`. The router arm
   consumes the draft (one dispatch, then gone). Post-command the
   standard mutation refresh re-renders the section with the new row.

The header add seam (`on_habits_create_asked`) routes `HabitCreate`;
its dispatch flips `habit_create_open` and refreshes (view intent,
the setter-opened shape). The cancel seam and the toggle's disarm
branch clear `habit_create_open`, the quota draft, and
`refresh_forced` (the convert-cancelled shape). The keyscope Escape
arm (D-6) calls the same disarm funnel — one disarm path, three
entry points.

### D-5 — Board resolution and the mount gate (finding 1)

`push_habits` already receives `board: Option<BoardId>`; it additionally
pushes `window.set_habits_create_mountable(!kiosk && board.is_some())`
(new plain property, forwarded like `habits-editing-visible`). The
today-view mount gate widens to
`if (root.habits.length > 0 || root.habits-create-mountable)`:
- Desktop, board active, zero habits → the section mounts with the
  header and the add affordance; the first habit can be created.
  **This is the chunk's core structural fix.**
- Kiosk → `create-mountable` is false; the gate degenerates to the
  old `rows > 0` (no empty-header noise on the appliance; D-P13
  posture untouched, and the form/add button are editing-gated anyway).
- No active board → false; neither section nor affordance mounts
  (the origin's no-board mounting rule, now actually enforceable).

### D-6 — Escape and cancel (deviation from the origin ledger, recorded)

The origin ledger says "Escape cancels through the action layer".
**Decision: Escape disarms via a `keyscope` dismissal arm, not a
keymap binding** — `keyscope::habit_create_dismisses(open, stroke)`
(the `timer_setter_dismisses` twin, `keyscope.rs:232`), wired in
`special_scope_arms` ahead of the resolver (`adapter.rs:810-822`
region): while `habits_create_open` and the stroke is Escape, the arm
runs the disarm funnel and consumes the stroke. **Reason**: a
`Today`-context Escape binding would statically shadow Global
`TaskMoveCancel`'s Escape on the whole Today view (exact-match-first,
finding 4) — a regression this chunk may not buy; a dedicated mask
`ContextId` exceeds the origin's machinery budget for an S–M chunk.
The keyscope arm *is* the action layer's existing funnel for
presentation-state Escape (the timer setter's recorded ruling, r4
D33). Cancel parity: mouse Cancel button ≡ Escape ≡ the keyboard
toggle — all three hit the same disarm funnel; "click ≡ key" holds.

## 3. Non-goals (unchanged from the origin §4)

No engine/domain/state/storage changes; no board picker (section board
= active board); no habit target-editing UI; no new optional create
fields (D-H7a); no new `Tone` variants; no at-create scheduling.

## 4. TDD plan (test-first order)

Per-test: write failing → implement → green; every commit standalone.

1. **Presentation** (`presentation.rs` tests module): the quota text →
   `Result<NonZeroCount, QuotaParse>` mapping (new tiny pure fn beside
   `habit_target_phrase`: empty → default 1, junk/zero/overflow →
   typed err) — asserts the typed outcome, plus the
   `FieldValidity`+hint mapping fn's totality over the err enum.
2. **Keymap** (`keymap/mod.rs`): T6 extended with `HabitCreate` —
   failing until the three preset rows land. Existing totality stays
   green; snapshots regenerate.
3. **Seam inventory** (`tests/seam_inventory.rs`): the new callback
   names — failing until the Slint surface declares them.
4. **Adapter exact-command test** (`lib.rs`, the
   `banner_actions_send_exact_commands` shape): fixture state with an
   active board → invoke the header add affordance → fill fields →
   invoke confirm → assert the channel received exactly
   `StateCommand::CreateHabit { board, title, target:
   HabitTarget::PerDay(NonZeroCount 3) }` (typed `matches!` +
   field equality) and nothing else.
5. **Refusal tests** (same module): quota `"0"` and junk → confirm
   returns/absorbs, channel empty, validity pushed invalid; empty
   title → channel empty. Stale-arm: armed form with the board
   resolved away → confirm dispatches nothing and the flag clears.
6. **Kiosk gate test**: kiosk UiSettings → `habits-create-mountable`
   false, the add button and form absent from the mounted section,
   rows render (mirror of
   `habit_rows_render_stored_order_and_kiosk_gates_the_actions_only`'s
   structural assertions).
7. **Toggle/dismiss test**: keyboard `HabitCreate` resolve on the
   Today chain opens; second stroke disarms; Escape-while-armed
   consumes and disarms, Escape-while-closed falls through to
   `TaskMoveCancel` (the non-shadowing pin — D-6's reason, pinned).
8. **Engine snapshots**: `cargo nextest run -p taskboard-state` s45
   family green, untouched.

## 5. Quality gates

Full AGENTS §10 suite through `scripts/run-quality-gates.sh`; hand-off
is stage 3's job (rebase, PR, checks green, mergeStateStatus CLEAN,
worktree teardown).

## 6. Risks

- **Snapshot churn**: the registry + resolved-preset keymap snapshots
  and any menu-projection snapshots regenerate with the two new action
  rows — expected, reviewed by eye (label/rank sanity).
- **Walk/focus on the newly mounted form**: the fields ride the
  standard control walk; the focus-coverage probe suite may need a new
  surface row — treated as a finding if a probe fails, not a gate
  weakening.
- **`Ctrl+Shift+H` conflicts with future chord plans**: chosen for
  mnemonic strength; free in all presets today; the collision check
  is the totality test's job forever.
- **Estimated diff size**: ~500–650 lines across ~10 files
  (`habits.slint`, `today-view.slint`, `app-window.slint`,
  `adapter.rs`, `models.rs`, `presentation.rs`, `actions.rs`,
  `router.rs`, `keymap/presets.rs`, `keyscope.rs`, `keymap/mod.rs`,
  plus the three test files and regenerated snapshots). S–M as
  dispatched.

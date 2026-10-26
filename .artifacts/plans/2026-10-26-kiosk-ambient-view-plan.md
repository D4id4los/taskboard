# [Plan] Kiosk ambient view + display rotation (chunk K13, #363) — TDD implementation plan

- **Date**: 2026-10-26
- **Kind**: stage-1 implementation plan for chunk 1 of the kiosk
  closure dispatch (branch `feat/ui-kiosk-ambient-view`, worktree
  `../taskboard-WT-feat-ui-kiosk-ambient-view`, branched off `main` @
  `a503dd3`). TDD-first: every milestone names its failing tests
  before the production change.
- **Origin** (normative rank per dispatch plan MD-4): the wave
  implementation plan `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
  (§2 KDF-10/KDF-11, §3 K13, §4 gates); the views-finding design
  `.artifacts/plans/2026-10-19-views-finding-design-plan.md` (D-V1
  rotation-readiness, D-V3 the kiosk degeneration, §4's kiosk
  consolidation); issue #363's body (the scope fence); the framing
  §4 record (D-K2 created the issue, D-K10 the sequencing); the
  2026-10-21 remaining-steps report (step 4, the critical-path
  record); the ListView design-round report + #373's dispatch plan
  (what the compositions now are — this chunk composes them); the
  kiosk closure dispatch plan §0/§2 (chunk covers, acceptance,
  traps, MD-1…MD-6). All symbols below were re-located on the
  worktree base `a503dd3`; line numbers are plan-day references.
- **Status**: READY for stage 2. Every open question is settled with
  a recorded decision below. No deviation from any ACCEPTED ruling
  exists — the two interpretation calls (no forced compact
  breakpoint; no new kiosk composition file) stay inside D-V3's
  letter and are argued in decisions D4/D5.

## 1. Mission & scope fence

Make the kiosk appliance's window a passive ambient display: a
display-mode state machine over the **one pinned board** that
rotates the board view's kanban reading and the today view's
upcoming reading on a UI-side dwell timer, with zero input, zero
engine commands, and byte-identical desktop behaviour. The banner
and the sync badge's offline tone render in both modes.

**In**: the `KioskMode`/`KioskRotation` state machine (pure, in
`view_state.rs`); the rotation timer (adapter-side `slint::Timer` —
decision D2); `[kiosk.rotation]` config keys with typed validation
(decision D3); the pin-resolution matrix extension (decision D4);
the flip's focus/scroll story (decision D7); the no-engine-command
structural assertion (decision D8); unit tests + three journey legs
+ two new kiosk goldens; the CHANGELOG Unreleased line.

**Out** (dispatch plan §2 non-goals, restated so nobody re-derives):
no multi-board rotation ring (backlog — the pin is one board); no
capture/edit surface reachable in a kiosk mode (the landed gates
already enforce this; no new capture work); no animation; no
renderer/backend change (the D250 posture is ruled); no engine
commands from rotation (`FlowOutcome::Command` never produced); no
`IdTables` re-keying or model-semantic change (#373 just landed
them); no detect-side work (#463/#464); no interaction-pause
semantics for the timer (an operator's keystrokes do not delay the
next flip — see decision D2's dwell record); no toolbar/status chrome
surgery beyond the landed kiosk gates (D5); no Q6 input (that is
K14's).

## 2. Verified baseline (symbols are the stable keys)

What exists on `a503dd3` and is consumed, never re-derived:

| Seam | Symbol (file) | Notes |
|---|---|---|
| View cycle | `ViewId::{Today, Board, StackDetail, TaskDetail, Settings}` (`view_state.rs:43`) | `step_view` cycle; Settings excluded |
| Kiosk start view | `install` seeds `ViewState::new_starting(if kiosk { Board } else { Today })` (`adapter.rs:~2067`) | the kiosk mount pin: `the_kiosk_mount_still_lands_on_the_board` (`tests/journeys.rs:4863`) |
| Board-pinned today | `ViewState::today_domain(state, kiosk, day)` (`view_state.rs:693`); `push_today(..., kiosk, ...)` (`models.rs:1175`) | the D-V3 filter (kiosk keeps only the focused board's rows) is **landed** in both the walk domain and the model pass |
| Passive gates | `push_chrome` inbox count 0 on kiosk (`models.rs:~586`); `push_notifications` R3.5.3 banner-only gate (`models.rs:421`); `today_row_context(state, task, kiosk, multi_board)` board-badge suppression (`presentation.rs:473`) | "no kiosk-specific code beyond the filter rule" already holds |
| Banner + offline tone | `StatusBar` (`app-window.slint:330`) mounted **unconditionally** in the window's VerticalLayout; rows from `banner_rows(state, now)` (`presentation.rs:316`); badge from `sync_badge_for_board` (`presentation.rs:208`) | carry-over across a view flip is structural (§ D6) |
| Focus-into-view (#373) | `Adapter::push_focus_scroll_hint` (`adapter.rs:~788`) → `FocusScrollHint` global (`focus-scroll.slint`) → `ListView` seek (`widgets/listview.slint:26`) | must not break — decision D7 |
| Refresh pass | `Adapter::refresh` (`adapter.rs:436`): load → `reconcile` → `push(...)` → settings push; `push` takes `kiosk: bool` and `DayCtx` | the flip reuses this exact pass |
| Repaint driver | `start_repaint_driver` (`lib.rs:356`): repeated 100 ms timer, drains signals, wall-minute boundary | stays single-responsibility; rotation does NOT ride it (decision D2) |
| Adapter-side timer precedent | `status_timer: Rc<slint::Timer>` (`lib.rs:181/220`), armed by the adapter, mock-clock-fired in tests | the exact lifecycle pattern the rotation timer copies |
| Test clock | journey `TestingBackend` with `mock_time: true` (`tests/support/driver.rs:423`); `driver.advance_time(ms)` fires timers including slint timers (`driver.rs:59-61`) | the pause/advance seam — no sleeps anywhere |
| Mount hand-over | `mount(state, signals, system, commands, requests, settings)` (`lib.rs:193`); `UiSettings` (`lib.rs:62`) | gains one field (decision D2) |
| Appliance hand-over | `run_kiosk` (`main.rs:449`): resolves the pin (`main.rs:472`, the sole production caller of `resolve_fixed_board`), sets `ui_settings.kiosk = true`, mounts | gains the rotation hand-over lines |
| `[kiosk]` config | `KioskSection { fixed_board: Option<Uuid> }` (`config.rs:449`), `deny_unknown_fields`, absent table = default; `resolve_fixed_board(&[Uuid]) -> Result<Option<Uuid>, ConfigError>` (`config.rs:1012`); typed errors `UnknownFixedBoard { id }` (`config.rs:~690`), `InvalidWebConfig { field }` + `WEB_REFRESH_MIN_SECS = 5` floor precedent (`config.rs:554`) | grows rotation keys (decision D3) and the matrix cell (decision D4) |
| Non-committing exit | `discard_edit() -> [SetMode(Normal), RefreshNow]` (`view_state.rs:1068`) | the flip's flow end (decision D7) |
| Benches (must not break) | `VIEW_PINS [("today", false), ("board", true)]`, `mounted(n, kiosk)` (`benches/refresh_draw.rs:87/115`) | mount with `UiSettings { kiosk, ..default }` — the new settings field defaults `None`, benches byte-identical |
| Snapshots (must not change) | `tests/snapshots/keymap_snapshots__*.snap` — the registry + resolved presets | rotation is **not** an action: zero registry/keymap diff |

## 3. The settled decisions

### D1 — The state machine lives in a kiosk-shell struct, not in `ViewState`

**Decision**: a new pure type pair in `view_state.rs`:

```rust
/// The kiosk ambient display's modes (issue #363): the two
/// degenerations the rotation cycles. Declaration order carries the
/// config mapping; the ViewId mapping is total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KioskMode { Board, Today }

impl KioskMode {
    #[must_use] pub const fn view(self) -> ViewId; // Board → Board, Today → Today
}

/// The kiosk display-mode state machine (KDF-11, D-V3): the enabled
/// modes in their declared rotation order plus the dwell. Pure data —
/// the adapter owns the timer; `tick` is the only transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KioskRotation {
    modes: Vec<KioskMode>,      // validated ≥ 2 distinct, construction-site rule
    dwell: std::time::Duration, // validated ≥ floor at the config boundary
    active: usize,              // index into modes
}

impl KioskRotation {
    /// `modes` must hold ≥ 2 distinct entries — the config validation
    /// owns the refusal; this minting constructor is the trust
    /// boundary (the house A1 pattern: fallible, no derive crates).
    pub fn new(modes: Vec<KioskMode>, dwell: Duration)
        -> Result<Self, KioskRotationError>;
    #[must_use] pub fn active(&self) -> KioskMode;
    #[must_use] pub fn dwell(&self) -> Duration;
    /// The one transition: advance to the next mode in declared
    /// order (wrapping) and return it. No time lives here — the
    /// timer IS the dwell state.
    pub fn tick(&mut self) -> KioskMode;
}
```

with a private-field newtype error `KioskRotationError` (a small
`thiserror` enum, `thiserror` per AGENTS §4) distinguishing
`TooFewModes` / `DuplicateModes`. **Rationale**: `ViewState` is the
desktop navigation model — the focus walk, flows, subjects, widths.
Rotation is a shell concern over it; folding it into `ViewState`
would (a) put kiosk fields on every desktop construction, (b) force
`reconcile` to reason about rotation, and (c) blur the A1 trust
boundary (the config validation feeds a fallible minting
constructor, which `ViewState::new`'s infallible shape cannot
carry). The `Option<KioskRotation>` on the adapter makes the
desktop path structurally inert (`None` = no state, no timer, no
diff — the byte-identical guarantee is a type, not a gate). The
"no ring" non-goal is untouched: `modes` cycles *display modes of
one board*; boards never enter the machine.

The adapter applies a tick through one new pure `ViewState` method:

```rust
/// The kiosk rotation flip (issue #363, decision D7): end any flow
/// without committing, reset focus and the compact stack selection
/// to the mode's canonical head, and switch the view. Pure view
/// state; the only outcome is the repaint.
pub fn enter_kiosk_mode(&mut self, state: &AppState, mode: KioskMode) -> Vec<FlowOutcome>
```

`ViewState` gains no field. Mount seeding: with rotation present,
`install` seeds `new_starting(rotation.active().view())` so the
appliance opens on `modes[0]`; without it the existing Board seed
(and the journey pin) stands.

### D2 — The rotation timer is a dedicated SingleShot `slint::Timer`, not the repaint driver's tick

**Decision**: `mount` creates `rotation_timer: Rc<slint::Timer>`
beside `status_timer` (the same lifetime pattern) and `install`
stores it on the `Adapter` together with `rotation:
Option<KioskRotation>`. When rotation is present, install arms it:

```rust
slint::Timer::start(SingleShot, rotation.dwell(), closure)
```

The closure (weak window + `Rc<RefCell<Adapter>>` + the timer `Rc`,
the `start_repaint_driver` borrow pattern): upgrade the window →
`adapter.borrow_mut().rotation_tick(&window)` → re-arm with the
returned dwell. `Adapter::rotation_tick` does exactly: `let mode =
self.rotation.as_mut()...tick()`, `let outcomes =
self.view_state.enter_kiosk_mode(&state, mode)`, apply outcomes
(`RefreshNow` → `self.refresh(&window)`), clear the focus-scroll
hint (D7), return the dwell for re-arm; window-gone → stop (no
re-arm).

**Why not the repaint driver's tick**: the driver's 100 ms cadence
would quantize every dwell (drift), split one responsibility across
two owners, and make the driver's closure carry kiosk state. The
SingleShot re-arms with the *full* dwell each flip — dwell
semantics are exact and stateless (no accumulated-count field
anywhere; the timer is the dwell state — same philosophy as
`status_timer`'s auto-clear).

**Dwell semantics (record)**: `dwell_secs` is the per-mode display
time. Each flip re-arms the full dwell; there is no interaction
pause and no carry-over of remaining time (an operator's keystrokes
do not delay or extend anything — the display is passive per D-V3;
interaction-pause is a recorded out-of-scope idea for the K14
ledger). Rotation keeps flipping while an operator works; the next
tick snaps the window back to the rotation's active mode (recorded:
rotation wins over operator navigation — it is a display-mode
machine, not a navigation lock).

**Test seam**: the journey harness's mock clock (`advance_time`)
fires slint SingleShot timers deterministically — the same
pause/advance discipline the AGENTS §5 rule names for rotation. No
`tokio::time::sleep`, no real waits, anywhere.

### D3 — The `[kiosk]` rotation keys: exact names, types, validation

**Decision**: rotation keys join the existing `[kiosk]` section as
an optional nested table (the section is `deny_unknown_fields`, so
today's behaviour is preserved bit-for-bit when the table is
absent):

```toml
[kiosk]
fixed_board = "…"        # unchanged, optional

[kiosk.rotation]         # presence = rotation enabled
modes = ["board", "today"]  # required, 1..=2 entries, distinct
dwell_secs = 30             # required, u64, floor KIOSK_ROTATION_MIN_DWELL_SECS = 10
```

```rust
/// The kiosk rotation's deployment profile (issue #363, KDF-10's
/// reserved key space). An absent table decodes to the default (no
/// rotation) — the pre-K13 behaviour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotationSection {
    pub modes: Vec<KioskRotationMode>,  // required: no silent default
    pub dwell_secs: u64,                // required: no silent default
}

/// One rotation display mode. Lowercase TOML spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KioskRotationMode { Board, Today }
```

Validation (in `AppConfig::validate`, the `InvalidWebConfig`/`web.refresh_secs`
floor precedent):

- Both keys **required** inside a present table: an omitted key is a
  serde **decode error** (no `#[serde(default)]` — an implicit
  `modes = ["board","today"]` would be a silent default, the exact
  trap the dispatch plan names).
- Unknown keys: decode error (`deny_unknown_fields`, the
  `fixed_baord` typo-guard test's pattern).
- `modes.len() < 2` or duplicate entries → typed
  `ConfigError::InvalidKioskConfig { field: "kiosk.rotation.modes" }`
  (a one-mode "rotation" is a no-op wearing a config; a duplicate
  splits the dwell cycle pointlessly — both are config mistakes,
  not runtime fallbacks).
- `dwell_secs < KIOSK_ROTATION_MIN_DWELL_SECS` (new `pub const … :
  u64 = 10`, beside `WEB_REFRESH_MIN_SECS = 5`) → typed
  `ConfigError::InvalidKioskConfig { field: "kiosk.rotation.dwell_secs" }`.
  A wall display flipping faster than 10 s is a strobe, never an
  ambient cadence; the floor blocks typo-scale values without a
  quality ceiling (floors on input are sanctioned, AGENTS §5).
- The stable-discriminants test (`the_new_kinds_extend_the_stable_discriminants`)
  gains `InvalidKioskConfig` → `"kiosk-setting-invalid"`.

**Mapping to the UI crate (SRP)**: `config.rs` stays UI-type-free —
it validates and exposes `&Option<RotationSection>`. `run_kiosk`
maps it into the hand-over:
`ui_settings.kiosk_rotation = config.kiosk.rotation.as_ref().map(|r|
KioskRotation::new(r.modes.iter().map(KioskMode::from)…,
Duration::from_secs(r.dwell_secs)).expect("validated at boot"))` —
the `expect` is safe by construction (validation ran in
`validate()`; the comment states that constraint) and the mapping
sits in `main.rs` beside the existing `ui_settings.kiosk = true`
hand-over lines. **Desktop paths never read the section** (the
`[kiosk]` table is kiosk-mode-only, the `resolve_fixed_board` doc's
existing rule): `run_desktop` is untouched — the byte-identical
guarantee's second half.

### D4 — The pin-resolution matrix (absent pin × board count, every cell)

**Decision**: extend `resolve_fixed_board` in place (one resolution
site, its sole production caller is `run_kiosk`; update its doc to
the matrix). The new cell is **typed error** — the dispatch plan
explicitly offers "typed config error or the stage-1 plan's recorded
fallback", so this is a sanctioned option, not a ruling deviation:

| `fixed_board` | live boards at boot | resolution | test |
|---|---|---|---|
| `Some(pin)` | pin live | `Ok(Some(pin))` | exists (`resolve_fixed_board_resolves_a_live_board`) |
| `Some(pin)` | pin absent (unknown or deleted) | `Err(UnknownFixedBoard { id: pin })` | exists (`resolve_fixed_board_rejects_an_unknown_board_naming_the_pin`) |
| `None` | 0 | `Ok(None)` — boot proceeds; the view seeds empty; boards arriving later seed by the live-board rule | exists (`resolve_fixed_board_returns_none_without_a_pin`) |
| `None` | exactly 1 | `Ok(None)` — the seed rule (`normalize_board` → first live board) deterministically lands on that board | exists (same test, 1-element case) |
| `None` | ≥ 2 **at boot** | **`Err(KioskPinRequired { count })`** — new variant, discriminant `"kiosk-pin-required"` | new test |
| `None` | ≥ 2 **arriving post-boot** (sync pulls after a 0/1-board boot) | the documented fallback: the seed rule pins the first live board (canonical order); no runtime refusal — a wall going blank because data arrived is worse than a deterministic pick; recorded here so nobody "fixes" it into a ring or a crash | covered by the view-state seed rule's existing tests; the plan records the semantics |

Rationale for the new cell: a multi-board wall silently showing an
arbitrary board violates the deployment contract the pin was created
for ("the wall shows the pinned board or nothing boots",
`UnknownFixedBoard`'s doc); the multi-board ring is explicitly
backlog, so the only correct pin-less multi-board boot is a refusal
that names the remedy (set `fixed_board`). KDF-10's original
"absent pin + N = the ambient view's rotation scope" phrasing is
superseded by the dispatch plan's narrower reading (the pin is the
scope; D-V3 gives no ring) — recorded here per MD-5 so the
supersession is visible.

`KioskRotation` does not interact with the matrix: rotation cycles
*modes* of whatever board the pin (or seed) resolved.

### D5 — What each mode renders: the existing compositions, no new file

**Decision**: `KioskMode::Board` renders the existing
`board-view.slint` kanban; `KioskMode::Today` renders the existing
`today-view.slint` sections — through the existing
`current-view`/`View` conditional mount (`app-window.slint:216/248`).
**No `kiosk.slint` composition is created** (the implementation
plan's "or a kiosk composition over the existing views — stage 1
settles" settles: no). A wrapper would duplicate the model wiring of
both views for zero new pixels and drift against the desktop paths
— the opposite of the byte-identical discipline. The "ambient view"
is realized as: kiosk mount + the landed degeneration gates + the
rotation.

**Compact reading of D-V3 (recorded, so stage 3 can verify against
the ruling)**: no breakpoint is forced on kiosk. D-V3's kiosk
bullet states the today view's degeneration as "compact layout
(single column, no board badges, no input chrome — the Q8 compact
path already drops chrome)" and lands exactly one mechanism ("No
kiosk-specific code lands in this wave beyond the filter rule"):
the board-pin filter. The compact *machinery* is the breakpoint
path, driven by geometry — a kiosk window under 720 logical px
degenerates compactly on its own (R9 intact); at the D250's
1366×768 the board mode is the multi-column kanban (future-directions
§3.7's "kanban ⇄ upcoming-list" verbatim) and the today mode is the
stacked section list. The "no board badges" and "passive" halves are
the landed gates (`today_row_context` kiosk branch; inbox count 0;
notification surface empty; capture/palette keyboard-only with no
chrome advertising them). Forcing `Responsive` to Compact at full
width would invent a second breakpoint mechanism for a wall display
that reads better multi-column — an unhedged interpretation call,
recorded here; the alternative reading (pin Compact on kiosk) is a
one-site change in the width push if the user rules differently at
review.

### D6 — Banner / offline carry-over: no new wiring; assertions only

The `StatusBar` (banner slot + sync badge) sits in the window's
`VerticalLayout` **outside** the view switch (`app-window.slint:201–340`);
`push` derives `banner_rows(state, now)` and the board's sync badge
regardless of `vs.view()`, and rotation never touches
`ViewState.board` (the pin). Carry-over is therefore structural.
The chunk adds **zero wiring** and pins it: the rotation journey leg
asserts, across a flip, `banner-model` row count unchanged and
`sync-model.phase` unchanged (structural facts — enum + counts, no
text), in both modes.

### D7 — Focus/scroll across a flip: deliberate reset to the canonical head

**Decision (reset, not restore)**: every flip routes through
`enter_kiosk_mode` (D1), which, in order:

1. ends any active flow **without committing** — `discard_edit`'s
   exact non-committing exit (`[SetMode(Normal), RefreshNow]`),
   whose outcomes merge into the flip's (passive-display hygiene;
   an operator's half-open create row never commits on a tick);
2. sets `view = mode.view()`;
3. clears `focused_task = None`;
4. resets `selected_stack` to the board's first live stack (the
   compact leg's switcher lands deterministically at the head);
5. returns `[RefreshNow]` (the discard's `SetMode(Normal)` folds in).

**Why reset**: the two modes' row domains are disjoint (board
reading order vs today sections) — a "restored" offset is
meaningless across them, and a passive wall must show the head of
the current list, where the urgent rows live (overdue first in
Today; reading order first on Board). **Scroll positions reset by
construction**: the `if (root.current-view == …)` conditional mount
destroys the outgoing view's element tree, so the incoming
`ListView`s mount at `content-y = 0` — no scroll code exists on the
flip path. **#373's seam is untouched and explicitly re-armed**:
`push_focus_scroll_hint` still fires only on real focus moves; the
flip additionally resets the `FocusScrollHint` global
(`target_a = -1`, `target_b = -1`, `nonce += 1`) inside
`rotation_tick` so a stale board-column hint can never be
mis-read by the Today sections' bindings (the nonce bump is a
guarded no-op seek: `scroll-row-into-view` returns on `row < 0`).
The one behaviour this protects — keyboard focus moves scrolling
the focused card into view — is owned by the existing #373 tests,
which must stay green unmodified (stage 3's trap list names it).

### D8 — The no-engine-command structural assertion

Three layers, in decreasing strength:

1. **Structural by omission**: rotation never enters the action
   layer. `actions.rs`/`router.rs` take a **zero-line diff** — no
   `ActionId`, no `UiEffect`, no registry row. The router cannot
   emit a rotation command because rotation has no path into it
   (the implementation plan K13 files list named these files as
   *guard* sites, not change sites — stage 1's re-location records
   that the guard is the empty diff).
2. **Unit scan test** (the lowest layer that owns the outcome
   vocabulary): `enter_kiosk_mode`'s and `KioskRotation::tick`'s
   combined outcomes, asserted across a sweep of starting states
   (each `Flow` variant active × each `KioskMode`) to contain no
   `FlowOutcome::Command(_)` —
   `rotation_transition_never_produces_an_engine_command`
   (`view_state.rs` test module).
3. **Journey wiring check**: the rotation journey leg drains the
   `FakeEngine` command log after N dwell advances and asserts it
   empty (the harness's existing command-log pattern; this is the
   end-to-end witness the dispatch acceptance names).

### D9 — Test plan (TDD order) and the journey legs

Unit tests land **first** per milestone, red, then the production
change greens them:

**M1 — config surface** (`crates/taskboard-app/src/config.rs`,
test module beside the chunk-9 kiosk tests):
- `rotation_section_absent_decodes_to_no_rotation` (absent table +
  `deny_unknown_fields` behaviour unchanged);
- `rotation_section_decodes_modes_and_dwell` (both keys, enum
  spellings `board`/`today`, order preserved);
- `rotation_missing_modes_is_a_decode_error` /
  `rotation_missing_dwell_is_a_decode_error` (no silent defaults);
- `rotation_unknown_key_is_a_decode_error` (the typo guard);
- `rotation_needs_two_distinct_modes_is_typed` (< 2, and the
  duplicate case, both `InvalidKioskConfig { field: "kiosk.rotation.modes" }`);
- `rotation_dwell_below_the_floor_is_typed` (9 → error naming
  `kiosk.rotation.dwell_secs`; 10 → passes — floor, not ceiling);
- `resolve_fixed_board_refuses_an_unpinned_multi_board_boot` (the
  new matrix cell: `None` pin + 2 live ids → `KioskPinRequired { count: 2 }`);
- `the_new_kinds_extend_the_stable_discriminants` extended
  (`InvalidKioskConfig`, `KioskPinRequired`).

**M2 — the state machine** (`view_state.rs` test module):
- `kiosk_rotation_mints_only_distinct_mode_pairs` (`new` rejects
  < 2 / duplicates — the constructor-side mirror of the config rule);
- `kiosk_rotation_ticks_in_declared_order_and_wraps`
  (`["today","board"]` starts Today — declared order, not a
  hardcoded one; ticks cycle);
- `enter_kiosk_mode_resets_to_the_mode_head` (view set; flow ended
  — asserting `Flow::None` and `buffer` empty from an active
  `Flow::Create`; focus `None`; stack selection = first live stack);
- `rotation_transition_never_produces_an_engine_command` (D8.2's
  sweep over flows × modes).

**M3 — the timer + hand-over wiring** (`lib.rs` unit test beside
`mount_applies_the_kiosk_fullscreen_profile`, plus the journeys):
- `mount_without_rotation_arms_nothing` (default settings — the
  byte-identical desktop witness at the unit level: no timer work,
  `UiSettings::default().kiosk_rotation == None` constructed, mount
  shape unchanged);
- rotation arming itself is exercised end-to-end by journey leg J-K1
  (unit-testing a slint Timer's re-arm without the mock clock would
  re-implement the platform — the journey IS the lowest layer that
  owns timer firing; recorded as the test-home decision).

**M4 — journey legs** (`tests/journeys.rs`, `driver::run` +
`Harness::mount_with_settings`, mock clock only):
- **J-K1 the rotation flips**: mount `kiosk: true` + rotation
  `{["board","today"], dwell 30 s}` on the multi-board fixture;
  assert start view = modes[0]; `advance_time(29_999)` → no flip;
  `advance_time(1)` → `View::Today`; again → `View::Board`;
  `focused_task` none after each flip; FakeEngine command log
  empty across all advances (D8.3); `banner-model` length and
  `sync-model.phase` unchanged across flips (D6).
- **J-K2 the passive reading**: same mount, flipped to Today;
  assert the today model's per-section row counts equal the pinned
  board's `today_domain` (the board-filter at the surface);
  `toolbar_inbox_count == 0` and `notification_model.length == 0`
  in **both** modes (the kiosk gates hold across rotation); the
  flip back renders the board.
- **J-K3 the desktop is inert**: default settings (no kiosk bit);
  `advance_time` past several would-be dwells → view stays
  `View::Today`, no flips, command log unchanged — the
  byte-identical acceptance's behavioural witness (the existing
  journeys stay green unmodified, which is the stronger half).

**M5 — goldens**: two new scenes, deliberate first-record pass:
`the_kiosk_board_golden_holds` and `the_kiosk_today_golden_holds`
(`tests/golden_visuals.rs`, one matrix cell each — the mid cell,
scale from the existing matrix, `kiosk: true` + rotation in the
`UiSettings`, capture after one forced flip so *both* modes' goldens
exist; new files `kiosk-board-*.png` / `kiosk-today-*.png` under
`tests/goldens/`). **Zero existing-golden drift is permitted** — the
chunk changes no desktop markup; a drifted board/today golden is a
finding, not a regen. **Insta scope: none** — no action, keymap, or
registry change (`tests/snapshots/keymap_snapshots__*.snap`
untouched); the domain/state crates are not touched at all. No
`.pending` files ship.

## 4. Files changed (the whole diff)

| File | Change |
|---|---|
| `crates/taskboard-app/src/config.rs` | `RotationSection`, `KioskRotationMode`, `KIOSK_ROTATION_MIN_DWELL_SECS`, `KioskSection.rotation` field, `InvalidKioskConfig` + `KioskPinRequired` variants, `validate()` arms, M1 tests |
| `crates/taskboard-app/src/main.rs` | `run_kiosk`: the rotation hand-over mapping (D3) — a few lines inside the existing `ui_settings` block |
| `crates/taskboard-ui-slint/src/view_state.rs` | `KioskMode`, `KioskRotation` (+ `KioskRotationError`), `ViewState::enter_kiosk_mode`, M2 tests |
| `crates/taskboard-ui-slint/src/lib.rs` | `UiSettings.kiosk_rotation` field (+ `Default::None`), `rotation_timer` creation + `install` arg, M3 unit test |
| `crates/taskboard-ui-slint/src/adapter.rs` | `Adapter.rotation` + `Adapter.rotation_timer` fields, `install` param + seeding/arming, `Adapter::rotation_tick`, the hint reset (D7) |
| `crates/taskboard-ui-slint/tests/journeys.rs` | J-K1/J-K2/J-K3 |
| `crates/taskboard-ui-slint/tests/golden_visuals.rs` + `tests/goldens/` | the two kiosk scenes |
| `CHANGELOG.md` | one Unreleased line |
| `crates/taskboard-ui-slint/src/actions.rs`, `router.rs`, `models.rs`, `presentation.rs`, any `.slint` file, `benches/` | **zero diff** (D8.1; the bench seams and #373 surfaces are consumed untouched) |

SPDX headers: only existing files change (no new `.rs` file — the
state machine lives in `view_state.rs`, config types in
`config.rs`); the crate rules (AGPL-3.0-only UI, `forbid(unsafe_code)`
everywhere else) are therefore inherited, not newly applied.

**Estimated diff: ~700–800 lines, of which ~450 are tests** (config
~200, view_state ~180, journeys ~200, goldens/harness ~80, adapter +
lib ~90, main.rs ~10, CHANGELOG ~3). An M-size chunk on the house
scale.

## 5. Risks & stage-3 focus

1. **The stale-hint trap** (D7): a board-column `FocusScrollHint`
   surviving into a Today flip could mis-seek a section. The reset
   + nonce bump closes it; stage 3 verifies the reset exists in
   `rotation_tick` and that the #373 journeys stay unmodified.
2. **Timer lifecycle**: the rotation timer must stop when the slint
   loop ends — the `Rc` drops with the `UiHandle`/`Adapter` (the
   `status_timer` precedent). Stage 3 checks no strong
   `AppWindow`/adapter cycle is created by the closure (weak
   upgrade pattern only).
3. **`expect` at the hand-over mapping** (D3): safe only because
   `validate()` ran; the comment must state that constraint so a
   future caller cannot bypass validation silently. Stage 3 judges
   whether the mapping should instead be a fallible helper — either
   is acceptable; silence about the constraint is not.
4. **The re-entrancy of the tick closure**: it borrows the adapter,
   refreshes, and re-arms inside a timer callback — the same shape
   the repaint driver uses; stage 3 confirms no
   `RefCell` double-borrow path (the closure upgrades the window
   before borrowing, mirrors `start_repaint_driver`).
5. **The matrix cell is a behaviour change** for existing pin-less
   multi-board kiosk configs (they now refuse at boot): deliberate,
   sanctioned by the dispatch plan's "typed config error" option,
   recorded in D4's table. The K14 exit report should mention it.
6. **Test flakiness**: none may be introduced — mock clock only,
   no sleeps, no wall-clock assertions; goldens ride the existing
   deterministic renderer rules (ASCII titles, bundled fonts).

## 6. Handoff

Stage 2 implements M1→M5 in order (TDD, standalone commits), runs
the full AGENTS §10 gate suite (fmt, clippy `-D warnings`, nextest,
doc tests, `cargo deny`, the CI-identical three-command llvm-cov
sequence with the quarantine step), adds the CHANGELOG line, and
reports per-gate results + deviations. Stage 3 rebases onto
`origin/main`, reviews against the acceptance list (dispatch plan
§2: timer flips with no input; the pin matrix per D4; banner +
offline in both modes; no capture affordance; desktop byte-identical;
the structural no-command assertion; gates green; PR body
`Closes #363`), re-runs the gates, ships the PR, and tears the
worktree down. No issue edits anywhere (MD-2).

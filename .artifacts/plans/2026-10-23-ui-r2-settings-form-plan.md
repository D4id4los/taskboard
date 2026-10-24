# [Plan] GUI Review Round 2 — Chunk 5: Settings form, feedback, and help (D26–D31)

- **Date**: 2026-10-23 (stage-1 planning agent run 2026-10-24)
- **Kind**: chunk plan (three-stage dispatch, chunk 5 of 8) — TDD-first,
  no implementation in the planning stage.
- **Origin**: design round
  `.artifacts/plans/2026-10-23-gui-review-round2-plan.md` §6 (D26–D31,
  normative), facts F-C1/F-C2/F-C3/F-C6 (+ F-C4/F-C5 inheritance),
  §10 testing strategy, §11 code inventory, §14-R2/R5; dispatch plan
  §1/§6 (chunk 5, branch `feat/ui-r2-settings-form`).
- **The R1-R5 reversal (recorded, user-directed)**: this chunk
  **reverses round-1 R5** ("the tones, never message text" in
  settings) — the status line renders mapped human text (D28). The
  design round records the reversal explicitly (§0 "Round-1
  relationship"). **The no-text-assertions TEST rule still binds in
  full** (§14-R5, dispatch §1 code-quality mandate): status text and
  help texts are pinned by mapping *coverage/totality* and
  *non-emptiness* — never by string content, substrings, or
  disjunctions.
- **Precondition gate (passed)**: chunk 4
  (`feat/ui-r2-taskview-textfields`) is merged into `origin/main` —
  verified via `git log origin/main --oneline` (tip `0ffd29a`, the
  D25 review-nits fix; branch deleted post-merge). Worktree created
  off fetched `origin/main` after a clean `scripts/worktree-doctor.sh`
  run (free 73 G, floor 25 G).
- **Scope guard**: `taskboard-domain`, `taskboard-state`,
  `taskboard-storage-sqlite`, `taskboard-sync-nextcloud` diffs are
  **empty**; reviewers assert it. `taskboard-app` gets **one recorded
  exception** (§3, D-B: the `KeymapUpdate.preset` fill — 3 lines in
  `ui_requests.rs`); the dispatch plan §6 permits an app diff "with a
  recorded reason". `ui_settings.rs` (app-side tests) must stay green
  **unchanged**.

## 1. Verified facts (anchored, post-chunk-4 `main`; worktree line numbers)

- **F1 (F-C1, scroll trap)** the settings view is one `VerticalLayout`
  under a FocusScope (`ui/views/settings-view.slint:135-336`); the only
  in-repo `Flickable` is the action menu's
  (`ui/compositions/action-menu.slint:110-232`) with its scroll
  indicator at 237-257. At scale 3.0 the content exceeds the viewport
  with no scroll (F17). The menu records the Slint fact that
  **max-height is not honoured on a Flickable-hosted panel** — the
  panel binds its own height (action-menu.slint:57-61).
- **F2 (F-C2, picker)** the keymap picker is three `SettingsButton`s
  with the inverted-`enabled` hack `enabled: preset-index != N`
  (settings-view.slint:274-297); `preset-index` is written exactly
  once, at mount (`src/adapter.rs:1540-1547`), and `push_settings`
  (1234-1275) never writes it — the selection cannot move (F14a).
  `apply_keymap_update` (1223-1228) swaps only `self.keymap` and the
  findings counter. `self.keymap =` has **exactly one site in the
  crate** (1224, the reply drain) — the reply lane is the only keymap
  writer.
- **F3 (F-C3, marker)** the outcome marker is a bare `Rectangle`
  (`min-height: space-3`, no text, no cap, never reset)
  (settings-view.slint:325-334); with all cells stretch-0 the runtime
  hands it leftover height (the big green bar, F14b).
- **F4 (F-C6, scale)** the scale row is a label + `TextField`
  committing on Return (`scale-committed`, settings-view.slint:305-323;
  `settings_scale_committed`, adapter.rs:1443-1468): parse → finite +
  0.5–3.0 gate → live-apply `Theme.base-scale` → `SetScale` over the
  lane with the previous value parked; the Err reply reverts
  (adapter.rs:1184-1198). Semantics sound; only the widget form (F16)
  and the scroll trap are wrong.
- **F5 (outcomes)** the adapter folds every `SettingsError`
  (13 variants, `src/requests.rs:66-95`; `Field`×`InvalidKind`
  4×3 lattice at 98-116) into five coarse tones via
  `SettingsVm::outcome_of` (adapter.rs:1056-1073), pushed as the Slint
  `SettingsOutcome` enum (settings-view.slint:20-27) at exactly one
  site in `push_settings` (adapter.rs:1267-1274). The concrete error
  is dropped at the fold — D28's fine text needs it retained (§4).
- **F6 (gesture precedent)** the column drag is the in-repo
  press/tick/release trio: `width_drag: Option<(StackId, f32)>` on the
  adapter (adapter.rs:124), `on_board_stack_width_press` seeds,
  `_drag` ticks live, `_release` persists exactly once via
  `take()` (adapter.rs:1648-1726). D29's slider gesture mirrors this.
- **F7 (mock clock)** the testing backend's `mock_elapsed_time` fires
  real `slint::Timer`s: the tooltip delay test advances 599→1→600 ms
  (`tests/usability_components.rs:132-233`;
  `tests/support/driver.rs:165-167 advance_time`). D28's auto-clear
  uses the identical mechanism — no sleeps, no callback-seam
  indirection needed.
- **F8 (reply mechanics)** journeys play the app half:
  `harness.drain_requests()` pulls the lane, the test sends on the
  oneshot, `harness.handle.drain_setting_replies()` applies — J7 at
  `tests/journeys.rs:2332-2391` does exactly this for the preset flow,
  **constructing the `KeymapUpdate` literal itself** (2368-2375).
- **F9 (app-side preset handler)** `set_keymap_preset`
  (`crates/taskboard-app/src/ui_requests.rs:394-415`) builds
  `KeymapUpdate` with the `Preset` **in scope**; `Keymap`
  (`src/keymap/mod.rs:22-27`) carries no preset identity, and deriving
  one by diffing against preset tables is unsound under user
  overrides.
- **F10 (chunk-4 inheritance)** `TextField` is a layout citizen
  (content-free `min-width: Theme.space-15`,
  `ui/widgets/text-field.slint:40-42`) with the checkmark
  `commit-affordance` (94-125); the settings `LabeledField`s already
  pass `commit-affordance: true` (settings-view.slint:110). Chunk 5
  reuses the primitive **unchanged** (chunk-4 plan §3: "chunk 5's
  slider+field reuses the primitive unchanged").
- **F11 (refresh pass)** `refresh` ends with `push_settings`
  (adapter.rs:380-402, the call at 401); every refresh re-pushes all
  settings globals. Consequences: (a) the D27 echo rides every refresh
  for free; (b) the status expire must reset the **VM** (not just the
  property), or the next refresh resurrects the status.
- **F12 (Slint 1.18.1 a11y surface, probed on the pinned crate
  `i-slint-common-1.18.1 enums.rs:466-540`,
  `i-slint-compiler-1.18.1 typeregister.rs:269-292`)**:
  `AccessibleRole` has `RadioGroup` ("a container grouping related
  RadioButtons"), `RadioButton`, `Slider`; the reserved properties
  include `accessible-checked: bool` (two-way-bindable),
  `accessible-value`, `accessible-value-minimum/-maximum/-step`, and
  the `accessible-action-increment/-decrement` callbacks. Every a11y
  decision below is directly expressible — no fallback probes remain
  on the role surface.
- **F13 (tests today)** journeys J1–J6 (`tests/journeys.rs:1963-2326`)
  pin open/close, menu routing, form save routing + redaction, row
  liveness; **J7** (2332-2391) pins the preset flow. App-side
  `crates/taskboard-app/tests/ui_settings.rs` pins the ceremony over
  wiremock; its preset test (470-531) reads only `update.keymap` — a
  new `KeymapUpdate` field compiles and stays green.
- **F14** `KeyFeedback` is declared but never rendered
  (app-window.slint:36-40,71) — not the green box, out of scope
  (design F-E8).

## 2. D26 — `SegmentedControl` (F15) + D-G the inverse-enabled purge (F-C2)

### 2.1 API (recorded)

New L1 primitive `ui/widgets/segmented-control.slint`:

```slint
export component SegmentedControl inherits Rectangle {
    in property <[string]> options;      // segment labels, index order is the contract
    in property <int> selected-index: 0; // exactly-one by construction; no "none" state
    in property <string> group-label;    // the a11y name of the group
    callback picked(int);
}
```

- **Selection model**: a single int index owned by the caller —
  no internal selection state, no none/empty state (the settings mount
  always seeds a valid index; `UiSettings.keymap_preset` is total).
  Two-way data flow is avoided deliberately: the widget renders the
  caller's index and emits `picked`; the caller decides what the index
  means (this is what makes D-B's reply-sourced echo possible at all).
- **Container paint**: `radius-small` border at `focus-ring-width`
  in `Theme.border`, `Theme.surface` background — the grouped look
  (design D26: "one grouped container"). Segments: a `HorizontalLayout`
  of equal cells (`horizontal-stretch: 1` each).
- **Segment paint**: selected → `Theme.surface-raised` background +
  `RoleText` `text-primary`; unselected → transparent + `text-secondary`;
  unselected hover → `Theme.surface-hover` (the established hover
  language, action-menu rows / board-bar segments).
- **A11y (F12, all probed present)**: container
  `accessible-role: AccessibleRole.RadioGroup` +
  `accessible-label: group-label`; each segment carries a FocusScope
  overlay (the icon-button pattern, `ui/widgets/icon-button.slint:55-64`:
  `focus-on-tab-navigation: true`) with
  `accessible-role: AccessibleRole.RadioButton`,
  `accessible-label: <option text>`,
  `accessible-checked: selected-index == <index>`, and
  `accessible-action-default => picked(<index>)`. The TouchArea click
  hits the same `picked` — click ≡ AT-activation.
- **Keyboard operability, recorded honestly**: AT activation (tab +
  default action) is the keyboard path. The app's own action layer has
  no `settings.preset-*` registry action and this chunk does not invent
  one (the registry round-up was chunk 3's closed scope); the picker is
  pointer- and AT-operable, matching the icon-button precedent.
- **Reuse story (recorded)**: any N-options-one-choice row. This
  chunk's consumer is the keymap picker (N=3). Chunk 6's backend picker
  is *not* one (it is a disabled-rows list); chunk 6's board selector
  (D32) *may* become a consumer — its stage 1 decides. A second real
  consumer is the trigger to extract shared row chrome, not before.
- **Index-order contract (recorded)**: `[Standard, Emacs, Vi]` — the
  same contract the adapter's request seam already implements
  (`[Preset::Standard, Preset::Emacs, Preset::Vi].get(index)`,
  adapter.rs:1417-1421) and the mount seed pushes today. Documented at
  the use site; segment labels are markup-owned strings (as the three
  buttons' are today).

### 2.2 The purge

The three `SettingsButton`s with `enabled: preset-index != N`
(settings-view.slint:274-297) are replaced by one `SegmentedControl`
consuming `SettingsModel.preset-index` and emitting
`root.preset-picked(i)` — the window seam (adapter.rs:2578-2583) is
unchanged. The `SettingsButton` component itself stays (Save/Test/Remove
still use it).

### 2.3 Tests

- usability-components-level pin (through the app window, settings
  open): the three segments are addressable by label
  ("Standard"/"Emacs"/"Vi"), expose `RadioButton` roles (small driver
  extension: `Element::is_radio_button()` beside `is_accessible_button`,
  driver.rs:130-132 — the read already exists on `ElementHandle`), and
  clicking a segment routes exactly one `SetKeymapPreset` (the J7
  mechanics).
- `accessible-checked` readability through the testing backend is the
  one open probe: **primary** — pin "exactly one segment checked, and
  which one follows `preset-index`" via the a11y read if the backend
  exposes it (probe at stage-2 start); **fallback** — the checked
  *state* contract pins on `SettingsModel.preset-index` (the model the
  widget renders from) + the echo journey, and the visual
  single-selection reads as a Tier-4 item. Either way the role pins and
  the click-equivalence pin land.
- The inverted-`enabled` hack's death is a *structural* absence:
  no `enabled:` binding on preset rows (grep-reviewed; no new test).

## 3. D27 — the keymap selection echoes the **replied** preset (F-C2/F14a)

### 3.1 Echo source (recorded)

**`KeymapUpdate` grows `preset: Preset`** (`src/requests.rs:121-125`).
The app side fills it in `set_keymap_preset`
(`ui_requests.rs:394-415`, the `Preset` is already the function's
parameter).

**Recorded reason for the app-crate diff** (dispatch §6 gate): the
review contract forbids a click-echo — the selection must reflect the
*replied* preset. The replied payload names no preset today (F9), and
reconstructing one from the built `Keymap` is unsound under user
overrides (a heavily-overridden emacs keymap is not `Keymap::from_preset`
comparable). One honest field on the reply is the minimal fix; it rides
an existing lane message with no ceremony change. `ui_settings.rs`
stays green unchanged (F13).

### 3.2 Echo path (which function writes `preset-index`, and when)

- `SettingsVm` gains `preset: Preset` (default `Standard`), seeded in
  `install` from the existing `preset` arg (adapter.rs:1530-1533). The
  ad-hoc mount write at 1540-1547 dies — the first `refresh` (lib.rs
  mount calls it immediately) pushes it via the normal path. One write
  path total.
- `apply_keymap_update` (1223-1228) additionally sets
  `self.settings.preset = update.preset` — the *only* place the VM's
  preset changes after mount.
- `push_settings` (1267 area) additionally writes
  `global.set_preset_index(preset_index(self.settings.preset))` — new
  `fn preset_index(Preset) -> usize` helper (single source, replaces
  the mount match). Because every refresh ends in `push_settings`
  (F11), the model always mirrors the adapter's last-known persisted
  preset.
- **REPL/config-initiated changes**: the reply lane is the only keymap
  writer in the crate (F2 — single `self.keymap =` site). A REPL-side
  config edit today reaches a *running* UI only if some future lane
  message swaps `self.keymap`; the D27 design is future-proof for that
  by construction — any such swap lands on the next `push_settings`
  with no further code. Recorded as the honest scope: the echo covers
  every change that reaches the adapter's keymap state.

### 3.3 Tests (J7 extended, not weakened; the echo gets its own journey)

- **J7 (`the_preset_pick_routes_and_the_reply_swaps_the_keymap_live`,
  journeys.rs:2332-2391)** — every existing assertion stays verbatim;
  the only edit is the reply literal gaining
  `preset: Preset::Vi` (compile-required by §3.1).
- **New J8 `the_preset_selection_echoes_the_replied_preset_not_the_click`**:
  mount (standard, index 0) → open settings → `invoke_settings_preset_picked(2)`
  → **index still 0** (no optimistic click-echo) → reply
  `KeymapUpdate { preset: Vi, .. }` → drain → **index 2** → a synthetic
  second reply for Emacs → drain → **index 1** (the reply, not the
  click, is the source; covers "a synthetic reply for a different
  preset moves it", dispatch §6 acceptance b).

## 4. D28 — the status line replaces the tone marker (F-C3, F21-feedback)

### 4.1 Mapping table — new `src/settings_status.rs` (recorded location)

One module owns the status line's presentation policy (text, lifetime;
the dot's colour stays markup-side). SPDX `AGPL-3.0-only` (ui-crate
tier). API:

```rust
pub const STATUS_VISIBLE_MS: u64 = 5_000;

pub fn status_text(outcome: SettingsOutcome, detail: Option<&SettingsError>)
    -> &'static str;
fn error_text(err: &SettingsError) -> &'static str;      // exhaustive, 13 arms
fn invalid_text(field: Field, kind: InvalidKind) -> &'static str; // exhaustive 4×3
```

- **Totality is compiler-enforced**: every `match` is exhaustive over
  its enum with **no wildcard arm** — a future `SettingsError`/`Field`/
  `InvalidKind`/`SettingsOutcome` variant fails to compile here until
  mapped (the review trap (c), dispatch §6).
- `status_text` arms: `None` → `""` (hidden); `Busy` → the
  operation-in-progress sentence; `Ok` → the done sentence;
  `Rejected`/`Unavailable`/`Failed` → `detail.map(error_text)`
  unwrap-or the outcome's generic sentence (the seam-local rejections —
  parse miss, pending gate — have no `detail`).
- `error_text` gives F21's typed granularity (bad URL format vs
  unreachable vs credential refusal vs store failure vs …), one arm per
  `SettingsError` variant, with `Invalid` delegating to the
  `Field`×`InvalidKind` lattice. Strings are short, present-tense,
  user-actionable; their *content* is never asserted anywhere (§7).
- **Duration (recorded)**: `STATUS_VISIBLE_MS = 5_000` — long enough
  to read a short sentence at any scale, short enough to feel
  ephemeral; distinct from the tooltip's 600 ms *arming* delay (a
  different thing it must not be confused with). One constant, one
  place to re-tune.

### 4.2 Adapter state machine

- `SettingsVm` gains `detail: Option<SettingsError>` (the concrete
  error retained past the coarse fold — F5). The Slint
  `SettingsModel` gains `in-out property <string> status-text: ""`.
- **New single choke point**
  `fn set_settings_outcome(&mut self, window, outcome, detail: Option<SettingsError>)`:
  assigns outcome + detail, pushes settings, drives the timer. Every
  existing assignment site routes through it (~14 sites: the drain arms
  1140-1203, `send`'s channel failure 1049, the save/test/remove seams'
  Busy + invalid-draft Rejected (1055, 1076, 1103, 1126, 1410), the
  preset/scale seams (1437, 1446, 1451, 1466)). No outcome write
  bypasses it afterwards (grep-reviewed).
- **Timer state machine (recorded exactly)**:

| transition | outcome shown | timer |
|---|---|---|
| `set(Busy)` | busy text | `stop()` — a pending op must never auto-clear mid-flight |
| `set(terminal: Ok/Rejected/Unavailable/Failed)` | terminal text | `restart(SingleShot, STATUS_VISIBLE_MS)` — every new outcome re-arms |
| timer fires → `settings_status_expired(window)` | if `pending.is_some()` → **no-op** (a stale fire during a new Busy); else `None` + `detail = None` + push | `stop()` |

- **Where the timer lives**: `status_timer: Rc<slint::Timer>` created
  in `lib.rs::mount` beside `install_tooltip_delay`, passed into
  `adapter::install` (new arg; `install` is `pub(crate)`, only lib.rs
  calls it — no app-side ripple), stored on the `Adapter`;
  `UiHandle` holds its own `Rc` clone for lifetime exactly like
  `tooltip_timer` (lib.rs:144-150). The fire closure holds
  `Weak<RefCell<Adapter>>` + `Weak<AppWindow>` (no Rc cycle). Fires
  under the testing backend's mock clock (F7) — tests drive it with
  `driver.advance_time`, the 599/600 pattern.
- **Markup**: the bare marker (settings-view.slint:325-334) becomes a
  status row: `HorizontalLayout { StatusDot, RoleText(status-text) }`
  with **`min-height == max-height` == one control row** (the
  `SettingsButton` row formula: `label.preferred-height + 2·space-2`)
  and **`vertical-stretch: 0`** — structurally unable to absorb layout
  height (the F-C3 mechanism, review trap (b)). `visible:
  last-outcome != none`.
- **Tone dot**: the shipped colour conditional formalized into a
  `StatusDot` (the `RowDot` shape, settings-view.slint:53-63):
  ok → `Semantic.success`, busy → `Semantic.sync-active`
  (one deliberate fix: today's busy tone is `Theme.surface-raised`,
  invisible on the surface — recorded), rejected/unavailable →
  `Semantic.warning`, failed → `Semantic.danger`. Tones are markup
  conditionals over the model enum — no new theme tokens.

### 4.3 Tests

- `settings_status.rs` unit tests:
  - the `Field`×`InvalidKind` lattice loop (12 combos) → non-empty;
  - the `SettingsError` totality loop (all 13 variants) → non-empty;
  - the coarse-outcome loop → `None` → empty, the other five →
    non-empty;
  - **the mapping-totality proptest**
    (`proptest!`, `proptest` is already a dev-dependency —
    `Cargo.toml:51`, the `widths.rs` precedent): a `Strategy`
    generating every variant (a `prop_oneof!` over the 13, `Invalid`
    generated over the lattice) asserts `status_text(Rejected, Some(e))`
    non-empty and — the part the compiler *can't* see — that the
    generator's variants are exactly the enum's (a new variant breaks
    the compile; a *removed* one breaks the generator's exhaustiveness
    match arm listing).
- Journeys (settings block, after J8):
  - `a_terminal_status_shows_text_then_auto_clears_under_the_mock_clock`:
    `invoke_settings_scale_committed("not a number")` (seam-local
    Rejected, no lane round-trip) → status visible, `status-text`
    non-empty, `last-outcome == Rejected` → `advance(STATUS_VISIBLE_MS - 1)`
    still visible → `advance(1)` hidden (`last-outcome == None`,
    text empty).
  - `a_new_outcome_re_arms_the_status_clear`: terminal outcome →
    advance 3_000 → second terminal outcome → advance 3_000 (7 s after
    the first) **still visible** → advance 2_000 → hidden (the restart
    pin, the tooltip-restart test's shape).
  - `busy_status_holds_until_the_reply_lands`: valid-draft Save click →
    Busy → advance 2·STATUS_VISIBLE_MS → **still visible** (busy never
    auto-clears) → reply `Ok(rows)` + drain → Ok visible → advance
    STATUS_VISIBLE_MS → hidden.
- Non-emptiness is the sanctioned assertion shape for text (design §10,
  F19/D31 precedent: "each field's tip content is non-empty — never the
  text"); no string content, substring, or disjunction assertion
  anywhere.

## 5. D29 — the scale becomes slider + field (F16)

### 5.1 Slider anatomy (new L1 `ui/widgets/slider.slint`)

```slint
export component Slider inherits Rectangle {
    in property <float> minimum;
    in property <float> maximum;
    in property <float> step;              // tick snapping
    in property <float> value;             // one-way: rendered, not owned
    callback preview(float);               // during the gesture
    callback committed(float);             // release / AT step
}
```

- Paint: full-width track (`focus-ring-width` tall, `Theme.border`,
  pill radius), filled portion in `Semantic.focus-ring`, round thumb
  (`Theme.space-4`, `surface-raised` + focus-ring border).
- Gesture: a track-covering `TouchArea`; while pressed, pointer x →
  `value = minimum + round((raw/width · range)/step)·step`, clamped →
  `preview(v)` per tick; release → `committed(v)`. **Step-snap on
  every tick** (0.1 for the scale — F16's "≈0.1 steps").
- A11y (F12, probed): `accessible-role: Slider`,
  `accessible-value`/`-minimum`/`-maximum`/`-step` set, and
  `accessible-action-increment/-decrement` stepping ±step through the
  **same** `preview`→`committed` path (one AT action = one commit —
  the AT has no gesture to release, so its action *is* the commit).
- `value` is one-way (`in`): the widget renders what it is told. The
  scale row binds `value: Theme.base-scale`, so live-applied commits,
  reply reverts, and the preview itself all move the thumb — one source
  of truth, no duplicate state.
- Keyboard users keep the text field (design D29: "the field stays
  authoritative"; the slider's keys are the AT actions).

### 5.2 Seams and semantics (press / preview / release — the width-drag trio, F6)

- `app-window.slint` grows three callbacks:
  `settings-scale-pressed()`, `settings-scale-preview(float)`,
  `settings-scale-released(float)`.
- Adapter state: `scale_drag: Option<f64>` (the revert point — the
  pre-gesture `base-scale`), mirroring `width_drag` (adapter.rs:124).
- **pressed**: `pending.is_some()` → ignore (the gate is silent, the
  existing `settings_preset_picked` precedent, 1426-1428); else record
  `Some(current base-scale)`.
- **preview(v)**: `scale_drag.is_none()` → ignore; else clamp
  0.5–3.0 → `Theme.set_base_scale(v)` — **dispatches nothing, writes
  no outcome, pushes nothing** (pure visual preview; `Theme` is
  seam-written only).
- **released(v)**: `take()` the drag; if it was `None` → ignore; gate
  `pending.is_some() || !v.is_finite() || !(0.5..=3.0).contains(&v)` →
  revert `base-scale` to the recorded pre-gesture value +
  `set_settings_outcome(Rejected, None)`; else
  `apply_scale_commit(window, v, previous = recorded value)` —
  **the shared commit core** (live-apply, send `SetScale`, Busy), which
  the field path (`settings_scale_committed`, 1443-1468) is refactored
  to call with `previous =` its own pre-commit `base-scale`. Both paths
  clamp identically by construction (same gate expression, same
  revert-on-Err reply arm at 1184-1198 — the Err reply's revert restores
  `previous`, which for the slider is the pre-**gesture** value, so a
  failed commit undoes the whole preview).
- **One request per gesture, by proof**: the preview arm contains no
  `send` and no `UiRequest` construction — the only lane write is in
  `apply_scale_commit`, reached once per release.
- The field keeps Return-commit semantics and stays authoritative
  (unchanged code path; D9-of-round-1 preserved).

### 5.3 Tests

- `the_slider_previews_live_and_commits_once_per_gesture`: pressed →
  five previews (in-range floats) → **lane empty** and `base-scale`
  tracks the previews (the visible live feedback) → released(final) →
  **exactly one** `SetScale` carrying the final value → reply Ok →
  drain → Ok outcome, `base-scale` final.
- `a_slider_gesture_is_inert_while_an_op_is_pending`: preset pick
  (pending) → pressed + previews + released → **no** `SetScale`,
  `base-scale` unchanged, outcome unchanged (the silent-gate
  contract).
- `a_slider_commit_reverts_on_a_typed_error_reply`: pressed → preview
  → released → reply `Err(SettingsError::PersistFailed)` → drain →
  `base-scale` back to the pre-gesture value, outcome
  `Unavailable` (`outcome_of(PersistFailed)`) — the revert is
  outcome-agnostic and restores the persisted scale, not the preview.
- Existing app-side scale-bound tests (`ui_settings.rs:434,449,515`)
  and the field journey path stay untouched.

## 6. D30 — the settings view scrolls (F17) + D31 InfoTips (F19)

### 6.1 Flickable composition (recorded)

- `SettingsView` keeps `inherits FocusScope`; inside, the
  `VerticalLayout` becomes: **header row first** (back button + title,
  settings-view.slint:139-153 — pinned, `vertical-stretch: 0`, outside
  the scroll), then `flick := Flickable { vertical-stretch: 1; content
  := VerticalLayout { …every section…, status row last } }`.
- **What stays pinned: the header only** (design D30). Status line and
  scale controls are the scroll content's tail — "the scrollable extent
  reaches the status line and scale controls".
- **Scrollbar choice (recorded)**: the action-menu's conditional
  indicator inline-copied (action-menu.slint:237-257 token math:
  track in `Theme.surface`, thumb in `Theme.border`, rendered iff
  `flick.viewport-height > flick.height`). A shared `ScrollBar` widget
  needs element-reference plumbing Slint 1.18 does not offer; with one
  new consumer the honest call is a conscious inline copy — revisit at
  the third consumer.
- **The menu's recorded Slint fact** (action-menu.slint:57-61: max-height
  is not honoured on a Flickable-hosted panel) is respected by hosting
  the Flickable in a **layout cell** (`vertical-stretch: 1`) under the
  view's FocusScope, never as an overlay child.
- **Scroll facts for journeys** (the `MenuPanelFacts` precedent,
  ui/menu-facts.slint): new global `SettingsScrollFacts` —
  `content-height` / `viewport-height` mirrored one-way via
  `changed`+`init` handlers (the mirror-facts pattern,
  action-menu.slint:82-104), and `viewport-y` **two-way** bound
  (`flick.viewport-y <=> SettingsScrollFacts.viewport-y`) so journeys
  can drive the scroll and gestures write back. *Stage-2 probe:* the
  two-way binding to a built-in property is expected to compile
  (two-way bindings to built-in in-out properties are legal Slint);
  recorded fallback if the pinned compiler refuses it: an in-out mirror
  property + a `changed` handler syncing into `viewport-y`, with the
  gesture's write-back skipped (journeys then assert reachability from
  the fact values rather than on-screen positions).

### 6.2 The scale-3.0 reachability contract (dispatch §6 acceptance f)

Journey `settings_scroll_reaches_the_scale_controls_at_scale_3`:
mount with `UiSettings { scale: 3.0, .. }`, `set_window_size` to the
scale-multiplied floors (480·3 × 400·3 logical), open settings →
- facts: `content-height > viewport-height` (the overflow exists —
  the F17 trap state, now scrollable instead of clipped);
- write `SettingsScrollFacts.viewport-y = −(content-height −
  viewport-height)` (scroll to the bottom), `advance_time(0)` for the
  change pass → the "Scale" field element (`find_by_label`) has
  `absolute_position` inside the window bounds, and the status row
  (the content's last child) likewise;
- the header "Back" button's y is **unchanged** before/after the scroll
  (pinned header);
- `window-w`/`window-h` out-properties (app-window.slint:146-147) are
  unchanged across the whole journey — the Window invariant
  (no `width:`/`height:` bindings on the Window, app-window.slint:83-93)
  holds at scale 3.0; grep-pin stays.

### 6.3 InfoTip widget + help-text constants (F19)

- New L1 `ui/widgets/info-tip.slint`:
  `in property <string> about;` — a `Theme.space-3` square with
  min/max locked (the icon-button square-lock pattern,
  icon-button.slint:49-52), circle border (`radius: width/2`,
  `text-secondary`) with a geometric "i" (dot + stem Rectangles, the
  D19 no-icon-font vocabulary). **Not** an `IconButton`: it routes no
  action and takes no click.
- **Trigger**: the canonical generalized trigger verbatim
  (`changed has-hover` → `TooltipState.hover(about, <help text>, [],
  abs-pos, size)` / `TooltipState.unhover()` — icon-button.slint:233-250
  pattern). The hover label is `about` (non-empty required by
  TooltipState, tooltip.slint:53); the help text rides the
  description slot.
- **A11y (recorded)**: the InfoTip itself stays AT-transparent
  (no role) — a hover-only control with no action would be an AT
  dead-end if made a focusable button. The help content's AT delivery
  is the **annotated field's `accessible-description`**: `LabeledField`
  gains `in property <string> help;` feeding both the rendered InfoTip
  and the `TextField` (which grows `in property <string>
  description;` → `accessible-description` — one-line, call-site-neutral
  widget change). Single source per help fact.
- **Help-text constants** live in `settings_status.rs` beside the
  status mapping (one module = the settings presentation-text home;
  design D31 "presentation constants"):
  `HELP_NAME` (free-form local kebab label, your choice),
  `HELP_SERVER` (the Nextcloud instance base URL),
  `HELP_USERNAME`, `HELP_APP_PASSWORD` (what it is + created in
  Nextcloud → Personal settings → Security → Devices & sessions),
  `HELP_SCALE`, `HELP_KEYMAP` (section header),
  `HELP_ROWS` (per-row: Test verifies the stored secret; Remove
  deletes the config entry and the stored secret).
- **Per-field placement list (D31, all seven)**: Name, Server,
  Username, App password (each inside `LabeledField`, after the label);
  Scale (the scale row); Keymap (the section header row); each
  connection row (inside `row-layout`).
- **Tests**:
  - presence journey `every_settings_field_carries_its_info_tip`:
    with one seeded connector row, the InfoTips are addressed by
    **element id** (`find_all_by_id`, driver.rs:82 — role-less
    elements have no accessible-label address, hence id addressing)
    and the count pins the seven placements;
  - wiring journey `an_info_tip_hover_arms_the_generalized_tooltip`:
    `hover_at` the first tip's centre → `TooltipState.pending` →
    `advance(600)` → visible with **non-empty** description → move
    away → hidden (r2 D13's dismissal lifecycle is already pinned in
    usability-components; only the arm is new here);
  - `settings_status.rs` unit loop: every `HELP_*` constant
    non-empty (structure, never content).

## 7. Testing strategy (normative carry-overs, this chunk's shape)

- **No text assertions** — status text, help texts, segment labels,
  tooltips: pinned via exhaustive-match compilation, totality
  proptest/lattice loops, non-empty checks, structure
  (roles/counts/ids/geometry), and typed model values. §14-R5's test
  rule survives the UI-tones reversal it guards.
- **No sleeps**: the auto-clear and every timing assertion ride
  `driver.advance_time` over the mock clock (F7); no
  `tokio::time::sleep`, no fixed waits, terminal states asserted.
- **One behaviour, one test, lowest layer**: mapping totality lives in
  `settings_status.rs` unit tests; auto-clear behaviour in journeys
  (it is adapter+timer behaviour); the app ceremony stays pinned in
  `ui_settings.rs` (untouched — no duplication through the lane).
- **J7 extended, never weakened** (prompt directive): only the
  compile-required reply literal changes; J1–J6 untouched;
  `ui_settings.rs` untouched.
- **New journeys** (9): J8 echo, terminal-shows+clears, re-arm,
  busy-holds, slider preview+single-commit, slider pending-inert,
  slider Err-revert, scale-3.0 reachability, InfoTip presence,
  InfoTip hover-arm (10 with the segmented-control pin in
  usability-components).
- **Property tests** (1): the mapping-totality proptest over the
  `SettingsError` generator.
- **Tier-4 manual checklist** (PR body, user-run):
  1. **Scroll feel** — flick the settings content at scale 3.0 in a
     minimum window: physics feel sane, the indicator appears only
     while overflowing, the header never scrolls away.
  2. **Slider feel** — drag the scale slider: 0.1 snapping, live
     preview tracks smoothly, release commits (text field still
     authoritative), **the window never resizes during preview**
     (F22 watch).
  3. **Status-line legibility** — text fits its one row at 0.5 and
     3.0; dot contrast; the 5 s lifetime reads as helpful, not
     jarring; a busy status never vanishes mid-operation.
  4. **Segmented control** — exactly-one-selected reads clearly;
     hover/focus states; no elongation at scale extremes.
  5. **InfoTip feel** — hover delay, placement near the small square,
     no flicker at the top rows.

## 8. Code inventory (this chunk)

- **New** (`crates/taskboard-ui-slint`):
  `ui/widgets/segmented-control.slint` (~90),
  `ui/widgets/slider.slint` (~110), `ui/widgets/info-tip.slint` (~70),
  `ui/settings-facts.slint` (~20), `src/settings_status.rs` (~200
  incl. tests).
- **Modified** (ui-slint): `ui/views/settings-view.slint` (scroll
  re-wrap, segmented swap, slider row, status row, InfoTips — the
  round-1 comment block at 2-7 updated to cite this plan),
  `ui/app-window.slint` (3 scale callbacks + view wiring),
  `ui/widgets/text-field.slint` (`description` property only),
  `src/adapter.rs` (echo, outcome choke point + timer, slider seams,
  `scale_drag`), `src/requests.rs` (`KeymapUpdate.preset`),
  `src/lib.rs` (timer creation + hand-over), `src/models.rs`
  (`preset-index`/`status-text` push fold into `push_settings`'s
  caller — no model-structure change beyond the two properties on the
  Slint global).
- **Modified** (app, recorded exception): `ui_requests.rs`
  (`set_keymap_preset` fills `preset` — ~3 lines).
- **Tests**: `tests/journeys.rs` (+~260), `tests/usability_components.rs`
  (+~70), `tests/support/driver.rs` (`is_radio_button` ~+8),
  `src/settings_status.rs` (tests inline).
- **Untouched**: `theme.slint` (no new tokens needed — recorded),
  `settings_form.rs`, `presentation.rs`, all lane/ceremony code,
  keymap presets/snapshots (no registry change), goldens (no settings
  goldens exist; none added — the segmented/slider/status visuals are
  Tier-4 items).
- **Estimated diff**: ~950–1200 lines (slint ~350, rust ~250,
  tests ~450, plan/changelog excluded).

## 9. Risks

- **R2 (Slint fights back) — the two recorded probes for stage 2**:
  (a) the two-way `viewport-y <=>` global binding (fallback §6.1);
  (b) `accessible-checked` readability through the testing backend
  (fallback §2.3). Both are probe-first, fallback-recorded; the role
  surface itself is already verified present (F12).
- **The app-crate diff** (§3.1): permitted only with this recorded
  reason; reviewers assert it stays ~3 lines and that `ui_settings.rs`
  passes unchanged.
- **Timer lifecycle**: the `Rc<Timer>` in the adapter + `Weak` in the
  fire closure avoids the reference cycle; the Busy-hold guard prevents
  a stale fire from clearing an in-flight status (§4.2 table). The
  mock clock makes every timing deterministic (F7).
- **The echo must not fight the push**: `push_settings` writes
  `preset-index` on every refresh (idempotent); the J8 pre-drain
  assertion (index unchanged after the click) is the pin that forbids
  a click-echo regression.
- **Status text width at 0.5/3.0**: texts are short by design; the
  fixed row height + `clip` guards the layout. Tier-4 item 3 covers the
  legibility feel; no golden is added (no settings goldens exist).
- **Insert-mode interplay**: the slider/segmented/InfoTips engage no
  text mode (no field focus); Escape still closes settings (J1's
  chain untouched); the typed paths keep the existing
  `settings_field_insert_engage` contract.
- **Chunk-6 forward-compatibility**: the status line is built here and
  consumed there (dispatch §6 non-goal note) — `status_text`'s
  outcome+detail shape extends to chunk 6's new outcomes by adding
  enum variants (compiler walks the stage-2 agent through every arm).

## 10. Implementation order (TDD-first, stage 2)

1. `settings_status.rs`: mapping + constants + unit tests + proptest
   (pure, no Slint — the compiler-enforced totality lands first).
2. `KeymapUpdate.preset` + app fill + J7's literal + J8 (echo red→green).
3. Status-line model text + adapter choke point + timer + journeys
   (show/clear, re-arm, busy-holds).
4. `SegmentedControl` widget + usability pin + the purge (picker swap).
5. `Slider` widget + seams + journeys (preview/commit, inert, revert).
6. Flickable re-wrap + facts global + reachability journey.
7. `InfoTip` + help constants + presence/hover journeys.
8. CHANGELOG Unreleased; full gate suite (AGENTS §10).

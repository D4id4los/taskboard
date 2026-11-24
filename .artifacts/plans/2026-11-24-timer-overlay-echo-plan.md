# Plan — chunk A5b: the timer overlay's SegmentedControl echo (#1306)

- **Branch**: `fix/timer-overlay-echo`
- **Issue**: #1306 (bug, p1, ui)
- **Dispatch context**: `.artifacts/plans/2026-11-24-backlog-reduction-round6-dispatch-plan.md` §1/§4; scope: `.artifacts/reports/2026-11-24-backlog-reduction-round6-scope-report.md` §A-A5.
- **Size**: S (slint caller wiring + adapter push + tests; ~40–80 production lines, ~100–150 test lines).

## 1. Diagnosis (verified, 2026-11-24, off `origin/main` @ dc96c034)

`SegmentedControl` (`crates/taskboard-ui-slint/ui/widgets/segmented-control.slint`)
is **deliberately stateless** (header, lines 5–9, r2 D26/D27): the caller
owns the selection; the widget renders `selected-index` and emits
`picked`. This is a design decision — **do not touch the widget**.

The caller `crates/taskboard-ui-slint/ui/compositions/timer-setter.slint`
misses the feedback half on **all three** of its `SegmentedControl`s:

| Widget | Lines | `selected-index` | Adapter mirror |
|---|---|---|---|
| Quick presets (`1m/5m/10m/25m/1h`) | 109–113 | never bound | none (D-T6: quick-fill, not a selection) |
| Day (`Today/Tomorrow`) | 137–141 | never bound | `setter_day_offset: u32` (adapter.rs:740, set by `on_timer_setter_day_picked` at 17365) |
| Insistence (`Normal/Alarm`) | 149–153 | never bound | `setter_insist: bool` (set by `on_timer_setter_insist_picked` at 17371, `[bare_mut]`, no refresh) |

The seams **do** mutate adapter state — the commit path carries the right
value; only the visual echo is missing, so `selected-index` sits at its
default `0` forever and every chooser reads as "stuck".

Working contrast (the unregression baseline): the Today-view timer line
binds both halves Slint-locally —
`selected-index: root.insistence-index; picked(index) => { root.insistence-index = index; }`
(`crates/taskboard-ui-slint/ui/compositions/timers.slint:417–422`).

## 2. Choosers inventory (round law c — every other overlay chooser audited)

All 13 other `SegmentedControl` call sites bind `selected-index`
correctly; **no second offender exists**, so **no backlog issue is
needed from the inventory** (verdicts recorded here instead):

- `compositions/habits.slint:238` — bound to `root.target-index` (local in-out). OK.
- `compositions/toolbar.slint:64` — bound to `root.switcher-index`. OK.
- `compositions/recur-editor.slint:127, 178, 206, 216` — all bound to `root.model.*` (adapter model). OK.
- `compositions/export-dialog.slint:103, 113, 208` — all bound to `root.model.*`. OK.
- `views/task-view.slint:548` — bound to `root.habit.quota-kind`. OK.
- `views/timers.slint:417` — bound to `root.insistence-index`. OK (the baseline).
- `views/settings-view.slint:926` — intentionally static (`segment-enabled: [true, false]`, `selected-index: 0`; the Standalone backend is not implemented). OK by contract.
- `views/settings-view.slint:1074, 1381, 1516, 1588, 1660, 1705, 1777` — all bound to `SettingsModel.*` / `item.*`. OK.

## 3. Design — the caller-side echo fix

One echo discipline per row, chosen by who owns the state:

- **Day + insistence (adapter-owned mirrors)**: **reply-sourced echo**
  (r2 D27) — the adapter pushes, the widget renders.
  - `TimerSetter` gains `in property <int> day-index: 0;` and
    `in property <int> insist-index: 0;`; each `SegmentedControl` binds
    `selected-index` to it. `picked` keeps dispatching only (§5.1: no
    inline side effects).
  - `push_timer_setter` (adapter.rs:3505) pushes
    `day-index = setter_day_offset as i32` and
    `insist-index = setter_insist as i32`. It runs every derive pass
    (called from the refresh funnel, adapter.rs:3422), so a pick
    re-pushing through `refresh_forced` echoes immediately.
  - **Decision D1**: convert the insist seam from `[bare_mut]` to the
    standard `bind mut (a, window)` + `refresh_forced(&window)` form
    (adapter.rs:17371). Cost: one cheap recompose pass per pick.
    Benefit: one echo path (adapter-pushed) for both adapter-owned
    rows, and the bare_mut carve-out — added precisely because nothing
    was being pushed — loses its reason to exist. Revisit if the
    pass cost ever matters; it does not at this surface.
  - The wipe rides for free: `clear_timer_setter` (adapter.rs:3492)
    already resets `setter_day_offset = 0` and `setter_insist = false`,
    so the next open seeds both indices at 0 via the push.
- **Quick presets (no adapter state — D-T6 quick-fill semantics)**:
  **Slint-local echo**, mirroring the timers.slint pattern — an
  `in-out property <int> preset-index: 0;` on `TimerSetter`;
  `picked(index) => { root.preset-index = index; root.preset-picked(index); }`.
  **Decision D2**: do not invent an adapter mirror for the preset
  index — the adapter deliberately does not track which preset was
  last tapped (a fill is not a selection; D-T6), and a Rust-invented
  mirror would be state without a consumer. The echo is purely visual.
  - **Decision D3 (reset semantics)**: verify at implementation time
    that `TimerSetter` mounts inside a conditional (`if` in
    timers.slint's overlay host); if it does, the in-out property dies
    with the panel and fresh-open reset is automatic. If it is
    permanently mounted, reset `preset-index` in the same
    `changed`/open seam that bumps `focus-nonce` (timer-setter.slint:53).
    Record which spelling landed in the commit body.

**Widget contract preserved**: no change to
`segmented-control.slint`; no pixel/text pins anywhere.

## 4. Tests (structural, typed — round law a; AGENTS §5)

Where: `crates/taskboard-ui-slint/tests/journeys.rs` (the timer-setter
journey block around 12871–13110; the generated window accessors make
`window.get_timer_setter_day_index()` etc. available once the `in`
properties exist).

1. **T1 — the echo pin (day)**: open the setter, invoke
   `on_timer_setter_day_picked(1)`; assert
   `get_timer_setter_day_index() == 1`. Structural: the selected-index
   flow, not text.
2. **T2 — the echo pin (insistence)**: invoke
   `on_timer_setter_insist_picked(1)`; assert
   `get_timer_setter_insist_index() == 1` (this also pins D1's refresh
   conversion).
3. **T3 — the echo pin (presets)**: invoke
   `on_timer_setter_preset_picked(3)`; assert
   `get_timer_setter_preset_index() == 3` (the local in-out path).
4. **T4 — the wipe/reset pin**: pick day=1, insist=1, dismiss, reopen;
   assert both pushed indices are 0 (the clear_timer_setter ride).
5. **T5 — unregression (Today-line)**: verify an existing test already
   pins the timers.slint insistence-index flow; if none exists, add one
   (set the in-out, assert the index round-trips) — lowest layer, one
   behavior one test. No quality ceilings, no text assertions.
6. **T6 — commit path still correct**: the existing
   timer-setter commit journeys (12871+) must stay green unchanged —
   the fix adds echo, never touches the composed grammar.

Determinism: no sleeps; the seams are synchronous adapter calls
followed by deterministic pushes — assert terminal state directly.

## 5. Open questions → recorded decisions

- *Which echo discipline per row?* → D1/D2 above (adapter-pushed where
  the adapter owns a mirror; Slint-local where nothing owns it).
- *Fix the widget?* → No (round law c; the stateless contract is r2
  D26/D27, a design decision).
- *Insist seam stays bare_mut?* → No; D1 converts it.
- *Other choosers with the same miss?* → None (§2 inventory); no
  backlog issues required.
- *UX journey plan?* → Not required: no new verb/badge/view/setting —
  this restores the intended behavior of an existing picker (dispatch
  plan §1 marks A5b "no UX plan").

## 6. Risks

- **Presets echo semantics**: a preset is a fill, not a persistent
  selection; the visual echo could be misread as state. Mitigation:
  D3's reset-on-open; the label field retains focus after a pick
  (D-T6), matching the fill semantics.
- **`refresh_forced` on insist pick** recomposes the whole pass —
  trivial at this surface, but if a test gets flaky-slow, the fallback
  is a targeted push, not a revert to bare_mut (record any deviation
  in the PR body).
- **Generated-accessor naming**: the slint codegen derives
  `get_timer_setter_*` spellings; verify against the generated bindings
  before pinning test names.

## 7. Handoff

Stage 2 implements §3/§4, gates on pre-commit hooks +
`scripts/run-quality-gates.sh cargo nextest run --workspace`, opens the
PR referencing #1306 (`Closes #1306` is the dispatcher's at-merge
bookkeeping, dispatch plan §18). Stage 1 ends at this plan commit.

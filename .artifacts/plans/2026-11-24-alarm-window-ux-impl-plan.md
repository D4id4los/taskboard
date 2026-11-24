# [Plan] The Alarm Window Redesign — Chunk B1 Implementation Plan (Round 6)

- **Date**: 2026-11-24
- **Kind**: chunk plan — implementation spec for chunk B1 of
  backlog-reduction round 6 (branch
  `feat/alarm-window-redesign`): the alarm insistence window's
  geometry, repositioning, and button-visibility redesign (#1308) +
  the #1251 severing gap.
- **Origin**: issue #1308 (normative body: the three findings);
  UX journey plan
  `.artifacts/plans/2026-11-24-alarm-window-ux-plan.md` (normative
  once accepted — every ID below cites it: US-n stories, US-n/S-k
  steps, W-x wiring rows, R-x rulings); round-6 scope report
  `.artifacts/reports/2026-11-24-backlog-reduction-round6-scope-report.md`
  §B (#1308 row, "journey-gated"); the closed anchor-aware rulings
  (#815, D-P11); the #1251 severing gap; the W1 button grammar
  (`docs/widget_interactions.org`); the #805/T-22 keyboard grammar.
- **Status**: DRAFT — gated on the journey plan's acceptance (the
  round-6 UX gate; `scripts/ux-coverage.sh … --gate` is the stage-2
  self-check, the gate is never CI).

## 1. Scope

Single chunk, one PR. Lands, in order:

1. **W-1 geometry rework** (`alarm-window.slint`): drop the fixed
   `width: Theme.space-15 * 4` and the `rows.length * space-8 +
   space-8` height formula; the content root's `VerticalLayout` sizes
   to its rows (each row's natural preferred size, title floor
   `Theme.card-title-min-width` for the elide). The keyboard
   `FocusScope` is re-homed: `vertical-stretch: 0` (and an explicit
   `height: 0`-class zero-layout-mass binding) so it stops absorbing
   leftover height while remaining the mount-focus keyboard surface —
   the T-22 grammar (Up/Down, d/s/c, Escape) is untouched.
   **W-4 rides in the same edit**: delete the stray
   `width: root.width; height: root.height` `Rectangle` inside each
   row's `ListRow` (`alarm-window.slint:104-110`) and declare the
   selection ring row-local (binds the row's own geometry).
2. **W-5 #1251**: the content root carries
   `max-width: Theme.layout-severing-max-width`; probe twin
   `tests/alarm_window_max_probe.slint` +
   `tests/probes/alarm_window_max_probe_tests.rs` following the
   per-view pattern of `1276151a` (h.max ≥ 99 999 after the
   materialization walk).
3. **W-2 drag strip**: a grab handle spanning the header's top edge;
   pointer drag extracts deltas and dispatches the new window-local
   `drag-by(length, length)` callback; `install_alarm_callbacks`
   gains the position applier (accumulate deltas →
   `window.set_position`), with the Wayland arm degrading to a logged
   no-op (journey plan R-2: under Wayland `no-frame` no-ops too, so
   the compositor title bar is the native affordance there). The
   applier is extracted behind a small seam so the tests assert the
   accumulator + degrade arm without a real WM.
4. **W-3 tooltip visibility**: import `TooltipState`/`TooltipPanel`
   into `alarm-window.slint` and mount a `TooltipPanel` inside the
   window (the `IconButton`s already raise `TooltipState.hover`;
   today the only panel lives in `app-window.slint:1810`). The three
   verb buttons and their W1 roles stay verbatim.
5. **Tests** per the journey plan §8: the two seam tests, two fold
   tests, one journey test, the probe twin; the routing/keymap/kiosk
   suites stay green untouched.
6. **Census + issues**: amend `docs/ux-surface-census.org` rows
   `alarm.*` (drag affordance, window-local tooltip evidence);
   comment on #1308 and #1251 at PR-open; close at merge per the
   normal flow.

## 2. Recorded decisions (resolved — no open questions)

| ID | Decision | Rationale |
|---|---|---|
| D-1 | Content-sized geometry (no fixed width, no height formula) | #1308 acceptance: "sized to its content (declarative geometry, no stretch-absorbing empty FocusScope)"; the title elide floor keeps rows legible for long titles |
| D-2 | Drag model: pointer drag on a strip → delta accumulation → `set_position`; Wayland degrades to logged no-op (R-2) | The only client-honored move channel on X11/Windows/macOS; Wayland forbids client positioning — the compositor title bar (present there because `no-frame` no-ops) is the recorded posture. The recorded alternative (persisted placement setting) is deferred to backlog #1321 |
| D-3 | Button visibility: window-local `TooltipPanel` over the shared `TooltipState` | The widgets already raise the state; the defect is the missing panel, not the button grammar. Two concurrent panels (main + alarm) rendering the same global is accepted — same text, independent windows |
| D-4 | Engine contract delta is empty | The verbs exist; #1308 is presentation-only. The only new seam is the window-local `drag-by` callback |
| D-5 | Kiosk posture: no delta | R3.5.3 fence (the lanes never mount on kiosk) stands; `the_kiosk_alarm_fence_stands_with_the_absence_hint` is the fence test |
| D-6 | The selection ring is re-declared row-local, not shrunk in place | A `Rectangle` bound to the row's own geometry cannot regress to window dimensions; keeping it inside `ListRow` would keep the window-sized-bind footgun one refactor away |

## 3. Traps (from the origin's facts — stage 2 reads these first)

1. **The FocusScope must keep mount focus.** Zeroing its layout mass
   (`vertical-stretch: 0`) must not drop `init => { self.focus(); }`
   — a zero-size focusable element still receives keys while focused;
   the keymap registry contract suite is the oracle.
2. **`request_attention` must not fire from drag motion.** The alarm
   pass re-shows the window on every derive; `set_position` during
   drag must never re-arm attention (US-2/AC-3's journey test pins
   the dragged position surviving a re-derive).
3. **Window identity vs position reset.** The lane holds the window
   across passes; the applier accumulates deltas against the *shown*
   window's live position (read-modify-write), never against a cached
   origin — a hide/show cycle between drag events would otherwise
   teleport the window.
4. **Wayland degrade is typed, not textual.** The degrade arm's test
   asserts the typed no-op outcome (`matches!`), never a log-message
   substring (AGENTS §5 no-text-assertions).
5. **Probe twin naming and shape** follow `1276151a` exactly
   (`alarm_window_max_probe`, the materialization walk, h.max ≥
   99 999); #1251's body is the pattern reference.
6. **Do not "fix" the `snooze`/`dismiss` payload semantics** — the
   index-keyed rows cell and `alarm_command` are landed contracts
   (US-1 regression fence); geometry work touches nothing behind the
   callbacks.
7. **`--gate` before commit**: run
   `scripts/ux-coverage.sh .artifacts/plans/2026-11-24-alarm-window-ux-plan.md --gate`
   as the stage-2 self-check; every W-x row must resolve BOUND against
   the landed tests.

## 4. Verification & quality gates

- New tests (journey plan §8, with the plan's exact homes — the gate
  §6 Test cells nominate these exact names):
  `probes/alarm_window_drag_tests.rs::us2_s2_drag_by_applies_the_accumulated_delta`
  and `::us2_s2_wayland_arm_degrades_to_a_noop`,
  `structural.rs::us3_s1_alarm_window_mounts_its_own_tooltip_panel`,
  `probes/alarm_window_ring_probe_tests.rs::us4_s1_selection_ring_is_row_local`,
  `journeys.rs::us2_s3_alarm_rederive_keeps_the_dragged_position`,
  `journeys.rs::us4_s2_alarm_escape_hides_committing_nothing`,
  `probes/alarm_window_max_probe_tests.rs::alarm_window_max_probe_severs_the_layout`.
- Kept-green regression fence: `router.rs` alarm verb routing, the
  keymap registry grammar suite, the mount tests, the kiosk fence test.
- Quality gates: `scripts/run-quality-gates.sh` full sequence at
  handoff; every `cargo` invocation through
  `scripts/memory-gate.sh`; pre-commit hooks are the docs-stage gate;
  the coverage trio never runs inside the chunk agent (dispatcher
  owns full-suite runs).

## 5. Out-of-scope (homed)

- Persisted window placement → **backlog #1321**.
- Snooze-duration choice, rich rows (timestamp, per-row menu) →
  **#352** (existing feature-scale timers/alarms UI item; not filed
  anew).
- OS-level max-hint probing on Wayland → **#778** (existing upstream
  tracker; not filed anew).

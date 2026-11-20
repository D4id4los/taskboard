# Journey/backend pins — chunk B4b plan (round 3)

- **Date**: 2026-11-20
- **Branch**: `test/journey-backend-pins` (off `origin/main` @ ee8cdd3d;
  worktree `/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-test-journey-backend-pins`)
- **Covers**: #1023 #1024 #995 #996 #997 #1059 #1018 (dispatch plan
  `2026-11-19-backlog-reduction-round3-dispatch-plan.md` §11; scope report
  §B4 normative)
- **Base**: B4a landed (PR #1107 merged) — `tests/support/harness.rs`
  (`Harness` / `run_journey*`) is the composition base for every new
  journey; hand-rolled setup in new journey code is a stage-3 finding.
- **Stage-1 scope**: plan only. No implementation, no push.

## 1. Landed shape this chunk builds on

- ui-slint tests are grouped: `journeys.rs` and `journey_suites.rs`
  (each `pub(crate) const HEADLESS_RENDERER: bool = true`), `probes/`
  members under `probe_composition.rs` (false) / `probe_pixel.rs`
  (true) / `structural.rs`; `inventory/` under its root. `#path` mod
  declarations, one `mod support` per binary (process-global
  `OnceLock` platform dispatch).
- `support/harness.rs`: `run_journey{,_with_settings,_with_files_sync}`
  + `Harness::{mount*, press, stroke, type_text, assert_commands,
  chain, ...}`. `journey_suites.rs` consumes it via `crate::support`
  (`support::harness::run_journey`).
- `journeys/habits_journey.rs` still carries its own private
  3-field `Harness` (verbatim move per B4a; not the shared helper).

## 2. Ordering (shared-seam law: backend first, journeys ride them)

| # | Item | Kind | Order |
|---|---|---|---|
| 1 | #1024 | FakeEngine settle-mask fold (pure Rust fixture) | 1st |
| 2 | #996 | DialogButton autofocus authoritative (ui widget) | 2nd |
| 3 | #995 | focus-restore quirk pin (probe) | 3rd |
| 4 | #1023 | stale-geometry quirk pin + driver doc | 4th |
| 5 | #1059 | habit editor negative-leg journeys (shared helper) | 5th |
| 6 | #1018 | habit entry-height geometry probe | 6th |
| 7 | #997 | J1 wiring inventory + gate RESOLVE | last (after 5) |

Rationale: #1024 is the fixture-continuity base the #1059 journeys may
settle against; #996/#995/#1023 change what the backend/widgets
report, so the journey pins (#1059/#1018/#997) are written against the
post-fix behavior; #997's gate run must observe the landed journey
tests, hence last.

## 3. Per-item shapes and recorded decisions

### 3.1 #1024 — FakeEngine `UpdateTask` settle arm folds the effort mask

**Decision**: fold the missing masks — `effort`, `importance`,
`urgency`, `recur`, `kind` — mirroring the engine's `UpdateTask`
application (each field's `Option` layer applied verbatim, including
`Some(None)`-clears where the domain mask carries double options; the
mask shape in `taskboard-domain/src/messages.rs` §TaskChanges is the
oracle — compare against the engine's own application, not this file's
siblings). `duedate`/`scheduled` style double-option semantics follow
whatever the domain mask field actually is per field.

**Test** (TDD red-first): a fixture-continuity test next to the
existing fake-engine self-tests (the `journeys.rs` support self-test
section that exercises `settle`): commit an `UpdateTask` whose mask
carries `effort` (+ one of `kind`/`recur`), `settle`, then assert the
refreshed typed model value — typed outcome, no text. Without the
fold the test reads the stale value (red); with it, green.

**Diff**: ~+45/-2 (`fake_engine.rs` + one test).

### 3.2 #996 — DialogButton initial focus becomes model-authoritative

**Decision**: the model-construction seam, not a Slint reduce. Slint
functions cannot loop/reduce over a model (the issue's named trap), so
the `autofocus → index` computation moves to **Rust where the
`DialogButtonModel` vec is built**: a typed helper (e.g.
`fn initial_focus_index(buttons: &[DialogButtonModel]) -> i32` — first
enabled row with `autofocus: true`, `-1` when none) in the models
module; every host site stops declaring a literal `initial-focus: int`
and instead derives the property from the same vec it builds the model
from. `modal-dialog-host.slint`'s `initial-focus` site property and
the `autofocus`-as-dead-metadata comment go; the row component keeps
its `initial-focus-index` mount property (the host binds it from the
site's derived value). W3's TextButton absorption (PR #1088, merged)
is the recorded precondition and it is satisfied.

**Test**: flip the G-13 pin level — `g13_button_model_routes_indices_and_lands_the_initial_focus`
keeps the index-routing half but the initial-focus half now asserts
the flag is authoritative: set one row's `autofocus: true` in the
probe model (no site index), mount, assert focus lands there
(typed: `modal_focused_button` index). The disabled-entry gate test
(`g13_a_disabled_button_never_focuses_nor_invokes`) keeps its law
with autofocus on a disabled row → no focus, helper returns the next
enabled autofocus row or -1.

**Diff**: ~+40/-25 (`modal-dialog-host.slint`, site call sites,
probe tests, models helper + unit test).

### 3.3 #995 — raw focus restored onto a disabled FocusScope across a visibility toggle

**Decision**: **pin + document, do not fix.** The restore path is
i-slint-backend-testing-level; the widget-level gates (enabled bit,
seam write guard) hold the observable law today, and the real
invalidation/exclusion fix belongs upstream or with #954's chunk-17
focus work. Concretely:

1. A `..._currently_pins_...`-named probe test in
   `probes/widget_framework_probe_tests.rs` (the g13/G-13 modal-host
   probe, which hosts the disabled-FocusScope toggle scenario):
   hide and re-show a panel containing a disabled FocusScope and pin
   today's raw `has-focus` restoration as recorded behavior —
   structural facts (`has-focus` reads) only. Named as debt so an
   upstream fix flips it loudly.
2. A driver-doc paragraph (`support/driver.rs` module doc): raw focus
   restoration after a visibility toggle can land on a disabled
   FocusScope; walk/focus oracles assert the widget-level gates, not
   raw `has-focus` on disabled scopes.
3. The upstream triage the issue suggests is out of this chunk's
   scope — a comment on #997-style bookkeeping is not needed; the
   issue stays open with the pin noted in the stage-2 report.

**Diff**: ~+70 (probe test + doc).

### 3.4 #1023 — stale geometry for conditional remounts (changed-mirror + element-query facets)

**Decision**: **pin + document, do not fix.** Both facets live in the
testing backend's mirroring/query machinery (facet of #954); the
round-9 migration already adopted the compensating law (typed root
seams over element queries on conditionally remounted subtrees), and
the real invalidation fix is #954's chunk-17 / W6 home. Concretely:

1. A `..._currently_pins_...` probe test (new member under
   `probes/`, mounted from `probe_pixel.rs` — geometry needs the
   rendered backend): mount a conditional subtree, toggle it away and
   back, and pin the recorded staleness — the element-query rect of
   the freshly remounted element vs the settled typed seam (structural
   facts; assert the *mismatch class* the issue recorded, or the
   typed-seam correctness, whichever the probe fixture expresses
   cleanly — recorded decision: assert the typed seam reports the
   settled value while the element query may read stale; no numeric
   px oracle beyond inequality/inequality-free structural facts).
2. Driver-doc paragraph in `support/driver.rs`: element-query
   geometry on freshly remounted conditional subtrees may be stale
   (pre-attach zeros / mid-layout reads); `changed panel-*` mirrors
   from a conditional mount may not deliver; journey oracles use
   typed root seams there.

**Diff**: ~+110 (probe member + slint fixture + doc).

### 3.5 #1059 — the habit editor's escape legs as journeys

**Decision**: three journeys appended to
`tests/journeys/habits_journey.rs`, **each composed through the
shared B4a helper** (`support::harness::run_journey` /
`Harness` from `crate::support`) — not the file's private 3-field
harness; the private harness stays for the pre-existing tests
(migrating it is filed as backlog, see §5). Typed seam assertions
only; no text oracles.

1. `us5_s2_invalid_edit_stays_mounted_without_dispatch` — drive the
   editor into an invalid count/time/month-day (per
   `HabitEditDraft::can_save`'s grammar), attempt save, assert: the
   editor surface stays mounted (typed router/view fact) and
   `harness.assert_commands(&[])` — nothing dispatched.
2. `h4_kiosk_remove_arm_is_inert` — kiosk mode, arm the remove from
   the habit context menu; assert the remove arm's kiosk refusal:
   the typed `UiEffect`/command drain stays empty for the remove
   half (the editor-gate journey's posture, extended to the arm) and
   the menu's armed state matches the desktop row model facts.
3. `us5_s3_escape_cancels_the_armed_menu` +
   `us5_s4_escape_cancels_the_editor` (or one journey covering both
   rungs if the grammar makes them one flow — recorded decision: one
   journey per rung, matching the J1 S3/S4 rows the chunk-10
   landing added) — drive the armed menu / the editor and press
   Escape through the shared nav-back grammar
   (`harness.press(ActionId::...)` resolved from the chain), assert
   the subject retires (typed view/router fact) and no `UpdateHabit`
   / remove dispatch escaped.

Tokens: derive from J1's §4 US-5 table (S3/S4 exist post-chunk-10;
S2's escape cell names leg 1) — exact step tokens so #997's gate
credits them; any leg J1 does not name gets its `bound-test:` line
added in the #997 step.

**Diff**: ~+260 (three journeys + fixture helpers reused).

### 3.6 #1018 — habit card entry-height parity probe

**Decision**: geometry probe following the `task_card_title_probe`
precedent exactly: a new `habit_entry_height_probe.slint` fixture +
`probes/habit_entry_height_probe_tests.rs` member mounted from
`probe_pixel.rs` (the precedent's root — the probe needs rendered
geometry). The fixture mounts `HabitsSection` (one habit row) beside
a `TaskCard`; the test asserts the **card min-heights match** (the
habit card and the task card resolve to the same ladder step;
equality between the two read heights — no fixed-px constant). The
strip-below-keeps-ladder-step law is the equality's meaning; the
substrate-identity argument stays prose (the fixture comment).

**Diff**: ~+120 (slint fixture + test + root `#[path]` line).

### 3.7 #997 — bind the habit editor journeys into J1's wiring inventory

**Decision**: bookkeeping + a mandatory gate run; no production code.

1. Read J1 (`2026-11-18-habit-parity-ux-plan.md`) §6/§8 and the
   chunk-10 landing's deviations section; for each habit-editor
   journey step (US-5/S1–S4, US-6/S1, US-6/S2) ensure a §6 Test cell
   or `bound-test:` line names the exact landed test path
   (`crates/taskboard-ui-slint/tests/journeys/habits_journey.rs::<name>`,
   the `crates/taskboard-state/tests/habits.rs` seam test for
   US-6/S1, plus #1059's new legs where J1's cells name them).
2. New journey names use the exact step tokens (`us6_s2_...`,
   `us5_s3_...`) — the ambient `us6_s2_*` tests in `journeys.rs`
   are what make exact tokens + explicit bound-test lines mandatory
   (AMBIGUOUS is not RESOLVE).
3. **Mandatory**: run
   `scripts/ux-coverage.sh .artifacts/plans/2026-11-18-habit-parity-ux-plan.md --gate`
   (stage 2, after the #1059 journeys land) — the US-6 (and any
   habit-editor) rows must read BOUND/RESOLVED; MISSING or AMBIGUOUS
   on a claimed row is not done. Record the gate output in the
   stage-2 report. This gate run is short (a scan, not the coverage
   trio) and may run in the foreground.

**Diff**: ~+25 (plan file edits) + the gate evidence.

## 4. Gates & discipline

- Per [D]: pre-commit hooks +
  `scripts/run-quality-gates.sh cargo nextest run --workspace`
  (background + poll; commit landed work before any wait; defer per
  the rule if the lock is held, reporting the holder).
- Expected-heavy builds (clippy) route through
  `scripts/memory-gate.sh`.
- `cargo fmt`/clippy clean on every commit; no `tokio::time::sleep`
  in tests; typed outcomes only; new `.rs` test files carry the
  AGPL-3.0-only header (ui-slint crate).
- Stage 2 commits: one standalone commit per item
  (`test(ui-slint): ...` / `fix(tests): ...` styles per type map),
  commit hygiene per AGENTS §8 (every commit builds — item order
  above keeps each commit green).

## 5. Out-of-scope findings → backlog issues (opened at stage 1)

1. `habits_journey.rs`'s private `Harness` predates the B4a helper —
   migrate the file's existing journeys onto `support::harness`
   (backlog issue, opened with this plan).
2. Any further findings during stage 2/3 per the standing
   authorization.

Numbers reported in the final report.

## 6. Risks

- **#996 site sweep**: the `initial-focus:` literal may have several
  host sites; missing one leaves a dead literal (clippy/slint warns
  in the best case, silently pins stale focus in the worst — the
  stage-3 functional focus names this trap).
- **#1024 double-option shapes**: the domain mask's per-field
  `Option` layering differs (plain vs double); folding mechanically
  by sibling example could invert a clear. The domain mask
  definition is the oracle, not the neighbor arms.
- **#1023/#995 pins are quirk pins**: they pin today's backend
  behavior; an upstream i-slint bump can flip them — that is the
  intent (the `..._currently_pins_...` naming makes the flip a
  triage, not a regression), but stage 3 must not "fix" the pin into
  an oracle of desired semantics.
- **#997 AMBIGUOUS ambient hits**: journeys.rs's ambient `us6_s2_*`
  tests may keep the row AMBIGUOUS even with a bound-test line if
  the token grammar counts ambient candidates — the gate output in
  the stage-2 report is the arbiter; if the grammar can't resolve,
  the fallback recorded here is renaming the #1059/new journeys to
  the exact unique tokens and re-running the gate (never weakening
  the gate script itself).
- **Probe binary platform invariants**: new probe members must mount
  from the root whose `HEADLESS_RENDERER` matches their backend
  needs (#1018 rendered geometry → `probe_pixel.rs`; #1023's
  geometry facet → same root; #995 rides the existing
  `widget_framework_probe_tests.rs` root).

## 7. Estimated diff size

~640 lines total: `fake_engine.rs` ~45; `modal-dialog-host.slint` +
models + probe ~65; #995 pin+doc ~70; #1023 probe+doc ~110;
#1059 journeys ~260; #1018 probe ~120; #997 plan edits ~25;
`#[path]`/root plumbing ~10. No production (non-test) code paths
change except `modal-dialog-host.slint` + its Rust model helper and
the fixture `fake_engine.rs`.

## 8. Acceptance

- Each item landed per §3; every open decision above is binding.
- `#997`'s gate rows RESOLVE (gate output recorded; no MISSING).
- New journeys compose through `support::harness` (no hand-rolled
  setup in new code).
- Diff confined to: `crates/taskboard-ui-slint/tests/**`
  (support, probes, journeys), `crates/taskboard-ui-slint/ui/widgets/
  modal-dialog-host.slint`, its Rust models/site call sites,
  `tests/support/fake_engine.rs`, and the J1 plan file.

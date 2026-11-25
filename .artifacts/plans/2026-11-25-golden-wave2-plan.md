# Golden Wave 2 — Chunk Plan (round 7, chunk G2, #1312)

- **Date**: 2026-11-25
- **Chunk**: G2 of `2026-11-25-backlog-reduction-round7-dispatch-plan.md` §5;
  branch `test/golden-wave2`; issue #1312 is the body-normative source.
- **Normative inputs**: the golden-coverage review report
  (`.artifacts/reports/2026-11-21-golden-coverage-review-report.md`) §6
  lists D5–D7 and R1–R6; the dispatch plan §1/§5; AGENTS §5 clock
  discipline (the frozen render clock law, `UiSettings::clock`).
- **Landed base** (origin/main e8025b04): G1's deterministic PNG encoder
  (`golden_compare::encode_png`, pinned settings, byte-identity corpus
  test), the 34-golden corpus re-based on the enriched fixture
  (`golden_state_enriched_at` / `today_overdue_enriched_at`), the
  manifest upsert (`golden_manifest::record` + the bijection test), the
  #1139 comparator (`golden_compare::matches`).

## 1. Seams (read and confirmed)

- **Harness**: `crates/taskboard-ui-slint/tests/journeys/golden_visuals.rs`
  — `capture_golden` (mount → `frame` escape steps → quiescence settle →
  snapshot → regen-or-compare → `manifest_record`), `scale_settings`
  (freezes `UiSettings::clock` at `fixture::PINNED_NOW`), `GoldenMeta`
  (fixture constructor + state note + scale), matrix constants
  (`BREAKPOINTS [600,1000,1500]`, `SCALES [0.5,3.0]`).
- **Editor/overlay staging**: overlays open via
  `handle.dispatch_action(ActionId::…)` after the walk (the focused-cell
  golden's idiom: `NavFocusBoard` → `NavFirstItem` → `TaskEdit`). The
  wave-2 verbs: `TaskDueEdit` (D6; the `DatePicker` context's grid opens
  over the due editor), `LabelManage` (D7; `Flow::Labels` over the
  subject task — the enriched fixture's task already carries label
  chips), the recur-editor flow (R2; `ContextId::RecurEditor`), the
  confirm flows (`Flow::ConfirmBoardReset` danger; the success-tone
  strip — see §4 D-R3), the board move/insert indicator (R6).
- **Fixtures** (`support/fixture.rs`): `golden_state_enriched_at`
  (labels, habits w/ streak chips, recur task, second board, degraded
  sync), `today_overdue_enriched_at` (overdue rows), `empty_board_state`,
  `today_overdue_state_at` (the overdue machinery the D5 banner
  rides), `harness_calendar`. The enriched fixture needs no structural
  enrichment for D5–D7/R1–R2 — the overdue banner, due editor, labels
  overlay, recur editor and habit rows all mount over the existing
  enriched state; only the *open-state escapes* are new.
- **Kiosk posture** (R1): the kiosk rotation settings idiom
  (`kiosk_rotation_settings`) + the presentation law "kiosk gates the
  actions, disables not hides" (`habit_editing_enabled`, presentation.rs
  :8124) — the kiosk-disabled habit affordance renders greyed on the
  habit row substrate.
- **Icon probe** (R5): `tests/icon_probe.slint` — the kinds wall + tone
  ladder, currently captured at 1.0 only (`icon-probe-1.0-396`).
- **Manifest**: regen upserts in place; the bijection test
  (`the_manifest_bijects_with_the_golden_corpus`) enforces one row per
  PNG with matching width/height.

## 2. Golden inventory (decided)

Naming convention: `<surface>-<state>-<scale>-<width>` where the state
segment distinguishes open/staged variants of the same surface (the
review report §5's naming finding — state in the name). All captures
at the default matrix height 900 unless the surface is content-sized
(alarm-window keeps its 64-px height; the icon probe keeps its own
window height), all with `UiSettings::clock = PINNED_NOW` and the UTC
zone pin.

### A. The deferred editors (3 goldens)

| # | Name | Scale/size | Fixture + staged content |
|---|---|---|---|
| D5 | `task-view-overdue-banner-1.0-1000` | 1.0, 1000×900 | `today_overdue_enriched_at(PINNED_NOW)`'s overdue task walked into detail; the overdue banner mounted with its mute/snooze affordances visible |
| D6 | `due-editor-datepicker-1.0-1000` | 1.0, 1000×900 | enriched task detail + `TaskDueEdit` (due editor open, the `DatePicker` grid overlay open over it) |
| D7 | `labels-overlay-1.0-1000` | 1.0, 1000×900 | enriched task (carries chips) + `LabelManage` (labels overlay open; the detail's label chips visible beneath) |

The staging law ([L](c)): every editor golden shows the editor WITH
content — D5's banner is over an overdue task (not an on-time one), D6's
editor opens on a task WITH a due date so the picker grid pre-selects,
D7's overlay opens over a task already carrying chips. An empty mount
under-shows and is a review finding.

### B. The review-critical states (12 goldens)

| # | Name | Scale/size | State |
|---|---|---|---|
| R1a | `today-habits-1.0-1000` | 1.0, 1000×900 | Today view with habit rows + streak chips (desktop posture, affordances enabled) |
| R1b | `today-habits-kiosk-0.5-1000` | 0.5, 1000×900 | kiosk rotation, today mode with habit rows; the kiosk-disabled posture (gated affordances render disabled, not hidden) |
| R2a | `recur-editor-modal-1.0-1000` | 1.0, 1000×900 | the enriched fixture's recurring task; recur editor modal open, closed dropdown |
| R2b | `recur-editor-dropdown-1.0-1000` | 1.0, 1000×900 | same modal with the frequency dropdown flyout OPEN (the flyout anchor law: the dropdown anchored to its trigger over the modal chrome) |
| R3a | `confirm-strip-danger-1.0-1000` | 1.0, 1000×900 | armed confirm strip, danger tone (the board-reset confirm over the enriched board) |
| R3b | `confirm-strip-success-1.0-1000` | 1.0, 1000×900 | armed confirm strip, success tone (the reconcile confirm; stage 2 confirms the exact flow — see §4) |
| R4a | `board-empty-1.0-1000` | 1.0, 1000×900 | `empty_board_state` board view; EmptyState alignment |
| R4b | `today-empty-1.0-1000` | 1.0, 1000×900 | Today with zero rows (no-tasks posture) |
| R4c | `search-palette-empty-1.0-1000` | 1.0, 1000×900 | palette open with a typed query matching nothing (the empty-results body) |
| R5a | `icon-probe-0.5-396` | 0.5, 392×372 | the kinds wall at the 0.5 icon edge |
| R5b | `icon-probe-3.0-396` | 3.0, 392×372 | the kinds wall at the 3.0 icon edge (icon-size coherence across the whole IconKind set — stage 2 verifies the wall covers every `IconKind` variant of `ui/widgets/icon.slint`, extending the wall if the 19-asset grid lags the enum) |
| R6 | `board-drag-indicator-1.0-1000` | 1.0, 1000×900 | the board with the insert indicator lit mid-move (the move signal's spacing) |

### C. Breakpoint variants for the seven wave-1 goldens (14 goldens)

The seven PR #1242 additions (settings, capture-overlay, search-palette,
review-walk, action-menu, alarm-window, plan-next-day) landed at 1.0 /
width 1000 only. The issue's law: 600/1500 variants for all seven —
14 new goldens, each with the SAME fixture/state description as its
parent row (the manifest's `state` field is identical; only the axes
differ):

- `settings-1.0-600`, `settings-1.0-1500`
- `capture-overlay-1.0-600`, `capture-overlay-1.0-1500`
- `search-palette-1.0-600`, `search-palette-1.0-1500`
- `review-walk-1.0-600`, `review-walk-1.0-1500`
- `action-menu-1.0-600`, `action-menu-1.0-1500`
- `plan-next-day-1.0-600`, `plan-next-day-1.0-1500`
- `alarm-window-1.0-600`, `alarm-window-1.0-1500`

**Decision — alarm-window's breakpoints**: the window is content-sized
(351×64 today); the 600/1500 variants widen the window with the same
content (the height stays 64). This honors the issue's "seven" verbatim;
if the surface clamps its own width, the captured PNG records that truth
and the manifest row matches the decoded size.

**Total new goldens: 29. Corpus after wave 2: 63.**

## 3. Comparator determinism proof (decided)

1. Every new golden is captured through the landed deterministic encoder
   (G1): `golden_compare::encode_png` with pinned settings.
2. **Regen-twice proof per new golden**: run the suite with
   `TASKBOARD_REGEN_GOLDENS=1` twice (each run through
   `scripts/memory-gate.sh`, backgrounded per the long-command law),
   `sha256sum` the goldens dir after each run, diff the digests —
   byte-identical is the acceptance evidence, recorded in the stage-2
   report.
3. The two existing corpus tests extend automatically and bind the new
   goldens with no new code: `the_committed_corpus_reencodes_byte_identical`
   (encoder-graph drift law) and `the_manifest_bijects_with_the_golden_corpus`
   (one manifest row per new PNG, matching width/height).
4. No comparator tolerance change, no new tolerance knobs ([L](c): the
   #1139 comparator is untouched).

## 4. Open questions → recorded decisions

- **D5's fixture**: `today_overdue_enriched_at` vs extending
  `golden_state_enriched_at` with one overdue task. **Decision**: walk
  into the overdue task from `today_overdue_enriched_at` (the overdue
  machinery is already proven there; no fixture edit). If the detail's
  overdue banner requires a command the FakeEngine path doesn't cover,
  fall back to enriching `golden_state_enriched_at` — recorded then.
- **R3's success tone**: the danger strip is the board-reset confirm
  (`Flow::ConfirmBoardReset`, proven flow). The success-tone strip's
  flow (reconcile confirm per actions.rs :387) may need the oneshot
  preview request answered. **Decision**: stage 2 uses the reconcile
  flow if reachable hermetically; if the preview plumbing blocks the
  capture, the break-rule removal confirm (or whichever flow renders
  the success strip) is substituted and the substitution recorded —
  the invariant is *the strip in its success tone*, not a specific flow.
- **R6's driver mechanism**: the insert indicator may need pointer-move
  simulation through `SlintDriver` rather than a pure action dispatch.
  **Decision**: stage 2 prefers whatever move-mode entry the keyboard
  grammar offers (the adapter's move verbs) so the capture stays on the
  established dispatch idiom; pointer synthesis only if no dispatch
  route lights the indicator, recorded then.
- **R5's coverage**: the kinds wall currently mirrors 19 vendored
  assets. **Decision**: stage 2 enumerates `IconKind`
  (`ui/widgets/icon.slint`) against the wall; the wall is extended to
  the full enum BEFORE the two scale captures (the probe's cell-grid
  mirror test updates in step per its own header law). The golden names
  keep the `-396` stem only if the window size is unchanged; the stem
  follows the actual window (the manifest is the truth, and §5's
  mismatch finding below is the lesson).
- **Where the new tests live**: `golden_visuals.rs` grows the capture
  tests grouped by family (editors / review states / breakpoint
  extensions) reusing `capture_golden` verbatim — no harness
  generalization beyond what a family needs (a matrix test per family
  only where the issue asks for a matrix; each single-state golden is
  one `#[test]`, matching the existing one-behavior-one-test layout).
- **No production-code change**: diffs confined to
  `golden_visuals.rs`, `fixture.rs` (only if a decision above forces a
  fixture edit), `icon_probe.slint` + its mirror test (R5 extension),
  the new PNGs, and the manifest rows (auto-upserted by regen).

## 5. Execution order (stage 2)

1. Editors D5–D7 (three tests, regen, commit).
2. Review states R1–R6 (twelve tests; the R5 wall extension first,
   regen, commit).
3. Breakpoint variants (the seven capture fns get width-parameterized
   paths + the matrix loops; regen, commit).
4. The regen-twice byte-identity proof over the whole corpus; record
   digests.
5. Gate: pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest
   run --workspace` (background + poll; defer per the flock rule).
   Every cargo invocation through `scripts/memory-gate.sh`.

## 6. Risks

- **Overlay-open fragility**: the walk-then-dispatch idiom depends on
  focus state; a mis-anchored dropdown (R2b) or a refused kiosk/no-subject
  dispatch (D6/D7 adapter refusals) silently captures the closed state.
  Mitigation: the manifest `state` note + a stage-2 structural
  assertion (the overlay's context/predicate read) before snapshot.
- **Success-tone strip reachability** (R3b) — substitution risk, §4.
- **Icon wall drift** (R5): extending the wall moves its window size,
  which changes the existing `icon-probe-1.0-396` golden → that rebase
  rides this chunk deliberately (one whole-corpus regen commit covers
  it; the manifest bijection test forces the row update).
- **Clock discipline**: every new capture freezes
  `UiSettings::clock = PINNED_NOW` + `pin_utc_zone()` — the banner's
  "overdue by" phrase, the date-picker's pre-selected day and the
  recur editor's next-occurrence phrase are all clock-derived text.
- **Estimated diff size**: ~1,200–1,500 lines of Rust
  (golden_visuals.rs ~+900, fixture/icon-probe ~+150–300) plus 29 new
  PNGs (~2–6 MB total) and 29 manifest rows. No production-code diff.

## 7. Backlog issues filed by this stage

- #1390 — `backlog: golden name stems embed stale width/height
  (alarm-window-1.0-1000 is 351×64; icon-probe-1.0-396 is 392×372)` —
  a doc-goal readability defect found while reading the manifest (the
  review report §5's naming finding, two concrete instances). Wave 2's
  new names follow the actual window; the existing two rows stay as-is
  here (out of scope).

# UI Golden Coverage Review — Report

**Date:** 2026-11-21 · **Branch:** `docs/ui-golden-coverage-review` · **Scope:** review only (no goldens/fixtures changed)

**Goal.** The golden screenshots (`crates/taskboard-ui-slint/tests/goldens/`, 27 PNGs, produced by `tests/journeys/golden_visuals.rs` via the Slint software testing backend) are to be reused as (a) documentation screenshots and (b) input for automated UI reviews (spacing/alignment, font/icon sizing, positioning). This review audits whether they can carry that weight: surface/state coverage, fixture representativeness, and compare-mechanism sensitivity.

**Oracle.** The UX surface census (`docs/ux-surface-census.org`, round-9 state) sliced by Board / Today / Capture / Task detail / Settings / Review / Alarm window / focus-and-modal choreography. Cross-checked against the UI tree (`ui/views/`, `ui/compositions/`, `ui/widgets/`).

---

## 1. What the 27 goldens cover today

| Golden set | Surface | Axes captured |
|---|---|---|
| `board-{0.5,3.0}-{600,1000,1500}` (6) | Board, populated 2-stack fixture | scale × breakpoint |
| `today-{0.5,3.0}-{600,1000,1500}` (6) | Today desktop, overdue + running-timer chip | scale × breakpoint |
| `kiosk-board-0.5-1000`, `kiosk-today-0.5-1000` | Kiosk rotation modes | kiosk axis |
| `task-view-{0.5,1.5,3.0}-{500,900}` (6) | Task detail default state | scale × scroll height |
| `task-view-focus-cell-1.0-900` | Task detail focused due cell | keyboard-focus state |
| `notification-3.0-1000` | Notification surface (task + alarm rows) | delivered-notification state |
| `focus-overlay-1.0-1000` | Concentration overlay, running Work block | overlay state |
| `track-entries-1.0-1000` | Time-entries overlay, populated | overlay state |
| `review-empty-1.0-1000`, `review-ledger-1.0-1000` | Review view, both empty and populated | empty vs populated |
| `icon-probe-1.0-396` | Icon determinism probe composition | probe (not a doc surface) |

Coverage character: **strong on the two primary views and their presentation axes; very thin everywhere else.** Roughly 20 of the census's user-visible surfaces/overlays have zero golden.

## 2. Coverage matrix (surface × state → golden or gap)

Legend: ✅ covered · ❌ gap · ⊖ not a user-visible surface.

| Census slice / surface | Populated | Empty | Error/degraded | Focus/interaction |
|---|---|---|---|---|
| Board (desktop) | ✅ board-* | ❌ empty-board CTAs | ❌ offline sync badge (fixture carries it in journeys only) | ❌ drag ghost/insert indicator, armed confirm strip, stack count badge |
| Board kiosk | ✅ kiosk-board | ❌ | ❌ archived-roster indicator (kiosk disabled posture) | ❌ |
| Stack view | ❌ (no golden at all) | ❌ | — | ❌ |
| Today (desktop) | ✅ today-* | ❌ no-tasks Today | ❌ files-sync warning chip, transient status notices | ❌ habit context menu |
| Today habits | ❌ habit rows never golden'd | ❌ | ❌ quota error hint | ❌ |
| Timers / break rules | ❌ timer-setter, rule-list never golden'd | ❌ | — | ❌ |
| Capture overlay | ❌ | ❌ replaced-flow notice | — | ❌ |
| Search palette | ❌ | ❌ empty-state caption | — | ❌ |
| Action menu (2-tier, context quick bar) | ❌ | ❌ disabled-row notice | — | ❌ |
| Task detail | ✅ task-view-* | ❌ blank-detail | ❌ overdue banner (mute/snooze) | ✅ focus cell · ❌ armed-field rings |
| Due editor / date picker | ❌ | — | — | ❌ |
| Recur editor (ModalDialogHost) | ❌ | — | ❌ paused chip, convert strip | ❌ |
| Labels overlay + chips | ❌ overlay | ❌ | ❌ staged delete strip | ❌ |
| Importance/urgency dropdowns | ❌ | — | — | ❌ (flyout open, check glyph) |
| Habit detail variant (H4) | ❌ | — | — | ❌ |
| Track entries overlay | ✅ | ❌ empty | — | ❌ |
| Review view | ✅ both legs | ✅ | — | ❌ review-walk-overlay (16 walk verbs, planner embed, summary) |
| Plan-next-day view | ❌ | ❌ | — | ❌ placement form |
| Settings view | ❌ (no golden at all) | ❌ | ❌ connection cards, roster rows, env-locked badges, facts panel, undo-window badge | ❌ publish picker flyout |
| Notification surface | ✅ | ❌ | — | ❌ |
| Alarm window | ❌ (distinct surface from the notification overlay; never golden'd) | ❌ awaiting-rows empty | — | ❌ keyboard walk |
| Focus overlay | ✅ running | ❌ idle/break phase | — | ❌ |
| Modal containment / tab trap | ⊖ choreography | — | — | ❌ (mask + wrap not visually pinned) |

**Gap verdict.** Seven census surfaces have zero golden coverage: settings-view, plan-next-day-view, review-walk-overlay, capture-overlay, search-palette, action-menu, alarm-window; plus the editors/anchored overlays (due, recur, labels, dropdowns) and all habit/timer compositions. For a documentation goal, the most visible absences are Settings, the capture→triage flow, the search palette, the editors, and the review walk — these are the screenshots a docs site would need first. For a review goal, every widget in `ui/widgets/` is only photographed if some covered surface mounts it in a visible state; many primitives (dropdown, tooltip, menu-shell, modal-dialog-host, status-notice tones, confirm-strip tones, slider, badge tones) are currently visible in **no** golden or only in one tone.

## 3. Fixture representativeness

`tests/support/fixture.rs::golden_state_at` (the golden-tier fixture): 1 board, 2 stacks, 3 tasks (ASCII titles), one running timer, one review-card pin, sync idle.

Findings:

1. **Too sparse to showcase the app.** No labels, no habits, no recurrence, no reminders, no estimates/completed work, no descriptions, single board. Features like label chips, recur chips, reminder badges, habit rows, estimates, and the multi-board nav exist in the product and have census rows but appear in no golden fixture (some exist in journey fixtures — `journey_state_at` adds overdue + offline badge — but journeys aren't goldens).
2. **ASCII-only titles** (a determinism measure) means the goldens never exercise the font stack's real-world glyph coverage — acceptable for regression, wrong for documentation.
3. **Only 3 tasks** — no overflow, elision, or scroll behavior is photographed (task-view-500 does capture scroll of the detail pane).
4. **One healthy state, few degraded states.** Offline badge, delivery-health notice, files-sync warning, quota hints are all implemented but never in a golden.
5. **Clock discipline is exemplary** (pinned `PINNED_NOW`, TZ=UTC, midnight-aligned stamps) — any fixture enrichment must keep this per `docs/testing_strategy.org` §6.

## 4. Compare-mechanism sensitivity audit (empirical)

Method: a throwaway Rust harness (in `/tmp`, replicating `golden_compare.rs`'s `gray_at`/`cell_grid`/`max_cell_diff` verbatim — 32×24 box-averaged Rec.601 luma cells, tolerance ≤8/255) applied synthetic mutations to six real goldens spanning scales 0.5–3.0 and reported max cell diff. Harness not committed; parameters reproducible from the table.

| Mutation class | Scale 0.5 (600px) | Scale 1.0 (1000px) | Scale 3.0 (1500px) |
|---|---|---|---|
| Badge appears 10×10 | caught (9) | missed (6–7) | missed (3–6) |
| Badge appears 16×16 | caught (23) | caught (11–17) | missed (6) |
| Badge 24×24 | caught (27) | caught (21–26) | caught, borderline (10–12) |
| Content band shift 1–2px | 1px missed / 2px caught | 1px missed / 2px borderline (9) | missed up to 2px |
| Content band shift 4–8px | caught | caught (9–18) | caught (10–35) |
| Icon swap 8×8 inverted | — | caught at diff ≈ 11–12 | missed (7) |
| Font-metric shift ~3px sub-band | — | caught on ledger (12), missed elsewhere (3–7) | missed (4–7) |
| **Hue-only change (red↔blue, equal luma, 40×16)** | caught, accidental (11) | **missed (5–8)** | **missed (5)** |

Conclusions:

1. **The comparator degrades exactly where the screenshots are biggest.** At scale 3.0 a cell is ~47×37px, so any sub-20px element change (badges, icon swaps, small spacing corrections) averages away. The regression classes it was calibrated for (card moved/deleted) are large; the classes an automated *layout-quality* review cares about (2–4px spacing drift, badge presence, icon identity, font-metric shifts) are at or below tolerance at high scales.
2. **Chroma is discarded.** Rec.601-luma-only compare makes same-luma hue swaps (semantic colors: danger vs success vs info) structurally invisible. Automated UI review must see color; today's comparator cannot.
3. **Small drifts pass with only a tracing warn** (diff 1–8) — appropriate for a flake-tolerant regression tier, wrong as the *basis* for layout review.
4. **Stale doc comment:** `golden_compare.rs:3` says "hand-rolled dHash-64 … Hamming distance ≤ 8 of 64 bits"; the implementation is the 32×24 cell grid. Documentation-only fix candidate.

**Verdict:** the current comparator is fit for its regression purpose but **not sufficient** as the foundation for automated spacing/alignment/color review. For that goal, a separate analysis pass over the same goldens is needed — candidates: full-resolution per-channel diff with region-of-interest/structural analysis, or a much finer cell grid plus chroma cells, run *outside* the CI gate (review tooling, not a test assertion).

## 5. Fitness for the documentation goal

- **Reproducibility is excellent:** software renderer, bundled Inter/JetBrains Mono, pinned clock, byte-stable probes — regenerating a doc screenshot set is deterministic.
- **Publishing resolution:** the 0.5-scale renders (600–1500px wide) are usable for docs; the 3.0 set doubles as high-DPI assets. A doc pipeline can pick per surface.
- **Naming is only partly self-describing:** `<surface>-<scale>-<size>.png` says nothing about the *state* (which fixture, which overlay open, which badge lit). A manifest (fixture + state description per golden) is needed before an index can be auto-generated for docs. Recorded as backlog.

## 6. Recommendations (prioritized)

Documentation-critical goldens to add:

| # | Golden scenario | Fixture work |
|---|---|---|
| D1 | Settings view, populated connection + board roster | settings fixture (fake connections/roster) |
| D2 | Capture overlay open over board | trivial (open flow) |
| D3 | Search palette with results | roster fixture |
| D4 | Action menu (2-tier + context quick bar) | existing board fixture |
| D5 | Task detail with overdue banner | extend journey fixture to goldens |
| D6 | Due editor + date-picker overlay open | existing task fixture |
| D7 | Labels overlay + task label chips | labels fixture |
| D8 | Review walk overlay (guidance + planner embed) | review fixture |
| D9 | Plan-next-day view with pool + boxes | plan fixture |
| D10 | Alarm window with awaiting rows | alarm fixture |

Review-critical (layout-quality targets):

| # | Scenario | Why |
|---|---|---|
| R1 | Habit rows on Today (incl. kiosk-disabled posture) | card-substrate spacing, chips |
| R2 | Recur editor modal + dropdown open states | modal-host chrome, flyout anchor law |
| R3 | Armed confirm strips (danger + success tones) | tone correctness (invisible to luma compare!) |
| R4 | Empty states (empty board, no-tasks Today, empty palette) | EmptyState alignment |
| R5 | Icon inventory probe extension (all IconKinds at 0.5/3.0) | icon-size coherence |
| R6 | Drag/insert-indicator state on board | spacing of the move signal |

Fixture enrichment (shared by both goals): labels (incl. LABEL_CHIP_CAP overflow), habits with streak chips, one recurring task, one estimated+tracked completed task, one second board, one long/overflow title, one degraded sync state — all under the pinned-clock discipline.

Comparator: do not tighten the CI gate (it would re-open the flake war the tolerance was calibrated to avoid). Instead build the automated review as a separate offline tool (untracked or `--ignored` test) doing full-resolution chroma+structure analysis, seeded by the sensitivity thresholds above (badge ≳20px @3.0 invisible; hue always invisible).

## 7. Backlog issues

- #1139 — comparator is luma-only / cell-averaged (chroma + sub-20px blindness at 3.0)
- #1140 — golden manifest (self-describing fixture + state index) for doc reuse
- #1141 — golden coverage gaps (7 zero-coverage surfaces) + fixture enrichment

## Appendix A — Inventory cross-check

All 8 `ui/views/*.slint`, 19 `ui/compositions/*.slint`, and 37 `ui/widgets/*.slint` files were enumerated; every composition/view appears in §2's matrix (empty cells read as "not covered"); widget primitives are covered only transitively, as noted in §2.

# The Golden Coverage Wave — Plan

**Chunk:** round 4 S7 (`test/golden-coverage-wave`) · **Date:** 2026-11-22
**Normative inputs:** `.artifacts/reports/2026-11-21-golden-coverage-review-report.md`
(§6 priority lists D1–D10 / R1–R6), issues #1139 (comparator), #1140 (manifest),
#1141 (gap goldens), #1229 (render-prose mutants), dispatch plan
`2026-11-21-backlog-reduction-round4-dispatch-plan.md` §1/§4 S7 row.
**Laws that bind:** typed outcomes; goldens are committed artifacts → the frozen
render clock law (AGENTS §5; `docs/testing_strategy.org` §6.1–6.2); a
comparator-upgrade-exposed drift is a finding to fix, not a tolerance to loosen;
no quality ceilings; no text assertions (typed/structural facts only).

---

## 1. Current seams (verified in the worktree)

- Harness: `crates/taskboard-ui-slint/tests/journeys/golden_visuals.rs` —
  `pin_utc_zone()`, `scale_settings(scale)` freezing `UiSettings::clock` at
  `fixture::PINNED_NOW` (2026-11-06 12:00 UTC), `capture_golden(...)` doing
  regen-or-compare via `TASKBOARD_REGEN_GOLDENS=1`. 27 committed PNGs in
  `tests/goldens/`.
- Comparator: `tests/support/golden_compare.rs` — 32×24 box-averaged Rec.601
  **luma-only** cells, `MAX_CELL_DIFF = 8`, exact-dimension precondition,
  drift ≤ tolerance warns. Doc-comment at `golden_compare.rs:3` still claims
  "dHash-64, Hamming ≤ 8/64" — stale (the #1139 drift).
- Fixture: `tests/support/fixture.rs::golden_state_at` — 1 board, 2 stacks,
  3 ASCII tasks, one running timer (`now − 12 m`), review-card pin, sync idle.
  Helpers already exist for alarms, reflections, break rules, open entries,
  completed tasks, deliveries.

## 2. Wave order (dispatch-law decision)

**Phase A — comparator upgrade (#1139) → Phase B — manifest (#1140) →
Phase C — gap goldens (#1141, enriched fixture).** Rationale: the manifest
must describe the *post-wave* golden set (writing it in A would churn it in C),
and the new goldens in C must be minted against, and accepted by, the upgraded
comparator so their acceptance encodes the new sensitivity classes from day
one. Each phase is its own commit on the single chunk branch; one PR for the
chunk (one coherent deliverable).

## 3. Phase A — comparator upgrade (#1139)

### 3.1 Design decision: upgrade in place, gate semantics preserved

The review report §6 recommends *not* tightening the CI gate and building the
automated review offline. **Decision:** reconcile the two directives by
upgrading `golden_compare` so the *classes* it can see grow (chroma, scale-aware
cells) while the *regression gate* the 27 existing goldens pass through keeps
its calibrated headroom:

1. **Chroma cells.** Extend the cell grid with per-cell chroma channels
   (Rec.601 Cb/Cr, same box-averaging). A same-luma hue swap (danger↔success
   tone) then reads a large chroma-cell diff — the audit's "always invisible"
   class becomes visible at every scale.
2. **Scale-aware cell grid.** Cell *physical* size instead of a fixed 32×24
   count: the grid resolution derives from the image dimensions so a cell is
   roughly constant in physical pixels (at scale 3.0/1500px the current grid
   is ~47×37px/cell; scale-aware sizing restores sub-20px sensitivity where
   the audit measured blindness). The grid is derived purely from
   `(width, height)`, so both compare sides always agree by construction.
3. **Doc-comment drift fixed** (`golden_compare.rs:3`): header now describes
   the cell grid, the chroma extension, and the scale-aware sizing.
4. **Tolerance recalibration recorded** in the module docs and the PR body:
   luma budget stays `MAX_CELL_DIFF = 8` (same class, finer grid ⇒ recalibrate
   only if re-recording noise on the 27 demands it); a separate
   `MAX_CHROMA_CELL_DIFF` budget calibrated the same way (±1 LSB noise
   proptest as the noise floor, the mutation audit's thresholds as the signal
   floor). Both budgets are constants with evidence comments, never tuned per
   golden.
5. **The mutation audit's blind spots become pinned unit cases** (typed
   structural outcomes, synthetic images — no PNG fixtures needed):
   a ≤16px badge appearing at the largest render size exceeds the budget;
   a same-luma red↔blue band exceeds the chroma budget at every grid;
   a 2px band shift at high resolution exceeds the budget. These are
   `golden_compare` unit tests — they pin the comparator's sensitivity, not
   any UI state.

**Trap (a) procedure:** if the upgraded comparator flags one of the existing 27
on an unregenerated render (i.e. the *committed* golden disagrees with the
current renderer beyond the new budgets), that is a newly-caught real drift:
diagnose and fix the render or the stale golden as a finding — never widen the
budget to make it pass. The software renderer is byte-stable and the clock is
frozen, so a legitimate diff of the 27 is expected to be exactly 0; anything
else is either machine noise (re-measure, then recalibrate the noise floor
*evidence comment*, not the budget) or a finding.

### 3.2 Acceptance (Phase A)

- All existing 27 goldens pass unchanged (no regen, no budget loosening);
  any failure is triaged per the trap procedure and recorded in the PR body.
- New pinned unit tests pass for the three formerly-blind mutation classes.
- Proptest noise floor holds for the chroma cells (±1 LSB ⇒ ≤1 chroma level).

## 4. Phase B — manifest (#1140)

**Decision:** a checked-in `tests/goldens/manifest.toml`, generated by the
golden harness at regen time and verified in the normal test path. Structure:

```toml
[[golden]]
name = "settings-1.0-1000"
fixture = "settings_roster_state_at(PINNED_NOW)"   # constructor, not prose
state = "settings view; connection cards + board roster rows rendered"
axes = { scale = 1.0, width = 1000, height = 900 }
```

- **Generation:** the regen path (`TASKBOARD_REGEN_GOLDENS=1`) writes/updates
  the manifest entry for every golden it captures (the harness already knows
  surface, fixture, axes at the call site — the capture functions carry the
  description).
- **Verification:** a single test (lowest layer: this is the manifest's own
  behavior) parses the manifest and asserts structural agreement with the
  committed file set: every PNG has exactly one manifest entry and vice versa;
  the `scale`/`width`/`height` axes match the decoded PNG dimensions. Typed
  outcomes (`matches!(err, …)`/structural facts) — no text assertions on the
  prose fields.
- The existing 27 get their manifest entries written in this phase (fixture =
  `golden_state_at`/`journey_state_at`-derived per the harness call sites).

**Acceptance (Phase B):** manifest ↔ file-set bijective; a test deleting an
entry (or an orphan PNG) fails with a typed error; entries for all 34 goldens
(27 + the 7 to come) are present by the end of Phase C.

## 5. Phase C — gap goldens (#1141)

**Scope decision:** the chunk's title says *7 missing goldens* — exactly the
seven zero-coverage surfaces from #1141's title: **D1 settings-view, D2
capture-overlay, D3 search-palette, D4 action-menu, D8 review-walk-overlay,
D9 plan-next-day-view, D10 alarm-window.** The editors (D5/D6/D7) and the
R1–R6 review-critical list stay out of scope (already owned by #1141; the
wave plan records them as the next slice, not silent debt).

**Golden format decision:** one golden per surface at scale 1.0, 1000×900
(the `review-*`/`notification-*` convention — the doc-publishable sweet spot
per report §5). No scale matrices for the new surfaces; the matrix axes are
already covered by the board/today/task-view families.

**Fixture decision: an enriched *variant*, not a widening in place.** The
dispatch prompt says "the sparse `golden_state_at` fixture widened"; widening
`golden_state_at` itself would shift pixels in all 27 existing goldens and
force a wholesale re-record inside the same chunk that upgrades the
comparator — conflating two drift sources in one diff. **Decision:** build
`golden_state_enriched_at(now)` as a superset constructor (calls
`golden_state_at`, then layers the report §6 enrichment: a label roster with
chips incl. `LABEL_CHIP_CAP` overflow, one recurring task, one reminder badge,
a second board, one long/overflow ASCII title, one degraded sync state — all
ASCII-titled and midnight/pin-aligned per the clock law; the clock-frozen
render pass makes wall-derived text byte-stable). New goldens mount it; the
existing 27 stay byte-stable. Re-basing the 27 onto the enriched fixture is
recorded as a follow-up backlog issue, not this chunk.

Per-surface capture notes (each: fixture + the frame closure's view escape,
reusing `capture_golden`; overlays use the same open flows the journey
fixtures already drive):

| # | Golden | Fixture | State |
|---|---|---|---|
| D1 | `settings-1.0-1000` | enriched + fake connection/roster rows | settings view rendered |
| D2 | `capture-overlay-1.0-1000` | enriched | capture overlay open over the board |
| D3 | `search-palette-1.0-1000` | enriched (query hits the roster) | palette open with results |
| D4 | `action-menu-1.0-1000` | enriched | 2-tier menu + context quick bar state |
| D8 | `review-walk-1.0-1000` | review fixture (`walk` surfacing card cleared per `golden_state_at`'s pin law, or the deliberate-due variant the walk journeys use) | walk overlay + planner embed |
| D9 | `plan-next-day-1.0-1000` | enriched | plan view with pool + boxes |
| D10 | `alarm-window-1.0-1000` | enriched + `add_alarm`/`deliver_alarm` | alarm window with awaiting rows |

Determinism per surface: every wall-derived caption rides the frozen
`UiSettings::clock` (§6.2's content freeze) and midnight-aligned stamps; any
surface with a transient (dwell, animation) uses the existing
`capture_golden` settle steps (`driver.advance_time(0)`, the kiosk-dwell
precedent). Overlay opens must assert a typed precondition (the mount's
view/state field) before snapshotting — the renderer-presence assertion
already guards the raster.

**Acceptance (Phase C):** the 7 new goldens pass the upgraded comparator
deterministically (CI-green twice locally before handoff — no ceiling
assertions, exact match under the budgets); manifest entries cover all 34.

### 5.1 The #1229 interaction (recorded decision)

#1229's ~50 CLI `render_text_*` mutation survivors name this wave as their
home — but a UI golden renders the *UI* tree; it does not execute the
commands-crate CLI renderers, so goldens cannot kill that family directly.
**Decision:** Phase C kills the *UI-render prose analogue* for the seven new
surfaces (byte-exact goldens pin whatever prose those surfaces render — age
phrases, quota hints, roster rows). The CLI prose family needs its own
mechanism (structural pins on the branch-taking comparisons + a byte-exact
CLI-output characterization fixture, which is a *committed artifact* of the
same clock-law shape). Commented on #1229 with this finding; the plan does
not claim those kills.

## 6. Open questions — resolved

| Question | Decision |
|---|---|
| Tighten the CI gate to the new sensitivity? | No — new sensitivity lives in new budgets over new channels; the luma budget stays 8; the recalibration evidence is recorded, and the pinned unit tests carry the audit's thresholds. |
| Where does the manifest live? | Checked-in `tests/goldens/manifest.toml`, harness-generated on regen, structurally verified by one test. |
| Widen `golden_state_at` in place? | No — `golden_state_enriched_at` superset; the 27 stay byte-stable; re-base follow-up issue. |
| Scale matrix for new surfaces? | No — one 1.0/1000 golden per surface. |
| What if the upgraded comparator flags an existing golden? | Finding → fix (render or stale golden), never a budget widening (dispatch trap (a)). |
| Non-midnight due stamps? | Forbidden — midnight-aligned via `harness_calendar()`, per `journey_state_at`'s precedent. |
| Do the 7 new goldens need the UX journey gate? | No — pure test-coverage internals, no user-visible capability change (AGENTS §7 exemption). |

## 7. Risks

- **Comparator upgrade exposes real drift in the 27** — expected-if-any per
  the trap; costs a diagnosis loop mid-phase. Mitigation: run Phase A's
  suite before committing the manifest phase.
- **Chroma budget calibration** — the audit gives signal floors but the noise
  floor must be re-measured (the ±1 LSB proptest generalizes to chroma);
  a too-tight chroma budget would flake on renderer anti-aliasing. The
  renderer is deterministic, so the true noise floor is 0; budgets stay
  evidence-commented constants.
- **Overlay capture seams** — the seven surfaces need frame closures that
  navigate to states no golden has mounted before (settings roster fakes,
  palette query injection, alarm-window mode). Driver seams mostly exist
  (journeys drive every one of these flows); residual risk is a missing
  seam costing a small driver addition.
- **Enriched-fixture interplay with the review-card pin and sync
  derivation** — enrichment must re-run `derive_sync_states` after minting
  rows (the `journey_state_at` precedent) and must keep the review card
  cleared or deliberately present per surface.
- **Gate cost** — the ui-slint golden suite runs under the flock wrapper;
  commit all landed work before any long wait ([D] law).

## 8. Estimated diff

- `golden_compare.rs`: ~+150/−30 (chroma grid, scale-aware sizing, docs,
  pinned unit tests, proptest extension).
- `manifest.toml`: +~180 (34 entries) + one verification test ~+70.
- `fixture.rs`: `golden_state_enriched_at` + helpers ~+150.
- `golden_visuals.rs`: 7 capture blocks + manifest hooks ~+350.
- Binaries: 7 new PNGs (~50–200 KB each) + `manifest.toml`.
- **Total:** ~900–1,100 source lines + binary artifacts, one PR.

## 9. Stage-2/3 standard blocks

Implementation follows the standard blocks: pre-commit hooks +
`scripts/run-quality-gates.sh cargo nextest run --workspace` (background +
poll; commit landed work before waiting); PR open + `gh pr checks` green is
the handoff; worktree removed per the lifecycle law after PR-green.

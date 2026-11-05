# [Plan] Review-view tracking statistics (#623 slice A)

- **Date**: 2026-11-05
- **Kind**: stage-1 TDD implementation plan — chunk 4 of the v0.2
  dispatch (`feat/review-tracking-stats`), written at base `947b9f76`
  (post-chunk-3 `origin/main`). No code in this commit.
- **Origin** (normative): implementation plan
  `.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md`
  §5.3 slice A + rulings R-3 (hand-rolled charts, no chart crate) +
  facts VF-1 (the Review home's skeleton discipline) / VF-4
  (`tracked_in_window`, day-window machinery) / VF-8 (push-pass purity)
  / VF-10 (house rules); issue #623 + its 2026-11-04 ruling comment;
  the dispatch plan §5 (`2026-11-04-v0.2-focus-reflection-dispatch-plan.md`,
  acceptance + stage-3 traps); the effort-calibration round's D-4/D-8/
  D-9/D-10/D-12
  (`.artifacts/plans/2026-10-31-effort-calibration-surfaces-plan.md`).
- **Status**: READY for stage 2. Every open question carries a
  recorded decision (D-S1…D-S12; the D-S namespace avoids colliding
  with the landed D-1…D-12 of the calibration round). Deviations from
  a ruling go back to the user, never silently adapted.
- **Acceptance** (mirrors dispatch §5 chunk 4): D-8/D-9 skeleton tiers
  keep holding as sections grow (structural tests pin them); all
  aggregates are pure functions with fixed-stamp tests incl. boundary
  splits; the Work-block limitation is recorded at the projection
  (doc + test naming); bars compose from pushed values only (no
  slint-side math beyond geometry); empty/collapse tiers pinned;
  kiosk unchanged (rotation still never lands here); CHANGELOG entry;
  full gate suite green.

## 1. Verified anchors (base `947b9f76`)

Symbols are stable keys; line numbers are plan-day references.

| Anchor | Where | Notes |
|---|---|---|
| `TimeEntryKind::{Work, Respite}` | `taskboard-domain/src/tracking.rs:71` | kebab serde `"work"`/`"respite"`; `Work` is `#[default]`; exhaustive two-arm enum |
| `tracked_in_window(entries, (start, end), now)` | `tracking.rs:195` | THE overlap-sum clipping oracle: spans clip at boundaries, overlaps sum, inverted spans clamp 0, open accrual clamps at `min(window.end, now)` |
| `work_in_window` | `tracking.rs:221` | the Work-filtered delegate — the filter precedent |
| `day_window` / `trailing_days_window` / `window_start` | `days.rs:55/80/18` | local-day window composition over `(day_start_time, offset)`; DST-safe by the fixed-offset rule |
| `calibrate(state, window, now)` | `calibration.rs:122` | the projection precedent: pure `&AppState` + bounds + `now`, documented conventions and limitations at the module head |
| `ReviewWindowModel` / `ReviewRowModel` / `ReviewModel` | `ui/views/review-view.slint:19–49` | the landed skeleton: two `AggregateSection`s + one `ListView` ledger; `StatCell` (`:53`), `AggregateSection` (`:76`); D-9 tiers in the view body (`:134–153`) |
| `review_seconds_phrase` / `review_gap_phrase` / `review_window` | `ui-slint/src/presentation.rs:1918/1930/1952` | second-granular phrases; the three-sign-class structural rendering |
| `push_review(window, state, day, calendar, kiosk, models)` | `ui-slint/src/models.rs:1052` | kiosk belt first (`:1060` early return), then per-pass window derivation + `calibrate`; `REVIEW_WINDOW_DAYS: [u32; 2] = [7, 30]` (`:1136`, D-4) |
| `empty_review_model()` | `models.rs:1153` | the belt's collapsed model — grows tracking fields (D-S6) |
| Review push-level pins | `models.rs:3690/3745/3775/3818` + rig `:3624` | `the_review_model_mirrors_calibrate`, `the_kiosk_belt_emits_the_empty_review_model`, `the_review_empty_state_collapses_in_tiers`, `the_gap_phrases_distinguish_the_three_sign_classes` |
| Review journeys + goldens | `tests/journeys.rs:12030`, `tests/golden_visuals.rs:785/820` | `the_review_surface_ledgers_and_returns`; `the_review_empty_golden_holds` (D-9 tier 1), `the_review_ledger_golden_holds` (D-8) |
| Kiosk rotation pin | `ui-slint/src/view_state.rs:2657` | `the_kiosk_rotation_never_lands_on_review` — untouched by this chunk |
| Chunk-3 territory check | — | landed plan-next-day touched `presentation.rs`/`models.rs`/`view_state.rs`/`days.rs` but **not** `review-view.slint` (last touched by `8571e956`) nor `tracking.rs` (last `200e1f46`); no symbol collisions with this plan's names (checked: `tracking_stats`, `TrackingSection`, `StatBars`, `DailyTotal`, `WorkBlock`, `respite_in_window`, `daily_series` — all free at base) |

## 2. Scope & non-goals

**In scope**: tracked-time aggregates over the landed 7/30-day windows
(Work vs Respite split; per-day totals), Work-block readings
(count/duration, honest semantics), hand-rolled bar strips (R-3) fed
by pure presentation values, the collapse-tier extension, the kiosk
belt extension, CHANGELOG.

**Non-goals** (dispatch §5; violations are stage-3 findings):
- journal/reflection trends, mood/energy/health-flag readings,
  past-entries browsing, the journal export leg — **slice B** (gated
  on #621's chunks; #623 stays OPEN after this chunk, annotated
  slice A);
- chart crates (R-3 — adding one is a finding, not a shortcut);
- new windows beyond the 7/30 skeleton (D-4 constants; a config knob
  is a v2 decision);
- CSV/JSON export additions (the `export` verb already covers raw
  entries — VF-3);
- focus-session vocabulary of any kind (D-S5's limitation is recorded,
  not papered over);
- row activation/context actions anywhere in the new sections (the
  D-8 read-only posture: a ledger, not a task list);
- kiosk rendering of the new sections (D-S8: belt + rotation keep
  holding);
- window labels/tooltips/ticks on the bar strips (v1 shape-only;
  absolute magnitudes live in the stat cells — D-S4).

## 3. The design

### D-S1 — Section set: two tracking sections appended, skeleton untouched

The view grows from "two aggregate sections + one ledger" (D-8) to
**four aggregate sections + one ledger**, top to bottom:

1. `AggregateSection` "Last 7 days" (calibration — landed, untouched)
2. `AggregateSection` "Last 30 days" (calibration — landed, untouched)
3. **`TrackingSection` "Tracking · last 7 days"** — new
4. **`TrackingSection` "Tracking · last 30 days"** — new
5. the ledger heading + `ListView` (landed, untouched)

Each `TrackingSection` renders: caption, three stat cells (Work total,
Respite total, Work blocks), and one bar strip (the per-day series,
D-S4). Rationale: *appending* keeps the landed skeleton's visual
signature intact (interleaving sections by window would reflow the
landed rows and churn the D-8 pins); the per-window pairing survives
because each new section is captioned with its window. Wording
("Tracking · …") is UX copy — tests assert structure, never text
(AGENTS §5).

### D-S2 — Windows: the 7/30 skeleton is reused verbatim

The tracking folds consume **the same** `week_window`/`month_window`
that `push_review` already derives per pass via
`trailing_days_window` (`REVIEW_WINDOW_DAYS`, D-4 constants) — no new
window derivation, no new constants. The per-day series buckets tile
exactly these windows (law L-3): bucket *i* spans
`[window_start(dateᵢ), window_start(dateᵢ₊₁))` over the trailing
local dates, so the 7-day section's strip is the 30-day strip's last
seven buckets — the redundancy is deliberate (the near strip is the
readable shape, the far strip the trend; symmetry keeps the skeleton
stable).

### D-S3 — The projection: `taskboard-domain/src/tracking_stats.rs`

New domain module (the `calibration.rs` precedent: pure `&AppState`- 
adjacent fold, no IO, no clock reads, `now`/bounds as data; module
doc records the conventions and the limitation). Vocabulary:
`tracking_stats` — tracking-side statistics; slice B's reflection
trends get their own module later; nothing shadows the undo journal's
exported names (VF-3).

```rust
/// The Work/Respite split of one window's tracked time.
pub struct WindowSplit { pub work: TimeDelta, pub respite: TimeDelta }

/// Both kinds fold through tracked_in_window — the single clipping
/// oracle (work side: the Work-filtered rows, the work_in_window
/// shape; respite side: the Respite-filtered rows). No third window
/// function joins the public family; the kind filter is local here.
pub fn window_split<'a>(entries: impl IntoIterator<Item = &'a TimeEntry>,
                        window: (DateTime<Utc>, DateTime<Utc>),
                        now: DateTime<Utc>) -> WindowSplit

/// One local day's split (the bucket identity rides along; the push
/// pass consumes only the values — dates never cross into the model).
pub struct DayTotal { pub date: NaiveDate, pub split: WindowSplit }

/// The trailing `days` local-day buckets, oldest first, tiling
/// `trailing_days_window(now, day_start_time, offset, days)` exactly.
/// Per bucket: window_split over that bucket's window — the per-day
/// totals ride the same oracle (the boundary split is performed by
/// tracked_in_window, never re-implemented here).
pub fn daily_split_series(entries, days: NonZeroU32,
                          day_start_time: NaiveTime, offset: FixedOffset,
                          now: DateTime<Utc>) -> Vec<DayTotal>

/// The Work-block reading. Membership: the entry's START instant lies
/// in the window (`start ∈ [window.start, window.end)`) — the
/// calibration precedent (membership by date, work never clipped);
/// durations are FULL (`TimeEntry::duration(now)`: open entries
/// accrue to `now`, inverted spans clamp 0), so count and duration
/// stay coherent (count × mean = total). A block that started before
/// the window is uncounted here while its in-window time still shows
/// in `window_split` — two stated semantics, both pinned by tests.
/// Work-kind entries only (see D-S5).
pub struct WorkBlockReading { pub blocks: usize, pub total: TimeDelta }
pub fn work_blocks<'a>(entries: impl IntoIterator<Item = &'a TimeEntry>,
                       window: (DateTime<Utc>, DateTime<Utc>),
                       now: DateTime<Utc>) -> WorkBlockReading
```

Cost note (conscious acceptance): the per-pass fan-out is
`(7 + 30) × 2` kind-folds + 2 block folds + 2 window splits ≈ 80
`tracked_in_window` scans × O(entries). The review surface is a
refresh-pass projection, not a ticking one (D-10(c) posture), and on
kiosk the folds sit **after** the belt's early return — zero appliance
cost. Modest by the kiosk budget; re-derive only if a profiling pass
ever says otherwise.

### D-S4 — Bar composition: ONE reusable `StatBars` strip widget (decided once)

**Decision**: a single minimal widget, `ui/widgets/stat-bars.slint` —
not per-section inline shapes. Cited reuse vocabulary: the theme's
`Theme`/`RoleText`/`Semantic` tokens and the widgets-directory pattern
(`empty-state.slint`, `status-dot.slint` — small single-purpose
imports); `review-view.slint` keeps "composition only" (its own
header rule) instead of hosting 37 bar `Rectangle`s twice. The named
second consumer is slice B (mood/energy series over the same windows)
— the widget takes its segment colors as `in` properties (defaulting
to theme tokens) so slice B re-instances it rather than forking it.
No speculative generality beyond that: no labels, ticks, tooltips, or
value overlays (v1; the absolute magnitudes live in the section's
stat cells).

Shape: a horizontal strip of equal-stretch bars, one per day; each
bar is two stacked segments — **Work bottom, Respite top** — heights
set from pushed fractions (geometry only: `fraction × strip height`).
A day with no tracked time renders a 1px baseline tick (gaps read as
"days with nothing", never as missing bars — the strip's bar count is
the window's day count, structurally stable under D-9 tier 2).

**Values, not pixels**: the push boundary carries `f32` fractions,
normalized presentation-side (pure fn) — slint computes nothing but
geometry (VF-8). Per day: `work = work_seconds / strip_max`,
`respite = respite_seconds / strip_max`, where `strip_max` is the max
day total (`work + respite`) across the strip; all-zero strip → all
zeros (never NaN); fractions clamp to ≤ 1.0. The strip shows *shape*;
the record of "fractions hide absolute magnitude" lives at the widget
doc and the phrase fn — the honest minimal-chart posture (R-3: no
axes, no gridlines, no scale chrome).

```slint
export struct StatBarModel { work: float, respite: float }
export struct TrackingSectionModel {
    work: string,       // collapsed (section empty) → "" — the dash renders structurally
    respite: string,
    blocks: string,
    bars: [StatBarModel],   // length == the window's day count, always (7 / 30)
    empty: bool,            // no tracked time of either kind in the window
}
// ReviewModel gains: tracking-week / tracking-month (Rust: tracking_week / tracking_month)
```

Colors: existing `Theme`/`Semantic` tokens only (stage 2 picks the
work/respite pair from what `theme.slint` exposes; no new palette
entries). Contrast between the two segments must be token-native —
if the theme cannot answer, that is a stage-2 deviation to report,
not a new color to invent.

### D-S5 — Work blocks, not focus sessions: the naming + the recorded limitation

**The limitation**: time entries carry no focus-mode marker. The
`[focus]` runtime is stateless (D-F1) and stamps nothing into
`TimeEntry`; a tracked hour under focus mode and a plain tracked hour
are indistinguishable in history. Therefore:

- the vocabulary is **"Work blocks"** everywhere — domain fn
  (`work_blocks`, `WorkBlockReading`), model field (`blocks`),
  presentation glue, view caption cell, CHANGELOG; never "focus
  sessions", never bare "sessions";
- the limitation is recorded at the projection: `work_blocks`' doc
  states that blocks are `TimeEntryKind::Work`-marked entries, not
  focus-mode spans, and that focus-session statistics would need an
  entry-level marker (a future entity change, out of scope);
- the pin test carries the limitation in its name:
  `work_blocks_are_kind_marked_entries_not_focus_sessions` (asserts
  Respite entries never count as blocks).

This is the impl-plan §5.3 ruling verbatim ("the projection names
Work blocks, and the limitation is recorded, not papered over").

### D-S6 — Model + push: the derivation rides the existing pass

`push_review` extends in place (no second pass):

1. the kiosk belt returns early **before any fold** (as today) —
   `empty_review_model()` grows the two collapsed tracking sections
   (bars: full-length zero series? no — the belt emits
   `bars: []`-empty model: the appliance never projects the surface,
   and the belt model is never rendered because the rotation never
   routes there; the belt test asserts the collapsed fields);
2. the desktop path derives, per window: `window_split` +
   `work_blocks` over the already-computed `week_window`/`month_window`,
   and `daily_split_series` for the 7- and 30-day strips;
3. presentation glue (`D-S7`) turns the projections into phrases and
   bar fractions; the section `empty` flag is
   `split.work + split.respite == TimeDelta::zero()`;
4. rows/push mechanics untouched (`push_review_model` + the
   `MirroredRows` diff handle the new arrays the same way — the bars
   ride a plain `ModelRc` set with the whole struct each pass; they
   are small, fixed-length, and never need row-diffing).

Zero-count vs collapsed: a non-empty section renders real phrases
even when a count is zero ("0 work blocks" is an observed fact);
the dash is reserved for the collapsed (empty) section — the landed
"unestimated is the dash, never a zero phrase" distinction, kept.

### D-S7 — Presentation glue (pure, fixed-stamp tested)

```rust
/// Phrases for one tracking section: Work total, Respite total, and
/// the Work-block reading (count + duration in one phrase), through
/// review_seconds_phrase (the second-granular precision contract).
/// empty collapses all three to "" (the view renders dashes).
pub struct TrackingSectionPhrases { pub work: String, pub respite: String,
                                    pub blocks: String, pub empty: bool }
pub fn review_tracking_section(split: &WindowSplit,
                               blocks: &WorkBlockReading) -> TrackingSectionPhrases;

/// The strip's bar fractions (D-S4's normalization; len preserved;
/// all-zero → all zeros; each fraction ≤ 1.0, never NaN).
pub fn review_bar_fractions(series: &[DayTotal]) -> Vec<StatBarModel>;
```

### D-S8 — Kiosk unchanged, three layers keep holding (D-10 posture)

- **(a) Structural**: `KioskMode` maps Board/Today only; the rotation
  pin (`view_state.rs:2657`) is untouched and must stay green. No
  routing, no `View` enum change (Review already exists in it).
- **(b) Push belt**: `push_review`'s kiosk early return now also
  carries the collapsed tracking sections (they sit inside
  `empty_review_model()`); the belt test extends (M-4 below). A
  future routing bug still cannot project tracking data onto the
  appliance.
- **(c) No ticking surface**: everything derives per refresh pass
  from stamped data; `compute_seconds_surfaces_active` gains no term;
  an open entry's accrual updates on the next refresh (the landed
  frozen-when-closed class).

### D-S9 — Collapse tiers (D-9 extension), per new section

1. **Whole view (tier 1, extended)**: the view collapses to
   `EmptyState` when the 30-day calibration reading is empty **and**
   the 30-day tracking split is zero — i.e.
   `empty = month_rows.is_empty() && tracking_month.empty`. This is a
   deliberate semantic extension: a state with tracked time but no
   comparable completed tasks now keeps the view standing (collapsed
   calibration sections + populated tracking sections) instead of
   folding to nothing. Pinned (M-3).
2. **Per-section (tier 2)**: a tracking section whose window holds no
   entries renders dashes for all three cells and a full-length
   all-zero strip (baseline ticks only) — the skeleton (caption +
   cells + bar count) never vanishes, values collapse. Pinned (M-2).
3. **Ledger (tier 3)**: untouched.

The landed tier tests (`the_review_empty_state_collapses_in_tiers`)
must keep passing **unchanged** — their fixtures (no entries) are
unaffected by the tier-1 extension.

### D-S10 — Read-only posture holds

The new sections add no activation, no context actions, no focus
containers, no `ActionId` — page-level scroll only (the D-8 rule;
`ScrollPageHint` family 4 unchanged). No keymap/preset snapshot
regens (verify: zero `ActionId` growth). The review journey's
"dispatches nothing" assertion keeps holding.

### D-S11 — CHANGELOG

One `### Added` entry under `[Unreleased]` for #623 **slice A**
(say so; the issue stays open for slice B): user-visible behavior —
tracked-time statistics in the Review view: Work vs Respite totals,
per-day bar strips, and Work-block readings over the 7/30-day
windows. No implementation narration, no "focus" wording (D-S5).

### D-S12 — Out of scope, named so nobody re-derives it

- entry-level focus-mode markers (would enable real focus-session
  statistics) — an entity change, fenced; if ever wanted it goes
  through a design round;
- per-strip day labels/tooltips, axes/scale chrome — v2 chart polish;
- a CLI `review`/`stats` verb — the domain module makes it cheap
  later (the calibration D-12 precedent);
- daily-average/derived cells beyond the three named — scope
  discipline;
- window configuration — D-4 constants stand;
- the today view's totals — #490 territory.

## 4. TDD catalogue — stage 2's commits, in order

Red-first lives in the pure layers (domain, presentation) where tests
can compile against absent functions only via the test-first commit
pattern the house uses (`test(...)` red → `feat(...)` green). The
Slint scaffolding (structs + widget + view composition) cannot host a
compiling red test before it exists — the landed round-4/chunk-3
shape applies: the pure layers carry the red-first discipline, the
UI wiring commit carries its push-level pins and the golden regen as
data in the same visual unit (mirroring landed `528a6818` +
`8600407f`). Every commit standalone (rebase-merge hygiene).

### Commit 1–2 — the domain projection (laws first)

`test(domain): the tracking statistics projection's laws` (red) →
`feat(domain): the tracking statistics projection`. New
`tracking_stats.rs` + `lib.rs` re-export. Fixed-stamp units + props
(`proptest` over the `test_support` strategies; every law names its
failure symptom):

- **L-1** `window_split_tiles_the_tracked_total_prop` — `work +
  respite == tracked_in_window(all rows)` for random
  entries/windows/now (the exhaustive two-arm kind split; symptom: a
  kind arm double-counted or dropped).
- **L-2** `window_split_work_side_matches_work_in_window_prop` — the
  work side equals the landed `work_in_window` (symptom: a second
  clipping implementation).
- **L-3** `daily_series_tiles_the_trailing_window` — fixed stamps:
  first bucket start == trailing start, last bucket end == trailing
  end, consecutive adjacency, `len == days` (symptom: bucket edges
  computed by `now − n·24h` instead of `window_start` composition).
- **L-4** `daily_series_buckets_come_from_the_single_oracle_prop` —
  each bucket equals `window_split` over that bucket's window
  (oracle re-derivation, the tracked_in_window house pattern;
  symptom: per-bucket math drifting from the oracle).
- **L-5** `a_span_crossing_the_day_boundary_counts_in_both_buckets` —
  fixed stamps: 23:00→01:00 entry splits 1h/1h across the midnight
  bucket edge (the headline boundary-split test).
- **L-6** `the_day_start_time_moves_the_bucket_edges` — a `05:00`
  day start puts a 03:00 stamp into the previous local day's bucket
  (the night-owl rule riding `window_start`).
- **L-7** `work_blocks_select_by_start_and_report_full_durations` —
  a block started before the window is uncounted while its in-window
  time still shows in `window_split` (both semantics pinned side by
  side); an open block accrues to `now`.
- **L-8** `work_blocks_are_kind_marked_entries_not_focus_sessions` —
  Respite entries never count as blocks (the limitation's name-bearing
  pin, D-S5).
- **L-9** `empty_entries_yield_zero_splits_and_no_blocks` — the total
  floor.
- **L-10** `open_accrual_fills_the_running_day_up_to_now` — an open
  entry's running-day bucket clamps at `now` (never into the future).

### Commit 3–4 — the presentation glue

`test(ui-slint): the review tracking phrases and bar fractions` (red)
→ `feat(ui-slint): the review tracking phrase and fraction glue`.

- **P-1** `review_tracking_section_phrases_and_collapse_flag` —
  empty → all phrases empty + flag set; non-empty → phrases
  non-empty, distinct inputs give distinct forms (structural
  `assert_ne` on phrases for different durations, never text
  matching).
- **P-2** `review_bar_fractions_normalize_to_the_strip_maximum` —
  the max day's segment sum ≈ 1.0 (float tolerance), other days
  scaled, all-zero strip → all zeros (no NaN), lengths preserved,
  no fraction above 1.0.

### Commit 5 — the UI unit (skeleton + widget + derivation together)

`feat(ui-slint): the stat-bars strip and the review tracking sections`.
`StatBarModel`/`TrackingSectionModel` structs; `stat-bars.slint`
(D-S4); `TrackingSection` component + the two appends in
`review-view.slint` (D-S1); `ReviewModel` gains the fields;
`push_review` derives (D-S6); `empty_review_model()` grows the
collapsed sections (the belt, D-S8(b)); goldens regen **as data**
where pixels change: `the_review_ledger_golden_holds` regenerates
(the populated golden gains the sections);
`the_review_empty_golden_holds` must NOT change (tier 1 renders the
`EmptyState` only — if it diffs, the tier-1 wiring is wrong: stop and
fix, never regen). Existing push-level pins stay green untouched
(their fixtures carry entries via `add_completed_task`, which plants
`Work` entries — verify `the_review_model_mirrors_calibrate` still
holds; its assertions touch only calibration fields, so it must).

### Commit 6 — the push-level pins

`test(ui-slint): the tracking sections' push-level tiers`
(`on_slint_thread` + `review_rig`):

- **M-1** `the_tracking_sections_mirror_the_domain_projection` —
  planted entries → work/respite phrases non-empty, `bars` lengths ==
  7 and 30, the running day's fractions strictly positive.
- **M-2** `the_tracking_sections_collapse_in_tiers` — no entries →
  section `empty` flags set, empty phrases, full-length all-zero
  bars; and the whole-view tier still collapses on the bare fixture.
- **M-3** `tracked_time_alone_keeps_the_review_view_standing` —
  entries, no comparable completed tasks → `model.empty == false`,
  calibration sections' `empty` flags set, tracking sections
  populated (the extended tier-1 pin, D-S9).
- **M-4** `the_kiosk_belt_extends_to_the_tracking_sections` —
  `kiosk: true` → tracking sections collapsed, bars empty, ledger
  rows 0 (the belt extension; complements the landed
  `the_kiosk_belt_emits_the_empty_review_model`).

### Commit 7 — journey + changelog

`test(ui-slint): the review journey reads the tracking sections` —
extend `the_review_surface_ledgers_and_returns` (journeys.rs:12030)
minimally: the tracking model fields assert structurally (bars
lengths, non-empty work phrase) alongside the landed ledger
assertions; the read-only contract assertion (`assert_commands(&[])`)
stays. Then `docs(changelog): the review tracking statistics entry`
(D-S11; may fold into this commit pair's tail per house habit).

Stage 3 afterwards: rebase, full gate suite through
`scripts/run-quality-gates.sh`, push, PR titled
`feat(ui-slint): the review view's tracking statistics` with body
"Partially addresses #623" (slice A — the issue stays open), checks
green, mergeStateStatus `CLEAN`, worktree removed.

## 5. Risks

- **Tier-1 semantics extension** (D-S9): states that previously
  collapsed (tracked time but nothing comparable) now keep the view
  standing. Deliberate and pinned (M-3); the landed tier tests are
  fixture-unaffected. The empty golden must not diff — that is the
  canary for getting tier 1 wrong.
- **Fractions hide absolute magnitude** (D-S4): the strip is
  shape-only; mitigated by the stat cells carrying the magnitudes and
  the widget doc recording it.
- **Fold fan-out** (D-S3): ≈ 80 `tracked_in_window` scans per refresh
  pass, O(days × entries). Refresh-pass cadence, not ticking; zero on
  kiosk (behind the belt). Accepted consciously; revisit only on
  profiling evidence.
- **Golden churn**: only the populated golden changes; regen is
  data-only and reviewed (the goldens' own header rule). Two goldens
  exist today; the empty one is a no-diff canary.
- **f32 across the boundary**: Slint-native float; precision is
  irrelevant at bar-height granularity (P-2 asserts with tolerance,
  exact-zero only for the empty case).
- **Theme token contrast** (work vs respite segments): if the theme
  cannot answer with existing tokens, stage 2 reports the deviation
  instead of inventing a color.
- **Territory**: `presentation.rs`/`models.rs` are shared with landed
  chunk 3, but this branch already sits on post-chunk-3 `main` — no
  expected conflict; stage 3 still rebases per protocol.

## 6. References

- Dispatch plan §5 chunk 4 (acceptance + stage-3 traps):
  `.artifacts/plans/2026-11-04-v0.2-focus-reflection-dispatch-plan.md`
- Implementation plan §2 R-3, §4 VF-1/VF-4/VF-8/VF-10, §5.3 slice A,
  §6 (chunk table), §8 (the skeleton-drift risk this plan's pins
  answer):
  `.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md`
- Issue #623 (+ the 2026-11-04 hand-rolled-charts ruling comment)
- Effort-calibration round D-4/D-8/D-9/D-10/D-12 (the skeleton,
  collapse tiers, kiosk layers, out-of-scope fences this chunk grows
  within):
  `.artifacts/plans/2026-10-31-effort-calibration-surfaces-plan.md`
- Seams: `crates/taskboard-domain/src/{tracking.rs,days.rs,calibration.rs}`;
  `crates/taskboard-ui-slint/ui/views/review-view.slint`;
  `crates/taskboard-ui-slint/src/{presentation.rs,models.rs,view_state.rs}`;
  `crates/taskboard-ui-slint/tests/{journeys.rs,golden_visuals.rs}`
  (+ `support/fixture.rs::add_completed_task`)

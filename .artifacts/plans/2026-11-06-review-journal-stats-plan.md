# [Plan] Review-view reflection trends + past-entries (#623 slice B)

- **Date**: 2026-11-06
- **Kind**: stage-1 TDD implementation plan — chunk 4 of the v0.2
  completion dispatch (`feat/review-journal-stats`), written at base
  `44155642` (post-chunk-3 `origin/main`; chunks 1–3 all landed —
  reflection domain #636, storage/CLI #637, review walk #638/#621).
  No code in this commit.
- **Origin** (normative): completion implementation plan §4.4
  (`.artifacts/plans/2026-11-05-v0.2-completion-implementation-plan.md`);
  issue #623 + the 2026-11-04 session ruling (hand-rolled minimal
  charts, no chart crate — the projection stays pure values, rendering
  owns the shape); the dispatch plan §5 chunk-4 covers/acceptance +
  stage-3 traps; **slice A's landed plan** D-S1…D-S12
  (`.artifacts/plans/2026-11-05-review-tracking-stats-plan.md` — the
  skeleton discipline this chunk extends); DR-7 round doc §3-B (the
  `ReflectionEntry`/`ReflectionAttributes` shapes aggregated here);
  the effort-calibration round's D-8/D-9 tier rulings.
- **Status**: READY for stage 2. Every open question carries a
  recorded decision (D-B1…D-B11; the D-B namespace avoids colliding
  with the landed D-S* of slice A and D-J* of DR-7). Deviations from a
  ruling go back to the user, never silently adapted.
- **Acceptance** (dispatch §5): trend derivations pure with
  fixed-stamp tests over window edges; tier pins structurally extended
  (values collapse, skeleton stable); bars compose from pushed values
  only; the ledger pages as family 4 (the slice-A pattern); empty-state
  collapse; CHANGELOG entry; the PR annotates-closes **#623**.

## 1. Verified anchors (base `44155642`)

Symbols are stable keys; line numbers are plan-day references.

| Anchor | Where | Notes |
|---|---|---|
| `ReflectionEntry` | `taskboard-domain/src/reflection.rs:290` | tombstone-in-place row: stored `window_start`/`window_end` (never derived at read), `day` label, `text`, `attributes`, `created_at`/`updated_at`, `deleted`; `is_live()` |
| `ReflectionAttributes` | `reflection.rs:225` | `mood`/`energy: Option<Rating>`, `flags: BTreeSet<HealthFlag>`, `extra_flags: BTreeSet<String>` (tolerant parking, never merged back) |
| `Rating` | `reflection.rs:82` | A1 newtype, `1..=5`, `try_new` mint + `get()`; strict serde decode |
| `HealthFlag` | `reflection.rs:130` | curated nine (kebab serde, declaration order = `Ord`); unknown names park in `extra_flags` verbatim |
| `ReviewCadence` | `reflection.rs:34` | Day/Week/Month — multiple entries per window legal (D-J3) |
| `ReflectionState` | `reflection.rs:335` | `BTreeMap<ReflectionId, ReflectionEntry>`; `live_entries()` = the one live-read seam (tombstones filtered at projection sites) |
| Landed chunk-1 projections | `reflection.rs:535–625` | `review_window` (the flooring law), `window_misses`/`window_done` (the ids-only pure shape) — the pure-projection precedents |
| `tracking_stats` module | `taskboard-domain/src/tracking_stats.rs` | slice A's projection: `window_split`, `DayTotal`, `daily_split_series`, `work_blocks` — the module-per-statistics-family precedent; slice B gets its own module |
| `StatBars` / `StatBarModel` | `ui/widgets/stat-bars.slint` | slice A's strip; colours are `in` properties **already ruled as slice B's re-instance seam** ("the named second consumer is slice B") |
| `TrackingSectionModel` / `ReviewModel` | `ui/views/review-view.slint:40–70` | the appended-section shape; `ReviewModel.empty` = the tier-1 predicate |
| `review_tracking_section` / `review_bar_fractions` / `ReviewBar` | `ui-slint/src/presentation.rs:1990/2028/2021` | the phrase + fraction glue precedent (pure, fixed-stamp tested) |
| `push_review` / `empty_review_model` / `REVIEW_WINDOW_DAYS` | `ui-slint/src/models.rs:1096/1234/1217` | the per-pass derivation: kiosk belt first, then `trailing_days_window` per window, folds, whole-struct model set |
| Review push-level pins | `models.rs:3690+` | `the_review_model_mirrors_calibrate`, `the_kiosk_belt_emits_the_empty_review_model`, `the_review_empty_state_collapses_in_tiers`, tracking-section fixtures |
| Walk territory (chunk 3, landed) | `presentation.rs`, `models.rs`, `view_state.rs` | the walk overlay/today-card own their own fns; no `review-view.slint` collisions — chunk 3 did not touch this view (checked at base) |
| Collision check | — | `reflection_stats`, `ReflectionSection`, `ReflectionSectionModel`, `RatingBarModel`, `window_ratings`, `daily_rating_series`, `flag_occurrences`, `review_reflection_section`, `review_rating_fractions`, `ReflectionRowModel`, `reflection_week`/`reflection_month`, `RatingMean`, `FlagOccurrences` — all free at base (exact-word grep) |

## 2. Scope & non-goals

**In scope**: reflection-attribute trend readings over the landed
7/30-day windows (mood/energy means, health-flag occurrence counts),
per-day mood/energy strips, the past-entries ledger (newest-first over
all live stored entries), the tier extensions, the kiosk belt
extension, CHANGELOG.

**Non-goals** (dispatch §5 + the fence; violations are stage-3
findings):
- **trend mining / pattern inference of any kind** — counts, means,
  and phrases only; no correlation, no advice, no "flag × load
  cross-reading" beyond juxtaposition (the X9 guardrail; #623's
  cross-reading wording is fenced to a future ruling);
- chart crates (R-3; the 2026-11-04 ruling comment);
- new windows beyond the 7/30 skeleton, configurable windows,
  weekly/monthly pivots (#640);
- journal export additions (the `export` verb already covers raw
  entries — VF-3; dispatch lists the export leg as slice B's
  **non-goal** by the landed CLI);
- any write path, activation, or context action in the new sections
  (the D-8 read-only posture: past entries are a ledger, not an
  editor — editing rides the walk/CLI verbs, not this view);
- kiosk rendering of the new sections (belt + rotation keep holding);
- the walk's surfaces (chunk 3 landed them; this chunk reads stored
  entries, it does not touch the overlay, today card, or stage rows).

## 3. The design

### D-B1 — Section set: two reflection sections + one entries ledger, appended; skeleton untouched

The view grows from "two calibration + two tracking sections + one
ledger" to **four aggregate sections + two ledgers**, top to bottom:

1. `AggregateSection` "Last 7 days" (calibration — untouched)
2. `AggregateSection` "Last 30 days" (calibration — untouched)
3. `TrackingSection` "Tracking · last 7 days" (slice A — untouched)
4. `TrackingSection` "Tracking · last 30 days" (slice A — untouched)
5. **`ReflectionSection` "Reflection · last 7 days"** — new
6. **`ReflectionSection` "Reflection · last 30 days"** — new
7. **the "Journal entries" ledger** — new (`ReflectionRowModel` rows)
8. the "Completed tasks" ledger (landed, untouched)

Each `ReflectionSection` renders: caption, three stat cells (Mood
mean, Energy mean, Flag occurrences), the per-flag counts line
(D-B4), and one mood/energy strip (D-B6). Appending keeps slice A's
visual signature intact (D-S1's rationale verbatim); the per-window
pairing survives via the captioned window. Wording is UX copy — tests
assert structure, never text (AGENTS §5).

### D-B2 — Windows: the 7/30 skeleton reused verbatim

The reflection folds consume the same `week_window`/`month_window`
`push_review` already derives (`trailing_days_window` over
`REVIEW_WINDOW_DAYS`, D-S2/D-4 verbatim) — no new derivation, no new
constants. The per-day rating buckets tile exactly these windows
(slice A's law L-3): bucket *i* spans
`[window_start(dateᵢ), window_start(dateᵢ₊₁))`, so the 7-day strip is
the 30-day strip's last seven buckets.

**Membership semantics (recorded at the projection)**: an entry joins
the bucket/trend window whose half-open range contains the entry's
**stored `window_start` instant** — stored, never derived at read
(D-J2). The limitation recorded, not papered over: a Week/Month
entry's stored `window_start` is its period floor, so it stamps at the
*start* of its (possibly long) reviewed window, not at the entry's
journaling instant — the honest semantics (D-J3's no-origin-marker
limitation surfaces here; the pin test carries it in its name).
`created_at` is deliberately **not** the membership key: it is a
device-side write stamp, meaningless across future roaming.

### D-B3 — The projection: `taskboard-domain/src/reflection_stats.rs`

New domain module (the `tracking_stats` precedent: pure, no IO, no
clock reads — `now`/bounds as data; module doc records the membership
rule and the period-floor limitation). Vocabulary: `reflection_stats`
— nothing shadows the undo journal's exported names (VF-3).

```rust
/// Mean of one rating field over a window: sum/count as integers
/// (the fixed-stamp oracle — no float in the domain layer; the
/// presentation phrase renders `total.count == 0 → None`).
pub struct RatingMean { pub total: u32, pub count: usize }
impl RatingMean { pub fn mean(&self) -> Option<f32>; }  // presentation-side render aid

/// One window's attribute fold over LIVE entries only (tombstones
/// filtered here, the `live_entries` seam — the caller may pass any
/// iterator; the fold re-checks `is_live()`).
/// Membership: stored `window_start ∈ [start, end)` (D-B2).
pub struct WindowRatings {
    pub mood: RatingMean,
    pub energy: RatingMean,
    /// Occurrences: an entry carrying k flags contributes k.
    /// Curated flags keyed by variant; `extra_flags` parked names
    /// counted verbatim into `extra` (never merged into the curated
    /// map — the decode-side tolerance rule holds at read too).
    pub flags: FlagOccurrences,
}
pub struct FlagOccurrences {
    pub curated: BTreeMap<HealthFlag, usize>,
    pub extra: BTreeMap<String, usize>,
}
pub fn window_ratings<'a>(entries: impl IntoIterator<Item = &'a ReflectionEntry>,
                          window: (DateTime<Utc>, DateTime<Utc>)) -> WindowRatings;

/// The trailing `days` local-day buckets, oldest first, tiling
/// `trailing_days_window(now, day_start_time, offset, days)` exactly
/// (the `daily_split_series` shape verbatim). Per bucket:
/// `window_ratings` over that bucket's window — the boundary split is
/// performed by the same membership rule, never re-implemented.
pub struct DayRatings { pub date: NaiveDate, pub ratings: WindowRatings }
pub fn daily_rating_series(entries, days: NonZeroU32,
                           day_start_time: NaiveTime, offset: FixedOffset,
                           now: DateTime<Utc>) -> Vec<DayRatings>;

/// The past-entries reading: every LIVE entry, newest first —
/// ordered by `(window_start desc, id desc)` (the deterministic
/// BTreeMap-order companion; descending so the newest journal line is
/// the first row). No window, no cap: the ledger virtualizes (the
/// ListView pages as family 4), the push cost is O(n) per refresh
/// pass — fine at journal scale; re-derive only if profiling says so.
pub fn past_entries<'a>(state: &'a ReflectionState) -> Vec<&'a ReflectionEntry>;
```

Cost note (the slice-A posture verbatim): the per-pass fan-out adds
`(7 + 30)` rating folds + 1 ordering pass ≈ 37 scans × O(entries),
**after** the belt's early return — zero appliance cost.

### D-B4 — Phrases: means, flag counts — counts and phrases only

```rust
/// Mood/Energy cells: `mean()` rendered as one fixed-decimal phrase
/// with the scale ("3.5 / 5" shape — the exact vocabulary is stage-2
/// copy; tests assert structure). No rated entries in the window →
/// "" — unanswered is the dash, never a zero (the landed
/// unestimated-is-the-dash distinction). Flags cell: total
/// occurrences across the window's entries ("N" — an observed fact,
/// rendered even at 0 once any entry exists).
pub struct ReflectionSectionPhrases { pub mood: String, pub energy: String,
                                       pub flags: String, pub per_flag: String,
                                       pub empty: bool }
pub fn review_reflection_section(ratings: &WindowRatings) -> ReflectionSectionPhrases;
```

`per_flag` is the counts-only line: `curated ∪ extra` occurrences,
count descending then name ascending (the deterministic double sort;
curated names by their kebab form, `extra` verbatim), rendered as
joined `name ×count` captions; empty when no flags occurred — a
missing line is the honest "no flags", not a collapsed section (the
section's `empty` flag answers only to zero LIVE entries, D-B7).

### D-B5 — Past-entries ledger shape: one `ReflectionRowModel` row per live entry

```slint
export struct ReflectionRowModel {
    day: string,        // the stored `day` label, as stored
    cadence: string,    // the walk vocabulary's cadence phrase (structure-only)
    text: string,       // the free-text body (elided, one line)
    mood: string,       // "" = unrated (the dash renders structurally)
    energy: string,
    flags: string,      // joined kebab names, declaration order; "" = none
}
// ReviewModel gains: reflection-week / reflection-month
// (ReflectionSectionModel) + reflection-rows (Rust: reflection_rows)
```

Row rendering follows the landed ledger row exactly (caption columns
+ elided title-class text, explicit height rule, no activation, no
walk targets — read-only). The text body renders **verbatim**; no
excerpting/truncation logic beyond the elide (no inference). The
ledger is NOT windowed: entries older than 30 days still appear —
"one place to look back" is the issue's own framing, and hiding old
entries would silently contradict the tombstone-in-place contract's
"never silent absence" reading.

**Two ledgers, one pattern**: the new ledger replicates the landed
`ledger-clip`/`ListView` pattern inline (family 4 paging, the
`#373` viewport bound, the explicit row height). A shared component
is deliberately NOT extracted: the row bodies differ per ledger, the
pattern is ~20 lines, and extraction churns the landed D-8 pins for
zero reuse (the slice-A posture: composition stays in the view).

### D-B6 — Bar composition: `StatBars` re-instanced, `StatBarModel` renamed — decided once

The slice-A widget doc already names slice B as the second consumer
via the colour `in` properties: **`StatBars` is re-instanced, not
forked, and no new chart widget exists** — no sparkline (bars are the
ruled v1 shape; a sparkline would be a second widget with one
consumer, deferred until a second consumer exists).

One vocabulary fix rides here, decided once: `StatBarModel`'s fields
`work`/`respite` are tracking-side words; the reflection strip would
smear them. The struct is renamed to neutral strip vocabulary —
`base: float` (lower segment) / `stack: float` (upper segment) — with
the widget's segment colours already parameterized. Compile-guided
churn: slice A's push mapping (`models.rs:1195–1199`), the widget
body, and the empty-day predicate (`base + stack == 0`). Mapping:
mood → `base`, energy → `stack`.

Fractions: **fixed-scale normalization** — `mean / 5.0` per rated
bucket, no strip-max rescaling (ratings own a natural 1..=5 scale;
normalizing by strip max would hide absolute level, which for ratings
IS the reading). Days with no rated entries are baseline ticks. All
the R-3 posture holds: no axes/ticks/tooltips; the magnitudes live in
the section's stat cells.

```rust
/// Per-day mood/energy fractions of the 5-point scale (fixed-scale
/// normalization, not strip-max: ratings have a natural scale).
/// Unrated bucket → 0.0/0.0 (baseline tick); every fraction finite,
/// ≤ 1.0, never NaN; length == series len, always.
pub fn review_rating_fractions(series: &[DayRatings]) -> Vec<ReviewBar>;
```

### D-B7 — Model + push: the derivation rides the existing pass

`push_review` extends in place (the D-S6 recipe verbatim):

1. the kiosk belt returns early **before any fold**; `empty_review_model()`
   grows the two collapsed reflection sections (`per_flag`/`bars`
   empty, `empty: true`) and the empty entries ledger — the belt test
   extends (M-4);
2. the desktop path derives, per window: `window_ratings` over the
   already-computed `week_window`/`month_window` +
   `daily_rating_series` for the strips; `past_entries` once for the
   ledger;
3. presentation glue (D-B4/D-B6) turns the folds into phrases and
   fractions; section `empty` = zero live entries in the window;
4. **the ledger rows ride a second `MirroredRows<ReflectionRowModel>`
   handle** (`PushModels.review_entries`) — the entries set is
   unbounded and mutation-prone (journal verbs run mid-session), so
   it rows-diffs like the calibration ledger instead of whole-struct
   sets; `push_review_model` gains the second rows parameter (or a
   sibling fn — stage-2's choice, the diff semantics are the pin).

Zero-count vs collapsed: flag occurrences render "0"-class phrases
once any entry exists (observed fact); rating cells dash when
unrated; the dash at section level is reserved for the zero-entry
window.

### D-B8 — Kiosk unchanged, three layers keep holding (D-10 posture)

(a) Structural: no routing/`View` change; the rotation pin
(`view_state.rs:2657`) stays green untouched. (b) The belt carries
the collapsed reflection sections + empty ledger inside
`empty_review_model()`. (c) No ticking surface: `compute_seconds_surfaces_active`
gains no term; everything derives per refresh pass from stamped data.

### D-B9 — Collapse tiers (D-9 extension), per new section

1. **Whole view (tier 1, extended again)**:
   `empty = month_rows.is_empty() && tracking_month_empty &&
   reflection_month_empty && entries_ledger_empty` — any content
   anywhere keeps the view standing (a journals-only state now gets
   its surface; the D-S9 deliberate-extension rationale verbatim).
   Pinned (M-3).
2. **Per-section (tier 2)**: a reflection section whose window holds
   no live entries renders dashes in all three cells, no per-flag
   line, and a full-length all-zero strip (baseline ticks) — the
   skeleton (caption + cells + bar count) never vanishes. The
   entries ledger with zero live entries: the heading stays, the list
   is empty (the ledger IS the skeleton; there are no values to
   collapse). Pinned (M-2).
3. **Ledger rows (tier 3)**: the calibration ledger untouched.

The landed tier tests keep passing **unchanged** (their fixtures
carry no reflection entries; the tier-1 predicate only ever widens
standing, never collapses).

### D-B10 — Read-only posture holds

No activation, no context actions, no walk targets anywhere in the
new sections (dispatch trap (e): the ledger pages as family 4 with
the viewport pattern intact). The past entries' *editing* stays with
the walk/CLI verbs; this view renders stored rows verbatim.

### D-B11 — Naming & the journal collision fence

The chunk's vocabulary is `reflection_*` / `Reflection*` exclusively —
never "journal stats", never `journal_*` symbols (the undo journal
owns that namespace, VF-3). Display copy may say "Journal entries"
for the ledger heading (the user-facing word for stored reflection
entries per DR-7's own prose); symbol-level tests stay structural.
`extra_flags` are counted and shown verbatim, never merged into the
curated map (D-J3's rule holds at read).

## 4. Test catalogue (TDD; the failing tests come first)

### Domain — `reflection_stats` (unit, fixed-stamp; fixtures anchor `window_start` inside named buckets)

1. `window_edges_attribute_to_the_right_bucket` — an entry whose
   `window_start == window.end` falls in the NEXT window; `== start`
   falls in this one (the half-open law; dispatch trap (b)).
2. `tombstoned_entries_never_enter_the_folds` — deleted rows with
   in-window stamps contribute nothing to means/flags/orderings.
3. `multiple_entries_per_window_all_count` — two Day entries same
   window: means fold over both, flags sum both (D-J3's legality).
4. `rating_means_are_exact_integer_folds` — sum/count over mixed
   `Some`/`None`; all-unrated → `mean() == None`.
5. `period_floor_entries_stamp_at_their_window_start` — a Month
   entry whose stored `window_start` predates its `day` label joins
   the bucket containing the stored instant, not the label (D-B2's
   recorded limitation, pinned by name).
6. `flag_occurrences_count_curated_and_extra_separately` — k flags on
   one entry = k occurrences; unknown parked names land in `extra`,
   never in `curated`.
7. `daily_series_buckets_tile_the_window_exactly` — 7 buckets span
   `trailing_days_window(…, 7)`'s bounds; bucket 0 is the oldest;
   the fixed-stamp fixture pins one entry migrating buckets when
   `now` advances a day (advance the STAMP, never a clock).
8. `past_entries_orders_newest_first_by_window_start_then_id_desc`
   — mixed cadences, tied `window_start` → id breaks descending.
9. Property (the `tracking_stats` law precedent): for arbitrary
   entry sets and `now`, bucket membership partitions entries —
   every live in-window entry counted exactly once across the series.

### Presentation — `presentation.rs` (pure, fixed-stamp)

10. `review_reflection_section_phrases_collapse_only_on_zero_entries`
    — empty window → all `""`, `empty`; entries-without-ratings →
    rating cells `""` (the unanswered dash), flags phrase renders.
11. `the_per_flag_line_sorts_count_desc_then_name_asc` — deterministic
    double sort incl. an extra flag by verbatim name.
12. `review_rating_fractions_normalize_by_the_five_point_scale` —
    mean 4 → 0.8; unrated → 0.0; every fraction ≤ 1.0, finite
    (never NaN — the strip-max-free law of D-B6).

### Push — `models.rs`

13. `the_review_model_mirrors_the_reflection_folds` (extends the
    landed mirrors pin) — pushed phrases/fractions equal the domain
    folds over the same fixture windows.
14. `the_kiosk_belt_emits_the_empty_review_model` (extends M-4's
    landed test) — the belt model's reflection sections collapsed +
    empty entries handle.
15. `the_reflection_empty_state_collapses_in_tiers` — tier 2 (no
    entries in window → dashes + full-length zero strip, skeleton
    intact) + tier 1 extension (a journals-only state stands the
    view; a fully-empty state folds).
16. `the_entries_ledger_diffs_against_mutation` — a journal verb
    between passes (add + tombstone) yields the minimal row
    replacement set; newest-first holds after each pass.

### Journeys & goldens — `tests/journeys.rs`, `tests/golden_visuals.rs`

17. `the_review_surface_ledgers_and_returns` (extend) — navigate to
    Review with seeded entries; assert the new sections' structure
    (row counts, ledger length) — structure, never text.
18. Goldens: the review goldens regenerate data-only (clock frozen
    per `UiSettings::clock` — the §6.2 seam and slice A's
    `fix(ui-slint): the review ledger golden freezes its clock`
    precedent); new fixture entries carry fixed stamps + frozen
    render clock.
19. The kiosk rotation pin stays green untouched (M/a).

## 5. Deviation ledger

None at plan time. The one deliberate touch of landed code is the
`StatBarModel` field rename (D-B6) — compile-guided, vocabulary-
motivated, reported to the user here rather than discovered in
review; if stage 2 finds golden/snapshot churn beyond the mechanical
rename, STOP and report instead of adapting silently.

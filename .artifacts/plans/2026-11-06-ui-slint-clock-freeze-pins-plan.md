# [Plan] The ui-slint clock-freeze pins — chunk 2 of the clock-seam remediation (A1, B1, B3)

- **Date**: 2026-11-06
- **Kind**: stage-1 implementation plan (TDD-first) for chunk 2 of
  `.artifacts/plans/2026-11-06-clock-remediation-dispatch-plan.md`.
- **Branch / worktree**: `fix/ui-slint-clock-freeze-pins` at
  `../taskboard-WT-fix-ui-slint-clock-freeze-pins`, off `main` @
  `e42c1160`.
- **Normative origin** (binding; deviations are recorded in §7):
  - the implementation plan
    `.artifacts/plans/2026-11-06-clock-remediation-plan.md` §0 (scope),
    §1 (D-1 the pinned instant, D-4 the regen, D-5 the TZ matrix, D-6
    no weakening), §3 (this chunk's catalogue: §3.1 A1/#627, §3.2 B1,
    §3.3 B3 + the #663 correction);
  - the audit `.artifacts/reports/2026-11-06-clock-seam-audit-report.md`
    §4.1 row A1, §4.2 rows B1/B3;
  - the rule `docs/testing_strategy.org` §6.1–6.2 (oracle → tool) and
    `docs/coding_guidelines.org` §2.2;
  - AGENTS.md §5 (clock discipline, no weakening, no text assertions),
    §8, §10.
- **One mechanism throughout**: freeze the render-pass content clock
  (`UiSettings::clock = Some(PINNED_NOW)` → `Adapter::day_ctx`,
  `src/adapter.rs:966-970`), derive every seed the pass renders from
  the same instant, and (goldens only) regenerate once. Cadence is
  never frozen — the repaint tick and mock clock run as before
  (`src/lib.rs:97-104`).

---

## 0. What the seam does and does not reach (read before editing)

Everything the nine goldens and the four B1 tests assert flows
through `day_ctx` — verified for this plan, so the pin fully
determines each oracle (the §6.2 precondition):

| Rendered content | Source of its `now` | Seamed? |
|---|---|---|
| Toolbar tracking chip `· 12m` | `models.rs:2126-2134` via `day.now` | yes |
| Tracked-today rows + day-total phrase (#627) | `models.rs:2360` `tracked_today_rows(state, day.window, day.now)` | yes |
| Notification rows' `fired 30m ago` / `fired 1d ago` | `models.rs:988→772` `notification_rows(…, now)` with `now = day.now` | yes |
| Focus overlay remaining text | `models.rs:1018→1060` `focus_overlay(state, focus, now)` = `(open_entry.start + work) − day.now` | yes |
| Plan-next-day window / boxes | `models.rs:2606` `next_day_window(day.now, …)` | yes |
| Review bars / tracking sections (models rig) | the `review_rig` mount → the adapter's `day_ctx` | yes |

Not seamed, and deliberately untouched (R2/R3): the seconds-surface
*cadence* term (`adapter.rs:1171` — decides whether a 1 s tick is
armed, renders nothing), the due-editor validity reads
(`adapter.rs:1200/1302/1433/1475/1570/1618`), the alarm pass. None
of the tests in this chunk render those paths' content.

## 1. Decisions

### D-C1 — `PINNED_NOW` is a true `const`, one per test binary

chrono 0.4.45 (`Cargo.lock`) makes the whole readable chain `const`:
`NaiveDate::from_ymd_opt` → `and_hms_opt` → `and_utc`. So the
constant reads as a date, not an epoch literal:

```rust
/// The pinned render clock of every clock-exposed test in this
/// binary (docs/testing_strategy.org §6.2, #660): a fixed mid-day UTC
/// instant — the review-ledger template's capture class. Seeds derive
/// from it; the mount freezes `day_ctx` at it. Never read the wall
/// clock next to it.
pub const PINNED_NOW: chrono::DateTime<chrono::Utc> = {
    let Some(date) = chrono::NaiveDate::from_ymd_opt(2026, 11, 6) else {
        panic!("2026-11-06 is a calendar date");
    };
    let Some(instant) = date.and_hms_opt(12, 0, 0) else {
        panic!("12:00:00 is a wall time");
    };
    instant.and_utc()
};
```

Homes (the ui-slint crate has exactly two test binaries that need it):

| Binary | Home | Consumers |
|---|---|---|
| integration tests (`tests/golden_visuals.rs`, `tests/journeys.rs`) | `tests/support/fixture.rs` as `pub const PINNED_NOW` (the `support` module already carries `#![allow(dead_code)]` for per-binary partial use, `support/mod.rs:10`) | the nine golden journeys, the review-ledger template (§3.4), the two B1 journeys |
| the lib's `#[cfg(test)]` binary (`src/flow_table_tests.rs`, `src/models.rs` tests) | `src/test_platform.rs` as `pub(crate) const PINNED_NOW` (already `#[cfg(test)]`-gated at `src/lib.rs:58`, and already the shared lib-test home — `on_slint_thread`) | A1/#627, the placed-box reopen test, the models review rigs |

The value is identical in both (D-1); a stage-3 reviewer checks the
two literals match.

### D-C2 — `closed_entry_at(state, task, start: DateTime<Utc>)`

Signature:

```rust
/// Mints one CLOSED one-hour entry on `task` at the absolute instant
/// `start` (`[start, start + 1 h)`), no clock read — the fixed-clock
/// sibling of `closed_entry_on` for tests whose render pass is
/// pinned through `UiSettings::clock`.
fn closed_entry_at(
    state: &mut AppState,
    task: TaskId,
    start: chrono::DateTime<chrono::Utc>,
) -> taskboard_domain::TimeEntryId
```

Same entry shape as today's helper (`title "closed"`, `Work`,
`created_at = updated_at = start`, `end = start + 1 h`). **`closed_entry_on`
becomes a thin wrapper**: it computes its local-wall `start` exactly
as now (`:2808-2814`) and delegates to `closed_entry_at` — the four
other call sites (`:2939`, `:3149`, `:3420`, and chunk 3's family)
are untouched in behaviour, and chunk 3's later refactor touches
only the wrapper's date source. Only `:3556-3557` (the #627 test)
switches to `closed_entry_at`.

### D-C3 — the #627 seeds (A1)

At `PINNED_NOW = 2026-11-06 12:00:00 UTC`, under the default
calendar (`day_start = (00:00, UTC)`, `src/lib.rs:164-167`), the
frozen window is `[2026-11-06 00:00Z, 2026-11-07 00:00Z)`:

| Task | `closed_entry_at(…, start)` | Lies in window | Counted at frozen now |
|---|---|---|---|
| `task_one` (board one) | `PINNED_NOW − 3 h` → `09:00–10:00Z` | yes | 1 h, filtered out by the viewed board |
| `task_two` (board two) | `PINNED_NOW − 1 h` → `11:00–12:00Z` | yes | `min(end, now) − start` = exactly 1 h |

Both entries are strictly inside the frozen window and fully past at
the frozen `now`, so `test_tracked_today_rows() == vec![task_two]`
and `tracked_day_total == tracked_total_phrase(&part_total(1 h))`
hold at any wall time and any zone (the instants are UTC; the
window is UTC). The assertions are kept verbatim (D-6). The mount at
`:3570-3573` becomes `UiSettings { kiosk: true, clock:
Some(PINNED_NOW), ..UiSettings::default() }`; the seed comment at
`:3554-3555` is replaced by the pin rationale.

### D-C4 — the models rig: **pin**, centralized in `review_rig`

Inspection (`src/models.rs:3842-3866`): the review test rigs mount
through `review_rig(state, settings)` → `mount(…)` → the adapter
builds `DayCtx` itself. No test-side `now` can reach the pass's
`now` other than through `UiSettings::clock`, so single-read capture
*cannot* collapse the dual read here — the pin is the only tool that
fully determines the oracle's inputs (§6.2's exact-instant row: "a
pin also works when the path flows through `day_ctx`" — here it is
the only thing that does).

Shape (smallest diff; every call site untouched):

- `review_rig` applies the pin to whatever settings it receives:
  `UiSettings { clock: Some(PINNED_NOW), ..settings }` (the two
  custom-settings callers at `:3987` and `:4190` keep their fields).
- The module's three clock reads all move to the constant:
  `completed(…)` (`:3871`) stamps `done = PINNED_NOW − age`;
  `tracked(…)` (`:3894-3916`) stamps `start = PINNED_NOW − 3 h`
  (= `09:00Z`, inside the UTC day by construction, so the #658
  midnight clamp is dead code and is **dropped** — the comment
  shrinks to one line naming why `−3 h` is safe at the pinned
  instant); the open entry at `:4157` starts at `PINNED_NOW`.
- The positivity assertion (`:4110-4113`) and every other assertion
  are untouched.

Why every rig and not just the `tracked()` residual: a pinned pass
with a wall-stamped `completed()` would put `done` *after* the frozen
`now` on every run after 2026-11-06 — the #663 lesson (audit the
inherited seeds, not just the one named). The three helpers are the
module's complete seed surface (`grep -n "Utc::now\|Local::now"
src/models.rs` → exactly `:3871`, `:3896`, `:4157`; `:3199` is the
pure now-as-data section and stays).

### D-C5 — the B1 tests (three sites + D-C4)

| Site | Change |
|---|---|
| `tests/journeys.rs:13761` `the_reorder_routes_the_two_swap_writes` | `fixture::golden_state_at(PINNED_NOW)` (inherits the chip seed correctly — §3.4); `next_day_window(PINNED_NOW, …)` for `early`/`late`; mount via the existing `Harness::mount_with_settings(…, UiSettings { clock: Some(PINNED_NOW), ..UiSettings::default() })` (`:59`) |
| `tests/journeys.rs:13677` `the_placement_routes_one_mask_and_undo_returns_it` | same fixture + mount; the expected instant at `:13715` uses `next_day_window(PINNED_NOW, …)`; **delete** `harness_now()` and its self-recorded-race comment (`:13912-13917`; it has exactly one caller) |
| `src/flow_table_tests.rs:3610` `the_placed_box_reopen_seeds_the_form_from_the_instant` | seed `instant = next_day_window(PINNED_NOW, …).0 + 2 h`; both mounts via the existing `guard_rig(state, UiSettings { clock: Some(PINNED_NOW), ..Default })` (`:1416` — identical to `rig` modulo settings; no new rig); the stale-stamp arm's seed `Utc::now() − 3 d` (`:3654`) becomes `PINNED_NOW − 3 d` (an inherited seed: at the wall clock it drifts into the frozen future after 2026-11-09 and would stop being "stale") |
| `src/models.rs:3894` | D-C4 |

### D-C6 — the fixture `_at` builders (B3 plumbing)

In `tests/support/fixture.rs`:

- `pub fn golden_state_at(now: DateTime<Utc>) -> (Fixture, AppState)`
  — today's body with the open entry at `now − 12 m`;
  `golden_state()` delegates with `chrono::Utc::now()` (the ~150
  structural-only journey consumers keep their shape — audit C3; the
  doc comment names the delegation and points committed-artifact
  consumers at `_at`).
- `pub fn today_overdue_state_at(now: DateTime<Utc>) -> (TwoBoardFixture, AppState, [TaskId; 2])`
  — the overdue anchor stays midnight-aligned at the *parameter's*
  yesterday (`(now − 1 d).date_naive().and_hms(0,0,0).and_utc()`),
  the chip entry at `now − 12 m`; `today_overdue_state()` delegates
  with `Utc::now()`.
- Not converted (out of scope, unrendered or non-golden):
  `deliver_task_notification`'s rule `created_at` (`:251`, C3) and
  `journey_state()` (`:286`, a journeys-only consumer).

### D-C7 — the nine golden journeys: seeds and mounts

Every journey below: `let now = fixture::PINNED_NOW;` (or the const
inline), seeds via the `_at` builders, mount settings `UiSettings {
clock: Some(now), ..<existing settings builder> }`. The helper
functions grow a `clock` field rather than each caller re-spelling
the struct: `scale_settings(scale)` and `kiosk_rotation_settings()`
set `clock: Some(PINNED_NOW)` (every caller of both is a golden; the
review-ledger/-empty journeys build their own settings and are not
affected — verify with `grep -n "scale_settings(" tests/golden_visuals.rs`).

| Journey | line | fixture builder | extra seeds re-relativized |
|---|---|---|---|
| `the_board_goldens_hold_across_the_scale_breakpoint_matrix` | `:148` (`capture_and_compare`) | `golden_state_at(PINNED_NOW)` | — |
| `the_focused_cell_golden_pins_the_ring` | `:190` | `golden_state_at` | — (own `mount`: add `clock`) |
| `the_today_goldens_hold_across_the_scale_breakpoint_matrix` | `:272` (`capture_today`) | `today_overdue_state_at(PINNED_NOW)` | — |
| `the_kiosk_board_golden_holds` | `:380` (`capture_kiosk`) | `golden_state_at` | — |
| `the_kiosk_today_golden_holds` | `:400` (`capture_kiosk`) | `today_overdue_state_at` | — |
| `the_notification_golden_holds` | `:429` | `golden_state_at` | `due` = `(PINNED_NOW − 1 d)` at 00:00Z (`:444-448`); `fired = PINNED_NOW − 30 m` (`:452`) |
| `the_task_view_goldens_hold_across_the_scale_height_matrix` | `:503` (`capture_task_view`) | `golden_state_at` | — |
| `the_focus_overlay_golden_holds` | `:587` | `golden_state_at` | own `mount`: add `clock`; **add `pin_utc_zone()`** (the one golden without it — parity, audit B3) |
| `the_track_entries_golden_holds` | `:667` | `golden_state_at` | — (the two closed spans are already absolute `2026-03-15` instants; they stay) |

Doc comments that currently argue "a run never lives long enough to
cross the minute/second bucket" (`:268-271`, `:425-428`, `:497-501`)
are rewritten to state the pin: the content clock is frozen, so the
phrase is byte-stable by construction, not by run brevity.

### D-C8 — the review-ledger template joins the `_at` builder (byte-identical)

The implementation plan's #663 correction says this chunk's
`golden_state_at(PINNED_NOW)` *supersedes* the review-ledger
journey's in-test re-stamp loop (`golden_visuals.rs:848-860`), while
its acceptance line says the template is "untouched". Resolution:
the journey's **pixels and assertions stay untouched**; its seed
plumbing is simplified to `golden_state_at(PINNED_NOW)` + `let now =
fixture::PINNED_NOW;`, deleting the re-stamp loop and the inline
`with_ymd_and_hms`. The `_at` builder stamps the identical instant
(`now − 12 m` on task1's open entry, which is all the loop set), so
`review-ledger-1.0-1000.png` must come out of the regen
**byte-identical** (`git status` shows it unmodified). That identity
is the regression proof that the builder reproduces the template; if
the PNG changes, STOP — the builder differs from the template and
the chunk is wrong, not the golden.

## 2. TDD-first sequence (stage 2 executes in this order; one commit per step where the tree is green)

Each step is red → green → (refactor). "Red" for a flake class means
a *deliberate probe* that proves the assertion now reads the pinned
clock and nothing else — run locally, never committed.

### Step 1 — the constants and the `_at` builders (no test behaviour change yet)

1. Add `PINNED_NOW` to `tests/support/fixture.rs` and
   `src/test_platform.rs` (D-C1).
2. Add `golden_state_at` / `today_overdue_state_at`; make the
   unsuffixed builders delegate (D-C6).
3. Add `closed_entry_at`; make `closed_entry_on` delegate (D-C2).
4. Green: `cargo nextest run -p taskboard-ui-slint` — nothing
   observable changed; `cargo clippy --workspace --all-targets -D
   warnings` clean (no dead code — every new item has a consumer by
   step 3; if the lint fires before then, the step is incomplete,
   not to be `allow`ed).

### Step 2 — A1 / #627 (`flow_table_tests.rs:3463`)

- **Red probe**: convert the seeds to D-C3 but mount with `clock:
  Some(PINNED_NOW − 2 h)` (10:00Z): `task_two`'s `11:00–12:00Z` entry
  lies in the frozen future, clips to zero work, drops from the rows
  → the rows assertion fails. This is the exact #627 mechanism
  (clip-through-now), now reproducible on demand at any wall time.
- **Green**: mount with `clock: Some(PINNED_NOW)`; rows `== [task_two]`,
  total `1 h`.
- Commit: `fix(ui-slint): pin the kiosk tracked-section test's clock
  (#627)`.

### Step 3 — B1 (D-C5)

- **Red probe** (the placement journey): pin the mount but leave the
  expected instant on `Utc::now()`: `next_day_window(wall now)` ≠
  `next_day_window(PINNED_NOW)` on every day after 2026-11-06 → the
  `scheduled` assertion fails. That is the dual-read race made
  deterministic (the seed and the pass disagree on "tomorrow").
- **Green**: both derive from `PINNED_NOW`; delete `harness_now()`.
- Same shape for the reorder journey and the placed-box reopen test
  (probe: pinned mount, wall-clock seed → the box lands outside the
  plan window → `boxes.row_count()` ≠ 2 / the seed commit mismatches).
- Models rig (D-C4): probe by pinning `review_rig` first and running
  the module — `the_tracking_sections_mirror_the_domain_projection`
  fails (`completed()` stamps `done` in the frozen future, the
  `tracked()` entry too — the running-day bar is empty, `last.work >
  0.0` fails). Then move the three helpers to the constant → green.
- Commit: `fix(ui-slint): the planner and review rigs derive their
  seeds from the pinned clock`.

### Step 4 — B3 (D-C7, D-C8)

- Convert the nine journeys + the review-ledger template's plumbing.
- **Red probe** (one journey suffices — the notification golden):
  pinned mount, `fired = Utc::now() − 30 m` left on the wall clock →
  at the frozen `now` the alarm sits in the future → the row's caption
  changes bucket → the compare fails against the committed PNG. Then
  re-relativize → green against the *current* PNG (see §3 for why
  the current PNGs are expected to still match).
- **Regen** (D-4, exactly once):
  `TASKBOARD_REGEN_GOLDENS=1 cargo nextest run -p taskboard-ui-slint
  --test golden_visuals` (the run fails by design after writing).
- **Compare**: `cargo nextest run -p taskboard-ui-slint --test
  golden_visuals` green.
- Commit: `fix(ui-slint): freeze the nine clock-exposed goldens
  through the content seam` — PNGs that `git status` lists as
  modified ride this commit; the commit body lists them.

### Step 5 — the D-5 zone matrix and the gates (§4)

## 3. What the regen is expected to produce (so the reviewer knows what "unexplained" means)

The current goldens were captured with the chip at `now − 12 m`
rendered sub-second after seeding, i.e. they already show `· 12m`,
`fired 30m ago`, `fired 1d ago`, the overdue row at the one-day
bucket, and the focus overlay at `13:00` remaining (`25 m − 12 m`).
The pinned capture renders the *same text* exactly. So the expected
regen outcome is **zero or near-zero pixel change** across the 24
PNGs; the pin removes the failure *window*, not today's pixels.
Reviewer rule: any PNG that changes must show a diff confined to the
chip / notification-row / overlay-countdown strip; any change
elsewhere (layout, colours, other rows) is a rendering-order side
effect of pinning and must be explained or the chunk is rejected
(implementation plan §7, first risk). The three goldens outside the
nine — `review-ledger` (D-C8), `review-empty`, `icon-probe` — are also
rewritten by the regen (the variable is global) and must come back
byte-identical.

The PNG set (24 across the nine journeys, of the 27 committed):
`board-{0.5,3.0}-{600,1000,1500}` (6), `today-{0.5,3.0}-{600,1000,1500}`
(6), `task-view-{0.5,1.5,3.0}-{500,900}` (6), and one each of
`task-view-focus-cell-1.0-900`, `kiosk-board-0.5-1000`,
`kiosk-today-0.5-1000`, `notification-3.0-1000`,
`focus-overlay-1.0-1000`, `track-entries-1.0-1000` (6).

## 4. Verification procedure (acceptance)

1. **The zone matrix (D-5)** — each of the three, from the worktree:
   ```sh
   for tz in UTC Pacific/Kiritimati Etc/GMT+12; do
     TZ=$tz cargo nextest run -p taskboard-ui-slint --test golden_visuals
     TZ=$tz cargo nextest run -p taskboard-ui-slint --test journeys \
       -E 'test(the_reorder_routes_the_two_swap_writes) | test(the_placement_routes_one_mask_and_undo_returns_it)'
     TZ=$tz cargo nextest run -p taskboard-ui-slint --lib \
       -E 'test(the_kiosk_tracked_section_filters_to_the_viewed_board) | test(the_placed_box_reopen_seeds_the_form_from_the_instant) | test(/^models::tests::[^:]+$/)'
   done
   ```
   (the models filter is the module's top-level tests — the review
   push section lives directly under `models::tests`, not in its
   `today_pipeline`/`palette_pipeline` submodules; nine call sites
   plus the rig helper mount through `review_rig`). All green in all
   three zones is the acceptance signal; wall-hour independence
   follows by construction (every input is the constant). Note the
   goldens' own `pin_utc_zone()` overrides `TZ` in-process — the
   matrix still proves the *lib and journeys* legs, and the focus
   overlay golden now carries the pin like its siblings.
2. **Clock-read audit** of the converted sites:
   `grep -n "Utc::now\|Local::now" tests/golden_visuals.rs` → zero
   hits; `tests/support/fixture.rs` → exactly the three delegations
   (`golden_state`, `today_overdue_state`, `journey_state`) plus
   `deliver_task_notification`'s unrendered rule stamp; `src/models.rs`
   → zero hits in the review-push section; `src/flow_table_tests.rs` →
   only `closed_entry_on`'s wrapper read and chunk 3's family.
3. **Goldens compare green** without the variable after the single
   regen; `git status` confirms the byte-identical set of §3.
4. **The full §10 gates** through `scripts/run-quality-gates.sh`
   (several agents share the box).
5. Handoff: PR open, `gh pr checks` green, PNG diff (if any) visually
   reviewed on GitHub; worktree removed per AGENTS §8.

## 5. Bookkeeping (stage 3)

- **#627** closes with this PR: the fix commit + the determinism
  argument (every input to the rows/total oracle is `PINNED_NOW`).
- **#658, #659, #660** get closing comments referencing the audit
  report §5 (implementation plan §6 — default owner is this chunk's
  stage 3).
- No new backlog issues expected from this chunk; if stage 2 finds a
  non-chip pixel change it cannot explain, that is a STOP-and-report,
  not an issue.

## 6. Estimated diff

| File | Lines (±) | Notes |
|---|---|---|
| `tests/support/fixture.rs` | ~+45 | const + two `_at` builders + delegations |
| `tests/golden_visuals.rs` | ~±70 | nine journeys' seeds/mounts, two settings helpers, doc comments, review-ledger plumbing (−12) |
| `tests/journeys.rs` | ~±30 | two journeys, `harness_now` removed |
| `src/flow_table_tests.rs` | ~+45 / −15 | `closed_entry_at`, wrapper, #627 seeds/mount, placed-box seeds/mounts |
| `src/models.rs` | ~±25 | `review_rig` pin, three helpers |
| `src/test_platform.rs` | ~+15 | const |
| `tests/goldens/*.png` | 0–24 binaries | expected zero or near-zero; see §3 |

≈ 250–300 Rust lines over six files, no production code touched
(`src/adapter.rs`, `src/lib.rs`, `src/presentation.rs` unchanged).

## 7. Risks and recorded deviations

- **Regen breadth** (implementation plan §7): 24 PNGs rewritten by one
  global regen; §3 fixes the reviewer's expectation (near-zero diff)
  so an unexplained region cannot hide.
- **D-C4 pins every review rig, not just the `tracked()` residual** —
  a deliberate widening of §3.2's "either is acceptable" to the whole
  module's seed surface, forced by the #663 inherited-seed rule
  (`completed()` would otherwise stamp `done` in the frozen future).
- **D-C8 touches the review-ledger journey's plumbing** despite the
  acceptance line's "template untouched": pixels/assertions untouched,
  the regen's byte-identity is the proof; the implementation plan's
  own #663 correction asks for this supersession.
- **The `const` form** relies on chrono's `const fn` chain (0.4.45,
  pinned by `Cargo.lock`); a future chrono bump that un-consts one of
  them would fail to compile loudly — the fallback is D-1's `fn
  pinned_now()` form, no semantic change.
- **Chunk 3 ordering**: it edits `closed_entry_on`'s wrapper after
  this chunk merges (implementation plan §7); the wrapper shape here
  (compute local `start`, delegate) is designed so chunk 3 replaces
  only the date source.
- **The placed-box test's stale arm** was a hidden date bomb at the
  wall clock once pinned (`now − 3 d` crosses the frozen `now` on
  2026-11-09); it is re-relativized here, not left for chunk 3.

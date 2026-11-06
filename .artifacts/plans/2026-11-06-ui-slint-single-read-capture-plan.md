# [Plan] The ui-slint single-read capture — chunk 3 of the clock-seam remediation (B2, R3)

- **Date**: 2026-11-06
- **Kind**: stage-1 implementation plan (TDD-first) for chunk 3 of the
  clock-seam remediation dispatch
  (`.artifacts/plans/2026-11-06-clock-remediation-dispatch-plan.md` §4).
- **Branch / worktree**: `fix/ui-slint-single-read-capture` at
  `../taskboard-WT-fix-ui-slint-single-read-capture`, branched off
  `main` @ `e1a309b2` (chunk 2 merged and verified).
- **Normative origin** (binding; deviations recorded in §5):
  `.artifacts/plans/2026-11-06-clock-remediation-plan.md` §0/§1/§4 —
  **D-3** (one clock capture per test, feeding every seed and every
  expectation; the adapter's own read is the accepted residual) and
  **D-6** (no assertion weakening, expectations derived via the same
  calendar, not re-implemented logic); audit report §4.2 row **B2**
  ("single-read capture … Do *not* extend the seam here"); ruling
  **R3 accepted** (test-only; no new seam field). AGENTS §5
  clock-discipline bullet, §8, §10 apply.
- **One mechanism throughout**: collapse each rewritten test's
  test-side clock reads to exactly one capture, taken at the top of
  the test body, and derive every seed and every expected instant
  from it. Zero production diffs.

## 0. The residual — read before editing

The editor paths bypass `day_ctx` (audit §2 seam 4's "stops at"
list), so `UiSettings::clock` cannot freeze them and R2/R3 decline a
new seam field. Three adapter reads stay unshared:

- `seed_track_form` and `commit_track_form` each read
  `chrono::Local::now().date_naive()` (`src/adapter.rs` — post-chunk-2
  locations in the `seed_track_form` body and the `commit_track_form`
  body; the audit cites `:1570`/`:1618` pre-chunk-2).
- `recompose_timer_setter` reads `chrono::Utc::now()`
  (`:1302` in the audit's numbering) and feeds it to
  `compose_timer_at` for the "tomorrow" roll.

The single-read capture collapses the *test's* side to one read; the
race narrows to "local midnight falls between the test's capture and
the adapter's own read" — a millisecond-wide window that only bites
exactly at local midnight. That residual is **accepted** (R2/R3) and
must be *named in a comment at each rewritten test* (§1 D-S3). Stage 2
must not "fix" it; stage 3 must reject any attempt to.

## 1. Decisions

### D-S1 — the helper: a dated variant that `closed_entry_on` delegates to

Post-chunk-2 state: `closed_entry_at(state, task, start: DateTime<Utc>)`
exists (chunk 2's absolute sibling); `closed_entry_on(state, task,
hour)` still reads `Local::now().date_naive()` internally and serves
five call sites, three of which are chunk-3-converted and two of which
are already-fine (`the_delete_affordance_arms_then_confirms…`,
`the_kiosk_gate_refuses_the_entries_open`).

Shape: **move the wall-clock construction into a dated variant and
make the old name a one-line delegation** —

```rust
/// Mints one CLOSED entry with exact local instants on `task`, at
/// `hour` and `hour + 1` on the *given* local date (the fixed-zone
/// trick: the test asserts exact instants, so the seed and the
/// expectation derive from the same local wall-clock — one capture
/// the test owns, per D-3/R3).
fn closed_entry_on_date(
    state: &mut AppState,
    task: TaskId,
    hour: u32,
    today: chrono::NaiveDate,
) -> taskboard_domain::TimeEntryId { …the existing with_ymd_and_hms body… }

/// `closed_entry_on_date` at the *device's current* local date — for
/// the already-fine call sites whose oracle is relative.
fn closed_entry_on(state: &mut AppState, task: TaskId, hour: u32) -> … {
    closed_entry_on_date(state, task, hour, chrono::Local::now().date_naive())
}
```

Why this and not a `NaiveDate` parameter added to `closed_entry_on`:
it leaves the two already-fine call sites byte-identical (the dispatch
non-goal) and keeps exactly one copy of the local-wall-clock
construction. `closed_entry_at` is untouched. Note only one of the
three converted tests actually seeds an entry (`the_track_form_
edit_commit…`); the dated variant exists for it, and the other two
tests need only the capture (D-S2).

### D-S2 — per-test: where the single capture lives

The capture is the **first statement** of each test body (inside
`on_slint_thread` / the `run` closure), before any mount, seed, or
form open — so every later derivation, test-side and assertion-side,
reads the same binding. Each test body must contain exactly one
`Local::now|Utc::now` hit (the capture) — grep-verifiable per the
dispatch acceptance.

1. `the_track_form_edit_commit_routes_the_exact_adjust_mask`
   (`src/flow_table_tests.rs`, audit `:2933`):
   `let today = chrono::Local::now().date_naive();` at the top.
   Seed: `closed_entry_on_date(&mut state, a_id, 10, today)`.
   `start_local` = `Local.with_ymd_and_hms(today.year(), …, 10, 0, 0)`
   (replaces the three `Local::now()` reads at the seed-echo
   assertion); `expected_end` = same construction at `12:30`
   (replaces the three reads at the mask expectation). Assertions
   keep identical semantics: same `"10:00"`/`"11:00"` echoes, same
   `AdjustTimeEntry` mask, same instants-by-construction.
2. `the_track_form_add_commit_routes_logtime_with_the_exact_instants`
   (audit `:2852`): the body already has a single mid-flight capture
   (`today`, after the form is seeded) — the change is *position and
   completeness*: hoist it to the top of the test body and reuse it
   for the date-text assertion and the `expected_start`/`expected_end`
   derivations (it already feeds the derivations). No helper use —
   this test seeds no entry.
3. `the_add_commit_routes_over_the_form_subject_not_the_focus`
   (audit `:3260`): hoist its existing `today` capture from
   post-commit to the top of the test body; `expected_start` (and
   `expected_start + 1 h`) derive from it as today. Semantics
   unchanged.
4. `the_setter_composes_the_custom_row_grammar`
   (`tests/journeys.rs:10936`): `let today =
   chrono::Local::now().date_naive();` as the first statement of the
   `run` closure, before `Harness::mount`. The expectation at
   `:10998-11006` (`today + 1 day` at 09:30 local, `from_local_
   datetime` → `with_timezone(&Utc)`) derives from that binding —
   same construction as today, no new calendar logic (D-6). This
   journey is *not* clock-pinned and gains no pin: its oracle is
   exact-instant, not a committed artifact, and its only committed
   state (the golden fixtures it inherits) is untouched.

### D-S3 — the residual-race comment (binding wording)

Each of the four tests carries this comment immediately above its
capture, adjusted per test for which adapter reads apply (comments
state the constraint; function names, not line numbers — lines
drift):

For the three flow-table tests (the track-form paths):

```rust
// Single-read capture (D-3/R3): `today` is this test's only clock
// read; the entry seed and every expected instant derive from it.
// Residual (accepted, R2/R3): the adapter's `seed_track_form` and
// `commit_track_form` keep their own `Local::now().date_naive()`
// reads — a local midnight falling between this capture and the
// adapter's read is the accepted millisecond-wide window.
```

(For the two tests that seed no entry, drop "the entry seed and".)

For the setter journey (the compose path):

```rust
// Single-read capture (D-3/R3): `today` is this test's only clock
// read; the expected `At` instant derives from it. Residual
// (accepted, R2/R3): the adapter's `recompose_timer_setter` keeps
// its own `Utc::now()` read feeding `compose_timer_at` — a local
// midnight falling between this capture and the compose is the
// accepted millisecond-wide window.
```

### D-S4 — no production diffs, no seam, no assertion edits

Zero changes outside `src/flow_table_tests.rs` and
`tests/journeys.rs`. Assertions keep their exact-instant semantics
(D-6); nothing becomes `…_currently_pins_…`; the already-fine call
sites (`closed_entry_on` at the delete-affordance and kiosk-gate
tests) must not appear in the diff beyond the delegation being
call-compatible. Chunk 2's pins (`closed_entry_at`, `PINNED_NOW`
family) are untouched.

## 2. TDD-first sequence (stage 2 executes in order; one commit per green step)

The B2 race cannot be reproduced on demand (it needs a midnight
straddle), so the "red" for each step is the structural oracle the
dispatch names: a per-test grep showing more than one clock read
before the conversion and exactly one after. Run it before and after
each step:

```sh
awk '/^fn the_track_form_|^fn the_add_commit_routes_|^fn the_setter_composes_/,/^}/' \
  <file> | grep -c 'Local::now\|Utc::now'
```

### Step 1 — the dated helper (D-S1)

Refactor `closed_entry_on` into the delegation + `closed_entry_on_date`
pair. Green: `cargo nextest run -p taskboard-ui-slint` — all five
`closed_entry_on` call sites behave identically; diff shows no test
behaviour change.

### Step 2 — the edit test (D-S2.1 + D-S3)

Convert, add the residual comment. Red probe (pre-edit): the test body
contains 6 in-body clock reads, plus the seventh hidden inside
`closed_entry_on`. Green: 1 (the capture), grep-verifiable; all
assertions byte-identical in meaning.

### Step 3 — the add and subject tests (D-S2.2/2.3 + D-S3)

Hoist both captures to the top of their bodies. Green: 1 read each;
the add test's date-text assertion and both tests' instant
expectations derive from the hoisted binding.

### Step 4 — the setter journey (D-S2.4 + D-S3)

Capture at the top of the closure; rewire the `:10998` expectation.
Green: the closure body has exactly one clock read.

### Step 5 — verification

- Per-test grep matrix: exactly one hit in each of the four bodies.
- `cargo nextest run -p taskboard-ui-slint` green.
- TZ sanity (D-5 spirit — these tests are local-zone by
  construction, so cross-zone *consistency* is the signal):
  `TZ=Pacific/Kiritimati` and `TZ=UTC` runs of the two affected
  binaries.
- Full §10 gates via `scripts/run-quality-gates.sh`.

## 3. Expected diff (so the reviewer knows the shape)

Test-only, two files, ≈110–140 changed lines:

- `src/flow_table_tests.rs`: helper split (~20 lines), edit-test
  rewrite (~30), add-test hoist (~6), subject-test hoist (~8), three
  residual comments (~24).
- `tests/journeys.rs`: capture + expectation rewire (~8), one
  residual comment (~8).
- No PNG regen (no golden touched), no production file.

## 4. Risks

- **The residual comment is the deliverable's memory**: if stage 2
  paraphrases it into hand-waving ("tests may race at midnight"), the
  audit's R2/R3 ruling loses its witness. Stage 3 checks the wording
  names the adapter's reads (D-S3).
- **Hoisting changes nothing semantically — verify**: the captures
  are pure reads; the only behavioural risk is a test that
  *accidentally* relied on a second read (none of the four does —
  their extra reads all re-derive the same local date).
- **`Local.with_ymd_and_hms` non-single outcomes** (DST): unchanged
  from today's shape — the `.single().expect("resolvable")` stays;
  the existing tests already tolerate this (22:00/23:00/20:00/21:00/
  10:00/12:30/09:30 are never DST-gap hours in practice, and the
  construction is unchanged).
- **The journeys closure is long**: the capture lives far from its
  use (~60 lines); the residual comment sits at the capture, and the
  expectation site needs no second comment (D-S3 wording says "the
  expected `At` instant derives from it" — one comment per test).

## 5. Deviations from the normative origin

None. D-3's "optional `NaiveDate` or a dated sibling" is settled as
the dated sibling (D-S1); the per-test capture placement (top of body)
is D-3's own wording ("captured at the top of the test"); the residual
comments are D-3's requirement with exact wording fixed at D-S3.

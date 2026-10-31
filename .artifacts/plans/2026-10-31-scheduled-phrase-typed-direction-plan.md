# Plan — typed scheduled-phrase direction (issue #458)

- **Date**: 2026-10-31
- **Chunk**: backlog round 2, chunk 1.4 ("typed scheduled-phrase direction")
- **Branch**: `refactor/scheduled-phrase-typed-direction` (worktree
  `../taskboard-WT-refactor-scheduled-phrase-typed-direction`, branched
  off `origin/main` @ 5bf90c63)
- **Origin**: cargo-mutants survivor `presentation.rs:153:18` (`<` →
  `>` flips every direction arm of `scheduled_phrase` and survives the
  suite); issue #458; sweep report
  `.artifacts/reports/2026-10-26-mutation-testing-ui-pure-modules-report.md`
  (Cluster B).
- **Licence note**: `taskboard-ui-slint` is an application crate — new
  `.rs` files (there should be none; all edits land in existing files)
  would carry `// SPDX-License-Identifier: AGPL-3.0-only`, and the
  crate-level `#![forbid(unsafe_code)]` already stands (AGENTS §6).

## 1. Problem

`scheduled_phrase` (presentation.rs ~:147) computes `delta =
scheduled - now`, buckets `delta.abs()` via `duration_bucket`, and
selects wording by `(delta < TimeDelta::zero(), bucket)`. The mutant
`delta < zero` → `delta > zero` swaps which wording class each
direction receives ("starts in {q}" ↔ "started {q} ago"). The only
directional test,
`scheduled_phrase_is_empty_when_absent_and_directional_when_set`
(~:2188), pins structural facts only (non-empty, future ≠ past at one
bucket) — the flip preserves both, so the mutant survives.

The function's *contract is wording*, but AGENTS §5 bans text
assertions. Issue #458 offers two resolutions; this plan records the
decision below.

## 2. Decision

**Resolution 2 — type the direction (house A1 style).** The direction
becomes a typed value; the kill is structural, with zero text
assertions, matching the type-driven policy (AGENTS §2). Resolution 1
(a wording-class text assertion) is rejected: it is a §5 exception by
fiat, and the type-driven route needs no exception at all — the
precedent (`DurationBucket`, due_phrase's structure-over-text tests)
is already in this exact module.

### Recorded decision — the two `<` → `<=` equivalents

The survivors at `presentation.rs:134:14` (`due_phrase`) and
`:153:18` (`scheduled_phrase`) turning `<` into `<=` are
**understood-dead and untouched**: `delta == 0` ⇒ `duration_bucket(0)
= Now` ⇒ both functions return the `Now` phrase from their first match
arm *before* the comparison is consulted, so `<` and `<=` are
equivalent on every reachable input (sweep report §2 Cluster A,
provable-equivalent). No ignore-list entry, no test, no code churn
aims at them. **Consequence for this chunk:** the refactor must keep
the `Now` arm ahead of the direction consultation (it does, see §3),
preserving that equivalence proof.

## 3. The enum shape (hand-rolled, private-field, no derive crates)

New private-in-module types next to `DurationBucket` — newtypes and
enums-with-data only; no derive crates (AGENTS §2 A1):

```rust
/// The direction of a scheduled start relative to now, minted from a
/// time delta. `Same` exists so the zero-delta arm needs no separate
/// comparison — the bucket comparison stays dead-equivalent (Cluster A).
enum Direction {
    Future, // delta > 0  — "starts in {q}"
    Past,   // delta < 0  — "started {q} ago"
}
```

`Direction` is a plain two-variant enum with **no fields**, so no
constructor invariant is needed and no private-field newtype machinery
applies (A1's fallible-mint pattern is for invariants; there is none
here). Minting is a total associated function off the sign of the
delta, with the zero case resolved first by the bucket:

```rust
impl Direction {
    /// Total: the sign of a non-zero delta; zero never reaches this
    /// (the `Now` arm returns first).
    fn of(delta: TimeDelta) -> Self {
        if delta < TimeDelta::zero() { Self::Past } else { Self::Future }
    }
}
```

Deliberate leniency documented at the mint: `of(TimeDelta::zero())`
would yield `Future`, but zero is unreachable past the `Now` arm —
same shape as the existing `<`/`<=` equivalence, now encoded once in
one function instead of smeared through a 7-arm tuple match.

The `scheduled_phrase` body becomes:

```rust
let delta = scheduled - now;
let bucket = duration_bucket(delta.abs());
match bucket {
    DurationBucket::Now => format!("starts now{suffix}"),
    DurationBucket::Minutes(n) => format!("starts {}{n}m{suffix}", Direction::of(delta)),
    ...
}
```

— or, preferred for zero string machinery churn, keep the arm structure
and match on `(Direction::of(delta), bucket)` after the `Now` early
return:

```rust
match bucket {
    DurationBucket::Now => format!("starts now{suffix}"),
    bucket => match (Direction::of(delta), bucket) {
        (Direction::Future, DurationBucket::Minutes(n)) => format!("starts in {n}m{suffix}"),
        (Direction::Future, DurationBucket::Hours(n)) => format!("starts in {n}h{suffix}"),
        (Direction::Future, DurationBucket::Days(n)) => format!("starts in {n}d{suffix}"),
        (Direction::Past, DurationBucket::Minutes(n)) => format!("started {n}m ago{suffix}"),
        (Direction::Past, DurationBucket::Hours(n)) => format!("started {n}h ago{suffix}"),
        (Direction::Past, DurationBucket::Days(n)) => format!("started {n}d ago{suffix}"),
        (Direction::Future | Direction::Past, DurationBucket::Now) => unreachable!(),
    },
}
```

Implementation freedom: the exact match nesting is the implementer's
call; the *contract* is (a) `Direction` is the only place the sign
comparison lives, (b) the `Now` bucket never consults the direction
(Cluster-A equivalence preserved), (c) wording strings are unchanged
byte-for-byte (the phrase is user-visible text; no rewording rides in
a refactor).

`Display` is deliberately **not** implemented: the wording is not a
`Direction`-owned concern (it composes bucket quantity + suffix), and
a `Display` impl would invite the text-assertion surface this chunk
exists to avoid. The direction's observable surface is the enum value
itself, tested structurally.

## 4. The structural pin catalogue

The mutant `<` → `>` now flips the *enum* handed to each arm. Kills:

| # | Test (all in `presentation.rs` `mod tests`) | Assertion shape | Kills |
|---|---|---|---|
| P1 | `direction_partition_is_typed` — for each of Minutes/Hours/Days inputs (±2 units past 60 s/3600 s/86400 s edges), `Direction::of(+d)` / `Direction::of(-d)` are `matches!(.., Direction::Future)` / `Direction::Past` | enum matches! | direct: flipping the sign in `of` fails both arms |
| P2 | replace the `assert_ne!(future, past)` pin in the existing directional test with, per bucket: the future phrase and past phrase each still render non-empty **and** the direction selector is exercised through `scheduled_phrase` by asserting on a new pub (or `#[cfg(test)]`-visible) helper `scheduled_direction(scheduled, now) -> Direction` that the format step consumes | typed outcome | the `<` → `>` mutant in `scheduled_phrase` now flips the returned `Direction`, and P1-style matches! assertions fail |

Concrete shape for (b): extract `Direction::of(delta)`-at-call-site
into `fn scheduled_direction(delta: TimeDelta) -> Direction` (or
expose `Direction` + `of` under `#[cfg(test)]`; prefer a small
`pub(crate)`/`pub` fn only if models.rs needs it — it does not, so
keep it module-private and let the tests reach it as `super::*`).
Then:

- `scheduled_phrase_is_empty_when_absent_and_directional_when_set` is
  **retained as-is** (it pins the empty-contract and the
  one-bucket-inequality structural facts and stays valid);
- new test
  `scheduled_direction_is_future_for_later_and_past_for_earlier`
  asserts `matches!(scheduled_direction(some_future_delta),
  Direction::Future)` and `matches!(scheduled_direction(some_past_delta),
  Direction::Past)` at the Minutes bucket **plus one arm per remaining
  bucket** (Hours, Days) so the wording arms are each fed a direction
  that the mutant flip would invert.

The one-pin-per-direction rule: **exactly one `matches!` assertion per
direction per bucket-class** — never a string compare. The
"representative bucket" is Minutes; Hours/Days pins guard the
wording-arm wiring, not the direction.

Zero text assertions are added. The wording strings themselves remain
unpinned (presentation wording, per the `due_phrase` precedent).

## 5. Evidence protocol for the mutant dying

The claim "<" → ">" in `scheduled_phrase` now dies" is verified, not
assumed:

1. **Scoped invocation** (chunk-local, not the full 407-mutant lane):

   ```sh
   SQLX_OFFLINE=true cargo mutants \
     -p taskboard-ui-slint \
     --file crates/taskboard-ui-slint/src/presentation.rs \
     -j 4
   ```

   **No exclude filters**: the two `<`→`<=` equivalents are expected
   and accepted, and the run is judged by exactly one criterion:

2. **Acceptance criterion**: the mutant at the direction-selection
   site (`delta < TimeDelta::zero()` inside `scheduled_phrase`,
   mutation `<` → `>`) is reported **CAUGHT**. The two `<=`
   equivalents at `:134`/`:153` remaining MISSED is the recorded,
   expected outcome (§2) and is not a failure of this chunk.

3. **Fallback**: cargo-mutants is unavailable locally → record the
   gap in the handoff, land the structural tests + quality gates
   anyway, and note the follow-up mutant run for the weekly lane; do
   not fabricate a kill claim.

4. Full `cargo mutants` over the other two files (resolver, view_state)
   is **out of scope** — the weekly lane owns it.

## 6. TDD step order

1. **Red**: write the P1/P2 pins (§4) against `Direction` /
   `scheduled_direction` — compile fails (types absent). Add the types
   minimally so the tests compile and fail on behaviour (or, if the
   types alone make them pass, that is the point: the pin *is* the
   deliverable; run them green).
2. **Refactor**: `scheduled_phrase` consumes `Direction::of` (§3);
   wording bytes untouched. Existing suite stays green throughout.
3. **Mutant evidence**: §5 protocol; record the run summary in the
   handoff/PR body.
4. **Gates**: `scripts/run-quality-gates.sh` full sequence (fmt,
   clippy `-D warnings`, nextest, doc tests, cargo deny, the
   three-command coverage lane) from the worktree.

## 7. Risks

- **Snapshot/golden coupling**: `scheduled_phrase` output feeds
  `models.rs:1242/1475` (`SharedString` fields). Wording is
  byte-identical by contract (§3c), so no goldens change; if a golden
  *does* diff, the refactor broke wording — fix the refactor, never
  regenerate the golden.
- **`unreachable!` arm** in the nested-match shape (§3): if clippy or
  the implementer dislikes it, the flat `match (Direction::of(delta),
  bucket)` with a `DurationBucket::Now` arm rendering the
  `starts now` phrase (absorbing the early return) is the
  fallback — equivalence still holds because `of` is only consulted
  after `Now` is excluded in that shape. Either shape is acceptable;
  pick one, do not mix.
- **Mutant-run cost**: scoped to one file (~96 mutants), minutes not
  the 58-minute full lane; `-j 4` to stay well under memory parity
  with the quality-gate lock.
- **Scope discipline**: no rewording, no `due_phrase` touch (its `<`→
  `<=` survivor is Cluster-A-equivalent and out of scope), no
  `.cargo/mutants.toml` changes, no models.rs edits.

## 8. Estimated diff

~60–90 lines net across one file (`presentation.rs`): ~25 lines of
type + mint, ~10 lines of `scheduled_phrase` rewiring, ~40–55 lines of
new/extended tests. No new files, no dependency changes, no goldens.

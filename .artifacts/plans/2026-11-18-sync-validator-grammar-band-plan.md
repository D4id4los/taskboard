# Sync Validator Grammar + Band — Chunk N-E Plan

- **Date**: 2026-11-18
- **Kind**: stage-1 implementation plan (post-round-9 parallel round,
  dispatch plan `.artifacts/plans/2026-11-18-post-round9-parallel-dispatch-plan.md`
  §1/§7, chunk N-E).
- **Branch**: `refactor/sync-validator-grammar-band` (worktree
  `../taskboard-WT-refactor-sync-validator-grammar-band`).
- **Covers**: #937 (DeckColor grammar dedent) + #885 (cadence-style
  revalidation band for the Deck sidecar skip).
- **Scope crate**: `taskboard-sync-nextcloud` only. Domain grammar and
  the files-crate skew twin are read-side references, not edit targets.
- **UX coverage**: no user-visible surface changes (engine-internal
  cache guard + private grammar delegation) — journey-exempt.
- **Status**: READY. Every open question resolved as D-1…D-9 below.

## 1. Landed base (read and verified)

- The #868 seam: `crates/taskboard-sync-nextcloud/src/actor.rs`
  — `CachedAppData.fetched_at` (stamped from the injected
  `SyncActorConfig.clock`), the row-5 equal-validator skip guard at
  the `clock.now().timestamp() - cached.fetched_at < SKEW_BAND_SECS`
  check, and the pub `SKEW_BAND_SECS: i64 = 2` constant (the #133
  named tolerance twinned per the skew plan §4 convention; the files
  crate carries its own twin in
  `crates/taskboard-sync-files-nextcloud/src/etag.rs`).
- The band test:
  `crates/taskboard-sync-nextcloud/tests/actor_wiremock.rs`
  `a34c_the_sidecar_skip_trusts_the_cache_only_inside_the_freshness_band`
  — anchored to `actor::SKEW_BAND_SECS`, wiremock download-journal
  request counts as oracle, `SettableClock` time control.
- The domain grammar:
  `crates/taskboard-domain/src/entities.rs` —
  `Color::parse(&str) -> Result<Color, InvalidColor>` (line 66):
  lowercase, then exactly-six-ASCII-hex-digits check;
  `Color::as_str()` is the normalized accessor. Exported from
  `taskboard_domain` (lib.rs line 105).
- The sync grammar re-implementation:
  `crates/taskboard-sync-nextcloud/src/color.rs` — private
  `DeckColor::validate` (len == 6 on the raw input, all
  `is_ascii_hexdigit`, not `#`-prefixed, lowercased).
- Dependency graph: `taskboard-sync-nextcloud/Cargo.toml` already
  declares `taskboard-domain.workspace = true` (plus the
  test-support feature in dev-deps) — **delegation is legal per
  AGENTS §2** (sync → domain is the sanctioned direction; domain
  gains no new dependency). Grammars are provably equivalent: any
  input the sync `validate` accepts is pure ASCII hex (invariant
  under lowercasing), so domain's lowercase-then-check accepts
  exactly the same set; the sync-side `!starts_with('#')` check is
  redundant (`#` is not a hex digit).

## 2. Decisions

### D-1 (#937) — delegation shape: `DeckColor::from_hex` delegates to `taskboard_domain::Color::parse`

The private `DeckColor::validate` fn is **deleted**. The minting arm
becomes a one-liner over the domain grammar:

```rust
pub fn from_hex(input: &str) -> Result<Self, ParseColorError> {
    taskboard_domain::Color::parse(input)
        .map(|c| Self(c.as_str().to_owned()))
        .map_err(|_| ParseColorError)
}
```

Everything else about `DeckColor` stays verbatim: the newtype (it is
a *sync-API* type, not a domain entity — Deck's wire color, kept so
the model's serde boundary is untouched), `ParseColorError` (the
crate's typed outcome is preserved — no error-type churn for
callers), the lenient `from_server` pass-through (issue #937 says
explicitly: keep it verbatim), `FromStr`, serde impls, `Display`,
`as_str`.

### D-2 — the redundant `#`-prefix check is not preserved

`Color::parse` rejects `#`-prefixed input already (`#` fails the
hex-digit byte check). The sync-side explicit
`!input.starts_with('#')` is dead weight once the grammar is
delegated; dropping it does not widen the accept set. Recorded here
so the stage-2 diff and stage-3 review both expect its removal.

### D-3 (#937) — the test matrix transfers, it does not shrink

Law [L](c) applies: every existing test in `color.rs` keeps its
valid/invalid coverage. The valid-lowercase test, the
invalid-classes enumeration (`#00c2e0`, short, long, non-hex,
embedded space, empty, short-all-zeros, `zzzzzz`), the FromStr
parity test, the display round-trip, the lenient-deserialize
pass-through (including the `"#FF0000"` case), the
normalized-serialization test, and the valid-inputs proptest all
stay, asserting against `DeckColor::from_hex` exactly as today.

This is consistent with "one behavior, one test, lowest layer": the
*grammar corpus* (accept/reject over the six-hex shape) is owned by
`taskboard-domain`'s `entities.rs` tests; the sync-side enumeration
is retained as the *binding* test of DeckColor's minting contract —
domain acceptance mapped to `ParseColorError`, lowercase
normalization carried into the newtype — which is a distinct typed
surface the domain test cannot cover. No new grammar-enumeration
tests are added sync-side (that would duplicate the domain corpus at
a higher layer).

### D-4 (#885) — band shape: revalidation due point, guard at the skew-adjusted boundary

The single-constant freshness guard (`age < SKEW_BAND_SECS`) is
replaced by a cadence-style trust window:

```rust
pub const SIDECAR_REVALIDATE_SECS: i64 = 300;
```

Guard predicate:

```rust
clock.now().timestamp() - cached.fetched_at
    < SIDECAR_REVALIDATE_SECS - SKEW_BAND_SECS
```

Semantics: the equal-validator skip is honored until the entry's
revalidation due point (`fetched_at + SIDECAR_REVALIDATE_SECS`);
the due point is pulled *forward* by the skew margin so the guard
never fires *late* — a time-derived conclusion ("this cache entry is
still inside its window") is never trusted within `SKEW_BAND_SECS`
of its boundary, which is exactly the #133 tolerance's meaning.
Net effect: one attachment GET per board per ~298 s instead of one
per scheduled cycle (default poll 30 s ⇒ ~10× fewer sidecar
re-fetches at production cadence; correctness-neutral, identical to
today's behavior at every instant outside the changed window).

`SIDECAR_REVALIDATE_SECS` is `pub`, module-scope in `actor.rs`,
documented as the Deck-crate cadence constant (not a twin of
anything): the files crate has no sidecar and is out of scope.

### D-5 (#885) — reconciliation with the #133 constant: one owner, no orphan

`SKEW_BAND_SECS` keeps **its owner** (the #133 skew-tolerance
convention, twinned per the skew plan §4) and keeps **its meaning**
(the skew margin at a time-boundary) — it is consumed, not replaced,
by the new guard, so it does not orphan. `SIDECAR_REVALIDATE_SECS`
is a *new* concern (cache freshness cadence) that the old shape
couldn't express; the issue's round law ("no second constant") was a
C4-round constraint, explicitly listed in #885 as the thing the
cadence-style alternative lifts. Both constants sit adjacent in
`actor.rs` with cross-referencing doc comments replacing the current
"the cadence-band alternative is backlog debt" text (the debt is
being paid; the comments must say so, and the doc lines citing the
backlog inside the skip arm and on `SKEW_BAND_SECS` are updated).

### D-6 — the a34c band test grows, never shrinks

The existing a34c test keeps all three of its legs and gains the
boundary shift (law [L](c)): in-band skip, boundary re-fetch. The
ages it advances are recomputed against the new window using **both
named constants** (never literals — the existing anchoring pattern,
now two-constant):

- **Leg 1** (fill): unchanged — cycle 1 downloads exactly once.
- **Leg 2** (in-band skip): advance to age
  `SIDECAR_REVALIDATE_SECS - SKEW_BAND_SECS - 1` ⇒ still no
  download (was: `SKEW_BAND_SECS - 1`). Strictly stronger: proves
  the skip survives many cycles, not just one.
- **Leg 3** (boundary): advance to age
  `SIDECAR_REVALIDATE_SECS - SKEW_BAND_SECS` ⇒ exactly one new
  download, entry restamped (was: age `SKEW_BAND_SECS`).
- **New leg 4** (window re-arms): after leg 3, advance another
  full window minus 1 (age since the *restamp*, not the original
  fetch) ⇒ still exactly 2 downloads — proves the restamp grants a
  fresh window and the guard is per-window, not per-entry-lifetime.

Doc comment on a34c updated to describe the cadence band (#885) and
cite both constants.

### D-7 — no other call-site changes

`SKEW_BAND_SECS` has exactly two references (the guard, the a34c
test) plus the doc-comment sites; `grep`-verified. No engine, app,
UI, or files-crate code reads the Deck guard. The wiremock suite's
other sidecar tests do not advance the clock past the old band, so
they are window-invariant (they never rely on the skip re-fetching;
leg-1-style fills dominate).

### D-8 — typed outcomes only; no sleeps anywhere

Both changes are pure computation over already-injected seams. Test
time control stays on the `SettableClock` (`start_paused`), oracles
stay request-count/journal and typed evidence matches — no text
assertions introduced.

### D-9 — explicit non-goals

- No change to `DeckColor`'s wire format, serde behavior, or the
  `from_server` leniency.
- No change to the files-crate `SKEW_BAND_SECS` twin or its etag
  guard (#885 names the Deck sidecar only; a files-side cadence band,
  if ever wanted, is its own issue).
- No configurable setting for `SIDECAR_REVALIDATE_SECS` — a boot-time
  constant until measured evidence says otherwise (the issue's
  trigger list).
- No colour-derivation/grammar changes outside the delegation
  (issue #937's non-goal).

## 3. Implementation phases

1. **Phase 1 — grammar dedent (#937)**: edit
   `crates/taskboard-sync-nextcloud/src/color.rs` per D-1/D-2;
   verify the D-3 test set passes untouched (except nothing — the
   tests are supposed to pass unchanged; any failure means the
   delegation changed behavior ⇒ stop and re-derive, per the
   "a new test failing can be a bug" rule inverted: an *old* test
   failing here means the refactor is wrong, not the test).
2. **Phase 2 — band (#885)**: edit `actor.rs` per D-4/D-5; grow a34c
   per D-6; update the doc comments citing the backlog debt.
3. **Phase 3 — gate**: pre-commit hooks + the flock-wrapped
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (background + poll; commit before any wait; flock-deferral rule
   applies). Coverage trio is dispatcher-owned, never run in-stage.

## 4. Test catalogue (full expected delta)

- `color.rs` tests: **zero modifications expected** (D-3). If any
  must change, that is a red flag to resolve before commit.
- `actor_wiremock.rs` `a34c`: legs 2–3 re-anchored, leg 4 added
  (D-6). One test, four legs — same behavior-per-test shape as
  today.
- Nothing else: no new unit tests for a constant; the guard's
  behavior is the integration test's subject (lowest layer that can
  observe it end-to-end is the wiremock actor suite; the guard math
  itself is two integer comparisons, not unit-testable in isolation
  without an inline reimplementation).

## 5. Risks

- **Grammar drift risk**: effectively nil — the delegation makes the
  accept sets identical by construction (proven in §1); the proptest
  stays as the drift alarm.
- **Band regression risk**: if the guard fired later than the
  boundary (off-by-one), stale content would be trusted past due.
  Leg 3 asserts the re-fetch at exactly the boundary; the `- 1`
  leg-2/leg-4 advances pin both sides. Direction of the inequality
  (`<`) means the boundary itself re-fetches — fail-closed, matching
  today's semantics.
- **Cost regression risk**: none beyond status quo — the window only
  widens the skip; every request the old shape made inside the new
  skip window was strictly redundant.
- **Rebase risk**: `actor.rs` is r9-adjacent? No — r9's collision
  framing puts sync-nextcloud outside r9 territory; N-E is the only
  parallel chunk touching this crate. The a34c test block is
  N-E-local.

## 6. Estimated diff size

~70–90 lines total: `color.rs` −10/+4 (validate deleted, delegation
arm, doc tweak); `actor.rs` +~30 net (new constant + docs, guard
predicate, comment updates); `actor_wiremock.rs` +~25 (leg 4 +
re-anchoring + doc comment). Zero new files. No dependency or
workspace-manifest changes.

## 7. Backlog obligations

None discovered during planning: #937 and #885 are themselves the
backlog debt being paid, and no out-of-scope work surfaced (D-9
non-goals remain covered by the existing issues where applicable).

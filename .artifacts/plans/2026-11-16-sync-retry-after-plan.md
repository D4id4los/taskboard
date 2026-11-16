# Sync Retry-After Chunk Plan (correctness-hygiene round, chunk F)

- **Date**: 2026-11-16
- **Round plan** (normative): `.artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md` §7
- **Dispatch plan**: `.artifacts/plans/2026-11-16-correctness-hygiene-dispatch-plan.md` §7
- **Covers**: #220 (honor Retry-After in client retry waits), #221
  (HTTP-date form + 503 coverage), #222 (push-abort Retry-After floor)
- **Branch**: `fix/sync-retry-after` — diff confined to
  `crates/taskboard-sync-nextcloud`
- **Status**: PLANNED (stage 1); re-anchored on chunk E's landed changes
  (PR #869 MERGED; sync-crate commits on `origin/main` @ `eb5a88c0`).
- **Dependency check**: verified — `gh pr view 869` = MERGED and E's
  transport commits (`5c1666fd`…`eb5a88c0` et al.) are on `origin/main`.
  E did not touch the retry surface (`backoff.rs`, `client.rs` retry
  loop, `poll.rs`, `push_exec/outcomes.rs`, `actor.rs` fail_cycle) —
  its changes live in the conditional-read/validators path; the seams
  below are unchanged from E's baseline.

## 1. Seams (re-anchored, file:symbol)

| Seam | Where | Today |
|---|---|---|
| Retry policy | `src/backoff.rs` `BackoffPolicy` | 3 attempts, 500 ms doubling, cap `max_delay` = 8 s; `delay(retry_index) -> Option<Duration>`; pure, property-tested |
| Retry loop | `src/client.rs` `DeckClient::send_with_retries` | retries while `DeckError::is_transport()`; sleeps exactly `backoff.delay(retry_index)` via the `RetrySleep` seam (`with_sleeper`); **ignores the advertised hint** |
| Header parse | `src/client.rs` `parse_retry_after` + `DeckClient::attempt` | delta-seconds only (strict ASCII digits, overflow ⇒ absent); status-agnostic extraction into `Raw.retry_after` |
| Status mapping | `src/client.rs` `map_status` | 429 ⇒ `RateLimited { retry_after }`; 503 ⇒ `Unavailable` (unit variant, **hint dropped**); 503 is transport-retryable but hintless |
| Poll floor | `src/poll.rs` `poll_backoff(streak, initial, max, floor)` + `src/actor.rs` `advertised_floor` / `LoopState.retry_after_floor` | a `RateLimited { Some(d) }` on read installs `st.retry_after_floor`; `poll_backoff` already clamps `floor.min(max)` (poll cap 60 s) |
| Push abort | `src/push_exec/outcomes.rs` `GroupOutcome::Aborted { landed }` + `src/actor.rs` abort arm (~L567) | the aborting `DeckError` is swallowed; `fail_cycle` gets `Duration::ZERO` — no floor from a 429 during push (#222) |
| Wiremock harness | `tests/contract.rs` (client-level, `RecordingSleeper`), `tests/actor_wiremock.rs` + `tests/common/hermetic.rs` (actor-level, paused clock), `src/push_exec/tests.rs` | client sleeps are recorded via `RecordingSleeper` (no real time); actor waits ride `start_paused` `advance` |

Existing wiremock oracles touched by this chunk:
- `contract.rs` `rate_limiting_exhausts_policy_and_reports_rate_limited`
  (429 + `Retry-After: 30`) asserts sleeps `[500 ms, 1 s]` — see §3.7.
- `contract.rs` `server_errors_retry_and_recover` (503, 503, 200, no
  header) — must stay byte-identical (acceptance: no-header behavior
  unchanged).
- `actor_wiremock.rs` A14 (exhausting 429 with hint → poll floor 2 s)
  — unchanged by this chunk (read-path floor already works).

## 2. Decisions (each resolves an open question)

### D1 — Where the header parse lives
`parse_retry_after` stays a free function in `client.rs`, widened to
both RFC 9110 forms with `now` as an explicit parameter:

```rust
fn parse_retry_after(header: &str, now: chrono::DateTime<chrono::Utc>) -> Option<Duration>
```

- All-ASCII-digit form: as today (`u64` seconds, overflow ⇒ absent).
- HTTP-date form: `chrono::DateTime::parse_from_rfc2822(header)`
  (IMF-fixdate is RFC 2822-compatible; `GMT` parses). Date ≤ `now` ⇒
  `Some(Duration::ZERO)` ("retry now" — present, not absent). Date >
  `now` ⇒ `Some(date - now)`, seconds saturated into `u64`
  (`Duration::MAX` on overflow — consumption clamps, D3).
- Anything else (empty, signed, decimal, garbage, non-ASCII) ⇒ `None`
  (**absent, never an error** — round-plan law; fallback to the policy
  backoff).
- The single impure call site is `DeckClient::attempt`, which passes
  `chrono::Utc::now()`. **Consequence**: the parser itself is pure and
  unit-testable with a fixed `now` — no new clock seam is added and no
  test asserts a wall-clock window. (If a timing flake ever appears in
  a wiremock date scenario, the sanctioned fallback is an
  `Arc<dyn Clock>` on the client — out of scope until then.)

### D2 — Retry-After surface type (carrier through the pipeline)
`Raw.retry_after: Option<Duration>` keeps its type: the enum form is
resolved to a `Duration` at parse time (D1), so nothing downstream
changes shape. `Raw`'s doc comment is updated ("delay-seconds and
HTTP-date forms; HTTP-date resolved against now at attempt time").

### D3 — The floor/cap interaction formula and the cap's value
Per the round plan: `honored = max(policy_delay, min(retry_after, policy_cap))` where
`policy_delay = BackoffPolicy::delay(retry_index)`.

- **Cap value: `BackoffPolicy::max_delay` (8 s default) — no new
  constant.** Rationale: (a) it bounds one call's retry budget
  identically whether the wait is curve-driven or hint-driven (total
  in-call wait ≤ 2 sleeps ≤ 8 s each, same as today); (b) honoring
  hints *longer* than 8 s inside the retry loop would stall the push
  pipeline behind a single call — the durable place for long hints is
  the cycle-level poll floor (`poll_backoff`, clamped at its own 60 s
  cap), which this chunk leaves as-is; (c) reusing the existing
  constant adds no config surface the round didn't ask for.
- The formula is implemented as `BackoffPolicy::defer` (D4), which
  returns the honored duration typed by *why* it was chosen.

### D4 — Typed deferred-outcome shape
New in `backoff.rs` (law (a): typed, never message text):

```rust
/// Why a retry waits what it waits: the policy's own curve, or the
/// server's (cap-bounded) hint dominating it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeferredBy {
    /// Exactly `BackoffPolicy::delay(retry_index)` (no hint, or a hint
    /// at/below the curve).
    Policy(Duration),
    /// `min(hint, max_delay)`, which exceeded the policy curve.
    ServerHint(Duration),
}

impl BackoffPolicy {
    /// `None` when the attempt budget is exhausted (same exhaustion rule
    /// as [`Self::delay`]).
    pub fn defer(&self, retry_index: u32, hint: Option<Duration>) -> Option<DeferredBy>;
}
```

- `send_with_retries` extracts the hint from the error via a new
  `DeckError::advertised_hint(&self) -> Option<Duration>`
  (`RateLimited { Some(d) }` / `Unavailable { Some(d) }` ⇒ `Some(d)`,
  everything else ⇒ `None`) and sleeps the honored duration:
  `self.backoff.defer(retry_index, err.advertised_hint())`. Budget
  exhaustion still returns the original error unchanged (its typed
  hint then flows to the actor's poll floor, unchanged).
- Structural invariant (tested with `proptest`, mirroring
  `poll_backoff`'s): `policy_delay ≤ honored ≤ max_delay`, and
  `honored == policy_delay` iff the variant is `Policy`.
- `DeferredBy` is the test oracle: the `RecordingSleeper` scenarios
  assert variants + durations, never logs. A `tracing::warn!` field
  (`deferred_by = ?outcome`) may accompany the existing retry warn —
  assertion-free.

### D5 — 503 participates (#221): widen `Unavailable`
`DeckError::Unavailable` becomes `Unavailable { retry_after: Option<Duration> }`
(chosen over mapping 503 ⇒ `RateLimited`: the status↔variant bijection
stays 1:1 and honest — 503 is not rate limiting — while the hint still
travels typed).

- `map_status` 503 arm passes `raw.retry_after`.
- Mechanical fallout (all in-crate, compile-driven): `error.rs`
  `is_transport`, `mapping.rs` match arms (L175/L194/L235) and the
  constructor sites in its tests, plus any test constructing
  `Unavailable` bare (`Unavailable { retry_after: None }`).
- `advertised_floor` (`actor.rs`) widens to match
  `Unavailable { retry_after: Some(d) }` alongside `RateLimited` — a
  hinting 503 now installs the poll floor on reads too.
- The existing no-header 503 wiremock scenario must stay green
  byte-identically (acceptance).

### D6 — Abort-path floor mechanics (#222)
Widen the abort variant to carry the floor, not the error (`DeckError`
is not `Clone`; the only thing `fail_cycle` needs is the floor):

```rust
GroupOutcome::Aborted { landed: Vec<PushOutcome>, floor: Duration },
```

- Construction sites populate `floor: advertised_floor(&err)`:
  `outcomes.rs classify()`'s transport arm (has `err`) and the direct
  transport-abort returns in `push_exec/execute.rs` (the `Err` arms at
  the create/update/delete call sites — stage 2 verifies each site has
  the error in scope; an abort site without an error in scope passes
  `Duration::ZERO`, documented at the site).
- `actor.rs`'s `Aborted` arm passes the carried `floor` to
  `fail_cycle` instead of `Duration::ZERO`. The storm then dies the
  same way the read-path floor already does: `fail_cycle` stamps
  `st.retry_after_floor`, and the next cycle's schedule goes through
  `poll_backoff(..., floor)`, which honors it up to the 60 s poll cap.
- No change to `poll.rs` (it already clamps `floor.min(max)`).

### D7 — 503 / HTTP-date wiremock coverage (#221)
One scenario each, on the existing `contract.rs` harness
(`RecordingSleeper` — no real time, law (b) satisfied by construction):

1. **delta-seconds hint honored (red→green)**: 429 + `Retry-After: 2`
   → sleeps `[2 s, 2 s]` (hint dominates 500 ms/1 s, under cap), typed
   `ServerHint(2 s)`. (New exact-value scenario; the *exhausting* case
   below replaces the old pinned test.)
2. **hint capped**: 429 + `Retry-After: 30` → sleeps `[8 s, 8 s]`,
   `ServerHint(8 s)` — the adversarial big-hint case (stage-3 review
   trap (b) covered by the scenario itself).
3. **hint below curve ⇒ policy**: 429 + `Retry-After: 0` → sleeps
   `[500 ms, 1 s]`, both variants `Policy` (a present-but-tiny hint
   must not *lower* the wait).
4. **absent header unchanged**: the existing no-header 429/503
   scenarios stay as-is, byte-identical sleeps `[500 ms, 1 s]`.
5. **HTTP-date capped, exact without tolerance**: 429 +
   `Retry-After: <RFC2822 of now + 3600 s>` → sleeps `[8 s, 8 s]`,
   `ServerHint(8 s)`. Exact because any parse of `now + 3600` (± skew)
   exceeds the 8 s cap — no wall-clock window is asserted (§6.1
   clock discipline: fixture anchored, oracle structural).
6. **HTTP-date in the past ⇒ absent ⇒ policy**: 429 +
   `Retry-After: <RFC2822 of now − 60 s>` → sleeps `[500 ms, 1 s]`,
   both `Policy`.
7. **503 with hint**: 503 + `Retry-After: 2` → sleeps `[2 s, 2 s]`,
   `ServerHint`; exhausts to `Unavailable { retry_after: Some(2 s) }`.
8. **unparseable garbage ⇒ absent**: 429 + `Retry-After: soon` →
   policy delays, `RateLimited { retry_after: None }`.

Exact *value* coverage for the date arithmetic (past ⇒ ZERO, future ⇒
delta, overflow saturation) lives in the pure `parse_retry_after` unit
tests with a fixed `now` — fully deterministic, no wall clock
anywhere in the test.

### D8 — #222 coverage
- `push_exec/tests.rs`: an abort from a rate-limited sub-call carries
  `floor = Some(hint)` (structural `matches!` on the widened variant).
- `actor_wiremock.rs` (A-series, paused clock): a push meeting a
  429+`Retry-After: N` aborts the cycle and the next cycle's schedule
  honors the floor — advance the paused clock past `poll_interval`,
  assert the next pull attempt fires only after `max(poll_backoff
  curve, N)` (clamped at 60 s). Mirrors A14's mechanics, on the
  push-abort path.

### D9 — The one existing test that pins today's behavior
`contract.rs` `rate_limiting_exhausts_policy_and_reports_rate_limited`
asserts `[500 ms, 1 s]` under `Retry-After: 30` with the comment "The
internal retry policy is untouched by the advertisement" — that is a
pin of the *limitation* #220 removes, not a contract. Per AGENTS §5
("no pins of today's behavior") the test is **updated** to the new
semantics (sleeps `[8 s, 8 s]`, exhaustion outcome unchanged:
`RateLimited { retry_after: Some(30 s) }` — that part must not
change; the error still carries the *unclamped* advertised hint for
the actor's floor). This is a semantics change mandated by the round,
not a weakening: the exhaustion/count assertions are kept verbatim.

## 3. Implementation order (TDD, each step red→green)

1. `parse_retry_after(header, now)` + its unit tests (pure; D1/D7-exact).
2. `DeferredBy` + `BackoffPolicy::defer` + proptests (D4) — red first
   (type doesn't exist).
3. Widen `DeckError::Unavailable` + fix all match/constructor sites
   (D5) — compile-driven, existing tests updated to
   `Unavailable { retry_after: None }` mechanically.
4. Wire the loop: `advertised_hint` + `defer` into `send_with_retries`
   (D4); scenarios D7.1–D8 as each goes red→green; update D9.
5. `Aborted { landed, floor }` + sites + actor arm (D6/D8).
6. `advertised_floor` widening (D5) + the read-path 503-floor scenario
   if not already covered by an A-series test (add one wiremock actor
   scenario only if no existing test covers a hinting 503 on read).

Commit granularity: one commit per numbered step (standalone builds,
`<type>(<module>)` subjects, e.g. `feat(sync-nextcloud): parse the
HTTP-date Retry-After form`).

## 4. Acceptance (verbatim from the dispatch plan, mapped)

- delta-seconds and HTTP-date scenarios produce the correct
  injected-clock wait — D7.1/D7.5 (recording sleeper = the injected
  clock for the client; paused tokio clock for the actor, D8).
- absent/unparseable header falls back to current backoff (typed, no
  error) — D7.6/D7.8; `DeferredBy::Policy`.
- floor ≤ honored wait ≤ cap asserted structurally — the `defer`
  proptests + `ServerHint(8 s)` exact scenarios.
- the abort path's rescheduling honors the floor — D8.
- existing retry behavior without a header unchanged — D7.4 and the
  untouched 503-no-header scenario.
- closes #220/#221/#222 at merge (dispatcher-owned).

## 5. Risks & notes

- **`Unavailable` widening blast radius**: several `mapping.rs`
  matches and test constructors; all compile-driven, in-crate. No
  domain/UI crate references `DeckError` variants by name in a way
  this breaks (deck errors are sync-crate-internal).
- **A14-style actor tests** must not regress: the read-path floor is
  untouched; only the abort path gains a floor.
- **`Retry-After: 0`** means "present, zero" — D7.3 pins that it never
  *lowers* the policy curve (the formula's `max` side).
- **No live-tier evidence** exists that Nextcloud emits either form
  (recorded on #221); all scenarios are wiremock — that limitation is
  already tracked on the issues, no new backlog entry needed.
- **Backlog scan result**: no out-of-scope work discovered during
  planning beyond what #220/#221/#222 already track (candidate: an
  injected `Arc<dyn Clock>` on the client for date parsing — recorded
  in D1 as a sanctioned fallback, not scheduled work; opened only if a
  scenario actually flakes).

## 6. Estimated diff

~450–600 lines total, overwhelmingly in
`crates/taskboard-sync-nextcloud`: `client.rs` (~90), `backoff.rs`
(~70), `error.rs` (~15), `mapping.rs` (~25), `push_exec/outcomes.rs`
(~10), `push_exec/execute.rs` (~15), `actor.rs` (~15); tests
`contract.rs` (~150), `backoff.rs` unit/proptest (~70),
`parse_retry_after` unit tests (~80), `push_exec/tests.rs` (~30),
`actor_wiremock.rs` (~50).

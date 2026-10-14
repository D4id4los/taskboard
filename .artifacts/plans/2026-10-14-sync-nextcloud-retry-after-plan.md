# [Plan] `Retry-After` in the Poll Backoff (Chunk P1)

- **Date**: 2026-10-14
- **Kind**: stage-1 implementation plan (TDD-first; this commit is plan
  only — no production code).
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md`
  §1 (execution model, §1.4 GUI no-go zones) and §2 chunk P1; backlog
  entry "[2026-10-10] `Retry-After` handling in the poll backoff"
  (`.artifacts/taskboard-potential-improvements-backlog.md:1014`),
  itself a leftover of the Phase 4 sync-actor plan §8 and the Deck
  client surface plan §9.
- **Branch / worktree**: `feat/sync-nextcloud-retry-after` in
  `../taskboard-WT-feat-sync-nextcloud-retry-after`, off
  `origin/main` @ `35c9508` (the dispatch-plan commit itself — see
  §5 for the base-choice note).
- **Goal**: a rate-limiting Nextcloud server can tell the sync actor
  how long to wait. The client parses `Retry-After` (delay-seconds)
  off 429 responses and surfaces it as typed data on
  `DeckError::RateLimited`; the actor threads it into
  `poll_backoff` as a **floor** on the doubling curve, clamped at
  the existing `backoff_max`. `poll_backoff` stays pure (a floor
  parameter, no clock, no timer) so paused-time actor tests stay
  deterministic. No new endpoints, no push-path behavior changes, no
  config surface (dispatch non-goals, honored in §2 D3/D4).
- **Status**: awaiting stage-2 implementation.

---

## 1. Verified starting point (anchored on origin/main @ 35c9508)

1. **`poll_backoff` is pure and trivially testable**
   (`crates/taskboard-sync-nextcloud/src/poll.rs:15`):
   `pub fn poll_backoff(streak: u32, initial: Duration, max: Duration) -> Duration`
   — `initial * 2^(streak-1)` saturating at `max`, three unit tests,
   no clock access. Re-exported at `lib.rs:37`; **no caller outside
   this crate** (verified by grep across the workspace), so the
   signature change is crate-internal plus the actor call site.
2. **429s surface as a bare unit variant.** `map_status`
   (`client.rs:201`) maps `StatusCode::TOO_MANY_REQUESTS` to
   `DeckError::RateLimited` (`error.rs:42`, no payload). The
   `Retry-After` header is readable exactly once — inside `attempt`
   (`client.rs:1052-1064`) — where only `ETag`/`Last-Modified` are
   extracted into `Raw.validators`; the header is dropped on the
   floor. `map_status` sees the status only, never headers.
3. **The client retries 429/503/transport internally**
   (`send_with_retries`, `client.rs:990-1023`): 3 attempts, sleeps
   500 ms / 1 s via the injectable `RetrySleep` seam, and returns the
   retryable error once the budget is exhausted. So the actor sees
   `RateLimited` only *after* the last 429 — the freshest possible
   hint is the one that should travel.
4. **The deadline seam is `reschedule`** (`actor.rs:270-277`):
   `poll_interval` after success, else
   `poll_backoff(st.failure_streak, cfg.backoff_initial, cfg.backoff_max)`.
   Failures funnel through `fail_cycle` (`actor.rs:440`), which bumps
   `failure_streak` and reports; it receives only the classified
   `SyncErrorKind`, not the `DeckError`. Three failure call sites:
     - engine state unreadable (`actor.rs:306`) — no `DeckError`;
     - push aborted (`actor.rs:375`) — `GroupOutcome::Aborted`
       (`push_exec.rs:92`) carries only `landed` outcomes; the
       `DeckError` is swallowed inside the executor;
     - pull failed (`actor.rs:407-411`) — the `DeckError` **is** in
       hand (`pull_snapshot` returns it; first failing read wins via
       `?`).
5. **`classify_read`** (`mapping.rs:187`) already folds `RateLimited`
   into `SyncErrorKind::Network` (→ `NetworkLost` broadcast, A7
   semantics). Domain is untouched by this chunk.
6. **`DeckError::RateLimited` match sites** (complete inventory, all
   compiler-forced by the variant widening): `client.rs:209`
   (construction), `client.rs:1004` (retryable set),
   `push_exec.rs:914` (`is_transport`), `mapping.rs:173,189` (two
   matches), `mapping.rs:402,466` (tests), `contract.rs:284` (test),
   and — the only out-of-crate site — `taskboard-app/src/
   commands/connect.rs:538` (`deck_variant`, an exhaustive
   log-only label match).
7. **Test tiers already have everything this chunk needs.** wiremock
   serves arbitrary response headers (`ResponseTemplate::
   insert_header` — proven by the ETag/304 fixtures,
   `tests/common/hermetic.rs:76-111`); the contract tier has the 429
   test with a `RecordingSleeper` asserting the exact retry sleep
   sequence (`contract.rs:268-287`); the actor tier runs under
   `#[tokio::test(start_paused = true)]` with the `next_report`
   helper advancing 10 ms steps (`actor_wiremock.rs:59-73`), and A7
   (`actor_wiremock.rs:431`) is the paused-time backoff-assertion
   template (`lost_at.elapsed() >= CFG.backoff_initial`). The
   production `TokioSleep` client sleeper fires deterministically
   under the paused clock (A7's 503-with-retries path proves it).
8. **Lane A serialization** (dispatch §1.2): P2 (attachment writes)
   waits for this PR to merge — both edit `client.rs`.

## 2. Design decisions

### D1 — Travel: a typed struct variant, not metadata

`DeckError::RateLimited` becomes

```rust
/// Rate limited (HTTP 429); retries exhausted. Carries the server's
/// `Retry-After` delay-seconds hint when present and parseable —
/// `None` when absent, malformed, or in the HTTP-date form.
#[error("rate limited")]
RateLimited { retry_after: Option<std::time::Duration> },
```

**Why the error and not a metadata channel**: the `Result<T,
DeckError>` return *is* the only channel from the ~30 client methods
to the actor (AGENTS §3 typed primitives; a side return value would
touch every method signature, and shared mutable metadata would be an
orchestrator-style bottleneck). The advertised delay is a fact *of
the failure*; thiserror's typed-payload style (`Server(u16)`,
`Transport(#[from] …)`) is the house precedent. The field is public —
matching the tuple variants' exposed payloads — so tests construct
and assert it structurally; no accessor indirection.

**Why one widened variant, not a sibling `RateLimitedAfter`**: one
HTTP status is one variant; two variants would force a *new* arm
into every exhaustive match (including the app's `deck_variant`) and
create classification ambiguity at each site. The widening instead
costs a mechanical `{ .. }` at each existing site (compiler-forced,
no silent misses) and exactly one out-of-crate line (§5).

### D2 — Scope: delay-seconds only; HTTP-date reads as absent

`parse_retry_after(header: &str) -> Option<Duration>` (private, pure,
in `client.rs`): strict `u64` seconds — `"30"` → `Some(30 s)`,
`"0"` → `Some(ZERO)` (server says "immediate"; a no-op floor by
construction). Everything else — whitespace-padded, negative,
non-numeric, and the RFC 9110 HTTP-date form (`Wed, 21 Oct 2015 …`)
— is `None`, and `None` leaves the doubling curve untouched.

**Why**: delay-seconds is *relative*, immune to kiosk/server clock
skew; the HTTP-date form needs "now" — a clock seam inside what must
stay a pure wire-parse (and a wall-clock dependency in the actor) —
for a form Nextcloud's rate limiter does not emit in practice
(delay-seconds observed). The crate already refuses to interpret
HTTP-dates on principle (`Validators`, `client.rs:180-187`: treated
as opaque, never parsed). If a live tier ever observes the date form,
it is a small additive follow-up (§7). Huge values parse fine
(`Duration::from_secs(u64::MAX)` is representable) — safety comes
from consumption-side clamping (D3), not parse-side rejection.

### D3 — `poll_backoff` floor: a `Duration` parameter, clamped at `max`

```rust
/// The wait before the next cycle after `streak` consecutive failures
/// (`streak >= 1`): `initial * 2^(streak - 1)`, capped at `max`, and
/// never below `floor` (the server's advertised `Retry-After`, already
/// clamped to `max` by the caller-facing rule below). `floor` is
/// `Duration::ZERO` when no hint exists.
pub fn poll_backoff(streak: u32, initial: Duration, max: Duration,
                    floor: Duration) -> Duration
```

computing `curve(streak, initial, max).max(floor.min(max))`.

- **`Duration`, not `Option<Duration>`**: `ZERO` is the natural "no
  hint" (a no-op under `.max`), keeping the function total and the
  three no-hint call sites unconditional.
- **Floor semantics** — the advertised delay never *shortens* the
  curve (it can only push the deadline later) and never *exceeds*
  `max`: the saturation cap stays the absolute ceiling.
- **Why clamp at `max` rather than honor arbitrarily large
  advertisements**: (a) a hostile or buggy server (`Retry-After:
  9999999`) must not be able to park the kiosk's autonomy
  indefinitely — clamped, the actor re-polls at `max`, eats one more
  429, and re-extends: self-healing by construction; (b) the config
  already answers "how long may a failure delay the poll" with one
  knob (`backoff_max`) — a second ceiling would be a new config
  surface, a dispatch non-goal; (c) the cost of a clamped re-poll is
  one conditional GET per `max` window — negligible at kiosk scale.
- **Overflow safety composes**: `floor.min(max)` bounds the delay
  before `reschedule`'s `Instant::now() + delay`, so a parsed
  `u64::MAX`-seconds hint can never panic the timer arithmetic.
- **Transposition risk** (three positional `Duration`s) is accepted
  for consistency with the existing signature; each parameter is
  pinned by name in the doc comment and by a dedicated unit test.

### D4 — Actor consumption: freshest hint wins, nudges stay immediate

- `LoopState` gains `retry_after_floor: Duration` (init `ZERO`).
  Every failure **overwrites** it via a new `floor: Duration`
  parameter on `fail_cycle`: the pull-failure site extracts it from
  the `DeckError` (small private helper in `actor.rs`,
  `RateLimited { retry_after: Some(d) } => d`, else `ZERO`); the
  engine-unreadable and push-abort sites pass `ZERO`. A stale
  advertisement can therefore never outlive the next failure
  observation, and the success path ignores the field entirely
  (`streak == 0` → `poll_interval`).
- `reschedule` passes `st.retry_after_floor` as `poll_backoff`'s
  floor. No `SyncActorConfig` change (D3).
- **Push-abort boundary recorded**: `GroupOutcome::Aborted` swallows
  the `DeckError`, so a 429 during push yields no floor. Widening
  `Aborted` is a push-path change (dispatch non-goal) for a case that
  self-corrects — the next cycle pulls unconditionally and meets the
  same rate limiter, installing the floor then.
- **Nudges stay immediate** (recorded, deliberate): `SyncNow` /
  `SetBoard` still trigger cycles at once (ADR 0006 absorber
  semantics); the floor governs *autonomous* poll deadlines only.
  The client's internal 3-attempt budget already bounds the server
  cost of a user-commanded cycle inside a rate-limit window.
- `is_transport` (`push_exec.rs:914`) and `classify_read`
  (`mapping.rs`) get the mechanical `{ .. }` only — no behavior
  change: `RateLimited { .. }` still → `Network` → the existing
  `NetworkLost` episode semantics (A7 pins those; not retested here).

### D5 — Client shape: header extraction in `attempt`, one construction site

`Raw` gains `retry_after: Option<Duration>`, extracted in `attempt`
from the `Retry-After` response header via `parse_retry_after`
(status-agnostic extraction; only the 429 construction consumes it —
see §7 for the 503 non-goal). `map_status(status: StatusCode)`
becomes `map_status(raw: &Raw)` so the 429 arm constructs
`RateLimited { retry_after: raw.retry_after }`; both closures
(`send_json`, `fetch_conditional`) already hold `raw`. The retryable
set in `send_with_retries` widens to `RateLimited { .. }`.
**Internal retry waits do not change** — the sleep sequence stays
exactly `BackoffPolicy`'s (bounded: ≤ 2 sleeps ≤ 8 s); honoring the
hint inside the retry loop would double the consumption surface for
none of the durable win (§7).

## 3. TDD test matrix (writing order)

Tier 0 — pure units (all `#[test]`, no runtime):

| Test | Layer | Pins |
|---|---|---|
| `parse_retry_after` table: `"30"`→`Some(30s)`, `"0"`→`Some(ZERO)`, `" 30"`, `"-1"`, `"abc"`, HTTP-date string → `None`, `"999999999999"`→`Some(huge)` (no panic) | `client.rs` | D2 strictness + huge-value totality |
| existing `poll_backoff` tests ×3: add `ZERO` floor, results unchanged | `poll.rs` | no regression |
| floor-below-curve → curve; floor-between-curve-and-max → floor; floor-above-max → `max` (clamp) | `poll.rs` | D3 precedence + clamping (dispatch acceptance) |
| proptest: ∀ streak/initial/max/floor — result ≥ curve-with-ZERO-floor ∧ result ≤ `max` | `poll.rs` | D3 invariants incl. overflow safety |

Tier 1 — client contract (wiremock, `RecordingSleeper`):

| Test | Layer | Pins |
|---|---|---|
| extend `rate_limiting_exhausts_policy_and_reports_rate_limited`: 429s now serve `Retry-After: 30`; assert `matches!(err, RateLimited { retry_after: Some(d) } if d == 30s)` and the sleep sequence stays `[500ms, 1s]` | `contract.rs` | D1 travel + D5 unchanged internal policy (typed assertion, no text) |
| new: 429 *without* the header → `retry_after: None` | `contract.rs` | absent-header fallback |

Tier A — actor, paused time (`start_paused`, `next_report` stepping):

| Test | Layer | Pins |
|---|---|---|
| `a14_rate_limited_poll_waits_the_advertised_retry_after`: AtomicBool-gated responder (healthy pull JSON ↔ `429 + Retry-After: 2`); cfg `backoff_initial 100ms, backoff_max 30s` (advertised 2 s sits strictly between — the discriminating zone). Healthy `SetBoard` → `Completed`; flip limited at `t0`; nudge → `Failed{Network}` (after the client's internal 1.5 s simulated retries), drain the `NetworkLost`; still-limited next report must satisfy `t0.elapsed() >= 2s` — a header-ignoring impl fires at `t0+1.6s` and fails; flip healthy → next report `Completed` (floor cleared with the streak) | `actor_wiremock.rs` | D4 end-to-end: advertisement reaches the autonomous deadline (dispatch acceptance) |

Lower-bound assertions only (house style; A7 precedent) — the 10 ms
stepping makes the earliest arrival exact without an upper bound.

## 4. Implementation steps (each commit standalone, rebase-merge hygiene)

1. `feat(sync): parse retry-after into poll backoff floor` — TDD:
   tier-0 poll.rs floor tests + proptest first, then the `poll_backoff`
   signature; update the three existing tests and `reschedule`'s call
   (floor `ZERO` via `LoopState` field, still unwired to errors).
2. `feat(sync): carry retry-after on the rate-limited error` — TDD:
   `parse_retry_after` table, then `Raw.retry_after` extraction,
   `map_status(&Raw)`, the variant widening with all nine mechanical
   match fixes (incl. the one app line, §5), contract-tier tests.
3. `feat(sync): rate-limited poll deadline honors advertised floor` —
   TDD: `a14` first (fails: deadline ignores the hint), then
   `LoopState.retry_after_floor`, `fail_cycle` floor param, the three
   call sites, `reschedule` wiring. All §10 gates green.
4. `docs(backlog): mark retry-after poll floor done` — final
   bookkeeping: ✅ DONE on the P1 entry (one-line note), plus the two
   §7 park entries appended with the entry template.

## 5. Parallel-safety audit (dispatch §1.4)

- **No-go zones — none touched**: no `taskboard-ui-slint/**`; no
  `bootstrap.rs`/`config.rs`/`main.rs`; no `AppState`/domain/
  `messages.rs` shape change (`taskboard-domain` diff is zero —
  `SyncErrorKind::Network` already absorbs rate limiting); no
  `ci.yml`, pre-commit, release files, version, ADRs.
- **One out-of-crate line**: `commands/connect.rs:538`
  `DeckError::RateLimited` → `DeckError::RateLimited { .. }` —
  compiler-forced by D1, log-label unchanged, in no lane's file set
  (P4 owns `status.rs`/`config.rs`, P5 `ipc/`, P6 `repl.rs`) and in
  no GUI chunk's set (GUI owns `bootstrap.rs`/`config.rs`/ui-slint).
  Adjacency-rebase expectation only.
- **Lane A**: this PR merges before P2 dispatch (both edit
  `client.rs`); stage 3 rebases onto `origin/main` before shipping.
- **Base-choice note (recorded deviation)**: the chunk brief's
  dispatch-plan file was absent from the *local* `main` checkout at
  planning start (local `main` was stale at `c501301`); it exists on
  `origin/main` @ `35c9508` ("docs(plan): parallel backlog dispatch
  during gui phase7"). The branch is therefore based on
  `origin/main@35c9508` — the then-current main per dispatch §1.3 —
  so stage-2/3 agents find the dispatch plan in-tree at the path the
  prompts reference.

## 6. Risks & mitigations

| Risk | Assessment / mitigation |
|---|---|
| Variant-widening breakage missed somewhere | Impossible to miss silently: every site is an exhaustive match; rustc enumerates them (nine found by grep, §1.6) |
| App-file ripple collides with a GUI lane | Different file from every GUI/P chunk (§5); worst case a trivial rebase per dispatch §1.4 cadence |
| Paused-time flake in `a14` | Same mechanics as A7 (proven): time advances only via `next_report`'s 10 ms steps; the 1.5 s internal-retry segment lands *inside* the first failed cycle, and the lower-bound assert (`≥ 2s`) discriminates a floor-ignoring impl (fires 1.6 s) reliably |
| Huge/negative/garbage header panics or wedges | Parse is strict-total (D2); consumption clamps at `max` before any `Instant + Duration` (D3); proptest pins `result ≤ max` |
| Floor outlives the rate-limit window | Freshest-hint-wins overwrite per failure, cleared with the streak on success (D4) |

## 7. Out of scope → backlog (stage 2 appends with the entry template)

- **Honor `Retry-After` inside the client's retry waits** (sleep
  `max(policy_delay, advertised)` between attempts) — bounded today
  (≤ 2 × 8 s); the durable win is the poll floor.
- **HTTP-date `Retry-After` form** and **`Retry-After` on 503
  (`Unavailable`)** — both additive on this exact shape (D1/D5) if a
  live tier ever observes them.
- **Push-abort floor** (`GroupOutcome::Aborted` carrying its
  `DeckError`) — push-path change, a dispatch non-goal;
  self-corrects via the next cycle's pull (D4).

## 8. Estimated diff

≈ 200–230 added/changed lines over 8 files:
`poll.rs` (+~65: param, docs, floor tests, proptest),
`client.rs` (+~85: parse fn + tests, `Raw` field, `map_status`,
retryable arm), `error.rs` (+~8), `actor.rs` (+~30), `contract.rs`
(+~25), `mapping.rs`/`push_exec.rs`/app `connect.rs` (4 one-line
`{ .. }` fixes), plus the backlog commit. No `Cargo.toml`, domain,
config, or CI changes.

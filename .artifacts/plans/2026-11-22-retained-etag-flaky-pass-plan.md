# Chunk B1 Plan — the retained-ETag flaky pass (#1165–#1170, #1214)

Stage-1 plan for chunk B1 of backlog-reduction round 5 (dispatch plan
`.artifacts/plans/2026-11-22-backlog-reduction-round5-dispatch-plan.md`
§5 — normative; scope report §B). Branch: `test/retained-etag-flaky-pass`
(worktree `../taskboard-WT-test-retained-etag-flaky-pass`, base
`origin/main` @ `5e13577c`). Stage 2 implements exactly per this plan.

## 0. Scope corrections (recorded decisions)

- **#1164 is not in this chunk.** The dispatch plan's B1 row says
  "#1164–#1170", but #1164 is the `backlog:` docs-token issue (the
  day-review plan's W-1 prose cell), and the dispatch plan's own E1 row
  lists #1164 there. **Decision:** B1 covers the five `flaky-test`
  issues **#1165, #1166, #1167, #1168, #1169, #1170** — that is five
  flaky observations over five tests in
  `crates/taskboard-sync-files-nextcloud/tests/suite/retained_etag_wiremock.rs`
  (s1, s2, s2t, s3, s3t, s3u minus one unaffected sibling) — plus
  **#1214**. The dispatcher's at-merge closure set is corrected
  accordingly (#1164 closes with E1).
- The S4 twins (`s4a`, `s4b`) share the harness but had no observed
  flake; the fix is harness-level, so they inherit it. They are not
  closed by this chunk (no issue exists; nothing to close).

## 1. The seam (read from the harness @ 5e13577c)

`tests/suite/retained_etag_wiremock.rs` — seven `#[tokio::test]`s over
one shared harness: a stateful `RetentionServer` (`Arc<Mutex<Model>>`
served through `Respond`) mounted on a wiremock `MockServer` with
`Mock::given(any())`, and a per-test
`WebDavFilesTransport::new(&mounted.uri(), …)`. Every test builds its
own client and its own model; the journal
(`model.journal.push(…)` in `Respond::respond`) is the tests' request
oracle, and exact request counts (PUTs/GETs per path) are pinned
assertions. All clocks injected; no sleeps (standing law b holds today).

The production transport
(`crates/taskboard-sync-files-nextcloud/src/client.rs`) builds
`Client::builder().build()` — reqwest defaults: keep-alive pooling with
idle connections parked per host — and `send_with_retries` retries the
transport-error class a bounded number of times with backoff delays.

## 2. Root cause (decision, with the mechanism spelled)

**Chosen hypothesis H-replay: a stale pooled keep-alive connection is
reused under in-process parallel load; the request on it fails at the
hyper layer and is *replayed* (hyper retries an idempotent request on a
reused connection that dies before a response; our `send_with_retries`
re-tries the transport class above that). Against a *stateful* model
the replay is visible and poisonous:**

- the attach create (`PUT` with `If-None-Match: *`) replayed ⇒ the
  first lands (201, file created), the replay answers **412**
  (`if_none_match_star` over an existing path) ⇒ attach fails with a
  **typed** (non-transport) error ⇒ `CycleVerdict::Failed {
  retryable: false }` — exactly the #1165/#1166/#1169/#1170 symptom;
- retried GETs land multiple times ⇒ the journal counts 4 where the
  scenario issues 1 — exactly #1167's symptom;
- count assertions (`puts == 2`, `gets == 1`) read inflated or zero
  (zero when the cycle died at attach before the scenario ran) —
  exactly #1166/#1168.

**The issue bodies' hypothesis (cross-test journal pollution via port
reuse) is recorded as REFUTED-on-the-mechanism:** per-test clients
embed the per-test server's port in the URL, so test A's traffic cannot
land on test B's model through any pool; and per-test models cannot
observe another test's journal. The *observed* signature (typed 412
failures + inflated journal counts within a single test) is fully
explained by in-test replay alone. Stage 2's reproduction evidence
confirms or overturns this (§4): if loaded-run failures show a *stale
pooled connection* error class (connection closed / reset) preceding
the typed failure, H-replay stands; if evidence shows genuine
cross-test traffic, the fallback H-port (below) applies.

**Fallback H-port (recorded, not chosen):** pooled sockets from a
dropped test runtime survive long enough for the OS to hand the
ephemeral port to a later test's `MockServer`, and a surviving
connection task delivers bytes to the wrong server. Only the fix
(§3) is the same; the evidence interpretation differs.

## 3. The fix (deterministic by construction)

**Harness-level: eliminate the pool in tests.** The transport gains a
test-only pool override, and the retained-etag harness (and only that
harness — the S4 twins ride it) uses it:

- `WebDavFilesTransport` gains `pub fn with_pool_disabled(mut self) ->
  Self` (test seam, `#[cfg(test)]`-visible via the crate's existing
  test-support surface or `pub(crate)` + the existing
  `test_support.rs` re-export pattern) that rebuilds the internal
  client as `Client::builder().pool_max_idle_per_host(0).build()`.
  Fresh connection per request ⇒ a stale pooled socket cannot exist ⇒
  the replay class is removed *structurally*, not made unlikely.
- `Harness::new` in `retained_etag_wiremock.rs` applies it.
- No assertion changes anywhere. The five tests' convergence contracts
  (#695 semantics) stay **byte-for-byte identical** (standing law c) —
  the diff touches only the harness construction and the client seam.
- No sleep, no yield count, no tolerance (standing law b; the stage-3
  grep-check passes trivially).

**Rejected alternatives (recorded):** raising the retry budget (a
bigger-anything variant, banned); serializing the tests
(`test-threads = 1` — hides the class, slows the suite, teaches
nothing); `tokio::time` tricks (no timing relationship exists to
perturb — the defect is socket lifecycle, not scheduling); waiting on a
structural fact post-request (nothing to await — the poison precedes
the first scenario request).

Note on the #1165–#1170 issue bodies: after the evidence lands, the
closure comments carry the corrected mechanism (per the standing
authorization, commented on the issues — §6).

## 4. Reproduction method (the R2-method, counts recorded)

Stage 2 records counts **pre-fix and post-fix**, both backgrounded
through `scripts/memory-gate.sh`, useful work between polls:

- Pre-fix: `cargo test -p taskboard-sync-files-nextcloud --test
  files_wiremock` (in-process parallel — the repro class) run N≥12
  times in a backgrounded loop; record failures per run
  (historical rate 1/12 on both layouts; expect the same order).
  A `RUST_LOG=debug`/`tracing` capture of one failing run names the
  error class immediately before the typed failure (the H-replay vs
  H-port discriminator).
- Post-fix: same loop, N≥30, **zero** failures recorded (floor-style
  acceptance — no ceiling asserted; law-compliant).
- nextest (process-per-test) is expected clean both sides — recorded,
  not gated on.

## 5. The five-issues-one-evidence-base closure structure

One PR, one evidence base (the §4 counts + the mechanism), closing
**#1165 #1166 #1167 #1168 #1169 #1170** together at merge: all five
share the harness and the mechanism; per-issue closure comments
reference the plan §2/§3 and the reproduction counts. If stage 2's
evidence *refutes* H-replay for any specific test (a genuinely
different failure signature observed), that test gets its own
`flaky-test` issue and the honest per-issue split per the dispatch
plan's acceptance clause — recorded as a deviation in the stage-2
report, never silently bundled.

## 6. #1214 — separate disposition (the masked-row midnight flaky)

`flow_table_tests::the_running_rows_edit_seeds_an_empty_end_and_the_mask_keeps_it_open`
fails when the run straddles local midnight: `adapter.rs::commit_track_form`
seeds `parse_track_form(.., chrono::Local::now().date_naive(), &chrono::Local)`
(the adapter's own wall read at `src/adapter.rs` ~3512), and the
fixture's `end_text = "23:59"` inverts the seeded start/end relation
when `today` flips mid-run.

**Disposition: the injected-clock seam on the adapter path — the
production wall read moves behind the adapter's existing frozen-clock
jurisdiction; the fixture anchors to `fixture::PINNED_NOW`.** This is
the same class the #921/#660 seam work already closed *elsewhere in the
same file* (the `frozen_now` lazy capture at `src/adapter.rs` ~2866
documents the pattern and its production contract: production hands
`None` and reads the wall clock exactly as before). Concretely:

- `commit_track_form` (and the sibling `commit_track_form_for_test`
  caller path) derives `today` (and the `Local` timezone offset) from
  the adapter's frozen-clock seam when set, wall clock otherwise —
  production behavior unchanged (the AGENTS §5 clock-discipline law:
  the *production read* is the seam, not a fixture clamp).
- The test freezes the render clock (`UiSettings::clock`) and anchors
  the fixture to `fixture::PINNED_NOW`; the midnight window ceases to
  matter structurally (deterministic by construction — not a "run
  before midnight" mitigation).
- **Not bundled** into the wiremock fix: separate commit, separate
  rationale, separate closure comment; the PR body references it with
  its own verdict.

## 7. Armed standbys (#697/#650/#968) — re-check only

Stage 3 verifies each issue's watchlist state (no new recurrence
observations since the last comment; capture intact) and comments only
if this pass learned something relevant. **No speculative fixes** (the
standing watchlist law). Expected outcome: "checked, no recurrence,
stays armed."

## 8. Open questions — all resolved

| Question | Decision |
|---|---|
| Is #1164 in this chunk? | No — E1's scope (§0). Closure set corrected. |
| One root cause or five? | One (H-replay, §2); fallback H-port shares the fix. |
| Reproduction method/counts | In-process parallel loop, N≥12 pre / N≥30 post, backgrounded via the memory gate (§4). |
| Where does the fix live? | Test-seam pool disable on the transport + the harness applies it (§3). Production default pooling untouched. |
| Do the band contracts change? | No — byte-for-byte (law c). |
| #1214 same fix? | No — the frozen-clock seam on the adapter path, separate commit/closure (§6). |
| Standbys? | Re-check + comment only (§7). |

## 9. Risks

- **H-replay is wrong in detail** (mitigated: §4's discriminator step —
  the error-class capture decides before the fix is written; the
  fallback H-port leads to the same diff, only the closure prose
  changes).
- **The pool-disable seam leaks into production defaults** (mitigated:
  the seam is opt-in, applied only in the retained-etag harness; the
  S4 twins and every other wiremock suite keep defaults — if stage 2
  finds the same class in sibling suites, that is a new `backlog:`
  issue, not a scope grab).
- **The #1214 seam widens adapter surface** (mitigated: it reuses the
  existing `frozen_now`/`UiSettings::clock` jurisdiction; production
  path reads identically; the file's own comments already document the
  contract).
- **Post-fix N=30 still flakes** — would falsify the whole
  hypothesis; escalate to the dispatcher with the captures rather than
  iterating fixes blind.

## 10. Estimated diff size

Small — **~80–120 lines total**: `client.rs` seam (~15),
`retained_etag_wiremock.rs` harness (~5, zero assertion lines),
`adapter.rs` clock-seam extension (~25 incl. comments), the flow_table
test fixture re-anchor (~15), plus plan/report prose. No new
dependencies; no format bumps; no UI files.

## 11. Acceptance (restated from the dispatch plan §5)

Shared-seam hypothesis confirmed with §4 counts; the fix
deterministic (structural pool elimination — await/deadline class, no
sleep/yield/tolerance anywhere in the diff); five issues close on the
one evidence base; #1214 dispositioned separately with its own
evidence; band contracts byte-identical; standbys re-checked not
touched.

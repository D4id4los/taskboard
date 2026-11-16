# Chunk E Plan — Sync ETag Retention + Clock Skew (#695 + #133)

Stage-1 plan for chunk E of the correctness-hygiene round (round plan
`.artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md` §6 —
normative; dispatch plan §6). Branch: `fix/sync-etag-skew`. This plan
records the design decisions; stage 2 implements exactly per it.

## 0. Scope correction (recorded decision)

The dispatch plan §1 confines the PR to `taskboard-sync-nextcloud`.
The #695 evidence lives in **`taskboard-sync-files-nextcloud`** (the
WebDAV substrate: `EtagStore`, `put_if_match`, the live pins
`it_nextcloud_live_files_lost_update_guard`/`..._etag_churn`).
**Decision:** the diff targets `taskboard-sync-files-nextcloud` (its
transport seam types it owns are in-crate, so the dispatch plan's
"+ transport types it owns" clause is satisfied) plus, for #133, the
one Deck-actor comparison site listed in §3 row 5
(`taskboard-sync-nextcloud/src/actor.rs:1056`). No other crate is
touched. `taskboard-domain` merge.rs comparisons are **out of scope**
(§3 rows 1–4 note why).

## 1. Facts on the ground (read from main @ dfdf2562)

- **Files publish path** (`cycle.rs::publish_appdata` /
  `rewrite_own_appdata`): guarded rewrite via
  `put_if_match(path, bytes, token)` where `token` is the stored
  `Etag`; 412 ⇒ `disambiguate_own_412` — **one** GET: 404 ⇒
  `OwnFileVanished` (token dropped, next cycle recreates), 200 ⇒
  `ForkDetected` stop-write (typed, in `st.forked_paths`). There is
  **no loop today** — the 412 path is already bounded (single
  disambiguation per cycle). What is missing is a *retry after
  re-resolution* and a *typed give-up* on the converged-content path.
- **Change detection** (`etag.rs::EtagStore.device_dirs` vs
  `read_dirs`, `dirty_devices()`): a peer's rewrite is detected solely
  by its directory ETag changing. Under hosted retention (#695: a
  same-second, same-byte-length PUT returns `204` and **keeps the old
  ETag**) that peer's rewrite is invisible ⇒ **false-clean** (F5/F6
  drift). Note: the payload shapes are fixed-width JSON with
  RFC3339 timestamps, so a same-second rewrite is *always*
  same-byte-length — "different length churns" is no protection at
  all for these documents.
- **Own-write bookkeeping** (`record_put`): the PUT response's etag
  overwrites the stored token. Under retention the response etag
  equals the pre-write token, so the client cannot tell "my write
  landed" from "my write landed and a peer's same-token write also
  landed in the same second" (F4 lost update: both clones hold token
  E0, both guarded PUTs succeed, the second silently wins).
- **The skip-compare is already content-based**
  (`cycle.rs` step 4: `StateHash::of(&merged)` vs `published_hash`,
  commented "never a server etag, F5") — the *own-path* publish
  loop is etag-independent for its skip decision. The exposure is
  peer-change detection and the lost-update window, not the
  skip-compare.
- **Deck actor validator compare**
  (`taskboard-sync-nextcloud/src/actor.rs:1056`):
  `cached.last_modified == candidate.last_modified` decides "unchanged
  ⇒ skip". Second-granular equality; the #133 skew site in the sync
  crates.
- **#133's domain residue**: `merge.rs` comparisons (document-clock
  fold `>=`, `Removed/Added` re-add `t >= r`, tombstone resurrect
  `lm >= stamp`) are governed by ADR 0004's tie-breaks
  (ties-to-document / ties-local, safe-leaning) and pinned as
  contract-or-debt by the chunk-S tests; the Δ-band guard
  (`skew_band_secs`, default 2) stays behind its observed-symptom
  trigger. This chunk does **not** build D-4c; it names the tolerance
  constant where the *sync crates* compare timestamps (§3).

## 2. Design decision

**Chosen: (b) revision-field precedence, with (a)'s post-write
verification GET as a bounded sub-step.** No heuristics.

**(b) — heartbeat `written_at` as the revision field for peer-change
detection.** Never conclude "peer unchanged" from ETag equality
alone. A peer is dirty iff *either* its directory ETag differs from
`read_dirs` (today's rule, kept) *or* its last-read heartbeat's
`written_at`/`last_clock` has advanced past the read's recorded mark
by more than the skew tolerance (§4). Concretely: `EtagStore`
gains `read_heartbeats: BTreeMap<DeviceId, DateTime<Utc>>` written
when a device is marked read; `dirty_devices()` returns devices where
the dir-etag compare fails **or** the freshly listed peer's stored
heartbeat stamp exceeds `read_heartbeats[device] + SKEW_BAND_SECS`.
This is *revision-field precedence over validator equality* exactly
as the round plan defines option (b): the heartbeat's monotonic-ish
`written_at` is the revision; the ETag is demoted to a fast path.
Every peer already publishes a heartbeat per cycle
(`write_heartbeat`), so the field exists on the wire today — no
format change, no `DOCUMENT_FORMAT` bump.

**(a) sub-step — retention-signature verification on the own path.**
After a guarded PUT returns 2xx with a response ETag **equal to the
pre-write token** (the retention signature), the client performs one
GET of the own path:
- served bytes == sent bytes ⇒ converged; refresh the stored token
  from the GET (a honest refresh, option (a)'s mechanic);
- served bytes != sent bytes ⇒ the same-second lost update happened:
  the typed `ForkDetected` path runs (fork alarm, stop-write, the
  served document enters `st.docs` as in `disambiguate_own_412`) —
  the sibling clone's own verification detects the mirror case, so
  the pair converges to full-subtree stop exactly as the existing
  fork-set comment describes. No silent overwrite survives both
  sides running this client.
A PUT whose response etag *differs* (normal churn) skips the GET
entirely — the non-pathological path is untouched.

**Alternatives rejected (recorded):**
- **(a) alone** — refresh on every second-boundary-adjacent write:
  the served bytes are only adjudicable by *comparison*, so (a) alone
  either trusts a retained etag (false-clean, banned) or degenerates
  into (b)'s content comparison on every write; and "when is a write
  second-boundary-adjacent" would need a clock peek at a place where
  the server's second, not ours, is what matters.
- **(c) alone** — If-Match discipline is already total
  (`put_create`/`put_if_match`, cold-store GET-first). Retention
  defeats precisely this: the server answers `204` to a *stale-guarded*
  same-second write because the current etag still *is* the guarded
  token. Server adjudication cannot be strengthened client-side; the
  gap needs client-side content/revision evidence.
- **Same-size + same-second ⇒ same-content heuristics** — banned by
  the round plan; not used anywhere.
- **Monotonic sequence/padding in the payload** (#695's substrate
  sketch): makes writes distinguishable only to *new* writers and
  needs a format bump; unnecessary once readers use the heartbeat
  revision. Recorded as a backlog option for the substrate's own
  re-characterization (§7).

## 3. The #133 comparison-site audit table

| # | Site | Compare today | Equal-second behavior | Verdict / action |
|---|---|---|---|---|
| 1 | `taskboard-domain/src/merge.rs:147` — document-clock fold `(doc_clock, doc_by) >= (local_clock, local_author)` | LWW with tie-to-document | tie resolves to document (ADR 0004, safe-leaning) | **No change.** Domain; tie-break is deliberate contract, pinned by chunk-S tests. Permanent debt documented on #133. |
| 2 | `merge.rs:215–217` — `(Removed{r}, Added{t}) if t >= r` re-add | re-add lands on equal instant | equal-second re-add lands | **No change.** Same contract family as row 1. |
| 3 | `merge.rs` resurrect decision (`lm >= tombstone`, ~:3192 comment) | server-fast resurrect flips under skew | equal-second re-add wins | **No change** (D-4c trigger-gated; chunk-S pins `…_currently_pins_skew_*` hold the debt visible). |
| 4 | `merge.rs:1557` — `DUP_WINDOW_SECS` dup-window | band compare, not ordering | n/a (window, not LWW) | **No change** — already a named band constant. |
| 5 | `taskboard-sync-nextcloud/src/actor.rs:1056` — `cached.last_modified == candidate.last_modified` (validator skip) | equality on second-granular server stamps | equal stamp ⇒ skip pull (correct when equal ⇒ equal content — Deck REST revisions churn on write) | **Add tolerance guard:** skip only when equal **and** the cached validator's own revision fetch is not due (see §5 note); pins unchanged for the non-pathological corpus. |
| 6 | `taskboard-sync-files-nextcloud/src/etag.rs` — `device_dirs` vs `read_dirs` (`dirty_devices`) | ETag inequality | **retained ETag ⇒ false-clean (the #695 bug)** | **Fix:** revision-field precedence (§2 b) — heartbeat stamp over ETag, `SKEW_BAND_SECS` applied. |
| 7 | `cycle.rs` `record_put` / verification — own-write token bookkeeping | response etag overwrite | retention ⇒ peer's same-token write indistinguishable | **Fix:** retention-signature verify GET (§2 a). |
| 8 | `status.rs::PeerFreshness { written_at, last_clock }` | display only (no convergence decision) | n/a | **No change** (display; no decision rides it). |
| 9 | `cycle.rs` step 4 `published_hash` skip-compare | content hash | n/a (content, not time) | **No change** — already etag-independent by design (F5-safe). |

Summary: **rows 6 and 7 are the fixes; row 5 gets the tolerance
guard; rows 1–4, 8, 9 are recorded no-change verdicts with reasons.**
The audit table is committed in this plan so stage 3 re-verifies each
row's verdict.

## 4. The skew-tolerance constant

```rust
/// taskboard-sync-files-nextcloud (pub(crate) in etag.rs; the Deck
/// actor imports the value from its own module-local constant of the
/// same name — the two crates do not depend on each other).
/// Tolerance for second-granular remote-vs-local stamp comparisons:
/// 1 s of representation loss (both sides truncate to whole seconds,
/// #695's hosted evidence shows second-granular validator behavior)
/// + 1 s of NTP-class cross-machine skew (#133's audit premise).
pub(crate) const SKEW_BAND_SECS: i64 = 2;
```

- **Value 2 s, justification against #695:** the hosted evidence is
  that validator churn is decided at whole-second granularity — two
  writes inside one server-second are indistinguishable to the
  validator family. A comparison that must not mis-order events needs
  to treat stamps within `1 s (representation) + 1 s (clock skew, the
  #133 premise) = 2 s` as unordered. This matches DR-4's already
  specified `skew_band_secs` default (2), so the two constants agree
  by construction.
- Named constant at module scope; **no call-site magic numbers** —
  the dispatch-plan acceptance requires this and stage 3 checks it.
- The Deck actor mirrors the constant locally
  (`taskboard-sync-nextcloud/src/actor.rs`), documented as a
  deliberate twin (the crates are independent; a shared home would
  drag a dependency edge across the workspace boundary for one
  integer). Alternative rejected: putting it in `taskboard-domain` —
  rows 1–4 of §3 deliberately do not consume it today, and a domain
  constant no domain code uses would be dead API.

## 5. Bounded-convergence spec (the stale-token / 412 family)

All paths below are **per cycle**; the cycle backoff (existing,
`failure_streak`) is the outer bound — nothing here loops.

1. **Guarded PUT → 412** (`publish_appdata` / `rewrite_own_appdata`):
   today one GET disambiguates and stops. **Change:** the 200 arm of
   `disambiguate_own_412` gains a *bounded re-resolution*: GET the
   own path, fold the served document into `st.docs` (as today), then
   re-attempt the guarded write **exactly once** with the refreshed
   token if the re-folded view still differs from served (a genuine
   convergence attempt, not a spin). A second 412 on that retry ⇒
   typed `ForkDetected` stop-write (existing outcome, reused — no new
   error variant needed).
   - Retry bound: **1 re-resolution per cycle** (`MAX_412_RESOLUTIONS
     = 1`, module constant, next to `SKEW_BAND_SECS`).
   - Typed give-up: `CycleOutcome::Ok { published: true, … }` is *not*
     emitted on give-up; the fork path publishes
     `CycleOutcome::Ok` with the path in `forked_paths` (typed,
     structural — status consumers already render `forked_paths`).
     No silent drop: the fork alarm finding fires (`finding()`) and
     the path is in the user-visible fork set.
2. **Guarded PUT → 204 with retention signature**: verify GET per
   §2(a); mismatch ⇒ `ForkDetected` (typed). Converged ⇒ token
   refreshed, `published_hash` set as today.
3. **create-once → 412** (attach/adoption arms): unchanged — already
   adopts via one GET, no loop.
4. **Unchanged guarantee for the non-pathological path**: a PUT whose
   response etag differs from the pre-write token takes exactly
   today's code path (record_put, published_hash, pending_heartbeat)
   — zero added requests, zero behavior deltas.

## 6. The characterization wiremock oracle (committed FIRST)

Location: `crates/taskboard-sync-files-nextcloud/tests/client_wiremock.rs`
(existing harness) plus a new `tests/retained_etag_wiremock.rs` for
the cycle-level scenarios. All time is the harness's controlled
clock — no wall clock, no sleeps (standing law b).

**The modeled server (`MockServer` behavior spec):**
- State per path: `etag: String`, `body: Vec<u8>`,
  `last_write_second: i64` (from the mock's injected clock).
- `PUT` (guarded or not): if `second(now) == last_write_second` **and**
  `body.len() == stored.len()` ⇒ `204`, **etag unchanged** (retention).
  Otherwise ⇒ `204` with a fresh etag. Update body + `last_write_second`.
- `PUT` with `If-Match` ≠ current etag ⇒ `412` (always — retention
  never rescues a genuinely stale token; #695 row 2).
- `GET`/`PROPFIND` serve current etag + body verbatim.

**Scenarios (each names its expected typed outcome):**
- **S1 false-clean pull (red before the fix):** peer writes a
  same-second same-size appdata rewrite; dir etag retained; the peer
  heartbeat's `written_at` advances. Expect: the device is pulled
  (2 GETs) in the next cycle — proves revision-field precedence.
  Before the fix this scenario yields 0 GETs (the bug, observed as
  red).
- **S2 lost-update guard (red before the fix):** clone A and clone B
  both hold token E0 (A's write retained it); B's guarded PUT
  succeeds in the same second. Expect: B's verification GET detects
  the content mismatch ⇒ `ForkDetected` in B's `forked_paths` (and
  mirror on A) — no silent convergence.
- **S3 stale-token 412 bounded convergence:** stale token ⇒ 412 ⇒
  GET ⇒ re-fold ⇒ one re-PUT (succeeds) ⇒ `published`. And the
  give-up twin: second 412 ⇒ `ForkDetected`, no third PUT observed
  on the mock (request-count assertion — structural, not text).
- **S4 non-pathological twins:** every S1–S3 shape with a *later
  second* or *different size* reproduces today's behavior exactly
  (etag churn, no verify GET — asserted by request count) — the
  corpus-unchanged oracle at the wire level.

**Oracle discipline:** assertions are typed outcomes
(`forked_paths` membership, `CycleOutcome`, GET/PUT counts from the
mock's journal), never status text or log text (standing law a).

## 7. Corpus-unchanged guarantee method

1. **Characterization-oracle-first commit order** (round-plan
   binding): commit the wiremock harness + S1–S4 with the mock model
   alone. S1/S2/S3 are **red** at this commit only in their new
   scenarios; the *existing* suite (docker tier, actor matrix
   `matrix_1..n`, contract tests, `insta` snapshots) is green,
   untouched. Stage 2's client change then flips S1–S3 green without
   touching existing snapshots.
2. If any existing snapshot/test changes, the diagnosis path applies
   (standing law c): the diff must be explainable as a *direct
   consequence of rows 6/7 of §3* (e.g. an extra verify GET a test's
   fake must now answer). Anything else ⇒ stop and re-plan. Snapshots
   are never weakened; an `..._currently_pins_...` rename is the only
   sanctioned debt shape.
3. The live pins (`it_live.rs` F4/F5) stay `#[ignore]`d and
   **unweakened** — they still pin the contract the design relies on;
   the manual live-tier verification step against the hosted account
   is recorded on #695 at handoff (round-plan live-tier discipline;
   #117 is not a prerequisite).
4. `taskboard-domain` prop tests and chunk-S pins must pass
   verbatim — §3 rows 1–4 change nothing there, so any failure there
   is a chunk bug, not a corpus drift.

## 8. Open questions — all resolved

| Question | Decision |
|---|---|
| Which crate? | `taskboard-sync-files-nextcloud` (+ one Deck-actor site, §0). |
| Validator design | (b) revision-field precedence, with (a)'s verify GET as bounded sub-step (§2). |
| Tolerance constant | `SKEW_BAND_SECS: i64 = 2`, module-scope, twinned per crate (§4). |
| 412 retry bound | 1 re-resolution per cycle; typed give-up = existing `ForkDetected` path; request-count oracles (§5). |
| Wiremock model | same-second+same-size ⇒ 204 + retained etag; stale If-Match always 412 (§6). |
| Deck-actor row-5 change shape | the skip stays for equal validators, but the tolerance guard wraps the *equal* branch so a validator due for refresh within the band is re-fetched on the next scheduled cycle rather than trusted indefinitely — stage 2 pins this with a pause/advance test, and if the Deck REST surface makes the guard untestable without new seams, row 5 is recorded as no-change with a comment and a backlog issue instead (bounded fallback, no scope creep). |
| Payload format change? | None — `DOCUMENT_FORMAT` unchanged; heartbeat fields already on the wire. |

## 9. Risks

- **R1 — heartbeat absent/stale peers:** a peer that never writes a
  heartbeat falls back to pure ETag dirty detection (today's
  behavior) — documented leniency at the type, not a heuristic.
- **R2 — verify GET cost on the pathological path:** one extra GET
  per retained-etag write; normal cadence (poll-interval writes)
  never hits the signature. Bounded, and the non-pathological path
  adds zero requests (§5.4).
- **R3 — docker-tier drift:** if the docker Nextcloud also retains
  etags, the existing docker-tier pins may red. Diagnosis path §7.2;
  the wiremock model was written from the hosted curl evidence, so a
  docker red is *evidence about the server*, handled on the branch,
  never by weakening.
- **R4 — clock discipline:** the mock's `last_write_second` and the
  cycle's stamps must both come from controlled clocks; stage 3
  audits that no wall-clock read leaked into the new tests.
- **R5 — the Deck row-5 guard** (§8) is the softest item; its
  bounded fallback keeps the chunk shippable if Deck's surface
  resists.

## 10. Estimated diff

`cycle.rs` (+~90), `etag.rs` (+~60), `client.rs` transport journal
(+~20), `status.rs` (0–5), Deck `actor.rs` (+~15 or 0 per §8),
new `tests/retained_etag_wiremock.rs` (+~350), existing wiremock
harness additions (+~60), doc comments throughout. **~600–800 lines,
test-heavy**, confined per §0.

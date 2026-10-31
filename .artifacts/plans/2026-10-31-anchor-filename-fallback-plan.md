# [Plan] Anchor filename fallback — Deck 1.19.1 decode gap, C4 triage, #282 rider

- **Date**: 2026-10-31
- **Kind**: stage-1 chunk plan — backlog round 2, chunk 1.1 (three-stage
  protocol: this plan → implementation → review), per
  `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md` §2 chunk 1.1.
- **Issues**: #448 (p1, the decode gap) + #447 (the pins/test half) +
  #449 (C4 triage) + rider #282 (dead-board drain — verify decision in §5);
  closes into #436 (lane re-run) — **not** closed by this chunk.
- **Branch**: `fix/anchor-filename-fallback` (worktree
  `/home/viktoria/src/taskboard-WT-fix-anchor-filename-fallback`, cut off
  `origin/main` @ `3fb69d1b` after a green `scripts/worktree-doctor.sh`
  run: 244G free / 25G floor).
- **Tier records (normative evidence)**:
  `.artifacts/reports/2026-10-25-tier2-convergence-fixtures-tier-records.md`
  §F-A (the measured 1.19.1 listing shape) / §F-B (C4: no anchor within
  the deadline, DB-verified zero attachments).
- **Scope ceiling (dispatch non-goals)**: no anchor format change; no
  outbox changes; no CI workflow edits; production diffs allowed only in
  `model.rs` / `anchor.rs` / the C4 root cause's honest fix (and the
  flush-path tracing §4 designates as part of that work). Everything else
  test-only.

## 0. Verdict & decisions up front

The production fix is **small and confined to `model.rs`**: extend
`ExtensionInfo` with the measured `basename` field and rank the
decode-time filename fallback `fileName → info.basename → info.filename`.
`anchor.rs` needs **no change** — `stored_file_name`
(`crates/taskboard-sync-nextcloud/src/anchor.rs:116`) already prefers the
normalized `extended_data.file_name` over the `data` payload, and
normalization at both decode paths (D20's contract: every decoded
`ExtendedData` answers `.file_name` with the stored filename) repairs the
scan, the wiremock S-series, and the docker `anchor_exists_on_server`
wait (`tests/it_docker.rs:660`) through the same production decode path.

| # | Decision | Recorded in |
|---|---|---|
| D1 | Ranking lives in `normalize_file_name` (model.rs); `ExtensionInfo` gains `basename: String`; `anchor.rs` untouched | §2 |
| D2 | Pins: recorded 1.19.1 listing fixture + decode test; ranking cases appended to the existing echo test; one wiremock S-series test on the 1.19.1 shape; the 1.17.5 echo pin stays and must demonstrably pass | §3 |
| D3 | C4 triage protocol with committed durable flush-ladder debug events + a test-only tracing subscriber; decision tree budget-exhaustion vs. write-500s vs. decode-mismatch vs. gate-never-fires; the "budget exhaustion" hypothesis is *a priori* weak (math in §4) | §4 |
| D4 | #282 rider: **verify-on-rig only** — a typed-error wire probe of Deck's write answers on a deleted board; **no fix this chunk** (out of the scope ceiling; p3; no outbox changes) | §5 |
| D5 | C1/C2 acceptance hardened with a test-only *one-anchor-card* assertion (the stage-3 trap: adopted, not re-created) | §6 |
| D6 | Two-tier run matrix + evidence format: two-tier matrix table in the stage-2 report, per the tier-records template, including the gate-tier export-prefix amendment | §6 |

## 1. Background (what the tier records established)

The C-series' first past-the-boot execution (2026-10-25) exposed two
distinct production-side anchor failures:

- **F-A (Deck 1.19.1, default tier)** — C1/C2 hang at the
  anchor-confirmation wait. The anchor card **is** created with
  `attachmentCount: 1`; the scan never confirms it. Measured wire shape
  of the attachment on a 1.19.1 card-detail listing (§F-A, verbatim):

  ```json
  {
    "id": 8, "type": "deck_file", "data": "taskboard-appdata.json",
    "extendedData": {
      "filesize": 278, "mimetype": "application/json",
      "info": { "dirname": ".", "basename": "taskboard-appdata.json",
                "extension": "json", "filename": "taskboard-appdata" }
    }
  }
  ```

  No `extendedData.fileName`; `info.filename` is the basename **without
  extension** (PHP `pathinfo` semantics). `normalize_file_name`
  (`model.rs:389`) folds `info.filename` → `"taskboard-appdata"`
  (non-empty), so `stored_file_name` (`anchor.rs:116`) prefers it over
  the correct `data` string `"taskboard-appdata.json"` and the
  `ANCHOR_ATTACHMENT_FILENAME` comparison never matches.
  **Production impact candidate**: the scan runs in the pull path of
  every sync cycle — on 1.19.1+ the anchor is re-created instead of
  adopted; a long-lived board accumulates unconfirmed anchor candidates.

- **F-B (Deck 1.17.5, gate tier)** — C4
  (`it_nextcloud_docker_deck1175_scheduled_fallback_convergence`) hangs
  at the same wait, but DB inspection shows **zero attachments and no
  anchor card**: the scheduled-fallback flush never produced the anchor
  within the 30 s deadline. Actor tracing was not captured; candidates
  per #449: sporadic 1.17.5 write-path `Server(500)`s, or a flush-path
  decode mismatch. Triage protocol in §4.

## 2. D1 — the `ExtensionInfo.basename` extension & the exact fallback ranking

**File: `crates/taskboard-sync-nextcloud/src/model.rs` only.**

1. `ExtensionInfo` (model.rs:424) gains a second field:

   ```rust
   pub struct ExtensionInfo {
       #[serde(default, rename = "filename")]
       pub file_name: String,
       /// The full stored filename *including* extension — Deck 1.19.1's
       /// listings carry it (measured, tier records §F-A); PHP pathinfo
       /// semantics, where `filename` is the basename *without*
       /// extension. Deliberate leniency: absent on the measured 1.17.5
       /// create echo, where the stored name rides `filename` instead.
       #[serde(default, rename = "basename", skip_serializing_if = "String::is_empty")]
       pub basename: String,
   }
   ```

   `String` + `#[serde(default)]` matches the house `ExtendedData` field
   style (A1: fallible minting is not needed — the trust boundary is the
   scan comparison, reached only through the ranked fallback);
   `skip_serializing_if = "String::is_empty"` keeps every serialization
   byte-identical to today (the upload path serializes
   `AttachmentUpload`, a separate struct — `ExtensionInfo` only ever
   echoes back inside `Attachment` objects the server manages).

2. `normalize_file_name` (model.rs:389) becomes the ranked fallback —
   **only when `file_name` is empty**, take the first non-empty of
   `info.basename`, then `info.filename`:

   ```rust
   fn normalize_file_name(mut data: ExtendedData) -> ExtendedData {
       if data.file_name.is_empty()
           && let Some(info) = &data.info
       {
           let ranked = if info.basename.is_empty() { &info.file_name } else { &info.basename };
           data.file_name = ranked.clone();
       }
       data
   }
   ```

   (exact code shape at stage 2's discretion; the *contract* is fixed:
   the three ranks in that order, each taken only when non-empty, and
   `fileName`-present never overwritten — the stage-3 functional trap.)

3. Both decode paths are covered by construction: `data_from_object_or_string`
   (model.rs:357, object form of `Attachment::data`) and
   `extended_data_normalized` (model.rs:376) both call
   `normalize_file_name`. The string form of `data` ("the string *is* the
   filename") is unaffected.

4. `anchor.rs` is **not** modified: `stored_file_name`'s preference
   (normalized `extended_data` → `data` payload) is correct once the
   extended-data value is the true stored name. The
   `scan_falls_back_to_the_data_payloads_file_name` unit test (anchor.rs:355)
   already pins the `data` fallback and must keep passing.

**Why ranking basename first is safe for 1.17.5**: the fallback only
fires when `fileName` is absent/empty. On the measured 1.17.5 create
echo, either `info` is empty (the recorded
`tests/fixtures/deck/attachment_create_response.json` has `"info": {}` —
the name then correctly comes from the `data` string, unchanged) or
`info.filename` alone carries the stored name (the unit pin at
model.rs:650) — in both cases `basename` is absent or empty, so rank 3
(`filename`) is taken exactly as today. On 1.19.1 listings `basename` is
present and wins over the extension-less `filename`. The one unverified
corner — whether 1.17.5 *listings* also carry `basename` — is made
irrelevant by the ranking (basename present ⇒ it is the full stored name
by pathinfo semantics; absent ⇒ today's behavior), and the C4 rig session
captures the real 1.17.5 listing JSON as bonus evidence (§4 step 5).

## 3. D2 — the measured-shape pins at tier 1 (TDD-first)

TDD order: **the failing pin lands first**, then the fix flips it green.
Every commit standalone (rebase-merge hygiene, AGENTS §8).

1. **Commit 1 (RED) — the recorded 1.19.1 listing pin.**
   - New fixture `tests/fixtures/deck/attachments_list_1_19_1.json`: an
     attachments-list envelope carrying the §F-A attachment **verbatim**
     (including `dirname`/`extension` — proving unknown-`info`-key
     tolerance — and the string `data` payload).
   - New model test `attachments_list_1_19_1_fixture_decodes_stored_name`:
     decodes through `decode_envelope`, asserts
     `extended_data.file_name == "taskboard-appdata.json"` (the desired
     semantics — fails today with `"taskboard-appdata"`, which is the
     bug reproduced at tier 1) and asserts `data.file_name` matches the
     string payload. The scan-level fact (the reserved-filename
     comparison matching) is pinned by the existing `scan_*` unit tests
     plus the new wiremock S-pin in §3.4 — no private-fn reach-in from
     the model tests.

2. **Commit 2 (GREEN) — the fix** of §2 (model.rs only).

3. **Commit 3 — ranking cases appended to the existing echo pin.**
   Extend `echo_decode_normalizes_both_server_shapes` (model.rs:636)
   with the 1.19.1 ranking pair:
   - `fileName` absent + `info` carrying both `basename` (full name) and
     `filename` (no extension) ⇒ `file_name` is the **basename** value;
   - `fileName` present **and** `info.basename` present ⇒ `fileName`
     wins (the stage-3 trap: never prefer basename over a present
     `fileName`);
   - `info.basename` present but empty string ⇒ falls through to
     `info.filename` (the recorded 1.17.5 echo-with-info case keeps
     passing verbatim — the compatibility contract).

4. **Commit 4 — the wiremock S-series pin on the 1.19.1 shape.**
   In `tests/actor_wiremock.rs`, extend the `sidecar` module with a
   1.19.1-shape detail builder (same fields as
   `detail_json_with_attachment`, but `extendedData` per §F-A: no
   `fileName`, `info` with `basename`/`filename`/`dirname`/`extension`,
   `filesize`) and **one new S-series test** running the full
   actor cycle (scan → confirm → `Use` ladder) against that shape —
   proving anchor confirmation through the ranked fallback at the actor
   tier, deterministically, without docker. Do **not** fork every
   existing S-test per shape: the 1.19.0 shape stays the suite default;
   the 1.17.5 compatibility contract stays pinned at unit level
   (commit 3) and live on the gate tier (§6).

5. **Commit 5 — the adopted-not-re-created assertion (D5).**
   In `tests/it_docker.rs` (test-only), harden C1/C2
   (`app_field_convergence` / `rule_convergence`): after the anchor wait
   succeeds, list the board once and assert **exactly one** anchor-titled
   card carrying exactly one `taskboard-appdata.json` attachment
   (structural counts only — no text assertions beyond the existing
   convention constants). This is the executable form of the dispatch
   acceptance "anchor adopted, not re-created" on a fresh board, and it
   guards the production-impact candidate (accumulating anchor
   candidates) against regressions.

No changes to `contract.rs` mounts or the 59-test wiremock suite's
existing expectations; the two new tests are additive.

## 4. D3 — the C4 triage protocol (#449)

C4 fails *differently* from C1/C2: the anchor is never created (F-B:
zero attachments, no anchor card in the DB). The protocol below
discriminates the four candidate causes with evidence; stage 2 fixes the
root cause TDD **only if it lands in production code** (dispatch
prompt 1.1.b item 3), else characterizes on the record.

### 4.1 Tracing (two kinds, two commit treatments)

- **Durable (committed, part of the C4 work — allowed production diff**:
  flush-path observability in `src/flush.rs`, `tracing::debug!`,
  field-based, no secrets, no text an assertion would match on):
  - the work-gate verdict: `has_flush_work` result with the deciding
    clause (`group queued` vs. `rebuild trigger` vs. `idle`);
  - the resolved `AnchorAction` variant and ladder variant;
  - per-ladder-step outcomes in `upload_with_create` /
    `upload_onto`: which step, typed error variant on failure
    (`?err`/`%err` is fine — the existing warn! sites at flush.rs:469/473
    already carry `%err`);
  - the `FlushVerdict` per cycle.
  The actor already logs cycle start/complete and the failure path
  (actor.rs:489/740/799); pull-stage per-cycle failures are covered by
  the existing `sync cycle failed` warn — add nothing there.

- **Session-only capture (test-only commit)**: the docker harness gains
  a subscriber init gated on an env var (`TASKBOARD_IT_TRACE=1` ⇒
  `tracing_subscriber` fmt to stderr, honoring `RUST_LOG`), in
  `tests/common/mod.rs` or the fixture boot path. Capture runs with
  `RUST_LOG=taskboard_sync_nextcloud=debug TASKBOARD_IT_TRACE=1`,
  output redirected to a file. **Uncommitted verbose
  `eprintln!` localization stays uncommitted** (the tier-records
  precedent: temporary markers, reverted before the final diff).

### 4.2 What to capture (per C4 run, gate tier)

1. The full trace file for the 30 s window (excerpted into the stage-2
   report; the report names the reproduction command).
2. Leftover server state after the run: the board's cards +
   attachments queried in-container (the F-B method — the board is torn
   down only after evidence is captured).
3. The real 1.17.5 **listing** attachment JSON for an anchor-shaped
   attachment (created by the f-series lifecycle run) — extends the
   measured record; resolves the §2 corner with wire evidence.
4. The C4 fixture's own stage markers: the existing boot/set-field
   asserts, plus which of the four outcomes below the traces show.

### 4.3 Decision tree (the four outcomes, and what each means)

Let the traces answer, in order:

1. **`has_flush_work == false` on every cycle** (no `group queued`, no
   rebuild trigger) — the scheduled lane never arms the flush on 1.17.5
   (e.g. the `card_startdate == false` gate or group formation). 
   *Production-code territory* → fix TDD in this chunk (flush.rs /
   actor.rs, the C4 allowance), root cause named in the report.
2. **Ladder runs, writes answer `Server(500)`** on `create_card` /
   `upload_attachment` — the known sporadic 1.17.5 write-path 500s.
   Discriminate *sporadic* from *systematic* with the math: client
   backoff is 3 attempts/call (500 ms + 1 s worst case, backoff.rs:22),
   the fixture actor polls every 300 ms with 200→800 ms failure backoff
   (sync_pair.rs:15) ⇒ **≈30–100 flush attempts inside the 30 s
   deadline**; a genuinely sporadic 500 cannot consume that budget.
   Every-attempt-500 ⇒ **environment/server-side characterization**
   (image bug), recorded on #449 with the traces; the poll loop already
   retries across cycles and no outbox/backoff change is in scope. Do
   not stretch the chunk.
3. **Upload lands (DB shows the attachment) but the cycle treats it as
   failed / the next scan cannot confirm** — a flush-path decode
   mismatch on the 1.17.5 echo (e.g. `attachment_stamp` returning
   `None`, or the confirmation decode). *Production-code territory* →
   fix in model.rs (in scope), pin the 1.17.5 echo shape that misbehaved.
4. **Cycles fail before the flush** (pull-stage failures every cycle,
   actor failure backoff visible) — characterize what the 1.17.5 pull
   hits (capability probe? listing?) and record on #449; fix only if the
   root cause is in the sync crate and small (the C4 allowance), else
   characterize.

C4's disposition is **evidence-backed either way** (stage-3 trap:
traces in the report, not assertions). The a-priori ranking of the
candidates, given F-B's zero-attachment DB state and the §4.3-2 math:
*gate never fires* (outcome 1) or *systematic write-500s* (outcome 2)
are the live hypotheses; *decode mismatch* (outcome 3) is unlikely
(f-series attachment lifecycle passes on the gate tier, tier records
D4 matrix) but not excluded for the anchor-shaped payload.

## 5. D4 — the #282 rider: **verify-on-rig only**

**Decision: verify, do not fix, this chunk.** Grounds: (i) the write-side
dead-board carve-out would touch the push executor's classification
(`mapping::classify_push` / `push_exec`) — a production diff outside the
scope ceiling (`model.rs`/`anchor.rs`/C4 root cause), and dispatch
non-goals exclude outbox changes; (ii) #282 is p3/unscheduled and its own
proposed approach says "verify Deck's actual write-answer on the tier-2
rig first"; (iii) the reads side is already verified and carved out
(actor.rs:1171–1191, "All reads of a dead board answer 404/403 —
tier-2 verified"); only the **write** answers are unmeasured.

**Probe protocol** (while the rig is up; ~15-line test-only docker
probe, `#[ignore]`d like its siblings):

1. Create a board (raw `DeckClient`), delete it (typed `delete_board`).
2. Against the deleted board id, attempt one write of each queued-op
   kind #282 concerns: create a card (the task-op proxy), create a
   stack, write a label — each via the raw client.
3. Assert only **typed decodability**: each call completes with a typed
   `DeckError` variant (whichever it is — no variant pin, no text
   assertions; pinning today's 403-vs-404 would be a "today's behavior"
   pin, AGENTS §5).
4. **Record the concrete variants** in the stage-2 report and in the
   #282 comment stage 3 posts (evidence, not contract): per kind, the
   observed variant(s), plus the executor consequence (which
   classification the current `classify_push` table yields — retry vs.
   finalize — read off the existing table, no code change).

If the rig session runs short, the downgrade is a manual probe session
(curl/`DeckClient` REPL) recorded in the report — the acceptance needs
#282 *verified*, not the probe test. #282 stays open either way (stage-3
prompt: close only if fixed).

## 6. D5/D6 — the two-tier run matrix & evidence format

### 6.1 Bring-up (per tier records, incl. the amended export prefix)

- Default tier: `taskboard-it-nextcloud`, port 8370 —
  `eval "$(scripts/nextcloud_it_setup.sh up)"` (re-mint credentials per
  run). Deck 1.19.1 / Nextcloud 35.
- Gate tier: `taskboard-it-nextcloud-deck1175`, port 8371 —
  `DECK_VERSION=v1.17.5 TASKBOARD_IT_NC_IMAGE=nextcloud:33-apache`
  with suffix `-deck1175`, **and `EXPORT_PREFIX=TASKBOARD_IT_DOCKER_DECK1175`**
  (tier-records deviation 2 — without it the gate run is four
  0.006 s false-pass skips). Deck 1.17.5 / Nextcloud 33.
- The tiers are shared with parallel batch-1 chunks (1.1 owns the sync
  crate lane): **never `down` a tier**; leave both up at handoff.

### 6.2 Matrix

| Tier | Runs | Expectation |
|---|---|---|
| Default (1.19.1) | C1 `…_sync_app_field_convergence`, C2 `…_sync_rule_convergence` | **pass** — anchor confirmed and **adopted** (one anchor card, §3 commit 5) |
| Default (1.19.1) | C3 `…_sync_echo_idempotence` (untouched adjacent), `…_attachment_content_lifecycle`, `…_startdate_round_trip` | pass — no echo/lifecycle regression |
| Gate (1.17.5) | C4 `…_deck1175_scheduled_fallback_convergence` | pass, **or** evidence-backed characterization per §4.3 |
| Gate (1.17.5) | `…_deck1175_attachment_content_lifecycle`, `…_deck1175_startdate_unsupported`, `…_deck1175_capabilities` | pass — untouched guard rows |
| Hermetic (both) | `cargo nextest run -p taskboard-sync-nextcloud` (unit + contract + wiremock + e2e_hermetic; the two new tier-1 pins inside) | all green |
| Workspace | full AGENTS §10 gate suite **via `scripts/run-quality-gates.sh`** (shared lock — parallel batch-1 box) | green |

Selective docker invocations:
`cargo nextest run -p taskboard-sync-nextcloud --run-ignored only -E 'test(<name>)' --no-fail-fast`.

### 6.3 Evidence format

Stage-2 report at
`.artifacts/reports/2026-10-31-anchor-filename-fallback-report.md`, per
the tier-records template:

- header (date, branch/HEAD, commit list, zero-unrelated-production-diff
  statement: the only production files touched are `model.rs` + the §4
  flush tracing + any C4 root-cause fix, named);
- the **two-tier matrix table** (test / outcome / duration) with the
  honesty check — any ≈0.006 s docker row is a false-pass skip and is
  discarded and re-run with the correct export prefix;
- C4: the §4.2 captures (trace excerpts, leftover-DB state, the 1.17.5
  listing JSON), the §4.3 outcome number, and the disposition
  (fixed-in-chunk with the fix's own TDD pins, or characterized);
- the #282 probe results (per-kind observed variants + executor
  consequence);
- deviations section;
- stage 3 lifts the two-tier records into the PR description and the
  per-issue evidence comments (#447, #448, #449; #436 gets a comment,
  #282 gets the probe comment and stays open).

## 7. Commit sequence (stage 2, TDD-first, each standalone)

1. `test(sync-nextcloud): pin the measured Deck 1.19.1 attachment listing shape` — RED (§3.1)
2. `fix(sync-nextcloud): rank the attachment filename fallback fileName → info.basename → info.filename` — GREEN (§2)
3. `test(sync-nextcloud): the ranked fallback keeps the 1.17.5 echo and a present fileName winning` — ranking pins (§3.3)
4. `test(sync-nextcloud): the 1.19.1 listing shape confirms the anchor through the actor scan` — wiremock S-pin (§3.4)
5. `test(sync-nextcloud): C1/C2 assert the anchor is adopted exactly once` — docker assertion (§3.5)
6. `feat(sync-nextcloud): durable flush-ladder tracing + the C4 capture harness` — §4.1 (split the test-only subscriber if cleaner)
7. C4 rig session → either `fix(sync-nextcloud): <named root cause>` (TDD) or the characterization-only report entry
8. `test(sync-nextcloud): probe Deck's write answers on a deleted board (#282)` (§5)

## 8. Acceptance (from the dispatch plan, made checkable)

- [ ] C1/C2 pass on Deck 1.19.1; the one-anchor assertion holds (adopted, not re-created)
- [ ] The 1.19.1 listing pin, the ranking pins, and the wiremock S-pin are green; the 1.17.5 echo pin demonstrably still passes (named in the report)
- [ ] C4 triaged with actor traces: fixed TDD, or characterized with the §4.2 evidence on #449
- [ ] #282 verified with the probe record (no fix)
- [ ] Zero unrelated production diffs (production touch list: model.rs, flush.rs tracing, optional C4 root cause — each named)
- [ ] Full gate suite green via the quality-gates wrapper; hermetic suite green
- [ ] No push, no PR from stage 2; worktree left for stage 3 (per prompt 1.1.b, stage 3 ships and tears down)

## 9. Risks

1. **The 1.17.5 listing shape is unmeasured** — the ranking is safe by
   construction (§2), and the C4 session captures the real JSON; if it
   contradicts the ranking's assumption (basename ≠ full stored name on
   some version), that is a new measured fact amending D2 — stop and
   record, do not silently re-rank.
2. **C4's root cause may be environmental** (systematic 1.17.5 500s) —
   the chunk then ends characterized-with-evidence, which the dispatch
   acceptance explicitly allows; the §4.3-2 math guards against chasing
   "budget exhaustion" that the attempt arithmetic cannot produce.
3. **Gate-tier false passes** — the export-prefix trap (§6.1); the
   honesty check in §6.3 makes any skip visible in the record.
4. **Shared rig contention** — batch-1 chunks share both tiers; runs are
   short and the crates' tests use per-run boards (`run_id()`), but a
   concurrent chunk's bring-up may re-mint credentials mid-run; re-run
   the affected row rather than diagnosing cross-talk.
5. **`anchors_on_server` wait depends on the same decode** — the C1/C2
   fixture wait decodes through the production path, so the fix repairs
   it; if it still fails after a green wiremock S-pin, that is a *new*
   finding (listing vs. detail shape divergence) — capture the JSON and
   treat as risk-1.

## 10. Estimated diff size

- Production: `model.rs` ≈ +20 (field, ranking, doc comments);
  `flush.rs` ≈ +15–20 (durable debug events); optional C4 root cause
  (unbounded until §4.3 speaks). **`anchor.rs`: 0.**
- Test-only: fixture JSON +1 file; model tests ≈ +50; wiremock S-variant
  + one test ≈ +40; C1/C2 assertion ≈ +15; tracing subscriber ≈ +20;
  #282 probe ≈ +40.
- Total ≈ **200–260 lines** (M), of which production ≈ 35–40 plus any
  C4 fix.

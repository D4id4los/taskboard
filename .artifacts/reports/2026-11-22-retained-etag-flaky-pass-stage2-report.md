# Chunk B1 stage-2 report — the retained-ETag flaky pass (#1165–#1170, #1214)

- **Date**: 2026-11-22
- **Branch**: `test/retained-etag-flaky-pass` (worktree
  `../taskboard-WT-test-retained-etag-flaky-pass`), base `5e13577c`,
  stage-1 plan `5afc7405`.
- **Landed commits**:
  - `e9000350` — `test(sync-files-nextcloud): keep the harness's
    MockServer alive for the test's lifetime`
  - `fe41db55` — `fix(ui-slint): derive the track-form parse anchor
    through the frozen-clock seam (#1214)`

## 1. The discriminator verdict — both plan hypotheses corrected

The stage-1 plan chose **H-replay** (stale pooled keep-alive connection
→ hyper replay → duplicated state-changing writes on the stateful
model) with **H-port** (cross-test traffic via port reuse) as fallback.
The mandated capture (§4) was run pre-fix and **falsified H-replay and
confirmed the port-reuse class**, with a mechanism neither hypothesis
spelled:

**The harness dropped its `MockServer` handle** at the end of
`Harness::new` (the struct's fields hold `server` and `transport` but
not `mounted`). Dropping the handle starts wiremock's async server
shutdown *mid-test*: the listener closes while the test is still
cycling, the OS hands the ephemeral port to a concurrently starting
suite member's `MockServer`, and the test's remaining requests land on
the *other* test's stateful `RetentionServer`. Every observed symptom
follows: the second attach's `PUT identity.json` (`If-None-Match: *`)
hits the foreign model's already-created file → **412 → typed attach
failure** (#1165/#1166/#1169/#1170); foreign journals **inflate or zero
the exact request-count assertions** (#1166/#1167/#1168); foreign
models **404 seeded paths** (`resource not found`); ForkDetected fires
on the device's own path. Load-probability, not mechanism, explains
1/12 historically vs 12/12 on the day (box load ~13 from the parallel
round).

**Capture evidence** (all temporary eprintln instrumentation, reverted
before landing):

- Per-request/per-thread capture: port `43817` received attach ladders
  from two distinct test threads (33 and 36); port `43795` from
  threads 34 and 35.
- `RUST_LOG=hyper=trace,reqwest=trace` with a temporary subscriber:
  **zero transport-layer errors and zero retries from any layer** —
  with the pool disabled *and* enabled. The planned
  `with_pool_disabled` seam was implemented first and **failed to fix
  the class** (4–6 failures per run with the seam active) — the
  falsification that forced the deeper capture.
- The single-test (serial) configuration never reproduced: the class
  requires in-process parallelism, consistent with all observations.

## 2. Reproduction counts

- **Pre-fix**: N=12, **12/12 failing** (issue-matching signatures at
  the pinned assertion sites; logs `/tmp/b1-prefix-run-*.log`,
  summary `/tmp/b1-prefix-results.txt`).
- **Post-fix (the landed MockServer-lifetime anchor)**: N=30,
  **30/30 passing, zero failures** (`/tmp/b1-postfix-results.txt`),
  plus 12 informal verification runs — all clean — under the same
  load (~13) that produced 12/12 failures pre-fix. Floor-style
  acceptance; no ceiling asserted.
- nextest (process-per-test) expected clean both sides — not gated on,
  per the plan.

## 3. The fix (deviation recorded)

**Deviation from the plan's §3**: the pool-disable seam is **not**
landed — it does not touch the mechanism (evidence above), and landing
it would be speculative generality. The landed fix is harness-only and
structural to the actual mechanism: the harness now holds a
`_mounted: MockServer` field, so the server cannot die before the test
does. ~11 lines, **zero assertion changes** (law c holds byte-for-byte
— `git show e9000350` touches only the struct field, the constructor
expression, and the comment). No sleep, no yield, no tolerance (law b
trivially holds). The S4 twins and every sibling suite share the
harness pattern's *fix* only where they hold handles — sibling suites
that also drop their `MockServer` handle are the same class and are
filed for the sweep (see §5).

## 4. #1214 — landed exactly per plan §6

`commit fe41db55`: the track-form parse anchor (`today` in
`seed_track_form` and `commit_track_form`) derives through the
adapter's `frozen_now` / `UiSettings::clock` jurisdiction (#660 seam,
#921 precedent); production hands `None` and reads the wall clock
identically. The flaky test's fixture anchors to
`test_platform::PINNED_NOW` via the new `frozen_clock_rig` helper —
deterministic by construction. Separate commit, separate closure
comment (issue comment 7082156443). The `track` test family (43
tests) passes.

## 5. Backlog issues opened

None new — the out-of-scope finding (sibling wiremock suites that also
drop their `MockServer` handle share the class) is recorded for the
dispatcher: the `client_wiremock` suite's harness was checked and holds
its handle; other suites (`taskboard-sync-deck`, app-level wiremock
suites) were **not** audited — recommend a `backlog:` follow-up if
stage 3 wants a repo-wide sweep. *(Stage 3: run `gh issue list
--search "MockServer"` first — none existed at stage-2 end.)*

## 6. Issue comments (dispatcher owns the at-merge closures)

- #1165 comment 7082148264, #1166 comment 7082148553, #1167 comment
  7082148796, #1168 comment 7082149029, #1169 comment 7082149264,
  #1170 comment 7082149583 — the corrected mechanism, the capture, the
  counts, the fix.
- #1214 comment 7082156443 — the seam disposition.
- Standbys #697/#650/#968: not touched (plan §7 — stage 3's re-check).

## 7. Gates

- Pre-commit hooks (fmt, slint token lint, memory-gated workspace
  clippy `-D warnings`): green on both commits.
- `scripts/run-quality-gates.sh cargo nextest run --workspace`:
  **green — 5181 passed, 65 skipped, exit 0** (Summary 62.9s wall,
  flock held via the wrapper).

## 8. Round-law compliance

(a) typed outcomes throughout — no text assertions added; (b)
deterministic by construction — the fix removes the socket-lifecycle
race structurally, no sleep/yield/tolerance anywhere in the diff;
(c) the band tests' convergence contracts are byte-identical (only the
harness struct/constructor changed); (d) no out-of-scope work
discovered requiring an issue (see §5's note for stage 3).

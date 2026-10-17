# [Plan] Flaky-Test Remediation — Watchlist #287/#289/#290

- **Date**: 2026-10-17
- **Kind**: implementation plan (post-investigation, **awaiting user
  acceptance**; per the artefact rules a dispatch plan with three-stage
  prompts is only cut *after* acceptance, if the user wants the chunks
  dispatched rather than executed directly).
- **Origin**: user directive of 2026-10-17 (investigate the flaky-test
  issues, find reasons, propose a fix plan); investigation report
  `.artifacts/reports/2026-10-17-flaky-test-watchlist-investigation-report.md`
  (all root-cause evidence lives there — this plan does not re-derive it).
- **Branch / worktree for execution**: one branch/PR per chunk (§3–§5
  name them), created off `main` at execution time; parallel worktrees
  per house convention `../taskboard-WT-<branch-with-dashes>`.
- **Status**: ACCEPTED (user, 2026-10-17 — plan extended in the same
  round with chunk 4 and the #288 closure task; kickoff prompt issued).

## 1. Root causes (one line each, full evidence in the report)

- **F-1 (#290, #287/a10, likely #287/2026-10-11)**: the sync crate's
  hermetic wait helpers bound their loops on the *paused* tokio clock
  (6k–12k iterations ≈ 0.4–0.9 s wall) while the actor progresses via
  real wiremock I/O — under CI contention the virtual budget expires
  before the I/O chain completes.
- **F-2 (#287/coverage lane)**: the `refused_client` fixture assumes a
  dropped listener's port still refuses; a concurrently started
  `MockServer` can win the port in the drop→connect window and turn the
  expected abort into a 404-rejected outcome.
- **F-3 (#289)**: host/kernel-level intermittent `EACCES` on fresh /tmp
  tmpfs paths, local-libtest-parallel only; not a code or test defect —
  nothing to fix in-repo; encode a capture/escalation protocol instead.

## 2. Scope, hard rules, non-goals

**Hard rules** (binding on every chunk):

1. Watchlist discipline: no `#[ignore]`, no retry-tolerance, no
   weakening of any predicate. A flaky test's *assertion* stays exactly
   what it asserts; only the *wait mechanics* around it change.
2. No nextest retry configuration as a "fix" (rejected: it converts a
   finding into a masked defect; the coverage-flake precedent fixed the
   cause and kept the gate honest).
3. Determinism property preserved: paused-time stepping stays the *only*
   place virtual time advances (module docs in both suites promise actor
   ticks fire exactly while waiting — never between assertions).
4. AGENTS §5 compliance on any new test code: no text assertions in
   tests, deterministic by construction, no sleeps.
5. Every chunk passes the full quality gate before handoff (§10 list).
6. Every chunk that closes an issue updates the issue (comment with the
   verdict + evidence permalink; close only when the fix has its soak
   evidence, mirroring #288's closure pattern).

**Non-goals:**

- No production-code changes (client timeouts, actor loop, bootstrap
  are all out of scope; the flake families are test-infrastructure and
  environment).
- No CI lane changes beyond what #288 already shipped (quarantine
  script stays as is).
- No in-repo change for #289 (F-3 is environmental; §5 defines the
  protocol-only work).

## 3. Chunk 1 — hybrid wall-clock deadlines in the hermetic wait helpers

**Fixes**: F-1 → #290, the a10 occurrence of #287, and (plausibly) the
2026-10-11 instrumented occurrence of #287.
**Branch**: `test/sync-hermetic-wall-deadlines` — one PR.

**Change.** In the four wait helpers, keep the virtual stepping but bound
the loop on `std::time::Instant` (wall clock). The four sites (complete,
per the report §4.1):

1. `crates/taskboard-sync-nextcloud/tests/e2e_hermetic.rs:128` —
   `eventually`: replace the `tokio::time::Instant` deadline with
   `let deadline = std::time::Instant::now() + Duration::from_secs(30);`
   and the assert condition accordingly; keep
   `tokio::time::advance(Duration::from_millis(5)).await;` per iteration.
2. `crates/taskboard-sync-nextcloud/tests/e2e_hermetic.rs:827` —
   `eventually_no_state`: same transformation (10 ms step is 5 ms here —
   keep 5 ms).
3. `crates/taskboard-sync-nextcloud/tests/common/hermetic.rs:116` —
   `start_server`: same, keep the 10 ms step.
4. `crates/taskboard-sync-nextcloud/tests/actor_wiremock.rs:62` —
   `next_report`: same, keep the 10 ms step.

**Semantics.**

- Deadline = **30 s wall** per wait (green runs complete in
  0.02–3 s today, instrumented included; 30 s caps a wedged test without
  slowing honest failure). If a chunk reviewer prefers a different
  constant, it lives in one `const WALL_DEADLINE: Duration` per suite —
  no magic numbers at use sites.
- The predicate is unchanged: it must still *become* true. The loop only
  stops lying about how much patience it has (a virtual minute was
  ~0.4 s of real patience).
- Fix the inverted panic wording while touching the lines:
  "predicate **not** satisfied within the deadline" (and the
  corresponding strings in `next_report`/`start_server`) — report §5.3.
- On deadline abort, include one structural fact in the panic payload to
  make any future failure self-diagnosing (report §6.3): for
  `eventually`, the current `sync.phase`; for `next_report`, whether the
  reports channel is still open. Structural state only — no message
  text assertions involved.

**Why wall clock is AGENTS-compliant**: §5 mandates "poll a predicate
with a deadline" — a deadline is only a deadline if it is measured in
time that the blocking resource (OS I/O) consumes. The paused-clock
stepping stays, so "deterministic by construction" (ticks fire only in
the wait loop) is intact; the wall budget bounds the *honest* failure
mode instead of manufacturing false ones.

**Acceptance criteria.**

- `cargo nextest run -p taskboard-sync-nextcloud` green; full gate green.
- Both suites' module docs updated: the "determinism" paragraphs gain one
  sentence stating the deadline is wall-clock while the stepping is
  virtual.
- **Soak protocol (the acceptance test for the fix)**: `workflow_dispatch`
  CI runs on the PR branch — ≥ 5 consecutive green `Test Execution` +
  `Code Coverage` runs, mirroring the #288 closure bar. Record the run
  IDs in the PR description.
- The i1/i2/a10 trio specifically: 3 local repetitions of each suite in
  the highest-contention shape available locally (`cargo nextest run
  --workspace` while a `cargo llvm-cov nextest` runs in a second
  worktree) — green throughout (no false pass is possible: the
  predicates are unchanged).

**Effort**: small (one file plus two suite files, ~40 lines net).
**Risk**: low — worst case a genuinely wedged actor now takes 30 s to
fail instead of 0.4 s; a hang is still bounded (vs. an unbounded wait).

## 4. Chunk 2 — `refused_client` precondition hardening

**Fixes**: F-2 → the coverage-lane occurrence of #287.
**Branch**: `test/push-exec-refused-port-precondition` — one PR, may be
stacked on chunk 1 (different files; independent either way).

**Change.** In `crates/taskboard-sync-nextcloud/src/push_exec.rs` test
module, replace the blind `bind(0)` → `drop` sequence with a
checked-and-regenerated fixture:

1. bind port 0, read the port, drop the listener;
2. probe-connect (`std::net::TcpStream::connect_timeout` with a short
   timeout, run inside a blocking-compatible context — the tests are
   plain `#[tokio::test]`, so use `std::net` directly);
3. require the probe to fail with `ConnectionRefused` — the fixture's
   precondition, now verified rather than assumed;
4. on any other probe outcome (connect succeeded → the port was won by
   another process; timeout → half-open listener), increment a
   regeneration counter, pick a fresh port, retry — bounded (e.g. 10
   attempts) with a panic if exhausted;
5. log the regeneration (e.g. `eprintln!`/`tracing`) with the count —
   converts this flake from an unexplained failure into a counter
   (report §6.3).

The scenario tests (`create_stack_transport_failure_aborts`,
`transport_failure_aborts_without_outcomes`,
`transport_failure_after_a_landed_create_keeps_its_evidence`, and any
other `refused_client` callers) are untouched: same client, same
expected abort.

**Acceptance criteria.**

- Full gate green; the push_exec tests unchanged in assertions.
- A comment on #287 records the mechanism (report §4.2), the hardening,
  and the logging hook; the issue stays **open** (two of its three
  occurrence families are covered by chunks 1–2, but the 2026-10-11
  instrumented occurrence's attribution stays unconfirmed until chunk 1
  has soak time — close #287 only when both chunks have soaked and no
  new occurrence appeared; the watchlist comment says so explicitly).

**Effort**: small (~30 lines in the fixture).
**Risk**: near zero — regeneration is strictly more conservative than
the status quo.

## 5. Chunk 3 — #289 protocol + watchlist bookkeeping

**Fixes**: nothing in-repo for F-3 (environmental); makes the next
occurrence cheap to attribute and the eventual escalation turnkey.
**Branch**: `docs/flaky-watchlist-protocol` — one PR (artefacts only).

**Change.**

1. **Capture protocol** (append to the #289 issue, and as the plan's
   normative text): on any future occurrence — record (a) exact command
   and runner (libtest parallel / llvm-cov / nextest), (b) full failure
   output, (c) `strace -f -o /tmp/repl_strace_<n>.txt -e
   trace=%file,network,process cargo test -p taskboard-app --lib
   repl_history` of a failing run, (d) the host kernel
   (`uname -v`) and boot id. The strace signature to look for
   (established in the report §4.3): same-thread consecutive syscalls
   with the second returning EACCES on a fresh `.tmpXXXXXX` path.
2. **Decisive next experiments** (user-assisted, in order):
   a. **User-shell run** of the §4.3 loop command from the report —
      green ⇒ the agent exec pipeline reopens as a suspect (re-attack
      hypothesis 2); failing ⇒ host/kernel confirmed. Record the
      outcome as an issue comment either way.
   b. **Root-assisted audit** (needs sudo, one session): `journalctl -k`
      around a failing run for AppArmor/Landlock denials; if empty,
      `auditctl -a always,exit -F arch=b64 -S mkdir,openat -F success=0`
      during a failing loop to catch the denying hook with its
      `subj=`/`exe=` fields.
   c. **Kernel bisect** only if (a) confirms host-level and (b) is
      silent: `7.0.0-15 → 7.0.0-22` or upstream; out of repo scope,
      recorded as the terminal escalation step.
3. **Interim developer guidance** (one paragraph in the issue, not in
   AGENTS.md): on this host, prefer the gate commands (`cargo nextest
   run --workspace`, `cargo llvm-cov nextest …`) over raw
   `cargo test --lib` for app-lib suites; raw libtest remains a valid
   local convenience, and its failures with the §4.3 strace signature
   are this environmental family — re-run, don't hunt production bugs.
   (The tests are correct; nothing is skipped or weakened.)
4. **Watchlist comments on #287/#290** citing the investigation report:
   #290 — root cause + chunk 1 pointer; #287 — per-occurrence verdicts
   (a10: root-caused; coverage lane: hardened; 2026-10-11: attributed
   plausibly, pending soak), closure conditions (§4 acceptance).
5. **Close #288 as completed.** Its verdict is confirmed and its soak
   bar long met (five consecutive green CI runs at fix time, plus the
   audit-era non-recurrence data points incl. a troubled lane run
   surviving on quarantine protection — report §3 roll-call). Final
   comment: cite the 2026-10-13 verdict, the 2026-10-16/17 audit
   confirmations, the investigation report as the retrospective
   cross-reference, and chunk 4 as the follow-up that aligns the local
   gate with the CI fix. Per house convention the closure *is* the
   archive.
6. **Backlog issues** for the report's §5 discoveries:
   - `backlog: sqlite test pools leak -shm/-wal sidecar files into /tmp`
     (labels `testing`, `dx`, `p3`): 10,923 files recorded 2026-10-17;
     fix candidates: dispose pools before drop / teardown assertion.
   - optionally fold the three 2026-10-11 IPC tempdir leftovers into the
     same issue as a second instance.

**Acceptance criteria.**

- #289 carries the capture protocol + next-step ladder as comments; no
  test file changed; no issue closed.
- Backlog issues exist with body = summary + permalink to the report
  (house convention: deep context stays in the artefact).

**Effort**: docs/comments only.
**Risk**: none.

## 5a. Chunk 4 — align the local coverage gate with the CI fix (AGENTS.md)

**Fixes**: the residual local exposure of the resolved #288 family —
the documented local gate still runs the monolithic
`cargo llvm-cov --workspace`, whose merge step aborts on a torn profraw
(SIGKILLed instrumented child) with no quarantine on that path
(report §4 of the follow-up Q&A: "CI-side only, locally opt-in").
**Branch**: `docs/local-coverage-gate-quarantine` — one PR (AGENTS.md
is the only file changed).

**Change.** In `AGENTS.md` §10, replace the `cargo llvm-cov --workspace`
gate line with the CI-identical three-command sequence:

```sh
cargo llvm-cov nextest --workspace --all-features --no-tests=warn --no-report
scripts/coverage-quarantine-profraw.sh target/llvm-cov-target
cargo llvm-cov report --lcov --output-path lcov.info
```

with one sentence of rationale (quarantines torn profraws from
legitimately killed instrumented children before the merge — the
2026-10-13 coverage-flake fix, now applied on both sides) and a pointer
to `scripts/coverage-quarantine-profraw.sh`. Keep the other §10 gate
lines untouched. Deliberately *not* done: a wrapper script — the user
chose the explicit three-command form, matching CI verbatim and keeping
AGENTS.md the single place the gate is written down.

**Acceptance criteria.**

- AGENTS.md §10 shows the sequence and rationale; no other section
  touched; `docs/` lint-adjacent gates green (fmt/clippy unaffected but
  run the full gate anyway — the pre-commit hook does).
- One local execution of the new sequence end-to-end on the branch
  (green; quarantine reports zero or logged drops) — evidence in the PR
  description.
- #288's closure comment (chunk 3) points here as the follow-up that
  removes the local gap.

**Effort**: trivial (one file, ~8 lines).
**Risk**: none — the sequence is exactly CI's, already validated
locally twice (2026-10-13 fix validation, 2026-10-16 audit baseline).

## 6. Execution order & handoff

1 → 2 → 3 → 4 (chunk 1 first: it addresses every CI-visible flake;
chunk 2 hardens the residual; chunk 4 aligns the local gate so the
local coverage run can no longer trip over what chunk 1/2 don't cover;
chunk 3 is bookkeeping that lands last so it can reference the others'
results — including closing #288 only after chunk 4 is in). Chunks 1
and 2 are independent and may proceed in parallel if desired; chunk 4
is independent of 1–2 but must precede chunk 3's #288 closure.

Handoff per chunk: PR open + `gh pr checks` green + §3/§4 acceptance
evidence in the PR description; the user merges on GitHub (rebase-merge
commit hygiene applies — one clean commit per chunk is achievable and
preferred).

## 7. Risks & rollback

| Risk | Likelihood | Mitigation / rollback |
|---|---|---|
| Wall deadline constant too tight for a future slower environment | low | One `const` per suite; bump with a watchlist comment, never per-site |
| A test relies on the virtual deadline being short (hidden timing assumption) | very low | Grepped all four sites; predicates are pure state reads; soak protocol catches regressions |
| Regeneration loop in `refused_client` masks a real regression (ports suddenly always taken) | very low | Bounded at 10 attempts with a loud panic + counter log |
| #289 recurs during chunk work and confuses gate runs | medium (it recurs ~60% under raw libtest) | §5.3 guidance: gate commands are unaffected; strace signature distinguishes the family in one look |

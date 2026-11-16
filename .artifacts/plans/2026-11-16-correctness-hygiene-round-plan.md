# [Plan] Correctness & Hygiene Round (post-round-8) — Implementation Plan

- **Date**: 2026-11-16
- **Kind**: round implementation plan — the accepted design for the
  work the operator approved from the 2026-11-16 recommendation
  report: the "Do now" set (the #731 verdict, the #713 domain bug,
  the flaky-test dedicated pass, the config/test-harness hygiene
  chunk) and the sync correctness round (#695+#133, the Retry-After
  family, the #708 design round). This plan settles scope, seams,
  constraints, and test-oracle choices at round level; each dispatch
  chunk's stage-1 agent writes the TDD-first chunk plan that refines
  them into code-level decisions.
- **Origin**: `.artifacts/reports/2026-11-16-post-round8-implementation-recommendation-report.md`
  (sets R-A/R-C/R-D/R-E/R-B, accepted by the operator 2026-11-16),
  plus the operator's decision that the second-tier issues (#327,
  #356/#355, #845, #764, #843) stay in the unscheduled queue for a
  later backlog round. Round 8 (`2026-11-16-gui-review-round8-plan.md`,
  ACCEPTED, in implementation) is **excluded territory** — see §1.
- **Status**: ACCEPTED 2026-11-16 — dispatchable; chunking in
  `2026-11-16-correctness-hygiene-dispatch-plan.md`.
- **Verified against**: `main` @ the 2026-11-16 round-8 journey-plans
  commit. Symbol anchors are symbol-level; expect drift by
  implementation time — re-anchor by symbol.

## 1. Excluded territory (the boundary this round must not cross)

Round 8 owns: `taskboard-ui-slint`, the UI adapter layers, the
keymap/preset surface, the domain **day-boundary** module (its chunk 3,
N-4), the Settings files surface, and the census/journey machinery
folds (#715/#716/#762/#782/#768/#760/#775). The C-6 deferrals
(#807/#809/#812/#813/#819/#820) stay open untouched. This round lives
in: `taskboard-domain` (recurrence module only), `taskboard-sync-nextcloud`,
`taskboard-app` config/tests, CI scripts, and flaky-test seams in the
`cli_tty`/`cli_ipc`/sync test suites. Chunk stage-1 plans must confirm
symbol-level file disjointness against the round-8 chunk branches
before implementing concurrently.

No chunk in this round is user-visible: all are engine, sync, test,
CI, or design-doc work — the UX journey-plan mandate does not bind
any of them (recorded per chunk in the dispatch plan).

## 2. Chunk A — the #731 verdict (deny advisories)

**Problem**: #731 (p2, security) claims cargo-deny advisories fail on
`main` (bundled SQLite CVE-2026-119375 under sqlx,
RUSTSEC-2026-0459), yet 2026-11-16 `main` CI runs are green. The state
is ambiguous, and every subsequent PR's required checks sit on it.

**Decision fork (binding — exactly one path is taken, evidence recorded on #731):**

- **Path A — green with cause**: run `cargo deny check` locally on a
  clean `main`. If it passes, identify why (advisory withdrawn or
  re-scoped; `libsqlite3-sys` moved via `Cargo.lock` drift; a deny
  config change already on `main`). Close #731 as completed with the
  command output + the cause. No PR, no diff.
- **Path B — red**: fix on a branch, in this order of preference:
  1. `cargo update -p libsqlite3-sys` (patch-line bump of the bundled
     SQLite) — respects the workspace's pinned-versions policy
     (versions pinned in root `Cargo.toml`; a lockfile-only move is
     not an unpinned duplicate).
  2. If sqlx's own requirement pins `libsqlite3-sys` below the fixed
     version: bump the sqlx patch/minor line in
     `[workspace.dependencies]` — a deliberate, recorded dependency
     upgrade, changelog-noted.
  3. Only if neither is available (fix not yet published upstream):
     a dated `[[advisories]]` ignore in `deny.toml` with a
     rationale comment naming the tracking path, **plus** an
     accepted-risk entry comment on register #284 (the audit
     register) with a re-check date. An ignore without a register
     entry is not an accepted risk, it is a buried one.
- **Path C — intermittent**: if red only under specific conditions
  (fresh clone vs warm lockfile), characterize the condition, then
  follow B with that condition named in the fix/ignore rationale.

**Acceptance**: the fork is resolved with command output on #731;
Path B/C ship a PR whose diff is exactly `Cargo.lock` and/or
`[workspace.dependencies]` and/or `deny.toml`; gates: `cargo deny
check` green + the flock-wrapped `cargo nextest run --workspace`
(lock-only bumps can move builds).

## 3. Chunk B — #713, the recur attribution bug (domain, S/M)

**Problem** (from the issue, reproduced deterministically on clean
`origin/main`): proptest case seed
`cc b6100c06ce343e69842316619ddc334bc20159a784dc364acfc151777caa3270` —
rule `RecurRule { series: 0, anchor: 2053-10-16, pattern: Weekly {
interval: EveryWeeks(1), on: WeekdaySet({3, 4}) }, bound: Until(2053-10-16),
continued_from: None, paused_until: None }`, day_start_seconds = 0,
offset_seconds = 0, now_offset = 0, stamps = [24]. The property
`stamps_attribute_to_exactly_one_member` (recur.rs, the "Done stamps
sit inside the window" assertion ~line 1770) fails: with a Wed/Thu
weekly pattern anchored on the Thursday `Until` bound day, the
generated stamp at anchor + 24h lands outside the window
`occurrence_outcome` attributes it to.

**Round-level constraints (binding):**

- The partition premise is the invariant: every generated stamp is
  attributed to exactly one member window, and Done stamps sit inside
  the window they are attributed to. Fix whichever side violates it —
  `schedule`'s member enumeration (a member leaking before the
  anchor's own window when the anchor's weekday is not the pattern's
  first weekday) or `occurrence_outcome`'s attribution across the
  window boundary. Stage-1's diagnosis decides which; both fixes are
  in-bounds.
- The property's assertion is **not** weakened (AGENTS §5). The fix
  makes it pass; the test stays as written.
- The persisted seed is committed to
  `crates/taskboard-domain/proptest-regressions/recur.txt` as the
  regression case, plus unit tests pinning the Wed/Thu-Until-Thursday
  shape and a neighboring shape (anchor on the first weekday; anchor
  on a non-member weekday) so the boundary is characterized, not just
  un-broken.
- No behavior change beyond the attribution/enumeration fix: schedules
  that were correct before the fix must produce identical output
  (snapshot the rule's schedule output for the existing test corpus —
  if any existing snapshot changes, that is a finding to diagnose,
  not a snapshot to refresh).

**Acceptance**: the persisted case passes; the full property passes;
existing recur corpus output unchanged; new boundary unit tests green;
no test weakened.

## 4. Chunk C — flaky-test dedicated pass (watchlist, L)

The sanctioned pass over the `flaky-test` label query (AGENTS §7;
round 8 §5 handed it off). **This chunk is the only place flaky tests
may be touched** — no other chunk in this round or round 8 fixes a
flaky test opportunistically.

**Issue set (8 open) and round-level dispositions:**

- **#697 + #521 + #846 — the `cli_tty` Ctrl-C family** (three
  distinct tests, one seam, cross-linked by the cleanup round): root
  cause the shared seam first — raw-mode tty read paths failing under
  full-suite load, with outer-timeout and masked-read variants. The
  diagnosis decides between: (a) a production testability seam
  (deterministic deadline/injected clock replacing a real-time wait —
  §5's clock discipline: pause/advance for tokio-clock dependencies,
  deadline-polling for I/O conditions), or (b) a genuine production
  race the tests are (correctly) catching — in which case the
  production fix is the deliverable and the flakiness reports were
  bug reports. One fix, three closures, family cross-link comments.
- **#682 — merge tie proptest (domain)**: fresh seed found `tie with
  b1 == b2` breaking `the_oldest_member_survives_and_a_tie_keeps_the_bound_card`.
  Working hypothesis: the tie-break is not deterministic (equal
  timestamps fall through to insertion order or id comparison). The
  fix is determinism at the tie-break (compare a stable secondary key
  — id), never a seed exclusion or weakening; the seed joins the
  regressions file.
- **#336 — sync push_exec 404 re-resolve (wiremock)**: load-sensitive
  wiremock timing; stabilize via deterministic mock sequencing or
  deadline polling on the actor's observable state, not sleeps.
- **#287 — actor_wiremock family**: same discipline as #336; if the
  root cause is shared with #336's mock harness, one harness fix may
  close both — the issues record the shared cause and both close on
  the same evidence.
- **#384 — l13_alias happy path stderr emptiness**: structural
  assertion (stderr stays empty) failing under load; stabilize the
  output-collection race (join handles / deadline polling on the
  child's exit), never assert less.
- **#570 — cli_ipc relay instance-locked on export**: the relay hit
  an instance lock mid-export under parallel load; either the
  serialized-instance assumption in the test is wrong (fix the test's
  premise) or the relay's locking has a real race window (fix
  production). Stage-1 diagnosis decides; the disposition is recorded.
- **#650 — no_local_ids_reach_the_dismissals_wire (CI-only)**:
  CI-only, single observation. Reproduce attempt is time-boxed: if it
  does not reproduce locally, the disposition is a documented
  hypothesis + watchlist comment (issue stays open), not a speculative
  fix.

**Round-level laws for the whole chunk:** a fix must make the test
*more* deterministic, never more permissive (floors not ceilings —
AGENTS §5 quality-ceilings law); no sleep-based stabilization; every
closure carries reproduction evidence (command, seed, run count);
tests that cannot be stabilized this pass keep their issue open with a
dated watchlist comment and a named hypothesis — an honest
not-fixed-yet beats a speculative change.

## 5. Chunk D — config/test-harness hygiene (app tests + scripts, M)

- **#728 — CWD restore**: replace manual cwd save/restore in config
  tests with an RAII guard type whose `Drop` restores the previous
  directory (panic-safe under unwind — the leak the issue describes
  is a mid-test panic skipping the manual restore). The guard lives
  in the test helpers of the owning crate; no production-code change
  is expected (if stage-1 finds production code mutating process cwd,
  that is a new backlog issue, not scope creep).
- **#729 — ambient env**: a test helper that saves, clears, and on
  drop restores every `TASKBOARD_*` variable, used by every
  default-path/override config test. Deterministic regardless of the
  operator's shell environment; nextest's per-process isolation is
  not relied upon (the helper must also be correct under a merged
  harness).
- **#730 — XDG sandbox portability**: the sandbox assumes
  `XDG_CONFIG_HOME`, which `dirs::config_dir` honors only on Linux.
  Decision: the sandbox helper sets the platform-appropriate config
  override per `cfg!(target_os)` (Windows: the known
  `dirs`-respected env/registry seam available to tests; macOS:
  `HOME`-relative redirection where `dirs` honors it) — and where a
  platform offers no seam, the affected cases are `cfg`-gated with a
  comment naming the platform gap, not deleted. The Linux path (dev
  platform) must keep exercising the identical code path it does
  today.
- **#607 — script cleanup**: load-reproduction scripts record child
  PIDs at spawn and kill those PIDs on exit (trap-based cleanup);
  `kill %N` job specs are a no-op in non-interactive shells — the
  fix removes reliance on them.
- **#833 rider — read-only audit**: audit the lib/journey test
  helpers for guarded-refresh staleness; findings become comments on
  #833 plus, where a fix is warranted, `backlog:` issues (audit is
  read-only; no helper rewrites ride this chunk).

**Acceptance**: the config suite passes with `TASKBOARD_*` set to
hostile values in the environment (the test proves the env lock), a
deliberate panic-in-fixture test (or an equivalent structural
argument) shows cwd restored, and the scripts' cleanup path is
exercised in CI or by a self-test run recorded on the issue.

## 6. Chunk E — sync correctness: ETag retention + clock skew (#695 + #133, L)

**Problem** (from #695, reproduced with curl on the hosted Tier-3
account): the DR-8 F4/F5 characterization ("a file's ETag changes on
every PUT") no longer holds unconditionally — a same-second,
same-byte-length rewrite returns `204` and keeps the old ETag;
different length or a later second churns it; a stale token still
gets `412`. The client's validator/compare logic must not
false-clean (treating the retained ETag as proof of sameness when
content differs) or false-stale (looping on 412) under this behavior,
including when local and remote clocks disagree by up to a second or
more (#133).

**Round-level decisions (binding):**

- **Characterization first, in a wiremock test**: a deterministic
  mock that models the hosted behavior (retained ETag on
  same-second+same-size PUT) is committed as the regression oracle
  before any client change. The mock controls time explicitly —
  no real-clock dependence (§5 clock discipline).
- **No false-clean**: the client must not conclude "remote is
  current" from an unchanged ETag when its own write could have been
  the same-second rewrite. The acceptable designs (stage-1 picks
  one, recorded): (a) post-write validator refresh (re-GET or
  ETag-returning PUT response handling) when the write is
  second-boundary-adjacent; (b) content/revision comparison
  (Deck's own revision fields) taking precedence over ETag equality
  for sameness conclusions; (c) conditional-write discipline
  (If-Match) so the server, not the client, adjudicates. Any of the
  three satisfies the round; a heuristic "same size + same second ⇒
  same content" assumption does not.
- **Skew tolerance (#133)**: remote-vs-local timestamp comparisons
  gain an explicit tolerance constant (documented, ≥ 1s, justified
  against the observed hosted behavior) applied where second-granular
  comparisons decide convergence; comparisons that today treat
  `remote_ts <= local_ts` as "not newer" must be audited for the
  equal-second case (the F4/F5 drift). The tolerance is a named
  constant in the sync crate, not magic numbers at call sites.
- **No 412 loops**: a stale-token rejection path must converge
  (re-resolve, re-apply, single retry bound — a bounded retry with a
  typed give-up outcome, not an unbounded loop and not a silent drop).
- **Live-tier discipline**: CI verifies against wiremock only; a
  manual live-tier verification step against the hosted account is
  recorded on #695 at handoff (the LAN-runner live tier #117 is not a
  prerequisite).

**Acceptance**: the retained-ETag wiremock scenario passes (no
false-clean, no 412 loop, bounded convergence); the existing pull/push
corpus behaves identically on the non-pathological path (existing
snapshots unchanged); skew tests use pause/advance or an injected
clock, never sleeps.

## 7. Chunk F — sync transport: the Retry-After family (#220/#221/#222, M)

- **#220 — honor Retry-After in client retry waits**: the retry
  policy reads `Retry-After` from 429/503 responses and uses
  `max(policy_floor, min(retry_after, policy_cap))` — the policy's
  own floor and cap still bound it; a missing/unparseable header
  falls back to the current backoff. (Decision: the cap stays — an
  unbounded server-suggested wait can stall the push pipeline
  indefinitely; the cap's value is a stage-1 decision recorded with
  rationale.)
- **#221 — HTTP-date form + 503 coverage**: the parser accepts both
  delta-seconds and HTTP-date forms, and 503 responses participate in
  the same handling (today the handling is narrower). HTTP-date
  parsing uses the approved stack (`chrono`), with an unparseable
  date treated as absent (fallback backoff), never as an error.
- **#222 — push-abort Retry-After floor**: when a push is
  aborted/refused, the Retry-After floor is respected by the abort
  path's rescheduling so the immediate re-push storm cannot recur.

**Laws for the chunk**: all timing tests run on pause/advance or an
injected clock (zero sleeps); wiremock is the server oracle — the
delta-seconds and HTTP-date forms get one scenario each plus the
absent-header fallback; typed outcomes for "deferred by policy" vs
"deferred by server hint" (no text/log assertions).

**Acceptance**: both header forms produce correct waits under the
injected clock; floor and cap bound server hints; the abort-path
rescheduling honors the floor; existing retry corpus behavior
unchanged when no header is present.

## 8. Chunk G — #708 design round: F1 sidecar convergence (research, M)

**Problem** (p1): F1 sidecar folds' local-author leg breaks the
convergence law for third readers — the sidecar write path lets a
local author's fold state diverge in a way a third syncing reader
cannot converge back, violating the round's convergence law.

**Deliverable**: a design round — an evidence report plus a design
plan (`.artifacts/plans/2026-11-17-…` or dated at execution) that
characterizes the broken leg (who writes what, in which order, what a
third reader observes), states the convergence-law formalization for
sidecar folds, and proposes exactly **one** design (options +
trade-offs recorded, one recommendation) with its migration/compat
story for existing sidecars on disk. **No production code in this
chunk.** It ends at a human gate: the operator accepts the design
before any dispatch may chunk its implementation.

**Evidence discipline**: every claim about current behavior carries
file:line or a repro command; claims about the hosted server's
behavior carry the curl evidence pattern #695 used.

## 9. Sequencing & parallelism (summary)

1. Chunk A first (alone — the gate-trust prerequisite).
2. Then, in disjoint crates: B (domain/recur) ∥ C (flaky pass) ∥ D
   (config/tests/scripts) — three parallel worktrees.
3. Sync: E then F serially (same crate; stage-1 of F re-bases on E's
   landed transport changes) — G (design, docs-only) runs parallel to
   anything.
4. The dispatcher owns: stage-exit verification between every stage,
   the full AGENTS §10 suite between stage-2 landing and stage-3 ship
   (every code chunk), the CI watch after each PR opens, teardown,
   and at-merge issue closures.

## 10. Second-tier — deliberately not in this round

#327 (cargo-mutants cap), #356/#355 (undo-v2 state-side), #845
(config set divergence), #764 (publish --to-files), #843 (overdue
investigation): the operator has ruled these wait for a later backlog
round — they stay in the unscheduled queue untouched; no dispatch in
this round may pick them up. The perf-architecture seven and the
feature-scale items remain deferred per the recommendation report.

# [Plan] The sync refresh tail (#1485) — chunk plan & agent prompts

- **Date**: 2026-11-27
- **Kind**: chunk plan — stage-1 (planning) output for issue #1485
  ("backlog: confirm/refute the multi-second refresh tail — the
  no-timeout Nextcloud HTTP client + persist-before-interpret"), with
  complete stage-2 (implementation) and stage-3 (review) prompts.
- **Origin**: issue #1485 (normative body), split out of #1432 /
  `.artifacts/plans/2026-11-25-dayreview-quota-bugs-plan.md` (its
  out-of-scope note, line 110). Evidence gathered by reading
  `origin/main` @ `cbc3eb4f`; no measurements were run in this stage.
- **Status**: READY — one code chunk (timeout posture on both
  connectors) plus a deterministic discriminating experiment for the
  latency magnitude; the persist-before-interpret leg is resolved by
  reading (see verdict).

## 1. Recorded reality (file:line on origin/main)

### 1.1 The HTTP client timeout posture — both connectors are no-timeout

Two `reqwest::Client` builders exist in the workspace (both actors;
no shared stack helper — each crate builds its own client inline):

| Client | Site | Posture |
|---|---|---|
| Deck REST (`DeckClient`) | `crates/taskboard-sync-nextcloud/src/client.rs:428` — `http: Client::builder().build()?` | **No `.timeout()`, no `.connect_timeout()`, no `.read_timeout()`** — reqwest 0.12 defaults (root `Cargo.toml:73`) mean *no* total and *no* connect timeout at all |
| WebDAV (`WebDavFilesTransport`) | `crates/taskboard-sync-files-nextcloud/src/client.rs:195` — `http: Client::builder().build()?` | Identical: no timeout of any kind |

Consequences, by reading reqwest 0.12 semantics:

- A stalled/dead-peer connection hangs the attempt **indefinitely**
  (the OS TCP stack's retransmission timeouts are the only bound —
  minutes, and forever on a black-holed established connection).
- The per-client retry wrapper multiplies the hang:
  `DeckClient::send_with_retries` (`client.rs`, `async fn
  send_with_retries`) retries a *transport-classified* failure up to
  `BackoffPolicy::new()` = 3 attempts, 500 ms/1 s backoff capped 8 s
  (`crates/taskboard-sync-nextcloud/src/backoff.rs:20-27`); the files
  side has the same shape (`WebDavFilesTransport::send_with_retries`,
  `crates/taskboard-sync-files-nextcloud/src/client.rs` ~line 218).
  An *unbounded* attempt that never errors never exhausts the budget —
  the retry wrapper only bounds failures, not hangs.

### 1.2 The suspected code path (the awaited refresh)

- The sync actor's select loop runs each cycle **inline**
  (`crates/taskboard-sync-nextcloud/src/actor.rs:286` and `:303` —
  `cycle(...).await` inside `tokio::select!`): a hung HTTP attempt
  wedges the whole actor — subsequent `SyncNow` nudges and `Detach`
  are not received until the attempt (and its retries) finish.
- The awaited refresh is the CLI/REPL `sync` verb and the board
  ceremonies: `commands/sync.rs::sync_on_engine` nudge
  (`StateCommand::RequestSync`, `commands/sync.rs:163`) then
  `wait_for_sync_result` (`commands/sync.rs:244`) — a predicate poll
  with a **wall-clock deadline of 60 s** (`--timeout_secs`,
  `crates/taskboard-app/src/cli.rs:215-217`, `default_value_t = 60`);
  `boards select`'s `wait_for_adoption` (`commands/boards.rs:1307`)
  is the same shape with the same 60 s default. A wedged cycle never
  publishes fresh evidence, so the waiter burns its whole 60 s budget
  and reports `TimedOut` — this **is** the observed "UI/process looks
  dead for ~half a minute" tail. The mechanism is fully accounted for
  by reading; only its live-world *magnitude* is unmeasured.
- Poll cadence backoff (2 s → 60 s, `crates/taskboard-sync-nextcloud/src/poll.rs::poll_backoff`,
  defaults in `crates/taskboard-domain/src/settings.rs:32-45`) is
  cycle-level and *not* a latency-tail cause by itself — a `SyncNow`
  nudge bypasses the armed deadline (the actor's command arm runs the
  cycle immediately). It only delays *autonomous* retries after a
  failure.

### 1.3 Persist-before-interpret

- The engine loop is a sequential FIFO over its inbox: per mutating
  message it plans, applies the batch to the repository (sqlite),
  and only then interprets (`crates/taskboard-state/src/actor.rs:5-10`,
  apply site `:966` region). The `RequestSync` command and every UI
  command queue behind any slow persist.
- By reading: the repository is a **local** sqlite file
  (`taskboard-storage-sqlite`), writes are batched per message, and
  there is no network or lock on that path that could take tens of
  seconds under normal operation. A slow-persist explanation requires
  an abnormal disk condition; nothing in the code or in
  `.artifacts/reports/` (grep for refresh tail / persist latency:
  no existing measurement) evidences one. Verdict per leg below.

### 1.4 Instrumentation that exists today

- Both clients emit `tracing::debug!/info!/warn!` **events** around
  each verb (e.g. `crates/taskboard-sync-nextcloud/src/client.rs:450-457`)
  and the retry wrapper logs `retry_index`/`?delay`/resource on each
  retryable failure. There is **no `#[tracing::instrument]` span**
  around `send_with_retries`/`attempt`/`cycle`, so a hung attempt is
  invisible as a duration — nothing times a request today. Cycle-level
  outcome bookkeeping exists in the actor (`actor.rs:902` region) and
  per-board `SyncCounters.cycles` is the freshness counter the waiters
  key on (`commands/sync.rs:216` region).
- Typed error taxonomy already has a home for a timeout: both error
  enums carry `Transport(#[from] reqwest::Error)` documented as
  "connection refused, DNS, TLS, **timeout**"
  (`crates/taskboard-sync-nextcloud/src/error.rs:20-22`,
  `crates/taskboard-sync-files-nextcloud/src/error.rs:19-21`), and
  `is_transport()` classifies it retryable. A reqwest timeout error
  therefore flows through the *existing* retry → cycle-level
  Network-class → backoff machinery with **no new variant needed**.

## 2. Verdict

**CONFIRMED (by reading) that the no-timeout client is a real bug and
the only code path that can produce the observed multi-second tail;
UNDETERMINED-BY-READING for the live magnitude; REFUTED (by reading,
with a caveat) for persist-before-interpret as the normal-condition
cause.**

Evidence summary:

1. *No-timeout client — CONFIRMED as a bug*: both builders
   (`sync-nextcloud/src/client.rs:428`, `sync-files-nextcloud/src/client.rs:195`)
   set no timeout of any kind; reqwest 0.12's default is unbounded.
   The issue's own framing ("a no-timeout HTTP client is a bug in its
   own right") is settled: this gets fixed regardless of magnitude.
2. *Mechanism of the tail — CONFIRMED by reading*: hung attempt →
   inline `cycle().await` wedges the sync actor (`actor.rs:286/:303`)
   → no fresh evidence → `wait_for_sync_result`/`wait_for_adoption`
   burn their 60 s deadline (`cli.rs:215`, `boards.rs:1307`) → the
   caller is frozen for the budget. Every link is in the code.
3. *Magnitude — UNDETERMINED-BY-READING*: whether real-world stalls
   hit OS TCP timeouts (~30 s+) or hang forever needs no live
   measurement to fix, and no idle-box measurement is prescribed:
   the discriminating experiment is **deterministic in the test
   suite** (§4), which both confirms the fix and pins the typed
   timeout behaviour. A live latency breakdown (tracing spans) is
   deferred as backlog if the operator wants the tail quantified on
   a real server.
4. *Persist-before-interpret — REFUTED as the normal-condition cause*:
   local batched sqlite writes with no cross-actor contention; no
   report or code path shows a seconds-scale persist under normal
   conditions. Caveat honestly recorded: an abnormal disk condition
   *would* serialize the engine FIFO, and today nothing times the
   persist leg either. The instrumentation gap is recorded as
   backlog (§7), not fixed in this chunk.

## 3. Decisions

1. **A timeout belongs on the client builders, at the transport-actor
   layer.** Both `DeckClient::new` and `WebDavFilesTransport::new`
   configure their `reqwest::Client` with:
   - `connect_timeout(Duration::from_secs(10))` — dial fails fast;
   - `read_timeout(Duration::from_secs(30))` — reqwest 0.12's
     **idle-read** timeout: bounds a stalled peer without capping the
     total transfer, so a legitimately slow attachment upload/download
     never trips it while silent-data stalls do. (Chosen over
     `.timeout(total)` precisely because the Deck attachment verbs
     upload/download arbitrary-size bodies — a global total timeout
     would be a correctness bug on large attachments.)
   - Constants as named `const`s next to the builders, documented
     with the *why* (idle-read, not total; attachment bodies).
2. **Typed outcome when a timeout fires: the existing
   `Transport(reqwest::Error)` variant, no new error variant.** The
   enums already document timeout as a Transport-class failure; the
   retry wrapper already retries it within the bounded budget, and a
   budget-exhausted timeout lands in the cycle's existing
   Network-class → `SyncPhase::Failed`/backoff path. Zero taxonomy
   churn; AGENTS §4 `thiserror` discipline preserved.
3. **Not user-visible ⇒ no UX journey plan.** docs/ux_planning.org:
   "Pure engine/storage/sync internals with no surface delta are
   exempt." A timeout turns an indefinite hang into the *existing*
   `Failed`-phase surface (a failure that already has UI/CLI
   rendering); no new verb, badge, view, or setting. `n/a — no
   surface delta`. (Ruling offered in §6 if the operator wants the
   timeouts configurable `[sync]` settings instead — that *would*
   surface a setting and trigger the UX-plan law.)
4. **Non-goal (this chunk)**: no change to the poll backoff curve,
   the 60 s CLI budgets, the persist ordering, or the engine FIFO —
   those are either correct by reading (§1.2/§1.3) or deferred
   (backlog, §7).

## 4. Discriminating experiment (deterministic, in-suite — stage 2)

No live measurement. The stage-2 suite pins the decision table:

- **Experiment A (Deck)**: a wiremock scenario whose route delays
  longer than `READ_TIMEOUT` on the response body. Expected: the
  client verb returns `Err(DeckError::Transport(_))` whose underlying
  `reqwest::Error::is_timeout()` is `true`, after at most the bounded
  retry budget — never a hang. (wiremock precedent:
  `crates/taskboard-sync-nextcloud/tests/nextcloud_wiremock.rs` family;
  a delayed responder needs *real* time — do **not** pause the tokio
  clock around the wiremock request itself; pause/advance only the
  actor-loop waits, per the existing suite's pattern.)
- **Experiment B (WebDAV)**: same shape against
  `WebDavFilesTransport` (a delayed conditional-PUT route). Expected:
  `Err(TransportError::Transport(_))` with `is_timeout()` true.
- **Experiment C (connect leg)**: a route pointing at a
  non-routable address (e.g. a black-holed IP) asserting the
  `connect_timeout` bounds the dial — typed Transport timeout.
- **Deciding numbers**: each experiment's wall time (real clock, in
  a `#[test]` with a generous outer assert-free patience window) is
  bounded by connect 10 s / idle-read 30 s ×≤3 attempts + backoff;
  the *behavioural* oracle is the typed outcome + `is_timeout()`, not
  a duration ceiling (AGENTS §5: no quality ceilings — assert the
  typed outcome; a smoke duration guard only against the old
  unbounded hang is a *floor-style* bound: "returns at all").

Invariants that stage 2's tests must uphold (sync correctness):

- **Conditional-PUT discipline**: a timed-out request is *not known*
  to have failed server-side; retrying a conditional-PUT with the
  same If-Match precondition is safe (server answers 412/precondition
  failure when the first attempt landed — the existing etag machinery
  `crates/taskboard-sync-files-nextcloud/src/etag.rs` already treats
  this as a typed outcome, no mutation is lost or doubled).
- **No lost mutations**: Deck outbox ops survive a cycle-level
  Network failure (the cycle ends early, the outbox persists —
  existing behaviour, re-asserted by existing suite tests; stage 2
  adds a timeout-flavoured variant only if the wiremock family lacks
  one for the timed-out-then-succeeds path).
- **Idempotent retries**: the pull side is read-only; the push side
  retries whole verbs with unchanged payloads.

## 5. Verification commands (stage 2/3 gate)

```sh
scripts/run-quality-gates.sh cargo nextest run --workspace   # expected: green
scripts/memory-gate.sh cargo clippy -p taskboard-sync-nextcloud -p taskboard-sync-files-nextcloud --all-targets -- -D warnings   # expected: no warnings
```

plus the pre-commit hooks (fmt + clippy, automatic). The dispatcher
owns the full AGENTS §10 suite between stage 2's landing and stage 3.

## 6. Rulings requested

1. **Hard-coded constants vs `[sync]` config settings for the two
   timeouts.** Recommended default: **hard-coded `const`s on the
   builders** — a tunable knob for transport hygiene is speculative
   generality (no operator need evidenced), and a new setting would
   trigger the UX-journey-plan law for a new user-visible setting.
   Proceed on this default unless overridden.
2. **Whether a live quantification of the tail on the real Nextcloud
   instance is wanted** (idle-box tracing-span measurement per
   docs/agent_dispatch.org §6 hygiene). Recommended default: **not
   now** — the deterministic experiments confirm the mechanism and
   the fix; a live measurement is backlog (§7).

## 7. Out-of-scope findings → backlog issues (to open at dispatch)

- `backlog:` **no duration instrumentation on the sync request path**:
  no `#[tracing::instrument]` span on `send_with_retries`/`attempt`/
  `cycle`, so request latency is invisible in traces; also nothing
  times the engine's persist leg. Categories `performance`, `dx`, p3.
- `backlog:` **consider a live-box quantification of the refresh
  tail** after the timeout fix (tracing spans needed first). p3,
  `performance`.

(Open per the §7 standing authorization; search first.)

## 8. STAGE 2 PROMPT — IMPLEMENTATION (verbatim, paste-ready)

You are the STAGE 2 (IMPLEMENTATION) agent for the chunk
"sync refresh tail" in the taskboard repo (GitHub: D4id4los/taskboard),
executing issue #1485 from the accepted chunk plan
`.artifacts/plans/2026-11-27-sync-refresh-tail-plan.md` (normative;
read it in full — §1 recorded reality, §3 decisions 1-4, §4
experiments and invariants, §5 gates).

Work ONLY inside `/home/viktoria/src/taskboard-WT-fix-sync-refresh-tail`
on branch `fix/sync-refresh-tail`, by absolute path. Do not touch the
main checkout or any other `taskboard-WT-*` directory. If the worktree
is missing, recreate it: `scripts/worktree-doctor.sh` from the main
checkout first (nonzero = STOP and report), then `git -C
/home/viktoria/src/taskboard worktree add ../taskboard-WT-fix-sync-refresh-tail
fix/sync-refresh-tail`.

**Long-command discipline** (binding — docs/agent_dispatch.org;
restated verbatim): A subagent is killed after ~600s without
observable activity. **Long-command discipline**: (1) *The coverage
trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is
the pre-commit hooks (automatic) plus
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(flock-safe; background + poll with useful work between polls if
needed); the dispatcher runs the full-suite runs, the CI watch,
teardown, and at-merge issue closures (a stage-3 agent's scope ends
at PR-open). (2) *Stage 3's scope ends at PR-open*: push, open the
PR, report — the dispatcher owns `gh pr checks --watch`,
`mergeStateStatus CLEAN`, teardown, and at-merge issue closures.
(3) *Any command expected over ~5 minutes runs backgrounded* with
short interleaved polls and useful work between polls — commit all
landed work BEFORE any wait. Large scratch under /var/tmp, never
/tmp. (4) **Every agent-launched cargo invocation routes through
`scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate
this discipline verbatim.* *Flock-deferral rule: when the gate lock is
contended and no other useful work remains, never block — commit all
landed work, report "gate deferred: lock held by <holder>", and end
the stage; the dispatcher runs the gate.*

Implement exactly plan §3:

1. `crates/taskboard-sync-nextcloud/src/client.rs` — in
   `DeckClient::new`, build the client with `connect_timeout` (10 s)
   and `read_timeout` (30 s idle-read) from named `const`s beside the
   builder, documented per plan §3.1 (why idle-read, not total:
   attachment upload/download bodies are arbitrary-size). Same for
   `WebDavFilesTransport::new` in
   `crates/taskboard-sync-files-nextcloud/src/client.rs:195`.
   No new error variants: a timeout surfaces as the existing
   `Transport(reqwest::Error)` in both enums (plan §3.2). Both crates
   keep `#![forbid(unsafe_code)]` and their SPDX headers.
2. Tests, lowest layer, per plan §4:
   - Experiment A: wiremock delayed-body route → Deck verb returns
     `DeckError::Transport(_)` with `reqwest::Error::is_timeout()`
     true; place in the existing wiremock family
     (`crates/taskboard-sync-nextcloud/tests/nextcloud_wiremock.rs`
     grouping). Do not pause the tokio clock around the wiremock
     request itself.
   - Experiment B: same shape for `WebDavFilesTransport`
     (`crates/taskboard-sync-files-nextcloud/tests/`).
   - Experiment C: non-routable dial → connect timeout surfaces as a
     typed Transport timeout (Deck side is sufficient if both share
     the constant-shape helper pattern; do both if the tests stay
     honest per crate).
   - Test rules (AGENTS §5, binding): typed outcomes only —
     `matches!(err, ...)` / `is_timeout()`; NO text assertions on
     error strings; NO duration ceilings on success (no "completes in
     ≤ X s") — the oracle is the typed outcome, "it returns at all"
     is the anti-hang bound; no `tokio::time::sleep` waits — where a
     wait is unavoidable around real wiremock time, poll a predicate
     with a deadline using real `tokio::time::timeout`, and keep
     `pause`/`advance` for actor-loop waits per the existing suite
     pattern; one behaviour, one test, lowest layer — do not
     duplicate the timeout test through the sync actor's cycle or an
     integration suite; the actor-level timed-out-then-recovers path
     is covered only if the existing wiremock actor suite lacks it
     (add one `SyncPhase::Failed`-lands test, not more).
3. Invariants you must not break (plan §4): conditional-PUT retry
   safety after a timed-out attempt (precondition machinery
   unchanged), no lost outbox mutations on a Network-class cycle end,
   retries send unchanged payloads.
4. Diff-shape rules: the production diff is confined to the two
   builder functions + the new `const`s (+ their doc comments) and
   the new tests; no changes to the error enums, the retry wrappers,
   `poll.rs`, the actor loops, the CLI budgets, or the state engine.
   If you believe a wider change is needed, STOP that part and report
   it instead of expanding the diff.
5. Gate: pre-commit hooks run automatically; then run
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (backgrounded; commit landed work first; flock-deferral rule
   above). The tests that must not have been weakened: the existing
   `poll.rs` backoff tests, the existing wiremock suites, and the
   `wait_for_sync_result`/`wait_for_adoption` unit tests in
   `crates/taskboard-app/src/commands/sync.rs`/`boards.rs` — none of
   their assertions may be relaxed.
6. Commit on the branch as
   `fix(sync): bound the Nextcloud HTTP clients with connect + idle-read timeouts (#1485)`
   with a body. Do NOT push, do NOT open a PR — stage 3 owns that.
   Report: SHA, files touched, gate outcome (or the flock-deferral
   line), any ruling deviations (none expected).

## 9. STAGE 3 PROMPT — REVIEW (verbatim, paste-ready)

You are the STAGE 3 (REVIEW) agent for the chunk "sync refresh tail"
in the taskboard repo (GitHub: D4id4los/taskboard), reviewing the
landed stage-2 work on branch `fix/sync-refresh-tail` against the
accepted chunk plan
`.artifacts/plans/2026-11-27-sync-refresh-tail-plan.md` (normative)
for issue #1485. Your scope ends at PR-open.

Work ONLY inside `/home/viktoria/src/taskboard-WT-fix-sync-refresh-tail`
on branch `fix/sync-refresh-tail`, by absolute path. Do not touch the
main checkout or any other `taskboard-WT-*` directory.

**Long-command discipline** (binding — docs/agent_dispatch.org;
restated verbatim): A subagent is killed after ~600s without
observable activity. **Long-command discipline**: (1) *The coverage
trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is
the pre-commit hooks (automatic) plus
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(flock-safe; background + poll with useful work between polls if
needed); the dispatcher runs the full-suite runs, the CI watch,
teardown, and at-merge issue closures (a stage-3 agent's scope ends
at PR-open). (2) *Stage 3's scope ends at PR-open*: push, open the
PR, report — the dispatcher owns `gh pr checks --watch`,
`mergeStateStatus CLEAN`, teardown, and at-merge issue closures.
(3) *Any command expected over ~5 minutes runs backgrounded* with
short interleaved polls and useful work between polls — commit all
landed work BEFORE any wait. Large scratch under /var/tmp, never
/tmp. (4) **Every agent-launched cargo invocation routes through
`scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate
this discipline verbatim.* *Flock-deferral rule: when the gate lock is
contended and no other useful work remains, never block — commit all
landed work, report "gate deferred: lock held by <holder>", and end
the stage; the dispatcher runs the gate.*

Review duties:

1. Verify the landed diff against plan §3/§4/§8.4: only the two
   builder functions + named `const`s + the new tests; no error-enum,
   retry-wrapper, actor-loop, poll, CLI-budget, or engine changes;
   both crates still `#![forbid(unsafe_code)]`; SPDX headers intact.
2. Verify the tests per plan §4 and AGENTS §5: typed outcomes
   (`is_timeout()` / `matches!`), no text assertions, no success
   ceilings, no sleeps, lowest layer only, and none of the named
   existing tests weakened (poll backoff tests, wiremock suites,
   `wait_for_sync_result`/`wait_for_adoption` unit tests).
3. **Code-quality mandate** (stage 3): beyond functional review,
   evaluate (a) code quality: naming, comment discipline (comments
   state constraints, not narration), idiomatic use of the approved
   stack, no dead code or speculative generality, test quality per
   AGENTS §5; (b) the Single Responsibility Principle: the crate-role
   boundaries of AGENTS §2 are the SRP line — network IO lives only
   in the transport actors; (c) a single level of abstraction per
   function body. Findings are fixed on the branch.
4. Gate: pre-commit hooks (automatic) plus
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (backgrounded; commit landed work first; flock-deferral rule
   above).
5. Handoff: rebase onto the fetched `origin/main` if needed, push
   with `git push --force-with-lease origin fix/sync-refresh-tail`
   (only if a rebase rewrote history; plain push otherwise), open the
   PR targeting `main` titled from the stage-2 commit, body linking
   issue #1485 ("Fixes #1485" is NOT yours to decide — the dispatcher
   closes at merge; link as "Refs #1485" unless the PR fully
   disposes of the issue, then say so in the body for the operator),
   and report the PR URL. Run `gh pr checks` once to record the
   initial state — do NOT watch. Then remove the worktree:
   `git -C /home/viktoria/src/taskboard worktree remove --force
   ../taskboard-WT-fix-sync-refresh-tail` (verify via
   `git worktree list`).

## 10. Estimated size

**S** — two builder functions + four `const`s, three wiremock-family
tests, one branch/PR; the only mild risk is the delayed-body wiremock
route plumbing in the existing hermetic harness.

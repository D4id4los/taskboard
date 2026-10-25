# [Plan] Tier-2 Convergence Fixture Repairs (backlog-triage chunk 3)

- **Date**: 2026-10-25
- **Kind**: chunk plan — stage-1 planning record for chunk 3 of the
  backlog-triage dispatch
  (`.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md` §4).
  Tests-only: `crates/taskboard-sync-nextcloud/tests/` plus this
  plan and the tier-records report. **Zero production-code diffs**
  (dispatch acceptance).
- **Branch / worktree**: `test/tier2-convergence-fixtures` /
  `../taskboard-WT-test-tier2-convergence-fixtures` (doctor exit 0,
  100 G free, floor 25 G).
- **Base**: `b2656a9` = fetched `origin/main` tip. Deviation from the
  literal "off main": the local `main` ref (30381b0) trailed origin;
  the branch is cut off the fetched tip per the dispatch mechanics'
  "branched off the then-current `main`" intent. The commits between
  the session's first fetch (68f110c) and the base are all docs/plan
  artefacts of a parallel design round — the sync test seams were
  re-verified byte-identical (`git diff 68f110c..b2656a9 -- crates/
  scripts/` empty).
- **Incident, recorded**: the first worktree instance (at 68f110c,
  pre-first-commit) was swept mid-session by a parallel agent's
  `worktree-doctor.sh` run — a fresh `-b` branch sitting at
  `origin/main`'s tip with a clean tree satisfies none of AGENTS §8's
  doctor protections (no unmerged commits, not dirty, not `main`) and
  reads as "HEAD merged, disposable". The worktree was re-created at
  the new tip and this plan restored; the plan file itself survived
  (untracked files outlive the sweep). Stage 2/3: commit promptly
  after (re)creating a worktree, and recreate per dispatch §1 if
  missing.
- **Normative origins**: issues #434 (boot race), #435 (version
  literal), #367 (stamp-collision flake triage); wave-exit report
  Findings F1, F2, F7
  (`.artifacts/reports/2026-10-25-daily-driver-wave-exit-report.md`);
  dispatch plan §4 (chunk covers + acceptance + the docker-tier
  chunk-note).
- **Line numbers below are plan-day references** (base `b2656a9`),
  never load-bearing.

## 0. What the harness reading established

All three issues were re-grounded against the tree at `68f110c`
before deciding anything:

- **#434 is exactly the D1 precedent's omission.** `boot_convergence`
  (`it_docker.rs:572`) calls `a.set_board(board.id)` (`:587`) and then
  immediately reads
  `*…stacks.keys().next().unwrap()` (`:603`) to place the `CreateTask`.
  `SyncPair::set_board` ends in `sync_now()` — a fire-and-forget
  `mpsc` send (`tests/common/sync_pair.rs:111` + `:115-120`) — so the
  first pull's completion is not awaited and A's `stacks` map can
  still be empty: the panic all four C-fixtures share. D1
  (`two_way_round_trip`, `it_docker.rs:121-132`) does the same dance
  *with* the await, commented "the cycle's *merge* is what adopts the
  remote stacks — wait for that", and ran green live in the wave-exit
  session. The fix is to give `boot_convergence` the same await.
- **#435's pin is over-specific by one component.** The fixture passes
  the literal `"1.19.0"` (`it_docker.rs:1024`) into the shared
  `capabilities_probe` body (`tests/common/mod.rs:875-895`), which
  asserts `caps.deck_version == Some("1.19.0")` exactly. The refreshed
  image ships Deck 1.19.1; the probe decoded it correctly — only the
  pin broke. Decisive production context: `flags_for`
  (`crates/taskboard-domain/src/capabilities.rs:52-73`) parses
  **major.minor only** (`NEWEST_KNOWN = (1, 19)`), so the derived
  capability row is patch-insensitive *by construction* —
  `flags_for("1.19.1") == flags_for("1.19.0")`.
- **#367 is already fixed by construction on `main`.** The flake's
  helper `await_card_stamp_advance` and its strict-`>` assertion no
  longer exist anywhere in the tree (grep-verified). Commit `4613ca0`
  (2026-10-22, "test(sync): make the attachment stamp advancement
  observable by construction") landed the deterministic repair: the
  fixture now polls the wall clock under the tier deadline until it
  leaves the pre-write second (`tests/common/mod.rs:546-555`,
  50 ms polls, deadline-asserted — no fixed sleep) *before* the
  in-place attachment update, and the strict `>` became `>=` plus a
  measured equality pin against the attachment's own stamp
  (`assert_stamp_advancement`, `:597-647`). The issue simply was never
  closed. Details in D3.

## 1. Decisions

### D1 — the boot await (fixes #434)

Insert immediately after `a.set_board(board.id).await;` in
`boot_convergence`, before the `CreateTask`:

```rust
// The fire-and-forget SyncNow leaves the first pull in flight; the
// cycle's merge is what adopts the remote stacks — await it before
// CreateTask reads A's stacks map (the C-series boot race).
eventually(&a, |app| {
    !app.stacks.is_empty()
        && app.boards.values().any(|b| {
            b.remote.as_ref().map(|r| r.board)
                == Some(taskboard_domain::RemoteBoardId(board.id))
        })
})
.await;
```

- **Which state**: A's published `AppState`, read lock-free inside
  `eventually` via `pair.engine.shared_state().load_full()` (the
  `ArcSwap` read path). Predicate = the D1 predicate verbatim,
  specialized to the created board: A's stack set is non-empty *and*
  the binding to remote board `board.id` is present in the published
  boards. Both conjuncts are load-bearing: the first is what the
  `unwrap()` below consumes; the second pins that it was *this*
  board's merge that landed, not any stale state.
- **Which deadline**: none added — `eventually`
  (`tests/common/sync_pair.rs:130-141`) already wraps the poll in
  `tokio::time::timeout(LIVE_DEADLINE, …)` with `LIVE_DEADLINE = 30 s`
  (`tests/common/mod.rs:117`) and a 20 ms poll interval. Deterministic
  by construction per AGENTS §5: deadline-guarded polling of terminal
  state, no sleeps, no transient intermediates. The same
  predicate + deadline is the D-series' proven shape (green live,
  wave-exit session).
- **Scope — A only, not B.** B's `stacks` map is consumed by nothing:
  `Convergence::b_task` and every C-body assert on B's *tasks/rules*
  (`it_docker.rs:637-647`, C1-C4 bodies), B adopts through the
  document fold, and B's task adoption is already awaited at the end
  of `boot_convergence` (`:620-625`) with B's actor self-polling at
  `poll_interval = 300 ms` (`sync_pair.rs:23`). A wait on B's stacks
  would be dead waiting.
- The panic site keeps its `.unwrap()` — after the await it is
  unreachable-empty, and D1 keeps the same `.unwrap()` under the same
  guarantee (house precedent).
- **Red-state evidence**: the wave-exit record (4/4 C-failures at the
  same scaffolding race, same tree) *is* the failing-test record;
  stage 2 does not re-produce the red at cost of a tier cycle before
  fixing. The green matrix (§3) is the fresh evidence.

### D2 — the capabilities fixture asserts a version floor (fixes #435)

**Assertion shape — parse + compare, floor as a typed tuple:**

- Change the shared body's signature from
  `capabilities_probe(client, expected_version: &str, expect_startdate)`
  to `capabilities_probe(client, min_version: (u64, u64, u64),
  expect_startdate)`. The floor is a tuple, not a string: a literal we
  control cannot need runtime parsing, and the tuple makes "compare
  component-wise" the std tuple `Ord` — nothing string-shaped to get
  wrong.
- Parse the *server-reported* version with a small private helper in
  the `suite` module, mirroring the domain's documented leniency
  (`parse_major_minor` + `leading_digits`, `capabilities.rs:78-90`):
  leading-digits prefix per component, suffix ignored, **at least
  major.minor required, patch defaults to 0 when absent**. `None`
  (missing/non-numeric major or minor) fails the fixture — a
  non-decodable version is a probe-contract failure. The leniency
  (suffix- and patch-optional) is deliberate and documented at the
  helper: a future `"1.20"`-style or suffix-bearing Deck report must
  still decode, exactly as the production derivation tolerates it.
- Assert `decoded >= min_version` (tuple-wise) with both values in the
  message, then keep the existing behavioral assert untouched:
  `flags_for(version).card_startdate == expect_startdate`.

**The floor and why CARD_STARTDATE presence remains the seam:**

- Default tier floor: **`(1, 19, 0)`** — the newest measured Deck
  generation (the production `NEWEST_KNOWN` boundary). Patch drift
  (1.19.1 today, 1.19.x tomorrow) survives by construction; a minor
  bump also passes the floor, which is correct — a refresh to 1.20.x
  is a re-measure event governed by the known-server matrix
  (`docs/testing_strategy.org` §8), not something a test pin should
  veto.
- Gate tier floor: **`(1, 17, 5)`** — the characterized historical
  generation. Decision: *both* call sites move to the shared
  parse-and-compare body (one body, one semantics — the alternative,
  exact-pin for the gate only, would fork the shared fixture). The
  gate's real guard is unaffected: a misconfigured gate tier that came
  up on a modern Deck would now pass the version check but **fail the
  `card_startdate == false` row** (`flags_for("1.19.x")` is
  `NATIVE`), i.e. detection is preserved, only the failing assert
  moves to the behavioral one.
- **CARD_STARTDATE presence remains the seam** because the production
  derivation reads only major.minor — no patch release can move the
  derived flag, so the version floor is environmental context ("the
  tier is the measured generation") while the contract under test is
  the derived capability row: `true` on the default tier, `false` on
  the gate. This is F7's point restated as the fixture's structure.

**Call sites** (both in `it_docker.rs`; `it_live.rs` calls neither
fixture — grep-verified, so the signature change is confined):
`it_nextcloud_docker_capabilities` → `((1, 19, 0), true)`;
`it_nextcloud_docker_deck1175_capabilities` → `((1, 17, 5), false)`.

**Below-minimum demonstration.** The acceptance requires "a
below-minimum decode fails it" to be demonstrable. Live tiers can
never demonstrate the below-floor branch (servers only report current
versions — this is precisely what bit us), so the demonstration is a
small `#[cfg(test)]` unit on the parse+compare edge in
`tests/common/mod.rs`: at-floor passes, patch-above passes,
minor-below fails, two-component leniency decodes. Recorded deviation
reason (AGENTS §5 "don't test test-helpers"): the rule guards
redundant coverage of helpers already exercised through real fixtures;
this helper's below-floor branch is *unexercisable* through any real
tier, so this unit is the only layer that can hold it — one behavior,
one test, lowest layer. Known wart, accepted: `tests/common/mod.rs`
compiles into five test binaries, so the unit runs 5× per workspace
run (sub-millisecond pure fns).

### D3 — #367 triage: already fixed by construction; close with evidence

**Decision: no code change — the issue is closed with the triage
verdict, not re-fixed.** The harness reading (§0) shows the
flake's exact failure mode (`attachment_stamp == stamp_before` making
the strict `>` assert fail) was eliminated on `main` by `4613ca0`:
the wall-clock escape makes the update land in a strictly later second
than the pre-write stamp, so the advancement *is observable* by
construction; the strictness was then deliberately relaxed to
`>=` + measured equality, which also makes the repaired asserts
**clock-skew-tolerant** (the escape sharpens observability, it is not
load-bearing for pass/fail). Residual-hazard check: the fixture is
tier-2-only (`it_live.rs` does not register it), so host clock ==
server clock (same docker daemon) — no skew scenario remains that can
re-introduce the flake. The repair satisfies the issue's own suggested
direction ("guaranteed by construction", the deadline-guarded variant,
no fixed sleeps) and AGENTS §5's determinism rule.

**Action**: the chunk PR body carries `Closes #367`, and stage 3 posts
the triage comment on the issue: root cause (same-second stamp
collision, second-granularity Deck stamps), fix commit `4613ca0` with
its one-line mechanism, the current assertion shape (`>=` + equality
pin + the wall-clock escape at `tests/common/mod.rs:546-555`), and
this chunk's green live re-run of
`it_nextcloud_docker_attachment_content_lifecycle` on both tiers (§3)
as post-fix live evidence.

### D4 — the two-tier re-run matrix and evidence format

**Matrix** (what must run, per tier):

| Tier | Bring-up | Tests |
|---|---|---|
| default (Deck 1.19.x) | `eval "$(scripts/nextcloud_it_setup.sh up)"` | the full `it_nextcloud_docker` set: `cargo nextest run -p taskboard-sync-nextcloud --run-ignored only -E 'test(it_nextcloud_docker)'` — includes C1/C2/C3, `…_capabilities`, `…_attachment_content_lifecycle` |
| gate (Deck 1.17.5) | `TASKBOARD_IT_SUFFIX=-deck1175 DECK_VERSION=v1.17.5 TASKBOARD_IT_NC_IMAGE=nextcloud:33-apache` setup (port 8371) | `-E 'test(it_nextcloud_docker_deck1175)'` — includes C4, `…_deck1175_capabilities`, `…_deck1175_attachment_content_lifecycle` |

- The default-tier run is the **full** docker set, not C-only, so the
  result is comparable with the wave-exit baseline (17 passed live /
  4 failed at the base) — the chunk's diff should move that baseline
  to all-green.
- **False-pass trap** (wave-exit method step 3): `--run-ignored only`
  alone yields fake passes — env-guarded fixtures early-return as
  0.006 s "passes" when credentials are unset. The setup script's
  exported credentials must be present, and the recorded evidence must
  show per-test durations inconsistent with early-return skips.
- Both tiers torn down afterwards (`scripts/nextcloud_it_setup.sh down`
  for each suffix); neither stage leaves tiers up.
- If docker is unavailable in a stage's session, the stage STOPs and
  reports — acceptance is unmeetable without the tiers, and no
  workaround (skipping the live run, weakening a fixture) is
  authorized.

**Evidence format**: stage 2 commits
`.artifacts/reports/2026-10-25-tier2-convergence-fixtures-tier-records.md`
with one section per tier: the bring-up env/command, date, base SHA,
the per-test outcome table (test name | outcome | duration), the
verbatim `cargo nextest` `Summary` line, and any honestly-recorded
divergence. Stage 3 distills the same table into the PR description as
the review evidence (the dispatch chunk-note's requirement) — the PR
carries the distilled matrix; the branch carries the raw record.

### D5 — commit and PR shape

- Three commits, each standalone (rebase-merge hygiene):
  1. `test(sync): boot_convergence awaits the first pull` — the D1
     fix (#434).
  2. `test(sync): capabilities fixture asserts a version floor` — the
     D2 fix (#435), including the parse helper + unit.
  3. `docs(report): tier-2 convergence fixture tier records` — the D4
     records file (lands after the runs).
- No `CHANGELOG.md` edit: nothing user-observable changes (dispatch
  §1 step 2 names only chunks 1 and 9).
- PR body: `Closes #434`, `Closes #435`, `Closes #367` (with the D3
  triage comment posted on the issue), the distilled tier matrix, and
  the explicit statement that the diff touches no production code.
- Stage 3 handoff per dispatch §1 step 3: rebase onto `origin/main`,
  PR open, checks green, `mergeStateStatus CLEAN`, then worktree
  teardown with the absolute-anchor command.

## 2. Stage-2 work order

1. Enter (recreate if missing) the worktree on
   `test/tier2-convergence-fixtures`; confirm base and clean tree.
2. Land D1 (commit 1) and D2 (commit 2). `cargo fmt --all -- --check`
   and `cargo clippy --workspace --all-targets -- -D warnings` after
   each; `cargo nextest run -p taskboard-sync-nextcloud` hermetically
   green (the new unit runs inside the hermetic binaries).
3. Bring up both tiers per D4; run the matrix; record; tear down.
4. Full AGENTS §10 gate suite (including the three-command coverage
   sequence); commit the records (commit 3). Hand off to stage 3.
5. **If any C-fixture fails past the boot fix** (a deeper convergence
   failure, not scaffolding): that is a bug candidate per AGENTS §5 —
   STOP and report with the transcript; do not weaken the fixture, do
   not widen this chunk into production changes.

## 3. Acceptance → evidence map

| Dispatch acceptance | Evidence |
|---|---|
| C1–C4 pass on both tiers | tier-records matrix rows (C1/C2/C3 default, C4 gate) |
| capabilities passes on 1.19.x and demonstrably asserts a floor | live `…_capabilities` green on 1.19.1 with floor `(1,19,0)`; the below-floor unit |
| #367 fixed deterministically or verdict recorded | D3: already fixed by construction (`4613ca0`); issue closed with evidence + this chunk's green attachment-lifecycle rows on both tiers |
| zero production-code diffs | branch diff touches `tests/` + `.artifacts/` only |

## 4. Risks

- **Worktree swept pre-first-commit** (observed this session — see the
  incident note): until a branch carries its first commit, the sweep
  doctor cannot distinguish a fresh stage-1 worktree from a disposable
  merged-HEAD one. Mitigation for this chunk: stage 2 commits as its
  first action after (re)creating the worktree; a swept-but-uncommitted
  worktree is recreated per dispatch §1 step 2. The underlying doctor
  blind spot is a backlog candidate (a `HEAD == origin/main` worktree
  on a non-`main`, non-landed branch deserves the same protection as a
  dirty tree), to be filed by stage 3, not fixed in this chunk.
- **Deeper C-series failures past the boot fix** (the fixtures have
  never run green — the race is the *recorded* failure, not proof the
  rest is sound): live-latency unknowns in the anchor scan, the
  document fold on B, the rule tombstone cycle. Mitigation: the matrix
  is the acceptance; failures route to the §2 step-5 STOP, not to
  fixture weakening. Bounded: the same cycle machinery is green at
  tier 1 (f-series 58/58) and the native lane ran green live.
- **`eventually`'s generic timeout panic** ("predicate satisfied
  within the live deadline") localizes poorly; accepted — the
  C-bodies' inner asserts and the transcripts identify the stage, and
  matching the D-series' proven shape beats inventing a new helper.
- **Docker-tier availability** gates the whole acceptance; STOP-and-
  report is the honest failure mode (D4).
- **Gate-tier floor semantics**: a misconfigured gate tier now fails
  on the behavioral row instead of the version pin (D2) — net
  detection preserved, failure site moved; recorded as intended.
- **Parallel-chunk docker contention** (chunk 2 / other sessions share
  the runner): the suffix/port split (`-deck1175`, 8371) is the
  existing isolation mechanism; tiers are torn down promptly.

## 5. Estimated diff size

- `tests/it_docker.rs`: ~+10 / −2 (the await block, two call-site
  argument changes).
- `tests/common/mod.rs`: ~+55 / −10 (signature change, parse helper +
  docs, unit mod, doc-comment updates).
- `.artifacts/reports/…-tier-records.md`: new, ~+60 (stage 2).
- Total code diff: **~65 lines across two test files**; no production
  code, no new fixtures, no CI changes (dispatch non-goals honored).

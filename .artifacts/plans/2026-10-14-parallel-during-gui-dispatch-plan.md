# [Plan] Parallel Backlog Lanes During Phase 7 GUI — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-14
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the eight chunks of the parallel
  conflict-free backlog lanes that run **alongside** the Phase 7 GUI
  implementation. No code in this commit.
- **Origin**: parallel-lanes conflict analysis (chat session
  2026-10-14) against the GUI implementation dispatch plan
  `.artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md`
  (§1–§6: its file/crate scope defines this plan's no-go zones) and
  the potential-improvements backlog (each chunk's entry named
  below); the D1 spike report
  `.artifacts/reports/2026-10-14-deck-roundtrip-strategy-report.md`
  (§3.2/§3.3 evidence for chunk 2). User approved the lane coverage
  and this dispatch 2026-10-14.
- **Status**: READY — all eight chunks are dispatchable now; they are
  order-independent **across** lanes, serialized **within** lane A
  (same crate, overlapping files). Deliberately **not** dispatched
  here: see §10.2.

## 1. Chunks & ordering

### 1.1 Chunks table

| Chunk | Backlog entry (origin) | Branch | Covers | Size |
|---|---|---|---|---|
| P1 | "[2026-10-10] `Retry-After` handling in the poll backoff" | `feat/sync-nextcloud-retry-after` | sync crate: parse `Retry-After` into `poll_backoff`'s floor, pure seam kept | S |
| P2 | "[2026-10-08] Deck attachment writes and content download" | `feat/sync-nextcloud-attachment-content` | sync crate: multipart upload/update/restore/delete + raw-bytes download on `DeckClient` (client surface only) | M |
| P3 | "[2026-10-09] Criterion bench for the persistence hot path" | `test/storage-hot-path-benches` | storage crate: `[[bench]]` for `load()` hydration + one `apply` batch at N ∈ {100, 1 000, 10 000} | S/M |
| P4 | "[2026-10-14] In-session `status` should surface `connector_attached`" + "[2026-10-12] `taskboard config path` diagnostic verb" | `feat/app-status-config-dx` | app crate: honesty micro-batch — session-status constructor takes the flag; `config path` pre-core verb | S |
| P5 | "[2026-10-14] Daemon-side attach verb" | `feat/app-ipc-attach-verb` | app crate: additive IPC verb — attach a running daemon's connector from persisted state (no secret on the wire) | M |
| P6 | "[2026-10-11] REPL history persistence (file-backed)" + "[2026-10-12] REPL completion: uuid candidates for selectors" | `feat/app-repl-history-uuid-completion` | app crate: file-backed REPL history (XDG data-dir decision) + uuid-disambiguated completion candidates | S/M |
| P7 | "[2026-10-12] Deterministic stamps for clock-gated fixture builders" | `test/domain-deterministic-stamps` | domain crate: fixed strictly-increasing fixture stamps; delete three Miri gates | S |
| P8 | "[2026-10-12] Raise the Miri pipeline/state case caps on a measured probe" | `ci/miri-cap-probe` | `miri.yml` only: measured cap raises, keep what fits ≤ 90 min per job | S |

### 1.2 Dependency graph

- **Lane A (sync crate): P1 → P2 serialized.** Both edit
  `crates/taskboard-sync-nextcloud/src/client.rs` (P1 the error/metadata
  surface, P2 new endpoints) — dispatch P2 only after P1's PR merged
  (default order; swapping is allowed if the user prefers the keystone
  first, but never concurrently).
- **Lane B (storage): P3** — independent of everything.
- **Lane C (app crate): P4 ∥ P5 ∥ P6** — different files within
  `taskboard-app` (P4 `commands/status.rs`+`commands/config.rs`; P5
  `ipc/`+client relay; P6 `repl.rs`+`completion.rs`); adjacent-line
  collisions possible in `commands/mod.rs` dispatch arms — land serially
  or accept trivial rebases.
- **Lane D (domain): P7** — independent; touches only domain test files.
- **Lane E (CI): P8** — independent (`miri.yml` only); P7 and P8 both
  append to `docs/testing_strategy.org` §10 (different paragraphs) —
  trivial-rebase expectation if concurrent.
- **Default dispatch order**: all lanes may start immediately; within
  each lane follow the serialization above.

### 1.3 Shared execution model & mandates (binding for every chunk)

The user's process, three agents per chunk, each in its own fresh
session:

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` (next to the project root, AGENTS §8) on
   a new branch off the then-current `main`, writes a detailed
   TDD-first plan at `.artifacts/plans/<today>-<topic>-plan.md` in the
   house `[Plan]` style (see existing plans), resolving every open
   question of the chunk's §n section with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing), implements per the approved
   plan, TDD, every commit standalone (rebase-merge hygiene), runs the
   full gate suite (AGENTS §10: fmt, clippy `-D warnings` on
   `--workspace --all-targets`, nextest, doc-tests, deny, llvm-cov),
   and in a final bookkeeping commit marks exactly the chunk's own
   backlog entry/entries ✅ DONE with a one-line note. Does **not**
   push or open a PR (P8's workflow_dispatch exception aside, §9).
   Reports gate results and deviations.
3. **Stage 3 — review agent**: reviews the entire branch diff versus
   the then-current `origin/main` against the chunk's acceptance
   criteria + AGENTS rules **plus the code-quality mandate below**,
   re-runs the gates, implements fixes for every finding on the branch
   (amend/reword, never duplicate-subject fixups). Its **last task is
   shipping**: push the branch, open the PR to `main` with `gh`, then
   `gh pr checks --watch` to completion — any failing check is a
   finding; diagnose, fix, push `--force-with-lease`, repeat until
   green. Reports a verdict. Never merges; never touches `main`
   (`main` is branch-protected — the user merges on GitHub).

Authorization granted by this dispatch: stage-3 agents push their
feature branch, open and update their PR, and iterate until CI is
green. **Nothing ever pushes to or merges into `main`.** P8's stage-2
agent may additionally push its branch (never a PR) because
workflow_dispatch iteration needs the branch on the remote — the WI-C
precedent.

**Code-quality mandate (stage 3, every chunk — user-directed):**
beyond functional review, the stage-3 agent evaluates:

- **Code quality**: naming, comment discipline (comments state
  constraints, not narration), idiomatic use of the approved stack,
  no dead code or speculative generality, test quality per AGENTS §5.
- **Single Responsibility Principle**: every module, actor, function,
  and type has exactly one reason to change; domain stays pure (no
  IO, no runtime), the engine orchestrates without legislating, the
  connector actor only syncs, the CLI only parses/dispatches — the
  crate-role boundaries of AGENTS §2 are the SRP line, and a
  violation of the boundary is a violation of SRP.
- **Single level of abstraction per function body**: every function
  body works at one level — a function either orchestrates
  (calls named operations) or implements (works in primitive data),
  never mixes; guard clauses and the happy path at the top level,
  detail pushed into named private functions. A body that interleaves
  channel-half plumbing with actor spawning and config surgery is a
  finding, even if it passes tests.

Findings from the mandate are fixed on the branch like any other
finding (refactor commits with proper `refactor(<module>):` subjects,
or amend where the commit is not yet shipped).

### 1.4 Parallel-with-GUI ground rules (the reason this plan exists — binding for every chunk)

The Phase 7 GUI implementation lands five chunks serially on `main`
while these lanes run. Every chunk of this plan MUST stay disjoint
from its scope:

**No-go zones** (owned by the GUI chunks — touching any of these is a
scope violation, not a merge conflict to resolve):

- `crates/taskboard-ui-slint/**` — entirely.
- `crates/taskboard-app/src/bootstrap.rs`, `src/config.rs`,
  `src/main.rs` — the GUI forwarder, the `[ui]` config table, the GUI
  entry composition (GUI chunks 1–2). P4's `config path` verb reuses
  `AppConfig::config_path`/`write_path` **read-only** from
  `commands/config.rs` — it must not edit `src/config.rs`; if a seam
  there is missing, STOP and report instead of widening.
- `AppState` shape the GUI renders: no change to `AppState.sync`'s
  singleton shape, no new `ReminderRef`/`EngineSignal` enum arms, no
  reshaping of `entities.rs`/`state.rs`/`messages.rs` types the views
  read. Additive private changes inside a chunk's own crate are fine;
  additive public domain fields need a recorded reason in the chunk
  plan.
- `.github/workflows/ci.yml` and the pre-commit hook (GUI chunks 1
  and 4). P3 explicitly does **not** remove the benchmark job's
  `continue-on-error` TODO — that rides the next CI-housekeeping
  round after the GUI merges.
- `release.yml`, `scripts/bump_version.sh`, the workspace version —
  GUI chunk 5 owns the v0.1.0 release.
- `docs/adr/` renumbering or new ADRs (GUI chunk 5 adds ADR 0015);
  P5 may append an additive completion note to ADR 0014 only.

**Shared-file expectations** (edit only your own lines; expect and
trivially resolve adjacency rebases):

| File | This plan | GUI plan |
|---|---|---|
| root `Cargo.toml` `[workspace.dependencies]` | P2 (reqwest `multipart` feature), P3 (criterion pin), P6 (rustyline file-history feature) | chunk 1 (slint pin) |
| `deny.toml` | P2/P3 only if new transitive licenses demand it | chunk 1 (Slint tri-license) |
| `.artifacts/taskboard-potential-improvements-backlog.md` | every chunk's final commit, own entries only | chunk 5 (its entries) |
| `docs/testing_strategy.org` §10 | P7 (gate deletions), P8 (cap policy) | none |
| `.github/workflows/miri.yml` | P8 only | none |

**Rebase cadence**: the GUI chunks merge to `main` one after another
for the coming days. Before every stage-3 ship, run
`git fetch origin --prune` and rebase the branch onto `origin/main`
dropping already-merged commits (the AGENTS §8 dance:
`git rebase --onto origin/main <tip-of-just-landed-branch> <branch>`;
`--force-with-lease` on push). With the file scopes above these
rebases are trivial; a *real* conflict is a signal the chunk crossed
a no-go zone — resolve it by shrinking the chunk, never by editing
GUI-owned files.

**v0.1.0 timing**: GUI chunk 5 releases v0.1.0. Chunks that merge
before its release PR ride v0.1.0; the rest simply land after the tag
and ride the next version. No chunk races or blocks the release.

## 2. Chunk P1 — `Retry-After` in the poll backoff (`feat/sync-nextcloud-retry-after`)

- **Covers**: backlog "[2026-10-10] `Retry-After` handling in the poll
  backoff". The client's retry policy ignores `Retry-After` on 429s
  and the poll backoff is a pure doubling curve
  (`crates/taskboard-sync-nextcloud/src/poll.rs:15`,
  `pub fn poll_backoff(streak, initial, max) -> Duration`); a
  rate-limiting server cannot tell the actor how long to wait.
- **Task**: surface the header's value from the client as data (typed
  error variant or response metadata — planner decides; both
  delay-seconds and HTTP-date forms must be considered, scope decided
  with a recorded reason), and thread it into the actor's next poll
  deadline as a **floor** on `poll_backoff`'s result. Keep
  `poll_backoff` pure (a floor parameter, no wall clock) so
  paused-time actor tests stay deterministic. Non-goals: no new
  endpoints, no push-path changes, no config surface.
- **Acceptance**: pure-function unit tests for the floor semantics
  (incl. floor-vs-curve precedence and clamping at `max`); a wiremock
  test serving `Retry-After` proving the client parse (typed value
  asserted, no text assertions); an actor-level paused-time test
  (`tokio::time::pause`/`advance`, never `sleep`) proving the next
  deadline respects the floor; gates green; backlog entry ✅ DONE.

**Prompt P1-1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk P1 of the parallel-during-GUI dispatch ("Retry-After in the poll backoff", branch feat/sync-nextcloud-retry-after) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1 (execution model + §1.4 GUI no-go zones) and §2 P1, and the backlog entry "[2026-10-10] Retry-After handling in the poll backoff".
2. Read the seams: crates/taskboard-sync-nextcloud/src/poll.rs (the pure poll_backoff), src/client.rs (the error/retry surface, how 429s surface today), src/actor.rs (where the poll deadline is computed), and the existing wiremock test tier's header-serving capability.
3. Create the worktree ../taskboard-WT-feat-sync-nextcloud-retry-after on a new branch feat/sync-nextcloud-retry-after off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-sync-nextcloud-retry-after-plan.md settling: how Retry-After travels out of the client (typed variant vs metadata — pick one, record why), delay-seconds vs HTTP-date scope (record the decision), the poll_backoff floor parameter shape, and the paused-time actor test design. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): sync retry-after poll backoff floor. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt P1-2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk P1 of the parallel-during-GUI dispatch ("Retry-After in the poll backoff").

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1/§2 P1 (the §1.4 GUI no-go zones apply), and the stage-1 plan (.artifacts/plans/*sync-nextcloud-retry-after*-plan.md) in the worktree ../taskboard-WT-feat-sync-nextcloud-retry-after (branch feat/sync-nextcloud-retry-after). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. Hard rules: poll_backoff stays pure (no Utc::now, no tokio timer access); no tokio::time::sleep in tests (pause/advance only); typed outcome assertions only — no text assertions.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Final commit: mark the backlog entry "[2026-10-10] Retry-After handling in the poll backoff" ✅ DONE with a one-line note.
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt P1-3 (review):**

```text
You are the stage-3 REVIEW agent for chunk P1 of the parallel-during-GUI dispatch (branch feat/sync-nextcloud-retry-after).

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1 (incl. the code-quality mandate and §1.4 GUI no-go zones) and §2 P1, and the stage-1 plan; then review the full branch diff (git diff origin/main...HEAD) in ../taskboard-WT-feat-sync-nextcloud-retry-after.
2. Functional focus: the pure seam survived (grep poll_backoff's body for clock/timer access); floor precedence is pinned by unit tests (floor-below-curve, floor-above-curve, floor-above-max clamping); the actor test advances paused time rather than sleeping; the wiremock test asserts the parsed typed value, not header text; no GUI no-go file appears in the diff.
3. Code-quality mandate: apply §1.3's mandate in full (this is a sync-crate chunk: the client only speaks HTTP, the actor only schedules — nothing else leaks in).
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: first git fetch origin --prune and rebase onto origin/main per dispatch §1.4 (trivial expected); then push the branch, open the PR to main with gh, and gh pr checks --watch to green — any failing check is a finding; diagnose, fix, push --force-with-lease, repeat. Never merge; never touch main.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks confirmation.
```

## 3. Chunk P2 — Deck attachment writes & content download (`feat/sync-nextcloud-attachment-content`)

- **Covers**: backlog "[2026-10-08] Deck attachment writes and content
  download", executed against the **evidenced** API surface of the D1
  spike report `.artifacts/reports/2026-10-14-deck-roundtrip-strategy-report.md`
  §3.2 (multipart round evidence) and §3.3 (attachment writes advance
  the card's entity stamp) — normative for wire facts. Today only the
  metadata list exists (`client.rs:487`, `attachments()`).
- **Task — client surface only**: `multipart/form-data` upload,
  update, restore, delete of card attachments, plus content download
  as raw bytes (a non-JSON response path beside `send_json` — planner
  designs the seam). Add the `multipart` feature to the workspace
  reqwest pin (root `Cargo.toml` — feature-add only, no version
  bump). Tier-1 wiremock tests for the CRUD paths; tier-2 (docker)
  coverage per the house Nextcloud tier strategy for at least upload
  + download + one update. **Scope fence**: no sidecar document
  format, no outbox/merge-policy changes, no Wave-1 fields — Wave 1
  consumes this surface later; nothing here decides sidecar semantics.
- **Traps (from the D1 evidence)**: multipart field names are exactly
  `type` + `file` (a `data=` field name 500s); the stored filename
  comes from the uploaded part's client filename (re-upload under a
  new name creates a second file; the same name 409s); attachment
  writes **advance the card's entity stamp** (§3.3) — tier-2 tests
  must expect conditional-read validators to invalidate.
- **Acceptance**: typed request/response models with serde
  roundtrips; wiremock CRUD tests (structural assertions); tier-2
  evidence in the PR body; `cargo deny` green with the multipart
  transitive tree; the scope fence holds (no domain/outbox diff);
  backlog entry ✅ DONE with a note that Wave 1 consumes the surface.

**Prompt P2-1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk P2 of the parallel-during-GUI dispatch ("Deck attachment writes & content download", branch feat/sync-nextcloud-attachment-content) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunk P1 (feat/sync-nextcloud-retry-after) is merged into main — the dispatch serializes lane A (verify with git log; if not, STOP and report).

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1 (execution model + §1.4 GUI no-go zones) and §3 P2, the backlog entry "[2026-10-08] Deck attachment writes and content download", and the D1 spike report .artifacts/reports/2026-10-14-deck-roundtrip-strategy-report.md §3.2/§3.3/§3.4 (normative wire facts: field names, filename semantics, 409 conflict, stamp advancement, payload limits).
2. Read the seams: crates/taskboard-sync-nextcloud/src/client.rs (attachments() at :487, send_json, the error surface), the crate's wiremock tier-1 harness, the docker tier-2 harness, and the root Cargo.toml reqwest pin.
3. Create the worktree ../taskboard-WT-feat-sync-nextcloud-attachment-content on a new branch feat/sync-nextcloud-attachment-content off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-sync-nextcloud-attachment-content-plan.md settling: the multipart body construction (typed, testable), the raw-bytes download seam beside send_json, endpoint coverage (upload/update/restore/delete/download) with the D1 facts baked into tests, tier-1 vs tier-2 split, and the reqwest feature addition. The scope fence is binding: no sidecar format, no outbox/merge changes. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): deck attachment content client surface. Do not implement; do not push.

Report: plan path, endpoint list, tier split, risks, estimated diff size.
```

**Prompt P2-2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk P2 of the parallel-during-GUI dispatch ("Deck attachment writes & content download").

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1/§3 P2 (the §1.4 GUI no-go zones and the scope fence apply), the D1 report §3.2/§3.3, and the stage-1 plan (.artifacts/plans/*sync-nextcloud-attachment-content*-plan.md) in the worktree ../taskboard-WT-feat-sync-nextcloud-attachment-content (branch feat/sync-nextcloud-attachment-content). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. Hard rules: multipart field names `type` + `file` exactly (D1 §3.2); the stored filename comes from the part's client filename; no text assertions; tier-2 tests must expect attachment writes to advance the card entity stamp (D1 §3.3); scratch payloads/captures never land in the repo — evidence lives in the PR description.
3. Run the full gate suite from AGENTS.md §10 (cargo deny included — the multipart transitive tree must pass) and make it green.
4. Final commit: mark the backlog entry "[2026-10-08] Deck attachment writes and content download" ✅ DONE with a one-line note that Wave 1's sidecar consumes this surface.
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, the tier-2 evidence summary, any deviation and why.
```

**Prompt P2-3 (review):**

```text
You are the stage-3 REVIEW agent for chunk P2 of the parallel-during-GUI dispatch (branch feat/sync-nextcloud-attachment-content).

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1 (incl. the code-quality mandate and §1.4 GUI no-go zones) and §3 P2, the D1 report §3.2/§3.3, and the stage-1 plan; then review the full branch diff (git diff origin/main...HEAD) in ../taskboard-WT-feat-sync-nextcloud-attachment-content.
2. Functional focus: the scope fence holds (no domain/outbox/merge diff, no sidecar semantics invented); multipart facts match the D1 evidence, not assumptions; the download path returns bytes without a JSON detour; tier-2 evidence (run URLs or captured outcomes summarized in the PR body) covers upload + download + update; the reqwest pin changed features only; deny green; no GUI no-go file in the diff.
3. Code-quality mandate: apply §1.3's mandate in full (the client speaks HTTP only; typed models; no speculative endpoints beyond the D1-documented surface).
4. Re-run the full gate suite yourself; any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: first git fetch origin --prune and rebase onto origin/main per dispatch §1.4 (trivial expected; root Cargo.toml adjacency with the GUI slint pin is the one hot spot); then push the branch, open the PR to main with gh, and gh pr checks --watch to green — any failing check is a finding; diagnose, fix, push --force-with-lease, repeat. Never merge; never touch main.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks confirmation.
```

## 4. Chunk P3 — Criterion benches for the persistence hot path (`test/storage-hot-path-benches`)

- **Covers**: backlog "[2026-10-09] Criterion bench for the persistence
  hot path". No `[[bench]]` targets exist; the CI `benchmark` job
  (ci.yml:219) already runs `cargo bench --workspace` and Bencher
  tracks it on `main` pushes — benches light up with **zero** ci.yml
  edits.
- **Task**: criterion benches in `taskboard-storage-sqlite` for
  `load()` hydration and one `apply` batch at N ∈ {100, 1 000,
  10 000} tasks, per the entry's sketch. Deterministic fixture
  construction (fixed seeds/values, no randomness); the sqlx offline
  mode CI pins (`SQLX_OFFLINE=true`) must hold — benches reuse the
  prepared query data like the tests do. Criterion joins the root
  `[workspace.dependencies]` pin (AGENTS §4). Non-goals: **no
  ci.yml edits** (the `continue-on-error` TODO removal rides the
  post-GUI CI-housekeeping round), no production-code changes; if a
  bench reveals a perf problem, it goes to the backlog — fixes are
  out of scope.
- **Acceptance**: `cargo bench` runs the new targets locally with
  sane output; clippy `--all-targets` clean (benches included);
  llvm-cov unaffected (bench targets excluded from coverage by
  construction — verify); fixtures deterministic (two runs, identical
  input shape); gates green; backlog entry ✅ DONE.

**Prompt P3-1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk P3 of the parallel-during-GUI dispatch ("Criterion benches for the persistence hot path", branch test/storage-hot-path-benches) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1 (execution model + §1.4 GUI no-go zones — note the explicit NO ci.yml edits) and §4 P3, and the backlog entry "[2026-10-09] Criterion bench for the persistence hot path".
2. Read the seams: crates/taskboard-storage-sqlite/src/repo.rs + connect.rs (load/apply hot path), the crate's existing test fixture builders (reuse them for bench fixtures), how CI pins SQLX_OFFLINE, and ci.yml's benchmark job (cargo bench --workspace — confirm no edit needed).
3. Create the worktree ../taskboard-WT-test-storage-hot-path-benches on a new branch test/storage-hot-path-benches off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-storage-hot-path-benches-plan.md settling: the bench target layout (criterion harness), deterministic fixture generation at the three N sizes, what exactly is measured (load hydration; one apply batch — define the batch), tempfile/isolation per bench iteration, and the criterion workspace pin. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): storage hot-path criterion benches. Do not implement; do not push.

Report: plan path, bench inventory, risks, estimated diff size.
```

**Prompt P3-2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk P3 of the parallel-during-gui dispatch ("Criterion benches for the persistence hot path").

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1/§4 P3, and the stage-1 plan (.artifacts/plans/*storage-hot-path-benches*-plan.md) in the worktree ../taskboard-WT-test-storage-hot-path-benches (branch test/storage-hot-path-benches). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan: criterion as a workspace-pinned dev-dependency of taskboard-storage-sqlite, the [[bench]] target(s), deterministic fixtures at N ∈ {100, 1000, 10000}. Hard rules: NO production-code changes; NO ci.yml edits; SQLX_OFFLINE holds; no sleeps.
3. Run the full gate suite from AGENTS.md §10 and make it green (clippy --all-targets includes the bench targets; verify llvm-cov ignores them), plus one local cargo bench smoke of the new targets.
4. Final commit: mark the backlog entry "[2026-10-09] Criterion bench for the persistence hot path" ✅ DONE with a one-line note.
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, the bench smoke summary, any deviation and why.
```

**Prompt P3-3 (review):**

```text
You are the stage-3 REVIEW agent for chunk P3 of the parallel-during-GUI dispatch (branch test/storage-hot-path-benches).

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1 (incl. the code-quality mandate and §1.4 GUI no-go zones) and §4 P3, and the stage-1 plan; then review the full branch diff (git diff origin/main...HEAD) in ../taskboard-WT-test-storage-hot-path-benches.
2. Functional focus: the diff contains zero production-code changes and zero ci.yml changes; fixtures are deterministic; the bench actually exercises load() and apply (not a stub); criterion is workspace-pinned exactly once; clippy --all-targets covers the benches.
3. Code-quality mandate: apply §1.3's mandate in full (benches are measurement code — one level of abstraction, named fixture builders, no copy-paste N-variants).
4. Re-run the full gate suite yourself plus a cargo bench smoke; any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: first git fetch origin --prune and rebase onto origin/main per dispatch §1.4 (root Cargo.toml criterion pin may sit adjacent to the GUI slint pin — trivial); then push the branch, open the PR to main with gh, and gh pr checks --watch to green — any failing check is a finding; diagnose, fix, push --force-with-lease, repeat. Never merge; never touch main.

Report: verdict, findings, what you fixed, PR URL, green-checks confirmation.
```

## 5. Chunk P4 — App honesty micro-batch: session-status flag + `config path` (`feat/app-status-config-dx`)

- **Covers**: backlog "[2026-10-14] In-session `status` should surface
  `connector_attached`" and "[2026-10-12] `taskboard config path`
  diagnostic verb". Two standalone commits, one branch.
- **Current state (verified on `main` @ `c501301`)**:
  `StatusView::server()` computes `connector_attached` from
  `app.connector_attached()` (`commands/status.rs:56-59`), but
  `StatusView::in_session()` (`status.rs:92-102`) hardcodes `None` —
  it predates ADR 0014's dynamic attach/detach. `AppConfig` exposes
  `config_path` (`src/config.rs:380`) and `write_path`
  (`src/config.rs:397`) read-only; `commands/config.rs` already hosts
  the pre-core `config print` (the `Local` IPC lane pattern).
- **Task**:
  (a) an in-session status constructor that takes the flag (or an
  `&App`-shaped seam that stays presentation-side), threaded from
  `dispatch_line`'s `Command::Status` arm via `app.connector_attached()`;
  (b) a `taskboard config path` verb beside `config print`: prints the
  resolved read path and the login write target, JSON and text, pure
  reuse of `config_path`/`write_path` — no side effects, no core boot.
  JSON-shape additions are pre-v0.1.0-sanctioned. Non-goals: no edits
  to `src/config.rs` (read-only reuse); no GUI-owned files.
- **Acceptance**: journey-level tests pin both surfaces additively
  (existing status/config journeys extended, none weakened); hermetic
  tests only; both backlog entries ✅ DONE.

**Prompt P4-1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk P4 of the parallel-during-GUI dispatch ("App honesty micro-batch: session-status flag + config path", branch feat/app-status-config-dx) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1 (execution model + §1.4 GUI no-go zones) and §5 P4, and the backlog entries "[2026-10-14] In-session status should surface connector_attached" and "[2026-10-12] taskboard config path diagnostic verb".
2. Read the seams: crates/taskboard-app/src/commands/status.rs (server() vs in_session() constructors, the emit path), src/commands/config.rs (config print's pre-core Local lane), src/config.rs READ-ONLY (config_path/write_path), the dispatch surface in commands/mod.rs + cli.rs, and the existing status/config journey tests.
3. Create the worktree ../taskboard-WT-feat-app-status-config-dx on a new branch feat/app-status-config-dx off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-app-status-config-dx-plan.md settling: the in-session constructor shape (flag parameter vs &App read — presentation-layer rule decides), the dispatch_line threading, the config path verb's clap shape and output view, and the additive journey coverage for both. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): app status honesty + config path verb. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt P4-2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk P4 of the parallel-during-GUI dispatch ("App honesty micro-batch").

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1/§5 P4 (the §1.4 GUI no-go zones apply — src/config.rs is read-only reuse), and the stage-1 plan (.artifacts/plans/*app-status-config-dx*-plan.md) in the worktree ../taskboard-WT-feat-app-status-config-dx (branch feat/app-status-config-dx). Recreate the worktree on that branch if missing.
2. Implement per the plan as two standalone commits ((a) session-status flag, (b) config path verb), TDD, hermetic tests only, typed outcome assertions — no text assertions on rendered output.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Final commit: mark both origin backlog entries ("In-session status should surface connector_attached", "taskboard config path diagnostic verb") ✅ DONE with one-line notes.
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, any deviation and why.
```

**Prompt P4-3 (review):**

```text
You are the stage-3 REVIEW agent for chunk P4 of the parallel-during-GUI dispatch (branch feat/app-status-config-dx).

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1 (incl. the code-quality mandate and §1.4 GUI no-go zones) and §5 P4, and the stage-1 plan; then review the full branch diff (git diff origin/main...HEAD) in ../taskboard-WT-feat-app-status-config-dx.
2. Functional focus: src/config.rs is untouched; the flag is read from the App, not re-derived from config; existing journeys extended additively, none weakened; config path is side-effect-free (no lock, no boot, no write); both commits stand alone.
3. Code-quality mandate: apply §1.3's mandate in full (the CLI layer only parses/dispatches; views stay dumb data).
4. Re-run the full gate suite yourself; any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: first git fetch origin --prune and rebase onto origin/main per dispatch §1.4; then push the branch, open the PR to main with gh, and gh pr checks --watch to green — any failing check is a finding; diagnose, fix, push --force-with-lease, repeat. Never merge; never touch main.

Report: verdict, findings, what you fixed, PR URL, green-checks confirmation.
```

## 6. Chunk P5 — Daemon-side attach IPC verb (`feat/app-ipc-attach-verb`)

- **Covers**: backlog "[2026-10-14] Daemon-side attach verb: connect a
  running daemon without restarting it" (ADR 0014 decision 7's
  follow-up). A one-shot `connect` persists config+secret but the
  running daemon never attaches (its boot already happened); the
  ceremony verbs are terminal-bound and refused over the socket —
  what the daemon needs is attach-from-persisted-state.
- **Current state (verified on `main` @ `c501301`)**: the additive-verb
  pattern is established by `Request::DetachConnector`/
  `Response::DetachConnector` (`ipc/protocol.rs:46`, `:85` — `v: 1`,
  no version bump for additive verbs); `App::attach_connector(&self,
  client: DeckClient)` exists (`bootstrap.rs:263`) with the
  `AlreadyAttached` guard; the frontend-agnostic ceremony functions
  live in `session_ceremonies.rs`.
- **Task**: an additive `AttachConnector`-shape verb: the client sends
  only the identity (server URL, username — **never the secret**);
  the daemon re-reads its config, loads its credential store, builds
  the `DeckClient`, and calls `attach_connector` (with the same
  boot-pull settle the REPL path uses). Typed outcomes mirror the
  detach vocabulary (`attached`/`already-attached`/`engine`/
  `draining`). The env-store caveat is documented behavior, not a
  blocker: attach works for the keyring store; for the env store the
  daemon's own environment governs. May append an additive completion
  note to ADR 0014 — no renumbering, no new ADR. Non-goals: no
  ceremony over the socket, no multi-connector lift, no GUI-owned
  files.
- **Acceptance**: IPC-level tests pin the verb (typed outcomes, the
  refused-secret invariant — a payload carrying a secret is a design
  failure); the daemon-side attach mirrors the REPL settle; backlog
  entry ✅ DONE; ADR 0014 note (if any) additive.

**Prompt P5-1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk P5 of the parallel-during-gui dispatch ("Daemon-side attach IPC verb", branch feat/app-ipc-attach-verb) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1 (execution model + §1.4 GUI no-go zones) and §6 P5, the backlog entry "[2026-10-14] Daemon-side attach verb", ADR 0014 (docs/adr/0014-connector-attach-session-login.org — decision 7 + the DetachConnector pattern), and ADR 0012 (the IPC envelope conventions).
2. Read the seams: crates/taskboard-app/src/ipc/protocol.rs (the DetachConnector request/response pair, the v-field convention), src/ipc/server.rs + client.rs (dispatch lanes), src/bootstrap.rs (App::attach_connector at :263, the guard, the boot-pull settle), src/session_ceremonies.rs (frontend-agnostic ceremony fns — where the client construction lives), and the WI-H/WI-J IPC test harness patterns.
3. Create the worktree ../taskboard-WT-feat-app-ipc-attach-verb on a new branch feat/app-ipc-attach-verb off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-app-ipc-attach-verb-plan.md settling: the request/response shapes (identity-only — prove the secret cannot ride), which client-side surface relays it (a flag on the existing connect verb vs a new verb — record the pick and why), how the daemon builds the DeckClient from its own config+store, the settle semantics, the typed-outcome vocabulary, and the ADR 0014 additive note. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): daemon-side attach IPC verb. Do not implement; do not push.

Report: plan path, wire shapes, decisions, risks, estimated diff size.
```

**Prompt P5-2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk P5 of the parallel-during-GUI dispatch ("Daemon-side attach IPC verb").

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1/§6 P5 (the §1.4 GUI no-go zones apply), and the stage-1 plan (.artifacts/plans/*app-ipc-attach-verb*-plan.md) in the worktree ../taskboard-WT-feat-app-ipc-attach-verb (branch feat/app-ipc-attach-verb). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD (IPC-level tests), every commit standalone. Hard rules: the wire payload carries identity only — never the secret; additive protocol variant with the v-field convention, no envelope version bump; typed outcomes mirror the detach vocabulary; the three ceremony verbs stay refused over the socket; no sleeps in tests (join/predicate polling).
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Final commit: mark the backlog entry "[2026-10-14] Daemon-side attach verb" ✅ DONE with a one-line note, and append the additive ADR 0014 completion note per the plan.
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, any deviation and why.
```

**Prompt P5-3 (review):**

```text
You are the stage-3 REVIEW agent for chunk P5 of the parallel-during-GUI dispatch (branch feat/app-ipc-attach-verb).

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1 (incl. the code-quality mandate and §1.4 GUI no-go zones) and §6 P5, ADR 0014, and the stage-1 plan; then review the full branch diff (git diff origin/main...HEAD) in ../taskboard-WT-feat-app-ipc-attach-verb.
2. Functional focus: no secret on the wire (diff the payload types character by character); the additive-verb convention followed exactly (v-field, no bump); the daemon builds its client from its OWN config+store, never from client-supplied credentials; the guard outcomes are typed and pinned by IPC tests; the ceremony refusals untouched; ADR note additive only; no GUI no-go file in the diff.
3. Code-quality mandate: apply §1.3's mandate in full (IPC only transports; the App owns lifecycle; no business logic in the relay).
4. Re-run the full gate suite yourself; any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: first git fetch origin --prune and rebase onto origin/main per dispatch §1.4; then push the branch, open the PR to main with gh, and gh pr checks --watch to green — any failing check is a finding; diagnose, fix, push --force-with-lease, repeat. Never merge; never touch main.

Report: verdict, findings, what you fixed, PR URL, green-checks confirmation.
```

## 7. Chunk P6 — REPL history persistence + uuid completion candidates (`feat/app-repl-history-uuid-completion`)

- **Covers**: backlog "[2026-10-11] REPL history persistence
  (file-backed)" and "[2026-10-12] REPL completion: uuid candidates
  for selectors". Two standalone commits, one branch.
- **Current state (verified on `main` @ `c501301`)**: the REPL uses
  in-memory `DefaultHistory` (`repl.rs:177`, entries added at `:198`);
  the rustyline workspace pin carries no features (root `Cargo.toml`
  — the file-history feature must be added, verify the exact v17
  feature name). Completion's `Kind` enum lives at
  `completion.rs:67`; title completion deduplicates, so two live
  tasks sharing a title yield one entry and the command answers
  `Ambiguous*`.
- **Task**:
  (a) file-backed history: load/save around the editor construction
  in `spawn_line_reader`; the open XDG data-dir decision (where the
  history file lives, 0600 permissions) is settled with a recorded
  decision — check the db-path default resolution for the existing
  data-dir precedent and follow it;
  (b) uuid candidates: extend the completion engine so hard-colliding
  titles (exactly the ambiguous case) additionally offer uuid-
  disambiguated candidates — display `title (uuid)`, insert the uuid;
  the menu stays quiet for unambiguous titles.
  Non-goals: no hints/ghost text (explicitly deferred by the
  completion plan), no remote-value caching (the boards-select entry
  stays open), no GUI-owned files.
- **Acceptance**: pure completion-engine tests (structural candidate
  assertions, no TTY); hermetic history tests (tempdir XDG/data
  overrides, permissions asserted); both backlog entries ✅ DONE.

**Prompt P6-1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk P6 of the parallel-during-GUI dispatch ("REPL history persistence + uuid completion candidates", branch feat/app-repl-history-uuid-completion) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1 (execution model + §1.4 GUI no-go zones) and §7 P6, the backlog entries "[2026-10-11] REPL history persistence (file-backed)" and "[2026-10-12] REPL completion: uuid candidates for selectors", and the completion plan .artifacts/plans/2026-10-12-app-repl-completion-plan.md (the Helper/Kind seams; hints explicitly deferred).
2. Read the seams: crates/taskboard-app/src/repl.rs (spawn_line_reader's editor construction at :177, history add at :198), src/completion.rs (the Kind enum at :67, the live-title generator), the app crate's existing XDG/dir-resolution precedent (db path default, config dirs), and the rustyline workspace pin.
3. Create the worktree ../taskboard-WT-feat-app-repl-history-uuid-completion on a new branch feat/app-repl-history-uuid-completion off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-app-repl-history-uuid-completion-plan.md settling: the rustyline file-history feature name for the pinned major, the history file location + permissions decision (follow the db-path data-dir precedent — record it), load/save failure behavior (a corrupt history file must not kill the session), and the uuid-candidate Kind extension (when candidates emit, display vs insert strings, dedupe rules). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): repl file history + uuid completion candidates. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt P6-2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk P6 of the parallel-during-GUI dispatch ("REPL history persistence + uuid completion candidates").

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1/§7 P6 (the §1.4 GUI no-go zones apply), and the stage-1 plan (.artifacts/plans/*app-repl-history-uuid-completion*-plan.md) in the worktree ../taskboard-WT-feat-app-repl-history-uuid-completion (branch feat/app-repl-history-uuid-completion). Recreate the worktree on that branch if missing.
2. Implement per the plan as two standalone commits ((a) file history, (b) uuid candidates), TDD, hermetic tests only (tempdir overrides; no TTY in CI tests); the completion engine's candidate functions stay pure; a corrupt history file degrades, never crashes.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Final commit: mark both origin backlog entries ("REPL history persistence (file-backed)", "REPL completion: uuid candidates for selectors") ✅ DONE with one-line notes.
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, any deviation and why.
```

**Prompt P6-3 (review):**

```text
You are the stage-3 REVIEW agent for chunk P6 of the parallel-during-gui-dispatch (branch feat/app-repl-history-uuid-completion).

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1 (incl. the code-quality mandate and §1.4 GUI no-go zones) and §7 P6, and the stage-1 plan; then review the full branch diff (git diff origin/main...HEAD) in ../taskboard-WT-feat-app-repl-history-uuid-completion.
2. Functional focus: history I/O never blocks or kills the editor thread on failure; permissions asserted in tests; uuid candidates appear ONLY for hard-colliding titles (the quiet-menu invariant); display/insert strings pinned structurally; hints/ghost text did not sneak in; no GUI no-go file in the diff.
3. Code-quality mandate: apply §1.3's mandate in full (candidate computation stays pure and testable; the REPL layer only wires).
4. Re-run the full gate suite yourself; any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: first git fetch origin --prune and rebase onto origin/main per dispatch §1.4 (the rustyline feature line in root Cargo.toml may sit adjacent to the GUI slint pin — trivial); then push the branch, open the PR to main with gh, and gh pr checks --watch to green — any failing check is a finding; diagnose, fix, push --force-with-lease, repeat. Never merge; never touch main.

Report: verdict, findings, what you fixed, PR URL, green-checks confirmation.
```

## 8. Chunk P7 — Deterministic stamps for clock-gated fixture builders (`test/domain-deterministic-stamps`)

- **Covers**: backlog "[2026-10-12] Deterministic stamps for
  clock-gated fixture builders". Three of the four realtime-clock
  Miri gates guard tests whose *fixtures* (not logic) read the clock:
  `test_support/contract.rs` stamps five `SystemClock.now()` outbox
  entries (sites at `:89`, `:177`, `:218`, `:223`, `:234` — verified),
  `outbox.rs`'s serde roundtrip stamps `Uuid::now_v7()` ×3 +
  `Utc::now()`, and `test_support/memory.rs`'s read-port test stamps
  one `Utc::now()`. Under Miri isolation each aborts, so the PendingOp
  roundtrip, the read-port contract, and the whole repository-contract
  harness get zero interpreted coverage.
- **Task**: replace the fixture stamps with fixed, **distinct,
  strictly increasing** timestamps (ordering semantics stay
  exercised): `contract.rs` routes its five stamps through a
  `clock_at`-style helper (the `Clock` trait seam in `clock.rs`; the
  harness already has `clock_at` helpers); `outbox.rs`/`memory.rs`
  use fixed `Utc.timestamp_opt` values and `Uuid::from_u128` ids.
  Delete exactly those three `#[cfg_attr(miri, ignore)]` gates; keep
  the two inherent ones (`clock.rs` `SystemClock`, `idgen.rs`
  `UuidV7Generator` — production code itself reads the clock there).
  Update `docs/testing_strategy.org` §10's scope note. Non-goals: no
  production-code changes (if a fixed stamp breaks a test, that is a
  finding to investigate per AGENTS §5's bug-vs-test rule — report,
  don't paper over).
- **Acceptance**: the three gates are gone, the two inherent ones
  remain; all touched tests green natively; one `workflow_dispatch`
  run of `miri.yml` (any branch) evidences the newly-interpreted
  tests passing under Miri (run URL in the PR body); backlog entry ✅
  DONE.

**Prompt P7-1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk P7 of the parallel-during-gui dispatch ("Deterministic stamps for clock-gated fixture builders", branch test/domain-deterministic-stamps) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md (especially §5's "a new test failing can be a bug" rule and the no-weakening rule), the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1 (execution model + §1.4 GUI no-go zones) and §8 P7, the backlog entry "[2026-10-12] Deterministic stamps for clock-gated fixture builders", and docs/testing_strategy.org §10.
2. Read the seams: crates/taskboard-domain/src/test_support/contract.rs (the five stamp sites), test_support/memory.rs, outbox.rs's serde roundtrip test, clock.rs (the Clock trait + the harness's clock_at helpers), and every #[cfg_attr(miri, ignore)] gate in the crate (grep) so exactly the right three die.
3. Create the worktree ../taskboard-WT-test-domain-deterministic-stamps on a new branch test/domain-deterministic-stamps off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-domain-deterministic-stamps-plan.md settling: the fixed-stamp scheme (distinct, strictly increasing — name the concrete values), the helper routing in contract.rs, the fixed uuid scheme (Uuid::from_u128 values), which gates die vs stay, and the testing_strategy §10 wording. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): deterministic fixture stamps for miri coverage. Do not implement; do not push.

Report: plan path, stamp scheme, risks, estimated diff size.
```

**Prompt P7-2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk P7 of the parallel-during-gui dispatch ("Deterministic stamps for clock-gated fixture builders").

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1/§8 P7, and the stage-1 plan (.artifacts/plans/*domain-deterministic-stamps*-plan.md) in the worktree ../taskboard-WT-test-domain-deterministic-stamps (branch test/domain-deterministic-stamps). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan: fixed distinct strictly-increasing stamps through the clock_at-style helper, fixed uuids, the three named gates deleted, the two inherent gates kept, testing_strategy.org §10 updated. Hard rules: ZERO production-code changes; if a test fails under the fixed stamps, apply AGENTS §5's bug-vs-test evaluation and report — never weaken the test.
3. Run the full gate suite from AGENTS.md §10 and make it green; then locally run the previously-gated tests under cargo miri (or state precisely why the local interpreter run was skipped and rely on stage 3's workflow_dispatch evidence).
4. Final commit: mark the backlog entry "[2026-10-12] Deterministic stamps for clock-gated fixture builders" ✅ DONE with a one-line note.
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, the miri evidence summary, any deviation and why.
```

**Prompt P7-3 (review):**

```text
You are the stage-3 REVIEW agent for chunk P7 of the parallel-during-gui dispatch (branch test/domain-deterministic-stamps).

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1 (incl. the code-quality mandate and §1.4 GUI no-go zones) and §8 P7, and the stage-1 plan; then review the full branch diff (git diff origin/main...HEAD) in ../taskboard-WT-test-domain-deterministic-stamps.
2. Functional focus: the diff contains zero production-code changes; exactly the three named gates died (grep the crate's remaining cfg_attr(miri) — the two inherent ones must remain); the fixed stamps are distinct and strictly increasing (ordering semantics still exercised); no test was weakened to fit.
3. Code-quality mandate: apply §1.3's mandate in full (fixture helpers named and single-purpose; no copy-pasted magic timestamps without the ordering rationale).
4. Re-run the full gate suite yourself; any failure is a finding. Then drive one workflow_dispatch run of miri.yml on the branch's remote ref (this requires pushing the branch first — authorized for this verification) and confirm the newly-interpreted tests pass; put the run URL in the PR body.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: git fetch origin --prune and rebase onto origin/main per dispatch §1.4 (testing_strategy §10 may adjacency-collide with P8 — trivial); then push (--force-with-lease if updating), open the PR to main with gh, and gh pr checks --watch to green — any failing check is a finding; diagnose, fix, push --force-with-lease, repeat. Never merge; never touch main.

Report: verdict, findings, what you fixed, PR URL, the miri run URL, green-checks confirmation.
```

## 9. Chunk P8 — Miri cap raise probe (`ci/miri-cap-probe`)

- **Covers**: backlog "[2026-10-12] Raise the Miri pipeline/state case
  caps on a measured probe". The tuned weekly job spends its budget
  unevenly (state job ≈ 8 min, properties ≈ 21 min, vs a ≈ 90-min
  bar) while the pipeline job runs its properties at
  `PROPTEST_CASES=1`; the caps were sized from priors, not the
  runner's curve. `miri.yml` already carries `workflow_dispatch`
  (verified) with per-job caps set via `-Zmiri-env-forward`.
- **Task**: one measured probe per candidate — raise the state-crate
  job's cap severalfold and the pipeline job's cap to 2 (≈ 15 min
  per interpreted case there), keep whatever measures ≤ 90 min per
  job, record the justification in the PR per testing_strategy §10's
  cap policy. **Hard rule**: every change is an environment/speed
  gate — never a weakened assertion; no test file changes at all.
  Verify the current caps on the branch first (the backlog's numbers
  may have drifted). Non-goals: no ci.yml, no test edits, no new
  jobs beyond what miri.yml already has.
- **Acceptance**: at least one green, in-budget workflow_dispatch run
  of the final config (URL in the PR); testing_strategy §10 records
  the new caps + the measurement; the diff is miri.yml +
  testing_strategy.org only; backlog entry ✅ DONE.

**Prompt P8-1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk P8 of the parallel-during-GUI dispatch ("Miri cap raise probe", branch ci/miri-cap-probe) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, the dispatch plan §1 (execution model + §1.4 GUI no-go zones — this chunk touches miri.yml ONLY among workflows) and §9 P8, the backlog entry "[2026-10-12] Raise the Miri pipeline/state case caps on a measured probe", docs/testing_strategy.org §10 (the cap policy + current scope), and .github/workflows/miri.yml.
2. Read the seams: miri.yml's four per-family jobs and their current PROPTEST_CASES values (verify on the branch — record the actual numbers), and the tuned run history the backlog cites (green run 38312493624 timings) plus any newer green runs visible via gh.
3. Create the worktree ../taskboard-WT-ci-miri-cap-probe on a new branch ci/miri-cap-probe off main.
4. Write the probe plan at .artifacts/plans/<today>-ci-miri-cap-probe-plan.md: the candidate ladder (which caps, which order), how each candidate is validated (workflow_dispatch from the pushed branch, gh run watch), the ≤ 90-min-per-job keep rule, and what testing_strategy §10 records. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): miri cap raise probe. Do not implement; do not push.

Report: plan path, the ladder, current-cap findings, risks.
```

**Prompt P8-2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk P8 of the parallel-during-gui dispatch ("Miri cap raise probe").

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1/§9 P8, and the stage-1 plan (.artifacts/plans/*ci-miri-cap-probe*-plan.md) in the worktree ../taskboard-WT-ci-miri-cap-probe (branch ci/miri-cap-probe). Recreate the worktree on that branch if missing.
2. Execute the probe ladder: push the branch (authorized for this chunk — workflow_dispatch iteration needs the remote ref; NEVER open a PR, never touch main), adjust .github/workflows/miri.yml caps one candidate at a time, drive workflow_dispatch runs (gh run watch), keep only changes whose jobs measure ≤ 90 min, and record each measurement. Hard rule: environment/speed gates only — zero test-file changes; if a candidate run fails for a non-time reason, that is a finding to report, not to hide by lowering a cap.
3. Document the final caps + measurements in docs/testing_strategy.org §10.
4. Final commit: mark the backlog entry "[2026-10-12] Raise the Miri pipeline/state case caps on a measured probe" ✅ DONE with a one-line note naming the green run.
5. The PR itself remains stage 3's last task. Never merge; never touch main.

Report: the probe log (candidate → run URL → timings → keep/revert), final config, deviations.
```

**Prompt P8-3 (review):**

```text
You are the stage-3 REVIEW agent for chunk P8 of the parallel-during-gui dispatch (branch ci/miri-cap-probe).

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md §1 (incl. the code-quality mandate and §1.4 GUI no-go zones) and §9 P8, and the stage-1 plan; then review the full branch diff (git diff origin/main...HEAD) in ../taskboard-WT-ci-miri-cap-probe.
2. Functional focus: the diff is miri.yml + testing_strategy.org §10 only (zero test files — the hard rule); every kept cap has a measured green run within budget linked in the PR; the §10 doc matches the shipped numbers; no ci.yml anywhere in the diff.
3. Code-quality mandate: apply §1.3's mandate as far as it reaches a CI-diff chunk — every change justified by a recorded measurement, no speculative caps kept "because it felt safe".
4. Re-run the standard gate suite where applicable (docs-only diff — fmt/clippy on the workspace confirm no accidental code drift); if the config changed during your review, drive a fresh workflow_dispatch run yourself and re-verify green-in-budget.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: git fetch origin --prune and rebase onto origin/main per dispatch §1.4; then push (--force-with-lease — the branch carries stage 2's iteration pushes), open the PR to main with gh — the description must link the green in-budget run(s) — and gh pr checks --watch to green; any failing check is a finding; diagnose, fix, push --force-with-lease, repeat. Never merge; never touch main.

Report: verdict, findings, what you fixed, PR URL, green in-budget run URL(s), green-checks confirmation.
```

## 10. Completion bookkeeping

### 10.1 Backlog closures per chunk

- **P1** closes "[2026-10-10] `Retry-After` handling in the poll
  backoff".
- **P2** closes "[2026-10-08] Deck attachment writes and content
  download" (note: Wave 1's sidecar consumes the surface) and leaves
  "[2026-10-14] Deck round-trip strategy" open (Wave 1 implements
  it).
- **P3** closes "[2026-10-09] Criterion bench for the persistence hot
  path"; the ci.yml benchmark-job `continue-on-error` TODO removal is
  explicitly NOT P3's — it rides the post-GUI CI-housekeeping round.
- **P4** closes "[2026-10-14] In-session `status` should surface
  `connector_attached`" and "[2026-10-12] `taskboard config path`
  diagnostic verb".
- **P5** closes "[2026-10-14] Daemon-side attach verb" (+ additive
  ADR 0014 note).
- **P6** closes "[2026-10-11] REPL history persistence (file-backed)"
  and "[2026-10-12] REPL completion: uuid candidates for selectors".
- **P7** closes "[2026-10-12] Deterministic stamps for clock-gated
  fixture builders".
- **P8** closes "[2026-10-12] Raise the Miri pipeline/state case caps
  on a measured probe".

### 10.2 Deliberately not dispatched here

- **Merge-policy family** (field-level merge refinement, add-wins
  OR-set labels, resurrect-as-new-card, clock-skew bias): the
  2026-10-12 backlog-batch triage deferral stands — wait for observed
  M1 real-server symptoms. (The add-wins item was floated during the
  lane analysis; it stays out for this reason.)
- **Tombstone purge**: trigger (local db growth in real use) unmet.
- **Wave-3 design rounds** (recurring tasks & habits, subtask
  hierarchy, reminder-rule sync protocol): zero code conflict, but
  front-running the approved wave order is a sequencing decision the
  user has not made — each can be dispatched anytime as its own
  docs-only round.
- **Encrypted-file credential store**: overlaps `src/config.rs` and
  the dependency/deny surface with GUI chunks 1–2; dispatch after the
  GUI merges.
- **Nextcloud major-version matrix**: overlaps `ci.yml` with GUI
  chunks 1 and 4; same timing.
- **Second-priority release targets** (macOS/aarch64): `release.yml`
  is GUI chunk 5's territory, and the entry itself waits on the Slint
  system-lib answers.
- **Multi-connector, standalone timers/alarms, undo/redo,
  multi-board**: each reshapes GUI-read types (`AppState.sync`,
  `ReminderRef`) or is GUI-coupled by design — not during Phase 7.

### 10.3 What opens next

- When GUI chunk 5 releases v0.1.0: a CI-housekeeping round can drop
  the benchmark job's `continue-on-error` TODO (P3's benches now
  exist) and revisit the parked ci.yml items (Nextcloud matrix).
- The **Wave 1 dispatch** (scheduled-start, effort/importance/
  urgency fields) consumes P2's attachment client surface + the D1
  report's sidecar design — P2 landing early de-risks its schedule.

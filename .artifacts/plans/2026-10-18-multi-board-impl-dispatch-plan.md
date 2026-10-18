# [Plan] Multi-board Implementation Round — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-18
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the 8 implementation chunks of the
  accepted multi-board wave (N boards × N connectors). No code in
  this commit.
- **Origin**: the accepted wave implementation plan
  `.artifacts/plans/2026-10-18-multi-board-wave-implementation-plan.md`
  (PR #340, merged 2026-10-18) — §2 design facts DF-1…DF-10, §3
  chunks 1–8 (branch names, files, behaviours, non-goals), §4
  quality gates, §5 sequencing & exit criteria, §6 completion
  bookkeeping, all normative. It chunks the accepted detailed design
  `.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`
  (PR #337; decision record §5 — ACCEPTED 2026-10-18, all nine
  recommendations, no deviations; §6 carries the ADR 0010 amendment
  text chunk 4 lands verbatim). The round's gate is open: the
  implementation plan's Status bullet (written after that plan's PR
  merges **and** remediation-wave-1 closes) is satisfied —
  remediation-wave-1 closed with PR #341 (merged 2026-10-18). Plus
  the user's 2026-10-18 dispatch request.
- **Status**: READY — chunk 1 dispatches once **this dispatch plan's
  own PR** merges; the round then runs 2 → 3 → 4 → 5 → {6 ∥ 7} → 8,
  each chunk's precondition being its predecessor's merge (§1
  dependency graph; MD-9). No chunk is gated on a mid-round human
  decision — every open question the round could face is already
  answered by an ACCEPTED decision or is a stage-1 planning decision.
  **Deliberately not dispatched here**: the entire implementation
  plan §1 Out list / design §7 backlog-candidates list (§10).

## 1. Chunks & ordering

| Chunk | Impl-plan slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | §3 chunk 1 | `feat/state-sync-status-map` | migration 0006 **whole** (binding-tag column + status rebuild), `AppState.sync` → `SyncStates(BTreeMap<BoardId, SyncStatus>)` + `aggregate()` fold, `ConnectorId` newtype, decode home in `spawn_storage_actor(default_connector)`, surfaces keep shapes via the fold, snapshot pass 1; **closes #298** | M |
| 2 | §3 chunk 2 | `feat/domain-board-scoped-commands` | the vocabulary motion: nine board-scoped `StateCommand` arms + `Execute` wire mirrors, error-set motion (−`NoBoard` −`BoardExists`, +`UnknownBoard` +`BoardDeleted`, `BoardAlreadyBound` widened), app mirrors, CLI `--board` + single-live default + typed ambiguity refusal, marked interim fills (REPL + UI), snapshot pass 2 | L |
| 3 | §3 chunk 3 | `feat/state-board-ceremonies` | `UnbindBoard` + `ReconcileByTitle` commands, `RemoteBoardInUse` refusal, bind-wipe rule with counted receipt, honest stranding, `destruct_on`/`read_scope` board-parameterised, `boards unbind`/`boards reconcile` verbs, bind ceremony stops sending `SetBoard`; **closes #170 and #179** | M |
| 4 | §3 chunk 4 | `feat/sync-connector-modules` | `ConnectorModule` trait + `NextcloudModule` + bootstrap registry loop (N=1), connector-tagged `Board.remote`, derived per-cycle board set with per-board cache keys, `SetBoard` retired **in the same motion**, ADR 0010 amendment (design §6 verbatim), v0.1.x additive-upgrade boot test | M |
| 5 | §3 chunk 5 | `feat/app-connector-registry` | `[[sync.connectors]]` decode + the typed-rejection list, shape-aware ceremony writer, N-actor registry + named routing + `ConnectorNameInUse`, per-connector events + the all-down authority, two-connector tier-2 exit criterion; **closes #187** | L |
| 6 | §3 chunk 6 | `feat/app-cli-connector-verbs` | additive `status` answer (`connectors: […]` + `boards: […]`, `sync` byte-shape unchanged), `--connector` flags, `ReplSession.current_board` + `boards use` (replaces chunk 2's interim fill), completion growth, the move-board exercise | M |
| 7 | §3 chunk 7 | `feat/ui-board-switcher` | `ViewState.board` + seed rule + `select_board`, `board.next/previous/select` actions + keymap rows, router dispatch, `IdTables` **not** re-keyed, `board-bar.slint` + compact degeneration, scripted keyboard-journey leg | M |
| 8 | §3 chunk 8 | `docs/multi-board-wave` | `docs/architecture.org` + `README.org` amendments, CHANGELOG promotion from per-chunk Unreleased lines, the wave exit verification record; **closes #125** — the wave exit | S |

**Dependency graph** (implementation plan §5): strictly serial spine
`1 → 2 → 3 → 4 → 5 → {6 ∥ 7} → 8`, with the user merging each PR on
GitHub before the next chunk's stage 1 starts. Chunk 1 lands **alone
first** — standing decision P1-1 (2026-10-17): nothing stacks on the
singleton it removes. Chunks 6 and 7 both gate on chunk 5's merge and
are mutually order-independent (disjoint crates: app vs ui) —
dispatch them concurrently as separate sessions or serially in
either order. Chunk 8 gates on **both** 6 and 7.

**Chunk 7's parallel option** (implementation plan §5): chunk 7's
true dependencies are chunks 1 + 2 (the per-board map and the
board-scoped command surface), so it *may* be dispatched in parallel
with 3–5 when the round wants it. **This dispatch plan dispatches
the serial spine** — chunk 7's prompts below say "after chunk 5"
(MD-1). The option stays recorded here: to invoke it, the user
dispatches prompt 7.1 once chunk 2 has merged instead, treats chunks
7 and 3–5 as concurrent sessions (never sharing worktrees), and
chunk 8 then waits for **all** of 3–7. Either way the stacked-PR /
rebase-dance rules below apply unchanged.

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); chunks 6 ∥ 7 (or 7 ∥ 3–5 under the
parallel option) may run concurrently as separate sessions; never
two agents share a worktree. Every chunk works in
`../taskboard-WT-<branch>` next to the project root (AGENTS §8),
branched off the **then-current `main`** — its predecessor has
merged, so **no PR is ever stacked** (MD-2). Disk hygiene (AGENTS §8
worktree lifecycle): any agent creating or recreating a worktree
runs `scripts/worktree-doctor.sh` from the main checkout first —
nonzero exit (free disk below the floor) stops the chunk; stage 3
removes the worktree as its final handoff task.

**Shared execution model** (binding for every chunk):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current
   `main`, re-locates the chunk's named symbols at its own start
   (implementation plan §2: finding IDs and symbols are the stable
   keys; line numbers anywhere in the plans are plan-day
   references), writes the TDD-first plan at
   `.artifacts/plans/<today>-multi-board-<slug>-plan.md` (MD-6) in
   the house `[Plan]` style, resolving every open question with a
   recorded decision, commits it (`docs(plan): …`). No
   implementation, no push. The implementation plan's §2/§3/§4/§5/§6
   and the design sections each chunk names are normative input, not
   suggestions — deviations need a recorded reason, and a deviation
   from an ACCEPTED decision (design §5) goes back to the user
   instead (MD-7).
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing), executes per the
   approved plan — TDD per `docs/testing_strategy.org`, every commit
   standalone (rebase-merge hygiene), the full AGENTS §10 gate suite
   green. Does **not** push or open a PR. Reports deviations.
3. **Stage 3 — review agent** — *protocol amended 2026-10-18; the
   dispatch-plan template carries the same text*: **begins by
   rebasing onto `main`** — `git fetch origin --prune`, then rebase
   the branch onto `origin/main` (AGENTS §8's rebase dance: resolve
   nothing unless real conflicts appear; drop already-merged
   duplicate commits), and push `--force-with-lease` if the rebase
   rewrote history; the review examines the diff against the
   **post-rebase** base. Then: reviews the entire branch diff versus
   that base against the chunk's acceptance criteria + AGENTS rules
   **plus the mandate below**, re-runs whatever gates apply,
   implements fixes for every finding on the branch (amend/reword,
   never duplicate-subject fixups). Its last tasks are shipping:
   push the branch, open the PR to `main` with `gh`, watch
   `gh pr checks --watch` to completion — any failing check is a
   finding; diagnose, fix, push `--force-with-lease`, repeat until
   green — **and then verify the PR's merge state**:
   `gh pr view --json mergeStateStatus` must report `CLEAN`;
   `BEHIND` or `DIRTY` ⇒ rebase onto `origin/main`, push
   `--force-with-lease`, let the checks re-run, and repeat; any
   other state ⇒ diagnose before handing off. **Then teardown**:
   remove the now-disposable worktree — from the project root,
   `git worktree remove --force ../taskboard-WT-<branch>` (everything
   precious is pushed; a later round that resumes the chunk
   recreates it on the branch). The handoff contract is **"PR open +
   checks green + merge state `CLEAN` + worktree removed"** so the
   user's merge on GitHub needs no additional rebase and the chunk
   leaves no 36–55G of build output behind. Reports a verdict. Never
   merges; never touches `main`.

**Code-quality mandate** (stage 3, chunks 1–7 — implementation
chunks; copied verbatim from the connect-flow implementation
dispatch plan §1): beyond functional review, the stage-3 agent
evaluates:

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
  body works at one level — a function either orchestrates (calls
  named operations) or implements (works in primitive data), never
  mixes; guard clauses and the happy path at the top level, detail
  pushed into named private functions. A body that interleaves
  channel-half plumbing with actor spawning and config surgery is a
  finding, even if it passes tests.

Findings from the mandate are fixed on the branch like any other
finding (refactor commits with proper `refactor(<module>):` subjects,
or amend where the commit is not yet shipped).

**Evidence & artefact mandate** (stage 3, chunk 8 — the docs chunk;
copied verbatim from the design-round dispatch plan §1, with the
tracker clause scoped by MD-4):

- every factual claim carries evidence (the command run + observed
  outcome, or a verifiable source + what it states) and experiments
  are reproducible from the artefact's method section;
- recommendations are decision-grade (options, trade-offs, exactly
  **one** recommendation, consequences for later work);
- repo hygiene (scratch code and captured payloads never land —
  evidence is excerpts inside the artefact; the PR diff is exactly
  the chunk's artefacts);
- scope discipline (out-of-scope ideas are recorded in the
  deliverable's backlog-candidates section for the user to promote
  to GitHub issues per AGENTS §7 — **no autonomous issue creation,
  no tracker edits beyond the §10 bookkeeping, no production
  code**).

**Round-wide evidence clauses** (bind all eight chunks): the PR diff
is exactly the chunk's artefacts — scratch code, probe crates, and
captured payloads never land (evidence lives in the stage reports
and the chunk-8 exit report); out-of-scope ideas discovered mid-chunk
go to the deliverable's backlog-candidates section (chunk 8's report)
for the user to promote, never into autonomous issue creation; the
only tracker edits in the round are the §10 bookkeeping (MD-4).

**Recorded method decisions** (design-round work-plan style; these
resolve every method question this dispatch plan faced):

- **MD-1 — chunk 7 placement**: the serial spine is dispatched
  (7 after 5, concurrent with 6). Rationale: 6 ∥ 7 already
  parallelizes the tail; pulling 7 alongside 3–5 buys little (3–5 is
  the long pole either way) while adding rebase exposure across
  three in-flight chunks and forcing chunk 8 to wait on all of 3–7
  explicitly. The implementation plan's parallel option stays
  user-invocable exactly as §1's dependency graph describes.
- **MD-2 — stacking policy**: no PR is ever stacked by default —
  each chunk branches off then-current `main` after its predecessor
  merged, so phantom conflicts cannot arise; concurrent chunks
  (6 ∥ 7) touch disjoint crates and the stage-3 rebase-first +
  `CLEAN` protocol absorbs any drift. If the user stacks anyway
  (or invokes MD-1's option with overlapping flight), every stacked
  branch does AGENTS §8's rebase dance proactively after each merge:
  `git fetch origin --prune; git rebase --onto origin/main
  <tip-of-the-branch-that-just-landed> <my-branch>; git push
  --force-with-lease` — never plain `--force`.
- **MD-3 — mandate split**: code-quality mandate (verbatim above) for
  chunks 1–7; evidence & artefact mandate for chunk 8; the
  round-wide evidence clauses for all eight.
- **MD-4 — tracker-edit mechanics**: closures ride the PR body —
  stage 3 writes `Closes #N` for the issues the chunk closes (impl
  §6 table), so GitHub closes them on merge and nobody edits the
  tracker by hand; annotations are exactly **one progress comment
  per mapped issue per chunk**, posted by stage 3 when the PR opens
  (e.g. chunk 2 on #125: "board-scoped vocabulary landed (<PR
  URL>)"). Nothing else: no label/milestone edits, no new issues —
  the round "does not create, label, or close anything beyond the
  five milestone issues" (impl §6).
- **MD-5 — tier-2 honest recording**: chunks 4–6's dockerized
  Nextcloud tests reuse the existing tier-2 harness conventions (the
  same corpus implementation plan §4 points at) — no new harness
  mechanics. When a runner lacks docker, the chunk **records that
  honestly** — in the stage report and the PR body (which tier-2
  tests ran, which could not and why) — never a silent skip; chunk
  8's exit report cites those records when scoring the tier-2 exit
  criteria.
- **MD-6 — stage-1 plan paths**: one work plan per chunk at
  `.artifacts/plans/<today>-multi-board-<slug>-plan.md`, slugs
  `sync-status-map`, `board-scoped-commands`, `board-ceremonies`,
  `connector-modules`, `connector-registry`, `cli-connector-verbs`,
  `ui-board-switcher`, `wave-exit-docs`. This dispatch plan itself
  ships without a separate stage-1 work plan — this MD section is
  its recorded-decisions artefact.
- **MD-7 — normative rank**: binding, in order: the implementation
  plan §2 (DF-1…DF-10) / §3 chunk sections / §4 / §5 / §6, then the
  design doc's per-chunk-named §3.x sections, its §5 ACCEPTED
  record, and its §6 ADR text (chunk 4 lands it verbatim). Design
  §4's landing-chunk column is advisory — superseded by the
  implementation plan §3's re-chunk record (the sketch's chunk 2
  split into 2 + 4; `SetBoard`'s retirement moved to 4; chunk 1
  carries the whole of 0006). Recorded-reason deviations are allowed
  against prose; a deviation from an ACCEPTED decision (design §5)
  goes back to the user before it is implemented.
- **MD-8 — anchor discipline**: stage-1 agents re-locate every named
  symbol at their own start; finding IDs and symbols are the stable
  keys, line numbers are plan-day references (implementation plan
  §2's own rule — repeated here because every chunk's prompts cite
  symbols from the design's re-verified table).
- **MD-9 — precondition mechanics**: each chunk's stage-1 prompt
  verifies its predecessor's branch is merged on `origin/main`
  (command inline in the prompt); chunk 1 additionally gates on this
  dispatch plan's own PR merging; chunk 8 gates on 5 **and** 6
  **and** 7 (on all of 3–7 if MD-1's option was invoked). A failed
  precondition = STOP and report, never proceed on a stale base.

Agent-session count: **24** (8 chunks × 3 stages), plus this
docs-only dispatch-plan chunk you are reading (the round's chunk 0).

## 2. Chunk 1 — SyncStatus per-board map + migration 0006, whole (`feat/state-sync-status-map`)

- **Covers** (implementation plan §3 chunk 1 — normative; design
  §3.2 option (a) ruled by the user as D-W9, §3.9 the 0006 sketch +
  decode home + rollback story, §3.3 the `ConnectorId` mint; DF-1
  status-payload slice, DF-3 mint chain head, DF-4 decode rule,
  DF-9 migration + snapshot pass 1, DF-10 topology invariant): new
  `crates/taskboard-storage-sqlite/migrations/0006_multi_board_sync_status.sql`
  landing **whole** — both (a) `boards.connector TEXT NULL` and (b)
  the `sync_status` rebuild (per-board PK, legacy-singleton backfill
  into the ≤1 bound live board, no seed, board-less singleton
  dropped); storage `repo.rs` (status load + upsert loop re-keyed by
  board), `codec.rs` (boards row codec: NULL decodes as the default
  connector, upserts write non-NULL), `actor.rs`
  (`spawn_storage_actor(default_connector: ConnectorId)`); domain
  `state.rs` (`AppState.sync` → `SyncStates(BTreeMap<BoardId,
  SyncStatus>)` + `get(board)` + `aggregate()`; global
  `last_updated` **unchanged**), `ids.rs` (`ConnectorId` newtype
  wrapping `String`), `persistence.rs` (`PersistedState.sync`
  shape); state `core.rs` (`sync_status()` getter,
  `mark_syncing(board)`), `actor.rs` (`ingest_sync_report` /
  `append_status_if_changed` re-keyed by the report's board;
  `go_offline` marks every *bound* board's row — today's semantics
  bit-for-bit in the one-connector world); app surfaces reading the
  fold: `commands/status.rs`, `commands/output.rs`,
  `commands/sync.rs` (wait decision table), `bootstrap.rs`
  (`wait_for_boot_pull`; passes `default_connector` into the storage
  spawn); ui `presentation.rs` (`SyncBadge` reads the fold; the
  selected-board read is chunk 7's). **Non-goals**: no plural
  connectors (chunk 5); no `Board.remote` widening — it stays
  `Option<RemoteBoardId>` until chunk 4 (the codec writes the
  default id from the spawn parameter, not a domain field); no
  `StateCommand` shape changes (chunk 2); no `SyncCommand` changes
  (chunk 4).
- **Acceptance**: migration lands whole in one file, one
  transaction, per design §3.9's sketch against 0001/0005
  conventions (chrono TEXT timestamps never in ORDER BY, no
  triggers, plain SQL in-transaction, no down-migration); the
  migration round-trip test mirrors the `connect.rs` round-trip
  pin's shape (build a pre-0006 db → migrate → the bound board's row
  decodes; the singleton is gone); a board-less standalone singleton
  is dropped (the design's recorded, priced deviation from 0005's
  bit-for-bit bar); every existing surface keeps its shape via
  `aggregate()` (daemon `status` keeps an aggregate-shaped `sync`
  field); **snapshot pass 1** is the one deliberate `insta`
  regeneration for the `AppState` widening (the
  `persisted_state`/`published_app_state` pairs across the 20-file
  corpus), its accepted diff reviewed as "the shape change, nothing
  else", no `.pending` snapshots ship; gates green; PR body carries
  `Closes #298`; annotation comment on #125 (impl §6).
- **Traps for stage 3** (from DF-4/DF-9 + design §3.9): the
  migration landing split (status rebuild without the
  `boards.connector` column, or vice versa) is a finding — P1-1
  exists because the singleton removal and the tag column are one
  file; the decode rule's resolution home is **only**
  `spawn_storage_actor`'s parameter — a boot-time UPDATE pass
  claiming legacy NULLs is the design's rejected alternative and a
  finding; `Board.remote` still `Option<RemoteBoardId>` (a domain
  binding type in this chunk is scope creep into chunk 4);
  `go_offline` writing a global phase instead of per-bound-board
  rows loses the chunk's semantics; `last_updated` widened to a map
  is a finding (DF-1: unchanged); snapshot diffs beyond the widening
  shape (value changes that are not the map) are findings.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the multi-board implementation round ("SyncStatus per-board map + migration 0006, whole", branch feat/state-sync-status-map) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: this round's dispatch plan .artifacts/plans/2026-10-18-multi-board-impl-dispatch-plan.md is on origin/main (git fetch origin --prune, then git cat-file -e origin/main:<path>). If it is not, STOP and report instead of planning.

1. Read AGENTS.md, then the implementation plan .artifacts/plans/2026-10-18-multi-board-wave-implementation-plan.md (§2 facts DF-1/DF-3/DF-4/DF-9/DF-10, §3 chunk 1 — normative: files, behaviours, non-goals; §4 gates; §5 sequencing; §6 bookkeeping), the design doc .artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md (§3.2 option (a) + the wire-compat consequence, §3.9 the 0006 SQL sketch + decode home + snapshot-regeneration rule, §3.3 the ConnectorId minting/typing paragraphs), and the dispatch plan §1/§2.
2. Read the current seams you will extend (re-locate every symbol yourself; line numbers in the plans are plan-day references): crates/taskboard-storage-sqlite/src/{repo.rs (status load + upsert loop), codec.rs (boards row codec), actor.rs (spawn_storage_actor)}, migrations/0001_init.sql and 0005 (conventions + precedents), crates/taskboard-domain/src/{state.rs (AppState.sync, SyncStatus, SyncPhase), ids.rs (RemoteBoardId — the newtype pattern to mirror), persistence.rs (PersistedState.sync, SyncStateReader)}, crates/taskboard-state/src/{core.rs (sync_status/mark_syncing), actor.rs (ingest_sync_report, append_status_if_changed, go_offline)}, crates/taskboard-app/src/{commands/status.rs, commands/output.rs, commands/sync.rs, bootstrap.rs (wait_for_boot_pull, the storage spawn call)}, crates/taskboard-ui-slint/src/presentation.rs (SyncBadge), plus the suites you must not break: the state-engine insta corpus (crates/taskboard-state/tests/) and the storage round-trip tests.
3. Run scripts/worktree-doctor.sh from the project root /home/viktoria/src/taskboard first (nonzero exit = free disk below the floor: STOP and report instead of planning), then create the worktree ../taskboard-WT-feat-state-sync-status-map on a new branch feat/state-sync-status-map off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-multi-board-sync-status-map-plan.md, settling at minimum: the final 0006 SQL against design §3.9's sketch (verify the sketch's assumptions against the live schema — column names, the singleton seed, the FK shape); the SyncStates newtype's exact API (get/aggregate semantics — the aggregate fold rule for phase/pending_ops the back-compat surfaces consume); the ConnectorId newtype (derives, serde, the kebab validation question — what lands here vs chunk 5's config validation); the decode home plumbing (who derives default_connector at bootstrap and how it reaches spawn_storage_actor); the mutator re-keying plan (mark_syncing(board), ingest re-key, go_offline over bound boards); the surface-by-surface aggregate-adoption list; the round-trip migration test design; the snapshot pass-1 catalogue and the review rule for the accepted diff; the full test matrix (storage, domain, engine, surfaces) with typed outcomes per AGENTS §5. Resolve every open question with a recorded decision; deviations from the implementation plan or design need a recorded reason.
5. Commit it as: docs(plan): per-board sync status map and migration 0006. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the multi-board implementation round ("SyncStatus per-board map + migration 0006, whole").

1. Read AGENTS.md, the implementation plan (§2 DF-1/DF-3/DF-4/DF-9/DF-10, §3 chunk 1, §4), the design doc (§3.2, §3.9, §3.3 mint paragraphs), the dispatch plan §1/§2, and the stage-1 plan (.artifacts/plans/*multi-board-sync-status-map*-plan.md) in the worktree ../taskboard-WT-feat-state-sync-status-map (branch feat/state-sync-status-map). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone (rebase-merge hygiene). The migration is one file, one transaction, 0001/0005 conventions (no triggers, no down-migration, chrono TEXT timestamps never in ORDER BY); Board.remote stays Option<RemoteBoardId>; no StateCommand/SyncCommand shape changes; typed outcomes everywhere, never text assertions; no sleeps — tokio::time::pause()/advance(), poll-a-predicate-with-deadline, join handles; fakes over mocks. New .rs files carry their crate's SPDX header (domain/state/storage: MIT OR Apache-2.0; app/ui: AGPL-3.0-only) and #![forbid(unsafe_code)] holds everywhere except taskboard-ui-slint.
3. Run the full gate suite from AGENTS.md §10 and make it green (the CI-identical three-command llvm-cov sequence included). The insta pass is ONE deliberate regeneration for the AppState widening — review the accepted diff as "the shape change, nothing else"; no .pending snapshots ship.
4. Bookkeeping: add this chunk's user-visible CHANGELOG.md Unreleased line (chunk 8 promotes it). Do NOT touch any tracker issue — closures/annotations are stage 3's PR-body/comment mechanics (dispatch plan MD-4). No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the multi-board implementation round (branch feat/state-sync-status-map).

1. First, rebase onto main: in ../taskboard-WT-feat-state-sync-status-map run git fetch origin --prune, then git rebase origin/main (drop already-merged duplicate commits; resolve only real conflicts), and git push --force-with-lease if the rebase rewrote history. Review the full branch diff (git diff origin/main...HEAD) against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DF-1/DF-3/DF-4/DF-9/DF-10, §3 chunk 1), the design doc (§3.2, §3.9, §3.3 mint paragraphs), the dispatch plan §1/§2 (the code-quality mandate applies to you in full), and the stage-1 plan.
3. Functional focus — this chunk's specific traps: the migration lands whole (both the boards.connector column and the status rebuild in the single 0006 file — either half alone is a finding); DF-4 decode rule exact (NULL decodes as spawn_storage_actor's default_connector; every upsert writes non-NULL; no boot-time UPDATE pass — the design's rejected alternative); Board.remote still Option<RemoteBoardId> — any domain binding widening is scope creep into chunk 4; global last_updated unchanged (a map widening is a finding); go_offline marks every bound board's row (a surviving global-phase write loses the per-board semantics); existing surfaces keep their shapes via aggregate() (diff the status/sync/wait-table surfaces — shape drift beyond the plan is a finding); snapshot pass 1 discipline (the accepted insta diff is the map widening, nothing else; no .pending files ship); the round-trip migration test exists and mirrors the connect.rs pin's shape (pre-0006 db → migrate → bound board's row decodes, singleton gone); no test weakened to pass (AGENTS §5).
4. Code-quality mandate: (a) code quality — naming, comment discipline (constraints not narration), no speculative generality (the newtype earns its keep; no plural-connector scaffolding — chunk 5's), test quality per AGENTS §5; (b) SRP — the domain carries data + pure functions only (ConnectorId lands in ids.rs, no config knowledge), storage owns persistence only, the engine owns state mutation only, surfaces only read; (c) single level of abstraction per function body — mixed-level bodies are findings even when tests pass.
5. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
7. Ship: push the branch, open the PR to main with gh (PR body carries "Closes #298" — the merge closes it; MD-4), then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green. Then verify gh pr view --json mergeStateStatus reports CLEAN; BEHIND/DIRTY ⇒ rebase onto origin/main, push --force-with-lease, let checks re-run, repeat; any other state ⇒ diagnose. Post the annotation comment on #125 (one progress note: map + 0006 landed, PR URL; MD-4). Never merge; never touch main.
8. Teardown: from the project root, git worktree remove --force ../taskboard-WT-feat-state-sync-status-map (everything is pushed; a resuming round recreates the worktree on the branch via git worktree add).

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation.
```

## 3. Chunk 2 — Domain vocabulary motion: board-scoped commands (`feat/domain-board-scoped-commands`)

- **Covers** (implementation plan §3 chunk 2 — normative; design §3.1
  all four vocabularies, the nine-arm table, the caller-side
  burden; §3.9 snapshot rule; DF-1 whole, DF-2 selection homes,
  DF-9 snapshot pass 2, DF-10): domain `command.rs` — the nine
  board-scoped arms gain `board: BoardId` (`CreateStack`,
  `CreateLabel`, `RenameBoard`, `SetBoardColor`, `SetBoardArchived`,
  `DeleteBoard`, `ResetBoard`, `BindBoard`; `CreateBoard` unchanged
  with its single-live-board refusal **lifted** — always mints);
  `live_board()` → `live_board_of(app, board)` (lookup + liveness);
  error set: −`NoBoard` −`BoardExists`, +`UnknownBoard(BoardId)`
  +`BoardDeleted(BoardId)`, `BoardAlreadyBound` widened to `{ board,
  bound }`; `messages.rs` Execute wire mirrors; app
  `commands/mod.rs` mirror widening; `cli.rs` +
  `commands/boards.rs`/`stacks.rs`/`labels.rs` — `--board <uuid>`
  flag on board-scoped verbs + the single-live-board default
  resolved at invocation with the **typed ambiguity refusal** when
  zero or ≥2 live boards make it ambiguous (DF-2); mechanical
  **interim fills** at the remaining call sites — `repl.rs`
  dispatch sites and the UI's command-construction sites resolve
  the first live board inline, each marked interim (chunks 6/7
  replace them). Outbox ops: **zero diff, verified**. **Snapshot
  pass 2** — the command-corpus regeneration. **Interim state**
  (stated loudly, DF-10's honest-visibility cousin): after this
  chunk multiple boards and bindings are engine-possible while the
  sync actor still syncs its first bound board only — per-board
  `pending_ops` climbs on the unsynced boards (chunk 1's map);
  multi-board is **not user-advertised** until chunk 5; the round
  must not "fix" the interim out of order (it closes at chunk 4).
  **Non-goals**: no ceremony redesign (chunk 3 — `select_on`'s
  app-side `BoardAlreadyBound` pre-check keeps compiling against
  the widened variant); no real selection homes (interim fills
  only); `SetBoard` kept (retires in chunk 4).
- **Acceptance**: all four vocabularies moved in one chunk —
  commands + Execute wire + `CommandError` + app mirrors (the
  status payload landed in chunk 1; the design §3.1 "same motion"
  prose is satisfied across chunks 1+2 per the implementation
  plan's re-chunk record, MD-7 — no intermediate shape where
  commands are board-scoped but status is not survives this chunk);
  outbox ops verified zero-diff; `CreateBoard` always mints (titles
  are not uniqueness keys); CLI default: flag omitted + exactly one
  live board ⇒ that board, zero or ≥2 ⇒ typed refusal; interim
  fills present, marked interim, minimally invasive; snapshot pass
  2 accepted as the command-shape change, nothing else; gates
  green; annotation comment on #125.
- **Traps for stage 3**: a half-run motion (domain arms moved but a
  mirror or the CLI surface left implicit, or the error set only
  partially moved) is the design's named trap; any `LocalOp` diff is
  a finding unless the recorded reason is overwhelming (DF-1: ops
  are already board-addressed / entity-scoped); interim fills that
  grow into real selection homes (session state, config) are scope
  creep — they are marked placeholders; `BoardExists`/`NoBoard`
  survivors anywhere are findings; the ambiguity refusal must be
  typed (no text-dependent behavior); the interim state must be
  visible (per-board pending_ops climbing is the honest signal — a
  change that silences it is a finding).

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the multi-board implementation round ("Domain vocabulary motion: board-scoped commands", branch feat/domain-board-scoped-commands) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunk 1 (branch feat/state-sync-status-map) is merged — git fetch origin --prune, then confirm with git log origin/main --oneline --all --grep="sync status" or gh pr list --state merged --limit 5 (chunk 1's PR). If it is not merged, STOP and report instead of planning.

1. Read AGENTS.md, then the implementation plan (§2 facts DF-1/DF-2/DF-9/DF-10, §3 chunk 2 — normative, incl. the interim-state paragraph; §4; §5; §6), the design doc (§3.1 in full — the nine-arm table, the CommandError list, the caller-side burden, the snapshot-churn note; §3.9 snapshot rule), and the dispatch plan §1/§3.
2. Read the current seams: crates/taskboard-domain/src/{command.rs (the nine live_board call sites + the resolver + CommandError), messages.rs (Execute wire)}, crates/taskboard-domain/src/outbox.rs (LocalOp — you must verify zero diff, not change it), crates/taskboard-app/src/{commands/mod.rs (mirrors), cli.rs, commands/boards.rs, commands/stacks.rs, commands/labels.rs, repl.rs (dispatch sites needing interim fills)}, the UI's command-construction sites (crates/taskboard-ui-slint/src/router.rs and view_state.rs consumers), plus the command-test corpus you will regenerate (crates/taskboard-state/tests/ snapshots).
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit: STOP and report), then create the worktree ../taskboard-WT-feat-domain-board-scoped-commands on a new branch feat/domain-board-scoped-commands off main. Re-locate every symbol yourself (stable keys = symbols/finding IDs; line numbers are plan-day references).
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-multi-board-board-scoped-commands-plan.md, settling at minimum: the live_board_of signature + typed refusal mapping (which lookup failures produce UnknownBoard vs BoardDeleted); the exact nine arm shapes incl. field order; the error-set motion + app mirror mapping table (every removed/widened/added variant and its app-side twin); the outbox zero-diff verification method (what you grep/inspect to prove it); the CLI mechanics (flag spelling, default resolution at invocation, the typed ambiguity refusal kind); the complete interim-fill site inventory (repl + UI), the marking convention, and the minimality rule; the CreateBoard refusal-lift test; the snapshot pass-2 catalogue; the full test matrix per AGENTS §5 (typed outcomes, proptest where pure logic appears). Resolve every open question with a recorded decision; deviations need a recorded reason.
5. Commit it as: docs(plan): board-scoped command vocabulary motion. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the multi-board implementation round ("Domain vocabulary motion: board-scoped commands").

1. Read AGENTS.md, the implementation plan (§2 DF-1/DF-2/DF-9/DF-10, §3 chunk 2, §4), the design doc (§3.1, §3.9 snapshot rule), the dispatch plan §1/§3, and the stage-1 plan (.artifacts/plans/*multi-board-board-scoped-commands*-plan.md) in the worktree ../taskboard-WT-feat-domain-board-scoped-commands (branch feat/domain-board-scoped-commands). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. All four vocabularies move in this chunk (commands, Execute wire, CommandError + app mirrors; status landed in chunk 1); outbox ops zero diff; interim fills are marked interim and minimal — no session state, no real selection homes; CreateBoard always mints; the CLI ambiguity refusal is typed; no text assertions; fakes over mocks. New .rs files carry their crate's SPDX header; #![forbid(unsafe_code)] holds (except taskboard-ui-slint, which carries none this chunk anyway).
3. Run the full gate suite from AGENTS.md §10 and make it green. Snapshot pass 2 is ONE deliberate insta regeneration for the command corpus — the accepted diff reviewed as "the shape change, nothing else"; no .pending snapshots ship.
4. Bookkeeping: add this chunk's CHANGELOG.md Unreleased line; do NOT touch any tracker issue (MD-4). State the interim loudly in your report (engine-possible multi-board, actor syncs first bound board only until chunk 4 — do not fix out of order). No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the multi-board implementation round (branch feat/domain-board-scoped-commands).

1. First, rebase onto main: in ../taskboard-WT-feat-domain-board-scoped-commands run git fetch origin --prune, then git rebase origin/main (drop already-merged duplicates; resolve only real conflicts), and git push --force-with-lease if the rebase rewrote history. Review the full branch diff (git diff origin/main...HEAD) against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DF-1/DF-2/DF-9/DF-10, §3 chunk 2), the design doc (§3.1 incl. the four-vocabulary acceptance, §3.9), the dispatch plan §1/§3 (the code-quality mandate applies to you in full), and the stage-1 plan.
3. Functional focus — this chunk's specific traps: the half-run motion (grep every board-scoped arm's construction sites — domain, app mirrors, repl, UI; any site still resolving implicitly, or the error set only partially moved, is a finding); LocalOp diff (any outbox change is a finding unless the recorded reason is overwhelming — DF-1 says zero diff); NoBoard/BoardExists survivors anywhere; CreateBoard refusal lifted (a surviving refusal is a finding; a new title-uniqueness refusal is worse); the CLI default rule exact (one live board ⇒ default; zero or ≥2 ⇒ typed refusal — assert typed kinds, never text); interim fills marked interim and minimal (a fill that grew session state or config is scope creep into chunks 6/7); snapshot pass 2 diff is the command-shape change only; the interim state stays visible (per-board pending_ops still climbs on unsynced boards via chunk 1's map — anything that silenced it is a finding); no test weakened to pass.
4. Code-quality mandate: (a) code quality — naming, comment discipline, no speculative generality, test quality per AGENTS §5; (b) SRP — the domain crate gains pure vocabulary only (no CLI/UI concepts leak in), selection stays caller-side, mirrors stay mechanical; (c) single level of abstraction per function body.
5. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
7. Ship: push the branch, open the PR to main with gh (no closure keywords this chunk; MD-4), then gh pr checks --watch to green — diagnose, fix, push --force-with-lease, repeat on any failure. Verify gh pr view --json mergeStateStatus reports CLEAN; BEHIND/DIRTY ⇒ rebase onto origin/main, push --force-with-lease, let checks re-run, repeat. Post the annotation comment on #125 (board-scoped vocabulary landed, PR URL). Never merge; never touch main.
8. Teardown: from the project root, git worktree remove --force ../taskboard-WT-feat-domain-board-scoped-commands.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation.
```

## 4. Chunk 3 — Engine ceremonies at N (`feat/state-board-ceremonies`)

- **Covers** (implementation plan §3 chunk 3 — normative; design
  §3.5 all five sub-questions, §3.1's ceremony error list; DF-5
  whole, DF-1's new arms/errors, DF-10): domain
  `command.rs`/`messages.rs` — new `UnbindBoard { board }`,
  `ReconcileByTitle { board }`; new
  `CommandError::RemoteBoardInUse { remote, board }`; state
  `actor.rs`/`core.rs` handlers — bind refusals per DF-5
  (`UnknownBoard`/`BoardDeleted`/`BoardAlreadyBound { board, bound
  }`; same-remote rebind = accepted no-op → re-adopt), the
  **bind-wipe rule** (`BindBoard` on a board whose descendants
  carry refs from a *different* remote wipes those refs, wipe
  counted in the bind receipt), unbind clears the binding and
  strands ops honestly, reconcile semantics per DF-5; app
  `commands/boards.rs` — `destruct_on`/`read_scope` parameterised
  by board (typed refusal states *that board's* counts; receipt
  names the board id), new `boards unbind <board>` and
  `boards reconcile <board>` ceremony verbs in the loud-flag
  shape, `select_on`'s single-board pre-check folds into
  pass-through of the engine's typed refusals, the bind ceremony
  stops *sending* `SetBoard` (the nudge is the broadcast `SyncNow`
  through `RequestSync`), `wait_for_adoption` parameterises by
  board watching that board's status row; `commands/mod.rs` app
  mirrors. `ReconcileByTitle` runs automatically after the bind
  ceremony's adoption-wait and is manually re-runnable,
  idempotent. **Non-goals**: no actor/routing changes (chunk 4);
  no config shape (chunk 5); `SetBoard` is no longer *sent* here
  but the variant + handler + `App::sync_commands()` remain
  (compile-clean pub surface; removed in chunk 4's motion).
- **Acceptance**: every DF-5 ceremony rule pinned by a typed-outcome
  test (refusals, bind-wipe count, stranded-ops honesty —
  `pending_ops` climbs, no silent cancellation; update-class ops on
  wiped refs dead-letter and are counted in the pre-count receipt);
  reconcile pinned as pure domain logic (trim + casefold, tasks
  within matched stacks only, per-field clock merge, duplicate
  create-ops cancelled as counted `FailOp`s, idempotent, no match ⇒
  both stay) with a proptest over the matcher; `destruct_on`
  refusal/receipt board-parameterised; the bind ceremony's nudge is
  the broadcast `SyncNow` (no targeted send); tombstone-finalizing
  `DeleteBoard` push drains through the board's owning connector
  (exactly one connector finalizes it); `SetBoard`/handler/
  `sync_commands()` still present and compile-clean (chunk 4
  removes them); gates green; PR body carries `Closes #170` +
  `Closes #179`; annotation comment on #125.
- **Traps for stage 3**: silent cancellation anywhere (ops deleted,
  dead-letters uncounted, `pending_ops` reset) is the honest-
  stranding trap; the bind-wipe wiping refs from the *same* remote
  (it must not — same-remote rebind re-adopts by refs, DF-5);
  reconcile matching tasks outside matched stacks, or fuzzy
  matching beyond trim+casefold (scope creep into the design's
  out-list); receipt counts that don't round-trip against
  `read_scope`'s pre-count; `destruct_on` refusals stating global
  instead of per-board counts; the ceremony still sending any
  targeted `SetBoard`; removing `SetBoard` early (chunk 4's motion,
  not this chunk's).

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the multi-board implementation round ("Engine ceremonies at N", branch feat/state-board-ceremonies) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunk 2 (feat/domain-board-scoped-commands) is merged (git fetch origin --prune; gh pr list --state merged --limit 5). If not, STOP and report instead of planning.

1. Read AGENTS.md, then the implementation plan (§2 facts DF-1/DF-5/DF-10, §3 chunk 3 — normative; §4; §5; §6), the design doc (§3.5 all five sub-questions — the multi-binding refusal list, unbind's three design points incl. the bind-wipe rule, title-match attach with the trade-off note, destruct/reset at N, adopt/resume; §3.1's CommandError ceremony list), and the dispatch plan §1/§4.
2. Read the current seams: crates/taskboard-app/src/commands/boards.rs (select_on, publish_on, read_scope, destruct_on, receipts, send_set_board, wait_for_adoption), crates/taskboard-domain/src/{command.rs (BindBoard + the error variants chunk 2 landed), ids.rs (RemoteStackRef.board / RemoteCardRef.board — the self-identifying refs), outbox.rs (DeleteBoard tombstone push, FailOp)}, crates/taskboard-state/src/{actor.rs (the command arms + dead-board carve-outs), core.rs}, the per-board status map chunk 1 landed (domain state.rs SyncStates — wait_for_adoption's watch target), plus the existing ceremony tests you must not break.
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit: STOP and report), then create the worktree ../taskboard-WT-feat-state-board-ceremonies on a new branch feat/state-board-ceremonies off main. Re-locate every symbol yourself.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-multi-board-board-ceremonies-plan.md, settling at minimum: the UnbindBoard/ReconcileByTitle arm shapes + engine handler decomposition; the bind refusal ladder + same-remote no-op re-adopt semantics; the bind-wipe rule's data flow (which descendant refs, how detected as different-remote, the counted receipt shape); honest stranding mechanics (what unbind leaves in the outbox; the dead-letter path for update-ops on wiped refs; what the pre-count receipt counts); the reconcile algorithm decomposition (the pure matcher fn — trim+casefold, stacks→labels→tasks within matched stacks — the clock-merge reuse, duplicate create-op cancellation as counted FailOps, the idempotence argument) + its proptest plan; destruct_on/read_scope(board) parameterisation + per-board refusal/receipt shapes; the new ceremony verbs' clap shapes + loud flags (the --confirm pattern); select_on's fold-to-pass-through; the broadcast-SyncNow nudge replacing send_set_board (SetBoard variant/handler/App::sync_commands() stay compile-clean — inventory them for chunk 4); wait_for_adoption(board) watching that board's status row; the tombstone-drain assertion (one owning connector); the full test catalogue with typed outcomes. Resolve every open question with a recorded decision; deviations need a recorded reason.
5. Commit it as: docs(plan): board ceremonies at N — unbind, reconcile, parameterised destruct. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the multi-board implementation round ("Engine ceremonies at N").

1. Read AGENTS.md, the implementation plan (§2 DF-1/DF-5/DF-10, §3 chunk 3, §4), the design doc (§3.5, §3.1 ceremony error list), the dispatch plan §1/§4, and the stage-1 plan (.artifacts/plans/*multi-board-board-ceremonies*-plan.md) in the worktree ../taskboard-WT-feat-state-board-ceremonies (branch feat/state-board-ceremonies). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. The reconcile matcher is pure domain logic with a proptest; honest stranding everywhere (no silent cancellation, every count in a receipt); loud flags on the new verbs (the --confirm pattern); typed outcomes, never text assertions; no sleeps — pause()/advance() or poll-with-deadline; fakes over mocks. SetBoard stays compile-clean (variant, handler, App::sync_commands()) — nothing sends it anymore. New .rs files carry their crate's SPDX header; #![forbid(unsafe_code)] holds outside taskboard-ui-slint.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Bookkeeping: add this chunk's CHANGELOG.md Unreleased line; do NOT touch any tracker issue (MD-4 — stage 3's PR body closes #170/#179). No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the multi-board implementation round (branch feat/state-board-ceremonies).

1. First, rebase onto main: in ../taskboard-WT-feat-state-board-ceremonies run git fetch origin --prune, then git rebase origin/main (drop already-merged duplicates; resolve only real conflicts), and git push --force-with-lease if the rebase rewrote history. Review the full branch diff (git diff origin/main...HEAD) against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DF-1/DF-5/DF-10, §3 chunk 3), the design doc (§3.5, §3.1), the dispatch plan §1/§4 (the code-quality mandate applies to you in full), and the stage-1 plan.
3. Functional focus — this chunk's specific traps: honest stranding (grep for any op deletion, silent dead-letter, or pending_ops reset — unbound-board ops must wait and climb; update-ops on wiped refs dead-letter AND are counted in the pre-count receipt); the bind-wipe rule's scope (refs from a DIFFERENT remote only — same-remote rebind re-adopts, never wipes; the wipe is counted in the bind receipt); the refusal ladder complete and typed (UnknownBoard/BoardDeleted/BoardAlreadyBound{board,bound}/RemoteBoardInUse{remote,board}); reconcile discipline (trim+casefold only — no fuzzy/normalized-key growth; tasks matched only within matched stacks; clock-merge reuse; duplicate create-ops cancelled as counted FailOps; idempotent; no match ⇒ both stay); destruct_on's refusal states THAT board's counts and the receipt names the board id; the bind ceremony sends no targeted SetBoard (nudge = broadcast SyncNow via RequestSync) while the variant/handler/App::sync_commands() remain compile-clean for chunk 4 (early removal is a finding, late sending is a finding); the tombstone push drains through exactly one owning connector; select_on's pre-check is a pass-through of the engine's typed refusals (one rule, one home); no text assertions; no test weakened to pass.
4. Code-quality mandate: (a) code quality — naming, comment discipline, no speculative generality (no fuzzy hooks, no dry-run flags — both are the design's out-list), test quality per AGENTS §5 (proptest covers the pure matcher); (b) SRP — the engine owns the ceremony semantics, the app layer owns orchestration/UX only, the domain owns vocabulary + the pure matcher; (c) single level of abstraction per function body.
5. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
7. Ship: push the branch, open the PR to main with gh (PR body carries "Closes #170" and "Closes #179"; MD-4), then gh pr checks --watch to green — diagnose, fix, push --force-with-lease, repeat on any failure. Verify gh pr view --json mergeStateStatus reports CLEAN; BEHIND/DIRTY ⇒ rebase onto origin/main, push --force-with-lease, let checks re-run, repeat. Post the annotation comment on #125. Never merge; never touch main.
8. Teardown: from the project root, git worktree remove --force ../taskboard-WT-feat-state-board-ceremonies.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation.
```

## 5. Chunk 4 — Connector identity end-to-end on the wire (`feat/sync-connector-modules`)

- **Covers** (implementation plan §3 chunk 4 — normative; design
  §3.4 trait + ADR amendment, §3.3 derived board sets + `SetBoard`
  retirement + the #157 guardrail, §3.6 legacy-scalar
  synthesis-of-one (the subset that mints the id), §3.9 the boot
  test; DF-3 mint chain, DF-4 codec tag sourcing, DF-1's
  `SyncCommand` → `{SyncNow, Detach}`, DF-5's guardrail, DF-10):
  new `crates/taskboard-app/src/connectors.rs` — the
  `ConnectorModule` trait per design §3.4's sketch (`id()`,
  `build_and_spawn(...)` over `Arc<dyn SyncStateReader>`, the
  report/system channels, `SyncActorConfig`), the
  `NextcloudModule` adapter wrapping `spawn_sync_actor`, `dyn`
  dispatch in a bootstrap registry loop (N=1 at this chunk);
  domain `entities.rs` — `Board.remote` widens to a
  connector-tagged binding carrying `RemoteBoardId` +
  `ConnectorId` (`ids.rs`/`messages.rs` as the widening needs);
  sync `actor.rs` — `spawn_sync_actor` gains the connector id; the
  resume/absorber path derives the actor's board set from tagged
  bindings **each cycle** (the generalisation of today's
  first-bound-board pick); `LoopState.target` becomes the derived
  per-board target set with per-board cache keys, **no
  cache-clear-on-retarget**; the stale-binding re-resolve (#258)
  composes unchanged (board-agnostic per-entity);
  `push_exec/` — push ownership filter: boards owned by *this*
  connector; state `actor.rs` + domain `messages.rs` —
  **`SetBoard` retired in the same motion that tags the bindings**
  (variant, engine handler, and `App::sync_commands()` removed
  together); app `bootstrap.rs` — the registry loop + legacy-scalar
  synthesis-of-one (resolved `nextcloud` + `[nextcloud]` ⇒ one
  connector named `nextcloud`; resolved `standalone` ⇒ none);
  storage `codec.rs` — upserts source the tag from the domain field
  per DF-4. **ADR 0010 amendment text lands here** — design §6's
  two blocks verbatim into
  `docs/adr/0010-standalone-first-optional-sync-connector.org`.
  **Behaviours**: a newly bound board is picked up by the owning
  connector's next read with no message at all (the broadcast
  `SyncNow` satisfies the ceremony's "cycle soon"; unchanged boards
  cheap via per-board validators — DF-5's #157 guardrail: cycle
  cost O(boards + changed cards)); **the v0.1.x additive-upgrade
  boot test** — v0.1.x-shaped db (one bound board, a queued op, a
  non-idle status) + v0.1.x config; boot the new build; assert the
  synthesized connector, tagged-default binding decode, status-row
  hydration, a cycle merging per the unchanged policy, the queued
  op draining (tier-2 dockerized, MD-5). **Non-goals**: no
  `[[sync.connectors]]` plural (chunk 5 — config decode stays the
  legacy scalar + synthesis-of-one); no authority mechanics (chunk
  5's events wait); no CLI verb growth (chunk 6); no second
  connector *kind* (nextcloud only).
- **Acceptance**: the binding tags + `SetBoard` removal land as one
  motion (the branch's diff shows both); the ADR 0010 amendment is
  design §6's text verbatim (both the decision-section block and
  the consequences-lines block); the derived board set is computed
  from persisted bindings each cycle with per-board cache keys and
  no cache-clear-on-retarget; the boot test passes on the tier-2
  runner (or its docker absence is honestly recorded, MD-5);
  `cargo deny` green (no new deps — the trait's boundary exit
  criterion); gates green; annotation comments on #187 and #125
  (chunk 4 does not close #187 — chunk 5's N=2 does).
- **Traps for stage 3**: `SetBoard` removal split from the binding
  tags (either half alone is the design's named sequencing trap);
  the ADR text paraphrased instead of verbatim (diff it against
  design §6 character-for-character); a `cache-clear-on-retarget`
  surviving in the absorber/resume path; the derived set computed
  once at boot instead of each cycle; the push filter consulting
  anything but the entity graph's tags; the registry loop growing
  N>1 plumbing before chunk 5's config (speculative generality);
  config decoding the array early (chunk 5's scope); the boot test
  asserting beyond the design's list (merge behavior must be
  bit-for-bit per 0005's epoch-default clocks — a changed merge
  policy is a finding).

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of the multi-board implementation round ("Connector identity end-to-end on the wire", branch feat/sync-connector-modules) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunk 3 (feat/state-board-ceremonies) is merged (git fetch origin --prune; gh pr list --state merged --limit 5). If not, STOP and report instead of planning.

1. Read AGENTS.md, then the implementation plan (§2 facts DF-1/DF-3/DF-4/DF-5/DF-10, §3 chunk 4 — normative; §4; §5; §6), the design doc (§3.4 the trait sketch + the narrow-seam check + the test-fake story, §3.3 the derived-board-set consequence + SetBoard's fate + the #157 amplification pricing, §3.6 the synthesis-of-one subset, §3.9 the additive-upgrade test spec, §6 the ADR text you will land verbatim), and the dispatch plan §1/§5.
2. Read the current seams: crates/taskboard-app/src/bootstrap.rs (connectors Mutex<Vec<SyncActor>>, the .first() pick in sync_commands, ensure_vacant/try_register, the spawn_sync_actor call), crates/taskboard-sync-nextcloud/src/actor.rs (spawn_sync_actor signature, the resume pull-target block, LoopState, the absorber), push_exec/ (the stale-binding re-resolve #258 landed, the push's board resolution), crates/taskboard-domain/src/{entities.rs (Board.remote, BoardClocks), messages.rs (SyncCommand/SetBoard, SyncReport), ids.rs (ConnectorId from chunk 1), persistence.rs (SyncStateReader, ValidatorKey — the per-board 304 guardrail)}, crates/taskboard-state/src/actor.rs (the SetBoard handler arm, the fan-out registry), crates/taskboard-storage-sqlite/src/codec.rs (the connector column codec from chunk 1), docs/adr/0010 (the stance headline + consequences lines you will replace), plus the sync actor and bootstrap test suites.
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit: STOP and report), then create the worktree ../taskboard-WT-feat-sync-connector-modules on a new branch feat/sync-connector-modules off main. Re-locate every symbol yourself.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-multi-board-connector-modules-plan.md, settling at minimum: the ConnectorModule trait's final shape vs the design §3.4 sketch (carry only what the sketch names — no discovery/plugin surface) + the NextcloudModule adapter + the registry loop at N=1; the entities.rs binding-type widening (exact type, serde/persistence consequences, the codec's new tag sourcing per DF-4); the derived-per-cycle board set mechanics (where computed, what it replaces in the resume/absorber path, per-board cache keys, the no-cache-clear-on-retarget argument, how #258's re-resolve composes); the push ownership filter; the SetBoard removal inventory (variant + engine handler + App::sync_commands() — one motion with the tags); the bootstrap synthesis-of-one subset of §3.6 (what config reading stays legacy); the ADR 0010 edit (design §6's two blocks verbatim — plan the exact replacement anchors); the v0.1.x boot-test design (db fixture shape, config fixture, the assertion list per §3.9, tier-2 harness conventions + MD-5 honest-recording if docker is absent); the FakeModule test-fake story for app-side tests; the full test catalogue. Resolve every open question with a recorded decision; deviations need a recorded reason.
5. Commit it as: docs(plan): connector identity end-to-end — module trait, tagged bindings, SetBoard retirement. Do not implement; do not push.

Report: plan path, decisions, risks (esp. the absorber/retarget interaction), estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of the multi-board implementation round ("Connector identity end-to-end on the wire").

1. Read AGENTS.md, the implementation plan (§2 DF-1/DF-3/DF-4/DF-5/DF-10, §3 chunk 4, §4), the design doc (§3.4, §3.3, §3.6 subset, §3.9, §6 verbatim ADR text), the dispatch plan §1/§5, and the stage-1 plan (.artifacts/plans/*multi-board-connector-modules*-plan.md) in the worktree ../taskboard-WT-feat-sync-connector-modules (branch feat/sync-connector-modules). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. SetBoard's removal and the binding tags are ONE motion; the ADR 0010 amendment is design §6's text verbatim (both blocks); the trait carries no discovery/plugin surface (the narrow-seam check); the derived board set recomputes each cycle with per-board cache keys and no cache-clear-on-retarget; config stays legacy-scalar + synthesis-of-one (no array decode); typed outcomes, no text assertions, fakes over mocks (the FakeModule story), no sleeps. New .rs files carry their crate's SPDX header (app: AGPL-3.0-only; domain/sync/state/storage: MIT OR Apache-2.0); #![forbid(unsafe_code)] holds outside taskboard-ui-slint.
3. The v0.1.x boot test is tier-2 against a dockerized Nextcloud per the existing harness conventions; if the runner lacks docker, record that honestly in your report (which assertions ran hermetically, which need the tier-2 runner) — never skip silently (MD-5). Run the full gate suite from AGENTS.md §10 and make it green.
4. Bookkeeping: add this chunk's CHANGELOG.md Unreleased line; do NOT touch any tracker issue (MD-4). No push, no PR. Never push to or merge main.

Report: per-gate results, tier-2 run/absence record, any deviation from the plan and why.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of the multi-board implementation round (branch feat/sync-connector-modules).

1. First, rebase onto main: in ../taskboard-WT-feat-sync-connector-modules run git fetch origin --prune, then git rebase origin/main (drop already-merged duplicates; resolve only real conflicts), and git push --force-with-lease if the rebase rewrote history. Review the full branch diff (git diff origin/main...HEAD) against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DF-1/DF-3/DF-4/DF-5/DF-10, §3 chunk 4), the design doc (§3.4, §3.3, §3.6 subset, §3.9, §6), the dispatch plan §1/§5 (the code-quality mandate applies to you in full), and the stage-1 plan.
3. Functional focus — this chunk's specific traps: SetBoard removal and binding tags as ONE motion (grep: the variant gone, the engine handler gone, App::sync_commands() gone, no construction site left — but ONLY because the tags + derived set replace it; either half alone is a finding); the ADR 0010 text is design §6's draft character-for-character (diff both blocks — paraphrase is a finding); the derived board set is per-cycle from persisted bindings (a boot-time computation or a surviving cache-clear-on-retarget is a finding); the push ownership filter reads the entity graph's tags only; the registry loop is N=1 with no speculative plural plumbing (chunk 5's); config decode stays legacy scalar + synthesis-of-one (any array decode is chunk-5 scope creep); the boot test asserts exactly §3.9's list with merge behavior bit-for-bit (0005's epoch-default clocks — any merge-policy drift is a finding); tier-2 recorded honestly if docker was absent (MD-5 — a silent skip is a finding); cargo deny green with zero dependency changes; the #258 re-resolve composed unchanged; no text assertions; no test weakened to pass.
4. Code-quality mandate: (a) code quality — naming, comment discipline, no speculative generality (the trait is exactly the sketch: id + build_and_spawn; no discovery/versioning), test quality per AGENTS §5; (b) SRP — the domain keeps the vocabulary + tagged binding only (the factory trait stays app-side — AGENTS §2 purity), the sync actor syncs, bootstrap constructs; (c) single level of abstraction per function body — the registry loop and the adapter read as orchestration over named steps.
5. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding. If your runner has docker, run the tier-2 boot test; if not, verify its honest-recording note and say so in your report (MD-5).
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
7. Ship: push the branch, open the PR to main with gh (no closure keywords this chunk; MD-4), then gh pr checks --watch to green — diagnose, fix, push --force-with-lease, repeat on any failure. Verify gh pr view --json mergeStateStatus reports CLEAN; BEHIND/DIRTY ⇒ rebase onto origin/main, push --force-with-lease, let checks re-run, repeat. Post annotation comments on #187 (connector identity end-to-end, trait promoted, ADR 0010 amended, PR URL) and #125. Never merge; never touch main.
8. Teardown: from the project root, git worktree remove --force ../taskboard-WT-feat-sync-connector-modules.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation.
```

## 6. Chunk 5 — App plural world: config, registry, authority (`feat/app-connector-registry`)

- **Covers** (implementation plan §3 chunk 5 — normative; design
  §3.6 full, §3.7 full, §3.3 registry/naming consequences; DF-6
  whole, DF-7 whole, DF-3): app `config.rs` — `[[sync.connectors]]`
  array-of-tables decode, entry struct
  `#[serde(deny_unknown_fields)]`, kebab + uniqueness name
  validation, the **full typed-rejection list** per DF-6 (scalar +
  array both present; `[nextcloud]` table + array; `standalone` +
  array; duplicate or non-kebab `name`; `nextcloud` resolved with
  an empty array — all loud, naming the offending keys);
  `commands/connect.rs` — the **shape-aware ceremony writer** (no
  array in file ⇒ today's scalar write verbatim; array present ⇒
  rewrite the default/first entry's `server_url`/`username`, never
  touch `backend`; round-trip holds in both worlds);
  `bootstrap.rs` — `ensure_vacant` →
  `ConnectorNameInUse(ConnectorId)` (same guard, named), N-actor
  spawn over the registry, `connector_commands(id)` named lookup,
  the daemon-side attach verb's name parameter, the **authority
  task** per DF-7; domain `messages.rs` — additive
  `SystemEvent::ConnectorLost(ConnectorId)` /
  `ConnectorRestored(ConnectorId)`; global `NetworkLost`/
  `NetworkRestored` **retired from the actors** (they become the
  authority's vocabulary); sync `actor.rs` — the offline FSM emits
  the per-connector variants, episode-edge semantics verbatim
  (exactly-once per episode; `fail_cycle` on transport-classified
  failures; report-then-restore order); state `actor.rs` — arms:
  `ConnectorLost(c)` → `go_offline` scoped to *c's boards* via the
  binding tags; `ConnectorRestored(c)` → forward the nudge; app
  `commands/mod.rs` — `require_connector` pluralised ("at least one
  connector configured", same typed `SyncDisabled` refusal). First
  entry = the default connector; array absent ⇒ legacy world
  verbatim; env override works for the scalar only (documented
  figment limitation). **Tier-2 two-connector test**: two
  dockerized Nextcloud instances, boards bound across both,
  per-connector health visible in status, one connector down ⇒ its
  boards' phases `Offline`, the global banner silent while the
  other responds (MD-5). **Non-goals**: no UI (chunk 7); no new
  connector *kind* (the trait's second implementation is post-wave,
  not an in-wave exit criterion); no per-board CLI surface growth
  beyond DF-6's verbs (chunk 6).
- **Acceptance**: two connectors configured and spawned is the
  normal path (the N=2 exit criterion made real); single-connector
  installs are byte-for-byte today's semantics (the authority is a
  pass-through with the same edge trigger); every DF-6 rejection
  pinned by a typed test naming the offending key; the ceremony
  writer round-trips in both file shapes; the two-connector tier-2
  test passes (or its docker absence is honestly recorded, MD-5);
  the authority's up/down set is seeded from configured modules and
  updated by attach/detach registry hooks (app-internal lifecycle
  bookkeeping, not inter-actor traffic); gates green; PR body
  carries `Closes #187`; annotation comment on #125.
- **Traps for stage 3**: a silent mix tolerance (scalar + array
  decoding without the typed rejection — probe case 2 showed figment
  won't reject it; loudness must come from validation) is the DF-6
  trap; the ceremony writer touching `backend` in the array world,
  or writing the array shape into a scalar-only file; the authority
  counting anything but the per-connector events (inferring from
  silence — "no report lately" — is the design's rejected option
  (c)); emit-site drift (a surviving global emit from an actor, or
  a per-connector emit that lost the exactly-once episode edge, or
  report-then-restore order flipped); the engine arms scoping
  `go_offline` by anything but the binding tags; `require_connector`
  changing its typed refusal kind; single-connector semantics
  drifting (the pass-through must keep the same edge trigger —
  diff the one-connector journeys).

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of the multi-board implementation round ("App plural world: config, registry, authority", branch feat/app-connector-registry) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunk 4 (feat/sync-connector-modules) is merged (git fetch origin --prune; gh pr list --state merged --limit 5). If not, STOP and report instead of planning.

1. Read AGENTS.md, then the implementation plan (§2 facts DF-3/DF-6/DF-7/DF-10, §3 chunk 5 — normative; §4; §5; §6), the design doc (§3.6 in full incl. the figment probe's eight observed cases and the typed-rejection list, §3.7 in full incl. the broadcast-provenance blocking fact and the flapping note, §3.3's registry/naming consequences), and the dispatch plan §1/§6.
2. Read the current seams: crates/taskboard-app/src/{config.rs (SyncBackend, SyncSection, sync_backend(), the validation-loudness block, the figment load chain), commands/connect.rs (persist + the toml_edit overlay), bootstrap.rs (chunk 4's registry loop + synthesis-of-one, ensure_vacant, attach/detach, the broadcast wiring), commands/mod.rs (require_connector)}, crates/taskboard-domain/src/messages.rs (SystemEvent), crates/taskboard-sync-nextcloud/src/actor.rs (the offline FSM: offline flag, fail_cycle's NetworkLost emit, the restored path), crates/taskboard-state/src/actor.rs (the NetworkLost/Restored arms + go_offline), plus the config/connect/bootstrap/sync-actor test suites.
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit: STOP and report), then create the worktree ../taskboard-WT-feat-app-connector-registry on a new branch feat/app-connector-registry off main. Re-locate every symbol yourself.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-multi-board-connector-registry-plan.md, settling at minimum: the ConnectorCfg entry struct + the resolution order (array present ⇒ exactly the array, first = default; absent ⇒ legacy verbatim incl. standalone ⇒ zero connectors); the six typed rejections' ConfigError kinds + the offending-key naming; the shape-aware writer's two paths + round-trip tests in both file shapes; the N-actor spawn over the registry + connector_commands(id) + ConnectorNameInUse + the attach verb's name parameter; the authority task's design (state: per-connector up/down set; inputs: seeded from configured modules + attach/detach hooks; subscription: the system broadcast; outputs: NetworkLost on last-down / NetworkRestored on first-up; the single-connector pass-through argument); the actors' emit rewrite to ConnectorLost/Restored with episode edges verbatim; the engine arms' tag-scoped go_offline; require_connector pluralisation; the two-connector tier-2 test design (two dockerized Nextclouds, boards across both, health visibility, one-down scenario; harness conventions + MD-5); the single-connector byte-for-byte pin list; the full test catalogue. Resolve every open question with a recorded decision; deviations need a recorded reason.
5. Commit it as: docs(plan): plural connectors — config array, registry, per-connector authority. Do not implement; do not push.

Report: plan path, decisions, risks (esp. the authority's edge semantics), estimated diff size.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of the multi-board implementation round ("App plural world: config, registry, authority").

1. Read AGENTS.md, the implementation plan (§2 DF-3/DF-6/DF-7/DF-10, §3 chunk 5, §4), the design doc (§3.6, §3.7, §3.3 consequences), the dispatch plan §1/§6, and the stage-1 plan (.artifacts/plans/*multi-board-connector-registry*-plan.md) in the worktree ../taskboard-WT-feat-app-connector-registry (branch feat/app-connector-registry). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. Every rejection is typed and names the offending key; episode-edge semantics verbatim (exactly-once per offline episode, fail_cycle on transport-classified failures, report-then-restore order); the authority infers from events only, never from silence; single-connector installs byte-for-bit today's semantics; typed outcomes, no text assertions (rejection kinds asserted by variant, offending-key presence asserted structurally), fakes over mocks, no sleeps — pause()/advance() for the FSM edges. New .rs files carry their crate's SPDX header; #![forbid(unsafe_code)] holds outside taskboard-ui-slint.
3. The two-connector test is tier-2 (two dockerized Nextcloud instances) per the existing harness conventions; if the runner lacks docker, record that honestly in your report — never skip silently (MD-5). Run the full gate suite from AGENTS.md §10 and make it green.
4. Bookkeeping: add this chunk's CHANGELOG.md Unreleased line; do NOT touch any tracker issue (MD-4). No push, no PR. Never push to or merge main.

Report: per-gate results, tier-2 run/absence record, any deviation from the plan and why.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 of the multi-board implementation round (branch feat/app-connector-registry).

1. First, rebase onto main: in ../taskboard-WT-feat-app-connector-registry run git fetch origin --prune, then git rebase origin/main (drop already-merged duplicates; resolve only real conflicts), and git push --force-with-lease if the rebase rewrote history. Review the full branch diff (git diff origin/main...HEAD) against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DF-3/DF-6/DF-7/DF-10, §3 chunk 5), the design doc (§3.6, §3.7, §3.3 consequences), the dispatch plan §1/§6 (the code-quality mandate applies to you in full), and the stage-1 plan.
3. Functional focus — this chunk's specific traps: the full DF-6 rejection list, each typed + naming the offending key + pinned by a test (a silently tolerated mix — figment probe case 2 — is the trap; so is a rejection that doesn't say which key); the ceremony writer's two paths (scalar file ⇒ today's write verbatim; array file ⇒ default entry's server_url/username only, backend never touched) with round-trip tests in both shapes; the actors emit ONLY the per-connector variants with episode edges verbatim (grep for surviving global emits from actors; check exactly-once per episode and report-then-restore order); the authority counts events, never infers from silence, seeds from configured modules + attach/detach hooks (inter-actor traffic for lifecycle is a finding), and emits NetworkLost on last-down / NetworkRestored on first-up; the engine's ConnectorLost arm scopes go_offline by binding tags (a global write is a finding); single-connector journeys byte-for-byte (diff them); require_connector keeps its typed SyncDisabled refusal with pluralised semantics; the two-connector tier-2 test exercises the exit criterion (boards across both, health visible, one down ⇒ its boards Offline + global banner silent) or its absence is honestly recorded (MD-5); no text assertions; no test weakened to pass.
4. Code-quality mandate: (a) code quality — naming, comment discipline, no speculative generality (no second connector kind, no per-connector poll knobs — both out-list), test quality per AGENTS §5; (b) SRP — config owns decode+validation, the authority owns counting, the actors own their own FSM, the engine owns state writes only; (c) single level of abstraction per function body — the authority task reads as subscribe → update set → maybe-emit.
5. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding. If your runner has docker, run the two-connector tier-2 test; if not, verify the honest-recording note and say so (MD-5).
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
7. Ship: push the branch, open the PR to main with gh (PR body carries "Closes #187" — the N=2 exit criterion verified; MD-4), then gh pr checks --watch to green — diagnose, fix, push --force-with-lease, repeat on any failure. Verify gh pr view --json mergeStateStatus reports CLEAN; BEHIND/DIRTY ⇒ rebase onto origin/main, push --force-with-lease, let checks re-run, repeat. Post the annotation comment on #125. Never merge; never touch main.
8. Teardown: from the project root, git worktree remove --force ../taskboard-WT-feat-app-connector-registry.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation.
```

## 7. Chunk 6 — CLI/REPL verb growth (`feat/app-cli-connector-verbs`)

- **Covers** (implementation plan §3 chunk 6 — normative; design
  §3.2 wire consequence — the additive JSON answer, §3.3 CLI
  consequences, §3.6 `require_connector` pluralised + `--connector`
  flags; DF-2 CLI/REPL homes, DF-6): app `commands/status.rs` +
  `commands/output.rs` — per-connector `status` rendering: the
  daemon JSON answer **keeps its `sync` field** (aggregate fold,
  byte-shape unchanged for the single-board v0.1.x install) and
  **adds** `connectors: […]` (name, derived health, boards with
  their statuses) + `boards: […]` arrays; human output renders the
  arrays; a connector with zero bound boards lists as
  configured-and-idle; `ipc/protocol.rs` — the additive answer
  shape; `cli.rs` + the targeting verbs — `--connector <name>`
  flags where targeting matters; `repl.rs` —
  `ReplSession.current_board: Option<BoardId>` + the
  `boards use <uuid>` line **replacing chunk 2's interim fill**
  (verbs pre-fill from it); `completion.rs` — board ids join the
  completer's tables + the new verbs/flags. The `--board`/
  `--connector` flag pair composes (board-scoped verbs name their
  board per DF-2's default rules; sync/status verbs may name their
  connector). **The move-board exit criterion is exercised here**:
  unbind + rebind onto the second connector's remote, verified by
  per-board status attribution moving (tier-2 two instances,
  MD-5). **Non-goals**: no UI (chunk 7); no engine/domain changes
  beyond the wire shape (app-side rendering + verb plumbing).
- **Acceptance**: the JSON answer is strictly additive (`sync`
  byte-shape pinned; new arrays rendered in human output);
  `boards use` + `current_board` replace the interim fill (no
  marked-interim REPL site survives); completion offers the new
  surface; the move-board journey passes on the tier-2 runner (or
  its docker absence is honestly recorded, MD-5); gates green;
  annotation comment on #125.
- **Traps for stage 3**: a removed/renamed field in the `status`
  answer (the `sync` field must survive byte-shape for the v0.1.x
  install — D-W6's additive door); per-connector health derived
  from anything but the per-board map + bindings (attribution is
  derived, never stored — DF-9); the zero-bound connector hidden
  (it must list as configured-and-idle — honest); a surviving
  REPL interim fill or a `current_board` that leaks into the
  domain crate (DF-2: selection never enters the domain);
  completion offering boards/verbs in the wrong contexts;
  the move-board journey asserting attribution movement per board
  (the criterion's wording), not just command success.

**Prompt 6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 6 of the multi-board implementation round ("CLI/REPL verb growth", branch feat/app-cli-connector-verbs) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunk 5 (feat/app-connector-registry) is merged (git fetch origin --prune; gh pr list --state merged --limit 5). Chunk 7 may be in flight concurrently — you share no files with it (app vs ui crates); never share worktrees. If chunk 5 is not merged, STOP and report instead of planning.

1. Read AGENTS.md, then the implementation plan (§2 facts DF-2/DF-6/DF-9 (attribution), §3 chunk 6 — normative; §4; §5; §6), the design doc (§3.2's wire-compat consequence, §3.3's caller-side burden + CLI consequences, §3.6's require_connector/--connector paragraphs), and the dispatch plan §1/§7.
2. Read the current seams: crates/taskboard-app/src/{commands/status.rs (StatusView.sync), commands/output.rs (SyncStatusView + renders), ipc/protocol.rs (the daemon answer shape), cli.rs, repl.rs (ReplSession, chunk 2's marked interim fills), completion.rs, commands/mod.rs (require_connector post-chunk-5)}, the per-board status map + bindings (crates/taskboard-domain/src/state.rs SyncStates + entities.rs tagged bindings — your attribution source), plus the existing CLI/REPL/ipc test suites and an L-series E2E test to mirror.
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit: STOP and report), then create the worktree ../taskboard-WT-feat-app-cli-connector-verbs on a new branch feat/app-cli-connector-verbs off main. Re-locate every symbol yourself.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-multi-board-cli-connector-verbs-plan.md, settling at minimum: the additive answer shape (field names/order for connectors[] and boards[], the sync field's byte-shape pin, serde mechanics, ipc protocol growth); the human rendering layout; the attribution derivation (per-board rows grouped by derived connector; zero-bound ⇒ configured-and-idle); which verbs gain --connector (the targeting-matters rule) and how --board/--compose; ReplSession.current_board mechanics (boards use <uuid>, pre-fill rules, unset behavior) + the interim-fill removal inventory; completion growth (board ids, verbs, flags, contexts); the move-board journey design (unbind + rebind onto connector 2's remote; assert per-board attribution moves; tier-2 two instances + MD-5); the full test catalogue (rendering at the output layer, routing only above it — one behavior, one test). Resolve every open question with a recorded decision; deviations need a recorded reason.
5. Commit it as: docs(plan): CLI/REPL connector verbs and per-connector status. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 6 of the multi-board implementation round ("CLI/REPL verb growth").

1. Read AGENTS.md, the implementation plan (§2 DF-2/DF-6/DF-9, §3 chunk 6, §4), the design doc (§3.2 wire consequence, §3.3 CLI consequences, §3.6), the dispatch plan §1/§7, and the stage-1 plan (.artifacts/plans/*multi-board-cli-connector-verbs*-plan.md) in the worktree ../taskboard-WT-feat-app-cli-connector-verbs (branch feat/app-cli-connector-verbs). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. The JSON answer is strictly additive (the sync field survives byte-shape); attribution derived from bindings, never stored; ReplSession.current_board stays app-side (nothing of selection enters the domain — DF-2); every marked REPL interim fill from chunk 2 is replaced; typed outcomes, no text assertions (JSON shape asserted structurally — serde round-trips/field presence, never rendered prose); hermetic tests + the existing E2E pipe-stdin harness; no sleeps. New .rs files carry the AGPL-3.0-only SPDX header (app crate).
3. The move-board journey is tier-2 (two dockerized Nextcloud instances) per the existing harness conventions; if the runner lacks docker, record that honestly in your report — never skip silently (MD-5). Run the full gate suite from AGENTS.md §10 and make it green.
4. Bookkeeping: add this chunk's CHANGELOG.md Unreleased line; do NOT touch any tracker issue (MD-4). No push, no PR. Never push to or merge main.

Report: per-gate results, tier-2 run/absence record, any deviation from the plan and why.
```

**Prompt 6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 6 of the multi-board implementation round (branch feat/app-cli-connector-verbs).

1. First, rebase onto main: in ../taskboard-WT-feat-app-cli-connector-verbs run git fetch origin --prune, then git rebase origin/main (drop already-merged duplicates; resolve only real conflicts — chunk 7 may have landed in parallel; you share no files), and git push --force-with-lease if the rebase rewrote history. Review the full branch diff (git diff origin/main...HEAD) against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DF-2/DF-6/DF-9, §3 chunk 6), the design doc (§3.2, §3.3, §3.6), the dispatch plan §1/§7 (the code-quality mandate applies to you in full), and the stage-1 plan.
3. Functional focus — this chunk's specific traps: the answer is strictly additive (diff the ipc protocol + status answer: sync survives byte-shape for the single-board install; no field removed or renamed — D-W6's only door); attribution derived from the binding tags (a stored connector field in status rows/answers beyond the map's derivation is a finding — DF-9); zero-bound connectors list as configured-and-idle (hidden = dishonest); the --board/--connector pair composes and DF-2's default rules hold (one live board ⇒ default; zero/≥2 ⇒ typed refusal); boards use + current_board replace every marked REPL interim fill (grep for surviving interim markers); selection stays out of the domain crate; completion contexts correct; the move-board journey asserts per-board attribution movement (not mere command success) and its tier-2 run/absence is honestly recorded (MD-5); rendering tested at the output layer only (no duplicated routing tests); no text assertions; no test weakened to pass.
4. Code-quality mandate: (a) code quality — naming, comment discipline, no speculative generality, test quality per AGENTS §5; (b) SRP — the CLI parses/dispatches, rendering lives in output, the REPL owns session state, nothing of selection or rendering leaks down into domain/state; (c) single level of abstraction per function body.
5. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding. If your runner has docker, run the move-board tier-2 journey; if not, verify the honest-recording note and say so (MD-5).
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
7. Ship: push the branch, open the PR to main with gh (no closure keywords this chunk; MD-4), then gh pr checks --watch to green — diagnose, fix, push --force-with-lease, repeat on any failure. Verify gh pr view --json mergeStateStatus reports CLEAN; BEHIND/DIRTY ⇒ rebase onto origin/main, push --force-with-lease, let checks re-run, repeat. Post the annotation comment on #125. Never merge; never touch main.
8. Teardown: from the project root, git worktree remove --force ../taskboard-WT-feat-app-cli-connector-verbs.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation.
```

## 8. Chunk 7 — UI board dimension (`feat/ui-board-switcher`)

- **Covers** (implementation plan §3 chunk 7 — normative; design
  §3.8 all integration points, §3.2 badge interplay; DF-8 whole,
  DF-2's UI home): ui `view_state.rs` — `ViewState.board:
  Option<BoardId>` (None = fresh install) + the **seed rule**
  (seeded by `presentation::live_board` when None — deterministic
  first-live-non-archived; survives as the seed **only**) +
  `select_board(&mut self, state, board) -> Vec<FlowOutcome>`
  mirroring `select_stack` (sets board, resets `selected_stack` to
  the new board's first stack, `focused_task` to that stack's
  first task, `RefreshNow`) — **replaces chunk 2's UI interim
  fills**; `presentation.rs` — `live_board` survives as the
  seeding rule only; the canonical board-order helper (same order
  as `boards list`); the badge reads the **selected board's**
  status row; `actions.rs` — `BoardNext` (`board.next`),
  `BoardPrevious` (`board.previous`), `BoardSelect`
  (`board.select`) registry rows (contexts `Board`/`StackDetail`/
  `TaskDetail` + `Global`, category navigation); `keymap/presets.rs`
  — entries present and resolving in `Board` + `ViNormal` (the
  preset table is the tunable — the design fixes existence +
  resolvability, not specific strokes); `router.rs` — dispatch (the
  exhaustive match forces every construction site);
  `models.rs` — the `push` builder derives its board argument from
  `ViewState.board`; **no `IdTables` re-keying** (tables are
  per-refresh of the viewed board by construction); new
  `ui/compositions/board-bar.slint` (one segment per live board;
  **compact hides the strip** — keyboard switching still works) +
  `board-layout.slint` (strip mount) + `adapter.rs` (segment clicks
  route through the action layer, mirroring the `select_stack`
  seam). **The scripted keyboard-journey leg**: with two boards
  seeded, the journey performs `board.next`, asserts the view
  re-rendered the second board's stacks, `board.previous` back —
  the exit criterion's pin. Kiosk inherits the compact
  degeneration. **Non-goals**: no engine commands from switching
  (DF-8's dumb-client rule); no config-pinned fixed-board kiosk
  (backlog); no UI board create/archive actions (backlog —
  `stack.create` is the precedent).
- **Acceptance**: the dumb-client rule holds (switching performs no
  engine command — `FlowOutcome::Command` never produced by
  board switching); the journey leg passes; no-wrap rule at the
  ends of the canonical order; `IdTables` shape unchanged; compact
  hides the strip; every chunk-2 UI interim marker replaced; gates
  green; annotation comment on #125.
- **Traps for stage 3**: any engine command from switching (grep
  the board-action dispatch for `FlowOutcome::Command` — the
  dumb-client rule is AGENTS §1 made concrete; a "helpful" sync
  nudge on switch is a finding); `IdTables` re-keyed per board (the
  design's rejected double-refresh); `live_board` surviving as a
  read path beyond the seed rule; `select_board` not mirroring
  `select_stack` exactly (focus reset semantics); keymap presets
  over-pinned (the chunk must guarantee existence + resolution in
  `Board`/`ViNormal`, not hard-code specific strokes as "the"
  bindings — the table is the tunable); badge reading the aggregate
  fold instead of the selected board's row (chunk 1's interim read
  must be upgraded here); the strip shown in compact/kiosk.

**Prompt 7.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 7 of the multi-board implementation round ("UI board dimension", branch feat/ui-board-switcher) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunk 5 (feat/app-connector-registry) is merged (git fetch origin --prune; gh pr list --state merged --limit 5). Chunk 6 may be in flight concurrently — you share no files with it (ui vs app crates); never share worktrees. (Parallel-option note: this round's default dispatches you after chunk 5; the implementation plan §5 also permits dispatch after chunk 2 — only the user invokes that.) If chunk 5 is not merged, STOP and report instead of planning.

1. Read AGENTS.md, then the implementation plan (§2 facts DF-2/DF-8/DF-9 (the badge read), §3 chunk 7 — normative; §4; §5; §6), the design doc (§3.8 in full — ViewState.board, the three ActionIds + registry rows, the keymap rule, select_board's contract, the IdTables no-re-keying argument, board-bar under R1, compact under R9, the keyboard-journey leg, the badge interplay; §3.2's badge paragraphs), the GUI requirements R1/R4/R9 (.artifacts/plans/2026-10-14-gui-design-requirements-plan.md), and the dispatch plan §1/§8.
2. Read the current seams: crates/taskboard-ui-slint/src/{view_state.rs (ViewState, select_stack, live_board consumer, chunk 2's interim fills), presentation.rs (live_board, SyncBadge's chunk-1 fold read), actions.rs (ActionId/ActionEntry/REGISTRY), keymap/{mod,presets}.rs, router.rs, models.rs (IdTables, push), adapter.rs (the select_stack click seam), compositions/ (toolbar/status-bar for the sibling pattern, board-layout.slint)}, plus the UI crate's test suites and the scripted-journey harness.
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit: STOP and report), then create the worktree ../taskboard-WT-feat-ui-board-switcher on a new branch feat/ui-board-switcher off main. Re-locate every symbol yourself.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-multi-board-ui-board-switcher-plan.md, settling at minimum: ViewState.board's type + the seed rule's exact trigger (when None, on which reads); select_board's contract mirroring select_stack (argument flow, resets, RefreshNow) + the next/previous walk (canonical order helper — same order as boards list — + the no-wrap rule); the three ActionId rows (contexts, category, dotted ids) + router dispatch; the keymap preset entries you add (existence + resolution in Board + ViNormal; which strokes you choose as defaults — recorded as tunable); the push builder's board argument switch + the no-re-keying argument; board-bar.slint's composition (segments, selected state, archived marker, the compact hide) + the strip mount + adapter click routing through the action layer; the badge's selected-board read upgrade (chunk 1 landed the fold read); the interim-fill removal inventory; the scripted journey leg design (two boards seeded, next/previous, re-render assertions); the kiosk compact degeneration; the full test catalogue (view-state unit tests: select/seed/no-wrap; no insta churn in the UI crate — plain assertions). Resolve every open question with a recorded decision; deviations need a recorded reason.
5. Commit it as: docs(plan): UI board dimension — view state, switcher actions, board bar. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 7.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 7 of the multi-board implementation round ("UI board dimension").

1. Read AGENTS.md, the implementation plan (§2 DF-2/DF-8/DF-9, §3 chunk 7, §4), the design doc (§3.8, §3.2 badge), the GUI requirements R1/R4/R9, the dispatch plan §1/§8, and the stage-1 plan (.artifacts/plans/*multi-board-ui-board-switcher*-plan.md) in the worktree ../taskboard-WT-feat-ui-board-switcher (branch feat/ui-board-switcher). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. The dumb-client rule is absolute: board switching performs NO engine command — FlowOutcome::Command is never produced by select_board/next/previous (pure view state, exactly like stack switching); no IdTables re-keying; presentation logic only — no business logic or state mutation in the UI layer (AGENTS §1/§2); every chunk-2 UI interim fill is replaced; typed/structural assertions, no text assertions (UI labels are UX); deterministic tests (no sleeps, no wall-clock). New .rs files carry the AGPL-3.0-only SPDX header (taskboard-ui-slint is the one crate exempt from #![forbid(unsafe_code)]).
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Bookkeeping: add this chunk's CHANGELOG.md Unreleased line; do NOT touch any tracker issue (MD-4). No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 7.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 7 of the multi-board implementation round (branch feat/ui-board-switcher).

1. First, rebase onto main: in ../taskboard-WT-feat-ui-board-switcher run git fetch origin --prune, then git rebase origin/main (drop already-merged duplicates; resolve only real conflicts — chunk 6 may have landed in parallel; you share no files), and git push --force-with-lease if the rebase rewrote history. Review the full branch diff (git diff origin/main...HEAD) against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 DF-2/DF-8/DF-9, §3 chunk 7), the design doc (§3.8, §3.2 badge), the dispatch plan §1/§8 (the code-quality mandate applies to you in full), and the stage-1 plan.
3. Functional focus — this chunk's specific traps: the dumb-client rule (grep the board-switching path for FlowOutcome::Command — none may exist; a sync nudge on switch is a finding even if convenient); IdTables not re-keyed (diff models.rs — the push builder's board argument switch is the whole change); live_board survives as the seeding rule ONLY (any other read path is a finding); select_board mirrors select_stack exactly (board set, selected_stack reset to the new board's first stack, focused_task to that stack's first task, RefreshNow); next/previous walk the canonical boards-list order with no wrap at the ends; the three ActionIds resolve in Board + ViNormal (assert resolution, not specific default strokes — the preset table is the tunable); the badge reads the selected board's row (not chunk 1's aggregate fold — the interim read must be upgraded); compact hides the strip (kiosk degenerates the same way) while keyboard switching remains; board-bar segment clicks route through the action layer (no direct view mutation — mirroring the select_stack seam); the scripted journey leg pins the exit criterion (two boards, next → second board's stacks re-rendered, previous back); every chunk-2 UI interim marker replaced; primitives reused, none restyled view-locally (R1); no text assertions; no test weakened to pass.
4. Code-quality mandate: (a) code quality — naming, comment discipline, no speculative generality (no board create/archive actions, no fixed-board kiosk config — both backlog), test quality per AGENTS §5; (b) SRP — the UI is a dumb presentation client (reads state, emits view-local outcomes; no business logic, no state mutation — AGENTS §1/§2), the action layer owns verbs, view_state owns selection; (c) single level of abstraction per function body.
5. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
7. Ship: push the branch, open the PR to main with gh (no closure keywords this chunk; MD-4), then gh pr checks --watch to green — diagnose, fix, push --force-with-lease, repeat on any failure. Verify gh pr view --json mergeStateStatus reports CLEAN; BEHIND/DIRTY ⇒ rebase onto origin/main, push --force-with-lease, let checks re-run, repeat. Post the annotation comment on #125. Never merge; never touch main.
8. Teardown: from the project root, git worktree remove --force ../taskboard-WT-feat-ui-board-switcher.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation.
```

## 9. Chunk 8 — Docs + wave exit verification (`docs/multi-board-wave`)

- **Covers** (implementation plan §3 chunk 8 — normative; design §4
  coherence map — the four-vocabulary table as the documentation
  spine, §3.9 compat summary; §5's exit-criteria → verifying-chunk
  map): `docs/architecture.org` — the connector registry +
  `ConnectorModule`, the per-board status map + derived
  attribution, the counted all-down authority, `SetBoard`'s
  retirement, the caller-owned selection story; `README.org` — the
  `[[sync.connectors]]` config shape incl. the legacy-scalar
  synthesis rule and the typed mix rejections, multi-board usage
  (`--board`/`boards use`/the switcher keys); `CHANGELOG.md` — the
  wave's user-visible entries promoted from the per-chunk
  Unreleased lines; the wave exit-criteria verification record at
  `.artifacts/reports/<date>-multi-board-wave-exit-report.md` —
  evidence pointers into the verifying chunks' tests, per the
  implementation plan §5 map: two connectors live simultaneously
  (tier-2) → chunk 5; multiple boards syncing across them → chunk
  5, exercised by 6; one board moved between connectors → chunk 6;
  v0.1.x config + DB upgrade additively keeps working → chunk 1
  (round-trip) + chunk 4 (boot test); board switching by scripted
  keyboard journey alone → chunk 7; `cargo deny` green with the
  promoted port → every chunk. **Non-goals**: no ADR 0010 text
  (landed in chunk 4); no new drift amendments (F10-03/F10-04/
  F15-18 stay backlog); no code.
- **Deliverable**: the two doc amendments, the CHANGELOG promotion,
  and the exit report — the entire allowed diff (plus the stage-1
  work plan).
- **Acceptance**: every exit-criterion row cites a named test in a
  named chunk (test path + what it asserts — no vague pointers);
  tier-2 rows record honestly whether the tier-2 runner ran in the
  verifying chunks (MD-5's records are cited, not papered over);
  the docs match the shipped code (the four-vocabulary table reads
  true against what landed — any drift between prose and code is a
  finding); the CHANGELOG promotion loses no per-chunk Unreleased
  line; the report's method section lets a reader re-verify each
  pointer; gates green (docs-only diff; hooks run regardless); PR
  body carries `Closes #125` — the wave exit, linking the
  verification record.
- **Traps for stage 3**: unevidenced claims (a row citing "chunk 5's
  tests" without naming the test is a finding); prose drifting from
  code (spot-check each architecture.org claim against the
  worktree's crates); re-opening accepted decisions or re-chunking
  history in prose; scope leakage (code, tracker edits, new
  amendments, backlog promotion — the backlog-candidates section is
  for the user, not for autonomous issues); CHANGELOG entries
  invented that no chunk landed (or landed entries dropped).

**Prompt 8.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 8 of the multi-board implementation round ("Docs + wave exit verification", branch docs/multi-board-wave) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunks 5, 6, AND 7 are all merged (git fetch origin --prune; gh pr list --state merged --limit 10 — feat/app-connector-registry, feat/app-cli-connector-verbs, feat/ui-board-switcher). If any is missing, STOP and report instead of planning.

1. Read AGENTS.md, then the implementation plan (§3 chunk 8 + §5's exit-criteria → verifying-chunk map + §6 — normative; §1's Out list), the design doc (§4 the coherence map — the four-vocabulary table; §3.9's compat summary; §7 the backlog-candidates list), and the dispatch plan §1/§9/§10.
2. Read what you will document and verify: docs/architecture.org + README.org + CHANGELOG.md (current state, the per-chunk Unreleased lines chunks 1–7 added), the verifying chunks' tests on main (chunk 1's migration round-trip + snapshot corpus, chunk 4's boot test, chunk 5's two-connector tier-2 test, chunk 6's move-board journey, chunk 7's scripted journey leg), and the tier-2 honest-recording notes in those chunks' PR bodies (MD-5).
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit: STOP and report), then create the worktree ../taskboard-WT-docs-multi-board-wave on a new branch docs/multi-board-wave off main.
4. Write the work plan at .artifacts/plans/<today>-multi-board-wave-exit-docs-plan.md: the architecture.org section-by-section amendment list (registry + ConnectorModule, per-board map + derived attribution, counted all-down authority, SetBoard retirement, caller-owned selection); the README.org amendment list (config shape + synthesis rule + typed rejections; --board/boards use/switcher keys); the CHANGELOG promotion plan (harvest each chunk's Unreleased lines, group as the wave's user-visible entries); the exit report's outline — one section per exit-criterion row, each with: the criterion, the verifying chunk + named test (path + what it asserts), and the tier-2 run/absence record where applicable; the verification method (how you will re-locate and re-run or re-read each cited test). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): multi-board wave exit docs and verification. Do not write the deliverables; do not push.

Report: plan path, the exit-criteria → test-pointer map you will verify, risks.
```

**Prompt 8.2 (research/execution):**

```text
You are the stage-2 EXECUTION agent for chunk 8 of the multi-board implementation round ("Docs + wave exit verification").

1. Read AGENTS.md, the implementation plan (§3 chunk 8, §5, §6, §1 Out list), the design doc (§4, §3.9, §7), the dispatch plan §1/§9/§10, and the stage-1 plan (.artifacts/plans/*multi-board-wave-exit-docs*-plan.md) in the worktree ../taskboard-WT-docs-multi-board-wave (branch docs/multi-board-wave). Recreate the worktree on that branch if missing.
2. Execute the work plan: (a) amend docs/architecture.org and README.org per the accepted lists — every claim true of the shipped code on main; (b) promote the CHANGELOG's per-chunk Unreleased lines into the wave's user-visible entries (lose nothing); (c) write .artifacts/reports/<date>-multi-board-wave-exit-report.md — one section per implementation-plan §5 exit-criterion row, each citing the verifying chunk's named test (path + what it asserts) and the tier-2 run/absence records where applicable (MD-5); include the method section that makes every pointer re-verifiable, and a backlog-candidates section restating the out-of-scope items for the user to promote (no autonomous issue creation). Zero production code; zero tracker edits.
3. Run the full gate suite from AGENTS.md §10 and make it green (docs-only diff; the pre-commit hooks run regardless).
4. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, the exit-criteria verdict summary, any deviation and why.
```

**Prompt 8.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 8 of the multi-board implementation round (branch docs/multi-board-wave).

1. First, rebase onto main: in ../taskboard-WT-docs-multi-board-wave run git fetch origin --prune, then git rebase origin/main (drop already-merged duplicates; resolve only real conflicts), and git push --force-with-lease if the rebase rewrote history. Review the full branch diff (git diff origin/main...HEAD) against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§3 chunk 8, §5, §6), the design doc (§4, §3.9, §7), and the dispatch plan §1/§9/§10 — the evidence & artefact mandate applies to you in full.
3. Functional focus — this chunk's specific traps: every exit-criterion row cites a named test that exists on main (open each cited path; a vague "chunk 5's tests" pointer is a finding); tier-2 rows record the honest run/absence notes from the chunks' PR bodies (a row claiming tier-2 verification the runner never ran is a finding); the docs match the shipped code (spot-check each architecture.org/README.org claim against the worktree — prose-code drift is a finding); the four-vocabulary table reads true against what landed; the CHANGELOG promotion is lossless (diff against the per-chunk Unreleased lines); no ADR 0010 text (chunk 4 landed it — duplication is a finding); no drift amendments, no code, no tracker edits in the diff; the backlog-candidates section restates the out-list for the user without promoting anything.
4. Evidence & artefact mandate (in full — see dispatch plan §1): every claim evidenced; repo hygiene (the PR diff is exactly the two doc amendments + CHANGELOG + the exit report + the work plan); decision-grade recommendations; scope discipline.
5. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
7. Ship: push the branch, open the PR to main with gh (PR body carries "Closes #125" — the wave exit — and links the exit report; MD-4), then gh pr checks --watch to green — diagnose, fix, push --force-with-lease, repeat on any failure. Verify gh pr view --json mergeStateStatus reports CLEAN; BEHIND/DIRTY ⇒ rebase onto origin/main, push --force-with-lease, let checks re-run, repeat. Never merge; never touch main.
8. Teardown: from the project root, git worktree remove --force ../taskboard-WT-docs-multi-board-wave.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation.
```

## 10. Completion bookkeeping

Implementation plan §6 is the normative map; closing happens **in
this round** — issues close as their chunks' PRs merge (via the PR
body's `Closes #N` keywords, MD-4 — GitHub does the edit on merge;
nobody edits the tracker by hand), and annotations are one progress
comment per mapped issue posted by each chunk's stage 3 at PR-open:

| Chunk | Issue action |
|---|---|
| 1 | **closes #298** (SyncStatus three-layer singleton, F16-22); annotates #125 (map + 0006 landed) |
| 2 | annotates #125 (board-scoped vocabulary landed) |
| 3 | **closes #179** (`--forget-board`/unbind), **closes #170** (title-match reconciliation); annotates #125 |
| 4 | annotates #187 (connector identity end-to-end, trait promoted, ADR 0010 amended), #125 |
| 5 | **closes #187** (multiple active connectors — the N=2 exit criterion verified); annotates #125 |
| 6 | annotates #125 |
| 7 | annotates #125 |
| 8 | **closes #125** (multi-board support — wave exit, verification record linked) |

Also rides the chunks: ADR 0010 amendment text → chunk 4 (design
§6's draft, verbatim); `docs/architecture.org` + `README.org` +
`CHANGELOG.md` promotion → chunk 8; per-chunk CHANGELOG Unreleased
lines land with each chunk (1–7) and are promoted by 8. The round
creates, labels, and closes **nothing** beyond the five milestone
issues above (impl §6; AGENTS §7).

**Deliberately not dispatched here** (implementation plan §1 Out
list = design §7 backlog candidates — restated so nobody re-derives
the boundary; the user promotes them per AGENTS §7, and chunk 8's
report restates them again): per-connector
`poll_interval`/backoff tuning; hysteresis beyond the episode-edge
rule; config-pinned fixed-board kiosk; UI board creation/archive
actions; fuzzy/normalized-key reconciliation growth
(`boards reconcile --dry-run` included); daemon-side per-connector
attach/detach verbs over IPC; connector configuration via env
(figment cannot express the array); ADR 0010 drift amendments
F10-03/F10-04/F15-18; #157 per-card refresh perf work (guardrail in
DF-5 — the issue stays unscheduled); Projects hierarchy,
multi-user decks, org-file connector, the D5 app-data substrate
implementation (its one shared decision point, F16-08, is decided —
D-W4), LAN sync server, CRDT collaboration. Also not dispatched:
any tracker work beyond the table above.

**What opens next**: once chunk 8's PR merges (and the user merges
the wave), the `wave-multi-board` milestone is complete — the
natural follow-ups are the user promoting the backlog candidates
above into issues, and (independently schedulable) the post-wave
horizon items the design names (the trait's second connector
implementation is the post-wave horizon, not an in-wave exit
criterion). No further dispatch round is implied by this document
beyond the eight chunks it dispatches.

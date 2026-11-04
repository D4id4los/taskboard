# [Plan] Files Substrate (B-min) — Implementation Dispatch Plan & Agent Prompts

- **Date**: 2026-11-05
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the 4 implementation chunks of the
  accepted DR-8 design round. No code in this commit.
- **Origin**: the accepted design plan
  `.artifacts/plans/2026-11-05-filesync-substrate-design-round-plan.md`
  (normative: §3 the design Parts A–F with decisions D-SF1…D-SF19,
  §5 test hooks, §6 risks, §7 the chunk table) — ACCEPTED by the user
  2026-11-05; the session rulings Q1–Q8 on issue #626; the transport
  facts `.artifacts/reports/2026-11-05-nextcloud-files-webdav-characterization-report.md`
  (F1–F8 + §2/§3, binding); the cross-milestone contract
  `.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md`
  §3. Chunk issues: #629, #630, #631, #632 (milestone
  `nextcloud-files-sync`); round issue #626 closes with chunk 4.
- **Status**: READY — chunks 1–2 dispatchable immediately and run
  **parallel to all v0.2 work** (disjoint files). Chunk 3 is **gated
  on v0.2's journal entity chunk** (the first per-user home) — the
  only cross-milestone gate. Chunk 4 follows 3. **Deliberately not
  dispatched here**: the settings/proofs second slice (post-B-min,
  own round), notify_push (websocket dep — own stack-approval),
  chunk/snapshot *emission* (the reserved 5.5 kinds — B-full
  territory), the Q6 auto-merge/manual-merge phases, B-full itself.

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | Size | Issue |
|---|---|---|---|---|---|
| 1 | D-SF2–D-SF5 + ADR 0020 | `feat/sync-files-core` | The pure core crate `taskboard-sync-files`: layout types, tolerant codecs, the fold + laws, state hash, heartbeat types | M | #629 |
| 2 | D-SF4, D-SF6–D-SF10, D-SF17 | `feat/sync-files-nextcloud` | The transport crate `taskboard-sync-files-nextcloud`: WebDAV client, conditional-PUT discipline, change detection, extractor, config, registry wiring, docker pins | M | #630 |
| 3 | D-SF11–D-SF13 | `feat/filesync-engine-seam` | `UserDataReport` family, the engine fold of imported sections, the journal import (fold-first); honours the v0.2 §3 contract | M | #631 |
| 4 | D-SF14–D-SF15 surfacing + docs | `feat/filesync-hardening` | Typed status findings + divergence alarm + `status` line, fork/trashbin runbooks, architecture.org/README, live Tier-3 pins | S | #632 |

**Dependency graph**: strictly serial 1 → 2 → 3 → 4 (each consumes
its predecessor's crate/wiring). Chunk 3 additionally waits on the
**external** v0.2 gate (the journal entity landed on `main`); if that
gate has not arrived when chunk 2 merges, chunk 4's docs/runbook/live-
pin work may be promoted ahead of 3 by the dispatching user (its code
surface is independent; note the re-sequencing in the PR bodies).
Nothing in any chunk touches v0.2's files **except** chunk 3's engine
ingestion seam — that addition is additive (a new report family), and
chunk 3 rebases onto whatever v0.2 state exists at its base.

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); never two agents share a worktree.
Every chunk works in `../taskboard-WT-<branch>` next to the project
root (AGENTS §8), branched off the then-current `main`, no PR ever
stacked. **Branch names are never reused after their PR lands**
(AGENTS §8, issue #415): a resume/second round of a chunk works on
`<branch>-v2` (or re-branches off the fetched `origin/main`), never on
the landed branch. Create worktrees with `git worktree add -b <branch>
../taskboard-WT-<branch>` (never `git worktree add <path> main`). Disk
hygiene (AGENTS §8 worktree lifecycle): any agent creating or
recreating a worktree runs `scripts/worktree-doctor.sh` from the main
checkout first — nonzero exit (free disk below the floor) stops the
chunk; stage 3 removes the worktree as its final handoff task.

**Shared execution model** (binding for every chunk):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current `main`,
   writes the TDD-first plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push. Origin
   sections marked normative are binding; deviations need a recorded
   reason.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing — if the branch or the
   worktree path's PR already landed, create `<branch>-v2` instead per
   AGENTS §8), executes per the approved plan — TDD, every commit
   standalone (rebase-merge hygiene), full AGENTS §10 gate suite green
   through `scripts/run-quality-gates.sh`. Does **not** push or open a
   PR. Reports deviations.
3. **Stage 3 — review agent** *(protocol amended 2026-10-18)*:
   **begins by rebasing onto `main`** — `git fetch origin --prune`,
   rebase the branch onto `origin/main` (drop already-merged duplicate
   commits; resolve only real conflicts), push `--force-with-lease` if
   the rebase rewrote history — then reviews the entire branch diff
   versus the **post-rebase** base against the chunk's acceptance
   criteria + AGENTS rules **plus the code-quality mandate below**,
   re-runs the full gate suite, implements fixes for every finding on
   the branch (amend/reword, never duplicate-subject fixups). Its last
   tasks are shipping: push the branch, open the PR to `main` with
   `gh`, watch `gh pr checks --watch` to completion — any failing
   check is a finding; diagnose, fix, push `--force-with-lease`,
   repeat until green — **then verify the PR's merge state**: `gh pr
   view --json mergeStateStatus` must report `CLEAN`; `BEHIND`/`DIRTY`
   ⇒ rebase onto `origin/main`, push `--force-with-lease`, repeat; any
   other state ⇒ diagnose before handing off. **Then teardown**:
   remove the now-disposable worktree — from the project root,
   `git -C /home/viktoria/src/taskboard worktree remove --force
   ../taskboard-WT-<branch>` (absolute anchor per AGENTS §8; everything
   precious is pushed; a later round that resumes the chunk recreates
   it on a fresh branch). Confirm via `git worktree list` that the
   removed path was the intended one. The handoff contract is "PR open
   + checks green + merge state `CLEAN` + worktree removed". Reports a
   verdict. Never merges; never touches `main`. (`main` is
   branch-protected — merges happen on GitHub by the user.)

**Code-quality mandate** (stage 3, every chunk — the house text):

> Beyond functional review, evaluate (a) code quality: naming, comment
> discipline (comments state constraints, not narration), idiomatic
> use of the approved stack, no dead code or speculative generality,
> test quality per AGENTS §5; (b) the Single Responsibility Principle:
> the crate-role boundaries of AGENTS §2 are the SRP line — a boundary
> violation is an SRP violation; (c) a single level of abstraction per
> function body: a function either orchestrates named operations or
> works in primitive data, never mixes. Findings are fixed on the
> branch like any other finding.

---

## 2. Chunk 1 — files core crate (#629) (`feat/sync-files-core`)

- **Covers**: design §3 Part A (D-SF2–D-SF3) + Part B (D-SF5) + the
  heartbeat/state-hash types (D-SF14's type half) + **ADR 0020**. The
  new pure crate `taskboard-sync-files`: the tree-layout types
  (`identity.json` payload; the device-subtree file taxonomy incl. the
  reserved 5.5 kinds `ops/*.json`, `snapshot-*.json` as *tolerated
  opaque* — parse-never, relay-never-choke); the per-user section
  document codecs (versioned envelope, per-section
  `BTreeMap<uuid, UserRecord{Entry, Tombstone, Verbatim}>`,
  hand-written tolerant serde — unknown fields ignored, unknown rows →
  `Verbatim`; reserved section names `settings`/`proofs` never
  emitted, relayed when seen); the pure fold — winner
  `max by (clock, author?, entry-id)` with the author slot present
  but `None` for every v1 writer; the canonical state-hash
  serialisation; the heartbeat payload types. **Non-goals**: any IO,
  async, transport, HTTP, or slint (chunk 2+); engine/storage changes
  (chunk 3); emitting reserved kinds; a `[files_sync]` config section.
- **Acceptance**: the D-SF5 laws proptested over generated document
  sets (commutativity, idempotence, associativity, tombstone-vs-edit
  clock races, Verbatim opacity, reserved-kind tolerance, author-None
  totality + a forward pin with `Some(author)` values); codec
  roundtrips incl. garbage rows (floors on garbage, no ceilings on
  good input); zero IO symbols in the crate (a `cargo tree`/grep pin
  in review); SPDX `MIT OR Apache-2.0`, `#![forbid(unsafe_code)]`;
  workspace-pinned deps only (`cargo deny` untouched); ADR 0020
  records the substrate contract (server-enforced path ownership,
  current-state v1 on the 5.5-compatible layout, the B-full
  reservations, the Q6 conflict posture + `base_clock` reservation).

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the DR-8 implementation dispatch ("files core crate", branch feat/sync-files-core) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the normative origin: .artifacts/plans/2026-11-05-filesync-substrate-design-round-plan.md §3 Parts A–B (D-SF1–D-SF5), §5 test hooks, §7 chunk 1 — plus the session rulings Q1–Q8 on issue #626 and the backbone context .artifacts/reports/2026-10-11-generic-file-sync-connector-protocol-options-report.md §5.4/§5.5 (the layout lineage).
2. Read the seams you mirror: crates/taskboard-domain/src/appdata.rs (AppDataSections, union, RuleEntry, the tolerant-serde row style — your codecs follow this recipe), the state-hash precedent (any canonical serialisation hash in-tree; else serde_json::to_string of BTree-ordered types — decide and record), the workspace root Cargo.toml ([workspace.dependencies] pinning), docs/adr/0017…0019 (the ADR house style; you write 0020).
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit: STOP and report), then create the worktree with git worktree add -b feat/sync-files-core ../taskboard-WT-feat-sync-files-core off main.
4. Write the TDD-first plan at .artifacts/plans/<today>-sync-files-core-plan.md settling: module layout of the new crate, the exact wire JSON shapes (with examples), the fold API surface (types + the order key), the state-hash canonicalisation rule, the reserved-kind/section tolerances, the proptest catalogue incl. the author-None totality + Some(author) forward pin, ADR 0020's outline.
5. Commit as: docs(plan): the files core crate. Do not implement; do not push.

Report: plan path, the fold API shape, decisions taken, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the DR-8 dispatch ("files core crate").

1. Read AGENTS.md, the design plan §3 Parts A–B/§5, the dispatch plan §2, and the stage-1 plan in ../taskboard-WT-feat-sync-files-core (branch feat/sync-files-core). Recreate the worktree on that branch if missing.
2. Implement exactly per the plan, TDD (failing tests first), every commit standalone. Hard rules: the crate is pure — no IO, no async, no HTTP, no UI deps; tolerant serde is hand-written (no derive beyond what appdata.rs already derives); unknown rows become Verbatim, never errors; reserved names (settings, proofs, ops/, snapshot-*) are relayed/ignored, never emitted; no text assertions; proptest per AGENTS §5; SPDX MIT OR Apache-2.0 + #![forbid(unsafe_code)] on every new file.
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. Bookkeeping: ADR 0020 (docs/adr/0020-*.org, house style); CHANGELOG Unreleased entry; do not close #629 (the PR body carries "Closes #629").
5. No push, no PR — shipping is stage 3's. Never push to or merge main.

Report: per-gate results, deviations from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the DR-8 dispatch (branch feat/sync-files-core).

1. Read AGENTS.md, the design plan §3 Parts A–B/§5 (the code-quality mandate in dispatch §1 applies to you), and the stage-1 plan; review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-sync-files-core. Begin by rebasing onto origin/main per the shared execution model.
2. Functional focus — this chunk's traps: (a) the order type is TOTAL with None authors — a fold where two documents with equal clocks and None authors can produce different winners depending on input order is a divergence bug (the proptest law must exist and be meaningful); (b) tolerance is structural: garbage input yields Verbatim/rejection data, never a decode error that kills a whole document (floors on garbage, no ceilings); (c) reserved kinds/names are never emitted — grep the diff for emissions; (d) zero IO/async in the crate; (e) ADR 0020 states the B-full reservations and the Q6 base_clock reservation accurately; (f) workspace dep pinning untouched.
3. Code-quality mandate: (copy verbatim from §1 of this dispatch plan).
4. Re-run the full gate suite yourself; any failure is a finding.
5. Fix every issue on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push, open the PR (title: feat(sync-files): the files core crate; body "Closes #629"), gh pr checks --watch to green; verify mergeStateStatus CLEAN (rebase + --force-with-lease if BEHIND/DIRTY). Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-sync-files-core; confirm via git worktree list.

Report: verdict, findings (functional + mandate), fixes, PR URL, green-checks confirmation, worktree-removed confirmation.
```

---

## 3. Chunk 2 — nextcloud files transport (#630) (`feat/sync-files-nextcloud`)

- **Covers**: design §3 Parts C–D (D-SF4, D-SF6–D-SF10, D-SF17). The
  crate `taskboard-sync-files-nextcloud` over chunk 1's core: the
  WebDAV client on the workspace reqwest stack; **conditional-PUT
  discipline** — `identity.json` create-once (`If-None-Match: *`,
  412 → adopt the winner; F3), own-path rewrites with `If-Match` and
  the **etag echoed verbatim as served** (F4 — the double-quoting
  pitfall is pinned by a docker-tier test), unexpected 412 on one's
  own path = the typed fork status (stop-write, keep-read, loud);
  **change detection** — one `PROPFIND Depth: 1` over `devices/` with
  `getetag` per cycle (F6), re-GET changed, **skip-PUT by local
  content hash** (F5: server etags are upload-churned, useless for
  skip decisions); the narrow hand-rolled PROPFIND extractor
  (fixed-shape request bodies, fuzzed; no XML dependency); device
  identity (`device.json`, create-once in the app data dir); the
  `[files_sync]` config section (`enabled = false` default, optional
  `poll_secs`; absent table = nothing spawns; one account — Q4 — via
  `CredentialStore`, ADR 0008 env escape unchanged); bootstrap
  registry wiring (mode-agnostic, D-SF18); **docker-tier pins of
  F3–F6** against the project's Nextcloud compose image. **Non-goals**:
  notify_push (fenced — characterization §2), engine ingestion
  (chunk 3), status surfacing beyond emitting the typed findings
  (chunk 4), chunked upload (F7 present but unused).
- **Acceptance**: docker-tier tests pin create-once (201/412),
  lost-update guard (204/412 with the verbatim-etag pin), etag churn
  (identical re-PUT → new etag; skip-PUT unaffected), tree-etag
  listing; the in-process fake-transport matrix (reordered, duplicated,
  garbage, torn, unexpected-412 deliveries) is deterministic
  (`tokio::time` paused); the actor never claims network authority
  (pinned); absent `[files_sync]` spawns nothing (bootstrap pin);
  `cargo deny` untouched.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the DR-8 implementation dispatch ("nextcloud files transport", branch feat/sync-files-nextcloud) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunk 1 (feat/sync-files-core) landed on main; verify (git log origin/main). If in flight, STOP and report.

1. Read AGENTS.md, then the normative origins: the design plan §3 Parts C–D (D-SF4, D-SF6–D-SF10, D-SF17–D-SF18) + §5; the characterization report F1–F8 + §2/§3 (.artifacts/reports/2026-11-05-nextcloud-files-webdav-characterization-report.md — the transport facts are binding); the rulings Q4/Q5/Q8 on #626.
2. Read the seams: chunk 1's crate (the core types you transport); crates/taskboard-sync-nextcloud/src/ (the client/actor/retry patterns you mirror — spawn_sync_actor's shape, the poll loop, Retry-After handling); crates/taskboard-app/src/{bootstrap.rs,connectors.rs,config.rs} (registry wiring, config-section patterns incl. [focus]'s absent-table posture, CredentialStore); the docker Nextcloud compose tier + wiremock patterns (tests you extend); crates/taskboard-app/src/data-dir usage (where device.json lives).
3. Run scripts/worktree-doctor.sh first (nonzero: STOP and report); create the worktree with git worktree add -b feat/sync-files-nextcloud ../taskboard-WT-feat-sync-files-nextcloud off main.
4. Write the TDD-first plan at .artifacts/plans/<today>-sync-files-nextcloud-plan.md settling: the client module shape (verbs needed: MKCOL, PROPFIND, GET, PUT-cond, DELETE-never?), the etag store (per-file last-known etags, in-memory LoopState), the fork-status type and its stop-write semantics, the cycle state machine (PROPFIND → changed-set → fold-core (chunk 1 fn) → own-publish hash-compare → conditional PUTs → heartbeat refresh), the extractor's exact request bodies + parse surface + fuzz target, config validation ladder arms, bootstrap wiring, the docker-tier + fake-transport test catalogue.
5. Commit as: docs(plan): the nextcloud files transport. Do not implement; do not push.

Report: plan path, the cycle state machine, the fork-status semantics, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the DR-8 dispatch ("nextcloud files transport").

1. Read AGENTS.md, the design plan §3 Parts C–D, the characterization report, the dispatch plan §3, and the stage-1 plan in ../taskboard-WT-feat-sync-files-nextcloud (branch feat/sync-files-nextcloud). Recreate the worktree on that branch if missing.
2. Implement exactly per the plan, TDD (failing tests first), every commit standalone. Hard rules: etags are echoed verbatim as served (the characterization's double-quoting bug is a pinned docker-tier test, not a comment); unexpected 412 on an own path is the typed fork status — stop writing that path, keep reading, surface loudly; skip-PUT compares a LOCAL content hash (server etags churn per upload — never use them for skip decisions); the PROPFIND bodies are fixed constants and the extractor parses only what we request; no notify_push, no websocket, no XML crate; the actor is not a network authority; absent [files_sync] = nothing spawns; deterministic tests (tokio time paused, fake transport; docker tier behind its filter); no text assertions; SPDX + forbid(unsafe) on new files.
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green (the docker tier rides its existing invocation).
4. Bookkeeping: CHANGELOG Unreleased entry; do not close #630 (PR body carries "Closes #630").
5. No push, no PR.

Report: per-gate results, deviations and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the DR-8 dispatch (branch feat/sync-files-nextcloud).

1. Read AGENTS.md, the design plan §3 Parts C–D, the characterization report F1–F8 (the code-quality mandate in dispatch §1 applies), and the stage-1 plan; review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-sync-files-nextcloud. Begin by rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) the etag discipline: create-once uses If-None-Match:* with 412→adopt; rewrites use If-Match with the SERVED etag verbatim; any etag string munging (trimming, re-quoting) is a finding; (b) the fork alarm: an unexpected 412 on an own path must stop writes for that path and keep reads — a retry loop that re-PUTs after a fork-indicating 412 is a finding; (c) skip-PUT must be local-hash-based — any use of server etags as content identity is a finding (F5); (d) the extractor's parse surface: it must tolerate garbage/torn bodies as typed rejections, and the fuzz target exists; (e) no network-authority claims; no board ops, no SyncReport emission; (f) the docker-tier pins actually assert the conditional-PUT semantics (201/412, 204/412), not just HTTP reachability.
3. Code-quality mandate: (copy verbatim from §1 of this dispatch plan).
4. Re-run the full gate suite; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR (title: feat(sync-files): the nextcloud files transport; body "Closes #630"), checks green, mergeStateStatus CLEAN. Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-sync-files-nextcloud; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green-checks confirmation, worktree-removed confirmation.
```

---

## 4. Chunk 3 — engine seam & journal import (#631) (`feat/filesync-engine-seam`)

- **Covers**: design §3 Part E (D-SF11–D-SF13). The per-user report
  family `UserDataReport { sections }` on the connector→engine channel
  — **`SyncReport` is not widened** (board-shaped, stays); the
  engine's fold of imported sections (the sidecar-fold precedent:
  clock-guarded imports, tombstones applied, `Verbatim` ignored) into
  the per-user homes; the journal import as first consumer —
  fold-first (read all peers, import), then publish the local journal
  as the device's first `appdata.json`; the push side materialises
  from published state per cycle (hash-compare, PUT iff changed —
  chunk 2's mechanism, now carrying the journal section). Honours the
  cross-milestone contract (v0.2 impl-plan §3) verbatim. **Non-goals**:
  `LocalOp`/outbox changes (none — pinned), board ops for per-user
  data (none — pinned), statistics/UI, settings/proofs sections.
- **Gate (external)**: v0.2's journal entity chunk must be landed on
  `main` (the first per-user home). The stage-1 agent verifies the
  entity + its state home exist under the vocabulary DR-7 ruled; if
  absent, **STOP and report** — never invent a placeholder entity.
- **Acceptance**: insta state snapshots of import/tombstone/skip
  journeys; the no-board-ops pin (no `UpdateAppData` enqueued by
  per-user imports); `SyncReport` untouched (grep pin); the import
  honours per-entry clocks/tombstones from the contract; end-to-end
  journey: two engines + the fake transport converge journals.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the DR-8 implementation dispatch ("engine seam & journal import", branch feat/filesync-engine-seam) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition (external gate): the v0.2 journal entity chunk is landed on main — verify the reflection/journal entity, its state home, and its storage exist (the vocabulary DR-7 ruled; see the v0.2 dispatch's chunk table). If absent, STOP and report instead of planning (never invent a placeholder entity).

1. Read AGENTS.md, then the normative origins: the design plan §3 Part E (D-SF11–D-SF13) + §5; the cross-milestone contract .artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md §3 (binding); the landed journal entity's own design doc (whatever DR-7's round produced — read it for the entity's exact shape).
2. Read the seams: the engine's sync-ingestion path (where SyncReport::Completed folds; your UserDataReport joins beside it), the sidecar folds in domain merge.rs/pipeline.rs (your fold semantics mirror), the connector actor's cycle from chunk 2 (where the report is emitted), AppState's per-user home (the journal state), the insta snapshot machinery.
3. Run scripts/worktree-doctor.sh first (nonzero: STOP and report); create the worktree with git worktree add -b feat/filesync-engine-seam ../taskboard-WT-feat-filesync-engine-seam off main.
4. Write the TDD-first plan at .artifacts/plans/<today>-filesync-engine-seam-plan.md settling: the UserDataReport type + channel join (SyncReport untouched), the engine fold arm(s), the import-vs-local clock guard (the sidecar-fold rule), the fold-first-then-publish attach ceremony, the publish derivation (state → journal section → document), the no-board-ops pin, the two-engine convergence journey design.
5. Commit as: docs(plan): the user-data engine seam and journal import. Do not implement; do not push.

Report: plan path, the seam shape, how the contract is honoured line-by-line, risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the DR-8 dispatch ("engine seam & journal import").

1. Read AGENTS.md, the design plan §3 Part E, the v0.2 contract §3, the dispatch plan §4, and the stage-1 plan in ../taskboard-WT-feat-filesync-engine-seam (branch feat/filesync-engine-seam). Recreate the worktree on that branch if missing.
2. Implement exactly per the plan, TDD (failing tests first), every commit standalone. Hard rules: SyncReport is not widened (a grep-visible pin test exists); per-user imports never enqueue board ops (pinned); the fold is clock-guarded exactly like the sidecar folds (import only strictly-newer; tombstones compete); the attach ceremony is fold-first (a device that publishes before importing can resurrect its own tombstones over peers' deletes — the ordering is a pinned property); deterministic tests only; no text assertions; SPDX + forbid(unsafe) on new files.
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. Bookkeeping: CHANGELOG Unreleased entry; do not close #631 (PR body carries "Closes #631").
5. No push, no PR.

Report: per-gate results, deviations and why.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the DR-8 dispatch (branch feat/filesync-engine-seam).

1. Read AGENTS.md, the design plan §3 Part E, the v0.2 contract §3 (the code-quality mandate in dispatch §1 applies), and the stage-1 plan; review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-filesync-engine-seam. Begin by rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) the fold-first ordering at attach (publish-before-import resurrection is the named bug class — the property test must exist); (b) clock-guard strictness (an import of an equal-or-older row must be a no-op, not a stamp bump); (c) the two-engine convergence journey is end-to-end through the fake transport, not two unit folds glued by assertion; (d) no board ops, no SyncReport widening, no LocalOp additions (grep pins); (e) the journal entity's own vocabulary/rules are consumed, not redefined here.
3. Code-quality mandate: (copy verbatim from §1 of this dispatch plan).
4. Re-run the full gate suite; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR (title: feat(state,sync-files): the user-data seam and journal import; body "Closes #631"), checks green, mergeStateStatus CLEAN. Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-filesync-engine-seam; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green-checks confirmation, worktree-removed confirmation.
```

---

## 5. Chunk 4 — hardening, status surfacing, runbooks (#632) (`feat/filesync-hardening`)

- **Covers**: design §3 Part F's surfacing half + D-SF15's surfacing +
  the docs. The typed sync-status findings — freshness ages (heartbeat
  `written_at`), quarantined/parked-file notices, the fork alarm from
  chunk 2's stop-write state, and the **divergence finding**
  (state hashes disagreeing past the quiescence window: two cycles +
  no pending writes) — surfaced on the sync-status surface + a
  `status` CLI line, in the existing status vocabulary's shape
  (minimal; the stale-peers taxonomy question stays open and
  un-invented). The runbooks as docs: the fork/device-rename
  procedure, trashbin recovery (F8). `docs/architecture.org` + README
  amendments (the substrate's crates, config, and posture). Live
  Tier-3 `#[ignore]` pins riding the `.env` convention, re-pinning
  F3–F6 against the real hosted tier. **Non-goals**: new UI views,
  notify_push, settings/proofs, any new status taxonomy.
- **Acceptance**: each finding is a typed status with structural
  assertions (no text pins beyond the phrase functions' own tests);
  the quiescence window is deterministic under paused time; runbooks
  are reviewable steps (rename procedure actually matches the code's
  stop-write semantics); live pins skip-never-fail without the env
  vars; architecture.org/README amended; CHANGELOG entry; **#626 (the
  round) closes with this chunk's PR**.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of the DR-8 implementation dispatch ("substrate hardening & surfacing", branch feat/filesync-hardening) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunks 1–3 landed on main; verify (git log origin/main). If 3 is in flight, STOP and report (this chunk consumes its reports).

1. Read AGENTS.md, then the normative origins: the design plan §3 Part F + D-SF14/D-SF15 + §6; the characterization report §2/§3 (the hosted-drift caveat your live pins address); issue #632.
2. Read the seams: the sync-status surface + its typed findings vocabulary (what chunk 2's actor already emits; where the UI/CLI read it), the CLI status verb (crates/taskboard-app/src/commands/status.rs), the heartbeat/state-hash machinery from chunks 1–2, docs/architecture.org + README.org structure, the Tier-3 test convention (docs/testing_strategy.org §8 + the existing #[ignore] live tests).
3. Run scripts/worktree-doctor.sh first (nonzero: STOP and report); create the worktree with git worktree add -b feat/filesync-hardening ../taskboard-WT-feat-filesync-hardening off main.
4. Write the TDD-first plan at .artifacts/plans/<today>-filesync-hardening-plan.md settling: the findings enum growth (or its surface mapping), the quiescence-window derivation (deterministic), the status-line shape, the runbook docs' home and content outline, the live-pin test set, the docs amendments.
5. Commit as: docs(plan): the substrate hardening and surfacing. Do not implement; do not push.

Report: plan path, the findings set, risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of the DR-8 dispatch ("substrate hardening & surfacing").

1. Read AGENTS.md, the design plan §3 Part F, the dispatch plan §5, and the stage-1 plan in ../taskboard-WT-feat-filesync-hardening (branch feat/filesync-hardening). Recreate the worktree on that branch if missing.
2. Implement exactly per the plan, TDD (failing tests first), every commit standalone. Hard rules: no new status taxonomy — the findings map onto the existing vocabulary's shape, and the stale-peers open question stays open; the quiescence window is clock-injected (deterministic under paused time); live Tier-3 tests are #[ignore] + skip-never-fail without TASKBOARD_IT_NEXTCLOUD_*; runbooks describe the code's actual stop-write/fork semantics (cite the symbols); no text assertions; SPDX on new files.
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. Bookkeeping: CHANGELOG Unreleased entry; docs/architecture.org + README amendments; the PR body carries "Closes #632" and "Closes #626" (the round completes here).
5. No push, no PR.

Report: per-gate results, deviations and why.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of the DR-8 dispatch (branch feat/filesync-hardening).

1. Read AGENTS.md, the design plan §3 Part F (the code-quality mandate in dispatch §1 applies), and the stage-1 plan; review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-filesync-hardening. Begin by rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) the divergence finding's quiescence window cannot fire on a merely-slow peer (freshness age and divergence are different findings); (b) no invented status taxonomy (diff-shape: existing vocabulary types, new arms/mappings at most); (c) the runbooks' rename procedure matches the actual fork semantics (a runbook telling the user to "just delete the other device's folder" would violate the readers-never-delete rule — a finding); (d) live pins assert the F3–F6 semantics, and skip cleanly without the env; (e) docs amendments match the shipped config/crate names.
3. Code-quality mandate: (copy verbatim from §1 of this dispatch plan).
4. Re-run the full gate suite; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR (title: feat(app,sync-files): the substrate status surfacing and runbooks; body "Closes #632. Closes #626."), checks green, mergeStateStatus CLEAN. Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-filesync-hardening; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green-checks confirmation, worktree-removed confirmation.
```

---

## 6. Completion bookkeeping

- **Issues closed by chunks**: 1 → #629; 2 → #630; 3 → #631;
  4 → #632 **and #626** (the round completes with its last chunk).
  The milestone `nextcloud-files-sync` itself completes when the user
  rules B-min done (journal roaming live); the **second slice**
  (settings/proofs) opens as its own round after that ruling — not
  from this dispatch.
- **ADR**: 0020 lands with chunk 1; no other chunk writes an ADR.
- **Docs**: architecture.org/README amendments are chunk 4's (not per
  chunk); CHANGELOG entries are per chunk (house rule).
- **Deliberately not dispatched here**: the settings/proofs second
  slice; notify_push (websocket dep, own stack-approval); chunk/
  snapshot emission (reserved 5.5 kinds — B-full); the Q6 auto-merge +
  manual diff/merge phases; B-full itself (own round when it opens,
  per the milestone description); any v0.2 work (own dispatch plan).
- **What opens next**: chunks 1–2 may dispatch immediately, in
  parallel with v0.2's chunks (disjoint files). Chunk 3 dispatches
  once v0.2's journal entity lands; chunk 4 last. When all four
  merge, the handoff to the user is: B-min complete — rule the
  milestone done, then the second-slice round and (in due course) the
  B-full round.

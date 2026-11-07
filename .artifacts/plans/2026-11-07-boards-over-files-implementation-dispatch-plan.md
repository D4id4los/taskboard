# [Plan] Boards over Files (B-full) — Implementation Dispatch Plan & Agent Prompts

- **Date**: 2026-11-07
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the 6 implementation chunks of the
  accepted DR-9 design round. No code in this commit.
- **Origin**: the accepted design plan
  `.artifacts/plans/2026-11-07-boards-over-files-design-round-plan.md`
  (normative: §0 rulings Q1–Q6, §1 anchors, §3 the design Parts A–F
  with decisions D-BF1…D-BF13, §5 test hooks, §6 risks, §7 the chunk
  table) — ACCEPTED by the user 2026-11-07; the rulings on issue #678;
  the backbone report
  `.artifacts/reports/2026-10-11-generic-file-sync-connector-protocol-options-report.md`
  (§5.5/§7.3/§7.4 — the lifecycle and F1 lineage); the DR-8 round +
  dispatch (the substrate being extended) and its characterization
  report (the transport facts). Chunk issues: #684, #685, #686,
  #687, #688, #689 (milestone `nextcloud-files-sync`); the round issue
  #678 closes with chunk 6.
- **Status**: READY — chunks 1–2 dispatchable immediately (disjoint
  files; may run alongside **#632**, B-min's hardening chunk, whose
  prompts live in the DR-8 dispatch plan §5). Chunks 3–6 are gated as
  marked (chunk 4+ additionally waits for #632's status vocabulary).
  **Deliberately not dispatched here**: #632 (already prompted
  elsewhere), the org pass (fenced to the far future as a separate
  connection, per ruling Q1), files→Deck moves, per-user attribution
  (#494), notify_push, LAN/folder-client transports.

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | Size | Issue |
|---|---|---|---|---|---|
| 1 | Part A, D-BF1–D-BF2 | `feat/f1-author-clocks` | Parallel author structs, `(clock, author)` order, additive text forms + migration 0023, sidecar `by`, convergence laws, **ADR 0022** | M/L | #684 |
| 2 | Part B, D-BF3 | `feat/userdata-settings-proofs` | The `settings`/`proofs` root sections (roam-set pinned as data; set-union proofs), `UserDataReport` fold arms | M | #685 |
| 3 | Part C, D-BF4–D-BF5 | `feat/remote-identity-widening` | Connector-tagged `Remote*` family, backend-tagged report payloads, files-ids-are-UUIDs, typed ownership map + loud second-claim refusal | M/L | #686 |
| 4 | Part D, D-BF6–D-BF7 | `feat/board-chunk-lifecycle` | Board subtrees, immutable materialized chunks, snapshots, GC, tombstones-forever, late-join | M/L | #687 |
| 5 | Part E, D-BF8–D-BF10 | `feat/boards-over-files` | Outbox routing by assignment, files-board cycles + Files-tagged reports, native app-data, no-sidecar | L | #688 |
| 6 | Parts F–G, D-BF11–D-BF12 | `feat/deck-to-files-move` | The guided move ceremony, retirement typing, fold-first adoption, mixed-fleet docs, wave exit | M | #689 |

**Dependency graph**: **1 ∥ 2** (disjoint files) → **3** (shares
domain clock/entity files with 1) → **4** (consumes 1's authors + 3's
tagged identities) → **5** (consumes 3's routing + 4's lifecycle) →
**6** (last). **External gate**: chunks 4–6 wait for **#632** to land
(its status/findings vocabulary is what 5–6 grow; 4 may start the
moment #632 merges, and 1–2 run alongside it freely). Parallel
streams: v0.2's remaining chunks (#623 slice B, #624 PDF) are
disjoint territory (ui-slint/app-export); no coordination needed
beyond the usual rebase-on-start.

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); never two agents share a worktree.
Every chunk works in `../taskboard-WT-<branch>` next to the project
root (AGENTS §8), branched off the then-current `main`, no PR ever
stacked. **Branch names are never reused after their PR lands**
(AGENTS §8, issue #415): a resume/second round works on `<branch>-v2`
or re-branches off the fetched `origin/main`. Create worktrees with
`git worktree add -b <branch> ../taskboard-WT-<branch>` (never
`git worktree add <path> main`; spell the worktree path suffix flat —
replace `/` in the branch name with `-`). Disk hygiene: any agent
creating or recreating a worktree runs `scripts/worktree-doctor.sh`
from the main checkout first — nonzero exit (free disk below the
floor) stops the chunk; stage 3 removes the worktree as its final
handoff task.

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
   ⇒ rebase onto `origin/main`, push `--force-with-lease`, let the
   checks re-run, repeat; any other state ⇒ diagnose before handing
   off. **Then teardown**: remove the now-disposable worktree — from
   the project root, `git -C /home/viktoria/src/taskboard worktree
   remove --force ../taskboard-WT-<branch>` (absolute anchor per
   AGENTS §8 — never a bare relative path, which resolves against
   whatever cwd the session happens to be in and can name a
   neighbour's worktree; everything precious is pushed; a later round
   that resumes the chunk recreates it on a fresh branch). Confirm via
   `git worktree list` that the removed path was the intended one. The
   handoff contract is "PR open + checks green + merge state `CLEAN` +
   worktree removed". Reports a verdict. Never merges; never touches
   `main`. (`main` is branch-protected — merges happen on GitHub by
   the user.)

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

## 2. Chunk 1 — F1 author-tagged clocks (#684) (`feat/f1-author-clocks`)

- **Covers**: design §3 Part A (D-BF1–D-BF2). The parallel author
  structs (`TaskAuthors`, `StackAuthors`, `LabelAuthors`,
  `BoardAuthors` — one `Option<AuthorId>` slot per clock, mirroring
  the clocks' shape; the existing match arms keep compiling); the
  planner stamps `Some(device_id)` when a device identity exists and
  `None` in headless tests; the duel order `(clock, author)` lands in
  the entity merges (`merge_task` + siblings) with the entity id as
  the never-reached residual; the clock **text forms extend
  additively** (an optional author suffix; tolerant parse — old rows
  decode authorless) + **migration 0023** (the `au_` columns the codec
  backfills as absent); the sidecar rows gain an optional `by`
  (tolerant, default-absent; old devices relay verbatim — the
  mixed-fleet pin); the **equal-clocks-different-authors convergence
  law** proptested. **ADR 0022** records the boards-over-files
  contract (partition, the chunk lifecycle to come, the F1 order, the
  widened vocabulary). **Non-goals**: boards/chunks/routing (chunks
  3–5), the settings/proofs slice (chunk 2 — parallel, do not touch
  its files), any Deck behavior change (authors ride neutrally).
- **Acceptance**: the convergence law is meaningful (two devices,
  equal stamps, different edits → one winner on every reader — a test
  that passes without the author leg is a finding); every clock decode
  site tolerant (old rows decode authorless — roundtrip-pinned); the
  Deck duel suite extended with author legs, behavior unchanged for
  authorless rows; ADR 0022 in house style; CHANGELOG entry; gates
  green.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the boards-over-files dispatch ("F1 author-tagged clocks", branch feat/f1-author-clocks) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the normative origin: .artifacts/plans/2026-11-07-boards-over-files-design-round-plan.md — §0 rulings (Q2 binding), §1 anchors, §3 Part A (D-BF1–D-BF2), §5 test hooks, §6 risks — plus issue #684 and the backbone report §7.3 (.artifacts/reports/2026-10-11-generic-file-sync-connector-protocol-options-report.md — F1's rationale: ties between devices are a convergence bug, not a fairness bug).
2. Read the seams you extend: crates/taskboard-domain/src/entities.rs (TaskClocks :385, StackClocks :426, LabelClocks :437, BoardClocks :525 — plain DateTime fields today), command.rs (the planner's stamping sites), merge.rs (the duel resolution sites), ids.rs (AuthorId — the sync-files core crate's export; decide the type's canonical home), persistence.rs + the storage codec's shared clock text forms (the decode::clock helper and the stored-families text-form vocabulary — your additive author suffix extends these), appdata.rs (the sidecar row codecs that gain optional `by`), the fold order already landed (crates/taskboard-sync-files/src/fold.rs — the None < Some author semantics your planner stamps feed), docs/adr/0021 (the house ADR style; you write 0022 — re-verify the next free number at your base).
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit: STOP and report instead of planning), then create the worktree with git worktree add -b feat/f1-author-clocks ../taskboard-WT-feat-f1-author-clocks off main.
4. Write the TDD-first plan at .artifacts/plans/<today>-f1-author-clocks-plan.md settling: the four author structs' exact shapes, the AuthorId canonical home and the app-layer device-identity handoff (how the planner receives the device_id — the config/state seam), the order change in every duel site, the text-form extension grammar (author suffix syntax + the tolerant parse table), migration 0023's column set, the sidecar `by` wire shape, the convergence proptest design, the ADR 0022 outline, the full test catalogue.
5. Commit as: docs(plan): the F1 author-tagged clocks. Do not implement; do not push.

Report: plan path, the stamping seam decision, the text-form grammar, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the boards-over-files dispatch ("F1 author-tagged clocks").

1. Read AGENTS.md, the design plan §3 Part A, the dispatch plan §2, and the stage-1 plan in ../taskboard-WT-feat-f1-author-clocks (branch feat/f1-author-clocks). Recreate the worktree on that branch if missing (if its PR already landed, create feat/f1-author-clocks-v2 instead — AGENTS §8).
2. Implement exactly per the plan, TDD (failing tests first), every commit standalone (rebase-merge hygiene). Hard rules: PARALLEL author structs — never repack the clock fields into tuples or a combined struct (the match-arm stability is the design); the planner never invents an author (None when no device identity — headless tests stay None); every clock decode site parses old forms authorless (roundtrip pins); the sidecar `by` is optional + default-absent and an old-reader simulation relays it verbatim (pinned); Deck lanes' behavior is unchanged for authorless rows (the extended duel suite proves it); typed outcomes, no text assertions, deterministic tests (AGENTS §5); SPDX MIT OR Apache-2.0 + #![forbid(unsafe_code)] on new files; migration number re-verified at base (0023 expected).
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. Bookkeeping: ADR 0022 (docs/adr/0022-*.org, house style — the boards-over-files contract); CHANGELOG Unreleased entry; do not close #684 (the PR body carries "Closes #684").
5. No push, no PR — shipping is stage 3's. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the boards-over-files dispatch (branch feat/f1-author-clocks).

1. Read AGENTS.md, the design plan §3 Part A + §5 (the code-quality mandate in dispatch §1 applies to you), and the stage-1 plan; review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-f1-author-clocks. Begin by rebasing onto origin/main per the shared execution model (drop already-merged duplicates; push --force-with-lease only if the rebase rewrote history).
2. Functional focus — this chunk's traps: (a) the convergence proptest is meaningful: two devices, EQUAL wall-clock stamps, different edits, both authors present — every reader must pick the same winner; if the test cannot fail without the author leg, it is a finding; (b) no clock-field repacking — the parallel-struct ruling is the design, a combined struct is a finding even if greener; (c) tolerance: old serialized rows (state snapshots, journal payloads, sqlite, sidecar documents) decode authorless everywhere — spot-check each decode site's roundtrip pin; (d) the sidecar `by` relay pin exists (an old-reader simulation); (e) the planner's author stamping is identity-driven, never time-or-mood-driven (None in headless tests stays None); (f) ADR 0022 states the partition/chunk/order/widening contract accurately and is numbered 0022 (re-verify at the post-rebase base).
3. Code-quality mandate: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation; (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch like any other finding.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh (title: feat(domain): the F1 author-tagged clocks; body "Closes #684"), then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until all checks are green. Then verify gh pr view --json mergeStateStatus reports CLEAN; BEHIND/DIRTY means rebase onto origin/main, push --force-with-lease, and let the checks re-run. main is protected: never merge; the user merges on GitHub.
7. Teardown: from the project root, git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-f1-author-clocks; confirm with git worktree list that the removed path was the intended one.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks confirmation, worktree-removed confirmation. Never merge; never touch main.
```

---

## 3. Chunk 2 — settings & proofs per-user sections (#685) (`feat/userdata-settings-proofs`)

- **Covers**: design §3 Part B (D-BF3, ruling Q3). The per-user root's
  reserved sections activate on the current-state documents:
  **`settings`** — the roaming subset of user preferences, with the
  roam-set classification decided at stage 1 against the D5 notes and
  **pinned as data** (a test asserts the classified list; machine
  paths, secrets, and keyring-adjacent state never roam); **`proofs`**
  — reminder delivery proofs as set-union rows keyed
  `(rule, occurrence)` (per-device facts union to "someone delivered/
  dismissed this"). The `UserDataReport` fold grows the two section
  arms; the reminder-state read side consumes foreign proofs as it
  consumes local ones. **Non-goals**: board machinery, F1's files
  (chunk 1 runs in parallel — do not touch entities.rs/command.rs
  clock territory), UI settings editors, proofs pruning (existing
  pruning rules untouched).
- **Acceptance**: section ingest journeys (typed outcomes); set-union
  proofs under duplicate + out-of-order delivery (idempotence pins);
  the roam-set classification pinned as data; tolerant codecs per the
  core crate's recipe (unknown rows → Verbatim); the two-engine
  journey extends with settings/proofs convergence; CHANGELOG entry;
  gates green.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the boards-over-files dispatch ("settings & proofs per-user sections", branch feat/userdata-settings-proofs) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the normative origins: the design plan §3 Part B (D-BF3) + §5; issue #685; the D5 notes (future-directions report §4 D5 — .artifacts/reports/2026-10-14-future-directions-analysis-report.md); the DR-8 round's Part F (the heartbeat/status context your sections join).
2. Read the seams: the core crate's document.rs (the reserved section names you activate; the tolerant codec recipe), fold.rs (the arms your rows join), the state crate's user-data lane (the UserDataReport fold arm you extend — the journal import's landed shape is your template), domain reminders.rs (delivery proofs' existing shape, the set-union design notes), app config.rs (the [preferences] family your roam-set classifies), the two-engine journal convergence journey (your test template).
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit: STOP and report), then create the worktree with git worktree add -b feat/userdata-settings-proofs ../taskboard-WT-feat-userdata-settings-proofs off main.
4. Write the TDD-first plan at .artifacts/plans/<today>-userdata-settings-proofs-plan.md settling: THE ROAM-SET (the classified list, each entry with its one-line rationale against the D5 notes — this is the chunk's one design afternoon; machine paths/secrets never roam), the settings row shape (keyed how; per-key clocks per the section recipe), the proofs row shape (the (rule, occurrence) key; the set-union semantics; how foreign proofs join the read side), the ingest arms, the test catalogue.
5. Commit as: docs(plan): the settings and proofs sections. Do not implement; do not push.

Report: plan path, THE ROAM-SET (the full classified list), the proofs read-side seam, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the boards-over-files dispatch ("settings & proofs per-user sections").

1. Read AGENTS.md, the design plan §3 Part B, the dispatch plan §3, and the stage-1 plan in ../taskboard-WT-feat-userdata-settings-proofs (branch feat/userdata-settings-proofs). Recreate the worktree on that branch if missing (landed PR ⇒ -v2 branch per AGENTS §8).
2. Implement exactly per the plan, TDD (failing tests first), every commit standalone. Hard rules: the roam-set is pinned as data (a test asserting the classified list exists — prose is not the pin); machine paths/secrets never enter a section (pinned); proofs are set-union (a re-delivered proof is a no-op, an out-of-order one converges — both pinned); unknown rows fold as Verbatim, never error; the fold arms follow the journal import's landed shape exactly; typed outcomes, no text assertions; deterministic tests; SPDX + forbid(unsafe) on new files.
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. Bookkeeping: CHANGELOG Unreleased entry; do not close #685 (the PR body carries "Closes #685").
5. No push, no PR.

Report: per-gate results, deviations and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the boards-over-files dispatch (branch feat/userdata-settings-proofs).

1. Read AGENTS.md, the design plan §3 Part B (the code-quality mandate in dispatch §1 applies), and the stage-1 plan; review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-userdata-settings-proofs. Begin by rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) the roam-set pin is a real assertion over a real list — a classification that lives only in a comment is a finding; (b) nothing device-specific (paths, secrets, keyring-adjacent, machine locale) rides the settings section — grep the classified values; (c) proofs set-union under duplicate and out-of-order delivery is pinned (these are the two failure modes the design exists for); (d) the ingest arms mirror the journal import's shape (no second fold style); (e) reserved-name tolerance: a document from a future version with unknown settings keys folds without error.
3. Code-quality mandate: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation; (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch like any other finding.
4. Re-run the full gate suite; any failure is a finding.
5. Fix every issue on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push, open the PR (title: feat(sync-files,state): the settings and proofs sections; body "Closes #685"), gh pr checks --watch to green; verify mergeStateStatus CLEAN (rebase + --force-with-lease if BEHIND/DIRTY). Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-userdata-settings-proofs; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green-checks confirmation, worktree-removed confirmation.
```

---

## 4. Chunk 3 — remote-identity widening & partition routing (#686) (`feat/remote-identity-widening`)

- **Covers**: design §3 Part C (D-BF4–D-BF5, ruling Q5). The `Remote*`
  id family widens to connector-tagged (`Deck(...)` | `Files(...)`);
  `SyncReport::Completed` carries backend-tagged snapshot payloads;
  the validator/binding machinery follows the tag (ADR 0010's
  promote-to-trait moment). **Files remote ids are the domain UUIDs**
  — no card-id bindings, no adoption ceremonies for files-boards; the
  Deck ceremony machinery stays Deck-tagged and untouched. Partition
  routing: a typed board→connector ownership map; a second claim on
  an owned board is a typed, loud refusal. **Non-goals**: files-board
  sync itself (chunk 5), the move ceremony (chunk 6), any Deck
  pipeline behavior change (regression-pinned).
- **Acceptance**: the second-claim refusal journey (typed, surfaced);
  a Deck board's full sync **golden-regression** vs the pre-widening
  pipeline (behavior byte-equivalent); exhaustive-match fallout
  handled everywhere (no wildcards, no `todo!()`); storage
  remote-identity columns/backfills per the widened types (migration
  per stage-1, re-verified number); CHANGELOG entry; gates green.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the boards-over-files dispatch ("remote-identity widening & partition routing", branch feat/remote-identity-widening) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunk 1 (feat/f1-author-clocks) landed on main (shared domain files); verify (git log origin/main). If in flight, STOP and report instead of planning.

1. Read AGENTS.md, then the normative origins: the design plan §3 Part C (D-BF4–D-BF5) + §5; issue #686; the backbone report §7.4 (the two integration choices — (b) "when committing" is the ruling); ADR 0010 (the connector vocabulary the widening promotes).
2. Read the seams: domain ids.rs (RemoteCardRef :246 + the Remote* family), remote.rs / the SyncReport payload types, state actor.rs (the report ingestion that discriminates backends), app connectors.rs/bootstrap.rs (the registry + assignment), storage (the remote-identity persisted shapes — bindings, remote_seen validators; what migrates), the Deck actor's construction sites (the Deck tag's only producers today).
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit: STOP and report), then create the worktree with git worktree add -b feat/remote-identity-widening ../taskboard-WT-feat-remote-identity-widening off main.
4. Write the TDD-first plan at .artifacts/plans/<today>-remote-identity-widening-plan.md settling: the tagged type shapes (enum vs generic — pick one, record why), the report payload generalization, the ownership map's home (config/registry/state — where the assignment lives and who enforces it), the second-claim refusal typing, the storage migration shape, the Deck regression-pinning strategy (goldens vs journeys), the test catalogue.
5. Commit as: docs(plan): the tagged remote identities and partition routing. Do not implement; do not push.

Report: plan path, the tagged-type decision, the ownership map's home, risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the boards-over-files dispatch ("remote-identity widening & partition routing").

1. Read AGENTS.md, the design plan §3 Part C, the dispatch plan §4, and the stage-1 plan in ../taskboard-WT-feat-remote-identity-widening (branch feat/remote-identity-widening). Recreate the worktree on that branch if missing (landed PR ⇒ -v2 per AGENTS §8).
2. Implement exactly per the plan, TDD (failing tests first), every commit standalone. Hard rules: the Deck pipeline's observable behavior is unchanged (the golden/journey regression pins exist and pass untouched); the widening is exhaustive — no wildcard arms, no todo!() anywhere the tagged type reaches; files remote ids ARE the domain UUIDs (any binding-table or adoption machinery reaching the Files tag is a finding); the second claim on an owned board is a typed refusal surfaced loudly (never an overwrite, never silent); storage migration number re-verified at base; typed outcomes, no text assertions; deterministic tests; SPDX on new files.
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. Bookkeeping: CHANGELOG Unreleased entry; do not close #686 (the PR body carries "Closes #686").
5. No push, no PR.

Report: per-gate results, deviations and why.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the boards-over-files dispatch (branch feat/remote-identity-widening).

1. Read AGENTS.md, the design plan §3 Part C (the code-quality mandate in dispatch §1 applies), and the stage-1 plan; review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-remote-identity-widening. Begin by rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) the Deck regression pins are untouched and still meaningful (a pin deleted or weakened to admit the refactor is a finding); (b) grep the diff for wildcards and todo!() on the tagged family's matches — exhaustive is the design; (c) the Files tag has no binding/adoption machinery attached (UUIDs are the identity — anything else is scope creep toward chunk 5); (d) the ownership refusal is typed and loud (a silent second-claim overwrite is the exact bug the map exists to prevent); (e) the storage migration roundtrips old rows into the widened shape losslessly.
3. Code-quality mandate: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation; (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch like any other finding.
4. Re-run the full gate suite; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR (title: feat(domain,sync): the tagged remote identities and partition routing; body "Closes #686"), checks green, mergeStateStatus CLEAN (rebase + --force-with-lease if needed). Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-remote-identity-widening; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green-checks confirmation, worktree-removed confirmation.
```

---

## 5. Chunk 4 — the board chunk & snapshot lifecycle (#687) (`feat/board-chunk-lifecycle`)

- **Covers**: design §3 Part D (D-BF6–D-BF7, ruling Q6). The core
  crate's board lifecycle: `taskboard/boards/<board-uuid>/` subtrees
  (own `identity.json`; per-device `info.json` with `last_seq`;
  immutable `ops/NNNN.json` chunks carrying **materialized entity
  states** — fields + clocks + authors per entity, never raw op
  references; `snapshot-NNN.json` as the owner's converged fold ≤ N);
  snapshots on boot + every N chunks (N a named constant); the owner
  deletes its own chunks ≤ a published snapshot's seq; **tombstones
  are forever**; late-join = snapshot floor + subsequent chunks;
  per-board lifecycles independent; the per-user root stays
  current-state — mixed kinds **across** subtrees, never within one.
  **Non-goals**: the transport cycle/routing (chunk 5), the ceremony
  (chunk 6), per-user section changes.
- **Acceptance**: the **snapshot-floor equivalence law** proptested
  (late-join fold == incremental fold over the same event set); GC
  never removes live data (pinned); the tombstone-resurrection
  scenario set (backbone §7.5: late-returning device, concurrent
  delete-vs-edit, folder fork); chunk codec fuzz (the parser trigger);
  the transport crates untouched except the core's exports; CHANGELOG
  entry; gates green.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of the boards-over-files dispatch ("board chunk & snapshot lifecycle", branch feat/board-chunk-lifecycle) in the taskboard repo at /home/viktoria/src/taskboard.

Preconditions: chunks 1 and 3 landed on main (authors + tagged identities — your payloads and keys consume both), and #632 (B-min hardening) landed (the status vocabulary your lifecycle surfaces join); verify each (git log origin/main). If any is in flight, STOP and report instead of planning.

1. Read AGENTS.md, then the normative origins: the design plan §3 Part D (D-BF6–D-BF7) + §5; issue #687; the backbone report §5.5 + §7.2/§7.5 (the layout, the GC rules, the worked deletion scenarios — your normative lineage); the DR-8 round's layout decision (the reserved kinds you activate).
2. Read the seams: the core crate (layout.rs — the reserved-kind taxonomy you now emit; document.rs — the codec recipe; fold.rs — the kind dispatch; heartbeat.rs — the last_seq growth; hash.rs), the transport crate's cycle.rs/etag.rs (the listing/conditional-PUT surface chunk 5 will drive — you export, it consumes), domain entities.rs (the materialized-state payload shape: fields + clocks + authors).
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit: STOP and report), then create the worktree with git worktree add -b feat/board-chunk-lifecycle ../taskboard-WT-feat-board-chunk-lifecycle off main.
4. Write the TDD-first plan at .artifacts/plans/<today>-board-chunk-lifecycle-plan.md settling: the exact tree layout + file codecs (chunk, snapshot, board identity, per-board info), the materialized-entity payload schema (per entity kind), the snapshot cadence constant + trigger points, the GC rules (own chunks ≤ published snapshot seq; tombstones never), the late-join floor algorithm, the mixed-kind tolerance rules, the fuzz target, the property catalogue (snapshot-floor equivalence, GC safety, resurrection scenarios), the test catalogue.
5. Commit as: docs(plan): the board chunk and snapshot lifecycle. Do not implement; do not push.

Report: plan path, the payload schema, the GC/snapshot constants, risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of the boards-over-files dispatch ("board chunk & snapshot lifecycle").

1. Read AGENTS.md, the design plan §3 Part D, the dispatch plan §5, and the stage-1 plan in ../taskboard-WT-feat-board-chunk-lifecycle (branch feat/board-chunk-lifecycle). Recreate the worktree on that branch if missing (landed PR ⇒ -v2 per AGENTS §8).
2. Implement exactly per the plan, TDD (failing tests first), every commit standalone. Hard rules: chunks are IMMUTABLE once published (any in-place rewrite path is a finding); payloads are materialized states with clocks AND authors (raw op references in a chunk are a finding — late joiners cannot resolve them); tombstones are never GC'd (the resurrection scenarios are pinned, not hoped); GC touches only the owner's own files below the published snapshot watermark; sequence numbers are GC watermarks only — never fold order (a fold that sorts by seq is the design error the backbone report names); the per-user root's current-state shape is untouched; the core crate stays IO-free; codec fuzz target included; typed outcomes, no text assertions; SPDX on new files.
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. Bookkeeping: CHANGELOG Unreleased entry; do not close #687 (the PR body carries "Closes #687").
5. No push, no PR.

Report: per-gate results, deviations and why.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of the boards-over-files dispatch (branch feat/board-chunk-lifecycle).

1. Read AGENTS.md, the design plan §3 Part D + the backbone report §7.5 (the code-quality mandate in dispatch §1 applies), and the stage-1 plan; review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-board-chunk-lifecycle. Begin by rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) the snapshot-floor equivalence property is a real proptest over generated event sets (fold(snapshot ⊕ tail) == fold(all chunks)) — if it cannot fail on a wrong floor, it is a finding; (b) immutability: grep for any rewrite/rename path over published ops files; (c) GC safety: the deletion predicate is owner-scoped AND watermark-bounded (either leg missing is a finding); tombstones survive every GC path; (d) the fold is clock/author-driven — any seq-ordering influence on merge outcomes is the named design error; (e) materialized payloads carry authors (chunk 1's stamp) — a chunk writer that drops the author leg silently is a finding; (f) mixed kinds: a reader that tries to fold a board subtree as current-state (or vice versa) is a typed rejection, never a misparse.
3. Code-quality mandate: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation; (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch like any other finding.
4. Re-run the full gate suite; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR (title: feat(sync-files): the board chunk and snapshot lifecycle; body "Closes #687"), checks green, mergeStateStatus CLEAN (rebase + --force-with-lease if needed). Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-board-chunk-lifecycle; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green-checks confirmation, worktree-removed confirmation.
```

---

## 6. Chunk 5 — boards over files (#688) (`feat/boards-over-files`)

- **Covers**: design §3 Part E (D-BF8–D-BF10). Boards riding the
  substrate: `plan_pushes` routes each board's ops to its owning
  connector (the chunk-3 ownership map); the files connector drains
  its boards' dirty entities into a chunk (materialized at drain, the
  `CardShape` precedent), reporting `PushOutcome::Applied` per op; the
  pull side folds peers' snapshots+chunks and reports
  `SyncReport::Completed` with the Files-tagged snapshot, folded by
  the **existing** per-field LWW pipeline (no forked merge path).
  Board-level sections (time entries, break rules, habit facts,
  reminder rules, dismissals) ride the board documents as sections;
  **app-only task fields are ordinary chunk fields** (one lane);
  **no sidecar, no anchor card, no `UpdateAppData` on files-boards**
  (the op routes as ordinary dirty-entity materialization — pinned).
  Standalone boards may opt into files roaming with no Deck.
  **Non-goals**: the move ceremony (chunk 6), Deck-board behavior
  (regression-pinned unchanged), org anything (fenced).
- **Acceptance**: two-engine board convergence over the fake transport
  (edits on both devices → one converged board); the Deck pipeline's
  regression pins still green; the **no-sidecar pin** (a files-board
  never creates an anchor card; `UpdateAppData` never enqueues for
  one); docker-tier + live Tier-3 legs ride the existing tiers;
  standalone-board adoption journey; CHANGELOG entry; gates green.

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of the boards-over-files dispatch ("boards over files", branch feat/boards-over-files) in the taskboard repo at /home/viktoria/src/taskboard.

Preconditions: chunks 3 and 4 landed on main (routing + lifecycle); verify (git log origin/main). If either is in flight, STOP and report instead of planning.

1. Read AGENTS.md, then the normative origins: the design plan §3 Part E (D-BF8–D-BF10) + §5 + §6 (this chunk is the wave's L — the risk notes name the split option); issue #688; the DR-8 round's Parts C–E (the cycle you extend); the characterization report (the transport facts your board cycle inherits).
2. Read the seams: the outbox + plan_pushes/push.rs (the routing you extend by assignment; the CardShape materialization precedent), the transport crate's actor/cycle (the board cycle you add beside the per-user one), the core crate's chunk-4 exports (what you emit/consume), state actor.rs (the report ingestion for Files-tagged snapshots — the existing merge pipeline), appdata.rs (the sidecar machinery files-boards must NOT engage), connectors.rs/bootstrap.rs (the assignment wiring), the docker + fake-transport tiers.
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit: STOP and report), then create the worktree with git worktree add -b feat/boards-over-files ../taskboard-WT-feat-boards-over-files off main.
4. Write the TDD-first plan at .artifacts/plans/<today>-boards-over-files-plan.md settling: the drain path (dirty-entity set per board → chunk emission → PushOutcome reporting → outbox accounting), the pull path (peers' snapshots+chunks → the fold → the Files-tagged report), the board-section mapping (which sections ride, keyed how — mirroring the sidecar sections), the app-only-fields-as-chunk-fields shape, the no-sidecar routing rule (where UpdateAppData is suppressed for files-boards and what replaces it), the standalone-adoption flow, the two-engine journey design, the Deck regression pin strategy, the test catalogue. If the design's Part E splits naturally into two PRs (drain/report vs sections/adoption), record the split proposal — the user rules at review, not you.
5. Commit as: docs(plan): the boards over files. Do not implement; do not push.

Report: plan path, the drain/pull cycle shape, the no-sidecar routing rule, the split proposal if any, risks, estimated diff size.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of the boards-over-files dispatch ("boards over files").

1. Read AGENTS.md, the design plan §3 Part E, the dispatch plan §6, and the stage-1 plan in ../taskboard-WT-feat-boards-over-files (branch feat/boards-over-files). Recreate the worktree on that branch if missing (landed PR ⇒ -v2 per AGENTS §8).
2. Implement exactly per the plan, TDD (failing tests first), every commit standalone. Hard rules: the merge pipeline is REUSED, not forked (a second merge implementation for files-boards is a finding — Files-tagged payloads flow through the existing per-field LWW path); a files-board never creates an anchor card and UpdateAppData never enqueues for one (the pin test exists and is meaningful); the Deck pipeline's regression pins pass untouched; chunk emission honors chunk 4's immutability/materialization rules (no shortcuts under pressure); board sections ride the board documents (never the per-user root); standalone adoption adds no Deck dependency anywhere; deterministic tests (fake transport; paused time); docker tier behind its filter; typed outcomes, no text assertions; SPDX on new files.
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. Bookkeeping: CHANGELOG Unreleased entry; do not close #688 (the PR body carries "Closes #688").
5. No push, no PR.

Report: per-gate results, deviations and why.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 of the boards-over-files dispatch (branch feat/boards-over-files).

1. Read AGENTS.md, the design plan §3 Part E + §6 (the code-quality mandate in dispatch §1 applies), and the stage-1 plan; review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-boards-over-files. Begin by rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) exactly one merge pipeline: grep for any second duel/fold implementation gated on the Files tag — the existing pipeline with widened types is the design; (b) the no-sidecar pin: a files-board with an anchor card, or an UpdateAppData op enqueued for one, is a finding (and the pin must be able to fail); (c) the two-engine convergence journey is end-to-end through the fake transport — concurrent edits on both devices converge to one board state (with chunk 1's author legs visible in the winner choice); (d) the Deck regression pins are untouched and meaningful; (e) outbox accounting is honest (PushOutcome::Applied only for ops actually in a published chunk — a pre-emptive Applied is data loss under crash); (f) board sections never leak into the per-user root or vice versa.
3. Code-quality mandate: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation; (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch like any other finding.
4. Re-run the full gate suite; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR (title: feat(sync-files,state,app): the boards over files; body "Closes #688"), checks green, mergeStateStatus CLEAN (rebase + --force-with-lease if needed). Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-boards-over-files; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green-checks confirmation, worktree-removed confirmation.
```

---

## 7. Chunk 6 — the Deck→files move ceremony & mixed-fleet exit (#689) (`feat/deck-to-files-move`)

- **Covers**: design §3 Parts F–G (D-BF11–D-BF12, ruling Q4). The
  guided move verb (`boards move --to-files <board>`) — four typed,
  resumable steps: (1) final Deck pull + quiescent outboard for the
  board; (2) publish the board subtree (the mover's first snapshot =
  the converged state); (3) retire the Deck binding (loud, typed,
  undoable-before-commit); (4) assign `Files` in the ownership map.
  Other devices adopt fold-first on their next cycle; a device offline
  through the move finds the retired binding typed-and-loud on return
  and adopts. Reverse direction fenced. Mixed-fleet documentation;
  board-cycle status lanes on #632's findings vocabulary; the **wave
  exit closes #678** (and this dispatch's completion bookkeeping §8).
  **Non-goals**: files→Deck moves, org anything, attribution.
- **Acceptance**: the **dual-tier move-ceremony journey** (real Deck
  container + the docker files tier — the full pull→publish→retire→
  assign arc against both backends); the offline-during-move adoption
  journey; the undo-before-commit window pinned; partition invariants
  hold throughout the move; runbook docs; CHANGELOG entry; the PR body
  carries "Closes #689" and "Closes #678".

**Prompt 6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 6 of the boards-over-files dispatch ("Deck→files move ceremony & mixed-fleet exit", branch feat/deck-to-files-move) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunk 5 (feat/boards-over-files) landed on main; verify (git log origin/main). If in flight, STOP and report instead of planning.

1. Read AGENTS.md, then the normative origins: the design plan §3 Parts F–G (D-BF11–D-BF12) + §5; issue #689; the ruling Q4 (v1 ceremony, reverse fenced); the DR-8 runbooks (the device-retirement/fork procedures your ceremony docs extend); the connector ceremonies' house style (crates/taskboard-app/src/connector_ceremonies.rs).
2. Read the seams: the connector ceremony machinery (the guided-verb patterns), the chunk-3 ownership map (the assignment your ceremony writes; the refusal typing), the chunk-4 lifecycle (the first-snapshot publication), the Deck pull/unbind paths (the retirement step), the CLI verb registration, the status/findings vocabulary from #632, the docker Deck tier + files tier (your dual-tier journey).
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit: STOP and report), then create the worktree with git worktree add -b feat/deck-to-files-move ../taskboard-WT-feat-deck-to-files-move off main.
4. Write the TDD-first plan at .artifacts/plans/<today>-deck-to-files-move-plan.md settling: the verb surface + flags, the four steps' typed states + the resumable state machine (crash between steps is resumable, never corrupting), the undo window's exact boundary, the retirement typing (what a returning offline device sees and how adoption runs), the dual-tier journey design, the runbook docs outline, the wave-exit bookkeeping this chunk owns.
5. Commit as: docs(plan): the Deck-to-files move ceremony. Do not implement; do not push.

Report: plan path, the ceremony state machine, the undo boundary, risks, estimated diff size.
```

**Prompt 6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 6 of the boards-over-files dispatch ("Deck→files move ceremony & mixed-fleet exit").

1. Read AGENTS.md, the design plan §3 Parts F–G, the dispatch plan §7, and the stage-1 plan in ../taskboard-WT-feat-deck-to-files-move (branch feat/deck-to-files-move). Recreate the worktree on that branch if missing (landed PR ⇒ -v2 per AGENTS §8).
2. Implement exactly per the plan, TDD (failing tests first), every commit standalone. Hard rules: every ceremony step is typed + resumable (a crash between steps leaves a diagnosable state, never a half-moved board); the undo window closes at the retirement commit, never after; the retired Deck binding is typed-and-loud on the returning device (a silent adoption or a silent ignore are both findings); adoption is fold-first (the journal precedent — publish-after-import ordering holds); the reverse direction is a typed refusal (fenced, not missing); the dual-tier journey runs against the real Deck container + the files tier; partition invariants hold at every step (the ownership map never shows two owners, even transiently — pin it); typed outcomes, no text assertions; SPDX on new files.
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. Bookkeeping: CHANGELOG Unreleased entry; the mixed-fleet runbook docs; the PR body carries "Closes #689" and "Closes #678".
5. No push, no PR.

Report: per-gate results, deviations and why.
```

**Prompt 6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 6 of the boards-over-files dispatch (branch feat/deck-to-files-move).

1. Read AGENTS.md, the design plan §3 Parts F–G (the code-quality mandate in dispatch §1 applies), and the stage-1 plan; review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-deck-to-files-move. Begin by rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) the ceremony state machine: enumerate every inter-step crash point in the tests — each must resume to a correct terminal state; (b) the ownership map never shows two owners even transiently during the move (the pin exists and would fail on a swap-order bug); (c) the dual-tier journey is genuinely dual (a Deck-tier mock standing in for the files tier is a finding); (d) the returning-offline device path is typed-and-loud then fold-first — check the ordering pin; (e) the reverse direction is a typed refusal with the fence cited (a 404-shaped missing feature is a finding); (f) the runbook docs match the shipped states exactly (docs describing steps the code does not have are findings).
3. Code-quality mandate: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation; (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch like any other finding.
4. Re-run the full gate suite; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR (title: feat(app): the Deck-to-files move ceremony and mixed-fleet exit; body "Closes #689. Closes #678."), checks green, mergeStateStatus CLEAN (rebase + --force-with-lease if needed). Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-deck-to-files-move; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green-checks confirmation, worktree-removed confirmation.
```

---

## 8. Completion bookkeeping

- **Issues closed by chunks**: 1 → #684; 2 → #685; 3 → #686; 4 → #687;
  5 → #688; 6 → #689 **and #678** (the round completes with its last
  chunk — the wave exit).
- **ADR**: 0022 with chunk 1; none elsewhere.
- **Docs**: per-chunk CHANGELOG only; chunk 6 owns the mixed-fleet
  runbook + the wave-exit report (`.artifacts/reports/`) — the exit
  records the partition model's operational story and the fence
  hand-offs (org pass → #184's far-future home, attribution → #494).
- **Deliberately not dispatched here**: #632 (already fully prompted
  in the DR-8 dispatch plan §5 — dispatch it first); the org pass
  (ruling Q1's far-future separate connection); files→Deck moves;
  per-user attribution (#494); notify_push; the LAN/folder-client
  transports.
- **What opens next**: #632 (any time — chunks 1–2 may run alongside
  it) → 1 ∥ 2 → 3 → 4 → 5 → 6. When chunk 6 merges, the handoff to
  the user is: B-full live end to end — trigger the wave-exit
  bookkeeping (already chunk 6's) and the milestone completion ruling;
  the substrate track's remaining fenced successors (org pass,
  transports, attribution) stay in their issues.

# [Plan] Connector Attach Implementation — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-12
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the three implementation chunks of the
  approved connector-attach design round (WI-J follow-ups). No code in
  this commit.
- **Origin**: `.artifacts/plans/2026-10-12-connect-flow-design-plan.md`
  §12 (direction approved 2026-10-12 across three review rounds;
  branch rebased onto `main` @ `bdd9d97` post-WI-H) and the user's
  chunking decision (3 dispatches, not one mega-branch).
- **Status**: READY — chunks 1 and 2 are **order-independent** (both
  branch off `main`, disjoint files); dispatch them serially or in
  parallel. Chunk 3 dispatches only after **both** have merged. The
  daemon-side attach IPC verb and the multi-connector lift (design
  §4.5 deferred list) are deliberately **not** dispatched here.

## 1. Chunks & ordering

| Chunk | Design slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | WI-J.1 | `feat/connect-engine-attach` | engine additive rewire (registry + fan-out + ack), `App::attach_connector`, fan-in reports inbox, `Mutex<Vec>` widening, shutdown drop-ordering step | M |
| 2 | WI-J.2 | `feat/app-connect-verb` | `connect` verb + `login` deprecated alias, ceremony extraction, D2/D3 guards + confirms, lanes, E2E | M |
| 3 | WI-J.3 | `feat/app-connect-session` | REPL session ceremonies (connect/login/logout incl. §4.6 in-session logout), disposition protocol, config refresh, completion, ADR 0014 + docs + backlog ✅ | M/L |

**Dependency graph**: chunk 1 (engine + bootstrap) and chunk 2
(one-shot CLI, pre-core by design) share no files of consequence —
either order, or parallel with trivial rebases. Chunk 3 requires both
(it calls `attach_connector` from the session loop and the extracted
ceremony functions). Default: serial 1 → 2 → 3, every branch off the
then-current `main`, no PR ever stacked (no phantom-conflict rebase
dance). If the user stacks anyway, every branch targets `main`, each
stage-3 agent expects the §8 rebase dance after an upstream merge
(`git rebase --onto origin/main <landed-tip> <branch>` +
`--force-with-lease`; see the 2026-10-12 backlog-batch dispatch plan
§2 for the full procedure).

**Shared execution model** (same as the backlog-batch dispatch plan
§2, binding for every chunk):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` next to the project root (AGENTS §8) on
   a new branch off the current `main`, writes the TDD-first
   implementation plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push. The
   design round's §3/§4/§5/§7/§8 sections are normative input, not
   suggestions — deviations need a recorded reason.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing), implements per the
   approved plan, every commit standalone (rebase-merge hygiene),
   runs the full gate suite (AGENTS §10: fmt, clippy `-D warnings`,
   nextest, doc-tests, deny, llvm-cov) and makes it green. Does
   **not** push or open a PR. Reports gate results and deviations.
3. **Stage 3 — review agent**: reviews the entire branch diff versus
   its base against the chunk's acceptance criteria + AGENTS rules
   **plus the code-quality mandate below**, re-runs the gates,
   implements fixes for every issue found on the branch (amend/reword,
   never duplicate-subject fixups). Its last task is shipping: push
   the branch, open the PR to `main` with `gh`, watch
   `gh pr checks --watch` to completion — any failing check is a
   finding; diagnose, fix, push `--force-with-lease`, repeat until
   green. Reports a verdict. Never merges; never touches `main`.

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

## 2. Chunk 1 — Engine topology + attach primitive (`feat/connect-engine-attach`)

- **Covers**: design round §4.2 (normative) —
  `taskboard-state/src/actor.rs`: `EngineCommand::AttachConnector
  { sync_out, reply }` (push, never replace), the nudge seam becomes
  `Vec<mpsc::Sender<SyncCommand>>` with fan-out in
  `forward_sync_now` (forwarded iff **any** send returns `Ok`/`Full`;
  `Closed` entries pruned lazily — covering both dead paths: the
  standalone boot's dropped receiver and a detached actor's dropped
  receiver), `EngineHandle::attach_connector` + receipt types.
  `taskboard-app/src/bootstrap.rs`: `sync:
  Mutex<Option<SyncActor>>` (WI-H, merged) widens to `connectors:
  Mutex<Vec<SyncActor>>` (mutex never held across `.await` — the
  WI-H discipline), the **permanent fan-in reports inbox** created
  at boot with its twin sender parked in `App`, the shutdown
  **drop-connector-halves-before-engine-join** step (design fact 14:
  the linger-drain gates on the reports channel closing),
  `App::attach_connector(&self, client)` mirroring the merged
  `detach_connector(&self)` (typed `AlreadyAttached` guard = the
  app-layer count-≤1 invariant; re-attach after detach permitted),
  `sync_commands()`/`connector_attached()`/`shutdown()` carried over.
  **No CLI surface, no domain diff** (`SyncCommand::Detach` is
  already merged), no sync-crate changes beyond receiving its
  reports-sender clone.
- **Acceptance**: standalone boot behavior byte-identical (nudge
  fails closed, no `Syncing` transient — pinned); attach → nudges
  forward and a report sent on a clone of the parked sender is
  ingested; dead entries prune on the next fan-out; `AlreadyAttached`
  refused typed; detach → re-attach works; standalone `App::shutdown`
  completes with `actor_aborted == false` (the ordering contract —
  a deliberately-kept sender variant documents the hang it
  prevents); the pre-existing engine/bootstrap/WI-H suites pass
  unchanged; gates green.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the connector-attach implementation ("Engine topology + attach primitive", branch feat/connect-engine-attach) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the approved design round .artifacts/plans/2026-10-12-connect-flow-design-plan.md (§4.2 is normative: the additive fan-out/fan-in topology, item by item; §1 facts 4/6/14/15 are the load-bearing invariants; §4.5's "fixed now" list bounds the scope; §12 chunk WI-J.1) and the dispatch plan .artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md §1/§2.
2. Read the current contracts you will extend: crates/taskboard-state/src/actor.rs (the select loop, EngineCommand, forward_sync_now, the linger-drain gate), crates/taskboard-app/src/bootstrap.rs (post-WIH Mutex<Option<SyncActor>>, detach_connector, shutdown ordering), crates/taskboard-sync-nextcloud/src/actor.rs (spawn_sync_actor signature, the Detach arm), plus the WI-H tests you must not break (crates/taskboard-state/tests/engine.rs, crates/taskboard-app/tests/bootstrap.rs).
3. Create the worktree ../taskboard-WT-feat-connect-engine-attach on a new branch feat/connect-engine-attach off main.
4. Write a detailed TDD-first implementation plan at .artifacts/plans/<today>-connect-engine-attach-plan.md in the house [Plan] style: the exact EngineCommand::AttachConnector shape and ack semantics (FIFO — the Flush durability story); the registry's initialization (the standalone boot's dead entry) and the fan-out/prune function decomposition; the permanent reports inbox (creation, parking, per-attach clones) and the exact shutdown drop-ordering step; the attach_connector body as orchestration over named steps; the full test catalogue from the chunk's acceptance list (byte-identical standalone semantics, attach/nudge/report, pruning over both dead paths, AlreadyAttached, detach→re-attach, the shutdown ordering contract) — deterministic by construction, fakes per AGENTS §5. Resolve every open question with a recorded decision; deviations from the design round need a recorded reason.
5. Commit it as: docs(plan): connector attach engine topology and attach primitive. Do not implement; do not push.

Report: plan path, decisions, risks (esp. the drain-ordering interaction), estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the connector-attach implementation ("Engine topology + attach primitive").

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-12-connect-flow-design-plan.md (§4.2, §4.5, §1 facts 4/6/14/15), the dispatch plan .artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md §1/§2, and the stage-1 chunk plan (.artifacts/plans/*connect-engine-attach*-plan.md) in the worktree ../taskboard-WT-feat-connect-engine-attach (branch feat/connect-engine-attach). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone (rebase-merge hygiene). The state crate gains the command arm + fan-out seam only — no connector knowledge, no CLI concepts leak into it; the app crate gains the attach primitive. New .rs files carry the correct SPDX header (taskboard-state: MIT OR Apache-2.0; taskboard-app: AGPL-3.0-only); #![forbid(unsafe_code)] holds. No time manipulation beyond injected seams; no sleeps in tests.
3. Run the full gate suite from AGENTS.md §10 and make it green (cargo llvm-cov included).
4. Do NOT touch the backlog origin entries — chunk 3 lands them. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the connector-attach implementation (branch feat/connect-engine-attach).

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-12-connect-flow-design-plan.md (§4.2, §4.5, §1 facts 4/6/14/15), the dispatch plan .artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md §1/§2 (including the code-quality mandate — it applies to you), and the stage-1 chunk plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-connect-engine-attach.
2. Functional focus: the engine stays connector-count-agnostic (grep for any single-connector assumption — an Option/the-article slip is a finding); standalone boot semantics are byte-identical (no Syncing transient from a parked-open command receiver — fact 4 is the trap); AttachConnector pushes and acks, never replaces; fan-out counts Ok AND Full as forwarded and prunes Closed lazily over both dead paths (standalone boot, detached actor); the shutdown ordering contract is pinned by a test (connector halves dropped before the engine join; actor_aborted == false on a standalone shutdown); detach→re-attach permitted, double-attach refused typed; the pre-existing engine/bootstrap/WI-H suites are unchanged (diff every touched test — environment/speed gates only, never weakened assertions).
3. Code-quality mandate: evaluate (a) code quality — naming, comment discipline (constraints not narration), no speculative generality (the Vec is multi-connector readiness the design mandates, not gold-plating — but nothing beyond §4.5's "fixed now" list), test quality per AGENTS §5 (typed outcomes, no text assertions); (b) the Single Responsibility Principle — the engine orchestrates channel topology without legislating connector policy, the App owns lifecycle bookkeeping only (the mutex never crosses an await), the sync actor is untouched except for receiving its sender clone, no crate boundary crossed backwards; (c) single level of abstraction — every function body either orchestrates named operations or works in primitives, never mixes; attach_connector reads as ceremony over named steps; mixed-level bodies are findings even when tests pass.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword per commit hygiene — no duplicate-subject fixups).
6. Last task — ship: push the branch, open the PR to main with gh, then watch the CI jobs to completion (gh pr checks --watch); any failing check is a finding — diagnose, fix on the branch, push --force-with-lease, repeat until all checks are green.

Report: verdict, findings (functional + quality mandate), what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

## 3. Chunk 2 — `connect` verb + `login` alias (`feat/app-connect-verb`)

- **Covers**: design round §3.1/§3.2/§7 (normative) —
  `cli.rs`: `Command::Connect { server, user, switch_backend,
  force }`; `Login` becomes the **deprecated alias** (same ceremony
  under the old name, `switch_backend = false, force = false`
  always, one-line stderr deprecation notice; stdout/JSON contract
  untouched). NEW `commands/connect.rs`: the frontend-agnostic
  ceremony (gather via flags or `PromptReader` → D2/D3 guards →
  verify via `DeckClient::boards()` → persist the `toml_edit`
  overlay, incl. the `sync.backend = "nextcloud"` rewrite iff
  switched → store the secret; a confirmed/forced re-point deletes
  the old identity's secret after the new verify), the typed
  refusals (`backend-explicit-standalone`, `connection-exists`),
  `ConnectReceipt` (extends `LoginReceipt` with `backend_switched`,
  `repointed`) + renders. `commands/login.rs`: the
  verify→persist→store tail moves into `connect.rs`
  behavior-preserving; `PromptReader` gains `read_confirm` (y/N).
  `commands/mod.rs`: one-shot dispatch arms + `route_for_ipc` rows
  (`connect` Local; `login` keeps Local). Interactivity follows the
  `StdinPrompts` TTY detection; `--json` on a non-TTY never blocks
  on a confirm. E2E via the existing pipe-stdin harness (L-series
  pattern). The connect-while-daemon liveness note/warning is the
  stage-1 agent's recorded UX decision (design §6).
- **Acceptance**: all D2/D3 guard modes pinned by scripted
  `PromptReader` fakes (flag → proceed; TTY confirm-yes/decline;
  no-TTY-no-flag → typed refusal; re-point confirm/`--force`/decline
  with old-secret deletion); same-identity re-verify idempotent, no
  prompt; alias routes to the same body with both flags false and
  the notice on stderr (no text assertions anywhere); structural
  assertions on the written TOML; lanes pinned; hermetic + E2E
  green; gates green. Origin backlog entries stay open (chunk 3).

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the connector-attach implementation ("connect verb + login alias", branch feat/app-connect-verb) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the approved design round .artifacts/plans/2026-10-12-connect-flow-design-plan.md (§3.1/§3.2 are normative: the verb shape, the D1 alias semantics, the D2 flag+confirm and D3 confirm+force guards, the multi-connector forward-compatibility note; §7 the ceremony seam; §12 chunk WI-J.2) and the dispatch plan .artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md §1/§3.
2. Read the current seams: crates/taskboard-app/src/{cli.rs, commands/login.rs, commands/logout.rs, commands/mod.rs (dispatch + route_for_ipc), config.rs (write_path, sync_backend)}, and one L-series login E2E test to mirror.
3. Create the worktree ../taskboard-WT-feat-app-connect-verb on a new branch feat/app-connect-verb off main (order-independent of chunk 1; rebase first if it already landed).
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-app-connect-verb-plan.md: the ceremony function decomposition (guards → verify → persist → store, one behavior one test home — the login tail is extracted, not duplicated); PromptReader::read_confirm and the TTY-detection rule (never blocks a piped/--json run); the exact clap shapes incl. the alias's narrower flag surface; the typed error kinds and the --json receipt additions; the config overlay mechanics (backend rewrite + re-point old-secret deletion, atomic write via write_path); the connect-while-daemon warning decision (design §6) recorded with reasons; the hermetic test matrix and the E2E journey. Resolve every open question with a recorded decision; deviations from the design round need a recorded reason.
5. Commit it as: docs(plan): connect verb and login alias. Do not implement; do not push.

Report: plan path, decisions (esp. the warning), risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the connector-attach implementation ("connect verb + login alias").

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-12-connect-flow-design-plan.md (§3.1, §3.2, §7), the dispatch plan .artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md §1/§3, and the stage-1 chunk plan (.artifacts/plans/*app-connect-verb*-plan.md) in the worktree ../taskboard-WT-feat-app-connect-verb (branch feat/app-connect-verb). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. Hermetic tests only (tempdirs + env + scripted PromptReader fakes; no network beyond the existing fake/E2E tiers, no TTY). New .rs files carry the AGPL-3.0-only SPDX header (app crate). No text assertions (AGENTS §5) — the deprecation notice and all guard guidance are asserted structurally or not at all.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Do NOT touch the backlog origin entries — chunk 3 lands them. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, any deviation and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the connector-attach implementation (branch feat/app-connect-verb).

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-12-connect-flow-design-plan.md (§3.1, §3.2, §7), the dispatch plan .artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md §1/§3 (including the code-quality mandate — it applies to you), and the stage-1 chunk plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-app-connect-verb.
2. Functional focus: verify-before-persist ordering holds in the extracted ceremony (grep for any persist path reachable without a verified client); every D2/D3 guard mode is pinned by a test with a scripted PromptReader (flag, confirm-yes, confirm-decline, no-TTY refusal; re-point confirm/force/decline + old-secret deletion after the new verify); a piped or --json invocation can never block on a prompt; the loudness rule — no code path rewrites sync.backend without the flag or a confirmed/forced re-point; the alias shares the ceremony body (one behavior, one test home — duplicated ceremony logic is a finding); route_for_ipc rows pinned; no text-based assertions anywhere; the E2E mirrors the L-series shape.
3. Code-quality mandate: evaluate (a) code quality — naming, comment discipline, no speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle — the CLI layer only parses/dispatches, the ceremony is frontend-agnostic (no stdout/stdin writes inside it — everything rides PromptReader and CommandIo), config persistence stays in the toml_edit overlay helpers, secrets stay behind CredentialStore; (c) single level of abstraction — every function body either orchestrates named operations or works in primitives, never mixes; the ceremony body reads as guard → verify → persist → store with detail in named helpers; mixed-level bodies are findings even when tests pass.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: push the branch, open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green.

Report: verdict, findings (functional + quality mandate), what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

## 4. Chunk 3 — REPL session ceremonies + ADR 0014 (`feat/app-connect-session`)

- **Covers**: design round §4.4/§4.6/§5/§6 (normative) —
  `repl.rs`: the session loop intercepts `Command::Connect`,
  `Command::Login` (alias + deprecation notice), and
  `Command::Logout` (**both** `LineRefusal::Login` and `Logout` die;
  `Daemon`/`ConfigCommand`/per-line `--config` stay); the session
  config refresh between lines; the §4.6 in-session logout ceremony
  (local teardown → `App::detach_connector` → config refresh; the
  round-3 scope addition per WI-H plan D7 — its tests prove the
  `sync-unavailable` rider applies in-session and a later `connect`
  line re-attaches). The **editor-thread disposition protocol**
  (§5.2: per-line `Next`/`Prompt{label, kind: Line|Masked|Confirm}`
  handshake; masked `rpassword` on the reader thread; prompted
  answers never enter history or completion; Ctrl-C cancels the
  ceremony typed) and `ReplPrompts` implementing `PromptReader`.
  `commands/logout.rs`: session-capable extraction (the one-shot
  wrapper + daemon nudge unchanged — WI-H's). `completion.rs`:
  `connect` verb/flag candidates. **Final commits**: formalize
  **ADR 0014** (`docs/adr/0014-connector-attach-session-login.org`,
  from the design round §9 draft), amend `docs/architecture.org`
  (comms-table rows, verb catalogue) and ADR 0011's refusal list
  note, mark both origin backlog entries ✅ DONE
  ("[2026-10-11] CLI commands to add a Nextcloud connection" and
  "[2026-10-11] `login` inside a running session (REPL / GUI)").
- **Acceptance**: hermetic session tests drive connect/login/logout
  lines through a scripted disposition peer (no TTY in CI); the full
  session journeys pin: connect attaches + refreshes the session
  config (connector preludes stop refusing), logout detaches +
  refreshes (a following `sync` line refuses typed
  `sync-unavailable`), re-connect re-attaches; a failed ceremony
  leaves the core untouched; the disposition protocol's unit tests
  pin that prompted reads bypass history; completion offers
  `connect` (+ flags) and never the refused surface; ADR 0014 +
  docs amendments + backlog ✅ land in the final commits; gates
  green.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the connector-attach implementation ("REPL session ceremonies + ADR 0014", branch feat/app-connect-session) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunks 1 (feat/connect-engine-attach) and 2 (feat/app-connect-verb) have merged into main. Verify with git log origin/main --oneline | head -30 before starting; if either is missing, STOP and report instead of planning.

1. Read AGENTS.md, then the approved design round .artifacts/plans/2026-10-12-connect-flow-design-plan.md (§4.4 session choreography, §4.6 in-session logout — the round-3 WI-H-D7 scope addition, §5 the disposition handshake, §6 GUI reuse constraints, §9 the ADR 0014 draft, §12 chunk WI-J.3) and the dispatch plan .artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md §1/§4.
2. Read the current seams on main: crates/taskboard-app/src/{repl.rs (the editor thread, session loop, LineRefusal, ReplSession), completion.rs, commands/logout.rs, commands/connect.rs (chunk 2's ceremony), bootstrap.rs (chunk 1's attach_connector)}, and the WI-H detach journeys you build on (crates/taskboard-app/tests/bootstrap.rs).
3. Create the worktree ../taskboard-WT-feat-app-connect-session on a new branch feat/app-connect-session off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-app-connect-session-plan.md: the disposition protocol's exact channel/state machine (who blocks on what, backpressure, Ctrl-C during a masked read, why the handshake is race-free where a poll-between-lines design is not); ReplPrompts over PromptReader incl. read_confirm; the session-loop interception order (parse-level vs session-layer, mirroring ADR 0011's routing rules) and the config-refresh step; the §4.6 logout ceremony and its rider/re-attach tests; the completion additions; the hermetic test design (scripted disposition peer — the terminal never appears); the ADR 0014 + architecture.org amendment wording and the backlog ✅ final commits. Resolve every open question with a recorded decision; deviations from the design round need a recorded reason.
5. Commit it as: docs(plan): connect session ceremonies, masked prompts and ADR 0014. Do not implement; do not push.

Report: plan path, decisions, risks (esp. the editor-thread handshake), estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the connector-attach implementation ("REPL session ceremonies + ADR 0014").

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-12-connect-flow-design-plan.md (§4.4, §4.6, §5, §6, §9), the dispatch plan .artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md §1/§4, and the stage-1 chunk plan (.artifacts/plans/*app-connect-session*-plan.md) in the worktree ../taskboard-WT-feat-app-connect-session (branch feat/app-connect-session). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. The async loop never touches stdin; all terminal I/O stays on the editor thread; secrets never enter history or completion; hermetic tests inject line events + a scripted disposition peer (no TTY, no network, no sleeps). New .rs files carry the AGPL-3.0-only SPDX header.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Final commits: formalize ADR 0014 from the design round §9 draft, amend docs/architecture.org (comms-table rows, verb catalogue) and note the ADR 0011 refusal-list change, mark both origin backlog entries ("[2026-10-11] CLI commands to add a Nextcloud connection", "[2026-10-11] `login` inside a running session (REPL / GUI)") ✅ DONE with one-line notes. The "[2026-10-12] Multiple active sync connectors" entry stays open.
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, any deviation and why.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the connector-attach implementation (branch feat/app-connect-session).

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-12-connect-flow-design-plan.md (§4.4, §4.6, §5, §6, §9), the dispatch plan .artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md §1/§4 (including the code-quality mandate — it applies to you), and the stage-1 chunk plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-app-connect-session.
2. Functional focus: no async runtime worker ever blocks on stdin (grep the loop for any direct terminal read); the disposition handshake is race-free by construction (the editor thread never re-arms readline before the loop's disposition — a poll-between-lines race is a design finding); prompted answers bypass add_history_entry and the completion helper (structurally pinned); the session journeys all pin terminal state (connect → attach + config refresh; logout → detach + sync-unavailable rider + refresh; re-connect → re-attach; failed ceremony → core untouched); no text-based assertions (AGENTS §5 — LineRefusal/notice text is UX); the refusal set shrank by exactly Login and Logout; completion never offers the refused surface; ADR 0014 matches the shipped code (the §4.5 count-agnostic invariant reads true against chunk 1's registry); both backlog entries ✅ in the final commits; the multi-connector entry untouched.
3. Code-quality mandate: evaluate (a) code quality — naming, comment discipline, no speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle — the editor thread only owns the terminal and reads (zero handler logic on it), the session loop owns routing and session mutation only, the ceremonies are frontend-agnostic functions, the REPL layer holds no business logic (UI-layer rule); (c) single level of abstraction — every function body either orchestrates named operations or works in primitives, never mixes; the session loop's connect/logout arms read as ceremony invocations with detail in named helpers; mixed-level bodies are findings even when tests pass.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: push the branch, open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until all checks are green.

Report: verdict, findings (functional + quality mandate), what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

## 5. Completion bookkeeping

- Chunk 3's final commits mark both origin backlog entries ✅ DONE
  and land ADR 0014 + the architecture.org/ADR 0011 amendments.
- The "[2026-10-12] Multiple active sync connectors" entry stays
  open — it is the future round that lifts the app-layer count-≤1
  guard and owns design §4.5's deferred list. Same for the
  daemon-side attach IPC verb (rides WI-H's additive-verb pattern;
  no envelope decision outstanding).
- The `login`-alias removal is a v0.1.0-release-prep call, not part
  of these chunks.
- After chunk 3 merges, the user may optionally prune/archive the
  completed backlog entries per the backlog's archive policy.

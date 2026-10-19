# [Plan] Kiosk Implementation Round — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-19
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the **phase-A** implementation chunks
  of the accepted kiosk wave. No code in this commit.
- **Origin**: the wave implementation plan
  `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
  (§2 design facts KDF-1…KDF-12, §3 chunks K1–K14, §4 gates,
  §5 sequencing & exit criteria, §6 completion bookkeeping — all
  normative) on the framing plan
  `.artifacts/plans/2026-10-19-kiosk-wave-design-plan.md` whose §4
  record is **ACCEPTED** (user rulings 2026-10-19: D-K1…D-K10 —
  D-K8 = option (a)); the design-of-record is
  `.artifacts/plans/2026-10-16-kiosk-daemon-gui-control-plan.md`
  (Option A + §6 Q1–Q7 rulings); the parallelism report
  `.artifacts/reports/2026-10-19-kiosk-daily-driver-parallelism-analysis-report.md`
  governs the dispatch posture (D-K10 accepted); plus the user's
  2026-10-19 dispatch request.
- **Status**: **READY — dispatchable now.** The phase-A round is
  **not** gated on the multi-board round (verified file-disjointness,
  parallelism report §2.3) — with exactly one sequencing rule: K8's
  dispatch window (§1, MD-K8). **Deliberately not dispatched here**:
  **K11** (dashboard implementation — gated on the K10 design
  round's acceptance + its HTTP-stack stack approval), **K13**
  (ambient view — gated on daily-driver chunk 10 by dependency),
  **K14** (the wave exit — serialised last, dispatched with the
  phase-C round after the spike), the on-device spike itself
  (user-executed, phase B), and the framing's already-applied
  tracker bookkeeping (done 2026-10-19 — nobody re-does it).

## 1. Chunks & ordering

| Chunk | Impl-plan slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | §3 K1 | `feat/app-gui-verb` | `Command::Gui` (explicit desktop-frontend verb, boots core + desktop window in-process regardless of `[app] mode`, typed `instance-locked` on a running db), handled before `route_for_ipc` (never IPC-relayed), completion entry, ADR 0009 amendment section, README/CHANGELOG; **annotates #237** | S |
| 2 | §3 K2 | `feat/ui-kiosk-profile` | `UiSettings.kiosk` (default `false`), mount-time fullscreen + decoration-suppression profile (verify the 1.18.1 API; fullscreen alone acceptable, caveat recorded in the stage-1 plan for ADR 0016), harness assertions both ways; **annotates #237** | S |
| 3 | §3 K3 | `feat/app-kiosk-lifecycle` | stacks on 1+2: `BareRoute::KioskAppliance` (TTY-independent), the shared daemon-tail extraction from `daemon_lifecycle`, `run_kiosk` (gate-before-mount, three-exit watcher, loud `KioskStart::Headless` fallback), mode gate refuses `Desktop` only, **ADR 0016**, architecture.org + flow-model rows; **annotates #237** | M |
| 4 | §3 K4 | `feat/packaging-desktop-file` | stacks on 1: `contrib/taskboard.desktop` (`Exec=taskboard gui`), staged in `scripts/package_release.py` (+ `--check`), README "Launching" table (post-`3ad16bb` semantics), CHANGELOG; **closes #237** | S |
| 5 | §3 K5 | `feat/perf-process-harnesses` | S1 RSS smoke harness (`VmRSS`, N=10 000 fixture, readiness-poll, ≤ 200 MB reference) + S3 cold-start stopwatch (spawn→first-frame, structured-field moment, ≤ 5 s reference) as `scripts/`, runbook entry, dev-machine numbers recorded; **closes #299 + #262** | S–M |
| 6 | §3 K6 | `feat/perf-refresh-draw-bench` | S2: extend `test_platform.rs` so the constructed `SoftwareRenderer` draws headlessly; refresh leg (ArcSwap load → reconcile → models::push) + draw leg timed separately at board scale; the API-blocker rule (headless-draw outcome = recorded evidence, complete either way); **closes #300** (or annotates with the evidence outcome) | M |
| 7 | §3 K7 | `feat/perf-mapping-bench` | S4 criterion bench over the pure mapping surface (`mapping.rs` DTO↔domain, `push.rs` `plan_pushes`/card shapes) at board scale, `hot_path` fixture conventions, Bencher-picked-up with zero CI edits; **closes #263** | S |
| 8 | §3 K8 | `feat/state-sync-metrics` | the dispatch window's chunk: per-board cumulative sync counters on `SyncStatus` (rides `SyncReport` ingestion publish points; `aggregate()` fold extended; one deliberate insta pass), `status` rendering (structural), handle-side atomic `EngineMetrics` (never in `AppState`); **closes #156 + #145** | S–M |
| 9 | §3 K9 | `feat/kiosk-deployment-profile` | stacks on 3 (+ after MB chunk 5 merges): `[kiosk]` config section (`fixed_board: Option<Uuid>`, typed resolution against live boards), `contrib/taskboard-kiosk.service` (system unit, `Restart=always`, config-file mode — never env-var), bring-up runbook **incl. the D250 i686 cross-build lane** (finding recorded on #233); **annotates #233**; #160 keeps its decision comment (sd_notify ruled at the spike) | S–M |
| 10 | §3 K10 | `design/kiosk-dashboard` | docs-only design round: HTTP stack (tiny_http vs hyper-server vs axum vs hand-rolled — the §1.7 dependency facts are the starting evidence) **+ stack approval is the user's**, bind/auth model (loopback default; LAN arm's token options), content scope, meta-refresh/no-JS, placement, standalone interaction, 2014-browser checklist; **annotates #234** | S–M |
| 12 | §3 K12 | `feat/app-file-credential-store` | no dependencies — dispatchable any time in phase A: third `CredentialStore` impl, **D-K8 ruled option (a)** (generated 0600 key file + AEAD envelope; passphrase/argon2 out of scope), `[nextcloud] credential_store = "file"`, typed failures, no silent fallback; any new crate needs stack approval at stage 1; **closes #159 + #126** | M |

**Dependency graph** (implementation plan §5, phase A): `1 ∥ 2 → 3
→ 4` (the appliance spine — 3 stacks on 1+2; 4 stacks on 1); `5 ∥
6 ∥ 7` (instrumentation, disjoint files, startable immediately,
even before 1); `8` in its **dispatch window** (MD-K8 below); `9`
after 3 **and** after the multi-board round's `feat/app-connector-
registry` chunk merges (same `config.rs` — rebase-scale otherwise);
`10` anytime, earliest is best (it gates K11); `12` anytime.
Default serial order for a single-dispatcher cadence:
`1 → 2 → 3 → 4 → 5 → 6 → 7 → 12 → 9`, with `10` running as a
docs-only session whenever desired and `8` held for its window.

**MD-K8 — the K8 dispatch window** (binding; parallelism report
§2.2): K8's emission site is `crates/taskboard-sync-nextcloud/src/
actor.rs` — rewritten by the multi-board round's chunk 4
(`feat/sync-connector-modules`, **not yet started** as of
2026-10-19) and by the daily-driver round's chunks 3–4 (gated on
multi-board completing). K8 dispatches either **before** MB chunk
4 starts, or **in the multi-board→daily-driver gap** (after the MB
round closes, before DD chunk 3 dispatches). Its stage-1 prompt
carries the window check inline; a failed window = STOP and report,
never proceed onto a contested base. Everything else in the round
is exempt — no other kiosk chunk touches that file.

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); independent chunks under the
parallel options may run concurrently as separate sessions; never
two agents share a worktree. **Concurrency cap (D-K10)**: at most
**two** kiosk implementation chunks in flight per host alongside
the multi-board round's own chunk — the worktree floor
(`scripts/worktree-doctor.sh`, nonzero = STOP) is the hard gate
per agent regardless. Every chunk works in
`../taskboard-WT-<branch>` next to the project root (AGENTS §8),
branched off the **then-current `main`** — within the round, only
chunk 3 stacks (on 1+2), so expect the post-merge rebase dance
there if 1/2 have not merged when 3's stage 3 ships; all other
chunks branch off main unstacked. Disk hygiene: any agent creating
or recreating a worktree runs `scripts/worktree-doctor.sh` from
the main checkout first; stage 3 removes the worktree as its final
handoff task.

**Shared execution model** (binding for every chunk):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current
   `main`, re-locates the chunk's named symbols at its own start
   (implementation plan §2: finding IDs and symbols are the stable
   keys; line numbers anywhere are plan-day references), writes
   the TDD-first plan at
   `.artifacts/plans/<today>-kiosk-<slug>-plan.md` (MD-6) in the
   house `[Plan]` style, resolving every open question with a
   recorded decision, commits it (`docs(plan): …`). No
   implementation, no push. The implementation plan's §2/§3/§4/§5/§6
   and the accepted design's §6 rulings each chunk names are
   normative input — deviations need a recorded reason; a deviation
   from an ACCEPTED ruling (the 2026-10-16 §6 record or the
   2026-10-19 D-K rulings) goes back to the user (MD-7).
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing), executes per the
   approved plan — TDD per `docs/testing_strategy.org`, every
   commit standalone (rebase-merge hygiene), the full AGENTS §10
   gate suite green. Does **not** push or open a PR. Reports
   deviations.
3. **Stage 3 — review agent**: **begins by rebasing onto `main`**
   — `git fetch origin --prune`, rebase the branch onto
   `origin/main` (AGENTS §8's rebase dance: drop already-merged
   duplicate commits; resolve only real conflicts), push
   `--force-with-lease` if the rebase rewrote history; review the
   entire branch diff versus the **post-rebase** base against the
   chunk's acceptance criteria + AGENTS rules **plus the mandate
   below**, re-run whatever gates apply, implement fixes for every
   finding on the branch (amend/reword, never duplicate-subject
   fixups). Then ship: push, open the PR to `main` with `gh`,
   watch `gh pr checks --watch` — any failing check is a finding;
   diagnose, fix, push `--force-with-lease`, repeat until green —
   then verify `gh pr view --json mergeStateStatus` reports
   `CLEAN` (`BEHIND`/`DIRTY` ⇒ rebase onto `origin/main`, push
   `--force-with-lease`, re-run; anything else ⇒ diagnose). **Then
   teardown**: `git worktree remove --force
   ../taskboard-WT-<branch>` from the project root. The handoff
   contract is **"PR open + checks green + merge state `CLEAN` +
   worktree removed"**. Reports a verdict. Never merges; never
   touches `main`.

**Code-quality mandate** (stage 3, chunks 1–9 and 12 —
implementation chunks; the house text): beyond functional review,
the stage-3 agent evaluates: (a) **code quality**: naming, comment
discipline (comments state constraints, not narration), idiomatic
use of the approved stack, no dead code or speculative generality,
test quality per AGENTS §5; (b) the **Single Responsibility
Principle**: every module, actor, function, and type has exactly
one reason to change; domain stays pure (no IO, no runtime), the
engine orchestrates without legislating, the connector actor only
syncs, the CLI only parses/dispatches, the UI only presents — the
crate-role boundaries of AGENTS §2 are the SRP line, and a
boundary violation is an SRP violation; (c) **a single level of
abstraction per function body**: a function either orchestrates
named operations or works in primitive data, never mixes. Findings
from the mandate are fixed on the branch like any other finding.

**Evidence & artefact mandate** (stage 3, chunk 10 — the design
chunk): every factual claim carries evidence (the command run +
observed outcome, or a verifiable source + what it states — the
§1.7 dependency facts are the audit's starting point, re-verified,
not trusted); recommendations are decision-grade (options,
trade-offs, exactly **one** recommendation each); repo hygiene
(scratch probes never land — evidence is excerpts inside the doc;
the PR diff is exactly the design doc); scope discipline
(out-of-scope ideas go to the doc's backlog-candidates section —
no autonomous issue creation; the only tracker edit is the one
#234 annotation, MD-4).

**Round-wide evidence clauses** (bind all eleven chunks): the PR
diff is exactly the chunk's artefacts — scratch code, probe
crates, and captured payloads never land; harness/bench numbers
are recorded in the chunk report as **dev-machine references**
(tier honesty — never silent skips, never CI gates); out-of-scope
ideas discovered mid-chunk go to the chunk's stage report for the
user to promote; the only tracker edits in the round are the §6
bookkeeping (MD-4).

**Recorded method decisions**:

- **MD-1 — no round gate**: the phase-A round is **not** gated on
  the multi-board round (parallelism report §2.3 — the appliance
  touches no domain contract and none of MB's rewritten files;
  the two rebase-scale adjacencies `cli.rs`/`config.rs` are
  handled by the stage-3 rebase and chunk 9's ordering rule
  respectively). Chunk 1/2/5/6/7/10/12 stage-1 prompts carry **no
  round-gate precondition**; they verify only their own
  predecessors (MD-9).
- **MD-2 — stacking policy**: no PR is ever stacked by default —
  each chunk branches off then-current `main` after its
  predecessors merged. **Exception**: chunk 3 *may* be implemented
  on top of unmerged 1+2 when parallel dispatch is used; its
  stage-3 prompt then carries the rebase dance **including
  dropping 1+2's already-merged duplicate commits** after they
  land (`git rebase --onto origin/main <tip-of-1-or-2-that-just-
  landed> feat/app-kiosk-lifecycle`), and its PR is opened only
  after 1+2 have merged. `--force-with-lease` always, never
  `--force`.
- **MD-3 — mandate split**: code-quality mandate for chunks 1–9
  and 12; evidence & artefact mandate for chunk 10; the round-wide
  evidence clauses for all eleven.
- **MD-4 — tracker-edit mechanics**: closures ride the PR body —
  stage 3 writes `Closes #N` per the implementation plan §6 table
  so GitHub closes on merge; annotations are exactly **one
  progress comment per mapped issue per chunk**, posted by stage 3
  when the PR opens. Nothing else: no label/milestone edits, no
  new issues, no reopening (#233's and #160's spike-side outcomes
  are the user's, not a chunk's).
- **MD-5 — tier & evidence honesty**: the harness chunks (5–7)
  record their dev-machine numbers in the stage report + PR body;
  they are references, never gates; chunk 6's headless-draw
  outcome (working or API-blocked) is recorded as evidence either
  way — an API-blocked draw leg is a **complete** outcome per
  F13-03's own framing, never a failure to hide.
- **MD-6 — stage-1 plan paths**:
  `.artifacts/plans/<today>-kiosk-<slug>-plan.md`, slugs
  `gui-verb`, `kiosk-window-profile`, `kiosk-appliance-lifecycle`,
  `packaging-launch-docs`, `process-smoke-harnesses`,
  `refresh-draw-bench`, `mapping-bench`, `sync-metrics`,
  `kiosk-deployment-profile`, `kiosk-dashboard-design`,
  `file-credential-store`.
- **MD-7 — normative rank**: binding, in order: the implementation
  plan §2 (KDF-1…KDF-12) / §3 chunk sections / §4 / §5 / §6, then
  the accepted design's §6 rulings (Q1–Q7), then the framing's §4
  D-K rulings, then each chunk's stage-1 plan. Recorded-reason
  deviations are allowed against prose; a deviation from an
  ACCEPTED ruling goes back to the user before it is implemented.
- **MD-8 — anchor discipline**: stage-1 agents re-locate every
  named symbol at their own start; symbols are the stable keys,
  line numbers are plan-day references. The `3ad16bb` commit
  changed the routing facts the 2026-10-16 wave-1 plan states —
  the implementation plan's KDF-1/2 supersede it; where the two
  disagree, KDF wins.
- **MD-9 — precondition mechanics**: each chunk's stage-1 prompt
  verifies its predecessors' branches are merged on `origin/main`
  (command inline). A failed precondition = STOP and report, never
  proceed on a stale base. Chunk 8's window check and chunk 9's
  MB-chunk-5 check ride the same mechanism.
- **MD-10 — dependency-approval gates**: chunk 10 names its HTTP
  stack **only as a recommendation** — no crate enters `Cargo.
  toml` in this round (K11 owns the pin, after the user's stack
  approval); chunk 12's stage-1 prompt carries the stack-approval
  gate inline for the AEAD crate it selects (a pure-Rust
  RustCrypto AEAD — `aes-gcm` or `chacha20poly1305` — plus a
  direct `getrandom`; if the user has not pre-approved, stage 1
  STOPs at the recommendation and asks). Nothing else new enters
  the tree.

Agent-session count: **33** (11 chunks × 3 stages), plus this
docs-only dispatch-plan chunk (the round's chunk 0), plus the
user's merges between chunks.

## 2. Chunk 1 — `taskboard gui` verb (`feat/app-gui-verb`)

- **Covers** (implementation plan §3 K1; the design §4.3/Q1/Q4;
  KDF-1): `cli.rs` `Command::Gui` + doc comment; `main.rs::run`
  handles `Gui` before `route_for_ipc`, calling
  `taskboard_app::ui::run_desktop(config)` after config load;
  `route_for_ipc` stays gui-blind (no arm, never relayed);
  completion entry if subcommands are enumerated; ADR 0009
  amendment section (the verb + the never-relayed rule); README +
  CHANGELOG Unreleased line. **Non-goals**: no kiosk routing (3),
  no window profile (2), no packaging (4).
- **Acceptance**: clap parse of `gui`; routing unit tests (the gui
  arm chosen regardless of TTY and of `[app] mode`; a negative
  asserting `route_for_ipc` has no gui lane); `run_desktop` stays
  thin glue (existing convention, exercised by the desktop path);
  gates green; annotation on #237.
- **Traps for stage 3**: a gui arm in `route_for_ipc` (the verb is
  a lifecycle arm — relaying it to a daemon would open the window
  in the daemon process); handling `Gui` after the IPC routing
  decision; a mode-gate touch (the gate is chunk 3's);
  `run_desktop` growing logic (glue stays glue).

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the kiosk implementation round ("taskboard gui verb", branch feat/app-gui-verb) in the taskboard repo at /home/viktoria/src/taskboard.

No round gate: this chunk has no predecessor (MD-1). Precondition: none beyond a clean start — verify origin/main is fetched.

1. Read AGENTS.md, then the implementation plan .artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md (§2 KDF-1/4/6, §3 K1, §4, §5, §6), the accepted design .artifacts/plans/2026-10-16-kiosk-daemon-gui-control-plan.md (§4 recommendation 1, §5 items 3–4, §6 Q1/Q4), and the dispatch plan .artifacts/plans/2026-10-19-kiosk-implementation-dispatch-plan.md §1/§2.
2. Read the current seams (re-locate every symbol yourself): crates/taskboard-app/src/cli.rs (the Command catalogue, doc-comment conventions), main.rs::run (the lifecycle-arm ordering before route_for_ipc), commands/mod.rs route_for_ipc (the IpcOnly/IpcOrLocal/Local-only table + its tests), ui.rs run_desktop, completion.rs (subcommand enumeration), tests that pin bare_invocation_route and the routing table. The 3ad16bb commit and ADR 0015's #236 amendment are the routing-truth context.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = STOP and report), then create the worktree ../taskboard-WT-feat-app-gui-verb on a new branch feat/app-gui-verb off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-kiosk-gui-verb-plan.md settling: the Command::Gui clap shape + doc comment; the exact insertion point in main.rs::run and its ordering rationale; the routing-test matrix (TTY × mode × gui); the ADR 0009 amendment text; the completion surface; the CHANGELOG line. Resolve every open question with a recorded decision; a deviation from an ACCEPTED ruling goes back to the user — STOP and report.
5. Commit it as: docs(plan): taskboard gui verb. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the kiosk implementation round ("taskboard gui verb").

1. Read AGENTS.md, the implementation plan (§2 KDF-1/4/6, §3 K1, §4), the design §4/§5/§6, the dispatch plan §1/§2, and the stage-1 plan (.artifacts/plans/*kiosk-gui-verb*-plan.md) in the worktree ../taskboard-WT-feat-app-gui-verb (branch feat/app-gui-verb). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone (rebase-merge hygiene). Hard rules: Gui is a lifecycle arm handled BEFORE route_for_ipc and never relayed; run_desktop stays thin glue; no kiosk routing, no mode-gate change, no packaging (other chunks'); typed outcomes, no text assertions; SPDX headers per crate.
3. Run the full gate suite from AGENTS.md §10 and make it green (the CI-identical three-command llvm-cov sequence included).
4. Bookkeeping: the CHANGELOG Unreleased line. Do NOT touch any tracker issue (MD-4). No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the kiosk implementation round (branch feat/app-gui-verb).

1. First, rebase onto main: in ../taskboard-WT-feat-app-gui-verb run git fetch origin --prune, then git rebase origin/main (drop already-merged duplicate commits; resolve only real conflicts), and git push --force-with-lease if the rebase rewrote history. Review the full branch diff (git diff origin/main...HEAD) against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 KDF-1/4/6, §3 K1), the design §4/§5/§6, the dispatch plan §1/§2 (the code-quality mandate applies in full), and the stage-1 plan.
3. Functional focus — this chunk's traps: a gui lane in route_for_ipc (a finding — the verb is never relayed); the lifecycle arm ordered after the IPC routing decision; the mode gate touched (chunk 3's); routing tests missing the TTY×mode matrix or the negative no-gui-lane assertion; run_desktop growing logic; text assertions.
4. Code-quality mandate: (a) naming/comment discipline/no speculative generality; (b) SRP — the app parses/dispatches, the ui presents; (c) one level of abstraction per body.
5. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
7. Ship: push the branch, open the PR to main with gh (no Closes line — the annotation is this chunk's only tracker action), then gh pr checks --watch; failing check = a finding — diagnose, fix, push --force-with-lease, repeat until green. Verify gh pr view --json mergeStateStatus reports CLEAN (BEHIND/DIRTY ⇒ rebase, push --force-with-lease, re-check). Post ONE annotation comment on #237 (the gui verb landed via this PR, with the PR URL) — MD-4. Never merge; never touch main.
8. Teardown: from the project root, git worktree remove --force ../taskboard-WT-feat-app-gui-verb.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation.
```

## 3. Chunk 2 — kiosk window profile (`feat/ui-kiosk-profile`)

- **Covers** (implementation plan §3 K2; Q5; KDF-5): `UiSettings`
  gains `kiosk: bool` (default `false`); the mount path applies
  fullscreen + decoration suppression when set; the ADR 0015
  harness path asserts the window property with `true` and its
  absence with `false`; token/theme lint gates unchanged.
  **Non-goals**: no consumer (3 passes the flag), no scale knob,
  no config key.
- **Acceptance**: flag defaults off — the desktop path
  byte-identical; the 1.18.1 API question (fullscreen +
  decorations) settled with recorded evidence, the caveat noted
  for ADR 0016 if fullscreen-only; harness assertions both ways;
  gates green; annotation on #237.
- **Traps**: a new `[ui]` config key (there is none — the profile
  derives from the mode, `[ui]` stays scale+keymap under
  `deny_unknown_fields`); decoration suppression guessed instead
  of API-verified; the desktop path changing when the flag is
  false; slint-side logic beyond the window-profile application.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the kiosk implementation round ("kiosk window profile", branch feat/ui-kiosk-profile) in the taskboard repo at /home/viktoria/src/taskboard.

No round gate (MD-1). Precondition: none.

1. Read AGENTS.md, the implementation plan (§2 KDF-5, §3 K2, §4–§6), the design (§3 Option A's window row, §6 Q5), the dispatch plan §1/§3, and ADR 0015 (the mount seam + the harness conventions).
2. Read the seams: crates/taskboard-ui-slint/src/lib.rs (mount, UiSettings, Theme.base-scale flow, the harness/testing-backend path), adapter.rs, the .slint window definition (app-window.slint), the crate's Cargo.toml (slint features; dev-only i-slint-backend-testing), the harness tests that mount headlessly. Verify the slint 1.18.1 API for fullscreen + window-decoration control IN THE CHECKED-OUT SOURCES (~/.cargo/registry/src or cargo doc) — record what exists; if decorations cannot be suppressed programmatically, fullscreen alone is the acceptable wave outcome and the caveat is noted for ADR 0016 (chunk 3's doc).
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-ui-kiosk-profile on a new branch feat/ui-kiosk-profile off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-kiosk-kiosk-window-profile-plan.md settling: the UiSettings field + default (flag off = desktop path byte-identical); the exact API calls and where they apply in the mount path; the harness assertion mechanics (property readable without a real display); the caveat text for chunk 3 if fullscreen-only. Record every decision; deviations from rulings go back to the user.
5. Commit as: docs(plan): kiosk window profile flag. Do not implement; do not push.

Report: plan path, decisions (esp. the API findings), risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the kiosk implementation round ("kiosk window profile").

1. Read AGENTS.md, the implementation plan (§2 KDF-5, §3 K2, §4), the design §6 Q5, the dispatch plan §1/§3, and the stage-1 plan in ../taskboard-WT-feat-ui-kiosk-profile (branch feat/ui-kiosk-profile). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, commits standalone. Hard rules: kiosk defaults false and the desktop path is byte-identical with it off; no new config key; the window property asserted in the harness path both ways; no slint-side logic beyond the profile application; SPDX headers (this crate is AGPL-3.0-only and NOT forbidden-unsafe-code).
3. Run the full gate suite (AGENTS §10) green, incl. the token/theme lint gates.
4. Bookkeeping: the CHANGELOG Unreleased line. No tracker edits (MD-4). No push, no PR.

Report: per-gate results, deviations.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the kiosk implementation round (branch feat/ui-kiosk-profile).

1. Rebase onto main first (fetch, rebase origin/main, push --force-with-lease if rewritten); review git diff origin/main...HEAD against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 KDF-5, §3 K2), the design §6 Q5, the dispatch plan §1/§3 (code-quality mandate in full), the stage-1 plan.
3. Functional focus: the flag defaults off and the desktop path is unchanged (a diff in the flag-off path is a finding); a new [ui] config key is a finding; the fullscreen/decoration API usage matches the stage-1 evidence (guessed APIs are a finding); harness assertions exist both ways; no consumer wiring (chunk 3's).
4. Code-quality mandate (naming/comments/no generality; SRP — presentation-only crate; one abstraction level).
5. Re-run the gates (AGENTS §10); failures are findings.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR (annotation on #237: the window profile landed, one note), checks to green, mergeStateStatus CLEAN (rebase+force-with-lease if needed). Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-ui-kiosk-profile.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 4. Chunk 3 — kiosk appliance lifecycle + ADR 0016 (`feat/app-kiosk-lifecycle`)

- **Covers** (implementation plan §3 K3; Q2/Q3; KDF-1/2/3/4; F16-26):
  `BareRoute::KioskAppliance` (TTY-independent; the existing
  kiosk→DaemonLifecycle test updated); the shared
  serve-drain-shutdown tail extracted from `daemon_lifecycle`
  (both paths call it, zero duplication); `run_kiosk`
  (bootstrap → IPC server → gate-before-mount → `UiSettings.
  kiosk = true` hand-over + forwarder + the three-exit watcher →
  event loop → drain → shutdown; else loud `error!` + the shared
  tail); the mode gate refuses `Desktop` only; **ADR 0016**
  ("Kiosk appliance mode": routing, gate + loud fallback with the
  panic evidence + the tolerated gate→mount race, close-exits,
  IPC on the seat, `daemon` stays headless, any chunk-2
  decoration caveat); architecture.org mode row + flow-model
  matrix row. **Non-goals**: no IPC verb change (KDF-6 — the
  fence stays unfired), no hardening beyond the documented race
  tolerance, no config section (9).
- **Acceptance**: route mapping tests (kiosk → appliance, both TTY
  values); the fallback asserted at the decision layer
  (`KioskStart`-shaped seam); the daemon path's existing tests
  green through the shared tail; gates green; annotation on
  #237. **Stacking note (MD-2)**: implement on 1+2's branch tips
  if they are unmerged; open the PR only after both merge, then
  run the full rebase dance dropping their duplicate commits.
- **Traps**: duplicated shutdown choreography (the tail helper is
  the point); mounting before gating (slint panics — KDF-2); a
  two-exit watcher (window close ∥ signal ∥ `server.stopped()`
  are three); the explicit `daemon` subcommand gaining a GUI path
  (it stays headless always); `route_for_ipc` or the IPC protocol
  touched; the gate→mount race "fixed" beyond the documented
  tolerance.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the kiosk implementation round ("kiosk appliance lifecycle + ADR 0016", branch feat/app-kiosk-lifecycle) in the taskboard repo at /home/viktoria/src/taskboard.

Preconditions (MD-9): chunks 1 (feat/app-gui-verb) and 2 (feat/ui-kiosk-profile) exist as branches; they may be merged OR still open (MD-2's parallel exception — record which state you found, plan accordingly).

1. Read AGENTS.md, the implementation plan (§2 KDF-1/2/3/4/6, §3 K3, §4–§6), the design (§3 Option A, §6 Q2/Q3 — ACCEPTED rulings), the dispatch plan §1/§4 (MD-2's stacking exception), ADR 0015 (incl. the #236 amendment), and the chunk-1/2 stage-1 plans (the verb arm and the profile flag you consume).
2. Read the seams (re-locate; the 3ad16bb and the 2026-10-18 engine-exit work reshaped these paths — trust symbols, not plan-day lines): crates/taskboard-app/src/ui.rs (bare_invocation_route + BareRoute + its tests, display_available + display_env_available, run_desktop's watcher/quit pattern, the UiSettings hand-over), main.rs (daemon_lifecycle's full shape incl. the select/join/drain tail and the second-signal force exit, the mode gate, the routing arm), ipc/server.rs (IpcServerHandle::stopped/join), the engine-exit typed-path tests, flow_model.org's mode matrix, architecture.org's mode statements.
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create the worktree: if 1+2 are merged, ../taskboard-WT-feat-app-kiosk-lifecycle on a new branch off main; if not, create it stacked on the later of 1/2's branch tips (branch from that tip, not main) — record the stacking in the plan.
4. Write a TDD-first plan at .artifacts/plans/<today>-kiosk-kiosk-appliance-lifecycle-plan.md settling at minimum: the tail-extraction signature (what daemon_lifecycle keeps vs delegates; the second-signal force exit rides the helper); KioskAppliance routing + the updated pinning test; run_kiosk's exact choreography (bootstrap → server → gate → hand-over/forwarder/watcher/event-loop → abort watcher+forwarder → join drain → shutdown; the headless fallback branch); the KioskStart decision seam shape for tests; the three-exit watcher's select set; the mode-gate narrowing; ADR 0016's full outline (routing, gate+fallback with the panic evidence, race tolerance, close-exits, IPC-on-seat, daemon-headless, any chunk-2 caveat); the docs rows. Resolve every question with a recorded decision; deviations from ACCEPTED rulings go back to the user.
5. Commit as: docs(plan): kiosk appliance lifecycle and ADR 0016. Do not implement; do not push.

Report: plan path, decisions, the 1+2 merge-state found, risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the kiosk implementation round ("kiosk appliance lifecycle + ADR 0016").

1. Read AGENTS.md, the implementation plan (§2 KDF-1/2/3/4/6, §3 K3, §4), the design §3/§6, the dispatch plan §1/§4 (MD-2), and the stage-1 plan in ../taskboard-WT-feat-app-kiosk-lifecycle (branch feat/app-kiosk-lifecycle — recreate it on the recorded base if missing).
2. Implement exactly per the approved plan, TDD, commits standalone. Hard rules: ONE extracted tail shared by daemon and appliance (no duplicated shutdown choreography); gate BEFORE mount (slint panics headless); the watcher selects THREE exits; the explicit daemon subcommand stays headless always; no IPC protocol/verb change; the gate→mount race stays tolerated (documented, not fixed); the fallback is loud (error!) and asserted at the decision layer; typed outcomes; no sleeps — join handles and the watcher seams; SPDX headers.
3. Run the full gate suite (AGENTS §10) green — the daemon path's existing tests must pass through the extracted tail unmodified.
4. Bookkeeping: the CHANGELOG Unreleased line. No tracker edits (MD-4). No push, no PR.

Report: per-gate results, deviations.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the kiosk implementation round (branch feat/app-kiosk-lifecycle).

1. Rebase onto main FIRST and fully (MD-2): git fetch origin --prune; if chunks 1+2 have merged, git rebase --onto origin/main <tip-of-the-last-of-1/2-that-landed> feat/app-kiosk-lifecycle (dropping their duplicate commits); else git rebase origin/main. Resolve nothing unless real conflicts. Push --force-with-lease. Open the PR ONLY if 1+2 are merged — otherwise STOP after the rebase and report the blocking state. Review git diff origin/main...HEAD.
2. Read AGENTS.md, the implementation plan (§2 KDF-1/2/3/4/6, §3 K3), the design §3/§6 rulings Q2/Q3, the dispatch plan §1/§4 (code-quality mandate in full), the stage-1 plan.
3. Functional focus — this chunk's traps: any second copy of the select/join/drain choreography (the shared tail is the deliverable); mount before gate; a two-exit watcher; a GUI path on the explicit daemon verb; an IPC protocol diff (the fence stays unfired — KDF-6); the race "fixed" beyond tolerance; the headless fallback silent instead of loud; route tests missing either TTY value; architecture.org/flow-model rows missing.
4. Code-quality mandate (naming/comments; SRP — the app owns lifecycles, the ui presents; one abstraction level; the extracted helper orchestrates, it does not re-implement).
5. Re-run the gates (AGENTS §10); failures are findings.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR (annotation on #237: the appliance lifecycle + ADR 0016 landed — the substantive kiosk chunk), checks to green, mergeStateStatus CLEAN (the rebase dance again if BEHIND). Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-app-kiosk-lifecycle.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 5. Chunk 4 — packaging & launch docs (`feat/packaging-desktop-file`)

- **Covers** (implementation plan §3 K4; the design §5 items 1–2;
  KDF-1): `contrib/taskboard.desktop` (`Exec=taskboard gui`,
  `Type=Application`, categories, theme-fallback icon reference);
  staged in `scripts/package_release.py` (`STAGED_FILES` + the
  `--check` list follows automatically); README.org "Launching"
  (the post-`3ad16bb` table: TTY → REPL; no-TTY+display → GUI;
  no-TTY headless → daemon; `mode = "kiosk"` → appliance; `gui` →
  desktop window; the env escape hatch; the `.desktop` install
  hint); CHANGELOG. **Non-goals**: no kiosk unit (9), no icon
  asset, no release-workflow edits beyond the staged list.
- **Acceptance**: the staged file appears in a `--check` run;
  README table matches the shipped routing exactly (verified
  against `bare_invocation_route`, not from memory); gates green;
  **closes #237** (the appliance is complete with 1–3 merged).
- **Traps**: a README table that documents the pre-`3ad16bb`
  routing (the display-aware cell is the one people get wrong);
  `--check` drift from `STAGED_FILES`; an env-var-based kiosk
  launch in any example (ADR 0015's #236 rule — config file, not
  env).

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of the kiosk implementation round ("packaging & launch docs", branch feat/packaging-desktop-file) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition (MD-9): chunk 1 (feat/app-gui-verb) merged on origin/main (git fetch origin --prune; verify its tip is contained). If not, STOP and report.

1. Read AGENTS.md, the implementation plan (§2 KDF-1, §3 K4, §4–§6), the design §5 (the P1 fix directions) + §6, the dispatch plan §1/§5.
2. Read the seams: scripts/package_release.py (STAGED_FILES, --check), contrib/ (the existing taskboard.service as the conventions template), README.org (the current launch statements — the #236 amendment's truths), CHANGELOG.md, and ui.rs bare_invocation_route as the routing truth the README table must match.
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-packaging-desktop-file on a new branch feat/packaging-desktop-file off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-kiosk-packaging-launch-docs-plan.md settling: the .desktop entry's exact keys (Exec, Type, Categories, Icon w/ theme fallback, TryExec if warranted); the staging diff + how --check follows; the README table's rows verified against the routing fn cell by cell; the CHANGELOG lines. Record every decision.
5. Commit as: docs(plan): desktop launcher and launch docs. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of the kiosk implementation round ("packaging & launch docs").

1. Read AGENTS.md, the implementation plan (§2 KDF-1, §3 K4), the design §5/§6, the dispatch plan §1/§5, and the stage-1 plan in ../taskboard-WT-feat-packaging-desktop-file (branch feat/packaging-desktop-file). Recreate the worktree if missing.
2. Implement exactly per the approved plan, commits standalone. Hard rules: Exec=taskboard gui (never an env-var mode); the README table matches bare_invocation_route exactly (verify per cell); --check stays consistent with STAGED_FILES; no kiosk unit here (chunk 9's).
3. Verify: run scripts/package_release.py --check (or its equivalent) and show it green with the staged launcher; re-verify each README row against the routing fn.
4. Bookkeeping: the CHANGELOG lines. No tracker edits (MD-4). No push, no PR.

Report: per-gate results, the packaging verification output, deviations.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of the kiosk implementation round (branch feat/packaging-desktop-file).

1. Rebase onto main first; review git diff origin/main...HEAD.
2. Read AGENTS.md, the implementation plan (§3 K4), the design §5/§6, the dispatch plan §1/§5 (code-quality mandate in full), the stage-1 plan.
3. Functional focus: the README table documenting stale routing (verify per cell against the fn — the #236 display-aware cell is the classic miss); STAGED_FILES/--check drift; an env-var launch example anywhere; Exec line wrong; missing CHANGELOG entries for verb+appliance+launcher.
4. Code-quality mandate (script hygiene; docs that state constraints not narration).
5. Re-run the gates (AGENTS §10) + the packaging verification yourself.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR (body carries "Closes #237" — the appliance milestone issue completes with this PR), checks to green, mergeStateStatus CLEAN. Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-packaging-desktop-file.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 6. Chunk 5 — process smoke harnesses: RSS + cold-start (`feat/perf-process-harnesses`)

- **Covers** (implementation plan §3 K5; audit S1/S3; KDF-7 rows
  1+4; MD-5): the S1 headless-daemon RSS smoke (spawn against the
  N=10 000 fixture DB, readiness-poll with a deadline — never a
  fixed sleep — read `VmRSS` from `/proc/<pid>/status`, report vs
  ≤ 200 MB) and the S3 cold-start stopwatch (spawn → first-frame
  moment via a structured signal/log field, ≤ 5 s reference) as
  `scripts/` following repo conventions; fixture reuse from the
  `hot_path` bench conventions; a `docs/testing_strategy.org`
  runbook entry; dev-machine numbers recorded. **Not** criterion
  benches, **not** CI gates. **Non-goals**: no UI-process variant
  (the spike extends on-device), no CI job, no budget assertion
  in tests (references, not gates).
- **Acceptance**: both harnesses run green on the dev machine and
  print their numbers (in the report + PR body); the fixture
  builds; the runbook entry documents invocation + reference
  semantics; gates green; **closes #299 + #262**.
- **Traps**: a criterion bench measuring its own process RSS (the
  S1 note forbids it); a fixed sleep where the readiness
  predicate belongs; parsing unstructured log text for the
  first-frame moment (a structured field, or an existing signal);
  silently skipping when the fixture is absent.

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of the kiosk implementation round ("process smoke harnesses: RSS + cold-start", branch feat/perf-process-harnesses) in the taskboard repo at /home/viktoria/src/taskboard.

No round gate (MD-1). Precondition: none.

1. Read AGENTS.md, the implementation plan (§2 KDF-7 rows 1+4, §3 K5, §4–§6), the audit report .artifacts/reports/2026-10-16-v0.1.0-audit-theme-perf-budgets-report.md (§2 budgets table rows 1+4, §3 sketches S1+S3 verbatim — they are the design), the dispatch plan §1/§6 (MD-5).
2. Read the seams: crates/taskboard-storage-sqlite/benches/{hot_path.rs, support/mod.rs} + tests/bench_fixtures.rs (the N=10 000 fixture conventions to reuse), scripts/ (existing harness conventions — bash/python pinning, argument style), the daemon lifecycle's readiness surface (the IPC socket binding, `status`), the mount path for the first-frame moment (what structured signal or log field can carry it — prefer an existing seam; a new structured tracing field is acceptable, parsed text is not), docs/testing_strategy.org (the runbook section conventions; the audit's P1-5 pattern).
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-perf-process-harnesses on a new branch feat/perf-process-harnesses off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-kiosk-process-smoke-harnesses-plan.md settling: each script's exact flow (spawn mechanics, fixture path, readiness predicate + deadline, VmRSS parse, first-frame moment mechanism, output format), the reference-vs-gate framing (nothing asserts ≤ budget in CI), the fixture build/refresh story, the runbook entry text. Record every decision.
5. Commit as: docs(plan): RSS and cold-start smoke harnesses. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of the kiosk implementation round ("process smoke harnesses: RSS + cold-start").

1. Read AGENTS.md, the implementation plan (§2 KDF-7, §3 K5), the audit's S1/S3 sketches, the dispatch plan §1/§6 (MD-5), and the stage-1 plan in ../taskboard-WT-feat-perf-process-harnesses (branch feat/perf-process-harnesses). Recreate the worktree if missing.
2. Implement exactly per the approved plan, commits standalone. Hard rules: readiness polls with deadlines, never fixed sleeps; the first-frame moment rides a structured seam (existing signal or a new structured field), never parsed prose; VmRSS from /proc/<pid>/status; no budget assertions anywhere in CI paths; scripts follow the repo's existing script conventions (pinned interpreter, --help); the fixture story is honest (built on demand, documented).
3. Run both harnesses; record the dev-machine numbers (they go in your report and the PR body — MD-5).
4. Run the full gate suite (AGENTS §10) green. Bookkeeping: the runbook entry. No tracker edits (MD-4). No push, no PR.

Report: per-gate results, BOTH harnesses' measured numbers explicitly, deviations.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 of the kiosk implementation round (branch feat/perf-process-harnesses).

1. Rebase onto main first; review git diff origin/main...HEAD.
2. Read AGENTS.md, the implementation plan (§3 K5), the audit's S1/S3 sketches, the dispatch plan §1/§6 (code-quality mandate in full), the stage-1 plan.
3. Functional focus: any criterion-bench RSS measurement (forbidden — S1's own note); a fixed sleep in the readiness path (finding); a text-parsed first-frame moment (finding); a CI budget assertion (references are never gates); missing measured numbers in the report (MD-5).
4. Code-quality mandate (script hygiene: pinned interpreter, argument validation, no dead flags; SRP — scripts orchestrate processes, they do not embed app logic).
5. Re-run the gates (AGENTS §10) and BOTH harnesses yourself; compare the numbers you observe with the reported ones.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR ("Closes #299" + "Closes #262"; the PR body carries the measured references), checks to green, mergeStateStatus CLEAN. Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-perf-process-harnesses.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, the numbers you reproduced, worktree removed.
```

## 7. Chunk 6 — refresh+draw bench (`feat/perf-refresh-draw-bench`)

- **Covers** (implementation plan §3 K6; audit S2; KDF-7 row 3;
  MD-5's evidence rule): extend `test_platform.rs`
  (`NullPlatform`/`NullWindowAdapter` — the renderer is
  constructed but never asked to draw today) so the mounted
  component renders a frame headlessly; time the refresh leg
  (ArcSwap load → `view_state.reconcile` → `models::push`
  rebuild) and the draw leg separately, at board-scale fixture N;
  the API-blocker rule: if slint 1.18.1 cannot draw headlessly,
  record the evidence and ship the refresh-leg bench + the seam
  extension — a complete outcome. **Non-goals**: no
  renderer/backend selection (the spike), no cadence change
  (F13-09 stands), no visual assertions (the golden lane owns
  correctness).
- **Acceptance**: refresh leg + draw leg measured independently
  (or the blocker evidenced + refresh alone); numbers recorded
  (report + PR body); the seam extension leaves the existing
  harness tests green; gates green; **closes #300** (or
  annotates with the evidence outcome — then the issue closes
  with the recorded evidence per F13-03's framing).
- **Traps**: the software renderer exercised through a GL path
  (or vice versa — the row-3 budget is software-renderer terms);
  refresh and draw timed as one number; the fixture at the wrong
  scale; a weakening of the "never asked to draw" tests without
  the recorded evidence.

**Prompt 6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 6 of the kiosk implementation round ("refresh+draw bench", branch feat/perf-refresh-draw-bench) in the taskboard repo at /home/viktoria/src/taskboard.

No round gate (MD-1). Precondition: none.

1. Read AGENTS.md, the implementation plan (§2 KDF-7 row 3, §3 K6, §4–§6), the audit's §2 row 3 + sketch S2 + the C2 note (the 100 ms cadence interaction), the dispatch plan §1/§7 (MD-5's evidence rule).
2. Read the seams: crates/taskboard-ui-slint/src/test_platform.rs (NullPlatform/NullWindowAdapter — renderer constructed, never drawn), the harness/mount path that uses it, adapter.rs (the refresh pass: ArcSwap load → reconcile → models::push), models.rs (the VecModel rebuild), the slint 1.18.1 software-renderer API in the checked-out sources (can i-slint-backend-testing or the software renderer render to a buffer without a windowing system? verify — this is the API-blocker question; record the answer with file paths as evidence), the existing harness tests that pin the "never asked to draw" behaviour, benches/ conventions in the workspace (criterion is workspace-pinned).
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-perf-refresh-draw-bench on a new branch feat/perf-refresh-draw-bench off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-kiosk-refresh-draw-bench-plan.md settling: the API verdict (draw-headlessly: yes/no, with evidence); the seam extension design (what NullWindowAdapter gains; which existing tests change and why that is honest); the bench form (criterion target vs harness script — decide per the crate's dev-deps); the refresh-leg instrumentation points; the draw-leg mechanism; the fixture scale; the two-lane timing separation; the blocker-fallback plan if the API verdict is no. Record every decision.
5. Commit as: docs(plan): refresh and draw benching seam. Do not implement; do not push.

Report: plan path, the API verdict + evidence, decisions, risks, estimated diff size.
```

**Prompt 6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 6 of the kiosk implementation round ("refresh+draw bench").

1. Read AGENTS.md, the implementation plan (§2 KDF-7 row 3, §3 K6), the audit's S2 sketch, the dispatch plan §1/§7, and the stage-1 plan in ../taskboard-WT-feat-perf-refresh-draw-bench (branch feat/perf-refresh-draw-bench). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, commits standalone. Hard rules: the row-3 budget is software-renderer terms — any GL/llvmpipe path is out; refresh and draw are timed separately; the seam extension keeps every existing harness test honest (changes are the stage-1 plan's, not opportunistic); the API-blocker fallback (refresh leg + evidence) is a complete outcome, never a silent cut; SPDX headers (AGPL crate, unsafe permitted here only).
3. Run the bench/harness; record BOTH numbers (or the blocker evidence + the refresh number) — report + PR body (MD-5).
4. Run the full gate suite (AGENTS §10) green. Bookkeeping: the CHANGELOG Unreleased line. No tracker edits (MD-4). No push, no PR.

Report: per-gate results, the measured numbers (or blocker evidence) explicitly, deviations.
```

**Prompt 6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 6 of the kiosk implementation round (branch feat/perf-refresh-draw-bench).

1. Rebase onto main first; review git diff origin/main...HEAD.
2. Read AGENTS.md, the implementation plan (§3 K6), the audit's S2 + row 3 + C2 note, the dispatch plan §1/§7 (code-quality mandate in full), the stage-1 plan.
3. Functional focus: a GL-path measurement (row 3 is software-renderer terms — finding); one combined refresh+draw number; harness-test weakening without the stage-1 evidence; a renderer/backend selection smuggled in (the spike's); missing numbers or evidence in the report (MD-5).
4. Code-quality mandate (the seam stays test infrastructure — no production-path entanglement; SRP; one abstraction level).
5. Re-run the gates (AGENTS §10) + the bench yourself; compare with the reported numbers.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR — if the draw leg works, "Closes #300"; if API-blocked, no Closes line + the annotation on #300 records the evidence outcome per F13-03's framing. Checks to green, mergeStateStatus CLEAN. Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-perf-refresh-draw-bench.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, the numbers/evidence you reproduced, worktree removed.
```

## 8. Chunk 7 — DTO↔domain mapping bench (`feat/perf-mapping-bench`)

- **Covers** (implementation plan §3 K7; audit S4; KDF-7 row 5):
  a criterion bench in `taskboard-sync-nextcloud/benches/` over
  the pure mapping surface (`mapping.rs` DTO↔domain arms;
  `push.rs` `plan_pushes` + card-shape fns) at board scale N with
  `hot_path` fixture conventions; the allowance-vs-30 s
  derivation recorded in the bench docs once numbers exist;
  Bencher picks it up via the existing `cargo bench --workspace`
  job with zero CI edits. **Non-goals**: no wiremock full-cycle
  harness (network-dominated), no mapping code changes.
- **Acceptance**: the bench runs and reports; numbers recorded;
  zero CI-workflow diffs; no production diff; gates green;
  **closes #263**.
- **Traps**: network/wiremock code in the bench (CPU legs only —
  that is the point); fixture scale mismatched to `hot_path`
  conventions; `plan_pushes` benched through the actor instead of
  the pure fn.

**Prompt 7.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 7 of the kiosk implementation round ("DTO↔domain mapping bench", branch feat/perf-mapping-bench) in the taskboard repo at /home/viktoria/src/taskboard.

No round gate (MD-1). Precondition: none.

1. Read AGENTS.md, the implementation plan (§2 KDF-7 row 5, §3 K7, §4–§6), the audit's §2 row 5 + sketch S4, the dispatch plan §1/§8.
2. Read the seams: crates/taskboard-sync-nextcloud/src/mapping.rs (the pure DTO↔domain arms), crates/taskboard-domain/src/push.rs (plan_pushes, card shapes), crates/taskboard-storage-sqlite/benches/{hot_path.rs, support/mod.rs} (the fixture + criterion conventions), the crate's Cargo.toml dev-deps, the CI benchmark job (confirm it runs --workspace with zero edits needed).
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-perf-mapping-bench on a new branch feat/perf-mapping-bench off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-kiosk-mapping-bench-plan.md settling: the benched function set (each mapping arm + the push fns — enumerate), the fixture construction (board scale N per hot_path conventions), the criterion grouping, the allowance-derivation note text, the zero-CI-diff verification step. Record every decision.
5. Commit as: docs(plan): mapping CPU-leg bench. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 7.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 7 of the kiosk implementation round ("DTO↔domain mapping bench").

1. Read AGENTS.md, the implementation plan (§3 K7), the audit's S4 sketch, the dispatch plan §1/§8, and the stage-1 plan in ../taskboard-WT-feat-perf-mapping-bench (branch feat/perf-mapping-bench). Recreate the worktree if missing.
2. Implement exactly per the approved plan, commits standalone. Hard rules: pure CPU legs only (no wiremock/network); the fixture mirrors hot_path conventions at the chosen N; no production code changes anywhere; the bench docs carry the 30 s-budget allowance derivation once numbers exist.
3. Run the bench; record the numbers (report + PR body — MD-5). Run the full gate suite (AGENTS §10) green.
4. Bookkeeping: none beyond the bench docs. No tracker edits (MD-4). No push, no PR.

Report: per-gate results, the measured numbers explicitly, deviations.
```

**Prompt 7.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 7 of the kiosk implementation round (branch feat/perf-mapping-bench).

1. Rebase onto main first; review git diff origin/main...HEAD.
2. Read AGENTS.md, the implementation plan (§3 K7), the audit's S4 sketch, the dispatch plan §1/§8 (code-quality mandate in full), the stage-1 plan.
3. Functional focus: any network/wiremock leg (finding); production-code diffs (there must be none); fixture/scale divergence from hot_path conventions; a CI-workflow diff (there must be none); missing numbers (MD-5).
4. Code-quality mandate (bench hygiene: black_box usage, no dead parameters; the allowance derivation is evidence-grade).
5. Re-run the gates (AGENTS §10) + the bench yourself; compare numbers.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR ("Closes #263"; numbers in the body), checks to green, mergeStateStatus CLEAN. Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-perf-mapping-bench.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, reproduced numbers, worktree removed.
```

## 9. Chunk 8 — sync metrics + engine counters (`feat/state-sync-metrics`)

- **Covers** (implementation plan §3 K8; D-K6; KDF-8; **the
  MD-K8 window**): per-board cumulative counters on `SyncStatus`
  (cycles, pushes, pulls, failures, last-cycle duration, offline
  episodes — additive, zero defaults, cumulative-since-boot
  documented), populated at the existing `SyncReport` ingestion
  publish points, carried by the `aggregate()` fold so CLI/UI
  surfaces extend without re-shaping; the sync actor emits the
  raw facts (the window-sensitive file); `status` rendering
  (structural assertions, no text); **#145**: a handle-side
  atomic `EngineMetrics` struct + a `status` surface reading it —
  never in `AppState`, no snapshot churn. One deliberate insta
  pass owns the domain-status widening. **Non-goals**: no
  histograms/quantiles, no metrics crate, no new wake source, no
  badge change beyond the fold.
- **Acceptance**: counters survive restart-from-zero semantics
  (documented); the fold carries the shapes (CLI `status` renders
  them structurally); the insta diff is exactly the widening; the
  engine counters read from the handle without an `AppState`
  diff; gates green; **closes #156 + #145**.
- **Traps**: a new channel/broadcast for metrics (rides existing
  ingestion — that is the ruling); engine counters in `AppState`
  (snapshot churn — the finding); a second insta pass or
  unrelated snapshot churn; the sync-actor diff growing beyond
  the emission points (this is the contested file — scope
  discipline is the window's whole point).

**Prompt 8.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 8 of the kiosk implementation round ("sync metrics + engine counters", branch feat/state-sync-metrics) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition (MD-K8, the dispatch window — check INLINE and STOP if it fails): the multi-board round's chunk 4 (branch feat/sync-connector-modules) must NOT have started landing (gh pr list --state open --search "connector modules" must show no merged/open PR for that branch), AND the daily-driver round must not have dispatched (its round gate: gh issue list --milestone wave-multi-board --state open must be non-empty). If the window has closed (MB-4 landed or the MB round completed AND DD dispatched), STOP and report — this chunk waits for the MB→DD gap instead.

1. Read AGENTS.md, the implementation plan (§2 KDF-8, §3 K8, §4–§6), the dispatch plan §1/§9 (MD-K8 + MD-4), the D-K6 ruling (framing §4 / briefing §1.6).
2. Read the seams (re-locate; MB chunks 1–2 have landed — the shapes are post-0006/post-board-scoped-vocabulary): crates/taskboard-domain/src/state.rs (SyncStates, SyncStatus, aggregate()), the SyncReport ingestion + publish points in taskboard-state (actor.rs/core.rs), crates/taskboard-sync-nextcloud/src/actor.rs (the cycle's completion/report points — the ONLY places this chunk may touch that file), the status surface chain (commands/output.rs SyncStatusView, the UI badge's fold read), the app handle/command-dispatch path for the EngineMetrics home, the insta corpus conventions.
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-state-sync-metrics on a new branch feat/state-sync-metrics off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-kiosk-sync-metrics-plan.md settling: the exact counter fields + types + zero-defaults; where each counter increments in the ingestion path; the aggregate() fold's carrying shapes; the status rendering (structural assertions catalogue); the EngineMetrics struct (atomics, handle placement, the status read); the insta-pass catalogue; the cumulative-since-boot doc text; the explicit list of sync-actor lines touched (minimal, emission-only). Record every decision.
5. Commit as: docs(plan): sync and engine metrics surfaces. Do not implement; do not push.

Report: plan path, decisions, the window state found, risks, estimated diff size.
```

**Prompt 8.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 8 of the kiosk implementation round ("sync metrics + engine counters").

1. Read AGENTS.md, the implementation plan (§2 KDF-8, §3 K8, §4), the dispatch plan §1/§9, and the stage-1 plan in ../taskboard-WT-feat-state-sync-metrics (branch feat/state-sync-metrics). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, commits standalone. Hard rules: metrics ride the EXISTING SyncReport ingestion + publish (no new channel, no new wake source, no broadcast); EngineMetrics is handle-side atomics — nothing in AppState, no extra snapshot churn; the insta pass is ONE deliberate regeneration for the status widening (review the accepted diff as the widening only; no .pending files); the sync-actor diff is emission-only at the planned points; typed outcomes, no text assertions in tests; SPDX headers.
3. Run the full gate suite (AGENTS §10) green.
4. Bookkeeping: the CHANGELOG Unreleased line. No tracker edits (MD-4). No push, no PR.

Report: per-gate results, the insta-pass diff summary, deviations.
```

**Prompt 8.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 8 of the kiosk implementation round (branch feat/state-sync-metrics).

1. Rebase onto main first (fetch, rebase origin/main, push --force-with-lease if rewritten); review git diff origin/main...HEAD.
2. Read AGENTS.md, the implementation plan (§2 KDF-8, §3 K8), the dispatch plan §1/§9 (code-quality mandate in full; MD-K8), the stage-1 plan.
3. Functional focus — this chunk's traps: a new channel/broadcast/wake source for metrics (the ruling forbids it); EngineMetrics in AppState or any snapshot diff beyond the status widening; a second/unrelated insta diff; the sync-actor diff exceeding the planned emission points (this file is why the window exists — scope discipline here is load-bearing); text assertions; missing restart-from-zero semantics doc.
4. Code-quality mandate (naming/comments; SRP — the domain carries status, the actor emits facts, the engine ingests, the handle counts; one abstraction level).
5. Re-run the gates (AGENTS §10); failures are findings.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR ("Closes #156" + "Closes #145"), checks to green, mergeStateStatus CLEAN (rebase+force-with-lease if needed). Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-state-sync-metrics.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 10. Chunk 9 — kiosk deployment profile + unit + runbook (`feat/kiosk-deployment-profile`)

- **Covers** (implementation plan §3 K9; KDF-10; the multi-board
  design's fixed-board deferral; the #233 i686 finding): the
  `[kiosk]` config section (`fixed_board: Option<Uuid>`, typed
  resolution against live boards at boot — unknown/dead board =
  typed config error; section absent = today's behaviour;
  `deny_unknown_fields`); `contrib/taskboard-kiosk.service`
  (system unit: config-file-driven `mode = "kiosk"` — never an
  env-var launch, `Restart=always`, display-env import,
  `WantedBy=graphical.target`); the bring-up runbook (image
  checklist: no desktop environment, `SLINT_BACKEND`/software-
  renderer selection, SSD/USB note, harness invocation, the four
  budget verdicts to record, **the D250 i686 cross-build lane** —
  target + toolchain setup; the i686 release-target decision
  rides spike exit); README kiosk-deployment pointer; CHANGELOG.
  **Non-goals**: no rotation keys (K13), no `backend-fb` feature
  flip (the spike rules), no `sd_notify` (#160 stays open, decide
  at the spike), no release-workflow changes.
- **Acceptance**: a pinned board resolves or boot fails typed;
  the section absent = byte-identical behaviour; the unit never
  launches via env var; the runbook's i686 lane names the actual
  target/toolchain steps; gates green; annotation on #233.
- **Traps**: rotation/config keys beyond `fixed_board` (K13's);
  an env-var mode launch in the unit; `sd_notify` sneaking in;
  resolution done outside the typed-config layer; `[kiosk]`
  colliding with the MB chunk-5 `[[sync.connectors]]` section
  (ordering rule below).

**Prompt 9.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 9 of the kiosk implementation round ("kiosk deployment profile + unit + runbook", branch feat/kiosk-deployment-profile) in the taskboard repo at /home/viktoria/src/taskboard.

Preconditions (MD-9): chunk 3 (feat/app-kiosk-lifecycle) merged on origin/main, AND the multi-board round's chunk 5 (feat/app-connector-registry) merged (same config.rs — the ordering rule). Verify both; if the MB chunk is not merged, either STOP or (if its diff is reviewable) plan the section to be conflict-free and record the risk — STOP is the default.

1. Read AGENTS.md, the implementation plan (§2 KDF-10, §3 K9, §4–§6), the dispatch plan §1/§10, the framing's i686 finding (§3 post-acceptance note), the #233 body + its 2026-10-19 annotation (the i686 bring-up finding), ADR 0015's #236 amendment (units keep explicit verbs / config-file modes).
2. Read the seams: crates/taskboard-app/src/config.rs (the section structs, deny_unknown_fields, typed-validation precedents, the AppMode decode), the live-boards query the resolution validates against (domain state), contrib/taskboard.service (unit conventions), the runbook destination conventions (contrib/ vs docs/), the cross-build surface (rustup targets; the release profile's kiosk comments).
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-kiosk-deployment-profile on a new branch feat/kiosk-deployment-profile off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-kiosk-deployment-profile-plan.md settling: the [kiosk] section shape + decode + the typed resolution error set; the absent-section = unchanged behaviour test; the unit file's full text (ExecStart, Restart=always, environment import, target); the runbook outline incl. the i686 lane (exact target triple, toolchain steps, build command, transfer) and the four budget-verdict recording slots; the README pointer. Record every decision.
5. Commit as: docs(plan): kiosk deployment profile and runbook. Do not implement; do not push.

Report: plan path, decisions, the MB-5 state found, risks, estimated diff size.
```

**Prompt 9.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 9 of the kiosk implementation round ("kiosk deployment profile + unit + runbook").

1. Read AGENTS.md, the implementation plan (§2 KDF-10, §3 K9), the dispatch plan §1/§10, and the stage-1 plan in ../taskboard-WT-feat-kiosk-deployment-profile (branch feat/kiosk-deployment-profile). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, commits standalone. Hard rules: fixed_board resolution is typed config validation (never a silent default, never a runtime warning); section absent = byte-identical behaviour; the unit launches via the config file's mode (never TASKBOARD_APP__MODE, never env vars); no sd_notify; no rotation keys; the runbook is evidence-grade (exact commands, the i686 target named); SPDX headers.
3. Run the full gate suite (AGENTS §10) green.
4. Bookkeeping: the CHANGELOG Unreleased line. No tracker edits (MD-4). No push, no PR.

Report: per-gate results, deviations.
```

**Prompt 9.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 9 of the kiosk implementation round (branch feat/kiosk-deployment-profile).

1. Rebase onto main first; review git diff origin/main...HEAD.
2. Read AGENTS.md, the implementation plan (§3 K9), the dispatch plan §1/§10 (code-quality mandate in full), the stage-1 plan.
3. Functional focus: an env-var launch in the unit or runbook (the ADR 0015 #236 rule); silent fixed_board fallback (typed refusal is the contract); rotation keys or sd_notify (out of scope); the absent-section path changed; the i686 lane naming a wrong target or skipping the toolchain steps.
4. Code-quality mandate (config-layer SRP; unit/runbook text states constraints).
5. Re-run the gates (AGENTS §10); failures are findings.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR (annotation on #233: the deployment profile + i686 lane landed; the on-device half awaits the spike — one note; no Closes line), checks to green, mergeStateStatus CLEAN. Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-kiosk-deployment-profile.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 11. Chunk 10 — web kiosk dashboard design round (`design/kiosk-dashboard`)

- **Covers** (implementation plan §3 K10; framing §5.1; KDF-12;
  MD-10's stack gate): the docs-only design-proposal plan at
  `.artifacts/plans/<date>-kiosk-dashboard-design-plan.md` — the
  §6-style question record with exactly one recommendation each:
  HTTP stack (tiny_http vs hyper-server vs axum vs hand-rolled;
  the briefing §1.7 dependency facts re-verified as evidence;
  **stack approval is the user's — the round recommends, the user
  approves**), bind + auth (loopback default; the LAN arm's
  token/warning/proxy options), content scope, meta-refresh/no-JS
  refresh, server placement on the daemon + appliance lifecycles,
  standalone-mode interaction, the 2014-browser checklist,
  caching, config-section sketch, risks. **No production code, no
  dependency edits** (MD-10). **Non-goals**: no K11 dispatch, no
  implementation probes that land.
- **Acceptance**: the doc lands with re-verified evidence anchors
  (the ArcSwap read path, the K8 metrics surface it will read);
  every question carries exactly one recommendation; the stack
  section separates "recommendation" from "user approval" as
  distinct steps; the PR diff is exactly the doc; **annotates
  #234** (design landed, awaiting the ruling).
- **Traps**: recommendations smuggled in as decided (every item
  awaits the session); a dependency added (the gate); scope
  growth into a management surface (read-only is the lane);
  unverified dependency facts (re-run the graph queries).

**Prompt 10.1 (planning = the whole chunk, docs-only):**

```text
You are the sole agent for chunk 10 of the kiosk implementation round ("web kiosk dashboard design round", branch design/kiosk-dashboard) in the taskboard repo at /home/viktoria/src/taskboard. This is a docs-only chunk: the deliverable is one design-proposal document; no production code, no Cargo.toml change (MD-10 — the HTTP stack is recommended, never added).

Precondition: none (docs-only, dispatchable anytime — earliest is best).

1. Read AGENTS.md, the implementation plan (§2 KDF-12, §3 K10, §4), the framing §5.1 + the D-K7 ruling, the briefing §1.7 (the dependency facts) and §2 (the Tier-2 questions), the accepted design §3 Option C, the #234 body (the tablet's constraints), and the dispatch-plan template's evidence & artefact mandate.
2. Re-verify the evidence yourself (do not trust the briefing's numbers): cargo tree -i hyper --workspace, the lockfile entries for axum/tiny_http/httparse, the ArcSwap read path a dashboard would use, the K8 metrics surface (or its planned shape if chunk 8 is still in flight — state which you found), the daemon/appliance lifecycle insertion points for a server task.
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-design-kiosk-dashboard on a new branch design/kiosk-dashboard off main.
4. Write the design doc at .artifacts/plans/<today>-kiosk-dashboard-design-plan.md in the house [Plan] style: background, verified baseline table, one §per question with options + trade-offs + EXACTLY ONE recommendation (HTTP stack; bind+auth incl. the LAN token variant; content scope; refresh; placement; standalone; browser-compat checklist; caching; config sketch; risks; backlog-candidates), and an explicit "user approval required" gate on the stack recommendation. Every factual claim carries its evidence.
5. Commit as: docs(plan): web kiosk dashboard design proposals. Do not implement; do not push.

Report: doc path, the recommendations one-line each, open questions for the user's session.
```

**Prompt 10.3 (review — the chunk has no stage 2):**

```text
You are the stage-3 REVIEW agent for chunk 10 of the kiosk implementation round (branch design/kiosk-dashboard, docs-only).

1. Rebase onto main first; review git diff origin/main...HEAD — it must be exactly one new .artifacts/plans document (anything else is a finding).
2. Read AGENTS.md, the implementation plan (§3 K10), the framing §5.1, the briefing §1.7/§2, the dispatch plan §1/§11 (the evidence & artefact mandate applies in full).
3. Functional focus: any question with zero or multiple recommendations (the record needs exactly one each); the stack "recommendation" and the "user approval" not separated as steps; dependency facts asserted without re-verification (spot-check the cargo tree output yourself); a management/write surface in the content scope; the 2014-browser constraints (no flex/grid reliance, no JS requirement) absent from the checklist; evidence-less claims.
4. Evidence & artefact mandate: every claim carries the command/source + observed outcome; the PR diff is exactly the doc; no tracker edits beyond the annotation.
5. Fix findings by editing the doc (amend/reword).
6. Ship: push, open the PR (annotation on #234: the design round landed, awaiting the user's decision session — one note, no Closes line), checks to green, mergeStateStatus CLEAN. Never merge; never touch main.
7. Teardown: git worktree remove --force ../taskboard-WT-design-kiosk-dashboard.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 12. Chunk 12 — encrypted-file credential store (`feat/app-file-credential-store`)

- **Covers** (implementation plan §3 K12; D-K8 **ruled option
  (a)**; KDF-9): the third `CredentialStore` impl — generated
  0600 key file + AEAD envelope (versioned blob; key + ciphertext
  beside the db by default); `[nextcloud] credential_store =
  "file"`; the key-file bootstrap ceremony (generate on first
  use, 0600, typed permission failure); never a silent fallback;
  passphrase/argon2 **out of scope** (additive port impl later if
  a desktop need appears). **MD-10 gate**: the AEAD crate
  (pure-Rust RustCrypto — `aes-gcm` or `chacha20poly1305`) plus a
  direct `getrandom` need **stack approval at stage 1** — the
  user's D-K8 ruling approved option (a)'s *shape*; the specific
  crate selection is stage 1's recorded decision and, if not
  covered by that ruling's spirit, a STOP-and-ask. **Non-goals**:
  no key rotation, no multi-profile, no wire change (secrets
  never ride IPC — pinned by test).
- **Acceptance**: the trio (keyring/env/file) selects explicitly
  by config; headless boot works with the key file present and
  fails typed without it; permission failure (0600 violated) is
  typed; the port's no-silent-fallback rule holds; round-trips
  via tempdir fakes; `cargo deny` green over the new tree; gates
  green; **closes #159 + #126**.
- **Traps**: a silent fallback to another store; key generation
  without explicit 0600 (or ignoring a violated mode); the
  AEAD nonce reused across writes (fresh per encryption, stored
  alongside); secrets in Debug output (the `AppPassword`
  redaction must wrap); config key drift from the D-K8 sketch;
  the port signature moving (it is unmoved by ruling).

**Prompt 12.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 12 of the kiosk implementation round ("encrypted-file credential store", branch feat/app-file-credential-store) in the taskboard repo at /home/viktoria/src/taskboard.

No predecessor (MD-1). The design gate is CLOSED: D-K8 ruled option (a) 2026-10-19 (generated 0600 key file + AEAD envelope; passphrase variant out of scope).

1. Read AGENTS.md, the implementation plan (§2 KDF-9, §3 K12, §4 incl. MD-10's dependency gate, §4–§6), the dispatch plan §1/§12, the briefing §1.8 (the threat-model honesty + the zero-code alternative the ruling declined), #159/#126 bodies, ADR 0008 (the CredentialStore port decision).
2. Read the seams: crates/taskboard-app/src/secrets.rs (the port, KeyringStore, EnvCredentialStore, AppPassword redaction, the typed Backend error), config.rs ([nextcloud] credential_store decode), the bootstrap call sites, the existing store tests (fake/tempdir conventions), Cargo.toml [workspace.dependencies] (the pin-once rule) + cargo deny configuration.
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-feat-app-file-credential-store on a new branch feat/app-file-credential-store off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-kiosk-file-credential-store-plan.md settling: the AEAD crate choice (aes-gcm vs chacha20poly1305 — one recommendation with the evidence) + the direct getrandom pin; the envelope format (versioned blob: magic/version/nonce/ciphertext); the key-file ceremony (path default, 0600 enforcement + violation = typed error, generation on first use); the config surface + decode tests; the store impl's read/write/round-trip semantics; the failure fence catalogue (missing key, bad mode, corrupt envelope, wrong version — each typed); the no-silent-fallback + no-Debug-leak tests; the cargo deny expectation. Record every decision. If you judge the crate selection exceeds the D-K8 ruling's grant, STOP and ask instead of deciding.
5. Commit as: docs(plan): encrypted-file credential store. Do not implement; do not push.

Report: plan path, the crate recommendation, decisions, risks, estimated diff size.
```

**Prompt 12.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 12 of the kiosk implementation round ("encrypted-file credential store").

1. Read AGENTS.md, the implementation plan (§2 KDF-9, §3 K12, §4), the dispatch plan §1/§12, and the stage-1 plan in ../taskboard-WT-feat-app-file-credential-store (branch feat/app-file-credential-store). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, commits standalone. Hard rules: the port signature is UNMOVED; the trio selects explicitly by config — never a silent fallback; fresh nonce per encryption; AppPassword redaction wraps everything; 0600 enforced with a typed violation error; no key rotation, no passphrase variant; the new crates pinned once in [workspace.dependencies]; cargo deny green over the new tree; typed outcomes; SPDX headers.
3. Run the full gate suite (AGENTS §10) green.
4. Bookkeeping: the CHANGELOG Unreleased line. No tracker edits (MD-4). No push, no PR.

Report: per-gate results, the cargo deny outcome, deviations.
```

**Prompt 12.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 12 of the kiosk implementation round (branch feat/app-file-credential-store).

1. Rebase onto main first; review git diff origin/main...HEAD.
2. Read AGENTS.md, the implementation plan (§2 KDF-9, §3 K12), the dispatch plan §1/§12 (code-quality mandate in full), the stage-1 plan.
3. Functional focus — this chunk's traps: any silent fallback (the port's rule is a ruling); nonce reuse or a hardcoded nonce; a secret reaching Debug/logs (the redaction wrapper bypassed); 0600 not enforced or the violation untyped; the port signature changed; key rotation or passphrase code (out of scope); crates not pinned once in workspace deps; missing version field in the envelope.
4. Code-quality mandate (crypto hygiene is constraint-comment territory — state the invariants the code can't show; SRP — the store stores, bootstrap selects; one abstraction level).
5. Re-run the gates (AGENTS §10) incl. cargo deny; failures are findings.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR ("Closes #159" + "Closes #126"), checks to green, mergeStateStatus CLEAN. Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-feat-app-file-credential-store.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 13. Completion bookkeeping

Per-chunk issue actions (implementation plan §6, normative):

| Chunk | Issue action |
|---|---|
| 1, 2, 3 | annotate #237 per chunk (one note each, MD-4) |
| 4 | **closes #237** (the appliance is complete) |
| 5 | **closes #299 + #262** (measured references in the PR bodies) |
| 6 | **closes #300** — or, if API-blocked, the evidence annotation and the issue closes citing the evidence (F13-03's framing) |
| 7 | **closes #263** |
| 8 | **closes #156 + #145** |
| 9 | annotates #233 (the deployment profile + i686 lane; the on-device half awaits the spike) — #160 keeps its spike-decision comment |
| 10 | annotates #234 (design landed, awaiting the ruling) |
| 12 | **closes #159 + #126** |

**Deliberately not dispatched here**: **K11** (dashboard impl —
dispatches from the accepted K10 design + the user's stack
approval; its dispatch plan amendment or a follow-up round covers
it), **K13** (ambient view #363 — gated on daily-driver chunk 10),
**K14** (the wave exit — with the phase-C round), the phase-B
spike (user-executed; K9's runbook is its entry point). Once the
last phase-A chunk merges, the next sessions are: the user's K10
decision ruling → K11's dispatch, and the spike. Nothing else
opens; no tracker edits beyond §13.

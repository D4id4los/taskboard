# Backlog-Reduction Round 3 — Scope Recommendation (2026-11-19)

Input: the mandated runtime family (#1069–#1072, from the
2026-11-19 build/test runtime analysis — **first in the round** per
the operator), the artefact sweep (committed
`chore/artefact-sweep-2026-11-19` @ `73a9e09e`, 20 files), and a sweep
of all 111 open issues (~50 filed since 11-18, mostly r9-chunk
follow-ups, the restore/ipc tail, and the runtime analysis).

Context: r9 and the Tier-2 round are fully landed (no open
non-dependabot PRs); the filed follow-up set from the hygiene-r2
close-out is almost entirely closed. The open list is now dominated by
three families: r9-era parity/doc follow-ups, the restore/ipc tail,
and the new runtime family.

## A. The round seed — runtime family (mandated, first)

| Chunk | Issue | Shape | Note |
|---|---|---|---|
| R1 | #1069 (lld) | S — `.cargo/config.toml` stanza + measured `--all-targets` delta | Highest ROI per line; also verify the app bin's ~3s loop is unaffected |
| R2 | #1071 (shutdown budget) | M — fixtures override `shutdown_timeout_secs` (mechanical) **plus** the actor-teardown investigation (why any actor burns 10s — production-relevant) | The investigation is the sanctioned dedicated pass for **#968** (`a12_shutdown_exits_the_loop` — same seam family: shutdown/timer races under load; the issue's own deterministic-fix candidate is the join-await). If the root cause covers #968's failure mode, close it with evidence — never opportunistically, this *is* the dedicated diagnosis |
| R3 | #1072 (proptest generator) | S — construct the accepted shape directly; the property's assertions stay | |
| R4 | #1070 (consolidate 98 test binaries) | L — group probe_tests binaries; nextest name-filtering preserved | **After R1** so the lld delta is measured in isolation; also unblocks #327 (the mutants lane's machine-time shrinks with the binary count — schedule it *after* this round) |

Tension noted by the report and accepted: consolidation groups
*binaries*, not test cases — one-behavior-one-test is untouched.

## B. Joining the round (recommended, ~8 chunks beside the seed)

**B1. Correctness bugs (domain/state):** **#986** (p2: same-instant
double `LogHabit` silently loses the second increment under the
equal-clock LWW tie — real under-count; the DEBT PIN test and the
Σ-property's `dt >= 1s` exclusion are the rails: fix the rule, *include*
the degenerate sequence in the property, never weaken) and **#1039 +
#1038** (one undecodable journal row aborts the whole boot load;
`Corrupted` carries no decode context — typed per-row quarantine/
skip policy decision + context, one chunk).

**B2. UI regression/bug bundle (small, r9 residue):** **#1004**
(capture modal lost border/shadow on ModalDialogHost — W1 regression),
**#1026** (palette's dead `accepted()` seam + the J2 W-4/§7 row
disagreement), **#1033** (legacy stack-walk chords cancel a live board
move — settle-then-run), **#1036** (embedded planner mount steals
focus inside the walk), **#1053** (walk-stops arithmetic excludes the
host button row). One chunk, all root-caused r9 filings with small
blasts.

**B3. App-test hygiene cluster:** **#984** (app test binaries compose
ambient `TASKBOARD_*` without the env lock — the #865 pattern's app
twin), **#1013** (bootstrap mints `device.json` in the operator's real
home during in-process suites — the #751 injection's residue),
**#982** (`test_workspace_with_app` drops its tempdir under the live
app), **#981** (fold the local `RefusingStore` doubles onto
`test_support`). One chunk, app tests.

**B4. Journey/testing-backend cluster:** **#1016** (p2: plain
`cargo test --test journeys` fails wholesale — the DriverOptions
OnceLock mix; split the headless:false variants into their own binary
or justify dropping them), **#1023** (stale geometry for conditional
remounts), **#1024** (FakeEngine UpdateTask effort-mask settle arm),
**#995** (raw-focus restore across visibility toggle), **#996**
(DialogButton initial-focus index), **#997** (bind the chunk-10 habit
journey tests into J1's wiring), **#1059** (habit editor escape legs
pinned), **#1018** (habit card geometry probe). One or two chunks;
**#1062** (p2: shared journey-harness helper — the SonarCloud
duplication driver) lands first inside this cluster since it is the
structural fix the cluster's fixes compose with.

**B5. Gate/law/docs residue (scripts+docs):** **#965** (substring
token matching), **#967** (fixture tree in the evidence universe),
**#970** (evidence scan includes `target/`), **#964** (stroke.rs doc
dedup), **#1063** (resource-law clarification into
`agent_dispatch.org`), **#1043** (scratch dirs on tmpfs — the ENOSPC
law), **#1042** (doctor vs bench worktrees), **#1046** (advance-to-
deadline gotcha into the clock-law docs), **#1047** (`visible:false`
layout-slot footgun doc). One chunk — the continuation of the N-A gate
work and the incident-law codification.

**B6. IPC/state follow-ups (tier-2 tail):** **#993** (CommandOutcome
serde round-trip pin), **#994** (the stop-ordering plan's over-counted
fallback sites — correct the plan/impl divergence), **#991** (status
during the stop-drain window — typed draining vs absent), **#990**
(RestoreTimeEntry repeat-identical-row idempotency). One chunk,
small/medium, engine+app.

**B7. Config dx pair (journey-gated):** **#1014** (batch config unset
— parity with the landed batch set) + **#1015** (manual reload CLI
verb — the C10 lane's escape hatch). One journey plan, two stories
(the P6 precedent); #1051 (files_sync.poll_secs has no live-write lane
— the #673 writer twin) is engine-side and ungated — it can ride the
same wave as its own small chunk.

**B8. Small-refactor bundle:** **#978** (flyout width tokens at
consumer mounts), **#1034** (subsume ControlNext Tab re-injection),
**#1060** (switch_plan_pane wrap arithmetic), **#1065** (FocusScope/
Return-Space grammar dedup), **#1027** (pin the overlay-exclusivity
fold row). One chunk, all refactors with existing test cover.

## C. Deferred, with homes (so nobody re-derives)

- **Feature rounds, journey-gated (not this hygiene round):**
  #955–#963 (habit backlog — approved, needs the journey wave),
  #945–#949 (O-5 CLI-parity remainder — #948 GUI export largest),
  #353/#354 (undo UI legs), #979 (restore dispatch surfaces — the
  verbs need their first surface), #1045 (ConfigReload UI lane),
  #1011 (banner Unmute), #976 (p2: R-4 outside-click flip — now
  actionable post-W2; first in the next UI round).
- **r9-parity remainder round (the habit/keyboard/menu follow-ups):**
  #1030–#1032, #1035, #1041, #1048, #1049, #1057, #1066–#1068 — all
  small parity/doc items best swept in one dedicated pass with journeys
  where user-visible.
- **Bigger refactors (next refactor round):** #1029 (severing-rule
  generalization), #1037 (flow-settle FK vocabulary), #1062's
  non-harness remainder if any.
- **Watches (conditional, no action now):** #988/#983/#987 (chrono
  posture), #975/#972 (Slint upstream), #1021 (sibling-path marker if
  resurrection bites), #1052 (build-cache revisit on sccache hash-key
  normalization), #919 (usage-evidence conditional), #980
  (informational for #357's design round — relabel `documentation`).
- **Flaky watchlist:** #697/#650 (armed — close on next CI recurrence
  with telemetry), #336/#287/#384/#883/#1020 (no fresh recurrence;
  #1020 rides the watchlist, not this round — C4 touches binary
  structure, not test logic).
- **#327 mutants lane:** schedule after R4 lands (the binary
  consolidation changes the mutants targeting and shrinks the machine
  time).
- **Standing deferrals unchanged:** the perf family
  (#131/#137/#138/#139/#140/#141/#142/#146/#148/#152/#153/#154/#155/#157),
  feature-scale items (#122/#121/#127/#181/#184/#185/#191/#193/#195/#198/#213/#215–#218/#219/#224/#225/#444/#440/#494/#117/#639–#642/#680/#691/#700/#701/#715/#716/#723/#762/#769/#770/#778/#782),
  C-6 triage deferrals (#807/#809/#812/#813/#819/#820), #113/#114/#115
  (operator intent gate still pending).

## D. Round shape

~12 chunks: R1–R4 (the seed, first), then B1–B8 in two parallel waves
under the resource law (≤3 agents, memory gate, flock-deferral; the
mutants-class runs stay exclusive). Journey gates only on B7.
Dependency edges: R1 → R4 (measurement isolation); B4 after #1062's
helper lands inside it; R2's investigation may re-classify #968.

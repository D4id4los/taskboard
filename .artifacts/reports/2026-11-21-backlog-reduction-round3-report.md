# Backlog-Reduction Round 3 ("runtime + sweep") — Close-Out Report

- **Date**: 2026-11-21
- **Plan**: `.artifacts/plans/2026-11-19-backlog-reduction-round3-dispatch-plan.md`
  (15 chunks, three-stage protocol; ≤3 concurrent lanes; R4 exclusive;
  §18 amendment added R2b/R2c by operator instruction)
- **Result**: **all 15 chunks merged**; zero open PRs; zero round
  worktrees left; every round issue closed.

## 1. Chunk outcomes

| Chunk | Issues | PR | Outcome (headline) |
|---|---|---|---|
| R1 lld linker | #1069 | #1081 | dev/test links through lld (probe-verified stable spelling); all-targets rebuild wall 3:09→1:50 (−42%); CI apt additions; release verified |
| R2 shutdown budget | #1071 | #1088 | suite wall 350→169.5s (−52%); 41→13 over-5s; teardown-stall hypothesis REFUTED (convicted: boot-pull settle predicate); one production fix (unwind sender leak); #968 stays open (disposition) |
| R3 push-phase generator | #1072 | #1084 | rejection sampling → direct construction; 97.4s→0.8s (~120×); cross-board divergence found (#1080) |
| R4 consolidate binaries | #1070 | #1098 | 98→47 binaries; build-side goal served; run-side wall regression (86.9→152.7s) operator-accepted, #1090 carries tuning |
| B1 LogHabit tie | #986 | #1101 | mint-side monotone clock bump; Σ exclusion removed; no migration; replay-deterministic |
| B2 journal decode | #1039, #1038 | #1136 | hybrid policy (entity abort-with-context / journal quarantine-with-signal); typed CorruptionDetail; 12-case matrix + proptest |
| Bui residue bugs | #1004, #1026, #1033, #1036, #1053 | #1109 | five fixes, per-issue red→green, walk/dialog surface tested as a set |
| B3 app-test hygiene | #984, #1013, #982, #981 | #1118 | env locks everywhere; BootIdentity seam; owned WorkspaceFixture; one RefusingStore; hostile-env pass converged |
| B4a harness helper | #1062, #1016 | #1107 | shared Harness + run_journey; duplication −45%; 404 mode literals→0; journeys.rs one-platform (convert-not-split, tier evidence) |
| B4b backend pins | #1023, #1024, #995, #996, #997, #1059, #1018 | #1119 | seven pins; #997 gate reads BOUND/FOUND 32 checks, 0 MISSING; #1115's escape-rung contradiction filed not fixed |
| B5 gate/law/doc residue | 9 issues | #1077 | boundary-anchored token matching + evidence-scoped exclusions (self-tested both directions); ENOSPC/bench-worktree/one-slot laws; zero FOUND→MISSING flips |
| B6 IPC/state follow-ups | #993, #994, #991, #990 | #1117 | serde derive + compile-exhaustive round-trip pin (count-pinned); plan D-2.3 corrected; draining status typed; idempotent restore |
| B7 batch unset + reload | #1014, #1015 | #1135 | atomic multi-key unset (prove_and_remove_doc twin); config reload through the landed reload_once; journey gate accepted incl. D-SCOPE |
| B7b poll write lane | #1051 | #1086 | files_sync.poll_secs Apply::live() entry + writer; CLI-only exception documented (#1083) |
| B8 small refactor bundle | #978, #1034, #1060, #1065, #1027 | #1124 | five behavior-identical refactors + the promised om_us2_s1 fold row; funnel-half finding commented on #1027 |
| R2b boot-pull predicate | #1082 | #1122 | §18 amendment: all-unbound-world settles immediately; the verb-boot 10s quantizer eliminated (sub-second residuals) |
| R2c drain contract | #1085 | #1138 | §18 amendment: option (a) by operator ruling — mid-cycle Shutdown checkpoints; teardown inversion 30s→6.6ms |

## 2. Operator rulings exercised

1. R4: ship-as-measured (run-side wall regression accepted, #1090).
2. P4-successor B7: journey acceptance + D-SCOPE (one-key refusals join
   ConfigBatchError).
3. B2: hybrid decode policy.
4. R2c: drain-contract option (a).

## 3. Round-level measurement wins

- Local all-targets rebuild: −42% (R1 lld).
- Suite wall: −52% (R2 budgets; the 10s boot quantizer eliminated by
  R2b's predicate).
- Push-phase property: ~120× (R3 generator).
- Journey duplication: −45% (B4a helper).
- Test binaries: 98→47 (R4; run-side trade accepted).

## 4. New backlog issues filed this round (round 3)

#1080, #1083, #1087, #1090, #1093–#1095, #1097, #1099–#1101-adjacent
(#1093/#1094/#1095 from stage 1s), #1103, #1106, #1108, #1110–#1113,
#1115, #1120, #1133, #1134, #1137, plus #1082/#1085 (armed by R2's
investigation, both fixed this round after scheduling) and #697/#1020
flaky-watchlist observations. All carry the originating plan/report
permalinks.

## 5. Process notes

- R4.2 was quota-cut mid-flight; resumed with explicit state salvage
  (the shared-module mechanics flaw it found was real and fixed).
- R2's commissioning hypothesis was refuted by its own investigation —
  the armed findings (#1082/#1085) were then scheduled in-round (§18)
  and both fixed.
- The CI/merge loop adopted this round: standing merge authority
  (rebase-merge, fresh CLEAN in the same command as the merge) — after
  two premature-close incidents (see below).
- Honest incidents: two premature issue-closes + worktree teardowns on
  BEHIND races (#1082, #1039/#1038 — both reopened, recovered, closed
  at the real merge); one Bui.3 hook-bypass on comment-only commits
  during a 55-min lock hang (#1110 filed for the missing liveness
  check); B2's stage-3 discovered a dropped serde-triage commit and
  folded it (dispatcher-committed in-flight refinement).

## 6. What opens next (scope report §C + the plan's pointers)

- The r9-parity remainder round; the CLI-parity feature round (#948
  export largest); #327's sharded mutants lane (now unblocked — R4
  landed); the chrono ruling follow-ups per #988's watch; #1090's
  run-side tuning; #1087's further consolidation (if measurement
  justifies); #1093's journey-gated degraded-boot UI surface.
- Deliberately deferred: the journey-gated feature families
  (#955–#963, #945–#949, #353/#354, #979, #1045, #1011, #976), bigger
  refactors (#1029/#1037), the watch sets, the flaky watchlist.

# Round close-out — backlog-reduction round 2026-11-17 ("hygiene r2")

- **Kind**: dispatcher close-out report — the round's final artifact.
- **Dispatch plan**: `.artifacts/plans/2026-11-17-backlog-reduction-dispatch-plan.md`
  (11 chunks) + two operator-approved additions (the resource-gate
  chunk C12, and C13/#923 born from the incident investigations).
- **Final state**: **11/11 round chunks + 2 follow-up chunks landed**;
  main @ `4dd66317`; every dispatch worktree torn down; 4 remaining
  sibling worktrees belong to the other (r8) session.

## 1. Chunks landed (all via reviewed PRs, all gates green)

| Chunk | PR | Issues closed | Notes |
|---|---|---|---|
| C1 flock-deferral law | #894 | #879 | compressed; carried the chrono deny-ignore (#891 filed, open for the stack ruling) |
| C5 same-ref collapse | #895 | #856 | #682 property byte-identity enforced; #892 filed (accepted comment staleness) |
| C4 audit-finding seams | #896 | #860, #868 | push-once semantics pinned; SKEW_BAND_SECS single constant; #885 tracks the band-shape alternative |
| C2 test-infra determinism | #898 | #857, #843 | canary evidence captured (dispatcher-owned, artifact 90d); #650 stays OPEN (armed); #893/#884 filed |
| C3 flaky diagnosis r2 | #899 | #570 | #697 stays OPEN (armed hypothesis; closes on next recurrence's telemetry); #886 referenced |
| C6 app/storage hygiene | #908 | #865, #876, #877 | ledger-derived rewind helper; hostile-env pass; #897/#905/#729 follow-ups |
| C12 resource gate | #906 | #887, #888, #889 | memory-gate.sh + doctor floor + cap law; floor measured (clippy peak ~3.9 GiB) |
| C11 boards publish --to-files | #911 | #764 | journey-gated; UX walkthrough PASS; #912 MINOR filed |
| C10 config-set propagation | #915 | #845 | journey-gated; concurrent-knob scenario verified; #909/#910/#914 filed |
| C8 RestoreStack foundation | #916 | #356 | snapshot corpus new-files-only; #900/#901/#913 filed |
| C9 per-board undo scoping | #928 | #355 | strict frontier rule; zero snapshot churn; #919 filed |
| C7 mutants one-off | #932 | — (#327 OPEN by design) | ipc 99/109, survivors dispositioned (#902 stands, #926 root-caused); commands/ 892 un-run with sizing arithmetic |

**Still open by design**: #327 (commands/ remainder), #650 (armed),
#697 (armed), #902/#926/#919/#912/#914/#897/#892/#885/#900/#901/#913
(#893, #884, #886, #905, #909/#910 — the filed follow-up set),
#930 (host-level tmpfs cap — operator), #904 (AGENTS.md wording),
#907 (shellcheck gate), #890 (shared build cache).

## 2. Incidents & the defense layers they produced

Two hard resource incidents during the round (full analysis:
`2026-11-17-oom-incident-concurrency-learning-report.md` + addendum):

1. **10:48 global OOM** — 7 fresh-worktree clippy builds + the
   dispatcher's llvm-cov suite, none serialized. → #906 (memory gate,
   agent-cap law, doctor floor) landed mid-round.
2. **~17:31 freeze** — a mutants run at 16 jobs (prompt violation) +
   ungated neighbors; no kernel OOM trace (swap-thrash hang). →
   #924 (operator enabled systemd-oomd), #931 (two-mode gate law:
   hold for short/heavy builds, preflight-only for self-capped long
   runs), #930 (host tmpfs cap, pending operator).
3. **Live corollary** (C7 evidence): ungated `-j 2` mutants runs were
   oomd-killed at 23–25 G peaks vs 13.2 G under the gate's
   serialization — recorded on #923's thread as input for a possible
   law refinement (mutants-specific job cap or gate-hold); the landed
   law text was NOT reopened.

**Operator-side defenses now live**: systemd-oomd active (#924); the
tmpfs `size=` fstab cap (#930) remains with the operator.

## 3. Process observations (for the next dispatch plan)

- Resume-over-redo paid for itself three times (first OOM: 7 stage-1
  plans landed before the kill; second: C5/C4/C2/C9 resumes from
  landed commits; the concurrency-limit failure of C9.3 resumed
  cleanly mid-review).
- The two journey gates (C10/C11) worked as designed — operator
  acceptance before implementation, walkthrough mandates at stage 3.
- Stage-2 agents can violate explicit invocation directions under
  long-run pressure (the 16-job mutants launch). The two-mode law +
  oomd now bound the damage; consider adding the invocation command
  itself to stage-2 prompt checklists.
- The memory gate's lock is a shared global: long holders starve all
  committers. The two-mode law fixes the classified cases; a
  mutants-specific cap refinement may follow from C7's numbers.

## 4. What opens next

- #650's re-diagnosis on the next CI recurrence (the #857 artifact
  mechanism is armed with 90-day retention).
- #697's closure path: next cli_tty CI recurrence + gap telemetry.
- #353/#354's journey planning once r8 completes (undo-v2 UI legs).
- #327's `commands/` mutants lane (~10–20 h machine time; sharded
  `--jobs 1` runs) — a dedicated pass, not opportunistic work.
- The next backlog round sweeps the filed follow-up set (§1's open
  list).

# Tier-2 Round ("post-r9 parallel tier-2") — Dispatch Close-Out Report

- **Date**: 2026-11-19
- **Plan**: `.artifacts/plans/2026-11-18-tier2-dispatch-plan.md`
  (9 chunks, three-stage protocol; ≤2 concurrent lanes, flock/deferral
  discipline; operator gates at P4/P6 journeys, P7 leg-2, P9 ruling)
- **Dispatcher**: this session (P1 additionally executed compressed,
  dispatcher = planner + implementer).

## 1. Chunk outcomes

| Chunk | Issues | PR | Outcome |
|---|---|---|---|
| P1 golden solo-fail (compressed) | #846 | #1009 | **MERGED**, issue closed. Not reproducible; render proven selection-deterministic (solo PNG == suite PNG byte-identical); armed diagnosis + updated recurrence capture policy. |
| P2 today due boundary | #893 | #1019 | **MERGED**, issue closed. Domain-owned `last_covered_instant` mint adopted by all five bare-day mint sites + the multi-select twin found in review; us6_s2 strengthened; census superseded. |
| P3 CLI verb & grammar hygiene | #874, #873 | #1012 | **MERGED**, issues closed. `ReverseFenced`/`ToDeck` deleted; collapsed `--to-files/--undo/--to-connection` grammar pinned by typed clap matrix; leniency documented (normalize+document upheld). Follow-on #1010. |
| P4 notification Unmute | #733 | — | **CLOSED by operator ruling (a)**: stage 2 proved the journey premise false — mute hides rows by documented contract (`reminders.rs`), so the Unmute toggle on the notification surface is unreachable; the real defect lives on the banner/status surface (#1011). 262-line WIP discarded per ruling. |
| P5 ceremony identity injection | #751 | #1022 | **MERGED**, issue closed. `device_json: &Path` through `run_on/run_until/app_world`; two recorded fallback tests hermetic (one as a full commit test on a wiremock DavMemory mount); review fixed the false post-boot panic invariant into a typed `FilesSyncDisabled` guard. |
| P6 config reload & batch | #909, #910 | #1054 | **PR OPEN** (rebased onto post-chunk-8 main after a stale-branch SonarCloud fail; CI re-running). Journey gate accepted; `notify = "8"` stack addition accepted; watcher (300 ms tokio-timer debounce, snapshot-comparison self-write suppression, C10 apply-half reuse) + atomic batch set; deny allowlist carries notify's CC0-1.0 (dispatcher fix a65d5cf3-class, landed on branch). |
| P7 files GC verb | #718, #755 | #1044 | **MERGED**, issues closed. Typed idempotent DELETE verb at three tiers; post-publish owner GC pass (D-BF7 pinned); unbind retirement per the **operator's amended Option A — device-scoped only** (never the shared subtree; fail-by-construction pin). Marker alternative recorded at #1021. |
| P8 files-sync live cadence | #673 | #1058 | **PR OPEN** (CI green, awaiting merge). Connector min-rule re-arm through `reschedule`; snapshot override field; doorbell ignore-arm superseded; DR-8 boundary intact; bounded-wait harness used (the 10 ms spin hazard recorded in-report). |
| P9 build-cache evaluation | #890 | #1050 | **MERGED**; **#890 CLOSED — ruling ACCEPTED**: scheme D (per-branch target dirs). B/C/E eliminated on measured evidence (E's 0.00% sccache Rust hit rate = the upstream Rust-hash-key gap). Rollout chunk queued for the post-r9 round; revisit trigger #1052. |

**Merged-to-main this round**: #1009, #1012, #1019, #1022, #1044,
#1050. **Awaiting your merge**: #1054, #1058 (both fully gated green
locally; #1054's CI re-running post-rebase).

## 2. Operator rulings exercised

1. P4 journey gate — accepted; later the blocker ruling (a).
2. P6 journey gate — journey plan + `notify` stack addition accepted.
3. P7 leg-2 — Option A with the binding device-scope amendment.
4. P9 — scheme D accepted, #890 closed.

## 3. Issues opened this round (backlog discipline)

#1010 (date-grammar unification), #1011 (banner/status Unmute — the
real #733 defect home), #1013 (ambient bootstrap identity home),
#1014 (batch unset parity), #1015 (manual reload verb), #1020 (flaky:
`transport_failure_aborts_without_outcomes`, watchlist), #1021
(sibling-path retirement marker alternative), #1042 (doctor sweep hits
bench/measurement worktrees), #1043 (tmpfs scratch unbounded; ENOSPC
killed three bench runs), #1045 (read-only surface for ConfigReload
outcomes), #1046 (tokio advance-exactly-to-deadline gotcha for the
clock-law docs), #1051 (files_sync.poll_secs has no live-write lane),
#1052 (revisit the build-cache ruling when sccache normalizes the Rust
hash key).

## 4. Incidents & deviations (honest ledger)

- **P9 stage-2 quota-cut twice** (agent terminated mid-battery). The
  dispatch-law "resume over redo" was only partially available: the
  first agent could not be resumed (no active task), so stage 2 was
  re-dispatched with explicit salvage instructions — arms A/B/C
  commits and the D/E raw run logs in `/var/tmp/p9-bench-logs` were
  salvaged; only the §6/§7 risk probes re-ran. Recorded in the report
  §0.3.
- **Bench worktree destroyed by a doctor sweep** mid-battery (its HEAD
  was origin-main-contained → disposable by design). P9 re-dispatch
  moved bench sites to doctor-unscanned `/var/tmp`; footgun filed as
  #1042; scratch ENOSPC as #1043.
- **P6 licenses gate failure**: `notify 8.2.2` declares CC0-1.0, not in
  the deny allowlist. Dispatcher fixed on the branch (dated rationale
  entry); gate re-run green. Not a stage-3 finding — surfaced first at
  the dispatcher's §10 run.
- **Flake #1020**: `taskboard-sync-nextcloud
  transport_failure_aborts_without_outcomes` failed once in P5's §10
  run, passed solo + on the full re-run; watchlist filed; not
  attributed to the branch.
- **#1054 stale-branch SonarCloud failure**: the concurrent r9 chunk-8
  landed mid-CI, leaving the PR BEHIND with a SonarCloud fail;
  dispatcher rebased (clean) and pushed `--force-with-lease`.
- **Stage-2 honest halts**: P4 (plan-vs-world blocker, uncommitted WIP
  preserved until the ruling); P8 stage-1 (flock deferral per the
  #879/#894 rule — gate lock held by the concurrent r9 habit-editing
  agent).
- **Review-caught same-class defects pulled into scope** (per plan):
  P2's multi-select schedule-today twin + P3's rebased-in leniency
  oracle; P6's relayed-`unset` misreport through the batch lattice
  (fixed, autosquash, no duplicate-subject commits anywhere).

## 5. What opens next (per the plan §11)

- The post-r9 round: undo UI legs (#353/#354 journeys), habit backlog
  #955–#963, the #946–#949 CLI-parity remainder, the **P9 rollout
  chunk** (per-branch target dirs: AGENTS §8/§10 law amendment + the
  /var/tmp budget; ruling accepted), P6/P7/P8 follow-ups already homed
  (#1014/#1015/#1045/#1051/#1021).
- Flaky watchlist: #1020, plus the armed #846 policy.

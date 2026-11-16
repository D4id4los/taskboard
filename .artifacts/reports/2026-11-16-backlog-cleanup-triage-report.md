# Backlog Cleanup Triage Report (2026-11-16)

Scope: all 140 open issues (#113–#846), assessed for inclusion in a backlog
cleanup round. Verdicts: **clean-up** (act in the round), **keep** (healthy,
leave), **close-now** (already resolved, closure is the cleanup).

## A. Close now — already resolved, no cleanup action needed

| Issue | Evidence |
|---|---|
| #654 | Duplicate watchlist entry for the same test as #697 (`tty_ctrl_c_during_a_stalled_line_…`). AGENTS.md mandates one issue per distinct test; #697 is the fuller one (has the watchlist preamble + 2 observation comments). Close #654, fold its single observation into #697 as a comment. |
| #767 | The pilot dispatch plan (`.artifacts/plans/2026-11-13-ux-journey-pilot-dispatch-plan.md`) is now tracked in git (`git ls-files` confirms). The issue's own condition ("must be committed") is met — close as completed. |
| #842 | The r1 erratum landed on `main` (commits `be95bdad` + `1ffe9e64`, PC-1/PC-5 supersession pattern per the chunk-6 decision comment). The comment says "closes when the chunk-6 PR merges" — that has happened. Close as completed. |

## B. Include in the cleanup round

### B1. Flaky-test consolidation (5 issues → 4)
- #654 → close into #697 (see A).
- #697, #521, #846 are three *distinct* `cli_tty` Ctrl-C tests on the same
  seam (raw-mode tty read under full-suite load). Cleanup action: cross-link
  them as siblings on the shared root cause; consider one umbrella
  `tty-under-load` triage issue so the eventual dedicated flaky pass fixes
  the seam once, not three times.
- #287, #336, #384, #570, #650, #682: healthy watchlist entries — keep, no
  action beyond the link pass above.

### B2. Label/normalization hygiene
- #140 lacks a `p1`/`p2`/`p3` label (all peers have one). It is a naming
  reconciliation task — `p3` fits.
- #213, #185, #184 use category labels only, no priority. Either add `p3` or
  accept "unscheduled = no priority"; pick one convention and apply it
  uniformly (currently the corpus is inconsistent).
- #440 and #731 are the only `ci` issues; #731 additionally carries
  `security` + `p2` with **no milestone** despite *failing CI on main* (see C1).
- #759 carries a `ux` label that appears on only one other issue (#691);
  normalize to `ui` or document `ux` as distinct from `ui`.

### B3. Milestone assignment for high-priority strays
- #708 (p1, architecture) — no milestone.
- #695 (p1, architecture/sync/reliability) — no milestone.
- #731 (p2, security, CI-red on main) — no milestone.
- #133 (p2, architecture, clock-skew) — no milestone.
- #327 (p2, testing/ci, cargo-mutants cap) — no milestone.
- #751 (p2, testing, ceremony identity injection) — no milestone.
Rule check: milestones are "when a decision is made to schedule", so the
cleanup round should not invent schedules — but these six should be
explicitly confirmed as "deliberately unscheduled" (leave) or promoted to
`v0.2` (which already holds the other p1/p2 accessibility items #668/#669/#670).

### B4. Cluster reviews (consolidate or mark as epics)
- **Undo v2 epic** #353–#357: coherent, self-consistent family, all updated
  together 2026-10-26. Keep all five; optionally mark #353 (branch-picker UI)
  as blocked-by #356 (RestoreStack) with GitHub sub-issue relationships so
  ordering survives agent handoffs.
- **Retry-After family** #220/#221/#222: three distinct behaviors; keep
  separate but cross-link.
- **Settings-Files UI family** #741/#743/#745/#748/#749/#750: six sibling UX
  gaps. Keep separate (each is one verb), but group them under a shared label
  (e.g. reuse `ui` + a new `settings` category) so a future design round can
  sweep them in one pass.
- **UX-triage series** #807–#823 (T-24…T-40): already normalized
  (`[ux-triage T-n]` titles, uniform labels, uniform comment provenance).
  Not cleanup targets — they are the *output* of a triage round and belong to
  the active UX retrofit wave (the untracked
  `2026-11-14-ux-retrofit-triage-dispatch-plan.md` should itself be committed,
  which is the same class of debt #767 recorded).

### B5. Staleness sweep on the migration cohort (#113–#231, created 2026-10-17)
These ~50 issues came from the markdown-backlog migration and have had zero
activity since. Do **not** bulk-close; instead a cheap liveness pass:
- Categories where the codebase has visibly moved: #148/#141/#137/#146
  (State Engine performance) and #154/#155/#157 (sync performance) — verify
  the premise still holds before they mislead a future agent.
- Dated re-evaluation reminders #428 (Prusti ~2027 Q2) and #429 (quarterly
  domain-churn re-measure): keep — they are timers, not debt.
- Audit registers #274/#277/#284/#285: keep (registers are living documents,
  last touched 2026-11-05).
- Candidates for genuine staleness closure if the premise is dead after
  review: #114 (cargo-dist "≥ 1.0.0"), #115 (second-priority release
  targets), #113 (SonarQube/Snyk) — third-party-tooling wishes with no
  recent signal; confirm intent or close.

## C. Escalate out of the backlog (not cleanup material)

1. **#731 — cargo-deny advisories fail on main (CVE under sqlx).** A red
   required gate is never backlog; per §8 it must be diagnosed and fixed on
   a branch. Recommend pulling this out of the queue into immediate work
   (or an explicit accepted-risk entry in #284 if unfixable upstream).
2. **#698 — stray merge-conflict markers in CHANGELOG.md.** Trivial doc fix;
   candidate to just fix in the next docs PR rather than sit in the queue.
3. **#782 — `// slint:` comment seams for ux-coverage.sh.** Small, blocks the
   coverage gate's callback checks for pre-framework adapters; good fit for
   the current UX framework rollout, not for the unscheduled queue.

## D. Verdict summary

- **Close now (3):** #654 (dup of #697), #767 (file committed), #842 (erratum landed).
- **Cleanup-round actions:** cross-link tty-flaky siblings; label
  normalization (#140, #213, #185, #184, #759); milestone confirmation for
  #708/#695/#731/#133/#327/#751; epic/sub-issue wiring (#353–#357,
  settings-Files family); liveness pass over the 2026-10-17 migration cohort
  with three probable staleness closures (#113/#114/#115 pending intent check).
- **Escalate:** #731 (CI red — immediate), #698 (fix opportunistically),
  #782 (attach to active UX wave).
- **Untouched (healthy):** everything else, notably the UX-triage series
  #807–#823, the audit registers, the dated re-evaluation timers, and the
  v0.2-milestoned items.

## Executed actions (2026-11-16)

Stage-2 execution log (worktree `taskboard-WT-chore-backlog-cleanup-round`,
branch `chore/backlog-cleanup-round`). Every mutation was idempotence-gated
with `gh issue view <n> --json state,labels,milestone` first. No A-item was
skipped as already-satisfied; one item (A15) resolved vacuously — see its
row. Comment texts follow the stage-1 plan verbatim unless noted.

| Item | Outcome | Evidence |
|---|---|---|
| A1 | done | `gh issue view 654` OPEN → fold comment on #697 ([comment](https://github.com/D4id4los/taskboard/issues/697#issuecomment-6915719768)) → closed `duplicate of #697`; `gh issue view 654` → CLOSED/DUPLICATE |
| A2 | done | gate: `git ls-files --error-unmatch .artifacts/plans/2026-11-13-ux-journey-pilot-dispatch-plan.md` succeeded; landing commit `f70a9e6a` → #767 closed COMPLETED with that citation |
| A3 | done (plan-text deviation) | `1ffe9e64` verified in branch history; `be95bdad` turned out to be a duplicate copy of the same erratum commit existing only on side branch `docs/ux-r6-p3-posture` — the closure comment cites `1ffe9e64` (the on-`main` landing) and notes the SHA discrepancy; #842 closed COMPLETED |
| A4 | done | D4 jq filter re-run live; pre-sweep hits exactly six: `843 backlog: investigate journey_state…`, `776 backlog: dead CLI verb boards use…`, `763 backlog: route()'s take-all…`, `762 backlog: the Dropdown widget…`, `607 backlog: load-reproduction scripts…`, `140 backlog: Reconcile storage naming…`; `p3` added to all six (post-view: each label set now carries `p3`); #213/#185/#184 already carried `p3` and were not hits; no non-`backlog:` issue touched |
| A5 | done | pre-check `gh issue list --label ux --state open` returned exactly 759, 691; both swapped to `ui` and commented (#759 post labels `p3,ui`; #691 post labels `p3,architecture,ui`); `gh label delete ux --yes` succeeded; post-check `index("ux")` → gone |
| A6 | done/no-override | D1: no operator override list → all six (#708 #695 #731 #133 #327 #751) gate-checked OPEN with `milestone: none`, then commented deliberately-unscheduled confirmation; #731's comment carries the triage §C escalation note; none milestoned |
| A7 | done | cross-link comments on #697 and #521 (D5 scope); #846 received the D5 decline comment (renderer-state golden, not the tty-Ctrl-C seam); no umbrella issue opened |
| A8 | done | task-list ordering comment on #356 (356→355→354→353; #357 out-of-line); pointer comments on #355/#354/#353/#357 |
| A9 | done | `gh label create settings-files` succeeded; applied to exactly #741/#743/#745/#748/#749/#750 (post-views confirm); directory comment on #750 |
| A10 | done | seven premise-holds comments with file:line evidence, all read in this worktree at this branch's HEAD: #148 (core.rs:129-152, core.rs:86-91, actor.rs:919), #141 (actor.rs:489), #137 (connect.rs:36), #146 (actor.rs:960-964 `forward_sync_now`), #154 (persistence.rs:152), #155 (actor.rs:557-565; layout drift `push_exec.rs` → `push_exec/` noted in the comment), #157 (actor.rs:1295-1352 detail-refresh loop). No closures — no premise was dead |
| A11 | done/user-gated | intent comments on #113/#114/#115 with their per-issue extra clauses; all three gate-checked OPEN before and remain open (never closed by this round) |
| A12 | done | diff3 markers `^\|{7}` at CHANGELOG.md:352 and 371 re-located live, both deleted; surrounding entry inspected for duplicated half-entries — none found (see the commit); standalone commit `docs(changelog): remove stray merge-conflict marker lines` |
| A13 | done | `.artifacts/plans/2026-11-14-ux-retrofit-triage-dispatch-plan.md` copied from the operator's working tree, `cmp` verified byte-identical before committing, committed verbatim |
| A14 | done | this triage report and `2026-11-16-backlog-cleanup-round-dispatch-plan.md` committed (this log is the report's only edit) |
| A15 | skipped-target-gone | D2 had verified the stray `.artifacts/reports/#2026-11-14-ux-retrofit-triage-report.md#` byte-identical (30731 bytes) to the tracked report at stage-1 time; at execution time the file no longer exists anywhere (`find` over the main checkout and the worktree returns no `*#*` report backup; `git ls-files | grep '#'` is empty) — nothing to delete, no `git rm` performed |
| A16 | done | this table |

**Errata against this report's body (recorded here, body unedited per
A14's scope):**

- §B1 called #697/#521/#846 "three distinct cli_tty Ctrl-C tests on the
  same seam"; the live #846 title is
  `flaky-test: golden_visuals the_kiosk_board_golden_holds …` — a
  renderer-state golden, not a tty Ctrl-C test. A7 followed the stage-1
  plan's D5 decision: #697 ↔ #521 cross-linked; #846 explicitly
  declined the link. No umbrella issue was opened for a family that
  does not exist; the dedicated flaky pass over the `flaky-test` label
  remains the tracker (stage-1 plan D5).
- §A3's cited commit pair: only `1ffe9e64` is on `main`; `be95bdad` is
  the same erratum re-committed on the `docs/ux-r6-p3-posture` side
  branch (see the A3 row).

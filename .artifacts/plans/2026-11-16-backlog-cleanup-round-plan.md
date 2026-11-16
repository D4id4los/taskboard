# Backlog Cleanup Round — Stage-1 Execution Plan (2026-11-16)

- **Origin**: dispatch plan
  `.artifacts/plans/2026-11-16-backlog-cleanup-round-dispatch-plan.md`
  §1/§2 (action catalogue A1–A16, binding); triage report
  `.artifacts/reports/2026-11-16-backlog-cleanup-triage-report.md`
  §A/§B (rationale).
- **Branch / worktree**: `chore/backlog-cleanup-round` in
  `../taskboard-WT-chore-backlog-cleanup-round`, created off local
  `main` @ `5c3c98d9` after a green `scripts/worktree-doctor.sh`
  (153 G free, floor 25 G; two KEEP'd local-only worktrees and one
  WARN'd unregistered dir left alone, per doctor output).
- **Live-state verification (2026-11-16)**: all 41 catalogue issues
  viewed with
  `gh issue view <n> --json number,state,title,labels,milestone`.
  **All are OPEN**; none was closed or already satisfied by someone
  else. **No A-item is dropped.** Deviations from the catalogue's
  planning-time assumptions are recorded per item below.
- **Stage-2 order**: A1–A11 GitHub ops → A16 execution log →
  A12–A15 commits (per dispatch plan §1 shared execution model).

## Resolved decisions

- **D1 — milestone override list (A6)**: the operator passed **no
  promotion list at dispatch time** → default applies: all six issues
  (#708, #695, #731, #133, #327, #751) get the unscheduled-confirmation
  comment; **no** `--milestone` edit. (Dispatch plan §3 confirms:
  milestone promotions only per override; there was none.)
- **D2 — A15 deletion condition**: RESOLVED **HOLD (delete)**.
  `diff '.artifacts/reports/#2026-11-14-ux-retrofit-triage-report.md#'
  .artifacts/reports/2026-11-14-ux-retrofit-triage-report.md` exits 0 —
  the stray file is **byte-identical** (30731 bytes) to the tracked
  report. It is an editor backup with zero unique content → stage 2
  deletes it inside the artefacts commit.
- **D3 — A12 marker pattern**: the #698 markers are **diff3-style
  `|||||||` lines**, not `<<<<<<<`/`=======`/`>>>>>>>`. Verified live:
  `grep -nE '^\|{7}' CHANGELOG.md` → lines **352** and **371**
  (`||||||| parent of bc6b2e6b …` / `||||||| parent of 12bff3cd …`).
  (The issue body's "lines 218 and 237" have drifted; line numbers in
  the closure comment must cite 352/371 as of this plan's commit.)
  The dispatch-plan acceptance grep misses this pattern; stage 3 must
  additionally grep `^\|{7}`.
- **D4 — A4 sweep filter**: the catalogue's prose jq ("missing any
  `p*` label") is sharpened to exact priority names — a naive
  `startswith("p")` would wrongly treat `performance` as a priority.
  Binding filter (dry-run executed 2026-11-16, output below):
  ```jq
  .[] | select((.title | startswith("backlog:"))
        and (([.labels[].name] | any(. == "p1" or . == "p2" or . == "p3")) | not))
      | "\(.number) \(.title)"
  ```
  Dry-run hits (exactly six): **#843, #776, #763, #762, #607, #140**.
  The catalogue's planning-time guesses #213/#185/#184 now already
  carry `p3` (verified live) — the filter excludes them; they are
  skipped-and-logged, not re-labelled. #776/#763/#762 are new hits
  beyond the planning-time list — exactly why the catalogue mandates
  the mechanical sweep.
- **D5 — A7 family membership**: triage §B1 calls #697/#521/#846
  "three distinct `cli_tty` Ctrl-C tests on the same seam". Live
  titles contradict this for **#846**: it is
  `flaky-test: golden_visuals the_kiosk_board_golden_holds …`
  (a renderer-state golden, not a raw-mode tty Ctrl-C test).
  Decision: A7 cross-links **#697 ↔ #521** as the tty-Ctrl-C seam
  siblings (same root cause, same fix-once mandate); #846 receives a
  comment that **declines** the tty-seam link, names its observed
  difference (renderer-state dependence, passes-in-full-suite
  fails-alone — the inverse failure mode), and notes it should be
  triaged by the dedicated flaky pass on its own merits. No umbrella
  issue is opened for a family that does not exist; if the flaky
  pass later finds a shared renderer seam, that pass opens the
  umbrella (recorded as an out-of-scope note in the execution log,
  not a new `backlog:` issue — the watchlist is already the tracker).
- **D6 — A10 premise verification depth**: stage 2 verifies each of
  the seven premises by reading the named seams in the current actor
  code and cites `file:line`. Closure only on verifiable in-repo
  evidence that the premise is dead; anything uncertain stays open
  with a "premise re-verified: holds" comment. Expected: most hold
  (they are forward-looking performance wishes); the comment text
  template below handles both outcomes.
- **D7 — comment authorship**: every comment is left as the
  authenticated `gh` user; no `--author` spoofing; no issue body
  edits — comments and labels only (plus the three closures).

## A-item command sequences & comment texts

Idempotence gate before every mutation (dispatch plan §1.2):
`gh issue view <n> --json state,labels,milestone` — abort that item
and log a skip if the observed state already satisfies it or the
issue is closed.

### A1 — Close #654 into #697

```sh
gh issue view 654 --json state --jq .state          # gate: must be OPEN
gh issue comment 697 --body-file /tmp/a1-fold.md    # fold observation
gh issue close 654 --reason NOT_PLANNED \
  --comment "$(cat /tmp/a1-close.md)"
```

`/tmp/a1-fold.md` (on #697):

> Folding in the single observation from the duplicate entry #654
> (per the one-issue-per-distinct-test rule, AGENTS §7): one failure
> of `tty_ctrl_c_during_a_stalled_line_cancels_and_the_next_line_executes`
> under full-suite load, observed 2026-11-05. #654 is closed as a
> duplicate of this issue; this issue remains the canonical watchlist
> entry for the test.

`/tmp/a1-close.md` (on #654):

> Closing as a duplicate: this is the same test as the canonical
> watchlist entry #697 (`tty_ctrl_c_during_a_stalled_line_cancels_and_the_next_line_executes`,
> also failing under full-suite load on the raw-mode tty-read seam).
> AGENTS §7 mandates one issue per distinct test; #697 is the fuller
> entry (watchlist preamble + observations) and now also carries this
> issue's 2026-11-05 observation, folded in as a comment.

### A2 — Close #767 (plan is tracked)

Gate: `git ls-files --error-unmatch .artifacts/plans/2026-11-13-ux-journey-pilot-dispatch-plan.md`
in the worktree must succeed; capture the landing commit with
`git log --oneline --diff-filter=A -- <path>`.

```sh
gh issue close 767 --reason COMPLETED --comment "$(cat /tmp/a2.md)"
```

> Closing as completed — the issue's own closure condition ("must be
> committed") is met: `git ls-files` shows
> `.artifacts/plans/2026-11-13-ux-journey-pilot-dispatch-plan.md` is
> tracked on `main` (added in `<commit>`). The file is no longer
> local-only/untracked.

(stage 2 substitutes the actual landing commit hash)

### A3 — Close #842 (erratum landed)

Gate: `git log --oneline | grep -E 'be95bdad|1ffe9e64'` — both
commits present in the branch's history (they are on `main`).

```sh
gh issue close 842 --reason COMPLETED --comment "$(cat /tmp/a3.md)"
```

> Closing as completed: the r1 erratum this issue asked for has
> landed on `main` — `be95bdad` + `1ffe9e64` ("docs(plan): the r1
> undo-claim erratum and the gate-debt ruling home") apply the
> PC-1/PC-5 supersession pattern per the chunk-6 decision comment.
> That comment's closure condition ("closes when the chunk-6 PR
> merges") is satisfied: the chunk-6 work is merged and the erratum
> text is on `main`.

### A4 — Priority-label sweep (`p3` to each hit)

```sh
gh issue list --state open --limit 1000 --json number,title,labels \
  --jq '<D6 filter>' | tee sweep-output.txt        # evidence for the log
for n in $(cut -d' ' -f1 sweep-output.txt); do
  gh issue view $n --json labels --jq '[.labels[].name]'   # idempotence gate
  gh issue edit $n --add-label p3
done
```

Expected hits at planning time: #843, #776, #763, #762, #607, #140.
Stage 2 re-runs the sweep and acts on **its own** output; every hit
gets `p3`; every non-hit is untouched. `sweep-output.txt` (the
pre-state and post-state runs) is excerpted into the execution log.

### A5 — `ux` → `ui` on #759 and #691, then label delete

```sh
gh issue edit 759 --remove-label ux --add-label ui
gh issue edit 691 --remove-label ux --add-label ui
gh label list --json name --jq '[.[].name] | index("ux")'   # post-check
gh label delete ux --yes   # only if no open issue carries ux
```

Comment on each (identical text, adapted issue number):

> Label normalization from the 2026-11-16 backlog cleanup round:
> replacing `ux` with `ui`. The `ux` label appears on only two issues
> corpus-wide and is not a maintained category in this repository's
> label set (`ui` is); no semantic change to the issue's scope.

Pre-check: `gh issue list --label ux --state open --json number --jq '.[].number'`
must return exactly `759` and `691` before deleting the label.

### A6 — Milestone-confirmation comments (D1: no override)

On each of #708, #695, #731, #133, #327, #751:

```sh
gh issue comment <n> --body "$(cat /tmp/a6.md)"
```

> Milestone confirmation from the 2026-11-16 backlog cleanup round:
> this issue is deliberately confirmed **unscheduled** — it stays in
> the unscheduled queue (AGENTS §7: milestones are assigned only when
> a scheduling decision is made). Promotion to a milestone (e.g.
> `v0.2`, which holds the other p1/p2 items) happens when that
> decision is made, not by this cleanup round.

(For #731, stage 2 may append one sentence: "Note: per triage §C the
CI-red cargo-deny state is escalated as immediate separate work,
outside this round." — the escalation itself stays out of scope.)

### A7 — tty-flaky seam cross-links (D5 scope: #697 ↔ #521; #846 declines)

On #697:

> Sibling seam note (2026-11-16 backlog cleanup): #521
> (`tty_ctrl_c_during_the_masked_read_fails_typed_and_the_session_survives`)
> fails on the same raw-mode tty-read seam under full-suite load.
> The eventual dedicated flaky triage pass over the `flaky-test`
> label should fix this seam once for both tests, not twice.

On #521: same text with #697's test name and number swapped.

On #846 (D5 decision text):

> Cleanup-round note (2026-11-16): triage initially grouped this
> issue with the cli_tty Ctrl-C flaky family (#697, #521), but the
> live title shows a different seam — this is a
> `golden_visuals the_kiosk_board_golden_holds` renderer-state
> dependence with the inverse failure mode (passes in full-suite
> runs, fails alone). It is **not** linked to the tty-Ctrl-C family;
> the dedicated flaky triage pass over the `flaky-test` label should
> triage it on its own merits.

### A8 — Undo-v2 ordering wiring

Tracking task-list comment on #356:

> Undo-v2 family ordering (2026-11-16 backlog cleanup). Foundation
> first; the UI consumes the journaled tree only after restore
> exists:
>
> - [ ] #356 — RestoreStack + cascaded-task restore (this issue; the
>       foundation)
> - [ ] #355 — per-board undo line scoping
> - [ ] #354 — reminder-surface undo (D-R1 verbs)
> - [ ] #353 — undo branch-picker UI over the journaled tree
>
> #357 (undo/recovery revisit for board-level catastrophic verbs) is
> exploratory and deliberately out of line — it neither blocks nor
> consumes the chain above.

Pointer comment on each of #355, #354, #353, #357:

> Part of the undo-v2 family — ordering is tracked in #356's
> task-list comment (2026-11-16 backlog cleanup round).

### A9 — settings-Files family grouping

```sh
gh label create settings-files --color 5319e7 \
  --description "settings/Files-card UX gap family (one design-round sweep)"
for n in 741 743 745 748 749 750; do gh issue edit $n --add-label settings-files; done
gh issue comment 750 --body "$(cat /tmp/a9.md)"
```

Directory comment on #750:

> Settings-Files UX gap family directory (2026-11-16 backlog
> cleanup; label `settings-files` applied to all six so a future
> design round sweeps them in one pass):
>
> - #741 — settings mount auto-list has no failure backoff
> - #743 — per-key env-lock display on the settings Files row
> - #745 — settings publish busy window per-row progress phase
> - #748 — expose the move ceremony's undo window (boards move
>   --undo has no GUI)
> - #749 — no disconnect/unbind action for files-bound board rows
> - #750 — this issue: per-board files adoption state on the Files
>   card (largest scope; hence this directory comment)

### A10 — Liveness pass over the migration cohort

For each of #148, #141, #137, #146, #154, #155, #157: read the named
seam in the current code, then one comment per issue:

> Premise re-verified 2026-11-16 (backlog cleanup liveness pass):
> holds — `<evidence: file:line of the seam and what it shows>`.

or, only with verifiable in-repo evidence:

> Premise dead (2026-11-16 liveness pass): `<evidence>` — closing as
> completed.

Seams to inspect (triage §B5): the State Engine's clone-per-message
path (#148, #141), read-pool / concurrent load handling (#137),
engine-side sync coalescing / debounced nudging (#146), the sync
pull/push pipeline validators (#154, #155), the per-card refresh loop
(#157). Doubt means leave open (dispatch plan §2 A10).

### A11 — User-gated intent comments (#113, #114, #115 — never closed)

On each, tailored body:

> Intent check from the 2026-11-16 backlog cleanup round (migration
> cohort staleness sweep): this is a third-party-tooling wish with no
> recent signal since the 2026-10-17 backlog migration. Please
> confirm you still want this (and it should stay open) or approve
> closure — this round asks and never closes. If confirming, a
> one-line "still wanted" reply is enough.

(#114 gets the extra clause: "the `≥ 1.0.0` premise in the title is
part of what to confirm"; #115: "confirm whether the second-priority
target list is still the intended one".)

### A16 — Execution log

Append to the triage report (in the worktree):

```markdown
## Executed actions (2026-11-16)

| Item | Outcome | Evidence |
|---|---|---|
| A1 | done | `gh issue view 654` → closed NOT_PLANNED; fold comment on #697 |
| A2 | done | `git ls-files …` + `gh issue view 767` → closed COMPLETED |
| A3 | done | `be95bdad`+`1ffe9e64` in history; `gh issue view 842` → closed COMPLETED |
| A4 | done | jq sweep pre/post output excerpted below; N hits → `p3` |
| A5 | done | `gh issue view 759/691` labels show `ui`; `ux` deleted (zero open issues) |
| A6 | done/no-override | confirmation comments on the six; none milestoned |
| A7 | done | cross-links #697↔#521; #846 declined per D5 |
| A8 | done | task list on #356; pointer comments ×4 |
| A9 | done | `settings-files` created; applied ×6; directory on #750 |
| A10 | done | 7 comments with file:line; closures if any cited |
| A11 | done/user-gated | 3 intent questions; all three still open |
| A12 | done | diff3 markers at lines 352/371 removed; commit `docs(changelog): …` |
| A13 | done | retrofit dispatch plan committed verbatim |
| A14 | done | triage report + dispatch plan committed |
| A15 | done/delete | byte-identical diff (D2) → stray file deleted |
| A16 | done | this table |
```
(one line per A-item with the real outcome; skipped items get
`skipped-because-already-satisfied` + the gate command that showed it)

### A12 — CHANGELOG fix (repo-side)

```sh
grep -nE '^\|{7}' CHANGELOG.md          # re-locate first (numbers drift)
# delete both marker lines (delete the larger line number first, or
# re-grep after each deletion); Edit-tool deletion is fine too
grep -nE '^\|{7}|^<{7}|^={7}|^>{7}' CHANGELOG.md   # must be clean
```

Commit: `docs(changelog): remove stray merge-conflict marker lines`.
Stage 2 first inspects the surrounding #622 entry text around the two
markers for duplicated halves (the issue body's instruction) and
removes any duplicated half-entries the same way. D3's diff3 pattern
is the acceptance grep, not the dispatch plan's `<<<<<<<` pattern.

### A13/A14/A15 — artefact commits (repo-side)

Single commit `docs(artifacts): land the backlog cleanup round artefacts`:
- `git add .artifacts/plans/2026-11-14-ux-retrofit-triage-dispatch-plan.md`
  (verbatim — byte-identical to the operator's working-tree copy;
  stage 3 checks this per dispatch plan §2 trap (d)),
- the triage report + this dispatch plan,
- the A16 execution-log edit,
- `git rm '.artifacts/reports/#2026-11-14-ux-retrofit-triage-report.md#'`
  (D2: deletion condition held).

Stage 2 smoke gate: `cargo fmt --all -- --check` (per prompt 1.2);
full §10 suite is stage 3's job through `scripts/run-quality-gates.sh`.

## Risks

1. **Issue-state races**: issues can move between stage 1's
   verification and stage 2's execution; the per-mutation
   idempotence gate + execution-log skips absorb this.
2. **A10 evidence quality**: a lazy "holds" comment is worse than
   none; stage 2 must cite file:line it actually read. Doubt → open.
3. **A12 line-number drift**: lines 352/371 are valid at this plan's
   commit only; stage 2 re-greps before deleting, and must inspect
   the surrounding entry for the duplicated-halves defect the issue
   body warns about.
4. **A9 label colour**: `--color 5319e7` is a placeholder; any
   colour is acceptable (the corpus does not enforce a palette).
5. **#846 membership error (D5)**: the triage report's §B1 claim is
   factually wrong about #846; this plan's declined-link comment is
   the corrective, but the triage report itself is committed as-is
   (A14 forbids editing origin docs beyond A16's log) — the execution
   log notes the erratum.
6. **Worktree created off local `main` (`5c3c98d9`)**, which is
   ahead of origin at snapshot time; stage 3's mandatory rebase onto
   `origin/main` reconciles any divergence before shipping.

## Estimated diff size

Four text files touched, no code: CHANGELOG.md (−2 lines, possibly a
few more if duplicated half-entries are confirmed), triage report
(+~25 log lines), plus two new files (this plan ~250 lines; the
retrofit dispatch plan committed verbatim, as on disk) and one
deletion (the stray 30731-byte backup). Net PR diff: small
docs-only, M-sized chunk per the dispatch plan (the bulk of the work
is GitHub-side and diffless).

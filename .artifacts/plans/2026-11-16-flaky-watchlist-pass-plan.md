# [Plan] The Flaky-Watchlist Dedicated Pass — Chunk C of the Correctness-Hygiene Round

- **Date**: 2026-11-16
- **Kind**: chunk implementation plan — the sanctioned dedicated pass over
  the `flaky-test` label query (AGENTS §7), chunk C of the
  correctness-hygiene round. Refines round plan
  `.artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md` §4
  (normative) and dispatch plan
  `.artifacts/plans/2026-11-16-correctness-hygiene-dispatch-plan.md` §1/§4
  into per-issue dispositions and exact changes.
- **Status**: READY for stage 2.
- **Branch/worktree**: `test/flaky-watchlist-pass-v2` @
  `../taskboard-WT-test-flaky-watchlist-pass-v2`, branched off
  `origin/main` @ `dfdf2562`. **Naming decision (D0)**: the prompt named
  `test/flaky-watchlist-pass`, but that branch's PR (#479, the
  2026-10-26 pass) already merged and its remote was deleted — AGENTS §8
  forbids reusing a landed branch name, so the chunk runs on
  `-v2` per the recorded law.
- **Prior passes folded into this one** (read in full at plan day):
  2026-10-25 pass (PR #479, merged), the pty flake pass
  (`.artifacts/plans/2026-10-31-pty-flake-pass-plan.md`, report
  `2026-11-01-pty-flake-pass-stage2-report.md`, seam commit `a5c237c8`
  on `main`), and the November watchlist pass
  (`.artifacts/plans/2026-11-03-watchlist-pass-plan.md`, report
  `2026-11-04-watchlist-pass-stage2-report.md`, merged). This chunk
  builds on their landed state — it never redoes a protocol whose
  bounded negative already stands, and it executes the closures those
  passes left drafted but unexecuted.

## 0. Plan-day correction to the round plan (recorded, binding for this chunk)

The round plan §4 groups "#697 + #521 + #846" as one `cli_tty` Ctrl-C
family. **#846 is not a tty test**: it is
`taskboard-ui-slint` `golden_visuals::the_kiosk_board_golden_holds`
(renderer-state dependence, single 2026-11-15 observation). The three
tests on the tty seam are #697's `tty_ctrl_c_during_a_stalled_line…`
(with closed sibling #654 folded into it, as the round plan's own
prompt notes) and #521's
`tty_ctrl_c_during_the_masked_read_fails_typed…`. Consequence:
#846 gets its own disposition (§4.5) and is **not** fixed in this
chunk — `taskboard-ui-slint` is round-8-owned excluded territory
(round plan §1). This correction is recorded here and cited in the
stage-3 PR body; it is a classification erratum, not a scope change.

## 1. The tty seam — diagnosis (D1)

### 1.1 What the seam is

The pty flake pass (2026-11-01, commit `a5c237c8`) rebuilt the whole
`cli_tty` harness: one reader thread per pty session owns the output
pipe and announces `PtyProgress::{Bytes, Eof}`; every wait is
`predicate ‖ progress ‖ idle-timeout` with
`PTY_IDLE_BUDGET = max(30 s, 3 × G_max)` (G_max = 10 s, the designed
silent window of the stalled-line leg's delayed wiremock route) and
`PTY_SESSION_BACKSTOP = 186 s = max(120 s, 3 × T_max)` (T_max = 62 s,
the CI-recorded legitimate total). Budgets are derived by formula,
never nudged; re-derivations use fresh
`TASKBOARD_PTY_TEST_GAP_LOG` telemetry, never tuning.

### 1.2 Root-cause verdict per issue

- **#521** (outer-60 s timeout under full-suite load, 2026-10-27):
  **test determinism, already fixed**. The old harness's only bound was
  the external `timeout 60`; a loaded machine makes legitimate sessions
  slow (62 s CI-recorded), the wrapper killed live sessions, EOF landed
  in unbounded blocking reads, and the test failed on assertions named
  "wedged". Slow-with-progress was structurally conflated with
  wedged-silent. The seam removes the conflation. The pty pass drafted
  the close-as-fixed disposition (stage-2 report §8) but **it was never
  posted — the issue sits open, fixed on `main` since 2026-11-01**.
  This chunk executes the closure.
- **#697 + #654** (`tty_ctrl_c_during_a_stalled_line…`): two
  observations. #654 (2026-11-05, 42 s runtime failure under load
  17–24, passed isolated at 10.1 s) **post-dates the seam landing** —
  it is the seam's one residual observation. Mechanism uncaptured (no
  failure output recorded; the 42 s shape is consistent with ~12 s of
  legitimate session plus one `PTY_IDLE_BUDGET` idle abort, i.e. the
  session produced no output chunk for 30 s mid-test — but that is a
  hypothesis, not evidence). **Which side is broken: undetermined.**
  The two candidate sides:
  - (a) *test determinism residual*: under load, a live child can be
    starved past 30 s without emitting a chunk — the idle budget
    conflates "silent wedge" with "starved so long no chunk arrives",
    and its thin headroom (exactly 3× the designed 10 s window) is
    recorded as a finding in the pty pass itself. Fix side: re-derive
    the budget from fresh gap telemetry under the recorded load class
    (the derivation formula sanctions this; it is not a tuned bump),
    and/or treat child-process liveness as progress.
  - (b) *production race the test correctly catches*: the #572 family
    (editor never re-arms after ^C) — but #572 is CLOSED as fixed, so
    an uncaptured 30 s silence after this fix would be a new
    production occurrence. Evidence decides; no speculative verdict.
  **Decision (D1)**: this chunk runs the bounded loaded repro with
  telemetry (§4.1) and rules from its output. Both sides are in-scope
  if the evidence lands: (a) is a budget re-derivation per the
  recorded formula, (b) is a new production issue (never fixed inside
  this pass's scope creep — it would be filed, not fixed, unless the
  diagnosis is unambiguous and the fix is confined to the diagnosed
  seam).

### 1.3 The no-weakening list for the tty tier (byte-identical)

- `PTY_IDLE_BUDGET` and `PTY_SESSION_BACKSTOP` may only change as a
  formula re-derivation with recorded fresh telemetry (comment block
  updated with the new G_max/T_max evidence); never a bare constant
  bump.
- Every `assert_ne!(status.code(), Some(124))` wedge pin, every
  `assert_eq!(status.code(), Some(0))` survival pin, the typed-kind
  assertions (`prompt`/`cancelled` envelopes; "the only permitted
  failure is the typed cancellation"), the "the line after the stalled
  one executed" pin, and the config-untouched pins stay as written.
- Marker/prompt strings stay synchronization gates, never asserted as
  output content (the seam's own law).
- No `std::thread::sleep`, `tokio::time::sleep`, or relaxed timeout
  may enter `cli_tty.rs` (the two pre-existing 50 ms/500 ms sleeps in
  unrelated daemon-readiness polls predate this family and are not
  this chunk's scope; touching them is a finding unless the tty
  protocol itself is converted).

## 2. Disposition matrix (every open question resolved)

| Issue | Test | Round-plan slot | Disposition | Side to fix | Code change in this chunk |
|---|---|---|---|---|---|
| #521 | `tty_ctrl_c_during_the_masked_read_fails_typed_and_the_session_survives` | tty family | **close as fixed** (seam landed `a5c237c8`; execute the drafted closure with fresh validation evidence) | test (done) | none |
| #697 (+#654 folded) | `tty_ctrl_c_during_a_stalled_line_cancels_and_the_next_line_executes` | tty family | **bounded loaded repro with gap telemetry** → (a) budget re-derivation + close, or (b) new production issue + stays open, or (c) bounded negative + stays open with refreshed watchlist | evidence decides (D1) | 0–30 lines (budget constants + derivation comments only) |
| #682 | `merge::duplicate_reconcile_tests::the_oldest_member_survives_and_a_tie_keeps_the_bound_card` | domain proptest | **fix now** — premise-corrected generator (D3) | test premise (strategy), production pinned not changed | ~40 lines domain |
| #846 | `golden_visuals::the_kiosk_board_golden_holds` | (erratum — not tty, §0) | **document + hypothesis, stays open** (`taskboard-ui-slint` = round-8 excluded territory; dated watchlist comment) | n/a this chunk | none |
| #287 | `actor_wiremock` family | sync wiremock | **recurrence scan + capture policy unchanged, stays open** (D4; two passes' bounded negatives stand) | n/a unless reproduced | none |
| #336 | `push_exec::tests::delete_404_re_resolves_and_deletes_at_the_fresh_binding` | sync wiremock | **one bounded loaded repro this pass** (the 2026-10-26 protocol has not been re-run since; §4.3), else stays open with capture policy | test determinism if reproduced (`.expect(1)` mock sequencing) | none unless reproduced |
| #384 | `l13_alias_happy_path_writes_stderr_connect_keeps_it_empty` | cli_login | **recurrence scan, stays open** (capture policy landed `60f8965a`; no speculative fix — the #384 precedent) | n/a unless reproduced | none |
| #570 | `cli_ipc t16_track_and_export_over_the_relay` | cli_ipc relay | **recurrence scan, stays open** (Nov pass's 30/30 loaded bounded negative + standing capture policy stand; `try_connect` transient-refusal hypothesis recorded, unfixed by design) | n/a unless reproduced | none |
| #650 | `appdata::tests::no_local_ids_reach_the_dismissals_wire` | domain appdata | **time-boxed local repro attempt, then documented hypothesis, stays open** (D5) | n/a this chunk (backlog #857 opened for the substrate cause) | none |

**Closure set for the dispatcher at merge**: #521 (fixed), #697
(only if repro outcome (a) — budget re-derivation landed; else stays
open with a dated comment), #682 (fixed), plus the closed-already
fold-in noted on #697's thread (#654 stays closed). All other issues
stay open with dated comments.

## 3. #682 — the merge tie proptest (fix now, D2/D3)

### 3.1 Plan-day diagnosis (re-anchored on `origin/main` @ `dfdf2562`)

Test: `crates/taskboard-domain/src/merge.rs:5973`. Generator draws
`s1, s2` (timestamps) and `b1, b2 in 1_u64..1_000_000`
**independently**. Production `collapse_for_task` (merge.rs:1660)
already has a deterministic tie-break — `(stamp, !is_bound)`, bound
sorts first on an exact stamp tie (DEC-5). The observed failing seed
(`s1 == s2, b1 == b2`) therefore does **not** show tie-break
nondeterminism; it shows the generator producing a **degenerate
group**: candidate ref == bound ref. In that shape
`doomed` (members with `ref != survivor`) is **empty**, so
`prop_assert_eq!(collapse.doomed.len(), 1)` fails — and it fails for
*any* stamp pair whenever `b1 == b2` (collision probability ≈ 2.6e-4
per 256-case run), not only on the tie. The round plan §4's working
hypothesis ("tie-break is not deterministic") is **corrected by the
plan-day code read**: the tie-break is deterministic; the premise
(a duplicate group has two distinct members) is violated by the
generator. The issue body itself offers both readings ("forbid the
collision in the strategy or define the expected outcome").

### 3.2 The fix (D3 — recorded decision)

**Fix the property premise in the strategy; the assertion stays
byte-identical; no seed exclusion.** Concretely:

1. Draw `b2` as a *distinct* member ref via the standard shift:
   `let cand_ref = if b2 >= b1 { b2 + 1 } else { b2 };` (uniform over
   `1..=1_000_000 \ {b1}` — distribution-preserving, not a
   special-case of the failing seed; the strategy now encodes the
   corrected premise "a collapse group's members are distinct cards").
   This is the round plan's own law honored: never a seed exclusion —
   the *shape* is excluded at the premise level, so no future
   collision of any values can flake the property.
2. The property's three assertions
   (`survivor`/`rebind`/`doomed.len()`) survive byte-identical.
3. The failing seed joins
   `crates/taskboard-domain/proptest-regressions/merge.txt` (existing
   format; the seed documents the discovered shape even though the
   corrected generator can no longer draw it — it stays as the
   regression record of the original failure, red on the old
   generator's tree, green on the fixed one).
4. **New unit test** pinning the degenerate same-ref shape's
   production behavior as it stands today
   (`candidate ref == bound ref` → `doomed` empty, survivor by stamp,
   `rebind` per `survivor_is_bound`), so production semantics for the
   shape are characterized, not silently left to the proptest's luck.
   The out-of-scope production question (a same-ref older candidate
   rewinds the binding via the rebind path) is filed as **backlog
   #856** — not fixed here (no behavior change beyond the diagnosed
   seam).

**Acceptance**: the persisted seed case passes; the property passes
unweakened with the corrected generator; the new unit test is green;
existing merge corpus unchanged; no other merge behavior moves.

## 4. Work order for stage 2 (in order; every commit standalone)

### 4.1 The tty leg (#521 closure + #697 repro)

1. Loaded repro protocol (binding, adapted from the #570 Nov-pass
   protocol and the pty pass's load generator): one-at-a-time
   iterations of
   `TASKBOARD_PTY_TEST_GAP_LOG=/tmp/tty-gap-$i.log cargo nextest run
   -p taskboard-app --test cli_tty
   tty_ctrl_c_during_a_stalled_line_cancels_and_the_next_line_executes`
   under 2×nproc `yes` spinners plus one parallel scratch
   `cargo build --workspace` for the first ~10 iterations. Bound:
   30 iterations or 60 min wall. **Capture-before-rerun on any
   failure**: the full nextest failure section verbatim, the gap log
   for that iteration, lane/load shape/commit. Run the masked-read
   leg's 10-iteration loaded loop in the same session for #521's fresh
   validation evidence (the pty pass's numbers are 2026-11-01; the
   closure cites both).
2. Outcome fork (D1, binding):
   - **Reproduced with telemetry showing a legit gap > 30 s** →
     re-derive `PTY_IDLE_BUDGET`/`PTY_SESSION_BACKSTOP` by the
     recorded formula with the new G_max/T_max named in the comment
     block; commit; loop until the failing shape passes 10/10 loaded;
     disposition comment: fixed, side = test determinism.
   - **Reproduced with a silent wedge whose transcript matches the
     #572 shape** → file a new production issue (the seam evidence
     attached), no production fix inside this pass, issue #697 stays
     open cross-linked.
   - **Bounded negative** → post the bounded-negative record + keep
     the standing capture policy ("the gap log and failure output
     before any re-run") on #697; stays open. The thin-headroom
     finding from the pty pass is cross-cited.
3. Draft the #521 closure comment (the pty pass's §8 draft + this
   session's fresh validation numbers + note that an idle-budget
   panic on this test now belongs to #572's class, not #521's).
   Stage 3 posts comments after gates are green (the Nov-pass
   convention).

### 4.2 The #682 leg

Per §3.2: corrected generator, persisted seed, unit test. TDD: add
the seed to `merge.txt` first, revert the generator fix mentally —
i.e. write the unit test and generator change, show the old generator
shape is characterized by the unit test, and show the property green
across a full re-run (256 cases × at least 3 fresh seeds to exceed
the flake probability class). Diff confined to
`crates/taskboard-domain/src/merge.rs` (generator + unit test) and
`proptest-regressions/merge.txt`.

### 4.3 The #336 leg (the only untouched bounded-repro slot)

The 2026-10-26 protocol (3× workspace, 10× crate) and the 2026-11-01
scan both stand, but no loaded crate-level loop has run since the
test's own harness moved (the detach-join budget work, PR #479). One
bounded session: 15 iterations of
`cargo nextest run -p taskboard-sync-nextcloud push_exec::tests::delete_404_re_resolves_and_deletes_at_the_fresh_binding`
under the §4.1 load shape. Any failure: capture verbatim (the four
`.expect(1)` mocks and `server.verify()` point at mock setup/verify —
the capture must include which mock's verify tripped). Negative →
dated comment, stays open, capture policy unchanged.

### 4.4 The scan leg (#287 / #384 / #570 / #650)

Thread-first, then
`gh run list --created ">=2026-11-04" --limit 300` with
`gh run view <id> --log-failed` on every failure, matching the family
signatures (`actor_wiremock` a10/a12/`create_stack_transport_failure`;
`l13_alias…`; `t16_track_and_export…`; `no_local_ids…`). One dated
comment per issue; the Nov-pass formats are the template. #650
additionally: a single time-boxed local attempt (20 loaded
iterations of the filtered test, the §4.1 load shape) — the plan-day
hypothesis stands regardless: **CI drew a proptest counterexample
twice and lost it with the ephemeral runner** (proptest regressions
persist only on the failing machine; local runs draw fresh seeds).
The substrate cause is filed as **backlog #857**; #650 stays open,
watchlist, with the hypothesis and the cross-link.

### 4.5 The #846 leg

Dated watchlist comment on #846: the recorded observation stands;
hypothesis (golden capture depends on first-test-ordering/shared
window renderer state, per the issue's own analysis); explicitly
not-this-chunk (round-8 `taskboard-ui-slint` territory, round plan
§1) and the capture policy: next occurrence records the drift report,
both PNGs, and the test-filter shape before any re-run. No fix.

## 5. Stage-2 gate

Pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest run
--workspace` (background + poll beyond ~5 min; all landed work
committed before any wait; repro loops backgrounded with plan/comment
drafting between polls). The full AGENTS §10 suite is the
dispatcher's between stage 2 and stage 3.

## 6. Risks

- The #697 repro may not reproduce (the class is ~1-in-many-hundreds);
  the bounded negative then leaves #697 open — acceptable and
  recorded as the honest outcome, per the round plan's own law.
- The budget re-derivation path (if it fires) must cite fresh
  telemetry; deriving from the #654 literature alone (42 s failure,
  no gap log) is **not** sufficient evidence — if telemetry cannot be
  captured because the failure does not reproduce, outcome (c) applies.
- #682's generator change must not shift the property's *coverage*
  (the `b1 == b2` shape moves from "property" to "unit test" — the
  unit test in §3.2.4 is what keeps the shape characterized).
- Comment posting is stage 3's task (after gates); stage 2 drafts
  them into its report.

## 7. Estimated diff size

- #682: ~40–60 lines in `crates/taskboard-domain` (generator shift,
  unit test, regressions entry).
- tty leg: 0 lines (outcome c), or ~20–40 lines (outcome a — two
  constants + comment blocks).
- Everything else: issue comments only (stage 3 posts; stage 2
  drafts).
- Total expected code diff: well under 100 lines.

## 8. Backlog issues opened at plan day

- **#856** — `collapse_for_task` same-ref candidate rewinds the
  binding via the rebind path when the candidate stamp is older
  (out-of-scope production semantics surfaced by the #682 diagnosis).
- **#857** — proptest counterexamples found in CI are lost with the
  ephemeral runner (the #650 substrate cause).

## 9. Decision index

- **D0**: branch renamed `test/flaky-watchlist-pass-v2` (landed-name
  reuse forbidden, AGENTS §8).
- **D1**: tty #697 side (test vs production) decided by the
  telemetry-equipped repro, three-way outcome fork recorded (§4.1.2).
- **D2**: #682's round-plan hypothesis corrected — the tie-break is
  deterministic; the failure shape is the degenerate same-ref
  generator draw (§3.1), with the issue body's own two-reading text
  as the authority.
- **D3**: #682 fixed at the strategy premise (distinct-member
  generator), assertion byte-identical, seed persisted as record,
  degenerate shape pinned by unit test, production untouched
  (backlog #856) (§3.2).
- **D4**: #287/#570 — prior bounded negatives stand; scan-only this
  pass (§4.4).
- **D5**: #650 — time-boxed local attempt, then documented
  hypothesis + backlog #857; stays open (§4.4).
- **D6**: #846 out of chunk scope (round-8 excluded territory);
  round-plan §4 grouping corrected in §0; stays open with dated
  comment (§4.5).

# Today virtualisation — chunk 4.1 stage-3 review report (#461)

- **Date**: 2026-11-03
- **Branch**: `perf/ui-today-virtualisation`, rebased onto `origin/main`
  (was `9f3508ee`-based; main had moved 7 r4 commits — rebase clean, no
  conflicts), worktree
  `../taskboard-WT-perf-ui-today-virtualisation`.
- **Reviewer verdict: APPROVED with comment fixes applied.** The
  stage-2 work is sound: the premise correction was right, the pins
  are pinned-actual, the semantics finding is real, and the bench
  disposition follows the design round's contract. Zero production
  diffs, as the plan budgeted.

## 1. What stage 3 verified (independently)

1. **The semantics correction re-derived, not trusted.** The fixture
   (`benches/support/mod.rs`) was re-read: `has_duedate` = order
   multiple of 3 with 2024 stamps; `has_done` = order multiple of 10 —
   so overdue = multiples of 3 **minus** multiples of 10, confirming
   the vacuous-landed-pin finding (task 0 is fixture-done, never an
   Overdue row; its flip is a Done-today insertion).
2. **A live-geometry probe** (temporary test, run and removed): after
   the 30-stroke walk on `build_state(600)`, the live window holds
   orders 75–99 with order 99 at the bottom edge (y ≈ 692) — position
   29 is order **99**, and order 87 sits at position 26, mid-window.
   This confirms the repeater's content-anchored behavior the pins
   encode: the insertion pin's uniform downward reflow, the
   above-viewport removal's exact-extent upward reflow, and the
   within-viewport close-up all behaved exactly as pinned (29/29
   virtualisation tests green on the rebased branch, including all
   three new pins).
3. **The trap clause is discharged**: the pins encode the pinned
   actual (content-anchored reflow under a fixed content offset), not
   the stage-1 D-2 desired semantics; the one red run stage 2 hit was
   resolved per the plan's own R1 protocol.
4. **The bench's kept lane**: verified the `refresh/{n}` lane body
   (no state change ⇒ the #463 guard's no-op path, as the new doc
   block states) and that no live unvirtualised lane was introduced —
   the D-4 structural disposition (engagement probes + recorded
   baselines) stands. One lane re-run by stage 3: see §4.
5. **IdTables mapping (design report §3c)**: zero production diff;
   the reverse helpers keep global model indices (stage-1 F4/D-3);
   the journeys and walk pins pass unmodified on the rebased branch.

## 2. Findings and fixes applied (comment-level; no assertion touched)

1. **Walk-position arithmetic wrong in all three new pins' comments**
   (and inherited from the plan/stage-2 drafts): they derived
   position 29 = order 87 by counting multiples of 3 without
   removing the fixture-done orders 30 and 60. Actual: position 29 =
   order 99; order 87 = position 26. The pins' *geometry* is
   unaffected — order 87 genuinely is a live mid-window row, which is
   precisely what the case-2/3 pins need an anchor row to be — but
   the "the walked-to card is live"/"the 29th row is order 87"
   claims misdocumented what is pinned. Fixed with the full
   position-map derivation on the within-viewport pin (the others
   reference it); "walked-to/focused" phrasing corrected to
   "anchor/witness/pinned row".
2. **The landed case-1 pin's doc comment asserted the false
   row-anchored semantics** and mislabeled its flip as "a removal
   above the viewport". Its plan mandate was "kept untouched", which
   stage 3 resolves as: the *test body* stays untouched (it still
   pins the real cross-section property — a Done-today insertion must
   not move the Overdue viewport), but the comment now states the
   honest scope and points at the true case-1 pin. Assertion
   messages reworded to match ("a Done-section insertion must not
   move the Overdue viewport's rows").
3. **Stage-2 report arithmetic**: its own correction line said
   "position 29 is order 93" — itself off by the order-90 removal.
   Corrected in place with the exact position map and a note that the
   correction was probe-verified.

No finding required a production change, a test-body change, or a
weakening (AGENTS §5 respected throughout).

## 3. Residual decision recorded: the above-viewport reflow is accepted

The pinned actual means a done flip *above* the viewport reflows the
visible rows by that row's extent (~70 px in the fixture) under a
fixed content offset — the viewport never teleports, never resets,
and within-viewport flips (the common click case) keep the user's
place exactly. Stage 3 accepts this as the ListView contract (it is
the standard content-anchored behavior of virtualised lists) rather
than filing it as a #373 bug; the idea of a row-anchored (visual)
anchoring mode is recorded upstream as a backlog issue for a future
round, not silently dropped.

## 4. Gate suite and lane evidence (stage 3's own run)

- Full gate suite via `scripts/run-quality-gates.sh` from the
  worktree (flock wrapper): fmt, clippy `-D warnings`, nextest
  workspace, doc tests, cargo deny, llvm-cov three-command sequence
  with quarantine + trailing `rm` — see the PR's checks for the
  CI-side confirmation; results in §5 of the final commit message
  thread / PR description.
- Bench lane re-run by stage 3: `draw-full/today` at all three N
  (decoupling re-confirmed; references, never gates).

## 5. Deviations from the stage-1 plan

Only those stage 2 already recorded (§5 of the stage-2 report), plus
stage 3's comment-level fixes above (the plan's "kept untouched"
mandate for the landed pin is narrowed to its test body). None
touches a production file; the branch's diff remains tests + docs.

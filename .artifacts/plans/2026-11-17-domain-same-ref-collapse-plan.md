# Chunk Plan — C5: domain same-ref collapse (#856)

- Round: backlog-reduction round (2026-11-17), chunk C5
- Branch: `fix/domain-same-ref-collapse`
- Normative: round plan §3-C5; dispatch plan §1/§6
- Scope size: S
- Stage: 1 (planning) — this plan; implementation is stage 2

## 1. The shape (issue #856)

`collapse_for_task` (`crates/taskboard-domain/src/merge.rs`, symbol-anchored
~1664 at planning time) folds the bound member and the candidate list into one
group and picks the survivor purely by `(stamp, !is_bound)`. When a candidate's
`RemoteCardRef` **equals** the bound member's ref:

- the doomed set is correctly empty (the dedup filter `*ref_ != survivor`
  drops the bound twin), but
- if the candidate stamp is older, `survivor_is_bound` is false ⇒ `rebind`
  true ⇒ the consumer rebinds the task to the ref it is already bound to,
  carrying the older stamp — `remote_seen` rolls backwards with no
  duplication resolved (the #856 rewind).

Today the shape is characterized (not prescribed) by
`the_degenerate_same_ref_group_characterizes_collapse` in `merge.rs` tests,
which explicitly names #856 as the open production question.

## 2. Ruling: structural no-op (confirmed — no deviation)

The round plan binds no-op as the ruling. Stage-1 consumer verification
**confirms** it; fold-into-bound is not taken.

### 2.1 Consumer verification (complete)

Every caller of `collapse_for_task` and of the `DuplicateCollapse` verdict:

1. **`detect_crash_window_duplicates` (merge.rs, ~1639)** — forwards the
   verdict; the candidate it collapses is a *distinct* snapshot card by
   construction of the caller.
2. **R-dup pass in `pipeline.rs` (~828)** — the only production consumer of
   the verdict's `rebind`/`survivor`/`doomed`. It filters candidates with
   `task_by_board_card.contains_key(&(candidate.id.board, candidate.id.card))`
   before detection; `task_by_board_card` is built from every task's `remote`
   ref (pipeline.rs ~515, retained to the board at ~571). **A candidate ref
   equal to the bound ref is therefore excluded at the pipeline level** —
   the same-ref shape is unreachable in production today. Additional safety:
   when `rebind` fires, the pipeline looks the survivor up in
   `card_observations` with `expect(...)`; under the no-op ruling the
   survivor is the bound ref, which is present in `card_observations`
   whenever `bound_obs` is `Some` (and when it is `None` no same-ref group
   can form, so the `expect` cannot newly fire).
3. **Tests/properties (merge.rs ~5891, ~5921, ~6000)** — direct calls; no
   reliance on older-stamp-wins for same-ref.

**Finding: no legitimate consumer relies on the older-stamp-wins shape for a
same-ref candidate.** The no-op ruling stands, and additionally functions as
defense-in-depth: it makes the invariant hold at the `collapse_for_task`
boundary itself, not just at today's pipeline filter.

### 2.2 Fold-into-bound — rejected

Folding the same-ref candidate's stamp into the bound member (e.g. taking the
older stamp as the bound member's stamp) would *still* rewind `remote_seen`
via the next rebind/refresh path and would change the survivor-stamp
semantics DEC-5 documents ("the oldest member survives" — for *distinct*
cards). No consumer needs it (§2.1). Rejected.

## 3. Decisions (all open questions resolved)

| # | Question | Decision |
|---|---|---|
| D1 | no-op vs fold-into-bound | **no-op**: a candidate whose ref equals the bound member's ref is dropped from the group before survivorship; the bound member (if present) survives; `rebind` false; `doomed` empty |
| D2 | same-ref candidate with `bound = None` | Not a same-ref shape — no bound member exists, current distinct-card survivorship is unchanged. The no-op rule is defined only against the bound member's ref. |
| D3 | multiple same-ref candidates (all equal to bound) | All dropped; identical to D1 for each member. |
| D4 | satisfied_op on a no-op collapse | Unchanged: passed through verbatim. The verdict is still formed (survivor = bound ref, doomed empty) so the pipeline's `op_results` / `dup_suppressed` bookkeeping keeps working; only the rewind path is removed. |
| D5 | existing characterization test `the_degenerate_same_ref_group_characterizes_collapse` | **Rewritten** to pin the desired semantics (no-op), not weakened. Its "older same-ref candidate" branch pins today's rewind *explicitly as the open #856 question* — this is the `..._currently_pins_...` situation AGENTS.md §5 names; the fix replaces the pin. Its "tie" and "newer" branches keep asserting the same outcomes (which the no-op preserves). |
| D6 | #682 proptest premise (`the_oldest_member_survives_and_a_tie_keeps_the_bound_card` + `proptest-regressions/merge.txt`) | **Untouched** — exactly as PR #864 landed it ([L](c)). The generator excludes same-ref at the premise level; the property still asserts `doomed.len() == 1` for distinct refs, which the no-op does not affect. |
| D7 | rewind-forbidden invariant test | New test: a collapse *sequence* mixing same-ref and distinct-ref candidates asserts `remote_seen`-relevant outcomes — every same-ref-bearing collapse yields `rebind == false` and empty `doomed`, so no collapse in the sequence can rewind the binding; the distinct-ref collapse in the middle still rebinds (routing sanity). Asserted via typed outcomes (`rebind`, `doomed`, `survivor`), not text ([L](a)). |
| D8 | where the fix lives | Inside `collapse_for_task` only (drop same-ref candidates pre-fold). No changes to `detect_crash_window_duplicates`, `rebind_task_to_survivor`, or the pipeline. Diff confined to the merge module + its tests. |
| D9 | UX journey plan | Not required — chunk is exempt (round plan chunk table: "no UX plan in origin"; no user-visible capability changes). |

## 4. Implementation outline (stage 2, TDD)

1. **Red**: update/extend the degenerate-shape test to assert the no-op
   outcomes for the older same-ref candidate (`rebind == false`, doomed
   empty, survivor = bound ref). Add the sequence invariant test (D7). Both
   fail against current behavior.
2. **Green**: in `collapse_for_task`, before the fold, drop every candidate
   whose ref equals the bound member's ref (when `bound` is `Some`). Keep
   the DEC-5 tie-break, doomed derivation, and tracing span untouched; the
   debug span's `doomed.len()` naturally reflects the no-op.
3. Doc comment on `collapse_for_task`: same-ref candidate = structural
   no-op (rewind forbidden), referencing #856 and DEC-5's distinct-card
   premise.
4. Diff confined to `crates/taskboard-domain/src/merge.rs` (function + tests).
   No new files; no new crate; `#![forbid(unsafe_code)]` already in force.

## 5. Acceptance

- Pinned unit tests: older same-ref candidate → bound survives, `rebind`
  false, `doomed` empty; tie and newer branches unchanged in outcome.
- Invariant test: `remote_seen` monotonicity across a collapse sequence
  (no same-ref collapse ever produces `rebind == true`).
- Property suite green with the #682 premise and regressions entry byte-identical.
- `cargo nextest run --workspace` green via `scripts/run-quality-gates.sh`;
  no other merge behavior change.

## 6. Risks

- **Low**: a future consumer passes same-ref groups deliberately expecting
  stamp-based survivorship — mitigated by the doc comment naming #856 and
  the pinned tests making the contract visible.
- **Negligible**: the pipeline's `expect("the survivor was matched from this
  cycle's evidence")` — under no-op the survivor is the bound ref, present in
  `card_observations` whenever the shape can form (§2.1 item 2).

## 7. Out-of-scope

Nothing new discovered requiring a backlog issue at planning time (the #856
issue itself already carries the shape and direction; the pipeline-level
exclusion is existing documented behavior, not debt).

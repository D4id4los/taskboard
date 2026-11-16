# The Recur Attribution Fix (#713) — Chunk B Plan

- **Date**: 2026-11-16
- **Kind**: stage-1 chunk plan (TDD-first) for the correctness-hygiene
  round, chunk B (`fix/domain-recur-done-window`).
- **Origins** (normative):
  `.artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md` §3,
  `.artifacts/plans/2026-11-16-correctness-hygiene-dispatch-plan.md` §1/§3,
  GitHub issue #713, AGENTS.md §5.
- **Status**: READY — diagnosis complete on a reproduced red test; no
  open questions remain (§7 records every decision).
- **Scope**: `crates/taskboard-domain`, recur module **tests only**.
  Zero production hunks. Non-goals: no `schedule`/`occurrence_outcome`
  behavior change; no new knob; no doc-comment rewrites beyond what a
  reviewer deems strictly necessary (prefer none).

## 1. Diagnosis (decided, with evidence)

### 1.1 The failing case

Property `stamps_attribute_to_exactly_one_member`
(`crates/taskboard-domain/src/recur.rs`, `arithmetic_tests`), assertion
**"Done stamps sit inside the window"** (line 1883). Minimal input =
issue #713's persisted seed:

- rule: `Weekly { interval: EveryWeeks(1), on: WeekdaySet({Thu, Fri}) }`,
  anchor **2053-10-16 (a Thursday)**, bound `Until(2053-10-16)`,
  no pause;
- `day_start_seconds = 0`, `offset_seconds = 0`, `now_offset = 0`,
  `stamps = [24]` → one completion at `ws(anchor) + 24h`
  = 2053-10-17T00:00Z.

### 1.2 Replay evidence (this worktree, 2026-11-16)

The seed was appended to
`crates/taskboard-domain/proptest-regressions/recur.txt` and the
property run on the branch base (= `origin/main` `dfdf2562`):

```
Test failed: Done stamps sit inside the window at
crates/taskboard-domain/src/recur.rs:1883.
minimal failing input: rule = RecurRule { ... anchor: 2053-10-16,
pattern: Weekly { interval: EveryWeeks(1), on: WeekdaySet({3, 4}) },
bound: Until(2053-10-16), ... }, day_start_seconds = 0,
offset_seconds = 0, now_offset = 0, stamps = [24]
```

The seed **does** bind and reproduce on current `origin/main` — it is
live, not stale. (Procedural note for every later agent on this chunk:
the first replay attempt appeared to PASS because the gate command was
accidentally run from the *main checkout* instead of the chunk
worktree; always anchor gate runs with an explicit `cd` into the
worktree. A hand-rolled scratch replay of the exact shape failed
identically, cross-confirming.)

### 1.3 The trace

With `day_start_time = 00:00`, `offset = UTC`, the property's own
arithmetic:

1. The property enumerates members **boundlessly**:
   `let boundless = RecurRule { bound: RecurBound::None, ..rule }`;
   `members = schedule(&boundless, anchor, horizon)` yields
   `[10-16 (Thu), 10-17 (Fri), 10-23, 10-24, …]`.
2. Attribution, however, is called with the **bounded** `rule`.
   Inside `occurrence_outcome(&rule, 10-16, …)`:
   `next = next_live_after(rule, 10-16) = 10-17 > Until(10-16)` →
   `window_end = None` — 10-16 is the rule's **final live member**
   (the `Until` bound closes the member list right after the anchor),
   and per D8 its window is deliberately **open-ended**
   (`recur.rs` §`occurrence_outcome` doc: "the final member is never
   `Missed` — the open head simply sits on the board").
3. The completion at 10-17T00:00Z is ≥ `ws(10-16)`, so the outcome is
   `Done { at: 10-17T00:00Z }`.
4. The property asserts `at < b` with `b = ws(members[1]) =
   ws(10-17)` — and `at == b`. Assertion fails.

### 1.4 Which side violates the partition premise — DECISION: the test, not production

Both candidates from the round plan §3 were evaluated:

- **`schedule` leaking a member before the anchor's window** —
  rejected. `schedule`'s enumeration is correct and irrelevant here:
  the anchor *is* a member (Thursday ∈ {Thu, Fri}) and it *is* the
  first member; nothing is enumerated out of order. `schedule(&rule, …)`
  correctly yields `[10-16]` (the next member 10-17 fails the
  `member > until` filter and the loop breaks).
- **`occurrence_outcome` attributing across the window boundary** —
  rejected. Its behavior is exactly the documented D8 contract: the
  `Until`-closed final member's window is open-ended; a completion at
  10-17T00:00Z attributes *to the final member* because no member
  exists after 10-16 whose window start would close it. This is by
  design, pinned elsewhere ("the final member is never Missed").
- **The property's premise mixing two domains** — **the bug**. It
  partitions the timeline over `schedule(&boundless, …)` members
  (10-16, 10-17, …) but asks attribution of the *bounded* rule, whose
  domain of members stops at 10-16. The pair `(ws(members[0]),
  ws(members[1]))` is not a window of the rule under attribution — it
  is a window of a *different series* (the boundless one). The test
  compares a function's answer about its own domain against a
  partition of someone else's. **Fix side: the test's domain
  selection.** Production code takes zero hunks.

### 1.5 Why this is not "weakening a correct test" (AGENTS §5)

The assertion itself is untouched (§3 keeps it verbatim). What changes
is which rule/member list each assertion pass runs over, and the fix
*adds* a pass instead of removing one:

- Pass A (new, the fix): run the full existing assertion set over
  `(rule, schedule(&rule, …))` — attribution domain and partition
  domain now coincide. The seed case passes because the sole member
  10-16 is the list's last, its window correctly open-ended, and the
  "windows partition" / "Done inside window" / "never Missed" laws all
  hold on the rule's real domain.
- Pass B (kept): run the identical assertions over
  `(boundless, schedule(&boundless, …))` — the pre-bug enumeration, now
  paired with the boundless rule for attribution so the domains match
  there too. Nothing that was covered before is covered less; the
  boundless coverage is preserved whole and its own attribution
  becomes *consistent* (today Pass B is also subtly wrong for rules
  with an `Until`: same mismatch, just not hit by a shrunk case yet —
  Pass B repairs that latent hole).

Net: strictly more verified ground, zero assertions removed, relaxed,
or commented out. This is a premise repair, not a weakening.

## 2. The fix (implementation shape)

In `stamps_attribute_to_exactly_one_member`, restructure the body into
a helper closure/local fn taking `(rule_under_test, members)` and call
it twice:

```rust
let horizon = rule.anchor + 400 days;                       // unchanged
let boundless = RecurRule { bound: RecurBound::None, ..rule.clone() };
// Pass A — the rule as generated: partition over the rule's own
// live members (Until-filtered), attributed by the same rule.
check_partition(&rule, &schedule(&rule, rule.anchor, horizon), …);
// Pass B — the boundless projection, attributed by the boundless
// rule (domain-matched; the pre-#713 enumeration, kept whole).
check_partition(&boundless, &schedule(&boundless, rule.anchor, horizon), …);
```

where `check_partition` contains **verbatim** the existing assertion
blocks (strictly-increasing window starts; Done-inside-window at the
`at >= a && at < b` strength; per-stamp partition with
`prop_assert_eq!(attributed, 1)`; final-member-never-Missed). Keep
`prop_assume!(!members.is_empty())` per pass (a pause may empty the
bounded list — the existing justification comment moves with it).
`prop_assert!`/`prop_assume!` require the macro context, so the helper
must stay inside the `proptest!` body (a local closure/fn in scope) or
take the failure-channel explicitly — the implementer picks whichever
compiles cleanly under `proptest`; the assertion texts must not change.

Exact assertions kept unweakened (current line anchors, re-anchor by
text):

- `prop_assert!(a < b, "window starts strictly increase")`
- `prop_assert!(at >= a && at < b, "Done stamps sit inside the window")`
- `prop_assert!(matches!(outcome, OccurrenceOutcome::Done { .. }), "the containing member's outcome is Done for its stamp")`
- `prop_assert!(!matches!(outcome, OccurrenceOutcome::Done { .. }), "a stamp outside the window never attributes")`
- `prop_assert_eq!(attributed, 1, "windows partition the timeline")`
- `prop_assert!(!matches!(outcome, OccurrenceOutcome::Missed), "the open final head is never Missed")`

## 3. TDD sequence for stage 2

1. **Red commit**: append the #713 seed line (exactly as in §1.2 /
   issue #713, including its `# shrinks to …` comment) to
   `crates/taskboard-domain/proptest-regressions/recur.txt`. Verify
   `scripts/run-quality-gates.sh cargo nextest run -p taskboard-domain
   stamps_attribute_to_exactly_one_member` **fails from the worktree
   cwd** (§1.2). Commit message: `test(recur): the #713 attribution
   seed — red on main`. (If the pre-commit hook suite forbids a
   knowingly-red commit, land the seed *with* the fix commit instead
   and keep the red demonstration in the plan log — never bypass or
   `--no-verify` a hook.)
2. **Green commit**: the §2 two-pass restructure. The seed case and
   the full property go green. Commit message:
   `fix(recur): match the attribution partition to the rule's own member domain`.
3. **Boundary units commit**: the §4 unit tests.
   `test(recur): pin the Until/final-member attribution boundaries`.
   All three commits standalone (rebase-merge hygiene, AGENTS §8).

## 4. New boundary unit tests (characterize, don't just un-break)

Plain `#[test]` fns in `recur.rs`'s `tests`/`arithmetic_tests` module,
typed outcomes only (`matches!`/`assert_eq!` on enums — no text), no
sleeps (pure functions; no time dependency beyond explicit instants).
Anchor dates picked inside the strategy's 2000–2100 band and verified
for weekday by the test itself (compute `Weekday::from` — no magic
comments).

1. `until_final_member_window_is_open` — the exact #713 shape:
   `Weekly { EveryWeeks(1), {Thu, Fri} }`, anchor = Thursday
   2053-10-16, `Until(anchor)`. Assert:
   `schedule(&rule, anchor, anchor+400d) == [anchor]`;
   `occurrence_outcome(…, completions=[ws+24h])` is
   `Done { at }` with `at == ws(anchor) + 24h`;
   with `now` far past, outcome is still `Done` (never `Missed`).
2. `until_anchor_on_first_weekday_open_final_window` — the
   neighboring shape with the anchor on the pattern's *first* weekday:
   `Weekly { EveryWeeks(1), {Thu} }`, anchor Thursday, `Until(anchor)`.
   Same three assertions — proves the open-final behavior is not a
   multi-weekday artifact.
3. `anchor_on_non_member_weekday_member_bounded_later` — anchor on a
   non-member weekday: `Weekly { EveryWeeks(1), {Mon} }`, anchor =
   Sunday, `Until` = the following Monday. Assert
   `schedule(&rule, anchor, …) == [that Monday]` (no member on the
   anchor itself, none before the first pattern weekday), stamp inside
   `[ws(anchor), ws(Monday))` attributes to **nothing** (before the
   first member: §4.4 pre-series stamps attribute nowhere —
   `occurrence_outcome(&rule, monday, …, [stamp])` is `NotYetReachable`
   / not `Done`), and a stamp inside Monday's window is `Done` on
   Monday.
4. `until_on_member_day_mid_series_window_closes_at_next_member` —
   the Until-bound-on-a-member-day case *not* at the series start:
   `Weekly { EveryWeeks(1), {Wed, Thu} }`, anchor = a Wednesday,
   `Until` = the *next* Wednesday. Assert
   `schedule == [anchor_wed, next_wed]`; a Thursday stamp
   (`ws(anchor) + 24h`) is `Done` attributed to `anchor_wed` and its
   window closes at `ws(next_wed)` (`at >= ws(anchor) && at <
   ws(next_wed)` — the ordinary closed window, the contrast case for
   test 1); with `now` past `ws(next_wed)` and no completions,
   `anchor_wed` is `Missed` while `next_wed` (final, open) never is.

## 5. Corpus-unchanged guarantee

The fix is test-only, so "existing schedule output byte-identical" is
proved structurally, not by new snapshots:

1. **Diff-scope proof**: `git diff origin/main -- crates/taskboard-domain`
   must show changed lines **only** inside `recur.rs`'s
   `#[cfg(test)]`-gated module range plus the one-line regressions-file
   append. Mechanically checkable: the stage-3 reviewer runs
   `git diff origin/main -- crates/taskboard-domain/src/recur.rs` and
   verifies no hunk touches a line outside the test modules (search
   the hunk headers against the `#[cfg(test)]` boundary line number).
   Any production hunk is an automatic finding — diagnose, don't
   refresh.
2. **Full suite green**: `scripts/run-quality-gates.sh cargo nextest
   run --workspace` (flock-wrapped, from the worktree cwd) plus the
   remaining AGENTS §10 gates at handoff (`cargo fmt --check`,
   `clippy -D warnings`, `cargo test --doc`, `cargo deny`, the
   coverage trio). Every existing recur test (enumeration laws,
   roll/badge laws, serde round-trips, the other proptests) passing
   unchanged is the behavioral corpus evidence — no snapshot is added,
   because there is no production delta to pin.

## 6. Risks

- **proptest helper ergonomics**: extracting `check_partition` inside
  the `proptest!` body must preserve `prop_assert!`'s filtered-failure
  semantics (numbered cases, shrinking). If the closure fights the
  macro, prefer a plain local `fn` taking the values plus returning
  `Result<(), TestCaseError>` — `proptest` supports that; do not drop
  to raw `assert!` (loses the case numbering).
- **Pass B with pauses**: for rules whose pause empties the *bounded*
  list, Pass A hits `prop_assume!` — fine. Pass B's boundless list is
  never pause-emptied (members still exist behind a pause in the
  boundless projection? `schedule` uses `next_live_after`, which lifts
  past `paused_until` even with `bound: None` — so Pass B's list is
  the post-pause live list and attribution with the boundless rule
  shares that same live notion; domains still match). If Pass B turns
  out to fail on a *pause* shape, that is a **new fact** — stop, do
  not weaken; record it and raise to the dispatcher (it would mean the
  live-member lift and window attribution disagree somewhere the
  current property never looked).
- **Hook-vs-red ordering**: see §3 step 1's fallback — the seed rides
  the fix commit if hooks forbid a red standalone commit.
- **Wrong-checkout gate runs** (the §1.2 hazard): all gate commands
  anchored with `cd /home/viktoria/src/taskboard-WT-fix-domain-recur-done-window`.

## 7. Open questions — all resolved

1. *Which side violates the partition premise?* — The property's
   boundless-enumeration/bounded-attribution mismatch (§1.4). Production
   code is contract-correct; zero production hunks.
2. *Is changing the property a weakening?* — No (§1.5): assertion
   texts verbatim, coverage strictly grows (two domain-matched passes).
3. *Does the persisted seed still bind on current main?* — Yes,
   reproduced red (§1.2); the earlier apparent PASS was a
   wrong-checkout artifact, not seed staleness.
4. *Keep the boundless coverage?* — Yes, as domain-matched Pass B
   (§1.5); it also repairs a latent Until-mismatch hole.
5. *New snapshots for corpus-unchanged?* — No: structural diff-scope
   proof + full suite green (§5); no production delta exists to pin.

## 8. Estimated diff

~120–170 changed lines, all in `crates/taskboard-domain`:
`recur.rs` test module (~110–150: the two-pass restructure + four unit
tests), `proptest-regressions/recur.txt` (+1), this plan file. Zero
production lines.

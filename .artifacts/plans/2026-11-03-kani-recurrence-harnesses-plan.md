# [Plan] Kani recurrence harnesses (#505) — chunk 1.4 implementation plan (stage 1)

- **Date**: the artefact name carries the dispatch date (2026-11-03,
  per the dispatch prompt's mandated path); **written 2026-11-04** on
  the chunk's worktree.
- **Kind**: stage-1 planning record for chunk 1.4 of backlog round 3
  (`.artifacts/plans/2026-11-03-backlog-round3-dispatch-plan.md`
  §"Chunk 1.4"). Implementation = new `#[cfg(kani)]` harnesses
  appended to the existing domain verify module + this plan;
  **zero production diffs** (dispatch acceptance). Stage 2 implements
  (prompt 1.4.b), stage 3 reviews and ships (prompt 1.4.c).
- **Origin (normative)**: issue **#505** — "`#[cfg(kani)]` harnesses
  for the recurrence arithmetic — `next_after`, `roll_target`,
  `occurs_on` are pure and total over small input domains, ideal
  bounded-model-check targets (the pilot's own candidate class).
  Unwind bounds documented at each harness per the pilot's
  conventions; never in the PR pipeline." Dispatched from the DR-6
  design round's §3.9 Kani note
  (`.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md`
  — the recurrence proptest catalogue P1–P4 is what these harnesses
  escalate to exhaustive-within-bounds).
- **Precedent (binding)**: the landed #426 Kani pilot — issue body +
  close comment of **PR #456** (merged 2026-10-26), its harness
  conventions in `crates/taskboard-domain/src/verify.rs`, its stage-1
  plan (`.artifacts/plans/2026-10-25-kani-pilot-plan.md` — §1 width
  rule, §6 memory budget, §7 bound hygiene are restated as binding
  rules below), and its stage-2 report
  (`.artifacts/reports/2026-10-26-kani-pilot-stage2-report.md`).
- **Branch / worktree**: `test/kani-recurrence-harnesses` /
  `../taskboard-WT-test-kani-recurrence-harnesses` (doctor exit 0,
  216 G free, floor 25 G).
- **Base (decision D-1)**: `origin/main` = `396bd6e3`. The brief
  says "off main"; local `main` was 17 commits behind the fetched
  `origin/main` (`c21c0e9c` is an ancestor), and two of those commits
  touch the seam file — `80e55cf6` adds `RecurRule::text_form` /
  `parse_text_form` (a serde-JSON wrapper, *not* part of the
  arithmetic) at `recur.rs:668-688`, shifting later line numbers by
  +21 and leaving every target function byte-identical (verified by
  diff). The fresh branch was rebased onto `origin/main` before any
  commit; all `recur.rs` line references below are against `396bd6e3`
  and are never load-bearing.

## 1. Mission & binding constraints

Dispatch plan §chunk 1.4 acceptance, each item mapped to its state at
the base:

| Acceptance item | State at `396bd6e3` | This chunk's work |
|---|---|---|
| Harnesses follow the pilot's conventions exactly (attribute style, bound documentation, proof-vs-assume discipline) | Conventions live in `crates/taskboard-domain/src/verify.rs` (311 lines, 3 harnesses) and the pilot plan | §3 restates them as binding rules; §4 specifies each harness against them |
| At least one harness finds its intended property provable (or the counterexample is triaged with a recorded disposition) | Not started | R3 (§4.3) is the near-certain proven harness (loop-free arms, full sampled ranges); §6 is the triage branch if a counterexample fires |
| The normal suite and gate run are untouched by the cfg gating | Verified by the pilot (report §3): clippy/nextest/coverage/deny never compile `cfg(kani)` | Gates re-run through `scripts/run-quality-gates.sh` at handoff; §7 |
| Zero production diffs | `#[cfg(kani)] mod verify;` gate + workspace `unexpected_cfgs` registration already on `main` | Diff scope = `verify.rs` only (+ a `#[cfg(test)]` repro test in `recur.rs` *only* on a counterexample, §6) |
| Never in the PR pipeline | `.github/workflows/kani.yml`: Mondays 04:00 UTC cron + `workflow_dispatch`, no PR trigger | Untouched: the domain job runs unfiltered, so the new harnesses ride the existing weekly job with no workflow diff (§7) |

## 2. The seams (input domains, loop inventory, unwind derivations)

### 2.1 Target functions (`crates/taskboard-domain/src/recur.rs` @ `396bd6e3`)

- `occurs_on(pattern, anchor, d)` — `:810`. Gate (`d ≥ anchor`) +
  `aligned` (`:785`). **Loop-free** (match, `rem_euclid`,
  `WeekdaySet::contains`, closed-form calendar helpers
  `clamped_day :765`, `nth_weekday :772`, `last_day_of :740`,
  `week_index :727`).
- `next_after(pattern, anchor, from)` — `:825`. Per-arm:
  - **Daily**: closed-form ceiling division. Loop-free.
  - **Weekly**: outer `loop` over week indices (`wi += 1`) until
    aligned — any `interval` consecutive indices contain exactly one
    aligned one → ≤ `interval + 1` trips; inner `for` over the ≤ 7
    weekdays. (Proptest interval ≤ 8 → outer ≤ 9, inner ≤ 7.)
  - **Monthly OnDay**: month-walk `loop`; the clamped candidate
    always exists, and the next month's is strictly greater than any
    day of the current month → ≤ 2 trips.
  - **Monthly NthWeekday**: month-walk `loop` skipping months that
    lack the n-th weekday. n ≤ 4 exists in every month (≥ 28 days) →
    ≤ 2 trips; n = 5 misses only months with
    `last_day < offset + 29` — the longest run of such months is 3
    (calendar-length pattern 31/28|29/31/30…; verified by the
    harness itself: a wrong derivation fires Kani's loud
    unwinding-assertion, it cannot pass silently) → ≤ 4 trips, bound
    set with slack.
- `roll_target(pattern, anchor, stale, today)` — `:1006` =
  `next_after(stale)` + the private backward scanner
  `prev_on_or_before` (`:892`):
  - **Daily**: closed-form `div_euclid`. Loop-free.
  - **Weekly**: backward week-walk `wi -= step`, ≤
    `(today − anchor)/7` ÷ `interval` + 1 trips; inner scan ≤ 7.
  - **Monthly** (both anchors): backward month-walk ≤
    `(today − anchor)/28` + 2 trips.

### 2.2 Input domains — constructor type vs proptest parity

The proptest strategies (`crates/taskboard-domain/src/
test_support/recur.rs`) define the laws' domains; the type mints
define the constructors' validity. Symbolic harness surfaces pick the
narrower where unwind forces it, the full valid domain where the arm
is loop-free (D-5):

| Input | Constructor-valid | Proptest parity | Harness surface |
|---|---|---|---|
| dates (`anchor`, `d`) | `NaiveDate` ±262k y | CE days of 2000-01-01..=2100-12-31 | `i32` CE day + assume in the proptest window (D-4) |
| `from` offset (P1) | — | `0..=40_000` d past anchor | `u16` + assume ≤ 40_000 |
| `from` back-offset (P1′) | — | `1..=4_000` d before anchor | `u16` + assume ≤ 4_000 |
| stale offset (P4) | — | `0..=20_000` d | `u16` (full range) / `u8` ≤ 64 at Rung A |
| gap (P4) | — | `1..=2_000` d | `u16` + assume ≤ 2_000 / `u8` ≤ 56 at Rung A |
| `EveryDays` | `u16 ≥ 1` | `1..=30` | full `u16 ≥ 1` (loop-free arm) |
| `EveryWeeks` | `u16 ≥ 1` | `1..=8` | `1..=8` (unwind-driven, D-5) |
| weekday mask | 7-bit, non-empty | `1..=127` | full `u8`, bits 0..6, masked ≠ 0 |
| `DayOfMonth` | `u8 1..=31` | `1..=31` | full valid domain |
| `NthWeek` | `u8 1..=5` | `1..=5` | full valid domain |

Pattern arm minting is pilot-style: one `u8` tag → `RecurPattern`
via index match (0 Daily, 1 Weekly, 2 Monthly-OnDay,
3 Monthly-NthWeekday), per-arm fields as the table above.

### 2.3 Unwind accounting

Harness-level `#[kani::unwind(N)]` applies to every loop in reach,
including the harness's own mints (the 7-bit weekday-set expansion
and `BTreeSet` inserts, ≤ 8 trips) and `BTreeSet` walks (≤ 7-node
tree, ≤ 8). Chrono date construction/decomposition on these paths is
loop-light (pilot H4 evidence: proven at unwind 8 over symbolic date
math). Per-harness bounds are derived in §4 and are **not final until
the doubling check passes** (§3 rule B6).

## 3. Binding harness conventions (restated from the landed pilot)

Every item below is verified pilot practice (pilot plan §1/§2/§6/§7,
stage-2 report §3/§5/§8) — stage 2 deviates only with a recorded
reason in the stage-2 report.

- **B1 — cfg isolation**: harnesses live in
  `crates/taskboard-domain/src/verify.rs` (the existing
  `#[cfg(kani)] mod verify;` gate in `lib.rs:67-68` — the gate line
  already exists; **no new gate, no manifest change, no workflow
  change**). Stable builds, clippy, nextest, doc tests, coverage and
  deny never compile harness code. The proof library
  (`kani::proof`/`any`/`assume`) auto-provisions from the Kani
  bundle — the crates.io `kani` placeholder crate is never added.
- **B2 — one harness = one law-set = one `#[kani::proof]` fn**,
  named after the proptest it escalates; the `_proven` suffix is
  appended only once that harness's doubling check (B6) is green;
  containment-rung harnesses carry their rung (`_rung_a`).
- **B3 — narrowest symbolic type**: CBMC bitblasts the *type*;
  `kani::assume` ranges never shrink the circuit. `i32`/`u16`/`u8`
  surfaces per §2.2; widened only after minting. Dates are minted
  CE-day-relative in the proptest's own window via
  `NaiveDate::from_num_days_from_ce_opt` + assume-`Some` (D-4).
- **B4 — never symbolize what the law cannot see** — and never
  symbolize `String` content at all (the measured CBMC memory bomb:
  pilot H2 Rung A and H5, both killed at the 6 GiB cap). The
  recurrence trio touches no strings; nothing here may introduce
  them.
- **B5 — no proof-by-assume of the property under test** (dispatch
  acceptance names this discipline): `kani::assume` constrains input
  domains only, each assume justified by parity or by the rung's
  documented narrowing. Case-splits are `if` guards on computed
  values, never assumes. A spec mirror (harness-local, written as an
  *independent* formulation, per the pilot's `spec_phase_rank` /
  `spec_parse_total` precedent) is specification, not assumption.
- **B6 — no unwritten bounds**: every harness documents, in its doc
  comment next to `#[kani::unwind]`, the loop-trip derivation and
  the bound-doubling re-run (2× bound, identical verdict/checks/
  unreachable counts) before its bound is declared final. An
  unwinding-assertion failure is an inconclusive bound, not a
  property violation — raise with an updated derivation, re-double,
  and record.
- **B7 — guard every local run**: 
  ```sh
  systemd-run --user --scope -p MemoryMax=6G -p MemorySwapMax=0 \
    /usr/bin/time -v cargo kani --harness <name> --output-format terse
  ```
  No exceptions (the pilot's §0 OOM incident killed a whole process
  chain). Cap discipline: two kills at a rung = recorded, descend —
  no third blind retry, no cap-raising.
- **B8 — commit the first green harness immediately** after the
  worktree is (re)created — a commit absent from `origin/main` is
  what makes the concurrent worktree-doctor sweeps refuse (pilot
  incident 1; this plan's commit is the branch's doctor-proofing
  commit for stage 1).
- **B9 — self-contained mints**: `test_support` is
  `#[cfg(any(test, feature = "test-support"))]` and therefore
  invisible under `cfg(kani)`; harness-local `mint_*` helpers only
  (pilot precedent). SonarCloud's duplication gate applies to
  harness code (the pilot's `5162a9a` fix) — mint literals are
  factored into helpers, spec mirrors are written as genuinely
  independent formulations, never transcriptions.
- **B10 — Kani pin**: 0.68.0 (local install verified this machine:
  Kani 0.68.0 / CBMC 6.11.0; kani.yml pins the same). A bump is a
  deliberate chore that re-runs every doubling check — out of scope
  here (D-10).

## 4. Harness catalogue

Five harnesses, smallest-to-hardest. Expected outcomes use Kani's
vocabulary: proven / failure + counterexample / unwinding-assertion
failure. "Ride" = escalates the named DR-6 §3.9 / `recur.rs`
proptest law from 256 samples to exhaustive-within-bounds.

### 4.1 R1 — `prop_occurs_on_total_anchor_gated_clamp_structure`

- **Laws** (over symbolic `(pattern, anchor, d)`):
  1. **Totality/panic-freedom** (implicit — every reachable
     overflow/assert on the symbolic path, incl. chrono internals).
  2. **Anchor gate**: `d < anchor ⇒ !occurs_on(..)` — exercised over
     a `±255`-day `i16` d-offset (full type range, no assume): the
     gate is order-only, so every path pair-class is inside the
     window; the wide-span behaviour is R2/R3's subject (offsets),
     not `occurs_on`'s.
  3. **The anchor is a member**: `occurs_on(pattern, anchor, anchor)`
     for every arm (the base case of the chain laws).
  4. **Clamp structure** (rides P3,
     `monthly_on_day_is_exactly_one_member_per_month`,
     `recur.rs:1883`): an OnDay member's day number equals
     `min(n, days-in-month)`, against a harness-local chrono-based
     days-in-month mirror (independent formulation of the private
     `last_day_of`).
- **Symbolic surface**: pattern tag `u8` (assume ≤ 3); per §2.2
  fields — Daily `every` full `u16 ≥ 1`, Weekly interval `1..=8` +
  full 7-bit mask, OnDay/NthWeek full valid domains; anchor `i32`
  CE day in the proptest window.
- **Unwind 4** (derivation: production path loop-free; the bound
  covers the mint's 7-bit set expansion and BTree walks). Doubling
  at 8. Expected: **proven** (cheapest harness; H3-class cost).

### 4.2 R2 — `prop_next_after_strict_after_member_and_anchor_floor_proven`

- **Laws** (rides P1 + its `from < anchor` sibling,
  `recur.rs:1808`/`:1822` — bundled per the pilot's H3
  multi-law precedent; symbolic `bool` selects the side):
  1. `from ≥ anchor` side: `next > from` (strictness) ∧
     `occurs_on(pattern, anchor, next)` (membership).
  2. `from < anchor` side: `next ≥ anchor` (the least-member floor)
     ∧ membership.
  3. Totality/panic-freedom across all four arms (the
     `expect`s in `next_after` are unreachable within the §2.2
     window — if Kani finds a path to one, that is a real finding
     for the triage rule, §6).
- **Symbolic surface**: pattern tag `u8`; anchor `i32` CE day;
  forward offset `u16` assume ≤ 40 000 (parity), back offset `u16`
  assume ≤ 4 000 (parity); per-arm fields per §2.2.
- **Unwind 16** (derivation: weekly outer ≤ 9 = interval 8 + 1;
  monthly NthWeekday ≤ 4–5 with n = 5's 3-miss-month run + slack;
  monthly OnDay ≤ 2; mint/BTree ≤ 8; 16 ≥ max need with the weekly
  arm the binding constraint). Doubling at 32.
- **Expected**: **proven**; widest formula of the forward class
  (H4-class symbolic date arithmetic plus the 4-way pattern
  multiplex — still far below the killed H5 shape, which was a
  16-way match × 3 symbolic ops × containers; this is 4-way × 1 ×
  none).
- **Containment**: law-scope shrink first (drop the `from < anchor`
  side into its own harness) if the formula walls; input ranges
  never silently narrow below the table (B5/D-5).

### 4.3 R3 — `prop_roll_target_daily_laws_proven` (the near-certain win)

- **Laws** (rides P4, `roll_target_laws recur.rs:1915`, Daily arms
  only — the one pattern with loop-free `next_after` *and*
  `prev_on_or_before`, so the proptest's full sampled ranges fit
  exhaustive-within-bounds at closed-form cost):
  1. `target > stale` (the head always moves forward).
  2. `target ≤ today ⇒ occurs_on(..) ∧ next_after(target) > today`
     (catch-up lands on the latest member ≤ today).
  3. `target > today ⇒ target == next_after(stale) ∧
     next_after(stale) > today` (skip-ahead; no member existed in
     `(stale, today]`).
  4. `Daily { every: 1 } ⇒ target == today`.
  5. Determinism: `roll_target(..) == roll_target(..)` (BMC proves
     it structurally — the no-hidden-state claim; asserted because
     P4 asserts it).
- **Symbolic surface**: `every` full `u16 ≥ 1`; stale offset `u16`
  full parity 0..=20 000; gap `u16` assume 1..=2 000; anchor `i32`
  CE day window.
- **Unwind 4** (derivation: both Daily paths closed-form; bound
  covers mint/BTree-class needs only). Doubling at 8.
- **Expected**: **proven** — this harness alone satisfies the
  dispatch's "at least one harness proven" acceptance item.

### 4.4 R4 — `prop_roll_target_all_arms_laws_rung_a`

- **Laws**: the P4 set of R3, all four arms, at containment **Rung
  A**: stale ≤ anchor + 64 d (`u8` + assume), gap ≤ 56 d (`u8` +
  assume). Within the rung, every P4 case split is reachable for
  every arm (Daily gaps ≤ 30, weekly ≤ 56, monthly ≤ 31 ≤ the
  span) — the case structure is exhaustively covered; what is lost
  vs P4 is span reach (20000/2000-day gaps), and the harness doc
  comment says so verbatim: the full-range law stays proptest-owned
  (the pilot's Rung-D disposition shape).
- **Symbolic surface**: pattern tag; per-arm fields per §2.2
  (weekly interval `1..=8` binding); anchor window as everywhere.
- **Unwind 20** (derivation: weekly backward walk ≤ 64+56)/7 ÷ 1
  + 1 = 18; monthly backward ≤ (120)/28 + 2 = 6; forward walks as
  R2; mint ≤ 8 → 20 ≥ binding need 18+2 slack). Doubling at 40.
- **Expected**: **proven**, the widest formula of the chunk
  (backward walks × chrono math × pattern multiplex). Rung A
  failing the cap twice → recorded, and **Rung B** (one guarded
  attempt at stale ≤ 1 200 / gap ≤ 400, monthly walk ≤ 47 → unwind
  56) is optional protocol: attempt once, on a second kill record
  the resource ladder and stop — the full-range law remains
  proptest-owned either way (D-7).

### 4.5 R5 — `prop_next_after_chain_gap_bounded_members`

- **Laws** (rides DR-6 §3.9 item 2 — repeated `next_after`
  enumerates the schedule — in its bounded core; the full
  `chain_enumeration_equals_schedule` law (`recur.rs:1841`) stays
  proptest-owned because `schedule` allocates a `Vec` and scans:
  B4-adjacent, recorded as D-8):
  1. Seed at the anchor: `anchor < m1 < m2` where
     `m1 = next_after(pattern, anchor, anchor)`,
     `m2 = next_after(pattern, anchor, m1)` — strict increase over
     two hops (repeated application terminates and advances).
  2. `occurs_on(pattern, anchor, m1)` ∧ `occurs_on(pattern, anchor,
     m2)` — the enumeration stays on the schedule.
  3. **No-skipped-member gap law** (the claim sampling cannot
     support): `m1 − anchor ≤ max_gap(pattern)` ∧
     `m2 − m1 ≤ max_gap(pattern)` with the harness-local spec
     `max_gap`: Daily `every`; Weekly `7·interval`; Monthly
     OnDay 31; Monthly NthWeekday 91 (worst: a Jan 5th-X to a Mar
     5th-X straddling a memberless February, 31+29+31). A
     counterexample here falsifies the derivation — that is the
     harness working (§6), not a nuisance.
- **Symbolic surface**: pattern tag; per-arm fields per §2.2;
  anchor window as everywhere; no `from` symbol (seeded at the
  anchor).
- **Unwind 16** (same weekly/monthly derivation as R2, ×2 hops —
  CBMC unwinds each loop instance separately). Doubling at 32.
- **Expected**: **proven**; the gap law is the chunk's sharpest
  new statement.

## 5. Run protocol

1. **Every local invocation** through B7's guard, `/usr/bin/time -v`
   capturing peak RSS into the budget table.
2. **Per-harness development runs**:
   `cargo kani -p taskboard-domain --harness verify::<name>
   --output-format terse`; commit immediately on first green (B8).
3. **Doubling re-runs** (B6) per harness before declaring its bound;
   both numbers recorded in the harness doc comment.
4. **Final CI-shaped run** (what the weekly job executes, under the
   guard): `cargo kani -p taskboard-domain --output-format terse` —
   must report **all domain harnesses successful, including the
   pilot's H2-Rung-C / H3 / H4** (regression duty, D-9). Wall and
   peak RSS recorded; the runner envelope check is peak RSS ≤ 10 GiB
   (16 GiB runner, rustc + OS headroom) — anything above the local
   6 GiB cap cannot pass step 1 anyway.
5. **Recording**: stage 2 appends the measured table (harness / laws
   / outcome / unwind + doubling / checks failed-unreachable / wall /
   peak RSS) to its stage-2 report and updates this plan's budget
   table; every harness's doc comment carries law, symbolic surface,
   unwind + derivation, doubling numbers, and containment notes —
   the audit shape the pilot's stage-3 review used.
6. **Gates at handoff**: the standard sequence through
   `scripts/run-quality-gates.sh` (fmt, clippy `-D warnings`,
   nextest, doc tests, deny, coverage triple). They exercise nothing
   Kani — that is the point (B1) — but they must stay green.
7. **No push, no PR from stage 2**; handoff at gates green; stage 3
   re-verifies harnesses independently (pilot §8 shape) and ships.

| Harness | Peak RSS | Verify wall | Status |
|---|---|---|---|
| R1 occurs_on structure | — | — | stage 2 |
| R2 next_after P1 both sides | — | — | stage 2 |
| R3 roll_target Daily full range | — | — | stage 2 |
| R4 roll_target all-arms Rung A | — | — | stage 2 |
| R5 chain gap-bounded members | — | — | stage 2 |

## 6. Counterexample triage rule

A **FAILED** verdict with a counterexample is evidence, never a
silently weakened harness (dispatch: "the counterexample is triaged
with a recorded disposition"):

1. **Classify first** (B6): an unwinding-assertion failure is an
   inconclusive bound — raise with an updated derivation and
   re-double; it does not enter triage. Only a genuine
   counterexample (property violated on a symbolic path inside the
   documented domain) does.
2. **Check the mint before the code**: a counterexample outside the
   documented domain means the harness mis-encodes §2.2 (minting
   bug) — fix the harness, record the correction in the stage-2
   report, re-prove. A counterexample *inside* the domain is a
   candidate production finding.
3. **Concrete playback**: re-run under the guard with
   `--enable-concrete-playback inplace`; move the values into a
   plain `#[test]` named `repro_<law>_kani_cex` in `recur.rs`'s
   `#[cfg(test)]` tests, with a comment linking the harness and the
   finding (pilot plan §5 mechanism; the playback test runs on
   stable in the normal PR pipeline — verify that on first use).
4. **A new failing test can be a production bug** (AGENTS §5):
   evaluate both sides before changing anything. Fix production on
   the branch when in scope and keep the repro green; if the fix is
   out of chunk scope, the repro test stands failing, the bug is
   named in the stage-2/3 report with a proposed fix prompt, and
   AGENTS' do-not-weaken rule applies.
5. **Evidence on the issue**: every triaged counterexample (and its
   disposition: repro-test-and-fix / harness-correction /
   documented production bug) is posted as a dated comment on issue
   **#505** — the triage record lives where the normative item
   lives (D-11). A confirmed production bug that needs its own fix
   chunk gets its own issue (labels `testing` + the affected
   category), cross-linked from the #505 comment.
6. The harness then re-proves (the counterexample disappears) and
   the repro test guards the fix forever after in the normal
   pipeline — the pilot's durable payoff path.

## 7. Diff scope, CI, gates

- **Allowed in the diff**: `crates/taskboard-domain/src/verify.rs`
  (appended section after the existing H2 block, pilot one-module-
  per-crate convention); this plan; the stage-2/3 report artefacts;
  — on a counterexample only — one `#[cfg(test)]` repro test in
  `recur.rs`.
- **Not in the diff**: any production module, any `Cargo.toml` (the
  `cfg(kani)` gate and `unexpected_cfgs` registration predate this
  chunk), `.github/workflows/kani.yml` — its domain job runs
  unfiltered (`cargo kani -p taskboard-domain`), so the five new
  harnesses join the Monday 04:00 UTC sweep automatically; the job's
  120-minute budget absorbs the added wall (pilot full-domain run:
  2:54–4:12; expect the chunk to roughly double it — still well
  inside).
- **PR pipeline**: untouched by construction (no PR trigger in
  kani.yml; harnesses invisible to every gate). Handoff remains
  "PR open + `gh pr checks` green"; the user merges.

## 8. Decisions (open questions, resolved)

- **D-1 — branch base**: `origin/main` `396bd6e3`, not local `main`
  (`c21c0e9c`, 17 commits behind, two touching `recur.rs` at the
  +21 line shift). The brief's "off main" is honored by rebasing the
  fresh branch onto the fetched tip — local `main` is stale, the
  pilot cut at the fetched tip for the same reason.
- **D-2 — module layout**: extend the existing `verify.rs`; no
  `verify/recur.rs` submodule. The pilot's binding convention is one
  harness module per crate; a split is reopened only if the file
  crosses ~1 000 lines (recorded threshold, not taken now).
- **D-3 — no `test_support` reuse**: harness-local mints (B9). The
  strategies' *ranges* are mirrored as assumes with the parity note,
  not imported.
- **D-4 — date minting**: `i32` CE-day within the proptest's own
  2000–2100 window via `from_num_days_from_ce_opt` + assume-`Some`.
  CE days are the production arithmetic's own unit
  (`week_index`, `aligned` both use `num_days_from_ce`) — minting in
  that unit keeps symbolic-to-production conversion exact; the
  circuit stays the pilot's 32-bit class.
- **D-5 — domain policy per field**: full constructor-valid domain
  where the arm is loop-free (Daily `every`, OnDay, NthWeek, the
  7-bit mask — stronger than parity, same cost); proptest-parity
  where unwind forces it (Weekly interval `1..=8`, offsets). The
  escalation claim per harness is stated against exactly the domain
  its assumes define — no blanket "escalates P1" over claims the
  bounds don't carry.
- **D-6 — offset types unsigned-narrow**: `u16` (parity 40 000 /
  20 000 / 2 000 / 4 000), `u8` at Rung A (64 / 56) — types chosen
  so the bitblast is small and the assumes only document parity or
  rung narrowing (B3's cost logic).
- **D-7 — R4 ladder**: Rung A mandatory; Rung B one guarded attempt,
  honest termination on the second kill. The full-range P4 law is
  proptest-owned regardless (recorded disposition in the harness
  doc).
- **D-8 — out of scope**: `schedule` (Vec + scan), `next_occurrence`
  and `occurrence_outcome` (Task/clock-bearing, string-adjacent),
  `RecurRule::text_form` (serde-JSON String — the measured wall
  class), and the whole §3.9 items 3/5–8 catalogue (P3's schedule
  half, attribution, streaks, fold-dedup, serde round-trip). #505
  names the trio; the rest stays with its proptests. Recorded so the
  boundary is not re-derived.
- **D-9 — regression duty**: the final CI-shaped domain run must
  re-prove the pilot's three domain harnesses alongside the new five
  — a shipped harness that stops proving is a stage-2 blocker, not
  a note.
- **D-10 — no Kani bump**: 0.68.0 locally and in kani.yml; a bump
  re-runs every doubling check and is its own chore.
- **D-11 — triage evidence home**: issue #505 comments (§6.5), not a
  new backlog issue, unless a confirmed production bug needs a fix
  chunk of its own.

## 9. Risks

1. **Symbolic date-arithmetic width (medium)**: the recurrence path
   is H4-class chrono math multiplexed 4-way by the pattern tag.
   Mitigation: expected-cost ordering (R3 cheapest certain win first),
   law-scope shrink ladder per harness (R2's sides split; R4's
   rungs), never silent input narrowing (B5).
2. **R4's backward-walk unwind × body cost (medium)**: unwind 20–56
   over chrono-heavy bodies is past the pilot's proven envelope
   (max 12). Mitigation: the Rung A/B ladder with the two-kill rule
   (B7); honest termination is a sanctioned exit, the full-range law
   is already proptest-owned (D-7).
3. **`max_gap` derivation wrong (low, self-correcting)**: R5's gap
   law encodes a calendar claim (91-day NthWeekday worst gap). A
   wrong derivation surfaces as a loud counterexample or unwinding
   assertion → §6 triage, derivation corrected — never silently
   relaxed to make the harness pass.
4. **Chrono hidden loops (low)**: H4's proven unwind-8 run is
   evidence the date paths are loop-light; Kani's default
   unwinding assertions make any surprise loud (stage-2 report §4).
5. **SonarCloud duplication gate (medium, known)**: it flagged the
   pilot's harness code (17.5% new-code duplication). Mint helpers
   and independent spec mirrors are mandated by B9; stage 3 watches
   the Sonar check on the PR.
6. **Concurrent worktree sweeps (hit twice in the pilot's round)**:
   this plan commit is the branch's doctor-proofing commit; stage 2
   commits each first-green harness immediately (B8).
7. **`origin/main` drift during the chunk** (17 commits landed here
   while local `main` sat still): stage 3 rebases per AGENTS §8
   before opening the PR; single-PR chunk, no stacked-rebase dance.
8. **Weekly CI wall growth (low)**: five new harnesses roughly
   double the domain job's wall (est. ≤ 8–10 min vs the 120-min
   budget); if R4 lands at the high end repeatedly, the follow-up
   (per-family harness filter in the job) is an issue-for-issue,
   not a this-chunk change.

## 10. Stage-2 execution order

1. Worktree hygiene: confirm the branch still exists after any
   concurrent doctor sweep; recreate per dispatch §1 if swept.
2. R3 first (near-certain proven, satisfies the acceptance bar
   immediately, commit on green).
3. R1, then R5 (cheap structure + the sharp gap law).
4. R2 (widest forward formula), then R4 descending its rung ladder.
5. Doubling re-runs as each bound is declared (B6); budget table
   (§5) updated as measurements land.
6. Final CI-shaped domain run incl. pilot-harness regression duty
   (D-9); gates through `scripts/run-quality-gates.sh`.
7. Stage-2 report (verdict table, ladders, deviations, §6 evidence
   if any); handoff at PR open + checks green (stage 3).

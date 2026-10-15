# [Plan] Deterministic Stamps for Clock-Gated Fixture Builders (Chunk P7)

- **Date**: 2026-10-15
- **Kind**: stage-1 implementation plan (TDD-first; this commit is plan
  only — no fixture changes, no gate deletions yet).
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md`
  §1 (execution model, §1.4 GUI no-go zones — note `miri.yml` is a
  **P8-only** shared file) and §8 chunk P7; backlog entry
  "[2026-10-12] Deterministic stamps for clock-gated fixture builders"
  (`.artifacts/taskboard-potential-improvements-backlog.md:2256`),
  itself originating from the WI-C review of green Miri run 38312493624;
  `docs/testing_strategy.org` §10 (the gate-scope bullet this chunk
  rewrites).
- **Branch / worktree**: `test/domain-deterministic-stamps` in
  `../taskboard-WT-test-domain-deterministic-stamps`, off `main` @
  `9b0239b` — verified equal to `origin/main` at planning time.
- **Goal**: replace the three Miri-gated tests' *fixture* clock reads
  (`SystemClock.now()` ×5, `Utc::now()` ×2, `Uuid::now_v7()` ×3) with
  fixed, distinct, strictly increasing stamps and fixed
  `Uuid::from_u128` ids, then delete exactly those three
  `#[cfg_attr(miri, ignore)]` gates so the PendingOp serde roundtrip,
  the read-port routing contract, and the whole repository-contract
  harness gain interpreted coverage. Zero production-code changes; no
  assertion is touched.
- **Status**: awaiting stage-2 implementation.

---

## 1. Verified starting point (anchored on `main` @ 9b0239b)

1. **The domain crate carries exactly five Miri gates; three are
   fixture-avoidable, two are inherent** (grep `cfg_attr(miri` over
   `crates/taskboard-domain/`):
   - `test_support/contract.rs:452` — `in_memory_repository_satisfies_
     contract`; the harness stamps five `SystemClock.now()` outbox
     entries (sites `:89`, `:177`, `:218`, `:223`, `:234`). **Dies.**
   - `test_support/memory.rs:87` — `in_memory_read_state_returns_the_
     post_apply_shape`; one `chrono::Utc::now()` at `:95` (ids already
     `from_u128`). **Dies.**
   - `outbox.rs:92` — `pending_op_roundtrips_through_serde`;
     `Uuid::now_v7()` ×3 + `Utc::now()` at `:94-98`. **Dies.**
   - `clock.rs:40` — `system_clock_returns_utc_instant`; the *subject*
     is `SystemClock` (`Utc::now()` in production code, `clock.rs:28`).
     **Stays** (inherent).
   - `idgen.rs:66` — production `UuidV7Generator` reads the realtime
     clock per id (`idgen.rs:31-57`). **Stays** (inherent).
   No other realtime-clock read exists anywhere in the crate (grep
   `Utc::now|now_v7|SystemTime|SystemClock` — the five test sites, the
   two inherent production seams, and doc comments only), so un-gating
   the three cannot ambush the core job with a fourth abort.
2. **Outbox stamps are pure fixture data in every consumer of these
   three tests.** In-memory `apply_actions` appends positionally and
   replaces in place by `op_id` (`persistence.rs:250-258`); the sqlite
   adapter hydrates `ORDER BY op_seq` (`repo.rs:478`), and its own
   roundtrip test pins that *identical* `queued_at` seconds collide
   harmlessly (`storage-sqlite/tests/roundtrip.rs:183`) — neither
   implementation orders by `queued_at`. (`pipeline.rs` does compare
   `op.queued_at <= read_at` (`:110`, `:217`, `:289`), but the
   pipeline tests construct their own ops with fixed `ts(BASE)`-style
   stamps and are untouched by this chunk.)
3. **The house fixed-stamp idiom already exists in this crate**:
   `fn ts(secs: i64) -> DateTime<Utc> { Utc.timestamp_opt(secs, 0)
   .unwrap() }` (`persistence.rs:328`); `pipeline.rs`'s test module
   builds every fixture op with `ts(BASE)` / `ts(BASE ± n)`
   (`pipeline.rs:1181` etc.); `contract.rs` itself already fixes every
   non-outbox timestamp to the `1_700_000_0xx` second-block
   (`last_success` `1_700_000_000` at `:184`, reminder section
   `1_700_000_100`–`1_700_009_999` at `:271-324`). The five
   `SystemClock.now()` calls are the harness's *only* nondeterminism.
4. **The contract harness is shared** with
   `taskboard-storage-sqlite/tests/contract.rs:6,12,22`, which runs it
   natively against `:memory:` and file-backed sqlite in the PR
   pipeline. Fixed stamps make those runs strictly more deterministic;
   nothing there compares `queued_at` to the wall clock.
5. **The un-gated tests land in the cheap core Miri job**
   (`miri.yml:43-60`, filter `--skip pipeline:: --skip remote::
   --skip state::`, `PROPTEST_CASES=16`), measured ≈ 57 min wall
   against the ≈ 90-min headroom bar (testing_strategy §10,
   2026-10-12). The three tests are pure in-memory logic (Mutex over
   `PersistedState`, serde over a small struct) — expected interpreted
   cost is seconds-to-minutes, not a budget threat. `test_support`
   compiles under `cfg(test)` for the crate's own tests
   (`lib.rs:47-48`, `#[cfg(any(test, feature = "test-support"))]`), so
   the job sees them.
6. **§1.4 no-go sweep**: the diff touches only domain test-support /
   `cfg(test)` code, `docs/testing_strategy.org` §10's gate bullet,
   and the backlog entry. No GUI-owned file, no production module, and
   — critically — **not `miri.yml`** (dispatch §1.4 shared-file table:
   P8 only).

## 2. Decisions

**D1 — Fixed-stamp scheme: the `1_700_100_0xx` second-block, one
second per stamp, strictly increasing by enqueue order.**

| # | Fixture (contract.rs site) | Stamp |
|---|---|---|
| 1 | `op` (CreateTask, op_id 3) — `:89` | `ts(1_700_100_001)` |
| 2 | `op2` (UpdateTask, op_id 4) — `:177` | `ts(1_700_100_002)` |
| 3 | `first` (CreateStack, op_id 5) — `:218` | `ts(1_700_100_003)` |
| 4 | `second` (CreateLabel, op_id 6) — `:223` | `ts(1_700_100_004)` |
| 5 | `replacement` (RenameStack, re-enqueue of op_id 5) — `:234` | `ts(1_700_100_005)` |

Rationale, recorded: (a) *distinct and strictly increasing* so any
timestamp-ordering semantics stay exercised — the dispatch's binding
requirement; (b) `replacement` (site 5) carries the **latest** stamp
while occupying `first`'s (site 3) queue position, mirroring real-clock
behavior and sharpening the position-vs-stamp distinction the
re-enqueue clause pins (`after.outbox == vec![replacement, second]`,
`contract.rs:240-244`); (c) the block sits in the same fixed-recent-
past region as the file's existing `1_700_000_0xx` stamps but in its
own clean sub-block, so an outbox stamp can never be visually (or
literally — harmless but misleading) confused with a reminder-section
stamp; (d) whole seconds, matching every other fixed stamp in the file
and sqlite's storage resolution.

**D2 — Helper routing in `contract.rs`: add the house `ts(secs)`
helper beside the existing `*_at` family; drop the `Clock` import.**

```rust
/// Fixed `queued_at` stamp for enqueued ops: whole seconds in the
/// harness's fixed-stamp block, called with strictly increasing
/// values so outbox ordering semantics stay exercised without a
/// realtime clock (Miri-clean fixture, testing_strategy §10).
fn ts(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).unwrap()
}
```

The name and shape are exactly the crate's established helper
(`persistence.rs:328`); the existing `clock_at` / `stack_clocks_at` /
`label_clocks_at` (`contract.rs:401-431`) stay as-is — they build
compound entity clocks, `ts` yields the bare instant. The five sites
become `queued_at: ts(1_700_100_00n)`. `use crate::clock::{Clock,
SystemClock};` (`contract.rs:14`) becomes unused and is removed (it
was the harness's only clock reference). The clock *trait* seam needs
no injection here: the harness constructs `PendingOp` values directly,
so a `FixedClock` object would be ceremony around a constant — the
backlog's "`clock_at`-style helper" routing is exactly this function.

**D3 — `outbox.rs` roundtrip fixture: fixed `Uuid::from_u128` ids +
one fixed stamp.**

```rust
let op = PendingOp {
    op_id: OpId(Uuid::from_u128(1)),
    op: LocalOp::AssignLabel(
        TaskId::from(Uuid::from_u128(2)),
        LabelId::from(Uuid::from_u128(3)),
    ),
    queued_at: Utc.timestamp_opt(1_700_100_011, 0).unwrap(),
};
```

with `use chrono::TimeZone;` added to the test module (for
`timestamp_opt`). Sequential small u128s match the crate-wide
`CountingIds`/contract idiom. **Fidelity note, recorded**: `OpId` is
documented UUIDv7 and `from_u128(1)` is not — accepted, because the
test's subject is serde symmetry, and uuid's serde representation is
version-agnostic (same canonical string either way); real v7
*generation* stays covered by `idgen.rs`'s tests (natively — its gate
is inherent and stays). This is the backlog's sanctioned
`Uuid::from_u128` approach verbatim. The stamp reuses D1's block at a
distinct offset (`…011`) purely for greppability; ordering is moot in
a single-op serde test, so no helper is warranted for one literal.

**D4 — `memory.rs` read-port fixture: one fixed stamp inline.**

`queued_at: chrono::Utc::now()` (`:95`) becomes
`Utc.timestamp_opt(1_700_100_021, 0).unwrap()` with
`use chrono::TimeZone;` in `read_port_tests`. The op's ids are already
`from_u128(1)`/`from_u128(2)`. Single site, ordering irrelevant (the
assertion is `read_state == snapshot`), so like D3 it stays an inline
literal rather than importing contract.rs's helper — one cross-module
`pub(crate)` promotion for one call would be coupling without benefit.

**D5 — Gate deletions: exactly three lines die, two stay.**

Delete `#[cfg_attr(miri, ignore)]` + its comment at `contract.rs:452`,
`memory.rs:87`, `outbox.rs:92`. Keep `clock.rs:40` and `idgen.rs:66`
verbatim — their comments already state the inherent reason. Do **not**
touch any state-crate gate (insta socketpair, timer wheel,
`sqlite_smoke`, live-engine properties) — out of scope. Do **not**
touch `miri.yml` (P8-only file): if the verification run shows a
budget problem, that is an escalation to the dispatch owner, never a
unilateral cap edit.

**D6 — `testing_strategy.org` §10 gate bullet: rewrite to the
post-P7 scope.** Replace the bullet at lines 311-319 (current text
enumerates all five gates) with:

```org
- *Realtime-clock tests carry scoped =#[cfg_attr(miri, ignore)]=
  gates* (isolation provides no =clock_gettime(REALTIME)=). Fixture
  builders must not read the clock: they stamp fixed, distinct,
  strictly increasing timestamps (the repository contract harness's
  =ts()= block) so ordering semantics stay exercised while the tests
  run interpreted — the former fixture gates (PendingOp roundtrip,
  read-port contract, repository contract harness) moved to fixed
  stamps on 2026-10-15 and now run interpreted. The remaining domain
  gates are inherent — production code itself reads the clock:
  =clock.rs= (SystemClock) and =idgen.rs= (production
  =UuidV7Generator=). Each gated test runs natively unchanged;
  existing gates follow the same pattern (insta S-series socketpair,
  =sqlite_smoke= tempdir+FFI, P4/P5 live-engine properties).
```

(P8 appends its cap-policy wording to a different bullet — dispatch
§1.4 expects only adjacency, trivially rebased.)

**D7 — TDD loop for a test-infrastructure change.** The meaningful
red/green is *under the interpreter*, not natively (natively nothing
can fail: no assertion references the stamp values — §1.2/§1.4
evidence):

1. *Red*: delete the three gates first, then run the targeted
   interpreter slice locally —
   `cargo +nightly miri test -p taskboard-domain --lib --tests -- outbox:: test_support::`
   — and observe the aborts (isolation denies `clock_gettime`);
   `SystemClock`/`Uuid::now_v7` aborts are the failing state.
2. *Green*: apply D1-D4; the same command passes; the full gate suite
   stays green.
   If nightly+miri is not installed locally, stage 2 records precisely
   that (`rustup component list --toolchain nightly | grep miri`
   output) and the red evidence rides stage 3's authorized
   `workflow_dispatch` run instead — the dispatch P7-2 prompt allows
   exactly this skip. The native suite is the regression net either
   way: `cargo nextest run -p taskboard-domain -p
   taskboard-storage-sqlite` before and after must be identically
   green (the storage crate re-runs the changed harness).

**D8 — Bug-vs-test rule (AGENTS §5), pre-applied.** A full sweep of
every consumer found nothing that compares these fixtures' `queued_at`
to a wall-clock value or orders by it (§1.2), so the expected outcome
is green across the board. If anything still fails under fixed stamps
— domain contract, storage contract, or a snapshot — treat it as a
production bug or a fixture-coupling bug with equal probability,
investigate, and report; **never weaken the assertion**. The one
known semantic nuance (replacement's later stamp in first's slot) is
preserved by D1, not papered over.

**D9 — Commit structure (rebase-merge hygiene; every commit
standalone):**

1. `test(domain): fixed fixture stamps drop three miri gates` —
   contract.rs + memory.rs + outbox.rs + the three gate lines +
   the §10 bullet (the doc names exactly the gates this diff deletes;
   splitting them would leave commit 1's diff contradicting the
   checked-in scope note).
2. `docs(backlog): deterministic fixture stamps done` — the backlog
   ✅ DONE note (stage 2's final bookkeeping commit).

plus this plan commit (stage 1). No fixup commits; amend before the
PR opens if review finds anything.

## 3. Implementation sketch (stage 2)

1. Baseline: `cargo nextest run -p taskboard-domain -p
   taskboard-storage-sqlite` green; record it.
2. Red per D7.1 (or record the local-miri skip).
3. `contract.rs`: add `ts` (D2) below `label_clocks_at`; replace the
   five `SystemClock.now()` sites per D1's table; remove the
   `use crate::clock::{Clock, SystemClock};` line; delete the
   `:452` gate. Also delete the stale trailing comment mention if the
   harness doc-comment references clock stamping (check the module
   doc — currently it does not).
4. `memory.rs`: D4 + delete the `:87` gate.
5. `outbox.rs`: D3 + delete the `:92` gate.
6. Green per D7.2; then the full AGENTS §10 gate suite
   (`fmt --check`, clippy `--workspace --all-targets -D warnings` —
   the removed import is the one clippy/rustc trap —, nextest
   workspace, doc-tests, deny, llvm-cov).
7. Confirm the crate-wide gate inventory reads exactly two
   `#[cfg_attr(miri, ignore)]` in `crates/taskboard-domain/`:
   `grep -rn "cfg_attr(miri, ignore)" crates/taskboard-domain/`.
8. Commits per D9; backlog ✅ DONE note. No push.

## 4. Verification & acceptance (from dispatch §8)

- Exactly three gates deleted; the two inherent gates remain (the
  stage-3 grep in §3.7 is the check).
- All touched tests green natively (domain + storage crates — the
  harness is shared).
- Full AGENTS §10 gate suite green.
- One `workflow_dispatch` run of `miri.yml` (any branch — stage 3
  pushes the branch for it, per the dispatch authorization) evidences
  the newly-interpreted tests passing and the core job's wall time
  still inside the ≈ 90-min bar; run URL goes in the PR body.
- Backlog entry ✅ DONE with a one-line note.

## 5. Risks & mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| Core Miri job budget creep from the newly-interpreted harness | Low (pure in-memory logic; ≈ 33 min headroom) | Stage 3's dispatch run checks the wall time; a breach escalates to the dispatch owner — `miri.yml` is P8's file, never edited here (D5) |
| A fixed stamp breaks a hidden consumer | Low (§1.2 sweep: positional ordering everywhere, no wall-clock comparisons) | AGENTS §5 bug-vs-test evaluation; report, never weaken (D8) |
| Local nightly+miri unavailable | Possible | D7's recorded-skip path; stage 3's CI run is the evidence |
| uuid fidelity objection (OpId not v7 in the roundtrip fixture) | None functionally | D3 records why serde coverage is version-agnostic; idgen tests keep v7 generation covered |
| §10 adjacency collision with P8's cap paragraph | Expected, trivial | Dispatch §1.4 already names it; rebase keeps both bullets |

## 6. Non-goals (fences)

- Zero production-code changes — the diff stays inside
  `#[cfg(any(test, feature = "test-support"))]` modules and
  `#[cfg(test)]` blocks. The harness body is test-support code
  consumed only by tests; shipped binary behavior is bit-identical.
- No `miri.yml` edits (P8-only), no ci.yml, no state-crate gates, no
  cap changes, no new helpers exported publicly, no GUI-owned file.

## 7. Estimated diff

≈ 35 changed lines across 5 files: `contract.rs` ~12 (helper + 5
sites + import + gate), `memory.rs` ~4, `outbox.rs` ~7,
`testing_strategy.org` ~9 (one bullet), backlog ~3. One plan commit
(this), two implementation commits (D9).

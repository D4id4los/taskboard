# State/Storage Small Residue — Chunk E2 Plan

- **Date**: 2026-11-22 (written 2026-11-23)
- **Round**: 5, chunk E2
- **Branch**: `fix/state-storage-small-residue`
- **Scope**: #1211, #1157, #1158, #1137, #1095
- **Normative inputs**: round-5 scope report §E
  (`.artifacts/reports/2026-11-22-backlog-reduction-round5-scope-report.md`),
  dispatch plan §9 E2, the five issue bodies (normative per [L](c)),
  the clean_ambient re-homing decision (`.artifacts/plans/2026-11-21-boot-fixture-residue-plan.md`
  §2 item 1, landed as #1113), the D-B2-6 watch-channel decision
  (`.artifacts/plans/2026-11-19-journal-decode-robustness-plan.md` §212),
  the B1 clock-bump rule (`.artifacts/plans/2026-11-19-domain-loghabit-tie-plan.md`).
- **Acceptance oracle**: per-item typed fixes; the hostile-env pass green
  (tests run with a hostile `TASKBOARD_*` ambient present); diffs confined
  to the named seams.

---

## 1. Item decisions

### 1.1 #1211 — zero-duration snooze: typed refusal at both boundaries

**Seam** (diagnosed): the GUI draft mints `HabitSnooze` only for a nonzero
count (`filter(|n| *n > 0)`; the save gate greys), while
`parse_reminder_override` (`crates/taskboard-app/src/cli.rs`, `snooze` arm)
parses `u8` and accepts `snooze=0h`/`snooze=0d` — a degenerate snooze whose
chain walk moves the occurrence by zero. `HabitSnooze`
(`crates/taskboard-domain/src/habit.rs:252`) is a plain `enum { Hours(u8),
Days(u8) }` with no constructor law: zero is representable.

**Decision (rule, stage-1; the A1 lens): typed refusal at both boundaries,
minted once in the domain.**

- Add fallible minting constructors on `HabitSnooze`:
  `pub fn hours(n: u8) -> Option<Self>` and `pub fn days(n: u8) ->
  Option<Self>`, returning `None` for `n == 0`, with the documented
  rule: *a zero-duration snooze is degenerate (the snooze chain walk
  moves the occurrence by zero) and is unmintable*.
- **CLI boundary** (`cli.rs`): route the parsed count through the mint;
  `None` is a typed refusal — extend the existing parse error shape
  (this parser returns `Result<_, String>` at the clap seam; the refusal
  message states the zero rule — message text is not asserted in tests).
- **GUI boundary**: the draft's `filter(|n| *n > 0)` hand-rolled gate is
  replaced by the same mint — the save gate greys because the mint
  refused, not because of a locally duplicated predicate. No UI logic
  moves; the UI still only dispatches events.
- **A1 record**: the house pattern is a hand-rolled fallible minting
  constructor at the trust boundary; no derive crate, no validate-and-
  forget (both call sites *consume* the `Option`). The enum variants stay
  public with `u8` payloads.
- **Deliberate leniency, documented at the type**: deserialization
  (persisted-state / journal decode) keeps the raw variant
  construction — decode is not a rule decision boundary, and journals
  minted before this law may legitimately carry a zero snooze; decode
  must not refuse to load history. See the backlog issue in §3 for the
  residual (pruning/normalizing historical zero-snooze rows).
- **Tests**: domain unit test for the mint (`None` at 0, `Some` at ≥1);
  CLI parse test asserting the typed refusal outcome for `snooze=0h` /
  `snooze=0d` (structural: `parse_reminder_override` errs, matching the
  existing parse-error test pattern — no message-text assertions per the
  no-text-assertions rule); existing GUI-side tests keep passing (the
  filter → mint swap is behavior-identical for n ≥ 1).

### 1.2 #1157 — finish the clean_ambient re-homing: re-home all seven legs

**Seam** (diagnosed): seven `clean_ambient()`-gated legs remain inline in
`crates/taskboard-app/src/ui_requests.rs` (lines 2448, 2515, 2580, 2622,
4314, 4355, 4380 — the legs named in the issue body). The #1113 precedent
re-homed the three env-composing legs into
`crates/taskboard-app/tests/services/ui_settings.rs` behind
`crate::common::taskboard_env_lock()`.

**Decision: option (a) for all seven — re-home onto the env-lock-holding
integration file; no per-leg un-gating justifications.**

- Rationale: the issue's option (b) (un-gate with a recorded
  justification that the leg's reads do not compose the env) requires a
  per-leg proof that survives future refactors; option (a) is uniform,
  removes the inline-gate class entirely, and lets `clean_ambient` be
  deleted in the same diff. All seven legs live in the same module and
  exercise `ui_requests` services the `ui_settings.rs` integration file
  already mounts (real engine + connection workspace); the port legs
  (`the_port_*`, `focus_gate_*`) move with their local helpers.
- After the last caller leaves, **delete `clean_ambient`** and its dated
  doc comment (the comment currently points at #1157 — that pointer is
  removed with it).
- Tests move verbatim where possible (module-private helpers move with
  them; anything used by remaining inline tests stays); the integration
  file gains the legs behind `taskboard_env_lock()`. Routing-only
  duplication is not introduced (one behavior, one test, lowest layer:
  the layer changes from in-file unit to integration, which is the
  re-homing, not duplication).
- Acceptance: hostile-env pass green — with a hostile `TASKBOARD_*` var
  in the ambient, the moved legs run (they hold the env lock and set
  their own ambient) and no inline test is silently skipped
  (`clean_ambient` no longer exists to skip them).

### 1.3 #1158 — derive_sync_states phantom rows for unbound boards: record-why

**Seam** (diagnosed): `derive_sync_states`
(`crates/taskboard-domain/src/state.rs` ~414) does
`sync.0.entry(*board).or_default()` for *every* board key, including
unbound/tombstoned boards. The rows exist to carry the attributed
`pending_ops` count the dead-board drain and `read_scope` read; the
aggregate sums rows, so idle phantom rows (Idle, `last_success: None`)
are otherwise harmless and are never written to storage (a display
derivation).

**Decision: record-why — no behavioral change.**

- The issue itself gates the split on "only if it ever matters"; nothing
  downstream observes the difference today (the #1080 fix already kept
  phase/`last_success` bookkeeping off unbound boards). Splitting the
  derived pending-ops counter from the per-bound-board contact row would
  reshape `SyncStates` for zero observable gain — API churn on an
  internal API is fine *when it buys something*; here it does not.
- The record: extend the existing doc comment on `derive_sync_states`
  with the invariant — *the map keys on every board, not only bound
  boards: a phantom default row (Idle, no last success) exists for
  unbound boards solely to carry an attributed pending-op count; phase/
  last_success are only ever set on bound boards (post-#1080); rows are
  never persisted. Splitting the counter from the contact row is ruled
  out until a consumer distinguishes them.* A unit test pins the
  documented shape (unbound board: row is exactly the default, and a
  pending op attributed to it still counts) if not already pinned by the
  #1080 tests — check first; do not duplicate (one behavior, one test).
- At-merge closure comments the issue with this decision.

### 1.4 #1137 — the diagnostics hop's Mutex cell: **already landed — verified, no-op**

**Diagnosed**: the issue is CLOSED (closed at the verified merge of PR
#1173, round-4 chunk S5). The current tree already carries the plan-named
replacement: `SqliteTaskRepository.diagnostics` is
`Arc<tokio::sync::watch::Sender<Option<LoadDiagnostics>>>`
(`crates/taskboard-storage-sqlite/src/repo.rs:63`), consumed via
`has_changed()` + `borrow_and_update()` in the `LoadDiagnosticsSource`
impl, with the Mutex rationale documented at the field. No `Mutex`
remains on the hop.

**Decision: no code.** The plan records the verification (field type,
trait impl, engine consumption at
`crates/taskboard-state/src/actor.rs:347`) so stage 2/3 and the
dispatcher do not re-scope it. The dispatcher's at-merge list
(dispatch plan §12, "E2 → #1211, …") drops #1137 from E2's closables —
it is already closed.

### 1.5 #1095 — the habit-log LWW guard's whole-second strings: normalize at the write boundary

**Seam** (diagnosed): the habit-log upsert guard
(`crates/taskboard-storage-sqlite/src/repo.rs` ~2756) compares
`WHERE excluded.clock > habit_log.clock` on RFC 3339 **strings**.
Lexicographic == chronological only for whole-second timestamps
(`…:00.5Z` sorts *before* `…:00Z`). Correct today: all row clocks are
whole seconds (and the B1 mint-side clock bump —
`.artifacts/plans/2026-11-19-domain-loghabit-tie-plan.md` — composes:
the bump preserves whole-second grammar and monotonicity per command).

**Decision: typed normalization at the storage boundary — the guard's
grammar is made correct by construction, not merely observed.**

- The persisted clock is written via
  `to_rfc3339_opts(SecondsFormat::Secs, true)` (whole seconds, `Z`) at
  the habit-log upsert (and read-side parsing already accepts RFC 3339).
  Any sub-second input is thereby refused precision at the boundary —
  an A1-flavored normalization: the string comparison's validity no
  longer depends on an upstream invariant it cannot see.
- A dated invariant comment at the guard states the law: *clocks in
  `habit_log` are whole-second RFC 3339 UTC (`SecondsFormat::Secs`);
  lexicographic ordering is chronological under that grammar. Sub-second
  precision would break this guard and is refused at the write boundary;
  if a future rule ever wants sub-second clocks (#986 rejected it), this
  guard must move to an epoch-integer comparison first.*
- **Tests**: a property test (`proptest`, pure comparison logic) that
  under the `SecondsFormat::Secs` grammar, string ordering equals
  `DateTime` ordering for arbitrary instants; a unit test that a
  fractional-second authored clock is stored whole-second (the
  normalization is observable). If the guard logic is extracted into a
  testable pure helper for this, keep it minimal — prefer testing via
  the repo round-trip if the SQL is the guard.
- The B1 composition note: the bump rule operates before the boundary
  and is unaffected; no interaction beyond the shared whole-second
  grammar.

---

## 2. Implementation order (stage 2 guidance)

1. #1157 re-homing (largest, mechanical; run the hostile-env pass
   immediately after).
2. #1211 mint + both boundaries.
3. #1095 normalization + property test.
4. #1158 doc record (+ pin test only if not already covered).
5. #1137 verification is already done (this plan); no stage-2 work.

Gate: pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest run
--workspace`; commit before any wait; if the lock is held, report
"gate deferred: <holder>" per [D].

## 3. Backlog issues opened by stage 1

- (listed in the stage report) — historical zero-snooze `HabitSnooze`
  rows may exist in journals minted before the #1211 law; decode is
  deliberately lenient; a future normalization/pruning pass is
  out-of-scope here.

## 4. Risks

- **#1157 is the bulk of the diff**: moving seven legs may surface
  hidden helper coupling (shared fixtures used by tests that stay).
  Mitigation: move helpers only when exclusively used by moved legs;
  shared fixtures stay in place and are imported.
- **#1211 GUI-side**: the `filter(|n| *n > 0)` swap must not change the
  save-gate behavior for n ≥ 1; snapshot/UI tests are the oracle.
- **#1095**: any existing row written with a non-canonical RFC 3339 form
  (e.g. `+00:00` offset) still parses; only the *write* grammar is
  pinned — no migration needed (whole-second invariant already held).
- **#1137 scope confusion**: the dispatch plan §9 lists it under E2, but
  it is closed/landed; stage 2 must not "fix" it again.

## 5. Estimated diff size

- #1157: ~400–600 lines moved (seven legs + helpers + `clean_ambient`
  deletion), low churn.
- #1211: ~40–60 lines (domain mint + two call sites + tests).
- #1095: ~30–50 lines (normalization + comment + tests).
- #1158: ~15 lines (doc comment; maybe a pin test).
- Total: ~500–700 lines, mostly mechanical relocation.

# [Plan] `execute_batch` Exhaustive Dispatch — Chunk 8, Remediation Wave 1

- **Date**: 2026-10-17
- **Kind**: implementation plan (stage 1 of the three-stage dispatch protocol)
- **Origin**: issue #243 (normative; `backlog: Audit: execute_batch
  exhaustive-match restructure`), finding **F5-03** (P2, axis E) in
  `.artifacts/reports/2026-10-17-v0.1.0-audit-findings-report.md`
  (§findings row F5-03, §agenda row E-1 — **schedule-now**, "structural
  insurance"), evidence chain in
  `.artifacts/reports/2026-10-16-v0.1.0-audit-crate-storage-sqlite-report.md`
  (action-dispatch map §Axis-E table + F5-03 entry), dispatched as chunk 8 of
  `.artifacts/plans/2026-10-17-remediation-wave1-dispatch-plan.md` (§1/§9).
- **Status**: READY — every open question settled below with a recorded
  decision. Evidence anchors verified against `main` @ `6070f90`
  (chunk 4's PR #318 is merged; the 4 → 8 same-crate chain condition is
  satisfied, no rebase owed at dispatch time).
- **Branch/worktree**: `refactor/execute-batch-exhaustive` @
  `../taskboard-WT-refactor-execute-batch-exhaustive`.

## 1. The finding, precisely

`crates/taskboard-storage-sqlite/src/repo.rs:683-1092` — `execute_batch`
dispatches a `&[PersistenceAction]` through **13 fixed-order partition
passes**, each a `for action in actions { if let <Variant> = action { …SQL… } }`
chain. A variant the pass does not name falls through silently: today all 14
domain variants are handled (audit §8-D6, verified 14/14), but the structure
has no compiler check. If a future `PersistenceAction` variant lands in the
domain crate without a storage arm, the whole action class is **silently
unpersisted while `apply` still returns `Ok(())`** — the audit's one
P0-shaped structure risk, graded P2 only because coverage is complete today.

The domain side of the seam is **already compiler-checked**:
`apply_actions` (`crates/taskboard-domain/src/persistence.rs:237-307`) is an
exhaustive `match` over the enum **in the same crate as its definition**, so
a new variant cannot exist without executable semantics. The storage side is
the only unprotected half. (The same-crate placement is exactly why the
domain needs no new mechanism — see §4.)

## 2. The seam today (facts the design must respect)

1. **The partition order is load-bearing semantics, not style.**
   - FK graph: boards → stacks/labels → tasks (`repo.rs:658-663`, plan
     decision 6), and reminder rules reference tasks (`repo.rs:972-977`).
     SQLite enforces it: `foreign_keys(true)` (`connect.rs:27`). A batch
     arriving as `[UpsertStack(new), UpsertBoard(new)]` succeeds today
     *because* the board partition runs first regardless of action order.
   - Op removals before appends (`repo.rs:873-874`): a batch
     `[EnqueueOp(x), CompleteOp(x)]` keeps `x` (delete executes first, then
     the insert), while single-pass batch-order execution would delete it.
     The comment states the intent: "a batch that completes and re-enqueues
     the same id keeps the new entry".
   - Within each partition, batch order is preserved (last write of a field
     wins exactly as the engine issued it).
2. **Full partition order** (13 passes @ `6070f90`): boards `:687` →
   stacks `:716` → labels `:756` → tasks + label-join rewrite `:794` →
   op removals (`CompleteOp`|`FailOp`, one shared or-pattern chain) `:875` →
   op appends `:888` → validators `:923` → sync status `:944` →
   reminder-rule upserts `:978` → rule deletes + dependent pruning `:1002` →
   deliveries `:1030` → snoozes `:1049` → mutes `:1067`.
3. **The enum** (`taskboard-domain/src/persistence.rs:177-215`): 14
   variants, `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`,
   **no `#[non_exhaustive]`** — downstream crates may match it exhaustively.
4. **Behavior anchors** (must pass unmodified): storage crate
   `tests/roundtrip.rs` (incl. the `persisted_state_roundtrips_through_sqlite`
   proptest and `outbox_preserves_enqueue_order_across_removals`, which pins
   the removal/append partition semantics), `tests/contract.rs` (port
   contract over memory + file), `tests/actor.rs`, chunk 4's decode-rejection
   and concurrency additions; the reminder partitions have **no crate-level
   test** but are exercised end-to-end by `taskboard-app`'s
   `tests/cli_reminders.rs` (typed receipts proven across CLI invocations on
   one real tempdir db — i.e. through `execute_batch`'s reminder passes and
   `load`).
5. `execute_batch` is the crate's only `PersistenceAction` dispatch site
   (workspace census: `port.rs`/`connect.rs` mention the type in signatures
   and test fixtures only).

## 3. Decision: one exhaustive pre-partition `match`, partition execution unchanged

**Mechanism.** At the top of `execute_batch`, one `for action in actions`
loop with a **single exhaustive `match`, no wildcard arm**, that pushes each
action's borrowed payload into a per-partition buffer; the 13 existing
partition bodies then iterate their own typed buffer instead of re-scanning
`actions` with `if let`. Buffer per arm:

| Match arm (14 variants → 13 arms) | Buffer | Feeds pass |
|---|---|---|
| `UpsertBoard(b)` | `Vec<&Board>` | boards |
| `UpsertStack(s)` | `Vec<&Stack>` | stacks |
| `UpsertLabel(l)` | `Vec<&Label>` | labels |
| `UpsertTask(t)` | `Vec<&Task>` | tasks (+ joins) |
| `CompleteOp(id) \| FailOp(id)` (one arm) | `Vec<&OpId>` | op removals |
| `EnqueueOp(op)` | `Vec<&PendingOp>` | op appends |
| `UpsertValidators(k, v)` | `Vec<(&ValidatorKey, &SyncValidators)>` | validators |
| `UpsertSyncStatus(s)` | `Vec<&SyncStatus>` | sync status |
| `UpsertReminderRule(r)` | `Vec<&ReminderRule>` | rule upserts |
| `DeleteReminderRule(id)` | `Vec<&ReminderRuleId>` | rule deletes |
| `RecordReminderDelivery(d)` | `Vec<&ReminderDelivery>` | deliveries |
| `RecordReminderSnooze(s)` | `Vec<&ReminderSnooze>` | snoozes |
| `SetTaskRemindersMuted(t, m)` | `Vec<(&TaskId, &bool)>` | mutes |

All buffers are references (the current `if let` patterns over
`&[PersistenceAction]` already bind `&T`) — **zero clones, zero new
allocations for empty partitions** (`Vec::new()` is allocation-free until
first push). The match carries one constraint comment stating why it must
never grow a wildcard (see §6 wording). Partition order, SQL text, bind
order, and tracing events are byte-identical; the pass-local design comments
(`:873-874`, `:972-977`) move with their passes; `apply_inner`'s fixed-order
comment (`:658-663`) is untouched; `#[allow(clippy::too_many_lines)]` stays
(the function is still one transaction body — its rationale comment remains
true). Net side effect: 13 full scans of the action slice become one
classification scan + walks of small buffers (strictly ≤ work; hot-path
benches unaffected in direction).

**Rejected alternatives** (each would violate a hard constraint of this
chunk):

- *Single-pass exhaustive match (one loop, 14 arms executing SQL inline).*
  **Not behavior-neutral**: cross-partition statement order would follow
  batch order — the `[EnqueueOp, CompleteOp]` case deletes the op
  (partition order deliberately keeps it), and `[UpsertStack, UpsertBoard]`
  violates the enforced FK. The dispatch plan's acceptance explicitly
  requires behavior-neutral. Rejected.
- *Per-pass exhaustive matches with explicit no-op arms for the other 13
  variants.* 14 arms × 13 passes, and every no-op arm re-institutionalizes
  exactly the silent-ignore this chunk exists to remove. Rejected.
- *`_ =>` wildcard + runtime tripwire* (`unreachable!` / new error /
  `debug_assert!`). A panic converts the latent data bug into a daemon/kiosk
  crash (worse than the disease on the app class this project targets); a
  typed return error would require a new `RepositoryError` variant — a
  domain enum change, out of scope by non-goal; a `debug_assert!` protects
  dev builds only, not release kiosk deployments. The compiler enforces all
  configurations at zero runtime cost. Rejected (the dispatch prompt's
  "prefer the compiler" guidance, adopted).
- *A domain-side test enumerating variants.* A test that re-lists the 14
  variants proves nothing the compiler doesn't already prove once the match
  is exhaustive (§4), and would be a maintenance duplicate of the arm list.
  Rejected.

## 4. The totality proof, named

**Mechanism: rustc's cross-crate match exhaustiveness check (error E0004),
anchored at the storage-side pre-partition match.**

- `PersistenceAction` is public in `taskboard-domain` and carries **no
  `#[non_exhaustive]`** (verified `persistence.rs:176-177`). Therefore any
  exhaustive `match` over it in `taskboard-storage-sqlite` is checked
  against the variant set at *that crate's* compile time: **adding a domain
  variant without a storage arm fails `cargo build -p
  taskboard-storage-sqlite`** with `error[E0004]: non-exhaustive patterns`
  at the pre-partition match. The build break is the proof; it is
  permanent, configuration-independent, and costs nothing at runtime.
- The proof cannot be weakened *silently*: adding `#[non_exhaustive]` to
  the enum would itself break this match (a wildcard becomes required),
  i.e. the escape hatch is also a loud build failure a reviewer must
  resolve deliberately.
- The domain half needs no new mechanism: `apply_actions` is already a
  same-crate exhaustive `match`, so a variant cannot exist without
  executable semantics; the storage arm is the remaining half, now closed
  by the same compiler rule applied across the crate boundary.
- **No `trybuild` compile-fail test**: it would add a dev-dependency
  outside the approved stack (AGENTS §4) to assert a property of the
  compiler rather than of project code. The dispatch acceptance ("a test or
  compile-level proof") is satisfied by the compile-level proof above.
- **Review guard** (stage 3, per dispatch §9 prompt 8.3): `grep` the
  `execute_batch` body for `_ =>` / `_ @` — the pre-partition match must
  contain none. The one legitimate wildcard in the file today
  (`UpsertSyncStatus`'s inner `match status.phase` at `:950` — over
  `SyncPhase`, a different enum with a legitimate catch-all) is expected to
  remain and must not be confused with the dispatch match.

## 5. Behavior-neutral diff shape (stage-2 obligations)

1. One commit: `refactor(storage): make execute_batch dispatch
   compiler-exhaustive` — the restructure and its totality mechanism are
   inherently the same edit (the match *is* the proof), satisfying the
   stage-2 rule that no intermediate commit is silently-unpersisted-unsafe.
2. Mechanical edit recipe: introduce the 13 buffers + classification loop
   at the top; per pass, replace the `for action in actions { if let
   <Pat>(x) = action {` header with `for x in <buffer> {` (or the tuple
   form for two-field arms) and dedent the body one level; **nothing inside
   any body changes** — SQL strings, bind expressions, `stmt_error`
   mappings, `tracing::trace!` fields and messages all stay byte-identical.
3. Self-check before committing: `git diff` must show, per pass, only the
   header change + re-indentation; `cargo insta`-style noise is impossible
   here (no snapshots touched), and the arm↔buffer table in §3 maps 1:1 to
   the diff hunks.
4. Gates (AGENTS §10) green; the storage suite + `taskboard-app`'s
   `cli_reminders` e2e green unmodified.

## 6. Comment to carry on the match (constraint, not narration)

```rust
// No wildcard arm: a new `PersistenceAction` variant must fail this
// crate's build (E0004) until storage persists it — a silent fall-through
// here means an action class accepted with Ok(()) and never written.
```

(Exact wording may be tuned by stage 2; the constraint it states is
binding.)

## 7. Non-goals

- **No enum redesign**: no new `PersistenceAction` variants, no
  `#[non_exhaustive]`, no serde/derive changes, no `RepositoryError`
  changes (rules out the tripwire alternative for good).
- **No new actions**, no schema/SQL changes, no signature changes to
  `execute_batch`/`apply_inner`/the port.
- **No repo.rs split** (F5-11's recorded verdict: keep as-is; revisit only
  past ~⅓ growth).
- **No drive-by fixes in the same file**: F5-06 (stale "eight SELECTs"
  count in `load_inner`'s allow-comment, `repo.rs:406`) and F5-04
  (duplicated rationale in `error.rs`) stay out despite the
  "when next touched" trigger on F5-06 — chunk purity per the dispatch
  protocol; F5-06 remains tracked in the owning report for its own one-line
  PR.
- **No new behavior tests** — the totality proof is structural (§4); the
  behavior-neutrality proof is the unmodified suite (§2.4) + the §5 diff
  discipline. (Noticed out-of-scope gap, for stage-2/3 bookkeeping per the
  wave rules: the reminder partitions have no crate-level roundtrip test —
  file a GitHub issue, `backlog:` + `testing`, noting `actions_from_state`
  in `tests/common/mod.rs` emits only 7 of 14 variants; do not fix here.)

## 8. Risks

| Risk | Mitigation |
|---|---|
| 400-line function restructure invites accidental body edits | §5.2 recipe + §5.3 self-check (header-change-only hunks); stage-3 diffs arms 1:1 against the old chains (dispatch 8.3) |
| Reminder partitions lack crate-level tests (behavior anchored only at app e2e) | statement-identity check in review; `cli_reminders.rs` (20 tests) exercises the passes end-to-end; gap recorded as an issue (§7) |
| Or-pattern / two-field arms bind differently than expected (`&OpId`, tuple refs) | table in §3 fixes the exact buffer types; compiler verifies borrows |
| Someone later re-adds a `_` arm "to unblock" a new variant | the §6 constraint comment names the rule; stage-3 grep guard; `#[non_exhaustive]` route is itself a loud build break |
| `main` moves under us before PR (other storage-touching PRs) | serial-chain rule 4 → 8 is already satisfied; if a later storage PR merges first, rebase onto `origin/main` per AGENTS §8 (`--force-with-lease` on push) |

## 9. Handoff

Stage 2 implements §5 on this branch (worktree recreated if missing); stage
3 reviews per dispatch §9 prompt 8.3 + the code-quality mandate, then ships
the PR (body: summary + `Closes #243`), watching `gh pr checks` to green.
Never merge; never touch `main`.

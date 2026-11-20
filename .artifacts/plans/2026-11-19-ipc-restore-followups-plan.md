# Chunk B6 plan — IPC/state follow-ups (#993/#994/#991/#990)

- **Date**: 2026-11-20
- **Round**: backlog-reduction round 3, chunk B6 (dispatch plan
  `.artifacts/plans/2026-11-19-backlog-reduction-round3-dispatch-plan.md`
  §13; scope report
  `.artifacts/reports/2026-11-19-backlog-reduction-round3-scope-report.md`
  §B6 — both live on the `chore/artefact-sweep-2026-11-19` branch at
  planning time; §13's normative text is quoted in this worktree's
  copy of the dispatch prompt and restated here where it binds).
- **Branch**: `fix/ipc-restore-followups` (off `origin/main` @ ee8cdd3d)
- **Stage**: 1 (planning). Stage 2 implements per-issue commits; stage 3
  reviews and opens the PR. No push/PR before stage 3.
- **UX gate**: none for this chunk — the round-3 dispatch table records
  "no UX plan in origin" for B6. One decision below (#991-D2) keeps the
  human-visible rendering out of the chunk and relegates it to a
  journey-gated backlog issue, preserving that law.

## 1. Seams read (stage-1 evidence)

- `crates/taskboard-domain/src/command.rs:260` — `CommandOutcome`
  currently derives **only** `Debug, Clone, PartialEq, Eq`; **no serde
  derive exists** (issue #993's premise "rests on the serde defaults"
  is aspirational: today the type cannot be serialized at all). ~40
  variants; the three restore-family receipts are
  `LabelRestored { label }` / `HabitRestored { habit }` /
  `EntryRestored { id }` (unit-field naming variants).
  `taskboard-domain` already depends on `serde` + `serde_json`
  (Cargo.toml:12–13) — `StateCommand`/`JournalEntry` derive serde for
  journal persistence, so adding derives to `CommandOutcome` crosses no
  crate boundary.
- `crates/taskboard-domain/src/undo/tests.rs:4228` —
  `the_restore_family_new_shapes_round_trip_serde`, the pattern to
  extend (construction → serde round-trip, plus hand-written-shaped
  decode checks).
- `crates/taskboard-app/src/ipc/protocol.rs` — the IPC wire has its own
  request/response types; `CommandOutcome` never crosses the socket
  verbatim. The pin therefore guards **prospective** decode stability
  for persisted/mirrored surfaces (the issue's words: "before any
  future receipt rename silently breaks persisted surfaces that mirror
  outcomes"), not an existing wire.
- `crates/taskboard-app/src/main.rs:100,179–185` — the IpcOrLocal
  transport arm consults `client::fallback_blocked_by_lock` and exits
  typed via `exit_blocked_fallback` (landed, `fix/ipc-stop-ordering-seams`).
- `crates/taskboard-app/src/commands/logout.rs:100–170` —
  `NudgeOutcome { NoDaemon, Answered, Transport, Version }`;
  `nudge_daemon` maps `try_connect → None` straight to `NoDaemon`;
  doc contract: never fails the logout. `connect.rs:660`
  `nudge_daemon_attach` — same shape. Neither boots a daemon.
- `.artifacts/plans/2026-11-18-ipc-stop-ordering-seams-plan.md` D-2
  step 3 (lines 152–162) names "the three fallback sites (`main.rs`
  IpcOrLocal transport, `logout.rs:164`, `connect.rs:665`)"; R-5 and
  the implementation step at line 302 repeat the three-site count.
  The two command-level sites are best-effort nudges, not fallback
  boots — the plan over-counted (issue #994).
- `crates/taskboard-app/src/commands/status.rs:34–118, 246–263` —
  `StatusView { daemon: bool, holder_pid: Option<u32>, … }`; the
  client-side `no_daemon` projection answers `daemon: false` +
  `holder_pid` with everything else `None` (the `{"daemon": false}`
  byte pin). Additive fields ride `#[serde(default)]` +
  `skip_serializing_if` (the `connectors`/`engine`/`files_sync`
  precedent, no protocol-version bump). The drain-window facts are
  already probeable via `ipc::client::fallback_blocked_by_lock`
  (client.rs:175 — socket artifact present **and** flock held).
- `crates/taskboard-domain/src/command.rs:3402–3425` —
  `plan_entry_restore`: owner-existence check, then
  `app.tracking.entries.get(&entry.id)` divergence guard
  (`*stored != *entry` ⇒ `TimeEntryAlreadyPresent`), then an
  unconditional `UpsertTimeEntry` + `UpdateAppData` enqueue — the
  identical-present row falls through to the upsert (issue #990).
  The house terminal-repeat precedent is `plan_label_restore`
  (command.rs:3353: live target ⇒ `no_op()`).

## 2. Decisions

### #993-D1 — CommandOutcome gains serde derives; the pin is compile-exhaustive

**Decision**: add `Serialize, Deserialize` to `CommandOutcome`'s
derive list in `crates/taskboard-domain/src/command.rs` (default serde
shape: externally tagged, variant names as-is, field keys as declared —
`{"LabelRestored":{"label":"…"}}` etc.). Without the derive the issue's
pin has no subject; the journal family already carries serde on domain
types, so this is consistent, and the type stays pure (no new runtime
dep, `serde` is already a workspace-pinned domain dependency).

**Pin shape**: one new test in
`crates/taskboard-domain/src/command.rs`'s test module (the outcome's
own module, lowest layer — not `undo/tests.rs`, which pins journal
shapes), composed of:

1. **Exhaustive witness**: a `#[test]`-adjacent helper
   `fn assert_every_variant_is_round_tripped(outcome: &CommandOutcome)`
   — an exhaustive `match` over `&CommandOutcome` with **no wildcard
   arm**, one `()` arm per variant. Adding a variant is a compile error
   in the test module, forcing the author to extend both the witness
   and the sample list — a hand list that can drift silently is a
   finding per the stage-3 law.
2. **Round-trip over every variant**: a `Vec<CommandOutcome>` sample
   constructing each variant exactly once (synthetic uuids/timestamps;
   entity-carrying variants use minimal valid `TimeEntry` /
   `ReflectionEntry` rows as in the undo test's fixtures). For each:
   `serde_json::to_string` → `from_str` → `assert_eq`. This is the
   generated/exhaustive form: the witness makes the list structurally
   complete.
3. **Hand-written JSON byte-stability** for the three restore receipts:
   decode string literals `{"LabelRestored":{"label":"<u128-uuid>"}}`,
   `{"HabitRestored":{"habit":"…"}}`, `{"EntryRestored":{"id":"…"}}`
   and assert the typed values — the field keys and variant names are
   pinned against silent rename. One literal per receipt; the
   round-trip in (2) covers the forward direction.

**Acceptance**: the serde pin round-trips every `CommandOutcome`
variant; a variant added later without a sample fails to compile.

### #994-D1 — per-site divergence disposition (law (c): record which per site)

| Site | Plan said | Code reality | Disposition |
|---|---|---|---|
| `main.rs` IpcOrLocal transport arm | probe + typed `Bootstrap(InstanceLocked)` on blocked fallback | landed exactly so (`exit_blocked_fallback`, client.rs probe) | **Plan right, code right** — no change; recorded as satisfied. |
| `logout.rs` `nudge_daemon` (~:163) | listed as a fallback site to gate | best-effort nudge: `try_connect → None` ⇒ honest `NoDaemon`; no in-process bootstrap exists to gate; contract says never fails the logout | **Code right, plan wrong** — amend the plan text. |
| `connect.rs` `nudge_daemon_attach` (~:660) | same | same shape, degrades through `map_attach_nudge` | **Code right, plan wrong** — amend the plan text. |

**Amendment mechanics** (plans are records — append, don't rewrite):
append a dated block "Amendment (2026-11-20, #994)" to
`.artifacts/plans/2026-11-18-ipc-stop-ordering-seams-plan.md` stating:
D-2 step 3's fallback-site list is corrected to the single site that
boots (`main.rs` transport arm); the two command-level sites are
best-effort nudges with no bootstrap to gate (their budgets and typed
`NoDaemon` degradation are the documented #570 semantics) — and mirror
the correction onto R-5's "three sites" and the line-302 implementation
step's "three-line conditional" phrasing in the same block.
Stage 2 also comments the disposition on issue #994 and on #886 (the
issue's proposed recording home), linking the amendment.

**NudgeOutcome during the drain window** — the issue's optional
re-examination: **rejected**. A distinct `NudgeOutcome` variant for
"drain window hit" would add a wire-adjacent variant whose only
consumer is a warning message upgrade, while the nudge's contract is
explicitly never-to-fail and its 1 s budget burn lands on the honest
`NoDaemon`. Recorded in the amendment block as a considered-and-closed
question; if a future UX round wants the distinction, it is a fresh
journey-gated decision, not a ride-along.

### #991-D1 — typed draining distinction: additive `Option<bool>` field, not a new variant

**Decision**: `StatusView` gains
`#[serde(default, skip_serializing_if = "Option::is_none")] pub draining: Option<bool>`,
populated **only** in the client-side `no_daemon(db_path)` projection:
`draining = fallback_blocked_by_lock(db_path).then_some(true)`. Semantics:
`daemon: false` + `draining: true` ⇒ the socket artifact exists beside a
held instance lock — a daemon is mid-shutdown (or wedged); `daemon: false`
with the field **absent** ⇒ genuinely no daemon (artifact absent, or a
stale artifact beside a free lock). Never `Some(false)`.

Why a field, not a variant/enum: `daemon` is a published wire bool with
a byte pin (`{"daemon": false}`) and many consumers; the additive-optional
pattern (the `connectors`/`engine`/`files_sync` precedent) needs no
protocol-version bump, keeps exit 0, and is byte-additive. A new
`daemon: "draining"` wire state would break the pin and every
`daemon: bool` consumer.

**Scope guard (UX law)**: stage 2 changes the **serde view and its
typed derivation only** — the human-readable text renderer of
`taskboard status` is untouched. A rendered "daemon draining" line is
perceivable-by-a-human work and needs a UX journey plan; it is filed as
a backlog issue (see §4) and is explicitly out of this chunk. The chunk
pins the *typed* distinction (draining vs absent are distinct
`StatusView` outcomes) per the dispatch acceptance.

**Pin**: unit tests at the `StatusView::no_daemon` layer with a
hermetic fixture that creates the socket sidecar + a held lock
(the `fallback_blocked_by_lock` tests in `ipc/client.rs:740–764`
already hold/release real flocks — reuse that fixture pattern):
- artifact + held lock ⇒ `daemon == false`, `draining == Some(true)`,
  `holder_pid` present;
- artifact + free lock ⇒ `draining == None` (the stale shape stays
  "absent");
- no artifact ⇒ `draining == None`;
plus a wire assertion that the `draining` key serializes only in the
draining answer (additivity, no version bump).

### #990-D1 — identical-row early NoOp; the comparison is whole-row `PartialEq`

**Decision**: in `plan_entry_restore`, the present-row arm splits:

- `*stored == *entry` (full `TimeEntry` structural equality — every
  field: id, task, task_title, start, end, both clocks, kind, `by`) ⇒
  `Ok(CommandPlan { actions: Vec::new(), outcome: CommandOutcome::NoOp })`.
- `*stored != *entry` ⇒ `CommandError::TimeEntryAlreadyPresent(entry.id)`
  (unchanged — the typed refusal for a divergent same-id row stays).

**Why whole-row equality, not a field subset**: the divergence guard
already uses `TimeEntry`'s `PartialEq` as "different"; the identical
case is its exact negation, so the pair stays total and monotone
tightening one comparison cannot silently loosen the other. A subset
(e.g. ignoring clocks) would have to argue per-field why a clock-only
divergence is "the same row" — that argument does not exist: a
clock-diverged row is a divergent fact and keeps the typed refusal.
The `UpdateTask` identical-position precedent (command.rs, the
terminal-repeat discipline) is the same whole-value comparison.

**Ordering**: the identity check happens where the divergence guard
sits today (after the owner-existence check), and the early return
precedes any action planning — the no-op plans **no** upsert and **no**
`UpdateAppData` flush trigger (a coalescing trigger is still an action;
the acceptance requires its absence).

**Pin**: unit test in `command.rs`'s test module: restore a entry into
an app that already holds the byte-identical row ⇒
`plan.actions == Vec::new()` and `plan.outcome == CommandOutcome::NoOp`;
the divergent-row refusal test (already landed, or added if absent)
stays green — no weakening.

## 3. Implementation order (stage 2, per-issue commits)

1. **fix(domain): RestoreTimeEntry repeat of an identical row is a NoOp** —
   #990 (smallest, pure domain, no derive churn).
2. **fix(domain): CommandOutcome gains serde derives and the round-trip pin** —
   #993.
3. **fix(app): status distinguishes a draining daemon from no daemon** — #991.
4. **docs(plan): correct the stop-ordering D-2.3 fallback-site count** — #994
   (amendment block + issue comments on #994 and #886; no code).

Gate: pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest
run --workspace` (flock-safe; defer per the #879/#894 rule if
contended, reporting the holder).

## 4. Backlog issues filed at planning

- `backlog:` human-readable `taskboard status` draining line (the
  journey-gated rendering twin of #991-D1's wire field) — **#1111**.

## 5. Risks

- **R-1 (#993 derive adds a serde impl to a ~40-variant enum)**: risk of
  an accidental field rename changing the wire — that is exactly what
  the pin now catches; the hand-written literals make the receipt keys
  explicit. Mitigation: the exhaustive witness is compile-checked.
- **R-2 (#991 field vs existing consumers)**: grep the
  `StatusView` construction/match sites — only `no_daemon` sets the
  field; `server`/`in_session` leave it `None` (absent), so daemon-side
  answers are byte-identical. The `{"daemon": false}` byte pin tests
  must stay green (field absent in the no-artifact fixture).
- **R-3 (#990 behavioral tightening)**: a journal replay or engine test
  that relied on the upsert-on-identical-repeat would surface; the
  flush is a coalescing trigger, so the honest risk is a test-count
  assertion on planned actions — fix the test, not the semantics, if
  the semantics are right per D-4c's accepted design.
- **R-4 (#994 amendment touches a landed plan file)**: the file exists
  on `origin/main`; the amendment is append-only, so no history
  conflict. The sweep branch holds the round-3 plan citing "correct the
  plan/impl divergence" — consistent either way.

## 6. Estimated diff

~250–350 lines total across four commits: #990 ≈ 15 code + ~40 test;
#993 ≈ 2 derive lines + ~110 test; #991 ≈ 15 code + ~70 test;
#994 ≈ 30 doc lines + issue comments.

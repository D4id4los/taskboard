# Boot/Fixture/Test-Infra Residue (round-4 chunk S4) — Chunk Plan

- **Date**: 2026-11-21
- **Kind**: chunk plan — stage-1 planning for dispatch round 4 chunk
  S4 (branch `test/boot-fixture-residue`), six test-infra residue
  items. No production behaviour change except where an item's shape
  requires one (#1134's storage seam, #1120's settle predicate, #1100's
  push-seam split); every change is test-law driven and carries its
  own failing-test-first evidence.
- **Origin** (normative, per item): the six issue bodies — #1113,
  #1075, #1134, #1120, #1106, #1100 — plus the scope report
  `.artifacts/reports/2026-11-21-backlog-reduction-round4-scope-report.md`
  §B-S4 and the dispatch plan
  `.artifacts/plans/2026-11-21-backlog-reduction-round4-dispatch-plan.md`
  §1/§4 (Prompt S4.1). Clock law: `docs/testing_strategy.org` §6.1–6.2.
- **Status**: READY — every open question below carries a recorded
  decision; stage 2 implements in the item order of §2 (it is also
  the commit order: one commit per item, standalone and green).

## 1. Seams inspected (stage-1 evidence, current origin/main f0c85359)

| Item | Seam | File:line (at planning) |
|---|---|---|
| #1113 | the three env-composing legs + `clean_ambient` gate | `crates/taskboard-app/src/ui_requests.rs` `:2256`, `:2588`, `:3967`; gate `:2115` |
| #1113 | re-homing target + env lock | `crates/taskboard-app/tests/services/ui_settings.rs` (`taskboard_env_lock()` used throughout) |
| #1113 | the three `pub(crate)` fns to widen | `ui_requests.rs` `:649` `run_settings_write`, `:1787` `undo_board_move`, `:1871` `publish_board_to_files` |
| #1075 | subprocess fixture budgets | `crates/taskboard-app/tests/cli/cli_tty.rs` (≈12 seeds of `shutdown_timeout_secs = 5`), `crates/taskboard-app/tests/core/repl.rs:1257` (2 s), `tests/common/mod.rs:363`+`:636` (already 1 s) |
| #1075 | slow-table measurement method | `.artifacts/plans/2026-11-19-shutdown-budget-plan.md` §D2 sweep step 3 ("re-run the slow-test measurement and diff against the runtime report's 41-over-5s table") |
| #1134 | the paused-clock breaker | `crates/taskboard-storage-sqlite/src/connect.rs` `pool_options()` `:34` (no `acquire_timeout` set → sqlx 30 s default racing its own connect under auto-advance), `open`/`open_memory` `:129`/`:152`, private `open_with` `:47` |
| #1134 | the boot path that composes it | `crates/taskboard-app/src/bootstrap.rs:1330` (`taskboard_storage_sqlite::open` inside `bootstrap_with_modules`) |
| #1120 | the settle predicate + documented leniency | `bootstrap.rs:1815` `wait_for_boot_pull`, the `remote.is_none()` arm and the #1120 leniency comment |
| #1120 | the ownership derivation to reuse | the D-BF5 enforcement point 2 at `bootstrap.rs:1332+` (configured-connector set vs persisted bindings) |
| #1106 | ceremony entry points (already parameterized) | `crates/taskboard-app/src/board_move_ceremony.rs` `run_on`/`run_until` take `device_json: &Path` (`:560`/`:606`/`:631`); `default_device_json()` `:586` |
| #1106 | the offending writer | `crates/taskboard-app/tests/it_docker.rs:1593` mints `default_device_json()` for the journey |
| #1106 | the verbs composing the real home | `commands/mod.rs:1206`/`:2197`, `main.rs:915`, `ui.rs:817` |
| #1100 | the exempted push seam | `crates/taskboard-ui-slint/tests/inventory/seam_inventory.rs:451–461` (`INVOKE_EXEMPT`), law fns `:463+` |
| #1100 | the two adapter call sites | `crates/taskboard-ui-slint/src/adapter.rs:1990` (kinds 4/5), `:4674` (`BeginEffortEdit`, kind 6) |
| #1100 | the slint-side function | `crates/taskboard-ui-slint/ui/app-window.slint:1773` `focus-detail-cell` (publishes `FocusFacts.anchor-kind = cell`) |

## 2. Per-item decisions

### Item 1 — #1113: re-home the three ui_requests env-composing legs

**Decision: the #865 re-homing, exactly as the issue body prescribes.**

1. Widen `undo_board_move`, `publish_board_to_files`,
   `run_settings_write` from `pub(crate)` to `pub` in
   `crates/taskboard-app/src/ui_requests.rs` (visibility only, no
   behaviour, no signature change — the `SettingsWriteOutcome` /
   `SettingsWriteError` / `LiveApply` types at `:598–619` go `pub`
   with them if the privacy check demands; widen the minimum set).
2. Move the three legs — `us6_ac1_restore_guards_refuse_typed`,
   `us4_s2_publish_guards_refuse_deck_bound`,
   `the_port_applies_live_to_a_real_engine` — into
   `crates/taskboard-app/tests/services/ui_settings.rs`, dropping the
   `_currently_pins_the_clean_ambient_read` suffixes and the
   `clean_ambient()` early-return gates: the integration suite holds
   `taskboard_env_lock()`, so the legs become runnable under any
   ambient (the hostile-env acceptance stays green by construction).
   The legs reuse the suite's existing fixture constructors (the
   `workspace`/`port_workspace` equivalents there); where a fixture
   helper does not exist yet, add it in that file, not in `common`.
3. **Scope boundary (recorded decision)**: only the three named legs
   move. The *other* `clean_ambient()`-gated tests in
   `ui_requests.rs` (`workspace_fixture_deletes_the_dir_only_after_the_app_teardown`,
   `current_connections_fills_bound_channel_structurally`,
   `us2_s2_stop_sync_guards_refuse_typed`,
   `stop_board_files_sync_sends_exact_unbind_command`,
   `the_port_engine_gone_reports_the_next_start_lane`,
   `the_port_no_engine_lane_reports_next_start`,
   `focus_gate_refusal_attributes_own_blame`) stay where they are and
   stay gated — the issue names three, and re-homing the rest is the
   residual class recorded as a new `backlog:` issue at stage 1 (see
   §5). `clean_ambient` itself stays (it still has callers) with its
   doc comment updated to point at the remaining legs.
4. **Test law**: typed outcomes only (`assert_eq!` on the existing
   typed errors `SettingsError::Gone`/`BindRefused`/`FilesSyncDisabled`,
   `LiveApply::Applied`) — the legs already comply; the move is
   verbatim minus gates plus env lock.

**Open question resolved**: do we also delete `clean_ambient` when the
three legs leave? **No** — seven gated callers remain; deleting it is
part of the residual-class issue, not this chunk.

### Item 2 — #1075: drop the subprocess fixtures' explicit shutdown budgets

**Decision: "drop" means *reduce to the 1 s hermetic floor*, not
*remove the explicit line*** — the production default is 10 s, so
removal would *raise* every teardown and reintroduce the 10 s
quantizer the dispatch prompt forbids.

1. Set `shutdown_timeout_secs = 1` in every subprocess-fixture seed:
   the ~12 `= 5` seeds in `tests/cli/cli_tty.rs`, the `= 2` seed in
   `tests/core/repl.rs:1257`, and confirm `tests/common/mod.rs`
   (`:363`, `:636`) stays at 1 s.
2. **Contract pins are untouchable** (the shutdown-budget plan D3
   inventory): the drain-wedge contract in `daemon_signals.rs`, the
   linger-drain 30 s pin, the dead-host 60 s detach budget
   (`ipc.rs:544`), the wiremock 5 s delay tests. If a 5 s seed in
   cli_tty turns out to *be* a contract pin (its assertion compares a
   duration), it is named in the stage-3 report and left at its pin —
   never weakened.
3. **Evidence mandate (dispatch trap (a))**: after the pass, re-run
   the slow-test measurement with the shutdown-budget plan's method
   (nextest run of the app test targets, timing table of every test
   over 5 s) and diff against the runtime report's 41-over-5s table.
   The evidence bar: **no test becomes newly 10 s-quantized** (no
   test's wall time gains a ~10 s step that the old table lacks), and
   the cli_tty/repl share of the table shrinks by the expected
   ~4 s × runs. The re-measured table lands in the stage-3 review
   report. The measurement is machine-heavy: run through
   `scripts/memory-gate.sh`, backgrounded, with the landed work
   committed *before* the wait.
4. **Determinism**: the fixtures already terminate by drain-EOF
   (R2c's drain contract, `conclude_session`'s doc), not by sleep; a
   1 s budget only bounds the kill path for a real wedge, which still
   fails the `code == Some(0)` pins — no quality ceiling introduced
   (a wedged process still fails loudly; a healthy one gets faster).

**Open question resolved**: does `common::write_test_config` need
touching? **No** — it already seeds 1 s (`:636`, the #1082 note); the
issue's "(5 s / 2 s)" predates that. Recorded so stage 2 does not
"fix" it upward.

### Item 3 — #1134: paused-time tokio clock breaks sqlx pool connect

**Decision: shape (a) from the issue — extend the storage open seam
with sqlx's `acquire_timeout`; the hermetic test drives it huge under
paused time. A seam, never a sleep; no wall clock in the test.**

1. `crates/taskboard-storage-sqlite/src/connect.rs`: add a tuning
   knob to the existing open seam —
   `pub async fn open_tuned(path, default_connector, tuning: PoolTuning)`
   where `PoolTuning { acquire_timeout: Duration }` (a small
   structural record, minted normally by callers; `open` /
   `open_memory` delegate with sqlx's default). `pool_options()`
   grows an `acquire_timeout` application. `#[derive(Debug)]` +
   plain fields; no newtype ceremony (internal, one consumer).
2. `crates/taskboard-app/src/bootstrap.rs`: thread the knob through
   `bootstrap_with_modules` as an explicit parameter
   (`pool_acquire_timeout: Option<Duration>`, `None` = sqlx default);
   the public `bootstrap`/`bootstrap_in` constructors keep their
   signatures and pass `None`. Production callers change nothing.
3. The hermetic bootstrap suite's paused-clock tests
   (`tests/core/bootstrap.rs`, the R2c teardown-inversion test's
   successor) pass `Some(Duration::MAX)`-class value (use a large
   finite constant, e.g. `Duration::from_secs(24*3600)`, so arithmetic
   cannot overflow) and then `tokio::time::pause()` freely — the
   pool-acquire race is defused because the acquire timer no longer
   fires during the (real-IO, clock-independent) sqlite connect +
   migrate. **The clock law compliance**: the test's time-dependent
   assertions (drain/join budgets) still use pause/advance; connect
   itself is real IO under the paused timer — permitted because the
   paused clock's auto-advance only fires *tokio* timers, and the
   injected huge `acquire_timeout` is exactly the injected-timer
   discipline (§6.1 tool 2 generalized to the pool's own timer).
4. **Failing-test-first**: stage 2 first converts the R2c
   workaround test's real-clock compromise to `start_paused` *without*
   the seam (fails with `PoolTimedOut`, repro of #1134), then lands
   the seam, then the test goes green paused. The workaround's doc
   comment (real wall clock + 1 s budget) is deleted with the
   workaround.
5. `open_memory` gains the same knob availability for symmetry only
   if a paused test needs it; no speculative parameter churn.

**Open question resolved**: why not shape (b) ("start the pool warm
before freezing")? **Rejected** — it would require splitting
`bootstrap_with_modules` into a warm-up/freeze two-phase API purely
for tests, a bigger production surface than one parameter; tie-breaker
"extend an existing seam rather than adding a new one". Shape (c)
(document the constraint) is the fallback only if the seam hits an
unexpected sqlx-internal second timer during stage 2 — then the
standing exception goes into `docs/testing_strategy.org` §6's seam
inventory with a `backlog:` follow-up, and the paused conversion of
that one test is dropped.

### Item 4 — #1120: boot-pull settle burns the full budget on deconfigured-only worlds

**Decision: the R2b plan's named fix — a typed ownership fact on the
state, consulted by the predicate; the app layer never re-derives
config ownership.**

1. `taskboard-state` (pure, zero IO): add a typed fact on
   `AppState`, e.g.
   `pub fn live_ownable_bindings_exist(&self, owners: &BTreeSet<ConnectorId>) -> bool`
   — true iff some live (non-deleted) board carries a `remote` binding
   whose connector is in `owners`. Pure predicate over state data;
   the ownership *set* is an argument (now-as-data analog), so the
   SRP line holds: the state derives *which bindings are ownable given
   owners*; the app derives *who the owners are* (it already does, at
   the D-BF5 enforcement point 2).
2. `bootstrap.rs::wait_for_boot_pull`: replace the unbound-world arm
   with the generalized one — settle immediately when
   `!state.live_ownable_bindings_exist(&owners)` where `owners` =
   the configured module connectors **plus `files-sync`'s connector id
   when `config.files_sync.enabled`** (the same owner set the D-BF5
   check validates bindings against; computed once at boot and passed
   in). This *subsumes* the R2b widening: a no-binding world is the
   empty-ownership case; the deconfigured-only world (bindings whose
   connectors no live actor owns) now settles immediately too — the
   unbound-world rule extended to the deconfigured class, per the
   issue.
3. The `SyncPhase::Syncing`/`last_success`/`Offline`/`Failed` arms and
   the deadline backstop stay verbatim — the new arm is strictly
   additive (a world with ownable bindings behaves exactly as today).
4. **Tests** (hermetic bootstrap suite, `tests/core/bootstrap.rs`,
   the settle-contract pins' home — lowest layer for the predicate):
   - new pin: a live board bound to a connector outside `owners`
     (the standalone-after-attach shape) → `wait_for_boot_pull`
     returns *without* the deadline expiring. Deterministic by
     construction: `start_paused`, call the predicate, and assert it
     returns before advancing the clock past the deadline (poll the
     join handle; never a sleep). The failing-test-first run shows
     the full-budget burn (under pause the deadline only passes on
     explicit `advance`, so the *pre-fix* assertion is: the call is
     still pending after advancing a short bounded step; post-fix it
     has returned without any advance).
   - the existing settle-contract pins keep passing unchanged (the
     generalized arm preserves every currently-settling world).

**Open question resolved**: does the predicate take `owners` as a new
parameter (breaking its `pub async fn wait_for_boot_pull(engine,
timeout)` signature)? **Yes** — the signature grows to
`wait_for_boot_pull(engine, owners, timeout)` with the public
hermetic-suite callers updated; the alternative (deriving owners
inside from config) is exactly what the issue forbids. The one
production caller (the boot path) passes the D-BF5 owner set.

### Item 5 — #1106: `default_device_json` test injection seam

**Decision: the ceremony seam already exists — use it. The fix is the
journey's one-line flip to a fixture tempdir; no new production
seam.**

1. `run_on`/`run_until` already take `device_json: &Path` (the
   injection seam; planned per the issue's shape, landed earlier).
   The verbs (`commands::boards`, `main.rs`, `ui.rs`) call
   `default_device_json()` and pass the resolved path in — that *is*
   the BootIdentity-precedent composition; there is nothing to add.
2. The offending writer is `tests/it_docker.rs:1593`: flip the
   journey to mint its device.json in a fixture tempdir (a new
   `tempfile::tempdir()` next to the existing `device_b` tempdir) —
   the acceptance is "no test writes outside its workspace", and this
   closes the last real-home writer in the suite.
3. **Declined (recorded decision)**: an override parameter on the
   *verbs* so tests could inject through the CLI surface. No test
   drives the board-move verbs through a composed process env today
   (checked: `cli_verbs.rs`'s `move_` tests are the *task* move; the
   board-move verb tests go through the ceremony seam), so the
   parameter is prophylactic — AGENTS §2 A1 need-driven law; it enters
   when a test actually needs it (the issue stays the record).
4. `it_docker` is the live-tier suite (docker gate). Stage 2 runs it
   when the tier is available; otherwise the flip lands with the
   static argument (the path is fixture-local by construction) and
   the stage-3 report names the tier run as deferred evidence.

### Item 6 — #1100: exactly-once invoke count for focus_detail_cell

**Decision: split the push seam per origin — the issue's primary
proposal. Anchor-kind keying (the alternative) is rejected: string-
matching `invoke_focus_detail_cell(4)`/`(6)` call sites by arity is a
text assertion in disguise.**

1. `ui/app-window.slint`: add
   `public function focus-effort-opener()` next to `focus-detail-cell`
   (`:1773`), publishing the identical `FocusFacts` tuple with
   `anchor-kind = 6` (same shape, same nonce bump — the fact
   contract is unchanged, so kinds 4/5 journeys and the M9 commit law
   are untouched). Keep `// slint: focus-effort-opener` comment tag
   (the manifest's convention).
2. `adapter.rs:4674` (`BeginEffortEdit`): invoke
   `invoke_focus_effort_opener()` (the slint codegen name for the new
   function); the `invoke_focus_detail_cell(kind)` at `:1990` keeps
   serving the focus-origin restore's kinds 4/5.
3. `tests/inventory/seam_inventory.rs`: move
   `"invoke_focus_detail_cell"` from `INVOKE_EXEMPT` into
   `INVOKE_INVENTORY` (exactly-once again, kinds 4/5 origin) and add
   `"focus_effort_opener"` to `INVOKE_INVENTORY` (exactly-once,
   effort-opener origin). `INVOKE_EXEMPT` keeps only
   `invoke_refocus_view`. Both law tests (exactly-once + no-unlisted)
   then cover the split seams; the no-unlisted side's arithmetic
   self-adjusts (exempt count drops by one, inventory grows by two —
   the sums still balance because the old seam's two calls become two
   seams with one call each).
4. The comment block that documented the exemption (round-9 chunk 11)
   moves to `INVOKE_INVENTORY` as the two origins' provenance note.

**Open question resolved**: does the new function need a distinct
anchor kind? **No** — it publishes kind 6 exactly as before; the
*seam* is per-origin, the *fact* is per-anchor. This keeps every
FocusFacts consumer (journeys, the focus coverage probe) byte-
identical.

## 3. Execution order & commit shape

One commit per item, in this order (dependencies: item 3's seam is
independent; item 4 touches the same file as item 3's threading —
land 3 before 4 to keep rebases trivial):

1. `test(ui-requests): re-home the env-composing port legs into the integration suite` (#1113)
2. `test(app): reduce the subprocess fixtures' shutdown budgets to the 1 s floor` (#1075)
3. `feat(storage): expose the pool acquire timeout through the open seam` + the paused conversion, one commit (#1134)
4. `fix(app): settle the boot pull immediately when no ownable binding is live` (#1120)
5. `test(app-docker): move the files journey's device.json into a fixture tempdir` (#1106)
6. `feat(ui): split the detail-cell push seam per dispatch origin` (#1100)
7. `docs(plan): the boot fixture residue chunk plan` (this file, committed first at stage 1)

Non-goals: the residual `clean_ambient` class (backlog issue), the
S5 gate items, any production sync/storage behaviour beyond the two
named seams, UX surfaces (none — `no UX plan in origin`, pure
test-infra + two typed seams).

## 4. Quality gates & stage-2/3 traps

- Standard gates through `scripts/run-quality-gates.sh`; heavy runs
  (`clippy`, the #1075 re-measurement) through `scripts/memory-gate.sh`,
  backgrounded past ~5 min with the landed work committed first.
- **Trap (a)** (#1075): the removal is evidence-backed — the
  re-measured slow table shows no quantizer return; without the table
  the PR does not open.
- **Trap (b)** (#1134): the fix is a seam, not a sleep — the paused
  conversion must show `start_paused` green with zero
  real-clock tolerance; any `std::thread::sleep`/wall fallback is a
  review reject.
- **Trap (c)**: the hostile-env pass stays green — after #1113 the
  three legs must run under a planted `TASKBOARD_*` ambient
  (`taskboard_env_lock` held), verified by running
  `tests/services/ui_settings.rs` with a planted ambient once in
  stage 2 (the C6/B3 acceptance pattern).
- Long-command discipline: pre-commit hooks + the flock-wrapped
  `cargo nextest run --workspace` are the stage gate; commit before
  any wait; on lock contention with no useful work left, report
  "gate deferred: lock held by <holder>" and end the stage.

## 5. Backlog issues opened at stage 1

- **#1157** — `backlog:` finish the `clean_ambient` re-homing class:
  the seven remaining gated legs in `ui_requests.rs` after #1113's
  three move (each leg either re-homes onto an env-lock-holding
  integration file or is un-gated with a recorded "reads do not
  compose env" justification; `clean_ambient` is deleted when the
  last caller leaves). Origin: stage-1 scope-boundary decision, item 1
  §2 above.

## 6. Risks

| Risk | Mitigation |
|---|---|
| #1120's generalized arm changes a currently-settling world's timing (behaviour regression at boot) | The arm is provably additive: every world the old arms settle, the new fact also settles (no-binding ⊂ no-ownable-binding); the existing settle pins run unchanged and a new pin covers the deconfigured class |
| #1075's 1 s budget is tight under `-j 4` load (the D2 untouched-table's original reason) | The drain contract (R2c) removed the wedge class the 5 s cushioned; the re-measured table is the arbiter — any flake observation goes to the flaky-test watchlist per AGENTS §7, never an opportunistic budget bump |
| #1134's huge `acquire_timeout` masks a genuine connect failure in the hermetic tests (failure surfaces only at the MIGRATOR or later IO) | The knob is test-only-threaded (`None` default in production); a genuinely broken db path still fails at `open_tuned`'s connect — the timeout only defers the *acquire* race, not IO errors |
| slint codegen name mismatch for `focus-effort-opener` (the `invoke_` prefix mapping) | Stage 2 compiles `taskboard-ui-slint` first; the seam-inventory test's exact-once count fails loudly on a name drift — the inventory string is the contract |
| `it_docker` flip unverifiable without the live tier | Fixture-local path is statically evident; stage-3 report names the tier run as deferred evidence (round's docker gate lane) |
| Widening `pub(crate)` → `pub` leaks internals into the crate's public API | `taskboard-app`'s lib target is the application facade already exercised by integration tests; the three fns' types widen with them; `cargo clippy -D warnings` + `cargo deny` gate the surface |

## 7. Estimated diff size

**M (≈ 300–450 lines changed across ~12 files)**: #1113 ~150 (leg
moves + fixtures + visibility), #1075 ~15 mechanical + the evidence
table (report-only), #1134 ~60 (seam + threading + test conversion),
#1120 ~80 (state fact + predicate + two pins), #1106 ~5, #1100 ~40
(slint fn + adapter site + inventory). No new crates, no new
dependencies.

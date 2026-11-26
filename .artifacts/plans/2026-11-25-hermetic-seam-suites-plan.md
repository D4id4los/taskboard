# The Hermetic Seam Suites — Chunk D2 Plan (round 8)

Branch: `test/hermetic-seam-suites` · Stage 1: diagnosis & design (this
document) · Stage 2: implementation · Stage 3: review.
Covers #1374, #1373, #1375, #1367, #1324, #1405, #1404 (dispatch plan
§8; scope report §D-D2). Every cargo invocation through
`scripts/memory-gate.sh`; the full-suite gate is
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(flock-safe, background + poll; commit landed work before any wait).

Global laws applied to every suite below: typed outcomes only (`matches!`,
`PartialEq` on typed rows/outcomes — no text assertions); deterministic by
construction (no sleeps — poll predicates with deadlines, or
`#[tokio::test(start_paused = true)]` + `advance`); fails-when-broken
mechanism named per item; one behaviour, one test, lowest layer.

---

## Item 1 — #1374: boot/wiring seams (ui.rs resolvers, CommandSink::dispatch, main.rs overrides)

### Diagnosis (verified against origin/main @ 086a3cd1)

- `crates/taskboard-app/src/ui.rs:281` `resolve_export_defaults`,
  `:302` `resolve_day_start`, `:710`
  `resolve_board_infos_from_state` / `:734` `resolve_board_infos` —
  no test at any tier (ui.rs's two `#[cfg(test)]` modules at :994 and
  :1361 cover the settings snapshot, keymap resolution, and the
  forwarder retire — not these).
- `crates/taskboard-app/src/ui.rs:21-34` the `CommandSink` trait +
  `EngineHandle` impl: `dispatch` never exercised against a real
  engine (the FakeSink tests at :1186 drive `spawn_ui_command_forwarder`
  only). `ui_command_channel` (:45) — capacity 64 is a doc fact with no
  pin. `surface_keymap_findings` (:53) — log-only, untested.
  `quit_until_accepted` (:771) — untested, and it is the only app-side
  bounded-wait loop.
- `crates/taskboard-app/src/main.rs:309` `apply_no_ipc_override` —
  pure, untested; main.rs has **no** `#[cfg(test)]` module today.
  `main.rs:101` `exit_blocked_fallback` — `-> !` via
  `std::process::exit`; not hermetically testable as spelled.

### Design

All app-tier unit tests (tier 1, lowest layer — the fns are pure or
seam-injectable; nothing here belongs in the engine/integration tiers).

1. **Resolver table tests** — new `mod resolver_tests` in ui.rs
   (beside the existing settings-snapshot tests, same fixture style):
   - `resolve_export_defaults`: build an `AppConfig` with
     `export.pdf_dir = Some("~/exports")` → assert the typed
     `ExportDialogDefaults` fields (`pdf_dir`/`export_dir` carry the
     expanded home path; `open_after` mirrors the config flag;
     `pdf_available == cfg!(feature = "pdf-export")` — a compile-time
     fact asserted as such). Second row: `pdf_dir = None` → falls to
     the platform default (assert the path *ends with* the expected
     stem structurally — no message text; the typed `Path` is the
     oracle). **Fails when broken**: a config-key rename or a
     tilde-expansion regression changes the typed dir/flag values.
   - `resolve_day_start`: `"07:30:00"` parses to `NaiveTime` 7:30;
     an empty/invalid string degrades to the default midnight (the
     documented `unwrap_or_default`). Offset leg: capture
     `chrono::Local::now().offset()` once at test start (single-read
     capture, testing-strategy §6.1) and assert equality — the fn
     samples the same instant's offset. **Fails when broken**: a
     config-key drift changes the parsed time; a timezone-leg
     regression breaks the single-read equality.
   - `resolve_board_infos_from_state`: seed an `AppState` with a live
     board (bound connector + channel), an archived board, and a
     deleted board → assert the returned `Vec<BoardInfo>` equals the
     live board's typed projection only (id/name/bound fields via
     `PartialEq`). This is the pin for the live-board filter rule
     shared with the board surface. `resolve_board_infos` (the engine
     wrapper) is the same behaviour one layer up — per the lowest-layer
     law, pin it only as a one-line delegation smoke over a real
     engine handle **reused from the sink test's fixture** (below), or
     leave it to the delegation (choose at stage 2 if the fixture
     import is awkward; the pure fn carries the rule).
2. **CommandSink::dispatch against a real engine** —
   `sink_test::dispatches_into_the_engine_inbox`: spawn a real engine
   with `taskboard_state::spawn_state_engine` over an
   `InMemoryRepository` + `ManualClock` (the `crates/taskboard-app/src/focus.rs:830`
   fixture precedent, same crate, no new deps), call
   `CommandSink::dispatch(&handle, StateCommand::CaptureTask{..})`,
   then poll `handle.shared_state()` with a deadline
   (`tokio::time::timeout`, no sleep) until the task exists in
   `state.tasks`. **Fails when broken**: a wrong envelope shape
   (`Execute` vs something else), a lost `try_send`, or a trait-impl
   mis-route never lands the task.
3. **`ui_command_channel` capacity pin** — `try_send` 64
   `StateCommand`s succeed, the 65th returns
   `TrySendError::Full(_)`. Pins `UI_COMMAND_CAPACITY = 64` as the
   documented unresponsive-UI pathology bound. **Fails when broken**:
   someone changes the constant or the channel construction.
4. **`surface_keymap_findings` smoke pin** — over the empty slice and
   a slice carrying every `Finding` variant, the call terminates and
   does not panic (typed fact: no assertion on log content — text law).
   Weak by nature (log-only fn); kept as a cheap structural pin, noted
   as such.
5. **`quit_until_accepted` bounded-wait pin** —
   `#[tokio::test(start_paused = true)]`: no event loop is running, so
   the fn must give up at its 10 s cap and return (the tokio paused
   clock auto-advances the 50 ms sleeps deterministically). Assert the
   function returns and (typed) that `taskboard_ui_slint::quit` was
   *not* required — termination is the assertion. **Fails when
   broken**: removing the cap makes the test hang/fail at the runtime
   budget; an early bare-quit path change shows up as an unexpected
   call panic in the stubbed UI seam if one is needed (stage 2 checks
   whether `taskboard_ui_slint::event_loop_running` is observable in
   tests; if the crate seam cannot report false reliably, pin the cap
   arithmetic via the elapsed virtual time instead).
6. **main.rs seams** — first `#[cfg(test)] mod tests` in main.rs:
   - `apply_no_ipc_override` table: `no_ipc = true` over a config with
     `ipc.enabled = true` → `false`; `no_ipc = false` returns the
     config byte-identical (`PartialEq` on `AppConfig`). **Fails when
     broken**: the flag-precedence regression (flag > file) reverts.
   - `exit_blocked_fallback` is `-> !` with `std::process::exit` —
     **small production refactor**: extract the typed-error
     construction into a testable
     `fn blocked_fallback_error(json: bool, db_path: &Path) ->
     CommandError` and leave `exit_blocked_fallback` a two-line shell
     (build + exit). Pin the extracted fn over a seeded sidecar
     (`pid=<n>` written to `<db>.lock` in a tempdir): the error is
     `CommandError::Bootstrap` wrapping `BootstrapError::InstanceLocked`
     with the recorded holder pid (typed `matches!` + pid equality).
     **Fails when broken**: the holder-pid read or the typed variant
     wiring regresses.

## Item 2 — #1373: status_remediation hermetic decision suite

### Diagnosis

`crates/taskboard-app/src/status_remediation.rs` — the seam split
(`RemediationEffects`, :30) exists but only the fold tests (:308, :329)
run hermetically; `run_remediation` / `kill_holder` / `force_unlock` /
`restart_daemon` / `holder_pid` run only inside the E2E
`tests/it_status_remediation.rs` (subprocess work, uninstrumented).
`holder_pid` reads the `pid=<n>` sidecar text via
`crate::lockfile::holder_pid` (`lockfile.rs:200`) — seedable by
writing the sidecar file in a tempdir.

### Design

A `#[cfg(test)] mod decision_tests` in status_remediation.rs (tier 1 —
the decision layer over fake effects; the wiring stays with the E2E).
Shared fixture: a `fake_effects(alive, signal_result, remove_result,
stop_leg, spawn_result)` builder over a recording `Arc<Mutex<Vec<…>>>`
log (fakes over mocks), and `seeded_workspace(db_sidecar_pid: Option<u32>)`
writing `"pid=<n>"` to `lockfile::lock_sidecar_path(db)` in a tempdir.
Scenario table (each a `#[tokio::test]`, asserted via `matches!` on
`StatusRemediationOutcome` plus recorded-effect facts):

1. kiosk posture (`may_control_processes = false`) → `Refused(IpcUnavailable)`
   for all three actions, and the effect log is **empty** (refusal
   precedes any effect — the trap-a law).
2. KillHolder, no sidecar pid → `Refused(HolderNotAlive)`, no signal recorded.
3. KillHolder, sidecar pid == `std::process::id()` → `Refused(SelfPid)`
   (seed with the real own pid — deterministic in-process).
4. KillHolder, stale pid (`alive = false`) → `Refused(HolderNotAlive)`,
   no signal recorded (the liveness re-check is the authorization).
5. KillHolder, live pid, signal ok → `HolderSignalled`, signal recorded
   for the seeded pid.
6. KillHolder, live pid, signal error → `Refused(IpcUnavailable)`.
7. ForceUnlock, live foreign pid → `Refused(HolderAlive)`, sidecar file
   still present on disk.
8. ForceUnlock, live own pid → `Refused(SelfPid)`.
9. ForceUnlock, stale pid → `LockReleased`, removal recorded **at
   `lock_sidecar_path(db)`** (path equality — the removal address is
   the contract).
10. ForceUnlock, no pid, removal error → `Refused(IpcUnavailable)`.
11. ForceUnlock, no pid, no db_path → `Refused(IpcUnavailable)`.
12. RestartDaemon, stop leg `Unavailable` → `Refused(IpcUnavailable)`,
    spawn never called (recorded log empty).
13. RestartDaemon, `Stopped` + spawn ok(pid) → `DaemonRestarted { pid }`
    (typed equality).
14. RestartDaemon, `NoDaemon` + spawn ok → `DaemonRestarted` (the
    stop-skip fact, not a refusal).
15. RestartDaemon, `Stopped` + spawn error → `Refused(SpawnFailed)`.

**Fails when broken**: any decision-table regression (wrong refusal
kind, effect fired before authorization, restart spawn on a refused
stop leg) flips a typed variant or a recorded-effect fact.

## Item 3 — #1375: repo_error constraint-mapping table

### Diagnosis

`crates/taskboard-storage-sqlite/src/error.rs:25` — the existing
`constraint_violations_map_to_corrupted` (:62) exercises only
`UniqueViolation` through a fixed `FakeDbError`; `ForeignKeyViolation`,
`NotNullViolation`, `CheckViolation`, the catch-all
`Database(_) => Unavailable` arm, and `ColumnDecode` are unexercised.

### Design

Tier 1, same module. Parametrize the fixture: `FakeDbError` gains a
`kind: sqlx::error::ErrorKind` field (fixture change, sanctioned by the
issue's "no fixture changes needed" spirit — the four-kind table *is*
the change). Table test
`every_constraint_kind_maps_to_the_corrupted_verdict`: one row per
kind → `matches!(repo_error(&err), RepositoryError::Corrupted(_))`;
plus `database_other_kind_maps_to_unavailable` (e.g. a
non-constraint kind — `ErrorKind` has no "other" variant, so use the
outer catch-all arms: `ColumnDecode` → `Corrupted` (assert the staged
`DecodeStage` via `matches!` on the detail if reachable) and `Io` →
`Unavailable` already pinned at :68; add the `Database(_)`-with-unknown
arm by constructing a second fixture kind outside the four). If
`ErrorKind` is exhaustive over the four constraint kinds, the
unknown-kind arm is pinned via the outer `_ =>` arm with an
`Io`/`ColumnDecode` row instead — record the choice in the test doc.
**Fails when broken**: any mapping flip (constraint → `Unavailable` or
catch-all → `Corrupted`) fails a row.

## Item 4 — #1367: the find verb

### Diagnosis (partial staleness — commented on the issue)

The pure projection layer is **already pinned**: `find.rs`'s
`#[cfg(test)]` module (landed with the verb itself, `6e2c8378`)
covers `project` ranking/order, `kind_filter`, limit truncation, the
empty/no-hits render, and the dangling-parent degradation. The issue's
"every function dead" claim predates that count or measured
differently. **Residual gaps**: (a) `row_for`'s habit arms (live +
tombstoned context line, #1218) — the fixture has no habits; (b)
`find_on` — the engine-seam wrapper (`shared_state().load_full()` →
`project`) is dead across test binaries; (c) the dispatch collapse
lanes — `commands/mod.rs:1663` (the JSON/stdout lane rendering
`render_text_list`) and `:2156` (the REPL/session lane calling
`find_on`) select the command in no test.

### Design

- (a) **Habit-row pins** — extend the existing find.rs fixture with a
  habit builder; two tests: live habit → context is the board title;
  tombstoned habit → context carries the tombstone marker (assert the
  `Some(context)` is present and differs from the live spelling —
  structural: `context.as_deref() != Some(live_title)` plus
  `is_some`; no message-text assertion on the marker word).
  **Fails when broken**: the habit projection or tombstone branch
  regresses.
- (b) **`find_on` engine-seam suite** — app-tier (tier 1.5) test
  spawning a real engine over `InMemoryRepository` (the focus.rs
  fixture, same as Item 1's), pushing `CreateTask`/`CreateBoard` via
  `execute`, then asserting `find_on(&app, …)` returns the typed
  `FindRow`s. O-series style: the crate's existing app-command suites
  drive lib code directly (`commands/mod.rs` header note). If booting
  a full `crate::App` (bootstrap) is what other O-series tests do,
  follow that precedent instead — stage 2 picks the lighter fixture
  that still exercises `shared_state().load_full()`.
- (c) **Dispatch collapse pin** — the established lane-table pattern:
  a typed pin that the `Find` command's lane spec routes to the
  `find` handler (mirroring the existing lane pins in
  `commands/mod.rs` — stage 2 copies the neighbouring lane-test shape;
  no subprocess, no text). **Fails when broken**: a lane-table edit
  that drops or reroutes `find` (or the `search` alias) fails the pin.

## Item 5 — #1324: journey first-match clicks order-fragile

### Diagnosis

`crates/taskboard-ui-slint/tests/journeys.rs:13165`
`click_segment` takes the **first** radio-role match for a label,
while popovers mount **last** in the tree; labels shared between an
already-mounted surface and a popover ("Alarm"/"Normal" vs the
timers' inline form) silently click the wrong element. The ordered-
fixture discipline ("last match is the popover's") is spelled in
`open_setter`'s doc (:13150) and followed only by
`click_popover_segment` (:13180) and `click_popover_label` (:13192) —
the discipline is duplicated per-helper, not enforced at the shared
address.

### Design

UI-journey integration tier (these helpers only exist in the journey
suite). Two-part fix:

1. **Fold the discipline into one helper**: a single
   `click_segment_scoped(harness, label, Mount::Inline | Mount::Popover)`
   (or a first/last boolean parameter — stage 2 picks the spelling
   that reads best at call sites); `click_segment` and
   `click_popover_segment` become one-line delegations (or call sites
   migrate — stage 2 decides by call-site count; keep the public test
   helper names stable to minimize churn). The ordered-fixture
   discipline doc moves to the shared helper; the per-helper docs
   point at it. **Fails when broken**: the existing popover journeys
   (the #1306 pins among them) fail their captured-command shapes if
   the scope resolution regresses.
2. **Debug-mode disambiguation guard**: in the shared helper (and
   `click_popover_label`), under `#[cfg(debug_assertions)]`-style
   gating or a `debug_assert!`, when a *scoped* lookup still resolves
   to more than one visible element of the addressed role, panic with
   a structural message (role + match count — a count, not UI text).
   This surfaces future shared-label ambiguities at the click site
   instead of as a wrong-element mystery. Journeys run in debug, so
   the guard is always live in the suite; stage 2 verifies no
   currently-ambiguous journey label trips it (if one does, that
   journey gets an explicit scope — which is the fix working).

## Item 6 — #1405: the NUL-byte literal (re-encode)

### Diagnosis

`crates/taskboard-app/src/secrets.rs:728` (test
`account_filenames_hex_encode_losslessly`): the fixture `"é中\u{0}"`
is spelled with a raw `0x00` byte (since `cf08f75f`), making tooling
classify the file as binary.

### Design

Replace the raw byte with the `\u{0}` escape — byte-identical string,
zero behaviour change; the existing test is the fails-when-broken
witness (it still asserts the same hex encoding). Verification: after
the edit, `file crates/taskboard-app/src/secrets.rs` reports text and
`grep -c 'SPDX'` finds the header (tooling reads the file again).
**The CI raw-control-byte check proposed in the issue is out of this
chunk's scope** (ci/ lane work) — recorded as a comment on #1405 for a
future `ci:` chunk (see Backlog section).

## Item 7 — #1404: AGENTS.md gains the two files crates (docs)

### Diagnosis

`AGENTS.md` §2 lists six crates and §6 four library crates;
`taskboard-sync-files` and `taskboard-sync-files-nextcloud` (landed
`27ae77e6`/`6e6f33e4`, 2026-11-05) are absent from both, while
`docs/architecture.org` items 7–8 carry the boundary wording and the
`Cargo.toml` members list names eight.

### Design

Edit `AGENTS.md` (this branch; it is the repo-root rulebook, checked
in):

- §2: add two bullets after `taskboard-sync-nextcloud`, boundary
  wording condensed from `docs/architecture.org` items 7–8:
  - `taskboard-sync-files`: the files-sync substrate's pure core
    (ADR 0020) — section-document codecs, tree-layout types, per-device
    fold. **Rule:** pure Rust, no network/runtime dependencies (the
    domain-crate discipline applies).
  - `taskboard-sync-files-nextcloud`: the files-sync transport actor
    over the substrate core — the WebDAV client (conditional-PUT
    discipline) on the workspace reqwest stack. **Rule:** network IO
    lives here, never in the substrate core.
- §6: add both to the `MIT OR Apache-2.0` library list (verified
  against their `lib.rs` SPDX headers).
- Consistency check (stage 2): the §2 bullet set and the `[workspace]
  members` list agree one-to-one; the optional automated members-vs-§2
  check from the issue is out of scope here (commented on #1404).

**Fails when broken (doc-consistency acceptance)**: stage 3 re-derives
the member list from `Cargo.toml` and each crate's licence tier from
its `lib.rs` header and diff-checks §2/§6 against them.

---

## Stage-2 work list (ordered)

1. #1405 re-encode (one-line) and #1404 AGENTS.md edit — commit
   together as the docs/small pair.
2. #1375 table (error.rs) — smallest suite first.
3. #1373 decision_tests (status_remediation.rs).
4. #1374: resolver_tests + sink/engine tests + capacity pin +
   `quit_until_accepted` pin (ui.rs) and the new main.rs test module
   with the `blocked_fallback_error` extraction refactor.
5. #1367: habit-row pins (find.rs fixture) → `find_on` suite → lane
   collapse pin (commands/mod.rs).
6. #1324: journey helper fold + debug guard (journeys.rs; run the
   journey suite subset via the memory gate, full suite at the gate).
7. Commit per item or per coherent pair
   (`test(<module>): …`); main.rs's small refactor is its own commit
   (`refactor(app): extract the blocked-fallback error construction`
   or fold into the test commit — keep every commit standalone-buildable
   for the rebase merge).

Gate discipline: pre-commit hooks + flock-wrapped
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(background + poll; commit landed work before waiting). If the lock is
contended and no useful work remains: commit, report
"gate deferred: lock held by <holder>", end the stage.

## Stage-3 traps

- **Text assertions creep**: the tombstoned-habit context, the
  no-hits render, and the journey guard message are the three
  temptation sites — assert structure (`Option` shape, match counts,
  typed inequality), never marker words.
- **`resolve_day_start`'s offset leg** must use single-read capture,
  not a re-sampled comparison across a tick boundary.
- **`quit_until_accepted`'s pin** hangs if the paused-clock
  interaction with `taskboard_ui_slint::event_loop_running` is not
  checked first — probe that seam before writing the test.
- **The engine-spawn fixture** (Items 1 and 4b) must reuse the
  focus.rs precedent verbatim (channel capacities, parked senders) —
  a "simplified" fixture that drops the reports sender parks nothing
  and wedges the engine shutdown.
- **#1324's debug guard**: if a currently-green journey trips it, that
  is the guard working — do not remove the guard; scope that journey's
  clicks explicitly.
- **#1274 probe repetition**: **N/A for D2** — the #1274/#1424
  clippy-lane residue is chunk D1's item (`ci/scheduled-lanes-debt`);
  D2 adds no CI lanes and runs no cold-clippy-outside-gate commands.
  Stage 3 verifies D1's landed state instead of re-probing here.
- **find.rs staleness**: do not "restore" tests for the pure layer
  that already exist (6e2c8378) — only the residual gaps in Item 4.

## Backlog issues

No new `backlog:` issues required: the two deferred sub-ideas (the
#1405 CI control-byte check; the #1404 members-vs-§2 automated check)
belong to their existing issues and were recorded as comments there
(stage 1 comments, see the chunk report); #1367's staleness finding
was likewise commented on the issue. Numbers to report: none new.

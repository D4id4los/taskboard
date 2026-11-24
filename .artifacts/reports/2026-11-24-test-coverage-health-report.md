# Test Coverage Health Report — has the suite kept up since the core-crates era?

**Date:** 2026-11-24 · **Working-tree commit:** `df5ae1a0` (2026-11-24 17:32) ·
**Core-era anchor commit:** `93ab5fb9` (2026-10-09, "fix(state): Phase 3 review
fixes" — the State Engine actor's landing commit)
**Author:** coverage-health analysis pass (LLM agent), per the task in the
session log. Read-only analysis: no production or test code changed.

---

## 1. Methodology

**Local coverage (authoritative per-file view).** The CI-identical
three-command sequence was run through `scripts/run-quality-gates.sh` (the
flock wrapper) at `df5ae1a0`:

```
cargo llvm-cov nextest --workspace --all-features --no-tests=warn --no-report
scripts/coverage-quarantine-profraw.sh target/llvm-cov-target
cargo llvm-cov report --lcov --output-path lcov.info
```

Result: **5,506 tests run, 5,506 passed, 0 failed, 65 skipped** (90.8 s
run; the skipped set is the documented `#[ignore]` tier gates — live
Nextcloud/docker/keyring/focus-probe; the workspace has 71 `#[ignore]`
markers, of which 6 are feature-gated out of this resolution). All 5,831
profraw files valid after quarantine.

**Core-era snapshot.** A detached worktree
(`../taskboard-WT-test-coverage-health-core-era`, doctor-checked first) at
`93ab5fb9` ran the identical sequence (260 tests, 0 failures) against the
old `Cargo.lock`. The per-crate numbers below come from that run.
*The worktree was removed after extraction; nothing was committed there.*

**Codecov (trend view).** `api.codecov.io` v2, public endpoints: repo
totals plus the full commit series (**1,485 commits with coverage, 75
pages**, fetched 2026-11-24). The web app is a JS SPA and renders nothing
for a fetcher, and the `file_tree`/`file_report` REST endpoints returned
404 — so per-file drill-downs come from the local lcov, and Codecov was
used for what it does best: the longitudinal curve. Patch-vs-base status
is `informational` by config (`codecov.yml`, the documented
logging-arms gap).

**Measurement pitfalls handled (matters for reproduction):**

- `cargo llvm-cov` emits **one SF record per (file × test binary)**. Naive
  per-record sums double-count every production file linked into several
  test binaries. All numbers below merge per-line across records
  (a line is covered if any binary covers it). Validation: the merged
  core-era total reproduces Codecov's stored figure for `93ab5fb9`
  **exactly** (97.64% @ 7,129 lines).
- Never-executed-function detection aggregates `FNDA` counts per
  (file, `FN` start line) across all records: a function counts as dead
  only when **every** binary's record shows 0 — monomorphizations count
  as executed if any instantiation ran.
- The lcov `LF` counts are executable lines (no comments/blanks), which
  is why they differ from `wc -l` source counts.

---

## 2. Overall & per-crate trend

### 2.1 The headline curve (Codecov, `main`)

| date | coverage | executable lines | missed |
|---|---|---|---|
| 2026-10-09 (first upload) | 98.49% | 4,396 | 66 |
| 2026-10-09 (state actor, `93ab5fb9`) | 97.64% | 7,129 | 168 |
| 2026-10-18 | 95.56% | 24,822 | 1,102 |
| 2026-10-19 (dip → recovery) | 92.97% → 95.72% | 30,459 | 1,299 |
| 2026-10-22..28 (dip) | 91.07–91.66% | 37,556–57,985 | 3,129–4,833 |
| 2026-10-29 | 94.54% | 63,985 | 3,490 |
| 2026-10-31..11-05 (dip, worst) | 91.25–91.56% | 72,464–91,490 | **7,721 peak (11-05)** |
| 2026-11-06 (recovery wave) | 95.28% | 94,901 | 4,473 |
| 2026-11-08..11-23 | 94.98–95.60% | 107,621–140,155 | 4,730–6,875 |
| **2026-11-24 (now)** | **95.18–95.25%** | **142,096** | **6,748** |

The pattern is a sawtooth: big UI/app feature waves drop coverage ~3–4 pp
(floors ~91%), followed within days by deliberate test backfill waves
that restore ~95%. The floor never broke below ~91%, and the recovery
waves (10-19, 10-29, 11-06) coincide with the test-suite commits visible
in the log for those dates (the 11-06 recovery rides the UI Tier-2
journey infrastructure landing). The codebase grew **32×** (4.4k → 142k
executable lines) over the window.

### 2.2 Per-crate: then vs now (local merged lcov)

| crate | then (2026-10-09) | now (2026-11-24) | Δ | missed then | missed now |
|---|---|---|---|---|---|
| `taskboard-domain` | **99.14%** (4,859 ln) | **98.00%** (48,768 ln) | −1.14 | 42 | 977 |
| `taskboard-state` | 91.08% (381 ln) | **98.23%** (1,410 ln) | **+7.15** | 34 | 25 |
| `taskboard-sync-nextcloud` | 95.88% (970 ln) | 95.14% (6,106 ln) | −0.74 | 40 | 297 |
| `taskboard-storage-sqlite` | 96.23% (901 ln) | 96.89% (3,828 ln) | +0.66 | 34 | 119 |
| `taskboard-app` | stub, 0% (18 ln) | **91.56%** (36,293 ln) | new | 18 | **3,063** |
| `taskboard-ui-slint` | stub | 96.32% (37,410 ln) | new | — | 1,377 |
| `taskboard-sync-files` | — (landed 11-05) | 96.81% (3,413 ln) | new | — | 109 |
| `taskboard-sync-files-nextcloud` | — (landed 11-05) | 92.71% (6,037 ln) | new | — | 440 |
| **total** | **97.64%** (7,129 ln) | **95.53%** (143,265 ln) | −2.11 | 168 | **6,407** |

Reading:

- **The core crates held.** `taskboard-domain` carries **10×** the lines
  at −1.14 pp; `taskboard-state` *improved* 7 pp (its 91.08% "then" is a
  two-days-old suite at the anchor commit; it is now the best-covered
  crate in the workspace at 98.23% with 25 missed lines total).
  `taskboard-storage-sqlite` improved slightly; `taskboard-sync-nextcloud`
  lost 0.74 pp while growing 6× (its rewrite added `move_marker.rs`, the
  worst file in the workspace — §3.1).
- **The erosion is concentrated in `taskboard-app`**: 3,063 missed lines
  = **48% of the workspace's entire miss mass** at 91.56%, the lowest
  crate. Second-thinnest is the 3-week-old
  `taskboard-sync-files-nextcloud` (92.71%, 440 missed).
- The new crates did *not* land at the core crates' bar:
  sync-files-nextcloud and app sit 5–7 pp under the domain/state band,
  while sync-files (the pure fold/codec crate) landed at 96.81% —
  tracking the domain pattern, exactly as §1/§4 of the testing strategy
  intend: purity is testable, IO actors are where suites thin out.

---

## 3. Findings, ranked by risk

Ranked by *risk to behavior*, per the mission — not by uncovered line
count. "Dead" below = never executed in any hermetic test binary (the
cross-binary FNDA merge); each finding names the behavior missing, the
introducing commit, and the test approach that fits the project's own
standards (typed outcomes, fakes over mocks, lowest layer).

### F1 — `write_move_marker`: the mixed-fleet marker path has no test at any tier (P1)

- **File:** `crates/taskboard-sync-nextcloud/src/move_marker.rs` —
  **24.29%** (17/70), worst file in the workspace.
- **Evidence:** the only covered path is the degenerate one: `stacks()`
  succeeds with *no live stacks* → typed no-op. Dead cross-binary: the
  anchor-scan closures (51–54), the anchor-card creation block (61–75),
  the sidecar download + parse (77–89), the fresh-document build
  (92–97), the marker insert + upload/update split (99–118), the
  `NotFound`/`Forbidden` no-op arm (46–47) and the
  `UnreadableDocument` typed error (89). Introduced `ca961b3d`
  2026-11-09 ("other devices adopt a moved board"); unchanged since. No
  wiremock, docker-tier, or live-tier test drives it (grep over
  `tests/` finds only a comment in
  `crates/taskboard-app/tests/services/ui_settings.rs:2257` about the
  no-op case).
- **Why it matters:** this is D-BF11 — the *only* channel that tells a
  device without the files tier that its board moved. A regression here
  is silent: mixed-fleet moves stop propagating, or worse, the
  `UnreadableDocument` guard (which exists to prevent *destroying* a
  sidecar this build cannot read) is never exercised, so the
  never-rewrite safety property has zero tests.
- **Test approach:** Tier-1 wiremock against the `DeckClient` harness
  (the same style as the existing deck-client contract suites): script
  stacks/attachments reads, assert the merged sidecar document by
  reading the uploaded attachment back; pins for the anchor-found,
  anchor-created, download-merge, and fresh-document paths; typed pins
  for `MoveMarkerError::UnreadableDocument` and the
  `NotFound`/`Forbidden` no-ops. The function is already idempotent by
  contract, so a repeat-call pin is one more case, not a new harness.

### F2 — the `taskboard find` verb has no tests at all (P2)

- **File:** `crates/taskboard-app/src/commands/find.rs` — 78.36%
  (58 missed); `find`, `find_on`, `render_text_list`, `kind_filter`,
  `row_for`, `project` all dead cross-binary.
- **Evidence:** zero drivers anywhere — no hermetic handler suite, and
  no binary-driving test in `tests/cli/`, `it_cli.rs`, or `it_docker.rs`
  selects the `find` command (grepped across all `tests/` trees;
  every `find` hit is `Iterator::find`). Landed `6e2c8378` 2026-10-24
  ("the top-level find verb") — three weeks untested.
- **Why it matters:** a complete user verb — argv parsing, board/task/
  habit-row projection, kind filtering, prose rendering, exit paths —
  with no behavioral pin. Any refactor of the row projections or the
  command dispatch can change or break `find` invisibly; the
  mutation-testing lanes do not cover app commands beyond the pdf
  window (#1338 tracks the missing lane).
- **Test approach:** the established app-tier pattern: a hermetic
  `find_on` suite over seeded tempdir state asserting typed
  `FindRow` projections and the kind-filter decisions (the O-series
  style), plus a collapse/dispatch pin so the argv surface stays wired.
  One behavior, one test, at the handler layer; the prose renderer gets
  structural pins only (no text assertions).

### F3 — five habits subcommands are wired but never tested (P2)

- **File:** `crates/taskboard-app/src/commands/habits.rs` — 80.53%
  (237 missed); dead cross-binary: `skip_on`, `pause_on`, `resume_on`,
  `history_on`, `stats_on`, `render_history`, `render_stats`,
  `window_state_view`, `row_state_label`.
- **Evidence:** `cli.rs:1768–1811` wires `habits skip|pause|resume|
  history|stats`; no test file anywhere drives them (grep across all
  test trees). The H-series (`tests/cli/cli_habits.rs`) covers
  add/log/adjust/update/delete only. `run`/`run_on`/`adjust_on` are
  covered (35/36 executions in the `cli` test binary), so the gap is
  exactly the five newer subcommands. `history`/`stats`/the window
  state view are the newest layer (2026-11-16 `6d240f0b`, 2026-11-24
  `fb52ebec`).
- **Why it matters:** the *engine semantics* of skip/pause/resume are
  well tested (state/domain at 98%); what is untested is the CLI
  receipt layer — selector resolution, receipt projection, the
  renderers, and the window-state label mapping. This is precisely the
  layer where a refactor can desynchronize receipts from engine facts
  without any test noticing.
- **Test approach:** extend `cli_habits.rs` in the H-series style:
  per-verb receipts asserted as typed values, refusals via
  `matches!`/`error_kind()`, the renderer pins structural, history/stats
  projections on seeded series (with clock rules per testing-strategy
  §6 — the window view is a wall-clock consumer and must anchor the
  fixture, not read `Local::now` unseeded).

### F4 — desktop notification delivery executes nowhere (P2)

- **File:** `crates/taskboard-app/src/notifications.rs` — 73.34% (165
  missed); dead cross-binary: `DesktopSink::deliver` (197) and
  `DesktopSink::withdraw` (247), the focus-boundary surfaces —
  `mount_focus_boundaries` (631), the `boundary` impls (544/558/600),
  `boundary_phrases` (586, pure), `resolve_boundary_surface` (530).
  Covered: `LogSink`, the queue/actor machinery, and the pure payload
  mappers `toast_payload`/`body_kind` (the inline test module).
- **Why it matters:** reminder/alarm/break firings reach the desktop
  *only* through `DesktopSink` — a payload-mapping or withdrawal-handle
  regression ships silently (no toast, no error surface: the consumer
  logs and drops by design, D-R13). The focus-boundary notification leg
  (the session boundary toasts) is equally dark. Much of the surface is
  the module's own port seam — `NotificationSink` was deliberately
  designed OS-free "so the port's implementors stay OS-free" — but no
  fake implements that port in any test today.
- **Test approach:** a handwritten fake notify backend behind the
  sink's backend seam (fakes-over-mocks §3): assert the D-R11 payload
  mapping (anchor-typed ids, alarm insistence → platform-critical),
  the withdrawal-handle lifecycle and `TRACK_CAP` behavior, and the
  typed `NotifyError` path. `boundary_phrases` is pure — a plain unit
  pin. No DBus needed anywhere.

### F5 — the CLI dispatch table's verb→handler closures are largely unexecuted (P2)

- **File:** `crates/taskboard-app/src/commands/mod.rs` — 76.75% (412
  missed, largest app file by miss mass).
- **Evidence:** dead cross-binary dispatch closures: boards
  `archive`/restore (`1218`/`1227`), `delete` (1236), `reset` (1245),
  `unbind` (1324); labels `restore` (1451); tasks
  `undo`/`undo_to`/`redo`/`redo_to`/`history` (1562–1609); the find
  render arm (1621); `outcome_mismatch` (966). The *handlers* are
  covered — the direct-call suites exercise them — so what is
  untested is the argv→handler *routing*, the wiring the real binary
  actually runs.
- **Why it matters:** routing bugs are the classic silent class: a
  wrongly-wired closure (wrong verb, dropped flag, swapped restore/
  archive boolean) passes every handler-level suite. Related known
  issues: #1230 (the exit-code family is mutation-survivable across
  seven verbs — same remedy class) and #1176 (the pdf-export dispatch
  branch untested in its feature tier).
- **Test approach:** generalize the `habits::collapse` pattern (one
  parse-through-clap + collapse + dispatch assertion per verb, asserted
  as the typed invocation — no text). A single table over the verb list
  closes both this finding and #1230's exit-code survivors in one
  harness.

### F6 — the GUI export dialog's Run path stops at field grammar (P2)

- **Files:** `crates/taskboard-ui-slint/src/export_dialog.rs` — 72.09%
  (24 missed; `parse_date`, `mint_spec`, `refusal_index`,
  `ExportVm::default` dead) and its adapter call sites
  (`adapter.rs:8990`, `9085`, `9124`, `9127`, `9373` — the
  mint/refusal wiring) unexecuted.
- **Evidence:** the Tier-2 journeys (`tests/journeys/export_journeys.rs`)
  cover the field-edit grammar (validity flags, the Run gate disabling)
  but never complete a Run, so `mint_spec` → `ExportSpec` and the
  refusal-index mapping never execute. Landed `e40f3c54` 2026-11-21
  (#948), wired further `3ca3fc10` 11-23.
- **Why it matters:** the dialog's entire purpose is producing the
  export spec; only its input validation is proven. A regression in
  date minting, spec defaults, or refusal→index mapping ships as a
  silently wrong export.
- **Test approach:** extend the journey to drive the Run gate with
  valid fields and assert the minted `ExportSpec` at the fake-engine
  boundary (`support/fake_engine.rs` — the same routing-only seam rule);
  pin `refusal_index` exhaustively over `ExportRefusal` (pure fn).

### F7 — files-sync-nextcloud: the error projection and cycle error paths are dark (P2)

- **Files:** `status.rs` **42.86%** (the `From<&TransportError> for
  LaneFailure` arms), `transport.rs` 70.59%, `poll.rs` 80.00%,
  `cycle.rs` 84.57% (145 missed incl. `adopt_own_file`, `steady_cycle`
  helpers, serde/`fmt` arms).
- **Evidence:** the crate's celebrated fake-transport matrix pins the
  *success* cycle behaviors (attach/adoption/quarantine/fork) but no
  test scripts a `Forbidden`/`NotFound`/`BadRequest`/`Server(status)`/
  `RateLimited` wire error through the projection into a lane decision —
  the corresponding `LaneFailure` arms never fire. The crate landed
  2026-11-05 and is already the second-thinnest (92.71%).
- **Why it matters:** this is the error/retry decision layer of a sync
  actor: which failures quarantine, which retry, which surface. Those
  decisions currently run for the first time in production.
- **Test approach:** extend the scripted fake-transport matrix (the
  existing `test-support` feature harness) with error-scripted cycles;
  assert typed `LaneFailure` outcomes and the state transitions they
  drive. Time-domain rules already satisfied: the matrix runs under
  tokio pause.

### F8 — status remediation decision logic is E2E-only (P3)

- **File:** `crates/taskboard-app/src/status_remediation.rs` — 55.21%
  (43 missed); dead: `run_remediation`, `kill_holder`, `force_unlock`,
  `restart_daemon`, `holder_pid`, and the `*_real` seam fns.
- **Evidence:** the only driver is `tests/it_status_remediation.rs`,
  which drives the *binary* (subprocess work is not instrumented — the
  documented coverage note in testing-strategy §8). The seam split
  (`pid_alive_real`/`signal_term_real`/`spawn_daemon_real`) exists
  precisely to make the decision layer hermetic-testable; nothing uses
  it in tests. Landed `e743041c` 2026-11-23.
- **Why it matters:** kill-vs-force-unlock-vs-restart decisions about a
  possibly-live daemon currently have no deterministic test; the E2E
  proves wiring, not the decision table.
- **Test approach:** hermetic suite over the decision layer with fake
  seam impls (injected alive/signal/spawn results), asserting typed
  `StatusRemediationOutcome`s per scenario (stale holder, live holder,
  unlock refused, restart fails).

### F9 — app boot/wiring seams without hermetic coverage (P3)

- **Files:** `crates/taskboard-app/src/ui.rs` — 78.85% (195 missed;
  `CommandSink::dispatch`, `ui_command_channel`,
  `surface_keymap_findings`, `resolve_export_defaults`,
  `resolve_day_start`, `resolve_board_infos`, `quit_until_accepted`,
  `run_desktop` dead); `main.rs` — 62.38% (196 missed;
  `apply_no_ipc_override`, `exit_blocked_fallback`, the daemon/IPC verb
  lanes dead).
- **Why it matters / scoping honesty:** part of this is *structural* —
  `run_desktop` needs a display and the binary lanes are uninstrumented
  (documented note); those belong to the E2E tiers. But the config
  resolvers (`resolve_export_defaults`, `resolve_day_start`) and the
  command-sink dispatch seam are pure or fake-engine-testable and sit
  dark today; `apply_no_ipc_override`/`exit_blocked_fallback` are
  likewise cheap pure pins.
- **Test approach:** table tests for the resolvers (config → typed
  settings), a fake-engine dispatch assertion for the sink, direct unit
  pins for the two main.rs seams. Leave display-bound paths to Tier 2/4.

### F10 — `repo_error`'s constraint mapping exercises one of four kinds (P3)

- **File:** `crates/taskboard-storage-sqlite/src/error.rs` — 59.09%.
- **Evidence:** the mapping table test constructs `FakeDbError` with
  `kind() == UniqueViolation` only; `ForeignKeyViolation`,
  `NotNullViolation`, `CheckViolation`, and the catch-all
  `Database(_) => Unavailable` arm are unexercised. (The dead fixture
  impls — `message`/`as_error`/`into_error`/`fmt` — are trait-mandated
  boilerplate sqlx never calls in this path; they inflate the miss
  count but are not gaps.)
- **Why it matters:** small, but this mapping is the storage actor's
  entire corruption verdict logic (deciding `Corrupted` vs transient
  `Unavailable` decides quarantine behavior).
- **Test approach:** extend the existing mapping test to a four-kind
  table (one `matches!` row each); no fixture changes needed.

---

## 4. Structural blind spots (patterns, not files)

1. **Uncovered `Err`/defensive arms are the dominant miss class.**
   Beyond F5/F7/F10: `taskboard-domain/src/undo.rs` (87.24%, 201
   missed) is single-line scatter — typed refusal arms
   (`TargetNotUndoable(BoardCeremony)`, `NothingToUndoForBoard`, …)
   inside fully-covered, property-tested planners; `command.rs`,
   `pipeline.rs`, `appdata.rs` show the same shape at 96–97%. This is
   the *accepted* residue class (`codecov.yml` documents the deliberate
   logging-arm gap; AGENTS §5 forbids text assertions that would pin
   log arms) — the residual risk is low because the planners' cores are
   proptest-bombarded (§4 of the testing strategy). Not issue-tracked
   per-arm; the mutation-testing lanes are the right oracle for
   survivors here (#1338 tracks the missing app lane).
2. **Direct-handler-call suites leave routing thin.** The app-tier
   suites call `commands::*_on` functions directly; the clap dispatch
   table that production actually executes is covered only where a
   `collapse`-pattern test exists (habits). F5 is the census; the
   remedy is one shared harness, not per-verb suites.
3. **Feature-complete verb families without drivers** (F2, F3): both
   landed as "verb + handler + cli wiring" commits whose test waves
   covered sibling verbs but not these. The per-crate quality gates
   catch *coverage* drops only at crate granularity — a 58-line verb
   inside a 36k-line crate is invisible to the sawtooth.
4. **Tier-2 journeys prove grammar, not completion.** The export dialog
   journeys (F6) validate field validity flags but never finish the
   flow; the pattern generalizes: journeys assert *interaction
   semantics*; nothing asserts the *produced artifact* at the
   fake-engine boundary for dialog-driven flows.
5. **OS-boundary modules sit dark above their seams when no fake
   implements the seam** (F4, F8). The seams exist by design; the
   missing piece is always a handwritten fake, never a mock library —
   consistent with §3 of the testing strategy.
6. **No zero-coverage files exist** — every one of the 194 files
   executes somewhere (floor: `move_marker.rs` at 24%). The suite
   reaches everything; it just stops early in the IO/actor layers.
7. **The 65 skipped tests are by design** — the documented `#[ignore]`
   tier gates (live/docker/keyring/focus probes). The coverage suite
   stays hermetic by construction, as §8 requires.

---

## 5. Baseline verdict

**Has coverage held, eroded, or improved since the core-crates stage?**

- **Overall: eroded, but bounded and actively fought.** 97.64% → 95.53%
  (local merged; Codecov agrees: 97.64% → 95.18–95.25%) while the
  executable surface grew 20× (7.1k → 143k lines) and missed lines grew
  38× (168 → 6,407). Two dip-recover cycles bottomed at ~91% and were
  deliberately restored within days; the floor never broke. The patch
  gate is informational by config, so the maintenance is human
  discipline, not enforcement — and the discipline has held at the
  crate level.
- **Where it held: the core crates.** `taskboard-domain` carries 10×
  the lines at 98.00% (−1.14 pp); `taskboard-state` *improved* to
  98.23% (+7.15 pp, now the best in the workspace);
  `taskboard-storage-sqlite` improved; `taskboard-sync-nextcloud` held
  within a point of its era value while growing 6×. The pure-tier
  strategy (now-as-data, injected clocks, proptest) is demonstrably
  doing its job.
- **Where it eroded: the app tier and the newest IO actor.**
  `taskboard-app` (91.56%, 48% of all missed lines) and
  `taskboard-sync-files-nextcloud` (92.71%) sit 4–7 pp under the
  domain/state band — the new crates did not land at the core crates'
  bar. Inside them, the erosion is not uniform noise: it is
  *concentrated, recent, and named* — a cross-tier sync path with zero
  tests at any tier (F1, 2026-11-09), a complete verb family untested
  for three weeks (F2, 2026-10-24), five wired subcommands without
  drivers (F3), a three-week-old sync crate whose error-decision layer
  has never run in a test (F7), and a GUI Run path that stops at
  validation (F6, 2026-11-21).
- **One-sentence answer:** the suite is *stronger than it has ever been
  in absolute terms* (5,506 tests, every file reached, core crates at
  98%+) but *weaker at the edge that grew*: the last six weeks of
  UI/app/sync-feature waves outpaced the test waves in specific,
  enumerable places, and eight of those places are concrete, cheaply
  testable gaps — not a general decay.

---

## 6. Issue list

Opened/commented per AGENTS §7 (searched first; one issue per idea;
bodies are summaries + a permalink here):

| issue | finding | one-line |
|---|---|---|
| [#1366](https://github.com/D4id4los/taskboard/issues/1366) | F1 | `write_move_marker` untested at every tier |
| [#1367](https://github.com/D4id4los/taskboard/issues/1367) | F2 | `taskboard find` verb: no tests at all |
| [#1368](https://github.com/D4id4los/taskboard/issues/1368) | F3 | five habits subcommands wired but never tested |
| [#1369](https://github.com/D4id4los/taskboard/issues/1369) | F4 | DesktopSink + focus-boundary delivery executes nowhere |
| [#1370](https://github.com/D4id4los/taskboard/issues/1370) | F5 | CLI dispatch closures unexecuted (census also commented on [#1230](https://github.com/D4id4los/taskboard/issues/1230#issuecomment-7145169287)) |
| [#1371](https://github.com/D4id4los/taskboard/issues/1371) | F6 | export dialog Run path untested past field grammar |
| [#1372](https://github.com/D4id4los/taskboard/issues/1372) | F7 | files-sync-nextcloud error projection/cycle error arms dark |
| [#1373](https://github.com/D4id4los/taskboard/issues/1373) | F8 | status_remediation decision logic E2E-only |
| [#1374](https://github.com/D4id4los/taskboard/issues/1374) | F9 | app boot/wiring seams without hermetic coverage |
| [#1375](https://github.com/D4id4los/taskboard/issues/1375) | F10 | `repo_error` constraint mapping: one of four kinds exercised |

## 7. Reproduction

- Local suite: `scripts/run-quality-gates.sh bash -c 'cargo llvm-cov
  nextest --workspace --all-features --no-tests=warn --no-report &&
  scripts/coverage-quarantine-profraw.sh target/llvm-cov-target &&
  cargo llvm-cov report --lcov --output-path lcov.info'`
- Core-era snapshot: detached worktree at `93ab5fb9`, same sequence.
- Codecov series: `GET api.codecov.io/api/v2/gh/D4id4los/repos/taskboard/commits?page=N`
  (75 pages; local copy was at `/tmp/codecov-commits/`, not archived —
  refetch on demand).
- Merged per-line accounting: one SF record per (file × test binary);
  merge `DA` records by `(path, line)` before summing. The naive
  per-record sum overstates lines by ~4% at HEAD and reproduces
  Codecov's totals only after the merge.

# [Report] M1 Exit Analysis — What to Build Before the GUI (Phase 7)

- **Date**: 2026-10-11
- **Context**: Phase 6 merged on `main` (M1 reached per the user; the
  tier-3 runbook is the acceptance record). This report surveys the
  built state, the backlog, CI health, and fresh coverage numbers, and
  recommends a work order for the pre-GUI window.
- **Method**: read roadmap + all phase plans + backlog; inspected the
  app/domain/state sources; ran `cargo llvm-cov --workspace` locally
  (SQLX_OFFLINE, hermetic tier — `#[ignore]`d docker tests excluded);
  inspected GitHub Actions history via `gh`.

---

## 1. What exists at M1 (verified in code, not just plans)

- **Core chain complete**: domain (entities, merge policy, pipeline,
  outbox, ports) → storage-sqlite (repo + actor + migrations) → state
  engine (command loop, ArcSwap publish, receipts) → sync actor
  (conditional-read poll, evidence-then-verdict push) → app bootstrap
  (config/secrets/lockfile/lifecycle).
- **CLI surface** (`cli.rs`): `login`, `boards list/select`,
  `tasks list/add/done/move`, `sync`, `daemon`; global `--json`;
  exit codes 0/1/2/130. Handlers are lib code (`commands/`) driven
  hermetically by L/V/K/O-series suites; J-series drives the real
  binary against dockerized Nextcloud.
- **Domain verbs with no CLI**: `UpdateTask` (title/description/
  duedate/archived), `DeleteTask`, `CreateStack`, `RenameStack`,
  `DeleteStack`, `CreateLabel`, `UpdateLabel`, `DeleteLabel`,
  `AssignLabel`, `UnassignLabel` — all planned, none surfaced
  (backlog entries exist, five of them duplicated, §5).
- **Structural Nextcloud coupling**: `bootstrap()` unconditionally
  loads credentials and builds a `DeckClient` (bootstrap.rs:265-266);
  `AppConfig::validate` requires `server_url`/`username`. No command
  runs on a never-logged-in machine. Boards **only** appear via sync
  adoption (`command.rs` `live_board()`; `CreateStack` etc. reject with
  `NoBoard`) — there is no local board-creation path at all.
- **Engine is already connector-tolerant**: the actor loop flags a
  closed sync-reports channel (`reports_open`) and keeps serving
  commands; `RequestSync` with no sync actor is a tolerated `debug!`
  (`actor.rs:458-477`, "no sync actor exists in engine-only tests").
  The stand-alone feature is therefore mostly an app-layer +
  board-creation change, not an engine rewrite.
- **Handler seam is REPL/IPC-ready in shape**: every verb already
  wraps an `AsyncFnOnce(&App)` body in `with_core` (tasks.rs:32-43).
  Extracting those bodies to take `&App` directly is the one refactor
  both a REPL and daemon IPC need.

## 2. Coverage snapshot (fresh, hermetic tier only)

`cargo llvm-cov --workspace` on `main` (clean tree), 2026-10-11:

| Scope | Lines | Notes |
|---|---|---|
| Workspace total | **92.53%** | 16,786 regions, 12,090 lines |
| taskboard-domain | ~99%+ | audited 2026-10-09 (see that report) |
| taskboard-state | ~93% | actor.rs 92.92% |
| taskboard-storage-sqlite | ~95% | error.rs 53.66% (mapping arms) |
| taskboard-sync-nextcloud | ~88.5% | **push_exec.rs 76.96%** |
| taskboard-app | **80.33%** | commands/mod.rs 58.42% |

Classified gaps:

1. **`push_exec.rs` 76.96% (255 lines)** — the stack/label push
   execution paths (`CreateStack`/`RenameStack`/`DeleteStack`/
   `CreateLabel`/… arms, roughly lines 163-324 plus error arms) have
   **no hermetic wiremock tests**; the hermetic tier exercises task
   pushes only. Genuine gap, and it grows in importance the moment the
   CLI grows `stacks`/`labels` verbs (users will push those ops).
2. **`commands/mod.rs` 58.42%** — the `dispatch` match arms and
   `report()` are routing glue driven only by subprocess (J/K) tests,
   which llvm-cov does not instrument. A small hermetic
   dispatch-routing suite (call `dispatch()` per `Command` variant,
   assert exit code 0/1) is legitimate routing-only testing and cheap.
3. **`secrets.rs` 72%** — live `KeyringStore` paths need a Secret
   Service; covered by the locally-run `TASKBOARD_IT_KEYRING=1` test,
   ignored on headless CI. Justified; already documented practice.
4. **`storage-sqlite/error.rs` 53.66%** — sqlx→port error mapping
   arms; partially coverable with corrupted-row fixtures, partially
   defensive. Low priority.
5. **Known accepted gap** (existing backlog entry): DeckClient
   endpoint logging arms; the crate's remaining client.rs uncovered
   lines match it.

Caveat: docker-tier and live-tier tests are `#[ignore]`d and do not
run under llvm-cov; binary-driving tests run their logic in
uninstrumented subprocesses (documented in the Phase 6 plan §6). The
flake watchlist entry (`actor_wiremock` under instrumented runs) did
**not** reproduce in this run.

## 3. CI health findings

- The **only ever run** of `nextcloud-integration.yml` (manual
  dispatch 2026-10-10 18:12, on `feat/app-bootstrap`) **failed**:
  `libdbus-sys` build-script panic — the runner lacked libdbus headers
  for the `keyring` dependency. The fix (apt-get `libdbus-1-dev` step)
  is already on `main`'s copy of the workflow, but the workflow **has
  never been re-dispatched and has never gone green**. The Phase 6
  "CI proof" step is therefore incomplete, and the first *scheduled*
  run (Wednesdays 04:00 UTC) will be the workflow's real maiden
  voyage. **Action: dispatch it on `main` now and confirm green.**
- Recent `ci.yml` runs on `main` are green. Mid-branch failures on
  2026-10-10 were transient/iterative, not standing issues.
- Per-PR CI stays hermetic by design (Docker Hub rate limits);
  nothing to change there.

## 4. Evaluation of the five candidate directions

### A. Stand-alone-first (sync optional) — **recommended first**

The backlog already carries this as a 2026-10-10 **user decision**
("Implement shortly after Phase 6, while the architecture change is
still minor") with a four-step sketch. Code inspection confirms the
sketch and adds one load-bearing fact it understates: **boards
currently only exist via sync adoption**, so stand-alone mode needs a
new local-board path (`CreateBoard`-style `StateCommand` + CLI
`boards init`/`init`), not just config/bootstrap changes. Scope:

- `[sync] backend = "none" | "nextcloud"` config (default `none` for
  fresh installs); validation skips `server_url`/`username` when
  `none`.
- `bootstrap()` skips credential load, `DeckClient`, and
  `spawn_sync_actor` for `none` (engine tolerance verified, §1).
- Domain: `CreateBoard { title, color }` command + persistence +
  single-board semantics on a local-only board. Outbox ops authored
  with no remote bindings must drain safely when a connector is later
  attached — and attach semantics must be defined (the single-board
  `min(BoardId)` invariant would mis-target with both a local and an
  adopted board; simplest honest rule: refuse attach while a
  local-only board exists, or bind-by-explicit-command).
- Connector port formalization in domain (SyncCommand/SyncReport
  vocabulary is already connector-agnostic); `taskboard-sync-nextcloud`
  becomes the first connector. ADR + `architecture.org` amendment
  ("stand-alone by default, sync when configured").

Why first: every later item (verbs, REPL, IPC, **and the Phase 7 UI's
first-run experience**) sits on the bootstrap shape A produces; A also
makes hermetic CLI tests trivial (no wiremock needed for verbs under
`backend = "none"`), which cheapens everything after it.

### B. Full CLI editing surface — **recommended second**

All domain verbs exist (§1); the work is additive app-crate handlers
in the established pattern. Bundle: `tasks edit/delete`; selector
upgrade (`resolve_task`: exact id → unique prefix ≥8 chars → exact
title, mirroring `resolve_stack`); `stacks add/rename/delete/move`;
`labels add/edit/delete`; `tasks label/unlabel`; `logout`
(`CredentialStore::delete` already ships); `config print`. Two small
pre-GUI UX items ride naturally here: **XDG config discovery**
(existing backlog entry — desktop deployments expect it before a GUI
exists) and the **storage-failure `SystemEvent`** (existing backlog
entry; the Phase 7 error banner consumes it).

### C. REPL (parted-style) — **viable; recommended after B**

High viability, moderate cost:

- Parse: `Cli::try_parse_from(shell-split(line))`; clap usage errors
  print inside the loop without exiting.
- Execution: extract the `with_core` bodies to `&App`-taking
  functions (mechanical; the seam already exists), boot once, loop.
- Line editing/history: `rustyline` (MIT; new workspace dep — cargo
  deny gate applies).
- **Open decision**: bare `taskboard` currently resolves `[app] mode`
  (daemon). Parted semantics = no-args → REPL. Since nothing ships
  pre-v0.1.0, now is the cheapest moment to change the default;
  recommended shape: no-args + TTY → REPL, no-args without TTY →
  daemon, explicit `daemon` subcommand always daemon. A REPL session
  holds the instance lock (document; stop the daemon first).
- Ordering rationale: a REPL is only as useful as its verb surface —
  after B it is a genuine daily driver; before B it is a demo.

### D. Daemon IPC — **recommended last of the four**

The backlog entry (2026-10-08) sketches it; the Phase 6 consumer
handoff map explicitly names the `--json` payload + error-kind
contract as the wire-format draft. Delivers: commands while the
daemon runs (kills the `instance-locked` friction), per-command
latency without core boot, `taskboard status` without triggering a
cycle, and the kiosk remote-management story. Design-heavy: UDS
(AF_UNIX, Win10+) vs Windows named pipes, length-framed versioned
JSON, CLI auto-detect with in-process fallback. Own plan + ADR.
Rides along naturally: the systemd unit template (existing backlog
entry) and the versioned-JSON-schema entry (its stated trigger is
exactly IPC consumers). Shares C's `&App` dispatch seam — do C's
refactor first and D's marginal cost drops.

### E. Coverage fill — **interleave, not a phase**

The two genuine gaps (§2): hermetic wiremock push tests for
stacks/labels (**before or with B** — B's verbs will drive those
server paths, and untested push paths under new verbs is the wrong
order), and the app dispatch-routing suite (cheap; can ride B's test
suite). Domain/state/storage need nothing beyond the documented
exceptions.

## 5. Hygiene items found (small, do early)

1. **Dispatch `nextcloud-integration.yml` on `main`** — prove the
   docker tier green post-libdbus-fix (§3). Minutes.
2. **Backlog duplicates**: five entries exist twice (appended 2026-10-10
   from the Phase 6 plan §7 and again later the same day): "Task
   selectors beyond exact uuid", "`tasks edit/delete`, `stacks`/
   `labels` subcommands", "`logout`", "`taskboard status` without
   triggering a cycle", "versioned `--json` schema". Dedup in the next
   docs commit; the REPL idea (C) is not yet in the backlog at all —
   append it when the direction is confirmed.
3. **Miri weekly job has never run green** (existing backlog entry:
   proptest interpreter cost vs the 120-min budget). Independent,
   CI-only, parallelizable on its own branch whenever convenient.
4. **MSRV CI job** (existing backlog entry): cheap, CI-only, same
   bucket as 3.

## 6. Strategic flag (not a pre-GUI build item)

**Reminders/notifications** are the project's stated identity
("task management **and reminder** system") with zero design so far;
the backlog schedules its own planning round "early post-M2". The
Phase 7 UI design (overdue banners on kiosk, notification surfaces on
desktop) will be shaped by it — recommend at least the **design
round** before the Phase 7 UI plan is finalized, so the UI does not
need reshaping after reminders land.

## 7. Recommended order (summary)

| # | Item | Why this position |
|---|---|---|
| 0 | Hygiene: docker-CI re-dispatch, backlog dedup (+ REPL entry) | minutes; unblocks trustworthy docker-tier signal |
| 1 | **A: stand-alone-first + connector port** | recorded user decision; everything builds on the new bootstrap; enables trivial hermetic verb tests |
| 2 | **Coverage fill: hermetic stack/label push tests** (± dispatch-routing suite) | de-risks B; the gap becomes load-bearing the day B ships |
| 3 | **B: full CLI editing surface** (+ XDG discovery, storage-failure event, `logout`, `config print`) | additive, daily value, all verbs exist in domain |
| 4 | **C: REPL** (parted-style; `&App` seam refactor + rustyline + no-arg default decision) | cheap after B; validates the seam D reuses |
| 5 | **D: daemon IPC** (versioned JSON protocol; `status`; kiosk management; systemd template) — *IMPLEMENTED 2026-10-11 as WI-5* (ADR 0012, `feat/app-daemon-ipc`): transparent auto-routing over `<db>.sock`, `status`, `daemon stop`, `contrib/taskboard.service` | biggest design piece; last of the four |
| 6 | Phase 7 (Slint UI) planning — with the reminders design round (§6) folded into its front end |

Mergers/trims that are defensible if capacity is tight: 2 folds into
3's plan (one PR); 4+5 could share one plan ("interactive and remote
frontends") at the cost of review size. 1 and 3 should not swap: A
first makes B's tests and handlers simpler, and B first would grow the
Nextcloud-coupled surface A then has to refactor.

## 8. Verification

- Coverage: `SQLX_OFFLINE=true cargo llvm-cov --workspace
  --summary-only` on clean `main`, 2026-10-11 (numbers in §2;
  the `actor_wiremock` watchlist flake did not reproduce).
- CI: `gh run list`/`gh run view 38074820350` for the failed
  docker-tier dispatch and its libdbus cause; current workflow file
  on `main` carries the fix.
- Code claims: file/line references in §1 verified against `main`
  this session.

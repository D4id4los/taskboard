# E2E Coverage Audit — WI-5 Daemon IPC Frontend

- **Date**: 2026-10-11
- **Trigger**: the user-mandated pass from plan
  `2026-10-11-m1-exit-wi5-daemon-ipc-frontend-plan.md` §6 step 7 — with
  the full binary-level surface in place (one-shots, REPL pty, daemon +
  IPC + fallback + `daemon stop`), walk the E2E tier against what the
  product promises and close the gaps.
- **Rule applied** (AGENTS §5): every gap is filled at the **lowest
  layer that can honestly test it**; binary rows assert process-level
  facts only (exit codes across process boundaries, stdout/stderr
  routing, real locks and sockets, signals, the IPC-vs-in-process
  byte-identity silence contract).

## 1. The matrix

Rows = user-facing journeys (plan §3.1 entry arms + §9 runbook steps).
Columns = the layer covering each. ✅ = covered, ⚠️ = gap (adjudicated
below), ➖ = deliberately not covered at this layer (reason given).

| # | Journey | Handler suites (L/V/K/O/ST/D/R) | IP-series | Binary E2E (E/K/J/pty) |
|---|---|---|---|---|
| 1 | `tasks/stacks/labels/boards core verbs` in-process | ✅ (V/K/O + D + ST) | ✅ IP1/IP2 | ✅ e1, e2 (fallback) |
| 2 | Core verbs **over IPC** (daemon serving) | ➖ transport-only (below) | ✅ IP1–IP4, IP8, IP11 | ✅ e1, e5 |
| 3 | Auto-routing silence (byte-identical stdout, code, streams) | ➖ | ✅ (structural: same code + parsed payloads) | ✅ e1 byte-identity row |
| 4 | Typed failures over IPC (`unknown-task`, envelopes) | ✅ D-series | ✅ IP2 | ➖ verb semantics (row 1 covers binary shape) |
| 5 | Clap surface over IPC (help code 0 / usage code 2) | ➖ | ✅ IP3 | ➖ unreachable: `Cli::parse` handles `--help`/usage **before routing** in the binary — the relay path only sees it via direct socket (IP3). Documented, not testable through the process CLI. |
| 6 | In-session refusals (`daemon`, `login`, `logout`, `config`, `--config`) | ✅ R-series | ✅ IP4 | ✅ (pty REPL refusals, pre-existing) |
| 7 | `--config` over IPC: same file honored, different file refused | ➖ | ✅ (IP4 refuses different-file; strip path exercised by binary e1, whose helper always passes `--config`) | ✅ e1 (implicit same-file honored) |
| 8 | `status` with daemon (live, pid, backend, phase; never a cycle) | ➖ | ✅ IP6 | ✅ e1 (pid == child pid) |
| 9 | `status` without daemon (`daemon: false`, `holder_pid`) | ✅ K6 (unit: holder read while locked) | ✅ IP10 (connect None) | ✅ e2, e1 (post-stop) |
| 10 | `status` never triggers a sync cycle (connector daemon) | ➖ | ✅ IP11 (polls until failed/auth, only status reads) | ➖ binary would need a live connector tier (J8 covers the sync path itself) |
| 11 | `daemon` (run): boots, serves socket | ✅ (bootstrap suites) | ✅ (spawn_server) | ✅ e1/e4/e5, pty `tty_explicit_daemon_stays_the_daemon_and_serves_standalone` |
| 12 | `daemon` under standalone backend (gate removed) | ➖ | ✅ IP1 (all IP rows are standalone) | ✅ pty standalone-serving row |
| 13 | `daemon stop`: ack → daemon exit 0 → socket removed | ➖ | ✅ IP7 | ✅ e1 |
| 14 | `daemon stop` without daemon (`no-daemon`, exit 1) | ➖ | ➖ (client-only path) | ✅ e3 |
| 15 | Second core on a live db → `instance-locked` | ✅ K1/K2/K5 | ➖ | ✅ K3 (second daemon, typed envelope) |
| 16 | REPL ↔ daemon mutex (REPL refuses while daemon holds) | ✅ R-series | ➖ | ✅ pty `tty_repl_refuses_while_the_daemon_holds_the_database` |
| 17 | Stale socket recovery (crashed daemon) | ➖ | ✅ IP9 (junk artifact) | ✅ e4 (`kill -9`) |
| 18 | Concurrent clients serialize, both answered | ➖ | ✅ IP8 (in-process) | ✅ e5 (two processes) |
| 19 | Connector daemon: sync fails auth, IPC still serves | ✅ (sync suites: failure classes) | ✅ IP11 (wiremock 401) | ➖ needs a real connector tier |
| 20 | Docker tier: `sync` over IPC end-to-end | ➖ | ➖ | ✅ J8 `it_nextcloud_docker_sync_over_ipc` (`#[ignore]`-gated, runbook step 12) |
| 21 | Signal shutdown (SIGTERM) while serving | ✅ (bootstrap shutdown) | ✅ (join/drain) | ✅ K3 (SIGTERM → release → successor serves) |
| 22 | Second-signal force exit during drain | ➖ (untestable deterministically without signal injection into a pty session) | ➖ | ➖ pre-existing gap, unchanged by WI-5 (signal-escalation backlog entry owns it) |
| 23 | REPL `status` line (in-session client-side shape) | ✅ R-series routing | ✅ (dispatch_line Status arm via D-table) | ➖ one more pty line in an existing journey; the arm's shape is unit-pinned |

## 2. Seed candidates from the plan §6 step 7 — adjudication

| Seed | Verdict |
|---|---|
| Byte-identity for `--help` and exit-2 usage lines (IPC vs in-process) | **Documented, not fillable at binary level**: the binary's clap handles help/usage before routing ever runs, so the relay path cannot observe them through the process CLI. The server-side rendering is pinned at IP3; the *relay* fidelity (the transport-relevant half) is pinned by the e1 byte-identity row on a real payload. |
| `status` while a REPL session holds the lock (binary `holder_pid`) | **Filled at a lower layer**: K6 proves the holder PID read while the lock is held; the binary composition (REPL boots + lock across processes) is already covered by the R-series and K3. A dedicated pty row would duplicate composition, not add an honest process fact. |
| Stale-socket recovery after `kill -9` | **Already filled**: e4 (binary, `kill -9` → restart → serves → graceful stop) on top of IP9 (junk artifact). |
| `daemon stop` with a request in flight (drain visibility) | **Deliberately left uncovered at binary level**: making a deterministic long-running request at process level requires hanging a real connector mid-request — not hermetic. The drain discipline (stop accepting → in-flight answered → artifact removed) is structurally exercised by IP7's join and the accept-loop drain; recorded here as accepted residual risk (same class as the signal-escalation backlog entry). |
| Concurrent CLI clients at binary level | **Filled**: e5 (two processes, both exit 0, both receipts present). |
| REPL error-line / `--json`-session pty journeys | **Out of WI-5 scope**: pre-existing R-series/pty territory (WI-4), unchanged by this work item; no new surface involved. |
| Docker-tier `sync` over IPC | **Filled**: J8, `#[ignore]`-gated like the J-series (runbook step 12's automated counterpart). |

## 3. Gaps filled by this audit (test ids)

- **K6** (`tests/lockfile.rs`): holder PID readable while locked —
  backs the `status` client-side answer.
- **e5** (`tests/cli_ipc.rs`): concurrent CLI processes over one
  daemon.
- **J8** (`tests/it_docker.rs`): binary daemon + `sync` over IPC
  against the dockerized tier (`#[ignore]`-gated).

## 4. Deliberately uncovered (with reasons)

- **Binary-level help/usage relay fidelity** — unreachable by
  construction (clap exits pre-routing); covered structurally at IP3 +
  the e1 byte-identity row. No backlog entry needed.
- **`daemon stop` with an in-flight request at binary level** —
  non-hermetic; drain covered structurally (IP7). Rides the same
  signal-escalation design round as request cancellation (plan §7
  backlog).
- **Second-signal force exit during IPC drain** — pre-existing,
  owned by the signal-escalation backlog entry, unchanged here.
- **Windows binary rows** — CI is Linux; Windows validation rides the
  release pipeline (plan §3 out-of-scope row).

## 5. Backlog appends spawned by the audit

None: every unfilled cell above either has an owner (signal-
escalation, Windows release pipeline) or is unreachable by
construction (documented in §4).

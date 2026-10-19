# [Plan] Kiosk Chunk 5 — process smoke harnesses: RSS + cold-start (TDD-first)

- **Date**: 2026-10-19
- **Kind**: stage-1 implementation plan (dispatch plan §6, chunk 5) for
  `feat/perf-process-harnesses` — the S1 RSS smoke harness and the S3
  cold-start stopwatch. TDD-first; no implementation in this commit.
- **Origin (normative, MD-7 rank)**: the wave implementation plan
  `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
  (§2 **KDF-7 rows 1+4**, §3 **K5**, §4, §5 — sequencing note (ii),
  §6); the audit report
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-perf-budgets-report.md`
  (§2 budgets table **rows 1 and 4**, §3 sketches **S1 and S3
  verbatim** — they are the design); the dispatch plan
  `.artifacts/plans/2026-10-19-kiosk-implementation-dispatch-plan.md`
  §1/§6 (**MD-5** tier honesty, MD-6: this path, MD-1: no round gate).
  Deviations from prose are recorded inline with reasons; none touches
  an ACCEPTED ruling.
- **Status**: READY — every open question settled below (§3 decisions
  D1–D12). Base `e0e3bf6` = `origin/main` (fetched 2026-10-19); local
  `main` (`ac77807`) is stale and untouched. **Round-state found**:
  chunks 1–4 of this round have merged (`1d51381` gui verb, `279d3b3`
  window profile, `e8257a6` appliance + ADR 0016, `fb10aa9` launcher +
  launch docs) — chunk 5 consumes chunk 1's `taskboard gui` verb as
  the S3 spawn surface (D6) and needs no rebase dance (nothing
  stacked).

## 1. What this chunk delivers

Two **process-level smoke harnesses** as `scripts/` tooling, plus the
one structural seam the cold-start harness needs:

1. **S1 — RSS smoke** (audit §3 S1, F13-01; budget row 1: *App RSS
   ≤ 200 MB*): spawn the real headless daemon against the N = 10 000
   bench-fixture database, wait for readiness and an RSS plateau by
   **polling predicates with deadlines** (never a fixed sleep), read
   `VmRSS` from `/proc/<pid>/status`, and report the steady-state and
   peak numbers against the 200 MB *reference*.
2. **S3 — cold-start stopwatch** (audit §3 S3, F13-04; budget row 4:
   *cold start → first useful frame ≤ 5 s*): spawn the real desktop
   binary (`taskboard gui`, chunk 1's verb) on the same fixture, time
   spawn → first-frame moment carried by a **structured tracing
   field** on a machine-readable log stream, and report the
   milliseconds against the 5 s *reference*.
3. The fixture generator both harnesses share: an example target that
   writes a file-backed N-task database using the **exact**
   `hot_path` bench fixture builder (D2).
4. A `docs/testing_strategy.org` runbook entry in the audit's P1-5
   (F14-01) pattern: prerequisite / invocation / honest caveats (D9).

**Non-goals** (chunk text): no criterion bench for either measurement
(S1's own note: a bench measuring its own process answers nothing);
no CI job and no workflow edits; **no budget assertion anywhere in CI
paths or exit codes** (references, never gates — MD-5); no
UI-process/appliance RSS variant (the spike extends the harness
on-device); no renderer/backend selection (the spike's); no rotation
of the fixture sizes (N = 10 000 is the budget row's scale; the
example takes `--tasks` for future needs).

### Seams re-located at plan time (base `e0e3bf6`)

| Symbol | Where (plan-day lines) | Role |
|---|---|---|
| Bench fixture builder | `crates/taskboard-storage-sqlite/benches/support/mod.rs` (`build_fixture`, `N_SIZES`, deterministic by construction) | the ONE builder both harnesses consume via `#[path]` include (D2) |
| The `#[path]` include precedent | `crates/taskboard-storage-sqlite/tests/bench_fixtures.rs:7-8` | how a non-bench target already reuses `benches/support/mod.rs` |
| `populate()` write-and-verify shape | `benches/hot_path.rs:45-70` | open → apply → load-count-verify; the example mirrors it |
| Daemon lifecycle | `crates/taskboard-app/src/main.rs` (`daemon_lifecycle`; bootstrap → `spawn_server` → select) | the S1 process; explicit `daemon` subcommand bypasses bare routing |
| `Command::Gui` lifecycle arm | `main.rs` (handled before IPC routing; `run_desktop`) | the S3 process (`taskboard gui --config …`) |
| IPC socket naming | `crates/taskboard-app/src/ipc/mod.rs:34-41` (`socket_sidecar`: `<db-stem>.sock` beside the db) | per-database socket follows the scratch db — no collision with any real instance |
| `status` IpcOnly + `StatusView` | `main.rs` `status_command`; `commands/status.rs:34-64` (serde shape; `daemon: bool` + `pid` discriminate) | the S1 readiness predicate — structural, existing seam |
| `emit_to` JSON emission | `commands/mod.rs:1592-1599` (one `serde_json::to_string` line on stdout) | `status --json` is single-line machine-readable |
| Standalone hermeticity | `bootstrap.rs:548-612` (no `[nextcloud]` section ⇒ `client = None`, no sync actor, no credential store consulted) | the S1/S3 daemon core does zero network I/O against the fixture |
| Config surface | `config.rs` (`[storage] db_path`, `[app] mode` kebab-case, `[sync] backend` auto→standalone, `deny_unknown_fields` sections) | the scratch config both harnesses write |
| Instance lock | `bootstrap.rs:523-542` (flock; `InstanceLock::acquire` before open) | OS-released on process death; scratch dir keeps runs isolated |
| Mount tail (the first-frame seam) | `crates/taskboard-ui-slint/src/lib.rs:133-175` (`mount` ends: "The first paint is a refresh, not a timer wait" → `handle.refresh()`) | where the moment is emitted (D5) |
| Slint 1.18.1 public frame API | `~/.cargo/registry/src/*/i-slint-core-1.18.1/api.rs:534-536` — only `Window::request_redraw`; **no public frame-presented callback** (grepped `frame_callback|first_frame|frame_rendered|rendering_*` over `slint-1.18.1` + `i-slint-core-1.18.1`) | evidence for D5's proxy framing |
| `init_tracing` | `main.rs:31-56` (fmt subscriber, stderr writer, `EnvFilter`; the `TASKBOARD_CONSOLE` runtime opt-in precedent at :38-42) | where the JSON log-format knob lands (D5) |
| Script conventions | `scripts/package_release.py` (`#!/usr/bin/env python3`, module docstring + Usage, argparse, builds release via cargo), `scripts/worktree-doctor.sh` | the harness script follows these |
| E2E child-process conventions | `docs/testing_strategy.org` §8 ("Config by file, never by env"; per-child `TASKBOARD_*` env hygiene) | the harnesses spawn children exactly so |
| Runbook entry pattern | `docs/testing_strategy.org` §8 "** Keyring round trip (local, manual only)" (Prerequisite / Invocation / honest-caveat bullets, `#+begin_src sh`); origin: audit-findings P1-5 = F14-01 | the §14 entry's template (D9) |

## 2. TDD order (red → green per step; house rules throughout)

The chunk's Rust surface is small and glue-adjacent by design; the
tests that matter pin the **two contracts the harnesses depend on**:

1. **First-frame contract test** (`taskboard-ui-slint`, the chunk's
   one new unit test): mount under a capturing subscriber and assert
   exactly one event on target `taskboard_perf` carrying the
   structured field `moment = "first-frame"`. Handwritten capture
   layer (a ~30-line `Layer` over `tracing_subscriber::Registry`
   collecting field maps — fakes over mocks; **dev-dep addition**:
   `tracing-subscriber.workspace = true` into
   `crates/taskboard-ui-slint [dev-dependencies]`, the workspace pin
   already carries `env-filter`+`fmt` so `Registry` is available).
   Asserts the typed field, never a message string. Red first: the
   emission does not exist yet.
2. **Fixture contract** (existing, unchanged): `tests/bench_fixtures.rs`
   stays the owner of builder determinism/shape/validity. The new
   example adds only write-to-file glue mirroring `populate()`
   (`open` → `apply` → `load()` count == N, loud panic otherwise) —
   main-glue class, deliberately untested; its correctness at the
   builder level is already pinned. The example's *file output* is
   verified every harness run by construction: the daemon hydrating
   N tasks is the acceptance (a wrong fixture fails readiness or
   produces a visibly wrong RSS number, never a silent pass).

Python-side: the repo has no python test lane (no pytest anywhere);
the harness scripts are verified by **execution evidence** — stage 2
runs both harnesses green and records the numbers (MD-5), stage 3
re-runs them and compares. The script carries argument validation and
`--help` instead of tests, per the `package_release.py` convention.

No sleeps anywhere in Rust test code (nothing here is time-dependent
in-process); the scripts poll with deadlines — the harness-side
sanctioned pattern (dispatch-plan trap: "a fixed sleep where the
readiness predicate belongs").

## 3. Decisions (every open question settled)

### D1 — One script, two subcommands

`scripts/process_smoke.py` (K5 allows "two harness scripts, or one
with two modes"). One responsibility — *process-shaped smoke
measurement* — with two measurement cores; the shared plumbing
(fixture ensure, scratch config, spawn, teardown, JSON summary) is
identical and would be duplicated across two files. Conventions:
`#!/usr/bin/env python3`, module docstring with a Usage block,
argparse subcommands `rss` and `cold-start`, **stdlib only** (Linux
`/proc` + `subprocess` + `tempfile` + `json`; no psutil, no deps).
Linux-only by design (`/proc/<pid>/status`); a non-Linux host fails
typed at startup (exit 4) — the dev platform is x86_64 Linux.
Exit codes: `0` measured-and-reported; `2` usage (argparse); `3`
measurement failure (readiness deadline, no RSS plateau, first-frame
timeout); `4` precondition failure (non-Linux, missing display,
missing binary that cannot be built, bad `--fixture` path).
`--help` documents every flag with its default.

### D2 — Fixture build/refresh story

- **Generator**: new example
  `crates/taskboard-storage-sqlite/examples/build-bench-fixture.rs`
  (SPDX `MIT OR Apache-2.0`, `#![forbid(unsafe_code)]` — examples are
  crate roots; the workspace lints already cover benches via
  `--all-targets`, examples join them automatically). It reuses the
  bench builder verbatim via the established mechanism:
  `#[path = "../benches/support/mod.rs"] mod support;` — the exact
  include `tests/bench_fixtures.rs:7-8` uses, so there is **one**
  fixture definition in the repo and the contract test keeps owning
  it. CLI: `--tasks <N>` (default `10000`) and `--out <PATH>`
  (required), hand-parsed (no clap in this crate). Mechanics mirror
  `hot_path.rs::populate`: current-thread tokio runtime, `open(out,
  ConnectorId::from("nextcloud"))`, `apply(fixture.actions)`,
  `load()` and assert the task count == N (loud panic — never
  silently measure an empty/short database), then drop the repo
  (clean close checkpoints WAL) and print the absolute path.
- **Fresh per run, no cache**: the script builds a fresh fixture into
  its per-run scratch directory (a `tempfile.mkdtemp`) — the apply
  costs ≈ 0.6 s at N = 10 000 and rebuilding removes the entire
  staleness question (no marker files, no schema-drift reuse). An
  `--fixture <path>` flag reuses an existing database file for
  iteration; it is a precondition failure (exit 4) if the path is
  missing. Non-goal: a shared on-disk fixture cache.
- **Build policy** (the `package_release.py` precedent — repo scripts
  may run cargo): the script resolves the `taskboard` binary at
  `target/release/taskboard` and the example at
  `target/release/examples/build-bench-fixture`, building whichever
  is missing (`cargo build --release -p taskboard-app` /
  `cargo build --release -p taskboard-storage-sqlite --example
  build-bench-fixture`), announcing each build loudly on stderr
  before it starts (a first run can take minutes — never a surprise).
  `--taskboard <path>` overrides the binary (no build attempted when
  the override is provided); `--no-build` turns absence into exit 4.

### D3 — S1 `rss` flow (exact)

1. Preconditions: Linux; binary + example resolved per D2.
2. Scratch dir (`--keep-dir` retains it for post-mortems; default
   removes it on success, keeps it on any failure).
3. Fixture: run the example → `<scratch>/taskboard.db` (N from
   `--tasks`, default 10 000).
4. Scratch config `<scratch>/taskboard.toml`, minimal:
   `[storage] db_path = "<scratch>/taskboard.db"` — nothing else
   (mode defaults `daemon`; no `[nextcloud]` ⇒ backend auto →
   standalone ⇒ no sync actor, no credential store, no network —
   verified seam `bootstrap.rs:548-612`). Child env: inherit, then
   **strip every `TASKBOARD_*` variable** and set
   `TASKBOARD_LOG_FORMAT=json` (D7) — the §8 per-child hygiene
   convention.
5. Spawn `taskboard --config <scratch>/taskboard.toml daemon`
   (explicit headless entry; the lifecycle arm, not bare routing).
   stdin = DEVNULL; child stdout/stderr captured to files under the
   scratch dir (printed in the failure report only).
6. **Readiness predicate** (deadline `--ready-deadline-secs`, default
   30; poll every 200 ms): run `taskboard --config <cfg> --json
   status`, parse stdout as one JSON object, require
   `daemon == true`. This is structural and reuses the IpcOnly
   surface — **not** the process exit code (`status` exits 0 even
   with no daemon; the `daemon` bool is the discriminator,
   `status.rs:4-8`). Cross-check the payload's `pid == <child pid>`
   and fail typed on mismatch (the per-db socket makes this
   deterministic). Deadline expiry = exit 3 with the captured daemon
   log tail printed.
7. **Soak to steady state** (deadline `--soak-deadline-secs`, default
   120; sample every 250 ms; minimum soak `--soak-min-secs`, default
   2): each sample reads `/proc/<child_pid>/status`, takes the
   `VmRSS:` line (kernel key:\tvalue-in-kB format — the one parse the
   script does on a *kernel* pseudo-file, documented), and records
   (t, kB). Steady = a sliding window of `--plateau-samples`
   (default 8) consecutive samples whose spread (max − min) is ≤
   `--plateau-band-kb` (default 4096 kB). Deadline without plateau →
   exit 3 with all samples printed (an incomplete measurement is a
   harness failure, not a budget verdict).
8. **Shutdown**: `taskboard --config <cfg> daemon stop` (graceful IPC
   shutdown — doubles as a smoke of the stop surface on a fresh
   instance), wait for child exit up to `--shutdown-grace-secs`
   (default 20 > the app's 10 s drain budget), then SIGKILL + exit 3
   (a daemon that will not die is a finding, never a hang).
9. **Output**: one JSON object (single line, stdout) + a human
   one-liner (stderr):

   ```json
   {"harness":"rss","n_tasks":10000,"binary":"…","fixture":"…",
    "ready_secs":0.42,"soak_secs":3.1,"samples":20,"steady":true,
    "vmrss_steady_kb":38210,"vmrss_peak_kb":41002,
    "budget_reference_mb":200,"within_reference":true,
    "scratch_dir":"…"}
   ```

   `vmrss_steady_kb` = median of the plateau window; `vmrss_peak_kb`
   = max observed since spawn (context). Machine keys are the
   contract; the human line is presentation.

### D4 — S3 `cold-start` flow (exact)

1. Preconditions: Linux; binary + example resolved per D2; **display
   gate**: `WAYLAND_DISPLAY` or `DISPLAY` set and non-empty in the
   harness environment (the `display_available` semantics: empty
   counts as absent) — else exit 4 with a typed message. Slint
   1.18.1 panics at backend init headless (KDF-2 evidence) — the
   gate is loud *before* spawn, and xvfb variants are out of scope
   (a GL-under-xvfb path changes the render stack; the spike owns
   on-device measurement).
2. Scratch dir + fixture + config, with one difference: the config
   carries `[app] mode = "desktop"` (inert for the `gui` verb, which
   is mode-blind, but it makes the scratch config self-describing),
   same env hygiene as D3.
3. Spawn **`taskboard gui --config <cfg>`** (chunk 1's verb — the
   documented desktop entry; boots core + window in-process; no
   daemon socket exists on this path, so teardown is signal-driven,
   step 6). `t0 = time.monotonic()` immediately before `Popen`.
4. Read the child's stderr live; parse each line as JSON
   (`TASKBOARD_LOG_FORMAT=json` is mandatory here — D5); select
   events whose **fields** carry `moment == "first-frame"`. Selection
   is on the structured field, never on message text.
5. `t1 = time.monotonic()` at the moment the selected line arrives:
   spawn→first-frame is measured **single-clock (monotonic,
   receipt-side)** — no wall-clock mixing, no timestamp-format
   parsing. Known bias: pipe+scheduling latency between the app's
   emission and the harness's receipt, ≤ 1 ms — noise against a
   5 s reference with 100 ms-scale legs; documented in the runbook.
   Deadline `--deadline-secs` (default 60) with no event → SIGKILL,
   print the captured stderr tail, exit 3.
6. **Teardown**: SIGTERM (the desktop exit watcher → `quit()` →
   graceful shutdown — the documented signal path), wait up to
   `--shutdown-grace-secs` (default 20), SIGKILL fallback (lock is
   flock-released on death; a stale socket sidecar is recovered by
   the next boot — no residue in the scratch dir either way).
7. **Output** (same contract as D3):

   ```json
   {"harness":"cold-start","n_tasks":10000,"binary":"…",
    "display":"wayland","first_frame_ms":812.4,
    "budget_reference_s":5,"within_reference":true,
    "scratch_dir":"…"}
   ```

   `display` records which session variable gated the run; the
   number is only comparable within the same backend — the runbook
   pins the caveat that `SLINT_BACKEND` passes through to the child
   and changes the measurement (default backend of the shipped
   build, no selection here — the spike's).

### D5 — The first-frame moment: a structured field on a JSON log stream

**What exists**: slint 1.18.1 exposes no public frame-presented
callback (verified: the only redraw surface is
`Window::request_redraw`, `i-slint-core-1.18.1/api.rs:534-536`). The
closest in-repo moment is `mount`'s own tail — "the first paint is a
refresh, not a timer wait" (`lib.rs:173-175`): after the initial
`handle.refresh()` the window's models hold the full first-frame
content; what remains unmeasured is `show()` → backend surface →
first presented frame (tens to a few hundred ms on a composited
desktop — flat against the 5 s budget, and the audit's S3 lists
"slint mount + first paint" as the leg it isolates).

**The change** (K5: "the mount seam emits the first-frame moment via
… a harness-visible log structured **field**"): one event at the
`mount` tail, after the initial refresh:

```rust
tracing::info!(target: "taskboard_perf", moment = "first-frame",
    "first-frame moment: initial refresh pushed models to the window");
```

Selected by the **field** (`moment`), never the message prose. The
appliance path (chunk 3) mounts through the same `mount`, so the
spike's on-device extension inherits the seam for free.

**Consumption** requires the log stream to be machine-readable: the
default `fmt` subscriber is human-pretty, and regexing it is exactly
the forbidden text-parsing. Therefore, two enabling edits:

1. **Workspace pin**: `tracing-subscriber` gains the `json` feature
   (`features = ["env-filter", "fmt", "json"]` in root
   `Cargo.toml:98`). Feature-only change to an approved crate; the
   underlying deps (`serde`, `serde_json`) are already in the graph;
   `cargo deny` verifies the tree unchanged in licenses (a recorded
   risk, §6).
2. **`init_tracing` knob** (`main.rs`): `TASKBOARD_LOG_FORMAT=json`
   selects `.json()` on the same fmt builder (same stderr writer,
   same `EnvFilter` default) — default output byte-identical when
   unset. Precedent: the `TASKBOARD_CONSOLE` runtime opt-in in the
   same function. Unrelated to the CLI `--json` payload flag (logs
   vs command payloads; the runbook says so).

**Alternatives rejected** (recorded reasons): an env-gated marker
file touched at the seam (a second, harness-only channel duplicating
observability; deviates from the plan-day "log structured field"
wording); an env-gated JSON line on stdout (changes the binary's
stdout contract — stdout is payloads/human output, not logs);
receipt-of-pretty-log regex (forbidden — parsed prose).

**Honesty label**: the moment is the mount-seam proxy for "first
rendered frame" — the event message and the runbook say exactly
that; the harness summary key stays `first_frame_ms` (the budget's
name) with the proxy semantics documented beside it, per the
tier-honesty rule (MD-5): references recorded as what they are.

### D6 — Reference-vs-gate framing (MD-5)

- `within_reference` (bool) and `budget_reference_mb` / `budget_reference_s`
  are **reported context only**; the exit code never encodes a budget
  comparison (D1's exit-code table). The budget rows are device-class
  (1.5 GHz in-order core, kiosk OS load); a dev-machine number is a
  smoke reference in either direction — the spike owns verdicts.
- No CI wiring of any kind: no workflow edit, no nextest
  participation, no bench target (the scripts and the example are
  invisible to the test lanes; the example compiles under
  `--all-targets` clippy/fmt only).
- The recorded numbers live in the chunk's stage report + PR body
  (MD-5), never asserted by a test.

### D7 — Child-process hygiene (the §8 conventions, applied to scripts)

Config by file, never by env: every child gets
`--config <scratch>/taskboard.toml`. Per-child env: the harness
strips all `TASKBOARD_*` variables from the inherited environment
before spawning (a developer's `TASKBOARD_APP_PASSWORD` or
`TASKBOARD_CONFIG` must not leak into a measurement), then sets
exactly `TASKBOARD_LOG_FORMAT=json`. `SLINT_BACKEND` and
`SLINT_SCALE_FACTOR` are *deliberately not stripped* — they are the
user's documented display escape hatches and part of the measured
configuration (the runbook notes they move the number).

### D8 — Defaults catalogue (single source for `--help` and the runbook)

| Flag | Default | Notes |
|---|---|---|
| `--tasks` | `10000` | the budget rows' scale (`N_SIZES[2]`) |
| `--fixture` | none | reuse path; missing file = exit 4 |
| `--taskboard` | `target/release/taskboard` | override skips builds |
| `--no-build` | off | absence becomes exit 4 |
| `--ready-deadline-secs` | `30` | S1 only |
| `--soak-deadline-secs` | `120` | S1 only |
| `--soak-min-secs` | `2` | S1 only |
| `--plateau-samples` | `8` | S1 only |
| `--plateau-band-kb` | `4096` | S1 only |
| `--deadline-secs` | `60` | S3 only (first-frame timeout) |
| `--shutdown-grace-secs` | `20` | both (> app's 10 s drain) |
| `--keep-dir` | off | both (scratch retained) |

Poll cadences: readiness 200 ms; RSS sampling 250 ms (fixed, bounded
by the deadlines — polling, not blind sleeping).

### D9 — Runbook entry (text settled; lands in `docs/testing_strategy.org`)

New top-level section appended after §13 (no renumbering):

```org
* 14. Performance Smoke Harnesses (local, manual only)

Two scripts measure the *process-shaped* budget rows on the dev
machine (audit theme perf-budgets, sketches S1/S3; wave plan KDF-7
rows 1+4). They are smoke references, never gates: nothing in CI runs
them, no test asserts their numbers, and the exit code never encodes
a budget comparison — the kiosk bring-up spike owns the device-class
verdicts. Both build a fresh deterministic fixture (the hot-path
bench builder at N = 10 000) into a scratch directory and drive the
real release binary against it; a first run may spend minutes in
=cargo build --release=.

** RSS steady state (S1 — budget row 1: App RSS ≤ 200 MB)

- *Prerequisite:* Linux (reads =/proc/<pid>/status=); the release
  binary (built on demand). No display needed.
- *Invocation:*

#+begin_src sh
scripts/process_smoke.py rss            # --help for every flag
#+end_src

- *What it does:* spawns =taskboard daemon= standalone (no network —
  no =[nextcloud]= config) on the fixture DB, polls =taskboard status
  --json= until =daemon = true= (deadline-bounded; never a fixed
  sleep), soaks /proc VmRSS until a bounded plateau, reports
  steady/peak kB against the 200 MB reference, then shuts the daemon
  down over IPC (=daemon stop=).
- *Reading the number:* dev-machine reference under a desktop's load
  profile — direction only for the 1 GB kiosk device; record it in
  the PR/report, never in an assertion.

** Cold start to first frame (S3 — budget row 4: ≤ 5 s)

- *Prerequisite:* a graphical session — =WAYLAND_DISPLAY= or
  =DISPLAY= set non-empty (empty counts as absent); Linux; the
  release binary. Headless runs fail typed before spawning (slint
  1.18.1 panics at backend init without a display).
- *Invocation:*

#+begin_src sh
scripts/process_smoke.py cold-start     # --help for every flag
#+end_src

- *What it does:* spawns =taskboard gui= on the fixture DB and times
  spawn → the =moment = "first-frame"= structured tracing event
  (=TASKBOARD_LOG_FORMAT=json= is set for the child; the field, not
  the message text, is selected). The moment is the mount seam's
  first refresh (models ready) — the show→presented delta has no
  public slint 1.18.1 API and is unmeasured; receipt-side timing
  adds ≤ 1 ms bias.
- *Reading the number:* valid only within one backend —
  =SLINT_BACKEND= / =SLINT_SCALE_FACTOR= pass through to the child
  and move the number; the default backend of the shipped build is
  what the reference means. Device verdicts belong to the spike.

Both harnesses are honest about failure: a readiness deadline, a
non-plateauing soak, or a missing first-frame event is a nonzero
exit with the captured child logs printed — never a silent skip.
```

### D10 — File-by-file change sheet

| File | Change | Size |
|---|---|---|
| `scripts/process_smoke.py` | new (D1, D3, D4, D7, D8) | ~360 lines |
| `crates/taskboard-storage-sqlite/examples/build-bench-fixture.rs` | new (D2) | ~80 lines |
| `crates/taskboard-ui-slint/src/lib.rs` | the one event at the mount tail (D5) | +4 lines |
| `crates/taskboard-ui-slint/src/lib.rs` (tests) | first-frame capture test (§2 step 1) | +55 lines |
| `crates/taskboard-ui-slint/Cargo.toml` | dev-dep `tracing-subscriber.workspace = true` | +2 lines |
| `Cargo.toml` (root) | `json` feature on the `tracing-subscriber` pin (D5) | 1 line |
| `crates/taskboard-app/src/main.rs` | `TASKBOARD_LOG_FORMAT` branch in `init_tracing` (D5) | +10 lines |
| `docs/testing_strategy.org` | §14 per D9 | +55 lines |
| `CHANGELOG.md` | Unreleased `Added` lines: the two harness scripts + the log-format knob | +4 lines |

Total ≈ 570 lines, no production-path logic beyond the one event and
the subscriber branch.

### D11 — Quality-gate notes (AGENTS §10 as applied here)

Full gate suite green at stage 2/3. Specifics: the example and the
new test compile under `clippy --workspace --all-targets -D warnings`
(examples are targets; `expect`-style glue is the bench precedent);
`cargo fmt --all` covers the example; `cargo deny` re-walks the graph
with the `json` feature (recorded risk §6); the coverage sequence is
unaffected (no new measured paths — the emission is one disabled-noop
event outside tests; the example is not exercised by nextest).
`SQLX_OFFLINE=true` for any cargo invocation, as usual.

### D12 — Bookkeeping

CHANGELOG Unreleased lines (D10). No tracker edits by stage 2 (MD-4);
stage 3 opens the PR with `Closes #299` + `Closes #262` and carries
both harnesses' measured numbers in the body (dispatch §6).

## 4. Implementation order (stage 2)

1. The example + `#[path]` include; run it by hand; verify the db
   hydrates through a `taskboard daemon` boot (manual, scratch dir).
2. The `json` feature + `init_tracing` knob; verify default output
   unchanged, `TASKBOARD_LOG_FORMAT=json` emits JSON lines.
3. The first-frame event red→green (capture-layer test first, then
   the emission).
4. `scripts/process_smoke.py`: `rss` first (it exercises fixture +
   spawn + readiness + teardown), then `cold-start` (reuses all of
   it, adds the stderr pump + field selection).
5. Run both; record numbers (report + PR body material).
6. Runbook entry + CHANGELOG; full gate suite.

## 5. Risks

1. **`json` feature widens the dependency graph** (serde/serde_json
   already present; `matchers`/`regex` likely via env-filter) —
   `cargo deny` at stage 2 is the verifier; a licensing surprise is
   implausible but would send the knob back to re-design (fallback:
   the marker-file alternative in D5, with its recorded reason).
2. **The mount-seam proxy under-counts** cold start by the
   show→presented delta — bounded and flat on desktop compositors;
   honestly labeled (D5); the spike's on-device stopwatch can compare.
3. **S3 needs a real display** — headless dev boxes get a typed
   precondition failure (never silent); xvfb deliberately out
   (renderer-path variance).
4. **Readiness couples to the `StatusView` serde shape** — the shape
   is contract-pinned in `status.rs`'s own tests (`daemon` bool is
   the documented discriminator), so drift is loudly tested today.
5. **Fixture duplication drift** — structurally impossible beyond the
   10-line write-glue: the builder itself is included by `#[path]`
   from the one definition the contract test owns.
6. **First runs spend minutes in release builds** — announced loudly
   (D2); `--no-build`/`--taskboard` give operators control.

## 6. Acceptance checklist (stage 2/3)

- [ ] Both harnesses run green on the dev machine; numbers recorded
      in the stage report + PR body (MD-5).
- [ ] The fixture builds fresh per run; `--fixture` reuse path typed.
- [ ] No fixed sleep anywhere: readiness/soak/moment are
      deadline-bounded polls.
- [ ] The first-frame moment is a structured field on a JSON log
      stream; no prose parsing anywhere in the script.
- [ ] No budget comparison affects any exit code; no CI/workflow diff.
- [ ] Runbook §14 landed per D9; CHANGELOG lines per D10.
- [ ] Full AGENTS §10 gate suite green.

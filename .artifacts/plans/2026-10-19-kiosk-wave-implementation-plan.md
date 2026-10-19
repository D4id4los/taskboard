# [Plan] Kiosk Wave — Implementation (appliance + instrumentation + kiosk surfaces)

- **Date**: 2026-10-19
- **Kind**: implementation plan for the `wave-kiosk` milestone,
  following the framing plan
  `.artifacts/plans/2026-10-19-kiosk-wave-design-plan.md` (same PR;
  its §4 decision record is PROPOSED — chunks marked below depend on
  specific rulings). It **supersedes
  `.artifacts/plans/2026-10-16-kiosk-mode-wave1-plan.md` as the
  implementation-of-record for the accepted Option-A design** (that
  doc and `.artifacts/plans/2026-10-16-kiosk-daemon-gui-control-plan.md`
  §6 remain the design-of-record; the supersession is a re-anchoring
  on post-`3ad16bb` facts plus the extension of the wave past wave-1
  to the whole milestone roster — framing §3). Chunked for one branch
  + PR per chunk; chunks are written so they can be implemented
  directly by an agent or promoted into a dispatch plan without
  restructuring. Each chunk names its normative origin sections;
  **deviation from an ACCEPTED ruling (the 2026-10-16 §6 record, or
  a framing §4 ruling once made) goes back to the user, never
  silently adapted.**
- **Status**: phase A (K1–K10) **READY** — no design dependency
  stands between these chunks and dispatch except the framing §4
  rulings that add/assign their issues; they do **not** wait for the
  multi-board round (verified disjointness: framing §3, parallelism
  report). Phase C (K11–K13) **GATED**: K11 on the K10 design
  round's acceptance (+ stack approval), K12 on the D-K8 key-
  management ruling, K13 on **daily-driver chunk 10** (`feat/
  ui-today-inbox` — `ViewId::Today` + the rotation-ready today view;
  a dependency and a file-set collision both). K14 last. The on-
  device spike (phase B) is user-executed and gates the milestone's
  closure, not the code chunks.

## 1. Scope

**In** (framing §2; the milestone roster + proposed additions):

- **The appliance** (#237): `taskboard gui` verb, `mode = "kiosk"`
  appliance lifecycle (TTY-independent bare routing, display-gated
  mount, loud headless fallback, close-exits-appliance, shared
  daemon tail), kiosk window profile, `.desktop` + launch docs,
  ADR 0016 + ADR 0009 amendment.
- **Audit instrumentation** (#299, #300, #262, #263): the S1 RSS
  smoke harness, S2 refresh+draw bench, S3 cold-start stopwatch,
  S4 DTO↔domain mapping bench — dev-machine numbers that feed the
  spike.
- **Metrics** (#156, +#145 if D-K3): state-borne per-board sync
  counters + handle-side engine counters.
- **Deployment profile** (#233 agent-side, #160): `[kiosk]` config
  section with the fixed-board pin (the multi-board design's
  deferred item), the kiosk systemd unit, the bring-up runbook; the
  `sd_notify` decision.
- **Web dashboard** (#234): design round (K10, docs-only) +
  implementation (K11).
- **Encrypted-file credential store** (#126 remainder = #159).
- **Ambient view + display rotation** (new issue per D-K2).
- **Wave exit docs** + the Q6 default-mode input (K14).

**Out** (framing §2.3): Option B attachable GUI + any kiosk-
management IPC verb (the F16-28 fence stays unfired; F16-27's cost
table stays parked); Android native; aarch64 legs; brightness/DPMS;
fleet management; watchdog beyond systemd; #194 IPC knob; the E-12
family; every other wave's items.

## 2. Design facts every chunk relies on

File paths + symbols are the stable keys — line numbers are
plan-day references (@ `5480869`); every implementing agent
re-locates symbols at its own start. Facts cite their ruling or
evidence source.

1. **KDF-1 — routing and the gate after `3ad16bb`** (PR #332; ADR
   0015 #236 amendment). `bare_invocation_route(mode, tty, display)`
   (`crates/taskboard-app/src/ui.rs:112-121`): `desktop` →
   `DesktopUi` (TTY-independent); `daemon`+TTY → `Repl`;
   `daemon`+display → `DesktopUi`; `kiosk` (and daemon without
   display) → `DaemonLifecycle`. The daemon mode gate
   (`main.rs:256-261`) refuses `Desktop | Kiosk` typed. The wave
   flips exactly: kiosk → a new `BareRoute::KioskAppliance`
   (**TTY-independent** — a seat has no operator TTY; `ssh -t` bare
   kiosk booting the appliance is acceptable; the explicit `daemon`
   subcommand stays the headless maintenance entry), and the gate
   refuses `Desktop` only. The 2026-10-16 wave-1 plan's routing
   table (its §2.1) is superseded by this fact.
2. **KDF-2 — the display gate exists** (`ui.rs:82-84`):
   `display_available(wayland, x11)` is pure (empty string counts as
   absent) with the env wrapper beside it — its doc comment already
   names the kiosk wave as consumer. Verified behaviour (2026-10-16,
   release artifact): with no display vars slint 1.18.1 **panics**
   during backend init (exit 101) instead of surfacing the typed
   mount error — hence gate-before-mount. A display var that
   outlives its server fails **loud and typed at mount** (the
   `3ad16bb` behaviour); the gate→mount race stays tolerated
   (ADR 0016 documents it; catch-unwind hardening remains the
   review-time option the 2026-10-16 plan recorded).
3. **KDF-3 — the shared daemon tail** (F16-26; F9-01's exit-path
   work landed 2026-10-18 — re-locate, do not assume the plan-day
   shape). `daemon_lifecycle` (`main.rs:248-340`): bootstrap →
   `spawn_system_event_logger` → `ipc::server::spawn_server` →
   select(signal, `server.stopped()`) → `server.join(shutdown_
   timeout)` bounded drain (second-signal force exit) →
   `app.shutdown()`. The appliance's headless fallback calls the
   **same extracted helper** — no duplicated shutdown
   choreography; the extraction (with its `Arc::try_unwrap`
   typed-path expectations) is K3's first move.
4. **KDF-4 — close-exits-appliance and the watcher** (Q2 ruling).
   `run_desktop` (`ui.rs:191-244`) returns when the slint loop ends
   (window close); SIGTERM converges via the existing watcher →
   `taskboard_ui_slint::quit()` pattern. The appliance watcher
   selects **three** exits: signal, `server.stopped()` (`daemon
   stop` over IPC), and window close — all land in the same drain +
   shutdown tail. The instance lock is unchanged: one core per
   database; a second appliance/GUI/daemon gets typed
   `instance-locked`; CLI clients never contend.
5. **KDF-5 — the mount seam and the window profile** (Q5 ruling;
   ADR 0015). `mount(state, signals, system, commands, UiSettings)`
   (`crates/taskboard-ui-slint/src/lib.rs:128`); `[ui] scale` flows
   through `Theme.base-scale` at mount (`lib.rs:141-144`). `UiSettings`
   gains `kiosk: bool` (default `false`); the profile = fullscreen +
   suppressed decorations (verify the 1.18.1 API —
   `Window::set_fullscreen` + decoration control; if decorations
   cannot be suppressed programmatically, fullscreen alone is
   acceptable wave-1, noted in ADR 0016). No new config key — the
   profile derives from the mode, and `[ui]` stays
   `deny_unknown_fields` with scale + keymap only.
6. **KDF-6 — the IPC surface needs nothing** (ADR 0012 amendment
   convention; F16-28). Protocol v1 verbs: `Exec` (argv relay, the
   server is the single clap authority), `Status`, `Shutdown`,
   `DetachConnector`, `AttachConnector` (+ `VersionMismatch`
   response). The appliance serves the socket exactly as the daemon
   does; `daemon stop` and `status` are the operator surface over
   ssh. **No kiosk-management verb lands in this wave** — the F8-01
   extraction fence does not fire; `StatusView`'s coupling resolves
   when a second wire consumer appears (F16-27/F16-28 stay parked).
7. **KDF-7 — the perf instrumentation targets** (perf-budgets
   report `§budgets` rows + S1–S4; decision session P1-6/P1-7
   bundled them into this wave). Row 1: RSS ≤ 200 MB (S1: headless
   daemon `VmRSS` smoke, N=10 000 fixture, `procfs` read of
   `/proc/<pid>/status`; **not** a criterion bench — a bench
   measuring its own process answers nothing). Row 3: full-frame
   repaint ≤ 50 ms / partial ≤ 20 ms @ 1366×768 software renderer
   (S2: extend the `test_platform.rs:18-28` seam — `NullWindowAdapter`
   holds a real `Window` + a `SoftwareRenderer` that is constructed
   but "never asked to draw"; the extension makes it draw headlessly
   — **API-blocker risk**: if 1.18.1 cannot, that outcome is
   recorded as evidence and the refresh leg (`ArcSwap` load →
   `view_state.reconcile` → `models::push` VecModel rebuild,
   `adapter.rs:188-196`, `models.rs:101`) benches alone). Row 4:
   cold start ≤ 5 s (S3: scripted spawn-to-first-rendered-frame
   stopwatch on the N=10 000 fixture DB; the storage leg is already
   bench-covered — `hot_path` `load/10000` = 129 ms dev-machine).
   Row 5: sync-cycle CPU legs (S4: criterion over
   `mapping.rs`/`push.rs` pure fns at board scale; the 30 s cycle
   wall is network-dominated — only the CPU share is in-repo
   measurable). The 100 ms repaint cadence (`adapter.rs:35`) is the
   composed-latency floor on any hardware (C2/F13-09) — recorded,
   not fixed in this wave; idle-CPU measurement is spike-side
   (F13-02).
8. **KDF-8 — metrics ride existing state machinery** (D-K6
   recommendation; #156's own proposal list). Sync status is
   state-borne since multi-board chunk 1: `AppState` carries
   per-board `SyncStates` with an `aggregate()` fold
   (`taskboard-domain/src/state.rs`) that keeps every existing
   surface (CLI `SyncStatusView` `output.rs`, UI badge
   `presentation.rs`) shape-stable. Counters (cycles, pushes,
   pulls, failures, last-cycle duration, offline episodes) extend
   the per-board status **through the existing `SyncReport`
   ingestion publish points** — no new channel, no new wake source;
   dashboards and the badge read lock-free via `ArcSwap`. Engine
   counters (#145) are a **handle-side atomic struct** (never in
   `AppState` — no snapshot churn, no insta pass); the handle
   already exists for command dispatch.
9. **KDF-9 — the credential port** (#126/#159). `CredentialStore`
   (`crates/taskboard-app/src/secrets.rs`) has `KeyringStore`
   (default; service `taskboard`, account `{username}@{host}`) and
   read-only `EnvCredentialStore` (`TASKBOARD_APP_PASSWORD`);
   secrets wrap in the redacting `AppPassword`; headless without a
   Secret Service is a typed `CredentialError::Backend` naming the
   env alternative — never a silent fallback. The file store is a
   **third impl behind the unmoved port**, selected by
   `[nextcloud] credential_store = "file"`; key management per
   D-K8 (generated 0600 key file default; passphrase-derived
   variant needs argon2 + stack approval). Unattended-kiosk
   standalone installs never touch any store.
10. **KDF-10 — the deployment surface** (#233 prep, #160;
    multi-board design deferral "config-pinned fixed-board kiosk").
    Today: `contrib/taskboard.service` is a **user** unit
    (`ExecStart=… taskboard daemon`, `Restart=on-failure`,
    `TimeoutStopSec=30`); ADR 0015's #236 amendment requires units
    to keep the explicit `daemon` verb (a unit importing a graphical
    environment would flip the GUI under the post-`3ad16bb`
    routing). The kiosk unit is **system-level**: config file with
    `mode = "kiosk"` (never an env-var launch), `Restart=always`,
    the seat's display vars imported, `WantedBy=graphical.target`.
    A new `[kiosk]` config section (typed, `deny_unknown_fields`)
    starts with `fixed_board: Option<Uuid>` (resolved typed against
    live boards at boot; absent pin + N live boards = the ambient
    view's rotation scope, absent pin + 1 board = that board) and
    grows rotation keys only in K13. `sd_notify` (#160): decide at
    the spike; default not-needed under `Type=simple` +
    `Restart=always`. The renderer path for the minimal image is
    config/env first (`SLINT_BACKEND`/software renderer selection),
    build-feature second (`renderer-software`/`backend-fb` features
    exist upstream, not compiled in — ADR 0015 deferral; the spike
    rules).
11. **KDF-11 — the passive-display primitives that exist** (for
    K13). Three views today (`ViewId`: Board/StackDetail/TaskDetail,
    `view_state.rs:24`); the overdue-reminder banner already renders
    rows with snooze/mute (`presentation.rs` `banner_rows`); the
    reminders domain's `overdue_tasks`/`due_soon` helpers are pure;
    the sync badge carries an `offline` phase tone. Daily-driver
    chunk 10 adds `ViewId::Today` (four sections, ids-only, one
    definition) explicitly **rotation-ready** (D-V1) and rules the
    kiosk degeneration **board-pinned, compact, passive** (D-V3) —
    K13 consumes, never re-derives. Rotation is a display-mode state
    machine (future-directions §3.7: kanban ⇄ upcoming-list) driven
    by a timer on the UI side; `testing_strategy.org` already names
    kiosk rotation as a time-dependent surface (pause/advance, no
    sleeps).
12. **KDF-12 — the dashboard's lane** (#234; backlog ruling:
    browser dashboard before native Android). An opt-in HTTP
    surface on the daemon lifecycle: loopback bind by default, any
    wider bind gated on the auth ruling (K10's central question);
    deliberately boring CSS for the 2014-era Android browser with a
    hands-on test flag; content = read-only board view + sync health
    (KDF-8 metrics) + offline state; meta-refresh polling, no-JS
    fallback. Stack approval (AGENTS §4) is a hard gate — no HTTP
    crate enters the tree before the K10 ruling. The server is an
    app-layer task reading `ArcSwap` state exactly like the UI
    (another dumb presentation client — ADR 0015's rule holds over
    HTTP).

## 3. Chunks

Sizes on the house scale; branch names per AGENTS §8. Issue
actions in §6.

### K1 — `taskboard gui` verb (S; branch `feat/app-gui-verb`; #237)

- **Origin**: 2026-10-16 wave-1 plan chunk 1 (Q1/Q4 rulings);
  `3ad16bb`'s message defers the verb to this wave; KDF-1.
- **Files**: `crates/taskboard-app/src/cli.rs` (`Command::Gui` — doc:
  explicit desktop-frontend selection; boots core + desktop window
  in-process regardless of `[app] mode`; a running instance fails
  typed `instance-locked`); `main.rs::run` (handle `Gui` **before**
  `route_for_ipc` — a lifecycle arm, never IPC-relayed;
  `route_for_ipc` stays gui-blind); `completion.rs`; ADR 0009
  amendment section (the verb + never-relayed rule); README +
  CHANGELOG.
- **Behaviours**: clap parse + routing unit tests (gui arm chosen
  regardless of TTY and mode; no IPC lane); `run_desktop` stays thin
  glue per the main-glue convention.
- **Non-goals**: no kiosk routing (K3), no window profile (K2).

### K2 — kiosk window profile flag (S; branch `feat/ui-kiosk-profile`; #237)

- **Origin**: 2026-10-16 wave-1 plan chunk 3 (Q5); KDF-5.
- **Files**: `crates/taskboard-ui-slint` — `UiSettings.kiosk`
  (default `false`); the mount-time fullscreen + no-decorations
  profile; the harness path asserts the window property both ways
  (ADR 0015 harness conventions).
- **Behaviours**: flag defaults off — the desktop path byte-identical;
  token/theme lint gates unchanged.
- **Non-goals**: no consumer yet (K3 passes the flag), no scale knob,
  no config key.

### K3 — kiosk appliance lifecycle + ADR 0016 (M; branch `feat/app-kiosk-lifecycle`; stacks on K1+K2; #237)

- **Origin**: 2026-10-16 wave-1 plan chunk 2 (Q2/Q3 rulings), as
  re-anchored by KDF-1/2/3/4; F16-26's seam map.
- **Files**: `crates/taskboard-app/src/ui.rs` (`BareRoute::
  KioskAppliance` variant, TTY-independent; the existing
  kiosk→DaemonLifecycle test updated); `main.rs` (extract the shared
  serve-drain-shutdown tail from `daemon_lifecycle`; new
  `run_kiosk(config, config_path, json)`: bootstrap → spawn server →
  if `display_env_available()`: ADR 0015 hand-over with
  `UiSettings.kiosk = true`, forwarder, the three-exit watcher
  (signal ∥ `server.stopped()` ∥ window-close → `quit()`), event
  loop on the main thread, then abort watcher/forwarder → drain →
  shutdown; else: `tracing::error!("no display available; continuing
  as headless daemon")` + the shared tail; the mode gate refuses
  `Desktop` only); **ADR 0016** "Kiosk appliance mode" (routing,
  gate + loud fallback with the panic evidence and the tolerated
  race, close-exits, IPC on the seat, `daemon` stays headless;
  decorations caveat if applicable); `docs/architecture.org` mode
  row + flow-model matrix row.
- **Behaviours**: route mapping (kiosk → appliance, both TTY values);
  the fallback asserted at the decision layer (`KioskStart::
  Headless`-shaped seam); the shared tail keeps the daemon path's
  existing tests green with zero duplication.
- **Non-goals**: no IPC verb changes (KDF-6), no hardening beyond
  the documented race tolerance.

### K4 — packaging & launch docs (S; branch `feat/packaging-desktop-file`; stacks on K1; #237)

- **Origin**: 2026-10-16 wave-1 plan chunk 4; the design doc §5.
- **Files**: `contrib/taskboard.desktop` (`Exec=taskboard gui`,
  `Type=Application`, categories; icon references `taskboard` with
  theme fallback — shipped SVG icon stays a follow-up nit);
  `scripts/package_release.py` (stage the file; `--check` follows
  the staged list); `README.org` "Launching" section (the
  post-`3ad16bb` semantics table: TTY → REPL, no-TTY+display → GUI,
  no-TTY headless → daemon, `mode = "kiosk"` → appliance, `gui` →
  desktop window; the env escape hatch; the `.desktop` install
  hint); CHANGELOG.
- **Behaviours**: the archive double-click story is complete (the
  #236 routing fix + the verb + the launcher).
- **Non-goals**: no kiosk unit (K9), no icon asset.

### K5 — process smoke harnesses: RSS + cold-start (S–M; branch `feat/perf-process-harnesses`; #299 + #262)

- **Origin**: perf-budgets report S1 (F13-01) + S3 (F13-04);
  KDF-7 rows 1 and 4.
- **Files**: `scripts/` (two harness scripts, or one with two modes,
  following the repo's bash/python conventions): S1 — spawn the
  headless daemon against the N=10 000 fixture DB, wait for
  steady-state (poll a readiness predicate with a deadline, never a
  fixed sleep), read `VmRSS` from `/proc/<pid>/status`, report
  against ≤ 200 MB; S3 — spawn-to-first-rendered-frame stopwatch
  (the mount seam emits the first-frame moment via an existing
  signal or a harness-visible log structured **field**, never a
  parsed text line; pass ≤ 5 s recorded as the dev-machine
  reference); fixture reuse from the `hot_path` bench conventions;
  `docs/testing_strategy.org` runbook entry (the audit's P1-5
  pattern).
- **Behaviours**: both harnesses run green on the dev machine and
  print their numbers; they are **not** CI gates (device-class
  budgets, dev-machine references — the spike owns the verdicts);
  documented as such.
- **Non-goals**: no criterion bench (KDF-7's S1 note), no CI job, no
  UI-process RSS variant (the appliance exists after K3 — the
  spike extends the harness to it on-device).

### K6 — refresh+draw bench (M; branch `feat/perf-refresh-draw-bench`; #300)

- **Origin**: perf-budgets report S2 (F13-03); KDF-7 row 3.
- **Files**: `crates/taskboard-ui-slint` — extend
  `test_platform.rs` (`NullPlatform`/`NullWindowAdapter`) so the
  constructed `SoftwareRenderer` actually renders a frame against
  the mounted component headlessly; a criterion bench (or the
  seam-supported equivalent the crate's dev-deps allow — decide at
  stage 1) driving refresh (the `ArcSwap`-load → `reconcile` →
  `models::push` path) **and** the draw, separately timed, at
  board-scale fixture N.
- **Behaviours**: refresh leg and draw leg timed independently
  (row 3 full/partial expectations stated as budget context; the
  **API-blocker rule**: if slint 1.18.1 cannot draw headlessly, the
  chunk records the evidence and ships the refresh-leg bench plus
  the seam extension — that is a complete outcome, not a failure
  (F13-03's own wording); the on-device draw verdict belongs to the
  spike.
- **Non-goals**: no renderer/backend selection (the spike), no
  cadence change (F13-09 stands), no visual-correctness assertions
  (the `golden_visuals` lane owns those).

### K7 — DTO↔domain mapping bench (S; branch `feat/perf-mapping-bench`; #263)

- **Origin**: perf-budgets report S4 (F13-05); KDF-7 row 5.
- **Files**: a criterion bench in `taskboard-sync-nextcloud`
  (`benches/`) over the pure mapping surface (`mapping.rs` DTO↔domain
  arms, `push.rs` `plan_pushes` + card-shape fns) at board scale N
  using the `hot_path` fixture conventions; the allowance-vs-30 s
  budget derivation recorded in the bench's docs once numbers
  exist.
- **Behaviours**: CPU legs measured; numbers land in the bench
  output (Bencher-tracked like `hot_path` — CI's existing
  `cargo bench --workspace` job picks it up with zero workflow
  edits).
- **Non-goals**: no wiremock full-cycle harness (network-dominated,
  stays labeled), no mapping code changes.

### K8 — sync metrics on `SyncStatus` + engine counters (S–M; branch `feat/state-sync-metrics`; #156, #145 if D-K3)

- **Origin**: D-K6 recommendation; KDF-8.
- **Files**: domain `state.rs` (per-board status fields: cumulative
  cycles/pushes/pulls/failures, last-cycle duration, offline
  episodes — additive, epoch/zero defaults); state `actor.rs`/
  `core.rs` (populate at the existing `SyncReport` ingestion
  publish; the `aggregate()` fold carries the same shapes so the
  CLI/UI surfaces extend without re-shaping); sync `actor.rs` (emit
  the raw facts the engine ingests — **this is the sequencing-
  sensitive file**, see §5); app `commands/output.rs` (`status`
  rendering of the counters; structural assertions, no text);
  engine side (#145 only): a handle-side atomic `EngineMetrics`
  struct + a `status` surface reading it — never in `AppState`, no
  insta pass.
- **Behaviours**: counters survive a restart from zero (cumulative
  per process run — document the choice); dashboards-to-come and
  `status` read the same state; no new wake source.
- **Non-goals**: no histograms/quantiles (counters + last-duration
  only), no metrics crate (state-borne by ruling), no UI badge
  change beyond what falls out of the fold.

### K9 — kiosk deployment profile + systemd unit + runbook (S–M; branch `feat/kiosk-deployment-profile`; #233 prep, #160)

- **Origin**: KDF-10; multi-board design's config-pinned-fixed-board
  deferral; #233's proposed approach.
- **Files**: app `config.rs` (the `[kiosk]` section:
  `fixed_board: Option<Uuid>` typed-resolved at boot against live
  boards — unknown/dead board = typed config error; section absent
  = today's behaviour); `contrib/taskboard-kiosk.service` (system
  unit: config-file-driven `mode = "kiosk"`, `Restart=always`,
  display-env import, `WantedBy=graphical.target`; a README-documented
  install path); `contrib/` or `docs/` bring-up runbook (image
  checklist: no desktop environment, `SLINT_BACKEND`/software-
  renderer selection, SSD/USB boot note for the spinning disks,
  harness invocation on-device, the four budget verdicts to record);
  CHANGELOG.
- **Behaviours**: a pinned board resolves or the boot fails typed;
  the unit never launches via env-var mode (ADR 0015's #236 rule);
  the runbook names the spike's measurement plan (audit axis J).
- **Non-goals**: no rotation keys (K13), no build-feature flip for
  `backend-fb` (the spike rules), no `sd_notify` (decide at the
  spike — #160 stays open with the decision recorded as a comment).

### K10 — web kiosk dashboard design round (S–M; branch `design/kiosk-dashboard`; docs-only; #234)

- **Origin**: framing §5.1; KDF-12; the D-D5 parallel-design
  precedent.
- **Files**: `.artifacts/plans/<date>-kiosk-dashboard-design-plan.md`
  — the design-proposal doc with §6-style questions + exactly one
  recommendation each: HTTP stack (candidates: `tiny_http`-scale /
  `axum`-scale / hand-rolled `hyper`; the deliberately-boring-page
  argument cuts toward small; **stack approval is the user's**);
  bind + auth (loopback default; wider bind gated on token vs
  none-with-loud-warning vs reverse-proxy assumption); content
  scope; refresh strategy (meta-refresh polling, no-JS fallback);
  server placement (app-layer task on the daemon lifecycle,
  opt-in config section); `standalone`-mode interaction; caching;
  the 2014-browser compatibility checklist.
- **Behaviours**: the doc lands with verified anchors (the
  `ArcSwap` read path, the KDF-8 metrics surface) and awaits the
  user's decision session; no production code.
- **Non-goals**: no implementation (K11), no dependency edits.

### K11 — dashboard implementation (M; branch `feat/app-web-dashboard`; #234; GATED on K10 acceptance)

- **Origin**: the accepted K10 design; KDF-12.
- **Files**: per the K10 ruling (expected: an app-layer HTTP module
  or `taskboard-app` sibling module; the approved crate pinned once
  in `[workspace.dependencies]`; server-rendered boring HTML over
  the lock-free state read; config section; `cargo deny` over the
  new tree).
- **Behaviours**: loopback default with the auth model exactly as
  ruled; the page renders boards + sync health + offline state;
  a hands-on tablet test is flagged for the user (honest-record
  convention if the device is unavailable to the agent).
- **Non-goals**: no write surface, no WebSocket/SSE (polling),
  no auth beyond the ruling.

### K12 — encrypted-file credential store (M; branch `feat/app-file-credential-store`; #126 + #159; GATED on D-K8)

- **Origin**: KDF-9; D-K8's default (generated 0600 key file;
  passphrase variant only with argon2 + stack approval).
- **Files**: app `secrets.rs` (the third `CredentialStore` impl;
  AEAD envelope or age-style format per stage-1's
  new-dependency audit — any new crate needs stack approval);
  `config.rs` (`credential_store = "file"`); the key-file
  bootstrap ceremony (generate on first use, 0600, typed
  permission failure); README/docs; tests via a fake store path
  + tempdir round-trips.
- **Behaviours**: never a silent fallback (the port's rule); the
  env/keyring/file trio selects explicitly by config; unattended
  kiosk boot works headless with the key file present and fails
  typed without it.
- **Non-goals**: no key rotation UI, no multi-profile keyring, no
  change to the wire (secrets never ride IPC — pinned).

### K13 — kiosk ambient view + display rotation (M; branch `feat/ui-kiosk-ambient-view`; the D-K2 issue; GATED on daily-driver chunk 10)

- **Origin**: future-directions §3.7; D-V3 + D-V1 rotation-readiness;
  KDF-11; KDF-10's rotation-keys reservation.
- **Files**: ui `view_state.rs` (a kiosk display-mode state machine:
  the pinned board's kanban ⇄ today/upcoming, compact + passive
  degeneration per D-V3), `actions.rs`/`router.rs` (rotation is
  UI-side timer-driven — no engine commands; `FlowOutcome::Command`
  never produced by rotation), `models.rs`, `presentation.rs`
  (board-pinned subset reads), a new `ui/views/kiosk.slint` (or a
  kiosk composition over the existing views — stage 1 settles);
  `config.rs` `[kiosk]` rotation keys (dwell per mode, enabled
  modes); `testing_strategy.org`-style time-dependent tests
  (`pause`/`advance`, no sleeps).
- **Behaviours**: rotation flips modes on the timer with no input;
  the overdue banner + offline tone carry over; the pinned board
  resolves per K9; no capture/edit surface on kiosk (DD-10's rule).
- **Non-goals**: no cross-board rotation (the pin is the scope; a
  rotating multi-board ring is backlog), no animation work, no
  renderer changes.

### K14 — docs + wave exit verification (S; branch `docs/kiosk-wave`; the wave exit)

- **Files**: `docs/architecture.org` (the shipped mode table + the
  kiosk subsystems), `README.org` (kiosk deployment section),
  CHANGELOG promotion, the exit report at
  `.artifacts/reports/<date>-kiosk-wave-exit-report.md` with the
  spike's on-device verdicts + the Q6 default-mode input recorded
  for the user's ruling; milestone `wave-kiosk` closure review.
- **Non-goals**: no code, no tracker edits beyond §6's.

## 4. Quality gates & conventions

Every chunk: the full AGENTS §10 gate set (`fmt --check`, `clippy
--workspace --all-targets -D warnings`, `nextest`, doc tests,
`cargo deny`, the CI-identical three-command llvm-cov sequence).
TDD per `docs/testing_strategy.org` with the house rules restated:
typed outcomes, never text assertions; no sleeps — `pause()`/
`advance()` (rotation timers!), deadline polls, join handles; fakes
over mocks; proptest where pure logic appears (rotation policy);
one behaviour, one test, at the lowest layer that owns it. New
`.rs` files carry the per-crate SPDX header; `#![forbid(
unsafe_code)]` everywhere except `taskboard-ui-slint`. **Snapshot
rule**: K8's domain-status widening owns its one deliberate insta
pass; no `.pending` snapshots ship. **Tier honesty**: harness
numbers are dev-machine references recorded in chunk reports, never
silent skips. **Branch/PR conventions**: one branch + PR per chunk;
no stacked PRs by default; `--force-with-lease` only. **Dependency
discipline**: no new crate (HTTP stack, argon2, AEAD/format crate)
enters before its stack-approval ruling — K10/K12 stage-1 prompts
carry that gate inline.

## 5. Sequencing & exit criteria

**Phase A spine**: `K1 ∥ K2 → K3 → K4`; `K5 ∥ K6 ∥ K7` (disjoint
files, dispatchable immediately, even before K1); `K8` in its
**sequencing window** — the sync-actor emission site collides with
multi-board chunk 4 (`feat/sync-connector-modules`) and
daily-driver chunks 3–4 (both rewrite `sync/actor.rs`): dispatch K8
either **before** multi-board chunk 4 starts or **in the
multi-board→daily-driver gap** (after the multi-board round closes,
before DD chunk 3 dispatches — the gap exists by DD's own MD-1
gate); never concurrent with either. `K9` after K3 and after the
multi-board config chunk (`feat/app-connector-registry`) merges
(same `config.rs`, rebase-scale otherwise). `K10` anytime (docs-
only; earliest is best — it gates K11). **Phase B** (user): the
spike after phase A, per the K9 runbook. **Phase C**: `K11` after
K10's acceptance (+ K8 merged — content); `K12` after D-K8 (any
time otherwise — `secrets.rs` is untouched by the other rounds);
`K13` **strictly after daily-driver chunk 10** (dependency +
file-set collision); `K14` last, serialised with the other waves'
exit chunks. Parallel groups never share worktrees; per-host
concurrency caps per the parallelism report §4.

**The wave's stated interims** (sequencing facts, stated loudly):
(i) between K2 and K3 the profile flag exists with no consumer —
correct, K3 is its only consumer by design; (ii) the harnesses
(K5–K7) measure the **daemon/desktop** processes until the spike
extends them to the appliance on-device — the appliance is K3's
deliverable and the dev-machine harnesses target the process
shapes that exist; (iii) `#160` stays open through the spike with
its decision recorded as an issue comment — closing it without the
spike's ordering evidence would be premature.

**Exit criteria → verifying chunks**:

| Exit criterion | Verified by |
|---|---|
| `mode = "kiosk"` boots the appliance: display-gated window, loud headless fallback, close-exits, `daemon stop` over ssh, `status` serves | K3 (+ K1/K2) |
| `taskboard gui` + `.desktop` + launch-docs table complete the double-click story | K1, K4 |
| RSS, cold-start, refresh+draw, sync-CPU instrumentation exist with recorded dev-machine numbers (or the S2 evidence outcome) | K5, K6, K7 |
| Sync/engine metrics readable lock-free; `status` renders counters structurally | K8 |
| Kiosk deployment: `[kiosk] fixed_board` typed, system unit, runbook | K9 |
| On-device budget verdicts (RSS ≤ 200 MB; repaint ≤ 50/20 ms; cold start ≤ 5 s; sync CPU share) + renderer/backend decision + #160 ruling + Q6 input | phase B spike report (user), recorded in K14 |
| Dashboard serves boring HTML under the ruled auth/bind model | K10 design + K11 |
| Headless credential story complete: file store, typed failures, no silent fallback | K12 |
| Ambient view rotates board-pinned, passive, timer-driven | K13 |
| Docs/CHANGELOG promoted; exit report; milestone closure review | K14 |

## 6. Completion bookkeeping

Closing happens in the implementation round — issues close as their
chunks' PRs merge (`Closes #N` in the PR body); annotations are one
progress comment per mapped issue per chunk. Per-chunk map:

| Chunk | Issue action |
|---|---|
| K1–K3 | annotate #237 per chunk; **K3 is the substantive land** |
| K4 | **closes #237** (appliance complete) |
| K5 | **closes #299**, **closes #262** |
| K6 | **closes #300** (or annotates with the evidence outcome if the draw leg is blocked — then the issue closes with the recorded evidence per F13-03's own framing) |
| K7 | **closes #263** |
| K8 | **closes #156**; **closes #145** if D-K3 assigned it |
| K9 | annotates #233 (prep half landed; on-device half pending); #160 keeps its decision comment |
| K10 | annotates #234 (design landed, awaiting ruling) |
| K11 | **closes #234** |
| K12 | **closes #159**; **closes #126** (remainder complete) |
| K13 | **closes** the D-K2 ambient-view issue |
| K14 | the spike-evidence exit report; #233 **closes on the recorded on-device verdicts** (the user's spike report is the evidence); milestone closure review; the Q6 input recorded |

The wave creates, labels, or closes nothing beyond this map plus
the framing §9 bookkeeping the user rules (create ambient-view
issue; assign #159/#145; close #123; milestone description).

# [Plan] Kiosk Wave 1 — Appliance Mode + `gui` Verb (implementation)

- **Date**: 2026-10-16
- **Kind**: implementation plan for the **accepted** kiosk design
  (origin: `.artifacts/plans/2026-10-16-kiosk-daemon-gui-control-plan.md`,
  decision record §6). Chunked for one branch + PR per chunk; chunks
  are written so they can be implemented directly by an agent or
  promoted into a dispatch plan without restructuring.
- **Status**: READY — chunks 1 and 3 are startable immediately after
  this plan's PR merges; chunk 2 stacks on 1+3; chunk 4 stacks on 1.
- **Decisions baked in** (origin §6): A accepted; `taskboard gui`
  verb included (Q1/Q4); kiosk window close exits the whole appliance
  (Q2); no-display → headless fallback with a loud log (Q3); wave-1
  window profile = fullscreen + `[ui] scale` on the desktop
  `AppWindow` (Q5); wave 2 = web dashboard before attachable GUI
  (Q7). Still open, post-wave: default `[app] mode` (Q6).

## 1. Scope

**In**: `mode = "kiosk"` becomes the appliance lifecycle (core + IPC
server + fullscreen kiosk window, display-gated); `taskboard gui` as
an explicit desktop-frontend verb; the kiosk window profile flag;
release packaging (`.desktop`, README "Launching"); ADR 0009
amendment + ADR 0016.

**Out (later waves)**: B (`taskboard gui` attaching to a *running*
daemon over state streams), C (web dashboard — next wave, before B),
a dedicated ambient kiosk view, renderer/backend selection (waits on
the *Kiosk hardware bring-up* spike), default-mode revisit, kiosk
window-close semantics beyond "close exits the appliance".

## 2. Design facts every chunk relies on

1. **Routing today** (`crates/taskboard-app/src/ui.rs:79-87`,
   `main.rs:148-172`): bare desktop → GUI (TTY-independent); bare
   daemon+TTY → REPL; bare daemon/kiosk non-TTY → daemon lifecycle;
   `main.rs:250-257` refuses Desktop/Kiosk inside the daemon
   lifecycle. Wave 1 changes: bare kiosk → **kiosk appliance
   lifecycle, TTY-independent** (a seat has no operator TTY; an
   `ssh -t` bare invocation booting the appliance is acceptable — the
   explicit `daemon` subcommand remains the headless entry).
2. **No-display behaviour (verified on the release artifact,
   2026-10-16)**: with `WAYLAND_DISPLAY`/`DISPLAY` unset, slint
   1.18.1 **panics** during backend init (exit 101, panic in
   `slint-1.18.1/lib.rs:784`) — it does **not** surface the typed
   `UiMountError`. The appliance lifecycle therefore gates on display
   presence **before** `mount()` and logs `error!` loudly on the
   headless fallback. The gate→mount race (display vanishing in
   between) is tolerated in wave 1 and documented in ADR 0016;
   optional hardening (catch-unwind around mount → headless
   fallback) may become a backlog entry at chunk-2 review.
3. **The daemon tail is the shared shape**: boot (`bootstrap`) →
   serve IPC (`ipc::server::spawn_server`) → wait for signal or
   `server.stopped()` → drain (`server.join(shutdown_timeout)`) →
   `app.shutdown()`. Both the daemon lifecycle and the appliance's
   headless fallback must share **one** extracted helper — no
   duplicated shutdown choreography.
4. **Window close = appliance exit** (decision Q2) comes naturally
   from the `run_desktop` shape: the slint event loop returns on
   window close, and the lifecycle then drains the IPC server and
   shuts the core down. `daemon stop` over IPC and SIGTERM converge
   into the same exit via the watcher task calling
   `taskboard_ui_slint::quit()` (same pattern as `run_desktop`'s exit
   watcher, extended with `server.stopped()`).
5. **Instance lock**: unchanged — one core per database; a second
   GUI/appliance/daemon on the same db gets the typed
   `instance-locked` error; CLI clients over IPC never contend.

## 3. Chunks

### Chunk 1 — `taskboard gui` verb (S; branch `feat/app-gui-verb`)

- `crates/taskboard-app/src/cli.rs`: new `Command::Gui` — doc
  comment: explicit desktop-frontend selection; boots the core and
  the desktop window **in this process** regardless of `[app] mode`;
  a running instance for the db fails typed `instance-locked`.
- `crates/taskboard-app/src/main.rs::run`: handle `Gui` **before**
  `route_for_ipc` (it is a lifecycle arm, never an IPC-relayed
  command): load config → `taskboard_app::ui::run_desktop(config)`.
  `route_for_ipc` must stay gui-blind (no arm, never relayed).
- Shell completion: regenerate/extend the completion list in
  `completion.rs` if subcommands are enumerated there.
- Docs: ADR 0009 amendment section (the new verb and its
  never-relayed rule); CHANGELOG under Unreleased.
- Tests: clap parse of `gui`; routing unit test (gui arm chosen
  regardless of TTY and of `[app] mode`); a negative asserting
  `route_for_ipc` has no gui lane. `run_desktop` itself stays thin
  glue (existing convention), exercised by the existing desktop path.

### Chunk 2 — kiosk appliance lifecycle (M; branch `feat/app-kiosk-lifecycle`; stacks on chunks 1+3)

- `crates/taskboard-app/src/ui.rs`:
  - `bare_invocation_route`: `Kiosk → BareRoute::KioskAppliance`
    (new variant, TTY-independent); update the existing test that
    pins kiosk → DaemonLifecycle.
  - `display_available()`: pure decision fn over the two winit
    seams — unit-testable as `display_available(wayland: Option<&str>,
    x11: Option<&str>)` with the env-reading wrapper kept trivial.
- `crates/taskboard-app/src/main.rs`:
  - Extract `serve_until_signal_then_shutdown(app, server, …)` from
    `daemon_lifecycle`'s tail (select/join/shutdown, second-signal
    force exit included); both the daemon path and the appliance
    fallback call it.
  - New `run_kiosk(config, config_path, json)`:
    1. `bootstrap` the core (identical to the daemon path);
    2. spawn the IPC server;
    3. if `display_available()` (env wrapper) **and** mount succeeds
       path: build the ADR 0015 hand-over with
       `UiSettings.kiosk = true`, spawn the forwarder, spawn the
       watcher task `{ select(wait_for_signal, server.stopped()) →
       taskboard_ui_slint::quit() }`, run the event loop on the main
       thread; on return: abort watcher + forwarder, drain the server
       (`server.join`), `app.shutdown()` — window close (Q2), SIGTERM,
       and `daemon stop` all land here;
    4. else: `tracing::error!("no display available; continuing as
       headless daemon")` and call the shared tail — the appliance
       degenerates to exactly today's daemon behaviour.
  - `daemon_lifecycle`'s mode gate now refuses only `Desktop`
    (explicit `daemon` + `[app] mode = "desktop"` stays a typed
    refusal; `Kiosk` is served by the new arm).
- Docs: **ADR 0016** "Kiosk appliance mode" — TTY-independent bare
  routing, display gate + loud fallback (with the panic evidence and
  the tolerated gate→mount race), close-exits-appliance, IPC served
  on the seat, explicit `daemon` stays headless; architecture.org
  mode table row.
- Tests: route mapping (kiosk → appliance, both TTY values);
  `display_available` decision table; the shared tail keeps the
  daemon path's existing tests green (no duplication); gate-absent
  fallback asserted at the decision layer (`KioskStart::Headless`),
  the env→gate wrapper and the mount wiring staying thin glue per the
  repo's main-glue convention.

### Chunk 3 — kiosk window profile flag (S; branch `feat/ui-kiosk-profile`; independent, land before chunk 2)

- `crates/taskboard-ui-slint`: `UiSettings` gains `kiosk: bool`
  (default `false`); `mount()` applies the kiosk profile when set:
  fullscreen + suppress window decorations (verify the exact 1.18.1
  API — `Window::set_fullscreen` and the decoration control; if
  decorations cannot be suppressed programmatically, fullscreen alone
  is acceptable for wave 1, noted in ADR 0016). `[ui] scale` flows
  unchanged (decision Q5: no new scale knob).
- Consumers: none yet (chunk 2 passes the flag); the flag defaults
  off so the desktop path is untouched.
- Tests: the UI crate's harness path (ADR 0015 harness / mcp smoke)
  asserts the fullscreen window property with `kiosk = true` and its
  absence with `false`; token/theme lint gates unchanged.

### Chunk 4 — packaging & launch docs (S; branch `feat/packaging-desktop-file`; stacks on chunk 1)

- `contrib/taskboard.desktop`: `Exec=taskboard gui`, `Type=Application`,
  desktop categories; icon: reference `taskboard` and accept the
  theme fallback for now (a shipped SVG icon is a follow-up nit).
- `scripts/package_release.py`: stage the `.desktop` file
  (`STAGED_FILES`) so archives carry it; `--check` updated
  automatically by the staged list.
- `README.org`: "Launching" section — bare-launch semantics table
  (TTY → REPL, no-TTY → daemon, kiosk → appliance, `gui` → desktop
  window), the `TASKBOARD_APP__MODE=desktop` escape hatch, and the
  `.desktop` install hint.
- CHANGELOG entries for the verb, the appliance mode, and the
  packaged launcher.

## 4. Quality gates & conventions

Every chunk: the full AGENTS §10 gate set
(`fmt --check`, `clippy -D warnings`, `nextest`, doc tests,
`cargo deny`, `llvm-cov`) — the new logic lands as pure,
unit-tested decisions (routing, gate, shared tail) with process
wiring kept as deliberately-untested thin glue, per the established
convention. TDD per testing_strategy; no sleeps (join handles and
signal/watcher seams); typed outcomes, no text assertions.

## 5. Sequencing & wave-2 pointer

`1 ∥ 3 → 2 → 4` (chunk 2 stacks on both 1 and 3 → stacked PRs, so
expect the post-merge rebase dance from AGENTS §8). After wave 1
ships and the on-device spike has numbers: **wave 2 = C (web
dashboard)** per decision Q7, with **B** (attachable GUI over IPC
streams) after it; the default-`[app] mode` question (Q6) is
revisited then.

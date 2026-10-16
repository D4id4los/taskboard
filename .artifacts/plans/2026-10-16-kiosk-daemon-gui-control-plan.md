# [Plan] Kiosk Daemon + GUI Control — Design Proposals (for review)

- **Date**: 2026-10-16
- **Kind**: design-proposal options paper. **ACCEPTED 2026-10-16** —
  the decision record is §6; the implementation plan for the accepted
  wave 1 is `.artifacts/plans/2026-10-16-kiosk-mode-wave1-plan.md`.
  This document stays the design-of-record for the wave-2 options
  (B, C) and the diagnosis (§1).
- **Origin**: user report 2026-10-16, two observations from trying the
  Linux binary produced by the v0.1.0 release CI run
  (`taskboard-0.1.0-x86_64-unknown-linux-gnu`, artifact of run
  38872697980):
  1. executing the binary from Dolphin spawns a process that never
     opens a GUI and has to be killed from the command line;
  2. kiosk modes will need a daemon controllable over CLI/ssh, with a
     GUI controlled by that daemon — or some other way for a running
     GUI to be controlled through the CLI over ssh.
- **Related backlog entries** (`.artifacts/taskboard-potential-improvements-backlog.md`):
  this date's *file-manager double-click* bug entry, *kiosk mode*
  feature entry, and *release.yml MCP guard* CI entry; the existing
  *[2026-10-16] Kiosk hardware bring-up* (NF310/D250, software
  renderer), *[2026-10-16] Web kiosk dashboard served by the daemon*
  (tablet path), and *[2026-10-11] IPC keep-alive / pipelining / event
  streams* entries.
- **ADRs touched by the proposals**: 0008 (bootstrap & modes), 0011
  (REPL frontend), 0012 (daemon IPC transport), 0015 (Slint dumb
  presentation client). Option A amends 0008's mode semantics; Option
  B likely earns its own ADR.

## 1. The two problems

### 1.1 P1 — Dolphin launch: no GUI, unkillable-looking process (diagnosed)

**Not a crash and not a GUI failure — it is the documented routing
working as designed, with an unlucky default.** Chain of facts:

1. The release archive ships no config file
   (`scripts/package_release.py` stages only license/README/changelog
   + binary), and no `taskboard.toml` exists at the tested machine's
   XDG config path. The figment chain therefore resolves
   `[app] mode` to its default — `AppMode::Daemon`
   (`crates/taskboard-app/src/config.rs:87-95`).
2. A file-manager launch (Dolphin → "Execute") has no TTY on stdin.
   The bare-invocation routing
   (`crates/taskboard-app/src/ui.rs:79-87`) maps
   `daemon + non-TTY → DaemonLifecycle`; only `desktop` mode reaches
   the GUI (`run_desktop`, `crates/taskboard-app/src/ui.rs:157-210`).
3. The daemon lifecycle
   (`crates/taskboard-app/src/main.rs:244-334`) boots the actor
   graph, opens the per-database IPC socket, and serves until a
   signal or `daemon stop` — by design it has no window and never
   exits on its own. Every log line goes to stderr, which Dolphin
   discards, so the process is completely invisible.

**Reproduction** (release artifact, isolated XDG dirs, no TTY): the
binary logs `resolved configuration … mode=Daemon`, then `daemon
serving commands over ipc`, and idles until SIGTERM, which shuts it
down cleanly. `TASKBOARD_APP__MODE=desktop` on the same binary opens
the GUI window (confirmed by the user on the real desktop). So:

- **Workaround today**: `TASKBOARD_APP__MODE=desktop taskboard …`
  (figment `__`-nesting, `crates/taskboard-app/src/config.rs:4-5,33`)
  or a config file with `[app] mode = "desktop"`.
- **Bug classification**: the behaviour is per ADR 0011/0012 routing
  intent, but the *product* behaviour "double-click → silent
  background daemon, no feedback anywhere" is a UX defect of the
  release packaging/docs, not of the routing. Fix directions in §5.

### 1.2 P2 — kiosk: daemon + GUI controlled through CLI/ssh

Requirement restated: on the kiosk machine the app runs unattended;
an operator on ssh must be able to control the *running* app with the
CLI; the kiosk seat shows a GUI that reflects exactly the state the
CLI commands mutate.

**Architectural note that frames every proposal**: the CLI never
controls the GUI directly. Under ADR 0001/0015 both are frontends of
one headless core — the GUI renders core state; CLI commands mutate
it; consistency comes from the state, not from GUI↔CLI messaging.
"GUI controlled by the daemon" therefore reduces to: *which process
hosts the window, and how does it observe the core.* The options
below differ only in that dimension.

## 2. What v0.1.0 already provides (the facts the proposals build on)

| Capability | State in v0.1.0 | Where |
|---|---|---|
| CLI control of a *running* core over ssh | **Works today**: bare-or-explicit daemon serves a per-database unix socket; every CLI verb relays argv over it (ADR 0012); `daemon stop` / `status` included | `crates/taskboard-app/src/ipc/{server,client}.rs`, `main.rs:95-138` |
| GUI | Desktop only, same-process: `ArcSwap<AppState>` reads + broadcast subscriptions + `mpsc` command channel out (ADR 0015 hand-over) | `crates/taskboard-app/src/ui.rs:157-210`, `crates/taskboard-ui-slint` `mount/run/quit` |
| Kiosk mode | Named in config, refused at boot ("not implemented yet") | `crates/taskboard-app/src/main.rs:250-257` |
| State serialization / GUI attach protocol | **Does not exist.** The IPC surface is an argv relay (`ExecOutcome{out,err,code}`), not a typed state API | ADR 0012 |
| Instance lock | One core per database; CLI clients connect without contending | `crates/taskboard-app/src/lockfile.rs` |
| Low-power rendering | Slint software renderer anticipated for kiosk HW (GMA 950/3150 have no usable GL); `backend-fb` + libinput candidate | backlog *Kiosk hardware bring-up* (2026-10-16) |

The decisive asymmetry: **remote CLI control is already solved**; the
missing piece is exclusively the on-seat GUI for `mode = "kiosk"`.

## 3. Proposals for P2

### Option A — kiosk = daemon core + on-seat native frontend, one process *(recommended first wave)*

`mode = "kiosk"` stops being refused and becomes the *appliance*
entry: boot the core exactly like the daemon path (storage, engine,
IPC server, system-event logger) **and** mount a fullscreen kiosk
window, structurally `run_desktop` plus `spawn_server`.

- **Routing sketch**: the bare-invocation arm (not the explicit
  `daemon` subcommand) routes `kiosk → run_kiosk(config)`. The
  explicit `taskboard daemon` subcommand stays headless *always* —
  that is the ssh-spawned maintenance path. A bare kiosk start with no
  display available falls back to the headless daemon lifecycle (or
  refuses typed — §6 Q3), so the same binary serves the seat and the
  ssh session.
- **The window**: reuse the desktop `AppWindow` behind a kiosk
  profile first (fullscreen, kiosk scale preset); the dedicated
  ambient-display view and renderer/backend choice (software renderer,
  `backend-fb`) follow the hardware bring-up spike's numbers.
- **Reuses**: the entire ADR 0015 hand-over, the UI command
  forwarder, the exit watcher, the instance lock, the IPC socket.
  Zero new protocol, zero new state machinery.
- **Limits**: a GUI crash takes the core down with it (restart is a
  whole-appliance restart — acceptable under a systemd
  `Restart=always` unit, see backlog *Systemd/journald packaging*);
  the GUI must run on the same machine as the core.
- **Effort**: small — a lifecycle function, a routing case, a window
  profile, packaging/docs. Tests: routing table extension
  (`bare_invocation_route`), lifecycle bootstrap/fallback (display
  seam injectable), snapshot coverage of the kiosk hand-over.

### Option B — attachable GUI process over IPC (`taskboard gui`)

The GUI becomes its own process connecting to the daemon's socket;
the IPC gains an `attach` verb streaming a full snapshot plus
state deltas; the client maintains a replica `AppState` and feeds the
existing ADR 0015 hand-over from it.

- **Wins**: GUI crash isolation (systemd restarts the window, core
  stays); GUI on a *different* machine than the core (thin-seat
  kiosk); the daemon becomes truly headless everywhere.
- **Costs**: a real protocol design (message framing, serde for the
  state tree, delta vs snapshot resync, reconnect backoff) on top of
  the currently argv-relay IPC — this is the natural first consumer of
  the backlog's *IPC keep-alive / pipelining / event streams* entry.
  Needs its own design round + ADR before any dispatch.
- **Effort**: large; strictly after A (A's lifecycle and window
  profile are prerequisites/subsets).

### Option C — daemon-served web dashboard *(already backlog, complementary)*

The *[2026-10-16] Web kiosk dashboard served by the daemon* entry
(opt-in HTTP surface, conservative HTML for ancient browsers, tablet
path). Not the kiosk-seat frontend, but the cheapest *management*
surface; composes with A or B. Keep sequenced separately with its own
auth-model question.

### Option D — network display forwarding (waypipe / ssh -X / VNC)

Zero code: ship desktop mode, forward the window to the seat.
Rejected as the product answer — it makes the kiosk depend on the
network and a compositor stack, contradicting the offline-resilient,
low-power goals; worth keeping only as an operational stopgap.

### Comparison

| | New IPC surface | GUI/core crash isolation | GUI off-box | Low-power fit | Effort |
|---|---|---|---|---|---|
| **A** kiosk lifecycle + in-process window | none | none (same process) | no | native, renderer-selectable | S |
| **B** `taskboard gui` over IPC streams | snapshot+deltas | yes (separate processes) | yes | native | L |
| **C** web dashboard | HTTP read surface | yes (browser) | yes (that's the point) | browser-dependent | M |
| **D** display forwarding | none | partial (X/Wayland layer) | yes | poor | — |

## 4. Recommendation & sequencing

1. **Next GUI wave: Option A** — it completes `mode = "kiosk"` with
   machinery that already exists, makes the ssh story concrete
   (`ssh kiosk taskboard …` against the running appliance; `taskboard
   daemon stop` to shut it down), and every artifact it adds
   (lifecycle, window profile, routing) is a strict subset of B.
2. Fold the **P1 fixes (§5)** into A's packaging/docs chunk so the
   desktop double-click story is fixed by the same release.
3. **Wave 2 by want**: B when GUI-crash isolation or an off-box seat
   becomes real (design round + ADR first); C for the tablet per its
   own entry.
4. Renderer/backend decision for the kiosk window rides the hardware
   bring-up spike (NF310 first), not this plan.
5. Per AGENTS §7: on acceptance of A (and the §6 answers), write the
   phase plan, then the dispatch plan.

## 5. P1 fix directions (desktop launch UX)

Ordered, first two are zero-code-change:

1. **README "Launching" section** (ships in the archive already):
   the bare-invocation semantics table (TTY → REPL, no-TTY → daemon,
   `mode = "desktop"` → GUI) and the `TASKBOARD_APP__MODE=desktop`
   escape hatch.
2. **Ship a `taskboard.desktop`** in the release archive (and a
   `contrib/` copy for manual installs):
   `Exec=env TASKBOARD_APP__MODE=desktop taskboard` — file-manager
   double-click gets a window; the raw binary keeps its
   appliance/daemon semantics for systemd.
3. **Optional sugar (review-decided)**: a `--mode <mode>` CLI override
   above the figment chain, or a `taskboard gui` verb equivalent to
   the env override — nicer for the `.desktop` `Exec` line and for
   `ssh host taskboard gui`-style one-shots; tiny, but touches the
   CLI surface contract (ADR 0009).
4. **Keep the default mode `daemon`** for now (systemd and the REPL
   workflows depend on it); revisit "default = desktop for bare
   interactive use" only after the kiosk wave settles — §6 Q6.

## 6. Decision record (user review, 2026-10-16)

All §6 questions from the PROPOSED draft, resolved:

1. **Q1 — proposal**: Option **A accepted** as the wave-1 kiosk
   design, with a `taskboard gui` verb included in the same wave.
2. **Q2 — kiosk window close**: closing the kiosk window **exits the
   whole appliance** (graceful shutdown, IPC drain included). Richer
   semantics (ignore/restart/supervisor) deferred until a need
   appears.
3. **Q3 — bare kiosk start without a display**: **headless fallback
   with a loud log.** Implementation constraint discovered while
   verifying (release artifact, display vars removed): slint 1.18.1
   **panics** during backend init without a display (exit 101,
   panic in `slint-1.18.1/lib.rs:784`) instead of surfacing the
   typed `UiMountError` — so the lifecycle must **gate on display
   presence before mounting** (`WAYLAND_DISPLAY`/`DISPLAY` seams),
   log `error!` on fallback, and only then attempt the window. The
   gate-to-mount race (display vanishing in between) is tolerated
   for wave 1; hardening idea in the wave plan.
4. **Q4 — launcher form**: a **`taskboard gui` verb**, not a
   `--mode` flag — it fits the existing subcommand surface (explicit
   frontend selection like `daemon`). The `.desktop` `Exec` line
   becomes `taskboard gui`; the env-var escape hatch stays documented
   as-is.
5. **Q5 — kiosk window profile for wave 1**: **fullscreen + scale
   preset on the existing desktop `AppWindow`** — enough to measure
   real performance on the target hardware (see backlog *Kiosk
   hardware bring-up*); the dedicated ambient view is a later wave.
6. **Q6 — default `[app] mode`**: **still open, deferred** to after
   the wave ships (needs the kiosk on-device experience first).
7. **Q7 — wave-2 priority**: **C (web dashboard) before B**
   (attachable GUI process).

## 7. References

- Routing: `crates/taskboard-app/src/ui.rs:79-87`
  (`bare_invocation_route`), `crates/taskboard-app/src/main.rs:148-172`
  (bare arm), `crates/taskboard-app/src/main.rs:250-257` (kiosk/desktop
  refusal in the daemon lifecycle)
- Default mode: `crates/taskboard-app/src/config.rs:87-95`; env
  nesting: `crates/taskboard-app/src/config.rs:4-5,33,451-453`
- Desktop lifecycle: `crates/taskboard-app/src/ui.rs:157-210`; UI
  hand-over: ADR 0015; IPC transport: ADR 0012; REPL: ADR 0011
- Packaging: `scripts/package_release.py` (staged file list);
  release workflow `.github/workflows/release.yml`
- Reproduction: v0.1.0 release run 38872697980 artifact
  `dist-x86_64-unknown-linux-gnu`, run under isolated
  `XDG_CONFIG_HOME`/`XDG_DATA_HOME`, no TTY — daemon mode confirmed,
  `TASKBOARD_APP__MODE=desktop` GUI confirmed on the user's desktop
- Backlog: *Kiosk hardware bring-up*, *Web kiosk dashboard served by
  the daemon* (both 2026-10-16), *IPC keep-alive / pipelining / event
  streams* (2026-10-11), *Systemd/journald packaging* (2026-10-10)

# [Plan] Kiosk Chunk 3 — kiosk appliance lifecycle + ADR 0016 (TDD-first)

- **Date**: 2026-10-19
- **Kind**: stage-1 implementation plan (dispatch plan §4, chunk 3) for
  `feat/app-kiosk-lifecycle` — the `mode = "kiosk"` appliance, the
  shared daemon tail, and ADR 0016. TDD-first; no implementation in
  this commit.
- **Origin (normative, MD-7 rank)**: the wave implementation plan
  `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
  (§2 **KDF-1/2/3/4/6**, §3 **K3**, §4–§6); the accepted design
  `.artifacts/plans/2026-10-16-kiosk-daemon-gui-control-plan.md` (§3
  **Option A**, §6 **Q2** (close-exits) and **Q3** (gate + loud
  fallback) — ACCEPTED 2026-10-16); the dispatch plan
  `.artifacts/plans/2026-10-19-kiosk-implementation-dispatch-plan.md`
  §1/§4 (MD-2's stacking exception; MD-6: this path); ADR 0015 incl.
  the 2026-10-18 #236 amendment; the chunk-1 stage-1 plan
  `.artifacts/plans/2026-10-19-kiosk-gui-verb-plan.md` (the `Gui`
  lifecycle arm this chunk routes around) and the chunk-2 stage-1 plan
  `.artifacts/plans/2026-10-19-kiosk-kiosk-window-profile-plan.md`
  (the `UiSettings.kiosk` flag this chunk consumes, and its **D4
  caveat text** ADR 0016 embeds verbatim). Deviations from prose are
  recorded inline with reasons; none touches an ACCEPTED ruling.
- **Status**: READY — every open question settled below (§3 decisions
  D1–D13); symbols re-located at plan time (base `279d3b3` =
  `origin/main`, fetched 2026-10-19).

## 0. Precondition & merge state found (MD-9 / MD-2)

**Chunks 1+2 are MERGED on `origin/main`.** Tip `279d3b3` ("feat(ui):
kiosk window profile flag") contains chunk 2's plan + implementation
commits (`473b9a5`, `279d3b3`) and chunk 1's (`e4abd6a`, `1d51381`,
`d098cc6` — verb + ADR 0009 amendment); `git cherry -v origin/main
feat/app-gui-verb` / `… feat/ui-kiosk-profile` mark every local
branch commit `-` (patch-equivalent — the local branches hold the
pre-rebase originals, nothing unmerged). **Consequence (MD-2): no
stacking** — the worktree branches straight off `origin/main`
(`../taskboard-WT-feat-app-kiosk-lifecycle`, branch
`feat/app-kiosk-lifecycle`, base `279d3b3`), and stage 3 runs the
plain rebase dance (no duplicate-commit dropping; nothing is stacked
behind this branch). Recorded here so stages 2–3 do not look for a
stack that does not exist.

## 1. What this chunk delivers

`mode = "kiosk"` stops being refused and becomes the **appliance**
(Option A / Q1): one process that boots the daemon core — storage,
engine, system-event logger, per-database IPC server — **and**, when
a display context exists, mounts the fullscreen kiosk window over the
same core (the ADR 0015 hand-over with `UiSettings.kiosk = true`).
Without a display the appliance **falls back loudly** (Q3) to the
headless serving daemon. Three exits converge on one graceful tail
(Q2): a shutdown signal, `daemon stop` over IPC, and closing the
window. The serve→drain→shutdown choreography is **extracted once**
from `daemon_lifecycle` and shared by the daemon path, the appliance's
headless fallback, and the appliance's post-event-loop drain (KDF-3) —
zero duplicated shutdown choreography. The bare-invocation routing
flips `kiosk → BareRoute::KioskAppliance`, **TTY-independent**
(KDF-1); the daemon mode gate narrows to refusing `Desktop` only, so
the explicit `daemon` subcommand stays headless **always** (the
ssh/systemd maintenance entry, ADR 0015 #236's rule). **ADR 0016**
("Kiosk appliance mode") records the whole shape. No IPC protocol or
verb changes (KDF-6 — the fence stays unfired); no race hardening
beyond the documented tolerance (Q3).

**Non-goals** (other chunks' / out of scope): no `[kiosk]` config
section or fixed-board pin (K9), no `.desktop`/README launch table
(K4 — the #236 table rewrite is verified there), no ambient view or
rotation (K13), no renderer/backend selection (the spike; ADR 0015
decision 6 unchanged), no `catch_unwind` mount hardening (Q3 records
it as a review-time option only), no new IPC verbs, no metrics
(K8).

### Seams re-located at plan time (base `279d3b3`)

| Symbol | Where (plan-day lines) | Role in this chunk |
|---|---|---|
| `BareRoute` + `bare_invocation_route` | `crates/taskboard-app/src/ui.rs:64-121` | gains `KioskAppliance`; the kiosk arm of the match flips |
| routing pin test | `ui.rs:311-352` (`bare_invocation_routing_maps_modes_per_d4`; kiosk block 343-351) | updated: kiosk × tty × display → `KioskAppliance` |
| `display_available` / `display_env_available` | `ui.rs:82-99` | unchanged; the gate's input (KDF-2) |
| `resolve_ui_settings` | `ui.rs:130-159` (`kiosk: false` at 156 — chunk 2's D6) | consumed; the appliance flips the field to `true` after resolution |
| `run_desktop` | `ui.rs:192-245` | the watcher/quit pattern, hand-over order, and exit precedence the appliance mirrors; **unchanged** |
| `observe_scale_escape_hatch` | `ui.rs:167-178` (private) | made `pub` (D7) so the appliance's mount path observes the same hatch |
| `UI_COMMAND_CAPACITY`, `spawn_ui_command_forwarder` | `ui.rs:39, 46-56` | reused; a `ui_command_channel()` helper (D7) keeps the capacity const at home |
| `run`'s bare block | `crates/taskboard-app/src/main.rs:165-190` | gains the `KioskAppliance` routing arm (D1) |
| `Command::Gui` arm | `main.rs:92-105` (chunk 1) | untouched — the verb bypasses bare routing by design |
| `daemon_lifecycle` | `main.rs:262-354` | gate narrowed (D6); tail delegated (D3); head otherwise unchanged |
| the tail to extract | `main.rs:324-353` (select → drain → second-signal force exit → typed outcome) | becomes `serve_and_shutdown` + `drain_and_shutdown` (D3) |
| `IpcServerHandle` `stopped`/`join` | `crates/taskboard-app/src/ipc/server.rs:70-135` | gains the owned-receiver wait form (D5); `join` untouched |
| `bootstrap`/`App`/`shutdown_from_arc`/`wait_for_signal`/`spawn_system_event_logger` | `crates/taskboard-app/src/bootstrap.rs:128, 283-295→, 681, 758-770, 785-807` | reused as-is; `run_until_signal` (`:775`) untouched — see D3's placement note |
| engine-exit typed-path tests | `crates/taskboard-app/tests/ipc.rs:936-977` (IP19 stray-ref `CoreStillShared`; IP20 exclusive-ownership happy path) | must stay green through the extracted tail, unmodified |
| pty daemon precedent | `crates/taskboard-app/tests/cli_tty.rs:789-862` (`tty_explicit_daemon_stays_the_daemon_and_serves_standalone`) | the spawn → socket-poll-with-deadline → `daemon stop` → exit-0 pattern the new binary tests mirror (pipe-based, no pty) |
| kiosk-mode REPL test | `crates/taskboard-app/tests/repl.rs:609-647` (`r13_…`) | doc comment reworded (D6) — the REPL stays mode-agnostic, but a kiosk machine's bare entry is now the appliance |
| ADR 0015 + #236 amendment | `docs/adr/0015-slint-ui-architecture.org:106-147` | gains one forward-pointer sentence (D9) |
| mode statements | `docs/architecture.org:18-24` (exec summary), `:320-325` (routing); `docs/flow_model.org:317-334` (matrix + kiosk row) | the docs rows (D10) |

## 2. TDD order (red → green per step; house rules throughout)

Typed/structural outcomes only — no text assertions on log lines
(the fallback's `error!` text included), Display strings, or envelopes
beyond exit codes and parsed-JSON structure. No sleeps-as-waits: the
binary tests poll the socket predicate with a deadline (the
`tty_explicit_daemon` pattern) and assert terminal states. One
behavior, one test, at the lowest layer that owns it.

1. **Red 1 — routing flip** (`ui.rs` tests): update the kiosk block of
   `bare_invocation_routing_maps_modes_per_d4`: for **both TTY values
   × both display values**, `bare_invocation_route(AppMode::Kiosk, tty,
   display) == BareRoute::KioskAppliance` (TTY-independence is the
   ruled behavior — all four cells). Fails to compile (no variant) —
   the house red.
2. **Red 2 — the decision seam** (`ui.rs` tests):
   `kiosk_start(false) == KioskStart::Headless` and
   `kiosk_start(true) == KioskStart::Appliance` (D2). Compile-red.
3. **Green 1**: `BareRoute::KioskAppliance` + the routing arm (D1) +
   `KioskStart`/`kiosk_start` (D2). Unit tests green.
4. **Red 3 — the fallback lifecycle, binary tier** (`tests/it_cli.rs`,
   D11): bare `taskboard` under `mode = "kiosk"`, no TTY (pipes),
   display vars **removed from the child env**
   (`Command::env_remove("WAYLAND_DISPLAY")`/`("DISPLAY")` — child-env
   configuration, not `std::env` mutation) → the socket binds
   (deadline-poll), `taskboard daemon stop` exits 0, the child exits 0
   (not 124 — the `timeout` wrapper guards the hang). Proves end to
   end: kiosk routes to the appliance, the gate takes the headless
   branch, the fallback **serves**, and the shared tail drains and
   shuts down cleanly on the remote stop.
5. **Red 4 — the explicit daemon stays headless under kiosk** (D11):
   `taskboard daemon` (explicit verb) under `mode = "kiosk"`, no TTY,
   display vars removed → serves (socket binds), `daemon stop` exits
   0, child exits 0. Pins the gate narrowing (kiosk no longer
   refused).
6. **Red 5 — the surviving gate arm** (D11): `taskboard daemon` under
   `mode = "desktop"` → exit 1 (refused; distinguished from serve
   (hang→timeout 124) and clap usage (2) by exit code alone — the
   typed `eyre` bail is the existing behavior, now pinned while the
   gate is rewritten).
7. **Green 2**: implement D3–D8 in order — the tail extraction first
   (daemon path compiles + existing daemon/pty/ipc/repl tests green
   through it), then the gate narrowing, then `run_kiosk` + the
   routing arm + the `stopped_from` enabler + the ui.rs visibility
   helpers, then the binary tests go green.
8. **Green 3**: docs + bookkeeping (D8–D12): ADR 0016, the ADR 0015
   pointer, the architecture.org/flow_model.org rows, the repl.rs doc
   reword, the CHANGELOG line.
9. Full gate suite (§5).

Deliberately **not** tested (recorded, like chunk-1's D8 table — the
GUI branch is unverifiable headless-by-design, which is *why* the gate
exists): the `Appliance` arm's mount/watcher/event-loop choreography,
the `kiosk = true` hand-over at the mount, and the watcher's
three-exit select are pinned **by construction and review**: the
stage-3 checklist walks them (mount strictly after the `KioskStart::
Appliance` match arm; the select set has exactly three exits; the
abort-then-tail order; `UiSettings.kiosk` flipped before `mount`).
CI cannot execute slint's winit backend — a test that tried would pin
the KDF-2 panic (exit 101), a foreign-crash contract, not ours.

## 3. Decisions (each open question settled)

### D1 — `BareRoute::KioskAppliance` and the routing flip

```rust
/// `mode = "kiosk"`: the appliance lifecycle (ADR 0016) — the daemon
/// core plus the display-gated on-seat window. TTY-independent: a
/// seat has no operator TTY (`ssh -t` booting the appliance is
/// acceptable); the headless fallback is decided inside the
/// lifecycle, not by this routing.
KioskAppliance,
```

`bare_invocation_route`'s kiosk arm (currently the
`Kiosk | Daemon => DaemonLifecycle` fallthrough, `ui.rs:117-119`)
becomes its own arm returning `KioskAppliance`; the bare `Daemon`
fallthrough stays. `main.rs`'s bare block gains the routing arm
between `DesktopUi` and `DaemonLifecycle` (match order follows the
enum):

```rust
taskboard_app::ui::BareRoute::KioskAppliance => {
    // The appliance owns its own mode handling — the display gate
    // and the loud headless fallback live inside `run_kiosk`
    // (ADR 0016), not in this routing.
    return run_kiosk(config, cli.config, cli.json).await;
}
```

The `DaemonLifecycle` arm's stale comment ("`kiosk` is refused by the
daemon lifecycle's own mode gate") is corrected to name only the
Desktop refusal. The doc comments on `bare_invocation_route` and the
`BareRoute` enum gain the kiosk sentence. Nothing else routes on
`BareRoute` (grep-verified: `main.rs:172-189` is the only consumer
outside `ui.rs`'s tests); the explicit `daemon` verb and the `Gui`
verb both bypass this routing (chunk 1's design) and are untouched.

### D2 — the `KioskStart` decision seam

The fallback is asserted at the decision layer (the dispatch
acceptance's wording), in `ui.rs` beside the routing/display fns:

```rust
/// The kiosk appliance's start decision (Q3): whether a display
/// context gates the window mount. Decided *before* any
/// slint-touching call — without a display, slint 1.18.1 panics
/// during backend init (exit 101) instead of surfacing the typed
/// mount error, so the appliance falls back to the headless daemon
/// lifecycle, loudly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KioskStart {
    /// A display context exists: mount the fullscreen appliance
    /// window over the booted core.
    Appliance,
    /// No display: keep serving headless (the loud fallback —
    /// `tracing::error!` then the shared daemon tail).
    Headless,
}

/// Pure decision over the display facts ([`display_env_available`]
/// is the untested env glue, per the main-glue convention).
#[must_use]
pub fn kiosk_start(display: bool) -> KioskStart {
    if display { KioskStart::Appliance } else { KioskStart::Headless }
}
```

`run_kiosk` consumes `kiosk_start(display_env_available())` in its
only env-reading statement — the same pure-decision/ glue-split the
`#236` gate uses. One behavior, one test (§2 step 2); the fallback's
*loudness* is a log side effect, never asserted (no text assertions).

### D3 — the tail extraction: two main-private helpers, force exit rides the helper

`daemon_lifecycle` **keeps**: the mode gate (narrowed, D6), credential
store resolution, `bootstrap` + the typed boot-failure envelope,
`spawn_system_event_logger`, the `BasicSession` + `spawn_server`
wiring, and the serving log line. It **delegates** everything from
the first-exit wait to the final outcome — currently `main.rs:324-353`
— to:

```rust
/// The shared serve→drain→shutdown tail (KDF-3): wait for the first
/// exit — shutdown signal or `daemon stop` over IPC — then the
/// bounded drain (server join, graceful core shutdown via
/// `shutdown_from_arc`) with the second-signal force exit armed
/// throughout. A core still shared at the tail is the typed
/// F9-01 runtime failure.
async fn serve_and_shutdown(
    app: Arc<taskboard_app::bootstrap::App>,
    server: taskboard_app::ipc::server::IpcServerHandle,
) -> color_eyre::Result<()>;

/// The tail without the serve wait — the appliance's GUI branch
/// enters here after its event loop ended (window close already
/// ended the serve phase) and the watcher/forwarder were aborted.
async fn drain_and_shutdown(
    app: Arc<taskboard_app::bootstrap::App>,
    server: taskboard_app::ipc::server::IpcServerHandle,
) -> color_eyre::Result<()>;
```

`drain_and_shutdown` is verbatim today's `main.rs:332-353`: the
`shutdown_timeout` from `app.config.app.shutdown_timeout_secs`, the
`server.join(timeout)` + `shutdown_from_arc(app)` drain future, the
`select!` whose second-signal arm warns and **`std::process::exit(130)`**
— the force exit **rides the helper**, per the dispatch prompt — then
the `CoreStillShared → eyre` mapping and the `actor_aborted` warning.
`serve_and_shutdown` is today's `main.rs:324-331` select plus a
delegating call. Two helpers rather than one flag-parameterised fn:
the GUI branch genuinely needs the drain without the wait, and an
`already_stopped: bool` parameter would be a worse read than two
named orchestrators (each one level of abstraction).

**Placement (recorded, follows the implementation plan §3 K3 file
list literally)**: both helpers are private to `main.rs`, beside
`daemon_lifecycle` — *not* promoted into the `taskboard_app` library.
Reasons: (1) the impl-plan's file list is normative rank 1 (MD-7) and
names `main.rs` as the extraction's home; (2) the lib stays free of
`std::process::exit` — `bootstrap::run_until_signal`'s doc
("the second-signal force-exit lives in `main`") records that as a
deliberate engine-exit-round choice, and a lib-resident helper with
`exit(130)` would contradict it; (3) the tested surfaces of this
chunk (routing, `KioskStart`) are lib seams exactly as the acceptance
names, and lifecycle coverage rides the binary tier (ADR 0009's tier
split — the same tier `daemon_lifecycle` has always lived at). The
cost — the helpers are not in-process unit-testable — is the cost
`daemon_lifecycle` already carries; the existing daemon/pty/ipc tests
and the new binary tests (D11) cover the tail through both entries.

Consequence for stage 3: the rebase-review checks that IP19/IP20 and
the pty daemon test pass **unmodified** through the extracted tail
(the extraction must be behavior-preserving for the daemon path), and
that no second copy of the select/join/drain choreography exists
anywhere (the trap list's point).

### D4 — `run_kiosk`'s exact choreography

`async fn run_kiosk(config: AppConfig, config_path:
Option<std::path::PathBuf>, json: bool) -> color_eyre::Result<()>`,
private to `main.rs`, called only from the D1 routing arm:

1. **Head** (mirrors `daemon_lifecycle`'s, minus any kiosk gate — this
   *is* the kiosk lifecycle): `resolve_credential_store` →
   `bootstrap(...)` wrapped `Arc::new`, boot failures reported typed
   through `report(json, …)` + exit 1 (the session-boot contract) →
   `spawn_system_event_logger` → `BasicSession` (with `config_path`)
   → `spawn_server` (`Err` propagates as `Err(err.into())`, exactly as
   the daemon's) → the serving log line (`"kiosk appliance serving
   commands over ipc"`).
2. **Gate** (Q3/KDF-2): `match taskboard_app::ui::kiosk_start(
   taskboard_app::ui::display_env_available())`.
3. **`Headless` branch**: `tracing::error!("no display available;
   continuing as headless daemon")` (the impl plan's exact wording —
   UX text, never asserted) → `serve_and_shutdown(app, server).await`.
   Nothing else: the socket is already serving, so the ssh operator's
   `status`/`daemon stop` work identically to the daemon's.
4. **`Appliance` branch** (mirrors `run_desktop`'s hand-over order,
   `ui.rs:192-244`, reusing its now-`pub` enablers — D7):
   `observe_scale_escape_hatch()` (before anything slint-side, D13 of
   the desktop round) → `resolve_ui_settings(&config.ui)` → set
   `ui_settings.kiosk = true` (chunk 2's reserved consumer site) →
   `surface_keymap_findings(&findings)` → `ui_command_channel()` +
   `spawn_ui_command_forwarder(commands_rx, app.engine.clone())` →
   `taskboard_ui_slint::mount(app.engine.shared_state(),
   app.engine.subscribe(), app.system.subscribe(), commands_tx,
   ui_settings)`.
   - **Mount failure** (the display-var-outlives-its-server case,
     typed `UiMountError`): do **not** early-`?` — the IPC server is
     already spawned, so dropping it silently would leak the socket
     sidecar and the actor tasks. Instead: abort the forwarder, run
     `drain_and_shutdown(app, server).await` (ignoring its outcome —
     the mount error is the reported one), and return
     `Err(err.into())`.
   - **Watcher** (Q2/KDF-4 — three exits): spawn *after* a successful
     mount, owning an `IpcServerHandle::stopped_receiver()` clone
     (D5):

     ```rust
     let stop_rx = server.stopped_receiver();
     let watcher = tokio::spawn(async {
         tokio::select! {
             () = wait_for_signal() =>
                 tracing::info!("shutdown signal received"),
             () = IpcServerHandle::stopped_from(stop_rx) =>
                 tracing::info!("shutdown requested over ipc"),
         }
         taskboard_ui_slint::quit();
     });
     ```

     Window close is the third exit and needs no arm: it ends the
     event loop directly (`run(handle)` returns), exactly as in
     `run_desktop`. The signal arm consumes the *first* signal; the
     force-exit listener arms only later, inside
     `drain_and_shutdown` — the same first-then-second ordering
     `daemon_lifecycle`'s comment documents (`main.rs:320-323`).
   - **Event loop on the main thread**: `let run_result =
     taskboard_ui_slint::run(handle);` (blocking, as in
     `run_desktop` — the multi-thread runtime's workers drive every
     actor).
   - **Post-loop**: `watcher.abort(); forwarder.abort();` then
     `drain_and_shutdown(app, server).await?; run_result?; Ok(())`.
     Precedence: the shutdown-tail error (a still-shared core is the
     process-level failure) outranks the loop result, mirroring
     `run_desktop`'s shutdown-before-`run_result?` order. A signal
     arriving during the post-window drain hits the helper's
     force-exit arm — correct second-signal semantics in every entry
     combination (signal→signal, stop→signal, close→signal).
5. The gate→mount race stays **tolerated** (Q3): a display vanishing
   between the gate and the mount fails loud and typed *at mount*
   (KDF-2's verified `3ad16bb` behaviour); no `catch_unwind`, no
   re-check. ADR 0016 documents the tolerance (D8).

### D5 — the owned stop-wait: two small `IpcServerHandle` additions

A `tokio::spawn`ed watcher needs `'static` owned state, but
`IpcServerHandle::stopped(&self)` borrows the handle — and the handle
must survive on the main flow to feed `drain_and_shutdown`'s `join`
(it is not `Clone`: it owns the join halves). Handing the whole
handle through the watcher is a dead end (aborting a parked watcher
would drop the server un-joined, leaking the sidecar). Decision —
the watch channel *is* multiply-consumable by design, so expose it:

```rust
impl IpcServerHandle {
    /// The stop notification's receiver half: spawned watcher tasks
    /// own their wait without owning the handle (whose [`Self::join`]
    /// half is consumed by the shutdown tail) — the kiosk appliance's
    /// three-exit watcher.
    #[must_use]
    pub fn stopped_receiver(&self) -> watch::Receiver<bool> {
        self.stop_rx.clone()
    }

    /// Resolves once a shutdown was requested — the owned-receiver
    /// form of [`Self::stopped`].
    pub async fn stopped_from(mut rx: watch::Receiver<bool>) {
        while !*rx.borrow_and_update() {
            if rx.changed().await.is_err() { return; }
        }
    }

    pub async fn stopped(&self) {   // body becomes:
        Self::stopped_from(self.stop_rx.clone()).await
    }
}
```

`stopped`'s behaviour is unchanged (its body was exactly the loop,
already operating on a clone); `tests/ipc.rs:298`'s existing
`stopped()` coverage carries the loop. **Stage-3 trap pre-emption**
(the chunk's trap list says "the IPC protocol touched"): this is
handle-side plumbing, not a wire verb — KDF-6's fence is about the
protocol v1 verb set (`Exec`/`Status`/`Shutdown`/`Detach`/`Attach`),
which this chunk does not touch; `route_for_ipc` and the wire codec
are untouched. Recorded here so the review reads the accessor as
planned, not as a fence breach (chunk 1's D3 note pattern).

### D6 — the mode-gate narrowing (+ the two test-adjacent rewords)

`daemon_lifecycle`'s gate (`main.rs:270-276`) refuses `Desktop` only:

```rust
// The daemon's entry owns the `[app] mode` gate (the core is
// mode-agnostic): `Desktop` has no daemon-side frontend — its
// frontend is the GUI (`taskboard` bare or `taskboard gui`). `Kiosk`
// passes: the explicit `daemon` verb is the headless maintenance
// entry under a kiosk config too (ADR 0015 #236's unit rule; the
// bare kiosk entry is the appliance, ADR 0016).
if let mode @ taskboard_app::config::AppMode::Desktop = config.app.mode {
    tracing::error!(?mode, "desktop mode has no daemon-side frontend");
    color_eyre::eyre::bail!("app mode {mode:?} has no daemon-side frontend");
}
```

(The message text is UX, reworded from "not implemented yet" — never
asserted.) Knock-ons: `docs/architecture.org`'s "refused at the daemon
mode gate until it lands" sentence and `docs/flow_model.org`'s
"refuses Desktop/Kiosk" + matrix row update (D10); and
`tests/repl.rs`'s `r13_a_kiosk_mode_session_boots_and_serves` doc
comment ("the console a kiosk machine gets") is reworded to state the
truth post-flip: `run_session` is mode-agnostic (any mode's console),
while a kiosk machine's *bare* entry is the appliance — the test body
is unchanged (it pins the REPL's mode-agnosticism, which still holds).

### D7 — the ui.rs enablers (three small visibility/refactor moves)

`run_kiosk` lives in the binary; three ui.rs items open to it, each
one the established "pub because the binary's wiring is the caller"
pattern (`display_env_available`'s own doc):

1. `observe_scale_escape_hatch` → `pub` (doc gains the appliance
   caller). The alternative — skipping the observation on the kiosk
   path — would blind kiosk operators to the one DPI escape hatch
   they can act on over ssh.
2. `ui_command_channel() -> (mpsc::Sender<StateCommand>,
   mpsc::Receiver<StateCommand>)` — new `pub` fn wrapping
   `mpsc::channel(UI_COMMAND_CAPACITY)`; the capacity const stays
   `pub(crate)` (its knowledge stays home; `run_desktop` switches to
   the helper too — one construction site again).
3. `surface_keymap_findings(&[keymap::Finding])` — the Q15 warn-loop
   (`ui.rs:199-207`) extracted `pub`, used by `run_desktop` and
   `run_kiosk` (the findings surfacing is part of the hand-over
   contract, not per-lifecycle prose).

No behavior change on the desktop path beyond the extraction being
behavior-preserving (`run_desktop`'s diff is mechanical).

### D8 — ADR 0016 full outline (`docs/adr/0016-kiosk-appliance-mode.org`)

Org format, house voice, `#+title: ADR 0016: Kiosk Appliance Mode`,
`*Date:* 2026-10-19`, `*Status:* Accepted (implemented by the kiosk
wave chunks 1–3; deployment profile, ambient view, and on-device
verdicts are later waves)`, `*Depends on:* ADR 0008 (bootstrap &
modes), ADR 0009 as amended 2026-10-19 (the `gui` verb), ADR 0012
(daemon IPC), ADR 0015 incl. the 2026-10-18 #236 amendment`. Sections:

1. **Context** — the kiosk requirement (#237): unattended seat, ssh
   operator control, offline resilience; the v0.1.0 state (remote CLI
   control solved by ADR 0012; `mode = "kiosk"` refused at the gate);
   the 2026-10-16 design round's Option A accepted (Q1) over the
   attachable-GUI (B) and web-dashboard (C) alternatives, with the
   comparison row (no new IPC surface / same-process GUI / native
   renderer-selectable window / effort S).
2. **Decision — routing**: the bare-invocation table with the kiosk
   row (`kiosk | any TTY | any display | appliance`), TTY-independence
   and its rationale (a seat has no operator TTY; `ssh -t` boots the
   appliance acceptably); the explicit `daemon` verb stays headless
   always (maintenance + the #236 unit rule); `taskboard gui` is the
   desktop frontend regardless of mode (ADR 0009 amendment); the
   daemon mode gate refuses `Desktop` only.
3. **Decision — the appliance lifecycle**: one process = the daemon
   core (storage, engine, system-event logger, per-database IPC
   server) + the ADR 0015 hand-over with `UiSettings.kiosk = true`
   (the chunk-2 flag's consumer); **close-exits (Q2)**: closing the
   window ends the appliance through the same graceful drain +
   shutdown as a signal or `daemon stop` (richer semantics deferred
   until a need appears); the shared serve→drain→shutdown tail with
   the second-signal force exit — one choreography, three entries.
4. **Decision — the display gate + loud fallback (Q3)**: the gate
   runs *before* any slint-touching call because the verified
   evidence (2026-10-16, release artifact) is that slint 1.18.1
   **panics** during backend init without a display (exit 101, panic
   in `slint-1.18.1/lib.rs:784`) instead of surfacing the typed
   `UiMountError` — hence `KioskStart` decides on
   `WAYLAND_DISPLAY`/`DISPLAY` (set-and-non-empty) first; the
   `Headless` arm logs `error!` and serves the daemon tail (the same
   binary serves the seat and the ssh session). **Race tolerance**:
   a display vanishing between gate and mount — and a display var
   outliving its server — fail loud and typed *at mount*; tolerated,
   not hardened (`catch_unwind` recorded as a review-time option
   only), mirroring ADR 0015 #236's identical tolerance.
5. **Decision — IPC on the seat (KDF-6)**: the appliance serves the
   socket exactly as the daemon; `daemon stop` and `status` are the
   operator surface over ssh; **no kiosk-management verb exists** —
   the protocol is unchanged (the F16-28 extraction fence stays
   unfired).
6. **Decision — the window profile**: chunk 2's D4 caveat text
   **verbatim** (fullscreen via `slint::Window::set_fullscreen(true)`
   at mount; no public decoration API in 1.18.1; decorations
   suppressed by construction on winit — `set_decorations(!no_frame
   || fullscreen)`; backend-switch semantics are the spike's).
7. **Consequences** — a GUI crash takes the core with it (whole-
   appliance restart under the K9 `Restart=always` system unit);
   the GUI must run on the core's machine (B is the off-box path,
   sequenced after C per Q7); the renderer/backend choice and the
   on-device budget verdicts ride the phase-B spike; the ambient
   view and rotation are K13; `#237` completes with K4's packaging.

### D9 — the ADR 0015 forward pointer

One sentence appended to ADR 0015's #236 amendment (after its
"Interplay with the kiosk design" paragraph): the kiosk row of that
amendment's routing table is superseded by ADR 0016 (bare kiosk now
boots the appliance; the mode gate refuses `Desktop` only). The dated
amendment text itself is history and is not rewritten — the pointer
keeps a reader of 0015 from acting on the stale row.

### D10 — the docs rows

- `docs/architecture.org` exec summary (`:18-24`): the `Kiosk`
  sentence becomes shipped-at-appliance-level — "*Kiosk* — the
  appliance (ADR 0016): the daemon core plus the display-gated
  fullscreen window, with a loud headless fallback; the passive
  ambient view is the later kiosk wave". The two-mode vision list's
  "(vision)" marker on kiosk stays for the *ambient display*
  ambition, with the appliance named as shipped.
- `docs/architecture.org` routing statement (`:320-325`): the
  parenthetical gains "bare `kiosk` boots the appliance (display-gated
  with a loud headless fallback, TTY-independent — ADR 0016)".
- `docs/flow_model.org` matrix paragraph (`:317-325`): the gate
  sentence ("refuses Desktop/Kiosk") → "refuses Desktop (kiosk passes
  — the explicit `daemon` verb is the headless maintenance entry)";
  the bare-routing sentence gains the kiosk→appliance cell (and its
  pre-#236 display-blind phasing is refreshed to the current
  display-split wording while there).
- `docs/flow_model.org` matrix row (`:334`): `| Kiosk appliance
  (bare kiosk; headless fallback = daemon row) | ✓ | ✓ | backend |
  ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — | — | … |` — ENG/STO/SYN/NTF/SYSLOG/
  IPCA+IPCD from the daemon row (the appliance boots the identical
  head), UIF/UI/EXITW from the desktop row (the hand-over + the
  three-exit watcher), RPL/EDT absent. The absent-edges cell is
  **derived at implementation time from `run_kiosk`'s actual wiring
  against the edge table**, not transcribed here — stage 2 verifies
  each ✓ against what `run_kiosk` spawns and recomputes the cell;
  note for that pass: unlike the plain daemon, the appliance's UI
  *subscribes to the system broadcast*, so the daemon row's
  "E-16/17 SIG may have zero subscribers" caveat does not carry over
  as-is.

### D11 — the binary test trio (`tests/it_cli.rs`)

All three mirror `tty_explicit_daemon`'s mechanics minus the pty
(pipes = the no-TTY fact itself), each under
`timeout 30`, each with its own tempdir + config, each removing
`WAYLAND_DISPLAY`/`DISPLAY` from the **child's** env
(`Command::env_remove` — child configuration, safe, no
`std::env` mutation):

1. `kiosk_bare_without_display_serves_headless_and_stops` — config
   `mode = "kiosk"`, bare invocation; socket binds (deadline poll),
   `daemon stop` → 0, child exits 0 (≠ 124).
2. `explicit_daemon_under_kiosk_mode_stays_headless` — same config,
   explicit `daemon`; same assertions (the gate narrowing pin).
3. `explicit_daemon_under_desktop_mode_is_refused` — config `mode =
   "desktop"`, explicit `daemon`; exit 1 (the surviving gate arm;
   exit-code-only assertion — see §2 step 6).

The display-gated GUI branch is not binary-testable on CI (the gate
exists precisely because mounting headless panics); §2's
by-construction table carries that duty.

### D12 — CHANGELOG (Unreleased → Added)

```markdown
- `mode = "kiosk"` boots the kiosk appliance (ADR 0016): the daemon
  core plus the display-gated fullscreen window — TTY-independent
  bare routing, a loud headless fallback that keeps serving over
  IPC, window-close/signal/`daemon stop` all converging on one
  graceful shutdown; the explicit `daemon` verb stays headless under
  any mode (#237).
```

### D13 — acceptance mapping (dispatch §4, chunk 3)

| Acceptance clause | This plan |
|---|---|
| route mapping tests, kiosk → appliance, both TTY values | D1 + §2 step 1 (all four tty×display cells) |
| fallback asserted at the decision layer (`KioskStart`-shaped seam) | D2 + §2 step 2 (+ the D11 binary lifecycle pin) |
| the daemon path's existing tests green through the shared tail | D3 (behavior-preserving extraction; IP19/IP20, the pty daemon test, repl suite unmodified) |
| gate refuses `Desktop` only; explicit `daemon` headless always | D6 (+ D11 tests 2–3) |
| three-exit watcher | D4 (select set: signal ∥ `stopped_from` ∥ loop-return) + D5 |
| gate-before-mount; race tolerated not fixed | D4 steps 2/5; ADR 0016 §4 (D8) |
| loud fallback (`error!` + shared tail) | D4 step 3; D2 |
| ADR 0016 incl. chunk-2 caveat | D8 (embeds chunk-2 D4 verbatim) |
| architecture.org + flow-model rows | D10 |
| no IPC verb change / no config section / no packaging | non-goals §1; D5 pre-empts the trap reading |
| annotate #237 | stage 3 (MD-4), not this stage |

## 4. Files & estimated diff

| File | Change | ~Lines |
|---|---|---|
| `crates/taskboard-app/src/ui.rs` | D1 variant+arm+docs, D2 `KioskStart`, D7 enablers, routing-test update, `kiosk_start` tests | +95/−12 |
| `crates/taskboard-app/src/main.rs` | D3 two helpers (tail moves), D4 `run_kiosk` + routing arm, D6 gate | +165/−35 |
| `crates/taskboard-app/src/ipc/server.rs` | D5 `stopped_receiver` + `stopped_from`, `stopped` delegates | +18/−2 |
| `crates/taskboard-app/tests/it_cli.rs` | D11 binary trio | +115 |
| `crates/taskboard-app/tests/repl.rs` | r13 doc reword | +3/−3 |
| `docs/adr/0016-kiosk-appliance-mode.org` | D8 (new) | +95 |
| `docs/adr/0015-slint-ui-architecture.org` | D9 pointer sentence | +3 |
| `docs/architecture.org` | D10 (two spots) | +10/−6 |
| `docs/flow_model.org` | D10 (sentence + matrix row) | +8/−4 |
| `CHANGELOG.md` | D12 | +6 |

**Total ≈ +520/−65 over 10 files** — house scale **M**, as
dispatch-sized. No new dependencies, no `Cargo.toml` changes; SPDX
headers: all touched `.rs` files exist already (`taskboard-app`
AGPL-3.0-only, `#![forbid(unsafe_code)]` — nothing here touches
that).

## 5. Gates & handoff

Full AGENTS §10 suite, green, in order: `cargo fmt --all --
--check`; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo nextest run --workspace`; `cargo test --doc --workspace`;
`cargo deny check`; the CI-identical three-command llvm-cov sequence
(+ `scripts/coverage-quarantine-profraw.sh` + `lcov.info` + the
trailing `rm -rf target/llvm-cov-target`). Expected hot spots: the
binary trio's runtime (three booted cores + sockets ≈ the pty
daemon test's class) and nothing else. Commits standalone
(rebase-merge hygiene): stage 2 lands the work as 2–3 commits —
`feat(app): the kiosk appliance lifecycle` (code + tests),
`docs(adr): kiosk appliance mode (ADR 0016)` (ADR 0016 + the 0015
pointer), and the docs rows either folded into the ADR commit or a
`docs(arch): kiosk appliance mode rows` — never WIP noise. Stage 2
does not push or touch the tracker (MD-4: the one #237 annotation is
stage 3's, posted when the PR opens — this chunk is the substantive
kiosk land). Stage 3: the plain rebase dance (nothing stacked — §0),
then the dispatch §4 review checklist with this plan's
by-construction table (§2) as the GUI-branch walk.

## 6. Risks

1. **The D5 accessor vs. the "IPC touched" trap** — the loudest
   coordination risk (chunk 1's D3 was the same shape): stage 3 is
   primed to flag any `ipc/` diff. Mitigated: D5 records the
   handle-side/wire-side distinction and KDF-6's actual fence (the
   verb set); the diff touches no verb, codec, or `route_for_ipc`. If
   the user rules D5 wrong at review, the fallback is restructuring
   `spawn_server` to return a separate stop-watcher half — a
   signature change confined to two call sites.
2. **The GUI branch is unverifiable on CI** — inherent (KDF-2's
   panic is why the gate exists). Mitigated: the by-construction
   table (§2) is stage 3's walk; the mount seam itself is
   chunk-2-tested; the watcher's quit pattern is `run_desktop`'s,
   review-pinned since the desktop round.
3. **Binary-test flake surface** — the trio spawns real processes
   with socket polls under `timeout 30`; the class is the existing
   pty daemon test's (no wiremock, no network, no pty here). The
   socket-poll deadline assertions follow the house no-sleeps-wait
   rule; a wedged child surfaces as 124, not a hang.
4. **Extraction regressions in the daemon path** — the tail move is
   mechanical but touches the F9-01 exit paths. Mitigated: IP19/IP20
   + the pty daemon test must pass unmodified (§2 step 7's ordering:
   extraction lands and is gate-checked *before* `run_kiosk` is
   added).
5. **Env leakage on dev machines** — a developer's `DISPLAY` would
   flip test 1's child to the GUI branch; pre-empted by
   `env_remove` on the child (D11). The *plan-time* repo greps found
   no other display-var readers outside `display_env_available` and
   the tests that stub it.
6. **Doc-row drift** — `flow_model.org`'s cited line anchors are
   plan-day (some already stale); stage 2 refreshes the *statements*
   against the shipped code and cites symbols, per the house anchor
   discipline (MD-8).

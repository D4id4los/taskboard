# [Plan] GUI Review Round 3, Chunk 7 — Reminder OS-Delivery Plumbing

- **Date**: 2026-10-26 (planned 2026-10-28)
- **Status**: PROPOSED — stage-1 plan for chunk 7 of the round-3
  dispatch. Branch `fix/ui-r3-reminder-delivery`, cut from fetched
  `origin/main` (d956ca32). Independent chunk — no predecessor gate
  (dispatch §1: "5 and 7 independent"); precondition verified: the
  round-3 artefact plans are on `origin/main`.
  `scripts/worktree-doctor.sh` run from the main checkout before
  worktree creation: PASS (240 G free, floor 25 G; three sibling
  worktrees live and kept).
- **Origin** (normative): design round
  `2026-10-26-gui-review-round3-plan.md` §3.7 (D30–D32), §1 finding
  R3-F30, §4 testing (delivery bullets), §5 inventory; the DR-3
  fence (`2026-10-26-reminders-surface-pair-design-round-plan.md` §1
  end + §3.1 "Fence" + §2 non-goals); dispatch plan §1/§8 (acceptance
  a–e, prompts 7.1–7.3).
- **Scope guard**: `taskboard-app` (`notifications.rs`,
  `bootstrap.rs`, `ui.rs`) + `taskboard-ui-slint` (`lib.rs` mount/
  repaint driver, `adapter.rs`, `models.rs`/`presentation.rs` chip
  push, `status-bar.slint`). `taskboard-domain`,
  `taskboard-state`, storage, sync: **empty diff** (acceptance e).
  Fenced territories: DR-3 chunk 1 owns the settlement re-time
  (**already landed on main** — `prune_settled_alarms` keys on
  delivered ∧ dismissed at `reminders.rs:678`, the in-app half of
  R3-F30 is healed); DR-3 Part D owns the timers chip and today
  sections (its chip properties are reused as a *pattern* only —
  no DR-3-owned property is edited).
- **All `file:line` anchors are this worktree at d956ca32.** The
  dispatch prompt's older anchors ("bootstrap wiring 782-786/857-871",
  "actor.rs:373") drifted with upstream merges; every anchor below
  was re-derived and read, not carried over.

## 0. The seam map (facts this plan is built on)

- **The port** (`notifications.rs:38-62`): `NotificationSink::
  deliver/withdraw`, boxed-future, fallible via the frozen unit
  `NotifyError` (`:30-32`) — deliberately cause-less; the backend
  cause is logged, never carried (`DesktopSink::deliver`
  `:186-196`).
- **The sinks**: `LogSink` (`:67`, one info line, cannot fail) and
  `DesktopSink` (`:136`, tracked handles capped `TRACK_CAP=256`,
  withdraw best-effort `:201-219`). `DesktopSink::deliver` renders
  the pure `toast_payload` (`:251`) and calls
  `notify_rust::Notification::show_async()` — under the workspace's
  `z-with-tokio` feature this opens a **fresh
  `zbus::Connection::session()` per toast**
  (notify-rust 4.18.2 `xdg/zbus_rs.rs:60-72`), so no connection
  outlives a delivery and a restarted notification daemon self-heals
  on the next fire.
- **The consumer**: `spawn_notifier` (`:297-332`) drains the engine's
  `NotifierMessage` mpsc; a failed deliver logs at warn
  (`:307-311`) and continues; a failed withdraw logs at debug
  (`:320-326`). Existing tests pin the two-variant routing and the
  survive-a-failure contract (`:411-465`, `:491-551`).
- **Resolution & wiring**: `resolve_sink` (`:283-290`, the D-R13
  matrix; explicit `desktop` is honored under daemon/kiosk);
  `bootstrap.rs`: `resolve_notifier_sink` (`:799-805`, one `Arc`),
  the channel (`:875`), the consumer spawned **before the engine**
  (`:889` with the ordering comment). The `App` carries the join
  handle; no health surface exists anywhere.
- **Config**: `[reminders]` (`config.rs:296-323`): `enabled`
  (default true), `tick_secs` (default 30), `default_before`,
  `sink` (default `auto`). The engine arms its tick on
  `reminder_cfg.enabled || day_roll.enabled`
  (`taskboard-state/src/actor.rs:381`); `reminder_engine_config`
  (`config.rs:1222-1239`) maps the table onto the scheduler knobs.
  **No config changes in this chunk** (D-R13: no knob beyond the
  recorded default).
- **The in-app fallback surface (read-only; DR-3's half)**:
  domain `active_notifications` (`reminders.rs:863`) derives fired ∧
  ¬dismissed rows at read; `presentation.rs:403 notification_rows`
  filters the anchor card; `models.rs:1190 push_status_bar` /
  `:492 push_notifications` push them. Under DR-3 chunk 1's landed
  semantics a delivered-but-un-dismissed occurrence **stays visible
  in-app** — the always-on fallback surface this plan relies on.
- **The UI intake**: `mount` (`ui-slint/src/lib.rs:229-236`) takes
  the state `ArcSwap`, the `EngineSignal` broadcast, the
  `SystemEvent` broadcast, the command/request senders, `UiSettings`.
  The repaint driver (`lib.rs:398-450`, cadence
  `adapter::REPAINT_CADENCE_MS`) drains both receivers
  non-blocking and refreshes on any hit — the "data changes repaint
  via the signal, the passage of time via the timer" doctrine.
  Adding a `SystemEvent` variant for delivery health is a **domain
  diff — forbidden**; a `SystemEvent` rides the engine's
  `broadcast::WeakSender` (`state/src/actor.rs:1109-1117`), the
  wrong actor's lane anyway. The health channel is therefore a new
  **app-owned `tokio::sync::watch`** handed over at mount, drained
  by the same repaint driver.

## 1. The live probe — method, evidence, verdict (acceptance a)

All commands were run 2026-10-28 on this machine (xmgneo-kubuntu,
Kubuntu 26.04 "resolute", KDE Plasma 6.6.4, Wayland, single 2560×1440
output). Raw captures in `/tmp/ui-r3-probe/` (session-local scratch;
the durable evidence is the transcript below — artefact mandate).

**(a) Session environment.** The agent shell and the *actual GUI
session* (read from the running `plasmashell`, pid 3923,
`/proc/3923/environ`) carry identical values:
`XDG_CURRENT_DESKTOP=KDE`, `XDG_SESSION_TYPE=wayland`,
`DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus`. The
"launched without the session bus" class cannot reproduce here.

**(b) Bare toasts.**
- Baseline `notify-send 0.8.8` ("baseline toast, tight capture"):
  exit 0; **visibly rendered** bottom-right (screenshot
  `baseline-t1.png`, cropped read confirms the Plasma toast).
- `notify-probe` — a 20-line cargo binary in `/tmp/ui-r3-probe/`
  against **the workspace's exact stack** (`notify-rust 4.18.2,
  default-features = false, features = ["z-with-tokio"]`):
  `show_async()` → `Ok(handle id 777)` (the daemon's Notify reply —
  a server-side acceptance receipt) and the toast **visibly
  rendered** (`notify-rust-toast.png`).
- Env-scrub variants: with `DBUS_SESSION_BUS_ADDRESS` **and**
  `XDG_RUNTIME_DIR` unset, the probe still delivered `Ok(id 790)` —
  zbus's internal discovery cascade (`/run/user/$uid/bus`) covers a
  fully scrubbed environment. A second rapid-fire toast failed only
  with `org.freedesktop.Notifications.Error.ExcessNotificationGeneration`
  — **Plasma rate-limits similar toasts** (recorded as a risk, §8).

**(c) The portal question.** `org.freedesktop.portal.Desktop` is
live (xdg-desktop-portal pid 4263, backend xdg-desktop-portal-kde
pid 4699) and `/org/freedesktop/portal/Notifications` resolves — but
the FDO name `org.freedesktop.Notifications` is **natively owned by
plasmashell** (`busctl --user status` → PID 3923). The portal path is
present yet unnecessary for a non-sandboxed client on this desktop.

**(d) Forced delivery through the real binary** (current `main`
build; isolated config `[app] mode='daemon'`,
`[reminders] tick_secs=1, sink='desktop'`, `[storage]
db_path=/tmp/.../taskboard.db`; `TASKBOARD_CONFIG` env; the user's
real database untouched):
- `taskboard timers add probe2 --in 15s`, then `taskboard daemon`:
  the engine recorded the delivery in `reminder_deliveries`
  (occurrence 22:58:28.35 → delivered_at 22:58:28.39, the immediate
  first tick) and the **toast visibly rendered** bottom-right
  ("taskboard · probe2 · due now", screenshot `app-toast-t1.png`
  at fire+1.5 s). A **successful `DesktopSink` delivery logs
  nothing** — success is invisible in the trail.
- Forced failure (`DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent/...`):
  the tick fired, `DesktopSink::deliver` logged the real cause at
  warn (`desktop toast delivery failed error=Failed to connect to
  address 'unix:path=/nonexistent/session-bus': No such file or
  directory (os error 2)`), the consumer logged a second, cause-less
  warn (`notification delivery failed err=NotifyError`) — **and the
  occurrence was still marked delivered** in `reminder_deliveries`.
  No other surface. Exactly the warn-and-swallow D30 names, plus a
  cause lost at the port boundary.

**Verdict.** *No transport, environment, or portal defect is
reproducible on this machine on current `main`*: the full lane
engine tick → `due_firings` → delivered-mark → `NotifierMessage`
mpsc → `DesktopSink` → zbus → plasmashell → **rendered toast**
works, from the session environment, from a fully scrubbed one, and
from a wrong-address one (which fails typed and loudly in the log).
The user's R3-F30 OS-half observation is explained by the recorded
history: the round-3 review (2026-10-26) predated or straddled the
2026-10-23 desktop-toast wiring (the checked-in `target/debug`
binary was built Oct 23 00:27, *before* that day's
`notifications.rs` commit ec17e82c 22:54), and its in-app half was
the DR-3 prune bug, fixed on `main` since. What genuinely ships from
this verdict is the **observability gap**: silent success, a
cause-less second failure line, no user-visible health surface, and
no boot-time diagnosis of a toasts-hostile desktop.

## 2. D31 settlement — the chosen fix and the rejected alternative

**Chosen: delivery observability** — typed attempt/outcome events
through a daemon-independent seam (D31's LogSink-testable seam), a
health snapshot surfaced to the status bar, a boot-time server
probe, and a recorded retry/fallback posture. This is the "fix per
verdict": the verdict names no backend to swap and no environment to
repair, so the deliverable is the machinery that makes the *next*
delivery failure on any machine self-diagnosing instead of silent.

**Rejected: the portal backend** (`org.freedesktop.portal.Notifications`).
Evidence: the FDO name is native to both release-target desktops
(plasmashell owns it here; GNOME ships a native implementor);
notify-rust speaks FDO directly and is proven end-to-end above; a
portal client adds an xdg-desktop-portal availability assumption,
its own request/response handle model (the portal Notify returns a
request handle, not a closable notification id), and a second
withdrawal semantic for zero functional gain. Revisit only if a
sandboxed distribution target (Flatpak/Snap) becomes a release
channel (#444-adjacent).

**Rejected: an env/transport repair in code.** Evidence: zbus's
discovery cascade already delivers from fully scrubbed environments
(probe b); an address override exists via the standard
`DBUS_SESSION_BUS_ADDRESS` for the exotic layouts; the only observed
failure (wrong address) already fails typed with the cause in the
log — the gap was *who ever reads that log*, which §3–§5 close.

**Recorded posture (D31's retry/fallback, D-R13 alignment):** one
attempt per fire, never retried, never fallback-sent to `LogSink`
(an explicit `desktop` choice failing must surface as a failure, not
silently degrade); the in-app notification row (delivered ∧
¬dismissed) is the always-on fallback surface by the landed R3
semantics; per-toast fresh connections make a daemon or bus outage
self-heal on the next fire; the delivered-mark is **not** re-timed
on sink failure — it is the local receipt guarding the in-app
surface, and conditioning it on the sink outcome would be a
domain/state change (forbidden here, and DR-3's territory).

## 3. The delivery seam (acceptance c — LogSink-testable)

All in `taskboard-app/src/notifications.rs` — the module already
owns the port; the seam stays beside it.

```rust
/// One delivery-pipeline observation (app-owned, ui-safe data).
pub(crate) enum DeliveryEvent {
    Attempted { reminder: ReminderRef, occurrence: DateTime<Utc> },
    Delivered { reminder: ReminderRef, occurrence: DateTime<Utc> },
    Failed    { reminder: ReminderRef, occurrence: DateTime<Utc> },
    Withdrawn { reminder: ReminderRef, occurrence: DateTime<Utc> },
    WithdrawFailed { reminder: ReminderRef, occurrence: DateTime<Utc> },
}

pub(crate) trait DeliveryObserver: Send + Sync + std::fmt::Debug {
    fn on_event(&self, event: DeliveryEvent);
}
```

- **Emission point: the consumer, not the sinks.** `spawn_notifier`
  gains `observer: Arc<dyn DeliveryObserver>` and emits
  Attempted→{Delivered|Failed} around every `sink.deliver`,
  Withdrawn/WithdrawFailed around every `sink.withdraw`. The sinks
  stay dumb (LogSink cannot fail, DesktopSink keeps its cause log +
  `NotifyError`); every implementor of `NotificationSink` — the
  CI fakes included — gets the same event stream with no daemon.
- **The production observer**: a `HealthTracker` owning a
  `tokio::sync::watch::Sender<DeliveryHealth>`:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeliveryHealth {
    pub last_attempt: Option<DateTime<Utc>>,
    pub last_success: Option<DateTime<Utc>>,
    pub last_failure: Option<DateTime<Utc>>,
    pub consecutive_failures: u32,
}
impl DeliveryHealth {
    /// The chip's visibility rule: degraded iff the last attempt
    /// failed (a later success clears it — litmus-safe: strictly
    /// better delivery can only clear the surface).
    pub fn degraded(&self) -> bool {
        matches!(self.last_failure, Some(f) if self.last_success.is_none_or(|s| f > s))
    }
}
```

  (timestamps from `Utc::now` at the consumer — delivery timing is
  wall-clock telemetry, not engine logic; no injected clock needed.
  The timestamps are *not* asserted in tests, per the deterministic
  rule — tests assert the event sequence and the `degraded`
  transitions.)
- **Existing tests keep their signatures**: `spawn_notifier`'s new
  parameter gets a `no_op()` observer const; the two routing tests
  compile unchanged. New pins (§7) drive a collecting observer.
- **Failure-cause discipline**: the real cause stays where it is
  readable — `DesktopSink`'s warn (`:190-194`) keeps the backend
  string. The consumer's second warn is **dropped** (the event is
  the record now); the `HealthTracker` carries no cause string (no
  fake classification of `notify_rust::Error`, which is opaque under
  zbus) — the chip says *degraded*, the log says *why*.

## 4. Boot-time sink diagnostics

In `bootstrap.rs`, after `resolve_notifier_sink` resolves
`SinkKind::Desktop`, one async probe before the actor graph spawns:

```rust
/// The boot-time toast diagnosis: names the notification server, or
/// says plainly that desktop toasts will not work (and that the
/// in-app surface remains).
async fn diagnose_desktop_sink(probe: &dyn ServerProbe) -> Option<SinkDiagnosis>
pub(crate) trait ServerProbe: Send + Sync {
    async fn server_info(&self) -> Result<ServerInfo, ProbeError>;
}
```

- Production impl wraps `notify_rust::get_server_information()`
  (async under `z-with-tokio`, verified present in the vendored
  4.18.2 sources, `xdg/zbus_rs.rs:104-117`).
- Outcomes: `Ok(info)` → `tracing::info!(name, vendor, version,
  "desktop toasts: notification server resolved")`;
  `Err` → `tracing::warn!(cause, "desktop toasts requested but no
  notification daemon is reachable; reminders surface in-app only")`.
  Log-only, no boot failure, no config knob — the D-R13 resolution
  stands, this only makes it *say* what it did. The trait keeps the
  unit tests bus-free (a stub probe, no zbus).
- Not re-run at delivery time: the per-toast fresh connection makes
  mid-run diagnosis redundant; the runtime truth is the health
  snapshot (§3), which updates on every attempt.

## 5. The user-visible failure surface (acceptance c)

**The status-bar delivery chip** (new, this chunk's own D30
territory — "delivery to the OS *and visibility of delivery*"; the
DR-3-owned timers/tracking chips are copied as a pattern, not
edited):

- **Data path**: `bootstrap_with_modules` creates the watch channel,
  hands the `HealthTracker` to `spawn_notifier`, and stores the
  `watch::Receiver<DeliveryHealth>` on `App` (`ui.rs` reads it).
  `mount` (`lib.rs:229`) gains the receiver as a new parameter (the
  existing channel-half hand-over precedent; both UI lifecycles —
  desktop and kiosk appliance — pass it). The adapter holds it; the
  repaint driver (`lib.rs:398-450`) drains it beside the two
  broadcasts (`has_changed()` → `borrow_and_update()` → `dirty`),
  so a failure repaints within one `REPAINT_CADENCE_MS` tick with no
  new thread or tokio task in the UI.
- **Mapping** (a total, unit-tested fn in `presentation.rs`, the
  `settings_status.rs` totality discipline): `degraded()` → chip
  visible with a short sentence (presentation constant; text never
  asserted — visibility + non-emptiness pins only); healthy → chip
  absent. Never auto-clears on a timer (unlike the settings status
  line): the chip clears when the *next attempt succeeds* — a state
  of the world, not a message.
- **Kiosk**: `auto` resolves `LogSink` there, which cannot fail, so
  the chip stays absent in practice; the mapping itself is
  mode-blind (no new kiosk gate — the sink matrix already decided).
- **Daemon with no UI**: log-only is the honest ceiling — a
  cross-process health lane would be new IPC (a domain
  `SystemEvent` variant is forbidden by acceptance e). Recorded as a
  known limitation in the fence comment (§6).
- The today-view notification rows need **no change**: they are the
  fallback surface already (DR-3's half, landed).

## 6. The DR-3 fence comments (acceptance d)

Two code comments, one issue note — the division of R3-F30:

1. `notifications.rs`, on the `DesktopSink` impl doc: *settlement —
   the delivered-vs-dismissed lifecycle, prune/compaction timing,
   and the in-app surface's persistence — is DR-3 chunk 1's
   territory (landed on `main`); this module owns OS delivery and
   delivery visibility only. Neither round re-times the other's
   fix; the delivered-mark is intentionally unconditional on the
   sink outcome (DR-1 R3: the mark is the local receipt, the in-app
   row is the fallback surface).* Cross-ref: #474.
2. `bootstrap.rs`, on `resolve_notifier_sink`: one line — the D-R13
   matrix is the resolution authority; DR-3 chunk 1 owns settlement;
   the chunk-7 additions (observer, probe, health) hang off this
   glue without altering resolution.
3. Issue #474 annotation (stage 3, dispatch §9): the delivery half
   landed via this chunk; settlement stays DR-3 chunk 1.

The domain's own fence (DR-3 §3.1) required *chunk 1's* issues to
note the division — already satisfied on the DR-3 side; this chunk
adds no domain diff, hence no domain comment.

## 7. Test catalogue (TDD-first; lowest layer per behaviour)

**Seam pins (`notifications.rs` tests; daemon-free, LogSink + fakes):**

| # | Test | Pins |
|---|---|---|
| T1 | `the_consumer_emits_attempt_then_delivered_for_a_landng_fire` | LogSink fire → events `[Attempted, Delivered]` in order, one per `NotifierMessage` (collecting observer via mpsc; join the consumer, no sleeps) |
| T2 | `the_consumer_emits_attempt_then_failed_for_a_refusing_sink` | the existing `FailingSink` shape + observer → first fire `[Attempted, Failed]`, second `[Attempted, Delivered]`; consumer survives (extends, not duplicates, `failed_delivery_does_not_kill_the_consumer`) |
| T3 | `withdraw_outcomes_are_observed_as_best_effort` | tracked-handle sink → `[Withdrawn]`; failing close → `[WithdrawFailed]`; **no** `degraded` effect (withdraw is best-effort by contract) |
| T4 | `health_tracker_transitions_degraded_on_failure_and_clears_on_success` | unit over `HealthTracker::apply(event)` → `degraded()` flips false→true→false; `consecutive_failures` resets (typed, no timestamps asserted) |
| T5 | `no_op_observer_is_a_no_op` | the default observer compiles into the existing routing tests unchanged (guards the seam's optionality) |

**Boot diagnostics pins (`bootstrap.rs` tests; stub `ServerProbe`):**

| # | Test | Pins |
|---|---|---|
| T6 | `a_resolved_server_logs_as_available` | stub Ok → diagnosis `Available` (structural: the enum, not the log text) |
| T7 | `an_unreachable_server_diagnosis_names_the_inapp_fallback` | stub Err → diagnosis `Unavailable`; no boot error (the `Result` is `Ok`) — the boot proceeds |

**UI pins (`ui-slint`; model/adapter level, no .slint internals):**

| # | Test | Pins |
|---|---|---|
| T8 | `the_delivery_chip_maps_degraded_health_to_visible` | the presentation mapping fn: `degraded=true` → visible + non-empty text; false → absent (structural/length-bounded, no text assertions) |
| T9 | `the_repaint_driver_refreshes_on_health_change` | component journey: mark the watch dirty → one refresh pass (the existing fake-engine harness pattern, broadcast-injection style) |

**Tier-4 (manual, this machine — the KDE toast checklist):** the
probe script of §1 as a runnable sequence: (1) `notify-send`
baseline; (2) the notify-rust probe binary; (3) the isolated-config
daemon + `timers add --in 15s` + `spectacle -b` capture at
fire+1.5 s (toast must render); (4) the bogus-bus run (warn pair in
the log, delivery marked, chip would show degraded under `gui`);
(5) caveat: ≥3 identical toasts inside the rate-limit window are
*expected* to be refused by Plasma (`ExcessNotificationGeneration`)
— space the probes or vary the label; (6) caveat: Plasma's
do-not-disturb suppresses rendering (history-only) — not
distinguishable by the app, recorded as environmental.

**Explicitly not tested here:** the real toast in CI (no daemon —
dispatch §4 rule); `notify-rust`/zbus internals; the domain's
`active_notifications` (read-only seam; DR-3 chunk 1's properties
already cover it); text content of any new label.

## 8. Risks

- **R1 — Plasma rate limiting** (observed in probe b): bursts of
  similar toasts (same summary within the window) are refused with
  `ExcessNotificationGeneration` — the sink then *fails* typed
  (correctly), the chip degrades, but the cause is Plasma's spam
  guard, not a broken bus. Accepted for v1: the failure is at least
  visible and logged; a per-fire summary diversification (label
  prefix) would be payload policy creep (D-R11 owns the payload).
- **R2 — DND renders nothing** while delivery "succeeds": the health
  surface shows healthy. Environmental, undetectable from the FDO
  API; recorded in the Tier-4 checklist.
- **R3 — `get_server_information` API drift**: the async re-export
  name under `z-with-tokio` was verified in the vendored 4.18.2
  sources; a version bump that moves it fails at compile time in
  exactly one call site (low blast radius).
- **R4 — mount signature churn**: adding the watch receiver to
  `mount` touches both UI lifecycles and the component-journey
  harness construction sites (compile-enforced, mechanical).
- **R5 — the user's original failure may still reproduce on *their*
  launch path** (a machine state this probe could not construct —
  e.g. a busless systemd system unit). If it does, the shipped
  machinery now names it in three places (boot warn, per-fire warn,
  status chip) instead of one silent swallow — the honest outcome of
  a diagnose-first chunk.
- **R6 — scope magnet**: insistence sound (DR-3 Part E), native
  toast buttons (#444), and settlement behavior all neighbor this
  seam. All fenced: this chunk touches the port's *observation*,
  not its payload, policy, or lifecycle.

## 9. Bookkeeping & delivery

- Estimated diff: **~330–400 lines incl. tests** (app
  `notifications.rs` +observer/events/tracker ~140; `bootstrap.rs`
  +probe/wiring ~60; `ui.rs` hand-over ~10; `ui-slint` mount/
  driver/adapter/models/presentation +chip ~120; `status-bar.slint`
  ~25; CHANGELOG ~10). M-size, matching dispatch §1.
- Stage-2 CHANGELOG entry: reminder OS-delivery observability
  (delivery health chip, boot-time sink diagnosis, typed delivery
  events).
- No issues closed here; #474 annotated at hand-off (dispatch §9).
  Stage discoveries (e.g. a confirmed rate-limit failure mode) go to
  `backlog:` issues per AGENTS §7.
- Quality gates per AGENTS §10 through
  `scripts/run-quality-gates.sh`; hand-off at PR-green per dispatch
  §1; worktree removed only after `mergeStateStatus: CLEAN`.

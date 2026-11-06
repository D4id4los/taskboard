// SPDX-License-Identifier: AGPL-3.0-only
//! The UI lifecycle and adaptation layer (GUI Phase 7, chunk 1).
//!
//! `taskboard-app` owns every IO seam and hands the presentation crate
//! typed values and channel halves (design §1.2): this module adapts the
//! booted core ([`crate::bootstrap::App`]) onto the UI's hand-over set.
//! Chunk 1 carries the *UI-command forwarder* — the actor that drains
//! the UI's command channel into the state engine; the desktop entry
//! lifecycle lands in this module too.

use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use taskboard_domain::StateCommand;
use taskboard_state::EngineHandle;

/// The engine-side sink the forwarder adapts onto. A trait so the
/// forwarder is hermetically testable without a state engine.
pub trait CommandSink: Send + 'static {
    /// Hands one command to the engine (fire-and-forget; drop logging
    /// on a full or closed inbox is the sink's own behaviour).
    fn dispatch(&self, command: StateCommand);
}

impl CommandSink for EngineHandle {
    fn dispatch(&self, command: StateCommand) {
        // EngineHandle::dispatch's own try_send drop logging (full or
        // closed inbox) is the state crate's tested behaviour, not
        // re-tested here (one behaviour, one test, lowest layer).
        EngineHandle::dispatch(self, command);
    }
}

/// Capacity of the UI → app command channel. The engine inbox is 256
/// deep; UI emission is human-input-scale, so 64 queued intents are
/// already an unresponsive-UI pathology rather than a throughput need.
pub(crate) const UI_COMMAND_CAPACITY: usize = 64;

/// The UI → app command channel at its house capacity — one
/// construction site for both UI lifecycles (desktop and kiosk
/// appliance).
#[must_use]
pub fn ui_command_channel() -> (mpsc::Sender<StateCommand>, mpsc::Receiver<StateCommand>) {
    mpsc::channel(UI_COMMAND_CAPACITY)
}

/// The Q15 startup-warning surfacing: typed keymap findings logged
/// with structured fields — the keymap still applied (drops fall back
/// to preset bindings). Part of the UI hand-over contract, shared by
/// the desktop and kiosk-appliance lifecycles.
pub fn surface_keymap_findings(findings: &[taskboard_ui_slint::keymap::Finding]) {
    for finding in findings {
        tracing::warn!(
            kind = ?finding.kind,
            action = ?finding.action,
            context = %finding.context,
            stroke = %finding.stroke,
            "keymap override finding (preset binding wins)"
        );
    }
}

/// Spawns the UI-command forwarder: drains `commands` into `sink` in
/// arrival order until the channel closes (the UI side dropped — the
/// forwarder's own exit, logged). The caller owns the handle's
/// retirement (await the clean exit, or abort once the UI session is
/// over).
pub fn spawn_ui_command_forwarder<T: CommandSink>(
    mut commands: mpsc::Receiver<StateCommand>,
    sink: T,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(command) = commands.recv().await {
            sink.dispatch(command);
        }
        tracing::info!("ui command channel closed; forwarder exiting");
    })
}

/// Where a bare `taskboard` invocation routes under a loaded config
/// (chunk decision D4): `Desktop` boots the GUI *regardless of TTY* —
/// `AppMode` is frontend selection (ADR 0008), and a terminal launch
/// (`taskboard` from a shell) is the normal way to start a desktop app.
/// The REPL stays the TTY front-end of `Daemon` mode (ADR 0011); an
/// explicit `daemon` subcommand bypasses this routing entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BareRoute {
    /// Boot the core and run the desktop UI lifecycle.
    DesktopUi,
    /// TTY + daemon mode: the REPL session (ADR 0011).
    Repl,
    /// Non-TTY daemon mode with no display either (systemd, ssh, cron
    /// — or the TTY-less REPL refusal, resolved by `repl::entry_point`):
    /// the daemon lifecycle.
    DaemonLifecycle,
    /// `mode = "kiosk"`: the appliance lifecycle (ADR 0016) — the daemon
    /// core plus the display-gated on-seat window. TTY-independent: a
    /// seat has no operator TTY (`ssh -t` booting the appliance is
    /// acceptable); the headless fallback is decided inside the
    /// lifecycle, not by this routing.
    KioskAppliance,
}

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
    if display {
        KioskStart::Appliance
    } else {
        KioskStart::Headless
    }
}

/// Whether a graphical session context exists, from the two session
/// environment variables. A variable that is set but *empty* is a
/// broken-session artifact and counts as absent. Pure so the #236
/// detection rule is testable without a display (and so the kiosk
/// wave's display gate can consume the same fn).
#[must_use]
pub fn display_available(wayland: Option<&str>, x11: Option<&str>) -> bool {
    wayland.is_some_and(|v| !v.is_empty()) || x11.is_some_and(|v| !v.is_empty())
}

/// Trivial env glue over [`display_available`]: reads
/// `WAYLAND_DISPLAY`/`DISPLAY`. Untested per the main-glue convention.
/// Pub because the binary's bare-invocation wiring is the caller.
#[must_use]
pub fn display_env_available() -> bool {
    display_available(
        std::env::var_os("WAYLAND_DISPLAY")
            .as_deref()
            .and_then(|v| v.to_str()),
        std::env::var_os("DISPLAY")
            .as_deref()
            .and_then(|v| v.to_str()),
    )
}

/// The pure D4 mode→route mapping, extracted so the behaviour change
/// (bare+TTY `desktop` now GUIs instead of opening a REPL) is
/// unit-testable without a display.
///
/// Daemon mode without a TTY splits by display presence (#236): a
/// file-manager launch inherits the graphical session's display
/// variables and boots the desktop UI instead of silently starting a
/// headless daemon; ssh/systemd/cron (no TTY, no display) keep the
/// daemon lifecycle, and the explicit `daemon` subcommand stays the
/// documented headless entry for sessions that do import a display.
#[must_use]
pub fn bare_invocation_route(mode: crate::config::AppMode, tty: bool, display: bool) -> BareRoute {
    match mode {
        crate::config::AppMode::Desktop => BareRoute::DesktopUi,
        crate::config::AppMode::Daemon if tty => BareRoute::Repl,
        crate::config::AppMode::Daemon if display => BareRoute::DesktopUi,
        crate::config::AppMode::Kiosk => BareRoute::KioskAppliance,
        crate::config::AppMode::Daemon => BareRoute::DaemonLifecycle,
    }
}

/// Resolves the `[ui]` table into the UI crate's typed hand-over
/// (D14): the validated effective keymap plus the Q15 startup-warning
/// summary. Pure — the figment layer stops at raw strings; the
/// keymap layer owns value validation. Findings are *returned* (the
/// caller surfaces them via `tracing`); nothing here can fail —
/// a hostile keymap never blocks UI start (Q15).
///
/// # Panics
///
/// Never on a validated config: `ui.scale` is minted via
/// `UiScale::new` and `AppConfig::validate` has already range-checked
/// it. Calling on an unvalidated `[ui]` table panics — same rule as
/// `AppConfig::server_url`/`username`.
#[must_use]
pub fn resolve_ui_settings(
    ui: &crate::config::UiSection,
    day_start: (chrono::NaiveTime, chrono::FixedOffset),
) -> (
    taskboard_ui_slint::UiSettings,
    Vec<taskboard_ui_slint::keymap::Finding>,
) {
    let raw = taskboard_ui_slint::keymap::RawKeymap {
        preset: match ui.keymap.preset {
            crate::config::KeymapPreset::Standard => taskboard_ui_slint::keymap::Preset::Standard,
            crate::config::KeymapPreset::Emacs => taskboard_ui_slint::keymap::Preset::Emacs,
            crate::config::KeymapPreset::Vi => taskboard_ui_slint::keymap::Preset::Vi,
        },
        overrides: ui
            .keymap
            .overrides
            .iter()
            .map(|(id, spec)| (id.clone(), spec.to_sequences()))
            .collect(),
    };
    let built = taskboard_ui_slint::keymap::build(&raw);
    let settings = taskboard_ui_slint::UiSettings {
        scale: taskboard_ui_slint::UiScale::new(ui.scale)
            .expect("ui.scale validated by AppConfig::validate — call only on a validated config"),
        startup: taskboard_ui_slint::adapter::StartupWarnings {
            keymap_findings: built.findings.len().try_into().unwrap_or(u32::MAX),
        },
        keymap: built.keymap,
        // Production always reads the real wall clock; the freeze seam
        // is the golden journeys' alone (UiSettings::clock doc).
        clock: None,
        kiosk: false, // the desktop hand-over; the kiosk appliance (K3) sets this
        // The desktop hand-over never rotates (K13): only the kiosk
        // appliance maps the validated `[kiosk.rotation]` table.
        kiosk_rotation: None,
        // The device-local widths (round-1 D16): copied verbatim —
        // uuid-keyed both sides, the mount converts to domain newtypes.
        stack_widths: ui.stack_widths.clone(),
        connectors: Vec::new(), // filled by the lifecycle (spawn sites) from the full config
        boards: Vec::new(),     // filled by the lifecycle from the engine state (r2 D32)
        keymap_preset: match ui.keymap.preset {
            crate::config::KeymapPreset::Standard => taskboard_ui_slint::keymap::Preset::Standard,
            crate::config::KeymapPreset::Emacs => taskboard_ui_slint::keymap::Preset::Emacs,
            crate::config::KeymapPreset::Vi => taskboard_ui_slint::keymap::Preset::Vi,
        },
        // The today window's derivation pair (chunk 10 D4): resolved
        // once here — nothing reads config mid-flight.
        day_start,
        // The due-soon horizon (round-3 D14/#230): the validated
        // `[ui] due_soon_days` as a typed delta (0 is legal — an
        // empty soon set).
        due_soon_horizon: chrono::TimeDelta::try_days(i64::from(ui.due_soon_days))
            .unwrap_or(taskboard_ui_slint::presentation::DUE_SOON_HORIZON),
        // Overwritten by the lifecycle from the engine config right
        // after this returns (the kiosk/connectors/boards precedent).
        reminder_defaults: taskboard_domain::ReminderDefaults::default(),
        // Overwritten by the lifecycle from the validated `[focus]`
        // table right after this returns (the reminder_defaults
        // precedent) — `None` while the mode is off.
        focus: None,
        // The general snapshot: filled by the lifecycle from the full
        // config (the connectors/boards precedent).
        general: taskboard_ui_slint::requests::SettingsSnapshot::default(),
        // The window-geometry restore value (#412): the raw table
        // mints here (the fallible `WindowGeometry::new`) — a refused
        // value hands `None` through and never blocks UI start (the
        // lenient-read contract; the `UiScale` expect-shape is
        // deliberately NOT copied — geometry's mint is fallible by
        // design).
        window_geometry: ui.window_geometry.and_then(|raw| {
            taskboard_ui_slint::geometry::WindowGeometry::new(raw.x, raw.y, raw.width, raw.height)
        }),
    };
    (settings, built.findings)
}

/// The today window's derivation pair (chunk 10 D4): the parsed
/// `[preferences] day_start_time` (config validation owns the refusal)
/// plus the device's fixed UTC offset at derivation time — the
/// `day_window` consumers contract verbatim.
#[must_use]
pub fn resolve_day_start(
    config: &crate::config::AppConfig,
) -> (chrono::NaiveTime, chrono::FixedOffset) {
    let day_start_time = config
        .preferences
        .day_start_time
        .parse::<chrono::NaiveTime>()
        .unwrap_or_default();
    let offset = *chrono::Local::now().offset();
    (day_start_time, offset)
}

/// The settings editor's general snapshot for the UI hand-over: the
/// ONE constructor both UI lifecycles call (the desktop `run_desktop`
/// and the kiosk appliance — the `resolve_ui_settings` precedent),
/// plus the env-override set sampled over the resolved write path
/// (plan D1: env vars are boot state, sampled once here; write replies
/// never re-derive it). A failed env read degrades to the empty set,
/// loud in the log — the snapshot is a display surface, never a boot
/// gate.
#[must_use]
pub fn resolve_settings_snapshot(
    config: &crate::config::AppConfig,
    config_write_path: &std::path::Path,
) -> taskboard_ui_slint::requests::SettingsSnapshot {
    let mut snapshot = settings_snapshot_from_config(config);
    snapshot.env_overridden =
        match crate::config::AppConfig::env_overridden_keys(Some(config_write_path)) {
            Ok(set) => set,
            Err(err) => {
                tracing::warn!(error = %err, "settings snapshot: the env-override read failed");
                std::collections::BTreeSet::new()
            }
        };
    snapshot
}

/// The snapshot body over one decoded config: typed editable values,
/// the read-only deployment facts, and the per-field apply semantics
/// (plan D12 — the apply authority is the app). The env-override set
/// stays empty — the hand-over wrapper fills it, and write replies
/// carry it empty by contract (the UI retains its hand-over set).
#[must_use]
#[allow(clippy::too_many_lines)] // the §3 inventory: one fact row per read-only key
pub fn settings_snapshot_from_config(
    config: &crate::config::AppConfig,
) -> taskboard_ui_slint::requests::SettingsSnapshot {
    use taskboard_ui_slint::requests::{ReminderSink, SectionApply as Apply, SettingsSnapshot};

    let mode_word = match config.app.mode {
        crate::config::AppMode::Daemon => "daemon",
        crate::config::AppMode::Desktop => "desktop",
        crate::config::AppMode::Kiosk => "kiosk",
    };
    let store_word = match config.nextcloud.credential_store {
        crate::config::CredentialStoreKind::Keyring => "keyring",
        crate::config::CredentialStoreKind::Env => "env",
        crate::config::CredentialStoreKind::File => "file",
    };
    let facts = vec![
        fact("app.mode", mode_word.to_owned(), Apply::OnRestart),
        fact(
            "app.shutdown_timeout_secs",
            config.app.shutdown_timeout_secs.to_string(),
            Apply::OnRestart,
        ),
        fact(
            "storage.db_path",
            config
                .storage
                .db_path
                .as_ref()
                .map_or_else(String::new, |p| p.display().to_string()),
            Apply::OnRestart,
        ),
        fact(
            "nextcloud.credential_store",
            store_word.to_owned(),
            Apply::OnRestart,
        ),
        fact(
            "web.enabled",
            config.web.enabled.to_string(),
            Apply::OnRestart,
        ),
        fact("web.bind", config.web.bind.clone(), Apply::OnRestart),
        fact(
            "web.refresh_secs",
            config.web.refresh_secs.to_string(),
            Apply::OnRestart,
        ),
        // Presence only — the one secret-adjacent config value never
        // renders its content.
        fact(
            "web.token",
            config.web.token.is_some().to_string(),
            Apply::OnRestart,
        ),
        fact(
            "kiosk.fixed_board",
            config
                .kiosk
                .fixed_board
                .as_ref()
                .map_or_else(|| "unset".to_owned(), std::string::ToString::to_string),
            Apply::OnRestart,
        ),
        fact(
            "kiosk.rotation",
            if config.kiosk.rotation.is_some() {
                "configured".to_owned()
            } else {
                "absent".to_owned()
            },
            Apply::OnRestart,
        ),
        fact(
            "ipc.enabled",
            config.ipc.enabled.to_string(),
            Apply::OnRestart,
        ),
        fact(
            "ipc.client_read_timeout_secs",
            config
                .ipc
                .client_read_timeout_secs
                .map_or_else(|| "derived".to_owned(), |secs| secs.to_string()),
            Apply::OnRestart,
        ),
        // The three pomodoro-era focus facts left the list when they
        // became editable rows (the focus settings editor, #625): a key
        // renders exactly once, never both editable and read-only.
        fact(
            "ui.keymap.overrides",
            config.ui.keymap.overrides.len().to_string(),
            Apply::OnRestart,
        ),
        // The forwarder-owned fenced subtrees (plan D6): their apply
        // is live — but the editor never writes them, the facts only
        // display presence.
        fact(
            "ui.stack_widths",
            config.ui.stack_widths.len().to_string(),
            Apply::Live,
        ),
        fact(
            "ui.window_geometry",
            if config.ui.window_geometry.is_some() {
                "stored".to_owned()
            } else {
                "absent".to_owned()
            },
            Apply::Live,
        ),
    ];
    let mut applies = std::collections::BTreeMap::new();
    for key in [
        "preferences.day_start_time",
        "reminders.enabled",
        "reminders.tick_secs",
        "reminders.default_before",
        "reminders.sink",
        "sync.poll_interval_secs",
        "sync.backoff_initial_secs",
        "sync.backoff_max_secs",
        "ui.due_soon_days",
        // The `[focus]` knobs (the focus settings editor, #625): the
        // runtime reads config once at boot, so every key is OnRestart.
        "focus.enabled",
        "focus.work_mins",
        "focus.break_mins",
        "focus.chain",
        "focus.long_break_mins",
        "focus.long_break_every",
        "focus.notify",
    ] {
        applies.insert(key.to_owned(), Apply::OnRestart);
    }
    let day_start_time = config
        .preferences
        .day_start_time
        .parse::<chrono::NaiveTime>()
        .unwrap_or_default();
    SettingsSnapshot {
        day_start_time,
        reminders_enabled: config.reminders.enabled,
        reminder_tick_secs: config.reminders.tick_secs,
        reminder_before: config.reminders.default_before.clone(),
        reminder_sink: match config.reminders.sink {
            crate::config::SinkChoice::Auto => ReminderSink::Auto,
            crate::config::SinkChoice::Log => ReminderSink::Log,
            crate::config::SinkChoice::Desktop => ReminderSink::Desktop,
        },
        poll_interval_secs: config.sync.poll_interval_secs,
        backoff_initial_secs: config.sync.backoff_initial_secs,
        backoff_max_secs: config.sync.backoff_max_secs,
        due_soon_days: config.ui.due_soon_days,
        focus_enabled: config.focus.enabled,
        focus_work_mins: config.focus.work_mins,
        focus_break_mins: config.focus.break_mins,
        focus_chain: config.focus.chain,
        focus_long_break_mins: config.focus.long_break_mins,
        focus_long_break_every: config.focus.long_break_every,
        focus_notify: config.focus.notify,
        env_overridden: std::collections::BTreeSet::new(),
        facts,
        applies,
    }
}

/// One read-only fact row: every deployment row applies at restart
/// (the boot graph's inventory; the forwarder-owned subtrees are the
/// Live exceptions, marked where they appear).
fn fact(
    key: &'static str,
    value: String,
    apply: taskboard_ui_slint::requests::SectionApply,
) -> taskboard_ui_slint::requests::SettingsFact {
    taskboard_ui_slint::requests::SettingsFact { key, value, apply }
}

/// The secret-free connector list for the settings hand-over (round-1
/// D2): `configured_connectors` (F-C4, the single source of "which
/// connectors exist") mapped onto the UI crate's row type. Resolved by
/// both UI lifecycles from the full config — `resolve_ui_settings`
/// deliberately sees only the `[ui]` table.
///
/// # Errors
///
/// [`ConfigError::InvalidConnectorName`] when the config's connector
/// names fail the kebab predicate — a raw, unvalidated config.
#[allow(clippy::result_large_err)] // mirrors the enum's one grant above
pub fn resolve_connector_infos(
    config: &crate::config::AppConfig,
) -> Result<Vec<taskboard_ui_slint::requests::ConnectorInfo>, crate::config::ConfigError> {
    Ok(crate::connectors::configured_connectors(config)?
        .into_iter()
        .map(|spec| taskboard_ui_slint::requests::ConnectorInfo {
            name: spec.name.to_string(),
            server_url: spec.server_url,
            username: spec.username,
        })
        .collect())
}

/// The board-selector rows for the settings hand-over (r2 D32): the
/// live boards (`!deleted && !archived` — the same rule the board
/// surface renders by, so the surface and the selector agree) with
/// their binding's connector name. Pure over the state snapshot; the
/// [`EngineHandle`](taskboard_state::EngineHandle) wrapper below is
/// the lifecycle-facing convenience.
#[must_use]
pub fn resolve_board_infos_from_state(
    state: &taskboard_domain::AppState,
) -> Vec<taskboard_ui_slint::requests::BoardInfo> {
    taskboard_ui_slint::presentation::live_boards(state)
        .into_iter()
        .map(|board| taskboard_ui_slint::requests::BoardInfo {
            id: board.id.as_uuid(),
            name: board.title.clone(),
            bound_connector: board
                .remote
                .as_ref()
                .map(|binding| binding.connector.to_string()),
        })
        .collect()
}

/// The engine-facing wrapper over
/// [`resolve_board_infos_from_state`] — the shape both UI lifecycles
/// call at hand-over (r2 D32).
#[must_use]
pub fn resolve_board_infos(engine: &EngineHandle) -> Vec<taskboard_ui_slint::requests::BoardInfo> {
    resolve_board_infos_from_state(&engine.shared_state().load())
}

/// The OS-DPI escape hatch (§6.2, D13): reads `SLINT_SCALE_FACTOR`
/// *before* any slint-touching call and logs its presence. The app
/// never sets it programmatically — the user's env is the input;
/// observability is the app's whole duty toward it. The winit backend
/// honours the variable at window creation; it composes with
/// `[ui] scale` (OS-DPI alignment × token multiplier). Pub because the
/// kiosk appliance's mount path observes the same hatch (ADR 0016).
pub fn observe_scale_escape_hatch() {
    match std::env::var("SLINT_SCALE_FACTOR") {
        Ok(value) => tracing::info!(
            value = %value,
            "SLINT_SCALE_FACTOR set: the winit backend will apply it at window creation"
        ),
        Err(std::env::VarError::NotPresent) => {}
        Err(std::env::VarError::NotUnicode(raw)) => {
            tracing::warn!(?raw, "SLINT_SCALE_FACTOR is not valid unicode; ignoring");
        }
    }
}

/// The desktop UI lifecycle (D4/D5): boot the core exactly like the
/// daemon path (same store selection, `LogSink` notifier), build the
/// UI hand-over, spawn the command forwarder, mount the window, then
/// run the slint event loop on the process main thread — see
/// [`run_desktop`].
///
/// Quits the slint event loop reliably from outside the loop thread.
/// A bare `quit()` before the loop exists is dropped by the backend
/// (`Ok` with no delivery), so: a quit landing pre-boot is parked by
/// [`taskboard_ui_slint::quit`] on the pre-loop flag ([`run`] honors
/// it), and here we additionally wait for the loop to actually run —
/// bounded at ~10 s — before delivering the quit through the live
/// proxy. Giving up is logged; `run`'s own result decides the outcome.
pub async fn quit_until_accepted() {
    let mut waited_ms = 0u32;
    while !taskboard_ui_slint::event_loop_running() {
        if waited_ms >= 10_000 {
            tracing::warn!("event loop never started; quit not delivered through the proxy");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        waited_ms += 50;
    }
    if let Err(err) = taskboard_ui_slint::quit() {
        tracing::warn!(error = %err, "event loop refused the quit");
    }
}

/// Retires a forwarder after the UI event loop is gone: awaits its
/// natural close-drain under a budget, aborting only a wedged task.
/// Returns whether the join had to abort (≤capacity intents lost —
/// bounded and signaled, never silent). The abort at the budget is
/// load-bearing beyond the intents: the forwarder holds an
/// `EngineHandle` clone, so a forwarder left running past the retire
/// would keep the engine's inbox open and wedge the shutdown drain
/// after it.
async fn retire_forwarder(mut join: JoinHandle<()>, budget: Duration, what: &str) -> bool {
    match tokio::time::timeout(budget, &mut join).await {
        Ok(join_result) => {
            if let Err(err) = join_result
                && err.is_panic()
            {
                tracing::error!(%what, "forwarder panicked; resumed");
            }
            false
        }
        Err(_elapsed) => {
            tracing::warn!(%what, "forwarder did not drain within its retire budget");
            join.abort();
            true
        }
    }
}

/// The desktop entry (`taskboard gui`): boots the core, mounts the
/// window, and run the slint event loop **on the process main thread**
/// (we are inside `block_on`; the multi-thread runtime's workers keep
/// driving every actor). Exit paths: window close ends the loop;
/// `ctrl_c`/SIGTERM land via the exit watcher calling
/// [`taskboard_ui_slint::quit`] — both reach the same graceful
/// shutdown.
///
/// # Errors
///
/// Bootstrap, mount, and event-loop failures, as typed errors.
pub async fn run_desktop(
    config: crate::config::AppConfig,
    config_write_path: std::path::PathBuf,
) -> color_eyre::Result<()> {
    // D13: the env var is read before anything slint-side.
    observe_scale_escape_hatch();

    let (mut ui_settings, keymap_findings) =
        resolve_ui_settings(&config.ui, resolve_day_start(&config));
    // The reminder defaults ride the hand-over (R3 chunk C): the
    // notification surface derives from the same value the engine
    // evaluates with.
    ui_settings.reminder_defaults = config.reminder_engine_config().defaults;
    // The focus timings ride the hand-over (#496): the overlay's
    // countdown derives from the same validated value the runtime
    // was spawned with (the kiosk gate lives at the spawn decision —
    // a kiosk hand-over never carries a timing because the runtime
    // is not spawned; the overlay's action gate re-checks anyway).
    ui_settings.focus = config.focus_config();
    surface_keymap_findings(&keymap_findings);
    let store = crate::secrets::resolve_credential_store(
        config.nextcloud.credential_store,
        config.credential_base_dir(),
    );
    let sink = crate::bootstrap::resolve_notifier_sink(&config);
    let app = crate::bootstrap::bootstrap(config, store, sink).await?;

    // The hand-over (D6): the app keeps the receiver half of the
    // command channel and forwards; the UI never sees an EngineHandle.
    let (commands_tx, commands_rx) = ui_command_channel();
    let forwarder = spawn_ui_command_forwarder(commands_rx, app.engine.clone());
    // The general snapshot before the workspace consumes the write
    // path (one read of the boot env, plan D1).
    ui_settings.general = resolve_settings_snapshot(&app.config, &config_write_path);
    let (requests_tx, requests_rx) = crate::ui_requests::ui_request_channel();
    let request_forwarder = crate::ui_requests::spawn_ui_request_forwarder(
        requests_rx,
        app.engine.shared_state(),
        crate::ui_requests::ConnectionWorkspace {
            config_write_path,
            store: crate::secrets::resolve_credential_store(
                app.config.nextcloud.credential_store,
                app.config.credential_base_dir(),
            ),
            control: app.connector_control(),
            engine: app.engine.clone(),
        },
    );

    ui_settings.connectors = resolve_connector_infos(&app.config)?;
    ui_settings.boards = resolve_board_infos(&app.engine);
    let handle = taskboard_ui_slint::mount(
        app.engine.shared_state(),
        app.engine.subscribe(),
        app.system.subscribe(),
        commands_tx,
        requests_tx,
        ui_settings,
        app.delivery_health.clone(),
    )?;

    // D5b: the physical exit paths are wired so shutdown is always
    // graceful (`app.exit` as an action is chunk 2). Thin main-glue
    // class — deliberately untested; the forwarder and config carry
    // the tests. The lifecycle watcher installs after the boot
    // returned (D-7/Q11) and runs the same `new(1)` ladder: second
    // signal = this tail's escalate arm (the skip-joins role), third =
    // the watcher's force rung.
    let signals = crate::signals::install_lifecycle_watcher();
    let crate::signals::LifecycleSignals {
        begin_drain: mut signal_quit,
        mut escalate,
        ..
    } = signals;
    let exit_watcher = tokio::spawn(async move {
        crate::signals::changed_or_never(&mut signal_quit).await;
        tracing::info!("shutdown signal received");
        quit_until_accepted().await;
    });

    let run_result = taskboard_ui_slint::run(handle);

    exit_watcher.abort();
    // F9-05: retire, don't abort outright. The queued intents are user
    // mutations; each forwarder's `while recv` loop *is* a drain once
    // the UI-side senders die with the event loop, and the request
    // forwarder's close path lands the debounced width flush an
    // unconditional abort skipped. A wedged forwarder is aborted at
    // the budget (the fact is warned — a silent ≤capacity-intent loss
    // is the shape this replaced; warn-only, not folded into the exit
    // decision, per the recorded plan decision). The two retires run
    // concurrently so the added exit latency is bounded at one budget,
    // not two — the event-loop teardown must stay invisible to a
    // supervisor.
    let retire_budget = Duration::from_secs(app.config.app.shutdown_timeout_secs);
    let (command_aborted, request_aborted) = tokio::join!(
        retire_forwarder(forwarder, retire_budget, "ui command forwarder"),
        retire_forwarder(request_forwarder, retire_budget, "ui request forwarder"),
    );
    if command_aborted {
        tracing::warn!("ui command forwarder aborted at its retire budget");
    }
    if request_aborted {
        tracing::warn!("ui request forwarder aborted at its retire budget");
    }
    // The GUI tail's escalate arm needs nothing from `App` (the owned
    // `shutdown` flushes first — the salvage obligation is satisfied
    // or in flight), so there is no ownership trap on this surface.
    let outcome = tokio::select! {
        outcome = app.shutdown() => outcome,
        () = crate::signals::changed_or_never(&mut escalate) => {
            let code = escalate.borrow_and_update().exit_code();
            tracing::warn!(code, "escalation during the desktop shutdown; forcing exit");
            std::process::exit(code);
        }
    };
    crate::bootstrap::exit_if_degraded(outcome, false);
    run_result?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskboard_ui_slint::requests::SectionApply;

    /// S-1/S-2 (the GUI settings editor's snapshot pins): the one
    /// constructor projects typed values from a decoded config, the
    /// apply map is exhaustive over the editable groups (a new
    /// editable group without an apply fact fails here), the fenced
    /// subtrees carry Live, the secret-adjacent value renders
    /// presence only, and the snapshot travels to `UiSettings`
    /// structurally.
    #[test]
    fn the_settings_snapshot_projects_typed_values_and_the_apply_map() {
        use crate::config::SinkChoice;
        let mut config = crate::config::AppConfig::default();
        config.preferences.day_start_time = "05:30".into();
        config.reminders.enabled = false;
        config.reminders.tick_secs = 45;
        config.reminders.default_before = "10m".into();
        config.reminders.sink = SinkChoice::Desktop;
        config.sync.poll_interval_secs = 60;
        config.sync.backoff_initial_secs = 10;
        config.sync.backoff_max_secs = 600;
        config.ui.due_soon_days = 14;
        config.web.token = Some("sekrit-token".into());
        config.focus.enabled = true;
        config.focus.work_mins = 50;
        config.focus.break_mins = 10;
        config.focus.chain = true;
        config.focus.long_break_mins = 20;
        config.focus.long_break_every = 6;
        config.focus.notify = false;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("taskboard.toml");
        let snapshot = resolve_settings_snapshot(&config, &path);

        assert_eq!(
            snapshot.day_start_time,
            chrono::NaiveTime::from_hms_opt(5, 30, 0).expect("t")
        );
        assert!(!snapshot.reminders_enabled);
        assert_eq!(snapshot.reminder_tick_secs, 45);
        assert_eq!(snapshot.reminder_before, "10m");
        assert_eq!(
            snapshot.reminder_sink,
            taskboard_ui_slint::requests::ReminderSink::Desktop
        );
        assert_eq!(snapshot.poll_interval_secs, 60);
        assert_eq!(snapshot.backoff_initial_secs, 10);
        assert_eq!(snapshot.backoff_max_secs, 600);
        assert_eq!(snapshot.due_soon_days, 14);
        assert!(snapshot.focus_enabled);
        assert_eq!(snapshot.focus_work_mins, 50);
        assert_eq!(snapshot.focus_break_mins, 10);
        assert!(snapshot.focus_chain);
        assert_eq!(snapshot.focus_long_break_mins, 20);
        assert_eq!(snapshot.focus_long_break_every, 6);
        assert!(!snapshot.focus_notify);

        // The apply map: exhaustive over the editable groups.
        assert_eq!(
            snapshot.applies.len(),
            16,
            "one apply fact per editable field"
        );
        for key in [
            "preferences.day_start_time",
            "reminders.enabled",
            "reminders.tick_secs",
            "reminders.default_before",
            "reminders.sink",
            "sync.poll_interval_secs",
            "sync.backoff_initial_secs",
            "sync.backoff_max_secs",
            "ui.due_soon_days",
            "focus.enabled",
            "focus.work_mins",
            "focus.break_mins",
            "focus.chain",
            "focus.long_break_mins",
            "focus.long_break_every",
            "focus.notify",
        ] {
            assert_eq!(
                snapshot.applies.get(key),
                Some(&SectionApply::OnRestart),
                "{key} applies at restart"
            );
        }

        // The env set: no env overrides over the empty fixture file.
        assert!(snapshot.env_overridden.is_empty());

        // The facts: the fenced subtrees are Live, everything else
        // OnRestart; the secret-adjacent value renders presence only.
        let find = |key| {
            snapshot
                .facts
                .iter()
                .find(|fact| fact.key == key)
                .unwrap_or_else(|| panic!("{key} is in the facts"))
                .clone()
        };
        assert_eq!(find("ui.stack_widths").apply, SectionApply::Live);
        assert_eq!(find("ui.window_geometry").apply, SectionApply::Live);
        assert_eq!(find("app.mode").apply, SectionApply::OnRestart);
        assert_eq!(find("web.token").value, "true", "presence, never the value");
        assert!(!format!("{snapshot:?}").contains("sekrit-token"));
        // The promoted keys left the facts (D-FS2): a key renders
        // exactly once, never both editable and read-only.
        for key in ["focus.enabled", "focus.work_mins", "focus.break_mins"] {
            assert!(
                snapshot.facts.iter().all(|fact| fact.key != key),
                "{key} is an editable row, not a fact"
            );
        }

        // S-2: the set + snapshot travel to UiSettings structurally.
        let settings = taskboard_ui_slint::UiSettings {
            general: snapshot.clone(),
            ..taskboard_ui_slint::UiSettings::default()
        };
        assert_eq!(settings.general, snapshot);
    }

    use std::sync::{Arc, Mutex};

    /// Fake sink: collects dispatch order/content hermetically (fakes
    /// over mocks; no engine spawned). The log lives behind an `Arc` so
    /// the test keeps a handle while the forwarder owns the sink.
    #[derive(Clone, Default)]
    struct FakeSink(Arc<Mutex<Vec<StateCommand>>>);

    impl CommandSink for FakeSink {
        fn dispatch(&self, command: StateCommand) {
            self.0.lock().expect("sink lock").push(command);
        }
    }

    fn sample_commands() -> Vec<StateCommand> {
        use taskboard_domain::StackId;
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        vec![
            StateCommand::CreateTask {
                title: "first".into(),
                stack,
                order: 0,
            },
            StateCommand::CreateTask {
                title: "second".into(),
                stack,
                order: 1,
            },
            StateCommand::CreateStack {
                board: taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7()),
                title: "done".into(),
                order: 2,
            },
        ]
    }

    /// F9-05: retiring a forwarder drains the queued intents (the
    /// natural close-drain), and reports *no* abort when the channel
    /// closes cleanly — the abort the retire replaced discarded the
    /// queue.
    #[tokio::test]
    async fn retiring_the_forwarder_drains_the_queued_intents_without_aborting() {
        let sink = FakeSink::default();
        let commands = sample_commands();
        let (tx, rx) = mpsc::channel::<StateCommand>(UI_COMMAND_CAPACITY);
        let forwarder = spawn_ui_command_forwarder(rx, sink.clone());

        for command in &commands {
            tx.send(command.clone())
                .await
                .expect("channel has capacity");
        }
        drop(tx); // the UI side going away is what makes the drain finite

        let aborted = retire_forwarder(forwarder, std::time::Duration::from_secs(30), "test").await;
        assert!(!aborted, "a clean close-drain never hits the budget");

        let collected = sink.0.lock().expect("sink lock").clone();
        assert_eq!(collected, commands, "every queued intent landed, in order");
    }

    /// F9-05's backstop: a forwarder whose UI-side sender never dies
    /// (a mounted component outliving the event loop, e.g. the MCP
    /// harness) cannot close-drain, so the retire must abort the task
    /// at the budget — a left-running forwarder holds an
    /// `EngineHandle` and would wedge the engine's shutdown drain
    /// after it. Paused time: the budget expiry is virtual-clock
    /// deterministic.
    #[tokio::test(start_paused = true)]
    async fn a_wedged_forwarder_is_aborted_at_its_retire_budget() {
        let sink = FakeSink::default();
        let (_tx, rx) = mpsc::channel::<StateCommand>(UI_COMMAND_CAPACITY);
        let forwarder = spawn_ui_command_forwarder(rx, sink.clone());
        // `_tx` is deliberately kept: no close-drain can ever arrive.

        let aborted = retire_forwarder(forwarder, Duration::from_secs(5), "test").await;
        assert!(aborted, "a wedged forwarder must hit the retire budget");
    }

    #[tokio::test]
    async fn forwards_the_sequence_in_order_and_exits_on_close() {
        let sink = FakeSink::default();
        let commands = sample_commands();
        let (tx, rx) = mpsc::channel::<StateCommand>(UI_COMMAND_CAPACITY);
        let forwarder = spawn_ui_command_forwarder(rx, sink.clone());

        for command in &commands {
            tx.send(command.clone())
                .await
                .expect("channel has capacity");
        }
        drop(tx); // the UI side going away ends the forwarder

        forwarder.await.expect("forwarder joins cleanly");
        let collected = sink.0.lock().expect("sink lock").clone();
        assert_eq!(collected, commands, "exact sequence, order preserved");
    }

    /// The pure D4 routing: `desktop` routes to the UI lifecycle
    /// regardless of TTY; `daemon` keeps the TTY-sensitive REPL rule
    /// and splits the non-TTY cell by display presence (#236 — a
    /// file-manager launch boots the GUI, ssh/systemd/cron stay
    /// headless); `kiosk` is the appliance lifecycle in every cell
    /// (ADR 0016).
    #[test]
    fn bare_invocation_routing_maps_modes_per_d4() {
        use crate::config::AppMode;
        // Desktop is display-blind and TTY-blind (D4).
        for tty in [true, false] {
            for display in [true, false] {
                assert_eq!(
                    bare_invocation_route(AppMode::Desktop, tty, display),
                    BareRoute::DesktopUi
                );
            }
        }
        // Daemon + TTY is the REPL under either display value.
        for display in [true, false] {
            assert_eq!(
                bare_invocation_route(AppMode::Daemon, true, display),
                BareRoute::Repl
            );
        }
        // The #236 fix: non-TTY daemon with a graphical session boots
        // the GUI instead of a silent headless daemon.
        assert_eq!(
            bare_invocation_route(AppMode::Daemon, false, true),
            BareRoute::DesktopUi
        );
        // Non-TTY daemon without a display stays the daemon lifecycle
        // (systemd, ssh, cron).
        assert_eq!(
            bare_invocation_route(AppMode::Daemon, false, false),
            BareRoute::DaemonLifecycle
        );
        // Kiosk is the appliance lifecycle in every cell: TTY-
        // and display-independent (ADR 0016) — the headless fallback
        // is decided inside the appliance, not by this routing.
        for tty in [true, false] {
            for display in [true, false] {
                assert_eq!(
                    bare_invocation_route(AppMode::Kiosk, tty, display),
                    BareRoute::KioskAppliance
                );
            }
        }
    }

    /// The Q3 start decision: a display context gates the window
    /// mount; without one the appliance falls back to the headless
    /// daemon lifecycle (loudly, at the caller).
    #[test]
    fn kiosk_start_splits_the_display_gate() {
        assert_eq!(kiosk_start(true), KioskStart::Appliance);
        assert_eq!(kiosk_start(false), KioskStart::Headless);
    }

    /// The #236 detection rule: a graphical session exists when either
    /// session variable is set non-empty; empty or absent counts as no
    /// display (broken-session artifact).
    #[test]
    fn display_available_needs_one_non_empty_session_var() {
        assert!(display_available(Some("wayland-0"), None));
        assert!(display_available(None, Some(":0")));
        assert!(display_available(Some(""), Some(":0")));
        assert!(!display_available(Some(""), Some("")));
        assert!(!display_available(None, None));
    }

    #[tokio::test]
    async fn exits_immediately_when_the_channel_starts_closed() {
        let sink = FakeSink::default();
        // The sender binding is `_`, so the channel starts closed; the
        // receiver is passed to the forwarder as-is.
        let (_, rx) = mpsc::channel::<StateCommand>(UI_COMMAND_CAPACITY);
        let forwarder = spawn_ui_command_forwarder(rx, sink.clone());
        // Join-based terminal-state assert: no sleeps, no polling.
        forwarder
            .await
            .expect("forwarder exits on a closed channel");
        assert!(sink.0.lock().expect("sink lock").is_empty());
    }
}

#[cfg(test)]
mod keymap_resolution_tests {
    use super::*;
    use crate::config::{KeymapPreset, KeymapSection, OverrideSpec};
    use std::collections::BTreeMap;

    /// T30 — the hand-over is typed and total: valid overrides land
    /// in `UiSettings.keymap`, the findings count rides
    /// `StartupWarnings`, and a hostile table changes nothing but the
    /// findings (never an error — Q15).
    #[test]
    fn resolve_hands_over_keymap_and_counts_findings() {
        // Hostile config: unknown action, malformed stroke, and a
        // conflict with the preset — plus one valid override.
        let ui = crate::config::UiSection {
            scale: 1.25,
            window_geometry: None,
            stack_widths: BTreeMap::new(),
            keymap: KeymapSection {
                preset: KeymapPreset::Standard,
                overrides: BTreeMap::from([
                    (
                        "ghost.action".to_owned(),
                        OverrideSpec::Single("X".to_owned()),
                    ),
                    (
                        "task.complete".to_owned(),
                        OverrideSpec::Single("Ctrl+!!".to_owned()),
                    ),
                    (
                        "app.undo".to_owned(),
                        OverrideSpec::Single("Ctrl+Q".to_owned()),
                    ),
                    (
                        "app.exit".to_owned(),
                        OverrideSpec::Single("Ctrl+\\".to_owned()),
                    ),
                ]),
            },
            due_soon_days: 7,
        };
        let (settings, findings) = resolve_ui_settings(
            &ui,
            (
                chrono::NaiveTime::MIN,
                chrono::FixedOffset::east_opt(0).expect("zero"),
            ),
        );

        // Structural asserts on the handed-over map (no strings).
        assert_eq!(settings.scale.get(), 1.25);
        assert_eq!(
            settings.due_soon_horizon,
            chrono::TimeDelta::try_days(7).unwrap(),
            "the knob hands over as a typed horizon"
        );
        assert_eq!(
            settings.startup.keymap_findings,
            u32::try_from(findings.len()).expect("findings fit u32")
        );
        assert_eq!(findings.len(), 3, "exhaustive: every offender listed");
        let seq = taskboard_ui_slint::keymap::stroke::parse("Ctrl+\\")
            .expect("the valid override's stroke parses");
        assert_eq!(
            settings
                .keymap
                .resolve(&[taskboard_ui_slint::actions::ContextId::Global], &[seq]),
            Some(taskboard_ui_slint::actions::ActionId::AppExit),
            "the one valid override landed in the keymap"
        );

        // The dropped overrides left the preset bindings intact.
        let undo = taskboard_ui_slint::keymap::stroke::parse("Ctrl+Z").expect("parses");
        assert_eq!(
            settings
                .keymap
                .resolve(&[taskboard_ui_slint::actions::ContextId::Global], &[undo]),
            Some(taskboard_ui_slint::actions::ActionId::AppUndo),
            "preset fallback survives the dropped override"
        );
    }

    /// The vi preset selection carries through to the handed-over
    /// keymap (mode contexts bound).
    #[test]
    fn preset_selection_reaches_the_keymap() {
        let ui = crate::config::UiSection {
            keymap: KeymapSection {
                preset: KeymapPreset::Vi,
                overrides: BTreeMap::new(),
            },
            ..crate::config::UiSection::default()
        };
        let (settings, findings) = resolve_ui_settings(
            &ui,
            (
                chrono::NaiveTime::MIN,
                chrono::FixedOffset::east_opt(0).expect("zero"),
            ),
        );
        assert_eq!(findings, Vec::new());
        let colon = taskboard_ui_slint::keymap::stroke::parse(":").expect("parses");
        assert_eq!(
            settings.keymap.resolve(
                &[taskboard_ui_slint::actions::ContextId::ViNormal],
                &[colon]
            ),
            Some(taskboard_ui_slint::actions::ActionId::AppMenu),
            "vi preset bound app.menu in the normal-mode context"
        );
    }

    /// T-18's app half — the resolved hand-over carries the config's
    /// width map verbatim (the mount's uuid→newtype conversion is the
    /// T-13 load path).
    #[test]
    fn resolve_copies_the_stack_width_map() {
        let board = uuid::Uuid::from_u128(0x1001);
        let stack = uuid::Uuid::from_u128(0x2001);
        let ui = crate::config::UiSection {
            stack_widths: BTreeMap::from([(board, BTreeMap::from([(stack, 240u32)]))]),
            ..crate::config::UiSection::default()
        };
        let (settings, _) = resolve_ui_settings(
            &ui,
            (
                chrono::NaiveTime::MIN,
                chrono::FixedOffset::east_opt(0).expect("zero"),
            ),
        );
        assert_eq!(
            settings
                .stack_widths
                .get(&board)
                .and_then(|s| s.get(&stack)),
            Some(&240u32)
        );
    }

    /// T6 — the lenient read at the hand-over (#412): a valid table
    /// hands a minted `Some`; degenerate dimensions (a zero width, an
    /// absurd `2^40` height) hand `None` — and the call still returns
    /// (a hostile geometry never blocks UI start, Q15/D3).
    #[test]
    fn resolve_mints_the_window_geometry_leniently() {
        let geometry = crate::config::WindowGeometryTable {
            x: -1920,
            y: 0,
            width: 1366,
            height: 768,
        };
        let (settings, _) = resolve_ui_settings(
            &crate::config::UiSection {
                window_geometry: Some(geometry),
                ..crate::config::UiSection::default()
            },
            (
                chrono::NaiveTime::MIN,
                chrono::FixedOffset::east_opt(0).expect("zero"),
            ),
        );
        let minted = settings.window_geometry.expect("the valid table mints");
        assert_eq!(
            (minted.x(), minted.y(), minted.width(), minted.height()),
            (-1920, 0, 1366, 768),
            "the mint carries the stored values verbatim"
        );

        for hostile in [
            crate::config::WindowGeometryTable {
                x: 0,
                y: 0,
                width: 0,
                height: 100,
            },
            crate::config::WindowGeometryTable {
                x: 0,
                y: 0,
                width: 100,
                height: 1 << 40,
            },
            crate::config::WindowGeometryTable {
                x: i64::from(i32::MAX) + 1,
                y: 0,
                width: 100,
                height: 100,
            },
        ] {
            let (settings, _) = resolve_ui_settings(
                &crate::config::UiSection {
                    window_geometry: Some(hostile),
                    ..crate::config::UiSection::default()
                },
                (
                    chrono::NaiveTime::MIN,
                    chrono::FixedOffset::east_opt(0).expect("zero"),
                ),
            );
            assert_eq!(settings.window_geometry, None, "a hostile table hands None");
        }

        // Absent: `None`, same as refused.
        let (settings, _) = resolve_ui_settings(
            &crate::config::UiSection::default(),
            (
                chrono::NaiveTime::MIN,
                chrono::FixedOffset::east_opt(0).expect("zero"),
            ),
        );
        assert_eq!(settings.window_geometry, None);
    }

    /// A1 (r2 D32) — the board-selector resolution over a hand-built
    /// `AppState`: live boards only (deleted/archived drop), the
    /// binding's connector name rides the row, uuid round-trips.
    #[test]
    fn board_infos_resolve_live_boards_with_binding_names() {
        use taskboard_domain::{Board, BoardClocks, BoardId, RemoteBoardBinding, RemoteBoardId};
        let live_unbound = BoardId::from_uuid(uuid::Uuid::from_u128(0x1001));
        let live_bound = BoardId::from_uuid(uuid::Uuid::from_u128(0x1002));
        let archived = BoardId::from_uuid(uuid::Uuid::from_u128(0x1003));
        let deleted = BoardId::from_uuid(uuid::Uuid::from_u128(0x1004));
        let epoch = BoardClocks::epoch();
        let board = |id, title: &str, remote| Board {
            id,
            remote,
            title: title.to_owned(),
            color: taskboard_domain::Color::new(""),
            archived: id == archived,
            deleted: id == deleted,
            clocks: BoardClocks {
                title: epoch,
                color: epoch,
                archived: epoch,
                deleted: epoch,
            },
            remote_seen: None,
        };
        let mut state = taskboard_domain::AppState::default();
        state.boards.insert(
            live_bound,
            board(
                live_bound,
                "bound",
                Some(RemoteBoardBinding {
                    connector: taskboard_domain::ConnectorId::from("work"),
                    board: RemoteBoardId(7),
                }),
            ),
        );
        state
            .boards
            .insert(live_unbound, board(live_unbound, "free", None));
        state.boards.insert(archived, board(archived, "old", None));
        state.boards.insert(deleted, board(deleted, "gone", None));

        let rows = resolve_board_infos_from_state(&state);
        assert_eq!(rows.len(), 2, "only the live boards resolve");
        let bound = rows.iter().find(|r| r.name == "bound").expect("row");
        assert_eq!(bound.id, live_bound.as_uuid(), "uuid round-trips");
        assert_eq!(bound.bound_connector.as_deref(), Some("work"));
        let free = rows.iter().find(|r| r.name == "free").expect("row");
        assert_eq!(free.bound_connector, None);
    }
}

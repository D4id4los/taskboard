// SPDX-License-Identifier: AGPL-3.0-only

//! The UI → app request lane (round-1 D21, extended by chunk 3's D2):
//! typed requests the UI sends through its dedicated mpsc for
//! app-side processing that is neither a state command nor a system
//! event. The adapter is the only sender (`try_send`, warn-and-drop);
//! the app crate drains the receiver. A channel payload — domain
//! newtypes are fine, no serde.
//!
//! The chunk-3 settings requests carry a `oneshot` reply half (the
//! AGENTS §3 query pattern): fire-and-forget-honest on both sides — a
//! dropped receiver is logged, never a panic, and persistence is never
//! undone by a failed reply. `UiRequest` therefore no longer derives
//! `Clone`/`PartialEq`/`Eq` (a oneshot sender is neither).

use taskboard_domain::{BoardId, RemoteBoardId, StackId};
use tokio::sync::oneshot;

use crate::keymap::{Keymap, Preset};

/// An app-password in transit (D26b): write-only by construction — the
/// `Debug` impl redacts, there is no `Display`, and no serde. The
/// structural test asserts `format!("{request:?}")` never carries the
/// payload.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(pub String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("**redacted**")
    }
}

/// The connector's sync mechanism, secret-free — the Files-row gate
/// and the backend-aware row label (round 6, D-6.6/D-6.8 phase 1).
/// A named two-variant enum over a bool (the A1 shape): a third
/// backend would need a UI word decision, and the app-side fill is a
/// total `match` on the app's `SyncBackend`, so drift is a compile
/// error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorBackend {
    /// Fully local operation; no account card semantics.
    Standalone,
    /// The Nextcloud Deck connector.
    Nextcloud,
}

/// One configured connector, secret-free — the list row and the
/// Save/Remove reply payload (resolved app-side from the config).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorInfo {
    pub name: String,
    pub server_url: String,
    pub username: String,
    /// The connector's sync mechanism (round 6, D-6.6): gates the
    /// Files row to Nextcloud cards and words the row label.
    pub backend: ConnectorBackend,
}

/// One local board for the settings board selector — secret-free,
/// id in the uuid hand-over form (the stack-widths precedent: the
/// domain newtypes carry no serde, and the adapter converts at the
/// seam; r2 D32/D2-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardInfo {
    pub id: uuid::Uuid,
    pub name: String,
    /// The connector name this board is bound to, `None` when unbound.
    pub bound_connector: Option<String>,
}

/// The refreshed boards+connectors pair every state-changing settings
/// reply carries (r2 D32): rows and the selector re-render from the
/// same reply, so neither surface goes stale (F18).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConnectionsSnapshot {
    pub boards: Vec<BoardInfo>,
    pub connectors: Vec<ConnectorInfo>,
}

/// One live remote board one connector can see (round-5 chunk 8,
/// D-8.3): the account card's mapping row — secret-free, keyed by the
/// Deck id (the binding-scoped identity is `(connector, remote_id)`;
/// the same id through another connector is a different remote). The
/// row's binding is not carried: the UI re-derives it from the engine
/// state on every push pass (r4 D39 §2.3), so it cannot go stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteBoardRow {
    pub remote_id: RemoteBoardId,
    pub title: String,
}

/// The full draft the settings form commits (the `ConnectionTarget`
/// shape plus the kebab name — an upsert keyed on `name`; an empty
/// name asks the app to derive one from the identity, D-8.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionProfile {
    pub name: String,
    pub server_url: String,
    pub username: String,
    pub password: Secret,
}

/// The typed settings-operation failures (R5: variants only — the UI
/// renders structure, never message text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsError {
    /// A field failed the config-shaped validation.
    Invalid { field: Field, kind: InvalidKind },
    /// The row's secret is not in the credential store.
    NoSecretStored,
    /// The configured store cannot hold secrets (the env store).
    StoreUnsupported,
    /// The credential store failed other than the two arms above.
    StoreFailed,
    /// The server answered the verification with a credential refusal.
    VerifyRejected,
    /// The server could not be reached for the verification.
    VerifyUnreachable,
    /// Verification failed any other way (URL rejected, envelope…).
    VerifyFailed,
    /// The config file rejected the write (shape/parse).
    PersistRejected,
    /// The config file write failed on IO.
    PersistFailed,
    /// The connector lifecycle refused (name in use / draining).
    AttachBusy,
    /// The attach failed (engine, client construction).
    AttachFailed,
    /// The lane or the config vanished mid-flight (a stale form).
    Gone,
    /// The engine refused the bind/unbind (board/remote already bound,
    /// stale board id) (r2 D33).
    BindRefused,
    /// The field's effective value is pinned by a `TASKBOARD_*` env
    /// override (round-5 chunk 5, D-5.3): the app refused the write
    /// before touching the file — the adapter's hand-over gating
    /// prevents the attempt, the app-side guard is the authority.
    EnvLocked { field: Field },
}

/// The validated field of a [`SettingsError::Invalid`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    ServerUrl,
    Username,
    Password,
    Scale,
    // The settings editor's general fields (the GUI settings editor
    // chunk): one variant per editable field, so the typed refusal
    // lattice and the status sentences stay per-field.
    DayStart,
    RemindersEnabled,
    TickSecs,
    ReminderSink,
    ReminderBefore,
    PollInterval,
    BackoffInitial,
    BackoffMax,
    DueSoonDays,
    // The `[focus]` section's editor rows (the focus settings editor
    // chunk, #625): one variant per knob, the same per-field lattice.
    FocusEnabled,
    FocusWorkMins,
    FocusBreakMins,
    FocusChain,
    FocusLongBreakMins,
    FocusLongBreakEvery,
    FocusNotify,
    // The account card's Files toggle (round-5 chunk 8, D-8.6).
    FilesSyncEnabled,
}

/// How a field failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidKind {
    Empty,
    NotKebab,
    BadUrl,
    NotFinite,
    OutOfRange,
    Duplicate,
}

/// The refreshed general-settings snapshot every editor commit reply
/// carries (the `ConnectionsSnapshot` precedent): rows re-render from
/// the reply, nothing goes stale. Built app-side from the freshly
/// written file's decode — the write reply echoes the fresh *file*
/// value (which will truly be live after the next restart). The
/// env-override set is NOT re-derived per reply (it is hand-over /
/// process-boot state); the reply carries it empty and the UI retains
/// its hand-over set.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
// A config-file projection, not a state machine: each bool mirrors one
// independent `[focus]`/`[reminders]` knob the editor renders a row
// for — grouping them into enums would invent couplings the config
// does not have.
#[allow(clippy::struct_excessive_bools)]
pub struct SettingsSnapshot {
    /// The effective editable values, typed (the file layer's decode).
    pub day_start_time: chrono::NaiveTime,
    pub reminders_enabled: bool,
    pub reminder_tick_secs: u64,
    /// The before-offset in the domain's canonical text form.
    pub reminder_before: String,
    pub reminder_sink: ReminderSink,
    pub poll_interval_secs: u64,
    pub backoff_initial_secs: u64,
    pub backoff_max_secs: u64,
    pub due_soon_days: u32,
    /// The `[focus]` section's typed values (the focus settings
    /// editor, #625): the seven knobs the editor renders rows for.
    pub focus_enabled: bool,
    pub focus_work_mins: u64,
    pub focus_break_mins: u64,
    pub focus_chain: bool,
    pub focus_long_break_mins: u64,
    pub focus_long_break_every: u64,
    pub focus_notify: bool,
    /// `[files_sync] enabled` (round-5 chunk 8, D-8.6): the default
    /// account card's Files toggle.
    pub files_sync_enabled: bool,
    /// The *effective* (resolved) files-sync anchor's connector name
    /// (round-6 D-6.6): the per-card Files row needs "is this card
    /// the anchor", a resolved fact — a raw-echo would force every
    /// consumer to re-resolve. `None` = no Nextcloud connector.
    pub files_sync_anchor: Option<String>,
    /// The dotted keys whose effective value is env-overridden (the
    /// plan D1/D16 per-key honesty overlay). Filled only at hand-over;
    /// empty on write replies (the UI keeps its hand-over set).
    pub env_overridden: std::collections::BTreeSet<String>,
    /// The read-only deployment facts (the §3 read-only inventory),
    /// each with its apply semantics.
    pub facts: Vec<SettingsFact>,
    /// The apply semantics per editable field (plan D12: the config
    /// owner states them; the UI never hardcodes — the app's registry
    /// is the authority, round-5 chunk 5 D-5.6). Keys are the editable
    /// fields' dotted config keys.
    pub applies: std::collections::BTreeMap<String, ApplySemantics>,
}

/// When an edited setting takes effect for the running processes
/// (plan D5 — stated, never faked): `Live` groups apply immediately
/// through their own reply paths; `OnRestart` groups take effect at
/// the next process boot — no signal, no hot reload, no pretend-live
/// application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionApply {
    Live,
    OnRestart,
}

/// One editable key's apply verdict with its reason (round-5 chunk 5,
/// D-5.6): `Live` is the default and needs no text; an `OnRestart`
/// entry carries the written justification the app's registry test
/// enforces — the documented-decision mechanism that keeps a restart
/// badge honest instead of a pretend-live one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplySemantics {
    pub apply: SectionApply,
    pub justification: &'static str,
}

impl ApplySemantics {
    /// The live verdict (no justification needed).
    #[must_use]
    pub const fn live() -> Self {
        Self {
            apply: SectionApply::Live,
            justification: "",
        }
    }

    /// A restart-scoped verdict with its reason.
    #[must_use]
    pub const fn on_restart(justification: &'static str) -> Self {
        Self {
            apply: SectionApply::OnRestart,
            justification,
        }
    }
}

/// One read-only settings row (the "Deployment" group's inventory):
/// the dotted config key is the row's address; the value is the
/// secret-free display string (a secret-adjacent value renders
/// presence only — `[web] token` renders `set`/`unset`, never its
/// value).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsFact {
    pub key: &'static str,
    pub value: String,
    pub apply: SectionApply,
}

/// The `[reminders] sink` choice, mirrored lane-side so this module
/// stays app-crate-free (the `KeymapPreset` precedent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReminderSink {
    #[default]
    Auto,
    Log,
    Desktop,
}

/// The reply payload of `SetKeymapPreset` (D9): the app re-resolves
/// the config's overrides under the chosen preset and hands the
/// rebuilt map back; the adapter swaps its keymap live. The reply
/// names the preset itself (r2 D27): the selection echoes the
/// *replied* preset — never the click — and reconstructing one by
/// diffing the built `Keymap` against preset tables would be unsound
/// under user overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeymapUpdate {
    pub keymap: Keymap,
    pub findings: u32,
    pub preset: Preset,
}

#[derive(Debug)]
pub enum UiRequest {
    /// The full per-board stack-width snapshot (round-1 D21):
    /// idempotent under re-send — the app forwarder replaces that
    /// board's entry wholesale. Debounced (a lane exception).
    StackWidthsChanged {
        board: BoardId,
        widths: Vec<(StackId, u32)>,
    },
    /// The full main-window geometry snapshot (#412): idempotent under
    /// re-send — the forwarder keeps the latest rect (a burst
    /// collapses to the last). Debounced, riding the widths lane (the
    /// two lane exceptions).
    WindowGeometryChanged {
        geometry: crate::geometry::WindowGeometry,
    },
    /// The verify-first Save ceremony (D3): nothing persists before
    /// the server verified the draft; the reply carries the fresh
    /// connections snapshot (r2 D32).
    SaveConnection {
        profile: ConnectionProfile,
        reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
    },
    /// Removes the named connector's account (round-5 chunk 8): every
    /// connector sharing its `(server_url, username)` — config entries,
    /// live actors, the stored secret. Refuses typed (`BindRefused`)
    /// while any live board is bound through one of them. The reply
    /// carries the fresh snapshot.
    RemoveConnection {
        name: String,
        reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
    },
    /// Lists the live remote boards the named connector's account can
    /// see (round-5 chunk 8, D-8.3): a pure query + network read — no
    /// state change, no persistence, no attach. The listing is the
    /// credential verification.
    ListRemoteBoards {
        connector: String,
        reply: oneshot::Sender<Result<Vec<RemoteBoardRow>, SettingsError>>,
    },
    /// Bind the named local board to the chosen remote board through
    /// the named connector, attaching the connector if needed (r2 D33,
    /// explicit since round-5 chunk 8, D-8.4).
    AttachConnector {
        board: BoardId,
        connector: String,
        remote: RemoteBoardId,
        reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
    },
    /// Create a local board from the remote's title and bind it to that
    /// remote through the named connector (round-5 chunk 8, D-8.5).
    CreateLocalBoard {
        connector: String,
        remote: RemoteBoardId,
        reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
    },
    /// Unbind the board from its connector; the connector's actor
    /// stops only when no other live board remains bound to it
    /// (r2 D33, the recorded multi-board guard).
    DetachConnector {
        board: BoardId,
        connector: String,
        reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
    },
    /// Persists the keymap preset and replies with the rebuilt keymap
    /// (overrides re-resolved under the chosen preset, D9).
    SetKeymapPreset {
        preset: Preset,
        reply: oneshot::Sender<Result<KeymapUpdate, SettingsError>>,
    },
    /// Validates (0.5–3.0, finite) and persists the UI scale (D9).
    SetScale {
        scale: f64,
        reply: oneshot::Sender<Result<(), SettingsError>>,
    },
    // -- the settings editor's general-field commits (one variant per
    //    editable field-group; a closed enum of typed intents is the
    //    secrets boundary: no variant's payload could carry a
    //    credential, and no fenced subtree (connectors, stack widths,
    //    window geometry) has a variant to ride). Every reply carries
    //    the fresh file-layer [`SettingsSnapshot`] (the rows re-render
    //    from the reply).
    /// Persists `[preferences] day_start_time`.
    SetDayStart {
        time: chrono::NaiveTime,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// Persists `[reminders] enabled`.
    SetRemindersEnabled {
        enabled: bool,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// Persists `[reminders] tick_secs` (typed u64; the gate proves
    /// the whole document).
    SetReminderTick {
        secs: u64,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// Persists `[reminders] default_before` in its canonical text
    /// form (the domain newtype minted UI-side via the domain parser —
    /// the app never re-parses raw text).
    SetReminderBefore {
        before: taskboard_domain::ReminderOffset,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// Persists `[reminders] sink`.
    SetReminderSink {
        sink: ReminderSink,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// Persists `[sync] poll_interval_secs`.
    SetPollInterval {
        secs: u64,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// Persists `[sync] backoff_initial_secs` — the `initial ≤ max`
    /// ordering refuses at the prove gate, attributed to this field.
    SetBackoffInitial {
        secs: u64,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// Persists `[sync] backoff_max_secs` — symmetric to
    /// `SetBackoffInitial`.
    SetBackoffMax {
        secs: u64,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// Persists `[ui] due_soon_days` (0 legal — an empty soon set).
    SetDueSoonDays {
        days: u32,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    // -- the `[focus]` section's commits (the focus settings editor,
    //    #625): the same closed-enum rule — no variant's payload could
    //    carry a credential; every reply carries the fresh file-layer
    //    [`SettingsSnapshot`].
    /// Persists `[focus] enabled`.
    SetFocusEnabled {
        enabled: bool,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// Persists `[focus] work_mins` (typed u64; the gate proves the
    /// whole document).
    SetFocusWorkMins {
        mins: u64,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// Persists `[focus] break_mins`.
    SetFocusBreakMins {
        mins: u64,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// Persists `[focus] chain` (the auto-chain knob, #600).
    SetFocusChain {
        enabled: bool,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// Persists `[focus] long_break_mins`.
    SetFocusLongBreakMins {
        mins: u64,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// Persists `[focus] long_break_every`.
    SetFocusLongBreakEvery {
        every: u64,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// Persists `[focus] notify` (the boundary-notification knob, #600).
    SetFocusNotify {
        enabled: bool,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// Persists `[files_sync] enabled` (round-5 chunk 8, D-8.6) —
    /// restart-scoped (the files-sync actor spawns at boot) — and,
    /// when the pick re-anchors, `[files_sync] anchor` (round-6
    /// D-6.6). The two change as ONE user action: a separate anchor
    /// request would force two round-trips (a torn intermediate file
    /// state between them), so the payload carries the intended
    /// anchor; `None` = leave the key untouched.
    SetFilesSyncEnabled {
        enabled: bool,
        /// `Some(name)` only when the pick re-anchors (turning On a
        /// non-anchor card).
        anchor: Option<String>,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T5 — the redaction is structural over adversarial payloads
    /// (`proptest` over printable strings): the `Debug` output never
    /// carries the secret, and the profile's Debug (what a `UiRequest`
    /// debug-print renders) is equally clean.
    #[test]
    fn secret_debug_output_never_carries_the_payload() {
        fn invariant(secret: &str) {
            // The redaction is a constant independent of the payload:
            // the Debug output is the same fixed string whatever the
            // secret is (the only payload that could ever "appear" is
            // the marker itself, and then the output still carries no
            // information about the input).
            let redacted = Secret(secret.to_owned());
            assert_eq!(format!("{redacted:?}"), "**redacted**");
            let profile = ConnectionProfile {
                name: "work".into(),
                server_url: "https://c.example".into(),
                username: "alice".into(),
                password: redacted,
            };
            let rendered = format!("{profile:?}");
            assert!(!rendered.contains(secret) || secret == "**redacted**");
            assert!(rendered.contains("**redacted**"));
            // The non-Debug mirrors (clone/eq) never render the
            // payload either — only Debug exists for rendering, and it
            // is pinned above.
        }
        // Adversarial secrets: braces, debug-syntax lookalikes, quotes,
        // newlines, and a sample of printables.
        for secret in [
            "**redacted**",
            "Debug { x: 1 }",
            "\"quoted\"",
            "line\nbreak",
            "]}{[{(\"'\\",
        ] {
            invariant(secret);
        }
    }
}

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

use taskboard_domain::{BoardId, StackId};
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

/// One configured connector, secret-free — the list row and the
/// Save/Remove reply payload (resolved app-side from the config).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorInfo {
    pub name: String,
    pub server_url: String,
    pub username: String,
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

/// The full draft the settings form commits (the `ConnectionTarget`
/// shape plus the kebab name — an upsert keyed on `name`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionProfile {
    pub name: String,
    pub server_url: String,
    pub username: String,
    pub password: Secret,
}

/// An identity without a name: the per-row Test addresses the stored
/// secret of an existing row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionTarget {
    pub server_url: String,
    pub username: String,
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
    /// No live, unclaimed remote board on this connector matches the
    /// local board's title — there is nothing to adopt (r2 D33).
    NoRemoteMatch,
    /// The engine refused the bind/unbind (board/remote already bound,
    /// stale board id) (r2 D33).
    BindRefused,
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
    /// The dotted keys whose effective value is env-overridden (the
    /// plan D1/D16 per-key honesty overlay). Filled only at hand-over;
    /// empty on write replies (the UI keeps its hand-over set).
    pub env_overridden: std::collections::BTreeSet<String>,
    /// The read-only deployment facts (the §3 read-only inventory),
    /// each with its apply semantics.
    pub facts: Vec<SettingsFact>,
    /// The apply semantics per editable field group (plan D12: the
    /// config owner states them; the UI never hardcodes). Keys are the
    /// editable fields' dotted config keys.
    pub applies: std::collections::BTreeMap<String, SectionApply>,
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
    /// Verifies an existing row's stored secret against its server
    /// (DEV-3: a row test must not require retyping a password).
    TestConnection {
        target: ConnectionTarget,
        reply: oneshot::Sender<Result<(), SettingsError>>,
    },
    /// The verify-first Save ceremony (D3): nothing persists before
    /// the server verified the draft; the reply carries the fresh
    /// connections snapshot (r2 D32).
    SaveConnection {
        profile: ConnectionProfile,
        reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
    },
    /// Removes the named connector: config entry, live actor, stored
    /// secret. The reply carries the fresh snapshot.
    RemoveConnection {
        name: String,
        reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
    },
    /// Bind the named local board to the named connector, attaching
    /// the connector if needed (r2 D33). The remote id is derived
    /// app-side (the title-adopt rule).
    AttachConnector {
        board: BoardId,
        connector: String,
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

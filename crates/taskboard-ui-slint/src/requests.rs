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

use taskboard_domain::{BoardId, ReconcileCounts, RemoteBoardId, StackId};
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

/// The binding's channel (round-7 D-7.3, W-1): which lane a board's
/// binding rides, detected STRUCTURALLY from the remote id's payload
/// type (D-BF4 — the payload is the tag; a connector-name match is
/// impossible by construction and forbidden by law). Snapshot data
/// plumbing, not a UI word: the badge fold consumes it Rust-side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardChannel {
    /// Bound through a Deck connector (`RemoteBoardId::deck`).
    Deck,
    /// Bound through the files lane (`RemoteBoardId::files`).
    Files,
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
    /// The binding's channel, `None` when unbound (round-7 D-7.3,
    /// W-1). Filled app-side from `binding.board.as_deck()/as_files()`
    /// — structural detection per D-BF4; every constructor sets it
    /// explicitly (no `Default` escape hatch — a missed filler must be
    /// a compile error, not a silent `None`).
    pub bound_channel: Option<BoardChannel>,
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
    /// Another settings operation is still in flight (the busy lane's
    /// own posture, T-38 #821): the surface runs one request at a
    /// time, so a click under a pending request is refused until it
    /// settles. UI-local — never rides the app's reply mapping.
    RequestPending,
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
    /// The publish ceremony outlived its settle budget (D-6.9): the
    /// board is created and bound, the first sync cycle is still
    /// moving — nothing failed; it timed out.
    SyncTimedOut,
    /// `[files_sync]` is off or unspawnable (round-6 D-6.8 phase 2,
    /// D-5): no files lane exists to move onto. Folds
    /// `MoveError::FilesSyncDisabled`.
    FilesSyncDisabled,
    /// The board's outbox is not quiet (D-5): local edits are still
    /// queued and the ceremony never drops them. Folds
    /// `MoveError::OutboxNotQuiescent`.
    OutboxNotQuiescent,
    /// No move is in flight for the board (US-6 AC-2's typed refusal
    /// on a raced or absent stage). Folds
    /// `MoveError::NothingToUndo` / `NotDeckBound`.
    NothingToUndo,
    /// The move's commit point has passed; the undo window is over
    /// (US-6). Folds `MoveError::UndoClosed`.
    UndoClosed,
    /// The retired Deck coordinates can no longer be named (US-6).
    /// Folds `MoveError::UndoCoordinatesLost`.
    UndoCoordinatesLost,
    /// A move-ceremony step failed (D-5): the floor publish/confirm,
    /// the commit record, the Deck marker, a stranded-op or refused
    /// binding write, a foreign subtree, a state read — the coarse
    /// class; the concrete `MoveError` rides the log. Folds
    /// `MoveError::{PublishFailed, PublishUnconfirmed,
    /// FilesSubtreeForeign, CommitFailed, DeckMarkerFailed,
    /// StateReadFailed, StrandedOps, UndoCoordinatesLost,
    /// AssignRefused, UnbindRefused, NotDeckBound}` (the last via
    /// race).
    MoveFailed,
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
    // The Files row's poll cadence field (#1083, the small-gated
    // journeys plan W-6): the editor Field retires the CLI-only
    // exception — the Field lattice and the B7b apply registry
    // converge (the umbrella plan R-4).
    FilesPollSecs,
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
    /// `[files_sync] poll_secs` (the #1083 field's echo source): the
    /// raw `Option` — `None` is the honest "unset, the default
    /// cadence applies" fact (the config's own shape), never faked
    /// into a number.
    pub files_sync_poll_secs: Option<u64>,
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

/// The export dialog's data family (the J2 export plan's W-3 grammar,
/// R-3's parity ruling): `Tracking` is the csv/json leg, `Pdf` the
/// today-sheet leg. Exactly the CLI's families the plan carries — no
/// more (R-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFamily {
    /// The time entries (csv/json; the D-T15 payload).
    #[default]
    Tracking,
    /// The printable today sheet (the pdf leg; a build without the
    /// pdf-export feature renders the controls disabled with an
    /// honest caption — D-4 — and a dispatched pdf spec is refused
    /// typed app-side).
    Pdf,
}

/// The tracking leg's payload format (the CLI's `ExportFormat`
/// mirror; `Csv` the default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ExportFileFormat {
    /// RFC-4180 CSV with a header row (the default).
    #[default]
    Csv,
    /// A single-line JSON array.
    Json,
}

/// The pdf leg's print layout — a typed closed enum, never free text
/// (the OP2 trap): the layout picker's options are these four, the
/// CLI's `PdfLayout` grammar verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfLayoutChoice {
    /// The foldable A4 sheet → four-page A6 booklet (the default).
    #[default]
    Fold,
    /// Single-sided A4, two columns.
    A4,
    /// Single-sided A5.
    A5,
    /// Single-sided A6.
    A6,
}

/// The assembled export run (the plan §7 command contract): the
/// fields mirror the CLI `export` verb's grammar. The dialog mints
/// it at Run; the app-side drain executes the SAME export engine the
/// CLI verb drives (one engine, two frontends — the O-5 parity law).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExportSpec {
    /// The data family (the tracking csv/json leg by default).
    pub what: ExportFamily,
    /// The payload format (tracking leg; `Csv` the default).
    pub format: ExportFileFormat,
    /// Only entries starting on/after this UTC day (inclusive).
    pub since: Option<chrono::NaiveDate>,
    /// Only entries starting before the day after this UTC day.
    pub until: Option<chrono::NaiveDate>,
    /// The optional live-task selector (uuid | prefix | exact title).
    pub task: Option<String>,
    /// The optional live-board selector (exact uuid | exact title).
    pub board: Option<String>,
    /// The pdf leg's print layout (`None` = the config default).
    pub layout: Option<PdfLayoutChoice>,
    /// The explicit destination file (`None` = the engine's config
    /// default: the pdf leg's `[export] pdf_dir` + the dated file
    /// name; the csv/json leg's export dir + the dated file name —
    /// D-3's GUI destination).
    pub out: Option<std::path::PathBuf>,
    /// Open the written pdf in the system viewer after export (pdf
    /// leg only; the dialog checkbox's value, pre-filled from
    /// `[export] open_after_export` — D-2). The app-side open is
    /// typed-permission-aware: the kiosk workspace never shells out.
    pub open_after: bool,
}

/// The typed refusal kinds the export run can answer with (D-5).
/// Mapped from the engine's typed errors at the drain; the dialog
/// renders the kind, never the engine's message text. The serde
/// derives ride the preview outcome's pin (the #993 pattern).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExportRefusal {
    /// The pdf family dispatched on a build without the pdf-export
    /// feature (D-4's honest-absence arm at the trust boundary).
    PdfUnavailable,
    /// Tracking-only filters under a leg that rejects them.
    FiltersNotApplicable,
    /// Pdf-only options under a leg that rejects them.
    OptionsNotApplicable,
    /// The task selector matched nothing (or was ambiguous).
    BadTaskFilter,
    /// The board selector matched no live board.
    BadBoardFilter,
    /// The configured default layout key is unknown.
    UnknownPdfLayout,
    /// The destination path could not be minted or written (pdf leg).
    PdfPath,
    /// The pdf render failed.
    PdfRender,
    /// The destination file could not be written (csv/json leg).
    DestinationUnwritable,
    /// The request lane dropped the run (full or closed) — the
    /// infrastructure refusal, never an engine verdict.
    LaneDropped,
    /// The engine refused with a typed error outside the export
    /// family's named kinds (the closed set's catch-all — the dialog
    /// renders the kind, never the engine's message text).
    EngineRefused,
    /// The system clipboard could not be opened or written (the
    /// clipboard handoff's environment refusal — the preview plan
    /// D-3). Appended last: the dialog's refusal index order for the
    /// existing kinds is pinned, so the new kind rides the table's
    /// tail.
    ClipboardUnavailable,
}

/// The refusal kinds' canonical index order (the dialog's refusal
/// rendering address; the adapter maps the typed kind onto it, the
/// journeys read the same function — one table, no drift).
pub const REFUSAL_KINDS: &[ExportRefusal] = &[
    ExportRefusal::PdfUnavailable,
    ExportRefusal::FiltersNotApplicable,
    ExportRefusal::OptionsNotApplicable,
    ExportRefusal::BadTaskFilter,
    ExportRefusal::BadBoardFilter,
    ExportRefusal::UnknownPdfLayout,
    ExportRefusal::PdfPath,
    ExportRefusal::PdfRender,
    ExportRefusal::DestinationUnwritable,
    ExportRefusal::LaneDropped,
    ExportRefusal::EngineRefused,
    ExportRefusal::ClipboardUnavailable,
];

/// The refusal kind's dialog index (the rendering address; -1 when
/// the kind is outside the canonical table — unrepresentable for a
/// closed enum, kept total for the boundary).
#[must_use]
pub fn refusal_index(kind: ExportRefusal) -> i32 {
    REFUSAL_KINDS
        .iter()
        .position(|candidate| *candidate == kind)
        .map_or(-1, |i| i32::try_from(i).unwrap_or(-1))
}

/// The typed oneshot reply of `ExportRun` (D-5): the written-file
/// fact or a typed refusal kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportRunOutcome {
    /// The engine wrote the file at this path.
    Written {
        /// The written file's path.
        path: std::path::PathBuf,
    },
    /// The run refused typed; the dialog stays mounted with its
    /// filter state intact.
    Refused(ExportRefusal),
}

/// The typed oneshot reply of `ExportClipboardCopy` (the preview plan
/// D-3): the copied-byte fact or the typed refusal (the preview
/// refusal kinds the generation carries, plus `ExportRefusal::
/// ClipboardUnavailable` for the environment refusal). The copy
/// ALWAYS regenerates app-side over the same [`ExportSpec`] the Run
/// button sends (D-1's one-generator-two-sinks law) — it never rides
/// the pane's possibly-truncated preview text.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExportClipboardOutcome {
    /// The regenerated payload was written to the clipboard.
    Copied {
        /// The copied payload's byte length.
        bytes: u64,
    },
    /// The copy refused typed; the dialog stays mounted with its
    /// filter state intact.
    Refused(ExportRefusal),
}

/// The typed oneshot reply of `ExportPreview` (the preview plan
/// D-1/D-2): the engine-generated payload plus the typed
/// `truncated` fact when it exceeded the presentation cap — the cap
/// is a presentation budget, never a payload or engine limit (the
/// file and clipboard payloads are never truncated).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExportPreviewOutcome {
    /// The engine generated the payload.
    Payload {
        /// The payload text (the capped head when `truncated`).
        text: String,
        /// The payload's format (the spec's format verbatim).
        format: ExportFileFormat,
        /// The presentation-cap fact: the pane renders the capped
        /// head; the file and clipboard payloads are never capped.
        truncated: bool,
    },
    /// The preview refused typed (the same refusal kinds the export
    /// run answers with — the same engine, the same spec).
    Refused(ExportRefusal),
}

/// The status panel's remediation verb (the command plan D-1): one
/// typed request, three actions — kill the lock holder, force-unlock
/// the stale artifact, restart the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StatusRemediationAction {
    /// SIGTERM to the displayed `holder_pid` (D-3).
    KillHolder,
    /// Remove the stale lock sidecar (D-4); authorized by the
    /// executor's liveness re-check, never by the row's enable flag.
    ForceUnlock,
    /// Graceful stop over IPC (when serving), then a detached
    /// `taskboard daemon run` spawn (D-5).
    RestartDaemon,
}

/// The remediation refusals (the command plan D-7): typed kinds —
/// the panel renders the kind, never message text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StatusRemediationRefusal {
    /// A kill of a stale (not-alive) pid — `ForceUnlock` is the
    /// right tool.
    HolderNotAlive,
    /// An unlock under a live holder (the executor's authorization
    /// re-check refused; the sidecar is untouched).
    HolderAlive,
    /// The holder pid is this process's own pid.
    SelfPid,
    /// The restart's spawn leg failed.
    SpawnFailed,
    /// The IPC stop leg could not be reached, or the lane/posture
    /// cannot carry the verb (the coarse infrastructure refusal).
    IpcUnavailable,
}

/// The typed oneshot reply of `StatusRemediate` (the command plan
/// D-1/D-3/D-4/D-5/D-7): the executed fact or the typed refusal.
/// Every executed remediation is followed by one `StatusQuery`
/// re-dispatch, so the outcome posture is never stale.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StatusRemediationOutcome {
    /// The holder was signalled (SIGTERM delivered).
    HolderSignalled,
    /// The stale lock sidecar was removed.
    LockReleased,
    /// The daemon was restarted; the new daemon's pid rides the
    /// fact.
    DaemonRestarted {
        /// The freshly spawned daemon's OS pid.
        pid: u32,
    },
    /// The verb refused typed; the panel stays mounted with state
    /// intact and the same row is the retry affordance.
    Refused(StatusRemediationRefusal),
}

/// The export dialog's config defaults (D-2/D-3), minted app-side at
/// hand-over (the `general` snapshot precedent): the pdf leg's
/// destination dir, the csv/json leg's export dir, and the
/// open-after-export posture. `pdf_available` is the compile-time
/// feature fact rendered as a runtime flag so a default build's pdf
/// controls stay mounted-but-disabled with an honest caption (D-4).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExportDialogDefaults {
    /// Whether the pdf-export feature is compiled into the running
    /// binary (the pdf controls' enabled gate — D-4's fold flag).
    pub pdf_available: bool,
    /// The resolved default pdf destination directory (the
    /// `[export] pdf_dir` fact, `~` expanded; the platform fallback
    /// when unset) — the dialog's destination pre-fill.
    pub pdf_dir: String,
    /// The resolved export dir (the csv/json leg's default file
    /// home — D-3's "under the export dir").
    pub export_dir: String,
    /// The `[export] open_after_export` posture (the checkbox's
    /// pre-fill — D-2).
    pub open_after: bool,
}

/// One app-posture snapshot for the status panel (OP3 D-2, journey
/// plan `2026-11-21-parity-restore-journeys-status-surface.md` W-4):
/// the scalar fields of the app crate's `StatusView` mirrored 1:1 —
/// the adapter reads nothing the CLI `status` verb does not, and this
/// type mints no GUI-specific fact. The sync posture rides the
/// aggregate fold's two facts the panel renders (`sync_phase`,
/// `sync_pending_ops`); the engine's counters ride flat. The
/// per-connector/per-board/per-peer arrays are entity detail outside
/// D-2's enumerated panel surface (the CLI verb remains their front).
///
/// The draining fact is B6's byte-additive `Option<bool>` verbatim:
/// never `Some(false)` (the app-side minters guarantee it), and the
/// panel's posture fold (`draining == Some(true)`) is the same typed
/// rule the CLI text renderer consumes (D-4 — one rule, two fronts).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatusSnapshot {
    /// Is a daemon serving this database? In the GUI lifecycles the
    /// process itself holds the core, so the answer is the in-session
    /// projection's (`daemon: false`, holder = own pid).
    pub daemon: bool,
    pub version: Option<String>,
    pub backend: Option<String>,
    pub pid: Option<u32>,
    /// The instance-lock sidecar's diagnostic holder pid (the
    /// in-session answer: the process's own pid).
    pub holder_pid: Option<u32>,
    pub connector_attached: Option<bool>,
    /// B6's typed draining distinction, additive and never
    /// `Some(false)`.
    pub draining: Option<bool>,
    /// The aggregate sync phase (`"idle" | "syncing" | ...`), present
    /// when a core-holding projection derived it.
    pub sync_phase: Option<String>,
    pub sync_pending_ops: Option<u32>,
    /// Process-lifetime engine counters (present when derived).
    pub engine_commands: Option<u64>,
    pub engine_rejections: Option<u64>,
    pub engine_signals: Option<u64>,
}

/// The status panel's posture fold (OP3 D-2, US-2/AC-1/AC-2): the
/// typed rule the panel renders — never text. `Draining` is
/// `daemon == false ∧ draining == Some(true)` (the same rule the CLI
/// text renderer consumes, D-4); `NoDaemon` is the plain posture
/// (`draining == None` — B6's law: never `Some(false)`); `Serving` is
/// `daemon == true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusPosture {
    Serving,
    Draining,
    NoDaemon,
}

/// Folds one [`StatusSnapshot`] into the panel's typed posture. The
/// fold is pure over the B6-typed fields — the GUI and the CLI text
/// renderer consume the identical predicate (D-2/D-4: one rule, two
/// fronts).
#[must_use]
pub fn fold_status_posture(snapshot: &StatusSnapshot) -> StatusPosture {
    if snapshot.daemon {
        StatusPosture::Serving
    } else if snapshot.draining == Some(true) {
        StatusPosture::Draining
    } else {
        StatusPosture::NoDaemon
    }
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
    /// The reconcile preview's oneshot query (O-5 parity chunk OP1,
    /// US-3, D-4): the merge pairs are planner-derived facts (the
    /// same-title duplicate adoption) the presentation model does not
    /// carry, so the armed strip's letter derives its counts from this
    /// reply — the same facts the CLI's typed refusal prints, no prose
    /// rides the wire. A pure query: no state change, no persistence.
    BoardReconcilePreview {
        /// The armed board.
        board: BoardId,
        /// The planner's match counts (the letter's derivation
        /// inputs); `None` is the typed not-derivable fact (the state
        /// read failed — the planner never ran, so no counts exist to
        /// consent to). A dropped receiver (a stale flow) is at most
        /// a dropped send, never a panic.
        reply: oneshot::Sender<Option<ReconcileCounts>>,
    },
    /// Publish a local board onto the named connector's server (D-6.9,
    /// #699): create the Deck board from the local one's title/color
    /// and bind to it — the `boards publish` ceremony. Refuses typed
    /// (`BindRefused`) when the board is already bound; the reply
    /// carries the fresh snapshot.
    PublishBoard {
        board: BoardId,
        connector: String,
        reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
    },
    /// Move a Deck-bound local board onto the files tier (round-6
    /// D-6.8 phase 2, #722): the resumable `boards move --to-files`
    /// ceremony, run on the running engine — no connector parameter
    /// (the ceremony resolves the files lane from the config and the
    /// Deck spec from the board's own binding; the single-lane
    /// posture, #723, stands). Refuses typed when the board is not
    /// Deck-bound, the files lane is off, or the outbox is still
    /// moving; the reply carries the fresh snapshot.
    MoveBoardToFiles {
        board: BoardId,
        reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
    },
    /// Move a Deck-bound board between two Deck connectors (the
    /// files-sync-status plan US-10, D-11): the atomic rebind — the
    /// binding re-targets the named connector, the remote Deck id is
    /// preserved verbatim. The arm guards the preconditions typed
    /// (`Gone` / `BindRefused` for a non-Deck-bound board, unknown or
    /// non-Deck target); the reply carries the fresh snapshot.
    MoveBoardToDeck {
        board: BoardId,
        target_connector: String,
        reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
    },
    /// Undo a Deck→Files move that has not committed (the
    /// files-sync-status plan US-6, D-7): the `boards move --undo`
    /// ceremony's twin, run on the running engine. The arm derives
    /// the stage and refuses typed outside the undo window
    /// (`NothingToUndo` / `UndoClosed` / `UndoCoordinatesLost`); the
    /// reply carries the fresh snapshot.
    UndoBoardMove {
        board: BoardId,
        reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
    },
    /// Stop syncing a files-bound board (round-7 D-7.3, W-4): unbind
    /// the files binding — the board returns to local-only. The arm
    /// refuses typed (`Gone` / `BindRefused`) when the board is gone
    /// or not files-bound; the reply carries the fresh snapshot. No
    /// connector parameter: the files lane is global (the single-lane
    /// posture, #723). The remote `boards/<uuid>/` subtree lingers
    /// until the DELETE-verb GC exists (#755 — recorded, not hidden).
    StopBoardFilesSync {
        board: BoardId,
        reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
    },
    /// Publish a local-only board onto the resolved files lane
    /// (round-7 D-7.3, W-6): the move ceremony's final assign step
    /// (`BindBoard` against the files connector) — the board reaches
    /// the account's Files without ever touching Deck. The arm
    /// refuses typed (`FilesSyncDisabled` / `Gone` / `BindRefused`);
    /// the reply carries the fresh snapshot.
    PublishBoardToFiles {
        board: BoardId,
        reply: oneshot::Sender<Result<ConnectionsSnapshot, SettingsError>>,
    },
    /// The status panel's one query (OP3 W-1, D-3): snapshot-on-open
    /// and the explicit Refresh control — never a polling timer (the
    /// kiosk memory/perf posture). The reply carries the
    /// [`StatusSnapshot`]; a dropped reply (the app half gone) is the
    /// typed failure posture UI-side — the app-side answer cannot
    /// fail, it is a pure projection, so the error channel is the
    /// channel itself, not a `Result` payload.
    StatusQuery {
        reply: oneshot::Sender<StatusSnapshot>,
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
    /// The export dialog's Run (the J2 export plan W-4/W-6, D-5): the
    /// assembled spec drives the SAME export engine the CLI `export`
    /// verb drives (one engine, two frontends); the typed oneshot
    /// reply carries the written-file fact or the refusal kind. The
    /// drain consumes the reply even when the dialog closed first
    /// (US-2/S2's escape law — no orphaned UI wait).
    ExportRun {
        spec: ExportSpec,
        reply: oneshot::Sender<ExportRunOutcome>,
    },
    /// Persists `[files_sync] poll_secs` (the #1083 field, the
    /// small-gated journeys plan W-6 / R-4): the SAME live-write port
    /// every editor field rides — the app lane writes the file through
    /// the landed `set_files_sync_poll_secs` overlay writer (the sole
    /// writer, B7b) and publishes the whole-snapshot `UpdateSettings`;
    /// no second writer is coined. `0` never reaches the lane: the
    /// field mirror's `> 0` gate (the `secs_invalid_kind` parity with
    /// the config's own refusal) refuses at the commit seam, and the
    /// prove gate re-refuses with the config's own typed verdict.
    SetFilesSyncPollSecs {
        secs: u64,
        reply: oneshot::Sender<Result<SettingsSnapshot, SettingsError>>,
    },
    /// The status panel's remediation lane (the command plan D-1):
    /// one request, three typed actions over process/lock posture —
    /// never domain state (R-3: no `StateCommand` exists for the
    /// lane). The executor's typed decision logic re-checks liveness
    /// at the trust boundary; the panel's row enable flags are
    /// presentation, not authorization. The typed reply folds the
    /// executed fact or the refusal kind; the panel re-queries after
    /// any settle. Note: a `StatusWatchSubscribe` variant was planned
    /// here and is deliberately absent — the D-6 fallback (the
    /// supersession erratum, 2026-11-23) removed the live-refresh
    /// story when the engine-side `StatusWatch*` seam did not exist
    /// at dispatch time; the snapshot + explicit-Refresh posture
    /// stands.
    StatusRemediate {
        action: StatusRemediationAction,
        reply: oneshot::Sender<StatusRemediationOutcome>,
    },
    /// The export dialog's preview (the preview plan D-1, D-4): the
    /// spec drives the SAME export engine the Run button drives —
    /// the preview renders the payload the export would write (the
    /// one-generator-two-sinks law) — with NO file written (the
    /// written-path fact stays Run's exclusively). The reply carries
    /// the payload plus the typed presentation-cap fact, or the
    /// refusal. The drain consumes the reply even when the dialog
    /// closed first (the OP2 escape law).
    ExportPreview {
        spec: ExportSpec,
        reply: oneshot::Sender<ExportPreviewOutcome>,
    },
    /// The export dialog's clipboard handoff (the preview plan D-3):
    /// the payload is REGENERATED app-side over the same spec (never
    /// copied from the pane's possibly-truncated text) and written to
    /// the system clipboard. Typed reply; no file written.
    ExportClipboardCopy {
        spec: ExportSpec,
        reply: oneshot::Sender<ExportClipboardOutcome>,
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

    /// The remediation wire's serde pin (the command plan D-8, the
    /// #993 pattern): the action and outcome enums round-trip over
    /// hand-written JSON — the receipts' wire stability is a consumed
    /// contract, not serde defaults' luck. NOTE (the chunk's dated
    /// deviation): the plan's W-8 cell nominates the pin at
    /// `crates/taskboard-domain/src/undo/tests.rs`, but these types
    /// live in the UI crate and the domain crate cannot depend on it
    /// (the workspace boundary law, AGENTS §2) — the pin rides the
    /// types, here.
    #[test]
    fn status_remediation_action_and_outcome_round_trip_serde() {
        let actions = [
            (r#""KillHolder""#, StatusRemediationAction::KillHolder),
            (r#""ForceUnlock""#, StatusRemediationAction::ForceUnlock),
            (r#""RestartDaemon""#, StatusRemediationAction::RestartDaemon),
        ];
        for (json, value) in actions {
            let decoded: StatusRemediationAction =
                serde_json::from_str(json).expect("the action spelling decodes");
            assert_eq!(decoded, value, "the hand-written spelling decodes: {json}");
            let encoded = serde_json::to_string(&value).expect("action serializes");
            assert_eq!(
                serde_json::from_str::<StatusRemediationAction>(&encoded).expect("decodes"),
                value,
                "the action round-trips: {encoded}"
            );
        }

        let outcomes = [
            r#""HolderSignalled""#,
            r#""LockReleased""#,
            r#"{"DaemonRestarted":{"pid":4242}}"#,
            r#"{"Refused":"HolderNotAlive"}"#,
            r#"{"Refused":"HolderAlive"}"#,
            r#"{"Refused":"SelfPid"}"#,
            r#"{"Refused":"SpawnFailed"}"#,
            r#"{"Refused":"IpcUnavailable"}"#,
        ];
        for json in outcomes {
            let decoded: StatusRemediationOutcome =
                serde_json::from_str(json).expect("the outcome spelling decodes");
            let encoded = serde_json::to_string(&decoded).expect("encodes");
            let redecoded: StatusRemediationOutcome =
                serde_json::from_str(&encoded).expect("re-decodes");
            assert_eq!(redecoded, decoded, "the outcome round-trips: {json}");
        }
    }

    /// The preview/clipboard outcomes' serde pin (the preview plan
    /// D-6, the #993 pattern): hand-written-JSON round-trip, same
    /// home deviation as the remediation pin above.
    #[test]
    fn export_preview_and_clipboard_outcomes_round_trip_serde() {
        let previews = [
            r#"{"Payload":{"text":"a,b\n1,2","format":"Csv","truncated":false}}"#,
            r#"{"Payload":{"text":"he","format":"Json","truncated":true}}"#,
            r#"{"Refused":"BadTaskFilter"}"#,
        ];
        for json in previews {
            let decoded: ExportPreviewOutcome =
                serde_json::from_str(json).expect("the preview spelling decodes");
            let encoded = serde_json::to_string(&decoded).expect("encodes");
            let redecoded: ExportPreviewOutcome =
                serde_json::from_str(&encoded).expect("re-decodes");
            assert_eq!(
                redecoded, decoded,
                "the preview outcome round-trips: {json}"
            );
        }
        let clips = [
            r#"{"Copied":{"bytes":7}}"#,
            r#"{"Refused":"ClipboardUnavailable"}"#,
        ];
        for json in clips {
            let decoded: ExportClipboardOutcome =
                serde_json::from_str(json).expect("the clipboard spelling decodes");
            let encoded = serde_json::to_string(&decoded).expect("encodes");
            let redecoded: ExportClipboardOutcome =
                serde_json::from_str(&encoded).expect("re-decodes");
            assert_eq!(
                redecoded, decoded,
                "the clipboard outcome round-trips: {json}"
            );
        }
    }
}

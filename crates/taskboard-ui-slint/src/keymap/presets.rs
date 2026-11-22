// SPDX-License-Identifier: AGPL-3.0-only

//! The three shipped keymap presets (chunk decision D8, design §5.4,
//! Q14): `standard` (CUA), `emacs` (C-x prefix families, source-
//! editor-authentic), and `vi` (normal-mode home-row motions, modes
//! as contexts). All three ship at core-verb coverage — no preset is
//! a second-class citizen, enforced by the contract tests in
//! [`crate::keymap`] (D21), not by this table.
//!
//! The exact strokes are a session-tunable draft: retuning re-runs
//! the contract tests and regenerates the keymap snapshots.

use crate::actions::{ActionId, ContextId};

/// A shipped preset. Serde naming lives app-side (the config enum);
/// this is the binding data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preset {
    #[default]
    Standard,
    Emacs,
    Vi,
}

impl Preset {
    /// The stable config/snapshot name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Emacs => "emacs",
            Self::Vi => "vi",
        }
    }

    /// The binding table: `(action, binding context, stroke
    /// spellings)`. Two rows may share `(action, context)` — that is
    /// the alternates shape (D7, e.g. emacs `menu.dismiss`). Row
    /// order is the resolver's chain-order contract — the shared
    /// window-local blocks chain at each preset's textual position
    /// (the C-5 precedence pin depends on the order).
    #[must_use]
    pub fn rows(self) -> Vec<(ActionId, ContextId, &'static [&'static str])> {
        match self {
            Self::Standard => chained(&[
                STANDARD_S0,
                HABIT_TODAY_ROWS,
                STANDARD_S0B,
                LABELS_CONTROL_WALK_ROWS,
                STANDARD_S1,
                CONTROL_WALK_ROWS,
                CONTROL_WALK_SETTINGS_ROWS,
                STANDARD_S2,
                TRACK_ENTRIES_ROWS,
                REVIEW_WALK_ROWS,
                STANDARD_S4,
                ALARM_WINDOW_ROWS,
                NOTIFICATION_ROWS,
            ]),
            Self::Emacs => chained(&[
                EMACS_S0,
                HABIT_TODAY_ROWS,
                EMACS_S0B,
                LABELS_CONTROL_WALK_ROWS,
                EMACS_S1,
                ALARM_WINDOW_ROWS,
                NOTIFICATION_ROWS,
            ]),
            Self::Vi => chained(&[
                VI_S0,
                HABIT_TODAY_ROWS,
                VI_S0B,
                CONTROL_WALK_ROWS,
                VI_S1,
                CONTROL_WALK_SETTINGS_ROWS,
                VI_S2,
                TRACK_ENTRIES_ROWS,
                REVIEW_WALK_ROWS,
                VI_S4,
                ALARM_WINDOW_ROWS,
                NOTIFICATION_ROWS,
                VI_S6,
                LABELS_CONTROL_WALK_ROWS,
            ]),
        }
    }

    /// The contexts this preset declares as masking (D11): walks that
    /// reach them stop. `ViInsert` deliberately shadows without
    /// masking (Ctrl-chords keep working while typing).
    #[must_use]
    pub const fn masks(self) -> &'static [ContextId] {
        // Every shipped preset masks below the open menu and the open
        // search palette (#348): both are modal surfaces; `ViInsert`
        // shadows without masking (D11).
        let _ = self;
        &[
            ContextId::Menu,
            ContextId::Palette,
            ContextId::FocusMode,
            ContextId::TrackEntries,
            ContextId::Labels,
            ContextId::ReviewWalk,
            ContextId::RecurEditor,
            ContextId::ExportDialog,
            // The alarm window (T-22, #805): a total mask like the
            // other modal surfaces — while the window holds, it owns
            // the keyboard exactly as a modal should (the window's
            // own key grammar consumes; nothing underneath fires).
            ContextId::AlarmWindow,
        ]
    }
}

/// The labels overlay's Tab/Shift+Tab walk pair (all presets).
const LABELS_CONTROL_WALK_ROWS: &[(ActionId, ContextId, &[&str])] = &[
    (ActionId::NavNextControl, ContextId::Labels, &["Tab"]),
    (ActionId::NavPrevControl, ContextId::Labels, &["Shift+Tab"]),
];

/// The habit family's Today-context rows (all presets — the O-8
/// parity law, D4/D7 + HF1/W-11 + W-5): the chunk-8 reorder pair and
/// arm/undo keys, the HF1 skip/pause/resume trio, and the W-5 inline
/// quota edit. The per-preset `HabitCreate` opener keeps its own
/// preset-specific comment upstream of this block.
const HABIT_TODAY_ROWS: &[(ActionId, ContextId, &[&str])] = &[
    // Chunk 8 (D4/D7, #934): keyboard reorder through the chunk-6
    // one-shot pair — the row-scoped movement grammar; Today's
    // shifted arrows are free (the board's shift-walk selection
    // extension is Board-scoped) and the arrows' shift-fold leniency
    // covers printable rows only, so the shifted arrows reach the
    // keymap intact. The arm/undo keys are the free chords (the
    // chunk-1 case-law spelling).
    (ActionId::HabitMoveUp, ContextId::Today, &["Shift+Up"]),
    (ActionId::HabitMoveDown, ContextId::Today, &["Shift+Down"]),
    (
        ActionId::HabitConvertArm,
        ContextId::Today,
        &["Ctrl+Shift+c"],
    ),
    (ActionId::HabitUndoLog, ContextId::Today, &["Ctrl+Shift+u"]),
    // HF1 (W-11, #955/#958): the skip/pause/resume trio — Today
    // context rows in every preset (the O-8 parity law; the free
    // Ctrl+Shift chords, the chunk-1 case-law spelling).
    (ActionId::HabitSkip, ContextId::Today, &["Ctrl+Shift+o"]),
    (ActionId::HabitPause, ContextId::Today, &["Ctrl+Shift+y"]),
    (ActionId::HabitResume, ContextId::Today, &["Ctrl+Shift+w"]),
    // The inline quota edit (W-5/O-8, #961): the focused row's key
    // twin of the phrase click — one stroke per preset (the O-8
    // parity law). The rebase moved it off HF2's original Ctrl+Shift+w
    // (HF1's HabitResume landed there first; a duplicated chord
    // cross-consumes) to the free q — the quota mnemonic.
    (
        ActionId::HabitQuotaEdit,
        ContextId::Today,
        &["Ctrl+Shift+q"],
    ),
    // HF3 (W-11, US-7/S1, O-8 parity): the history opener is bound in
    // every preset (the no-second-class gate) — the Today-context
    // chord is free (Ctrl+Shift+t is ViewToday's global row; the
    // Today chain resolves Today first, so no shadow).
    (ActionId::HabitHistory, ContextId::Today, &["Ctrl+Shift+i"]),
    // US-8 (W-9, #1057, O-8 parity): the streak-repair chord — the
    // focused habit's key twin of the card strip's repair button
    // (click ≡ key through the shared one-shot mint). The Today
    // chain's `d` was free in every preset; the case-law spelling.
    (ActionId::HabitRepair, ContextId::Today, &["Ctrl+Shift+d"]),
];

/// The control walk's Tab/Shift+Tab pairs over the four view contexts (the standard/vi letter) — the Settings pair chains separately where each preset's textual position puts it.
const CONTROL_WALK_ROWS: &[(ActionId, ContextId, &[&str])] = &[
    (ActionId::NavNextControl, ContextId::Today, &["Tab"]),
    (ActionId::NavPrevControl, ContextId::Today, &["Shift+Tab"]),
    (ActionId::NavNextControl, ContextId::Board, &["Tab"]),
    (ActionId::NavPrevControl, ContextId::Board, &["Shift+Tab"]),
    (ActionId::NavNextControl, ContextId::StackDetail, &["Tab"]),
    (
        ActionId::NavPrevControl,
        ContextId::StackDetail,
        &["Shift+Tab"],
    ),
    (ActionId::NavNextControl, ContextId::TaskDetail, &["Tab"]),
    (
        ActionId::NavPrevControl,
        ContextId::TaskDetail,
        &["Shift+Tab"],
    ),
];

/// The Settings control-walk pair (vi's textual position differs from standard's).
const CONTROL_WALK_SETTINGS_ROWS: &[(ActionId, ContextId, &[&str])] = &[
    (ActionId::NavNextControl, ContextId::Settings, &["Tab"]),
    (
        ActionId::NavPrevControl,
        ContextId::Settings,
        &["Shift+Tab"],
    ),
];

/// The time-entries overlay's ladder + walk (the standard/vi letter).
const TRACK_ENTRIES_ROWS: &[(ActionId, ContextId, &[&str])] = &[
    (
        ActionId::TaskTrackEntryDismiss,
        ContextId::TrackEntries,
        &["Escape"],
    ),
    (ActionId::NavNextControl, ContextId::TrackEntries, &["Tab"]),
    (
        ActionId::NavPrevControl,
        ContextId::TrackEntries,
        &["Shift+Tab"],
    ),
];

/// The review walk's cursor/back rows (the standard/vi letter).
const REVIEW_WALK_ROWS: &[(ActionId, ContextId, &[&str])] = &[
    (ActionId::ReviewWalkCursorUp, ContextId::ReviewWalk, &["Up"]),
    (
        ActionId::ReviewWalkCursorDown,
        ContextId::ReviewWalk,
        &["Down"],
    ),
    (ActionId::ReviewWalkBack, ContextId::ReviewWalk, &["Escape"]),
];

/// The alarm window's local grammar (all presets, the r5 letter).
const ALARM_WINDOW_ROWS: &[(ActionId, ContextId, &[&str])] = &[
    (ActionId::MenuPrevious, ContextId::AlarmWindow, &["Up"]),
    (ActionId::MenuNext, ContextId::AlarmWindow, &["Down"]),
    (ActionId::AlarmDismiss, ContextId::AlarmWindow, &["d"]),
    (ActionId::AlarmSnooze, ContextId::AlarmWindow, &["s"]),
    (ActionId::AlarmCancel, ContextId::AlarmWindow, &["c"]),
    (ActionId::MenuDismiss, ContextId::AlarmWindow, &["Escape"]),
];

/// The notification surface's grammar (all presets; the focus-restoration plan US-6/W-6, D-N1).
const NOTIFICATION_ROWS: &[(ActionId, ContextId, &[&str])] = &[
    (
        ActionId::MenuPrevious,
        ContextId::NotificationSurface,
        &["Up"],
    ),
    (
        ActionId::MenuNext,
        ContextId::NotificationSurface,
        &["Down"],
    ),
    (
        ActionId::MenuDismiss,
        ContextId::NotificationSurface,
        &["Escape"],
    ),
    (
        ActionId::NotificationDismiss,
        ContextId::NotificationSurface,
        &["d"],
    ),
    (
        ActionId::NotificationDone,
        ContextId::NotificationSurface,
        &["e"],
    ),
    (
        ActionId::NotificationSnooze,
        ContextId::NotificationSurface,
        &["s"],
    ),
    (
        ActionId::NotificationMute,
        ContextId::NotificationSurface,
        &["m"],
    ),
    (
        ActionId::NotificationCancel,
        ContextId::NotificationSurface,
        &["c"],
    ),
    (
        ActionId::NotificationTrackStart,
        ContextId::NotificationSurface,
        &["t"],
    ),
    (
        ActionId::NotificationOpen,
        ContextId::NotificationSurface,
        &["o"],
    ),
];

/// The preset chain (the parts in the resolver's row order — the
/// chain-order contract; the shared window-local blocks sit at each
/// preset's textual position).
fn chained(
    parts: &[&'static [(ActionId, ContextId, &'static [&'static str])]],
) -> Vec<(ActionId, ContextId, &'static [&'static str])> {
    parts.iter().flat_map(|part| part.iter().copied()).collect()
}

/// `standard` — CUA conventions: arrows *are* this preset's
/// convention, Ctrl-chords for the global verbs.
static STANDARD_S0: &[(ActionId, ContextId, &[&str])] = &[
    // Global
    (ActionId::AppUndo, ContextId::Global, &["Ctrl+Shift+z"]),
    (ActionId::AppRedo, ContextId::Global, &["Ctrl+y"]),
    // The picker entry (UF2, #353): the branches chord rides behind
    // the undo/redo pair; the vi preset stays chord-free (its
    // alphabet is dense — the menu row is the vi path, O-8's case
    // law: no forced chord where the alphabet objects).
    (
        ActionId::AppUndoBranches,
        ContextId::Global,
        &["Ctrl+Shift+e"],
    ),
    (ActionId::AppMenu, ContextId::Global, &["Ctrl+Shift+m"]),
    (ActionId::AppExit, ContextId::Global, &["Ctrl+Shift+q"]),
    (ActionId::AppSettings, ContextId::Global, &["Ctrl+,"]),
    (ActionId::AppHelp, ContextId::Global, &["F1"]),
    (ActionId::CaptureQuick, ContextId::Global, &["Ctrl+Shift+k"]),
    (
        ActionId::PaletteSearch,
        ContextId::Global,
        &["Ctrl+Shift+f"],
    ),
    // Focus mode (#496): one stroke from anywhere (the overlay is the
    // desktop's concentration affordance); the in-overlay rows use
    // the menu vocabulary — arrows, not j/k, for the same reason the
    // palette records (the panel has a text field), and the toggle
    // itself closes (Escape and the stroke both dismiss).
    (
        ActionId::FocusModeToggle,
        ContextId::Global,
        &["Ctrl+Shift+j"],
    ),
    // Chunk 10 (D11): the today jump is one stroke from anywhere, all
    // presets (P10's promise); view.inbox and task.due-today stay
    // menu-reachable without default strokes.
    (ActionId::ViewToday, ContextId::Global, &["Ctrl+Shift+t"]),
    // The plan-next-day surface (#622, D-6): the one new binding —
    // the opener (toggle; the planner-context Escape row closes).
    (
        ActionId::ViewPlanNextDay,
        ContextId::Global,
        &["Ctrl+Shift+p"],
    ),
    // The review walk's opener (UX R-4 D-8, #804): the CUA shift-chord
    // family, R free in this preset's Global (Ctrl+Shift+r is the recurrence
    // editor's TaskDetail binding — different context, no contract
    // contact).
    (ActionId::ReviewOpen, ContextId::Global, &["Ctrl+Shift+r"]),
    (
        ActionId::ViewPlanNextDay,
        ContextId::PlanNextDay,
        &["Escape"],
    ),
    // The planner's pane-switch grammar (J3 R-5, round 9 chunk 9):
    // left/right flip the boxes ↔ pool focus; the arrows are free in
    // this context (the settings slider's raw-arrow ownership does
    // not reach the planner).
    (ActionId::NavFocusLeft, ContextId::PlanNextDay, &["Left"]),
    (ActionId::NavFocusRight, ContextId::PlanNextDay, &["Right"]),
    (ActionId::NavNextView, ContextId::Global, &["Ctrl+PageDown"]),
    (ActionId::NavPrevView, ContextId::Global, &["Ctrl+PageUp"]),
    (ActionId::NavFocusBoard, ContextId::Global, &["Alt+1"]),
    // The board-return's alternate (J3 US-4, round 9 chunk 11 — the
    // D-table's Back ruling): Alt+Left, the shell's back convention —
    // the nav-back verb's second spelling (D7 alternates shape; the
    // primary stays Alt+1 here).
    (ActionId::NavFocusBoard, ContextId::Global, &["Alt+Left"]),
    // Round 3 (D29, #231): the banner verbs' preset bindings —
    // snooze/bell (Ctrl+Shift+U avoided: the IBus unicode composer);
    // key-reachable exactly when a banner target is armed, NoTarget
    // without one (the typed machinery).
    (
        ActionId::ReminderSnooze,
        ContextId::Global,
        &["Ctrl+Shift+s"],
    ),
    (ActionId::ReminderMute, ContextId::Global, &["Ctrl+Shift+b"]),
    // Round 2 (D16): move-cancel is all-preset via Escape (Global);
    // the emacs C-g alternate lives in Board, never Global (the
    // exact-match-first resolution keeps `app.redo`'s C-g prefix
    // reachable everywhere the Board context is absent).
    (ActionId::TaskMoveCancel, ContextId::Global, &["Escape"]),
    // Board
    // The directional rows (J3 US-1, round 9 chunk 9): the arrows
    // ARE this preset's directional-focus keys (R-1); up/down
    // re-point the item walk (the same walk at Board — the table's
    // up/down body is FocusNext/Prev's), left/right are new (stack
    // step). Shift+Down/Up are the explicit extend rows (#875): the
    // named-key intercept stays as the fallback front for
    // base-glyph-delivering layouts — the route-level rows win
    // (the intercept returns before route, so exactly one front
    // fires; the seam test pins it).
    (ActionId::NavFocusLeft, ContextId::Board, &["Left"]),
    (ActionId::NavFocusRight, ContextId::Board, &["Right"]),
    (ActionId::NavFocusUp, ContextId::Board, &["Up"]),
    (ActionId::NavFocusDown, ContextId::Board, &["Down"]),
    (ActionId::NavExtendNext, ContextId::Board, &["Shift+Down"]),
    (ActionId::NavExtendPrev, ContextId::Board, &["Shift+Up"]),
    (ActionId::NavFirstItem, ContextId::Board, &["Ctrl+Home"]),
    (ActionId::NavLastItem, ContextId::Board, &["Ctrl+End"]),
    (ActionId::TaskComplete, ContextId::Board, &["Space"]),
    (ActionId::TaskReopen, ContextId::Board, &["Ctrl+Space"]),
    (ActionId::TaskCreate, ContextId::Board, &["N"]),
    (ActionId::StackCreate, ContextId::Board, &["S"]),
    (ActionId::TaskEdit, ContextId::Board, &["Return"]),
    (ActionId::TaskMove, ContextId::Board, &["M"]),
    (ActionId::TaskDelete, ContextId::Board, &["Delete"]),
    // Chunk 10 (D3/D11): the today view carries the same item walk and
    // task-verb strokes as the board (the D3 registry widenings made
    // them available; these rows make them resolvable).
    // The directional rows (J3 US-1, round 9 chunk 9): up/down
    // re-point the today item walk (the same walk); left/right are
    // the control-walk first cut (R-4).
    (ActionId::NavFocusLeft, ContextId::Today, &["Left"]),
    (ActionId::NavFocusRight, ContextId::Today, &["Right"]),
    (ActionId::NavFocusUp, ContextId::Today, &["Up"]),
    (ActionId::NavFocusDown, ContextId::Today, &["Down"]),
    (ActionId::NavFirstItem, ContextId::Today, &["Ctrl+Home"]),
    (ActionId::NavLastItem, ContextId::Today, &["Ctrl+End"]),
    (ActionId::TaskComplete, ContextId::Today, &["Space"]),
    (ActionId::TaskReopen, ContextId::Today, &["Ctrl+Space"]),
    (ActionId::TaskEdit, ContextId::Today, &["Return"]),
    (ActionId::TaskMove, ContextId::Today, &["M"]),
    (ActionId::TaskDelete, ContextId::Today, &["Delete"]),
    // The today re-commit (UX R-4 D-8, #796): the bare mnemonic — the
    // round-10 D11 "menu-reachable without default strokes" note is
    // deliberately overturned (the accepted triage classed the
    // absence MAJOR: a today-re-commit verb living only in the
    // two-tier menu fails keyboard/mouse parity for the Today
    // surface's core loop).
    (ActionId::TaskDueToday, ContextId::Today, &["T"]),
    // Round 7 (D-7.2): the create form's opener — keyboard parity is
    // the accessibility ledger's demand, so all three presets carry
    // it (the habit family's deliberate exception).
    (ActionId::HabitCreate, ContextId::Today, &["Ctrl+Shift+h"]),
];

/// `standard` continued after the shared [`HABIT_TODAY_ROWS`] block.
static STANDARD_S0B: &[(ActionId, ContextId, &[&str])] = &[
    (ActionId::BoardNext, ContextId::Board, &["b"]),
    (ActionId::BoardPrevious, ContextId::Board, &["B"]),
    // The switcher flyout's opener (UX R-4 D-4, #794): the jump-to
    // mnemonic — a target-free stroke now opens the bar's flyout
    // (the b/B walk steps, Ctrl+Shift+g jumps).
    (ActionId::BoardSelect, ContextId::Board, &["Ctrl+Shift+g"]),
    (ActionId::BoardCreate, ContextId::Board, &["Ctrl+Shift+n"]),
    (ActionId::BoardRename, ContextId::Board, &["Ctrl+F2"]),
    // The color picker's opener (UX R-4 D-1, #791): the CUA function
    // sequence continues after F2's rename (the F3/F4 pickers'
    // precedent, one context over).
    (ActionId::BoardColorEdit, ContextId::Board, &["Ctrl+F3"]),
    // The empty-board preset (UX R-4 D-5, #795): the bare mnemonic —
    // free in this preset's Board context (verified against the
    // conflict contract); the dispatch gate makes a non-empty board
    // the typed no-op.
    (ActionId::BoardDefaultPreset, ContextId::Board, &["P"]),
    (
        ActionId::BoardDelete,
        ContextId::Board,
        &["Ctrl+Shift+Delete"],
    ),
    // Round 2 (D17/D18): Ctrl+Shift+arrows (not the design's
    // Ctrl+Alt-arrows — the GNOME/KDE workspace switch seizes those)
    // sit one modifier beside the width pair's Ctrl+arrows.
    (ActionId::StackNext, ContextId::Board, &["Ctrl+Shift+Right"]),
    (ActionId::StackPrev, ContextId::Board, &["Ctrl+Shift+Left"]),
    (ActionId::StackRename, ContextId::Board, &["F2"]),
    (ActionId::StackMove, ContextId::Board, &["Ctrl+Alt+m"]),
    (ActionId::StackDelete, ContextId::Board, &["Ctrl+Delete"]),
    // Round-1 chunk 2 (D18): the stack-width verbs share the CUA
    // Ctrl+arrows in the Board context.
    (ActionId::StackWider, ContextId::Board, &["Ctrl+Right"]),
    (ActionId::StackNarrower, ContextId::Board, &["Ctrl+Left"]),
    // StackDetail: the directional rows (J3 US-1, round 9 chunk 9) —
    // up/down re-point the item walk; left/right are the control walk.
    (ActionId::NavFocusLeft, ContextId::StackDetail, &["Left"]),
    (ActionId::NavFocusRight, ContextId::StackDetail, &["Right"]),
    (ActionId::NavFocusUp, ContextId::StackDetail, &["Up"]),
    (ActionId::NavFocusDown, ContextId::StackDetail, &["Down"]),
    (
        ActionId::NavFirstItem,
        ContextId::StackDetail,
        &["Ctrl+Home"],
    ),
    (ActionId::NavLastItem, ContextId::StackDetail, &["Ctrl+End"]),
    (ActionId::TaskCreate, ContextId::StackDetail, &["N"]),
    // TaskDetail: the item-walk member rows are RETIRED (J3 R-3,
    // 2026-11-19 — the subject walk is gone, census-dated) and the
    // directional rows (J3 US-1, round 9 chunk 9) take the detail's
    // keys: the CONTROL walk now (the degenerate ladder geometry is
    // the interpretation table's recorded reading). The title re-arm
    // keeps its F2.
    (ActionId::NavFocusLeft, ContextId::TaskDetail, &["Left"]),
    (ActionId::NavFocusRight, ContextId::TaskDetail, &["Right"]),
    (ActionId::NavFocusUp, ContextId::TaskDetail, &["Up"]),
    (ActionId::NavFocusDown, ContextId::TaskDetail, &["Down"]),
    // Round 3 (W-SM-d, F26): the title re-arm — F2, the CUA rename
    // convention, matching this preset's own `StackRename = F2`.
    (ActionId::TaskTitleEdit, ContextId::TaskDetail, &["F2"]),
    // The recurrence editor's opener (round-7 D-7.1): Ctrl+Shift+r, the
    // repeat mnemonic — a `TaskTitleEdit`-class editing verb, the one
    // series verb with a preset binding (the row verbs stay
    // pointer-grade).
    (
        ActionId::SeriesEdit,
        ContextId::TaskDetail,
        &["Ctrl+Shift+r"],
    ),
    // The priority pickers (UX R-3 D-1, #788): F3/F4 — the CUA
    // sequence continues after F2's rename (free in this preset's
    // TaskDetail; verified against the conflict contract).
    (ActionId::TaskImportanceEdit, ContextId::TaskDetail, &["F3"]),
    (ActionId::TaskUrgencyEdit, ContextId::TaskDetail, &["F4"]),
    // The labels overlay's opener (UX R-3 D-2, #789): Ctrl+Shift+l — the
    // labels mnemonic, verified free in all three presets'
    // TaskDetail at this HEAD (stage-1 D-4's recorded check).
    (
        ActionId::LabelManage,
        ContextId::TaskDetail,
        &["Ctrl+Shift+l"],
    ),
    // The roster-only mount's stroke (tag-color plan D-7, US-2): the
    // same action carries the same mnemonic across its contexts.
    (ActionId::LabelManage, ContextId::Board, &["Ctrl+Shift+l"]),
    // The detail editors' shortcuts (keymap-parity plan US-3, D-1/D-4):
    // the CUA function sequence continues after F4's pickers — F5/F6
    // the date editors, F7 the description editor, F8 the rule add
    // (F1-F4 and F12 were bound; F5-F11 free in this preset).
    (ActionId::TaskDueEdit, ContextId::TaskDetail, &["F5"]),
    (ActionId::TaskScheduledEdit, ContextId::TaskDetail, &["F6"]),
    (
        ActionId::TaskDescriptionEdit,
        ContextId::TaskDetail,
        &["F7"],
    ),
    (ActionId::ReminderRuleAdd, ContextId::TaskDetail, &["F8"]),
    // The clock pair (keymap-parity plan US-3 AC-3, D-1) at TaskDetail
    // and Today (the today-context rows for the tracking ids).
    (ActionId::TaskTrackStart, ContextId::TaskDetail, &["F9"]),
    (ActionId::TaskTrackStop, ContextId::TaskDetail, &["F10"]),
    (ActionId::TaskTrackStart, ContextId::Today, &["F9"]),
    (ActionId::TaskTrackStop, ContextId::Today, &["F10"]),
    // --- J3 US-4/US-5, round 9 chunk 11 (the D-table rows; plan
    // .artifacts/plans/2026-11-19-taskdetail-shortcuts-plan.md §3.2):
    // the tracker verbs reach Board/StackDetail (US-5, the same
    // spellings the r8 rows landed at TaskDetail/Today), the ONE
    // smart toggle binds in all four card contexts (R-8), and the
    // detail's remaining Tab-only verbs get strokes — Complete/
    // Reopen mirror this preset's Board/Today spellings, entries
    // takes F12 (the only free F-key), the effort pair takes the
    // free Ctrl+e / Ctrl+Shift+E (distinct-from-Ctrl+e per R-6 case
    // law).
    (ActionId::TaskComplete, ContextId::TaskDetail, &["Space"]),
    (ActionId::TaskReopen, ContextId::TaskDetail, &["Ctrl+Space"]),
    (
        ActionId::TaskTrackToggle,
        ContextId::TaskDetail,
        &["Ctrl+F9"],
    ),
    (ActionId::TaskTrackToggle, ContextId::Board, &["Ctrl+F9"]),
    (
        ActionId::TaskTrackToggle,
        ContextId::StackDetail,
        &["Ctrl+F9"],
    ),
    (ActionId::TaskTrackToggle, ContextId::Today, &["Ctrl+F9"]),
    (ActionId::TaskTrackStart, ContextId::Board, &["F9"]),
    (ActionId::TaskTrackStop, ContextId::Board, &["F10"]),
    (ActionId::TaskTrackStart, ContextId::StackDetail, &["F9"]),
    (ActionId::TaskTrackStop, ContextId::StackDetail, &["F10"]),
    (ActionId::TaskTrackEntries, ContextId::TaskDetail, &["F12"]),
    (ActionId::TaskEffortEdit, ContextId::TaskDetail, &["Ctrl+e"]),
    (
        ActionId::TaskEffortClear,
        ContextId::TaskDetail,
        &["Ctrl+Shift+e"],
    ),
    // The series row's chords (US-11, W-12, #1067) and the
    // tracker-kind toggle (US-12, W-13, #1068): the parity remainder's
    // detail rows. The pause chord's spelling is the R-7 move — the
    // plan's Ctrl+Shift+r candidate is SeriesEdit's landed round-7
    // chord in this preset's TaskDetail, the collision authority fails
    // the suite, and the spelling moves (never a quiet drop); the free
    // `p` carries the pause mnemonic in every preset. Ctrl+Shift+u is
    // free in this preset's detail chain. The kind toggle shadows the
    // global CaptureQuick chord inside the detail chain only — the
    // context-precedence shadow is the recorded cost (the emacs
    // C-c Ctrl+s snooze precedent), the identity is distinct
    // (Ctrl+Shift+k ≠ Ctrl+k).
    (
        ActionId::SeriesPause,
        ContextId::TaskDetail,
        &["Ctrl+Shift+p"],
    ),
    (
        ActionId::SeriesResume,
        ContextId::TaskDetail,
        &["Ctrl+Shift+u"],
    ),
    (
        ActionId::TaskToggleTracker,
        ContextId::TaskDetail,
        &["Ctrl+Shift+k"],
    ),
    // The scheduled field's quick verbs (UX-R8 chunk-5 US-1/AC-5,
    // D-10): the CUA F-key family continues after F10 — F11 free in
    // this preset (F1-F10 and F12 were bound; verified). The
    // Ctrl+Shift+U fallback is avoided (the IBus unicode composer
    // precedent, the snooze note's rule); the shift-chord F11
    // continuation carries the clear twin.
    (ActionId::TaskScheduleToday, ContextId::Board, &["F11"]),
    (ActionId::TaskScheduleToday, ContextId::Today, &["F11"]),
    (
        ActionId::TaskScheduleToday,
        ContextId::StackDetail,
        &["F11"],
    ),
    (ActionId::TaskScheduleToday, ContextId::TaskDetail, &["F11"]),
    (
        ActionId::TaskScheduleTomorrow,
        ContextId::Board,
        &["Ctrl+F11"],
    ),
    (
        ActionId::TaskScheduleTomorrow,
        ContextId::Today,
        &["Ctrl+F11"],
    ),
    (
        ActionId::TaskScheduleTomorrow,
        ContextId::StackDetail,
        &["Ctrl+F11"],
    ),
    (
        ActionId::TaskScheduleTomorrow,
        ContextId::TaskDetail,
        &["Ctrl+F11"],
    ),
    (
        ActionId::TaskUnschedule,
        ContextId::Board,
        &["Ctrl+Shift+F11"],
    ),
    (
        ActionId::TaskUnschedule,
        ContextId::Today,
        &["Ctrl+Shift+F11"],
    ),
    (
        ActionId::TaskUnschedule,
        ContextId::StackDetail,
        &["Ctrl+Shift+F11"],
    ),
    (
        ActionId::TaskUnschedule,
        ContextId::TaskDetail,
        &["Ctrl+Shift+F11"],
    ),
    // The overlay's own context (D-2): the Escape ladder + the tab
    // walk (the entries-overlay's per-view row pattern).
    (
        ActionId::LabelOverlayDismiss,
        ContextId::Labels,
        &["Escape"],
    ),
    // The export dialog's Escape rung (the J2 export plan W-1's
    // close callback): the dialog context is a total mask; the
    // close dispatches nothing (US-1/AC-2).
    (
        ActionId::ExportDialogClose,
        ContextId::ExportDialog,
        &["Escape"],
    ),
];

static STANDARD_S1: &[(ActionId, ContextId, &[&str])] = &[
    // Menu
    (ActionId::MenuNext, ContextId::Menu, &["Down"]),
    (ActionId::MenuPrevious, ContextId::Menu, &["Up"]),
    (ActionId::MenuInvoke, ContextId::Menu, &["Return"]),
    (ActionId::MenuDismiss, ContextId::Menu, &["Escape"]),
    // Palette (#348): the same CUA vocabulary as the menu; Return is
    // bound in the head context so the capture layer consumes it
    // before the field's `accepted` — Activate is jump-to.
    (ActionId::MenuNext, ContextId::Palette, &["Down"]),
    (ActionId::MenuPrevious, ContextId::Palette, &["Up"]),
    (ActionId::MenuInvoke, ContextId::Palette, &["Return"]),
    (ActionId::MenuDismiss, ContextId::Palette, &["Escape"]),
    // Focus mode (#496): the overlay's own context — Escape and the
    // toggle stroke close, arrows walk the switcher, Return starts
    // the selected task.
    (ActionId::FocusModeToggle, ContextId::FocusMode, &["Escape"]),
    (
        ActionId::FocusModeToggle,
        ContextId::FocusMode,
        &["Ctrl+Shift+j"],
    ),
    (ActionId::MenuNext, ContextId::FocusMode, &["Down"]),
    (ActionId::MenuPrevious, ContextId::FocusMode, &["Up"]),
    (ActionId::MenuInvoke, ContextId::FocusMode, &["Return"]),
    // D5: insert-mode exit — the same single key vi has (the insert
    // chain masks the view context; printables reach the field).
    (ActionId::ModeNormal, ContextId::ViInsert, &["Escape"]),
    // Round-1 chunk 3 (D28b): Escape closes the settings surface
    // (dialog semantics; the opener is the global Ctrl+, route).
    (ActionId::AppSettings, ContextId::Settings, &["Escape"]),
    (
        ActionId::StatusPanelOpen,
        ContextId::StatusPanel,
        &["Escape"],
    ),
    // The Settings directional rows are DELIBERATELY ABSENT in this
    // preset (J3 US-1 deviation, recorded 2026-11-19): the settings
    // pane's slider owns the raw arrows as its keyboard step
    // (the_slider_steps journey — a pointer-parity affordance the
    // arrows-as-directional plan did not anticipate); the settings
    // directional grammar rides emacs' chord rows, and the routed
    // NavFocus* ids still interpret as the control walk (the table's
    // Settings row stands).
    // Round 3 (D25): the control walk — Tab/Shift+Tab are the CUA
    // convention and were unbound in every context; the five view
    // contexts only (under insert the chain masks them and Tab stays
    // the fields' native traversal).
];

static STANDARD_S2: &[(ActionId, ContextId, &[&str])] = &[
    // Round 3 (D26): the scroll actions — PageDown/PageUp, the CUA
    // paging convention, in the five view contexts (never Global:
    // Global is chain-visible under insert and the page keys are
    // native text-field motions there).
    (ActionId::ViewScrollDown, ContextId::Today, &["PageDown"]),
    (ActionId::ViewScrollUp, ContextId::Today, &["PageUp"]),
    (ActionId::ViewScrollDown, ContextId::Board, &["PageDown"]),
    (ActionId::ViewScrollUp, ContextId::Board, &["PageUp"]),
    (
        ActionId::ViewScrollDown,
        ContextId::StackDetail,
        &["PageDown"],
    ),
    (ActionId::ViewScrollUp, ContextId::StackDetail, &["PageUp"]),
    (
        ActionId::ViewScrollDown,
        ContextId::TaskDetail,
        &["PageDown"],
    ),
    (ActionId::ViewScrollUp, ContextId::TaskDetail, &["PageUp"]),
    (ActionId::ViewScrollDown, ContextId::Settings, &["PageDown"]),
    (ActionId::ViewScrollUp, ContextId::Settings, &["PageUp"]),
    // Round 3 (D25): the picker keys — the CUA arrows and navigation on the DatePicker context (armed exactly while the grid holds focus). The time steppers stay button-only.
    (ActionId::PickerLeft, ContextId::DatePicker, &["Left"]),
    (ActionId::PickerRight, ContextId::DatePicker, &["Right"]),
    (ActionId::PickerUp, ContextId::DatePicker, &["Up"]),
    (ActionId::PickerDown, ContextId::DatePicker, &["Down"]),
    (
        ActionId::PickerPrevMonth,
        ContextId::DatePicker,
        &["PageUp"],
    ),
    (
        ActionId::PickerNextMonth,
        ContextId::DatePicker,
        &["PageDown"],
    ),
    (ActionId::PickerMonthStart, ContextId::DatePicker, &["Home"]),
    (ActionId::PickerMonthEnd, ContextId::DatePicker, &["End"]),
    (ActionId::PickerCommit, ContextId::DatePicker, &["Return"]),
    // The time-entries overlay (r4 chunk 10, D-TS2): the modal's own
    // three rows — the Escape ladder + the panel walk (the per-view
    // row pattern; pointer-driven otherwise, no preset binding for
    // the panel verbs).
];

static STANDARD_S4: &[(ActionId, ContextId, &[&str])] = &[
    // The alarm window (T-22, #805): the walk + the three verbs, the
    // window-local grammar's registry mirror (the window's
    // FocusScope handles the same keys; the ids carry them in the
    // keymap contract).
];

/// `emacs` — default-emacs-authentic (the review's tmux standard):
/// `C-x C-c` exits, `C-h` is the help prefix, `M-x` is the command
/// surface, org-mode owns the `C-c` prefix for our domain verbs, and
/// navigation rides `C-n`/`C-p`.
static EMACS_S0: &[(ActionId, ContextId, &[&str])] = &[
    // Global
    (ActionId::AppExit, ContextId::Global, &["Ctrl+x", "Ctrl+c"]),
    (ActionId::AppUndo, ContextId::Global, &["Ctrl+x", "u"]),
    (ActionId::AppRedo, ContextId::Global, &["Ctrl+g", "Ctrl+/"]),
    (
        ActionId::AppUndoBranches,
        ContextId::Global,
        &["Ctrl+x", "b"],
    ),
    (ActionId::AppMenu, ContextId::Global, &["Alt+x"]),
    (ActionId::AppHelp, ContextId::Global, &["Ctrl+h"]),
    (ActionId::AppSettings, ContextId::Global, &["Ctrl+x", "c"]),
    (ActionId::CaptureQuick, ContextId::Global, &["Ctrl+c", "c"]),
    (ActionId::PaletteSearch, ContextId::Global, &["Ctrl+s"]),
    // Focus mode (#496): the CUA toggle chord (Ctrl+j is free in this
    // preset's Global — emacs' own C-j has no domain verb here).
    (
        ActionId::FocusModeToggle,
        ContextId::Global,
        &["Ctrl+Shift+j"],
    ),
    (ActionId::ViewToday, ContextId::Global, &["Ctrl+Shift+t"]),
    // The review walk's opener (UX R-4 D-8, #804): the C-c family's
    // free Ctrl+v second stroke (the scroll pair owns bare C-v/Alt-v
    // in the view contexts — the chord is unambiguous).
    (
        ActionId::ReviewOpen,
        ContextId::Global,
        &["Ctrl+c", "Ctrl+v"],
    ),
    (ActionId::NavNextView, ContextId::Global, &["Ctrl+x", "]"]),
    (ActionId::NavPrevView, ContextId::Global, &["Ctrl+x", "["]),
    // The board-return (J3 US-4, round 9 chunk 11 — the D-table's
    // Back ruling): Alt+Left is this preset's PRIMARY (the D-table's
    // gap fix — emacs bound no board-return row a detail hint could
    // surface before; the C-x C-b row below is the alternate, the D7
    // alternates shape).
    (ActionId::NavFocusBoard, ContextId::Global, &["Alt+Left"]),
    (
        ActionId::NavFocusBoard,
        ContextId::Global,
        &["Ctrl+x", "Ctrl+b"],
    ),
    // Round 2 (D16): Escape cancels a move flow everywhere; the lone
    // C-g lives in Board only — over the Global context it is redo's
    // pending prefix (the resolver is exact-match-first, so a Global
    // C-g would make `app.redo` permanently unreachable).
    (ActionId::TaskMoveCancel, ContextId::Global, &["Escape"]),
    // Round 3 (D28): the double-C-g convention — the first C-g parks
    // as a pending prefix (of both C-g C-g and C-g C-/), the second
    // exactly resolves the cancel. Over the Board context the lone
    // C-g still cancels instantly (exact-match-first); from the
    // Today/TaskDetail chains the convention is the only C-g route.
    (
        ActionId::TaskMoveCancel,
        ContextId::Global,
        &["Ctrl+g", "Ctrl+g"],
    ),
    // Round 3 (D29, #231): the banner verbs join the C-c domain
    // family (C-c C-m avoided — Ctrl+M is CR-ambiguous; distinct from
    // C-c Ctrl+b board-create — no prefix relation).
    (
        ActionId::ReminderSnooze,
        ContextId::Global,
        &["Ctrl+c", "Ctrl+s"],
    ),
    (
        ActionId::ReminderMute,
        ContextId::Global,
        &["Ctrl+c", "Ctrl+b"],
    ),
    // Board (J3 US-1 retune, round 9 chunk 9): C-n/C-p carry
    // nav.focus-down/up — the same card walk (the table's up/down
    // body is FocusNext/Prev's). The extend twin rides
    // Ctrl+Shift+n/p (#875: the explicit Shift rows — the chorded
    // case law makes the delivered event resolve them; both free in
    // this preset's Board).
    (ActionId::NavFocusDown, ContextId::Board, &["Ctrl+n"]),
    (ActionId::NavFocusUp, ContextId::Board, &["Ctrl+p"]),
    (ActionId::NavExtendNext, ContextId::Board, &["Ctrl+Shift+n"]),
    (ActionId::NavExtendPrev, ContextId::Board, &["Ctrl+Shift+p"]),
    (ActionId::NavFirstItem, ContextId::Board, &["Alt+<"]),
    (ActionId::NavLastItem, ContextId::Board, &["Alt+>"]),
    (
        ActionId::TaskComplete,
        ContextId::Board,
        &["Ctrl+c", "Ctrl+t"],
    ),
    (ActionId::TaskCreate, ContextId::Board, &["Ctrl+o"]),
    (ActionId::TaskEdit, ContextId::Board, &["Return"]),
    (ActionId::TaskMove, ContextId::Board, &["Ctrl+c", "Ctrl+w"]),
    (ActionId::TaskDelete, ContextId::Board, &["Ctrl+d"]),
    // Chunk 10 (D3/D11): the today mirror of the board rows — the
    // retune re-points C-n/C-p onto nav.focus-down/up (J3 US-1,
    // round 9 chunk 9; the same today walk).
    (ActionId::NavFocusDown, ContextId::Today, &["Ctrl+n"]),
    (ActionId::NavFocusUp, ContextId::Today, &["Ctrl+p"]),
    (ActionId::NavFirstItem, ContextId::Today, &["Alt+<"]),
    (ActionId::NavLastItem, ContextId::Today, &["Alt+>"]),
    (
        ActionId::TaskComplete,
        ContextId::Today,
        &["Ctrl+c", "Ctrl+t"],
    ),
    (ActionId::TaskEdit, ContextId::Today, &["Return"]),
    (ActionId::TaskMove, ContextId::Today, &["Ctrl+c", "Ctrl+w"]),
    (ActionId::TaskDelete, ContextId::Today, &["Ctrl+d"]),
    // The today re-commit (UX R-4 D-8, #796): the C-c family's free
    // bare t (C-c Ctrl+t is the complete verb — the chord stays
    // unambiguous); the round-10 D11 note is overturned with the
    // accepted triage as authority.
    (ActionId::TaskDueToday, ContextId::Today, &["Ctrl+c", "t"]),
    // Round 7 (D-7.2): the create form's opener — the CUA chord, free
    // in this preset's Today (the accessibility ledger's parity row).
    (ActionId::HabitCreate, ContextId::Today, &["Ctrl+Shift+h"]),
];

/// `emacs` continued after the shared [`HABIT_TODAY_ROWS`] block.
static EMACS_S0B: &[(ActionId, ContextId, &[&str])] = &[
    // Round 3 (W-SM-d, F26): the title re-arm — Return at detail is
    // unambiguous once task-detail entry no longer auto-arms (emacs
    // binds Return only at Board/Today for `task.edit`).
    (ActionId::TaskTitleEdit, ContextId::TaskDetail, &["Return"]),
    // The recurrence editor's opener (round-7 D-7.1): the C-c
    // user-namespace's r — free at TaskDetail (the board family's
    // C-c r binds StackRename in the Board context only).
    (
        ActionId::SeriesEdit,
        ContextId::TaskDetail,
        &["Ctrl+c", "r"],
    ),
    // The priority pickers (UX R-3 D-1, #788): the CUA function keys
    // are free in emacs' TaskDetail too (no emacs-authentic
    // candidate; the recorded no-drift rule — the pair shares the
    // CUA strokes).
    (ActionId::TaskImportanceEdit, ContextId::TaskDetail, &["F3"]),
    (ActionId::TaskUrgencyEdit, ContextId::TaskDetail, &["F4"]),
    // The labels overlay's opener (UX R-3 D-2, #789): Ctrl+Shift+l is
    // free in emacs' TaskDetail (real emacs rebinds it in buffers,
    // but this preset's TaskDetail never binds it — the recorded
    // no-drift share, the StackWider pair's rule).
    (
        ActionId::LabelManage,
        ContextId::TaskDetail,
        &["Ctrl+Shift+l"],
    ),
    // The roster-only mount's stroke (tag-color plan D-7, US-2): the
    // same action carries the same mnemonic across its contexts.
    (ActionId::LabelManage, ContextId::Board, &["Ctrl+Shift+l"]),
    // The detail editors' shortcuts (keymap-parity plan US-3, D-1/D-4):
    // the org C-c user-namespace carries the domain verbs. C-c C-d is
    // the due editor (free at TaskDetail — BoardDelete's C-c Ctrl+d is
    // the Board context only); C-c ' the description editor (chorded
    // printable, shift kept by the chord rule).
    (
        ActionId::TaskDueEdit,
        ContextId::TaskDetail,
        &["Ctrl+c", "Ctrl+d"],
    ),
    (
        ActionId::TaskDescriptionEdit,
        ContextId::TaskDetail,
        &["Ctrl+c", "'"],
    ),
    // The scheduled twin (keymap-parity plan US-3 AC-1, D-1/D-5): the
    // Global ReminderSnooze chord (C-c C-s) is *shadowed* inside the
    // detail chain by context precedence — the view context resolves
    // first (chain-order first-match); the snooze stays key-reachable
    // in every other chain and menu-reachable everywhere (the D-5
    // pin holds both rows verbatim, no retune).
    (
        ActionId::TaskScheduledEdit,
        ContextId::TaskDetail,
        &["Ctrl+c", "Ctrl+s"],
    ),
    // The rule-add mnemonic (keymap-parity plan US-3/S4, D-1): free
    // at TaskDetail (C-c C-u is unbound anywhere in this preset).
    (
        ActionId::ReminderRuleAdd,
        ContextId::TaskDetail,
        &["Ctrl+c", "Ctrl+u"],
    ),
    // The clock pair (keymap-parity plan US-3 AC-3, D-1): the
    // two-stroke approximation of org's C-c C-x C-i/o (the house
    // chord grammar is two-stroke — the three-stroke original is
    // recorded, never reshaped). Free at TaskDetail and Today (bare
    // Ctrl+o is TaskCreate at Board/StackDetail — different
    // spellings, different contexts).
    (
        ActionId::TaskTrackStart,
        ContextId::TaskDetail,
        &["Ctrl+c", "Ctrl+i"],
    ),
    (
        ActionId::TaskTrackStop,
        ContextId::TaskDetail,
        &["Ctrl+c", "Ctrl+o"],
    ),
    (
        ActionId::TaskTrackStart,
        ContextId::Today,
        &["Ctrl+c", "Ctrl+i"],
    ),
    (
        ActionId::TaskTrackStop,
        ContextId::Today,
        &["Ctrl+c", "Ctrl+o"],
    ),
    // --- J3 US-4/US-5, round 9 chunk 11 (the D-table rows; plan
    // .artifacts/plans/2026-11-19-taskdetail-shortcuts-plan.md §3.2):
    // the tracker verbs reach Board/StackDetail (US-5), the ONE smart
    // toggle binds in all four card contexts (R-8), and the detail's
    // remaining Tab-only verbs join the org C-c user-namespace —
    // Complete/Reopen mirror the Board/Today spellings, entries the
    // free bare e second stroke, the effort pair C-c C-e / C-c X
    // (bare-shift twin of vi's X; C-c C-x is the unschedule chord).
    // The R-2 mis-stroke hazard (accepted by the D-table): `C-c i`
    // (bare, the toggle) vs `C-c C-i` (chorded, the start) are
    // distinct by the bare/chord rule with the `C-c c` capture
    // precedent — the contract test pins the pair, tooltips
    // disambiguate.
    (
        ActionId::TaskComplete,
        ContextId::TaskDetail,
        &["Ctrl+c", "Ctrl+t"],
    ),
    (ActionId::TaskReopen, ContextId::TaskDetail, &["Ctrl+Space"]),
    (
        ActionId::TaskTrackToggle,
        ContextId::TaskDetail,
        &["Ctrl+c", "i"],
    ),
    (
        ActionId::TaskTrackToggle,
        ContextId::Board,
        &["Ctrl+c", "i"],
    ),
    (
        ActionId::TaskTrackToggle,
        ContextId::StackDetail,
        &["Ctrl+c", "i"],
    ),
    (
        ActionId::TaskTrackToggle,
        ContextId::Today,
        &["Ctrl+c", "i"],
    ),
    (
        ActionId::TaskTrackStart,
        ContextId::Board,
        &["Ctrl+c", "Ctrl+i"],
    ),
    (
        ActionId::TaskTrackStop,
        ContextId::Board,
        &["Ctrl+c", "Ctrl+o"],
    ),
    (
        ActionId::TaskTrackStart,
        ContextId::StackDetail,
        &["Ctrl+c", "Ctrl+i"],
    ),
    (
        ActionId::TaskTrackStop,
        ContextId::StackDetail,
        &["Ctrl+c", "Ctrl+o"],
    ),
    (
        ActionId::TaskTrackEntries,
        ContextId::TaskDetail,
        &["Ctrl+c", "e"],
    ),
    (
        ActionId::TaskEffortEdit,
        ContextId::TaskDetail,
        &["Ctrl+c", "Ctrl+e"],
    ),
    (
        ActionId::TaskEffortClear,
        ContextId::TaskDetail,
        &["Ctrl+c", "X"],
    ),
    // The series row's chords (US-11, W-12, #1067) and the
    // tracker-kind toggle (US-12, W-13, #1068): the chord spellings
    // are free in this preset's detail chain — the shared pause/resume
    // pair (the R-7 note on standard's Ctrl+Shift+r collision does not
    // reach this preset) and the C-c user-namespace's bare k for the
    // kind toggle (the plan's emacs candidate; the bare second stroke
    // rides the row-verb convention).
    (
        ActionId::SeriesPause,
        ContextId::TaskDetail,
        &["Ctrl+Shift+p"],
    ),
    (
        ActionId::SeriesResume,
        ContextId::TaskDetail,
        &["Ctrl+Shift+u"],
    ),
    (
        ActionId::TaskToggleTracker,
        ContextId::TaskDetail,
        &["Ctrl+c", "k"],
    ),
    // The scheduled field's quick verbs (UX-R8 chunk-5 US-1/AC-5,
    // D-10 with the C-5 re-picks recorded): the plan's C-c C-o and
    // C-c C-u candidates are OCCUPIED (the clock pair's stop and the
    // rule-add mnemonic — verified against the chord inventory), so
    // the free-set re-picks: C-c C-y the today stamp (the yank-to-
    // today mnemonic; free), C-c C-n tomorrow (the next-day mnemonic;
    // free), C-c C-x the clear twin (org's C-x submenu mnemonic; free
    // in this preset).
    (
        ActionId::TaskScheduleToday,
        ContextId::Board,
        &["Ctrl+c", "Ctrl+y"],
    ),
    (
        ActionId::TaskScheduleToday,
        ContextId::Today,
        &["Ctrl+c", "Ctrl+y"],
    ),
    (
        ActionId::TaskScheduleToday,
        ContextId::StackDetail,
        &["Ctrl+c", "Ctrl+y"],
    ),
    (
        ActionId::TaskScheduleToday,
        ContextId::TaskDetail,
        &["Ctrl+c", "Ctrl+y"],
    ),
    (
        ActionId::TaskScheduleTomorrow,
        ContextId::Board,
        &["Ctrl+c", "Ctrl+n"],
    ),
    (
        ActionId::TaskScheduleTomorrow,
        ContextId::Today,
        &["Ctrl+c", "Ctrl+n"],
    ),
    (
        ActionId::TaskScheduleTomorrow,
        ContextId::StackDetail,
        &["Ctrl+c", "Ctrl+n"],
    ),
    (
        ActionId::TaskScheduleTomorrow,
        ContextId::TaskDetail,
        &["Ctrl+c", "Ctrl+n"],
    ),
    (
        ActionId::TaskUnschedule,
        ContextId::Board,
        &["Ctrl+c", "Ctrl+x"],
    ),
    (
        ActionId::TaskUnschedule,
        ContextId::Today,
        &["Ctrl+c", "Ctrl+x"],
    ),
    (
        ActionId::TaskUnschedule,
        ContextId::StackDetail,
        &["Ctrl+c", "Ctrl+x"],
    ),
    (
        ActionId::TaskUnschedule,
        ContextId::TaskDetail,
        &["Ctrl+c", "Ctrl+x"],
    ),
    (
        ActionId::LabelOverlayDismiss,
        ContextId::Labels,
        &["Escape"],
    ),
    // The export dialog's Escape rung (the J2 export plan W-1's
    // close callback): the dialog context is a total mask; the
    // close dispatches nothing (US-1/AC-2).
    (
        ActionId::ExportDialogClose,
        ContextId::ExportDialog,
        &["Escape"],
    ),
];

static EMACS_S1: &[(ActionId, ContextId, &[&str])] = &[
    // Round 2 (D16): the board-view C-g alternate for move-cancel.
    (ActionId::TaskMoveCancel, ContextId::Board, &["Ctrl+g"]),
    // The retune re-points C-f/C-b onto nav.focus-right/left (J3
    // US-1, round 9 chunk 9) — the same stack step at Board (the
    // table's horizontal body is StackNext/Prev's). StackNext/Prev
    // keep their alternate rows in the other presets (vi g l/g h,
    // Standard Ctrl+Shift+Left/Right); mid-move the directional keys
    // are the kept arms that hop the drop column (US-2 — the
    // settle-matrix fix).
    (ActionId::NavFocusRight, ContextId::Board, &["Ctrl+f"]),
    (ActionId::NavFocusLeft, ContextId::Board, &["Ctrl+b"]),
    (ActionId::BoardNext, ContextId::Board, &["b"]),
    (ActionId::BoardPrevious, ContextId::Board, &["B"]),
    // The switcher flyout's opener (UX R-4 D-4, #794): the C-c
    // family's free bare g second stroke (C-c Ctrl+b is board-create
    // — the chord stays unambiguous; the lone C-g is move-cancel).
    (ActionId::BoardSelect, ContextId::Board, &["Ctrl+c", "g"]),
    // Round 2 (D18): the org-mode C-c family extends — bare second
    // strokes for stack verbs, chorded for board verbs.
    (ActionId::StackCreate, ContextId::Board, &["Ctrl+c", "s"]),
    (ActionId::StackRename, ContextId::Board, &["Ctrl+c", "r"]),
    (ActionId::StackMove, ContextId::Board, &["Ctrl+c", "m"]),
    (
        ActionId::StackDelete,
        ContextId::Board,
        &["Ctrl+c", "Delete"],
    ),
    (
        ActionId::BoardCreate,
        ContextId::Board,
        &["Ctrl+c", "Ctrl+b"],
    ),
    (ActionId::BoardRename, ContextId::Board, &["Ctrl+c", "b"]),
    // The color picker's opener (UX R-4 D-1, #791): the C-c family's
    // free Ctrl+f second stroke (StackNext owns bare Ctrl+f here —
    // the chord is unambiguous, exact-match-first).
    (
        ActionId::BoardColorEdit,
        ContextId::Board,
        &["Ctrl+c", "Ctrl+f"],
    ),
    // The empty-board preset (UX R-4 D-5, #795): the C-c family's
    // free p second stroke (the chord stays unambiguous).
    (
        ActionId::BoardDefaultPreset,
        ContextId::Board,
        &["Ctrl+c", "p"],
    ),
    (
        ActionId::BoardDelete,
        ContextId::Board,
        &["Ctrl+c", "Ctrl+d"],
    ),
    (
        ActionId::TaskReopen,
        ContextId::Board,
        &["Ctrl+c", "Ctrl+Space"],
    ),
    (ActionId::TaskReopen, ContextId::Today, &["Ctrl+Space"]),
    // Round-1 chunk 2 (D18): no emacs-authentic candidate exists, so
    // the pair shares the CUA strokes (the design's recorded rule).
    (ActionId::StackWider, ContextId::Board, &["Ctrl+Right"]),
    (ActionId::StackNarrower, ContextId::Board, &["Ctrl+Left"]),
    // StackDetail (the retune): C-n/C-p carry nav.focus-down/up —
    // the same item walk.
    (ActionId::NavFocusDown, ContextId::StackDetail, &["Ctrl+n"]),
    (ActionId::NavFocusUp, ContextId::StackDetail, &["Ctrl+p"]),
    (ActionId::NavFirstItem, ContextId::StackDetail, &["Alt+<"]),
    (ActionId::NavLastItem, ContextId::StackDetail, &["Alt+>"]),
    (ActionId::TaskCreate, ContextId::StackDetail, &["Ctrl+o"]),
    // TaskDetail: the D-1 item-walk rows (C-n/C-p item walk,
    // M-</M-> jumps) are RETIRED (J3 R-3, 2026-11-19 — the subject
    // walk is gone, census-dated; the retune re-points the detail's
    // C-n/C-p onto nav.focus-down/up). The control walk keeps its
    // isearch pair (C-f/C-b, the r3 D25 rows below).
    // Menu
    (ActionId::MenuNext, ContextId::Menu, &["Ctrl+n"]),
    (ActionId::MenuPrevious, ContextId::Menu, &["Ctrl+p"]),
    (ActionId::MenuInvoke, ContextId::Menu, &["Return"]),
    (ActionId::MenuDismiss, ContextId::Menu, &["Ctrl+g"]),
    (ActionId::MenuDismiss, ContextId::Menu, &["Escape"]),
    // Palette (#348): the emacs menu vocabulary; both dismissals ride.
    (ActionId::MenuNext, ContextId::Palette, &["Ctrl+n"]),
    (ActionId::MenuPrevious, ContextId::Palette, &["Ctrl+p"]),
    (ActionId::MenuInvoke, ContextId::Palette, &["Return"]),
    (ActionId::MenuDismiss, ContextId::Palette, &["Ctrl+g"]),
    (ActionId::MenuDismiss, ContextId::Palette, &["Escape"]),
    // Focus mode (#496): the emacs menu vocabulary rides (both
    // dismissals); arrows as the palette-shaped alternates.
    (ActionId::FocusModeToggle, ContextId::FocusMode, &["Escape"]),
    (
        ActionId::FocusModeToggle,
        ContextId::FocusMode,
        &["Ctrl+Shift+j"],
    ),
    (ActionId::MenuNext, ContextId::FocusMode, &["Ctrl+n"]),
    (ActionId::MenuNext, ContextId::FocusMode, &["Down"]),
    (ActionId::MenuPrevious, ContextId::FocusMode, &["Ctrl+p"]),
    (ActionId::MenuPrevious, ContextId::FocusMode, &["Up"]),
    (ActionId::MenuInvoke, ContextId::FocusMode, &["Return"]),
    (ActionId::MenuDismiss, ContextId::FocusMode, &["Ctrl+g"]),
    // D5: insert-mode exit (see standard's note).
    (ActionId::ModeNormal, ContextId::ViInsert, &["Escape"]),
    // Round-1 chunk 3 (D28b): Escape closes the settings surface.
    (ActionId::AppSettings, ContextId::Settings, &["Escape"]),
    (
        ActionId::StatusPanelOpen,
        ContextId::StatusPanel,
        &["Escape"],
    ),
    // The planner's pane-switch grammar (J3 R-5, round 9 chunk 9):
    // C-f/C-b flip the boxes ↔ pool focus (free in this context —
    // the palette's isearch shadow does not reach the planner).
    (ActionId::NavFocusRight, ContextId::PlanNextDay, &["Ctrl+f"]),
    (ActionId::NavFocusLeft, ContextId::PlanNextDay, &["Ctrl+b"]),
    // chords carry nav.focus-* in every view context — the
    // horizontal pair reads as the control walk in the non-board
    // view contexts (R-4's first cut; the isearch C-f/C-b
    // *semantics* — the control step — survive under the new ids),
    // and C-n/C-p read as the control walk at Settings (r4 D19
    // kept). The board's controls keep native Tab (unbound →
    // Unhandled → traversal).
    (ActionId::NavFocusRight, ContextId::Today, &["Ctrl+f"]),
    (ActionId::NavFocusLeft, ContextId::Today, &["Ctrl+b"]),
    (ActionId::NavFocusRight, ContextId::StackDetail, &["Ctrl+f"]),
    (ActionId::NavFocusLeft, ContextId::StackDetail, &["Ctrl+b"]),
    (ActionId::NavFocusRight, ContextId::TaskDetail, &["Ctrl+f"]),
    (ActionId::NavFocusLeft, ContextId::TaskDetail, &["Ctrl+b"]),
    // The detail's vertical pair (J3 R-3's successor rows): the
    // control walk up the stop ladder — never the subject walk.
    (ActionId::NavFocusDown, ContextId::TaskDetail, &["Ctrl+n"]),
    (ActionId::NavFocusUp, ContextId::TaskDetail, &["Ctrl+p"]),
    (ActionId::NavFocusRight, ContextId::Settings, &["Ctrl+f"]),
    (ActionId::NavFocusLeft, ContextId::Settings, &["Ctrl+b"]),
    (ActionId::NavFocusDown, ContextId::Settings, &["Ctrl+n"]),
    (ActionId::NavFocusUp, ContextId::Settings, &["Ctrl+p"]),
    // Round 3 (D26): the scroll actions — C-v/M-v, emacs'
    // scroll-up/down commands, in the five view contexts.
    (ActionId::ViewScrollDown, ContextId::Today, &["Ctrl+v"]),
    (ActionId::ViewScrollUp, ContextId::Today, &["Alt+v"]),
    (ActionId::ViewScrollDown, ContextId::Board, &["Ctrl+v"]),
    (ActionId::ViewScrollUp, ContextId::Board, &["Alt+v"]),
    (
        ActionId::ViewScrollDown,
        ContextId::StackDetail,
        &["Ctrl+v"],
    ),
    (ActionId::ViewScrollUp, ContextId::StackDetail, &["Alt+v"]),
    (ActionId::ViewScrollDown, ContextId::TaskDetail, &["Ctrl+v"]),
    (ActionId::ViewScrollUp, ContextId::TaskDetail, &["Alt+v"]),
    (ActionId::ViewScrollDown, ContextId::Settings, &["Ctrl+v"]),
    (ActionId::ViewScrollUp, ContextId::Settings, &["Alt+v"]),
    // Round 3 (D25): the picker keys — emacs' calendar-mode vocabulary (C-v/M-v are calendar-scroll: an intentional shadowing of the Global scroll rows *within* the DatePicker context — chain-head priority, different contexts, no contract contact).
    (ActionId::PickerLeft, ContextId::DatePicker, &["Ctrl+b"]),
    (ActionId::PickerRight, ContextId::DatePicker, &["Ctrl+f"]),
    (ActionId::PickerUp, ContextId::DatePicker, &["Ctrl+p"]),
    (ActionId::PickerDown, ContextId::DatePicker, &["Ctrl+n"]),
    (ActionId::PickerPrevMonth, ContextId::DatePicker, &["Alt+v"]),
    (
        ActionId::PickerNextMonth,
        ContextId::DatePicker,
        &["Ctrl+v"],
    ),
    (
        ActionId::PickerMonthStart,
        ContextId::DatePicker,
        &["Alt+<"],
    ),
    (ActionId::PickerMonthEnd, ContextId::DatePicker, &["Alt+>"]),
    (ActionId::PickerCommit, ContextId::DatePicker, &["Return"]),
    // The entries overlay (r4 chunk 10, D-TS2): the emacs vocabulary
    // — the ladder's Escape plus its C-g alternate; the walk rides
    // the source-editor C-n/C-p pair.
    (
        ActionId::TaskTrackEntryDismiss,
        ContextId::TrackEntries,
        &["Escape"],
    ),
    (
        ActionId::TaskTrackEntryDismiss,
        ContextId::TrackEntries,
        &["Ctrl+g"],
    ),
    (
        ActionId::NavNextControl,
        ContextId::TrackEntries,
        &["Ctrl+n"],
    ),
    (
        ActionId::NavPrevControl,
        ContextId::TrackEntries,
        &["Ctrl+p"],
    ),
    // The review walk (DR-7 chunk 3, #638): the emacs vocabulary —
    // the ladder's Escape + its C-g alternate; the cursor rides the
    // source-editor C-p/C-n pair.
    (
        ActionId::ReviewWalkCursorUp,
        ContextId::ReviewWalk,
        &["Ctrl+p"],
    ),
    (
        ActionId::ReviewWalkCursorDown,
        ContextId::ReviewWalk,
        &["Ctrl+n"],
    ),
    (ActionId::ReviewWalkBack, ContextId::ReviewWalk, &["Escape"]),
    (ActionId::ReviewWalkBack, ContextId::ReviewWalk, &["Ctrl+g"]),
    // The alarm window (T-22, #805): the same window-local grammar
    // (the context scopes the single keys away from the emacs
    // chords; nothing collides).
];

/// `vi` — vim-authentic: everything lives in `ViNormal` (the honest
/// vim model — normal-mode keys mean nothing in insert mode), `g`/`d`
/// /`Z` are real prefix families (the resolver's accumulation
/// reproduces vim's operator-waits-for-motion behaviour), `ViInsert`
/// binds only Escape.
static VI_S0: &[(ActionId, ContextId, &[&str])] = &[
    // ViNormal carries everything; Global is empty. The motion keys
    // are the directional-focus keys (J3 US-1 retune, R-1 — round 9
    // chunk 9): j/k carry nav.focus-down/up (the same walk they
    // carried as NavNextItem/PrevItem), h/l join (free in ViNormal —
    // the g-family rows are prefixes, not conflicts). The extend
    // twin (#875): J/K are the explicit Board-context rows — a
    // Shift+j press folds the shift into the glyph (the D6 law) and
    // delivers `J` exactly; the named-key arrow intercept stays the
    // fallback front.
    (ActionId::NavFocusDown, ContextId::ViNormal, &["j"]),
    (ActionId::NavFocusUp, ContextId::ViNormal, &["k"]),
    (ActionId::NavFocusLeft, ContextId::ViNormal, &["h"]),
    (ActionId::NavFocusRight, ContextId::ViNormal, &["l"]),
    (ActionId::NavExtendNext, ContextId::Board, &["J"]),
    (ActionId::NavExtendPrev, ContextId::Board, &["K"]),
    (ActionId::NavNextView, ContextId::ViNormal, &["g", "t"]),
    (ActionId::NavPrevView, ContextId::ViNormal, &["g", "T"]),
    (ActionId::NavFocusBoard, ContextId::ViNormal, &["g", "g"]),
    // The review walk's opener (UX R-4 D-8, #804): the g-family's
    // free w (the walk), beside the view-nav's g t.
    (ActionId::ReviewOpen, ContextId::ViNormal, &["g", "w"]),
    (ActionId::ViewToday, ContextId::Global, &["Ctrl+Shift+t"]),
    (ActionId::TaskComplete, ContextId::ViNormal, &["Space"]),
    (ActionId::TaskCreate, ContextId::ViNormal, &["o"]),
    (ActionId::StackCreate, ContextId::ViNormal, &["O"]),
    (ActionId::TaskEdit, ContextId::ViNormal, &["Return"]),
    (ActionId::TaskDelete, ContextId::ViNormal, &["d", "d"]),
    (ActionId::TaskMove, ContextId::ViNormal, &["g", "m"]),
    // Round 7 (D-7.2): the create form's opener — the ledger's
    // parity row; the Today context is chain-visible in vi too
    // (the `[Today, ViNormal, Global]` chain shape).
    (ActionId::HabitCreate, ContextId::Today, &["Ctrl+Shift+h"]),
];

/// `vi` continued after the shared [`HABIT_TODAY_ROWS`] block.
static VI_S0B: &[(ActionId, ContextId, &[&str])] = &[
    // The today re-commit (UX R-4 D-8, #796): the bare mnemonic in
    // the Today context (the HabitCreate row's pattern — the view
    // context resolves before ViNormal); free in both contexts.
    (ActionId::TaskDueToday, ContextId::Today, &["t"]),
    // The scheduled field's quick verbs (UX-R8 chunk-5 US-1/AC-5,
    // D-10 with the C-5 re-picks recorded): the preset carries no
    // function keys (the F-family grep found none), so the letter
    // grammar carries the family — y the today stamp (yank-to-today;
    // free), Y tomorrow (the shifted twin; free), X the clear twin
    // (free; x is unbound at these contexts too). The ViNormal rows
    // (the board walk's motion chain) carry the same trio.
    (ActionId::TaskScheduleToday, ContextId::Today, &["y"]),
    (ActionId::TaskScheduleToday, ContextId::Board, &["y"]),
    (ActionId::TaskScheduleToday, ContextId::StackDetail, &["y"]),
    (ActionId::TaskScheduleToday, ContextId::TaskDetail, &["y"]),
    (ActionId::TaskScheduleTomorrow, ContextId::Today, &["Y"]),
    (ActionId::TaskScheduleTomorrow, ContextId::Board, &["Y"]),
    (
        ActionId::TaskScheduleTomorrow,
        ContextId::StackDetail,
        &["Y"],
    ),
    (
        ActionId::TaskScheduleTomorrow,
        ContextId::TaskDetail,
        &["Y"],
    ),
    (ActionId::TaskUnschedule, ContextId::Today, &["X"]),
    (ActionId::TaskUnschedule, ContextId::Board, &["X"]),
    (ActionId::TaskUnschedule, ContextId::StackDetail, &["X"]),
    (ActionId::TaskUnschedule, ContextId::TaskDetail, &["X"]),
    // Round 3 (D29, #231): the banner verbs — bare s/m in ViNormal
    // (free; no family growth). Global stays exactly one row (the
    // vi-shape contract).
    (ActionId::ReminderSnooze, ContextId::ViNormal, &["s"]),
    (ActionId::ReminderMute, ContextId::ViNormal, &["m"]),
    // The ONE smart tracking toggle (J3 R-8, round 9 chunk 11): `c`
    // = clock, free in ViNormal — one row serves all four card
    // contexts (TaskDetail/Today/Board/StackDetail fall through to
    // ViNormal in the chain; no per-context rows needed).
    (ActionId::TaskTrackToggle, ContextId::ViNormal, &["c"]),
    // The board-return's alternate (J3 US-4, round 9 chunk 11 — the
    // D-table's Back ruling): Alt+Left, beside the `g g` primary (the
    // D7 alternates shape; Global row, the vi-shape contract's Global
    // stays the Ctrl+Shift+t row + this alternate).
    (ActionId::NavFocusBoard, ContextId::Global, &["Alt+Left"]),
    // Round 2 (D16): vi's cancel lives in ViNormal, never Global (the
    // vi-shape contract keeps Global empty); Escape conflicts with
    // nothing there — ViInsert's Escape still wins while typing.
    (ActionId::TaskMoveCancel, ContextId::ViNormal, &["Escape"]),
    // Round 2 (D17/D18): the g-family extends — the d/D delete pair
    // convention, r/R renames, v for the stack move.
    (ActionId::StackNext, ContextId::ViNormal, &["g", "l"]),
    (ActionId::StackPrev, ContextId::ViNormal, &["g", "h"]),
    (ActionId::StackRename, ContextId::ViNormal, &["g", "r"]),
    (ActionId::StackMove, ContextId::ViNormal, &["g", "v"]),
    (ActionId::StackDelete, ContextId::ViNormal, &["g", "D"]),
    (ActionId::BoardCreate, ContextId::ViNormal, &["g", "n"]),
    (ActionId::BoardRename, ContextId::ViNormal, &["g", "R"]),
    // The color picker's opener (UX R-4 D-1, #791): the g-family's
    // free c (colors), beside the rename's g R.
    (ActionId::BoardColorEdit, ContextId::ViNormal, &["g", "c"]),
    // The empty-board preset (UX R-4 D-5, #795): the g-family's free
    // p (preset), beside the picker's g c.
    (
        ActionId::BoardDefaultPreset,
        ContextId::ViNormal,
        &["g", "p"],
    ),
    (ActionId::BoardDelete, ContextId::ViNormal, &["g", "d"]),
    // Round 2 (D20): H/G are vim's top/bottom motions, mapped to
    // first/last item in reading order.
    (ActionId::NavFirstItem, ContextId::ViNormal, &["H"]),
    (ActionId::NavLastItem, ContextId::ViNormal, &["G"]),
    (ActionId::TaskReopen, ContextId::ViNormal, &["U"]),
    (ActionId::BoardNext, ContextId::ViNormal, &["b"]),
    (ActionId::BoardPrevious, ContextId::ViNormal, &["B"]),
    // The switcher flyout's opener (UX R-4 D-4, #794): the g-family's
    // free b (no family conflict; the buffer-switch shadow is vim's,
    // ours is the flyout).
    (ActionId::BoardSelect, ContextId::ViNormal, &["g", "b"]),
    // Round-1 chunk 2 (D18): vim's own window-resize family; `Ctrl+w`
    // alone is unbound, so the prefix-shadow pass stays clean.
    (ActionId::StackWider, ContextId::ViNormal, &["Ctrl+w", ">"]),
    (
        ActionId::StackNarrower,
        ContextId::ViNormal,
        &["Ctrl+w", "<"],
    ),
    (ActionId::ModeInsert, ContextId::ViNormal, &["i"]),
    (ActionId::ModeInsert, ContextId::ViNormal, &["a"]),
    (ActionId::AppUndo, ContextId::ViNormal, &["u"]),
    (ActionId::AppRedo, ContextId::ViNormal, &["Ctrl+r"]),
    (ActionId::AppExit, ContextId::ViNormal, &["Z", "Q"]),
    (ActionId::AppSettings, ContextId::ViNormal, &["Z", "s"]),
    (ActionId::CaptureQuick, ContextId::ViNormal, &["Z", "c"]),
    (ActionId::AppMenu, ContextId::ViNormal, &[":"]),
    (ActionId::PaletteSearch, ContextId::ViNormal, &["/"]),
    // Focus mode (#496): `Z f` — the Z leader family's free letter
    // (Z c capture, Z Q exit, Z s settings).
    (ActionId::FocusModeToggle, ContextId::ViNormal, &["Z", "f"]),
    (ActionId::AppHelp, ContextId::ViNormal, &["F1"]),
    // ViInsert: the only insert binding (as in vim, only Escape acts).
    (ActionId::ModeNormal, ContextId::ViInsert, &["Escape"]),
    // Menu
    (ActionId::MenuNext, ContextId::Menu, &["j"]),
    (ActionId::MenuPrevious, ContextId::Menu, &["k"]),
    (ActionId::MenuInvoke, ContextId::Menu, &["Return"]),
    (ActionId::MenuDismiss, ContextId::Menu, &["Escape"]),
    // Palette (#348): arrows, not j/k — the palette has a text field,
    // so j/k are printables that must reach it.
    (ActionId::MenuNext, ContextId::Palette, &["Down"]),
    (ActionId::MenuPrevious, ContextId::Palette, &["Up"]),
    (ActionId::MenuInvoke, ContextId::Palette, &["Return"]),
    (ActionId::MenuDismiss, ContextId::Palette, &["Escape"]),
    // Focus mode (#496): arrows, not j/k — the palette's text-field
    // reason verbatim; the leader stroke and Escape both dismiss.
    (ActionId::FocusModeToggle, ContextId::FocusMode, &["Escape"]),
    (ActionId::FocusModeToggle, ContextId::FocusMode, &["Z", "f"]),
    (ActionId::MenuNext, ContextId::FocusMode, &["Down"]),
    (ActionId::MenuPrevious, ContextId::FocusMode, &["Up"]),
    (ActionId::MenuInvoke, ContextId::FocusMode, &["Return"]),
    // Round-1 chunk 3 (D28b): Escape closes the settings surface.
    (ActionId::AppSettings, ContextId::Settings, &["Escape"]),
    (
        ActionId::StatusPanelOpen,
        ContextId::StatusPanel,
        &["Escape"],
    ),
    // Round 3 (D25): the control walk — Tab/Shift+Tab in the five
    // view contexts (unbound today; under ViInsert the chain masks
    // them and Tab stays the fields' native traversal).
];

static VI_S1: &[(ActionId, ContextId, &[&str])] = &[
    // The recurrence editor's opener (round-7 D-7.1): bare r at the
    // task detail — vi-normal's repeat mnemonic (`g r` is the stack
    // rename's prefix family; bare r is free).
    (ActionId::SeriesEdit, ContextId::TaskDetail, &["r"]),
    // The priority pickers (UX R-3 D-1, #788): the function keys are
    // motion-free in normal mode — F3/F4 bind here too (the
    // no-second-class gate; the insert path never sees them).
    (ActionId::TaskImportanceEdit, ContextId::TaskDetail, &["F3"]),
    (ActionId::TaskUrgencyEdit, ContextId::TaskDetail, &["F4"]),
    // The detail editors' shortcuts (keymap-parity plan US-3, D-1/D-4):
    // single-mnemonic detail verbs at the TaskDetail context (the
    // SeriesEdit `r` precedent) — d the due editor (vim
    // delete-into-register: the date change as the detail's "delete";
    // vi's own `d d` rows live in the Board contexts), s the
    // scheduled twin, e the description editor, a the rule add (vim
    // append — the mnemonic add). Within the detail chain these
    // shadow their ViNormal spellings by context precedence (the D-5
    // rule; the shadow is the recorded cost).
    (ActionId::TaskDueEdit, ContextId::TaskDetail, &["d"]),
    (ActionId::TaskScheduledEdit, ContextId::TaskDetail, &["s"]),
    (ActionId::TaskDescriptionEdit, ContextId::TaskDetail, &["e"]),
    (ActionId::ReminderRuleAdd, ContextId::TaskDetail, &["a"]),
    // The clock pair (keymap-parity plan US-3 AC-3, D-1): I/O (vim's
    // insert-at-begin / open-below mnemonics) at TaskDetail and Today
    // (free in both contexts — the suite's totality pass is the
    // oracle; the Today rows are the tracking ids' second context).
    (ActionId::TaskTrackStart, ContextId::TaskDetail, &["I"]),
    (ActionId::TaskTrackStop, ContextId::TaskDetail, &["O"]),
    (ActionId::TaskTrackStart, ContextId::Today, &["I"]),
    (ActionId::TaskTrackStop, ContextId::Today, &["O"]),
    // --- J3 US-4/US-5, round 9 chunk 11 (the D-table rows; plan
    // .artifacts/plans/2026-11-19-taskdetail-shortcuts-plan.md §3.2):
    // the tracker pair reaches Board/StackDetail (US-5 — the same
    // uppercase twins; the smart toggle rides ViNormal `c` above),
    // the title re-arm gains `g i` (vim gi = insert at the last edit
    // position; the g-family's free second stroke), and the detail's
    // remaining Tab-only verbs take the detail-local letters —
    // entries the uppercase twin of the description editor's `e`,
    // the effort pair `f`/`F` (the natural uppercase twin pair; the
    // bare `c` clock toggle lives in ViNormal).
    (ActionId::TaskTrackStart, ContextId::Board, &["I"]),
    (ActionId::TaskTrackStop, ContextId::Board, &["O"]),
    (ActionId::TaskTrackStart, ContextId::StackDetail, &["I"]),
    (ActionId::TaskTrackStop, ContextId::StackDetail, &["O"]),
    (ActionId::TaskTitleEdit, ContextId::TaskDetail, &["g", "i"]),
    (ActionId::TaskTrackEntries, ContextId::TaskDetail, &["E"]),
    (ActionId::TaskEffortEdit, ContextId::TaskDetail, &["f"]),
    (ActionId::TaskEffortClear, ContextId::TaskDetail, &["F"]),
    // The series row's chords (US-11, W-12, #1067) and the
    // tracker-kind toggle (US-12, W-13, #1068): the shared chord
    // spellings (free in this preset's detail chain; the R-7 note on
    // standard's Ctrl+Shift+r does not reach here) and the g-family's
    // free k second stroke for the kind toggle (the plan's vi
    // candidate; exact-match-first keeps bare k's motion exact).
    (
        ActionId::SeriesPause,
        ContextId::TaskDetail,
        &["Ctrl+Shift+p"],
    ),
    (
        ActionId::SeriesResume,
        ContextId::TaskDetail,
        &["Ctrl+Shift+u"],
    ),
    (
        ActionId::TaskToggleTracker,
        ContextId::TaskDetail,
        &["g", "k"],
    ),
];

static VI_S2: &[(ActionId, ContextId, &[&str])] = &[
    // Round 3 (D26): the scroll actions ride ViNormal once — motions
    // are modal (the vi model); ViNormal is absent from insert chains,
    // so text is safe.
    (ActionId::ViewScrollDown, ContextId::ViNormal, &["Ctrl+f"]),
    (ActionId::ViewScrollUp, ContextId::ViNormal, &["Ctrl+b"]),
    // Round 3 (D25): the picker keys — the home-row motions on the DatePicker context (armed only while the grid holds focus, so the field's printables are untouched).
    (ActionId::PickerLeft, ContextId::DatePicker, &["h"]),
    (ActionId::PickerRight, ContextId::DatePicker, &["l"]),
    (ActionId::PickerUp, ContextId::DatePicker, &["k"]),
    (ActionId::PickerDown, ContextId::DatePicker, &["j"]),
    (
        ActionId::PickerPrevMonth,
        ContextId::DatePicker,
        &["Ctrl+b"],
    ),
    (
        ActionId::PickerNextMonth,
        ContextId::DatePicker,
        &["Ctrl+f"],
    ),
    (
        ActionId::PickerMonthStart,
        ContextId::DatePicker,
        &["g", "g"],
    ),
    (ActionId::PickerMonthEnd, ContextId::DatePicker, &["G"]),
    (ActionId::PickerCommit, ContextId::DatePicker, &["Return"]),
    // The entries overlay (r4 chunk 10, D-TS2): the ladder row plus
    // the tab walk (the vi preset's per-view row pattern).
];

static VI_S4: &[(ActionId, ContextId, &[&str])] = &[
    // The alarm window (T-22, #805): the same window-local grammar —
    // the arrows are the cursors (the menu-cursor vocabulary) and the
    // single keys fire the verbs (the context scopes them; nothing
    // collides in normal mode).
];

static VI_S6: &[(ActionId, ContextId, &[&str])] = &[
    // The labels overlay (UX R-3 D-2, #789): Ctrl+Shift+l is unbound in
    // normal mode here; the overlay's own context carries the
    // ladder + tab rows (the entries-overlay's vi pattern).
    (
        ActionId::LabelManage,
        ContextId::TaskDetail,
        &["Ctrl+Shift+l"],
    ),
    // The roster-only mount's stroke (tag-color plan D-7, US-2): the
    // same action carries the same mnemonic across its contexts.
    (ActionId::LabelManage, ContextId::Board, &["Ctrl+Shift+l"]),
    (
        ActionId::LabelOverlayDismiss,
        ContextId::Labels,
        &["Escape"],
    ),
    // The export dialog's Escape rung (the J2 export plan W-1's
    // close callback): the dialog context is a total mask; the
    // close dispatches nothing (US-1/AC-2).
    (
        ActionId::ExportDialogClose,
        ContextId::ExportDialog,
        &["Escape"],
    ),
];

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
                LABELS_CONTROL_WALK_ROWS,
                EMACS_S1,
                ALARM_WINDOW_ROWS,
                NOTIFICATION_ROWS,
            ]),
            Self::Vi => chained(&[
                VI_S0,
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
    (ActionId::NavNextView, ContextId::Global, &["Ctrl+PageDown"]),
    (ActionId::NavPrevView, ContextId::Global, &["Ctrl+PageUp"]),
    (ActionId::NavFocusBoard, ContextId::Global, &["Alt+1"]),
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
    (ActionId::NavNextItem, ContextId::Board, &["Down"]),
    (ActionId::NavPrevItem, ContextId::Board, &["Up"]),
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
    (ActionId::NavNextItem, ContextId::Today, &["Down"]),
    (ActionId::NavPrevItem, ContextId::Today, &["Up"]),
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
    // StackDetail
    (ActionId::NavNextItem, ContextId::StackDetail, &["Down"]),
    (ActionId::NavPrevItem, ContextId::StackDetail, &["Up"]),
    (
        ActionId::NavFirstItem,
        ContextId::StackDetail,
        &["Ctrl+Home"],
    ),
    (ActionId::NavLastItem, ContextId::StackDetail, &["Ctrl+End"]),
    (ActionId::TaskCreate, ContextId::StackDetail, &["N"]),
    // TaskDetail
    (ActionId::NavNextItem, ContextId::TaskDetail, &["Down"]),
    (ActionId::NavPrevItem, ContextId::TaskDetail, &["Up"]),
    (
        ActionId::NavFirstItem,
        ContextId::TaskDetail,
        &["Ctrl+Home"],
    ),
    (ActionId::NavLastItem, ContextId::TaskDetail, &["Ctrl+End"]),
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
    // Board
    (ActionId::NavNextItem, ContextId::Board, &["Ctrl+n"]),
    (ActionId::NavPrevItem, ContextId::Board, &["Ctrl+p"]),
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
    // Chunk 10 (D3/D11): the today mirror of the board rows.
    (ActionId::NavNextItem, ContextId::Today, &["Ctrl+n"]),
    (ActionId::NavPrevItem, ContextId::Today, &["Ctrl+p"]),
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
];

static EMACS_S1: &[(ActionId, ContextId, &[&str])] = &[
    // Round 2 (D16): the board-view C-g alternate for move-cancel.
    (ActionId::TaskMoveCancel, ContextId::Board, &["Ctrl+g"]),
    // Round 2 (D17): emacs' isearch/read-args chords — Ctrl+f/Ctrl+b
    // free in the Board context (PaletteSearch is Global Ctrl+s).
    (ActionId::StackNext, ContextId::Board, &["Ctrl+f"]),
    (ActionId::StackPrev, ContextId::Board, &["Ctrl+b"]),
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
    // StackDetail
    (ActionId::NavNextItem, ContextId::StackDetail, &["Ctrl+n"]),
    (ActionId::NavPrevItem, ContextId::StackDetail, &["Ctrl+p"]),
    (ActionId::NavFirstItem, ContextId::StackDetail, &["Alt+<"]),
    (ActionId::NavLastItem, ContextId::StackDetail, &["Alt+>"]),
    (ActionId::TaskCreate, ContextId::StackDetail, &["Ctrl+o"]),
    // TaskDetail (keymap-parity plan US-1, D-1): C-n/C-p carry the
    // *item* walk — the r4 D19 control-walk rows were recorded as a
    // move-in-waiting ("the item walk is semantically dead here") and
    // the D-2 route fix made the detail's item table live; the
    // intra-context contract makes this a move, not an add. The
    // control walk keeps its isearch pair (C-f/C-b, the r3 D25 rows
    // below) — freeing C-n/C-p contacts the control walk nowhere else
    // (the Board keeps its live task/stack walk; Today/StackDetail
    // are the recorded exclusions).
    (ActionId::NavNextItem, ContextId::TaskDetail, &["Ctrl+n"]),
    (ActionId::NavPrevItem, ContextId::TaskDetail, &["Ctrl+p"]),
    (ActionId::NavFirstItem, ContextId::TaskDetail, &["Alt+<"]),
    (ActionId::NavLastItem, ContextId::TaskDetail, &["Alt+>"]),
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
    // Round 3 (D25): the control walk — the isearch chords C-f/C-b,
    // free in the four non-board view contexts (the Board context
    // keeps them for stack.next/stack.prev, the recorded emacs
    // preservation; the board's controls keep native Tab: unbound →
    // Unhandled → traversal).
    (ActionId::NavNextControl, ContextId::Today, &["Ctrl+f"]),
    (ActionId::NavPrevControl, ContextId::Today, &["Ctrl+b"]),
    (
        ActionId::NavNextControl,
        ContextId::StackDetail,
        &["Ctrl+f"],
    ),
    (
        ActionId::NavPrevControl,
        ContextId::StackDetail,
        &["Ctrl+b"],
    ),
    (ActionId::NavNextControl, ContextId::TaskDetail, &["Ctrl+f"]),
    (ActionId::NavPrevControl, ContextId::TaskDetail, &["Ctrl+b"]),
    (ActionId::NavNextControl, ContextId::Settings, &["Ctrl+f"]),
    (ActionId::NavPrevControl, ContextId::Settings, &["Ctrl+b"]),
    // r4 D19: C-n/C-p join the walk in Settings (conflict-contract
    // clean — the rows were unbound there; the Board keeps its live
    // task/stack walk, the recorded exclusion for Today/StackDetail).
    (ActionId::NavNextControl, ContextId::Settings, &["Ctrl+n"]),
    (ActionId::NavPrevControl, ContextId::Settings, &["Ctrl+p"]),
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
    // ViNormal carries everything; Global is empty.
    (ActionId::NavNextItem, ContextId::ViNormal, &["j"]),
    (ActionId::NavPrevItem, ContextId::ViNormal, &["k"]),
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
];

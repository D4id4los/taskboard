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
    /// the alternates shape (D7, e.g. emacs `menu.dismiss`).
    #[must_use]
    pub const fn rows(self) -> &'static [(ActionId, ContextId, &'static [&'static str])] {
        match self {
            Self::Standard => STANDARD,
            Self::Emacs => EMACS,
            Self::Vi => VI,
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
        &[ContextId::Menu, ContextId::Palette]
    }
}

/// `standard` — CUA conventions: arrows *are* this preset's
/// convention, Ctrl-chords for the global verbs.
static STANDARD: &[(ActionId, ContextId, &[&str])] = &[
    // Global
    (ActionId::AppUndo, ContextId::Global, &["Ctrl+Z"]),
    (ActionId::AppRedo, ContextId::Global, &["Ctrl+Shift+Z"]),
    (ActionId::AppMenu, ContextId::Global, &["Ctrl+Shift+M"]),
    (ActionId::AppExit, ContextId::Global, &["Ctrl+Q"]),
    (ActionId::AppSettings, ContextId::Global, &["Ctrl+,"]),
    (ActionId::AppHelp, ContextId::Global, &["F1"]),
    (ActionId::CaptureQuick, ContextId::Global, &["Ctrl+K"]),
    (ActionId::PaletteSearch, ContextId::Global, &["Ctrl+F"]),
    // Chunk 10 (D11): the today jump is one stroke from anywhere, all
    // presets (P10's promise); view.inbox and task.due-today stay
    // menu-reachable without default strokes.
    (ActionId::ViewToday, ContextId::Global, &["Ctrl+T"]),
    (ActionId::NavNextView, ContextId::Global, &["Ctrl+PageDown"]),
    (ActionId::NavPrevView, ContextId::Global, &["Ctrl+PageUp"]),
    (ActionId::NavFocusBoard, ContextId::Global, &["Alt+1"]),
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
    (ActionId::BoardNext, ContextId::Board, &["b"]),
    (ActionId::BoardPrevious, ContextId::Board, &["B"]),
    (ActionId::BoardCreate, ContextId::Board, &["Ctrl+Shift+N"]),
    (ActionId::BoardRename, ContextId::Board, &["Ctrl+F2"]),
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
    (ActionId::StackMove, ContextId::Board, &["Ctrl+Alt+M"]),
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
    (ActionId::NavNextControl, ContextId::Settings, &["Tab"]),
    (
        ActionId::NavPrevControl,
        ContextId::Settings,
        &["Shift+Tab"],
    ),
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
];

/// `emacs` — default-emacs-authentic (the review's tmux standard):
/// `C-x C-c` exits, `C-h` is the help prefix, `M-x` is the command
/// surface, org-mode owns the `C-c` prefix for our domain verbs, and
/// navigation rides `C-n`/`C-p`.
static EMACS: &[(ActionId, ContextId, &[&str])] = &[
    // Global
    (ActionId::AppExit, ContextId::Global, &["Ctrl+x", "Ctrl+c"]),
    (ActionId::AppUndo, ContextId::Global, &["Ctrl+x", "u"]),
    (ActionId::AppRedo, ContextId::Global, &["Ctrl+g", "Ctrl+/"]),
    (ActionId::AppMenu, ContextId::Global, &["Alt+x"]),
    (ActionId::AppHelp, ContextId::Global, &["Ctrl+h"]),
    (ActionId::AppSettings, ContextId::Global, &["Ctrl+x", "c"]),
    (ActionId::CaptureQuick, ContextId::Global, &["Ctrl+c", "c"]),
    (ActionId::PaletteSearch, ContextId::Global, &["Ctrl+s"]),
    (ActionId::ViewToday, ContextId::Global, &["Ctrl+T"]),
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
    // Round 3 (W-SM-d, F26): the title re-arm — Return at detail is
    // unambiguous once task-detail entry no longer auto-arms (emacs
    // binds Return only at Board/Today for `task.edit`).
    (ActionId::TaskTitleEdit, ContextId::TaskDetail, &["Return"]),
    // Round 2 (D16): the board-view C-g alternate for move-cancel.
    (ActionId::TaskMoveCancel, ContextId::Board, &["Ctrl+g"]),
    // Round 2 (D17): emacs' isearch/read-args chords — Ctrl+f/Ctrl+b
    // free in the Board context (PaletteSearch is Global Ctrl+s).
    (ActionId::StackNext, ContextId::Board, &["Ctrl+f"]),
    (ActionId::StackPrev, ContextId::Board, &["Ctrl+b"]),
    (ActionId::BoardNext, ContextId::Board, &["b"]),
    (ActionId::BoardPrevious, ContextId::Board, &["B"]),
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
    // TaskDetail
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
];

/// `vi` — vim-authentic: everything lives in `ViNormal` (the honest
/// vim model — normal-mode keys mean nothing in insert mode), `g`/`d`
/// /`Z` are real prefix families (the resolver's accumulation
/// reproduces vim's operator-waits-for-motion behaviour), `ViInsert`
/// binds only Escape.
static VI: &[(ActionId, ContextId, &[&str])] = &[
    // ViNormal carries everything; Global is empty.
    (ActionId::NavNextItem, ContextId::ViNormal, &["j"]),
    (ActionId::NavPrevItem, ContextId::ViNormal, &["k"]),
    (ActionId::NavNextView, ContextId::ViNormal, &["g", "t"]),
    (ActionId::NavPrevView, ContextId::ViNormal, &["g", "T"]),
    (ActionId::NavFocusBoard, ContextId::ViNormal, &["g", "g"]),
    (ActionId::ViewToday, ContextId::Global, &["Ctrl+T"]),
    (ActionId::TaskComplete, ContextId::ViNormal, &["Space"]),
    (ActionId::TaskCreate, ContextId::ViNormal, &["o"]),
    (ActionId::StackCreate, ContextId::ViNormal, &["O"]),
    (ActionId::TaskEdit, ContextId::ViNormal, &["Return"]),
    (ActionId::TaskDelete, ContextId::ViNormal, &["d", "d"]),
    (ActionId::TaskMove, ContextId::ViNormal, &["g", "m"]),
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
    (ActionId::BoardDelete, ContextId::ViNormal, &["g", "d"]),
    // Round 2 (D20): H/G are vim's top/bottom motions, mapped to
    // first/last item in reading order.
    (ActionId::NavFirstItem, ContextId::ViNormal, &["H"]),
    (ActionId::NavLastItem, ContextId::ViNormal, &["G"]),
    (ActionId::TaskReopen, ContextId::ViNormal, &["U"]),
    (ActionId::BoardNext, ContextId::ViNormal, &["b"]),
    (ActionId::BoardPrevious, ContextId::ViNormal, &["B"]),
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
    // Round-1 chunk 3 (D28b): Escape closes the settings surface.
    (ActionId::AppSettings, ContextId::Settings, &["Escape"]),
    // Round 3 (D25): the control walk — Tab/Shift+Tab in the five
    // view contexts (unbound today; under ViInsert the chain masks
    // them and Tab stays the fields' native traversal).
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
    (ActionId::NavNextControl, ContextId::Settings, &["Tab"]),
    (
        ActionId::NavPrevControl,
        ContextId::Settings,
        &["Shift+Tab"],
    ),
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
];

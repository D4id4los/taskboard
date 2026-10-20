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
        // Every shipped preset masks below the open menu only;
        // `ViInsert` shadows without masking (D11).
        let _ = self;
        &[ContextId::Menu]
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
    (ActionId::NavNextView, ContextId::Global, &["Ctrl+PageDown"]),
    (ActionId::NavPrevView, ContextId::Global, &["Ctrl+PageUp"]),
    (ActionId::NavFocusBoard, ContextId::Global, &["Alt+1"]),
    // Board
    (ActionId::NavNextItem, ContextId::Board, &["Down"]),
    (ActionId::NavPrevItem, ContextId::Board, &["Up"]),
    (ActionId::TaskComplete, ContextId::Board, &["Space"]),
    (ActionId::TaskCreate, ContextId::Board, &["N"]),
    (ActionId::StackCreate, ContextId::Board, &["S"]),
    (ActionId::TaskEdit, ContextId::Board, &["Return"]),
    (ActionId::TaskMove, ContextId::Board, &["M"]),
    (ActionId::TaskDelete, ContextId::Board, &["Delete"]),
    (ActionId::BoardNext, ContextId::Board, &["b"]),
    (ActionId::BoardPrevious, ContextId::Board, &["B"]),
    // StackDetail
    (ActionId::NavNextItem, ContextId::StackDetail, &["Down"]),
    (ActionId::NavPrevItem, ContextId::StackDetail, &["Up"]),
    (ActionId::TaskCreate, ContextId::StackDetail, &["N"]),
    // TaskDetail
    (ActionId::NavNextItem, ContextId::TaskDetail, &["Down"]),
    (ActionId::NavPrevItem, ContextId::TaskDetail, &["Up"]),
    // Menu
    (ActionId::MenuNext, ContextId::Menu, &["Down"]),
    (ActionId::MenuPrevious, ContextId::Menu, &["Up"]),
    (ActionId::MenuInvoke, ContextId::Menu, &["Return"]),
    (ActionId::MenuDismiss, ContextId::Menu, &["Escape"]),
    // D5: insert-mode exit — the same single key vi has (the insert
    // chain masks the view context; printables reach the field).
    (ActionId::ModeNormal, ContextId::ViInsert, &["Escape"]),
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
    (ActionId::NavNextView, ContextId::Global, &["Ctrl+x", "]"]),
    (ActionId::NavPrevView, ContextId::Global, &["Ctrl+x", "["]),
    (
        ActionId::NavFocusBoard,
        ContextId::Global,
        &["Ctrl+x", "Ctrl+b"],
    ),
    // Board
    (ActionId::NavNextItem, ContextId::Board, &["Ctrl+n"]),
    (ActionId::NavPrevItem, ContextId::Board, &["Ctrl+p"]),
    (
        ActionId::TaskComplete,
        ContextId::Board,
        &["Ctrl+c", "Ctrl+t"],
    ),
    (ActionId::TaskCreate, ContextId::Board, &["Ctrl+o"]),
    (ActionId::TaskEdit, ContextId::Board, &["Return"]),
    (ActionId::TaskMove, ContextId::Board, &["Ctrl+c", "Ctrl+w"]),
    (ActionId::TaskDelete, ContextId::Board, &["Ctrl+d"]),
    (ActionId::BoardNext, ContextId::Board, &["b"]),
    (ActionId::BoardPrevious, ContextId::Board, &["B"]),
    // StackDetail
    (ActionId::NavNextItem, ContextId::StackDetail, &["Ctrl+n"]),
    (ActionId::NavPrevItem, ContextId::StackDetail, &["Ctrl+p"]),
    (ActionId::TaskCreate, ContextId::StackDetail, &["Ctrl+o"]),
    // TaskDetail
    (ActionId::NavNextItem, ContextId::TaskDetail, &["Ctrl+n"]),
    (ActionId::NavPrevItem, ContextId::TaskDetail, &["Ctrl+p"]),
    // Menu
    (ActionId::MenuNext, ContextId::Menu, &["Ctrl+n"]),
    (ActionId::MenuPrevious, ContextId::Menu, &["Ctrl+p"]),
    (ActionId::MenuInvoke, ContextId::Menu, &["Return"]),
    (ActionId::MenuDismiss, ContextId::Menu, &["Ctrl+g"]),
    (ActionId::MenuDismiss, ContextId::Menu, &["Escape"]),
    // D5: insert-mode exit (see standard's note).
    (ActionId::ModeNormal, ContextId::ViInsert, &["Escape"]),
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
    (ActionId::TaskComplete, ContextId::ViNormal, &["Space"]),
    (ActionId::TaskCreate, ContextId::ViNormal, &["o"]),
    (ActionId::StackCreate, ContextId::ViNormal, &["O"]),
    (ActionId::TaskEdit, ContextId::ViNormal, &["Return"]),
    (ActionId::TaskDelete, ContextId::ViNormal, &["d", "d"]),
    (ActionId::TaskMove, ContextId::ViNormal, &["g", "m"]),
    (ActionId::BoardNext, ContextId::ViNormal, &["b"]),
    (ActionId::BoardPrevious, ContextId::ViNormal, &["B"]),
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
];

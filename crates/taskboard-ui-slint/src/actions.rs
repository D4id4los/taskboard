// SPDX-License-Identifier: AGPL-3.0-only

//! The action registry (GUI Phase 7 chunk 2, design §5.2/§5.8): the
//! stable, namespaced vocabulary every keybinding, menu, and hint is
//! expressed in.
//!
//! Views bind to *actions*, never keys; the registry is the single
//! const table describing every v0.1.0 action — id, label,
//! menu-availability contexts, category, and the `enabled` bit. A
//! dumped registry (the insta snapshot in `tests/keymap_snapshots.rs`)
//! **is** the action documentation (§5.2).
//!
//! This module is pure: no slint types, no IO, no runtime (the §5.5
//! extraction seam — the module must stay liftable into its own crate).

use std::fmt;

/// A keymap resolution context (design §5.2): resolution walks an
/// ordered chain of these, first match wins; a context may mask the
/// contexts below it (declared in keymap data, never hardcoded).
///
/// `Global` terminates every chain. `Menu` is the modal menu context.
/// `ViNormal`/`ViInsert` are the vi-mode binding contexts — ordinary
/// contexts, not a subsystem (design §5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContextId {
    Global,
    Board,
    StackDetail,
    TaskDetail,
    Menu,
    ViNormal,
    ViInsert,
}

impl ContextId {
    /// The stable config/snapshot name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Board => "board",
            Self::StackDetail => "stack-detail",
            Self::TaskDetail => "task-detail",
            Self::Menu => "menu",
            Self::ViNormal => "vi-normal",
            Self::ViInsert => "vi-insert",
        }
    }
}

impl fmt::Display for ContextId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The menu grouping of actions (the R8 seam: menus are generated from
/// the registry, ordered by category — design §5.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    Application,
    Capture,
    Search,
    Navigation,
    Task,
    Menu,
    Mode,
}

impl Category {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Application => "application",
            Self::Capture => "capture",
            Self::Search => "search",
            Self::Navigation => "navigation",
            Self::Task => "task",
            Self::Menu => "menu",
            Self::Mode => "mode",
        }
    }

    /// The menu's section display label (usability pass D5): the
    /// declaration order remains the documented menu order; sections
    /// are derived from this mapping, never hand-written rows.
    #[must_use]
    pub const fn section_label(self) -> &'static str {
        match self {
            Self::Application => "Application",
            Self::Capture => "Capture",
            Self::Search => "Search",
            Self::Navigation => "Navigation",
            Self::Task => "Task",
            Self::Menu => "Menu",
            Self::Mode => "Mode",
        }
    }
}

/// The two-tier menu's submenu vocabulary (GUI review round 1, D6/D10;
/// stage-1 finalization of "grouping stage 1 finalizes"). Declaration
/// order is the parent rows' order on the root page — presentation
/// only: keymap resolution never reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MenuGroup {
    Task,
    BoardStack,
    ViewNav,
    Mode,
    Help,
}

impl MenuGroup {
    /// The parent row's label on the root page.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Task => "Task actions",
            Self::BoardStack => "Board & stack",
            Self::ViewNav => "View & navigation",
            Self::Mode => "Mode",
            Self::Help => "Help",
        }
    }

    /// Every group in declaration order (the root page's parent-row
    /// order).
    #[must_use]
    pub const fn all() -> &'static [MenuGroup] {
        &[
            MenuGroup::Task,
            MenuGroup::BoardStack,
            MenuGroup::ViewNav,
            MenuGroup::Mode,
            MenuGroup::Help,
        ]
    }
}

/// A dispatchable action: the dotted `scope.verb` vocabulary (design
/// §5.2) as a closed field-less enum. The string form lives in the
/// registry table — string ↔ enum conversion goes through
/// [`REGISTRY`], so an unknown string is typed, and the router's
/// exhaustive match makes adding an action touch every dispatch site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionId {
    AppExit,
    AppMenu,
    AppUndo,
    AppRedo,
    AppSettings,
    AppHelp,
    CaptureQuick,
    PaletteSearch,
    NavNextView,
    NavPrevView,
    NavFocusBoard,
    NavNextItem,
    NavPrevItem,
    BoardNext,
    BoardPrevious,
    BoardSelect,
    TaskCreate,
    StackCreate,
    TaskEdit,
    TaskComplete,
    TaskMove,
    TaskDelete,
    ReminderSnooze,
    ReminderMute,
    MenuNext,
    MenuPrevious,
    MenuInvoke,
    MenuDismiss,
    ModeInsert,
    ModeNormal,
}

impl ActionId {
    /// The stable dotted id (config override keys, menu models).
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::AppExit => "app.exit",
            Self::AppMenu => "app.menu",
            Self::AppUndo => "app.undo",
            Self::AppRedo => "app.redo",
            Self::AppSettings => "app.settings",
            Self::AppHelp => "app.help",
            Self::CaptureQuick => "capture.quick",
            Self::PaletteSearch => "palette.search",
            Self::NavNextView => "nav.next-view",
            Self::NavPrevView => "nav.prev-view",
            Self::NavFocusBoard => "nav.focus-board",
            Self::NavNextItem => "nav.next-item",
            Self::NavPrevItem => "nav.prev-item",
            Self::BoardNext => "board.next",
            Self::BoardPrevious => "board.previous",
            Self::BoardSelect => "board.select",
            Self::TaskCreate => "task.create",
            Self::StackCreate => "stack.create",
            Self::TaskEdit => "task.edit",
            Self::TaskComplete => "task.complete",
            Self::TaskMove => "task.move",
            Self::TaskDelete => "task.delete",
            Self::ReminderSnooze => "reminder.snooze",
            Self::ReminderMute => "reminder.mute",
            Self::MenuNext => "menu.next",
            Self::MenuPrevious => "menu.previous",
            Self::MenuInvoke => "menu.invoke",
            Self::MenuDismiss => "menu.dismiss",
            Self::ModeInsert => "mode.insert",
            Self::ModeNormal => "mode.normal",
        }
    }

    /// Resolves a dotted id through the registry; `None` for anything
    /// not in the v0.1.0 table.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        REGISTRY
            .iter()
            .find(|entry| entry.id.id() == id)
            .map(|entry| entry.id)
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// One registry row (design §5.2: `{ id, label, contexts, category }`
/// plus the honest `enabled` bit of §5.8).
#[derive(Debug, Clone, Copy)]
pub struct ActionEntry {
    pub id: ActionId,
    pub label: &'static str,
    /// Menu-availability contexts (§5.2): `Global`-tagged actions are
    /// offered in every context's menu; the view contexts are chunk
    /// 3's. Binding contexts are a *preset-table* concern, not a
    /// registry one.
    pub contexts: &'static [ContextId],
    pub category: Category,
    /// Whether dispatching performs the action's effect in this
    /// build. Reserved actions (§5.8) and chunk-3 flows (§12 row 2)
    /// are `false`: they dispatch a disabled no-op and read as
    /// disabled in the dumped registry.
    pub enabled: bool,
    /// Presentation-only display priority (round-1 D5): lower =
    /// earlier on a page. Spaces of 10 leave room for insertions.
    /// Keymap resolution never reads it.
    pub menu_rank: u16,
    /// One or two sentences: what the function does and its effect;
    /// disabled rows state that they are not yet available (round-1
    /// D10). Rendered as the row tooltip (D8) and the a11y
    /// description. Empty is allowed by the type; the registry
    /// review requires every row to carry one.
    pub description: &'static str,
    /// The submenu that owns this row in the two-tier menu (D6/D10);
    /// `None` = top-level (quick rows, Quit, the menu cursor verbs).
    /// Presentation-only.
    pub menu_group: Option<MenuGroup>,
}

const GLOBAL: &[ContextId] = &[ContextId::Global];
const VIEW_NAV: &[ContextId] = &[
    ContextId::Board,
    ContextId::StackDetail,
    ContextId::TaskDetail,
];
/// The board verbs' menu availability (design §3.8): global-capable
/// like the `nav.*` family — offered in every context's menu.
const GLOBAL_VIEW_NAV: &[ContextId] = &[
    ContextId::Global,
    ContextId::Board,
    ContextId::StackDetail,
    ContextId::TaskDetail,
];

/// The v0.1.0 action registry (design §5.2 table, decisions D2/D4/D5):
/// const data, ordered for the documentation dump. `app.menu`,
/// `app.exit`, `task.complete`, `task.delete`, the `menu.*` cursor
/// actions, and the `mode.*` flips dispatch for real in this chunk;
/// everything else is reserved or chunk-3 staged and reads as
/// disabled. The round-1 curation (D5/D10) adds the presentation-only
/// `menu_rank` / `description` / `menu_group` columns: the two-tier
/// menu is generated from them, never hand-written.
pub const REGISTRY: &[ActionEntry] = &[
    ActionEntry {
        id: ActionId::AppExit,
        label: "Quit taskboard",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: true,
        // The Quit-pinned-last guarantee is the projection rule, not
        // this number (a Tier-1 test pins the position).
        menu_rank: 1000,
        description: "Quits the application; local state is already saved.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::AppMenu,
        label: "Command menu",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: true,
        menu_rank: 60,
        description: "Opens or closes the command menu, the keyboard's way into every action.",
        menu_group: Some(MenuGroup::Help),
    },
    ActionEntry {
        id: ActionId::AppUndo,
        label: "Undo",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: false,
        menu_rank: 30,
        description: "Undoes the last change. Not yet available — tracked as backlog #356.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::AppRedo,
        label: "Redo",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: false,
        menu_rank: 40,
        description: "Re-applies the last undone change. Not yet available — tracked as backlog #353.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::AppSettings,
        label: "Open settings",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: false,
        menu_rank: 10,
        description: "Opens the settings view. Not yet available — lands with the settings chunk.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::AppHelp,
        label: "Open help",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: false,
        menu_rank: 70,
        description: "Shows the keymap and usage help. Not yet available — reserved.",
        menu_group: Some(MenuGroup::Help),
    },
    ActionEntry {
        id: ActionId::CaptureQuick,
        label: "Quick capture",
        contexts: GLOBAL,
        category: Category::Capture,
        enabled: false,
        menu_rank: 20,
        description: "Captures a thought into the inbox from anywhere. Not yet available — tracked as backlog #347.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::PaletteSearch,
        label: "Search",
        contexts: GLOBAL,
        category: Category::Search,
        enabled: false,
        menu_rank: 50,
        description: "Searches tasks across all boards. Not yet available — reserved.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavNextView,
        label: "Next view",
        contexts: GLOBAL,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 60,
        description: "Moves to the next view in the board → stack → task cycle.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavPrevView,
        label: "Previous view",
        contexts: GLOBAL,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 70,
        description: "Moves to the previous view in the board → stack → task cycle.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavFocusBoard,
        label: "Go to board",
        contexts: GLOBAL,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 80,
        description: "Returns to the board view from any deeper view.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavNextItem,
        label: "Focus next item",
        contexts: VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 90,
        description: "Moves the focus to the next card or switcher entry in reading order.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::NavPrevItem,
        label: "Focus previous item",
        contexts: VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 100,
        description: "Moves the focus to the previous card or switcher entry in reading order.",
        menu_group: Some(MenuGroup::ViewNav),
    },
    ActionEntry {
        id: ActionId::BoardNext,
        label: "Next board",
        contexts: GLOBAL_VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 60,
        description: "Switches to the next board in canonical order.",
        menu_group: Some(MenuGroup::BoardStack),
    },
    ActionEntry {
        id: ActionId::BoardPrevious,
        label: "Previous board",
        contexts: GLOBAL_VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 70,
        description: "Switches to the previous board in canonical order.",
        menu_group: Some(MenuGroup::BoardStack),
    },
    ActionEntry {
        id: ActionId::BoardSelect,
        label: "Select board",
        contexts: GLOBAL_VIEW_NAV,
        category: Category::Navigation,
        enabled: true,
        menu_rank: 80,
        description: "Switches to the board picked from the board bar (click a board segment).",
        menu_group: Some(MenuGroup::BoardStack),
    },
    ActionEntry {
        id: ActionId::TaskCreate,
        label: "New task",
        contexts: &[ContextId::Board, ContextId::StackDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 60,
        description: "Opens the inline create row and puts it into insert mode.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::StackCreate,
        label: "New stack",
        contexts: &[ContextId::Board, ContextId::StackDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 90,
        description: "Opens the new-stack field on the current board.",
        menu_group: Some(MenuGroup::BoardStack),
    },
    ActionEntry {
        id: ActionId::TaskEdit,
        label: "Edit task",
        contexts: &[ContextId::Board, ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 70,
        description: "Opens the focused task's detail view with the title selected for editing.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskComplete,
        label: "Complete task",
        contexts: &[ContextId::Board, ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 80,
        description: "Marks the focused task as done.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskMove,
        label: "Move task",
        contexts: &[ContextId::Board, ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 90,
        description: "Starts the move flow: pick a target stack, then confirm to move the focused task.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::TaskDelete,
        label: "Delete task",
        contexts: &[ContextId::Board, ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
        menu_rank: 100,
        description: "Deletes the focused task immediately (no confirmation step).",
        menu_group: Some(MenuGroup::Task),
    },
    // Q23's banner verbs (chunk-3 D4): menu-reachable via `app.menu`;
    // preset bindings are recorded backlog debt with the reminders-UI
    // wave.
    ActionEntry {
        id: ActionId::ReminderSnooze,
        label: "Snooze reminder",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 110,
        description: "Pushes the selected reminder's due moment out by one hour.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::ReminderMute,
        label: "Mute reminders",
        contexts: GLOBAL,
        category: Category::Task,
        enabled: true,
        menu_rank: 120,
        description: "Silences all reminders on the selected task.",
        menu_group: Some(MenuGroup::Task),
    },
    ActionEntry {
        id: ActionId::MenuNext,
        label: "Next menu entry",
        contexts: &[ContextId::Menu],
        category: Category::Menu,
        enabled: true,
        menu_rank: 900,
        description: "Moves the menu cursor to the next row of the active page.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::MenuPrevious,
        label: "Previous menu entry",
        contexts: &[ContextId::Menu],
        category: Category::Menu,
        enabled: true,
        menu_rank: 900,
        description: "Moves the menu cursor to the previous row of the active page.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::MenuInvoke,
        label: "Invoke menu entry",
        contexts: &[ContextId::Menu],
        category: Category::Menu,
        enabled: true,
        menu_rank: 900,
        description: "Activates the menu row under the cursor.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::MenuDismiss,
        label: "Dismiss menu",
        contexts: &[ContextId::Menu],
        category: Category::Menu,
        enabled: true,
        menu_rank: 900,
        description: "Steps the menu back one level, or closes it at the top level.",
        menu_group: None,
    },
    ActionEntry {
        id: ActionId::ModeInsert,
        label: "Enter insert mode",
        contexts: GLOBAL,
        category: Category::Mode,
        enabled: true,
        menu_rank: 60,
        description: "Opens the view's editing flow in insert mode.",
        menu_group: Some(MenuGroup::Mode),
    },
    ActionEntry {
        id: ActionId::ModeNormal,
        label: "Return to normal mode",
        contexts: GLOBAL,
        category: Category::Mode,
        enabled: true,
        menu_rank: 70,
        description: "Ends the active editing flow and returns to normal mode (Escape).",
        menu_group: Some(MenuGroup::Mode),
    },
];

/// The registry row for an action.
///
/// # Panics
///
/// Never in a reviewed tree: panics only if the registry lacks a row
/// for the variant — a broken-table bug caught by
/// `registry_covers_every_action_exactly_once`.
#[must_use]
pub fn entry(id: ActionId) -> &'static ActionEntry {
    // Every ActionId variant has a row; the registry is the one
    // definition site (a missing row is a broken-table bug caught by
    // `registry_covers_every_action` below, not a runtime branch).
    REGISTRY
        .iter()
        .find(|entry| entry.id == id)
        .expect("registry has a row for every ActionId variant")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry is the one definition site: every enum variant
    /// must have exactly one row.
    #[test]
    fn registry_covers_every_action_exactly_once() {
        const ALL: &[ActionId] = &[
            ActionId::AppExit,
            ActionId::AppMenu,
            ActionId::AppUndo,
            ActionId::AppRedo,
            ActionId::AppSettings,
            ActionId::AppHelp,
            ActionId::CaptureQuick,
            ActionId::PaletteSearch,
            ActionId::NavNextView,
            ActionId::NavPrevView,
            ActionId::NavFocusBoard,
            ActionId::NavNextItem,
            ActionId::NavPrevItem,
            ActionId::BoardNext,
            ActionId::BoardPrevious,
            ActionId::BoardSelect,
            ActionId::TaskCreate,
            ActionId::StackCreate,
            ActionId::TaskEdit,
            ActionId::TaskComplete,
            ActionId::TaskMove,
            ActionId::TaskDelete,
            ActionId::ReminderSnooze,
            ActionId::ReminderMute,
            ActionId::MenuNext,
            ActionId::MenuPrevious,
            ActionId::MenuInvoke,
            ActionId::MenuDismiss,
            ActionId::ModeInsert,
            ActionId::ModeNormal,
        ];
        for id in ALL {
            assert_eq!(REGISTRY.iter().filter(|e| e.id == *id).count(), 1);
        }
        assert_eq!(REGISTRY.len(), ALL.len());
    }

    /// Ids are unique and dotted `scope.verb` shaped (structural
    /// fact, not text matching — the two-part shape is the §5.2
    /// namespace contract).
    #[test]
    fn ids_are_unique_and_scoped() {
        for entry in REGISTRY {
            assert!(entry.id.id().contains('.'));
        }
        let unique: std::collections::BTreeSet<&str> = REGISTRY.iter().map(|e| e.id.id()).collect();
        assert_eq!(unique.len(), REGISTRY.len());
    }

    /// String ↔ enum conversion runs through the registry (D1): a
    /// known id round-trips, an unknown one is `None` (typed at the
    /// caller as `UnknownAction`).
    #[test]
    fn id_conversion_round_trips_through_the_registry() {
        assert_eq!(
            ActionId::from_id("task.complete"),
            Some(ActionId::TaskComplete)
        );
        assert_eq!(ActionId::from_id("mode.insert"), Some(ActionId::ModeInsert));
        assert_eq!(ActionId::from_id("nope.thing"), None);
        for entry in REGISTRY {
            assert_eq!(ActionId::from_id(entry.id.id()), Some(entry.id));
        }
    }

    /// The §5.8 reservation, asserted structurally: the reserved
    /// global actions exist with labels and are *not* enabled —
    /// exactly the set D4 names (the dumped snapshot shows them as
    /// disabled rows; this test locks the bit).
    #[test]
    fn reserved_globals_are_present_and_disabled() {
        for id in [
            ActionId::AppUndo,
            ActionId::AppRedo,
            ActionId::CaptureQuick,
            ActionId::PaletteSearch,
            ActionId::AppSettings,
            ActionId::AppHelp,
        ] {
            let row = entry(id);
            assert!(!row.enabled, "{} must read disabled", row.id);
            assert_eq!(row.contexts, GLOBAL);
        }
    }

    /// Post-flip (chunk-3 D6): the *only* disabled rows are the
    /// reserved set — every other action dispatches for real. The
    /// reserved bits themselves are pinned by
    /// `reserved_globals_are_present_and_disabled`.
    #[test]
    fn only_reserved_actions_stay_disabled() {
        for row in REGISTRY {
            if reserved_globals_are_present_and_disabled_set().contains(&row.id) {
                assert!(!row.enabled, "{} is reserved", row.id);
            } else {
                assert!(row.enabled, "{} must be enabled", row.id);
            }
        }
    }

    /// Round-1 D7 totality: every registry row carries a non-empty
    /// description (the row tooltip / a11y description source), and
    /// the top-level quick rows' ranks are strictly spaced (the
    /// documented insertion room).
    #[test]
    fn every_row_carries_a_description_and_spaced_top_ranks() {
        for row in REGISTRY {
            assert!(
                !row.description.is_empty(),
                "{} lacks a description",
                row.id
            );
        }
        let top: Vec<u16> = REGISTRY
            .iter()
            .filter(|row| {
                row.menu_group.is_none()
                    && row.id != ActionId::AppExit
                    // The menu-cursor rows never render on a page; only
                    // the projected quick rows must be rank-ordered.
                    && !row.contexts.contains(&ContextId::Menu)
            })
            .map(|row| row.menu_rank)
            .collect();
        for pair in top.windows(2) {
            assert!(
                pair[1].abs_diff(pair[0]) >= 10,
                "ranks spaced by 10: {pair:?}"
            );
        }
    }

    /// The reserved set as data for the post-flip diff (the original
    /// test's list, kept in one place).
    fn reserved_globals_are_present_and_disabled_set() -> std::collections::BTreeSet<ActionId> {
        std::collections::BTreeSet::from([
            ActionId::AppUndo,
            ActionId::AppRedo,
            ActionId::AppSettings,
            ActionId::AppHelp,
            ActionId::CaptureQuick,
            ActionId::PaletteSearch,
        ])
    }
}

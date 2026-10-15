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
    TaskCreate,
    TaskEdit,
    TaskComplete,
    TaskMove,
    TaskDelete,
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
            Self::TaskCreate => "task.create",
            Self::TaskEdit => "task.edit",
            Self::TaskComplete => "task.complete",
            Self::TaskMove => "task.move",
            Self::TaskDelete => "task.delete",
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
}

const GLOBAL: &[ContextId] = &[ContextId::Global];
const VIEW_NAV: &[ContextId] = &[
    ContextId::Board,
    ContextId::StackDetail,
    ContextId::TaskDetail,
];

/// The v0.1.0 action registry (design §5.2 table, decisions D2/D4/D5):
/// const data, ordered for the documentation dump. `app.menu`,
/// `app.exit`, `task.complete`, `task.delete`, the `menu.*` cursor
/// actions, and the `mode.*` flips dispatch for real in this chunk;
/// everything else is reserved or chunk-3 staged and reads as
/// disabled.
pub const REGISTRY: &[ActionEntry] = &[
    ActionEntry {
        id: ActionId::AppExit,
        label: "Quit taskboard",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: true,
    },
    ActionEntry {
        id: ActionId::AppMenu,
        label: "Command menu",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: true,
    },
    ActionEntry {
        id: ActionId::AppUndo,
        label: "Undo",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: false,
    },
    ActionEntry {
        id: ActionId::AppRedo,
        label: "Redo",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: false,
    },
    ActionEntry {
        id: ActionId::AppSettings,
        label: "Open settings",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: false,
    },
    ActionEntry {
        id: ActionId::AppHelp,
        label: "Open help",
        contexts: GLOBAL,
        category: Category::Application,
        enabled: false,
    },
    ActionEntry {
        id: ActionId::CaptureQuick,
        label: "Quick capture",
        contexts: GLOBAL,
        category: Category::Capture,
        enabled: false,
    },
    ActionEntry {
        id: ActionId::PaletteSearch,
        label: "Search",
        contexts: GLOBAL,
        category: Category::Search,
        enabled: false,
    },
    ActionEntry {
        id: ActionId::NavNextView,
        label: "Next view",
        contexts: GLOBAL,
        category: Category::Navigation,
        enabled: false,
    },
    ActionEntry {
        id: ActionId::NavPrevView,
        label: "Previous view",
        contexts: GLOBAL,
        category: Category::Navigation,
        enabled: false,
    },
    ActionEntry {
        id: ActionId::NavFocusBoard,
        label: "Go to board",
        contexts: GLOBAL,
        category: Category::Navigation,
        enabled: false,
    },
    ActionEntry {
        id: ActionId::NavNextItem,
        label: "Focus next item",
        contexts: VIEW_NAV,
        category: Category::Navigation,
        enabled: false,
    },
    ActionEntry {
        id: ActionId::NavPrevItem,
        label: "Focus previous item",
        contexts: VIEW_NAV,
        category: Category::Navigation,
        enabled: false,
    },
    ActionEntry {
        id: ActionId::TaskCreate,
        label: "New task",
        contexts: &[ContextId::Board, ContextId::StackDetail],
        category: Category::Task,
        enabled: false,
    },
    ActionEntry {
        id: ActionId::TaskEdit,
        label: "Edit task",
        contexts: &[ContextId::Board, ContextId::TaskDetail],
        category: Category::Task,
        enabled: false,
    },
    ActionEntry {
        id: ActionId::TaskComplete,
        label: "Complete task",
        contexts: &[ContextId::Board, ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
    },
    ActionEntry {
        id: ActionId::TaskMove,
        label: "Move task",
        contexts: &[ContextId::Board, ContextId::TaskDetail],
        category: Category::Task,
        enabled: false,
    },
    ActionEntry {
        id: ActionId::TaskDelete,
        label: "Delete task",
        contexts: &[ContextId::Board, ContextId::TaskDetail],
        category: Category::Task,
        enabled: true,
    },
    ActionEntry {
        id: ActionId::MenuNext,
        label: "Next menu entry",
        contexts: &[ContextId::Menu],
        category: Category::Menu,
        enabled: true,
    },
    ActionEntry {
        id: ActionId::MenuPrevious,
        label: "Previous menu entry",
        contexts: &[ContextId::Menu],
        category: Category::Menu,
        enabled: true,
    },
    ActionEntry {
        id: ActionId::MenuInvoke,
        label: "Invoke menu entry",
        contexts: &[ContextId::Menu],
        category: Category::Menu,
        enabled: true,
    },
    ActionEntry {
        id: ActionId::MenuDismiss,
        label: "Dismiss menu",
        contexts: &[ContextId::Menu],
        category: Category::Menu,
        enabled: true,
    },
    ActionEntry {
        id: ActionId::ModeInsert,
        label: "Enter insert mode",
        contexts: GLOBAL,
        category: Category::Mode,
        enabled: true,
    },
    ActionEntry {
        id: ActionId::ModeNormal,
        label: "Return to normal mode",
        contexts: GLOBAL,
        category: Category::Mode,
        enabled: true,
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
            ActionId::TaskCreate,
            ActionId::TaskEdit,
            ActionId::TaskComplete,
            ActionId::TaskMove,
            ActionId::TaskDelete,
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

    /// The chunk-3 staged handlers (D5) are bound-but-disabled until
    /// the views flip the bits.
    #[test]
    fn chunk3_staged_actions_are_disabled() {
        for id in [
            ActionId::NavNextView,
            ActionId::NavPrevView,
            ActionId::NavFocusBoard,
            ActionId::NavNextItem,
            ActionId::NavPrevItem,
            ActionId::TaskCreate,
            ActionId::TaskEdit,
            ActionId::TaskMove,
        ] {
            assert!(!entry(id).enabled);
        }
    }
}

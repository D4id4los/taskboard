// SPDX-License-Identifier: AGPL-3.0-only

//! The action router (chunk decision D16, design §5 pipeline): a
//! pure, exhaustive `ActionId` → outcome mapping. No slint types, no
//! channels — the adapter sends [`DispatchOutcome::Command`] outcomes
//! over the mpsc and applies [`UiEffect`]s to the shell state.

use taskboard_domain::{StackId, StateCommand, TaskId};

use crate::actions::{ActionId, entry as registry_entry};

/// The vi mode (D17): two plain shell states — ordinary state, not a
/// subsystem (D10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
}

/// UI-internal effects (design §5: "ui-internal actions → focus
/// moves, view switches, mode flips"). The adapter applies them to
/// the shell/window; `Exit` maps to the slint event-loop quit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEffect {
    Exit,
    OpenMenu,
    CloseMenu,
    MenuNext,
    MenuPrevious,
    /// The adapter closes the menu and re-dispatches the selected
    /// entry — bounded (the menu is closed first, no recursion).
    InvokeSelected,
    SetMode(Mode),
}

/// What dispatching an action produced (D16).
#[derive(Debug, Clone, PartialEq)]
pub enum DispatchOutcome {
    /// A domain intent for the engine (via the command mpsc).
    Command(StateCommand),
    /// A UI-internal effect for the shell/window.
    UiEffect(UiEffect),
    /// The action is reserved or chunk-3 staged: an absorbed no-op
    /// that reads as disabled in the registry (§5.8).
    Disabled,
    /// The action is enabled but needs a focused entity that is not
    /// there (chunk-2 sessions have no views, D16).
    NoTarget,
}

/// The focused-entity seam (D16): chunk 2's shell has no views, so
/// real sessions carry `None`s (keyboard task verbs resolve to
/// [`DispatchOutcome::NoTarget`]); chunk 3's focus architecture sets
/// them for real.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DispatchCtx {
    pub focused_task: Option<TaskId>,
    pub focused_stack: Option<StackId>,
}

/// The pure dispatch table. Every [`ActionId`] variant is matched —
/// adding an action without a routing decision is a compile error.
#[derive(Debug, Clone, Copy, Default)]
pub struct ActionRouter;

impl ActionRouter {
    #[must_use]
    pub fn dispatch(id: ActionId, ctx: &DispatchCtx) -> DispatchOutcome {
        // The registry's enabled bit decides before any target check:
        // a disabled action is an absorbed no-op even with focus set.
        if !registry_entry(id).enabled {
            return DispatchOutcome::Disabled;
        }
        match id {
            // Application (enabled in chunk 2).
            ActionId::AppExit => DispatchOutcome::UiEffect(UiEffect::Exit),
            ActionId::AppMenu => DispatchOutcome::UiEffect(UiEffect::OpenMenu),
            // Menu cursor (D3).
            ActionId::MenuNext => DispatchOutcome::UiEffect(UiEffect::MenuNext),
            ActionId::MenuPrevious => DispatchOutcome::UiEffect(UiEffect::MenuPrevious),
            ActionId::MenuInvoke => DispatchOutcome::UiEffect(UiEffect::InvokeSelected),
            ActionId::MenuDismiss => DispatchOutcome::UiEffect(UiEffect::CloseMenu),
            // Mode flips (D10): ordinary ui-internal actions.
            ActionId::ModeInsert => DispatchOutcome::UiEffect(UiEffect::SetMode(Mode::Insert)),
            ActionId::ModeNormal => DispatchOutcome::UiEffect(UiEffect::SetMode(Mode::Normal)),
            // Task verbs complete without views (D16): the focused
            // task suffices — the chunk-2 end-to-end path.
            ActionId::TaskComplete => match ctx.focused_task {
                Some(id) => DispatchOutcome::Command(StateCommand::SetTaskDone { id, done: true }),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::TaskDelete => match ctx.focused_task {
                Some(id) => DispatchOutcome::Command(StateCommand::DeleteTask { id }),
                None => DispatchOutcome::NoTarget,
            },
            // Reserved (§5.8) and chunk-3 staged (D5) — all disabled.
            ActionId::AppUndo
            | ActionId::AppRedo
            | ActionId::AppSettings
            | ActionId::AppHelp
            | ActionId::CaptureQuick
            | ActionId::PaletteSearch
            | ActionId::NavNextView
            | ActionId::NavPrevView
            | ActionId::NavFocusBoard
            | ActionId::NavNextItem
            | ActionId::NavPrevItem
            | ActionId::TaskCreate
            | ActionId::TaskEdit
            | ActionId::TaskMove => DispatchOutcome::Disabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskboard_domain::TaskId;
    use uuid::Uuid;

    fn ctx(task: bool) -> DispatchCtx {
        DispatchCtx {
            focused_task: task.then(|| TaskId::from_uuid(Uuid::now_v7())),
            focused_stack: None,
        }
    }

    /// T22 — the enabled task verbs route to exact typed commands.
    #[test]
    fn task_verbs_route_to_exact_commands() {
        let ctx = ctx(true);
        let task = ctx.focused_task.expect("set");
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskComplete, &ctx),
            DispatchOutcome::Command(StateCommand::SetTaskDone {
                id: task,
                done: true
            })
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskDelete, &ctx),
            DispatchOutcome::Command(StateCommand::DeleteTask { id: task })
        );
    }

    /// T22 — enabled task verbs without focus resolve to `NoTarget`,
    /// an absorbed no-op (chunk-2 sessions have no views).
    #[test]
    fn task_verbs_without_focus_are_no_target() {
        let ctx = DispatchCtx::default();
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskComplete, &ctx),
            DispatchOutcome::NoTarget
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::TaskDelete, &ctx),
            DispatchOutcome::NoTarget
        );
    }

    /// T22 — the disabled bit wins before any target check (§5.8:
    /// reserved actions are honest no-ops even with focus set).
    #[test]
    fn disabled_actions_are_absorbed_even_with_focus() {
        let ctx = ctx(true);
        for id in [
            ActionId::AppUndo,
            ActionId::AppRedo,
            ActionId::CaptureQuick,
            ActionId::PaletteSearch,
            ActionId::NavNextItem,
            ActionId::TaskCreate,
            ActionId::TaskEdit,
            ActionId::TaskMove,
        ] {
            assert_eq!(ActionRouter::dispatch(id, &ctx), DispatchOutcome::Disabled);
        }
    }

    /// T22 — the UI-internal effect table.
    #[test]
    fn ui_internal_actions_yield_effects() {
        let ctx = DispatchCtx::default();
        assert_eq!(
            ActionRouter::dispatch(ActionId::AppExit, &ctx),
            DispatchOutcome::UiEffect(UiEffect::Exit)
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::AppMenu, &ctx),
            DispatchOutcome::UiEffect(UiEffect::OpenMenu)
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::MenuDismiss, &ctx),
            DispatchOutcome::UiEffect(UiEffect::CloseMenu)
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::ModeInsert, &ctx),
            DispatchOutcome::UiEffect(UiEffect::SetMode(Mode::Insert))
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::ModeNormal, &ctx),
            DispatchOutcome::UiEffect(UiEffect::SetMode(Mode::Normal))
        );
    }
}

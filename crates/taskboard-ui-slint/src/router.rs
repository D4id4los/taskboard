// SPDX-License-Identifier: AGPL-3.0-only

//! The action router (chunk decision D16, design §5 pipeline): a
//! pure, exhaustive `ActionId` → outcome mapping. No slint types, no
//! channels — the adapter sends [`DispatchOutcome::Command`] outcomes
//! over the mpsc and applies [`UiEffect`]s to the shell state.

use chrono::TimeDelta;
use taskboard_domain::{BoardId, ReminderOffset, ReminderRuleId, StackId, StateCommand, TaskId};

use crate::actions::{ActionId, entry as registry_entry};
use crate::presentation::ReminderTarget;

/// How far a banner snooze pushes an occurrence out (D4): one hour.
pub const SNOOZE_POSTPONE: TimeDelta = TimeDelta::hours(1);

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
    /// Level-aware menu back (round-1 D6): a submenu steps back one
    /// level, the root closes.
    MenuBack,
    MenuNext,
    MenuPrevious,
    /// The adapter closes the menu and re-dispatches the selected
    /// entry — bounded (the menu is closed first, no recursion).
    InvokeSelected,
    SetMode(Mode),
    // --- chunk-3 view intents (D6/D9): names state the function; the
    // adapter resolves them against the view-state model.
    /// Advance board focus in reading order (also the move-flow's
    /// target-cycle: the adapter's interpretation table decides).
    FocusNext,
    /// Retreat board focus in reading order.
    FocusPrev,
    /// Enter the next view in the one view cycle.
    NextView,
    /// Enter the previous view in the one view cycle.
    PrevView,
    /// Return to the board view and its scope.
    FocusBoard,
    /// Open the inline create row.
    BeginCreate,
    /// Open the inline create row for a new stack on the live board
    /// (works on an empty board — no target stack needed).
    BeginCreateStack,
    /// The primary-activation intent: open/commit whatever the focus
    /// and the active flow mean by "activate".
    Activate,
    /// Ask for move mode (a second request cancels it).
    RequestMove,
    // --- chunk-7 board dimension (design §3.8): pure view state; the
    // adapter resolves them against the view-state model. Never a
    // `DispatchOutcome::Command`.
    /// Step to the next live board (canonical order, no wrap).
    BoardNext,
    /// Step to the previous live board.
    BoardPrevious,
    /// Select the parameterized board (`DispatchCtx::board_target`).
    BoardSelect(BoardId),
    // --- round-1 chunk 2 (D18): the keyboard stack-width verbs —
    // pure view intents (the BoardNext precedent); the adapter
    // computes the live ceiling and routes to the view state.
    /// Widen the focused stack's column by one step.
    StackWider,
    /// Narrow the focused stack's column by one step.
    StackNarrower,
    // --- round-1 chunk 3 (D28b): the settings surface — a `ViewId`
    // excluded from the view cycle; the adapter toggles it.
    /// Open the settings view (or close it — toggle semantics).
    OpenSettings,
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
    /// The banner row's concrete reminder target (D4): set by the
    /// adapter when a banner row is activated; snooze/mute route from
    /// it (`NoTarget` without one).
    pub reminder_target: Option<ReminderTarget>,
    /// The board-bar segment's board (design §3.8): set by the adapter
    /// when a segment is activated; `board.select` routes from it
    /// (`NoTarget` without one — a menu invocation carries no target).
    pub board_target: Option<BoardId>,
    /// The rule-list row's rule (chunk 6, D11): set by the adapter when
    /// a row's remove affordance activates; `reminder.rule-remove`
    /// routes from it (`NoTarget` without one).
    pub rule_target: Option<ReminderRuleId>,
    /// The rule-list add entry's parsed offset (chunk 6, D11): set by
    /// the adapter (preset or custom); `reminder.rule-add` routes from
    /// it together with the focused task (`NoTarget` without both).
    pub rule_add_offset: Option<ReminderOffset>,
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
            // Level-aware (round-1 D6): Escape steps back level by
            // level. The full close is the dismissal layer's and the
            // toggle's — the adapter applies `CloseMenu` directly.
            ActionId::MenuDismiss => DispatchOutcome::UiEffect(UiEffect::MenuBack),
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
            // The Q23 banner verbs (D4): concrete targets only.
            ActionId::ReminderSnooze => match ctx.reminder_target {
                Some(target) => DispatchOutcome::Command(StateCommand::SnoozeReminder {
                    reminder: target.reminder,
                    occurrence: target.occurrence,
                    postpone: SNOOZE_POSTPONE,
                }),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::ReminderMute => match ctx.reminder_target {
                Some(target) => DispatchOutcome::Command(StateCommand::SetTaskRemindersMuted {
                    task: target.task,
                    muted: true,
                }),
                None => DispatchOutcome::NoTarget,
            },
            // The rule-list verbs (chunk 6, D11): the banner-verb
            // pattern — concrete one-shot targets only; a menu
            // invocation carries neither.
            ActionId::ReminderRuleAdd => match (ctx.focused_task, ctx.rule_add_offset) {
                (Some(task), Some(offset)) => {
                    DispatchOutcome::Command(StateCommand::AddReminderRule { task, offset })
                }
                _ => DispatchOutcome::NoTarget,
            },
            ActionId::ReminderRuleRemove => match ctx.rule_target {
                Some(rule) => DispatchOutcome::Command(StateCommand::RemoveReminderRule { rule }),
                None => DispatchOutcome::NoTarget,
            },
            // The chunk-3 view intents (D6): the adapter resolves the
            // effects against the view-state model.
            ActionId::NavNextView => DispatchOutcome::UiEffect(UiEffect::NextView),
            ActionId::NavPrevView => DispatchOutcome::UiEffect(UiEffect::PrevView),
            ActionId::NavFocusBoard => DispatchOutcome::UiEffect(UiEffect::FocusBoard),
            ActionId::NavNextItem => DispatchOutcome::UiEffect(UiEffect::FocusNext),
            ActionId::NavPrevItem => DispatchOutcome::UiEffect(UiEffect::FocusPrev),
            ActionId::TaskCreate => DispatchOutcome::UiEffect(UiEffect::BeginCreate),
            ActionId::StackCreate => DispatchOutcome::UiEffect(UiEffect::BeginCreateStack),
            ActionId::TaskEdit => DispatchOutcome::UiEffect(UiEffect::Activate),
            ActionId::TaskMove => DispatchOutcome::UiEffect(UiEffect::RequestMove),
            // The board dimension (design §3.8): pure view intents.
            ActionId::BoardNext => DispatchOutcome::UiEffect(UiEffect::BoardNext),
            ActionId::BoardPrevious => DispatchOutcome::UiEffect(UiEffect::BoardPrevious),
            ActionId::BoardSelect => match ctx.board_target {
                Some(board) => DispatchOutcome::UiEffect(UiEffect::BoardSelect(board)),
                None => DispatchOutcome::NoTarget,
            },
            ActionId::StackWider => DispatchOutcome::UiEffect(UiEffect::StackWider),
            ActionId::StackNarrower => DispatchOutcome::UiEffect(UiEffect::StackNarrower),
            // Round-1 chunk 3 (D28b): the settings opener is live.
            ActionId::AppSettings => DispatchOutcome::UiEffect(UiEffect::OpenSettings),
            // Reserved (§5.8) — the only disabled rows left (D6).
            ActionId::AppUndo
            | ActionId::AppRedo
            | ActionId::AppHelp
            | ActionId::CaptureQuick
            | ActionId::PaletteSearch => DispatchOutcome::Disabled,
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
            reminder_target: None,
            board_target: None,
            rule_target: None,
            rule_add_offset: None,
        }
    }

    fn banner_target() -> crate::presentation::ReminderTarget {
        crate::presentation::ReminderTarget {
            task: TaskId::from_uuid(Uuid::now_v7()),
            reminder: taskboard_domain::ReminderRef::Default(TaskId::from_uuid(Uuid::now_v7())),
            occurrence: chrono::Utc::now(),
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

    /// Chunk 6 (D11): the rule-list verbs follow the banner-verb
    /// pattern — exact typed commands from the one-shot targets,
    /// `NoTarget` without them (a menu invocation carries neither).
    #[test]
    fn rule_verbs_route_from_their_targets() {
        let mut ctx = ctx(true);
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderRuleAdd, &ctx),
            DispatchOutcome::NoTarget,
            "no offset"
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderRuleRemove, &ctx),
            DispatchOutcome::NoTarget,
            "no rule target"
        );
        ctx.rule_add_offset = Some(
            taskboard_domain::ReminderOffset::before(chrono::TimeDelta::try_minutes(-5).unwrap())
                .unwrap(),
        );
        let task = ctx.focused_task.expect("set");
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderRuleAdd, &ctx),
            DispatchOutcome::Command(StateCommand::AddReminderRule {
                task,
                offset: ctx.rule_add_offset.expect("set")
            })
        );
        let rule = taskboard_domain::ReminderRuleId::from_uuid(Uuid::now_v7());
        ctx.rule_target = Some(rule);
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderRuleRemove, &ctx),
            DispatchOutcome::Command(StateCommand::RemoveReminderRule { rule })
        );
    }

    /// Chunk 6 (D11): the add verb needs focus AND offset — focus alone
    /// is `NoTarget`.
    #[test]
    fn rule_add_without_focus_or_offset_is_no_target() {
        let mut ctx = ctx(false);
        ctx.rule_add_offset = Some(taskboard_domain::ReminderOffset::at_due());
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderRuleAdd, &ctx),
            DispatchOutcome::NoTarget
        );
    }

    /// T22 — the disabled bit wins before any target check (§5.8:
    /// reserved actions are honest no-ops even with focus set).
    /// D4/D6 — the banner verbs route exact typed commands from the
    /// reminder target, `NoTarget` without one.
    #[test]
    fn reminder_verbs_route_from_the_target() {
        let mut ctx = ctx(false);
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderSnooze, &ctx),
            DispatchOutcome::NoTarget
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderMute, &ctx),
            DispatchOutcome::NoTarget
        );
        let target = banner_target();
        ctx.reminder_target = Some(target);
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderSnooze, &ctx),
            DispatchOutcome::Command(StateCommand::SnoozeReminder {
                reminder: target.reminder,
                occurrence: target.occurrence,
                postpone: SNOOZE_POSTPONE,
            })
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::ReminderMute, &ctx),
            DispatchOutcome::Command(StateCommand::SetTaskRemindersMuted {
                task: target.task,
                muted: true,
            })
        );
    }

    /// D6 — the staged view intents route to their named effects.
    #[test]
    fn view_intents_route_to_effects() {
        for (id, effect) in [
            (ActionId::NavNextView, UiEffect::NextView),
            (ActionId::NavPrevView, UiEffect::PrevView),
            (ActionId::NavFocusBoard, UiEffect::FocusBoard),
            (ActionId::NavNextItem, UiEffect::FocusNext),
            (ActionId::NavPrevItem, UiEffect::FocusPrev),
            (ActionId::TaskCreate, UiEffect::BeginCreate),
            (ActionId::StackCreate, UiEffect::BeginCreateStack),
            (ActionId::TaskEdit, UiEffect::Activate),
            (ActionId::TaskMove, UiEffect::RequestMove),
        ] {
            assert_eq!(
                ActionRouter::dispatch(id, &ctx(true)),
                DispatchOutcome::UiEffect(effect)
            );
        }
    }

    /// Design §3.8 — the board verbs: next/previous route to their
    /// effects; `board.select` routes the parameterized target and
    /// absorbs as `NoTarget` without one. Never a `Command` — the
    /// dumb-client pin at the routing layer.
    #[test]
    fn board_verbs_route_to_effects_and_the_parameterized_target() {
        assert_eq!(
            ActionRouter::dispatch(ActionId::BoardNext, &ctx(true)),
            DispatchOutcome::UiEffect(UiEffect::BoardNext)
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::BoardPrevious, &ctx(true)),
            DispatchOutcome::UiEffect(UiEffect::BoardPrevious)
        );
        let board = taskboard_domain::BoardId::from_uuid(Uuid::now_v7());
        let mut ctx = ctx(true);
        ctx.board_target = Some(board);
        assert_eq!(
            ActionRouter::dispatch(ActionId::BoardSelect, &ctx),
            DispatchOutcome::UiEffect(UiEffect::BoardSelect(board))
        );
        ctx.board_target = None;
        assert_eq!(
            ActionRouter::dispatch(ActionId::BoardSelect, &ctx),
            DispatchOutcome::NoTarget
        );
    }

    #[test]
    fn disabled_actions_are_absorbed_even_with_focus() {
        let ctx = ctx(true);
        for id in [
            ActionId::AppUndo,
            ActionId::AppRedo,
            ActionId::CaptureQuick,
            ActionId::PaletteSearch,
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
            DispatchOutcome::UiEffect(UiEffect::MenuBack),
            "menu.dismiss is the level-aware back (round-1 D6)"
        );
        assert_eq!(
            ActionRouter::dispatch(ActionId::MenuNext, &ctx),
            DispatchOutcome::UiEffect(UiEffect::MenuNext)
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

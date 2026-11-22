// SPDX-License-Identifier: AGPL-3.0-only

//! The keymap shell state (chunk decision D17): the pure half of the
//! adapter — menu cursor state, the vi mode, the modal mirrors that
//! arm the context stack (`keyscope::scope_stack`, round-5 chunk 7 —
//! `chain` now only delegates to it), and the focused-entity seam. No
//! slint types; the lib adapter is the only code that syncs this onto
//! window properties.

use crate::actions::{ContextId, MenuGroup};
use crate::router::{Mode, UiEffect};

/// Menu cursor state (round-1 D6): the generated entries live in the
/// window model; the shell tracks openness, the active page, and the
/// cursors. `selected`/`len` always describe the ACTIVE page;
/// `root_selected` preserves the root page's cursor across a submenu
/// round-trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuState {
    pub open: bool,
    pub selected: usize,
    /// The entry count of the ACTIVE page (wrapping bound for the
    /// cursor; 0 while closed).
    pub len: usize,
    /// `Some(group)`: a submenu page is active (fly-out or replace);
    /// `None`: the root page. The fly-out's anchor is presentation
    /// geometry (the activated row's position in the panel) and lives
    /// in the markup, not here.
    pub active_group: Option<MenuGroup>,
    /// The task context menu's open family (UX-R8 chunk-5 D-5): the
    /// task families park here beside `active_group` (mutually
    /// exclusive); the Escape ladder consults it first.
    pub task_group: Option<crate::menu::TaskMenuGroup>,
    /// The root page's cursor, preserved across a submenu round-trip.
    pub root_selected: usize,
}

/// The shell state (D17): mode, menu, the active view context
/// (`None` in chunk-2 sessions — no views exist), and the focused
/// entities.
#[derive(Debug, Clone, Copy, Default)]
#[allow(clippy::struct_excessive_bools)] // one flag per synced router gate
pub struct KeymapShell {
    pub mode: Mode,
    pub menu: MenuState,
    /// The search palette is open (#348, chunk 9): the second modal
    /// context — the scope stack flips to `[Palette, ViInsert, Global]` and the
    /// `Palette` mask makes it total. Mirrored from the view-state flow
    /// by the adapter's `sync_view_context`.
    pub palette: bool,
    /// The concentration overlay is open (#496): the third modal
    /// context — the scope stack flips to `[FocusMode, ViInsert, Global]` and
    /// the `FocusMode` mask makes it total. Mirrored from the
    /// view-state flow by the adapter's `sync_view_context` (the
    /// `palette` precedent).
    pub focus_overlay: bool,
    /// The time-entries overlay is open (r4 chunk 10, D-TS2): the
    /// fourth modal context — the scope stack flips to
    /// `[TrackEntries, ViInsert, Global]` and the `TrackEntries` mask
    /// makes it total. Mirrored from the view-state flow by the
    /// adapter's `sync_view_context` (the `palette` precedent).
    pub track_entries: bool,
    /// The review walk overlay is open (DR-7 chunk 3, #638): the
    /// fifth modal context — the scope stack flips to
    /// `[ReviewWalk, ViInsert, Global]` and the `ReviewWalk` mask
    /// makes it total. Mirrored from the view-state flow by the
    /// adapter's `sync_view_context` (the `palette` precedent).
    pub review_walk: bool,
    /// The notification surface is ENGAGED (US-6/W-6, D-N1's recorded
    /// posture): rows visible ∧ armed by the surface's first walk or
    /// verb stroke — the context rides the chain head exactly while
    /// this holds; disarmed by the Escape close, the last row's exit,
    /// or rows emptying. Synced by the adapter.
    pub notification_surface: bool,
    /// The export dialog is open (the J2 export plan W-1): the sixth
    /// modal context — the scope stack flips to
    /// `[ExportDialog, ViInsert, Global]` and the `ExportDialog` mask
    /// makes it total. Adapter-owned VM (not a flow); synced at the
    /// open/close seams.
    pub export_dialog: bool,
    /// The undo branch picker is open (UF2, #353): a desktop-only
    /// selection menu mounted off the undo affordance (R-2 — kiosk
    /// viewers keep the v1 reflex). Pure navigation: the picker
    /// dispatches nothing until a row commits. Synced by the adapter
    /// (the `export_dialog` precedent).
    pub undo_picker: bool,
    pub view: Option<ContextId>,
    /// The [`crate::router::DispatchCtx`] carried to the router.
    pub focused_task: Option<taskboard_domain::TaskId>,
    pub focused_stack: Option<taskboard_domain::StackId>,
    /// Whether the focused task is done (round 2, D19): the reopen
    /// verb's gate, synced by the adapter from the focused task.
    pub focused_task_done: bool,
    /// Whether the focused task is kind-marked (issue #492): the
    /// complete verb's router guard and the toggle's inversion, synced
    /// by the adapter from the focused task.
    pub focused_task_tracker: bool,
    /// Whether the focused task carries a scheduled day (UX-R8 D-3):
    /// `task.unschedule`'s route gate, synced by the adapter from the
    /// focused task.
    pub focused_task_scheduled: bool,
    /// Whether the toggle target holds THE open tracking entry (J3
    /// R-8, round 9 chunk 11): `task.track-toggle`'s state half,
    /// synced at dispatch time by the adapter's `route` (after the
    /// one-shot move — the fact answers about the exact resolved
    /// target).
    pub focused_task_tracking: bool,
    /// Whether the focused task carries an effort estimate (J3 US-4,
    /// round 9 chunk 11): `task.effort-clear`'s route gate, synced by
    /// the adapter from the focused task.
    pub focused_task_effort: bool,
    /// The date-picker grid holds slint focus (round-3 D25): the
    /// `DatePicker` context rides the chain head while it does —
    /// non-masking, event-mirrored from the grid's focus seam.
    pub date_picker_grid: bool,
}

impl KeymapShell {
    /// The off-doorway context stack (round-5 chunk 7): a thin
    /// delegate to [`crate::keyscope::scope_stack`] with no deltas
    /// armed — the boolean reconstruction is retired; the method
    /// survives only for the pinned-untouched `flow_table_tests`
    /// first-frame hint projection, which reads the mount chain.
    #[must_use]
    pub fn chain(&self) -> Vec<ContextId> {
        crate::keyscope::scope_stack(&crate::keyscope::ScopeInputs {
            shell: self,
            picker_open: false,
            editor_focus: false,
            flow: &crate::view_state::Flow::None,
            chain: &[],
            stroke: None,
            at_key_doorway: false,
        })
    }

    /// The router context (D16).
    #[must_use]
    pub fn dispatch_ctx(&self) -> crate::router::DispatchCtx {
        crate::router::DispatchCtx {
            focused_task: self.focused_task,
            focused_stack: self.focused_stack,
            habit_create_draft: None,
            habit_pause_target: None,
            habit_quota_draft: None,
            habit_move_up: None,
            habit_move_down: None,
            plan_target: None,
            create_target: None,
            reminder_target: None,
            board_target: None,
            rule_target: None,
            rule_add_when: None,
            timer_draft: None,
            track_entry_draft: None,
            track_entry_target: None,
            timer_target: None,
            break_rule_draft: None,
            break_rule_target: None,
            alarm_row: None,
            track_target: None,
            habit_target: None,
            habit_undo: None,
            habit_move: None,
            habit_repair: None,
            habit_convert_target: None,
            habit_remove_target: None,
            pattern_draft: None,
            series_target: None,
            pause_draft: None,
            convert_target: None,
            recur_edit_target: None,
            recur_draft_rule: None,
            day_window: None,
            today_last_instant: None,
            tomorrow_window_end: None,
            focused_task_done: self.focused_task_done,
            focused_task_tracker: self.focused_task_tracker,
            focused_task_scheduled: self.focused_task_scheduled,
            focused_task_tracking: self.focused_task_tracking,
            focused_task_effort: self.focused_task_effort,
        }
    }

    /// Applies a UI effect to the state (D17). `Exit` is *not*
    /// handled here — quitting the slint event loop is the adapter's
    /// job; it reports `false` for that case. Menu-opening needs the
    /// precomputed entry count (the entries themselves are a menu
    /// projection, D19).
    #[allow(clippy::too_many_lines)] // the effect match is one arm per effect
    pub fn apply(&mut self, effect: &UiEffect, menu_len: usize) -> bool {
        match *effect {
            UiEffect::Exit => return false,
            UiEffect::OpenMenu => {
                // The picker and the menu are sibling modals (UF2):
                // opening either closes the other.
                self.undo_picker = false;
                self.menu = MenuState {
                    open: true,
                    selected: 0,
                    len: menu_len,
                    active_group: None,
                    task_group: None,
                    root_selected: 0,
                };
            }
            UiEffect::CloseMenu => {
                self.menu = MenuState::default();
                // The picker's dismissal shares the close (AC-4: an
                // outside click / Escape closes with nothing
                // dispatched).
                self.undo_picker = false;
            }
            UiEffect::MenuBack => {
                // Level-aware (round-1 D6): a submenu page steps back
                // to the root with the cursor restored on the parent
                // row; at the root it is the full close.
                // The Escape ladder (UX-R8 chunk-5 D-5): the task
                // family's page steps down first, then the main
                // group's, then the close.
                if self.menu.task_group.is_some() {
                    self.menu.task_group = None;
                    self.menu.selected = self.menu.root_selected;
                    self.menu.len = menu_len;
                } else if self.menu.active_group.is_some() {
                    self.menu.active_group = None;
                    self.menu.selected = self.menu.root_selected;
                    self.menu.len = menu_len;
                } else if self.undo_picker {
                    // The picker's Escape ladder (UF2 AC-4): the first
                    // Escape closes the picker with nothing dispatched.
                    self.undo_picker = false;
                } else {
                    self.menu = MenuState::default();
                }
            }
            UiEffect::MenuNext => {
                if self.menu.len > 0 {
                    self.menu.selected = (self.menu.selected + 1) % self.menu.len;
                }
            }
            UiEffect::MenuPrevious => {
                if self.menu.len > 0 {
                    self.menu.selected = (self.menu.selected + self.menu.len - 1) % self.menu.len;
                }
            }
            UiEffect::UndoBranchesOpen => {
                // The picker mount (UF2 W-1): pure navigation — no
                // command dispatch on open. Mounting closes the menu
                // (sibling modals).
                self.menu = MenuState::default();
                self.undo_picker = true;
            }
            UiEffect::InvokeSelected => {
                // The adapter performs the close-then-redispatch (D16);
                // the shell only records the close.
                self.menu = MenuState::default();
            }
            UiEffect::SetMode(mode) => self.mode = mode,
            // The chunk-3 view intents (D6/D9) live in `view_state`;
            // the shell records nothing for them. The round-1 stack
            // width verbs (D18) and the palette opener (#348: the bit
            // mirrors the view-state flow in `sync_view_context`)
            // likewise live elsewhere.
            UiEffect::OpenPalette
            | UiEffect::FocusNext
            | UiEffect::FocusPrev
            // The directional-focus family (J3 US-1, round 9 chunk 9)
            // and its extend twin (#875) move no shell state — the
            // per-view readings live in `view_state`/the adapter.
            | UiEffect::FocusDirectional(_)
            | UiEffect::ExtendSelectionNext
            | UiEffect::ExtendSelectionPrev
            | UiEffect::NextView
            | UiEffect::PrevView
            | UiEffect::FocusBoard
            | UiEffect::ShowTask
            | UiEffect::ControlNext
            | UiEffect::ControlPrev
            | UiEffect::ScrollUp
            | UiEffect::ScrollDown
            | UiEffect::PickerKey(_)
            | UiEffect::BeginCreate { .. }
            | UiEffect::BeginCreateStack
            | UiEffect::Activate
            | UiEffect::RequestMove
            | UiEffect::BoardNext
            | UiEffect::BoardPrevious
            | UiEffect::BoardSelect(_)
            | UiEffect::StackWider
            | UiEffect::StackNarrower
            | UiEffect::OpenSettings
            // The status panel's toggle (OP3) moves no shell state —
            // the view intent lives in `view_state`.
            | UiEffect::OpenStatusPanel
            // The plan-next-day family (#622) moves no shell state —
            // the view/flow intents live in `view_state`.
            | UiEffect::OpenPlanNextDay
            | UiEffect::PlanBeginPlace(_)
            | UiEffect::PlanCommitPlace
            | UiEffect::PlanBoxEarlier(_)
            | UiEffect::PlanBoxLater(_)
            | UiEffect::PlanRemoveBox(_)
            // Round 2 (D16–D21): the round-up verbs live in
            // `view_state` too; the shell records nothing for them.
            | UiEffect::CancelMove
            | UiEffect::StackNext
            | UiEffect::StackPrev
            | UiEffect::StackDelete
            | UiEffect::StackRename
            | UiEffect::StackMove
            | UiEffect::BoardCreate
            | UiEffect::BoardDelete
            | UiEffect::BoardRename
            | UiEffect::BoardColorEdit
            | UiEffect::BoardDefaultPreset
            | UiEffect::OpenBoardBarFlyout
            | UiEffect::FocusFirst
            | UiEffect::FocusLast
            // Chunk 10: the today/inbox jumps and capture live in
            // `view_state`; the shell records nothing for them.
            | UiEffect::FocusToday
            | UiEffect::OpenInbox
            // Round 3 (W-SM-d): the title arm lives in `view_state`;
            // the shell records nothing for it. The keymap-parity D-4
            // editor arms are the same shape (funnel-level, adapter
            // side).
            | UiEffect::BeginTitleEdit
            | UiEffect::BeginDateEdit { .. }
            | UiEffect::BeginDescriptionEdit
            // The effort field's focus opener (J3 US-4, chunk 11):
            // adapter-side like the other editor arms; the shell
            // records nothing.
            | UiEffect::BeginEffortEdit
            | UiEffect::BeginCapture
            // The overlay toggle moves no shell state (the flow is the
            // truth; `sync_view_context` mirrors it).
            | UiEffect::ToggleFocusMode
            // The entries overlay's intents are view-state resolves
            // (the adapter's table); the shell records nothing.
            | UiEffect::TrackEntriesOpen
            // The habit trio's intents are adapter-interpreted view
            // intents (issue #506); the shell records nothing.
            | UiEffect::TrackEntryDismiss
            // The row-menu opener is adapter-interpreted (US-7, W-8,
            // #1032); the shell records nothing (the adapter's menu
            // state write owns the open). The disarm chord (US-10)
            // is a view-state flip the shell never records.
            | UiEffect::RowMenuRequested
            | UiEffect::RowCursorDisarm
            | UiEffect::SeriesPauseOpen
            | UiEffect::HabitPauseOpen
            | UiEffect::HabitConvertArm
            | UiEffect::HabitConvertCancel
            | UiEffect::HabitRemoveArm
            | UiEffect::HabitRemoveCancel
            | UiEffect::HabitEdit
            | UiEffect::HabitHistoryOpen
            | UiEffect::HabitCreateToggle
            | UiEffect::HabitQuotaEdit
            | UiEffect::SeriesConvertArm
            // The recur editor's intents are adapter-interpreted view
            // intents (round-7 D-7.1); the shell records nothing.
            | UiEffect::SeriesEditorOpen
            | UiEffect::SeriesDetachArm
            | UiEffect::SeriesDetachCancel
            // The priority pickers' openers are adapter-interpreted
            // view intents (UX R-3 D-1, #788); the shell records
            // nothing (the flag is the adapter's).
            | UiEffect::PriorityPickerOpen(_)
            // The labels overlay's intents are adapter-interpreted
            // view intents (UX R-3 D-2, #789); the shell records
            // nothing (the flow is the truth).
            | UiEffect::LabelsOpen
            | UiEffect::LabelsDismiss
            // The review walk's intents are adapter-interpreted view
            // intents (DR-7 chunk 3, #638); the shell records nothing
            // (the flow is the truth; `sync_view_context` mirrors it).
            | UiEffect::OpenReviewWalk
            | UiEffect::ReviewWalk(_)
            // The notification verbs live adapter-side (the routed
            // seam bodies; the shell state is untouched).
            // The board ceremony verbs (OP1) join them: pure
            // view-intent strips — the shell reads nothing from them
            // (the lifecycle family's shell-invisible row).
            | UiEffect::BoardArchive
            | UiEffect::BoardReset
            | UiEffect::BoardReconcile
            | UiEffect::NotificationVerb(_)
            // The export dialog's mount bit is the adapter's VM (the
            // J2 export plan W-1) — the shell records nothing.
            | UiEffect::ExportDialogOpen
            | UiEffect::ExportDialogClose => {}
        }
        true
    }

    /// The currently selected menu entry, if any (the adapter's
    /// `InvokeSelected` re-dispatch source).
    #[must_use]
    pub fn selected_entry(&self) -> Option<usize> {
        self.menu
            .open
            .then_some(self.menu.selected)
            .filter(|_| self.menu.len > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T23 — menu effects: open resets the cursor, next/previous wrap,
    /// close clears.
    #[test]
    fn menu_effects_drive_the_cursor() {
        let mut shell = KeymapShell::default();
        assert!(shell.apply(&UiEffect::OpenMenu, 3));
        assert_eq!(
            shell.menu,
            MenuState {
                open: true,
                selected: 0,
                len: 3,
                active_group: None,
                task_group: None,
                root_selected: 0
            }
        );
        assert_eq!(shell.selected_entry(), Some(0));

        shell.apply(&UiEffect::MenuNext, 3);
        shell.apply(&UiEffect::MenuNext, 3);
        assert_eq!(shell.selected_entry(), Some(2));
        shell.apply(&UiEffect::MenuNext, 3);
        assert_eq!(shell.selected_entry(), Some(0), "next wraps");
        shell.apply(&UiEffect::MenuPrevious, 3);
        assert_eq!(shell.selected_entry(), Some(2), "previous wraps");

        shell.apply(&UiEffect::CloseMenu, 0);
        assert_eq!(shell.menu, MenuState::default());
        assert_eq!(shell.selected_entry(), None);
    }

    /// T23 — mode flips are ordinary state (D10); exit is the
    /// adapter's job and reports `false`.
    #[test]
    fn mode_flips_are_plain_state_and_exit_is_reported() {
        let mut shell = KeymapShell::default();
        assert!(shell.apply(&UiEffect::SetMode(Mode::Insert), 0));
        assert_eq!(shell.mode, Mode::Insert);
        assert!(!shell.apply(&UiEffect::Exit, 0), "exit is not shell state");
    }

    /// T23 — a zero-length menu ignores cursor movements (nothing to
    /// select; division by zero guarded).
    #[test]
    fn cursor_movement_on_an_empty_menu_is_inert() {
        let mut shell = KeymapShell::default();
        shell.apply(&UiEffect::OpenMenu, 0);
        shell.apply(&UiEffect::MenuNext, 0);
        shell.apply(&UiEffect::MenuPrevious, 0);
        assert_eq!(shell.selected_entry(), None);
    }

    /// T-9 — the level-aware menu back (round-1 D6): at a submenu it
    /// returns to the root with the root cursor restored; at the root
    /// it fully closes; a full close clears every level from anywhere.
    #[test]
    fn menu_back_steps_levels_and_close_clears() {
        use crate::actions::MenuGroup;
        let mut shell = KeymapShell::default();
        shell.apply(&UiEffect::OpenMenu, 10);
        // Step into a submenu: root cursor parked, submenu cursor at 0.
        shell.menu.root_selected = 4;
        shell.menu.selected = 0;
        shell.menu.active_group = Some(MenuGroup::Task);
        shell.menu.len = 7;
        assert!(shell.apply(&UiEffect::MenuBack, 10));
        assert_eq!(
            shell.menu,
            MenuState {
                open: true,
                selected: 4,
                len: 10,
                active_group: None,
                task_group: None,
                root_selected: 4
            },
            "back restores the root page and its cursor"
        );
        assert!(shell.apply(&UiEffect::MenuBack, 0));
        assert_eq!(shell.menu, MenuState::default(), "back at the root closes");

        // A full close clears any level.
        shell.apply(&UiEffect::OpenMenu, 5);
        shell.menu.active_group = Some(MenuGroup::Mode);
        shell.apply(&UiEffect::CloseMenu, 0);
        assert_eq!(shell.menu, MenuState::default());
    }

    /// Design §3.8 — the board effects are view intents: the shell
    /// records nothing for them (they apply `true`, plain state
    /// unchanged; resolution happens in the view-state machine).
    #[test]
    fn board_effects_record_nothing_in_the_shell() {
        let mut shell = KeymapShell::default();
        let before = shell;
        for effect in [
            UiEffect::BoardNext,
            UiEffect::BoardPrevious,
            UiEffect::BoardSelect(taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7())),
        ] {
            assert!(shell.apply(&effect, 3), "a board effect is plain state");
        }
        assert_eq!(shell.menu, before.menu);
        assert_eq!(shell.mode, before.mode);
    }
}

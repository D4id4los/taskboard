// SPDX-License-Identifier: AGPL-3.0-only

use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, SharedString};

use taskboard_domain::{AppState, TaskId};

use super::Adapter;
use super::{CreateFocusHandoff, FlowKind, FocusHandover, Notice, Resolve};
use crate::AppWindow;
use crate::actions::ActionId;
use crate::router::{Direction, EffectClass, Mode, UiEffect};
use crate::view_state::{Flow, FlowOutcome, FocusEdge};

impl Adapter {
    /// The D9 interpretation table: one `match (flow, effect)` whose
    /// arms route to `view_state` methods named by what they do.
    #[allow(clippy::too_many_lines)] // one decision table, kept flat for review
    #[allow(clippy::match_same_arms)] // one arm per matrix decision
    pub(in crate::adapter) fn interpret(&mut self, window: &AppWindow, effect: UiEffect) {
        let compact = Self::is_compact(window);
        let state = self.state.load();
        let mut flow = self.responding_flow();
        let focus_walk = matches!(
            effect,
            UiEffect::FocusNext
                | UiEffect::FocusPrev
                // The directional steps ride the walk's hint/hand-off
                // halves (J3 US-1): up/down land on rows like
                // FocusNext does.
                | UiEffect::FocusDirectional(_)
                | UiEffect::ExtendSelectionNext
                | UiEffect::ExtendSelectionPrev
                | UiEffect::FocusFirst
                | UiEffect::FocusLast
        );
        // The show-task retarget arms the hint with the same
        // before/after discipline (round-3 D23 §3.3): `before` reads
        // the pre-effect focus so the jump always re-arms.
        let show_task = effect == UiEffect::ShowTask;
        let focus_before = self.view_state.walk_focus();
        let leaving = matches!(
            effect,
            UiEffect::NextView
                | UiEffect::PrevView
                | UiEffect::FocusBoard
                | UiEffect::FocusToday
                | UiEffect::BoardNext
                | UiEffect::BoardPrevious
                | UiEffect::BoardSelect(_)
        );

        // Leaving the edit field by effect first commits the buffer;
        // a move flow ends without dispatch (D9's recorded rule).
        let mut drag_teardown = false;
        if leaving {
            self.settle_flows_on_leaving(window, &state);
            // The settle ended the flow; the table reads the
            // post-settle machine (a stale read would resolve the
            // re-run against a flow that is already gone).
            flow = self.responding_flow();
        }

        let outcomes = match (&flow, effect) {
            // The priority pickers' seams (UX R-3 D-1, #788): Escape
            // (CancelMove's routing) closes the open picker first —
            // nothing dispatches (the cancel is the whole effect);
            // outside click rides the view's own dismissal seam.
            (_, UiEffect::CancelMove) if self.priority_open.is_some() => {
                self.priority_open = None;
                self.refresh_forced(window);
                Vec::new()
            }
            // The openers: flow-free flag flips over the viewed
            // subject — one picker at a time (opening one settles the
            // other), the kiosk/no-subject dispatches refuse (the
            // entries-overlay gate's shape), and a held foreign flow
            // keeps them inert at this arm (the resolver's Nav class
            // settles first and re-runs against Flow::None).
            (Flow::None, UiEffect::PriorityPickerOpen(field)) => {
                if self.kiosk || self.view_state.subject_task().is_none() {
                    Vec::new()
                } else {
                    // The picker overlay is a foreign settle (D-2
                    // site 4, the opener sweep): the rule cursor never
                    // survives under a mask.
                    self.view_state.clear_rule_cursor();
                    self.priority_open = Some(field);
                    self.refresh_forced(window);
                    Vec::new()
                }
            }
            // -- the pointer drag (r2 chunk-8): every cancel-shaped
            //    effect ends it without dispatching (Escape / C-g via
            //    `CancelMove`, the toggle re-request, mode.normal);
            //    Activate confirms the hovered target (the same
            //    "Activate confirms a move flow" semantic); the
            //    pointer owns target cycling, so the keyboard ring
            //    walk is absorbed mid-drag --
            (
                Flow::DragMove { .. },
                effect @ (UiEffect::Activate
                | UiEffect::CancelMove
                | UiEffect::RequestMove
                | UiEffect::SetMode(Mode::Normal)),
            ) => {
                let outcomes = if effect == UiEffect::Activate {
                    self.view_state.confirm_drag_move(&state)
                } else {
                    self.view_state.cancel_move()
                };
                drag_teardown = true;
                outcomes
            }
            (
                Flow::DragMove { .. },
                UiEffect::FocusNext | UiEffect::FocusPrev | UiEffect::FocusDirectional(_),
            ) => Vec::new(),
            // The selection-extend twin (#875) has no kept arms under
            // a live flow: the move/drag flows own the selection
            // semantics, and the Nav settle would cancel the flow
            // (the exact settle-matrix bug class US-2 fixes) — the
            // named inert.
            (
                Flow::Move { .. } | Flow::MoveStack { .. } | Flow::DragMove { .. },
                UiEffect::ExtendSelectionNext | UiEffect::ExtendSelectionPrev,
            ) => Vec::new(),
            // -- the move flows (D8/D9; round 2 adds the stack move;
            //    round 9 chunk 9 adds the directional kept arms — the
            //    same keys move the drop target mid-move, US-2;
            //    #1033 joins the legacy stack-walk verbs — the chords
            //    behind `StackNext`/`StackPrev` must hop the drop
            //    column mid-move, never settle the flow) --
            (
                flow @ (Flow::Move { .. } | Flow::MoveStack { .. }),
                effect @ (UiEffect::FocusNext
                | UiEffect::FocusPrev
                | UiEffect::FocusDirectional(_)
                | UiEffect::StackNext
                | UiEffect::StackPrev
                | UiEffect::Activate
                | UiEffect::RequestMove
                | UiEffect::CancelMove
                | UiEffect::SetMode(Mode::Normal)),
            ) => self.interpret_move(flow, effect, &state, window),
            // -- the edit flows (D5/D9): SetMode(Normal) discards,
            //    Activate commits, leaving settles first (above); the
            //    round-2 lifecycle flows join them --
            (
                Flow::Create { .. }
                | Flow::CreateStack
                | Flow::CreateBoard
                | Flow::EditTitle { .. }
                | Flow::RenameStack { .. }
                | Flow::RenameBoard { .. },
                effect @ (UiEffect::SetMode(Mode::Normal) | UiEffect::Activate),
            ) => self.interpret_edit(effect, &state),
            // The due editor (r2 D25): Escape discards (the clean
            // close — no commit, buffer/echo kept); the commit itself
            // rides `commit_date`, so Activate is a named inert here
            // (the field's `accepted` seam owns Return) — the r2
            // swallow-all arm is gone (round-3 F18): every other
            // effect settles the flow and re-runs via the resolver.
            (Flow::EditDate { .. }, UiEffect::SetMode(Mode::Normal)) => {
                // The Escape ladder (r4 D25): the phase the grid-focus
                // mirror reads decides — the picker phase's Escape is
                // the clean close (the view Escape still ends the
                // editor, R-c); the field phase's Escape is the phase
                // step back: the grid takes focus, the half-typed
                // buffer survives (the steal latch suppresses the
                // settle funnel).
                // No ladder without a picker: a dismissed overlay has
                // no grid to hand focus to — the only non-trapping
                // exit is the clean close (the machine cannot step
                // into a phase that no longer exists).
                let picker_open = self
                    .view_state
                    .date_editor()
                    .is_some_and(|editor| editor.picker.is_some());
                if !picker_open || self.shell.date_picker_grid {
                    self.view_state.discard_edit()
                } else {
                    if let Some(editor) = self.view_state.date_editor_mut() {
                        editor.focus_steal = true;
                    }
                    self.focus_handover = FocusHandover::Grid;
                    // The step-back holds insert mode — the grid
                    // phase's own shape (chunk 6's engage semantics):
                    // the editor is still armed, and the next Escape
                    // resolves as the grid phase's clean close.
                    self.shell.mode = Mode::Insert;
                    // The phase step pushes its one-shot flag itself
                    // (no outcomes ride it — the flow stays armed).
                    self.refresh_forced(window);
                    Vec::new()
                }
            }
            (Flow::EditDate { .. }, UiEffect::Activate | UiEffect::SetMode(Mode::Insert)) => {
                Vec::new()
            }
            // The description editor (round-3 D15): the field's
            // accepted seam owns Return (the checkmark composes);
            // Escape's mode.normal discards — the settle rule is the
            // text flows' (commit non-empty / discard empty). Insert
            // is the named inert, shared with the move flows.
            (
                Flow::EditDescription { .. },
                UiEffect::SetMode(Mode::Normal) | UiEffect::Activate,
            ) => self.interpret_description(effect, &state),
            // -- quick capture (chunk 10 D8): the GLOBAL action, from
            //    any flow — the XOR rule replaces without committing.
            //    The capture overlay is a modal takeover (r2 D13's own
            //    rule sentence): the tip dies into it even though the
            //    view identity may hold (the anchor stays mounted
            //    under the overlay).
            (_, UiEffect::BeginCapture) => {
                Self::dismiss_tooltip(window);
                // T-31 (#814): a live flow's replacement announces
                // itself (the kept arm's own replace, the resolver's
                // twin).
                if !matches!(self.view_state.flow(), Flow::None) {
                    self.arm_overlay_replacement_notice(window);
                }
                self.view_state.begin_capture(&state)
            }
            // -- the concentration overlay's toggle (#496): the
            //    matrix settles any foreign flow first (R3
            //    settle-then-run), then the overlay opens over it
            //    (the XOR rule); a second toggle closes. The kiosk
            //    dispatch no-ops (the exclusion's effect layer —
            //    layer 3: structurally unreachable even if a future
            //    surface bypasses the entry_states gate, layer 2).
            (_, UiEffect::ToggleFocusMode) if self.kiosk => Vec::new(),
            (Flow::FocusMode { .. }, UiEffect::ToggleFocusMode) => {
                Self::dismiss_tooltip(window);
                self.view_state.close_focus_mode()
            }
            (_, UiEffect::ToggleFocusMode) => {
                Self::dismiss_tooltip(window);
                self.view_state.open_focus_mode()
            }
            // -- the time-entries overlay (r4 chunk 10, D-TS2) --
            // The open request settles any foreign flow first (the
            // matrix's overlay cells), then opens over the viewed
            // task; the kiosk dispatch and a subjectless view refuse
            // (dispatch-time gating, D-TS8 — the menu row degrades
            // through this same arm). The modal-takeover half of the
            // r2 D13 rule: the tip dies into the overlay.
            (_, UiEffect::TrackEntriesOpen | UiEffect::UndoBranchesOpen) if self.kiosk => {
                Vec::new()
            }
            (
                Flow::TrackEntries { .. } | Flow::TrackEntryForm { .. },
                UiEffect::TrackEntriesOpen,
            ) => Vec::new(),
            // -- the labels overlay (UX R-3 D-2, #789): the open
            //    request settles any foreign flow first (the entries
            //    overlay's matrix cells), then opens over the viewed
            //    task; kiosk and a subjectless view refuse; the
            //    already-open case is inert (the modal discipline).
            (_, UiEffect::LabelsOpen) if self.kiosk => Vec::new(),
            (Flow::Labels { .. }, UiEffect::LabelsOpen) => Vec::new(),
            (_, UiEffect::LabelsOpen) => {
                // The pickers are flow-free siblings: opening the
                // overlay settles an open quick-list (the
                // exclusivity's shape). The subject resolution is
                // adapter-side (tag-color plan D-4): the detail view's
                // task takes the assignment-toggles mount; otherwise
                // the selected board takes the roster-only mount
                // (inert without one — the no-subject refusal lives
                // in the arm, the arm doc's own precedent).
                if let Some(task) = self.view_state.subject_task() {
                    Self::dismiss_tooltip(window);
                    self.priority_open = None;
                    let outcomes = self
                        .view_state
                        .open_labels(crate::view_state::LabelOverlaySubject::Task(task));
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                } else if !self.view_state.labels_board_mount_open() {
                    let outcomes = self.view_state.open_labels_board(&self.state.load());
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                }
                Vec::new()
            }
            (_, UiEffect::TrackEntriesOpen) => match self.view_state.subject_task() {
                Some(task) => {
                    Self::dismiss_tooltip(window);
                    self.view_state.open_track_entries(task)
                }
                None => Vec::new(),
            },
            // The recur row's intents (issue #506, plan D-8): the
            // pause open flips the adapter-owned flag over the
            // subject; the arm effects set their one-shot targets.
            // The kiosk dispatch is structurally inert (layer 3, the
            // entries-overlay precedent).
            (_, UiEffect::SeriesPauseOpen) if self.kiosk => Vec::new(),
            (_, UiEffect::SeriesPauseOpen) => {
                if let Some(task) = self.view_state.subject_task() {
                    // The recur-pause overlay is a foreign settle
                    // (D-2 site 4, the opener sweep).
                    self.view_state.clear_rule_cursor();
                    self.series_target = Some(task);
                    self.recur_pause_open = true;
                    self.recur_confirm_armed = false;
                    // The open echo rides adapter-owned flags outside
                    // the state Arc — the unchanged-refresh guard
                    // would eat the pass (the armed-create-form
                    // precedent, the HabitConvertArm twin; the US-11
                    // chord shares the arm, so click ≡ key holds).
                    self.refresh_forced(window);
                }
                Vec::new()
            }
            // The row-menu opener (US-7, W-8, #1032): the walk
            // subject's context menu through the pointer seams' own
            // openers — click ≡ key by construction; the keyboard
            // anchor is the toolbar fallback (no pointer point). The
            // subjectless walk is the typed refusal (the uniform law).
            (_, UiEffect::RowMenuRequested) => {
                self.open_row_menu_keyboard(window);
                Vec::new()
            }
            // The rule cursor's disarm chord (US-10, W-11, #1091):
            // armed — clear AND latch (the verticals return to the
            // control ladder; the flow table's `armed --> disarmed`
            // edge); disarmed — the typed inert (no state change).
            // The other clear sites stay the only other clearers (the
            // composition law: no second disarm site invented).
            // The roster remove's arm step (US-9, W-10, #1066's
            // roster half): the armed toggle over the roster cursor —
            // the same toggle the pointer seam has always run (one
            // armed subject, the repeat is not the confirm), now
            // cursor-addressed so click ≡ key holds. Cursorless (and
            // kiosk — the section's absence) is the typed refusal.
            (_, UiEffect::BreakRuleRemoveArm) if self.kiosk => Vec::new(),
            (_, UiEffect::BreakRuleRemoveArm) => {
                let live = self
                    .view_state
                    .break_rule_cursor()
                    .filter(|rule| self.tables.break_rules.contains(rule));
                if let Some(rule) = live {
                    self.break_rule_armed = if self.break_rule_armed == Some(rule) {
                        None
                    } else {
                        Some(rule)
                    };
                    self.refresh_forced(window);
                } else {
                    self.arm_no_target_notice(ActionId::BreakRuleRemove);
                    self.push_notice(window);
                }
                Vec::new()
            }
            (_, UiEffect::RowCursorDisarm) => {
                if self.view_state.rule_cursor().is_some() {
                    self.view_state.disarm_rule_cursor();
                    self.refresh_forced(window);
                } else {
                    self.arm_no_target_notice(ActionId::RowCursorDisarm);
                    self.push_notice(window);
                }
                Vec::new()
            }
            // The menu-routed conversion arms (cross-surface friction
            // plan D-2/D-3, #832): they route the real arming seams,
            // subject-resolved — the habit twin through the section's
            // focus fact, the series twin through the viewed subject.
            // A subjectless dispatch surfaces the typed refusal on the
            // status-notice lane (the T-19 twin; the verbs' global
            // silent `NoTarget` arm is the uniform law — chunk 8 D6,
            // #935's one-place pass).
            // The kiosk dispatch is structurally inert (the
            // entries-overlay precedent: the arming flags are desktop
            // strips a menu-routed arm must never flip invisibly).
            (_, UiEffect::HabitConvertArm | UiEffect::SeriesConvertArm) if self.kiosk => Vec::new(),
            (_, UiEffect::HabitConvertArm) => {
                // The subject resolves through the effective walk /
                // click union (chunk 8 D4): the keyboard walk's habit
                // landing arms the conversion exactly as the pointer
                // row click does — click ≡ key.
                let armed = match self.effective_habit() {
                    Some(habit) => self.arm_habit_convert(window, habit),
                    None => false,
                };
                if !armed {
                    self.arm_no_target_notice(ActionId::HabitConvertArm);
                }
                self.push_notice(window);
                // The armed-confirm echo rides adapter-owned flags
                // outside the state Arc — the unchanged-refresh guard
                // would eat the pass (the armed-create-form
                // precedent); the re-projection inside the arm
                // rebuilt the open menu's page (O-7 Q3).
                self.refresh_forced(window);
                Vec::new()
            }
            // The armed conversion's cancel step (H2 D4, O-7 Q3):
            // disarms INSIDE the open menu — the re-projected page
            // greys the confirm again; nothing was dispatched.
            (_, UiEffect::HabitConvertCancel) => {
                self.habit_convert_armed = None;
                self.habit_convert_target = None;
                self.pattern_draft = None;
                self.reproject_habit_menu(window);
                self.refresh_forced(window);
                Vec::new()
            }
            // The staged remove's arm (H4 D4, #944): staging touches
            // no engine state — the armed subject rides the adapter
            // flag and the open menu's page re-projects (the confirm
            // flips enabled). Kiosk dispatch is structurally inert
            // (the convert arm's exact posture).
            (_, UiEffect::HabitRemoveArm | UiEffect::HabitRemoveCancel) if self.kiosk => Vec::new(),
            (_, UiEffect::HabitRemoveArm) => {
                let armed = match self.focused_habit {
                    Some(habit) => {
                        let live = self
                            .state
                            .load()
                            .habits
                            .habits
                            .get(&habit)
                            .is_some_and(taskboard_domain::habit::Habit::is_live);
                        if live {
                            self.habit_remove_armed = Some(habit);
                            self.reproject_habit_menu(window);
                        }
                        live
                    }
                    None => false,
                };
                if !armed {
                    self.arm_no_target_notice(ActionId::HabitRemove);
                }
                self.push_notice(window);
                // The armed-confirm echo rides adapter-owned flags —
                // the unchanged-refresh guard would eat the pass
                // (the convert arm's precedent).
                self.refresh_forced(window);
                Vec::new()
            }
            // The armed remove's cancel step: disarms INSIDE the open
            // menu — the re-projected confirm greys again; nothing
            // was dispatched.
            (_, UiEffect::HabitRemoveCancel) => {
                self.habit_remove_armed = None;
                self.reproject_habit_menu(window);
                self.refresh_forced(window);
                Vec::new()
            }
            (_, UiEffect::SeriesConvertArm) => {
                let armed = match self.view_state.subject_task() {
                    Some(task) => self.arm_series_convert(task),
                    None => false,
                };
                if !armed {
                    self.arm_no_target_notice(ActionId::SeriesConvertArm);
                }
                self.push_notice(window);
                self.refresh_forced(window);
                Vec::new()
            }
            // The create form's toggle (round-7 D-7.2): kiosk stays
            // inert (the action gate, D-T16 — the affordance never
            // mounts there), and opening requires an active board —
            // the section's board IS the create board, so without one
            // the dispatch degrades to silence (the stale-arm law).
            // Armed again = discard-and-close (the ViewPlanNextDay
            // toggle posture).
            // The pause editor's opener (HF1, W-3): resolves the
            // effective habit subject (the walk/click union — click ≡
            // key) and flips the adapter-owned flag; kiosk is
            // structurally inert (the R-3 gate: the actions render
            // disabled, the editor never mounts).
            (_, UiEffect::HabitPauseOpen) if self.kiosk => Vec::new(),
            (_, UiEffect::HabitPauseOpen) => {
                if let Some(habit) = self.effective_habit() {
                    self.habit_pause_target = Some(habit);
                    self.habit_pause_open = true;
                    self.refresh_forced(window);
                } else {
                    self.arm_no_target_notice(ActionId::HabitPause);
                    self.push_notice(window);
                }
                Vec::new()
            }
            (_, UiEffect::HabitCreateToggle) if self.kiosk => Vec::new(),
            (_, UiEffect::HabitCreateToggle) => {
                // Round-6 A1 D-4 (2026-11-24, #1301): the today view is
                // board-free — the gate is "any live board exists"; the
                // commit mints into the first live board.
                if !taskboard_domain::order::live_boards(&state).is_empty() {
                    self.habit_create_open = !self.habit_create_open;
                    if !self.habit_create_open {
                        self.habit_create_draft = None;
                        self.habit_create_quota.clear();
                    }
                    // The armed form's mount is a view-state change
                    // outside the state Arc — forced (the refresh
                    // invariant's classification).
                    self.refresh_forced(window);
                }
                Vec::new()
            }
            // -- the recurrence-rule editor (round-7 D-7.1) --
            // Kiosk and a subjectless/closed view refuse (the
            // entries-overlay precedent); already open is inert (the
            // modal discipline). The opener arms the one-shot target,
            // opens the flow, and seeds the level's draft from the
            // task's rule or the anchor (the seeding law, D-D1) —
            // insert mode, the form's fields.
            (_, UiEffect::SeriesEditorOpen) if self.kiosk => Vec::new(),
            (Flow::EditRecur { .. }, UiEffect::SeriesEditorOpen) => Vec::new(),
            (_, UiEffect::SeriesEditorOpen) => match self.view_state.begin_edit_recur(&state) {
                Some(task) => {
                    Self::dismiss_tooltip(window);
                    self.recur_edit_target = Some(task);
                    let seeded = self.seed_recur_editor(&state, task);
                    if let Some(editor) = self.view_state.recur_editor_mut() {
                        *editor = seeded;
                    }
                    vec![FlowOutcome::SetMode(Mode::Insert), FlowOutcome::RefreshNow]
                }
                None => Vec::new(),
            },
            // The stop-repeating strip's flag flips: the
            // conversion-strip pattern over the editor level; inert
            // without the flow (a stale dispatch changes nothing).
            (_, UiEffect::SeriesDetachArm) => {
                if let Some(editor) = self.view_state.recur_editor_mut() {
                    editor.detach_armed = true;
                }
                Vec::new()
            }
            (_, UiEffect::SeriesDetachCancel) => {
                if let Some(editor) = self.view_state.recur_editor_mut() {
                    editor.detach_armed = false;
                }
                Vec::new()
            }
            // The Escape ladder (D-TS2), level-aware like `MenuBack`:
            // a focused form field disengages first (mode flip; the
            // overlay stays), then the form steps to the list, then
            // the overlay closes.
            (_, UiEffect::TrackEntryDismiss) if self.shell.mode == Mode::Insert => {
                vec![FlowOutcome::SetMode(Mode::Normal)]
            }
            // The ladder's second rung is the generic Cancelled rung
            // (D-H4): the form child ends alone, the list stays;
            // without the child the list's own close rung applies
            // (`SetMode(Normal)` rides it so a disengaged field never
            // re-masks — the `close_palette` shape).
            (_, UiEffect::TrackEntryDismiss) => match self.view_state.cancel_top_child(&state) {
                Some(outcomes) => outcomes,
                None => self.view_state.close_track_entries(),
            },
            // The labels ladder (D-2): a focused field's Insert
            // disengages first (the mode flip; the overlay stays),
            // then the in-place surfaces step out — an open form
            // discards, an armed row disarms — and the last rung
            // closes the overlay.
            (_, UiEffect::LabelsDismiss) if self.shell.mode == Mode::Insert => {
                vec![FlowOutcome::SetMode(Mode::Normal)]
            }
            (_, UiEffect::LabelsDismiss)
                if self.label_form.take().is_some()
                    || self.label_delete_armed.take().is_some()
                    || self.label_restore_armed.take().is_some()
                    || self.label_restore_all_armed =>
            {
                // The bulk flag joins the disarm rung (#1245): the
                // armed strip is a staged state, the Escape ladder's
                // "an armed row disarms" rung owns it — the overlay
                // closes only on the last rung.
                self.label_restore_all_armed = false;
                self.refresh_forced(window);
                Vec::new()
            }
            (_, UiEffect::LabelsDismiss) => self.view_state.close_labels(),
            // -- the capture commit/close (the palette's modal peer);
            //    NOT `interpret_edit` — capture's non-empty commit
            //    deliberately keeps the flow --
            (Flow::Capture, UiEffect::Activate) => self.view_state.commit_capture(),
            (Flow::Capture, UiEffect::SetMode(Mode::Normal)) => self.view_state.close_capture(),
            // The recurrence editor's Escape ladder (#976 SB1,
            // US-2/S3): the mode.normal resolution under the live
            // editor is the explicit cancel — the clean close (the
            // draft dies, nothing dispatches; the settle rule, not
            // the commit funnel). Insert/Activate stay the named
            // inert (the class row): only Escape cancels.
            (Flow::EditRecur { .. }, UiEffect::SetMode(Mode::Normal)) => {
                // Escape's clean close (#1221): the refused-commit
                // flag dies with the flow.
                self.set_recur_draft_invalid(window, false);
                self.view_state.end_edit_recur()
            }
            // vi's `i`/`a` open the view's editing flow rather than
            // flipping mode into the void (D5).
            (
                Flow::None | Flow::Move { .. } | Flow::MoveStack { .. },
                UiEffect::SetMode(Mode::Insert),
            ) => self.interpret_insert(&state),
            // The description editor's insert inert (round-3 D15):
            // re-entering insert inside the open editor changes
            // nothing (the field owns its printables).
            (Flow::EditDescription { .. }, UiEffect::SetMode(Mode::Insert)) => Vec::new(),
            // -- no flow: the intents begin things --
            (Flow::None, UiEffect::OpenPalette) => {
                // The palette is the second modal context (D8): the
                // modal-takeover half of the r2 D13 rule — the tip
                // dies into the overlay (the BeginCapture shape).
                Self::dismiss_tooltip(window);
                self.view_state.open_palette()
            }
            // The title action with no flow (round-3 W-SM-d): arms
            // the viewed task's editor; inert without a viewed
            // subject (the matrix's "no subject" cell).
            (Flow::None, UiEffect::BeginTitleEdit) => match self.view_state.subject_task() {
                Some(task) => self.view_state.begin_edit_title(&state, task),
                None => Vec::new(),
            },
            // The detail editors' keyboard openers (keymap-parity plan
            // US-3, D-4): the same funnels the row affordances call —
            // the round-3 D11 `begin_date_edit` body and the round-3
            // D15 description arm (click ≡ key at the funnel). The
            // subjectless detail is each funnel's typed inert.
            (Flow::None, UiEffect::BeginDateEdit { field }) => {
                self.begin_date_edit(window, field);
                Vec::new()
            }
            (Flow::None, UiEffect::BeginDescriptionEdit) => {
                self.begin_description_edit(window);
                Vec::new()
            }
            // The effort field's focus opener (J3 US-4, round 9 chunk
            // 11): a pure focus verb over the always-on M9 row — the
            // anchor family's detail-cell seam (kind 6, the effort
            // field's own anchor). Arms nothing, commits nothing (the
            // M9 commit-on-focus-loss law keeps the field as commit
            // owner); inert without a viewed subject (the matrix's
            // "no subject" cell).
            (Flow::None, UiEffect::BeginEffortEdit) => {
                if self.view_state.subject_task().is_some() {
                    window.invoke_focus_effort_opener();
                }
                Vec::new()
            }
            // The habit editor's opener (H4 D1, US-5): the Activate
            // shape over the one-shot habit subject — the shared
            // detail surface mounts the habit variant. Kiosk dispatch
            // is structurally inert (D7: the editor is unmountable; a
            // subjectless dispatch surfaces the typed refusal on the
            // notice lane — the T-19 twin).
            (_, UiEffect::HabitEdit) if self.kiosk => Vec::new(),
            (Flow::None, UiEffect::HabitEdit) => {
                // The subject resolution (#1303): the menu rows'
                // close-leg mint (the one-shot `habit_target` — the
                // row's own subject captured BEFORE the disarm law
                // cleared the opener's focus facts) resolves first;
                // the chain (the walk cursor, the row seams' mints)
                // follows. The mint is liveness-filtered here: a dead
                // target falls through to the chain, never answers.
                let subject = self
                    .habit_target
                    .take()
                    .filter(|habit| {
                        self.state
                            .load()
                            .habits
                            .habits
                            .get(habit)
                            .is_some_and(taskboard_domain::habit::Habit::is_live)
                    })
                    .or_else(|| self.habit_edit_subject());
                // The draft-before-mount law (#1303 §1.3): the draft
                // seeds from the stored row FIRST (the editor opens
                // showing the habit as it is; it dies with the editor
                // — cancel/escape/close, no draft persistence, D1),
                // and the editor mounts ONLY over a `Some` draft — an
                // unseedable subject lands in the same typed refusal
                // as a subjectless one. No `open_habit_editor` call
                // exists without a `Some` draft afterwards.
                let draft = subject.as_ref().and_then(|habit| {
                    self.state
                        .load()
                        .habits
                        .habits
                        .get(habit)
                        .map(crate::router::HabitEditDraft::from_habit)
                });
                if let Some(draft) = draft {
                    let habit = draft.habit;
                    self.habit_edit_draft = Some(draft);
                    let outcomes = self.view_state.open_habit_editor(habit);
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                } else if
                // Only a SUBJECTLESS dispatch falls through to the
                // walk cursor's task landing. A resolved-but-unseedable
                // subject is unrepresentable — the liveness filter and
                // the seed read the same state load — and must not
                // reach the task landing either, so the guard excludes
                // it (a silent no-op, never the task editor).
                subject.is_none()
                    && !self.kiosk
                    && self.view_state.focused_task().is_some()
                {
                    // The walk cursor's fall-through (D3a W-16, #1202):
                    // the Return row at Today is the habit editor's —
                    // over a TASK walk landing the same stroke belongs
                    // to the focused row's OWN verb (the rule-cursor
                    // activation's parity law: the cursor is a pointer,
                    // not a new verb family). The task editor opens
                    // through the landed Activate shape, unchanged.
                    let outcomes = self.interpret_begin(UiEffect::Activate, &state);
                    self.apply_outcomes(window, &outcomes);
                } else {
                    self.arm_no_target_notice(ActionId::HabitEdit);
                    self.push_notice(window);
                }
                Vec::new()
            }
            // The quota popover's opener (W-5, #961): the one arming
            // site both invocation paths converge on (the row phrase
            // click seam routes the same effect — click ≡ key). Kiosk
            // is structurally inert (R-3's disabled posture gates the
            // affordance); a subjectless dispatch surfaces the typed
            // refusal (the T-19 twin).
            (_, UiEffect::HabitQuotaEdit) if self.kiosk => Vec::new(),
            (Flow::None, UiEffect::HabitQuotaEdit) => {
                if let Some(habit) = self.habit_edit_subject() {
                    let state = self.state.load();
                    let seed = state.habits.habits.get(&habit).map(|row| {
                        (
                            // The typed seed (D3a W-8, #1196): the
                            // stored quota, clamped at the draft
                            // domain's max when above it — never the
                            // silent floor of 1
                            // (presentation::quota_seed's law).
                            crate::presentation::quota_seed(row.target.quota().get()),
                            crate::presentation::habit_target_phrase(row.target),
                        )
                    });
                    if let Some((quota, phrase)) = seed {
                        drop(state);
                        // The draft starts AT the stored quota (the
                        // popover shows the current count — US-4/AC-1);
                        // the kind phrase is the row's own (the picker
                        // is locked to the current kind — the inline
                        // edit is a one-number edit).
                        self.habit_quota_count = quota;
                        window.set_habits_quota_kind_phrase(SharedString::from(phrase));
                        window.set_habits_quota_open(true);
                        self.sync_quota_popover(window);
                        // The focus fact composes both directions (the
                        // row activation's law).
                        self.focused_habit = Some(habit);
                    } else {
                        drop(state);
                    }
                } else {
                    self.arm_no_target_notice(ActionId::HabitQuotaEdit);
                    self.push_notice(window);
                    // #1478: the refusal disarms the survivor too —
                    // the menu route's dispatch-scoped disarm leaves
                    // the quota exception's RESTORED pre-close focus
                    // alive past a refused dispatch; a dead subject
                    // minting nothing means the restore stands, so the
                    // clear happens here (the arm-refusal shape: the
                    // success path's mint above stays the only
                    // survivor writer).
                    self.focused_habit = None;
                }
                Vec::new()
            }
            // The habit history dialog's opener (HF3 W-8, US-7/S1):
            // the read-role month-grid projection mounts over the
            // resolved habit subject. Kiosk dispatch is structurally
            // inert (R-3 — the dialog is unmountable, D7); a
            // subjectless dispatch surfaces the typed refusal on the
            // notice lane (the T-19 twin). Zero commands route — the
            // dialog is a projection (US-7/S2–S4).
            (_, UiEffect::HabitHistoryOpen) if self.kiosk => Vec::new(),
            (Flow::None, UiEffect::HabitHistoryOpen) => {
                self.open_habit_history_from_subject(window);
                Vec::new()
            }
            // The rule cursor's activation (J3 US-6 D-3, round 9
            // chunk 12): over an armed cursor, Activate dispatches the
            // cursor'd row's OWN affordance — remove for an armed
            // rule, the due editor for an unarmed one (the row's
            // icon-button seams; the cursor is a pointer, not a new
            // verb family, R-5's parity law). Disarmed, the detail's
            // Activate falls through to `interpret_begin` untouched.
            (Flow::None, UiEffect::Activate)
                if self.view_state.view() == crate::view_state::ViewId::TaskDetail
                    && self.view_state.rule_cursor().is_some() =>
            {
                self.activate_rule_cursor(&state, window);
                Vec::new()
            }
            // The planner cursor's activation (US-15/S3, #1041): over
            // an armed cursor, Activate dispatches the cursor'd row's
            // OWN seam — the same `PlanBeginPlace` the row's
            // click/AT-action seam routes (click ≡ key; the rule
            // cursor's parity law). Disarmed, the planner's Activate
            // falls through to `interpret_begin` untouched.
            (Flow::None, UiEffect::Activate)
                if self.view_state.view() == crate::view_state::ViewId::PlanNextDay
                    && self.view_state.plan_cursor().is_some() =>
            {
                self.activate_plan_cursor(window);
                Vec::new()
            }
            (
                Flow::None,
                effect @ (UiEffect::Activate
                | UiEffect::RequestMove
                | UiEffect::BeginCreate { .. }
                | UiEffect::BeginCreateStack),
            ) => {
                // The ghost case (r4 chunk 4, D14): the identity tuple
                // does not move, but the hovered ghost itself unmounts
                // by construction of the flow the begin opens — an
                // explicit dismiss beside the flow's arming.
                if matches!(
                    effect,
                    UiEffect::BeginCreate { .. } | UiEffect::BeginCreateStack
                ) {
                    Self::dismiss_tooltip(window);
                }
                self.interpret_begin(effect, &state)
            }
            // -- no flow: the pure view-state defaults (the arms the
            //    resolver's settle-then-run re-dispatch resolves
            //    against; under a foreign flow the same effects
            //    settle first, audit §6 rows S) --
            (Flow::None, UiEffect::FocusToday) => self.view_state.focus_today(),
            (Flow::None, UiEffect::OpenInbox) => self.view_state.open_inbox(&state),
            // UX-R8 D-4's Escape cell: the move-cancel stroke with
            // no flow is the settle law's keyboard face — the
            // selection clears, nothing dispatches. The rule cursor's
            // D-2 site 5 joins: the global cancel stays
            // unambiguously a clear.
            (Flow::None, UiEffect::CancelMove) => {
                self.view_state.clear_rule_cursor();
                // The roster cursor joins (US-9, W-10): the global
                // cancel stays unambiguously a clear (the rule
                // cursor's D-2 site 5 twin).
                self.view_state.clear_break_rule_cursor();
                self.view_state.clear_plan_cursor();
                self.view_state.clear_selection()
            }
            (Flow::None, UiEffect::FocusNext) => {
                let day = self.day_ctx();
                self.view_state
                    .advance_focus(&state, compact, self.kiosk, &day)
            }
            (Flow::None, UiEffect::FocusPrev) => {
                let day = self.day_ctx();
                self.view_state
                    .retreat_focus(&state, compact, self.kiosk, &day)
            }
            // The directional-focus table (J3 US-1, round 9 chunk 9):
            // one declarative home for the per-view reading — the
            // `push_scroll_page_hint` precedent. The control-walk
            // views return no outcomes here: the post-table tail runs
            // the traversal injection for them (one walk
            // implementation, two dispatch fronts — the D-2
            // route-level rule). Every audited view carries a row or
            // an explicit exclusion comment in
            // [`Self::interpret_directional`].
            (Flow::None, UiEffect::FocusDirectional(direction)) => {
                let day = self.day_ctx();
                self.interpret_directional(&state, direction, compact, &day)
            }
            // The selection-extend route front (#875 RIDER): the same
            // guard the intercept branch carries — Board only, the
            // typed no-op elsewhere (the vi rows must not leak
            // selection semantics into other views through the
            // router).
            (
                Flow::None,
                effect @ (UiEffect::ExtendSelectionNext | UiEffect::ExtendSelectionPrev),
            ) => {
                if self.view_state.view() == crate::view_state::ViewId::Board {
                    let direction = if effect == UiEffect::ExtendSelectionNext {
                        1
                    } else {
                        -1
                    };
                    self.view_state.extend_selection_walk(&state, direction)
                } else {
                    Vec::new()
                }
            }
            (Flow::None, UiEffect::NextView) => self.view_state.next_view(&state),
            (Flow::None, UiEffect::PrevView) => self.view_state.prev_view(&state),
            (Flow::None, UiEffect::FocusBoard) => self.view_state.focus_board(),
            (Flow::None, UiEffect::BoardNext) => self.view_state.next_board(&state),
            (Flow::None, UiEffect::BoardPrevious) => self.view_state.previous_board(&state),
            (Flow::None, UiEffect::BoardSelect(board)) => {
                self.view_state.select_board(&state, board)
            }
            (Flow::None, effect @ (UiEffect::StackWider | UiEffect::StackNarrower)) => {
                let direction = if effect == UiEffect::StackWider {
                    1
                } else {
                    -1
                };
                // The resolved row plus the scaled step (r4 D8): the
                // same assembly the models push reads — one derivation.
                let row = crate::models::board_row(window, &state, &self.view_state);
                let scale = f64::from(window.global::<crate::Theme>().get_base_scale()).max(0.001);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let step = (f64::from(crate::widths::STACK_WIDTH_STEP_PX) * scale).round() as i64;
                self.view_state.resize_focused_stack(
                    &state,
                    direction,
                    &crate::view_state::RowStep {
                        stacks: &row.stacks,
                        widths: &row.widths,
                        mins: &row.mins,
                        step,
                        scale,
                    },
                )
            }
            // Round 2 (D17/D20): the stack wrap-walk and the first/last
            // jumps — pure view state, the BoardNext precedent.
            (Flow::None, UiEffect::StackNext) => self.view_state.step_stack(&state, 1),
            (Flow::None, UiEffect::StackPrev) => self.view_state.step_stack(&state, -1),
            (Flow::None, effect @ (UiEffect::FocusFirst | UiEffect::FocusLast)) => {
                let edge = if effect == UiEffect::FocusFirst {
                    FocusEdge::First
                } else {
                    FocusEdge::Last
                };
                {
                    let day = self.day_ctx();
                    self.view_state
                        .jump_focus(&state, edge, compact, self.kiosk, &day)
                }
            }
            // The armed delete's repeat stroke (UX R-2 D-2, #784):
            // the invoking verb's second stroke confirms the armed
            // subject (or re-arms to a new one) — the two-step
            // invoke; the strip's Confirm seam is its pointer twin
            // (click ≡ key at every step).
            (Flow::ConfirmDelete(_), effect @ (UiEffect::StackDelete | UiEffect::BoardDelete)) => {
                self.interpret_lifecycle(&flow, effect, &state)
            }
            // Round 2 (D18/D21): the board/stack lifecycle verbs —
            // target resolution is the view state's; the engine owns
            // every delete refusal.
            (
                Flow::None,
                effect @ (UiEffect::StackDelete
                | UiEffect::StackRename
                | UiEffect::StackMove
                | UiEffect::BoardCreate
                | UiEffect::BoardDelete
                | UiEffect::BoardRename
                | UiEffect::BoardColorEdit
                | UiEffect::BoardDefaultPreset),
            ) => self.interpret_lifecycle(&flow, effect, &state),
            // The board ceremony verbs (OP1, US-1/2/3): the same
            // shape as the armed-delete row — the first stroke arms
            // (over `Flow::None`), the armed subject's repeat stroke
            // confirms (click ≡ key at every step; the reconcile's
            // pending-preview guard is the view state's).
            (
                Flow::None,
                effect @ (UiEffect::BoardArchive | UiEffect::BoardReset | UiEffect::BoardReconcile),
            ) => self.interpret_ceremony(effect, &state),
            (
                Flow::ConfirmBoardArchive { .. } | Flow::ConfirmBoardReset { .. },
                effect @ (UiEffect::BoardArchive | UiEffect::BoardReset),
            ) => self.interpret_ceremony(effect, &state),
            (Flow::ConfirmBoardReconcile { .. }, UiEffect::BoardReconcile) => {
                self.interpret_ceremony(UiEffect::BoardReconcile, &state)
            }
            // The status panel's toggle (OP3 W-1, the settings
            // opener's twin, D-5): the open is a pushed view level,
            // and the snapshot query dispatches HERE — the route arm
            // is the query's sole dispatcher (one query per mount,
            // OP3 D-3; #1281 retired the doubled init seam).
            (Flow::None, UiEffect::OpenStatusPanel) => {
                if self.view_state.view() == crate::view_state::ViewId::Status {
                    self.view_state.close_status_panel()
                } else {
                    let outcomes = self.view_state.open_status_panel();
                    self.dispatch_status_query(window);
                    outcomes
                }
            }
            // Round-1 chunk 3 (D28b, D6.3): toggle semantics — the
            // menu row, `Ctrl+,`, and a second press all close an open
            // settings surface. No explicit dismiss: both toggle
            // directions swap the mounted view, the identity diff at
            // the choke point covers them (r4 chunk 4).
            (Flow::None, UiEffect::OpenSettings) => {
                if self.view_state.view() == crate::view_state::ViewId::Settings {
                    {
                        let mut outcomes = self.view_state.close_settings();
                        // The armed strip dies with the surface
                        // (cross-surface friction plan D-4, #836):
                        // leaving settings disarms, the strip is only
                        // visible while mounted.
                        self.settings_disarm_item();
                        // US-5 (#706): the restore's subject-liveness
                        // guard runs before the context sync, so the
                        // focus-restore funnel lands on the fallback.
                        outcomes.extend(self.view_state.repair_restored_root(&state));
                        outcomes
                    }
                } else {
                    self.view_state.open_settings()
                }
            }
            // -- the export dialog (the J2 export plan W-1) --
            // Kiosk: the dispatch is structurally inert (R-2 — the
            // dialog does not mount; the entry points are filtered
            // at the menu mint and the Settings view is unreachable).
            (_, UiEffect::ExportDialogOpen) if self.kiosk => Vec::new(),
            // The dialog is a Nav-class view intent (the settings
            // opener's shape): under a held flow the resolver settles
            // first and re-runs against Flow::None — the arms below
            // are the flowless cells only (the pair matrix's law).
            (Flow::None, UiEffect::ExportDialogOpen) if self.export_vm.is_some() => Vec::new(),
            (Flow::None, UiEffect::ExportDialogOpen) => {
                self.export_vm = Some(crate::export_dialog::ExportVm::seeded(
                    &self.export_defaults,
                ));
                // The mount never dispatches anything (US-1/S1 — pure
                // presentation state); the refresh pushes the seeded
                // model.
                vec![FlowOutcome::RefreshNow]
            }
            (Flow::None, UiEffect::ExportDialogClose) => self.export_dialog_close_outcomes(),
            // -- the plan-next-day surface (#622, D-4/D-3) --
            // Kiosk: the dispatch is structurally inert (the
            // entries-overlay refusal's layer 3 — D-5).
            (
                _,
                _effect @ (UiEffect::PlanBeginPlace(_)
                | UiEffect::PlanCommitPlace
                | UiEffect::PlanBoxEarlier(_)
                | UiEffect::PlanBoxLater(_)
                | UiEffect::PlanRemoveBox(_)),
            ) if self.kiosk => Vec::new(),
            (_, UiEffect::OpenPlanNextDay) if self.kiosk => Vec::new(),
            // A held flow keeps the planner intents inert (the modal
            // discipline: Escape out of the flow first) — the
            // placement form's own rung excepted: form → pane.
            (Flow::PlanPlace { .. }, UiEffect::OpenPlanNextDay) => {
                // The discard's wipe is the level's drop (D-H2).
                self.view_state.close_plan_place()
            }
            // The commit funnel: parse → mask → ONE command (the
            // view never sees a `StateCommand`); invalid input
            // dispatches nothing — the typed refusal keeps the text.
            (Flow::PlanPlace { task }, UiEffect::PlanCommitPlace) => self.commit_plan_place(*task),
            (Flow::None | Flow::ReviewPlanner, UiEffect::PlanCommitPlace) => Vec::new(),
            // -- the review walk (DR-7 chunk 3, #638) --
            // Kiosk: the dispatch is structurally inert (layer 2 of
            // the three-layer exclusion, D-J14 — the pinned gate).
            (_, UiEffect::OpenReviewWalk | UiEffect::ReviewWalk(_)) if self.kiosk => Vec::new(),
            // Already in the walk: the opener is not a toggle (the
            // modal discipline — Escape out first).
            (Flow::ReviewWalk { .. }, UiEffect::OpenReviewWalk) => Vec::new(),
            // The opener (the action, the Today card, the menu): the
            // reference derives from `review_due` when a window is
            // due, else the current day window (the evening review
            // before the boundary).
            (Flow::None, UiEffect::OpenReviewWalk) => {
                let reference = self.walk_reference(&state);
                let cadence = taskboard_domain::ReviewCadence::Day;
                self.view_state.open_review_walk(cadence, reference)
            }
            // The carrier's intents under any child level: the child
            // has no kept arms — the verb table runs against the walk
            // level beneath (inert without one; the fixture's depth-1
            // shape).
            (
                Flow::EntryDatePicker
                | Flow::WalkFilePick { .. }
                | Flow::WalkDatePick
                | Flow::ReviewPlanner,
                UiEffect::ReviewWalk(intent),
            ) => match self.view_state.walk_flow() {
                Some(Flow::ReviewWalk {
                    cadence, reference, ..
                }) => self.interpret_walk(window, cadence, reference, intent, &state),
                _ => Vec::new(),
            },
            // The carrier's intents: the verb table.
            (
                Flow::ReviewWalk {
                    cadence, reference, ..
                },
                UiEffect::ReviewWalk(intent),
            ) => self.interpret_walk(window, *cadence, *reference, intent, &state),
            // The embedded planner's cursor arms (US-22, #1234): the
            // walk's planner child shares the standalone planner's K4
            // cursor — the verticals route to the SAME
            // `walk_plan_cursor` seam (R-9's one-mechanism law; the
            // class matrix's Nav settle would kill the child, which is
            // exactly the gap this kept arm closes), and an armed
            // cursor's Activate dispatches the cursor'd row's OWN
            // `PlanBeginPlace` seam (the `Flow::None` twin above; the
            // child keeps the form's mount via `open_plan_place_child`).
            // A horizontal is untouched — the walk's own grammar keeps
            // the class matrix's cell.
            (
                Flow::ReviewPlanner,
                effect @ UiEffect::FocusDirectional(Direction::Up | Direction::Down),
            ) => {
                let UiEffect::FocusDirectional(direction) = effect else {
                    unreachable!("arm matched")
                };
                let step = i32::from(direction == Direction::Down) * 2 - 1;
                self.walk_plan_cursor(step)
            }
            (Flow::ReviewPlanner, UiEffect::Activate)
                if self.view_state.plan_cursor().is_some() =>
            {
                self.activate_plan_cursor(window);
                Vec::new()
            }
            // Toggle semantics (the settings opener's twin); the
            // planner-context Escape row rides the same route.
            (Flow::None, UiEffect::OpenPlanNextDay) => {
                if self.view_state.view() == crate::view_state::ViewId::PlanNextDay {
                    let mut outcomes = self.view_state.close_plan_next_day();
                    // US-5 (#706): the restore's subject-liveness
                    // guard (see the OpenSettings close arm).
                    outcomes.extend(self.view_state.repair_restored_root(&state));
                    outcomes
                } else {
                    self.view_state.open_plan_next_day()
                }
            }
            // The row verbs (dispatch-time gating: the rows only
            // render on the surface; a stale target is inert). Under
            // the embedded planner the form mounts as the planner's
            // own child level (J4 D-2 — replacing the top flow would
            // kill the planner step).
            (Flow::ReviewPlanner, UiEffect::PlanBeginPlace(task)) => {
                let outcomes = self.view_state.open_plan_place_child(task);
                // The child mount seeds nothing: a placement begun
                // from the pool carries no prior instant (the seed
                // rule reads the placed box, not the pool row).
                outcomes
            }
            (Flow::None, UiEffect::PlanBeginPlace(task)) => {
                let outcomes = self.view_state.open_plan_place(task);
                // Re-opening a placed box edits that box (D-3): the
                // form seeds from the row's instant — tomorrow's box
                // only (a stale scheduled stamp from any earlier day
                // seeds nothing; the commit would re-place to
                // tomorrow). The seed rides the freshly minted level.
                if let Some(scheduled) = state.tasks.get(&task).and_then(|t| t.scheduled) {
                    let (next_start, _) = taskboard_domain::days::next_day_window(
                        self.day_ctx().now,
                        self.day_start.0,
                        self.day_start.1,
                    );
                    if scheduled >= next_start {
                        let phrase = crate::presentation::plan_box_time_phrase(
                            Some(scheduled),
                            self.calendar(),
                        );
                        if let Some(place) = self.view_state.plan_place_mut() {
                            place.time_text = phrase;
                        }
                    }
                }
                outcomes
            }
            (Flow::None | Flow::ReviewPlanner, UiEffect::PlanRemoveBox(task)) => {
                // The unplace's wipe is the level's drop (D-H2).
                self.view_state.remove_plan_box(task)
            }
            (
                Flow::None | Flow::ReviewPlanner,
                effect @ (UiEffect::PlanBoxEarlier(_) | UiEffect::PlanBoxLater(_)),
            ) => {
                let (UiEffect::PlanBoxEarlier(task) | UiEffect::PlanBoxLater(task)) = effect else {
                    unreachable!("arm matched")
                };
                self.swap_plan_box(task, effect == UiEffect::PlanBoxEarlier(task), &state)
            }
            // nav.show-task (round-3 D23): acts on the palette's
            // selection — the Task retarget rides `palette_jump`'s
            // `show_task` arm, the other targets keep today's
            // retargets, a stale/absent cursor closes only. Inert
            // dispatched outside a palette flow (no preset binds it).
            (Flow::Palette { .. }, UiEffect::ShowTask) => {
                let selected = match self.view_state.flow() {
                    Flow::Palette { selected } => *selected,
                    _ => 0,
                };
                match self.tables.palette.get(selected).copied() {
                    Some(target) => self
                        .view_state
                        .palette_jump(&state, target, self.today_date()),
                    None => self.view_state.close_palette(),
                }
            }
            (Flow::None, UiEffect::ShowTask) => Vec::new(),
            // A mode flip with no view meaning stays plain shell
            // state (D10) — e.g. Escape out of an empty insert.
            (Flow::None, UiEffect::SetMode(mode)) => {
                self.shell.apply(&UiEffect::SetMode(mode), 0);
                Vec::new()
            }
            // The mask's own walk (US-2/W-2, D-W2): the control walk
            // inside a masked site is the mask's traversal — kept
            // (no view mutation; the walk's clamp consumes it in this
            // table's post-tail), never a settle trigger. The masked
            // set is the clamp's host census: the palette, capture,
            // focus overlay, entries family, recur editor, and labels
            // overlay.
            (
                Flow::Palette { .. }
                | Flow::Capture
                | Flow::FocusMode { .. }
                | Flow::TrackEntries { .. }
                | Flow::TrackEntryForm { .. }
                | Flow::EditRecur { .. }
                | Flow::Labels { .. },
                UiEffect::ControlNext | UiEffect::ControlPrev,
            ) => Vec::new(),
            // Round 3 (W-SM-b): the wildcard arm is gone. Every
            // remaining pair — a foreign flow meeting an effect its
            // kept arms don't own — goes through the named resolver:
            // settle per the flow's kind rule, then re-run (R3/R4).
            // No `(Flow, effect)` pair is silent any more; the
            // compiler enforces the resolver's totality over
            // `EffectClass` and the pair-table test pins every cell
            // to the audit matrix.
            (_, effect) => {
                self.resolve_conflict(window, &flow, effect, &state);
                Vec::new()
            }
        };
        self.apply_outcomes(window, &outcomes);
        if focus_walk || show_task {
            self.push_focus_scroll_hint(window, focus_before);
        }
        if drag_teardown {
            // After the outcomes: the cancel/confirm has ended the
            // view-state flow; the synthetic exit now tears the
            // native drag down and its `drag-finished(none)` echo is
            // absorbed by the flow-guarded seam.
            Self::teardown_runtime_drag(window);
        }
        // The control walk (round-3 D25): the traversal re-injection
        // runs after the outcomes, outside the table — the walk never
        // mutates view state, it moves slint focus only. Under a
        // masked site the walk clamps instead (US-2/W-2, D-W2): the
        // cursor steps within the host's extent and wraps at the mask
        // receive focus. The degenerate extent (≤ 1) stays on 0.
        // J3 R-4 (round 9 chunk 9): the directional table's
        // control-walk views ride the same injection — Left/Up walk
        // backwards, Right/Down forwards. #1034: Tab/Shift-Tab is the
        // same decision's special case — the one
        // [`Self::control_walk_step`] function states the whole walk
        // half of the stop ladder, and the tail consumes its verdict
        // once (the old double `directional_control_step` call is
        // gone).
        if let Some(backwards) = self.control_walk_step(&state, effect) {
            if self.mask_armed(window) {
                let facts = window.global::<crate::MaskWalkFacts>();
                let extent = facts.get_extent();
                let cursor = facts.get_cursor();
                let next = if extent <= 1 {
                    0
                } else if backwards {
                    (cursor + extent - 1) % extent
                } else {
                    (cursor + 1) % extent
                };
                window.invoke_mask_focus(next);
            } else {
                let flag = self.walk_reentry.clone();
                Self::dispatch_traversal(window, &flag, backwards, &self.walk_timer);
            }
        }
        // The scroll actions (round-3 D26): the focused container
        // pages after the outcomes (the tables are fresh).
        if matches!(effect, UiEffect::ScrollUp | UiEffect::ScrollDown) {
            self.push_scroll_page_hint(window, effect == UiEffect::ScrollDown);
        }
    }

    /// The scroll actions' container resolution (round-3 D26): the
    /// focused task's column on the wide board, the selected stack's
    /// viewport on the compact board and the stack detail, the focused
    /// task's section on today, the settings pane adapter-side through
    /// the two-way facts, and a typed inert on the task detail (no
    /// container) or without a focused task (the ends hold).
    pub(in crate::adapter) fn push_scroll_page_hint(&mut self, window: &AppWindow, down: bool) {
        const FAMILY_SETTINGS: i32 = 0;
        const FAMILY_BOARD: i32 = 1;
        const FAMILY_TODAY: i32 = 2;
        const FAMILY_STACK: i32 = 3;
        const FAMILY_REVIEW: i32 = 4;
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let to_hint_index = |value: usize| value as i32;
        let page = if down { 1 } else { -1 };
        let state = self.state.load_full();
        let hint: Option<(i32, i32)> = match (self.view_state.view(), Self::is_compact(window)) {
            (crate::view_state::ViewId::Board, false) => self
                .view_state
                .focused_task()
                .and_then(|focused| self.tables.board_position(focused))
                .map(|(column, _)| (FAMILY_BOARD, to_hint_index(column))),
            (crate::view_state::ViewId::Board, true) => {
                // The compact leg's selected viewport: the selected
                // column (the focused task's, when the walk seeded one).
                let selected = self.view_state.selected_stack();
                let column = selected.and_then(|stack| {
                    self.view_state
                        .stack_ids(&state)
                        .iter()
                        .position(|s| *s == stack)
                });
                column.map(|c| (FAMILY_STACK, to_hint_index(c)))
            }
            (crate::view_state::ViewId::StackDetail, _) => Some((FAMILY_STACK, -1)),
            // The planner's two panes are a fixed surface (#622): no
            // paging container — a typed inert like the task detail.
            (crate::view_state::ViewId::PlanNextDay | crate::view_state::ViewId::TaskDetail, _) => {
                None
            }
            // The status panel (OP3): one whole-page viewport, no
            // paging container — the same typed inert.
            (crate::view_state::ViewId::Status, _) => None,
            (crate::view_state::ViewId::Today, _) => self
                .view_state
                .focused_task()
                .and_then(|focused| self.tables.today_position(focused))
                .map(|(section, _)| (FAMILY_TODAY, to_hint_index(section))),
            (crate::view_state::ViewId::Settings, _) => {
                Self::page_settings(window, down);
                return;
            }
            // The review page (#214): one whole-page viewport, the
            // view's own ListView pages as family 4.
            (crate::view_state::ViewId::Review, _) => Some((FAMILY_REVIEW, -1)),
        };
        // The settings family only ever rides the facts path.
        let Some((family, index)) = hint.filter(|(family, _)| *family != FAMILY_SETTINGS) else {
            return;
        };
        let scroll = window.global::<crate::ScrollPageHint>();
        scroll.set_family(family);
        scroll.set_index(index);
        scroll.set_page(page);
        scroll.set_nonce(scroll.get_nonce() + 1);
    }

    /// The settings pane's adapter-side paging (round-3 D26): the
    /// facts global already carries the heights and the two-way
    /// `viewport-y` — the adapter writes the clamped target and no
    /// markup change is needed. Clamps at both ends by construction.
    pub(in crate::adapter) fn page_settings(window: &AppWindow, down: bool) {
        let facts = window.global::<crate::SettingsScrollFacts>();
        let viewport = facts.get_viewport_height();
        let content = facts.get_content_height();
        if viewport <= 0.0 || content <= viewport {
            return;
        }
        let max = content - viewport;
        let target = if down {
            facts.get_viewport_y() + viewport
        } else {
            facts.get_viewport_y() - viewport
        };
        facts.set_viewport_y(target.clamp(0.0, max));
    }

    /// The focus-into-view seam (#373 design round §3a, D-5): after a
    /// keyboard focus move's outcomes — the refresh has rebuilt the
    /// models and the id tables — resolve the newly focused task
    /// through the tables (global model indices, never
    /// viewport-relative) and edge-trigger the `FocusScrollHint`
    /// global. The matching `ListView` instance consumes it and scrolls
    /// the row into view; pointer selections are visible by
    /// construction and hint nothing. A move that changed nothing
    /// (the walk's ends) re-arms nothing.
    /// The one-shot arm's consumption (UX-R8 chunk-5 D-7): `Some`
    /// = the handoff fired (the arm consumed with exactly one new
    /// id) — the hint push edge-triggers on the handoff, never on a
    /// prior focus. The arm always clears.
    pub(in crate::adapter) fn consume_create_focus_arm(&mut self) -> Option<CreateFocusHandoff> {
        let arm = self.create_focus_arm.clone()?;
        let state = self.state.load_full();
        if Arc::ptr_eq(&state, &arm.snapshot) {
            // The engine has not applied yet (the commit's own
            // RefreshNow pass) — the arm holds for the publish.
            return None;
        }
        let live: std::collections::BTreeSet<TaskId> = state
            .tasks
            .values()
            .filter(|task| task.stack == arm.stack)
            .map(|task| task.id)
            .collect();
        drop(state);
        self.create_focus_arm = None;
        let mut new_ids = live.difference(&arm.before).copied();
        let (Some(created), None) = (new_ids.next(), new_ids.next()) else {
            // Zero or >1 new ids: a refused create or a racing capture
            // — no handoff (the one-shot law).
            return None;
        };
        let before = self.view_state.walk_focus();
        self.view_state.set_focused(Some(created));
        Some(CreateFocusHandoff { before })
    }

    pub(in crate::adapter) fn push_focus_scroll_hint(
        &mut self,
        window: &AppWindow,
        before: crate::view_state::WalkFocus,
    ) {
        // The generalized no-change guard (A5a D-2, #1302): a landing
        // whose whole subject tuple (task, habit, roster) is unchanged
        // re-arms nothing — the ends-hold law, widened from the
        // task-only capture. `before == landing` covers the old
        // `focused.is_none() || focused == before` shape.
        let landing = self.view_state.walk_focus();
        if landing == before {
            return;
        }
        // A slint int carries the index; the tables' lengths are far
        // below i32 reach.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let to_hint_index = |value: usize| value as i32;
        let hint = match self.view_state.view() {
            crate::view_state::ViewId::Board => landing
                .task
                .and_then(|task| self.tables.board_position(task))
                .map(|(column, row)| (to_hint_index(column), to_hint_index(row))),
            // The today walk's subject resolution (A5a D-2): task
            // precedence (the walk_subject law), then the habit row,
            // then the break-rule roster cursor — each against the
            // same `(section, row)` convention, the habit/roster rows
            // against the synthetic page slots
            // (`IdTables::TODAY_HABIT_SLOT`/`TODAY_ROSTER_SLOT`) that
            // no inner section ListView consumes.
            crate::view_state::ViewId::Today => {
                if let Some(task) = landing.task {
                    self.tables
                        .today_position(task)
                        .map(|(section, row)| (to_hint_index(section), to_hint_index(row)))
                } else if let Some(habit) = landing.habit {
                    self.tables
                        .today_habit_position(habit)
                        .map(|(section, row)| (to_hint_index(section), to_hint_index(row)))
                } else {
                    landing
                        .roster
                        .and_then(|rule| self.tables.today_roster_position(rule))
                        .map(|(section, row)| (to_hint_index(section), to_hint_index(row)))
                }
            }
            crate::view_state::ViewId::StackDetail => landing
                .task
                .and_then(|task| self.tables.stack_position(task))
                .map(|row| (-1, to_hint_index(row))),
            // The planner's pane walk (#1320, D-1320a/b): the cursor
            // task resolves `(pane, row)` — the #622 fixed-page
            // decision is NOT reopened (no paging hint; the narrowed
            // exception is the focus law only), the fixed surface's
            // own Flickable consumes the landing.
            crate::view_state::ViewId::PlanNextDay => landing
                .plan
                .and_then(|task| self.tables.plan_position(task))
                .map(|(pane, row)| (to_hint_index(pane), to_hint_index(row))),
            // The review page carries no focusable rows (D-8): the
            // focus-into-view seam never targets it, like the settings
            // facts path and the detail's own container.
            crate::view_state::ViewId::TaskDetail
            | crate::view_state::ViewId::Settings
            | crate::view_state::ViewId::Review
            // The status panel: no paging container (OP3).
            | crate::view_state::ViewId::Status => None,
        };
        let Some((target_a, target_b)) = hint else {
            return;
        };
        let scroll = window.global::<crate::FocusScrollHint>();
        scroll.set_target_a(target_a);
        scroll.set_target_b(target_b);
        scroll.set_nonce(scroll.get_nonce() + 1);
    }

    /// The move-flow walk's scroll hint (UX-R8 D-3): the armed
    /// `Flow::Move`'s (target, slot) as a `FocusScrollHint` edge —
    /// `target-a` = the aimed column (this pass's stack index),
    /// `target-b` = the gap's clamp row `min(gap, len-1)` (a gap
    /// after the last card seeks the last card). Inert without a
    /// live move flow (the stale-hint guard's shape).
    pub(in crate::adapter) fn push_move_scroll_hint(
        &mut self,
        window: &AppWindow,
        state: &taskboard_domain::AppState,
    ) {
        let Flow::Move { target, slot, .. } = self.view_state.flow() else {
            return;
        };
        let (target, slot) = (*target, *slot);
        let Some(column) = self
            .view_state
            .stack_ids(state)
            .iter()
            .position(|stack| *stack == target)
        else {
            return;
        };
        let len = state.tasks_in_stack(target).len();
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let to_hint_index = |value: usize| value as i32;
        let scroll = window.global::<crate::FocusScrollHint>();
        scroll.set_target_a(to_hint_index(column));
        scroll.set_target_b(to_hint_index(slot.min(len.saturating_sub(1))));
        scroll.set_nonce(scroll.get_nonce() + 1);
    }

    /// The control walk's traversal re-injection (round-3 D25): the
    /// synthetic `Tab` key press drives the runtime's own tab
    /// traversal (`window.rs` key handler); the previous direction is
    /// the Shift-wrapped variant (the journey driver's modifier
    /// technique, the `teardown_runtime_drag` synthetic-event
    /// precedent). `walk_reentry` is held across the dispatch so the
    /// injected stroke rejects out of `intercept-key` before the
    /// callback borrows — one press = exactly one stop, never a loop.
    pub(in crate::adapter) fn dispatch_traversal(
        window: &AppWindow,
        walk_reentry: &Rc<std::cell::Cell<bool>>,
        backwards: bool,
        timer: &slint::Timer,
    ) {
        // Deferred (see the field's doc): the callback fires outside
        // the current key event, where the synthetic Tab reaches the
        // window's traversal fallback instead of dying re-entrantly.
        let weak = window.as_weak();
        let reentry = Rc::clone(walk_reentry);
        timer.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(0),
            move || {
                let Some(window) = weak.upgrade() else {
                    return;
                };
                reentry.set(true);
                let win = window.window();
                let tab: slint::SharedString = "\t".into();
                let inject = |win: &slint::Window, tab: &slint::SharedString| {
                    if backwards {
                        let shift: slint::SharedString =
                            slint::platform::Key::into(slint::platform::Key::Shift);
                        win.dispatch_event(slint::platform::WindowEvent::KeyPressed {
                            text: shift.clone(),
                        });
                        win.dispatch_event(slint::platform::WindowEvent::KeyPressed {
                            text: tab.clone(),
                        });
                        win.dispatch_event(slint::platform::WindowEvent::KeyReleased {
                            text: shift,
                        });
                    } else {
                        win.dispatch_event(slint::platform::WindowEvent::KeyPressed {
                            text: tab.clone(),
                        });
                    }
                };
                // The text-input dead-end (#1203, round 5 A2): a
                // `TextInput` accepts the synthetic Tab without moving
                // the traversal — the step that LANDS on a field is a
                // legitimate stop (the fields are ladder stops, the
                // walk-coverage inventories' reading-order contracts),
                // but a step FROM a field went nowhere and the walk
                // dead-ended (the detail's lower half was unreachable
                // past the effort field). When the walk departs a
                // field, re-inject until the traversal actually moves
                // off text focus (bounded — one press stays one stop,
                // never a loop).
                let departing_a_field = window
                    .global::<crate::EditingFacts>()
                    .get_text_focus_count()
                    > 0;
                inject(win, &tab);
                if departing_a_field {
                    for _ in 0..3 {
                        if window
                            .global::<crate::EditingFacts>()
                            .get_text_focus_count()
                            == 0
                        {
                            break;
                        }
                        inject(win, &tab);
                    }
                }
                reentry.set(false);
            },
        );
    }

    /// The native drag's teardown seam (r2 chunk-8 D-G): a synthetic
    /// `PointerExited` — the runtime tears any in-flight drag down
    /// and fires `drag-finished(none)` — absorbed by the flow-guarded
    /// seam: the view-state flow is already gone, and `TouchArea`'s
    /// Exit arm clears `pressed`/hover so the real release cannot
    /// degrade into `clicked`. The one synthetic event: the runtime
    /// exposes no other public in-flight-drag cancel.
    pub(in crate::adapter) fn teardown_runtime_drag(window: &AppWindow) {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerExited);
    }

    /// vi's insert intent (D5): `i`/`a` open the view's editing flow
    /// rather than flipping mode into the void; `TaskDetail` edits its
    /// subject in place.
    pub(in crate::adapter) fn interpret_insert(
        &mut self,
        state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        match self.view_state.view() {
            crate::view_state::ViewId::TaskDetail => self.view_state.edit_subject(state),
            _ => match self.view_state.create_target_stack(state) {
                Some(stack) => self.view_state.begin_create(stack),
                None => Vec::new(),
            },
        }
    }

    /// The no-flow begin intents (D5/D9), factored out of the table.
    /// The stack-create refusal (a never-booted session offers nothing
    /// to create a stack on) stays in the view-state method — the one
    /// home for the board-existence refusal.
    pub(in crate::adapter) fn interpret_begin(
        &mut self,
        effect: UiEffect,
        state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        match effect {
            UiEffect::Activate => self.view_state.activate_focused(state),
            UiEffect::RequestMove => self.view_state.start_move(state),
            UiEffect::BeginCreateStack => self.view_state.begin_create_stack(state),
            // The ghost's clicked column (r4 D4): the payload wins.
            UiEffect::BeginCreate {
                target: Some(stack),
            } => self.view_state.begin_create(stack),
            // The keyboard's route: focused → selected → first,
            // byte-identical (r4 D4 acceptance e).
            _ => match self.view_state.create_target_stack(state) {
                Some(stack) => self.view_state.begin_create(stack),
                None => Vec::new(),
            },
        }
    }

    /// The rule cursor's activation (J3 US-6 D-3, round 9 chunk 12):
    /// the cursor'd row's own affordance set read off the projection's
    /// armed bit — `ReminderRuleRemove` for an armed rule (the one-shot
    /// `rule_target` minted exactly as `on_task_rule_remove_clicked`
    /// mints it) and the due editor for an unarmed one (the row's
    /// jump seam, `begin_date_edit`). Immediate, no staged confirm:
    /// the row's icon-button affordance is immediate today (R-5's
    /// strict parity law). A cursor whose rule died pre-dispatch
    /// re-resolves against the live roster and no-ops (the stale-id
    /// absorb). The cursor itself survives the verb — the roster the
    /// prune re-evaluates against changes under the dispatch, never
    /// by the verb's own hand.
    pub(in crate::adapter) fn activate_rule_cursor(
        &mut self,
        state: &AppState,
        window: &AppWindow,
    ) {
        let Some(rule_id) = self.view_state.rule_cursor() else {
            return;
        };
        let Some(rule) = state.reminders.rules.get(&rule_id) else {
            return; // the stale-id absorb: the dead cursor holds, nothing dispatches
        };
        let subject_duedate = self
            .view_state
            .subject_task()
            .and_then(|task| state.tasks.get(&task))
            .and_then(|task| task.duedate);
        if crate::presentation::rule_armed(rule, subject_duedate) {
            self.rule_target = Some(rule_id);
            self.route(window, ActionId::ReminderRuleRemove);
        } else {
            // The unarmed row's jump affordance (r4 D30): the due
            // editor opens for the subject — the same funnel the
            // row's calendar icon drives.
            self.begin_date_edit(window, crate::view_state::DateField::Due);
        }
    }

    /// The focused planner pane's rows (US-15): the id tables' flat
    /// `plan_rows` split at `plan_box_count` — the exact derivation
    /// the two panes render, so the walk's landing and the rendered
    /// rows agree by construction.
    pub(in crate::adapter) fn plan_pane_rows(&self) -> &[taskboard_domain::TaskId] {
        let boxes = self.tables.plan_box_count.min(self.tables.plan_rows.len());
        match self.view_state.plan_focus_pane() {
            0 => &self.tables.plan_rows[..boxes],
            _ => &self.tables.plan_rows[boxes..],
        }
    }

    /// The planner cursor's walk (US-15, #1041 — the rule cursor's
    /// `walk_rule_cursor` law over the pane's id table): the first
    /// vertical stroke over a non-empty pane arms the cursor at row 0
    /// (arm-then-step — the arming stroke does not move past row 0);
    /// each further stroke steps within the pane, clamping at both
    /// edges (no wrap — the ends hold). An empty pane is the typed
    /// no-op (never arm). The cursor state is the K4 field in the
    /// view state (R-9), the mechanics live here because the pane's
    /// row source is the adapter-owned id tables.
    pub(in crate::adapter) fn walk_plan_cursor(&mut self, direction: i32) -> Vec<FlowOutcome> {
        let rows = self.plan_pane_rows().to_vec();
        if rows.is_empty() {
            return Vec::new();
        }
        let current = self
            .view_state
            .plan_cursor()
            .and_then(|cursor| rows.iter().position(|id| *id == cursor));
        let Some(current) = current else {
            self.view_state.select_plan_cursor(rows[0]);
            return vec![FlowOutcome::RefreshNow];
        };
        let next = i64::from(direction) + i64::try_from(current).unwrap_or(0);
        match rows.get(usize::try_from(next).unwrap_or(usize::MAX)) {
            Some(task) => {
                self.view_state.select_plan_cursor(*task);
                vec![FlowOutcome::RefreshNow]
            }
            None => Vec::new(), // clamp: no wrap, the ends hold
        }
    }

    /// The plan-place begin tail (#1237): mint the one-shot
    /// `plan_target`, route `PlanBeginPlace`, then the seed echo — a
    /// placed row's re-open seeds the form from the box's instant
    /// (D-3), a pool row's mounting field shows empty. One helper,
    /// three call sites (the cursor activation and the box/pool
    /// click/AT-action seams); behavior-identical to the inlined
    /// tails it replaces.
    pub(in crate::adapter) fn begin_plan_place(
        &mut self,
        window: &AppWindow,
        task: TaskId,
        placed: bool,
    ) {
        self.plan_target = Some(task);
        self.route(window, ActionId::PlanBeginPlace);
        if placed {
            window.set_plan_time_text(SharedString::from(
                self.view_state
                    .plan_place()
                    .map_or(String::new(), |place| place.time_text.clone()),
            ));
        } else {
            window.set_plan_time_text(SharedString::new());
        }
    }

    /// The planner cursor's activation (US-15/S3): the cursor'd row's
    /// own seam — `PlanBeginPlace` with the one-shot `plan_target`
    /// minted exactly as the row's click/AT-action seam mints it, the
    /// placed row's instant seed and the pool row's empty seed riding
    /// verbatim. A cursor whose task died pre-dispatch absorbs (the
    /// stale-id rule): the cursor holds, nothing dispatches (the
    /// refresh clamp prunes it at the next pass). The cursor survives
    /// the verb — the clamp, never the verb's own hand, retires it.
    pub(in crate::adapter) fn activate_plan_cursor(&mut self, window: &AppWindow) {
        let Some(task) = self.view_state.plan_cursor() else {
            return;
        };
        let Some(flat) = self.tables.plan_rows.iter().position(|id| *id == task) else {
            return; // the stale-id absorb
        };
        self.begin_plan_place(window, task, flat < self.tables.plan_box_count);
    }

    /// The edit flows' commit/discard arms (D5/D9; the round-2
    /// lifecycle flows included), factored out of the table.
    pub(in crate::adapter) fn interpret_edit(
        &mut self,
        effect: UiEffect,
        state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        match effect {
            UiEffect::SetMode(Mode::Normal) => self.view_state.discard_edit(),
            // The table routes only the commit/discard pair here.
            _ => self.view_state.commit_edit(state),
        }
    }

    /// The description editor's arms (round-3 D15), factored out of
    /// the table: Escape discards; Activate commits through the
    /// description funnel (the buffer verbatim — empty is the clear).
    pub(in crate::adapter) fn interpret_description(
        &mut self,
        effect: UiEffect,
        _state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        match effect {
            UiEffect::SetMode(Mode::Normal) => self.view_state.discard_edit(),
            _ => self.view_state.commit_description(),
        }
    }

    /// The control walk's decision function (#1034): the whole walk
    /// half of the stop ladder in one place. Tab steps forwards,
    /// Shift-Tab backwards (the round-3 D25 re-injection), and a
    /// directional stroke is the per-view table's verdict
    /// ([`Self::directional_control_step`]); every other effect is
    /// not a walk. Pure.
    pub(in crate::adapter) fn control_walk_step(
        &self,
        state: &taskboard_domain::AppState,
        effect: UiEffect,
    ) -> Option<bool> {
        match effect {
            UiEffect::ControlNext => Some(false),
            UiEffect::ControlPrev => Some(true),
            UiEffect::FocusDirectional(_) => self.directional_control_step(state, effect),
            _ => None,
        }
    }

    /// The directional control-walk decision (J3 R-4 first cut, the
    /// per-view table's horizontal rows): `Some(backwards)` when the
    /// (view, direction) pair reads as a control step, `None` when
    /// the view's direction has outcomes of its own (the item walks,
    /// the board's stack step, the planner's pane switch) or the
    /// effect is not directional. Pure — the interpret table's
    /// [`Flow::None`] arm and the post-table tail both read it.
    pub(in crate::adapter) fn directional_control_step(
        &self,
        state: &taskboard_domain::AppState,
        effect: UiEffect,
    ) -> Option<bool> {
        let UiEffect::FocusDirectional(direction) = effect else {
            return None;
        };
        let backwards = matches!(direction, Direction::Left | Direction::Up);
        match self.view_state.view() {
            // Today/StackDetail: left/right are the control walk
            // first cut (R-4); up/down are the item walks (the
            // `interpret_directional` outcomes own them — `None`
            // here, the tail must not inject a traversal step).
            crate::view_state::ViewId::Today | crate::view_state::ViewId::StackDetail => {
                matches!(direction, Direction::Left | Direction::Right).then_some(backwards)
            }
            // TaskDetail: the rule cursor owns the verticals whenever
            // the roster can arm one (J3 US-6 D-5, round 9 chunk 12 —
            // `interpret_directional` returns outcomes — arm, step,
            // or clamp — and the tail must not ALSO inject a traversal
            // step); an empty roster (nothing to arm) and every
            // horizontal stay the control walk (chunk 9's row,
            // byte-identical while disarmed).
            crate::view_state::ViewId::TaskDetail
                if matches!(direction, Direction::Up | Direction::Down)
                    && self.view_state.rule_walk_owns_stroke(state) =>
            {
                None
            }
            // TaskDetail/Settings: every other direction is the control
            // walk — the stop ladder is linear today (the recorded
            // degenerate horizontal: up/down ride the same ladder).
            crate::view_state::ViewId::TaskDetail
            | crate::view_state::ViewId::Settings
            // The status panel's ladder is linear too (OP3): the
            // Refresh button is the walk's one stop.
            | crate::view_state::ViewId::Status => Some(backwards),
            _ => None,
        }
    }

    /// The directional-focus per-view table (J3 US-1, round 9 chunk
    /// 9): the one declarative home for what a direction means on
    /// each surface — focus movement, never task selection. Every
    /// audited view carries a row or an explicit exclusion comment.
    /// The control-walk views return no outcomes (the post-table
    /// tail injects the traversal — see
    /// [`Self::directional_control_step`]).
    #[allow(clippy::match_same_arms)] // one arm per table decision, comments included
    pub(in crate::adapter) fn interpret_directional(
        &mut self,
        state: &taskboard_domain::AppState,
        direction: Direction,
        compact: bool,
        day: &crate::view_state::DayCtx,
    ) -> Vec<FlowOutcome> {
        use crate::view_state::ViewId;
        match (self.view_state.view(), direction) {
            // Board: left/right step the stack walk (the
            // `UiEffect::StackNext/Prev` bodies); up/down walk the
            // task cards.
            (ViewId::Board, Direction::Left | Direction::Right) => {
                let step = i32::from(direction == Direction::Right) * 2 - 1;
                self.view_state.step_stack(state, step)
            }
            // The item walks (Board/Today/StackDetail up/down — the
            // FocusNext/Prev bodies; habit rows ride the Today
            // substrate).
            (ViewId::Board | ViewId::Today | ViewId::StackDetail, Direction::Down) => self
                .view_state
                .advance_focus(state, compact, self.kiosk, day),
            (ViewId::Board | ViewId::Today | ViewId::StackDetail, Direction::Up) => self
                .view_state
                .retreat_focus(state, compact, self.kiosk, day),
            // Today/StackDetail left/right: the control walk first cut
            // (R-4) — the tail owns it, no outcomes here.
            (ViewId::Today | ViewId::StackDetail, Direction::Left | Direction::Right) => Vec::new(),
            // TaskDetail's verticals walk the rule cursor (J3 US-6
            // D-5): the list's geometry is vertical, the declarative
            // arm sits beside the control-walk row. The arming gate
            // lives in `walk_rule_cursor` — a stroke over a non-empty
            // roster arms at row 0 then walks (the arming stroke does
            // not move past row 0, D-2's arm-then-step); an empty
            // roster is the typed no-op and the tail's control walk
            // proceeds (the D-5 negative — `directional_control_step`
            // returns `None` exactly when this arm owns the stroke).
            (ViewId::TaskDetail, Direction::Up | Direction::Down) => {
                let step = i32::from(direction == Direction::Down) * 2 - 1;
                self.view_state.walk_rule_cursor(state, step)
            }
            // TaskDetail/Settings: the control walk along the stop
            // ladder — NEVER the subject walk (J3 R-3: the viewed task
            // does not change from the detail at all). The ladder is
            // linear, so the horizontal degenerates onto it (the
            // recorded geometry; AC-1 accepts the per-geometry
            // reading) and both axes at Settings walk (r4 D19
            // behavior kept). The tail owns the rows.
            (ViewId::TaskDetail | ViewId::Settings | ViewId::Status, _) => Vec::new(),
            // The planner (J3 R-5): left/right switch the pane
            // (boxes ↔ pool); up/down arm and walk the K4 cursor over
            // the focused pane's rows (US-15, #1041 — the recorded
            // typed inert retires; R-9: the cursor rides the K4 state,
            // never a second cursor mechanism).
            (ViewId::PlanNextDay, Direction::Left) => self.view_state.switch_plan_pane(-1),
            (ViewId::PlanNextDay, Direction::Right) => self.view_state.switch_plan_pane(1),
            (ViewId::PlanNextDay, Direction::Up | Direction::Down) => {
                let step = i32::from(direction == Direction::Down) * 2 - 1;
                self.walk_plan_cursor(step)
            }
            // Review: no `NavFocus*` preset rows are bound in its
            // context (the CONTROL_NAV set excludes it) — the
            // explicit exclusion; the surface keeps its own cursors.
            (ViewId::Review, _) => Vec::new(),
        }
    }

    /// The move flows' arms (D8/D9; the round-2 stack move joins),
    /// factored out of the table.
    pub(in crate::adapter) fn interpret_move(
        &mut self,
        flow: &Flow,
        effect: UiEffect,
        state: &taskboard_domain::AppState,
        window: &AppWindow,
    ) -> Vec<FlowOutcome> {
        match (flow, effect) {
            // Focus next/prev cycle the target; Activate confirms;
            // every cancel-shaped effect (the dedicated verb — Escape
            // / emacs C-g —, the toggle re-request, and
            // SetMode(Normal), which no longer falls through to the
            // inert default leaving the flow intact) ends the flow
            // without dispatching (D16/F-E2). The directional
            // up/down kept arm (J3 US-2, round 9 chunk 9) rides the
            // same ring byte-identically.
            (
                _,
                effect @ (UiEffect::FocusNext
                | UiEffect::FocusPrev
                | UiEffect::FocusDirectional(Direction::Up | Direction::Down)),
            ) => {
                let direction = match effect {
                    UiEffect::FocusNext | UiEffect::FocusDirectional(Direction::Down) => 1,
                    _ => -1,
                };
                let outcomes = match flow {
                    Flow::Move { .. } => self.view_state.cycle_move_target(state, direction),
                    _ => self.view_state.cycle_move_stack_target(state, direction),
                };
                // The walk's scroll-into-view half (UX-R8 D-3, US-1/S2):
                // the aimed column's viewport seeks the aim point —
                // `target-a` = aimed column, `target-b` = the gap's
                // clamp row (`min(gap, len-1)`), nonce re-edge. The
                // hint filters per-ListView (both legs read it); no
                // new hint family.
                if !outcomes.is_empty() {
                    self.push_move_scroll_hint(window, state);
                }
                outcomes
            }
            // The directional left/right kept arms (J3 US-2): during
            // a card move the horizontal keys hop the drop *column*
            // (`cycle_move_stack_column` — the row index survives,
            // clamped); during a stack move they hop the insertion
            // slot (the same ring the FocusNext/Prev kept arm runs —
            // the slot domain is horizontal there). The move survives
            // every hop (US-2/AC-1). #1033: the legacy stack-walk
            // verbs (`StackNext`/`StackPrev` — the `g l`/`g h` ViNormal
            // chords and the Standard/emacs `Ctrl+Shift+Left/Right`
            // rows in `actions.rs`) join the kept arms with the same
            // reading: `StackNext` → +1, `StackPrev` → -1. The
            // `(Flow::None, StackNext/Prev)` arms below stay verbatim
            // — outside a move flow the stack walk steps as before.
            (
                _,
                effect @ (UiEffect::FocusDirectional(Direction::Left | Direction::Right)
                | UiEffect::StackNext
                | UiEffect::StackPrev),
            ) => {
                let direction = match effect {
                    UiEffect::FocusDirectional(Direction::Right) | UiEffect::StackNext => 1,
                    _ => -1,
                };
                let outcomes = match flow {
                    Flow::Move { .. } => self.view_state.cycle_move_stack_column(state, direction),
                    _ => self.view_state.cycle_move_stack_target(state, direction),
                };
                if !outcomes.is_empty() {
                    self.push_move_scroll_hint(window, state);
                }
                outcomes
            }
            (Flow::Move { .. }, UiEffect::Activate) => self.view_state.confirm_move(state),
            (Flow::MoveStack { .. }, UiEffect::Activate) => {
                self.view_state.confirm_move_stack(state)
            }
            _ => self.view_state.cancel_move(),
        }
    }

    /// The lifecycle verbs' arms (round 2, D18/D21), factored out of
    /// the table to keep it readable: deletes dispatch through the
    /// view state (the engine owns every typed refusal), renames and
    /// the board create open their fields, the stack move toggles.
    pub(in crate::adapter) fn interpret_lifecycle(
        &mut self,
        flow: &Flow,
        effect: UiEffect,
        state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        match effect {
            UiEffect::StackDelete => self.view_state.delete_stack(state),
            UiEffect::StackRename => self.view_state.begin_rename_stack(state),
            UiEffect::StackMove => match flow {
                Flow::None => self.view_state.begin_move_stack(state),
                Flow::MoveStack { .. } => self.view_state.cancel_move(), // toggle
                _ => Vec::new(),
            },
            UiEffect::BoardCreate => self.view_state.begin_create_board(),
            UiEffect::BoardDelete => self.view_state.delete_board(state),
            UiEffect::BoardRename => self.view_state.begin_rename_board(state),
            UiEffect::BoardColorEdit => self.view_state.begin_board_color(state),
            UiEffect::BoardDefaultPreset => {
                // The dispatch-time empty-board gate (UX R-4 D-5,
                // #795): the CTA renders only on the empty-board leg,
                // so the pointer path cannot fire on content — the
                // key could; the typed no-op is the safe side (a sync
                // refresh mid-stroke flips the verdict to inert, the
                // recorded R-3 law). The board-less refusal stays
                // apply_board_preset's.
                let Some(board) = self.view_state.board() else {
                    return Vec::new();
                };
                let empty = !state
                    .stacks
                    .values()
                    .any(|stack| stack.board == board && !stack.deleted);
                if empty {
                    self.view_state
                        .apply_board_preset(state, &crate::board_presets::DEFAULT_KANBAN)
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    /// The board ceremony verbs' arms (OP1, US-1/US-2/US-3): the view
    /// state owns the arm/confirm split (the `interpret_lifecycle`
    /// shape); the reconcile arm additionally sends the oneshot
    /// preview request when the arm is fresh — the reply drains into
    /// the flow's `ReconcilePreviewState` under the repaint cadence.
    pub(in crate::adapter) fn interpret_ceremony(
        &mut self,
        effect: UiEffect,
        state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        // The kiosk gate (OP1 D-5, the structural exclusion): the
        // board surface does not mount on kiosk (board-stack plan
        // PC-4 §6), so the verbs are absent there — no armed letter,
        // no preview request, no engine command (the
        // `TrackEntriesOpen` refusal shape; no disabled affordances
        // are minted for a surface that does not exist).
        if self.kiosk {
            return Vec::new();
        }
        match effect {
            UiEffect::BoardArchive => self.view_state.begin_board_archive(state),
            UiEffect::BoardReset => self.view_state.begin_board_reset(state),
            UiEffect::BoardReconcile => {
                let outcomes = self.view_state.begin_board_reconcile(state);
                if let crate::view_state::Flow::ConfirmBoardReconcile {
                    board,
                    preview: crate::view_state::ReconcilePreviewState::Pending,
                } = self.view_state.flow().clone()
                    && self
                        .reconcile_preview
                        .as_ref()
                        .is_none_or(|(pending, _)| *pending == board)
                {
                    let (reply, reply_rx) = tokio::sync::oneshot::channel();
                    let request =
                        crate::requests::UiRequest::BoardReconcilePreview { board, reply };
                    if self.requests.try_send(request).is_ok() {
                        self.reconcile_preview = Some((board, reply_rx));
                    } else {
                        // The lane is full or closed (degraded app):
                        // the strip stays armed in Pending — the
                        // unlettered confirm is refused view-state
                        // side (D-4's staging law), and the user's
                        // Escape/Cancel disarms. The typed failure
                        // fact is the dead reply lane's (the drain's
                        // Failed arm), not the send's.
                        tracing::warn!(
                            "request channel full or closed; the reconcile preview was dropped"
                        );
                    }
                }
                outcomes
            }
            _ => Vec::new(),
        }
    }

    /// The `OpenMenu` settle hook (audit §5): the menu effects are
    /// applied in `apply_effect`, before the table — so the matrix's
    /// "`OpenMenu` settles a foreign flow" cell (the hamburger face
    /// of F9) lives here, at the application layer. The menu-internal
    /// effects run through unchanged (they only arrive while the
    /// menu is open, and an open menu masks flows via the chain).
    pub(in crate::adapter) fn settle_foreign_flow_for_modal(&mut self, window: &AppWindow) {
        let state = self.state.load();
        self.settle_flow(window, &state);
    }

    /// The flow kind the matrix rows range over (audit §6).
    pub(in crate::adapter) fn flow_kind(flow: &Flow) -> FlowKind {
        match flow {
            // The modals (the concentration overlay #496; the entries
            // overlay's two surfaces, r4 chunk 10) and the empty flow
            // read the same: the matrix's overlay cells cover them —
            // the kept arms own their opens/closes, settle leaves the
            // overlay (its close paths are its exits).
            Flow::FocusMode { .. }
            | Flow::TrackEntries { .. }
            | Flow::TrackEntryForm { .. }
            // The labels overlay (UX R-3 D-2, #789): the modals' row
            // — its kept arms own the open/dismiss; settle is its
            // close (the overlay family's row).
            | Flow::Labels { .. }
            // The color picker (UX R-4 D-1, #791): the armed-strip's
            // row — settle is its disarm; the strip's seams own the
            // commit.
            | Flow::BoardColorPick { .. }
            // The placement form (#622): the kept planner arms own
            // its intents and settle leaves the form (its exits are
            // its close paths) — the overlay cells' shape.
            | Flow::PlanPlace { .. }
            // The review walk (#638): the fifth modal — its kept arms
            // own the walk's intents; settle is its close (the
            // palette family's terminator).
            | Flow::ReviewWalk { .. }
            // The embedded planner child (J4 D-2): the modals' row —
            // its close paths are its exits (the Cancelled/Completed
            // exit contract advances the walk's stage through
            // `on_child_exit`).
            | Flow::ReviewPlanner
            // The recurrence editor (round-7 D-7.1): the sixth modal —
            // settle is its clean close (the overlay family's row; a
            // foreign settle never commits a draft).
            | Flow::EditRecur { .. }
            // The child levels (chunk 3's ports): the modals' row —
            // the children have no kept intents of their own; their
            // close paths are their exits and their settle arm is the
            // Cancelled end (D5).
            | Flow::EntryDatePicker
            | Flow::WalkFilePick { .. }
            | Flow::WalkDatePick
            // The armed-delete strip (UX R-2 D-2): the modals' row —
            // settle is its disarm (nothing dispatches); its kept
            // intents own the confirm/re-arm.
            | Flow::ConfirmDelete(_)
            // The board ceremony strips (OP1): the armed-delete's row
            // — the foreign settle resolves through the view state's
            // cancel arms; the strips' own seams own the confirm.
            | Flow::ConfirmBoardArchive { .. }
            | Flow::ConfirmBoardReset { .. }
            | Flow::ConfirmBoardReconcile { .. }
            | Flow::None => FlowKind::None,
            Flow::Create { .. }
            | Flow::CreateStack
            | Flow::CreateBoard
            | Flow::EditTitle { .. }
            // The description behaves as a text flow everywhere
            // (settle/leaving rules) — the kind vocabulary only.
            | Flow::EditDescription { .. }
            | Flow::RenameStack { .. }
            | Flow::RenameBoard { .. } => FlowKind::Text,
            Flow::EditDate { .. } => FlowKind::Due,
            Flow::Move { .. } | Flow::MoveStack { .. } => FlowKind::Move,
            Flow::DragMove { .. } => FlowKind::Drag,
            Flow::Palette { .. } => FlowKind::Palette,
            Flow::Capture => FlowKind::Capture,
        }
    }

    /// The R3/R4 conflict resolver (round-3 W-SM-b, plan §3): the
    /// named classifier that replaced the wildcard arm. Its match is
    /// exhaustive over [`EffectClass`] — the compiler enforces the
    /// totality, and the pair-table test pins every `(Flow, effect)`
    /// cell to the audit matrix.
    pub(in crate::adapter) fn resolve_conflict(
        &mut self,
        window: &AppWindow,
        flow: &Flow,
        effect: UiEffect,
        state: &taskboard_domain::AppState,
    ) {
        if matches!(flow, Flow::None) {
            // Unreachable from the offer walk: every effect class has
            // a `(Flow::None, _)` arm above, and the menu/system
            // effects are applied in `apply_effect` before the table
            // (audit §5). The pair-table harness drives this resolver
            // with the empty flow directly, so the guard stays.
            return;
        }
        match Self::resolve(Self::flow_kind(flow), effect.class()) {
            Resolve::Inert => {}
            Resolve::CancelOnly => {
                // Settle only — the effect is consumed by the settle
                // (the K cells: the due editor's C-g clean close, the
                // modals' cancel). The cancel IS the site's own exit
                // arm — the origin survives it and the restore fires.
                self.settle_flow(window, state);
            }
            Resolve::Replace => {
                // The one X: the capture overlay's fire-drill replace
                // without committing (chunk-10 D8).
                debug_assert!(matches!(effect, UiEffect::BeginCapture));
                // T-31 (#814): the replacement announces itself — the
                // typed fact arms before the overlay mounts (the
                // settle rule still bounds data-at-risk; the notice
                // adds only the announcement).
                self.arm_overlay_replacement_notice(window);
                let was_drag = matches!(flow, Flow::DragMove { .. });
                let outcomes = self.view_state.begin_capture(state);
                self.width_drag = None;
                self.scale_drag = None;
                self.apply_outcomes(window, &outcomes);
                if was_drag {
                    Self::teardown_runtime_drag(window);
                }
            }
            Resolve::SettleThenRun => {
                // The foreign settle hands the focus to the re-run
                // effect (the walk carries its own target) — the
                // census restore arms (US-1/W-1) belong to the site's
                // own exits, so the origin is dropped when the settle
                // clears the recorded site (the walk's continuation
                // keeps today's letter).
                let settled_site = self.open_focus_site(window);
                // Settle per the flow's kind rule, then re-run the
                // effect once against the settled state — bounded
                // one-level re-dispatch (the `InvokeSelected`
                // precedent, `router.rs`). The settle-nothing skip is
                // retired (r4 D4b): `resolve_conflict` early-returns
                // on `Flow::None`, so the gate was provably dead —
                // and it was the only shape that could silently eat a
                // re-dispatch if the kept-arm set ever shifted.
                // T-31 (#814): the palette's settle-then-run announces
                // the flow it displaced (the capture twin; only the
                // overlay-open class announces — the other
                // settle-then-run effects run in place).
                if matches!(effect, UiEffect::OpenPalette) {
                    self.arm_overlay_replacement_notice(window);
                }
                self.settle_flow(window, state);
                if self
                    .focus_origin
                    .as_ref()
                    .is_some_and(|origin| Some(origin.seam) == settled_site)
                {
                    self.focus_origin = None;
                }
                self.interpret(window, effect);
            }
        }
    }

    /// The overlay-over-live-flow announcement's arming (T-31, #814):
    /// the typed fact (a live flow was replaced) on the status-notice
    /// lane, captured before the overlay mounts.
    pub(in crate::adapter) fn arm_overlay_replacement_notice(&mut self, window: &AppWindow) {
        self.notice = Some(Notice {
            text: crate::presentation::flow_replacement_notice_phrase(),
            until: chrono::Utc::now() + crate::presentation::OUTCOME_NOTICE_HORIZON,
        });
        self.push_notice(window);
    }

    /// The matrix (audit §6) as a pure function: no silent default —
    /// each arm names the decision (the arms deliberately share
    /// bodies where the matrix cells agree).
    #[allow(clippy::match_same_arms)] // one arm per matrix decision
    pub(in crate::adapter) fn resolve(kind: FlowKind, class: EffectClass) -> Resolve {
        use EffectClass as C;
        use FlowKind as K;
        use Resolve as R;
        match (kind, class) {
            // The one Replace: the capture overlay's fire-drill (D8).
            (_, C::BeginCapture) => R::Replace,
            // CAPTURE row: one overlay — a palette request is a
            // named inert.
            (K::Capture, C::OpenPalette) => R::Inert,
            // CANCEL: clean close (K) everywhere a kept arm doesn't
            // own it; under a text flow the row's own seams own the
            // buffer, so C-g is a named inert there.
            (K::Text, C::Cancel) => R::Inert,
            (_, C::Cancel) => R::CancelOnly,
            // BEGIN/TITLE: R4 — dead buttons cannot exist; the
            // auto-settle-then-run is the visible feedback (the
            // keymap-parity D-4 editor arms share the cell).
            (_, C::Begin | C::Title) => R::SettleThenRun,
            // NAV / LIFECYCLE / overlay-open (palette, the focus
            // overlay, the entries overlay): R3 settle-then-run.
            (
                _,
                C::Nav
                | C::Lifecycle
                | C::OpenPalette
                | C::FocusOverlay
                | C::TrackOverlay
                | C::LabelsOverlay,
            ) => R::SettleThenRun,
            // The plan-next-day family (#622): a held flow keeps
            // every planner intent inert (the modal discipline
            // verbatim — Escape out of the flow first).
            (_, C::PlanSurface) => R::Inert,
            // The review walk family (#638): the overlay-family shape
            // — a foreign flow settles, then the walk opens over it
            // (R3 settle-then-run); the walk's kept intents are the
            // pair overrides.
            (_, C::ReviewWalk) => R::SettleThenRun,
            // EDIT residue (the effects the kept arms don't own —
            // SetMode(Insert) under a text/due flow): named inert.
            (_, C::Edit) => R::Inert,
            // MENU/SYSTEM/PICKER: applied in `apply_effect` before
            // the table (audit §5; the picker keys are the same
            // family, round-3 D25); unreachable here — typed inert,
            // never silent.
            (_, C::Menu | C::System | C::Picker) => R::Inert,
        }
    }
}

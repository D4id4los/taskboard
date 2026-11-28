// SPDX-License-Identifier: AGPL-3.0-only

use slint::ComponentHandle as _;

use taskboard_domain::{AppState, TaskId};

use super::Adapter;
use super::{HabitMoveSlots, MenuAnchor, MenuKind, MenuSubject, Notice};
use crate::AppWindow;
use crate::actions::{ActionId, ContextId};
use crate::adapter::seams::settle_pointer_conflict;
use crate::menu::{self};
use crate::router::UiEffect;
use crate::view_state::Flow;

impl Adapter {
    #[allow(clippy::too_many_lines)] // the effect application switch, kept flat
    pub(in crate::adapter) fn apply_effect(&mut self, window: &AppWindow, effect: UiEffect) {
        // The total-dismiss seam (r4 chunk 4, D14): an anchor that
        // unmounts while hovered never fires `unhover` — the callback
        // dies with the element. So the dismiss stops being a per-arm
        // memory and becomes a property of the pass: snapshot the
        // view identity at entry; if the pass swapped the mounted
        // view, board, or subject, the tooltip is gone. No effect —
        // present or future — can be forgotten, because there is
        // nothing left to remember. Identity-preserving effects
        // (focus steps, scrolls, the walk) survive by construction;
        // the structural walk test pins the partition.
        let before = self.view_state.view_identity();
        let palette_before = self.shell.palette;
        // The focus walk's single handoff (US-4/W-4, D-H4): a walk
        // step that lands on a row carries its own focus target — the
        // identity-diff tail's view-scope re-anchor is suppressed for
        // it (the double-handoff that parks the key root over the
        // walked row is the #670 fix). `Some → Some` only: a retreat
        // to the switcher (Some → None) and every other identity
        // change keep the tail call.
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
        match effect {
            UiEffect::Exit => {
                self.quit_requests += 1;
                let _ = crate::quit();
            }
            UiEffect::UndoBranchesOpen => {
                // The kiosk structural exclusion (R-2): the entry
                // renders absent, the effect is inert (the
                // TrackEntriesOpen shape; the desktop-qualified E).
                if self.kiosk {
                    return;
                }
                // The picker mount (UF2 W-1): the modal-takeover
                // trio — settle any foreign flow, kill the tooltip,
                // reset the editing facts (the `OpenMenu` arm's
                // exact shape) — then mount. Pure navigation: no
                // command dispatch on open (the plan's W-1 row).
                self.settle_foreign_flow_for_modal(window);
                Self::dismiss_tooltip(window);
                Self::reset_editing_facts(window);
                // The picker's selectable-row count (#1227, I-1): the
                // two reflex rows plus the projection's rows — the
                // cursor machinery's wrapping bound (the shell stores
                // it at mount).
                let picker_len = {
                    let undo = &self.state.load_full().undo;
                    2 + undo.recent.len() + undo.branch_children.len()
                };
                self.shell.apply(&effect, picker_len);
                self.sync_menu_models(window);
                self.sync_undo_picker_models(window);
            }
            UiEffect::OpenMenu => {
                // Round 3 (the matrix's OpenMenu cell, audit §5): a
                // foreign flow settles first — the modal must not
                // sit on an armed flow that keeps masking keys after
                // dismiss (F9's hamburger face).
                self.settle_foreign_flow_for_modal(window);
                // The menu is the modal context now (r2 D13): no tip
                // survives the open.
                Self::dismiss_tooltip(window);
                // The D20 reset seam 3 (modal opens): the reservation
                // and the mask must not outlive the surface that
                // armed them.
                Self::reset_editing_facts(window);
                // The projection is computed for the pre-menu chain
                // (the active context's actions, §5.9); the menu
                // context itself masks the chain once open (D11).
                let chain = self.current_chain(window);
                self.menu_anchor = MenuAnchor::Hamburger;
                self.menu_chain = chain;
                {
                    let state = self.state.load_full();
                    let flow = self.view_state.flow().clone();
                    let states = self.entry_states(&state, &flow);
                    self.root_entries = self.mint_root_entries(&states);
                }
                self.menu_entries = self.root_entries.clone();
                self.shell.apply(&effect, self.menu_entries.len());
                self.sync_menu_models(window);
            }
            UiEffect::InvokeSelected => {
                // Palette first (defensively — `Palette` masks `Global`,
                // so the menu cannot be open while the flow holds):
                // Activate is jump-to.
                if self.shell.palette {
                    self.apply_palette_invoke(window);
                } else if let Some(index) = self.shell.picker_selected() {
                    // The picker's invoke (#1227): the cursor's row
                    // routes through the exact seams the click path
                    // uses (US-4/AC-2: invoke ≡ click, byte-identical
                    // dispatch) — the close-then-dispatch shape via
                    // `undo_picker_commit`.
                    self.shell.apply(&UiEffect::InvokeSelected, 0);
                    self.invoke_picker_row(window, index);
                } else if let Some(index) = self.shell.selected_entry() {
                    // Close first, then one re-dispatch — bounded
                    // (D16); the close itself lives in `invoke_entry`
                    // so the mouse path shares it (round-1 D3).
                    self.invoke_entry(window, index);
                } else {
                    self.apply_close_menu(window);
                }
            }
            UiEffect::CloseMenu => {
                if self.shell.palette {
                    let outcomes = self.view_state.close_palette();
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                } else {
                    self.apply_close_menu(window);
                }
                // The picker's dismissal shares the close (the AC-4
                // law; the shell cleared the bit).
                self.sync_undo_picker_models(window);
            }
            UiEffect::MenuBack => {
                if self.shell.undo_picker {
                    // AC-4 (UF2): the picker's Escape closes with
                    // nothing dispatched (it is a pure selection
                    // menu).
                    self.shell.undo_picker = false;
                    self.sync_undo_picker_models(window);
                } else if self.shell.notification_surface {
                    // The surface's Escape (US-6/W-6, D-N1): closes
                    // with nothing dispatched (rows keep their
                    // delivery state) and restores per the focus
                    // contract (the census row 12).
                    self.notification_cursor = None;
                    window.set_notification_selected(-1);
                    self.sync_view_context(window);
                    if !self.exit_focus_handoff(window) {
                        window.invoke_refocus_view();
                    }
                } else if self.shell.palette {
                    // Esc / dismiss / mouse-out all mean
                    // close-without-effect (the discard_edit shape).
                    let outcomes = self.view_state.close_palette();
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                } else {
                    // The Escape ladder (UX-R8 chunk-5 D-5, US-2/AC-4):
                    // a task-family submenu steps down first, then a
                    // main group's page, then the close — Escape 1
                    // closes the submenu, Escape 2 the menu.
                    let was_submenu = self.active_submenu().is_some();
                    self.shell.apply(&effect, self.root_entries.len());
                    if was_submenu {
                        // Restored to the root page: the active page is
                        // the root page again.
                        self.menu_entries = self.root_entries.clone();
                        self.sync_menu_models(window);
                        self.finish_mutation(window);
                    } else {
                        // The ladder's close leg (#1115): the root's
                        // MenuBack is the full close — it funnels
                        // through `apply_close_menu` (#1328, the
                        // helper's reset-at-every-close law), which
                        // carries the same one-shot hygiene this leg
                        // applied inline (#1115's staging disarm, the
                        // #1303 focus-fact clears) plus the page-model
                        // resets (`menu_entries`/`root_entries`/
                        // `menu_chain`/`menu_kind`/`menu_subject`/
                        // quick bar) the inline shape missed. The
                        // disarm order is load-bearing and preserved:
                        // the shell close first, then the helper's
                        // model reset → staging disarm → sync.
                        self.apply_close_menu(window);
                    }
                }
            }
            UiEffect::NotificationVerb(verb) => {
                // The notification verbs (US-6/W-6, D-N1): each routes
                // exactly the seam body the row's button callback
                // runs — the click ≡ key law (us6_s2 pins the
                // structural equality). A verb stroke arms the walk
                // at row 0 (the AC-1 pre-selection).
                self.apply_notification_verb(window, verb);
            }
            UiEffect::MenuNext | UiEffect::MenuPrevious => {
                if self.shell.notification_surface {
                    // The notification walk (US-6/W-6, D-N1): the r5
                    // grammar — armed, the step clamps at both ends
                    // (no wrap).
                    let direction: i32 = if matches!(effect, UiEffect::MenuNext) {
                        1
                    } else {
                        -1
                    };
                    self.notification_walk(window, direction);
                } else if self.shell.palette {
                    let visible = self.tables.palette.len();
                    let direction = matches!(effect, UiEffect::MenuNext);
                    let outcomes = self
                        .view_state
                        .palette_cursor(i32::from(direction) * 2 - 1, visible);
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                } else if matches!(self.view_state.flow(), Flow::FocusMode { .. }) {
                    // The focus overlay's switcher (the FocusMode
                    // preset's Down/Up): the palette-cursor ring over
                    // the rendered rows.
                    let visible = slint::Model::row_count(&window.get_focus_switcher_model());
                    let direction = matches!(effect, UiEffect::MenuNext);
                    let outcomes = self
                        .view_state
                        .focus_mode_cursor(i32::from(direction) * 2 - 1, visible);
                    self.apply_outcomes(window, &outcomes);
                    self.finish_mutation(window);
                } else if self.shell.undo_picker {
                    // The picker cursor (#1227, I-1): the shell's
                    // menu-cursor machinery moves over the picker's
                    // row count (stored at mount); the sync mirrors
                    // the cursor onto the window global.
                    self.shell.apply(&effect, self.shell.menu.len);
                    self.sync_undo_picker_models(window);
                } else {
                    self.shell.apply(&effect, self.menu_entries.len());
                }
            }
            UiEffect::OpenBoardBarFlyout => {
                // The target-free BoardSelect flow (UX R-4 D-4, #794):
                // arm the switcher flyout directly over the facts —
                // the window-level panel mounts from them (the
                // site-1 group), the walk rides the adapter's key arm,
                // and the anchor geometry is the bar's mirrored
                // mirror. Zero live boards: inert (no rows to walk).
                // Pre-table, view-state blind — the tooltip tail and
                // the settle matrix do not apply.
                let facts = window.global::<crate::BoardBarFacts>();
                let rows = i32::try_from(slint::Model::row_count(&window.get_board_bar_model().rows)).unwrap_or(i32::MAX);
                if rows > 0 {
                    let selected = window.get_board_bar_model().selected;
                    facts.set_highlight(selected.clamp(0, rows - 1));
                    facts.set_switcher_request(true);
                    facts.set_open(true);
                    let dd = window.global::<crate::DropdownFacts>();
                    dd.set_consumer(crate::DropdownConsumer::BoardSwitcher);
                    dd.set_anchor_x(facts.get_anchor_x());
                    dd.set_anchor_y(facts.get_anchor_y());
                    dd.set_anchor_width(facts.get_anchor_width());
                    dd.set_anchor_height(facts.get_anchor_height());
                    dd.set_open(true);
                }
                return;
            }
            UiEffect::PickerKey(key) => self.apply_picker_key(window, key),

            UiEffect::OpenPalette
            | UiEffect::SetMode(_)
            | UiEffect::FocusNext
            | UiEffect::FocusPrev
            | UiEffect::NextView
            | UiEffect::PrevView
            | UiEffect::FocusBoard
            | UiEffect::ShowTask
            | UiEffect::ControlNext
            | UiEffect::ControlPrev
            | UiEffect::ScrollUp
            | UiEffect::ScrollDown
            | UiEffect::BeginCreate { .. }
            | UiEffect::BeginCreateStack
            | UiEffect::BeginTitleEdit
            // The detail editors' arm verbs (keymap-parity plan D-4)
            // interpret like every begin intent — the funnel refusals
            // live in the arms.
            | UiEffect::BeginDateEdit { .. }
            | UiEffect::BeginDescriptionEdit
            | UiEffect::BeginEffortEdit
            | UiEffect::Activate
            | UiEffect::RequestMove
            | UiEffect::StackWider
            | UiEffect::StackNarrower
            | UiEffect::OpenSettings
            // The status panel's toggle (OP3 W-1) interprets like the
            // settings opener — its kept arm owns the view swap.
            | UiEffect::OpenStatusPanel
            | UiEffect::BoardNext
            | UiEffect::BoardPrevious
            | UiEffect::BoardSelect(_)
            // Round 2 (D16–D21): the round-up verbs interpret like
            // every other view intent; their (flow, effect) arms land
            // with the view-state commits.
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
            // The board ceremony verbs (OP1): interpret like every
            // view intent — the arm/confirm split and the kiosk
            // refusal live in the arms.
            | UiEffect::BoardArchive
            | UiEffect::BoardReset
            | UiEffect::BoardReconcile
            | UiEffect::FocusFirst
            | UiEffect::FocusLast
            // The directional-focus family (J3 US-1, round 9 chunk 9)
            // and its extend twin (#875) interpret like every view
            // intent — the per-view readings live in the table.
            | UiEffect::FocusDirectional(_)
            | UiEffect::ExtendSelectionNext
            | UiEffect::ExtendSelectionPrev
            // Chunk 10: the today/inbox jumps and capture route
            // through the same interpretation table. The focus
            // overlay's toggle joins them (#496) — the kiosk no-op
            // lives in its interpret arm.
            | UiEffect::FocusToday
            | UiEffect::OpenInbox
            | UiEffect::BeginCapture
            | UiEffect::ToggleFocusMode
            // The entries overlay's open request and its Escape
            // ladder interpret like every view intent (r4 chunk 10,
            // D-TS2) — the kiosk/subject refusals live in the arms;
            // the panel's focus hand-off is the composition's
            // changed-height grab (the walk probe's P5).
            | UiEffect::TrackEntriesOpen
            | UiEffect::TrackEntryDismiss
            | UiEffect::LabelsOpen
            | UiEffect::LabelsDismiss
            // The habit trio's intents interpret like every view
            // intent (issue #506) — the kiosk/subject refusals live
            // in the arms.
            | UiEffect::SeriesPauseOpen
            // The row-menu opener (US-7, W-8, #1032) interprets like
            // every view intent — the subjectless refusal lives in
            // the arm.
            | UiEffect::RowMenuRequested
            // The rule cursor's disarm chord (US-10, W-11, #1091)
            // interprets like every view intent — the typed inert
            // lives in the arm.
            | UiEffect::RowCursorDisarm
            // The roster remove's arm step (US-9, W-10) interprets
            // like every view intent — the cursorless refusal lives
            // in the arm.
            | UiEffect::BreakRuleRemoveArm
            | UiEffect::HabitPauseOpen
            | UiEffect::HabitConvertArm
            | UiEffect::HabitConvertCancel
            | UiEffect::HabitRemoveArm
            | UiEffect::HabitRemoveCancel
            | UiEffect::HabitCreateToggle
            | UiEffect::SeriesConvertArm
            // The H4 editor opener interprets like every view intent
            // (the kiosk/subject refusals live in the arm).
            | UiEffect::HabitEdit
            | UiEffect::HabitQuotaEdit
            // The HF3 history opener interprets like every view
            // intent (the kiosk/subject refusals live in the arm).
            | UiEffect::HabitHistoryOpen
            // The recur editor's intents (round-7 D-7.1): the opener
            // and the detach strip's flag flips interpret like every
            // view intent — the kiosk/subject refusals live in the
            // arms.
            | UiEffect::SeriesEditorOpen
            | UiEffect::SeriesDetachArm
            | UiEffect::SeriesDetachCancel
            // The priority pickers' openers (UX R-3 D-1, #788) — the
            // kiosk/no-subject refusals live in the arms.
            | UiEffect::PriorityPickerOpen(_)
            // The plan-next-day family (#622): the surface's intents
            // interpret like every view intent — the kiosk/modal
            // refusals live in the arms.
            | UiEffect::OpenPlanNextDay
            | UiEffect::PlanBeginPlace(_)
            | UiEffect::PlanCommitPlace
            | UiEffect::PlanBoxEarlier(_)
            | UiEffect::PlanBoxLater(_)
            | UiEffect::PlanRemoveBox(_)
            // The review walk family (#638): the opener and the
            // carrier intents interpret like every view intent — the
            // kiosk refusal and the walk arms live in the table.
            | UiEffect::OpenReviewWalk
            | UiEffect::ReviewWalk(_)
            // The export dialog's pair (the J2 export plan W-1): the
            // interpretation table owns the arms — the mount bit is
            // the adapter's VM.
            | UiEffect::ExportDialogOpen
            | UiEffect::ExportDialogClose => self.interpret(window, effect),
        }
        // The pass's tail: the identity diff decides (see the entry
        // comment). The explicit dismisses below (the menu's
        // modal-takeover, the ghost/modal takeovers) are idempotent
        // under this — an explicit call names a non-identity reason
        // (a hovered element that unmounts, or a modal that takes
        // over, without the mounted view moving).
        if self.view_state.view_identity() != before {
            Self::dismiss_tooltip(window);
            // The D20 reset seam 2 (view swaps): the same unmount
            // hazard as the flow end, one enumerate-once tail.
            Self::reset_editing_facts(window);
            // The swap unmounts whatever held slint focus (and any
            // stale focus must not survive into the new surface) — the
            // D15 hand-off rides the swap as it rode every dispatch
            // before the r4 chunk-6 gating. The focus-origin restore
            // runs first (US-1/W-1, D-F1's ordering law): the
            // view-scope call only when nothing element-level resolved.
            // The single handoff (US-4/W-4, D-H4): a walk step that
            // landed on a row carries its own focus target — the
            // view-scope re-anchor is suppressed for it (the
            // double-handoff that parks the key root over the walked
            // row is the #670 fix). A retreat to the switcher
            // (Some → None) and every other identity change keep it.
            let walk_carried = focus_walk && self.view_state.focused_task().is_some();
            if !self.exit_focus_handoff(window) && !walk_carried {
                window.invoke_refocus_view();
            }
        }
        if self.shell.palette && !palette_before {
            // The D20 reset seam 3's palette leg: the modal surface
            // refocuses its own field (which re-publishes); the armed
            // facts must not outlive what armed them.
            Self::reset_editing_facts(window);
        }
    }

    /// The full close shared by every path that means "close the
    /// menu": the shell state and both page models clear (round-1 D6).
    /// The palette's Activate (keyboard `menu.invoke` or a row
    /// click): resolve `tables.palette[selected]` against the fresh
    /// snapshot and jump; a stale index is inert (close only).
    pub(in crate::adapter) fn apply_palette_invoke(&mut self, window: &AppWindow) {
        let selected = match self.view_state.flow() {
            crate::view_state::Flow::Palette { selected } => *selected,
            _ => 0,
        };
        let before = self.view_state.walk_focus();
        let target = self.tables.palette.get(selected).copied();
        let state = self.state.load();
        let outcomes = match target {
            Some(target) => self
                .view_state
                .palette_jump(&state, target, self.today_date()),
            // A row count that shrank past the cursor: close only.
            None => self.view_state.close_palette(),
        };
        self.apply_outcomes(window, &outcomes);
        // Round-3 D23 §3.3: the invoke's retarget arms the scroll
        // hint the same way the walk effects do (a stack-target jump
        // emits its StackDetail hint here; an inert close re-arms
        // nothing — the unchanged-focus guard holds).
        self.push_focus_scroll_hint(window, before);
        self.finish_mutation(window);
    }

    /// The per-keystroke palette pass (design D10): mirror the buffer,
    /// reset the cursor (every keystroke re-ranks — a stale mid-list
    /// cursor on a fresh ranking is a lie), then the one targeted
    /// [`crate::models::push_palette`] call — no full refresh, no
    /// debounce.
    pub(in crate::adapter) fn apply_palette_query(&mut self, window: &AppWindow, text: &str) {
        self.view_state.set_buffer(text.to_owned());
        if let Some(crate::view_state::Flow::Palette { selected }) = self.view_state.flow_mut() {
            *selected = 0;
        }
        let state = self.state.load();
        self.tables.palette =
            crate::models::push_palette(window, &state, &self.view_state, &mut self.models);
        self.finish_mutation(window);
    }

    /// The disarm reconcile's predicate (J5 US-4 AC-2, #927): the
    /// pinned single subject no longer resolves (a soft-deleted task
    /// stays in the map — liveness is the `deleted` bit, the same
    /// reading the id tables derive), or a snapshotted selection lost
    /// every member. Partial losses keep the menu (its rows
    /// dispatch-time-gate per task, the stale-absorb law).
    pub(in crate::adapter) fn menu_subject_dead(&self, state: &AppState) -> bool {
        let live = |id: &TaskId| state.tasks.get(id).is_some_and(|task| !task.deleted);
        match &self.menu_subject {
            MenuSubject::None => false,
            MenuSubject::Single(id) => !live(id),
            MenuSubject::Selection(members) => members.iter().all(|member| !live(member)),
        }
    }

    pub(in crate::adapter) fn apply_close_menu(&mut self, window: &AppWindow) {
        // Every close (the effect, the dismissal layer, the toggle,
        // and both mouse leaf paths sharing it) dismisses the tooltip
        // (r2 D13).
        Self::dismiss_tooltip(window);
        self.shell.apply(&UiEffect::CloseMenu, 0);
        self.menu_entries.clear();
        self.root_entries.clear();
        self.menu_chain.clear();
        self.menu_anchor = MenuAnchor::Hamburger;
        self.menu_kind = MenuKind::Main;
        self.menu_subject = MenuSubject::None;
        // The one-shot hygiene (#1303's disarm law): the opener's
        // focus facts die with the menu — a surviving
        // `focused_habit`/`menu_row_subject` would answer a later,
        // unrelated dispatch with a stale habit (the wrong-subject
        // editor). The menu-row route re-arms them for the ONE
        // dispatch (`route_menu_action`); a plain close keeps them
        // dead. The engine-sync re-derivation reads
        // `menu_row_subject` only while the menu is OPEN (the
        // main-menu page rebuild) — a close fires on a closed menu,
        // so the clear is safe there.
        self.focused_habit = None;
        self.menu_row_subject = None;
        self.quick_bar_entries.clear();
        // One rule (#1115): a menu close disarms every habit confirm
        // staging — the armed one-shot must not survive a hidden menu
        // (a later confirm on a closed menu is exactly the #1059
        // armed-confirm grammar's failure mode). The step-down leg of
        // `MenuBack` deliberately does NOT join: the menu is still
        // open there.
        self.disarm_habit_confirm_staging();
        self.sync_menu_models(window);
        // The direct callers (toggle, dismissal layer, mouse invoke)
        // bypass `route`'s trailing sync; the shared helper keeps the
        // globals identical to it (r2 D12).
        self.finish_mutation(window);
    }

    /// Activating a parent row (round-1 D6, keyboard `InvokeSelected`
    /// or a root-panel click): park the root cursor, open the group's
    /// page as the active one. A parent row never dispatches. UX-R8
    /// chunk-5 D-5: a task-family parent mounts the family's curated
    /// child page (`shell.menu.task_group` parks beside
    /// `active_group`; the Escape ladder consults it first).
    #[allow(clippy::too_many_lines)] // the two submenu kinds, kept flat
    pub(in crate::adapter) fn activate_submenu(
        &mut self,
        window: &AppWindow,
        submenu: crate::menu::Submenu,
    ) {
        self.shell.menu.root_selected = self.shell.menu.selected;
        self.shell.menu.selected = 0;
        match submenu {
            crate::menu::Submenu::Group(group) => {
                self.shell.menu.active_group = Some(group);
                self.shell.menu.task_group = None;
                {
                    let state = self.state.load_full();
                    let flow = self.view_state.flow().clone();
                    let states = self.entry_states(&state, &flow);
                    self.menu_entries =
                        menu::group_page(&self.keymap, &self.menu_chain, &states, group);
                }
            }
            crate::menu::Submenu::Task(family) => {
                self.shell.menu.task_group = Some(family);
                self.shell.menu.active_group = None;
                if matches!(
                    self.menu_row_subject,
                    Some(crate::presentation::RowSubject::Habit(_))
                ) {
                    self.menu_entries = self.habit_submenu_page(family);
                } else {
                    let subject = self.current_task_subject();
                    {
                        let state = self.state.load_full();
                        let flow = self.view_state.flow().clone();
                        let states = self.entry_states(&state, &flow);
                        self.menu_entries = menu::task_submenu_page(
                            &self.keymap,
                            &self.menu_chain,
                            subject,
                            &states,
                            family,
                        );
                    }
                }
            }
        }
        self.shell.menu.len = self.menu_entries.len();
        self.sync_menu_models(window);
        // The F7 fix (r2 D12): the mouse parent path pushes the same
        // globals the keyboard path pushes — `menu-active-group` and
        // friends — so the fly-out/replace page actually mounts.
        self.finish_mutation(window);
    }

    /// The ACTIVE page's invocation (keyboard `menu.invoke`, and the
    /// rendered page's mouse rows): a parent row opens its group's
    /// page, a leaf closes the menu and re-dispatches through the same
    /// router (round-1 D3: close first, route second).
    pub(in crate::adapter) fn invoke_entry(&mut self, window: &AppWindow, index: usize) {
        let page = self.menu_entries.clone();
        let Some(entry) = page.get(index) else {
            return;
        };
        if let Some(sub) = entry.submenu {
            self.activate_submenu(window, sub);
            return;
        }
        let Some(action) = entry.action else {
            return;
        };
        // T-19 (#802): the doctrine's letter — every click reacts. A
        // disabled row's invocation sets the status-line notice built
        // from the row's own registry description (the why-text, the
        // modality-neutral channel) instead of absorbing into the
        // router's Disabled arm.
        if !entry.enabled {
            self.arm_disabled_menu_notice(entry.description);
            self.push_notice(window);
            return;
        }
        self.route_menu_action(window, action);
    }

    /// The refused-commit flag's push (#1221): the field and the
    /// window mirror move together — the edit seams' `refresh_forced`
    /// pass does not run the shell `sync`, so the mutation sites push
    /// the half themselves.
    pub(in crate::adapter) fn set_recur_draft_invalid(
        &mut self,
        window: &AppWindow,
        invalid: bool,
    ) {
        self.recur_draft_invalid = invalid;
        window.set_recur_draft_invalid(invalid);
    }

    /// The habit confirm staging's disarm (#1115): a menu close
    /// clears all three armed one-shots (convert, remove, restore) —
    /// the one-subject law's close leg.
    pub(in crate::adapter) fn disarm_habit_confirm_staging(&mut self) {
        self.habit_convert_armed = None;
        self.habit_remove_armed = None;
        self.habit_restore_armed = None;
    }

    /// The dispatch-outcome notice's arming (T-41/T-43, #824/#826):
    /// the habit-log and created-timer commands echo their typed facts
    /// onto the status-notice lane (the +1's surplus delta; the
    /// created occurrence's target). `true` when a notice armed.
    pub(in crate::adapter) fn arm_outcome_notice(
        &mut self,
        command: &taskboard_domain::StateCommand,
    ) -> bool {
        match command {
            taskboard_domain::StateCommand::LogHabit { .. } => {
                self.notice = Some(Notice {
                    text: crate::presentation::habit_log_notice_phrase(),
                    until: chrono::Utc::now() + crate::presentation::OUTCOME_NOTICE_HORIZON,
                });
                true
            }
            // HF1 (US-1/S2): the skip's acknowledgement rides the same
            // lane — the chip renders the standing state (US-1/S3),
            // the notice is the dispatch-time event stamp.
            taskboard_domain::StateCommand::SkipHabit { .. } => {
                self.notice = Some(Notice {
                    text: crate::presentation::habit_skip_notice_phrase(),
                    until: chrono::Utc::now() + crate::presentation::OUTCOME_NOTICE_HORIZON,
                });
                true
            }
            taskboard_domain::StateCommand::CreateAdhocAlarm { label, when, .. } => {
                self.notice = Some(Notice {
                    text: crate::presentation::adhoc_alarm_notice_phrase(label, when),
                    until: chrono::Utc::now() + crate::presentation::OUTCOME_NOTICE_HORIZON,
                });
                true
            }
            _ => false,
        }
    }

    /// The disabled-row notice's arming (T-19, #802): the typed fact
    /// (entry disabled) plus the registry description, captured at
    /// event time (the resurrect notice's event-stamp shape).
    pub(in crate::adapter) fn arm_disabled_menu_notice(&mut self, description: &str) {
        self.notice = Some(Notice {
            text: crate::presentation::disabled_menu_notice_phrase(description),
            until: chrono::Utc::now() + crate::presentation::MENU_NOTICE_HORIZON,
        });
    }

    /// The menu-invoked arm's typed refusal (cross-surface friction
    /// plan D-3, #832): the conversion arms' T-19 twin — the
    /// subjectless/no-target dispatch surfaces on the status-notice
    /// lane from the action's registry description. Structural fact
    /// only (`notice.is_some()`); the phrase is never asserted.
    /// The habit editor's one-shot subject (H4 D1): the menu row
    /// routes the bare id — the opener's focus fact mints the target
    /// (the habit menu's `focused_habit`; the repair seam's exact
    /// arithmetic). The convert arm's resolution order, habit-side.
    /// The habit history dialog's ONE opener (HF3 W-8, US-7/S1):
    /// the action arm and the editor's History row converge here —
    /// the subject resolution, the kiosk/subject refusals, and the
    /// view-state write happen in exactly one place (the arm
    /// convention). The editor variant's subject rides the
    /// view-state's habit subject when no row focus carries one.
    pub(in crate::adapter) fn open_habit_history_from_subject(&mut self, window: &AppWindow) {
        let habit = self
            .habit_edit_subject()
            .or_else(|| self.view_state.subject_habit());
        if let Some(habit) = habit {
            let now = self.day_ctx().now;
            let day = taskboard_domain::DayCalendar {
                day_start_time: self.day_start.0,
                offset: self.day_start.1,
            };
            let today = day.today(now);
            let outcomes = self.view_state.open_habit_history(habit, today);
            self.apply_outcomes(window, &outcomes);
            self.finish_mutation(window);
        } else {
            self.arm_no_target_notice(ActionId::HabitHistory);
            self.push_notice(window);
        }
    }

    pub(in crate::adapter) fn habit_edit_subject(&self) -> Option<taskboard_domain::HabitId> {
        // D3a (W-16, #1202): the walk cursor joins the subject chain —
        // the Return row's habit landing IS the subject (the
        // chord verbs' `effective_habit` resolution; the walk cursor
        // is the fallback, the row seams' mints stay first so the
        // menu path cannot drift).
        let habit = self
            .focused_habit
            .or(match self.menu_row_subject {
                Some(crate::presentation::RowSubject::Habit(habit)) => Some(habit),
                _ => None,
            })
            .or_else(|| match self.view_state.walk_subject() {
                Some(crate::presentation::RowSubject::Habit(habit)) => Some(habit),
                _ => None,
            });
        habit.filter(|habit| {
            self.state
                .load()
                .habits
                .habits
                .get(habit)
                .is_some_and(taskboard_domain::habit::Habit::is_live)
        })
    }

    pub(in crate::adapter) fn arm_no_target_notice(&mut self, id: ActionId) {
        self.notice = Some(Notice {
            text: crate::presentation::no_target_notice_phrase(
                crate::actions::entry(id).description,
            ),
            until: chrono::Utc::now() + crate::presentation::MENU_NOTICE_HORIZON,
        });
    }

    /// The habit conversion strip's ONE arming site (cross-surface
    /// friction plan D-2, #832, AC-1's identicality guarantee): the
    /// row seam and the menu-routed effect arm converge here — the
    /// strip flags and the window echo are written in exactly one
    /// place. `false` on a dead habit or an unresolvable row index
    /// (the caller surfaces the typed refusal; the pointer seam stays
    /// silent — its stale-index posture is unchanged).
    pub(in crate::adapter) fn arm_habit_convert(
        &mut self,
        window: &AppWindow,
        habit: taskboard_domain::HabitId,
    ) -> bool {
        let target = {
            let state = self.state.load();
            let Some(row) = state.habits.habits.get(&habit) else {
                return false;
            };
            if !row.is_live() {
                return false;
            }
            row.target
        };
        self.focused_habit = Some(habit);
        self.habit_convert_armed = Some(habit);
        self.habit_convert_target = Some(habit);
        self.pattern_draft = Some(crate::presentation::convert_pattern_default(target));
        // The armed confirm lives INSIDE the open habit menu (O-7 Q3):
        // the one sanctioned rebuild of the built-once menu (the D-1
        // pin) — same anchor, same kind, the confirm flips to enabled.
        self.reproject_habit_menu(window);
        true
    }

    /// The habit menu's ONE re-projection (H2 D4): rebuilds the page
    /// over the open menu's subject and anchor — the arm flip
    /// (confirm greys → enables) and the cancel step's disarm both
    /// land here. No-op unless the open menu IS the habit menu of the
    /// still-armed subject.
    pub(in crate::adapter) fn reproject_habit_menu(&mut self, window: &AppWindow) {
        let Some(crate::presentation::RowSubject::Habit(subject)) = self.menu_row_subject else {
            return;
        };
        if !self.shell.menu.open {
            return;
        }
        let entries = self.habit_menu_entries(subject);
        self.root_entries = entries;
        // The ACTIVE page rebuilds (the arm routes from the open
        // submenu — the flip must land in the page the user sees,
        // never yank the menu back to its root).
        self.menu_entries = match self.shell.menu.task_group {
            Some(family) => self.habit_submenu_page(family),
            None => self.root_entries.clone(),
        };
        self.shell.menu.len = self.menu_entries.len();
        self.shell.menu.selected = self.shell.menu.selected.min(self.menu_entries.len());
        self.sync_menu_models(window);
    }

    /// The habit menu's per-verb verdicts (H2 D4/D6/A3): the armed
    /// trio flips with the armed subject, the repair row greys
    /// without a make-up window (applicable, precondition-blocked —
    /// never withheld), and on kiosk EVERY habit verb greys from the
    /// states map (reading a menu is passive; the arm additionally
    /// refuses at dispatch).
    pub(in crate::adapter) fn habit_menu_states(
        &self,
        state: &taskboard_domain::AppState,
        habit: taskboard_domain::HabitId,
    ) -> std::collections::BTreeMap<ActionId, bool> {
        let mut states = self.entry_states(state, &self.view_state.flow().clone());
        let armed = self.habit_convert_armed == Some(habit);
        states.insert(ActionId::HabitConvertArm, !armed);
        states.insert(ActionId::HabitConvertToSeries, armed);
        states.insert(ActionId::HabitConvertCancel, armed);
        // The staged remove's verdicts (H4 D4): the arm greys while
        // armed (the confirm replaces its enabled state), the confirm
        // flips on with the arm, the cancel joins armed-only.
        let remove_armed = self.habit_remove_armed == Some(habit);
        states.insert(ActionId::HabitRemove, !remove_armed);
        states.insert(ActionId::HabitRemoveConfirm, remove_armed);
        states.insert(ActionId::HabitRemoveCancel, remove_armed);
        let repair = state
            .habits
            .habits
            .get(&habit)
            .is_some_and(taskboard_domain::habit::Habit::is_live)
            && state.habits.habits.get(&habit).is_some_and(|row| {
                let (counts, _) = taskboard_domain::habit_event::derived_window_views(
                    row,
                    state.habits.events.values(),
                    self.calendar(),
                );
                crate::presentation::habit_repair_target(
                    row,
                    &counts,
                    self.day_ctx().now,
                    self.calendar(),
                )
                .is_some()
            });
        states.insert(ActionId::HabitRepair, repair);
        if self.kiosk {
            for id in [
                ActionId::HabitLog,
                // The tracking toggle greys with the rest (journey plan
                // W-2, #1305 — the greyed-affordance law: reading a menu
                // is passive; the control's own gate is habits.slint's
                // editing-enabled binding).
                ActionId::HabitTrackToggle,
                // The HF1 trio greys with the rest (R-3: visible-
                // disabled, never withheld).
                ActionId::HabitSkip,
                ActionId::HabitPause,
                ActionId::HabitResume,
                ActionId::HabitUndoLog,
                ActionId::HabitRepair,
                ActionId::HabitMoveUp,
                ActionId::HabitMoveDown,
                ActionId::HabitConvertArm,
                ActionId::HabitConvertToSeries,
                ActionId::HabitConvertCancel,
                ActionId::HabitEdit,
                // The HF3 history opener greys too (the visible-
                // disabled posture: the dialog is desktop-only, R-3 —
                // the row reads "exists, not for this device").
                ActionId::HabitHistory,
                ActionId::HabitRemove,
                ActionId::HabitRemoveConfirm,
            ] {
                states.insert(id, false);
            }
        }
        states
    }

    /// The habit menu's page over a subject (H2 D4): the curated core
    /// plus the verdicts; also mints both reorder one-shots from the
    /// row source (the row seam's exact arithmetic).
    pub(in crate::adapter) fn habit_menu_entries(
        &mut self,
        habit: taskboard_domain::HabitId,
    ) -> Vec<crate::menu::MenuEntry> {
        let state = self.state.load_full();
        // #1311: the chain is the opener's — `open_habit_context_menu`
        // installs the Today-prefixed chain, so this projection, the
        // Convert submenu, and the arm-flip re-projection all resolve
        // the Today-bound verbs' hints through `self.menu_chain` (one
        // derivation, no per-consumer patching).
        let chain = self.menu_chain.clone();
        // The reorder pair's slots (H2 D4): the shared mint (chunk 8
        // D4 — one arithmetic, `habit_move_slots` over the section's
        // shared live order: menu open, keyboard walk, drag gap).
        let (up, down) = Self::habit_move_slots(&state, habit);
        self.habit_move_up = up;
        self.habit_move_down = down;
        let states = self.habit_menu_states(&state, habit);
        let subject = crate::presentation::task_menu_subject(
            &state,
            Some(crate::presentation::RowSubject::Habit(habit)),
        );
        // The undo-family restore moved out of the menu (#1290's
        // enumeration: the deleted-habits section owns the roster
        // rows — the newest-only menu arm retired with it).
        let page = crate::menu::habit_context_page(&self.keymap, &chain, subject, &states);
        drop(state);
        page
    }

    /// The reorder pair's shared mint (chunk 8 D4, #934 — the
    /// arithmetic delegated to its one home,
    /// `presentation::habit_move_slots`): target gap indices over the
    /// section's live board-scoped row source (#1198 chunk A1 D1 —
    /// the engine resolves the orders). Two invocation paths converge
    /// here: the menu open and the keyboard walk's pre-mint. `None`
    /// for a stale or positionless habit (the consumers take the
    /// typed `NoTarget`).
    pub(in crate::adapter) fn habit_move_slots(
        state: &taskboard_domain::AppState,
        habit: taskboard_domain::HabitId,
    ) -> HabitMoveSlots {
        crate::presentation::habit_move_slots(state, habit)
    }

    /// The habit verbs' effective subject (chunk 8 D4): the walk
    /// cursor's habit landing when the walk has entered — a task
    /// landing resolves to `None` (a stale click fact can never
    /// answer a task walk's stroke) — the pointer click fact
    /// otherwise.
    pub(in crate::adapter) fn effective_habit(&self) -> Option<taskboard_domain::HabitId> {
        match self.view_state.walk_subject() {
            Some(crate::presentation::RowSubject::Habit(habit)) => Some(habit),
            Some(crate::presentation::RowSubject::Task(_)) => None,
            None => self.focused_habit,
        }
    }

    /// The open habit menu's submenu page (H2 D4): the Convert
    /// family's children over the habit verdicts (the confirm flips
    /// with the armed subject, the cancel row joins while armed).
    /// Empty for any other family (a habit subject has no Reminders/
    /// Timers/Entries page).
    pub(in crate::adapter) fn habit_submenu_page(
        &self,
        family: crate::menu::TaskMenuGroup,
    ) -> Vec<crate::menu::MenuEntry> {
        let Some(crate::presentation::RowSubject::Habit(habit)) = self.menu_row_subject else {
            return Vec::new();
        };
        let state = self.state.load_full();
        let states = self.habit_menu_states(&state, habit);
        let subject = crate::presentation::task_menu_subject(
            &state,
            Some(crate::presentation::RowSubject::Habit(habit)),
        );
        crate::menu::habit_convert_children(
            &self.keymap,
            &self.menu_chain,
            subject,
            &states,
            family,
        )
    }

    /// The habit row's context menu opener (H2 D4/B1): focus the
    /// habit (the row click's focus-fact discipline), then open the
    /// curated page anchored at the pointer. Stale indices absorb
    /// silently (the notification-row guard shape).
    /// The row-menu key's adapter half (US-7, W-8, #1032): the walk
    /// subject's context menu through the pointer seams' own openers
    /// (click ≡ key by construction). The keyboard anchor is the
    /// toolbar fallback — the `Hamburger` anchor flipped after the
    /// opener (the openers hard-code the pointer point; the keyboard
    /// has none) — re-synced so the projection reads the anchor the
    /// same pass. A subjectless walk (and a stale habit landing) is
    /// the typed refusal.
    pub(in crate::adapter) fn open_row_menu_keyboard(&mut self, window: &AppWindow) {
        let today = self.view_state.view() == crate::view_state::ViewId::Today;
        match self.view_state.walk_subject() {
            Some(crate::presentation::RowSubject::Habit(habit)) => {
                let index = self.tables.habits.iter().position(|&row| row == habit);
                if let Some(index) = index {
                    self.open_habit_context_menu(window, 0.0, 0.0, index);
                    self.menu_anchor = MenuAnchor::Hamburger;
                    self.sync(window);
                } else {
                    self.arm_no_target_notice(ActionId::RowMenu);
                    self.push_notice(window);
                }
            }
            Some(crate::presentation::RowSubject::Task(task)) => {
                // The today surface carries the due-today extra (the
                // today context seam's curated row); the board's is
                // the bare page (the same split the pointer seams
                // speak).
                let extra: &[ActionId] = if today {
                    &[ActionId::TaskDueToday]
                } else {
                    &[]
                };
                self.open_task_context_menu(
                    window,
                    0.0,
                    0.0,
                    extra,
                    Some(crate::presentation::RowSubject::Task(task)),
                );
                self.menu_anchor = MenuAnchor::Hamburger;
                self.sync(window);
            }
            None => {
                self.arm_no_target_notice(ActionId::RowMenu);
                self.push_notice(window);
            }
        }
    }

    pub(in crate::adapter) fn open_habit_context_menu(
        &mut self,
        window: &AppWindow,
        x: f32,
        y: f32,
        index: usize,
    ) {
        settle_pointer_conflict(self, window);
        let Some(&habit) = self.tables.habits.get(index) else {
            return;
        };
        let Some(row) = self.state.load().habits.habits.get(&habit).cloned() else {
            return;
        };
        if !row.is_live() {
            return;
        }
        // Opening another subject's menu disarms (H2 D4: one subject
        // armed at a time — the remove arm obeys the same law).
        if self.habit_convert_armed.is_some_and(|armed| armed != habit) {
            self.habit_convert_armed = None;
            self.habit_convert_target = None;
            self.pattern_draft = None;
        }
        if self.habit_remove_armed.is_some_and(|armed| armed != habit) {
            self.habit_remove_armed = None;
        }
        self.focused_habit = Some(habit);
        // #1429: the right-click is the row activation's twin — the
        // walk cursor seats too (`ViewState::activate_habit_row`, the
        // exclusivity law's landing write: a prior task cursor
        // clears), the same discipline the left-click seam and the
        // today-row context pattern apply. The menu opens only past
        // this point: focus landed exactly when the id resolved live
        // above (the stale/dead early returns), so the gate is the
        // resolution order itself.
        let outcomes = self.view_state.activate_habit_row(habit);
        self.apply_outcomes(window, &outcomes);
        self.menu_row_subject = Some(crate::presentation::RowSubject::Habit(habit));
        // #1311: the habit menu's context chain — Today first (the
        // menu opens exclusively over the Today view's habit rows, and
        // the habit verbs are bound exactly there), Board and Global
        // following for any verb bound above. The projection (this
        // open, the Convert submenu, the re-projections) reads it
        // through `self.menu_chain`.
        self.menu_chain = [ContextId::Today, ContextId::Board, ContextId::Global].to_vec();
        let entries = self.habit_menu_entries(habit);
        self.open_context_menu_with(window, x, y, entries, Vec::new());
    }

    /// The series conversion strip's ONE arming site (the habit
    /// twin's pair): the recur row's seam and the menu-routed effect
    /// arm converge here. `false` without a live recur rule (the
    /// caller surfaces the typed refusal).
    pub(in crate::adapter) fn arm_series_convert(&mut self, task: TaskId) -> bool {
        let pattern = {
            let state = self.state.load();
            let Some(rule) = state.tasks.get(&task).and_then(|t| t.recur.as_ref()) else {
                return false;
            };
            crate::presentation::convert_target_default(&rule.pattern)
        };
        self.series_target = Some(task);
        self.convert_target = Some(pattern);
        self.recur_confirm_armed = true;
        true
    }

    /// A root-panel row's invocation (round-1 D6): identical to
    /// [`Self::invoke_entry`] at the root level; inside a submenu the
    /// root panel stays mounted (fly-out), so its rows resolve against
    /// the root page — a parent switches the open group, a quick row
    /// like Quit dispatches.
    pub(in crate::adapter) fn invoke_root_entry(&mut self, window: &AppWindow, index: usize) {
        let submenu = self.root_entries.get(index).and_then(|entry| entry.submenu);
        if let Some(sub) = submenu {
            self.shell.menu.selected = index;
            self.activate_submenu(window, sub);
            return;
        }
        // The disabled guard rides the root twin too (the keyboard
        // `menu.invoke` and the fly-out rows both land here).
        let disabled = self.root_entries.get(index).is_some_and(|entry| {
            entry.submenu.is_none() && entry.action.is_some() && !entry.enabled
        });
        if disabled {
            self.arm_disabled_menu_notice(self.root_entries[index].description);
            self.push_notice(window);
            return;
        }
        let action = self.root_entries.get(index).and_then(|entry| entry.action);
        let Some(action) = action else {
            return;
        };
        self.route_menu_action(window, action);
    }

    /// A menu leaf row's close-then-route (the `invoke_entry` /
    /// `invoke_root_entry` tails' shared body, #1303): the close leg
    /// disarms the opener's focus one-shots (the disarm law), and
    /// the row's ONE route re-arms them from the open-time capture —
    /// nothing survives the dispatch. The Edit habit row mints its
    /// subject into the one-shot `habit_target` INSTEAD: the habit
    /// context menu's row resolves the menu's own armed subject, the
    /// main menu's row resolves the current view context ONLY (the
    /// walk cursor) — a one-shot minted by a different surface's
    /// menu never answers with the wrong-subject editor (#1303's
    /// filed defect).
    pub(in crate::adapter) fn route_menu_action(&mut self, window: &AppWindow, action: ActionId) {
        let focus = self.focused_habit;
        let row_subject = self.menu_row_subject;
        if action == ActionId::HabitEdit && !self.kiosk && self.habit_target.is_none() {
            let subject = match row_subject {
                Some(crate::presentation::RowSubject::Habit(habit)) => Some(habit),
                _ => self.view_state.walk_subject().and_then(|walk| match walk {
                    crate::presentation::RowSubject::Habit(habit) => Some(habit),
                    crate::presentation::RowSubject::Task(_) => None,
                }),
            };
            if let Some(habit) = subject.filter(|habit| {
                self.state
                    .load()
                    .habits
                    .habits
                    .get(habit)
                    .is_some_and(taskboard_domain::habit::Habit::is_live)
            }) {
                self.habit_target = Some(habit);
            }
        }
        self.apply_close_menu(window);
        if action != ActionId::HabitEdit {
            self.focused_habit = focus;
            self.menu_row_subject = row_subject;
        }
        self.route(window, action);
        // The dispatch-scoped disarm (#1303): the re-armed facts die
        // with the dispatch — every habit-subject consumer reads them
        // at route/arm time. The quota opener is the ONE exception:
        // its arm mints `focused_habit` for the popover's later
        // commit leg (the commit resolves fresh through the chain),
        // so that mint must survive the route.
        self.menu_row_subject = None;
        if action != ActionId::HabitQuotaEdit {
            self.focused_habit = None;
        }
    }
}

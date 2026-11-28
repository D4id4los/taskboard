// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use slint::language::KeyboardModifiers;
use slint::{ComponentHandle as _, ModelRc, SharedString};

use taskboard_domain::{AppState, StateCommand};

use super::Adapter;
use super::settings_vm::to_stroke;
use super::{FocusHandover, FocusOrigin, FocusSeam, MenuAnchor, OriginAnchor};
use crate::actions::{ActionId, ContextId, REGISTRY};
use crate::adapter::seams::{habit_open_entry, habit_undo_target};
use crate::menu::MenuEntry;
use crate::resolver::Resolution;
use crate::router::{ActionRouter, DispatchOutcome, Mode, UiEffect};
use crate::view_state::Flow;
use crate::{AppWindow, KeyFeedback, KeyboardState};

impl Adapter {
    /// The menu greying map (round-3 D27c): the enumerated
    /// state-dependent rows only — everything else keeps
    /// dispatch-time `NoTarget` machinery (the closed set; R-E).
    /// Built at menu open and in the models pass while the menu
    /// holds, so engine syncs re-grey a live menu.
    pub(crate) fn entry_states(
        &self,
        state: &taskboard_domain::AppState,
        flow: &Flow,
    ) -> BTreeMap<ActionId, bool> {
        use crate::actions::ActionId;
        let focused = self.view_state.focused_task();
        let focused_done = focused
            .and_then(|id| state.tasks.get(&id))
            .is_some_and(|task| task.done.is_some());
        let move_flow_live = matches!(
            flow,
            Flow::Move { .. } | Flow::MoveStack { .. } | Flow::DragMove { .. }
        );
        let tracking_live = crate::presentation::tracking_chip(state).is_some();
        let subject = self.view_state.subject_task();
        BTreeMap::from([
            (ActionId::TaskMoveCancel, move_flow_live),
            (ActionId::TaskComplete, focused.is_some() && !focused_done),
            (ActionId::TaskReopen, focused.is_some() && focused_done),
            (ActionId::TaskMove, focused.is_some()),
            (ActionId::TaskEdit, focused.is_some()),
            (ActionId::TaskDelete, focused.is_some()),
            (ActionId::TaskTitleEdit, subject.is_some()),
            (
                ActionId::TaskTrackStart,
                focused.is_some() && !tracking_live,
            ),
            (ActionId::TaskTrackStop, tracking_live),
            // The concentration overlay (#496): desktop-only (kiosk's
            // three-layer exclusion, layer 2) and only while focus
            // mode is configured — no timing input, no overlay.
            (
                ActionId::FocusModeToggle,
                !self.kiosk && self.focus.is_some(),
            ),
            // The review walk's opener (#638): desktop-only (layer 2
            // of the exclusion, D-J14 — the `FocusModeToggle` gate).
            (ActionId::ReviewOpen, !self.kiosk),
            // Undo v1 surfaces (undo/2, ADR 0019): availability reads
            // the published `UndoView` (memory-only, derived at every
            // publish) — the whole v1 binding. A `NotUndoable` head
            // still shows `can_undo` (R-U6 refuses at planning, typed).
            (ActionId::AppUndo, state.undo.can_undo),
            (ActionId::AppRedo, state.undo.can_redo),
            // The v2 picker entry (UF2, #353): desktop-only (R-2 — the
            // structural exclusion), live when the line has anything to
            // pick (a reflex row or a branch row).
            (
                ActionId::AppUndoBranches,
                !self.kiosk && (state.undo.can_undo || state.undo.can_redo),
            ),
            // The live pause arm (UX-R8 chunk-5 D-4): the commit row
            // greys unless the pause editor's draft is armed — the
            // TaskMoveCancel flow-grey precedent.
            (ActionId::SeriesPauseCommit, self.pause_draft.is_some()),
        ])
    }

    /// Pushes the shell-derived state one-way onto the window
    /// globals (D18); nothing renders it in chunk 2 — chunk 3's
    /// status bar reads these.
    pub(in crate::adapter) fn sync(&self, window: &AppWindow) {
        window.set_palette_open(self.shell.palette);
        // The capture overlay's window bit (chunk 10 D8): from the
        // view-state flow, the palette precedent.
        let capture = matches!(self.view_state.flow(), crate::view_state::Flow::Capture);
        window.set_capture_open(capture);
        // The field seeds at overlay init only (the rename-field rule):
        // the buffer rides while the flow holds, the slot clears on
        // close — a refresh never rewrites a live field's text (D9
        // rule 4).
        window.set_capture_initial_text(if capture {
            SharedString::from(self.view_state.buffer().to_owned())
        } else {
            SharedString::from(String::new())
        });
        // The recur editor's window-level mirrors (round-7 D-7.1): the
        // overlay mounts exactly while the flow holds; the model
        // re-seeds from the level's buffer every pass (the TimeField
        // contract — the widget never holds state).
        let recur_open = matches!(self.view_state.flow(), Flow::EditRecur { .. });
        window.set_recur_editor_open(recur_open);
        // The refused-commit mirror (#1221): rides the editor's open
        // state — the flag dies with the flow (buffers-never-outlive-
        // the-flow rule).
        window.set_recur_draft_invalid(recur_open && self.recur_draft_invalid);
        if recur_open {
            window.set_recur_editor_model(self.recur_editor_model());
        }
        // The export dialog's window-level mirrors (the J2 export
        // plan W-1..W-5): the mount bit rides the VM; the model
        // re-seeds every pass (the recur-editor contract — the
        // widget never holds state).
        let export_open = self.export_vm.is_some();
        window.set_export_dialog_mounted(export_open);
        if export_open {
            window.set_export_model(self.export_dialog_model());
        }
        let palette_selected = match self.view_state.flow() {
            crate::view_state::Flow::Palette { selected } => *selected,
            _ => 0,
        };
        window.set_palette_selected_index(i32::try_from(palette_selected).unwrap_or(i32::MAX));
        let keyboard = window.global::<KeyboardState>();
        keyboard.set_vi_insert(self.shell.mode == crate::router::Mode::Insert);
        keyboard.set_menu_open(self.shell.menu.open);
        // The undo branch picker's mount bit (UF2, #353); the models
        // ride the open seam (a refresh never rewrites a live picker —
        // the stale-row refusal is the typed contract, W-6).
        keyboard.set_undo_picker_open(self.shell.undo_picker);
        // The picker grid's context mirror (round-3 D25): the harness
        // chain and the markup read the same fact.
        keyboard.set_date_picker_grid(self.shell.date_picker_grid);
        keyboard
            .set_menu_selected_index(i32::try_from(self.shell.menu.selected).unwrap_or(i32::MAX));
        keyboard.set_menu_root_selected_index(
            i32::try_from(self.shell.menu.root_selected).unwrap_or(i32::MAX),
        );
        match self.menu_anchor {
            MenuAnchor::Hamburger => {
                keyboard.set_menu_pointer_anchored(false);
            }
            MenuAnchor::Pointer(x, y) => {
                keyboard.set_menu_pointer_anchored(true);
                keyboard.set_menu_pointer_x(x);
                keyboard.set_menu_pointer_y(y);
            }
        }
        // The active page's parent-row index (round-1 D6; UX-R8 D-5
        // widens the shape): the fly-out/replace machinery keys off
        // this int for both the main groups and the task families —
        // the presentation reads the position, never the kind.
        keyboard.set_menu_active_group(match self.active_submenu() {
            None => -1,
            Some(sub) => self
                .root_entries
                .iter()
                .position(|entry| entry.submenu == Some(sub))
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        });
    }

    /// The open submenu's target (UX-R8 chunk-5 D-5): the main
    /// group's page or the task family's — the ladder and the
    /// parent-row position sync read it.
    pub(in crate::adapter) fn active_submenu(&self) -> Option<crate::menu::Submenu> {
        match (self.shell.menu.active_group, self.shell.menu.task_group) {
            (Some(group), _) => Some(crate::menu::Submenu::Group(group)),
            (None, Some(family)) => Some(crate::menu::Submenu::Task(family)),
            (None, None) => None,
        }
    }

    /// The undo branch picker's row models (UF2, #353 W-2/W-3): the
    /// typed projections pushed one-way onto the window global —
    /// sibling branches from `UndoView.branch_children`, undo-step
    /// rows from the `recent` window. Rows carry the journal entry id
    /// (string form across the seam; re-typed at the trust boundary)
    /// and the engine-derived label. A close empties the bit; the
    /// models stay (a refresh never rewrites a live picker — the
    /// stale-row refusal is the typed contract, W-6).
    pub(in crate::adapter) fn sync_undo_picker_models(&self, window: &AppWindow) {
        let keyboard = window.global::<KeyboardState>();
        keyboard.set_undo_picker_open(self.shell.undo_picker);
        if !self.shell.undo_picker {
            return;
        }
        // The cursor mirror (#1227): the shell's picker-scoped
        // selection (0 while closed — the mount resets it).
        keyboard.set_undo_picker_cursor(i32::try_from(self.shell.menu.selected).unwrap_or(0));
        let undo = &self.state.load().undo;
        let rows = |labels: &[taskboard_domain::UndoLabel]| -> Vec<crate::UndoPickerRowModel> {
            labels
                .iter()
                .map(|row| crate::UndoPickerRowModel {
                    id: row.id.as_uuid().to_string().into(),
                    label: row.label.as_str().into(),
                    // D-3: empty string ↔ `None` across the seam —
                    // the row renders label-only, never a blank dash.
                    payload: row.payload.as_deref().unwrap_or("").into(),
                })
                .collect()
        };
        keyboard.set_undo_picker_branch_rows(ModelRc::new(slint::VecModel::from(rows(
            &undo.branch_children,
        ))));
        keyboard
            .set_undo_picker_target_rows(ModelRc::new(slint::VecModel::from(rows(&undo.recent))));
    }

    /// The two-tier menu models (round-1 D6): the root page model
    /// stays mounted for the whole open lifecycle; the submenu model
    /// carries the active group's page (empty at the root).
    pub(in crate::adapter) fn sync_menu_models(&self, window: &AppWindow) {
        let keyboard = window.global::<KeyboardState>();
        keyboard.set_menu_entries(Self::menu_model(&self.root_entries));
        keyboard.set_menu_submenu_entries(Self::menu_model(&self.menu_entries));
        keyboard.set_menu_quick_bar(Self::quick_bar_model(
            &self.quick_bar_entries,
            &self.keymap,
            &self.menu_chain,
        ));
    }

    /// The quick bar as the slint model rows (UX-R8 chunk-5 D-2): the
    /// Rust icon vocabulary maps onto the generated `IconKind` here —
    /// menu.rs stays slint-free. Each row carries its action's first
    /// chain-visible binding (J5 US-4 AC-1) — the same projection the
    /// menu rows resolve through `row()`; empty when unbound.
    pub(in crate::adapter) fn quick_bar_model(
        entries: &[crate::menu::QuickBarEntry],
        keymap: &crate::keymap::Keymap,
        chain: &[ContextId],
    ) -> ModelRc<crate::QuickBarEntryModel> {
        ModelRc::from(
            entries
                .iter()
                .map(|entry| {
                    let hint = keymap
                        .first_binding_display(entry.action, chain)
                        .unwrap_or_default();
                    crate::QuickBarEntryModel {
                        action: entry.action.id().to_owned().into(),
                        // The registry label — the element's address / the
                        // a11y label (D16).
                        label: REGISTRY
                            .iter()
                            .find(|row| row.id == entry.action)
                            .map_or("", |row| row.label)
                            .into(),
                        icon: match entry.icon {
                            crate::menu::QuickBarIcon::Calendar => crate::IconKind::Calendar,
                            crate::menu::QuickBarIcon::CalendarPlus => {
                                crate::IconKind::CalendarPlus
                            }
                            crate::menu::QuickBarIcon::CalendarMinus => {
                                crate::IconKind::CalendarMinus
                            }
                            crate::menu::QuickBarIcon::Play => crate::IconKind::Play,
                            crate::menu::QuickBarIcon::Stop => crate::IconKind::Stop,
                        },
                        enabled: entry.enabled,
                        visible: entry.visible,
                        hint_keys: ModelRc::new(slint::VecModel::from(
                            hint.iter()
                                .map(|stroke| SharedString::from(stroke.as_str()))
                                .collect::<Vec<_>>(),
                        )),
                    }
                })
                .collect::<Vec<_>>()
                .as_slice(),
        )
    }

    /// One page as the slint model rows.
    pub(in crate::adapter) fn menu_model(entries: &[MenuEntry]) -> ModelRc<crate::MenuEntryModel> {
        ModelRc::from(
            entries
                .iter()
                .map(|entry| crate::MenuEntryModel {
                    action: entry
                        .action
                        .map_or(String::new(), |id| id.id().to_owned())
                        .into(),
                    // #1283: the Cow label projects verbatim — the
                    // owned (engine-derived) spelling survives the
                    // shell bridge.
                    label: entry.label.as_ref().to_owned().into(),
                    description: entry.description.into(),
                    enabled: entry.enabled,
                    submenu: entry.submenu.is_some(),
                    hint: ModelRc::new(slint::VecModel::from(
                        entry
                            .hint
                            .iter()
                            .map(|stroke| SharedString::from(stroke.as_str()))
                            .collect::<Vec<_>>(),
                    )),
                })
                .collect::<Vec<_>>()
                .as_slice(),
        )
    }

    /// The capture-phase entry point (D18): slint event → `Stroke` →
    /// resolver. Accepts iff the resolver consumed the stroke;
    /// `Unhandled` rejects so delivery continues (Tab traversal).
    ///
    /// The D20 pulls run here, before the resolver feed, because every
    /// keystroke re-reads the editing-facts singleton (Slint globals
    /// do not notify Rust — pull-at-intercept is self-healing by
    /// construction): a focused text field engages insert mode, a
    /// fieldless insert disengages — and never under an armed flow or
    /// the picker grid: the flow outcomes own their mode transitions
    /// (a flow arms Insert before its field mounts and holds it to the
    /// settle, the grid phase holds it while the *field* lost focus to
    /// the grid; flipping Normal there would hand Escape to vi's
    /// move-cancel instead of the clean close). The engage pull's
    /// idempotence against the flow outcomes is the convergence.
    #[allow(clippy::too_many_lines)] // the resolver's dispatch arm + the scope arms, kept flat
    pub(in crate::adapter) fn intercept(
        &mut self,
        window: &AppWindow,
        text: &SharedString,
        modifiers: KeyboardModifiers,
    ) -> bool {
        if self.walk_reentry.get() {
            return false;
        }
        let Some(stroke) = to_stroke(text, modifiers) else {
            return false;
        };
        {
            let facts = window.global::<crate::EditingFacts>();
            let text_focus = facts.get_text_focus_count();
            if text_focus > 0 && self.shell.mode != Mode::Insert {
                self.shell.apply(&UiEffect::SetMode(Mode::Insert), 0);
                self.sync(window);
            } else if text_focus == 0
                && self.shell.mode == Mode::Insert
                && !self.shell.date_picker_grid
                && matches!(self.view_state.flow(), Flow::None)
            {
                self.shell.apply(&UiEffect::SetMode(Mode::Normal), 0);
                self.sync(window);
            }
        }
        // The printable monopoly (round-5 chunk 7, D-KS2): while a
        // text field holds slint focus, it claims every bare
        // printable — and Space, printable text — outright, ahead of
        // every scope arm and the resolver. Chord-class strokes pass
        // to the scope stack: the field-phase picker scope serves its
        // navigation strokes mid-typing (F11) while the field keeps
        // every printable.
        {
            let facts = window.global::<crate::EditingFacts>();
            if crate::keyscope::monopoly_declines(
                self.shell.mode,
                usize::try_from(facts.get_text_focus_count()).unwrap_or(usize::MAX),
                self.resolver.has_pending(),
                stroke,
            ) {
                Self::set_feedback(window, KeyFeedback::None);
                return false;
            }
        }
        // The editor scope's table (D-δ2/D-KS3), then the retired
        // special cases' scope arms (D-KS4) — each ends the intercept
        // when it claims the stroke.
        if let Some(consumed) = self.editor_scope_arm(window, stroke) {
            return consumed;
        }
        if let Some(consumed) = self.special_scope_arms(window, stroke) {
            return consumed;
        }
        // The chain reflects the view-state machine, not a stale seam
        // write (D6: shell view/focus derive from view_state). The
        // key doorway arms the scope deltas (D-δ1 with its
        // Return/Escape exemption; D-δ2 comes with the editor scope).
        self.sync_view_context(window);
        let chain = self.chain_with(window, Some(stroke), true);
        // The activation reservation (special case C, r4 §2 D18):
        // while a task-view activation stop holds focus, bare
        // Return/Space belong to the focused cell — the resolver is
        // still fed first (prefix semantics preserved: a pending
        // prefix aborts on Return exactly as now), but an `Action`
        // answer or `Unhandled` yields, and the focused cell's
        // `key-pressed` activates. The modal surfaces reset the fact
        // anyway; the check is the belt.
        let reserved = {
            let facts = window.global::<crate::EditingFacts>();
            crate::keyscope::activation_reserved(
                facts.get_activation_focus_count() > 0,
                self.shell.menu.open,
                self.shell.palette,
                stroke,
            )
        };
        match self.resolver.feed(stroke, &chain, &self.keymap) {
            Resolution::Action(id) => {
                if reserved {
                    Self::set_feedback(window, KeyFeedback::None);
                    return false;
                }
                Self::set_feedback(window, KeyFeedback::None);
                // UX-R8 D-4's shift-walk extension: the named-key walk
                // rows with shift held extend the selection instead of
                // the plain focus step (the board surface; other
                // surfaces keep the plain step — R-1's scoping). The
                // printable-bound rows fold shift away (the recorded
                // leniency: arrows only).
                if stroke.shift
                    && matches!(id, ActionId::NavNextItem | ActionId::NavPrevItem)
                    && self.view_state.view() == crate::view_state::ViewId::Board
                {
                    let before = self.view_state.walk_focus();
                    let state = self.state.load_full();
                    let direction = if id == ActionId::NavNextItem { 1 } else { -1 };
                    let outcomes = self.view_state.extend_selection_walk(&state, direction);
                    self.apply_outcomes(window, &outcomes);
                    self.push_focus_scroll_hint(window, before);
                    return true;
                }
                self.route(window, id);
                true
            }
            Resolution::Pending => {
                Self::set_feedback(window, KeyFeedback::PrefixPending);
                true
            }
            Resolution::Aborted => {
                Self::set_feedback(window, KeyFeedback::PrefixAbort);
                true
            }
            Resolution::Unhandled => {
                // The notification surface's engagement arm (US-6/W-6,
                // D-N1, the regression-bar revision): the grammar arms
                // the surface only as a FALLBACK — the stroke found no
                // binding in the visible chain (a dead key), so the
                // ambient claim wins everywhere the chain binds the
                // key first (the focus overlay's walk regression is
                // the recorded bar). Down/Up engage on the unbound
                // chains; the verb keys engage everywhere they are
                // dead.
                if !self.shell.notification_surface
                    && !self.tables.notifications.is_empty()
                    && let Some(gram) = crate::keyscope::notification_grammar_stroke(&stroke)
                {
                    match gram {
                        crate::keyscope::NotificationGrammar::Walk(direction) => {
                            Self::set_feedback(window, KeyFeedback::None);
                            self.notification_walk(window, direction);
                        }
                        crate::keyscope::NotificationGrammar::Verb(verb) => {
                            Self::set_feedback(window, KeyFeedback::None);
                            self.apply_notification_verb(window, verb);
                        }
                    }
                    return true;
                }
                Self::set_feedback(window, KeyFeedback::None);
                false
            }
        }
    }

    /// The editor scope's command executor (round-5 chunk 7, D-KS3):
    /// the pure arithmetic (`keyscope::apply_editor_command`) over the
    /// widget's mirrored buffer and the tracked cursor; the widget
    /// applies the outcome through the `EditorScope` seam (Rust never
    /// touches `TextInput` internals). Cut/copy/paste ride the single
    /// adapter slot — never the OS clipboard.
    pub(in crate::adapter) fn editor_command(
        &mut self,
        window: &AppWindow,
        command: crate::keyscope::EditorCommand,
    ) -> bool {
        use crate::keyscope::EditorCommand;
        // The discard and the vi re-insert ride the existing mode
        // funnel (the same outcome as Escape today — no new settle).
        match command {
            EditorCommand::Discard => {
                self.apply_effect(window, UiEffect::SetMode(Mode::Normal));
                return true;
            }
            EditorCommand::EnterInsert => {
                self.apply_effect(window, UiEffect::SetMode(Mode::Insert));
                return true;
            }
            _ => {}
        }
        let scope = window.global::<crate::EditorScope>();
        let buffer = scope.get_buffer_mirror().to_string();
        let mirror = usize::try_from(scope.get_cursor_offset()).unwrap_or(0);
        // The mirror lags the widget's programmatic moves by an event
        // pass: adopt it only when it moved against BOTH the last
        // seen value and the tracked cursor (a genuine native move —
        // typing, mouse, native arrows); a lagging write equals one of
        // the two and yields to the tracked cursor.
        let cursor = if mirror != self.editor_cursor_seen && mirror != self.editor_cursor {
            mirror
        } else {
            self.editor_cursor
        };
        let edit = crate::keyscope::apply_editor_command(
            &crate::keyscope::EditorState {
                buffer: &buffer,
                cursor,
                mark: self.editor_mark,
                clipboard: self.editor_clipboard.as_deref(),
            },
            command,
        );
        self.editor_mark = edit.mark;
        self.editor_clipboard = edit.clipboard;
        scope.set_anchor_offset(i32::try_from(edit.mark.unwrap_or(edit.cursor)).unwrap_or(-1));
        scope.set_command_replaces(edit.text.is_some());
        scope.set_command_text(edit.text.unwrap_or_default().into());
        scope.set_command_cursor(i32::try_from(edit.cursor).unwrap_or(-1));
        scope.set_command_code(command.code());
        scope.set_command_generation(scope.get_command_generation() + 1);
        self.editor_cursor = edit.cursor;
        self.editor_cursor_seen = mirror;
        true
    }

    /// The editor scope's table arm (round-5 chunk 7, D-δ2/D-KS3):
    /// consulted before the resolver — but only when the ordinary
    /// resolver holds no pending prefix (the keymap's own prefixes,
    /// emacs' C-x family, own the stroke first, so C-x C-c still
    /// exits inside the editor). `Some(consumed)` ends the intercept.
    /// The mark and the accumulator self-heal whenever the scope is
    /// not armed — no registration outlives its surface.
    pub(in crate::adapter) fn editor_scope_arm(
        &mut self,
        window: &AppWindow,
        stroke: crate::keymap::Stroke,
    ) -> Option<bool> {
        let editor_armed = matches!(self.view_state.flow(), Flow::EditDescription { .. })
            && window
                .global::<crate::EditingFacts>()
                .get_editor_focus_count()
                > 0;
        if !editor_armed || self.resolver.has_pending() {
            self.editor_resolver.reset();
            if !editor_armed {
                self.editor_mark = None;
            }
            return None;
        }
        let table = crate::keyscope::editor_table(self.preset);
        match self.editor_resolver.feed(stroke, &table) {
            crate::keyscope::EditorClaim::Command(command) => {
                Self::set_feedback(window, KeyFeedback::None);
                Some(self.editor_command(window, command))
            }
            crate::keyscope::EditorClaim::Consumed => {
                Self::set_feedback(window, KeyFeedback::PrefixPending);
                Some(true)
            }
            crate::keyscope::EditorClaim::Pass => None,
        }
    }

    /// The retired special cases' arms (round-5 chunk 7, D-KS4): each
    /// arming fact is the retired condition verbatim (declared in
    /// `keyscope`), each fires where its block fired — after the
    /// monopoly, before the resolver; each pin journey is the
    /// acceptance contract. `Some(consumed)` ends the intercept.
    pub(in crate::adapter) fn special_scope_arms(
        &mut self,
        window: &AppWindow,
        stroke: crate::keymap::Stroke,
    ) -> Option<bool> {
        // The request-driven switcher flyout's walk (UX R-4 D-4,
        // #794): the flyout armed by the target-free BoardSelect
        // stroke has no focused field instance (the widget's own
        // ladder serves the pointer path), so the identical key
        // vocabulary rides here against the facts — the walk's state
        // seam. Escape closes with nothing dispatched; Return routes
        // the highlighted row's selection (the segment seam's route)
        // and closes. Runs ahead of the ladder-decline block (whose
        // decline would dead-end the keys without a focused field).
        if let Some(consumed) = self.switcher_flyout_key(window, stroke) {
            return Some(consumed);
        }
        // The dropdown ladders (special case D, r4 D39/D40, round-5
        // D6): while a flyout is open, its keys decline the single
        // key path — the focused field's modal arm owns them.
        let ladder_open = if window.global::<crate::SettingsBoardFacts>().get_open() {
            Some(crate::keyscope::LadderGlobal::SettingsBoard)
        } else if window.global::<crate::BoardBarFacts>().get_open() {
            Some(crate::keyscope::LadderGlobal::BoardBar)
        } else if window.global::<crate::TimeFieldFacts>().get_open() {
            Some(crate::keyscope::LadderGlobal::TimeField)
        } else if window.global::<crate::DropdownFacts>().get_open() {
            // The task detail's pickers (round-9 chunk 15 D-7): the
            // class facts global's open bit, LAST — the three legacy
            // globals above win their own flyouts first, so this arm
            // catches only the detail's dropdown ladders.
            Some(crate::keyscope::LadderGlobal::TaskDetailPriority)
        } else {
            None
        };
        if crate::keyscope::dropdown_ladder_declines(ladder_open, stroke) {
            Self::set_feedback(window, KeyFeedback::None);
            return Some(false);
        }
        // The timer setter's dismissal arm (special case A, r4 D33):
        // the panel is presentation state, not a flow — its Escape is
        // an explicit decline routed to the existing dismissal funnel.
        // Nothing commits; the draft clears with the panel.
        if crate::keyscope::timer_setter_dismisses(window.get_timer_setter_open(), stroke) {
            self.clear_timer_setter();
            window.set_timer_setter_open(false);
            self.refresh_forced(window);
            return Some(true);
        }
        // The habit create form's dismissal arm (round-7 D-7.2, the
        // timer setter's twin): bare Escape ahead of the resolver —
        // the armed form is presentation state; nothing commits.
        // (A Today-context Escape binding would statically shadow
        // Global TaskMoveCancel on the whole view — the funnel exists
        // for exactly this shape.)
        if crate::keyscope::habit_create_dismisses(window.get_habits_create_open(), stroke) {
            self.disarm_habit_create(window);
            return Some(true);
        }
        // The pause editor's dismissal arm (D3a W-7, #1208): the
        // create form's exact twin — bare Escape ahead of the
        // resolver. The editor's date field holds the keyboard's
        // insert phase while mounted (the D20 pull), so the global
        // escape/cancel preset is physically unreachable from this
        // surface — the funnel is the action layer's existing shape
        // for exactly this (no second Escape *binding*: the keymap
        // is untouched, the C-5 collision rule holds). The dismissal
        // resolves into the landed habits-pause-cancelled funnel —
        // cancel writes nothing (the destructive-role dismissal law).
        if crate::keyscope::habit_pause_dismisses(window.get_habits_pause_open(), stroke) {
            self.disarm_habit_pause(window);
            return Some(true);
        }
        // The due editor's Down arm (special case B, r2 D25, round-3
        // D10): consumed ahead of the resolver so the insert-mode
        // chain never has to bind it. With the picker already open
        // (the default-open), the same stroke is the grid's focus
        // hand-over — the one-shot flag rides the push, and the
        // panel's `changed` moves focus.
        if crate::keyscope::due_down_arms(
            matches!(self.view_state.flow(), Flow::EditDate { .. }),
            stroke,
        ) {
            let picker_open = self
                .view_state
                .date_editor()
                .is_some_and(|editor| editor.picker.is_some());
            if picker_open {
                // The field→grid hand-over is a programmatic steal —
                // the funnel must not settle on its focus loss (r4 D24).
                if let Some(editor) = self.view_state.date_editor_mut() {
                    editor.focus_steal = true;
                }
                self.focus_handover = FocusHandover::Grid;
                // The one-shot flag is a derive input the push consumes — forced.
                self.refresh_forced(window);
            } else {
                self.open_due_picker(window, true);
            }
            return Some(true);
        }
        None
    }

    /// The create form's disarm funnel (round-7 D-7.2): one path,
    /// three entry points — the toggle's discard branch, the Cancel
    /// seam, the Escape arm. Clears the armed flag, the one-shot
    /// draft, and the quota draft (the buffers never outlive the
    /// flow).
    pub(in crate::adapter) fn disarm_habit_create(&mut self, window: &AppWindow) {
        self.habit_create_open = false;
        self.habit_create_draft = None;
        self.habit_create_quota.clear();
        window.set_habits_create_open(false);
        self.refresh_forced(window);
    }

    /// The pause editor's disarm funnel (HF1, W-3): one path — the
    /// commit's consumed subject, the Cancel seam, the subject-death
    /// absorb. Clears the armed flag and the one-shot subject (the
    /// create funnel's shape).
    pub(in crate::adapter) fn disarm_habit_pause(&mut self, window: &AppWindow) {
        self.habit_pause_open = false;
        self.habit_pause_target = None;
        window.set_habits_pause_open(false);
        self.refresh_forced(window);
    }

    /// The quota popover's push (W-5, #961): the stepper draft and its
    /// typed validity ride the window properties — `0` is the invalid
    /// draft (the `NonZeroCount` mint's law at the UI boundary; the
    /// commit stays blocked, no route), every `≥ 1` value is valid.
    pub(in crate::adapter) fn sync_quota_popover(&self, window: &AppWindow) {
        window.set_habits_quota_count(self.habit_quota_count);
        window.set_habits_quota_valid(self.habit_quota_count >= 1);
    }

    /// The quota popover's disarm (the escape rungs' funnel): the
    /// window flag drops and the one-shot draft dies with it (the
    /// buffers never outlive the flow).
    pub(in crate::adapter) fn disarm_habit_quota(&mut self, window: &AppWindow) {
        window.set_habits_quota_open(false);
        self.habit_quota_draft = None;
    }

    pub(in crate::adapter) fn set_feedback(window: &AppWindow, feedback: KeyFeedback) {
        window.global::<KeyboardState>().set_last_feedback(feedback);
    }

    /// The selection verbs' dispatch (UX-R8 D-6): Complete sends one
    /// `SetTaskDone` per member in anchor order (a tracker member's
    /// completion is impossible — the per-task absorb drops it and
    /// continues, the #492 shape); Delete arms the multi strip
    /// (`ConfirmDelete(Tasks)`, the anchor-order snapshot) — the
    /// armed repeat confirms, Escape/a settle disarm (the shared
    /// grammar). The schedule trio (J5 US-2, the multi bar's verbs)
    /// rides the same per-member shape: one sparse `UpdateTask` per
    /// member, the router's single-target masks over anchor order
    /// (the unschedule's per-task absorb skips already-unscheduled
    /// members).
    pub(in crate::adapter) fn route_selection_verb(&mut self, window: &AppWindow, id: ActionId) {
        let state = self.state.load_full();
        match id {
            ActionId::TaskComplete => {
                let members = self.view_state.selection_anchor_order(&state);
                for member in members {
                    let Some(task) = state.tasks.get(&member) else {
                        continue; // the per-task absorb (the id-table law)
                    };
                    if task.kind == taskboard_domain::entities::TaskKind::Tracker {
                        continue; // the completion is impossible (#492)
                    }
                    self.send(StateCommand::SetTaskDone {
                        id: member,
                        done: true,
                    });
                }
            }
            ActionId::TaskDelete => {
                let members = self.view_state.selection_anchor_order(&state);
                let outcomes = self.view_state.arm_multi_delete(members);
                self.apply_outcomes(window, &outcomes);
            }
            ActionId::TaskScheduleToday => {
                // The batch twin of the router's today pair-write
                // (#1428, journey plan D-1): both fields in one mask —
                // the day's first covered instant (start) and its last
                // covered instant (due, #893). The domain mints, never
                // a local `end − ε` (#884).
                let first = self.day_ctx().window.0;
                let last = taskboard_domain::days::last_covered_instant(self.day_ctx().window.1);
                self.schedule_selection(&state, first, last);
            }
            ActionId::TaskScheduleTomorrow => {
                // The batch twin of the router's tomorrow pair-write
                // (D-2): the covered-instant law holds for the batch
                // path too — tomorrow's first instant and its last
                // covered instant, the raw exclusive end retired.
                let (first, end) = taskboard_domain::days::next_day_window(
                    self.day_ctx().now,
                    self.day_start.0,
                    self.day_start.1,
                );
                let last = taskboard_domain::days::last_covered_instant(end);
                self.schedule_selection(&state, first, last);
            }
            ActionId::TaskUnschedule => {
                let members = self.view_state.selection_anchor_order(&state);
                for member in members {
                    if state
                        .tasks
                        .get(&member)
                        .is_some_and(|task| task.scheduled.is_some())
                    {
                        self.send(StateCommand::UpdateTask {
                            id: member,
                            changes: taskboard_domain::TaskChanges {
                                scheduled: Some(None),
                                ..taskboard_domain::TaskChanges::default()
                            },
                        });
                    }
                }
            }
            _ => unreachable!("the intercept's guard owns the verb set"),
        }
        self.finish_mutation(window);
    }

    /// The schedule pair's per-member dispatch (the arms' shared body):
    /// one sparse `UpdateTask` per selection member in anchor order,
    /// `scheduled` and `duedate` set to the shared day-window pair
    /// (#1428, journey plan D-1), everything else untouched.
    pub(in crate::adapter) fn schedule_selection(
        &mut self,
        state: &AppState,
        first: DateTime<Utc>,
        last: DateTime<Utc>,
    ) {
        for member in self.view_state.selection_anchor_order(state) {
            self.send(StateCommand::UpdateTask {
                id: member,
                changes: taskboard_domain::TaskChanges {
                    scheduled: Some(Some(first)),
                    duedate: Some(Some(last)),
                    ..taskboard_domain::TaskChanges::default()
                },
            });
        }
    }

    /// One fire-and-forget command send (the route arm's convention):
    /// a full or closed inbox is logged and dropped — never a panic
    /// (D16's forwarder convention).
    pub(crate) fn send(&self, command: StateCommand) {
        if self.commands.try_send(command).is_err() {
            tracing::warn!("command channel full or closed; dropped");
        }
    }

    /// One action through the router (design §5 pipeline's last hop).
    #[allow(clippy::too_many_lines)] // the per-bit consumption table reads flat
    pub(crate) fn route(&mut self, window: &AppWindow, id: ActionId) {
        // The focused-subject key translation (chunk 8 D3, #934): a
        // task-row verb resolved under a habit-focused walk routes
        // its habit twin — one stroke, one binding per context (the
        // preset tables stay the stroke truth; click ≡ key holds in
        // both directions). The identity for every pointer path and
        // every task/focusless walk by construction.
        let mut id = crate::presentation::translate_for_subject(id, self.view_state.walk_subject());
        // The selection's verb intercept (UX-R8 D-6, US-4/S3–S4): a
        // non-empty selection turns Complete/Delete — and the multi
        // bar's schedule trio (J5 US-2) — into the per-task sequences
        // (click ≡ key — the menu rows, the bar buttons and the
        // preset strokes land here alike). The track twins stay
        // single-target (Stop is global, Start rides the focused
        // member — the R-3 grammar). Empty selection keeps the
        // existing single-target semantics untouched.
        if matches!(
            id,
            ActionId::TaskComplete
                | ActionId::TaskDelete
                | ActionId::TaskScheduleToday
                | ActionId::TaskScheduleTomorrow
                | ActionId::TaskUnschedule
        ) && !self.view_state.selection().is_empty()
        {
            self.route_selection_verb(window, id);
            return;
        }
        // The per-action consumption set (cross-surface friction plan
        // D-6, #763): only the dispatched action's declared one-shots
        // move into the ctx — an undeclared field stays in the adapter,
        // so an interleaved dispatch can no longer eat a confirm's
        // target. The declaration table is router-owned
        // (`route_consumes`); the totalness test pins the vocabulary.
        // The habit menu's verbs (H2 D4): a menu row routes the bare
        // id — the subject resolves HERE from the opener's focus fact
        // when no seam pre-minted the one-shot (the repair seam's
        // exact arithmetic; a subjectless dispatch stays NoTarget).
        // Chunk 8 (D4): the walk's habit landing feeds the same
        // resolution — the effective subject is the walk cursor's
        // when the walk has entered (the exclusivity law makes a
        // task landing a typed `None`), the pointer click fact
        // otherwise; a stale click fact can never answer a task
        // walk's stroke.
        //
        // The R-8 kind resolution (US-14, #1031): the shared
        // Shift+Up/Down stroke is bound to the habit pair (Today
        // context); under a TASK walk landing the stroke flips to its
        // task twins — the twin law, one stroke one binding (the
        // preset tables stay the stroke truth; no second binding is
        // coined). Today-scoped because the binding is. On kiosk the
        // pair is an editing verb (the habit mint's HAB-14 reading):
        // the flip refuses and the dispatch takes the typed NoTarget.
        // A habit or focusless landing keeps the habit pair's path
        // untouched (US-14/S3). The twins' own ids mint too: the Task
        // group page's menu rows route `TaskMoveUp`/`TaskMoveDown`
        // directly (click ≡ key — the same dispatch entry, no stroke
        // to flip), so the pair is live from the menu exactly as from
        // the shared stroke.
        if self.view_state.view() == crate::view_state::ViewId::Today
            && !self.kiosk
            && matches!(
                id,
                ActionId::HabitMoveUp
                    | ActionId::HabitMoveDown
                    | ActionId::TaskMoveUp
                    | ActionId::TaskMoveDown
            )
            && let Some(task) = self
                .view_state
                .walk_subject()
                .and_then(|subject| match subject {
                    crate::presentation::RowSubject::Task(task) => Some(task),
                    crate::presentation::RowSubject::Habit(_) => None,
                })
        {
            id = if matches!(id, ActionId::HabitMoveUp | ActionId::TaskMoveUp) {
                ActionId::TaskMoveUp
            } else {
                ActionId::TaskMoveDown
            };
            self.task_move = Some(task);
        }
        if let Some(habit) = self.effective_habit() {
            match id {
                // The chord's subject mint (#1305, W-3/D-8): the
                // keyboard path mints the one-shot habit subject exactly
                // as the row seams do (the skip chord's shape).
                ActionId::HabitLog | ActionId::HabitSkip | ActionId::HabitTrackToggle
                    if self.habit_target.is_none() =>
                {
                    self.habit_target = Some(habit);
                }
                // HF1 (W-11, O-8 parity): the resume chord over the
                // walk's landing mints the pause editor's one-shot
                // subject exactly as the row seams do — a subjectless
                // dispatch stays the typed NoTarget.
                ActionId::HabitResume if self.habit_pause_target.is_none() => {
                    self.habit_pause_target = Some(habit);
                }
                ActionId::HabitUndoLog if self.habit_undo.is_none() => {
                    self.habit_undo = habit_undo_target(self, habit);
                }
                ActionId::HabitRepair if self.habit_repair.is_none() => {
                    let state = self.state.load_full();
                    let target = state.habits.habits.get(&habit).and_then(|row| {
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
                    });
                    drop(state);
                    if let Some((target_window, count)) = target {
                        self.habit_repair = Some((habit, target_window, count));
                    }
                }
                // The keyboard reorder pair (chunk 8 D4, #934): the
                // walk's landing mints the same one-shots the menu
                // open mints — the shared slot arithmetic, one
                // mint site. On kiosk the mint refuses (the pair is
                // an editing verb, HAB-14): the binding stays
                // visible, the dispatch takes the typed NoTarget.
                ActionId::HabitMoveUp | ActionId::HabitMoveDown if self.kiosk => {}
                ActionId::HabitMoveUp if self.habit_move_up.is_none() => {
                    let state = self.state.load_full();
                    let (up, _) = Self::habit_move_slots(&state, habit);
                    self.habit_move_up = up;
                }
                ActionId::HabitMoveDown if self.habit_move_down.is_none() => {
                    let state = self.state.load_full();
                    let (_, down) = Self::habit_move_slots(&state, habit);
                    self.habit_move_down = down;
                }
                _ => {}
            }
        }
        // US-11 (W-12, #1067): the resume chord mints the series
        // one-shot from the viewed subject exactly as the recur row's
        // click seam does — click ≡ key; a subjectless dispatch stays
        // the typed NoTarget (the router arm's refusal). The guard
        // keeps a pre-minted seam target (the pointer twin) authoritative.
        // US-9 (W-10): the roster remove's arm toggle reads the
        // roster cursor inside the routed arm (below) — the pointer
        // twin re-roots through the same route, one semantics one
        // home; no route-side mint exists (the toggle owns the
        // armed slot).
        if id == ActionId::SeriesResume
            && self.series_target.is_none()
            && let Some(task) = self.view_state.subject_task()
        {
            self.series_target = Some(task);
        }
        let wants = crate::router::route_consumes(id);
        let mut ctx = self.shell.dispatch_ctx();
        if wants.wants(crate::router::OneShotSet::CREATE_TARGET) {
            ctx.create_target = self.create_target.take();
        }
        if wants.wants(crate::router::OneShotSet::REMINDER_TARGET) {
            ctx.reminder_target = self.reminder_target;
            self.reminder_target = None;
        }
        if wants.wants(crate::router::OneShotSet::BOARD_TARGET) {
            ctx.board_target = self.board_target;
            self.board_target = None;
        }
        if wants.wants(crate::router::OneShotSet::RULE_TARGET) {
            ctx.rule_target = self.rule_target;
            self.rule_target = None;
        }
        if wants.wants(crate::router::OneShotSet::RULE_ADD_WHEN) {
            ctx.rule_add_when = self.rule_add_when;
            self.rule_add_when = None;
        }
        if wants.wants(crate::router::OneShotSet::TIMER_DRAFT) {
            ctx.timer_draft = self.timer_draft.take();
        }
        if wants.wants(crate::router::OneShotSet::TIMER_TARGET) {
            ctx.timer_target = self.timer_target.take();
        }
        if wants.wants(crate::router::OneShotSet::BREAK_RULE_DRAFT) {
            ctx.break_rule_draft = self.break_rule_draft.take();
        }
        if wants.wants(crate::router::OneShotSet::BREAK_RULE_TARGET) {
            ctx.break_rule_target = self.break_rule_target.take();
        }
        if wants.wants(crate::router::OneShotSet::TRACK_ENTRY_DRAFT) {
            ctx.track_entry_draft = self.track_entry_draft.take();
        }
        if wants.wants(crate::router::OneShotSet::TRACK_ENTRY_TARGET) {
            ctx.track_entry_target = self.track_entry_target.take();
        }
        if wants.wants(crate::router::OneShotSet::TRACK_TARGET) {
            ctx.track_target = self.track_target.take();
        }
        if wants.wants(crate::router::OneShotSet::PLAN_TARGET) {
            ctx.plan_target = self.plan_target.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_TARGET) {
            ctx.habit_target = self.habit_target.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_UNDO) {
            ctx.habit_undo = self.habit_undo.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_MOVE) {
            ctx.habit_move = self.habit_move.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_MOVE_UP) {
            ctx.habit_move_up = self.habit_move_up.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_MOVE_DOWN) {
            ctx.habit_move_down = self.habit_move_down.take();
        }
        if wants.wants(crate::router::OneShotSet::TASK_MOVE) {
            ctx.task_move = self.task_move.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_REPAIR) {
            ctx.habit_repair = self.habit_repair.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_CONVERT_TARGET) {
            ctx.habit_convert_target = self.habit_convert_target.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_REMOVE_TARGET) {
            // The staged remove's armed subject (H4 D4): the arm
            // recorded it; the confirm consumes the one-shot (the
            // armed state clears with the dispatch — a repeat confirm
            // is the disarmed `NoTarget`).
            ctx.habit_remove_target = self.habit_remove_armed.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_PAUSE_TARGET) {
            ctx.habit_pause_target = self.habit_pause_target.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_CREATE_DRAFT) {
            ctx.habit_create_draft = self.habit_create_draft.take();
        }
        if wants.wants(crate::router::OneShotSet::HABIT_QUOTA_DRAFT) {
            ctx.habit_quota_draft = self.habit_quota_draft.take();
        }
        if wants.wants(crate::router::OneShotSet::PATTERN_DRAFT) {
            ctx.pattern_draft = self.pattern_draft.take();
        }
        if wants.wants(crate::router::OneShotSet::SERIES_TARGET) {
            ctx.series_target = self.series_target.take();
        }
        if wants.wants(crate::router::OneShotSet::PAUSE_DRAFT) {
            ctx.pause_draft = self.pause_draft.take();
        }
        if wants.wants(crate::router::OneShotSet::CONVERT_TARGET) {
            ctx.convert_target = self.convert_target.take();
        }
        if wants.wants(crate::router::OneShotSet::RECUR_EDIT_TARGET) {
            ctx.recur_edit_target = self.recur_edit_target.take();
        }
        if wants.wants(crate::router::OneShotSet::RECUR_DRAFT_RULE) {
            ctx.recur_draft_rule = self.recur_draft_rule.take();
        }
        // The today window rides every dispatch (chunk 10 D4): state-
        // derived from the held pair, never consumed.
        ctx.day_window = Some(self.day_ctx().window);
        // The today re-commit stamps' fact (#893, the chunk plan
        // D3.2): the window's last covered instant over the same
        // frozen day facts — the domain-owned mint, never a local
        // `end − ε` re-derivation (the #884 rule).
        ctx.today_last_instant = Some(taskboard_domain::days::last_covered_instant(
            self.day_ctx().window.1,
        ));
        // The schedule quick verbs' pair-write facts (#1428, journey
        // plan D-1/D-2): today's and tomorrow's first covered instants
        // (the day_start_time-aware window starts) and tomorrow's last
        // covered instant (the covered-instant law — the raw exclusive
        // end is retired). One domain mint per fact, no local
        // arithmetic.
        ctx.today_first_instant = Some(self.day_ctx().window.0);
        let tomorrow = taskboard_domain::days::next_day_window(
            self.day_ctx().now,
            self.day_start.0,
            self.day_start.1,
        );
        ctx.tomorrow_first_instant = Some(tomorrow.0);
        ctx.tomorrow_last_instant = Some(taskboard_domain::days::last_covered_instant(tomorrow.1));
        // The smart toggle's state half (J3 R-8, round 9 chunk 11):
        // synced at dispatch time — the fact must answer about the
        // exact resolved target, which only exists after the one-shot
        // move above (the shell sync cannot see it). The read is
        // `send_card_track_toggle`'s re-rooted (one semantics, one
        // home — the router arm consumes it).
        if id == ActionId::TaskTrackToggle {
            let open = crate::presentation::tracking_open_task(&self.state.load());
            ctx.focused_task_tracking =
                open.is_some_and(|open| Some(open) == ctx.track_target.or(ctx.focused_task));
        }
        // The habit toggle's state half (#1305, the twin read): answers
        // about the exact resolved target — the one-shot `habit_target`
        // minted above, else the focused habit.
        if id == ActionId::HabitTrackToggle {
            let state = self.state.load();
            ctx.habit_tracking = ctx
                .habit_target
                .is_some_and(|habit| habit_open_entry(&state, habit).is_some());
        }
        let outcome = ActionRouter::dispatch(id, &ctx);
        match outcome {
            DispatchOutcome::Command(command) => {
                // T-41/T-43 (#824/#826): the acknowledgement rides the
                // status-notice lane (the apply_outcomes twin).
                if self.arm_outcome_notice(&command) {
                    self.push_notice(window);
                }
                // Non-blocking, human-input-scale (D16): a full inbox
                // is logged and dropped — the forwarder convention.
                if self.commands.try_send(command).is_err() {
                    tracing::warn!(action = %id, "command channel full or closed; dropped");
                }
            }
            DispatchOutcome::UiEffect(effect) => self.apply_effect(window, effect),
            // The typed refusal's uniform law (chunk 8 D6, #935 /
            // O-7 Q5): a target-less dispatch surfaces on the
            // status-notice lane from the action's registry
            // description — one law at one place, not per-verb spot
            // fixes (the subjectless habit verb, the stale one-shot
            // and the un-armed convert are covered by construction;
            // US-14's R-8 resolution gave the Shift+Up-on-a-task-row
            // stroke its task twins, so the residual NoTarget is a
            // focusless stroke). `Disabled` keeps its posture:
            // the disabled-row notice is already armed at the
            // registry arm (invoke_entry's T-19 twin).
            DispatchOutcome::Disabled => {}
            DispatchOutcome::NoTarget => {
                self.arm_no_target_notice(id);
                self.push_notice(window);
            }
        }
        self.finish_mutation(window);
    }

    /// The dismiss-all tooltip primitive (r2 D13): the global's
    /// existing `unhover()` — invoked from the effect-application
    /// points where the anchor is about to unmount or a modal context
    /// takes over. Rule: a tooltip never outlives the element that
    /// spawned it.
    pub(in crate::adapter) fn dismiss_tooltip(window: &AppWindow) {
        window.global::<crate::TooltipState>().invoke_unhover();
    }

    // === The focus-origin fact (the focus-restoration plan US-1/W-1,
    // === D-F1; the chunk plan §2 census table is the site list) ====

    /// The shared post-mutation sync (r2 D12): the view-context seam
    /// realignment plus the shell → window global push. `route`'s
    /// trailing pair, factored out so the mouse menu paths
    /// (`activate_parent`, `apply_close_menu`, the `MenuBack` arm)
    /// update the same globals the keyboard path updates — the parent
    /// rows' `menu-active-group` used to stay stale (F7).
    /// The currently open census site, if any (the funnel-side read:
    /// recording and restoring both key on this, so no exit arm can
    /// be forgotten — the `apply_outcomes` pass-tail logic). The chain
    /// checks keep a nested child level (the due editor's picker
    /// phase, the walk's pickers) recognized as its parent site: the
    /// child's exit arms restore *into* the parent (the census's
    /// "the picker exits inward" letter).
    pub(in crate::adapter) fn open_focus_site(&self, window: &AppWindow) -> Option<FocusSeam> {
        use crate::view_state::Flow;
        let chained = |pred: &dyn Fn(&Flow) -> bool| self.view_state.chain_has_flow(pred);
        // Child-bearing sites first (deeper levels own the keyboard).
        if chained(&|f| {
            matches!(
                f,
                Flow::ReviewWalk { .. }
                    | Flow::WalkFilePick { .. }
                    | Flow::WalkDatePick
                    | Flow::EntryDatePicker
            )
        }) {
            return Some(FocusSeam::ReviewWalk);
        }
        if chained(&|f| matches!(f, Flow::TrackEntries { .. } | Flow::TrackEntryForm { .. })) {
            return Some(FocusSeam::TrackEntries);
        }
        if chained(&|f| matches!(f, Flow::Labels { .. })) {
            return Some(FocusSeam::LabelsOverlay);
        }
        if chained(&|f| matches!(f, Flow::EditRecur { .. })) {
            return Some(FocusSeam::RecurEditor);
        }
        if chained(&|f| {
            matches!(
                f,
                Flow::EditDate {
                    field: crate::view_state::DateField::Due,
                    ..
                }
            )
        }) {
            return Some(FocusSeam::DueEditor);
        }
        if chained(&|f| {
            matches!(
                f,
                Flow::EditDate {
                    field: crate::view_state::DateField::Scheduled,
                    ..
                }
            )
        }) {
            return Some(FocusSeam::ScheduledEditor);
        }
        if chained(&|f| matches!(f, Flow::EditTitle { .. })) {
            return Some(FocusSeam::TitleEditor);
        }
        if chained(&|f| matches!(f, Flow::EditDescription { .. })) {
            return Some(FocusSeam::DescriptionEditor);
        }
        if chained(&|f| matches!(f, Flow::PlanPlace { .. })) {
            return Some(FocusSeam::PlanPlace);
        }
        if chained(&|f| matches!(f, Flow::Palette { .. })) {
            return Some(FocusSeam::Palette);
        }
        if chained(&|f| matches!(f, Flow::Capture)) {
            return Some(FocusSeam::Capture);
        }
        if chained(&|f| matches!(f, Flow::FocusMode { .. })) {
            return Some(FocusSeam::FocusOverlay);
        }
        // The create/rename flows are deliberately NOT census sites
        // (the recorded deviation from the chunk plan's §2 row 16):
        // the inline create row's commit is the braindump grammar —
        // its exit re-seeds the walk through reconcile, and a restore
        // armed beside it fights that re-seed (the ten-step journey's
        // walk state). The rename cells' exits keep today's letter for
        // the same reason: the cell itself is the re-entry point the
        // flow arms in place.
        if chained(&|f| {
            matches!(
                f,
                Flow::Move { .. } | Flow::MoveStack { .. } | Flow::DragMove { .. }
            )
        }) {
            return Some(FocusSeam::MoveFlow);
        }
        if chained(&|f| matches!(f, Flow::ConfirmDelete(_))) {
            return Some(FocusSeam::ConfirmDelete);
        }
        if chained(&|f| matches!(f, Flow::BoardColorPick { .. })) {
            return Some(FocusSeam::BoardColorPick);
        }
        match self.priority_open {
            Some(crate::router::PriorityField::Importance) => {
                return Some(FocusSeam::PriorityDue);
            }
            Some(crate::router::PriorityField::Urgency) => {
                return Some(FocusSeam::PriorityScheduled);
            }
            None => {}
        }
        if window.get_timer_setter_open() {
            return Some(FocusSeam::TimerSetter);
        }
        if self.shell.menu.open {
            return Some(FocusSeam::ActionMenu);
        }
        None
    }

    /// Whether the control walk clamps (US-2/W-2, D-W2): the masked
    /// host set is the census's modal hosts — the palette, capture,
    /// timer setter, focus overlay, track entries, due editor, recur
    /// editor, and labels overlay. The action menu and the review walk
    /// own their key grammars outright (their row walks are not the
    /// control walk), so they stay off this list.
    pub(in crate::adapter) fn mask_armed(&self, window: &AppWindow) -> bool {
        matches!(
            self.open_focus_site(window),
            Some(
                FocusSeam::Palette
                    | FocusSeam::Capture
                    | FocusSeam::TimerSetter
                    | FocusSeam::FocusOverlay
                    | FocusSeam::TrackEntries
                    | FocusSeam::DueEditor
                    | FocusSeam::RecurEditor
                    | FocusSeam::LabelsOverlay
            )
        )
    }

    /// The open pass's recording half: the anchor is the invoking
    /// element's identity at record time — the logical walk focus,
    /// else the subject (subject-affine sites), else the view. The
    /// stack/board variants are the *restore* vocabulary (the census's
    /// header anchors resolve against them); the generic ladder never
    /// mints them — a plain view context with a seeded selection must
    /// restore to the view, not silently jump to a column.
    pub(in crate::adapter) fn mint_focus_origin(&self, seam: FocusSeam) -> FocusOrigin {
        let state = self.state.load_full();
        let anchor = if let Some(task) = self.view_state.focused_task() {
            OriginAnchor::Task(task)
        } else if seam.subject_affine() && self.view_state.subject_task().is_some() {
            OriginAnchor::Task(self.view_state.subject_task().expect("checked above"))
        } else {
            OriginAnchor::View
        };
        // The container snapshot: a board row falls to its stack
        // header; every other anchor's container is the view scope
        // (the plan AC-2's letter — "the row's stack header / the
        // view root").
        let fallback = match anchor {
            OriginAnchor::Task(task) => {
                if self.view_state.view() == crate::view_state::ViewId::Board {
                    let stack_ids = self.view_state.stack_ids(&state);
                    self.tables
                        .board_position(task)
                        .and_then(|(column, _)| stack_ids.get(column).copied())
                        .map_or(OriginAnchor::View, OriginAnchor::Stack)
                } else {
                    OriginAnchor::View
                }
            }
            OriginAnchor::Stack(_) | OriginAnchor::View => OriginAnchor::View,
        };
        FocusOrigin {
            seam,
            anchor,
            fallback,
        }
    }

    /// One restore resolution: `Some` names the anchor family function
    /// to invoke (and the logical-focus writes the row anchor needs —
    /// rows carry no slint focus stop; the walk's emphasis look is the
    /// row focus, so a `Task` anchor re-arms the walk state and the
    /// scroll hint beside the fact push). `None` = the anchor is gone
    /// at every level; the caller falls to the view-scope refocus.
    #[allow(clippy::type_complexity)] // one resolution shape, local to the restore
    pub(in crate::adapter) fn resolve_anchor(
        &self,
        state: &taskboard_domain::AppState,
        anchor: &OriginAnchor,
    ) -> Option<(i32, i32, i32)> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let to_index = |value: usize| value as i32;
        match *anchor {
            OriginAnchor::View => None,
            OriginAnchor::Task(task) => {
                if !state.tasks.contains_key(&task) {
                    return None;
                }
                match self.view_state.view() {
                    crate::view_state::ViewId::Board => {
                        let (column, row) = self.tables.board_position(task)?;
                        Some((1, to_index(column), to_index(row)))
                    }
                    crate::view_state::ViewId::Today => {
                        let (section, row) = self.tables.today_position(task)?;
                        Some((1, to_index(section), to_index(row)))
                    }
                    crate::view_state::ViewId::StackDetail => {
                        let row = self.tables.stack_position(task)?;
                        Some((1, -1, to_index(row)))
                    }
                    // The detail's own cell is the row anchor's host
                    // (census rows 14/15's "the title/description cell").
                    crate::view_state::ViewId::TaskDetail => Some((4, -1, -1)),
                    _ => None,
                }
            }
            OriginAnchor::Stack(stack) => {
                let column = self
                    .view_state
                    .stack_ids(state)
                    .iter()
                    .position(|s| *s == stack)?;
                Some((2, to_index(column), -1))
            }
        }
    }

    /// The restore's fact half: resolve the origin (anchor, then the
    /// recorded container), publish the `FocusFacts` tuple through the
    /// anchor family's seam, and land the logical half of a row
    /// restore. Returns whether an element-level restore happened —
    /// `false` means the caller's view-scope refocus fires (D-F1's
    /// ordering law: the view-scope call only runs when the origin
    /// resolved to the view).
    pub(in crate::adapter) fn push_focus_restore(
        &mut self,
        window: &AppWindow,
        origin: &FocusOrigin,
    ) -> bool {
        let state = self.state.load_full();
        let fallback = origin.fallback;
        let Some((mut kind, a, b)) = self
            .resolve_anchor(&state, &origin.anchor)
            .or_else(|| self.resolve_anchor(&state, &fallback))
        else {
            return false;
        };
        // The detail's per-seam cell table (round 5 A2, #1205's
        // mechanism B): a `Task` anchor at `TaskDetail` resolves to
        // kind 4 — the census's single detail cell — and the opener's
        // identity used to be discarded there (every overlay restored
        // onto the title). The seam names the opener's own cell; the
        // view carries the matching claimants. Only the detail's
        // collapse is remapped: overlays opened from row contexts
        // keep their row anchors (kinds 1–3).
        if kind == 4 {
            kind = match origin.seam {
                FocusSeam::DescriptionEditor => 5,
                FocusSeam::DueEditor => 7,
                FocusSeam::ScheduledEditor => 8,
                FocusSeam::PriorityDue => 9,
                FocusSeam::PriorityScheduled => 10,
                FocusSeam::LabelsOverlay => 11,
                FocusSeam::RecurEditor => 12,
                FocusSeam::TrackEntries => 13,
                // The seams with no detail cell of their own (the
                // title editor — kind 4 is its own cell — the effort
                // opener's kind-6 push, the modals) keep the resolved
                // cell.
                _ => 4,
            };
        }
        let facts = window.global::<crate::FocusFacts>();
        facts.set_seam_id(origin.seam.id());
        match kind {
            1 => {
                // The logical half: the walked look lands on the row
                // (the emphasis is the row's focus render) and the
                // scroll hint brings it into view — the walk-stop
                // sibling the census names.
                let task = match (origin.anchor, fallback) {
                    (OriginAnchor::Task(task), _) | (_, OriginAnchor::Task(task)) => Some(task),
                    _ => None,
                };
                if let Some(task) = task.filter(|t| state.tasks.contains_key(t)) {
                    self.view_state.set_focused(Some(task));
                }
                window.invoke_focus_task_row(a, b);
            }
            2 => {
                // The stack header's selection look (the segment ring's
                // selected branch) — the header's own anchor.
                if let OriginAnchor::Stack(stack) = origin.anchor {
                    self.view_state.select_stack(&state, stack);
                }
                window.invoke_focus_stack_header(a);
            }
            3 => window.invoke_focus_board_cell(a),
            4..=13 => window.invoke_focus_detail_cell(kind),
            _ => return false,
        }
        // The row restore's scroll-into-view half (the FocusScrollHint
        // consumer pairs with the fact push).
        if kind == 1 {
            // The empty `before` triple arms unconditionally (the
            // row-restore's deliberate always-seek; A5a D-2 widened
            // the capture type only).
            self.push_focus_scroll_hint(window, crate::view_state::WalkFocus::default());
        }
        true
    }

    /// The sync-time funnel (`sync_view_context` runs at the end of
    /// every mutation pass): record on open, restore on close. The
    /// restore's `false` here needs no view-scope call — the closes
    /// that only ride this funnel (the flag/property sites) had no
    /// flow to end and no identity to swap; today's behavior for them
    /// was "nothing", and the element restore is the whole delta.
    pub(in crate::adapter) fn sync_focus_origin(&mut self, window: &AppWindow) {
        if self.applying_outcomes {
            return;
        }
        // The walk clamp's armed push (US-2/W-2): the flag is the site
        // table's read, published every pass so the hosts and the
        // tests see the current mask without a walk dispatch.
        window
            .global::<crate::MaskWalkFacts>()
            .set_armed(self.mask_armed(window));
        if let Some(origin) = self.focus_origin.take() {
            if self.open_focus_site(window) == Some(origin.seam) {
                // Still open (a refresh pass mid-site): keep holding.
                self.focus_origin = Some(origin);
            } else {
                self.push_focus_restore(window, &origin);
            }
        }
        if let Some(seam) = self
            .open_focus_site(window)
            .filter(|open| self.focus_origin.as_ref().is_none_or(|o| o.seam != *open))
        {
            // Last-open-wins: a second site's open overwrites.
            self.focus_origin = Some(self.mint_focus_origin(seam));
        }
    }

    /// The exit funnels' focus half (the D-F1 ordering law): consume
    /// the origin when its site is gone; `true` = an element-level
    /// restore fired and the caller's view-scope refocus is suppressed.
    pub(in crate::adapter) fn exit_focus_handoff(&mut self, window: &AppWindow) -> bool {
        if self.open_focus_site(window).is_none()
            && let Some(origin) = self.focus_origin.take()
        {
            return self.push_focus_restore(window, &origin);
        }
        false
    }

    pub(in crate::adapter) fn finish_mutation(&mut self, window: &AppWindow) {
        self.sync_view_context(window);
        self.sync(window);
    }

    /// The context chain under the pulled scope facts (round-5 chunk
    /// 7, D-KS1): the declarative stack construction that replaced
    /// `KeymapShell::chain`'s boolean reconstruction. Every input is
    /// re-read at the call — no stored registrations can outlive
    /// their surface (the D-KS1 pull rule). The deltas arm only at
    /// the key doorway; hint/refresh passes call the `None`/`false`
    /// form and get the byte-legacy chain.
    pub(in crate::adapter) fn current_chain(&self, window: &AppWindow) -> Vec<ContextId> {
        self.chain_with(window, None, false)
    }

    /// [`Self::current_chain`] with the key doorway's pending stroke
    /// and doorway flag (the D-δ1 exemption and the delta arms).
    pub(in crate::adapter) fn chain_with(
        &self,
        window: &AppWindow,
        stroke: Option<crate::keymap::Stroke>,
        at_key_doorway: bool,
    ) -> Vec<ContextId> {
        let flow = self.view_state.flow().clone();
        let chain = self.view_state.chain_tags();
        let editor_focus = window
            .global::<crate::EditingFacts>()
            .get_editor_focus_count()
            > 0;
        crate::keyscope::scope_stack(&crate::keyscope::ScopeInputs {
            shell: &self.shell,
            picker_open: self
                .view_state
                .date_editor()
                .is_some_and(|editor| editor.picker.is_some()),
            editor_focus,
            flow: &flow,
            chain: &chain,
            stroke,
            at_key_doorway,
        })
    }

    /// Keeps the shell's view seam aligned with the view-state
    /// machine (the D16/D17 seam, now driven for real).
    pub(in crate::adapter) fn sync_view_context(&mut self, window: &AppWindow) {
        // The focus-origin funnel (US-1/W-1): record on open, restore
        // on close — before the shell mirror, so the restore's logical
        // writes (the walked look, the stack selection) land in the
        // same push the pass derives.
        self.sync_focus_origin(window);
        let state = self.state.load();
        self.shell.view = Some(self.view_state.context());
        self.shell.focused_task = self.view_state.focused_task();
        self.shell.focused_stack = self.view_state.focused_stack(&state);
        // The modal masks' truth is chain-wide (chunk 3): a child
        // level holding the chain's top must not unmask its parent —
        // the walk's context stays armed while its File-to or
        // Pick-a-date child is on top.
        self.shell.palette = self
            .view_state
            .chain_has_flow(|flow| matches!(flow, crate::view_state::Flow::Palette { .. }));
        self.shell.focus_overlay = self
            .view_state
            .chain_has_flow(|flow| matches!(flow, crate::view_state::Flow::FocusMode { .. }));
        self.shell.track_entries = self.view_state.chain_has_flow(|flow| {
            matches!(
                flow,
                crate::view_state::Flow::TrackEntries { .. }
                    | crate::view_state::Flow::TrackEntryForm { .. }
            )
        });
        self.shell.review_walk = self
            .view_state
            .chain_has_flow(|flow| matches!(flow, crate::view_state::Flow::ReviewWalk { .. }));
        // The notification surface's engagement mirror (US-6/W-6,
        // D-N1): armed while the cursor holds AND rows exist; rows
        // emptying disarms (the posture's letter).
        if self.tables.notifications.is_empty() {
            self.notification_cursor = None;
        } else if let Some(cursor) = self.notification_cursor {
            self.notification_cursor = Some(cursor.min(self.tables.notifications.len() - 1));
        }
        self.shell.notification_surface = self.notification_cursor.is_some();
        // The export dialog's mirror (the J2 export plan W-1): the
        // adapter-owned VM's truth (not a flow), synced like the
        // modal booleans above.
        self.shell.export_dialog = self.export_vm.is_some();
        // Round 2 (D19): the reopen gate's done stamp, from the same
        // focused task — the router stays pure.
        self.shell.focused_task_done = self
            .view_state
            .focused_task()
            .and_then(|id| state.tasks.get(&id))
            .is_some_and(|task| task.done.is_some());
        // The kind flag (issue #492): the complete verb's router guard
        // and the toggle's inversion read the same focused task.
        self.shell.focused_task_tracker = self
            .view_state
            .focused_task()
            .and_then(|id| state.tasks.get(&id))
            .is_some_and(|task| task.kind == taskboard_domain::entities::TaskKind::Tracker);
        // The scheduled-day fact (UX-R8 chunk-5 D-3): task.unschedule's
        // route gate — the focused_task_done shape.
        self.shell.focused_task_scheduled = self
            .view_state
            .focused_task()
            .and_then(|id| state.tasks.get(&id))
            .is_some_and(|task| task.scheduled.is_some());
        // The effort estimate's presence (J3 US-4, round 9 chunk 11):
        // task.effort-clear's route gate — the scheduled-day fact's
        // shape.
        self.shell.focused_task_effort = self
            .view_state
            .focused_task()
            .and_then(|id| state.tasks.get(&id))
            .is_some_and(|task| task.effort.is_some());
    }

    /// The geometry capture (#412, D4): samples the window's normal
    /// state once per cadence tick and emits one full-snapshot
    /// request when it changed. Guard chain, in order: never under
    /// the kiosk profile (D6 — fullscreen owns the appliance's
    /// geometry; the path is bypassed, not special-cased); never
    /// maximized/minimized/fullscreen (a maximized window's bounds
    /// are the workspace's, not the user's normal geometry, and
    /// minimized geometry is stale/zero on several platforms); the
    /// mint refuses degenerate snapshots silently (which keeps the
    /// headless default 0×0 quiet); the dedupe baseline swallows
    /// unchanged re-samples. A full or closed request inbox drops the
    /// request with the baseline unset — the next tick retries.
    pub(crate) fn capture_window_geometry(&mut self, window: &AppWindow) {
        if self.kiosk {
            return;
        }
        let slint_window = window.window();
        if slint_window.is_maximized()
            || slint_window.is_minimized()
            || slint_window.is_fullscreen()
        {
            return;
        }
        let position = slint_window.position();
        let size = slint_window.size();
        let Some(geometry) = crate::geometry::WindowGeometry::new(
            i64::from(position.x),
            i64::from(position.y),
            i64::from(size.width),
            i64::from(size.height),
        ) else {
            return;
        };
        if self.last_window_geometry == Some(geometry) {
            return;
        }
        let request = crate::requests::UiRequest::WindowGeometryChanged { geometry };
        if self.requests.try_send(request).is_ok() {
            self.last_window_geometry = Some(geometry);
        } else {
            tracing::warn!("request channel full or closed; geometry capture dropped");
        }
    }
}

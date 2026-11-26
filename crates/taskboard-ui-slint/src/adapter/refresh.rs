// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Arc;

use chrono::{DateTime, Utc};

use slint::{ComponentHandle as _, ModelRc, SharedString};

use taskboard_domain::{AppState, TaskId};

use super::Adapter;
use super::{CreateFocusArm, FlowLiveness, FocusHandover, MenuKind};
use crate::actions::ActionId;
use crate::level_state::TrackFormState;
use crate::menu::{self};
use crate::router::UiEffect;
use crate::view_state::{Flow, FlowOutcome};
use crate::{AppWindow, Breakpoint, KeyboardState, Responsive};

impl Adapter {
    /// Applies flow outcomes verbatim (D9): the adapter never
    /// re-decides. `RefreshNow` re-derives the models immediately —
    /// the timer-driven refresh (D1) covers the passive side. A mode
    /// flip is pushed onto the window global here (the shell's one-way
    /// sync, D18): the text-commit seams end in this method without a
    /// `route()` around them, and the visible chain (the status bar's
    /// mode read) must not go stale.
    #[cfg(test)]
    pub(crate) fn apply_outcomes_for_test(&mut self, window: &AppWindow, outcomes: &[FlowOutcome]) {
        self.apply_outcomes(window, outcomes);
    }

    pub(in crate::adapter) fn apply_outcomes(
        &mut self,
        window: &AppWindow,
        outcomes: &[FlowOutcome],
    ) {
        let mut mode_changed = false;
        self.applying_outcomes = true;
        // The clip-focus law's echo window for the OUTCOME pushes
        // (#1419 option A): applying outcomes writes the slint models
        // exactly like a refresh push (a flow opening here rebuilds
        // the field subtrees), so its deferred focus-lost echo is the
        // synthetic unmount echo too. Arm the flag unless a push pass
        // already holds it; like the pass arm, it disarms on the
        // first focus-lost or the first user-intent seam — never on
        // this function's exit.
        self.refresh_push_in_progress.set(true);
        let flow_ended =
            self.flow_open == FlowLiveness::Open && matches!(self.view_state.flow(), Flow::None);
        for outcome in outcomes {
            match outcome {
                FlowOutcome::Command(command) => {
                    // T-41/T-43 (#824/#826): the acknowledgement rides
                    // the status-notice lane — the typed fact arms at
                    // dispatch time (the disabled-row notice's
                    // event-stamp shape).
                    if self.arm_outcome_notice(command) {
                        self.push_notice(window);
                    }
                    // The create-focus handoff's arming (UX-R8 chunk-5
                    // D-7, US-5): the subject stack's live ids at
                    // commit time — the empty-buffer settle never
                    // emits the command (view_state's Flow::Create
                    // guard), so it arms nothing.
                    if let taskboard_domain::StateCommand::CreateTask { stack, .. } = command {
                        let state = self.state.load_full();
                        let before: std::collections::BTreeSet<TaskId> = state
                            .tasks
                            .values()
                            .filter(|task| task.stack == *stack)
                            .map(|task| task.id)
                            .collect();
                        let snapshot = Arc::clone(&state);
                        drop(state);
                        self.create_focus_arm = Some(CreateFocusArm {
                            stack: *stack,
                            before,
                            snapshot,
                        });
                    }
                    if self.commands.try_send(command.clone()).is_err() {
                        tracing::warn!("command channel full or closed; dropped");
                    }
                }
                FlowOutcome::SetMode(mode) => {
                    self.shell.apply(&UiEffect::SetMode(*mode), 0);
                    mode_changed = true;
                }
                // The view machine's re-derive verdict: forced — the
                // derive inputs are adapter-side, the pointer is not.
                FlowOutcome::RefreshNow => self.refresh_forced(window),
                FlowOutcome::StackWidthsChanged => {
                    // The persist-now signal (round-1 D21): the full
                    // per-board snapshot over the request lane,
                    // warn-and-drop on a full/closed inbox — the next
                    // event retries.
                    if let Some(board) = self.view_state.board() {
                        let request = crate::requests::UiRequest::StackWidthsChanged {
                            board,
                            widths: self.view_state.stack_widths_of(board),
                        };
                        if self.requests.try_send(request).is_err() {
                            tracing::warn!("request channel full or closed; dropped");
                        }
                    }
                }
            }
        }
        if mode_changed {
            self.sync(window);
        }
        // The D15 focus hand-off: a flow's end unmounts the field that
        // held slint focus; the active view scope takes it back (the
        // §7.3 journey caught the keyboard dead-end).
        self.flow_open = if matches!(self.view_state.flow(), Flow::None) {
            FlowLiveness::Flowless
        } else {
            FlowLiveness::Open
        };
        if flow_ended {
            // The D20 reset seam 1 (flow ends): the funnel that drives
            // `refocus_view` is where a focused field unmounts — the
            // runtime never fires its loss event, so the counts would
            // stick and mask the shell forever.
            Self::reset_editing_facts(window);
            // The focus-origin restore first (US-1/W-1, D-F1's
            // ordering law): the view-scope refocus only fires when
            // the origin resolved to the view — an element-level
            // restore lands on the origin, not the window top.
            if !self.exit_focus_handoff(window) {
                window.invoke_refocus_view();
            }
        }
        self.applying_outcomes = false;
    }

    /// The D20 reset seams (see the flow-end call): both counts to 0
    /// wherever the tree changes without delivering loss events. A
    /// field that mounts focused re-publishes on its init focus grab,
    /// so a reset never strands an engaged field.
    pub(in crate::adapter) fn reset_editing_facts(window: &AppWindow) {
        let facts = window.global::<crate::EditingFacts>();
        facts.set_text_focus_count(0);
        facts.set_activation_focus_count(0);
        facts.set_editor_focus_count(0);
        // The generation bump tells every live publisher the counts
        // were re-baselined, so its next focus event re-syncs its
        // latch instead of paying a stale decrement.
        facts.set_generation(facts.get_generation() + 1);
    }

    /// The per-refresh settings mirror (round-5 chunk 5, D-5.7): every
    /// settings-derived input the pass reads comes from the published
    /// snapshot, so a committed setting is visible at the next refresh
    /// — the publish itself swaps the state Arc, which is what makes
    /// [`Self::refresh`] run the forced pass.
    pub(in crate::adapter) fn sync_settings(
        &mut self,
        settings: &taskboard_domain::SettingsSnapshot,
    ) {
        self.day_start = (settings.day_start_time, self.boot_offset);
        self.due_soon_horizon = settings.due_soon_days.horizon();
        self.reminder_defaults = settings.reminder_defaults();
        // A kiosk never carries a timing (the spawn exclusion's first
        // layer; the overlay's action gate re-checks anyway).
        self.focus = if self.kiosk {
            None
        } else {
            settings.live_focus()
        };
    }

    /// The injected day-boundary vocabulary ([`Self::day_ctx`]'s
    /// sibling): the held `day_start` pair as the domain `DayCalendar`
    /// the recurring badge's derivations read (DR-6/4).
    pub(crate) fn calendar(&self) -> taskboard_domain::DayCalendar {
        taskboard_domain::DayCalendar {
            day_start_time: self.day_start.0,
            offset: self.day_start.1,
        }
    }

    /// The pass's time bundle (chunk 10 D4): a fresh `now` and the
    /// window derived from the held pair — recomputed on every pass,
    /// so the rollover rides the existing minute-boundary repaint.
    /// The pinned day anchor the history dialog's month mounts at
    /// (the subject opener's computation, shared by the palette's
    /// search-row arm — one mechanism, W-10).
    pub(crate) fn today_date(&self) -> chrono::NaiveDate {
        taskboard_domain::DayCalendar {
            day_start_time: self.day_start.0,
            offset: self.day_start.1,
        }
        .today(self.day_ctx().now)
    }

    pub(crate) fn day_ctx(&self) -> crate::view_state::DayCtx {
        let now = self.frozen_now.unwrap_or_else(chrono::Utc::now);
        let window = taskboard_domain::days::day_window(now, self.day_start.0, self.day_start.1);
        crate::view_state::DayCtx {
            now,
            window,
            calendar: taskboard_domain::DayCalendar {
                day_start_time: self.day_start.0,
                offset: self.day_start.1,
            },
        }
    }
    /// The placement commit funnel (#622, D-3/D-8): parse the two
    /// buffers through the due-editor's parsers verbatim, then route
    /// ONE `UpdateTask` with both masks (the effort mask present
    /// only when entered — untouched otherwise), one undo step. A
    /// parse refusal dispatches nothing and keeps the typed text.
    pub(in crate::adapter) fn commit_plan_place(&mut self, task: TaskId) -> Vec<FlowOutcome> {
        let Some(place) = self.view_state.plan_place() else {
            return Vec::new();
        };
        let Some(time) = crate::due_edit::parse_time_lenient(&place.time_text) else {
            return Vec::new();
        };
        let effort = if place.effort_text.trim().is_empty() {
            None
        } else {
            match taskboard_domain::Effort::parse_text_form(&place.effort_text) {
                Some(effort) => Some(effort),
                None => return Vec::new(),
            }
        };
        let day = self.day_ctx();
        let cal = self.calendar();
        let (next_start, _) =
            taskboard_domain::days::next_day_window(day.now, self.day_start.0, self.day_start.1);
        let date = cal.calendar_date(next_start);
        let instant = cal.instant(date, time);
        // The commit's consume is the flow's end — the level drops
        // with its buffers (D-H2).
        self.view_state.end_plan_place();
        vec![
            FlowOutcome::Command(taskboard_domain::StateCommand::UpdateTask {
                id: task,
                changes: taskboard_domain::TaskChanges {
                    scheduled: Some(Some(instant)),
                    effort: effort.map(Some),
                    ..taskboard_domain::TaskChanges::default()
                },
            }),
            FlowOutcome::RefreshNow,
        ]
    }

    /// The box swap (#622, D-3): the addressed box's scheduled
    /// instant and the temporal neighbor's exchange — two honest
    /// `UpdateTask` writes (each journal-armed); the instant set is
    /// invariant, so reorder never creates or needs arithmetic. An
    /// edge box (no neighbor that way) or a boxless neighbor is
    /// inert.
    pub(in crate::adapter) fn swap_plan_box(
        &mut self,
        task: TaskId,
        earlier: bool,
        state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        let day = self.day_ctx();
        let placed = taskboard_domain::days::plan_next_day(
            &state.tasks,
            day.now,
            self.day_start.0,
            self.day_start.1,
        )
        .placed_scheduled;
        let Some(index) = placed.iter().position(|id| *id == task) else {
            return Vec::new();
        };
        let neighbor = if earlier {
            if index == 0 {
                return Vec::new();
            }
            placed[index - 1]
        } else {
            let Some(neighbor) = placed.get(index + 1) else {
                return Vec::new();
            };
            *neighbor
        };
        let mine = state.tasks.get(&task).and_then(|t| t.scheduled);
        let theirs = state.tasks.get(&neighbor).and_then(|t| t.scheduled);
        let (Some(mine), Some(theirs)) = (mine, theirs) else {
            return Vec::new();
        };
        let swap = |id: TaskId, instant: chrono::DateTime<chrono::Utc>| {
            FlowOutcome::Command(taskboard_domain::StateCommand::UpdateTask {
                id,
                changes: taskboard_domain::TaskChanges {
                    scheduled: Some(Some(instant)),
                    ..taskboard_domain::TaskChanges::default()
                },
            })
        };
        vec![
            swap(task, theirs),
            swap(neighbor, mine),
            FlowOutcome::RefreshNow,
        ]
    }

    /// The per-second tick's single gate (r4 D17, §2 decision 5): the
    /// one place that decides whether any mounted surface's displayed
    /// value changes with the wall second. **This function is the
    /// single tick channel** — chunks 8/9 (countdown rows) extend THIS
    /// term; never introduce a second tick channel.
    ///
    /// D17's term today: the mounted task view's subject holds any
    /// open entry — only then does the displayed total change per
    /// second (a foreign task's open entry does not move the row). The
    /// verb button may show Start during the transient double-open
    /// while the subject's older open entry still accrues — the tick
    /// follows the arithmetic truth, the button follows the D-T8
    /// display pick; recorded. Chunks 8/9's terms (r4 D30/D32): the
    /// subject's At-kinded rule rows and the timers surfaces are the
    /// seconds-changing extensions — chunk 8 landed the rule-row term
    /// below.
    ///
    /// Kiosk exclusion, three layers: (a) structural — kiosk pins the
    /// view to Board, the task view never mounts; (b) the explicit
    /// `!self.kiosk` term — freezes the decision even if kiosk view
    /// gating ever changes; (c) the driver only consults the wall
    /// second while this flag holds. Layer (b) is the test-pinned one.
    pub(in crate::adapter) fn compute_seconds_surfaces_active(
        &mut self,
        state: &AppState,
        now: DateTime<Utc>,
    ) {
        let mut active = false;
        if !self.kiosk {
            // The concentration overlay's countdown (#496): the
            // remaining-time text changes with the wall second while
            // the flow holds — THIS term is the mandated extension
            // point (the doc above); never a second tick channel.
            // Unreachable on kiosk by the three-layer exclusion (the
            // flow cannot open there), frozen by this explicit
            // `!self.kiosk` term as before.
            let focus_overlay_active = matches!(
                self.view_state.flow(),
                crate::view_state::Flow::FocusMode { .. }
            );
            // The task view's elapsed total (#463's gate, D17).
            let task_view_active = self.view_state.view() == crate::view_state::ViewId::TaskDetail
                && self.view_state.subject_task().is_some_and(|subject| {
                    state
                        .tracking
                        .open_entries()
                        .any(|entry| entry.task == subject)
                });
            // The rule rows' per-second countdown (r4 D30, F23): the
            // subject's At-kinded rules carry the only seconds-changing
            // text in the section. The doc-comment's named extension
            // point — never a second tick channel.
            let at_rule_rows = self.view_state.view() == crate::view_state::ViewId::TaskDetail
                && self.view_state.subject_task().is_some_and(|subject| {
                    state.reminders.rules.values().any(|rule| {
                        rule.task == subject
                            && matches!(rule.kind, taskboard_domain::ReminderKind::At(_))
                    })
                });
            // The timers rows' per-second countdown (r4 D32, §2
            // decision 5's amendment): the unfired set is the only
            // timers text that changes with the wall second — an
            // awaiting-only map renders clamped zeros plus a fixed
            // instant, static, no tick owed. The doc-comment's named
            // extension point, discharged — never a second tick
            // channel.
            let timer_rows = !crate::presentation::timer_rows(state, now).is_empty();
            active = task_view_active || focus_overlay_active || at_rule_rows || timer_rows;
        }
        self.seconds_surfaces_active = active;
    }

    /// The D1 refresh, guarded (#463): the derive+diff half
    /// (reconcile, `models::push`, the live-menu re-grey) runs only
    /// when the loaded snapshot is not the one the models already
    /// carry. The cheap half — the §8 buffer wipe and the settings
    /// push — runs on every call: flows can end without outcomes, and
    /// the settings rows fold the liveness map and the settled
    /// replies, none of which live in the state Arc.
    ///
    /// Invariant for future callers: anything the derive reads that is
    /// NOT the state Arc (`view_state`, shell, keymap, due/picker state,
    /// the wall clock) must reach this method via [`Self::refresh_forced`]
    /// or behind a state change — the audit table in
    /// `.artifacts/plans/2026-10-31-unchanged-refresh-guard-plan.md` §5
    /// is the reference classification.
    pub(crate) fn refresh(&mut self, window: &AppWindow) {
        let state = self.state.load_full();
        let unchanged = self
            .last_derived
            .as_ref()
            .is_some_and(|derived| Arc::ptr_eq(derived, &state));
        if unchanged {
            self.retire_levels(window);
            self.push_settings(window);
            return;
        }
        self.refresh_forced(window);
    }

    /// The date combobox's filtered quick-list rows (r4 D26): the
    /// armed gate (round-5 F10) — disarmed = the full 96-slot grid
    /// (empty digits match everything); the first keystroke re-arms
    /// the per-keystroke narrowing. No armed editor renders no slots.
    pub(in crate::adapter) fn quick_time_slots(
        editor: Option<&crate::level_state::DateEditorState>,
        time_text: &str,
    ) -> Vec<chrono::NaiveTime> {
        if editor.is_some_and(|e| e.picker.is_some()) {
            let digits = if editor.is_some_and(|e| e.time_filter_armed) {
                crate::due_edit::filter_digits(time_text)
            } else {
                String::new()
            };
            crate::due_edit::quarter_slots()
                .into_iter()
                .filter(|slot| crate::due_edit::quick_list_matches(&digits, *slot))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// The unguarded D1 pass: reconcile + full re-derive + the cheap
    /// tail. Every caller whose inputs moved outside the state Arc
    /// calls this directly (the plan §5 audit's forced set).
    #[allow(clippy::too_many_lines)] // the pass is one linear pipeline
    pub(crate) fn refresh_forced(&mut self, window: &AppWindow) {
        // The clip-focus law's absorb seam arm (#1419, operator
        // ruling 2026-11-25, issue comment 7172599685): the pass
        // below re-derives and pushes the slint models, and a model
        // push can unmount/remount the focused text field — the
        // unmount fires `text_flow_focus_lost` as a synthetic unmount
        // echo, not a user dismissal. The flag arms the seam's absorb
        // for the echo's delivery window (it extends past this body —
        // see the disarm note at the tail).
        self.refresh_push_in_progress.set(true);
        // The create-focus handoff's consumption (UX-R8 chunk-5 D-7,
        // US-5/AC-1): the stack's live ids diffed against the commit-
        // time snapshot — exactly one new id moves the focus (the
        // walk handoff's existing setter) and arms the scroll hint;
        // zero or >1 (a refused create, a racing capture) clear the
        // arm with NO handoff — the one-shot law (R-d).
        let create_focus_before_focused = self.consume_create_focus_arm();
        self.sync_view_context(window);
        let state = self.state.load_full();
        self.sync_settings(&state.settings);
        self.view_state.reconcile(&state);
        // The push-side kiosk belt (W-15, #1049): the reconcile's
        // dead-subject arm's twin — a habit-variant mount that slipped
        // past the dispatch gates steps back on the same pass, before
        // any model reads the posture.
        {
            let outcomes = self.view_state.kiosk_habit_belt(self.kiosk);
            if !outcomes.is_empty() {
                self.apply_outcomes(window, &outcomes);
            }
        }
        // The pinned object menu's disarm (J5 US-4 AC-2, #927): the
        // built-once model's subject died while the menu held — close
        // on the same reconciliation pass (bar + rows disarm together,
        // the CloseMenu path), never a frame late. A `None` subject
        // (stack/board menus, the main menu) never disarms; a
        // partially-deleted selection keeps its live members.
        if self.menu_kind == MenuKind::Object && self.menu_subject_dead(&state) {
            self.apply_close_menu(window);
        }
        // The walk's refresh-time normalization (J4 D-3): the cursor
        // clamps against the pushed row count, an emptied stage
        // advances through the symmetric skip law, a dead candidate
        // disarms.
        if let Flow::ReviewWalk {
            cadence, reference, ..
        } = self.view_state.walk_flow().unwrap_or(Flow::None)
        {
            let (rows, _) = self.walk_rows(&state, cadence, reference);
            let candidates = crate::presentation::walk_file_candidates(&state);
            self.view_state.clamp_review_walk(&state, rows, &candidates);
        }
        // After the reconcile — the reconciled view-state is what the
        // term reads. The pass's clock read feeds the timers term (r4
        // D32: one derivation, no second list to drift).
        self.compute_seconds_surfaces_active(&state, chrono::Utc::now());
        self.retire_levels(window);
        let chain = self.current_chain(window);
        let day = self.day_ctx();
        // The time combobox's pushed half (r4 D26): the filtered
        // quick-list rows over the static slots, and the nearest-slot
        // seek index against the picker's current time.
        // The editor's push half reads its level; without one the
        // mirrors render exactly the wiped state the old enforce pass
        // produced (empty buffer, the `--:--` value echo).
        // The quick options' open-time seed (task-detail-editor UX
        // plan US-2/D-7, N-6): the FIRST pass of an armed editor
        // without a picker-held time captures the device now's local
        // time-of-day — captured once, so the per-pass derivation
        // never snaps the highlight back to "now" over the user's
        // ladder walk (a picker-held time wins for the whole session).
        if let Some(editor) = self.view_state.date_editor_mut()
            && editor.time_open_seed.is_none()
            && editor
                .picker
                .as_ref()
                .is_none_or(|picker| picker.time().is_none())
        {
            // The clock seam's jurisdiction includes the seed capture
            // (#921): under a mount's frozen clock the seed derives
            // from the frozen instant, so an exact-artifact test reads
            // one oracle instead of racing the capture's wall clock.
            // Production is unchanged — every production hand-over
            // passes `None` (`UiSettings::clock`'s contract).
            editor.time_open_seed = Some(match self.frozen_now {
                Some(now) => now.with_timezone(&chrono::Local).time(),
                None => chrono::Local::now().time(),
            });
        }
        let editor = self.view_state.date_editor();
        let time_text = editor.map_or("--:--", |e| e.time_text.as_str());
        let time_slots = Self::quick_time_slots(editor, time_text);
        // The quick options' focused slot (task-detail-editor UX plan
        // US-2/D-7, N-6): the flyout's open-time seed (captured once —
        // see the lazy capture above) — a picker-held time wins
        // (re-opening after a typed entry stays on the entry), else
        // the device now's local time-of-day at capture. The list
        // opens scrolled to the current time, never the list head, and
        // the ladder walk owns the highlight afterwards (no per-pass
        // recompute snap-back).
        let time_nearest_row =
            crate::due_edit::nearest_slot(&time_slots, editor.and_then(|e| e.time_open_seed));
        let now2 = chrono::Utc::now();
        let due = crate::models::DueEditorPush {
            text: editor.map_or("", |e| e.buffer.as_str()),
            echo: editor.map_or("", |e| e.echo.as_str()),
            picker: editor.and_then(|e| e.picker.as_ref()),
            focus_on_mount: self.focus_handover == FocusHandover::Grid,
            field_focus: self.focus_handover == FocusHandover::Field,
            horizon: self.due_soon_horizon,
            time_text,
            time_valid: crate::due_edit::time_field_valid(time_text),
            time_slots: &time_slots,
            time_nearest_row,
            date_valid: crate::due_edit::field_validity(
                editor.map_or("", |e| e.buffer.as_str()),
                now2,
                &chrono::Local,
            ),
            syntax_hint: crate::due_edit::DUE_SYNTAX_HINT,
            time_hint: crate::due_edit::TIME_SYNTAX_HINT,
            offset_valid: crate::presentation::rule_when_validity(
                &self.rule_offset_draft,
                now2,
                &chrono::Local,
            ),
            offset_hint: crate::presentation::RULE_WHEN_SYNTAX_HINT,
        };
        // The focus hand-over is one-shot: consumed by this push.
        self.focus_handover = FocusHandover::None;
        #[cfg(test)]
        {
            self.derive_passes += 1;
        }
        // The recur row's flags ride the pass (issue #506): they
        // clear when the subject stops recurring — the buffers never
        // outlive their surface.
        let subject_recurs = self
            .view_state
            .subject_task()
            .and_then(|id| state.tasks.get(&id))
            .is_some_and(|t| t.recur.is_some());
        if !subject_recurs {
            self.recur_pause_open = false;
            self.recur_confirm_armed = false;
        }
        // The armed habit subject absorbs when its habit dies (the
        // stale-index law at the id-table level — the armed flag is
        // id-keyed since H2, so a vanished subject simply disarms via
        // the open menu's re-projection).
        // The armed habit subject absorbs when its habit dies: a dead
        // subject disarms on the pass (the buffers-never-outlive rule).
        if !self.habit_convert_armed.is_some_and(|armed| {
            state
                .habits
                .habits
                .get(&armed)
                .is_some_and(taskboard_domain::habit::Habit::is_live)
        }) {
            self.habit_convert_armed = None;
        }
        // The armed remove absorbs when its habit dies (the same
        // stale-index law; the armed flag is id-keyed).
        if !self.habit_remove_armed.is_some_and(|armed| {
            state
                .habits
                .habits
                .get(&armed)
                .is_some_and(taskboard_domain::habit::Habit::is_live)
        }) {
            self.habit_remove_armed = None;
        }
        // The armed restore absorbs when its habit leaves the roster's
        // restorable set (the stale law inverted: a restored or
        // re-created habit has nothing left to resurrect).
        if !state
            .roster
            .tombstoned_habits
            .iter()
            .any(|row| Some(row.id) == self.habit_restore_armed)
        {
            self.habit_restore_armed = None;
        }
        // The armed bulk restore absorbs when the roster empties (the
        // same stale law — nothing left to resurrect in bulk).
        if self.habit_restore_all_armed && state.roster.tombstoned_habits.is_empty() {
            self.habit_restore_all_armed = false;
        }
        // The pause editor's one-shot subject absorbs when its habit
        // dies (the same stale-index law; HF1) — the open editor
        // unmounts with it.
        if !self.habit_pause_target.is_some_and(|armed| {
            state
                .habits
                .habits
                .get(&armed)
                .is_some_and(taskboard_domain::habit::Habit::is_live)
        }) {
            self.habit_pause_open = false;
            self.habit_pause_target = None;
        }
        // The editor's draft dies with its subject (the editor's
        // escape/cancel discipline — the buffer-never-outlives rule).
        if self.view_state.subject_habit().is_none() {
            self.habit_edit_draft = None;
        }
        let recur = crate::models::RecurRowPush {
            pause_open: self.recur_pause_open,
            confirm_open: self.recur_confirm_armed,
        };
        // The priority pickers' flag (UX R-3 D-1): it never outlives
        // its subject (the recur flags' rule) — a dead/switched
        // subject clears it on the pass.
        if self.view_state.subject_task().is_none() {
            self.priority_open = None;
        }
        // The labels form dies with its overlay (the same rule): any
        // pass that reads a non-Labels flow clears the draft.
        if !matches!(
            self.view_state.flow(),
            crate::view_state::Flow::Labels { .. }
        ) {
            self.label_form = None;
        }
        let priority = crate::models::PriorityPickerPush {
            open: self.priority_open,
        };
        let label_form = self
            .label_form
            .as_ref()
            .map(|form| crate::models::LabelFormPush {
                open: true,
                title: form.title.clone(),
                color: form.color.clone(),
            });
        // M9: the always-on effort row's seed — the armed buffer when
        // the user typed (the lazy editor exists), the subject's
        // stored text form otherwise (empty when unset). The field is
        // always mounted; there is no open flag any more.
        let effort_stored = self
            .view_state
            .subject_task()
            .and_then(|id| state.tasks.get(&id))
            .and_then(|t| t.effort.as_ref())
            .map(|e| e.text_form())
            .unwrap_or_default();
        let effort_editor = Some(crate::models::EffortEditorPush {
            buffer: self
                .effort_editor
                .as_ref()
                .map_or(effort_stored.as_str(), |editor| editor.buffer.as_str()),
        });
        // The armed strip's mirror (the track-entries armed-index
        // rule): the armed row's canonical roster position, or none.
        // US-6 pass-side clear (#840, task-detail-editor UX plan
        // D-11): an armed identity that no longer resolves against the
        // live roster cannot outlive its row — the lingering `Some`
        // would leave the strip unrenderable (mirror index −1) and eat
        // the next Escape rung. The `priority_open` subject-death
        // rule's shape.
        if let Some(armed) = self.label_delete_armed
            && state.labels.get(&armed).is_none_or(|label| label.deleted)
        {
            self.label_delete_armed = None;
        }
        // The armed label restore absorbs when its label leaves the
        // roster's tombstoned set (the stale law — a restored or
        // re-created label has nothing left to resurrect).
        if let Some(armed) = self.label_restore_armed
            && state
                .roster
                .tombstoned_labels
                .iter()
                .all(|row| row.id != armed)
        {
            self.label_restore_armed = None;
        }
        // The armed bulk label restore absorbs when the tombstone set
        // empties (the stale law inverted — nothing left in bulk).
        if self.label_restore_all_armed && state.roster.tombstoned_labels.is_empty() {
            self.label_restore_all_armed = false;
        }
        // The armed entry restore absorbs when the journal id left the
        // roster (restored, re-created, or evicted — the bounded
        // honesty of the retention law).
        if let Some(armed) = self.entry_restore_armed
            && state
                .roster
                .deleted_entries
                .iter()
                .all(|row| row.journal_id != armed)
        {
            self.entry_restore_armed = None;
        }
        // The armed rule clear absorbs when its subject has no rules
        // left (D-2: never ceremony a no-op).
        if self.rules_clear_armed
            && self
                .view_state
                .subject_task()
                .is_none_or(|task| state.reminders.rules.values().all(|rule| rule.task != task))
        {
            self.rules_clear_armed = false;
        }
        // The overlay roster's board (tag-color plan D-8): the board
        // mount names its subject directly; the detail mount derives
        // the subject task's board.
        let subject_board = match self.view_state.labels_subject() {
            Some(crate::view_state::LabelOverlaySubject::Board(board)) => Some(board),
            _ => self
                .view_state
                .subject_task()
                .and_then(|t| state.tasks.get(&t))
                .and_then(|t| state.stacks.get(&t.stack))
                .map(|stack| stack.board),
        };
        let label_armed_index = self
            .label_delete_armed
            .and_then(|id| {
                state
                    .labels
                    .values()
                    .filter(|l| !l.deleted && Some(l.board) == subject_board)
                    .position(|l| l.id == id)
            })
            .and_then(|p| i32::try_from(p).ok())
            .unwrap_or(-1);
        // The break-rule roster's armed index (T-14, the label form's
        // armed-index convention): the armed id's position in the
        // canonical `(threshold, id)` order this pass derives — a
        // vanished subject's index is -1 (no strip renders).
        let break_rule_armed_index = self
            .break_rule_armed
            .and_then(|armed| {
                // Aggregated over all boards since round-6 A1 D-3
                // (2026-11-24) — the same order the roster renders.
                crate::presentation::break_rule_rows(&state, None)
                    .iter()
                    .position(|row| row.rule == armed)
            })
            .and_then(|p| i32::try_from(p).ok())
            .unwrap_or(-1);
        self.models.walk_abort_armed = self.walk_abort_armed;
        // The restore rosters' pushed halves (W-2/W-9, D-4): the
        // deleted sections' rows and armed mirrors. Kiosk pushes empty
        // sections — the affordances render absent, the CLI carries
        // the kiosk restore path.
        let (label_deleted_rows, label_restore_armed_index): (Vec<slint::SharedString>, i32) =
            if self.kiosk {
                (Vec::new(), -1)
            } else {
                let overlay_board = match self.view_state.labels_subject() {
                    Some(crate::view_state::LabelOverlaySubject::Board(board)) => Some(board),
                    _ => self
                        .view_state
                        .subject_task()
                        .and_then(|t| state.tasks.get(&t))
                        .and_then(|t| state.stacks.get(&t.stack))
                        .map(|stack| stack.board),
                };
                let rows: Vec<&taskboard_domain::RestoreRow<taskboard_domain::LabelId>> = state
                    .roster
                    .tombstoned_labels
                    .iter()
                    .filter(|row| {
                        state.labels.get(&row.id).is_some_and(|label| {
                            label.deleted && Some(label.board) == overlay_board
                        })
                    })
                    .collect();
                let armed = self
                    .label_restore_armed
                    .and_then(|id| rows.iter().position(|row| row.id == id))
                    .and_then(|p| i32::try_from(p).ok())
                    .unwrap_or(-1);
                (
                    rows.into_iter()
                        .map(|row| slint::SharedString::from(row.name.as_str()))
                        .collect(),
                    armed,
                )
            };
        window.set_label_overlay_has_deleted(!label_deleted_rows.is_empty());
        window.set_label_overlay_deleted_rows(ModelRc::new(slint::VecModel::from(
            label_deleted_rows,
        )));
        window.set_label_overlay_restore_armed_index(label_restore_armed_index);
        let (entry_deleted_rows, entry_restore_armed_index): (Vec<slint::SharedString>, i32) =
            if self.kiosk {
                (Vec::new(), -1)
            } else {
                let armed = self
                    .entry_restore_armed
                    .and_then(|id| {
                        state
                            .roster
                            .deleted_entries
                            .iter()
                            .position(|row| row.journal_id == id)
                    })
                    .and_then(|p| i32::try_from(p).ok())
                    .unwrap_or(-1);
                (
                    state
                        .roster
                        .deleted_entries
                        .iter()
                        .map(|row| slint::SharedString::from(row.label.as_str()))
                        .collect(),
                    armed,
                )
            };
        window.set_track_has_deleted(!entry_deleted_rows.is_empty());
        window.set_track_deleted_rows(ModelRc::new(slint::VecModel::from(entry_deleted_rows)));
        window.set_track_restore_armed_index(entry_restore_armed_index);
        // The deleted-habits section's pushed halves (#1290/#1245, D-4):
        // the roster's rows and armed mirrors; kiosk pushes empty — the
        // affordances render absent, the CLI carries the restore path.
        let (habit_deleted_rows, habit_restore_armed_index): (Vec<slint::SharedString>, i32) =
            if self.kiosk {
                (Vec::new(), -1)
            } else {
                let armed = self
                    .habit_restore_armed
                    .and_then(|id| {
                        state
                            .roster
                            .tombstoned_habits
                            .iter()
                            .position(|row| row.id == id)
                    })
                    .and_then(|p| i32::try_from(p).ok())
                    .unwrap_or(-1);
                (
                    state
                        .roster
                        .tombstoned_habits
                        .iter()
                        .map(|row| slint::SharedString::from(row.name.as_str()))
                        .collect(),
                    armed,
                )
            };
        window.set_habits_has_deleted(!habit_deleted_rows.is_empty());
        window.set_habits_deleted_rows(ModelRc::new(slint::VecModel::from(habit_deleted_rows)));
        window.set_habits_restore_armed_index(habit_restore_armed_index);
        window.set_habits_restore_all_armed(self.habit_restore_all_armed && !self.kiosk);
        // The labels overlay's bulk armed mirror (#1245).
        window.set_label_overlay_restore_all_armed(self.label_restore_all_armed && !self.kiosk);
        // The clear-all row's pushed halves (W-4): the subject's rule
        // count (the row's consequence names it), the clearable
        // verdict (D-2: zero rules is the disabled row, never a
        // ceremony), and the armed mirror.
        let clear_count = self.view_state.subject_task().map_or(0, |task| {
            state
                .reminders
                .rules
                .values()
                .filter(|rule| rule.task == task)
                .count()
        });
        if self.rules_clear_armed && clear_count == 0 {
            self.rules_clear_armed = false;
        }
        window.set_rules_clearable(clear_count > 0);
        window.set_rules_clear_count(i32::try_from(clear_count).unwrap_or(i32::MAX));
        window.set_rule_clear_armed(self.rules_clear_armed);
        self.tables = crate::models::push(
            window,
            &state,
            &self.view_state,
            &self.keymap,
            &chain,
            day,
            self.calendar(),
            self.kiosk,
            &self.reminder_defaults,
            self.focus.as_ref(),
            &mut self.models,
            &due,
            recur,
            priority,
            label_form.as_ref(),
            effort_editor.as_ref(),
            label_armed_index,
            break_rule_armed_index,
            (self.reflection_active, self.reflection_armed),
            self.habit_edit_draft.as_ref(),
        );
        // The planner cursor's clamp/prune (US-15, #1041 — the K4
        // law's refresh clamp, the rule cursor's reconcile arm): a
        // dead id, a foreign view, or a pane that no longer holds the
        // cursor clears it — never a stale row answering a stroke.
        // The walk's embedded planner child is a live planner mount
        // too (US-22, #1234): the cursor survives there — the surface
        // check reads the chain, not just the root view.
        let planner_live = self.view_state.view() == crate::view_state::ViewId::PlanNextDay
            || self
                .view_state
                .chain_has_flow(|flow| matches!(flow, Flow::ReviewPlanner));
        if !planner_live
            || !self
                .view_state
                .plan_cursor()
                .is_some_and(|cursor| self.plan_pane_rows().contains(&cursor))
        {
            self.view_state.clear_plan_cursor();
        }
        // The create-focus handoff's hint half (UX-R8 chunk-5 D-7):
        // the tables are fresh — edge-trigger the FocusScrollHint
        // global toward the newly focused card (the machinery chunk 4
        // extended; the board/compact ListViews consume it).
        if let Some(handoff) = create_focus_before_focused {
            self.push_focus_scroll_hint(window, handoff.before);
        }
        // Round-3 D27c: engine syncs re-grey a live menu — the pages
        // rebuild from the fresh state while the menu holds (the
        // cursor's length bound rides along). UX-R8 D-1 (N-11): the
        // re-derivation is scoped to the main menu — an object menu's
        // model is built once at open and never swapped (the repro
        // journey pins the swap this guard forbids).
        if self.shell.menu.open && self.menu_kind == MenuKind::Main {
            let flow = self.view_state.flow().clone();
            let states = self.entry_states(&state, &flow);
            self.root_entries = self.mint_root_entries(&states);
            self.menu_entries = match self.active_submenu() {
                Some(crate::menu::Submenu::Group(group)) => {
                    menu::group_page(&self.keymap, &self.menu_chain, &states, group)
                }
                Some(crate::menu::Submenu::Task(family)) => {
                    // The habit menu's submenu page (H2 D4): the
                    // Convert family reads the habit verdicts (the
                    // armed confirm's flip + the cancel row).
                    if matches!(
                        self.menu_row_subject,
                        Some(crate::presentation::RowSubject::Habit(_))
                    ) {
                        self.habit_submenu_page(family)
                    } else {
                        let subject = self.current_task_subject();
                        menu::task_submenu_page(
                            &self.keymap,
                            &self.menu_chain,
                            subject,
                            &states,
                            family,
                        )
                    }
                }
                None => self.root_entries.clone(),
            };
            self.shell.menu.len = self.menu_entries.len();
            self.sync_menu_models(window);
        }
        self.push_settings(window);
        // The setter popover's pushed halves (r4 D33): cheap, every
        // pass — the popover reads them only while open.
        self.push_timer_setter(window);
        // The entries form's pushed halves (r4 D-TS4): the same
        // every-pass shape; the wipe rides the push.
        self.push_track_form(window);
        // The create form's pushed halves (round-7 D-7.2): the armed
        // flag and the quota validity are adapter-owned; cheap every
        // pass (the setter popover's shape).
        window.set_habits_create_open(self.habit_create_open);
        // The pause editor's pushed halves (HF1, W-3): the armed flag
        // and the desktop mount gate are adapter-owned; cheap every
        // pass (the create form's shape).
        window.set_habits_pause_open(self.habit_pause_open);
        window.set_habits_pause_mountable(!self.kiosk);
        let (quota_validity, quota_hint) =
            crate::presentation::habit_quota_validity(&self.habit_create_quota);
        window.set_habits_quota_validity(match quota_validity {
            crate::presentation::QuotaValidity::Neutral => crate::FieldValidity::Neutral,
            crate::presentation::QuotaValidity::Valid => crate::FieldValidity::Valid,
            crate::presentation::QuotaValidity::Invalid => crate::FieldValidity::Invalid,
        });
        window.set_habits_quota_error_hint(SharedString::from(quota_hint));
        // The pass materialized: close the cache only now — a
        // re-entrant pass before this line would see the stale cache
        // and derive again, never skip a needed pass.
        self.last_derived = Some(state);
        // The focus-origin tail funnel (US-1/W-1): a reconcile that
        // dropped an overlay's flow (the vanished-subject rule) closed
        // the site *after* this pass's entry funnel ran — the restore
        // fires here, over the freshly pushed models, so the origin's
        // row is on the page the fact names.
        self.sync_focus_origin(window);
        // NOTE: the echo flag is intentionally NOT disarmed here. The
        // deferred focus-lost echo of this push lands after the
        // dispatch's synchronous processing ends, so a tail disarm
        // always precedes it. The flag disarms on the FIRST focus-lost
        // (the seam's absorb-and-disarm) or on any user-intent seam
        // (the macro arms) — a genuine interaction un-arms before its
        // own focus move can be mis-absorbed.
    }

    /// Recomposes the setter's grammar text + fire instant from the
    /// custom row's state (r4 D-T4): the composer emits existing-
    /// grammar text (`at <RFC-3339 local-offset>`) re-parsed by the
    /// one parser; a DST-gap pick composes `None` — the draft becomes
    /// the unparseable `"at "` (the invalid flavour, the honest
    /// refusal), and the funnel never emits a miss.
    pub(in crate::adapter) fn recompose_timer_setter(&mut self) {
        let Some(time) = self.setter_time else {
            self.setter_composed = None;
            self.setter_fire = None;
            self.timer_when_draft.clear();
            return;
        };
        let now = chrono::Utc::now();
        let minutes = u32::try_from(crate::due_edit::minutes_of_day(time)).unwrap_or(0);
        self.setter_composed = crate::presentation::compose_timer_at(
            minutes,
            self.setter_day_offset,
            now,
            &chrono::Local,
        );
        self.setter_fire = crate::presentation::compose_timer_instant(
            minutes,
            self.setter_day_offset,
            now,
            &chrono::Local,
        );
        self.timer_when_draft = self
            .setter_composed
            .clone()
            .unwrap_or_else(|| "at ".to_owned());
    }

    /// Clears the setter's draft state (dismiss and commit share it —
    /// drafts are widget-local, nothing survives the panel).
    pub(in crate::adapter) fn clear_timer_setter(&mut self) {
        self.setter_filter_armed = false;
        self.setter_time = None;
        "--:--".clone_into(&mut self.setter_time_text);
        self.setter_day_offset = 0;
        self.setter_insist = false;
        self.setter_composed = None;
        self.setter_fire = None;
        self.timer_when_draft.clear();
    }

    /// The setter popover's pushed halves (r4 D33): the `TimeField`'s
    /// contract — the adapter's canonical mirror re-seeds the widget
    /// buffer only when the picker state moved, never while the user
    /// types (the typed text echoes verbatim through the draft).
    pub(in crate::adapter) fn push_timer_setter(&self, window: &AppWindow) {
        let digits = if self.setter_filter_armed {
            crate::due_edit::filter_digits(&self.setter_time_text)
        } else {
            String::new()
        };
        let slots: Vec<chrono::NaiveTime> = crate::due_edit::quarter_slots()
            .into_iter()
            .filter(|slot| crate::due_edit::quick_list_matches(&digits, *slot))
            .collect();
        let nearest = crate::due_edit::nearest_slot(&slots, self.setter_time);
        window.set_timer_setter_time_text(SharedString::from(self.setter_time_text.clone()));
        window.set_timer_setter_time_validity(
            match crate::due_edit::time_field_valid(&self.setter_time_text) {
                None => crate::FieldValidity::Neutral,
                Some(true) => crate::FieldValidity::Valid,
                Some(false) => crate::FieldValidity::Invalid,
            },
        );
        window.set_timer_setter_time_hint(SharedString::from(crate::due_edit::TIME_SYNTAX_HINT));
        // The fresh-VecModel-per-pass shape (the date editor's slot
        // push, models.rs:1614): the flyout reads it only while open.
        window.set_timer_setter_time_slots(ModelRc::new(slint::VecModel::from(
            slots
                .iter()
                .map(|slot| crate::TimeSlotRow {
                    label: SharedString::from(crate::due_edit::format_time_hm(*slot)),
                    minutes: crate::due_edit::minutes_of_day(*slot),
                })
                .collect::<Vec<crate::TimeSlotRow>>(),
        )));
        window.set_timer_setter_time_nearest_row(i32::try_from(nearest).unwrap_or(0));
        // The echoed selection halves (#1306): the day + insistence
        // rows render these mirrors — reply-sourced echo (r2 D27),
        // one push path for both adapter-owned rows. The wipe rides
        // here too: `clear_timer_setter` reseeds both, so a fresh
        // open pushes 0/0.
        window.set_timer_setter_day_index(i32::try_from(self.setter_day_offset).unwrap_or(0));
        window.set_timer_setter_insist_index(i32::from(self.setter_insist));
        window.set_timer_setter_fire_time(SharedString::from(self.setter_fire.map_or(
            String::new(),
            |fire| {
                format!(
                    "@ {}",
                    crate::presentation::iso_local_fire_time(fire, &chrono::Local)
                )
            },
        )));
        window.set_timer_setter_commit_enabled(
            crate::presentation::timer_when_validity(
                &self.timer_when_draft,
                chrono::Utc::now(),
                &chrono::Local,
            ) == Some(true),
        );
    }

    /// The entries form's pushed halves (r4 D-TS4): the tri-state
    /// mirrors over the live parse (the due editor's shape). Runs
    /// every pass — cheap; the panel reads them only while mounted.
    /// The wipe rides here too: the buffers never outlive the flow
    /// (the §8 invariant's track-form twin).
    #[allow(clippy::too_many_lines)] // one pushed half per property, kept flat
    pub(in crate::adapter) fn push_track_form(&mut self, window: &AppWindow) {
        // The buffers live in the form's level (W5b D-H2): the drop
        // is the wipe, the filter latches ride inside the state — no
        // per-pass wipe pass here.
        // The armed delete dies with the overlay (D-TS5's
        // disarm-on-close; state-truth, nothing ticks) — but it
        // survives the form surface's ladder step back to the list.
        if !matches!(
            self.view_state.flow(),
            Flow::TrackEntries { .. } | Flow::TrackEntryForm { .. }
        ) {
            self.track_entry_target = None;
        }
        // The armed row's index resolves through the id table — a
        // refresh that drops the armed row renders unarmed (the
        // stale-index no-op).
        let armed_index = self
            .track_entry_target
            .and_then(|armed| {
                self.tables
                    .track_entries
                    .iter()
                    .position(|entry| *entry == armed)
            })
            .map_or(-1, |position| i32::try_from(position).unwrap_or(-1));
        window.set_track_armed_index(armed_index);
        if armed_index < 0 {
            self.track_entry_target = None;
        }
        let Some(form) = self.view_state.track_form() else {
            window.set_track_form_open(false);
            return;
        };
        let today = chrono::Local::now().date_naive();
        let parse = crate::presentation::parse_track_form(
            &form.date_text,
            &form.start_text,
            &form.end_text,
            form.seed.is_some(),
            today,
            &chrono::Local,
        );
        let validity = |valid: Option<bool>| match valid {
            None => crate::FieldValidity::Neutral,
            Some(true) => crate::FieldValidity::Valid,
            Some(false) => crate::FieldValidity::Invalid,
        };
        let to_slots = |slots: &[chrono::NaiveTime]| {
            ModelRc::new(slint::VecModel::from(
                slots
                    .iter()
                    .map(|slot| crate::TimeSlotRow {
                        label: SharedString::from(crate::due_edit::format_time_hm(*slot)),
                        minutes: crate::due_edit::minutes_of_day(*slot),
                    })
                    .collect::<Vec<crate::TimeSlotRow>>(),
            ))
        };
        let slots_for = |text: &str, armed: bool| -> Vec<chrono::NaiveTime> {
            let digits = if armed {
                crate::due_edit::filter_digits(text)
            } else {
                String::new()
            };
            crate::due_edit::quarter_slots()
                .into_iter()
                .filter(|slot| crate::due_edit::quick_list_matches(&digits, *slot))
                .collect()
        };
        let start_slots = slots_for(&form.start_text, form.start_filter_armed);
        let end_slots = slots_for(&form.end_text, form.end_filter_armed);
        let picker = self.view_state.picker().map(|level| &level.picker);
        let grid = picker.map_or_else(
            || ModelRc::new(slint::VecModel::from(Vec::new())),
            |picker| {
                use chrono::Datelike as _;
                let now = chrono::Local::now().date_naive();
                ModelRc::new(slint::VecModel::from(
                    picker
                        .grid()
                        .chunks(7)
                        .map(|chunk| {
                            ModelRc::new(slint::VecModel::from(
                                chunk
                                    .iter()
                                    .map(|cell| {
                                        let date = cell.date;
                                        crate::DueDayModel {
                                            label: SharedString::from(
                                                date.map_or(String::new(), |d| d.day().to_string()),
                                            ),
                                            blank: date.is_none(),
                                            cursor: date.is_some_and(|d| d == picker.cursor()),
                                            today: date.is_some_and(|d| d == now),
                                            kind: crate::models::to_slint_due_kind(date.map_or(
                                                crate::presentation::DueKind::None,
                                                |d| {
                                                    crate::due_edit::cell_kind(
                                                        d,
                                                        now,
                                                        self.due_soon_horizon,
                                                    )
                                                },
                                            )),
                                        }
                                    })
                                    .collect::<Vec<crate::DueDayModel>>(),
                            ))
                        })
                        .collect::<Vec<ModelRc<crate::DueDayModel>>>(),
                ))
            },
        );
        window.set_track_form_open(true);
        window.set_track_form_date_text(SharedString::from(form.date_text.clone()));
        window.set_track_form_date_validity(validity(parse.date_valid));
        window.set_track_form_date_hint(SharedString::from(crate::presentation::TRACK_DATE_HINT));
        // The grid's mirror follows the picker child's presence (N2):
        // the child's death closes the grid on the next pass.
        window.set_track_form_grid_open(picker.is_some());
        window.set_track_form_grid(grid);
        window.set_track_form_start_text(SharedString::from(form.start_text.clone()));
        window.set_track_form_start_validity(validity(parse.start_valid));
        window.set_track_form_start_hint(SharedString::from(crate::due_edit::TIME_SYNTAX_HINT));
        window.set_track_form_start_slots(to_slots(&start_slots));
        window.set_track_form_start_nearest_row(
            i32::try_from(crate::due_edit::nearest_slot(&start_slots, None)).unwrap_or(0),
        );
        window.set_track_form_end_text(SharedString::from(form.end_text.clone()));
        window.set_track_form_end_validity(validity(parse.end_valid));
        window.set_track_form_end_hint(SharedString::from(crate::due_edit::TIME_SYNTAX_HINT));
        window.set_track_form_end_slots(to_slots(&end_slots));
        window.set_track_form_end_nearest_row(
            i32::try_from(crate::due_edit::nearest_slot(&end_slots, None)).unwrap_or(0),
        );
    }

    #[cfg(test)]
    pub(crate) fn track_form_mut_for_test(&mut self) -> Option<&mut TrackFormState> {
        self.view_state.track_form_mut()
    }

    #[cfg(test)]
    pub(crate) fn test_armed_delete_is(&self, entry: taskboard_domain::TimeEntryId) -> bool {
        self.track_entry_target == Some(entry)
    }

    #[cfg(test)]
    pub(crate) fn test_armed_delete_is_none(&self) -> bool {
        self.track_entry_target.is_none()
    }

    #[cfg(test)]
    pub(crate) fn test_tracked_today_rows(&self) -> Vec<taskboard_domain::TaskId> {
        self.tables.tracked_today.clone()
    }

    /// Seeds the form's adapter state for the armed flow (the begin
    /// seam's shape: inside the armed window, before the outcomes'
    /// refresh). Add seeds today's date and two unset times; edit
    /// seeds the entry's local date and `HH:MM` times — an open
    /// entry's end seeds empty (the keep-open semantics, D-TS4).
    #[cfg(test)]
    pub(crate) fn seed_track_form_for_test(&mut self, seed: Option<taskboard_domain::TimeEntryId>) {
        self.seed_track_form(seed);
    }

    /// The track-form's clock jurisdiction (#1214, the #921/#660
    /// precedent): the parse anchor `today` derives through the
    /// frozen-clock seam when one is mounted (`UiSettings::clock`'s
    /// contract — every production hand-over passes `None` and reads
    /// the wall clock exactly as before), so a test pinning
    /// clock-derived form content reads one oracle and a local
    /// midnight straddling a run cannot invert the seeded span.
    pub(in crate::adapter) fn track_form_today(&self) -> chrono::NaiveDate {
        self.frozen_now
            .map_or_else(chrono::Local::now, |now| now.with_timezone(&chrono::Local))
            .date_naive()
    }

    pub(in crate::adapter) fn seed_track_form(
        &mut self,
        seed: Option<taskboard_domain::TimeEntryId>,
    ) {
        use chrono::TimeZone as _;
        let Flow::TrackEntryForm { .. } = self.view_state.flow() else {
            return;
        };
        let state = self.state.load_full();
        let today = self.track_form_today();
        let entry = seed.and_then(|id| state.tracking.entries.get(&id));
        let start_local = entry.map(|e| chrono::Local.from_utc_datetime(&e.start.naive_utc()));
        let Some(form) = self.view_state.track_form_mut() else {
            return;
        };
        *form = TrackFormState {
            seed,
            date_text: start_local.map_or(today.format("%Y-%m-%d").to_string(), |l| {
                l.format("%Y-%m-%d").to_string()
            }),
            start_text: start_local.map_or("--:--".to_owned(), |l| l.format("%H:%M").to_string()),
            end_text: entry.and_then(|e| e.end).map_or_else(
                || "--:--".to_owned(),
                |end| {
                    chrono::Local
                        .from_utc_datetime(&end.naive_utc())
                        .format("%H:%M")
                        .to_string()
                },
            ),
            start_filter_armed: false,
            end_filter_armed: false,
            anchor: crate::level_state::FocusAnchor::View,
        };
    }

    /// The commit funnel (D-TS4, F24): the live validity table is
    /// authoritative — the draft fills the one-shot ctx fields only
    /// when it passes (the form's subject arms `track_target`: the
    /// add resolves over the viewed task, never the focus fallback),
    /// the router dispatches the existing verb, and the surface steps
    /// back to the list. A refusal keeps the form open with the
    /// invalid flavour (never swallowed silence).
    #[cfg(test)]
    pub(crate) fn commit_track_form_for_test(&mut self, window: &AppWindow) -> bool {
        self.commit_track_form(window)
    }

    pub(in crate::adapter) fn commit_track_form(&mut self, window: &AppWindow) -> bool {
        let Some(form) = self.view_state.track_form() else {
            return false;
        };
        // The form's subject rides the flow, not the focus: the
        // one-shot target seam (the tracking chip's rule — the subject
        // is not guaranteed `focused_task`, whose fallback would
        // swallow or retarget the add).
        let Flow::TrackEntryForm { task, .. } = self.view_state.flow() else {
            return false;
        };
        let subject = *task;
        let today = self.track_form_today();
        let parse = crate::presentation::parse_track_form(
            &form.date_text,
            &form.start_text,
            &form.end_text,
            form.seed.is_some(),
            today,
            &chrono::Local,
        );
        let Some(draft) = parse.draft else {
            self.refresh_forced(window);
            return false;
        };
        self.track_target = Some(subject);
        self.track_entry_draft = Some(draft);
        self.track_entry_target = form.seed;
        let action = if form.seed.is_some() {
            ActionId::TaskTrackEntryAdjust
        } else {
            ActionId::TaskTrackEntryLog
        };
        self.route(window, action);
        // The commit completes the form child (N1): the parent list
        // ignores the payload-less completion and stays — the overlay
        // stays open (the multi-add posture).
        let snapshot = self.state.load();
        let outcomes = self.view_state.complete_track_entry_form(&snapshot);
        self.apply_outcomes(window, &outcomes);
        self.finish_mutation(window);
        true
    }

    /// The flow-owned one-shots' reconciliation (cross-surface
    /// friction plan D-6, #763, AC-3): a one-shot left in the adapter
    /// after its owner flow ended is stale — the per-`Flow`-kind
    /// ownership table clears it at the retire drain. Armed-strip
    /// one-shots (`habit_convert_target`, `series_target`,
    /// `convert_target`, `pattern_draft`) are deliberately NOT
    /// flow-owned: their law is the arm seams' re-arm-before-every-
    /// route (defense-in-depth, unchanged) and their flags disarm via
    /// the existing Cancel/settle paths. The walk/picker children own
    /// none.
    pub(in crate::adapter) fn reconcile_one_shots(&mut self, retired: &[crate::surfaces::Level]) {
        for level in retired {
            match level.flow() {
                Some(crate::view_state::Flow::EditRecur { .. }) => {
                    self.recur_edit_target = None;
                    self.recur_draft_rule = None;
                }
                Some(crate::view_state::Flow::TrackEntryForm { .. }) => {
                    self.track_entry_draft = None;
                    self.track_entry_target = None;
                }
                _ => {}
            }
        }
    }

    /// The retire drain (W5b D3): the retired levels' exit
    /// side-effects ride here, one consumption per level — never a
    /// per-flow wipe rule. The flat chain's only parent anchor is
    /// `View`, whose restore the flowless hand-off in
    /// `apply_outcomes` already performs, so the drain adds no focus
    /// call; the date editor's grid disarm is the first side-effect
    /// consumer (the date family's commit).
    pub(in crate::adapter) fn retire_levels(&mut self, window: &AppWindow) {
        let retired = self.view_state.drain_retired();
        if retired.is_empty() {
            return;
        }
        // The stale one-shot reconciliation (cross-surface friction
        // plan D-6, #763): flow-owned one-shots die with their level —
        // consumption is not ownership.
        self.reconcile_one_shots(&retired);
        // The flat chain's levels never carry a non-view parent
        // anchor (D-H7); nested anchors consume at the drain from
        // chunk 3's ports on.
        debug_assert!(retired.iter().all(|level| matches!(
            level.state().anchor(),
            crate::level_state::FocusAnchor::None | crate::level_state::FocusAnchor::View
        )));
        // The picker dies with its level (§8): the grid cannot hold
        // focus any more, so the `DatePicker` context disarms here as
        // well as through the focus seam — a missed focus-loss on
        // unmount can never leave the context armed over the next
        // editor (R-C's exactness).
        if retired
            .iter()
            .any(|level| matches!(level.state(), crate::level_state::LevelState::DateEditor(_)))
        {
            self.shell.date_picker_grid = false;
            window.global::<KeyboardState>().set_date_picker_grid(false);
        }
        // The form's picker child dies with its level (N2): the grid
        // mirror cannot wait for a refresh that might not come.
        if retired
            .iter()
            .any(|level| matches!(level.state(), crate::level_state::LevelState::Picker(_)))
        {
            window.set_track_form_grid_open(false);
        }
    }

    /// §8 (W-SM-c), W5b D-H2: the date editor's buffers live in the
    /// `EditDate` level — non-empty ⇔ armed is a construction fact,
    /// the drop is the wipe, and the drain ([`Self::retire_levels`])
    /// owns the exit's grid disarm. There is no per-pass wipe pass
    /// any more.
    /// Whether the window is at the compact breakpoint (D17).
    pub(in crate::adapter) fn is_compact(window: &AppWindow) -> bool {
        window.global::<Responsive>().get_current_breakpoint() == Breakpoint::Compact
    }
}

// SPDX-License-Identifier: AGPL-3.0-only

use slint::{ModelRc, SharedString};

use taskboard_domain::TaskId;

#[cfg(test)]
use super::MenuAnchor;
#[cfg(test)]
use crate::router::UiEffect;
#[cfg(test)]
use crate::shell::KeymapShell;
#[cfg(test)]
use crate::view_state::ViewState;
#[cfg(test)]
use taskboard_domain::StackId;

use super::Adapter;
use super::Notice;
use crate::AppWindow;
use crate::router::{Mode, WalkIntent};
use crate::view_state::{Flow, FlowOutcome, WalkStage, WalkStep};

impl Adapter {
    /// The per-kind settle, adapter side (round-3 W-SM-b, plan §2):
    /// the pure settle fold applied, plus the adapter-owned
    /// attachments — the due picker dies with the due flow, the
    /// gesture buffers (`width_drag`/`scale_drag`, §8) die with the
    /// surfaces they address, and a settled drag tears the native
    /// runtime gesture down (`PointerExited` is the one public
    /// cancel, [`Self::teardown_runtime_drag`]). Every settle trigger
    /// funnels here — the L1 focus-loss and L2 pointer seams (LOCAL in
    /// the D-H4 classification), the leaving effects, the `OpenMenu`
    /// modal settle, and the resolver's CancelOnly/SettleThenRun/
    /// Replace cells (FOREIGN) — so the pure side applies the
    /// innermost-first fold ([`ViewState::settle_foreign`]); at the
    /// reachable depths (at most one flow level live) that fold
    /// degenerates to the local top-level settle, byte-identically.
    /// Idempotent on an empty chain.
    pub(in crate::adapter) fn settle_flow(
        &mut self,
        window: &AppWindow,
        state: &taskboard_domain::AppState,
    ) {
        // §8: the settle step owns the gesture buffers — they are
        // flow-independent by construction, and a leaving effect
        // unmounts the surfaces that would consume them (the release
        // can never arrive). Dropped before the None early-return: a
        // settling pass with no flow armed still retires them.
        self.width_drag = None;
        self.scale_drag = None;
        // UX-R8 D-4's settle arm: every foreign settle clears the
        // selection alongside the flow settle — even with no flow
        // level live (the empty-selection early return below must
        // not skip it).
        if !self.view_state.selection().is_empty() {
            let outcomes = self.view_state.clear_selection();
            self.apply_outcomes(window, &outcomes);
        }
        let flow = self.view_state.flow().clone();
        if matches!(flow, Flow::None) {
            return;
        }
        let was_drag = matches!(flow, Flow::DragMove { .. });
        let outcomes = self.view_state.settle_foreign(state);
        self.apply_outcomes(window, &outcomes);
        if was_drag {
            Self::teardown_runtime_drag(window);
        }
    }

    // -- the review walk (DR-7 chunk 3, #638; plan D-W3) ----------------

    /// The per-pass row bundle for the walk (plan D-W2/D-W4): every
    /// row set derives from `AppState` + the flow's window on every
    /// read — nothing about the rows is stored.
    pub(in crate::adapter) fn walk_rows(
        &self,
        state: &taskboard_domain::AppState,
        cadence: taskboard_domain::ReviewCadence,
        reference: chrono::NaiveDate,
    ) -> (crate::view_state::WalkRows, taskboard_domain::ReviewWindow) {
        let calendar = self.calendar();
        let now = self.day_ctx().now;
        let window = taskboard_domain::review_window(cadence, reference, calendar);
        let rows = crate::view_state::WalkRows {
            inbox: crate::presentation::walk_inbox_rows(state).len(),
            carry: crate::presentation::walk_carry_rows(&state.tasks, cadence, window, now).len(),
        };
        (rows, window)
    }

    /// The opener's reference date (plan D-W6): the due window's day
    /// when `review_due` names one, else the current day window (the
    /// evening review before the boundary).
    pub(in crate::adapter) fn walk_reference(
        &self,
        state: &taskboard_domain::AppState,
    ) -> chrono::NaiveDate {
        let calendar = self.calendar();
        let now = self.day_ctx().now;
        taskboard_domain::review_due(&state.reflection, calendar, now)
            .map_or_else(|| calendar.today(now), |due| due.window().day())
    }

    /// The abort-confirm helpers (J4 D-6): `End review` closes the
    /// walk (decisions already made are applied `StateCommand`s — the
    /// command journal keeps them; un-committed drafts die with the
    /// walk, the documented child-death rule) and restores focus;
    /// `Keep reviewing` drops the arm, the walk stays verbatim.
    pub(in crate::adapter) fn abort_accept(&mut self, window: &AppWindow) -> Vec<FlowOutcome> {
        self.walk_abort_armed = false;
        let outcomes = self.view_state.close_review_walk();
        Self::reset_editing_facts(window);
        window.invoke_refocus_view();
        outcomes
    }

    pub(in crate::adapter) fn abort_dismiss(&mut self) -> Vec<FlowOutcome> {
        self.walk_abort_armed = false;
        vec![FlowOutcome::RefreshNow]
    }

    /// The abort arm rung (J4 R-2): the Entry stage — nothing decided,
    /// nothing typed — closes silently (the one silent exit); any
    /// other live stage arms the confirm.
    pub(in crate::adapter) fn abort_arm(&mut self) -> Vec<FlowOutcome> {
        if self.view_state.walk_stage() == Some(crate::view_state::WalkStage::Entry) {
            return self.view_state.close_review_walk();
        }
        if !self.walk_abort_armed {
            self.walk_abort_armed = true;
            return vec![FlowOutcome::RefreshNow];
        }
        Vec::new()
    }

    /// The walk's intents (the `ReviewWalk(WalkIntent)` carrier's
    /// interpretation): the cursor/back/advance verbs resolve against
    /// the view state with the pass-derived rows; the row verbs
    /// resolve the cursor row from the pass's row ids and route the
    /// honest commands. Typed inert outside its stage; a `Handoff`
    /// from the advance runs the planner transition.
    #[allow(clippy::too_many_lines)] // one verb table, kept flat
    pub(in crate::adapter) fn interpret_walk(
        &mut self,
        window: &AppWindow,
        cadence: taskboard_domain::ReviewCadence,
        reference: chrono::NaiveDate,
        intent: WalkIntent,
        state: &taskboard_domain::AppState,
    ) -> Vec<FlowOutcome> {
        let (rows, window_bounds) = self.walk_rows(state, cadence, reference);
        let calendar = self.calendar();
        match intent {
            WalkIntent::CursorUp => self.view_state.review_walk_cursor(-1, rows),
            WalkIntent::CursorDown => self.view_state.review_walk_cursor(1, rows),
            WalkIntent::Advance => {
                self.walk_abort_armed = false;
                // Completing the embedded planner ends the child
                // `Completed(None)` (D-2's exit contract): the parent's
                // consumption advances the walk to Prompts.
                if self.view_state.walk_stage() == Some(WalkStage::Planner) {
                    self.view_state.clear_plan_cursor();
                    let state_guard = self.state.load();
                    return self.view_state.end_top_flow_with(
                        &state_guard,
                        crate::surfaces::GuestExit::Completed(None),
                    );
                }
                match self.view_state.advance_review_walk(rows) {
                    WalkStep::Advanced => vec![FlowOutcome::RefreshNow],
                    _ => Vec::new(),
                }
            }
            WalkIntent::Prev => {
                self.walk_abort_armed = false;
                // Back out of the planner ends the child `Cancelled`
                // (D-6's rung, J4 US-3/S1): the exit contract returns
                // the walk to Carry, the walk stays open.
                if self.view_state.walk_stage() == Some(WalkStage::Planner) {
                    self.view_state.clear_plan_cursor();
                    let state_guard = self.state.load();
                    let outcomes = self
                        .view_state
                        .end_top_flow_with(&state_guard, crate::surfaces::GuestExit::Cancelled);
                    Self::reset_editing_facts(window);
                    window.invoke_refocus_view();
                    return outcomes;
                }
                let outcomes = self.view_state.back_review_walk(rows);
                Self::reset_editing_facts(window);
                window.invoke_refocus_view();
                outcomes
            }
            WalkIntent::Planner => match self.view_state.walk_open_planner() {
                WalkStep::Planner => vec![FlowOutcome::RefreshNow],
                _ => Vec::new(),
            },
            WalkIntent::Back => {
                // The TrackEntryDismiss ladder's first rung: a focused
                // walk field (the armed date field, a prompt editor)
                // disengages Insert first — the field stays mounted,
                // the ladder stays put.
                if self.shell.mode == Mode::Insert {
                    // The disengage must also retire the focus facts:
                    // the armed field keeps its latch while the walk
                    // means "Insert is off" — the pre-arm would
                    // re-engage on the next stroke otherwise (the
                    // generation re-baseline absorbs the field's stale
                    // decrement).
                    Self::reset_editing_facts(window);
                    return vec![FlowOutcome::SetMode(Mode::Normal)];
                }
                // A confirm already armed: the second Escape dismisses
                // it ("Keep reviewing") — the walk stays verbatim.
                if self.walk_abort_armed {
                    return self.abort_dismiss();
                }
                // The ladder's sub-state rung is the generic Cancelled
                // rung (D-H4): an armed child — the File-to picker,
                // the Pick-a-date field, the embedded planner — ends
                // alone (the planner's Cancelled exit returns the walk
                // to Carry, D-2); without a child the abort law
                // applies (J4 R-2): Entry closes silently, any other
                // stage arms the confirm.
                // The planner child's Cancelled rung clears the K4
                // cursor pane-scoped (US-22/S4 — the landed clear law).
                if self.view_state.walk_stage() == Some(WalkStage::Planner) {
                    self.view_state.clear_plan_cursor();
                }
                let outcomes = match self.view_state.cancel_top_child(state) {
                    Some(outcomes) => outcomes,
                    None => self.abort_arm(),
                };
                // A rung that unmounted a field (the child's death,
                // the stage-back out of Prompts) or the walk itself
                // killed slint focus with it and fired no loss event —
                // the facts reset is the adapter's documented seam for
                // exactly that (the modal-open/view-swap precedent),
                // and the view scope re-anchors (the D15 hand-off).
                Self::reset_editing_facts(window);
                window.invoke_refocus_view();
                outcomes
            }
            // -- the inbox row verbs --
            WalkIntent::InboxFile => {
                let candidates = crate::presentation::walk_file_candidates(state);
                self.view_state.review_walk_file(&candidates)
            }
            WalkIntent::InboxKeep | WalkIntent::CarryLeave => self.step_walk_row(window, rows),
            WalkIntent::FileTargetPrev => {
                let candidates = crate::presentation::walk_file_candidates(state);
                self.view_state.review_walk_file_target(-1, &candidates)
            }
            WalkIntent::FileTargetNext => {
                let candidates = crate::presentation::walk_file_candidates(state);
                self.view_state.review_walk_file_target(1, &candidates)
            }
            WalkIntent::FileCommit => {
                let Some(target) = self.view_state.walk_file_armed() else {
                    return Vec::new();
                };
                if !matches!(
                    self.view_state.walk_flow().as_ref(),
                    Some(Flow::ReviewWalk { .. })
                ) {
                    return Vec::new();
                }
                // The commit completes the child with the armed stack
                // (N3a); the walk's consumption builds the MoveTask
                // (the end-of-stack append the move flow computes —
                // plan D-W4), bumps the tally's `filed` (D-8) and
                // steps the row — R2's order preserved by
                // construction.
                self.view_state.end_top_flow_with(
                    state,
                    crate::surfaces::GuestExit::Completed(Some(
                        crate::surfaces::LevelResult::Stack(target),
                    )),
                )
            }
            // -- the carry row verbs --
            WalkIntent::CarryTomorrow => {
                let Some(task) = self.walk_cursor_task(state, cadence, reference, rows) else {
                    return Vec::new();
                };
                // "Tomorrow" is relative to the REVIEWED day (plan
                // D-W4): the task lands at the next window's opening
                // instant; the planner leg that follows re-times it.
                // Due dates are never touched.
                let next = taskboard_domain::review_window(
                    cadence,
                    reference.succ_opt().unwrap_or(reference),
                    calendar,
                );
                let step = vec![FlowOutcome::Command(
                    taskboard_domain::StateCommand::UpdateTask {
                        id: task,
                        changes: taskboard_domain::TaskChanges {
                            scheduled: Some(Some(next.start())),
                            ..taskboard_domain::TaskChanges::default()
                        },
                    },
                )];
                self.view_state
                    .walk_tally_note(crate::view_state::WalkTallyNote::Moved);
                self.finish_walk_row(window, rows, step)
            }
            WalkIntent::CarryPickDate => self.view_state.review_walk_pick_date(),
            WalkIntent::CarryDateCommit => {
                // The parse-keep-text rule lives in the child (N3b):
                // an unparseable date dispatches nothing and the child
                // stays live with the text (the due editor's parser
                // verbatim, plan D-W4).
                let Some(text) = self
                    .view_state
                    .walk_date()
                    .map(|child| child.date_text.clone())
                else {
                    return Vec::new();
                };
                let Ok(Some(instant)) =
                    crate::due_edit::parse(&text, self.day_ctx().now, &calendar.offset)
                else {
                    return Vec::new();
                };
                let Some(task) = self.walk_cursor_task(state, cadence, reference, rows) else {
                    return Vec::new();
                };
                // The commit completes the child with the parsed
                // instant — the honest payload (D3's "minted
                // adapter-side"). The walk parent's consumption is
                // the default ignore here: the cursor task's
                // derivation needs the adapter's day context, so the
                // UpdateTask is built above and the row advance (with
                // the handoff ask) rides `finish_walk_row` below —
                // R2's order preserved; the child's death IS the
                // flattened in-level clear.
                let state_guard = self.state.load();
                let _ = self.view_state.end_top_flow_with(
                    &state_guard,
                    crate::surfaces::GuestExit::Completed(Some(
                        crate::surfaces::LevelResult::Date(instant),
                    )),
                );
                let step = vec![FlowOutcome::Command(
                    taskboard_domain::StateCommand::UpdateTask {
                        id: task,
                        changes: taskboard_domain::TaskChanges {
                            scheduled: Some(Some(instant)),
                            ..taskboard_domain::TaskChanges::default()
                        },
                    },
                )];
                self.view_state
                    .walk_tally_note(crate::view_state::WalkTallyNote::Moved);
                self.finish_walk_row(window, rows, step)
            }
            // -- the prompt commit verbs --
            WalkIntent::Record => {
                let template = taskboard_domain::DEFAULT_TEMPLATES
                    .iter()
                    .find(|template| template.cadence == cadence)
                    .expect("a curated template per cadence");
                let answers = self
                    .view_state
                    .walk()
                    .map_or(Vec::new(), |walk| walk.answers.clone());
                let text = taskboard_domain::compose_reflection_text(template, &answers);
                if text.is_empty() {
                    // The gentle end (DR-7 D-J9): an all-blank record
                    // dispatches nothing — never relying on a NoOp.
                    // T-47 (#830): the no-op echoes its typed fact on
                    // the status-notice lane (the walk stage is no
                    // longer the only signal).
                    self.notice = Some(Notice {
                        text: crate::presentation::blank_record_notice_phrase(),
                        until: chrono::Utc::now() + crate::presentation::OUTCOME_NOTICE_HORIZON,
                    });
                    self.push_notice(window);
                    // The walk advances to the completion summary (D-8):
                    // `recorded: no` is presented honestly — never a
                    // silent close.
                    if let Some(Flow::ReviewWalk { stage, .. }) = self.view_state.walk_flow_mut() {
                        *stage = crate::view_state::WalkStage::Complete;
                    }
                    let outcomes = vec![FlowOutcome::RefreshNow];
                    self.apply_outcomes(window, &outcomes);
                    // The prompt editors died with the stage change (no
                    // loss events): reset the facts, re-anchor focus.
                    Self::reset_editing_facts(window);
                    window.invoke_refocus_view();
                    return Vec::new();
                }
                // The rating drafts mint at the trust boundary
                // (#1431, the T-15 grammar): blank is the legal
                // unrated; an invalid draft refuses the record —
                // nothing dispatches, the walk stays on the Prompts
                // stage with the flavours rendered.
                let walk_drafts = self
                    .view_state
                    .walk()
                    .map_or((String::new(), String::new()), |walk| {
                        (walk.mood_text.clone(), walk.energy_text.clone())
                    });
                let rating_draft = |draft: &str| -> Option<Option<u8>> {
                    if draft.is_empty() {
                        Some(None)
                    } else {
                        draft
                            .parse::<u8>()
                            .ok()
                            .and_then(|n| taskboard_domain::reflection::Rating::try_new(n).ok())
                            .map(|rating| Some(rating.get()))
                    }
                };
                let mood = rating_draft(&walk_drafts.0);
                let energy = rating_draft(&walk_drafts.1);
                let (Some(mood), Some(energy)) = (mood, energy) else {
                    let flavour = |parsed: Option<Option<u8>>, draft: &str| {
                        if draft.is_empty() {
                            crate::FieldValidity::Neutral
                        } else if parsed.is_some() {
                            crate::FieldValidity::Valid
                        } else {
                            crate::FieldValidity::Invalid
                        }
                    };
                    window.set_walk_mood_validity(flavour(mood, &walk_drafts.0));
                    window.set_walk_energy_validity(flavour(energy, &walk_drafts.1));
                    return Vec::new();
                };
                let mut outcomes = vec![FlowOutcome::Command(
                    taskboard_domain::StateCommand::RecordReflection {
                        cadence,
                        day: window_bounds.day(),
                        window: window_bounds.bounds(),
                        text,
                        mood,
                        energy,
                        flags: std::collections::BTreeSet::new(),
                    },
                )];
                self.view_state
                    .walk_tally_note(crate::view_state::WalkTallyNote::Recorded);
                if let Some(Flow::ReviewWalk { stage, .. }) = self.view_state.walk_flow_mut() {
                    *stage = crate::view_state::WalkStage::Complete;
                }
                outcomes.push(FlowOutcome::RefreshNow);
                self.apply_outcomes(window, &outcomes);
                // The prompt editors died with the stage change (no
                // loss events): reset the facts, re-anchor focus.
                Self::reset_editing_facts(window);
                window.invoke_refocus_view();
                Vec::new()
            }
            // The guilt-free exit (P12): lands on the completion
            // summary — `recorded: no` is presented honestly (D-8).
            WalkIntent::Finish => {
                if let Some(Flow::ReviewWalk { stage, .. }) = self.view_state.walk_flow_mut() {
                    *stage = crate::view_state::WalkStage::Complete;
                }
                let outcomes = vec![FlowOutcome::RefreshNow];
                self.apply_outcomes(window, &outcomes);
                Self::reset_editing_facts(window);
                window.invoke_refocus_view();
                Vec::new()
            }
            // The completion summary's exit (D-8): close + restore.
            WalkIntent::Complete => {
                let outcomes = self.view_state.close_review_walk();
                self.apply_outcomes(window, &outcomes);
                Self::reset_editing_facts(window);
                window.invoke_refocus_view();
                Vec::new()
            }
            // The abort ladder (J4 D-6): the arm (the host's outside
            // click; Entry closes silently), the accept (close + the
            // applied decisions stay), the dismiss (verbatim stay).
            WalkIntent::AbortArm => {
                let outcomes = self.abort_arm();
                if self.view_state.walk_flow().is_none() {
                    // The Entry silent exit closed the walk.
                    Self::reset_editing_facts(window);
                    window.invoke_refocus_view();
                }
                outcomes
            }
            WalkIntent::AbortAccept => {
                let outcomes = self.abort_accept(window);
                self.apply_outcomes(window, &outcomes);
                Vec::new()
            }
            WalkIntent::AbortDismiss => {
                let outcomes = self.abort_dismiss();
                self.apply_outcomes(window, &outcomes);
                Vec::new()
            }
        }
    }

    /// The cursor row's task id in the Carry forward stage (typed
    /// inert outside it or past the rows).
    pub(in crate::adapter) fn walk_cursor_task(
        &self,
        state: &taskboard_domain::AppState,
        cadence: taskboard_domain::ReviewCadence,
        reference: chrono::NaiveDate,
        rows: crate::view_state::WalkRows,
    ) -> Option<TaskId> {
        let Some(Flow::ReviewWalk { cursor, .. }) = self.view_state.walk_flow() else {
            return None;
        };
        let window = taskboard_domain::review_window(cadence, reference, self.calendar());
        crate::presentation::walk_carry_rows(&state.tasks, cadence, window, self.day_ctx().now)
            .get(cursor)
            .copied()
            .filter(|_| rows.carry > 0)
    }

    /// The armed walk child dies with the row step (N3b): the
    /// flattened model's next-row clear — the child's Cancelled end
    /// through the one exit seam.
    pub(in crate::adapter) fn end_armed_walk_child(&mut self, window: &AppWindow) {
        if self.view_state.walk_date_child() || self.view_state.walk_file_armed().is_some() {
            let state = self.state.load();
            let _ = self
                .view_state
                .end_top_flow_with(&state, crate::surfaces::GuestExit::Cancelled);
            self.refresh_forced(window);
        }
    }

    /// A commandless row step (`Keep in inbox`, `Leave as is`): the
    /// tally's `left` bump (D-8), then the cursor/stage step (the
    /// skip law lands on the next open stage).
    pub(in crate::adapter) fn step_walk_row(
        &mut self,
        window: &AppWindow,
        rows: crate::view_state::WalkRows,
    ) -> Vec<FlowOutcome> {
        self.end_armed_walk_child(window);
        self.view_state
            .walk_tally_note(crate::view_state::WalkTallyNote::Left);
        match self.view_state.review_walk_next_row(rows) {
            WalkStep::Advanced => vec![FlowOutcome::RefreshNow],
            _ => Vec::new(),
        }
    }

    /// A row step that dispatches `step` first (the carry commits):
    /// the command lands, then the cursor advances (the skip law may
    /// pass straight through to Prompts).
    pub(in crate::adapter) fn finish_walk_row(
        &mut self,
        window: &AppWindow,
        rows: crate::view_state::WalkRows,
        step: Vec<FlowOutcome>,
    ) -> Vec<FlowOutcome> {
        self.end_armed_walk_child(window);
        let mut outcomes = step;
        match self.view_state.review_walk_next_row(rows) {
            WalkStep::Advanced => {
                outcomes.push(FlowOutcome::RefreshNow);
                outcomes
            }
            _ => outcomes,
        }
    }

    /// The leaving rule (D9, factored out of the table): a view
    /// switch by effect settles the flow through the same per-kind
    /// rule every other trigger uses (round-3 R1–R3). Its r2 D13
    /// dismiss half moved to the `apply_effect` choke point (r4
    /// chunk 4, D14): this method's only caller is `interpret`'s
    /// leaving block, itself only reached from `apply_effect`, so the
    /// identity diff subsumes the dismiss on every path — one
    /// mechanism, one site.
    pub(in crate::adapter) fn settle_flows_on_leaving(
        &mut self,
        window: &AppWindow,
        state: &taskboard_domain::AppState,
    ) {
        self.settle_flow(window, state);
        // The priority pickers are flow-free (UX R-3 D-1): the same
        // leaving rule closes them — a view switch never strands an
        // open quick-list (the recur flags' outlive rule).
        if self.priority_open.take().is_some() {
            self.refresh_forced(window);
        }
    }

    // -- test seams (the lib flow/buffer harnesses; never compiled
    //    into a release binary) ------------------------------------

    /// The pair-table harness drives the interpretation table
    /// directly, one `(Flow, effect)` pass at a time.
    #[cfg(test)]
    pub(crate) fn interpret_for_test(&mut self, window: &AppWindow, effect: UiEffect) {
        self.interpret(window, effect);
    }

    /// The dismiss-walk harness (r4 chunk 4, D14's structural walk):
    /// the tooltip seam lives on `apply_effect` — the choke point —
    /// not on `interpret`, so the walk drives the full pass.
    #[cfg(test)]
    pub(crate) fn apply_effect_for_test(&mut self, window: &AppWindow, effect: UiEffect) {
        self.apply_effect(window, effect);
    }

    /// The dismiss-walk harness, palette half: seeds the palette's
    /// resolved row targets (the walk's `ShowTask`/`InvokeSelected`
    /// preconditions).
    #[cfg(test)]
    pub(crate) fn test_seed_palette(&mut self, targets: Vec<taskboard_domain::SearchTarget>) {
        self.tables.palette = targets;
    }

    /// The control walk's guard seam (round-3 D25): drives one Tab
    /// stroke through the real intercept path so the re-entrancy
    /// rejection is asserted against the production guard, and the
    /// cleared flag resolves the bound walk row exactly once.
    #[cfg(test)]
    pub(crate) fn intercept_tab_for_test(&mut self, window: &AppWindow) -> bool {
        self.intercept(
            window,
            &SharedString::from("\t"),
            slint::language::KeyboardModifiers::default(),
        )
    }

    /// Resets every machine the table reads so pair passes are
    /// order-independent (the flow table and the buffer journeys).
    #[cfg(test)]
    pub(crate) fn test_reset(&mut self) {
        self.view_state = ViewState::new();
        self.shell = KeymapShell::default();
        self.menu_entries.clear();
        self.root_entries.clear();
        self.menu_anchor = MenuAnchor::default();
        self.width_drag = None;
        self.scale_drag = None;
        self.walk_abort_armed = false;
    }

    /// The date editor's adapter-owned buffers, for the §8 invariant
    /// journeys: non-empty ⇔ `Flow::EditDate` armed. (The journeys
    /// land with the buffer-invariants commit; the seam carries its
    /// allow until then.)
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn due_buffers_empty(&self) -> bool {
        self.view_state.date_editor().is_none_or(|editor| {
            editor.buffer.is_empty() && editor.echo.is_empty() && editor.picker.is_none()
        })
    }

    /// Seeds the date editor's buffers non-empty (the §8 invariant
    /// journeys).
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn test_seed_due_buffers(&mut self) {
        let editor = self
            .view_state
            .date_editor_mut()
            .expect("the \u{a7}8 journeys arm the date editor first");
        editor.buffer = "tomorrow".to_owned();
        editor.echo = "stale echo".to_owned();
        editor.picker = Some(crate::due_edit::DuePickerState::open(
            chrono::Local::now().date_naive(),
            None,
            &chrono::Local,
        ));
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn arm_width_drag(&mut self, left: StackId, right: StackId) {
        self.width_drag = Some((left, right, 100.0));
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn width_drag_armed(&self) -> bool {
        self.width_drag.is_some()
    }

    /// The offer walk's answer (D-H5/D4): the effect is offered to
    /// the levels innermost-first — a flow level answers (its kept
    /// arms, else the total `resolve` table), a view level bubbles
    /// (chunk 1 gives view levels no kept arms; the root's
    /// `(Flow::None, _)` arms are total). The walk therefore always
    /// stops at the innermost flow level, or at the root when none is
    /// live, and a `Handled` effect never reaches an outer level —
    /// `Surfaces::offer` is the walk, the chain proptest instruments
    /// it. The interpretation table is keyed on this level's flow.
    pub(in crate::adapter) fn responding_flow(&self) -> Flow {
        self.view_state
            .surfaces()
            .offer(|level| level.flow().cloned())
            .unwrap_or(Flow::None)
    }

    /// The recur editor's seeding seam (round-7 D-7.1, plan D-D1): the
    /// level's draft from the task's rule when one carries, else the
    /// anchor-derived bare-flag defaults — the anchor is
    /// `series_reference_date` (never raw-UTC, never "today" unless
    /// the chain lands there).
    pub(in crate::adapter) fn seed_recur_editor(
        &self,
        state: &taskboard_domain::AppState,
        task: taskboard_domain::TaskId,
    ) -> crate::level_state::RecurEditorState {
        let now = self.day_ctx().now;
        let calendar = self.calendar();
        state.tasks.get(&task).map_or_else(
            || crate::presentation::seed_recur_editor(None, calendar.today(now)),
            |t| {
                let anchor = taskboard_domain::series_reference_date(t, calendar, now);
                crate::presentation::seed_recur_editor(t.recur.as_ref(), anchor)
            },
        )
    }

    /// The recur editor's pushed model (round-7 D-7.1): the buffer's
    /// mirrors, the per-field flavours, the live echo (the assembled
    /// rule's canonical text form, or the typed invalid-field hint),
    /// and the commit's gate. Computed per pass while the flow holds
    /// — the widget never holds state.
    pub(in crate::adapter) fn recur_editor_model(&self) -> crate::RecurEditorModel {
        use crate::level_state::RecurPatternKind;
        use crate::presentation::{FieldFlavour, recur_field_flavours, recur_field_hint};

        let flavour = |f: FieldFlavour| match f {
            FieldFlavour::Neutral => crate::FieldValidity::Neutral,
            FieldFlavour::Valid => crate::FieldValidity::Valid,
            FieldFlavour::Invalid => crate::FieldValidity::Invalid,
        };
        let Some(draft) = self.view_state.recur_editor() else {
            return Self::recur_editor_model_closed();
        };
        let now = self.day_ctx().now;
        let day = self.calendar();
        let [
            every,
            weekly,
            // The weekday toggles carry no field mark (the echo names
            // an empty set); the flavour stays unused here.
            _weekdays,
            monthly_day,
            monthly_nth,
            until,
            count,
        ] = recur_field_flavours(draft, now, day);
        // The echo: the rule's canonical text form when the whole
        // draft mints over the (open) subject; the typed hint
        // otherwise. Without a subject the overlay is closing — the
        // closed model's shape.
        let head = self
            .view_state
            .subject_task()
            .and_then(|task| self.state.load().tasks.get(&task).cloned());
        let (echo, echo_valid) = match head.as_ref().filter(|t| t.is_open()) {
            Some(head) => match crate::presentation::assemble_recur_draft(draft, head, day, now) {
                Ok(rule) => (SharedString::from(rule.text_form()), true),
                Err(Some((field, invalid))) => {
                    (SharedString::from(recur_field_hint(field, &invalid)), false)
                }
                Err(None) => (SharedString::new(), false),
            },
            None => (SharedString::new(), false),
        };
        // The commit's gate is the plan's named validity fold (the
        // same parse, the flag-only view).
        let commit_enabled = crate::presentation::recur_field_validity(draft, now, day).is_ok()
            && head.as_ref().is_some_and(taskboard_domain::Task::is_open);
        crate::RecurEditorModel {
            pattern_kind: match draft.pattern {
                RecurPatternKind::Daily => 0,
                RecurPatternKind::Weekly => 1,
                RecurPatternKind::Monthly => 2,
            },
            every_days_text: SharedString::from(draft.every_days.clone()),
            every_days_validity: flavour(every),
            weekly_interval_text: SharedString::from(draft.weekly_interval.clone()),
            weekly_interval_validity: flavour(weekly),
            weekdays_checked: ModelRc::new(slint::VecModel::from(draft.weekdays.to_vec())),
            monthly_mode: match draft.monthly_mode {
                crate::level_state::MonthlyMode::OnDay => 0,
                crate::level_state::MonthlyMode::NthWeekday => 1,
            },
            monthly_day_text: SharedString::from(draft.monthly_day.clone()),
            monthly_day_validity: flavour(monthly_day),
            monthly_nth_text: SharedString::from(draft.monthly_nth.clone()),
            monthly_nth_validity: flavour(monthly_nth),
            monthly_weekday: i32::try_from(draft.monthly_nth_weekday.num_days_from_monday())
                .expect("a weekday index fits i32"),
            bound_kind: match draft.bound {
                crate::level_state::RecurBoundKind::Never => 0,
                crate::level_state::RecurBoundKind::Until => 1,
                crate::level_state::RecurBoundKind::Count => 2,
            },
            until_text: SharedString::from(draft.until_text.clone()),
            until_validity: flavour(until),
            count_text: SharedString::from(draft.count.clone()),
            count_validity: flavour(count),
            echo,
            echo_valid,
            commit_enabled,
            detach_armed: draft.detach_armed,
        }
    }

    /// The closed editor's neutral model (the mount gate's else half).
    pub(in crate::adapter) fn recur_editor_model_closed() -> crate::RecurEditorModel {
        crate::RecurEditorModel {
            pattern_kind: 0,
            every_days_text: SharedString::new(),
            every_days_validity: crate::FieldValidity::Neutral,
            weekly_interval_text: SharedString::new(),
            weekly_interval_validity: crate::FieldValidity::Neutral,
            weekdays_checked: ModelRc::new(slint::VecModel::from(vec![
                false, false, false, false, false, false, false,
            ])),
            monthly_mode: 0,
            monthly_day_text: SharedString::new(),
            monthly_day_validity: crate::FieldValidity::Neutral,
            monthly_nth_text: SharedString::new(),
            monthly_nth_validity: crate::FieldValidity::Neutral,
            monthly_weekday: 0,
            bound_kind: 0,
            until_text: SharedString::new(),
            until_validity: crate::FieldValidity::Neutral,
            count_text: SharedString::new(),
            count_validity: crate::FieldValidity::Neutral,
            echo: SharedString::new(),
            echo_valid: true,
            commit_enabled: false,
            detach_armed: false,
        }
    }
}

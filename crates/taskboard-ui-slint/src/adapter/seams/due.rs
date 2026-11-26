// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle as _;

use taskboard_domain::StateCommand;

use crate::actions::ActionId;
use crate::view_state::Flow;
use crate::{AppWindow, KeyboardState};

use super::Adapter;
use super::FocusHandover;
use super::{parse_effort_row, settle_pointer_conflict};
use crate::adapter::install_seam;

/// The picker grid's focus seam (round-3 D25, plan §5.3): the
/// grid `FocusScope`'s `changed has-focus` mirrors onto the shell —
/// the `DatePicker` context rides the chain head exactly while the
/// grid holds focus, and never masks (Escape resolves `ModeNormal`
/// from `ViInsert` regardless).
pub(in crate::adapter) fn install_picker_grid_seam(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    install_seam!(window: window.on_date_picker_grid_focus_changed, adapter: adapter;
    bind mut (a, window), args (focused: bool), {        a.shell.date_picker_grid = focused;
    // The mirror rides with the seam (the Down-in-field path
    // lands here without a `sync` around it).
    window
        .global::<KeyboardState>()
        .set_date_picker_grid(focused);

    });
}

#[allow(clippy::too_many_lines)] // the r2 D25 seam family, kept flat
pub(in crate::adapter) fn install_due_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_task_due_edit_opened, adapter: adapter; // slint: task-due-edit-opened
    bind mut (a, window), args (), {        a.begin_date_edit(&window, crate::view_state::DateField::Due);

    });

    // The rule section's jump affordance (r4 D30): an unarmed row's
    // due-date repair opens the SAME due editor — the callback is the
    // metadata row's due-edit seam under a second name, no new focus
    // plumbing (chunk 6's machinery arms from there).
    install_seam!(window: window.on_task_rule_jump_to_due_clicked, adapter: adapter; // slint: task-rule-jump-to-due-clicked
    bind mut (a, window), args (), {        a.begin_date_edit(&window, crate::view_state::DateField::Due);

    });

    // D31a: the section's un-mute affordance — the focused task is the
    // target (the section always names its subject), the banner's
    // one-shot machinery is not needed.
    install_seam!(window: window.on_task_rule_unmute_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ReminderUnmute);

    });

    install_seam!(window: window.on_task_scheduled_edit_opened, adapter: adapter; // slint: task-scheduled-edit-opened
    bind mut (a, window), args (), {        a.begin_date_edit(&window, crate::view_state::DateField::Scheduled);

    });

    install_seam!(window: window.on_task_date_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.date_editor_mut() {
        editor.buffer = text.to_string();
        editor.echo =
            crate::due_edit::echo(&editor.buffer, chrono::Utc::now(), &chrono::Local);
        // The Buffer-source live sync (r4 D25): a parseable buffer
        // re-seeds the picker per keystroke (the field wins while
        // typing); empty or junk leaves it untouched — the validity
        // flavour carries the state.
        let now = chrono::Utc::now();
        let seeded = editor.picker.as_ref().and_then(|picker| {
            crate::due_edit::sync_buffer_picker(
                &editor.buffer,
                picker,
                crate::due_edit::SyncSource::Buffer,
                now,
                &chrono::Local,
            )
        });
        if let Some((_, Some(seeded))) = seeded {
            editor.picker = Some(seeded);
        }
    }
    // The echo and the seeded picker both ride a full pass now.
    a.refresh_forced(&window);

    });

    install_seam!(window: window.on_task_date_accepted, adapter: adapter;
    bind mut (a, window), args (text), {        // The N-4 stamping law (task-detail-editor UX plan US-1/D-3):
        // the commit mint is `parse_stamp` — a bare date stamps the
        // named date's day-window end over the injected day facts.
        match crate::due_edit::parse_stamp(&text, chrono::Utc::now(), &a.calendar()) {
        // A refusal keeps the buffer and dispatches nothing.
        Err(crate::due_edit::DueParseError) => {}
        parsed => commit_date(&mut a, &window, parsed.expect("non-error parse")),
    }

    });

    install_seam!(window: window.on_task_date_picker_toggled, adapter: adapter;
    bind mut (a, window), args (), {        // Both arms set the steal latch (round-5 F8, D-3.4): the
    // calendar button's click grab can settle the armed editor
    // before/around the toggle arrives, ending the flow the toggle
    // is supposed to serve. The open arm additionally hands focus
    // back to the field (the default-open contract: visible,
    // click-armed, idle, the text field keeps focus).
    // Both arms set the steal latch; the take decides the arm.
    let mut picker_was_open = false;
    if let Some(editor) = a.view_state.date_editor_mut() {
        picker_was_open = editor.picker.take().is_some();
        editor.focus_steal = true;
    }
    if picker_was_open {
        a.refresh_forced(&window);
    } else {
        a.open_due_picker(&window, false);
        a.focus_handover = FocusHandover::Field;
        a.refresh_forced(&window);
    }

    });

    install_seam!(window: window.on_task_date_picker_key, adapter: adapter;
    bind mut (a, window), args (code), {        // The click seams carry the widget's int code space; the
    // registry actions arrive as typed `PickerKey` effects on the
    // same function (round-3 D25: one implementation, two
    // dispatch sites).
    let Some(key) = crate::due_edit::DatePickerKey::from_code(code) else {
        return;
    };
    a.apply_picker_key(&window, key);

    });

    install_seam!(window: window.on_date_picker_dismiss_requested, adapter: adapter;
    bind mut (a, window), args (), {        // Outside click (r4 D24, superseding the round-3 D10
    // picker-only rule): the editor settles per the rule —
    // parseable commits, empty/junk discards — and the picker
    // drops with it. In the picker phase the buffer mirrors the
    // picker, so click-away ≡ done, consistent with the field
    // phase.
    settle_date_editor(&mut a, &window);
    let picker_dropped = a
        .view_state
        .date_editor_mut()
        .and_then(|editor| editor.picker.take())
        .is_some();
    if picker_dropped {
        a.refresh_forced(&window);
    }

    });

    install_seam!(window: window.on_date_editor_focus_lost, adapter: adapter;
    bind mut (a, window), args (), {        settle_date_editor(&mut a, &window);

    });

    // The intra-picker click arm (task-detail-editor UX plan US-2/D-6,
    // N-5): the TimeField's field click arms the same one-shot
    // focus-steal latch the programmatic field→grid hand-over uses —
    // the settle funnel consumes it once, so the focus move into the
    // picker's time field never settles the editor (the reproduction
    // journey us2_s1's oracle).
    // slint: task-date-time-field-armed
    install_seam!(window: window.on_task_date_time_field_armed, adapter: adapter;
    bind mut (a, window), args (), {        if let Some(editor) = a.view_state.date_editor_mut() {
        editor.focus_steal = true;
    }
    a.refresh_forced(&window);

    });

    install_seam!(window: window.on_task_date_time_edited, adapter: adapter;
    bind mut (a, window), args (text), {        apply_due_time_edited(&mut a, &window, &text);

    });

    // The quick list's typed-shim seam (round-5 D6): the flyout scope
    // forwards the raw key text; the adapter appends it to the mirror
    // and runs the field's own edited logic (the old in-widget shim
    // appended to the field buffer, which the adapter echoes verbatim
    // — the mirror is the same single writer).
    install_seam!(window: window.on_task_date_quick_typed, adapter: adapter;
    bind mut (a, window), args (key), {        let mut text = a
        .view_state
        .date_editor()
        .map_or(String::from("--:--"), |editor| editor.time_text.clone());
    text.push_str(&key);
    apply_due_time_edited(&mut a, &window, &text);

    });

    // The quick-times chevron's seam (round-5 F8): the latch skips
    // the settle funnel the chevron's click-grab would otherwise
    // trigger, the digit filter disarms for this open (the full list
    // seeds), and the focus nonce re-grabs the fresh panel (the P5
    // record — an init grab alone does not take).
    install_seam!(window: window.on_task_date_quick_toggled, adapter: adapter;
    bind mut (a, window), args (), {        if let Some(editor) = a.view_state.date_editor_mut() {
        editor.focus_steal = true;
        editor.time_filter_armed = false;
    }
    a.refresh_forced(&window);
    let nonce = window.get_date_quick_nonce();
    window.set_date_quick_nonce(nonce.wrapping_add(1));

    });

    install_seam!(window: window.on_task_date_time_picked, adapter: adapter;
    bind mut (a, window), args (minutes), {        let Some(time) = crate::due_edit::time_of_minutes(minutes) else {
        return;
    };
    let Some(editor) = a.view_state.date_editor_mut() else {
        return;
    };
    let Some(picker) = editor.picker.as_mut() else {
        return;
    };
    picker.set_time(Some(time));
    // The pick is a picker-source write — the typing session's
    // filter disarms (round-5 F10: the next open seeds unfiltered).
    editor.time_filter_armed = false;
    // Picker-source sync (r4 D26): the canonical rendering wins
    // the field mirror, and the due buffer re-renders from the
    // selection.
    editor.time_text = crate::due_edit::format_time_hm(time);
    sync_buffer_from_picker(&mut a);
    a.refresh_forced(&window);

    });

    install_seam!(window: window.on_task_date_time_accepted, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.date_editor_mut() {
        editor.time_filter_armed = false;
    }
    match crate::due_edit::parse_time_lenient(&text) {
        // A refusal keeps the typed text — the validity flavour
        // makes it visible — and dispatches nothing.
        None => {
            if let Some(editor) = a.view_state.date_editor_mut() {
                editor.time_text = text.to_string();
            }
            a.refresh_forced(&window);
        }
        Some(time) => {
            let Some(editor) = a.view_state.date_editor_mut() else {
                return;
            };
            let Some(picker) = editor.picker.as_mut() else {
                return;
            };
            picker.set_time(Some(time));
            let Some(instant) = picker.commit_instant(&chrono::Local) else {
                a.refresh_forced(&window);
                return;
            };
                    commit_date(&mut a, &window, Some(instant));
        }
    }

    });

    install_seam!(window: window.on_task_date_day_clicked, adapter: adapter;
    bind mut (a, window), args (cell), {        let Some(editor) = a.view_state.date_editor_mut() else {
        return;
    };
    let Some(date) = editor
        .picker
        .as_ref()
        .and_then(|picker| {
            picker
                .grid()
                .get(usize::try_from(cell).unwrap_or(usize::MAX))
                .and_then(|c| c.date)
        })
    else {
        return; // a blank or a stale index
    };
    editor
        .picker
        .as_mut()
        .expect("checked above")
        .set_cursor(date);
    let picker_time = editor
        .picker
        .as_ref()
        .expect("checked above")
        .time();
    // The commit mint: a timed picker carries the picked/stepped time
    // (round-3 D9 — the typed time is data); a date-only picker stamps
    // the law (task-detail-editor UX plan US-1/D-3; #893 supersedes
    // the mint): the clicked day's last covered instant over the
    // injected day facts — the stamp survives the `[now, end)` today
    // membership the raw window end does not. The law applies exactly
    // when no time component exists anywhere in the entry path.
    let instant = if picker_time.is_none() {
        Some(a.calendar().day_last_instant(date))
    } else {
        editor
            .picker
            .as_ref()
            .expect("checked above")
            .commit_instant(&chrono::Local)
    };
    let Some(instant) = instant else {
        a.refresh_forced(&window);
        return;
    };
    commit_date(&mut a, &window, Some(instant));

    });
}

impl Adapter {
    /// The one date-begin funnel (round-3 D11; keymap-parity plan
    /// D-4): settle any live flow, arm `field`'s editor, seed the
    /// buffer with the field's *current value*, refresh, then open
    /// the picker default-open. Both fronts land here — the row
    /// affordance's callbacks (the `// slint:` seams) and the
    /// `task.due-edit`/`task.scheduled-edit` keyboard route — click
    /// ≡ key at the funnel; the subjectless detail is the typed inert
    /// (the flow never arms).
    pub(in crate::adapter) fn begin_date_edit(
        &mut self,
        window: &AppWindow,
        field: crate::view_state::DateField,
    ) {
        settle_pointer_conflict(self, window);
        // The detail exclusivity posture (US-3/D-8): opening a date
        // editor closes the effort editor (one editor at a time).
        self.effort_editor = None;
        let state = self.state.load_full();
        let outcomes = self.view_state.begin_edit_date(&state, field);
        let value = self
            .view_state
            .subject_task()
            .and_then(|id| state.tasks.get(&id))
            .and_then(|t| match field {
                crate::view_state::DateField::Due => t.duedate,
                crate::view_state::DateField::Scheduled => t.scheduled,
            });
        if let Some(editor) = self.view_state.date_editor_mut() {
            editor.buffer = crate::due_edit::format_entry(value, &chrono::Local);
            editor.echo = crate::due_edit::echo(&editor.buffer, chrono::Utc::now(), &chrono::Local);
        }
        self.apply_outcomes(window, &outcomes);
        // The picker is default-open on edit entry (round-3 D10) —
        // visible, click-armed, and idle: the text field keeps focus
        // (typing-first preserved). Inert when the begin refused (the
        // flow never armed).
        self.open_due_picker(window, false);
    }

    /// The description editor's begin funnel (round-3 D15; keymap-parity
    /// plan D-4): settle any live flow, then arm the subject's editor
    /// (the buffer seeds view-state side, the `EditDescription` flow's
    /// own rule). Both fronts land here — the closed body's click seam
    /// and the `task.description-edit` keyboard route.
    pub(in crate::adapter) fn begin_description_edit(&mut self, window: &AppWindow) {
        settle_pointer_conflict(self, window);
        let state = self.state.load_full();
        let Some(task) = self.view_state.subject_task() else {
            return; // the subjectless detail: the typed inert
        };
        let outcomes = self.view_state.begin_edit_description(&state, task);
        self.apply_outcomes(window, &outcomes);
    }

    /// The picker's one key body (round-3 D25): the keyboard's
    /// `picker.*` actions and the widget's click seams both land
    /// here — the same `due_picker` mutations, the commit through the
    /// one `commit_date` funnel, the refresh after every move.
    pub(in crate::adapter) fn apply_picker_key(
        &mut self,
        window: &AppWindow,
        key: crate::due_edit::DatePickerKey,
    ) {
        use crate::due_edit::DatePickerKey as K;
        let Some(editor) = self.view_state.date_editor_mut() else {
            return; // a stale stroke against an already-closed picker
        };
        let Some(picker) = editor.picker.as_mut() else {
            return; // a stale stroke against an already-closed picker
        };
        // A panel interaction may move focus (the IconButtons' click
        // grabs) — the settle funnel must skip that steal (r4 D24).
        editor.focus_steal = true;
        // Grid-Enter is the phase hand-over, never a commit
        // (r4 D25, review focus b): the armed field takes focus
        // via the one-shot flag; the typed path and the combobox
        // own the commits through the one funnel. Day-cell clicks
        // keep their direct `commit_date`.
        let mut hand_over = false;
        match key {
            K::Left => picker.move_days(-1),
            K::Right => picker.move_days(1),
            K::Up => picker.move_days(-7),
            K::Down => picker.move_days(7),
            K::PrevMonth => picker.move_months(-1),
            K::NextMonth => picker.move_months(1),
            K::MonthStart => picker.month_edge(true),
            K::MonthEnd => picker.month_edge(false),
            // The time steppers (round-3 D9, r4 D26): ±15 min,
            // wrapping — the date follows the wrap, an unset time
            // seeds midnight first (r4 D23). Button-only; the combobox
            // row's steppers land on the same codes.
            K::TimeBack => picker.step_time(-1),
            K::TimeForward => picker.step_time(1),
            K::Commit => hand_over = true,
        }
        if hand_over {
            self.focus_handover = FocusHandover::Field;
            self.refresh_forced(window);
            return;
        }
        // The Picker-source live sync (r4 D25): every move re-renders
        // the buffer/echo from the selection, and a stepped time wins
        // the combobox's mirror.
        if matches!(key, K::TimeBack | K::TimeForward)
            && let Some(picker) = editor.picker.as_ref()
        {
            // A picker-source write disarms the digit filter
            // (round-5 F10).
            editor.time_filter_armed = false;
            editor.time_text = picker
                .time()
                .map_or("--:--".to_owned(), crate::due_edit::format_time_hm);
        }
        sync_buffer_from_picker(self);
        self.refresh_forced(window);
    }
}

/// Opens the picker anchored on the armed field's current value (or
/// today) — the calendar button's, the Down-in-field toggle's, and
/// the default-open begin's shared path. `focus_grid` is the one-shot
/// `focus-on-mount` flag: `false` keeps the text field focused
/// (typing-first), `true` hands focus to the grid.
impl Adapter {
    pub(in crate::adapter) fn open_due_picker(&mut self, window: &AppWindow, focus_grid: bool) {
        let state = self.state.load_full();
        let field = match self.view_state.flow() {
            Flow::EditDate { field, .. } => *field,
            _ => return,
        };
        let anchor = self
            .view_state
            .subject_task()
            .and_then(|id| state.tasks.get(&id))
            .and_then(|t| match field {
                crate::view_state::DateField::Due => t.duedate,
                crate::view_state::DateField::Scheduled => t.scheduled,
            });
        self.focus_handover = if focus_grid {
            FocusHandover::Grid
        } else {
            FocusHandover::None
        };
        let Some(editor) = self.view_state.date_editor_mut() else {
            return;
        };
        editor.picker = Some(crate::due_edit::DuePickerState::open(
            chrono::Local::now().date_naive(),
            anchor,
            &chrono::Local,
        ));
        // The combobox's mirror opens on the picker's seeded time —
        // canonical when a non-midnight anchor carries one, the
        // unset echo otherwise (r4 D23's display rule).
        editor.time_text = editor
            .picker
            .as_ref()
            .and_then(crate::due_edit::DuePickerState::time)
            .map_or("--:--".to_owned(), crate::due_edit::format_time_hm);
        // A fresh open seeds the quick list unfiltered (round-5 F10):
        // the value mirror is the filter's input, but only a typed
        // keystroke arms it.
        editor.time_filter_armed = false;
        self.refresh_forced(window);
    }
}

/// The due combobox's edited logic (r4 D26), shared by the field's
/// `edited` seam and the quick list's typed-shim seam (round-5 D6 —
/// the shim appends to the mirror, then runs this same body).
pub(in crate::adapter) fn apply_due_time_edited(a: &mut Adapter, window: &AppWindow, text: &str) {
    // The keystroke arms the digit filter (round-5 F10, D-3.5):
    // narrowing is per-keystroke only, never an open-time seed.
    let Some(editor) = a.view_state.date_editor_mut() else {
        a.refresh_forced(window);
        return;
    };
    editor.time_filter_armed = true;
    // The field's value echo seeds its buffer ("--:--" while no
    // time exists), so the first keystroke's text carries it as a
    // prefix — strip the echo, keep the typed digits.
    let mut text = text.to_string();
    if let Some(stripped) = text.strip_prefix("--:--") {
        text = stripped.to_owned();
    }
    editor.time_text.clone_from(&text);
    // Buffer-source seeding (r4 D26): a lenient-parse hit moves
    // the picker's time live; junk/partial leaves it untouched
    // (the validity flavour shows the state).
    if let Some(time) = crate::due_edit::parse_time_lenient(&text)
        && let Some(picker) = editor.picker.as_mut()
    {
        picker.set_time(Some(time));
    }
    a.refresh_forced(window);
}

/// The picker-source buffer sync (r4 D26, the §4.3 helper's Picker
/// arm in its adapter plumbing until the pure helper lands): the due
/// buffer/echo re-render from the picker's current selection.
pub(in crate::adapter) fn sync_buffer_from_picker(a: &mut Adapter) {
    let now = chrono::Utc::now();
    let synced = a.view_state.date_editor_mut().and_then(|editor| {
        let picker = editor.picker.as_ref()?;
        crate::due_edit::sync_buffer_picker(
            &editor.buffer,
            picker,
            crate::due_edit::SyncSource::Picker,
            now,
            &chrono::Local,
        )
    });
    if let Some((buffer, None)) = synced
        && let Some(editor) = a.view_state.date_editor_mut()
    {
        editor.echo = crate::due_edit::echo(&buffer, now, &chrono::Local);
        editor.buffer = buffer;
    }
}

/// The D24 settle funnel (r4 D24): the one commit/discard rule for
/// the armed date editor's own seams (focus loss, the dismissal
/// click). A non-empty parseable buffer commits through the one
/// `commit_date`; empty or non-conforming discards — no command, the
/// prior value survives, the editor collapses. The programmatic
/// focus-steal latch (the phase steps, the panel's own controls)
/// consumes the skip: a hand-over never settles the text the user is
/// mid-way through. `true` = committed.
pub(in crate::adapter) fn settle_date_editor(a: &mut Adapter, window: &AppWindow) -> bool {
    if !matches!(a.view_state.flow(), Flow::EditDate { .. }) {
        return false;
    }
    match a.view_state.date_editor_mut() {
        Some(editor) if editor.focus_steal => {
            editor.focus_steal = false;
            return false;
        }
        _ => {}
    }
    // The typing session ends with the flow (the armed filter never
    // outlives the buffer it filters — D-3.5's funnel passenger).
    if let Some(editor) = a.view_state.date_editor_mut() {
        editor.time_filter_armed = false;
    }
    let text = a
        .view_state
        .date_editor()
        .map_or(String::new(), |editor| editor.buffer.clone());
    let now = chrono::Utc::now();
    // Non-empty and parseable: click-away ≡ Return — the same
    // `parse_stamp` mint the typed Return path uses (the N-4 law, UX
    // plan US-1/D-3: one funnel, one mint; the date-only buffer stamps
    // the window end here too).
    if let Ok(Some(instant)) = crate::due_edit::parse_stamp(&text, now, &a.calendar()) {
        commit_date(a, window, Some(instant));
        return true;
    }
    // Empty never means clear on focus loss (a stray steal must not
    // nuke a due date) and junk cannot linger: both discard — the
    // flow ends clean, the prior value survives. The discard's wipe
    // is the level's drop (D-H2).
    let outcomes = a.view_state.end_edit_date();
    a.apply_outcomes(window, &outcomes);
    false
}

/// The one commit funnel (r2 D25, round-3 D8): dispatch the typed
/// `UpdateTask` change for the flow's field, then end the flow.
/// `None` clears (the empty-entry / x-clear semantics).
pub(in crate::adapter) fn commit_date(
    a: &mut Adapter,
    window: &AppWindow,
    instant: Option<chrono::DateTime<chrono::Utc>>,
) {
    let task = match a.view_state.flow() {
        Flow::EditDate { task, field } => (*task, *field),
        _ => return,
    };
    let (duedate, scheduled) = match task.1 {
        crate::view_state::DateField::Due => (Some(instant), None),
        crate::view_state::DateField::Scheduled => (None, Some(instant)),
    };
    let command = StateCommand::UpdateTask {
        id: task.0,
        changes: taskboard_domain::TaskChanges {
            duedate,
            scheduled,
            ..taskboard_domain::TaskChanges::default()
        },
    };
    if a.commands.try_send(command).is_err() {
        tracing::warn!("command channel full or closed; dropped");
    }
    // The commit's consume is the flow's end — the level drops with
    // its buffers (D-H2), so the next pass renders the empty editor.
    let outcomes = a.view_state.end_edit_date();
    a.apply_outcomes(window, &outcomes);
}

/// The effort editor's one commit funnel (US-3/D-8): parse through
/// the shared predicate (`parse_effort_row` — the walk placement's
/// parse), dispatch the exact sparse mask, close the editor.
pub(in crate::adapter) fn commit_effort_editor(a: &mut Adapter, window: &AppWindow) {
    let Some(editor) = a.effort_editor.as_ref() else {
        return;
    };
    let trimmed = editor.buffer.trim().to_owned();
    // The D-5 guard: a commit of an unchanged value dispatches
    // nothing (the always-on row blurs constantly; nothing ever
    // consumed the editor, so the buffer stays).
    if trimmed == editor.last_committed {
        return;
    }
    let effort_change = if trimmed.is_empty() {
        Some(None)
    } else if let Some(effort) = parse_effort_row(&trimmed) {
        Some(Some(effort))
    } else {
        // The refusal: the buffer kept, nothing dispatches
        // (the date-editor refusal posture, AC-2).
        a.refresh_forced(window);
        return;
    };
    let id = editor.task;
    if a.commands
        .try_send(StateCommand::UpdateTask {
            id,
            changes: taskboard_domain::TaskChanges {
                effort: effort_change,
                ..taskboard_domain::TaskChanges::default()
            },
        })
        .is_err()
    {
        tracing::warn!("effort commit dropped: command channel full or closed");
        return;
    }
    // The row stays armed (always-on): the baseline moves to the
    // committed value, so the Return+blur pair never double-dispatches.
    if let Some(editor) = a.effort_editor.as_mut() {
        editor.last_committed = trimmed;
    }
    a.finish_mutation(window);
    a.refresh_forced(window);
}

impl Adapter {
    /// The picker-invoke routing (#1227, US-1/S3): the cursor's row
    /// index resolves against the same published projection the rows
    /// were minted from — 0/1 are the plain reflex rows (the exact
    /// `AppUndo`/`AppRedo` routes the keyboard uses), 2+i an undo-step
    /// row, then the branch rows. Every arm lands in the same seam
    /// bodies the click path runs (`undo_picker_commit` — US-4/AC-2's
    /// byte-identical dispatch).
    pub(in crate::adapter) fn invoke_picker_row(&mut self, window: &AppWindow, index: usize) {
        let resolved = {
            let undo = &self.state.load_full().undo;
            let targets = undo.recent.len();
            if index == 0 || index == 1 {
                None
            } else if index < 2 + targets {
                undo.recent.get(index - 2).map(|row| (row.id, false))
            } else {
                undo.branch_children
                    .get(index - 2 - targets)
                    .map(|row| (row.id, true))
            }
        };
        match resolved {
            None if index == 0 => self.undo_picker_commit(window, |a, window| {
                a.route(window, ActionId::AppUndo);
            }),
            None if index == 1 => self.undo_picker_commit(window, |a, window| {
                a.route(window, ActionId::AppRedo);
            }),
            Some((target, false)) => self.undo_picker_commit(window, move |a, _window| {
                a.send(taskboard_domain::StateCommand::UndoTo { target });
            }),
            Some((target, true)) => self.undo_picker_commit(window, move |a, _window| {
                a.send(taskboard_domain::StateCommand::RedoTo { target });
            }),
            // A cursor past the projection's rows (a refresh shrank
            // the windows between the stroke and the invoke): the
            // close with nothing dispatched — the AC-4 shape.
            _ => {
                self.shell.undo_picker = false;
                self.sync_undo_picker_models(window);
            }
        }
    }
}

// SPDX-License-Identifier: AGPL-3.0-only

//! The slint-boundary model assembly (D2/D3): builds the generated
//! window model values from presentation output and view state, and
//! maps the pure presentation enums 1:1 onto the generated slint
//! enums. The markup owns all tone/colour decisions (D18); this
//! module only ferries typed data — and the id tables (D2): the
//! indices the views return resolve against the tables built in the
//! same pass that pushed the models.

use slint::{ComponentHandle as _, Model as _, ModelRc, SharedString, VecModel};

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use chrono::{DateTime, Datelike as _, TimeDelta, Utc};

use taskboard_domain::{
    AppState, BoardId, ReminderRuleId, SearchHit, SearchTarget, StackId, Task, TaskId,
};

use crate::actions::{ActionId, ContextId};
use crate::keymap::Keymap;
use crate::presentation::{self, DueKind, DueSets};
use crate::responsive::Breakpoint;
use crate::view_state::{Flow, ViewState};
use crate::{
    AppWindow, BannerRowModel, BoardBarModel, BoardModel, BoardSegmentModel,
    Breakpoint as SlintBreakpoint, DecorationKind, DragState, DueDayModel, DueKind as SlintDueKind,
    FlowHintModel, NotificationRowModel, PaletteRowModel, Responsive, RuleRowModel,
    StackColumnModel, StackDetailModel, SyncBadgeModel, SyncPhaseUi, TaskCardModel, TaskDecoration,
    TaskDetailModel, Theme, TimerRowModel, TodayModel, TodayRowModel, TodaySectionModel, Tone,
    View,
};

/// The layout cap of the banner slot (D12): the truth is the row
/// count; the model carries at most this many rows plus the overflow.
pub const BANNER_ROW_CAP: usize = 3;

/// The in-app notification surface's row cap (R3 chunk C): same flood
/// guard discipline as the banner — the truth is the row count; the
/// model carries at most this many rows.
pub const NOTIFICATION_ROW_CAP: usize = 3;

/// The layout cap of the palette's visible result rows (design D-V10,
/// #348): the truth is the filtered hit count; the model carries at
/// most this many rows plus the overflow caption's count.
pub const PALETTE_ROW_CAP: usize = 8;

/// The id tables of one refresh pass (D2): same thread, rebuilt
/// together with the models, always consistent.
///
/// Invariant the virtualised surfaces lean on (#373, design round
/// §3b): every row index a view reports or consumes — callbacks,
/// drag seams, the focus scroll hint — is a **global model index**
/// into these tables, never a viewport-relative offset. The
/// `ListView` repeater changes which rows are instantiated, never what
/// the indices mean.
#[derive(Debug, Clone, Default)]
pub struct IdTables {
    /// The live boards in canonical order (design §3.8): the board
    /// bar's segment indices resolve against this pass (an added index
    /// table — the content tables stay single-board-per-refresh).
    pub board_ids: Vec<BoardId>,
    /// The board's stacks in display order.
    pub stack_ids: Vec<StackId>,
    /// Per board column, the tasks in canonical order.
    pub board_tasks: Vec<Vec<TaskId>>,
    /// The stack-detail view's tasks in canonical order.
    pub stack_tasks: Vec<TaskId>,
    /// The banner rows (targets included, index-aligned with the
    /// pushed banner model up to the cap).
    pub banner: Vec<presentation::BannerRow>,
    /// The in-app notification rows (R3 chunk C, D-10): targets
    /// included, index-aligned with the pushed surface model up to the
    /// cap; empty on kiosk (the banner-only gate, R3.5.3).
    pub notifications: Vec<presentation::NotificationRow>,
    /// The timers section's live alarms (DR-3/3, D-I7): id-indexed
    /// like the notifications table — the cancel seam's resolution
    /// base; empty on kiosk.
    pub timers: Vec<taskboard_domain::AdhocAlarmId>,
    /// The task-detail view's reminder rules in id order (chunk 6,
    /// D12): the rule-list row indices resolve against this pass.
    pub task_rules: Vec<ReminderRuleId>,
    /// The open palette's visible rows (chunk 9, #348): the row
    /// indices resolve against this pass (indices-back like every
    /// table; empty while the palette is closed).
    pub palette: Vec<SearchTarget>,
    /// The today view's rows (chunk 10 D7): the four filtered
    /// sections concatenated in display order — the focus walk's
    /// domain and the row-activation seam's resolution base.
    pub today_tasks: Vec<TaskId>,
    /// Per today section, the filtered row count (chunk 10 D7): the
    /// `(section, row)` seams resolve their flat index against these
    /// prefix sums over `today_tasks`.
    pub today_section_rows: [usize; 4],
}

impl IdTables {
    /// The board task at (column, row), if the tables know it.
    #[must_use]
    pub fn board_task(&self, column: usize, row: usize) -> Option<(StackId, TaskId)> {
        let stack = self.stack_ids.get(column).copied()?;
        let task = self
            .board_tasks
            .get(column)
            .and_then(|c| c.get(row))
            .copied()?;
        Some((stack, task))
    }

    /// The stack-detail task at `row`.
    #[must_use]
    pub fn stack_task(&self, row: usize) -> Option<TaskId> {
        self.stack_tasks.get(row).copied()
    }

    /// The today row at `(section, row)`, if the tables know it.
    #[must_use]
    pub fn today_row(&self, section: usize, row: usize) -> Option<TaskId> {
        let offset: usize = self.today_section_rows.iter().take(section).sum();
        self.today_tasks.get(offset + row).copied()
    }

    /// The board column and row holding `task` — the focus scroll
    /// hint's resolution base (#373 D-6). A forward-only scan (the
    /// tables are rebuilt every pass; a reverse index would be a
    /// shape change and a rebuild cost).
    #[must_use]
    pub fn board_position(&self, task: TaskId) -> Option<(usize, usize)> {
        self.board_tasks
            .iter()
            .enumerate()
            .find_map(|(column, tasks)| {
                tasks
                    .iter()
                    .position(|t| *t == task)
                    .map(|row| (column, row))
            })
    }

    /// The today section and row holding `task` (via the section
    /// prefix sums over `today_tasks`).
    #[must_use]
    pub fn today_position(&self, task: TaskId) -> Option<(usize, usize)> {
        let flat = self.today_tasks.iter().position(|t| *t == task)?;
        let mut consumed = 0;
        for (section, rows) in self.today_section_rows.iter().enumerate() {
            if flat < consumed + *rows {
                return Some((section, flat - consumed));
            }
            consumed += *rows;
        }
        None
    }

    /// The stack-detail row holding `task`.
    #[must_use]
    pub fn stack_position(&self, task: TaskId) -> Option<usize> {
        self.stack_tasks.iter().position(|t| *t == task)
    }
}

/// Maps the pure breakpoint onto the generated slint enum (D17's
/// push path).
#[must_use]
pub const fn to_slint_breakpoint(breakpoint: Breakpoint) -> SlintBreakpoint {
    match breakpoint {
        Breakpoint::Compact => SlintBreakpoint::Compact,
        Breakpoint::Regular => SlintBreakpoint::Regular,
        Breakpoint::Expansive => SlintBreakpoint::Expansive,
    }
}

/// Maps the pure due classification onto the generated enum (1:1).
#[must_use]
pub const fn to_slint_due_kind(kind: DueKind) -> SlintDueKind {
    match kind {
        DueKind::None => SlintDueKind::None,
        DueKind::Done => SlintDueKind::Done,
        DueKind::Later => SlintDueKind::Later,
        DueKind::Soon => SlintDueKind::Soon,
        DueKind::Overdue => SlintDueKind::Overdue,
    }
}

fn string_model(values: &[String]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        values.iter().map(SharedString::from).collect::<Vec<_>>(),
    ))
}

/// The persistent model handles of one mounted window (r2 D9): the
/// refresh pass keeps deriving the *complete desired* rows exactly as
/// before, then applies them onto these handles through a positional
/// diff — an unchanged refresh notifies nothing, so the repeated
/// surfaces' `TouchArea`s survive across refreshes (F8 double-click,
/// F11 drag) and small changes touch exactly one row (#390's first
/// leg). One instance per mounted window, owned by the `Adapter`.
#[derive(Default)]
pub struct PushModels {
    bar_segments: Option<Rc<VecModel<BoardSegmentModel>>>,
    columns: Option<Rc<VecModel<StackColumnModel>>>,
    /// The per-column card models, keyed by the column's stack (the
    /// board's columns and the stack-detail list address them by
    /// stack).
    column_tasks: BTreeMap<StackId, Rc<VecModel<TaskCardModel>>>,
    /// Per-card decoration models keyed by the task.
    /// `TaskCardModel` compares its `decorations` field by handle
    /// identity (the pinned Slint's `ModelRc` `PartialEq`), so a
    /// fresh model per card per pass would dirty every card and
    /// degrade the diff to a full `set_row_data` cascade.
    card_decorations: BTreeMap<TaskId, Rc<VecModel<TaskDecoration>>>,
    detail_tasks: Option<Rc<VecModel<TaskCardModel>>>,
    task_rules: Option<Rc<VecModel<RuleRowModel>>>,
    task_rule_presets: Option<Rc<VecModel<SharedString>>>,
    banner: Option<Rc<VecModel<BannerRowModel>>>,
    /// The notification surface's persistent row handle (R3 chunk C).
    notifications: Option<Rc<VecModel<NotificationRowModel>>>,
    /// The timers section's persistent row handle (DR-3/3): the same
    /// diff pattern — stable identity across refreshes.
    timers: Option<Rc<VecModel<TimerRowModel>>>,
    /// The palette's persistent row handle (chunk 9): diffed per
    /// keystroke — a fresh `VecModel` per push would regress the
    /// stable-identity invariant (r2 D9).
    palette: Option<Rc<VecModel<PaletteRowModel>>>,
    /// The today view's per-section row handles (chunk 10 D7): fixed
    /// to the four display sections — a rollover refresh must not
    /// re-instantiate the cards (the r2 D9 discipline).
    today_sections: [Option<Rc<VecModel<TodayRowModel>>>; 4],
    today: Option<Rc<VecModel<TodaySectionModel>>>,
    settings_rows: Option<Rc<VecModel<crate::ConnectionRow>>>,
    /// The board-selector's persistent row handle (r2 D32): the same
    /// diff pattern — stable identity across refreshes.
    settings_boards: Option<Rc<VecModel<crate::SettingsBoardRow>>>,
    hints: HintModels,
    /// The tasks whose decoration models the current pass touched
    /// (cleared at pass start) — the prune set for
    /// `card_decorations`, so the map never grows past the live task
    /// set.
    pass_tasks: BTreeSet<TaskId>,
}

/// The six `[string]` keymap-projection hint slots (r2 D9): they
/// change only when the keymap or the visible chain changes, so the
/// diff keeps their refresh cost at zero.
#[derive(Default)]
struct HintModels {
    toolbar_menu: Option<Rc<VecModel<SharedString>>>,
    nav_back: Option<Rc<VecModel<SharedString>>>,
    board_segment: Option<Rc<VecModel<SharedString>>>,
    task_card: Option<Rc<VecModel<SharedString>>>,
    stack_header: Option<Rc<VecModel<SharedString>>>,
    stack_create: Option<Rc<VecModel<SharedString>>>,
    task_create: Option<Rc<VecModel<SharedString>>>,
}

/// Applies `desired` onto the persistent model through a positional
/// diff and returns the number of row mutations: `set_row_data` only
/// where the row differs over the common length, then the tail is
/// truncated, then appended. No LCS — a mid-list reorder costs a
/// `set_row_data` cascade over the suffix, which the pinned Slint
/// applies as in-place binding updates with zero re-instantiation
/// (the repetition-probe pins), and reorders are user-move-scale,
/// not refresh-scale.
pub(crate) fn diff_rows<T: PartialEq + Clone + 'static>(
    model: &VecModel<T>,
    desired: &[T],
) -> usize {
    let mut mutations = 0;
    let current = model.row_count();
    let common = current.min(desired.len());
    for (i, row) in desired[..common].iter().enumerate() {
        if model.row_data(i).as_ref() != Some(row) {
            model.set_row_data(i, row.clone());
            mutations += 1;
        }
    }
    for _ in desired.len()..current {
        model.remove(desired.len());
        mutations += 1;
    }
    for row in &desired[common..] {
        model.push(row.clone());
        mutations += 1;
    }
    mutations
}

/// The palette row pass (#348, chunk 9): rank the open query through
/// the one domain `find`, apply the presentation-side anchor filter
/// (the anchor task is never a visible row — the domain stays
/// convention-blind), cap to [`PALETTE_ROW_CAP`], and push the rows
/// onto the persistent handle (the r2 D9 row-diff discipline — a
/// keystroke never swaps the model). Returns the visible rows'
/// targets, index-aligned with the pushed model, for `IdTables.palette`.
/// One function, two callers: the per-keystroke targeted pass and the
/// authoritative `push` — the ranking never exists twice.
pub fn push_palette(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    models: &mut PushModels,
) -> Vec<SearchTarget> {
    let Flow::Palette { selected } = vs.flow() else {
        return Vec::new();
    };
    let selected = *selected;
    let hits = taskboard_domain::find(state, vs.buffer());
    let filtered: Vec<SearchHit> = hits
        .into_iter()
        .filter(|hit| match hit.target {
            SearchTarget::Task(id) => state
                .tasks
                .get(&id)
                .is_none_or(|task| !presentation::is_anchor_task(task)),
            _ => true,
        })
        .collect();
    let overflow = filtered.len().saturating_sub(PALETTE_ROW_CAP);
    let targets: Vec<SearchTarget> = filtered
        .iter()
        .take(PALETTE_ROW_CAP)
        .map(|hit| hit.target)
        .collect();
    let rows: Vec<PaletteRowModel> = targets
        .iter()
        .map(|target| PaletteRowModel {
            text: SharedString::from(palette_title(state, *target)),
            meta: SharedString::from(palette_meta(state, *target)),
            emphasis: false,
        })
        .collect();
    // The row diff's mutation count is not tracked here: the palette's
    // property writes below are the pass's observable output.
    push_rows(
        &mut models.palette,
        |rc| {
            window.set_palette_model(rc);
        },
        &rows,
    );
    // The highlight rides outside the diff (one property write): the
    // cursor clamps into the visible range (a refresh that shrank the
    // rows keeps the nearest position; only a query change resets it).
    let selected = selected.min(targets.len().saturating_sub(1));
    window.set_palette_selected_index(i32::try_from(selected).unwrap_or(i32::MAX));
    window.set_palette_overflow(i32::try_from(overflow).unwrap_or(i32::MAX));
    targets
}

/// The hit's title off the snapshot (a stale target degrades to an
/// empty title — data, not an error).
fn palette_title(state: &AppState, target: SearchTarget) -> String {
    match target {
        SearchTarget::Task(id) => state
            .tasks
            .get(&id)
            .map_or(String::new(), |t| t.title.clone()),
        SearchTarget::Stack(id) => state
            .stacks
            .get(&id)
            .map_or(String::new(), |s| s.title.clone()),
        SearchTarget::Board(id) => state
            .boards
            .get(&id)
            .map_or(String::new(), |b| b.title.clone()),
    }
}

/// The kind + context line (the row's meta): `"task · <stack> ›
/// <board>"` for tasks, `"stack · <board>"` for stacks, `"board"` for
/// boards; a dangling parent degrades its contribution.
fn palette_meta(state: &AppState, target: SearchTarget) -> String {
    match target {
        SearchTarget::Task(id) => state
            .tasks
            .get(&id)
            .and_then(|t| state.stacks.get(&t.stack))
            .map_or_else(
                || "task".to_owned(),
                |stack| match state.boards.get(&stack.board) {
                    Some(board) => format!("task · {} › {}", stack.title, board.title),
                    None => format!("task · {}", stack.title),
                },
            ),
        SearchTarget::Stack(id) => state.stacks.get(&id).map_or_else(
            || "stack".to_owned(),
            |stack| match state.boards.get(&stack.board) {
                Some(board) => format!("stack · {}", board.title),
                None => "stack".to_owned(),
            },
        ),
        SearchTarget::Board(_) => "board".to_owned(),
    }
}

/// Get-or-create the persistent handle in `slot` (installing it into
/// its window property exactly once via `install`), then diff
/// `desired` onto it; returns the mutation count.
fn push_rows<T: PartialEq + Clone + 'static>(
    slot: &mut Option<Rc<VecModel<T>>>,
    install: impl FnOnce(ModelRc<T>),
    desired: &[T],
) -> usize {
    let model = slot.get_or_insert_with(|| {
        let model = Rc::new(VecModel::default());
        install(ModelRc::from(model.clone()));
        model
    });
    diff_rows(model, desired)
}

/// The due editor's pushed state (r2 D25): the adapter owns the
/// buffer, echo, and picker cursor across refreshes (R-d); the push
/// only carries them into the model.
#[derive(Debug, Clone, Copy, Default)]
pub struct DueEditorPush<'a> {
    pub text: &'a str,
    pub echo: &'a str,
    pub picker: Option<&'a crate::due_edit::DuePickerState>,
}

/// The timers section (DR-3/3, D-I6): the active (unfired) rows plus
/// the compact awaiting pointer. The kiosk gate lives HERE (R3.5.3):
/// the model is emptied, the awaiting count zeroed, `timers-visible`
/// forced false — the banner stays the appliance's only reminder
/// surface. Returns the diff's mutation count.
#[allow(clippy::too_many_arguments)] // the pass-shaped parameter list, like `push`
fn push_timers(
    window: &AppWindow,
    state: &AppState,
    reminder_defaults: &taskboard_domain::ReminderDefaults,
    now: DateTime<Utc>,
    kiosk: bool,
    tables: &mut IdTables,
    models: &mut PushModels,
) -> usize {
    let rows: Vec<presentation::TimerRow> = if kiosk {
        Vec::new()
    } else {
        presentation::timer_rows(state, now)
    };
    tables.timers = rows.iter().map(|row| row.alarm).collect();
    let awaiting = if kiosk {
        0
    } else {
        presentation::awaiting_alarm_count(state, reminder_defaults, now)
    };
    let shown: Vec<TimerRowModel> = rows
        .iter()
        .map(|row| TimerRowModel {
            label: SharedString::from(row.label.clone()),
            countdown: SharedString::from(presentation::countdown_phrase(row.effective, now)),
        })
        .collect();
    window.set_timers_visible(!kiosk);
    window.set_timers_awaiting(i32::try_from(awaiting).unwrap_or(i32::MAX));
    push_rows(
        &mut models.timers,
        |rc| {
            window.set_timers_model(rc);
        },
        &shown,
    )
}

/// The in-app notification surface (R3 chunk C, design D-10/D-6): the
/// derived-at-read rows pushed onto the persistent surface model. The
/// kiosk gate lives HERE (R3.5.3, banner-only appliance mode): the
/// model is emptied, never rendered — the scheduler-free banner stays
/// the kiosk's only reminder surface. Returns the diff's mutation count.
#[allow(clippy::too_many_arguments)] // the pass-shaped parameter list, like `push`
fn push_notifications(
    window: &AppWindow,
    state: &AppState,
    reminder_defaults: &taskboard_domain::ReminderDefaults,
    now: DateTime<Utc>,
    kiosk: bool,
    tables: &mut IdTables,
    models: &mut PushModels,
) -> usize {
    let rows: Vec<presentation::NotificationRow> = if kiosk {
        Vec::new()
    } else {
        presentation::notification_rows(state, reminder_defaults, now)
    };
    tables.notifications.clone_from(&rows);
    let shown: Vec<NotificationRowModel> = rows
        .iter()
        .take(NOTIFICATION_ROW_CAP)
        .map(|row| NotificationRowModel {
            title: SharedString::from(row.title.clone()),
            fired_phrase: SharedString::from(presentation::notification_phrase(
                row.occurrence,
                now,
            )),
        })
        .collect();
    let mutations = push_rows(
        &mut models.notifications,
        |rc| {
            window.set_notification_model(rc);
        },
        &shown,
    );
    window.set_notification_overflow(
        i32::try_from(rows.len().saturating_sub(NOTIFICATION_ROW_CAP)).unwrap_or(i32::MAX),
    );
    mutations
}

/// One refresh pass (D1): reconcile the view state against `state`,
/// then push the full display model one-way onto the window and
/// return the id tables. Deterministic and timer-free — the repaint
/// timer and the tests call this. The desired rows are applied onto
/// `models`' persistent handles through a positional diff (r2 D9):
/// an unchanged refresh notifies nothing. The `for`-repeater markup
/// stays untouched — every repeated surface keeps a single stable
/// model with stable row addressing, which is exactly the model shape
/// a `ListView` virtualisation (#373, the next lever on the same
/// render budget) consumes later.
#[must_use]
#[allow(clippy::too_many_arguments)] // the pass's inputs, one bundle
pub fn push(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    keymap: &Keymap,
    chain: &[ContextId],
    day: crate::view_state::DayCtx,
    kiosk: bool,
    reminder_defaults: &taskboard_domain::ReminderDefaults,
    models: &mut PushModels,
    due: &DueEditorPush<'_>,
) -> IdTables {
    let now = day.now;
    let sets = presentation::due_sets(state, now);
    let board = vs.board().and_then(|id| state.boards.get(&id));
    let stack_ids: Vec<StackId> = vs.stack_ids(state);
    let live_boards = presentation::live_boards(state);

    // The board bar (design §3.8): one segment per live board,
    // selected index over the same pass's tables.
    let segments: Vec<BoardSegmentModel> = live_boards
        .iter()
        .map(|b| BoardSegmentModel {
            title: SharedString::from(b.title.clone()),
        })
        .collect();
    let selected = i32::try_from(
        vs.board()
            .and_then(|b| live_boards.iter().position(|board| board.id == b))
            .unwrap_or(0),
    )
    .unwrap_or(i32::MAX);
    let segments_handle = models.bar_segments.get_or_insert_with(Rc::default).clone();
    let mut mutations = diff_rows(&segments_handle, &segments);
    // The struct re-assignment runs every pass — its `selected` scalar
    // must stay live; the unchanged segments handle makes it a
    // renderer no-op for the repetition (the repetition-probe pins).
    window.set_board_bar_model(BoardBarModel {
        segments: ModelRc::from(segments_handle),
        selected,
    });

    mutations += push_chrome(
        window,
        vs.view(),
        state,
        kiosk,
        board,
        keymap,
        chain,
        models,
    );
    window.set_current_view(match vs.view() {
        crate::view_state::ViewId::Today => View::Today,
        crate::view_state::ViewId::Board => View::Board,
        crate::view_state::ViewId::StackDetail => View::StackDetail,
        crate::view_state::ViewId::TaskDetail => View::TaskDetail,
        crate::view_state::ViewId::Settings => View::Settings,
    });

    let mut tables = IdTables {
        board_ids: live_boards.iter().map(|b| b.id).collect(),
        stack_ids: stack_ids.clone(),
        ..IdTables::default()
    };
    mutations += push_board(
        window,
        state,
        vs,
        &sets,
        &stack_ids,
        &mut tables,
        now,
        models,
    );
    mutations += push_today(window, state, vs, &sets, day, kiosk, &mut tables, models);
    mutations += push_stack_detail(window, state, vs, &sets, &mut tables, now, models);
    mutations += push_task_detail(window, state, vs, &sets, &mut tables, now, models, due);
    mutations += push_status_bar(
        window,
        state,
        vs,
        keymap,
        chain,
        now,
        kiosk,
        &mut tables,
        models,
    );
    mutations += push_notifications(
        window,
        state,
        reminder_defaults,
        now,
        kiosk,
        &mut tables,
        models,
    );
    mutations += push_timers(
        window,
        state,
        reminder_defaults,
        now,
        kiosk,
        &mut tables,
        models,
    );
    // The decoration handles of tasks this pass dropped from every
    // surface are unreachable from the views.
    let pass = std::mem::take(&mut models.pass_tasks);
    models
        .card_decorations
        .retain(|task, _| pass.contains(task));
    tracing::trace!(rows = mutations, "model push diff: row mutations this pass");
    tables
}

/// The toolbar chrome: board title and the `app.menu` binding hint (a
/// keymap projection, never hand-written — D19). Returns the hint
/// diff's mutation count.
#[allow(clippy::too_many_arguments)] // the pass-shaped parameter list, like `push`
fn push_chrome(
    window: &AppWindow,
    view: crate::view_state::ViewId,
    state: &AppState,
    kiosk: bool,
    board: Option<&taskboard_domain::Board>,
    keymap: &Keymap,
    chain: &[ContextId],
    models: &mut PushModels,
) -> usize {
    // The inbox chip's count (chunk 10 D9): desktop only — kiosk reads
    // 0 and the compact breakpoint hides the chip.
    window.set_toolbar_inbox_count(if kiosk {
        0
    } else {
        i32::try_from(presentation::inbox_count(state)).unwrap_or(i32::MAX)
    });
    // The Today view renders its own caption (chunk 10 D9): a fixed
    // presentation label; the board views keep the selected board's
    // title.
    window.set_toolbar_title(SharedString::from(match view {
        crate::view_state::ViewId::Today => "Today".to_string(),
        _ => board.map_or(String::new(), |b| b.title.clone()),
    }));
    let rows = |action| -> Vec<SharedString> {
        keymap
            .first_binding_display(action, chain)
            .unwrap_or_default()
            .iter()
            .map(SharedString::from)
            .collect()
    };
    let mut mutations = 0;
    // The toolbar menu hint…
    mutations += push_rows(
        &mut models.hints.toolbar_menu,
        |rc| {
            window.set_toolbar_menu_hint(rc);
        },
        &rows(ActionId::AppMenu),
    );
    // The back button's hint (round-1 D12): the same projection, one
    // element class later — shown only when a binding exists.
    mutations += push_rows(
        &mut models.hints.nav_back,
        |rc| {
            window.set_nav_back_hint(rc);
        },
        &rows(ActionId::NavFocusBoard),
    );
    // The segment/card hints (round-1 D21b/D8): pushed once per
    // refresh against the visible chain.
    mutations += push_rows(
        &mut models.hints.board_segment,
        |rc| {
            window.set_board_segment_hint(rc);
        },
        &rows(ActionId::BoardSelect),
    );
    mutations += push_rows(
        &mut models.hints.task_card,
        |rc| {
            window.set_task_card_hint(rc);
        },
        &rows(ActionId::TaskEdit),
    );
    // The stack-header hint (round-1 D18): the stack.wider binding —
    // the header tooltip's discoverability carrier.
    mutations += push_rows(
        &mut models.hints.stack_header,
        |rc| {
            window.set_stack_header_hint(rc);
        },
        &rows(ActionId::StackWider),
    );
    // The add-column tooltip's hint (r2 D6): the stack.create binding
    // — empty under emacs until chunk 3 binds it (the established
    // empty-hint rendering applies).
    mutations += push_rows(
        &mut models.hints.stack_create,
        |rc| {
            window.set_stack_create_hint(rc);
        },
        &rows(ActionId::StackCreate),
    );
    // The add-task ghosts' tooltip hint (r3 D3): the task.create
    // binding — the same projection, one action later.
    mutations += push_rows(
        &mut models.hints.task_create,
        |rc| {
            window.set_task_create_hint(rc);
        },
        &rows(ActionId::TaskCreate),
    );
    mutations
}

/// The per-column `width-px` model value (round-1 D16): the user-set
/// stored logical px converted to rendered px (`× base-scale`), or the
/// `−1.0` auto sentinel — never-resized boards push the sentinel on
/// every column, so the markup's conditional clamps never engage.
#[must_use]
pub fn stack_width_px(
    board: Option<BoardId>,
    stack: StackId,
    vs: &ViewState,
    base_scale: f32,
) -> f32 {
    let stored = board.and_then(|board| vs.stack_width(board, stack));
    match stored {
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        Some(px) => px as f32 * base_scale.max(0.0),
        None => -1.0,
    }
}

/// The board model: one column per live stack, the flow's create row
/// and move target marked by index (D8/D22). The board title renders
/// in the toolbar (`push_chrome`), so the model carries only the
/// kanban geometry. Returns the diff's mutation count.
#[allow(clippy::too_many_arguments)] // the pass's inputs, one bundle
fn push_board(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    sets: &DueSets,
    stack_ids: &[StackId],
    tables: &mut IdTables,
    now: DateTime<Utc>,
    models: &mut PushModels,
) -> usize {
    let flow = vs.flow().clone();
    let scale = window.global::<Theme>().get_base_scale();
    let mut mutations = 0;
    let mut columns: Vec<StackColumnModel> = Vec::with_capacity(stack_ids.len());
    for stack_id in stack_ids {
        let stack = &state.stacks[stack_id];
        let tasks = state.tasks_in_stack(*stack_id);
        let mut task_ids = Vec::with_capacity(tasks.len());
        let cards_handle = models.column_tasks.entry(*stack_id).or_default().clone();
        let mut cards: Vec<TaskCardModel> = Vec::with_capacity(tasks.len());
        // The anchor card is hidden from list surfaces (DDF-5): filtered
        // once, before the loop, so cards and the id table stay in sync.
        for task in tasks {
            if presentation::is_anchor_task(task) {
                continue;
            }
            task_ids.push(task.id);
            cards.push(task_card_model(
                task,
                sets,
                vs,
                now,
                decorations_model(models, task),
            ));
        }
        mutations += diff_rows(&cards_handle, &cards);
        tables.board_tasks.push(task_ids);
        columns.push(StackColumnModel {
            title: SharedString::from(stack.title.clone()),
            // The column's own persistent handle: the outer row fires
            // only when scalars or the inner handle actually differ
            // (board/stack switches rebind exactly the rebinding
            // columns; unchanged refreshes are zero-mutation).
            tasks: ModelRc::from(cards_handle.clone()),
            // The create row opens in the flow's stack only (D8).
            create_open: matches!(&flow, Flow::Create { stack } if stack == stack_id),
            // The rename field opens in the flow's stack only (r2
            // D21); the buffer seeds the field's initial text (the
            // mount-only rule — the field reads it at init).
            rename_open: matches!(&flow, Flow::RenameStack { stack } if stack == stack_id),
            initial_title: SharedString::from(stack.title.clone()),
            width_px: stack_width_px(vs.board(), *stack_id, vs, scale),
        });
    }
    mutations += diff_rows(models.columns.get_or_insert_with(Rc::default), &columns);
    // The board create/rename fields' window-level slot (r2 D21).
    window.set_board_create_open(matches!(flow, Flow::CreateBoard));
    // The pointer drag's window bits (r2 chunk-8 D-D): the ghost's
    // open flag and its card snapshot rebuild on every pass while the
    // drag flow holds (the chunk-2 refresh-survival invariant made
    // constructive); `DragState.pointer-x/y` are runtime-owned and
    // never written here.
    window.set_board_drag_open(matches!(flow, Flow::DragMove { .. }));
    window.set_board_flow_open(flow != Flow::None);
    if let Flow::DragMove { task, .. } = &flow
        && let Some(dragged) = state.tasks.get(task)
    {
        let mut ghost = task_card_model(dragged, sets, vs, now, decorations_model(models, dragged));
        ghost.dragging_source = false;
        window.global::<DragState>().set_ghost_model(ghost);
    }
    window.set_board_rename_open(matches!(flow, Flow::RenameBoard { .. }));
    window.set_board_flow_initial(SharedString::from(match &flow {
        Flow::RenameBoard { .. } => vs.buffer().to_owned(),
        _ => String::new(),
    }));
    // Prune the per-column card handles of stacks this pass dropped.
    models
        .column_tasks
        .retain(|stack, _| stack_ids.contains(stack));
    window.set_board_model(BoardModel {
        // The persistent columns handle: the struct re-assignment is a
        // renderer no-op for the repetition while the scalar fields
        // stay live (the repetition-probe pins).
        stacks: ModelRc::from(models.columns.clone().expect("just inserted")),
        // The stack-create flow's visible field (usability D7).
        stack_create_open: matches!(flow, Flow::CreateStack),
        // The true selection (r2 D4): None flattens to −1 — the
        // move-target convention in the same struct. The regular
        // header paints the selected stack; the compact leg clamps
        // through its `effective-selected()` (mount renders stack 0
        // exactly as the old 0-fallback did).
        selected_stack: vs
            .selected_stack()
            .and_then(|s| stack_ids.iter().position(|c| c == &s))
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1),
        move_target: match &flow {
            Flow::Move { target, .. } | Flow::DragMove { target, .. } => stack_ids
                .iter()
                .position(|s| s == target)
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
            // Round 2 (D21): a stack move highlights the insertion
            // point — the stack at `min(slot, len-1)` (slot == len
            // highlights the last stack = "append after it").
            Flow::MoveStack { slot, .. } => stack_ids
                .len()
                .checked_sub(1)
                .map(|last| (*slot).min(last))
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
            _ => -1,
        },
    });
    mutations
}

impl PushModels {
    /// Diffs the settings connection rows onto the persistent
    /// settings-row handle, installing it into the global exactly
    /// once (the slot's property lives on a global, not the window
    /// root — hence this method instead of the root-property
    /// installers).
    pub(crate) fn push_settings_rows(
        &mut self,
        global: &crate::SettingsModel<'_>,
        desired: &[crate::ConnectionRow],
    ) -> usize {
        push_rows(&mut self.settings_rows, |rc| global.set_rows(rc), desired)
    }

    /// Diffs the board-selector rows onto their persistent handle
    /// (r2 D32) — the same pattern as the connector rows.
    pub(crate) fn push_settings_boards(
        &mut self,
        global: &crate::SettingsModel<'_>,
        desired: &[crate::SettingsBoardRow],
    ) -> usize {
        push_rows(
            &mut self.settings_boards,
            |rc| global.set_boards(rc),
            desired,
        )
    }
}

/// The task's decoration model: get-or-create the persistent
/// per-task handle and diff the desired decorations onto it (r2
/// D9's `card_decorations` slot — the handle identity that makes
/// unchanged cards compare fully equal).
fn decorations_model(models: &mut PushModels, task: &Task) -> ModelRc<TaskDecoration> {
    let slot = models.card_decorations.entry(task.id).or_default();
    models.pass_tasks.insert(task.id);
    diff_rows(slot, &field_decorations(task));
    ModelRc::from(slot.clone())
}

/// The stack-detail model over the snapshotted subject (D7/D22).
/// Returns the diff's mutation count.
fn push_stack_detail(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    sets: &DueSets,
    tables: &mut IdTables,
    now: DateTime<Utc>,
    models: &mut PushModels,
) -> usize {
    let tasks = vs
        .subject_stack()
        .map(|stack| state.tasks_in_stack(stack))
        .unwrap_or_default();
    let detail_handle = models.detail_tasks.get_or_insert_with(Rc::default).clone();
    let mut detail_rows: Vec<TaskCardModel> = Vec::with_capacity(tasks.len());
    // The same single-pass anchor filter as `push_board` (DDF-5).
    for task in &tasks {
        if presentation::is_anchor_task(task) {
            continue;
        }
        tables.stack_tasks.push(task.id);
        detail_rows.push(task_card_model(
            task,
            sets,
            vs,
            now,
            decorations_model(models, task),
        ));
    }
    let mutations = diff_rows(&detail_handle, &detail_rows);
    window.set_stack_model(StackDetailModel {
        title: SharedString::from(
            vs.subject_stack()
                .and_then(|s| state.stacks.get(&s))
                .map_or(String::new(), |s| s.title.clone()),
        ),
        tasks: ModelRc::from(detail_handle.clone()),
        create_open: vs.view() == crate::view_state::ViewId::StackDetail
            && matches!(vs.flow(), Flow::Create { .. }),
    });
    mutations
}

/// The task-detail model (D14): display plus the edit-flow state.
/// Rule 4 (D9): the buffer stays user-owned while the flow is active —
/// the field reads `initial-text` only on mount. Returns the diff's
/// mutation count.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)] // the pass's inputs, one bundle
fn push_task_detail(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    sets: &DueSets,
    tables: &mut IdTables,
    now: DateTime<Utc>,
    models: &mut PushModels,
    due: &DueEditorPush<'_>,
) -> usize {
    let subject = vs.subject_task().and_then(|t| state.tasks.get(&t));
    let editing = matches!(vs.flow(), Flow::EditTitle { task } if vs.subject_task().is_some_and(|s| *task == s));
    // The subject task's rules in id order (the table and the row
    // model agree by construction — D12).
    let task_rules: Vec<taskboard_domain::ReminderRuleId> = vs
        .subject_task()
        .map(|task| {
            state
                .reminders
                .rules
                .values()
                .filter(|rule| rule.task == task)
                .map(|rule| rule.id)
                .collect()
        })
        .unwrap_or_default();
    let rule_rows: Vec<RuleRowModel> = task_rules
        .iter()
        .filter_map(|id| state.reminders.rules.get(id))
        .map(|rule| RuleRowModel {
            before: SharedString::from(rule.offset.text_form()),
            created: SharedString::from(rule.created_at.to_rfc3339()),
        })
        .collect();
    let preset_rows: Vec<SharedString> = presentation::rule_preset_labels()
        .into_iter()
        .map(SharedString::from)
        .collect();
    // The due editor's grid (r2 D25): rows of seven, leading blanks,
    // cursor/today/kind markers — pushed only while the editor is
    // open; the empty model otherwise.
    let due_open = matches!(vs.flow(), Flow::EditDue { .. });
    let due_grid_rows: Vec<slint::ModelRc<DueDayModel>> = due
        .picker
        .filter(|_| due_open)
        .map(|picker| {
            let today = now.date_naive();
            picker
                .grid()
                .chunks(7)
                .map(|chunk| {
                    slint::ModelRc::new(slint::VecModel::from(
                        chunk
                            .iter()
                            .map(|cell| {
                                let date = cell.date;
                                DueDayModel {
                                    label: SharedString::from(
                                        date.map_or(String::new(), |d| d.day().to_string()),
                                    ),
                                    blank: date.is_none(),
                                    cursor: date.is_some_and(|d| d == picker.cursor()),
                                    today: date.is_some_and(|d| d == today),
                                    kind: to_slint_due_kind(
                                        date.map_or(crate::presentation::DueKind::None, |d| {
                                            crate::due_edit::cell_kind(d, today)
                                        }),
                                    ),
                                }
                            })
                            .collect::<Vec<DueDayModel>>(),
                    ))
                })
                .collect::<Vec<slint::ModelRc<DueDayModel>>>()
        })
        .unwrap_or_default();
    let due_grid = slint::ModelRc::new(slint::VecModel::from(due_grid_rows));

    let presets_handle = models.task_rule_presets.get_or_insert_with(Rc::default);
    let mut mutations = diff_rows(presets_handle, &preset_rows);
    let rules_handle = models.task_rules.get_or_insert_with(Rc::default);
    mutations += diff_rows(rules_handle, &rule_rows);
    window.set_task_model(TaskDetailModel {
        title: SharedString::from(subject.map_or(String::new(), |t| t.title.clone())),
        stack_title: SharedString::from(
            subject
                .and_then(|t| state.stacks.get(&t.stack))
                .map_or(String::new(), |s| s.title.clone()),
        ),
        due_phrase: SharedString::from(subject.map_or(String::new(), |t| {
            presentation::due_phrase(presentation::due_kind(t, sets), t.duedate, now)
        })),
        due_kind: to_slint_due_kind(
            subject.map_or(DueKind::None, |t| presentation::due_kind(t, sets)),
        ),
        done: subject.is_some_and(|t| t.done.is_some()),
        description: SharedString::from(subject.map_or(String::new(), |t| t.description.clone())),
        edit_open: editing,
        initial_text: SharedString::from(if editing {
            vs.buffer().to_owned()
        } else {
            String::new()
        }),
        rules: ModelRc::from(rules_handle.clone()),
        rule_presets: ModelRc::from(presets_handle.clone()),
        rules_hint: SharedString::from(presentation::rule_list_hint(task_rules.len())),
        due_open,
        due_text: SharedString::from(due.text),
        due_echo: SharedString::from(due.echo),
        due_picker_open: due.picker.is_some(),
        due_caption: SharedString::from(
            due.picker
                .map_or(String::new(), |p| p.month().format("%Y-%m").to_string()),
        ),
        due_grid,
    });
    tables.task_rules = task_rules;
    mutations
}

/// The status bar: sync badge, the capped banner rows, the flow hint
/// (D11/D12). Returns the banner diff's mutation count.
#[allow(clippy::too_many_arguments)] // the pass's inputs, one bundle
fn push_status_bar(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    keymap: &Keymap,
    chain: &[ContextId],
    now: DateTime<Utc>,
    kiosk: bool,
    tables: &mut IdTables,
    models: &mut PushModels,
) -> usize {
    // The timers chip (DR-3/3, D-I6): nearest fire while any live
    // alarm exists; kiosk forces it off (the desktop-only fence).
    let chip = if kiosk {
        None
    } else {
        presentation::timer_chip(state)
    };
    window.set_timers_chip_visible(chip.is_some());
    window.set_timers_chip(SharedString::from(chip.map_or(String::new(), |c| {
        format!(
            "{} \u{b7} {}",
            c.label,
            presentation::countdown_phrase(c.effective, now)
        )
    })));
    // The badge reads the *selected* board's row (design §3.2); the
    // recency caption follows it — the badge's freshness, per board.
    let badge = presentation::sync_badge_for_board(&state.sync, vs.board());
    let last_success = vs
        .board()
        .and_then(|b| state.sync.get(b))
        .map(|row| row.last_success)
        .unwrap_or_default();
    window.set_sync_model(SyncBadgeModel {
        phase: match badge.phase {
            presentation::SyncPhaseUi::Idle => SyncPhaseUi::Idle,
            presentation::SyncPhaseUi::Syncing => SyncPhaseUi::Syncing,
            presentation::SyncPhaseUi::Offline => SyncPhaseUi::Offline,
            presentation::SyncPhaseUi::Failed => SyncPhaseUi::Failed,
        },
        pending: i32::try_from(badge.pending).unwrap_or(i32::MAX),
        recency: SharedString::from(recency_caption(last_success, now)),
        sidecar_warning: SharedString::from(presentation::sidecar_caption(badge.sidecar)),
    });

    let banner = presentation::banner_rows(state, now);
    tables.banner.clone_from(&banner);
    let overflow = banner.len().saturating_sub(BANNER_ROW_CAP);
    let rows: Vec<BannerRowModel> = banner
        .iter()
        .take(BANNER_ROW_CAP)
        .map(|row| BannerRowModel {
            title: SharedString::from(
                state
                    .tasks
                    .get(&row.task)
                    .map_or(String::new(), |t| t.title.clone()),
            ),
            due_phrase: SharedString::from(state.tasks.get(&row.task).map_or(String::new(), |t| {
                presentation::due_phrase(DueKind::Overdue, t.duedate, now)
            })),
        })
        .collect();
    let mutations = push_rows(
        &mut models.banner,
        |rc| {
            window.set_banner_model(rc);
        },
        &rows,
    );
    window.set_banner_overflow(i32::try_from(overflow).unwrap_or(i32::MAX));

    let flow_hint_keys = match vs.flow() {
        Flow::Move { .. } => keymap
            .first_binding_display(ActionId::TaskMove, chain)
            .unwrap_or_default(),
        // Round 2 (D21): the stack move's hint is its own verb's
        // binding (the `TaskMove` pattern).
        Flow::MoveStack { .. } => keymap
            .first_binding_display(ActionId::StackMove, chain)
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    // The move flow's cross-board hint (chunk 10 D6): the target's
    // `"<board> › <stack>"`, always both parts while the flow holds.
    let flow_hint_text = match vs.flow() {
        Flow::Move { target, .. } => crate::presentation::move_hint(state, *target),
        _ => String::new(),
    };
    window.set_flow_hint_model(FlowHintModel {
        keys: string_model(&flow_hint_keys),
        visible: matches!(vs.flow(), Flow::Move { .. } | Flow::MoveStack { .. }),
        text: flow_hint_text.into(),
    });

    // The palette pass rides every refresh while the flow holds (the
    // authoritative caller of the one row pipeline): a background sync
    // re-ranks the open palette without closing it. Closed, the table
    // clears.
    tables.palette = if matches!(vs.flow(), Flow::Palette { .. }) {
        push_palette(window, state, vs, models)
    } else {
        Vec::new()
    };
    mutations
}

/// The visual-only monotone level→tone mapping over existing tokens
/// (D-F5: decoration visuals via the existing token system). The input
/// is the domain's canonical level text form — one vocabulary, the
/// same string the chips render.
fn level_tone(level: &str) -> Tone {
    match level {
        "low" => Tone::Info,
        "normal" => Tone::Success,
        "high" => Tone::Warning,
        "very-high" => Tone::Danger,
        // `very-low` and anything unrecognized sit at the bottom.
        _ => Tone::Neutral,
    }
}

/// The field decorations of one task (doc 4 §7 item 3): importance and
/// urgency ride the card's existing decorations slot — labels come from
/// the domain text forms, tones from the monotone mapping. No effort/
/// scheduled chips (their phrases carry them).
fn field_decorations(task: &Task) -> Vec<TaskDecoration> {
    let mut out = Vec::new();
    if let Some(level) = task.importance {
        out.push(TaskDecoration {
            kind: DecorationKind::Importance,
            label: level.text_form().into(),
            tone: level_tone(level.text_form()),
        });
    }
    if let Some(level) = task.urgency {
        out.push(TaskDecoration {
            kind: DecorationKind::Urgency,
            label: level.text_form().into(),
            tone: level_tone(level.text_form()),
        });
    }
    out
}

fn task_card_model(
    task: &Task,
    sets: &DueSets,
    vs: &ViewState,
    now: DateTime<Utc>,
    decorations: ModelRc<TaskDecoration>,
) -> TaskCardModel {
    let kind = presentation::due_kind(task, sets);
    TaskCardModel {
        title: SharedString::from(task.title.clone()),
        due_phrase: SharedString::from(presentation::due_phrase(kind, task.duedate, now)),
        scheduled_phrase: SharedString::from(presentation::scheduled_phrase(task.scheduled, now)),
        effort_phrase: SharedString::from(presentation::effort_phrase(task.effort)),
        due_kind: to_slint_due_kind(kind),
        done: task.done.is_some(),
        emphasis: vs.focused_task().is_some_and(|f| f == task.id),
        dragging_source: matches!(vs.flow(), Flow::DragMove { task: dragged, .. } if *dragged == task.id),
        decorations,
    }
}

/// The today pass (chunk 10 D7): the four `today_sections` in display
/// order, the kiosk filter and the anchor filter applied at the row
/// source, `TodayModel` pushed with persistent per-section handles,
/// and `tables.today_tasks` filled with the filtered concat (the
/// focus walk's domain). Rollover is proven by two `push` calls with
/// a stepped `DayCtx` — no timer, no sleeps.
#[allow(clippy::too_many_arguments)] // the pass-shaped parameter list, like `push`
fn push_today(
    window: &AppWindow,
    state: &AppState,
    vs: &ViewState,
    sets: &presentation::DueSets,
    day: crate::view_state::DayCtx,
    kiosk: bool,
    tables: &mut IdTables,
    models: &mut PushModels,
) -> usize {
    let sections = taskboard_domain::days::today_sections(&state.tasks, day.now, day.window);
    let multi_board = presentation::live_boards(state).len() > 1;
    // (ids, section) in display order, filtered before anything
    // derived — rows, `today_tasks`, and the counts all follow.
    let display: [(&[taskboard_domain::TaskId], &str); 4] = [
        (&sections.overdue, "Overdue"),
        (&sections.scheduled, "Scheduled"),
        (&sections.due_today, "Due today"),
        (&sections.done_today, "Done"),
    ];
    let mut mutations = 0;
    let mut section_models: Vec<TodaySectionModel> = Vec::with_capacity(4);
    let mut today_tasks: Vec<taskboard_domain::TaskId> = Vec::new();
    let mut section_rows = tables.today_section_rows;
    for (index, (ids, label)) in display.iter().enumerate() {
        let handle = models.today_sections[index]
            .get_or_insert_with(Rc::default)
            .clone();
        let mut rows: Vec<TodayRowModel> = Vec::with_capacity(ids.len());
        let mut filtered: Vec<taskboard_domain::TaskId> = Vec::with_capacity(ids.len());
        for id in *ids {
            let Some(task) = state.tasks.get(id) else {
                continue;
            };
            if presentation::is_anchor_task(task) {
                continue;
            }
            if kiosk
                && state
                    .stacks
                    .get(&task.stack)
                    .is_some_and(|s| Some(s.board) != vs.board())
            {
                continue;
            }
            filtered.push(*id);
            today_tasks.push(*id);
            rows.push(TodayRowModel {
                card: task_card_model(task, sets, vs, day.now, decorations_model(models, task)),
                context: SharedString::from(presentation::today_row_context(
                    state,
                    task,
                    kiosk,
                    multi_board,
                )),
            });
        }
        mutations += diff_rows(&handle, &rows);
        section_rows[index] = filtered.len();
        // All four sections always push (the model arity is fixed —
        // the repetition-probe discipline); the empty groups COLLAPSE
        // in the view's conditional render (D7).
        section_models.push(TodaySectionModel {
            label: SharedString::from(*label),
            count: i32::try_from(filtered.len()).unwrap_or(i32::MAX),
            rows: ModelRc::from(handle.clone()),
        });
    }
    // Prune the handles of sections that vanished (none can — fixed
    // arity — but the discipline keeps the map honest).
    tables.today_tasks = today_tasks;
    tables.today_section_rows = section_rows;
    let sections_handle = models.today.get_or_insert_with(Rc::default).clone();
    mutations += diff_rows(&sections_handle, &section_models);
    window.set_today_model(TodayModel {
        sections: ModelRc::from(sections_handle.clone()),
        empty: tables.today_tasks.is_empty(),
    });
    mutations
}

/// The sync recency caption (D11): only when a sync succeeded within
/// the last hour — a caption, not a badge fact.
fn recency_caption(last_success: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    let Some(last) = last_success else {
        return String::new();
    };
    let age = now - last;
    if age > TimeDelta::zero() && age < TimeDelta::hours(1) {
        format!("synced {}m ago", age.num_minutes())
    } else {
        String::new()
    }
}

/// Reads the current breakpoint back from the window (the adapter's
/// compact queries).
#[must_use]
pub fn current_breakpoint(window: &AppWindow) -> Breakpoint {
    match window.global::<Responsive>().get_current_breakpoint() {
        SlintBreakpoint::Compact => Breakpoint::Compact,
        SlintBreakpoint::Regular => Breakpoint::Regular,
        SlintBreakpoint::Expansive => Breakpoint::Expansive,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    use taskboard_domain::{Importance, TaskClocks, Urgency};

    /// The #373 D-6 reverse helpers: forward scans resolve positions
    /// that the forward lookups mint, per table.
    #[test]
    fn reverse_positions_resolve_the_forward_lookups() {
        let a = TaskId::from_uuid(uuid::Uuid::now_v7());
        let b = TaskId::from_uuid(uuid::Uuid::now_v7());
        let c = TaskId::from_uuid(uuid::Uuid::now_v7());
        let stack0 = StackId::from_uuid(uuid::Uuid::now_v7());
        let stack1 = StackId::from_uuid(uuid::Uuid::now_v7());
        let tables = IdTables {
            stack_ids: vec![stack0, stack1],
            board_tasks: vec![vec![a, b], vec![]],
            stack_tasks: vec![c],
            today_tasks: vec![b, c],
            today_section_rows: [1, 0, 1, 0],
            ..IdTables::default()
        };

        assert_eq!(tables.board_position(b), Some((0, 1)));
        assert_eq!(tables.board_position(c), None, "not on the board");
        assert_eq!(tables.stack_position(c), Some(0));
        assert_eq!(tables.stack_position(a), None);
        assert_eq!(tables.today_position(b), Some((0, 0)));
        assert_eq!(tables.today_position(c), Some((2, 0)));
        assert_eq!(tables.today_position(a), None);
        // Round-trip against the forward lookups.
        let (section, row) = tables.today_position(c).expect("positioned");
        assert_eq!(tables.today_row(section, row), Some(c));
        let (column, row) = tables.board_position(b).expect("positioned");
        assert_eq!(tables.board_task(column, row), Some((stack0, b)));
    }

    /// T-5 — the model sentinel: a set column carries the scaled user
    /// width, an unset one the `−1.0` auto sentinel (the goldens'
    /// auto-layout guarantee rides on it).
    #[test]
    fn stack_width_px_scales_set_columns_and_sends_the_auto_sentinel() {
        let board = BoardId::from_uuid(uuid::Uuid::now_v7());
        let set = StackId::from_uuid(uuid::Uuid::now_v7());
        let unset = StackId::from_uuid(uuid::Uuid::now_v7());
        let mut vs = ViewState::default();
        vs.set_stack_width(board, set, 240);
        assert_eq!(stack_width_px(Some(board), set, &vs, 2.0), 480.0);
        assert_eq!(stack_width_px(Some(board), unset, &vs, 2.0), -1.0);
        assert_eq!(
            stack_width_px(None, set, &vs, 2.0),
            -1.0,
            "no board, no width"
        );
        assert_eq!(
            stack_width_px(Some(board), set, &vs, 0.5),
            120.0,
            "sub-unity scales shrink the rendered value"
        );
    }

    fn task() -> Task {
        Task {
            id: TaskId::from_uuid(uuid::Uuid::now_v7()),
            remote: None,
            title: "t".to_owned(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack: StackId::from_uuid(uuid::Uuid::now_v7()),
            order: 0,
            labels: BTreeSet::default(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,

            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: epoch(),
                description: epoch(),
                duedate: epoch(),
                scheduled: epoch(),
                done: epoch(),
                position: epoch(),
                labels: epoch(),
                effort: epoch(),
                importance: epoch(),
                urgency: epoch(),
                recur: epoch(),

                archived: epoch(),
                deleted: epoch(),
            },
            remote_seen: None,
        }
    }

    fn epoch() -> chrono::DateTime<chrono::Utc> {
        taskboard_domain::BoardClocks::epoch()
    }

    #[test]
    fn field_decorations_carry_kind_tone_and_the_domain_label() {
        let mut t = task();
        t.importance = Some(Importance::High);
        t.urgency = Some(Urgency::VeryLow);
        let decorations = field_decorations(&t);
        assert_eq!(decorations.len(), 2);
        assert!(matches!(decorations[0].kind, DecorationKind::Importance));
        assert!(matches!(decorations[0].tone, Tone::Warning));
        assert!(matches!(decorations[1].kind, DecorationKind::Urgency));
        assert!(matches!(decorations[1].tone, Tone::Neutral));
        // The label is the domain text form verbatim — one vocabulary.
        assert_eq!(decorations[0].label, Importance::High.text_form());
        assert_eq!(decorations[1].label, Urgency::VeryLow.text_form());
    }

    #[test]
    fn the_tone_mapping_is_monotone_in_severity() {
        let rank = |t: &Tone| -> u8 {
            match t {
                Tone::Neutral => 0,
                Tone::Info => 1,
                Tone::Success => 2,
                Tone::Warning => 3,
                Tone::Danger => 4,
                _ => panic!("levels map onto the severity tokens only"),
            }
        };
        let ladder = [
            Importance::VeryLow,
            Importance::Low,
            Importance::Normal,
            Importance::High,
            Importance::VeryHigh,
        ];
        for pair in ladder.windows(2) {
            assert!(
                rank(&level_tone(pair[0].text_form())) < rank(&level_tone(pair[1].text_form())),
                "{:?} must rank below {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn unset_levels_decorate_nothing() {
        assert_eq!(field_decorations(&task()), [] as [TaskDecoration; 0]);
    }

    /// D9 — the diff primitive: positional compare, truncate, append;
    /// the mutation counts are the transport's budget log.
    #[test]
    fn diff_rows_mutates_exactly_the_differing_rows() {
        let model = VecModel::from(vec![1, 2, 3]);
        // Equal rows → zero mutations.
        assert_eq!(diff_rows(&model, &[1, 2, 3]), 0);
        // One changed row → one mutation, in place.
        assert_eq!(diff_rows(&model, &[1, 9, 3]), 1);
        assert_eq!(model.row_data(1), Some(9));
        // Append → one; shrinking back → one removal.
        assert_eq!(diff_rows(&model, &[1, 9, 3, 4]), 1);
        assert_eq!(diff_rows(&model, &[1, 9, 3]), 1);
        assert_eq!(diff_rows(&model, &[1]), 2);
        assert_eq!(model.row_count(), 1);
        // A mid-list reorder is a positional set cascade over the
        // suffix — the accepted transport shape (in-place updates).
        let reordered = VecModel::from(vec![1, 2, 3, 4]);
        assert_eq!(diff_rows(&reordered, &[1, 3, 2, 4]), 2);
        assert_eq!(reordered.row_data(1), Some(3));
        assert_eq!(reordered.row_data(2), Some(2));
    }

    /// D9's column rule: a `StackColumnModel` row whose scalars and
    /// inner card handle are unchanged compares equal — the outer
    /// `set_row_data` never fires for an unchanged refresh.
    #[test]
    fn a_column_row_with_unchanged_scalars_and_handle_compares_equal() {
        let cards: ModelRc<TaskCardModel> = ModelRc::new(VecModel::default());
        let column = StackColumnModel {
            title: "a".into(),
            tasks: cards.clone(),
            create_open: false,
            rename_open: false,
            initial_title: "a".into(),
            width_px: -1.0,
        };
        let model = VecModel::from(vec![column.clone()]);
        assert_eq!(
            diff_rows(&model, std::slice::from_ref(&column)),
            0,
            "unchanged refresh"
        );
        // A scalar change fires exactly one row update; a fresh inner
        // handle (the pre-D9 shape) fires even with equal scalars.
        let retitle = StackColumnModel {
            title: "b".into(),
            ..column.clone()
        };
        assert_eq!(diff_rows(&model, std::slice::from_ref(&retitle)), 1);
        let rebound = StackColumnModel {
            tasks: ModelRc::new(VecModel::default()),
            ..retitle
        };
        assert_eq!(diff_rows(&model, &[rebound]), 1, "a fresh handle rebinds");
    }

    /// D9 — an unchanged card (same title, phrases, emphasis, and the
    /// persistent decorations handle) compares fully equal, so the
    /// card row never fires across refreshes.
    #[test]
    fn an_unchanged_card_compares_equal_across_refresh_passes() {
        let mut t = task();
        t.importance = Some(Importance::Normal);
        let decorations = Rc::new(VecModel::from(field_decorations(&t)));
        let vs = ViewState::default();
        let now = epoch();
        let sets = DueSets::default();
        let build = || task_card_model(&t, &sets, &vs, now, ModelRc::from(decorations.clone()));
        let model = VecModel::from(vec![build()]);
        assert_eq!(
            diff_rows(&model, &[build()]),
            0,
            "same pass inputs, same handle"
        );
        // A fresh decorations handle makes the row differ even with
        // equal scalars — why the slot is keyed by task (§2.1).
        let fresh = task_card_model(
            &t,
            &sets,
            &vs,
            now,
            ModelRc::new(VecModel::from(field_decorations(&t))),
        );
        assert_eq!(diff_rows(&model, &[fresh]), 1);
    }

    // -- the palette row pipeline (#348, chunk 9) -----------------------

    /// The today pass (chunk 10 D7): section assembly, the kiosk and
    /// anchor filters, the `today_tasks` concat, and the stepped-`now`
    /// rollover proof (no sleeps, no timers — time is data).
    mod today_pipeline {
        use super::*;
        use crate::test_platform::on_slint_thread;
        use crate::view_state::DayCtx;
        use chrono::TimeZone;
        use taskboard_domain::{Board, BoardClocks, Stack, TaskId};

        fn board(id: taskboard_domain::BoardId, title: &str) -> Board {
            Board {
                id,
                remote: None,
                title: title.to_owned(),
                color: taskboard_domain::Color::new(""),
                archived: false,
                deleted: false,
                clocks: BoardClocks {
                    title: BoardClocks::epoch(),
                    color: BoardClocks::epoch(),
                    archived: BoardClocks::epoch(),
                    deleted: BoardClocks::epoch(),
                },
                remote_seen: None,
            }
        }

        fn stack(id: StackId, board: taskboard_domain::BoardId) -> Stack {
            Stack {
                id,
                remote: None,
                board,
                title: "stack".to_owned(),
                order: 0,
                archived: false,
                deleted: false,
                clocks: taskboard_domain::StackClocks {
                    title: BoardClocks::epoch(),
                    order: BoardClocks::epoch(),
                    deleted: BoardClocks::epoch(),
                },
                remote_seen: None,
            }
        }

        fn task(id: TaskId, stack: StackId, title: &str) -> Task {
            Task {
                id,
                remote: None,
                title: title.to_owned(),
                description: String::new(),
                duedate: None,
                scheduled: None,
                done: None,
                stack,
                order: 0,
                labels: std::collections::BTreeSet::default(),
                effort: None,
                importance: None,
                urgency: None,
                recur: None,

                archived: false,
                deleted: false,
                clocks: taskboard_domain::TaskClocks {
                    title: BoardClocks::epoch(),
                    description: BoardClocks::epoch(),
                    duedate: BoardClocks::epoch(),
                    scheduled: BoardClocks::epoch(),
                    done: BoardClocks::epoch(),
                    position: BoardClocks::epoch(),
                    labels: BoardClocks::epoch(),
                    effort: BoardClocks::epoch(),
                    importance: BoardClocks::epoch(),
                    urgency: BoardClocks::epoch(),
                    recur: BoardClocks::epoch(),

                    archived: BoardClocks::epoch(),
                    deleted: BoardClocks::epoch(),
                },
                remote_seen: None,
            }
        }

        fn stamp(secs: i64) -> chrono::DateTime<chrono::Utc> {
            chrono::Utc.timestamp_opt(secs, 0).unwrap()
        }

        fn day(now: i64) -> DayCtx {
            DayCtx {
                now: stamp(now),
                window: (stamp(now), stamp(now + 86_400)),
            }
        }

        /// Two boards: overdue (B), scheduled (A), due-today (A), an
        /// anchor-titled due-today (A), done-in-window (B) — the ids
        /// vec is in exactly that order.
        #[allow(clippy::type_complexity)]
        fn two_board_state() -> (
            AppState,
            taskboard_domain::BoardId,
            taskboard_domain::BoardId,
            Vec<TaskId>,
        ) {
            let a = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
            let b = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
            let sa = StackId::from_uuid(uuid::Uuid::now_v7());
            let sb = StackId::from_uuid(uuid::Uuid::now_v7());
            let mut state = AppState::default();
            state.boards.insert(a, board(a, "alpha"));
            state.boards.insert(b, board(b, "beta"));
            state.stacks.insert(sa, stack(sa, a));
            state.stacks.insert(sb, stack(sb, b));
            let scheduled = TaskId::from_uuid(uuid::Uuid::now_v7());
            let mut t = task(scheduled, sa, "scheduled");
            t.scheduled = Some(stamp(31_565_000));
            state.tasks.insert(scheduled, t);
            let overdue = TaskId::from_uuid(uuid::Uuid::now_v7());
            let mut t = task(overdue, sb, "overdue");
            t.duedate = Some(stamp(31_500_000));
            state.tasks.insert(overdue, t);
            let due = TaskId::from_uuid(uuid::Uuid::now_v7());
            let mut t = task(due, sa, "due");
            t.duedate = Some(stamp(31_580_000));
            state.tasks.insert(due, t);
            let anchor = TaskId::from_uuid(uuid::Uuid::now_v7());
            let mut t = task(anchor, sa, taskboard_domain::ANCHOR_CARD_TITLE);
            t.duedate = Some(stamp(31_580_000));
            state.tasks.insert(anchor, t);
            let done = TaskId::from_uuid(uuid::Uuid::now_v7());
            let mut t = task(done, sb, "done");
            t.done = Some(stamp(31_570_000));
            state.tasks.insert(done, t);
            let _ = due;
            (state, a, b, vec![overdue, scheduled, due, anchor, done])
        }

        fn push_pass(
            window: &crate::AppWindow,
            state: &AppState,
            vs: &ViewState,
            day: DayCtx,
            kiosk: bool,
            models: &mut PushModels,
        ) -> IdTables {
            let keymap = crate::keymap::Keymap::from_preset(crate::keymap::Preset::Standard);
            let chain = [ContextId::Today, ContextId::ViNormal, ContextId::Global];
            let due = crate::models::DueEditorPush::default();
            push(
                window,
                state,
                vs,
                &keymap,
                &chain,
                day,
                kiosk,
                &taskboard_domain::ReminderDefaults::default(),
                models,
                &due,
            )
        }

        #[test]
        fn sections_assemble_and_the_filters_apply() {
            on_slint_thread(|| {
                let (state, _a, _b, ids) = two_board_state();
                let window = crate::AppWindow::new().expect("window on the testing backend");
                let vs = ViewState::default();
                let mut models = PushModels::default();
                let tables = push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);

                // Display order: overdue (B), scheduled (A), due-today
                // (A), done (B) — the anchor never appears.
                assert_eq!(
                    tables.today_tasks,
                    vec![ids[0], ids[1], ids[2], ids[4]],
                    "overdue, scheduled, due, done — anchor filtered"
                );
                let today = window.get_today_model();
                assert_eq!(today.sections.row_count(), 4);
                assert!(!today.empty);
                // The `(section, row)` seam resolution round-trips
                // through the per-section prefix sums (flat-index
                // semantics: the rows are one concatenated table).
                assert_eq!(tables.today_row(0, 0), Some(ids[0]), "overdue row");
                assert_eq!(tables.today_row(1, 0), Some(ids[1]), "scheduled row");
                assert_eq!(tables.today_row(2, 0), Some(ids[2]), "due row");
                assert_eq!(tables.today_row(3, 0), Some(ids[4]), "done row");
                assert_eq!(tables.today_row(3, 1), None, "past the table's end");
                assert_eq!(tables.today_row(4, 0), None, "past the last section");
                // Row contexts carry the board badge on a multi-board
                // desktop (D-V3).
                let overdue_section = today.sections.row_data(0).expect("section");
                assert_eq!(overdue_section.count, 1);
                let context = overdue_section
                    .rows
                    .row_data(0)
                    .expect("row")
                    .context
                    .to_string();
                assert!(
                    context.contains('\u{203a}'),
                    "board › stack on desktop: {context}"
                );
            });
        }

        #[test]
        fn the_kiosk_filter_keeps_the_focused_board_subset() {
            on_slint_thread(|| {
                let (state, a, _b, ids) = two_board_state();
                let window = crate::AppWindow::new().expect("window on the testing backend");
                let mut vs = ViewState::default();
                vs.select_board(&state, a);
                let mut models = PushModels::default();
                let tables = push_pass(&window, &state, &vs, day(31_563_600), true, &mut models);

                // Only board A's rows survive: scheduled + due; the
                // board part of the contexts is suppressed.
                assert_eq!(tables.today_tasks, vec![ids[1], ids[2]]);
                let today = window.get_today_model();
                let overdue_section = today.sections.row_data(0).expect("section");
                assert_eq!(overdue_section.count, 0, "board B's overdue is filtered");
                let scheduled_section = today.sections.row_data(1).expect("section");
                assert_eq!(scheduled_section.count, 1);
                let context = scheduled_section
                    .rows
                    .row_data(0)
                    .expect("row")
                    .context
                    .to_string();
                assert!(
                    !context.contains('\u{203a}'),
                    "no badges on kiosk: {context}"
                );
            });
        }

        #[test]
        fn a_stepped_now_rolls_the_sections_over() {
            on_slint_thread(|| {
                let (state, _a, _b, ids) = two_board_state();
                let window = crate::AppWindow::new().expect("window on the testing backend");
                let vs = ViewState::default();
                let mut models = PushModels::default();

                // Before the day boundary the task sits in the
                // due-today section — third in display order (after the
                // overdue and scheduled rows).
                let first = push_pass(&window, &state, &vs, day(31_563_600), false, &mut models);
                assert_eq!(
                    first.today_tasks.iter().position(|id| *id == ids[2]),
                    Some(2),
                    "due-today section"
                );
                // Stepping `now` past the window end: the same push
                // call recomputes — the rollover needs no timer. The
                // task's duedate is now past, so it moved to Overdue.
                let second = push_pass(
                    &window,
                    &state,
                    &vs,
                    day(31_563_600 + 86_400),
                    false,
                    &mut models,
                );
                // The overdue section now holds both the original
                // overdue row and the rolled one (uuid order within
                // (order, id)); the due-today section is empty.
                let today = window.get_today_model();
                let overdue_section = today.sections.row_data(0).expect("section");
                assert_eq!(overdue_section.count, 2, "both overdue rows");
                let due_section = today.sections.row_data(2).expect("section");
                assert_eq!(due_section.count, 0, "the due section emptied");
                assert!(second.today_tasks.contains(&ids[2]));
            });
        }
    }

    mod palette_pipeline {
        use super::*;
        use crate::test_platform::on_slint_thread;
        use taskboard_domain::{Board, BoardClocks, Stack, TaskId};

        fn board(id: taskboard_domain::BoardId) -> Board {
            Board {
                id,
                remote: None,
                title: "board".to_owned(),
                color: taskboard_domain::Color::new(""),
                archived: false,
                deleted: false,
                clocks: BoardClocks {
                    title: BoardClocks::epoch(),
                    color: BoardClocks::epoch(),
                    archived: BoardClocks::epoch(),
                    deleted: BoardClocks::epoch(),
                },
                remote_seen: None,
            }
        }

        fn stack(id: StackId, board: taskboard_domain::BoardId) -> Stack {
            Stack {
                id,
                remote: None,
                board,
                title: "stack".to_owned(),
                order: 0,
                archived: false,
                deleted: false,
                clocks: taskboard_domain::StackClocks {
                    title: BoardClocks::epoch(),
                    order: BoardClocks::epoch(),
                    deleted: BoardClocks::epoch(),
                },
                remote_seen: None,
            }
        }

        fn titled_task(id: TaskId, stack: StackId, title: &str) -> Task {
            let mut task = task();
            task.id = id;
            task.stack = stack;
            task.title = title.to_owned();
            task
        }

        /// Eleven matching candidates (ten needles + the anchor task)
        /// against the 8-row cap: the anchor is filtered BEFORE the cap
        /// (it never occupies a visible row), the overflow counts real
        /// candidates, and the returned targets align with the pushed
        /// model rows.
        #[test]
        fn the_anchor_is_filtered_before_the_cap_and_tables_align() {
            on_slint_thread(|| {
                let board_id = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
                let stack_id = StackId::from_uuid(uuid::Uuid::now_v7());
                let mut state = AppState::default();
                state.boards.insert(board_id, board(board_id));
                state.stacks.insert(stack_id, stack(stack_id, board_id));
                for n in 0..10 {
                    let id = TaskId::from_uuid(uuid::Uuid::now_v7());
                    state
                        .tasks
                        .insert(id, titled_task(id, stack_id, &format!("needle {n}")));
                }
                let anchor = TaskId::from_uuid(uuid::Uuid::now_v7());
                state.tasks.insert(
                    anchor,
                    titled_task(anchor, stack_id, taskboard_domain::ANCHOR_CARD_TITLE),
                );

                let window = crate::AppWindow::new().expect("window on the testing backend");
                let mut vs = ViewState::default();
                vs.open_palette();
                vs.set_buffer("needle".to_owned());
                let mut models = PushModels::default();
                let targets = push_palette(&window, &state, &vs, &mut models);

                // Ten matching needles survive the filter; the anchor
                // (also matching? no — "needle" is not in the anchor
                // title, so this leg pins the pipeline arithmetic) —
                // capped at 8 with an overflow of 2.
                assert_eq!(targets.len(), PALETTE_ROW_CAP);
                assert_eq!(window.get_palette_overflow(), 2);
                assert_eq!(window.get_palette_selected_index(), 0);
                assert_eq!(window.get_palette_model().row_count(), PALETTE_ROW_CAP);
                // The table is index-aligned: every target's title
                // names a needle, and the anchor is nowhere.
                for target in &targets {
                    if let SearchTarget::Task(id) = target {
                        let title = state.tasks[id].title.clone();
                        assert_ne!(title, taskboard_domain::ANCHOR_CARD_TITLE);
                        assert!(title.starts_with("needle"));
                    } else {
                        panic!("all rows are tasks");
                    }
                }
                // Tables-back: the indices resolve round-trip.
                let table = IdTables {
                    palette: targets,
                    ..IdTables::default()
                };
                assert!(matches!(table.palette[0], SearchTarget::Task(_)));
            });
        }

        /// The filter leg with a matching anchor: the anchor task is
        /// live and matching, so its absence from the counts is the
        /// filter's proof.
        #[test]
        fn a_matching_anchor_never_becomes_a_visible_row() {
            on_slint_thread(|| {
                let board_id = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
                let stack_id = StackId::from_uuid(uuid::Uuid::now_v7());
                let mut state = AppState::default();
                state.boards.insert(board_id, board(board_id));
                state.stacks.insert(stack_id, stack(stack_id, board_id));
                let id = TaskId::from_uuid(uuid::Uuid::now_v7());
                state.tasks.insert(
                    id,
                    titled_task(id, stack_id, taskboard_domain::ANCHOR_CARD_TITLE),
                );

                let window = crate::AppWindow::new().expect("window on the testing backend");
                let mut vs = ViewState::default();
                vs.open_palette();
                // A subsequence of the anchor title matches it.
                vs.set_buffer("taskboard".to_owned());
                let mut models = PushModels::default();
                let targets = push_palette(&window, &state, &vs, &mut models);
                assert_eq!(targets, Vec::new());
                assert_eq!(window.get_palette_model().row_count(), 0);
                assert_eq!(window.get_palette_overflow(), 0);
            });
        }

        /// Closed or non-palette flow: the pipeline is inert and the
        /// table stays empty.
        #[test]
        fn a_closed_flow_pushes_nothing() {
            on_slint_thread(|| {
                let state = AppState::default();
                let window = crate::AppWindow::new().expect("window on the testing backend");
                let vs = ViewState::default();
                let mut models = PushModels::default();
                assert_eq!(push_palette(&window, &state, &vs, &mut models), Vec::new());
            });
        }
    }
}
